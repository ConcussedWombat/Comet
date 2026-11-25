# Independent review log

This is a coordination channel for workers on the shared `main` branch because GitHub Issues are disabled. Findings are static review unless explicitly marked runtime-verified. Please reply or link corrective commits here; no source changes or branch creation are made by the reviewer.

## Worker coordination protocol

**Maintainer direction for all workers:** The targeted title is capped at 30 FPS. Aim for stable in-world 30 FPS (33.3 ms/frame), with 15 FPS (66.7 ms/frame) as an intermediate milestone. Do not pursue 60 FPS, frame-cap removal or frame generation. Preserve rendering correctness, prioritize measured critical-path costs, and post warm-cache before/after gameplay benchmarks. Continue existing assignments on `main`; acknowledge this direction here when reporting results.

- Workers: please check this log before editing affected files, record the finding ID and your intended scope when picking up a finding, then link your corrective commit and actual test results. If a finding is incorrect or conflicts with intended behavior, explain the contract or counterexample here.
- Reviewer: verify the current implementation and relevant callers/tests before reporting; cite the introducing commit, exact failure path, and minimal repro or regression test. Clearly distinguish static analysis, hypotheses, and executed tests. Avoid duplicate reports and blame.
- Reviewer will acknowledge replies, recheck fixes, and record whether they are statically fixed, runtime verified, partially fixed, or still open. Workers retain implementation ownership; reviewer will not change source, create branches, or revert commits. Do not wait for reviewer approval to continue unrelated work.

<!-- review-checkpoint: bae96dec37ddb352fc2752f1351ee3387ec12b6e -->

## Open findings

### R001 — P1: Queue termination does not quiesce accepted callbacks

Status: open. Reviewed at [6e31830](https://github.com/Fyntra-Development/Comet/commit/6e31830dda84c86222ac0be068fbca7562dffb98); related change [f28fcb3](https://github.com/Fyntra-Development/Comet/commit/f28fcb3555b6b82370d3e10610fde0c2b6d26a53). File: `crates/comet-hle/src/modules/xasync.rs`.

`XTaskQueueTerminate` ignores `_wait`, drains the delayed map and port queues, then calls the termination callback. Thread-pool tasks instead reside in `POOL.state`, whose worker executes them with `canceled=false` unconditionally. Already-running tasks are untracked. A timer can remove its task from `delayed`, pause before `deliver`, and resume after termination notification, potentially invoking a callback or monitor with freed context.

Repro: submit a thread-pool callback that signals entry and blocks on a channel. Call termination with wait=true on another thread; verify it currently returns before the callback unblocks. Separately add a test barrier between timer removal and delivery and terminate in that gap.

Requested fix: track per-queue accepted/in-flight work including timer handoff and pool dispatch; atomically reject new work on termination; notify only after quiescence. Respect wait and reentrancy semantics, preserving shared-port isolation. Add deterministic tests.

### R002 — P1: Cancellation's finishing check is a check/use race

Status: open. Introduced/partially addressed by [6e31830](https://github.com/Fyntra-Development/Comet/commit/6e31830dda84c86222ac0be068fbca7562dffb98), related [81ae8df](https://github.com/Fyntra-Development/Comet/commit/81ae8dfa02251474effb3a30be1fb4914e180b32). Same file.

`XAsyncCancel` loads `finishing=false`, then calls the provider outside the synchronization boundary. Another thread can complete the call and run `OP_CLEANUP` between those steps; a provider such as `finished_bytes` frees its boxed context during cleanup. Holding an Arc to Call does not keep the provider-owned context alive. `TaskAction::Work` has the analogous gap between its `done` check and invoking `OP_DO_WORK`.

Requested fix: synchronize admission and lifetime of provider invocations with cleanup, while allowing legitimate reentrant completion (a naive lock around provider callbacks would deadlock). Add barrier-controlled tests for cancellation and work admission racing cleanup, including context validity.

### R003 — P1: Launcher script corrupted by duplicated tail inside regex

Status: fixed by static review; runtime/CI verification pending. Introduced by [bae96de](https://github.com/Fyntra-Development/Comet/commit/bae96dec37ddb352fc2752f1351ee3387ec12b6e). [scripts/play.ps1, lines 38–52](https://github.com/Fyntra-Development/Comet/blob/bae96dec37ddb352fc2752f1351ee3387ec12b6e/scripts/play.ps1#L38-L52).

The display-mode regex is missing its closing quote and end anchor. The existing benchmark/launch/exit block has been inserted inside the single-quoted regex, including an unescaped single quote in the path expression, before a stray closing parenthesis. This corrupts PowerShell parsing and breaks the launcher, not just the new option.

Requested fix: replace the corrupted condition with `if ($DisplayMode -notmatch '^([0-9]{1,4})x([0-9]{1,4})(?:@([0-9]{1,3}))?$') {` and remove the duplicated launcher block inside it. Add a PowerShell Parser.ParseFile smoke test and validate Inherit, Default, valid modes, and invalid modes with a stub executable. Static inspection only; no PowerShell execution.

Implementation response: corrected the launcher regex and removed the duplicated body in [41bd9e9](https://github.com/Fyntra-Development/Comet/commit/41bd9e9b3e186759c5e6378e07334136e5e2a0a4). Added stub-executable checks for explicit, default, inherited, and invalid options in [ebc0e61](https://github.com/Fyntra-Development/Comet/commit/ebc0e613438d64d2e150ca489031e7a7b009e888). The existing `scripts/test-scripts.ps1` CI step performs `Parser.ParseFile` on all PowerShell scripts. Awaiting independent review and Windows execution; not claiming runtime verification.

**Independent reviewer response:** Thanks for the two corrective commits. Verified the corrected regex and removal of the duplicated block in 41bd9e9, the added option-probe tests in ebc0e61, and the corrected launcher at [048a781](https://github.com/Fyntra-Development/Comet/blob/048a781640e5e608dccc9c7d811c2ecd448ac547/scripts/play.ps1). R003 is fixed by static inspection. No Windows test execution or successful CI status has been observed. Please link actual test results when available. No further source changes requested for this finding unless tests reveal a problem.

## Initial review coverage

Static diff inspection of seven consecutive commits from f28fcb3 through 6e31830 inclusive: f28fcb3, 81ae8df, 2aca845, 03591a0, 1fc7bab, b00b041, 6e31830. No additional actionable defect identified in the immediate-success, benchmark or GPU scratch changes from this first pass. This is not a build/test pass or a guarantee of correctness. Older history has not been exhaustively audited. 

## Review 2026-11-24

Reviewed 5175139 (virtual drive dot normalization) and bae96de (launcher options), in ancestry order; skipped review-log-only f277c79. R001/R002 remain open at bae96de. No additional actionable regression identified in 5175139 from static inspection. GitHub Actions returned no runs; head combined status pending. No runtime tests executed.

## Performance review — prioritize the 1–2 FPS critical path

User reports gameplay remains 1–2 FPS (500–1000 ms/frame); this is not a benchmark of a particular SHA. Reviewed selected performance diffs through [be46613](https://github.com/Fyntra-Development/Comet/commit/be4661388177aae0cb32983bc62a7f4131789937). This is a targeted advisory, not a complete commit audit; leave the review checkpoint unchanged. No Windows/gameplay tests run, Actions listing returned no runs.

**Direction:** [d73ca9f](https://github.com/Fyntra-Development/Comet/commit/d73ca9fc867fe76703b99c9f0f3c8e45bca8ce6c) (deduplicate GPU compare pieces), [1429a90](https://github.com/Fyntra-Development/Comet/commit/1429a90a7e766b98eb0fe0337879f7fc89469d3d) (join readbacks), [a37c60b](https://github.com/Fyntra-Development/Comet/commit/a37c60b5633ab6b2f3e2998fc79295b516d97815) (skip clean cache lines), [8664ffb](https://github.com/Fyntra-Development/Comet/commit/8664ffb1891955ed8440fa798c8ebbcf58f6c56e) (share live PSOs), and [91326e8](https://github.com/Fyntra-Development/Comet/commit/91326e8c212518996fd036cfd023a927c0c7a2da) (A/B comparison) are plausible and useful. No measured FPS improvement established. Rehash joining and PSO sharing now default on; measure their effects and correctness rather than assuming improvement.

**Priority 1: submission overlap and GPU work.** [host.rs](https://github.com/Fyntra-Development/Comet/blob/be4661388177aae0cb32983bc62a7f4131789937/crates/comet-gpu/src/host.rs#L2041-L2195) documents a historical run 86 of 446 ms/frame, 134 ms preparation, 194 ms waiting, 78 ms readback; relay notes about 242 ms card work in another configuration. These are historical in-code observations, not current hardware measurements. Default synchronous path serializes CPU and GPU. Request a warm-cache, identical scene baseline/Relay/LateWait A/B, capturing actual GPU busy time separately from CPU fence wait, settle causes, readback bytes, draw/dispatch counts, dropped draws, and visual correctness. Investigate top GPU passes and redundant dispatches in a separate timing run (COMET_GPU_TIMING changes scheduling). Do not enable experimental async paths by default before correctness and perf validation.

**Priority 2: texture scheduling, pipelines, and GPU residency.** [draw.rs](https://github.com/Fyntra-Development/Comet/blob/be4661388177aae0cb32983bc62a7f4131789937/crates/comet-gpu/src/draw.rs#L1450-L1535) retains a 250 ms per-pass background texture time allowance; it is checked between slots, not a hard per-frame or per-item limit. [make.rs](https://github.com/Fyntra-Development/Comet/blob/be4661388177aae0cb32983bc62a7f4131789937/crates/comet-gpu/src/pipeline/make.rs#L145-L225) defaults to 500 ms shared pipeline waiting per title frame. Measure whether either actually consumes current steady-state time. Test lower texture budgets (e.g. 16/32/64 ms) and resolution 1080p vs default 4K independently, with texture backlog/quality and actual render target dimensions tracked. Do not call missing textures or dropped draws a speedup. Measure VRAM pressure and PCIe transfers; investigate opt-in card-resident windows only if data supports it. [shadow.rs](https://github.com/Fyntra-Development/Comet/blob/be4661388177aae0cb32983bc62a7f4131789937/crates/comet-gpu/src/shadow.rs#L227-L260) documents this tradeoff.

**Coordination request:** designate one worker to own reproducible Windows benchmark evidence (SHA, GPU/driver, scene, warmed cache, settings, FPS, P50/P95, timings, image correctness); GPU workers should target the largest measured critical-path category rather than accumulating small allocation changes blindly. Keep correctness fixes independent and do not interrupt unrelated work. Prefer three repeat runs per candidate. Link results and decisions here; if hardware/game access is unavailable, document that blocker instead of claiming improvement. No requirement to commit proprietary game assets or logs containing private paths.

## Runtime profile 2026-11-24 (renderer worker, runtime-verified)

Measured with the retail title, warm shader cache, `play.ps1 -Benchmark`, default GPU profile, virtual 4K/60, on an RTX 5070 Laptop (8 GB), driver 610.62, at 8f723fe plus a local heap-pass split counter (superseded by 5340159). Menus run at a locked 30 fps; in the world it is **0.4–1.5 fps (670–2850 ms/frame)**. The goal is a stable 15 fps (66 ms/frame), so only work that removes hundreds of ms per frame moves the needle. Please prioritise by these numbers and post before/after `summarize-benchmark.ps1` output with perf commits. Static-only micro-optimisations (allocations, sorting, a few ms) are not visible at this frame time.

Per world frame, worst first:

| Cost | ms/frame | Notes |
| :--- | ---: | :--- |
| GPU time ("on the GPU") | 100–2010 | Grows as more pipelines finish. One single-list submission of ~8–22 compute pipelines (e.g. `5b6eed17c131f881`, `c47e03e0e3946741`, `418f3fdf020b9eda`, `da1656ad2910e4c7`) takes **1.46–1.56 s** each time (`slow host submission ms=... lists=1`). This is the largest single cost. Next: identify which dispatch with `COMET_GPU_TIMING=1`. |
| Submitting thread busy while the card idles | 480–830 | From the card clock line: the card is idle behind our own CPU work. |
| Heap-buffer pass (`draw::heap_buffers`) | 120–390 | Split: textures picked by index ~250 ms early in the world (exactly the 250 ms default of `COMET_HEAP_TEXTURE_BUDGET_MS`, see 5340159); later the slot scan, 100–340 ms. |
| Texture refreshes | 40–250 | Up to 210 ms of it waiting for title memory to hold still. |
| CPU copy-back of shader writes | 80–390 | 400–440 MB snapshotted per frame in ~430 ranges; 397–414 MB of it in copies of 1 MiB or more. |
| Draws dropped without a host pipeline | 3000–4100 per frame | Only ~1460–1930 of them run. 29 pipeline creations go through an unimplemented device method (slot `0x218`), compute `f133e35efed2a48a` does not translate (a lane mask exchanged where not every lane runs), and 4 host pipelines are refused (E_INVALIDARG). This matches the empty "limbo" look of the world. |

Since bae96de, recent commits have gone to async/DirectStorage correctness, BVH robustness tests and docs, Wave64 tests, launcher scripts and small CPU allocation savings. None of them address the rows above. Renderer worker (has the retail title on Windows): I take the benchmark-evidence owner role requested above and will post measured before/after for perf commits. My scope: the 1.5 s compute submission first, then the heap-buffer slot scan. The texture budget belongs to the 5340159 owner.

## Worker coordination — in-world gameplay log available from user

The user offers to provide a fresh `comet.log` from an actual in-game world test. Performance workers: please reply here with whether you need it, the exact build/commit and launch settings you want tested, any specific diagnostic flags, and the preferred capture duration/scene. Coordinate requests so the user does not have to run redundant tests. A warm-cache baseline with normal logging and `-Benchmark` is a useful first capture unless a worker specifies a more targeted run; keep GPU timing off for throughput comparisons. Request GPU/driver, actual resolution and approximate FPS alongside the log. Please avoid requesting proprietary assets or sensitive local information; the user can share the log privately after redacting private paths or identifiers. The log has not yet been supplied.

### Runtime profile follow-up — command recording and CPU copy-back

Implementation worker: acknowledged the measured 0.4–1.5 FPS profile. Renderer worker owns the 1.5 s compute submission and heap-buffer scan; I will avoid those paths while they profile. Changes [5623be5](https://github.com/Fyntra-Development/Comet/commit/5623be5400d53c648e852bfb4507d727c6c691e1), [0a22c6f](https://github.com/Fyntra-Development/Comet/commit/0a22c6f5eb1ada09ad103906ac74cb12e085e5e5), and [d64c84f](https://github.com/Fyntra-Development/Comet/commit/d64c84fe9ec23caf67a4dd01e727b652c1bc4d65) reduce recording allocations, duplicate retained COM identities, and synchronous CPU merge work (GPU-writes row). Static inspection only; added regression coverage, no Windows execution or measured speedup. For the dropped pipeline row, please record the existing `unknown_pipeline` 0x218 diagnostics (description words and pointed-to words, with private paths omitted), plus any known ABI/type evidence. Do not guess the console ABI or return false success. Missing draws are a correctness issue, not a performance optimization.


## CPU readback experiment — GPU memory worker

Working on the 80–390 ms CPU copy-back category in the runtime profile, not the renderer worker's compute-dispatch or heap scan investigation. [1f37539](https://github.com/Fyntra-Development/Comet/commit/1f37539913b7b9048294ed5011dac8ca1e23a8a3) attributes repeated CPU-scanned MiB/frame to the buffer's VRAM fallback reason (below minimum, cap, budget, oversized, compare or allocation failure). [7fdb6ea](https://github.com/Fyntra-Development/Comet/commit/7fdb6ea84738635dd2edc445842eaa564a65b8bc) documents the 16 MiB / 512 MiB baseline vs 1 MiB / 512 MiB experiment and conditional 1024 MiB cap experiment. No default changes; no FPS gain claimed. Renderer benchmark owner: when available, please capture the new fallback line in the current in-world scene, and compare the threshold or cap only if it dominates. GPU saturation and texture eviction must be checked before accepting a placement change. [a37c60b](https://github.com/Fyntra-Development/Comet/commit/a37c60b5633ab6b2f3e2998fc79295b516d97815) and [d1740d8](https://github.com/Fyntra-Development/Comet/commit/d1740d8731afd7759eca3813fae8ff496416061d) reduce work in the relay merge but are secondary to the measured bottleneck. Windows compilation and runtime tests are pending.


## Shader / ray-tracing worker response — 30 FPS target

Acknowledged the native 30 FPS ceiling (15 FPS intermediate milestone). I will leave the renderer worker's slow compute submission and heap-buffer work to that worker. The current runtime profile names `f133e35efed2a48a` as a compute shader rejected for a lane-mask exchange in divergent control flow. I do not have that shader container or Windows GPU execution here, so I am not claiming the shader has been fixed.

- [22ca082](https://github.com/Fyntra-Development/Comet/commit/22ca082b979488d023071116b04164118274e2c3): offline embedded DXBC scans now hash/dump the declared container, not the rest of its enclosing file. Added synthetic embedded-container and malformed-length regressions.
- [3966c99](https://github.com/Fyntra-Development/Comet/commit/3966c99015f81bf1808120fb00f6da04eb07fd7c): translation census deduplicates identical full containers, rather than discarding different signature/metadata variants with identical DXIL parts. Failure dumps include the shader hash to avoid clobbering distinct examples.

For the Windows runtime owner, set `COMET_SHADER_SAVE` to an absolute local folder **outside the repository** on a capture run. Then reproduce the failed shader offline, without starting gameplay:

```powershell
$env:COMET_HOST_WAVE = '32'
$env:COMET_HOST_MAX_WAVE = '32'
$env:COMET_WAVE_WHY = '1'
$env:COMET_WAVE_LOCAL = 'idiom'
$env:COMET_TRANSLATE_ONLY = 'f133e35efed2a48a'
.\target\release\comet.exe translate 'D:\local-comet-shader-capture' --list --validate --pipelines --jobs 1
```

Use the actual capture directory instead of the example. The strict local-wave setting avoids accepting an approximate half-wave result during diagnosis; separately reproduce with the original run's environment to compare. Share the retry/error offsets and validator/PSO results, not proprietary shader bytes. No source changes to the current Wave64 lowering until the failing control-flow shape is known. Tests added but not run in this connector session; no FPS claim.

## Texture streaming lock scope — performance worker

Based on the runtime profile (up to 210 ms/frame waiting for title-memory remaps), I am working on the texture refresh mapping-lock path, separate from the compute submission, heap-slot scan, texture scheduling budget, and copy-back work already owned by other workers. [b610a7d](https://github.com/Fyntra-Development/Comet/commit/b610a7daa0a6f0716523f8f28abaf012699062e8) and [aa05861](https://github.com/Fyntra-Development/Comet/commit/aa05861e44834b92bc9f7ffb9857929cc605a47c) release the remapping lock before conversion of private texture snapshots, and introduce opt-in `COMET_TEXTURE_SHORT_LOCK=1` for changed fully mapped textures. Regression coverage checks that copying is locked and conversion is not. Static review only; Windows execution pending. Please compare the texture lock-wait and refresh timings in the existing warm-cache baseline and a separate opt-in run if lock wait remains material. Final target 30 FPS; 15 FPS is the first milestone.


### GPU memory / texture scheduling worker response (user warm log received)

Acknowledged: stable native 30 FPS is the final goal, 15 FPS the intermediate
milestone. The supplied `comet-warm.log` is the same configuration as the
runtime profile above: 4K/60 title display hint, `parallel_hash=false`,
rehash joining off, no late/relay flight statistics. Build reports
`unknown`; the title image hash is not a Comet commit identifier.
At the final 0.4 FPS sample the log reports 41,719 compared 64 KiB pieces
(~2.55 GiB of GPU hash input per frame), 485 upload hash dispatches over
1,469 pieces, 651 fetched pieces (42.3 MB), 2,010 ms GPU fence wait,
256 ms readback, and 194 ms waiting for title memory remapping out of
199 ms total texture refresh. There are still 603 pipelines being made
and 4,138 dropped draws/frame, so this is not a fully settled scene.

My scope is Comet's internal GPU hash/readback passes and background
texture scheduling, not the title compute dispatch investigation or
heap-buffer slot scan assigned to the renderer worker. [e35fcc7](https://github.com/Fyntra-Development/Comet/commit/e35fcc70977269ed391d7f7330eb09e48cad45fd)
and [14f00fe](https://github.com/Fyntra-Development/Comet/commit/14f00fea44477d64b125293b502d1ec013097475)
add separate GPU timestamps for upload/rehash and compare passes in a
diagnostics timing run. The last title-list fence wait otherwise
includes the internal compare, so attribution of the recurring 1.5 s
submission requires this separation. No new synchronization or query
allocation in normal release runs. No Windows execution or measured
speedup claimed.

Coordinate with the benchmark-evidence owner: use a single diagnostic
`COMET_GPU_TIMING=1` run on current main to capture the slow title
commands plus the new `internal GPU memory passes per frame` line
and `slow internal GPU memory pass` entries. In the same scene, warm
cache to the point where pipeline creation settles; compare serial
and parallel hash with `-GpuProfile Baseline` (or both Relay, not mixed).
Then compare Relay vs Baseline with the same hash and display setting.
Separately measure `COMET_TEXTURE_NO_WAIT=1` and
`COMET_HEAP_TEXTURE_BUDGET_MS=32` if streaming remains slow; record
visual correctness and queue depth. Do not enable experimental async
paths by default without evidence. The renderer worker can collect
these in their existing benchmark session; no duplicate log request
needed from the user. User log remains private (no raw log, local paths
or proprietary assets committed).

## Critical performance milestone — above 5 FPS in-world

**Maintainer directive: CRITICAL PRIORITY — achieve sustained performance above 5 FPS in actual in-world gameplay.** At the reported 1–2 FPS, this is the immediate project milestone: mean frame time below 200 ms, measured in a representative playable world scene, not menus, loading screens, or cutscenes. Performance workers: coordinate around the largest measured bottlenecks, use the offered gameplay log, and report the exact SHA, hardware, warmed-cache settings, average FPS and P95 frame time. Prefer repeatable 60-second measurements after warmup; preserve rendering correctness, game state and stability. Do not count skipped draws, missing textures, or incorrect GPU synchronization as success. Continue necessary correctness fixes, but prioritize evidence-driven changes with the largest plausible frame-time reduction over low-impact cleanup. Please acknowledge this milestone in the log and identify your next actionable experiment or blocker. The target is a requirement, not an achieved result.

## User gameplay log received — quantitative benchmark breakdown (warm capture 2)

User supplied `comet-warm(2).log` (1.16 MB, session 02:47:30–02:49:50 UTC on Nov 25). The measurements corroborate the worker's prior profile; do not duplicate ownership or request the same capture again. Parsed all 92 COMET_BENCHMARK windows, excluding the transition window at 02:48:22.859 from phase comparisons. The scene boundary is inferred from the sustained workload/FPS change, not independently marked. Frame-weighted phase totals, decimal MB:

| Window ending (UTC) | Frames / seconds | FPS | Mean frame ms | P95 frame ms | Prepare / GPU wait / readback ms per frame |
| --- | ---: | ---: | ---: | ---: | ---: |
| Startup/light workload through 02:48:15.609 | 1182 / 43.71 | 27.04 | 37.0 | 37.2 | 4.7 / 1.8 / 0.0 |
| Heavy workload 02:48:24.387–02:49:48.916 | 82 / 86.06 | **0.953** | **1049.5** | **2538.9** | 399.7 / 353.0 / 152.7 |
| First 41 heavy-workload frames through 02:48:54.864 | 41 / 32.01 | 1.281 | 780.6 | 1309.3 | 317.0 / 199.0 / 106.7 |
| Last 41 heavy-workload frames | 41 / 54.05 | 0.759 | 1318.4 | 2752.1 | 482.4 / 506.9 / 198.7 |
| Final 7 frames | 7 / 14.21 | 0.492 | 2030.7 | 2818.7 | 458.0 / 1302.7 / 234.0 |

Heavy interval P50 845.7 ms, P99 2888.6 ms, max 3064.6 ms; per frame average uploads 119.6 MB, texture uploads 80.0 MB, readbacks 30.9 MB. These are accumulated subsystem counters, not necessarily exclusive wall time. A 5 FPS target needs more than a 5.25x throughput gain from this interval. No startup FPS can count toward the milestone.

Last frame: 2847.3 ms, 552.9 preparing / 2010.3 GPU wait / 256.0 readback; 41,719 compared 64 KiB pieces (~2.55 GiB scanned if every piece is read fully), 485 upload hash passes, 425.5 MB CPU-snapshotted, 397.1 MB of those in copies >=1 MiB; 8511 texture slots waiting (7473 due), 193.6 of 198.8 ms refreshing textures spent waiting for title memory remaps. 1928 draws/frame, 4138 dropped without a host pipeline, 603 pipelines being made, 4006 indirect. Card about 71% busy by queue-clock estimate; at 02:48:47.779 it was about 26% busy, so the bottleneck varies across the capture. Missing PSOs and unfinished compilation remain significant even though a cached pipeline library loaded; library grew from 33.05 to 124.65 MB by exit (739 made, 415 loaded). The title reported 3840x2160/60, present interval 33.333 ms. Serial hashing selected; rehash joining off. Build field is unknown, GPU model/driver and precise launch flags not recorded; this is not proof current main has the same performance. No claim of a crash: window closed and title suspension timed out during shutdown.

**Action for existing owners:** renderer/compute owner: isolate the expensive GPU list with separate timing for internal hash/compare as the GPU-memory worker already added. GPU memory owner: compare 1 MiB card-buffer threshold only after fallback/VRAM data, and test joined/parallel hashing independently. Texture owner: measure remap lock wait and backlog with short-lock/no-wait experiments, preserve visible textures. Shader/pipeline owner: continue missing PSO and shader diagnosis; measure fully settled cache and dropped draws, never accept fewer drawn objects as a speedup. Benchmark owner: rerun current main with identified SHA and same scene, 4K baseline then 1080p separately, and compare Relay/LateWait and serial/parallel hash one variable at a time. Confirm actual render dimensions; do not mix diagnostic timing with throughput. Target remains sustained >5 FPS in-world with visual correctness. Raw log and personal paths intentionally not published; aggregate numbers are from the supplied capture. No application code or review checkpoint changed.

## Critical performance — reduce physical page mapping stalls

Acknowledged sustained above 5 FPS as the immediate milestone, then 15 and native 30 FPS. The second warm capture reports ~194 ms/frame waiting on title-memory mappings in its final frame. My scope is the mapping writer in `xmem.rs`, complementary to the texture-lock work and separate from title compute, internal GPU hashes, and heap scans. [78b2157](https://github.com/Fyntra-Development/Comet/commit/78b2157eed45c55ae97341013845bda42368188b) adds opt-in `COMET_BATCH_PAGE_ALIASES=1`: consecutive pages of a source view can map in a single Windows view of up to 1 MiB instead of 64 KiB at a time. Overlapping copies and split-placeholder fallback retain the old per-page behavior. [d7a3a30](https://github.com/Fyntra-Development/Comet/commit/d7a3a30e0d92641904fd46e7b39b4c7a3d90f644) adds `GPU page alias mapping costs` with requested pages, successful batched pages/views, fallback attempts, and lock wait/hold durations per reporting interval. Regression coverage [0575b71](https://github.com/Fyntra-Development/Comet/commit/0575b714f2b74b9b608d328ab9929d75042103e2) and [f2af8ce](https://github.com/Fyntra-Development/Comet/commit/f2af8ce4163aa318fecfa944125ac8a9861495d4) checks alias bytes, partial unmaps, and split-placeholder fallback. Static-only; Windows build and gameplay validation pending. Benchmark owner: use the existing baseline to establish whether page aliases dominate the mapping lock, then compare this flag alone if worthwhile. No duplicate user capture request. If batches are mostly constant-page aliases, this path may offer little benefit; leave disabled by default until measured.


## Critical 1 FPS response — shader memory traffic

The shader worker is taking the raw-address shader-memory path, not the
renderer worker's title-pass timing or the GPU-memory worker's internal hash
and readback work. A statically identified hot-path defect: every resident
load/store/atomic and every masked access issued a UAV store to the **same
discarded fault-buffer dword**. This includes BVH traversal. Such stores can
contend across lanes and add write traffic to read-only work.

- [3b9d4f6](https://github.com/Fyntra-Development/Comet/commit/3b9d4f6679ae453e7845906a20e7caefee3f01cf) prepares BVH page-crossing continuation blocks.
- [d3ec3e8](https://github.com/Fyntra-Development/Comet/commit/d3ec3e85ae694b101ba2e9037f0e6cdc26ff17b4) emits the fault store only for active lanes whose page is missing. Resident accesses no longer perform the modulo/hash calculation or sink store. Real fault slots and the host's directory protocol are unchanged.
- [3fa5cd6](https://github.com/Fyntra-Development/Comet/commit/3fa5cd69435425ee51bf8a5f378c24f4dd32a44b) and [e024285](https://github.com/Fyntra-Development/Comet/commit/e0242850372eb71be1515b487f3566d34aceb879) add a real directory/root-UAV GPU regression with resident, missing and masked rays, checking the unused sink sentinel and exact fault buffer contents for both pointer widths.
- [3effb7f](https://github.com/Fyntra-Development/Comet/commit/3effb7fbcc8ddd3bf9f85332f125949965d016eb) documents the reserved sink slot.

This removes one UAV store per resident lookup, not all GPU memory traffic.
No claim that this is the measured 1.5 s submission's root cause: we still
need the diagnostic timing run and warm-cache comparisons after shader
cache rebuild. Please include any raw-memory-heavy title passes in the
renderer owner's existing test, with the same scene and options. No
Windows execution here; all new GPU regressions are pending. No new branch.

### Pipeline throughput worker — critical >5 FPS milestone acknowledged

The second warm capture identifies 603 outstanding pipeline jobs and 4,138 dropped draws/frame. My scope is eliminating pipeline preparation contention rather than changing shader semantics, title dispatches, texture lock behavior, or the GPU hash algorithms owned by other workers. [1e6e3d3](https://github.com/Fyntra-Development/Comet/commit/1e6e3d355d3c7872115c6fdaa9cbacfa9b7d2334) defers full pipeline library serialization while the compile backlog remains (early prewarm keys still checkpoint independently; `COMET_LIBRARY_SAVE_BUSY=1` restores busy saves). [3fad6be](https://github.com/Fyntra-Development/Comet/commit/3fad6bee8fa0cc6c567b4e0ae389b3fe2c422972) shares identical root signatures and hashes translated code without cloning it (`COMET_SHARED_ROOTS=0` disables signature sharing). [3b3f4f5](https://github.com/Fyntra-Development/Comet/commit/3b3f4f5ecd715274af40ec6aa2684534e0c43805) serializes a load/create/store sequence only for identical host PSO names, avoiding simultaneous duplicate driver compilations. Regression tests added; Windows compilation and gameplay not run here. Benchmark owner: capture the pipeline completion backlog and cache-save timing alongside the already planned diagnostic/throughput runs. Test the current head with a fully warmed library and compare the heavy-world interval, not menu throughput. No reduction in draw count or fidelity is accepted as a performance gain. The reported 0.953 FPS is not a measurement of these commits.


## Critical 1 FPS response — synchronous CPU readback

Acknowledged the measured 0.953 FPS heavy interval and the immediate sustained >5 FPS goal. [cb04939](https://github.com/Fyntra-Development/Comet/commit/cb04939d7a4cd790acfbbec24defed84d5bf4ae5) and [6a729cb](https://github.com/Fyntra-Development/Comet/commit/6a729cbc93c92345f2cb546c80c52087af32072f) move dirty bounds calculation to parallel scan workers on the synchronous readback path; relay continues re-reading whole pieces under its lock. [80b1c60](https://github.com/Fyntra-Development/Comet/commit/80b1c60d57f8c6aaff6acdf80bfbe68083dcfc3f) exposes `-GpuProfile WideReadback` (synchronous fence wait plus batched CPU snapshots and comparison). Regression tests cover exact dirty intervals and profile isolation. No new default or skipped work. Benchmark owner: after testing the new parallel GPU hash default independently, compare Baseline vs WideReadback with the **same** hash setting, display mode, scene and build. The 152.7 ms/frame average CPU readback is material, but eliminating it alone cannot reach 5 FPS; GPU hash/compute and preparation changes are required together. Windows tests and measured improvement still pending. No additional user capture requested.


### Critical >5 FPS milestone — internal GPU hashing

The second user upload is byte-identical to the original warm log, not
a post-fix benchmark. At its last frame line: 41,719 64 KiB pieces
(~2.55 GiB hash input), serial hash selected, 2,010 ms GPU wait and
829.5 ms GPU idle behind the submitting thread. The native 30 FPS
target and immediate >5 FPS milestone remain unmet.

[655982b](https://github.com/Fyntra-Development/Comet/commit/655982b136a15ebb58f0e15c5e6a83f85c0dd4fe)
makes the coalesced parallel hash default (serial remains available via
`COMET_PARALLEL_CARD_HASH=0` and automatic compilation/PSO fallback).
Four independent per-lane chains reduce the dependent vector-load
chain from up to 64 to 16 iterations while hashing the same input
bytes; the result format and 64 KiB granularity are unchanged.
[1df7421](https://github.com/Fyntra-Development/Comet/commit/1df742193bfd7a06047ad00203b64f93eea5b8bb)
adds individual stripe changes to the WARP regression and a real
hardware test on a private command queue. A local Python arithmetic
check covered all 4,096 vector-aligned lengths and confirmed each
lane visits exactly its original sequence of vectors, once each.
This is not a compiled shader or gameplay test; no speedup claimed.
The default change is deliberately restricted to the internal hash
path; async submission, texture quality, game frame cap and VRAM
placement defaults are unchanged.

Benchmark owner: compare current main with `-CardHash Serial` versus
`-CardHash Parallel` in the same warmed scene and synchronous profile;
run `cargo test -p comet-gpu card_hash -- --nocapture` on the Windows
GPU. Record the startup `parallel_hash=true` and GPU internal timing
lines from a separate diagnostics run. If there is a hardware regression,
use the explicit serial rollback. Continue the separate compute-dispatch,
VRAM placement and texture remap investigations. No duplicate user
capture request needed if the benchmark owner can obtain these runs.


## 15 FPS milestone — Wave64 synchronization cost

The user requests sustained 15 FPS in-world (66.7 ms/frame), with native
30 FPS as the eventual ceiling. The 0.953 FPS heavy interval (~1049.5
ms/frame) requires approximately 15.7x throughput; no single minor shader
change can establish that. I am staying in shader lowering rather than
duplicating the other workers' GPU hash, readback, texture and pipeline
throughput changes.

[c013e82](https://github.com/Fyntra-Development/Comet/commit/c013e826ba287fc0ece54ee1ca915b1382788779)
fuses the two guarded Wave64 first-lane exchanges into one LDS round trip:
four whole-group barriers become two per affected read. The presence
and value of each half use separate scratch pairs. The original path
remains when there is insufficient LDS; extra allocation reserves the
existing thread-number counter and is bounded by 32 KiB. No approximate
half-wave results or render skipping. The independent scratch-index
model checked 17,344 half-wave slots across group sizes 33–1024 with no
aliasing. [53d0990](https://github.com/Fyntra-Development/Comet/commit/53d099083c8472ac8906eb43a01177db066a7288)
adds guarded first-lane regressions covering lower/upper-only masks,
multiple console waves and partial groups. Windows build, validator and
GPU execution pending. The largest measured title compute pass is not
yet attributed to this operation, so no frame-time gain is claimed.

Benchmark owner: include shader-heavy passes in the already planned
diagnostic timing run, after the new build's shader cache settles. Do not
enable unverified Wave64 approximations just to raise FPS. No duplicate
user capture request and no additional branches.

### Review response — R002 ownership

I am taking R002 (provider admission versus cleanup in `xasync.rs`) in response to the maintainer's request to check reviews. Plan: defer final publication/cleanup until admitted provider invocations return, prevent new invocations after completion has been requested, and add deterministic concurrency regressions. R001 is separate; GPU workers retain their current FPS assignments. No branch creation.

### Texture/page mapping worker — review checked, redundant remap scope

Checked the independent review log before editing. R001/R002 are open runtime async findings outside my rendering scope; R003 is statically fixed. No independent Windows validation of the alias or texture-lock changes is recorded. I am investigating redundant GPU alias operations, especially the constant-page mappings noted in `queue.rs`: only an exact existing nonoverlapping view of the same section and offset may be reused, with protection and GPU invalidation semantics preserved. Keep this experiment isolated from the other workers' GPU compute, hash, readback and pipeline changes. The immediate >5 FPS, intermediate 15 FPS and native 30 FPS targets remain unchanged.


### Readback follow-up — 1 FPS critical path

[0b6816f](https://github.com/Fyntra-Development/Comet/commit/0b6816fe8231b8eded954ad151b9dbaa1c81c280) bulk-merges uncontested 64-byte blocks from card-resident readbacks while retaining the original byte conflict rule. [4db81fc](https://github.com/Fyntra-Development/Comet/commit/4db81fcaefe48fcd6cfb5860dd5d011b9292d211) searches dirty bounds in 256-byte blocks and locates exact byte boundaries inside changed blocks. Regression tests added; a separate JavaScript model checked 64 scalar merge combinations and 61 boundary cases, not the compiled Rust. [f5de3e7](https://github.com/Fyntra-Development/Comet/commit/f5de3e78a3bc15dd8f8b9c9f0a04032538366a94) exposes the existing experimental VRAM merge worker path independently via `-CardMerge Serial|Parallel`, with launcher regression coverage. Benchmark owner: include this as a separate candidate only if readback remains material after the GPU hash fix. No change to synchronization, render work or default merge settings. No post-change world FPS or Windows execution available to me.

Mapping review follow-up: [e4f53f3](https://github.com/Fyntra-Development/Comet/commit/e4f53f3fcdc976937179d8c1b4ba5f8d61e20234) adds opt-in `COMET_REUSE_PAGE_ALIASES=1`. Reuses only a whole destination view with the same section identity/offset, nonoverlapping source, page alignment and complete source span. The fast path restores read-write protection using VirtualProtect, invalidates/re-registers GPU view tracking, and retains normal remapping on mismatch/failure. [bbe25cc](https://github.com/Fyntra-Development/Comet/commit/bbe25cc6910738a378f21204e8e3a1f5d43cc110) tests restored protection, aliased byte visibility, mismatched and overlapping mappings and fallback. `GPU page alias mapping costs` now records `reused_pages`. Windows test and gameplay not executed here. Benchmark owner: if mapping lock costs are still material, compare this switch alone against the same build and scene, not together with batch aliases. A low reuse count means this optimization is not addressing the critical path. Request independent review of protection/guard invalidation and exact view eligibility before considering default enablement.
