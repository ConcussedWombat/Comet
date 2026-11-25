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
