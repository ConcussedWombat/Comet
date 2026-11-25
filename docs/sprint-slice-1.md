# Slice 1 — hair solver (Worker 1)

Scope: c47e03e0e3946741 / e35b7de6a9d2bfd4, shader emit/lower. First investigate user data offset 168 loop count and u4 binding; ask benchmark owner for sanitized disassembly, actual count histogram and resource binding (no title shader bytes in Git). Coordinate shared lowering changes with slice 2. No skipped dispatches or iteration clamps. Validate build/clippy and same-scene A/B before claiming completion.

## Diagnosis and validation handoff

The existing diagnostic capture attributes 193 ms / 80 dispatches to c47e03e0e3946741 and 161 ms / 80 dispatches to e35b7de6a9d2bfd4 (354 ms summed command time, 2.41 and 2.01 ms/dispatch respectively). The separate skip A/B in the review also skipped f22a76fcb967ee59, so its ~270 ms frame-time delta is not an isolated hair measurement. The note about the 7,000-instruction loop is from the benchmark owner's offline inspection, not a verified reproduction here. At 254 iterations that is up to ~1.78 million DXIL instructions per lane on a path executing the entire body; actual active lanes, branch paths, instruction mix and driver ISA may be very different. No LDS/barriers in the observed host shader means replacing barriers is not a supported diagnosis.

### Verify the inputs before changing arithmetic

- At the translated user-data cbuffer, byte offset 168 is word 42 (row 10, component 2). For the solver whose disassembly shows the shift by 24, report `(u42 >> 24) & 255`. Do **not** infer that the other hair shader uses that count, or that 255 is invalid.
- `lower.rs:user` maps native parameter+dword to the host index, subtracting 16 for spilled layouts; word 42 is not necessarily native SGPR 42. `user_at` emits the load at entry, so it is already loop invariant.
- `draw.rs:op_text` annotates **every** user-data value with a heap lookup (even ordinary constants). For tables it divides the value by 32; for other types it uses the value unchanged. It also reconstructs the title's root values instead of the actual final `values` array passed to SetComputeRoot32BitConstants, which can contain regenerated table offsets. Consequently the reported null annotation for `u4=0xa67100` is a lead, not proof of a null UAV. Verify HostShader.user_data's root/slot/dword for index 4, the actual bound heap index or root address and shadow range, and whether the shader really uses that descriptor as a UAV. For index 42, compare the final bound word to the native parameter's actual value.
- Benchmark owner: [worker1-hair-log.py.txt](worker1-hair-log.py.txt) analyzes the existing slow-list notes without publishing retail shader bytes or paths. Run `python docs/worker1-hair-log.py.txt <existing-diagnostic-log>`; `--self-test` passed locally. Notes are the six slowest commands per qualifying list, not an unbiased dispatch census. For a complete histogram, instrument the actual bound values in the existing diagnostics path, limited to these two hashes, and report aggregate counts and null classifications. Please share shader native/host disassembly excerpts around loop entry and backedge and the dominant instruction/resource types.

### Candidate A: fixed CBV row reuse (exact, not applied to the runtime)

[worker1-fixed-cbv-rows.patch.txt](worker1-fixed-cbv-rows.patch.txt) replaces repeated dynamic component selection and repeated CBV row loads for vector constant-buffer accesses where the only dynamic term is the existing 16-byte indexed stride. It does not assume extra offsets, change the count, widen accesses, modify resource bindings, or alter float semantics. For a four-dword aligned access the emitted row loads go from four to one; a crossing row needs two. The driver may already optimize some of this; **first check whether these shaders actually contain the affected accesses**. If not, defer this candidate and investigate the measured instruction mix. This touches a shared lowering helper, so Worker 2 should not apply an overlapping patch simultaneously.

**Revised patch [47c917f](https://github.com/Fyntra-Development/Comet/commit/47c917fa5eb3373bbcd933689137e4cd78340b97):** unset or `COMET_HAIR_CBV_ROWS=1` preserves the original batch-1 CBV path for every stage, by explicit maintainer direction following the visual fix. `COMET_HAIR_CBV_ROWS=0` restores old lowering. A comma-separated list of captured DXBC header digests (32 hex characters each, not the 16-digit BLAKE3 log hashes) selects matching compute shaders only, for attribution. Set `COMET_HAIR_CBV_TRACE=1` during replay for static counters. No changes to GPU cache or shared runtime files; use separate cache directories, because the cache does not include this env value. The alternate path logs `HAIR-CBV-ROWS: sites=N row_loads_removed_before_opt=M` on successful lowering; these are static sites, not dynamic instruction counts. A zero count rules this candidate out for the affected shader. For an eligible load, the address is unchanged and the three component comparisons/selects per dword disappear; the generic driver may already fold some of them. No evidence yet that this explains most of the runtime gap.

All ten patch hunks were checked against lower.rs blob c486ecd3f295be0b335ce229a2687f45c13ea1d3 and emit/mod.rs blob 57d52937235f8327fdb3b0b08ecad91b6134b799. An independent arithmetic model covered 800 low-nibble/row/count/component cases including unsigned address wrap. The patch includes Rust tests of wrapping row selection and digest selection. This is **not** a compiled Rust or DXIL validator result.

On the Windows benchmark machine, from current main, with no concurrent local changes:

```powershell
git apply --check docs/worker1-fixed-cbv-rows.patch.txt
git apply docs/worker1-fixed-cbv-rows.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-shader fixed_cbv_components_match_wrapping_byte_addresses --locked
cargo test -p comet-shader hair_cbv_selection_rejects_missing_or_wrong_digests --locked
```

### Exact A/B commands (Windows owner)

Use the existing local `.cdxil` captures, outside Git. If the two captures have no distinct nonzero header digests, stop rather than broadening the selection. The log hashes identify full containers by BLAKE3; they are NOT the header digests.

```powershell
$cap = 'D:\local-comet-shader-capture' # replace with actual local capture dir
$hashes = @('c47e03e0e3946741','e35b7de6a9d2bfd4')
$digests = @($hashes | ForEach-Object {
    $files = @(Get-ChildItem -LiteralPath $cap -Filter "$_.*.cdxil" -File)
    if ($files.Count -ne 1) { throw "Need exactly one capture for $_" }
    $b = [IO.File]::ReadAllBytes($files[0].FullName)
    if ($b.Length -lt 32 -or [Text.Encoding]::ASCII.GetString($b,0,4) -ne 'DXBC') {
        throw "Not a DXBC container: $_"
    }
    $d = [BitConverter]::ToString($b,4,16).Replace('-','').ToLowerInvariant()
    if ($d -eq ('0' * 32)) { throw "Zero header digest: $_" }
    $d
})
if (($digests | Sort-Object -Unique).Count -ne 2) { throw "Non-distinct digests" }
$env:COMET_TRANSLATE_ONLY = $hashes -join ','
$env:COMET_HAIR_CBV_ROWS = '0'
$env:COMET_HAIR_CBV_TRACE = '1'
# Use the diagnostics-enabled release executable as built by the owner.
.\target\release\comet.exe translate $cap --list --validate --pipelines --jobs 1
$env:COMET_HAIR_CBV_ROWS = $digests -join ','
.\target\release\comet.exe translate $cap --list --validate --pipelines --jobs 1
Remove-Item Env:COMET_TRANSLATE_ONLY -ErrorAction SilentlyContinue
```

Record the emitted HAIR-CBV-ROWS line for each hash and baseline/candidate DXIL census. If sites=0 for both, the direct hair-shader CBV path cannot explain the visual result; compare the global/default version against the hair-only version and inspect upstream producers. Keep the observed visual correction until causality is established. A row count decrease before the optimizer does not guarantee a decrease in final DXIL or native instructions. Confirm validation and PSO creation for both shaders. Inspect other shaders for duplicate header digests if using a broader local capture set; do not enable if the identity is ambiguous.

For gameplay, use **separate warmed `COMET_CACHE_DIR` paths** (e.g. `D:\comet-w1-baseline` and `D:\comet-w1-candidate`) with the same executable, scene, and other flags. Set `COMET_HAIR_CBV_ROWS=0` for the pre-fix process and `=1` for the original batch-1 correction; the digest selection is a third experiment. Do not reuse either cache for a different digest selection. This deliberately avoids colliding with Worker 2's cache-key patch; before permanent integration, ask the cache owner to include the selection in the cache key.

In a diagnostics build, set `COMET_GPU_TIMING=1` for both runs. For each comparable frame, both hashes must still have the same dispatch count (~80 each), same dimensions, and comparable actual loop-count distribution. Compare per-hash **total GPU milliseconds and milliseconds per dispatch**: baseline 193 ms / 2.41 ms and 161 ms / 2.01 ms respectively. A first meaningful target is at least 10% lower for an affected shader (<=173.7 ms / <=2.17 ms, or <=144.9 ms / <=1.81 ms), not a claimed gain. Stretch goal is an order of magnitude, which this isolated CBV fix alone may not achieve. Record P50/P95 of repeated dispatch timing where possible. No new dropped draws/dispatches; preserve correct positions, finite vertex output and identical simulation iteration count. Then run normal release-mode warm-world FPS/P95 with timing off; do not use diagnostic-mode FPS as acceptance. The maintainer explicitly wants the CBV path retained for correctness even if it incurs a regression. Do not interpret a faster corrupted render as success.

### R1 / W2 coordination

R1: please log actual final bound word 42 (not reconstructed title state) and its `HostShader.user_data` slot/root/dword mapping for c47e; check u4's actual descriptor kind, bound heap index, base/range and validity on both hashes. Check input contents at the solver entry, nonfinite/implausible output positions after each solver and before vertex fetch, and readback/upload ordering. Compare input/output after your binding fix before concluding the shader math or loop count is wrong. Do not clamp count or skip solver work. I will keep my changes in lowering only. If the actual loop high byte is excessive relative to the native dispatch's expected count, that is a binding/data correctness lead, not authorization to reduce it.

W2: my candidate touches lower.rs and emit/mod.rs, not shader_cache.rs; the hunk locations do not coincide with your mbcnt hunk at the time of writing. Apply/test independently first. Coordinate a cache-key addition only before permanent integration; no cross-worker source overwrite. 15 fps intermediate, 30 fps title ceiling. No screenshots, captures or other game assets in the repo.

**Status:** input source mapping reviewed; runtime count/binding verification, exact shader replay, Windows build/clippy, and measured speedup remain pending. No claim that the 1 FPS issue is resolved.

The candidate files use `.txt` suffixes to comply with the existing asset-policy extension allowlist. Python can execute a text-named script directly; `git apply` does not depend on the extension. The cache fingerprints the executable, but not COMET_HAIR_CBV_ROWS; use separately warmed cache directories even with the same candidate build.


## Follow-up — isolate the ~10% regression, then optimize the solver

The owner's batch-1 results are 1.77 FPS (565 ms) on reference Relay versus 1.59 FPS (628 ms) with batch switches off: 63 ms/frame extra, 10.2% lower FPS (~11.2% longer frames). The three remaining candidates are W1 CBV, W6 intervals, and W5 lifetime/present. Run 8 (batch without W1) is in progress per the review; no result posted as of this handoff. Keep CBV for correctness as directed.

Use the exact same batch build/configuration for the next comparison, with **only** CBV varied: original all-stage correction (`=1`/unset) vs `=0`. If disabling restores ~565 ms/frame, CBV accounts for much of the measured regression; if not, continue the W6/W5 bisect. Then compare global vs two-digest selection. If the hair-only setting still corrects hair and restores the rest of the frame, prefer the scoped behavior after validation. With `COMET_GPU_TIMING=1`, report GPU ms/dispatch and count for both solvers plus other passes, dropped draws and actual work. Correctly rendered strands can change downstream geometry/lighting work, so a frame delta alone is not proof of slower hair kernels. Separate cache directories for all three settings. Do not change R1 binding flags during this comparison.

## Candidate B — actual solver hot-loop analysis and arithmetic LICM

[worker1-hair-loop-licm.patch.txt](worker1-hair-loop-licm.patch.txt) ([654181f](https://github.com/Fyntra-Development/Comet/commit/654181f83b2129996f80fe4488012b8d44d10c9f)) must be applied **after the latest CBV patch**. It adds a natural-loop census after the existing normal DXIL optimizer (which has no loop-invariant motion) and an optional conservative LICM experiment. Only the selected compute digests receive it. Normal shader optimization must remain enabled.

- `COMET_HAIR_LOOP_DIGESTS`: comma-separated exact nonzero DXBC header digests from the captures above.
- `COMET_HAIR_LOOP=trace`: no code motion; prints `HAIR-NATIVE` opcode histogram and `HAIR-LOOP` per natural loop: body ops, phis, CBV loads, other memory/effect calls, wave/quad ops, selects, full eligible arithmetic count and moved=0. Offline replay each hash separately so output cannot be misattributed. The counters are static, not dynamic instruction execution or hardware ISA. Check especially the reported ~7k body and select inflation (e.g. `v_movrels_b32`, `v_movreld_b32`, `v_movrelsd_b32` lower to chains of compare/select per VGPR).
- `COMET_HAIR_LOOP=1`: same trace, moves up to 32 eligible arithmetic instructions per loop into a unique unconditional preheader. Requires operands available there and source block to dominate all latches and exits. No memory loads/stores, resource handles, wave/quad calls, phi, branches, iteration guards, float reassociation or iteration count changes. No change if preheader is unsuitable. Extra live values can worsen register pressure; this is deliberately bounded. A high candidate count suggests more headroom; zero rules out this candidate. Even 32 moved instructions in a 7k-op loop is under 0.5% of static body count; do not claim a 10x gain. Treat this as a diagnostic/first optimization, not the complete hair fix.

```powershell
git apply --check docs/worker1-fixed-cbv-rows.patch.txt
git apply docs/worker1-fixed-cbv-rows.patch.txt
git apply --check docs/worker1-hair-loop-licm.patch.txt
git apply docs/worker1-hair-loop-licm.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-shader hair_loop_ --locked
$env:COMET_HAIR_CBV_ROWS = '1'
$env:COMET_HAIR_LOOP_DIGESTS = $digests -join ','
$env:COMET_HAIR_LOOP = 'trace'
# Use the diagnostics-enabled release executable; repeat per hash.
$env:COMET_TRANSLATE_ONLY = 'c47e03e0e3946741'
.\target\release\comet.exe translate $cap --list --validate --pipelines --jobs 1
$env:COMET_HAIR_LOOP = '1'
.\target\release\comet.exe translate $cap --list --validate --pipelines --jobs 1
Remove-Item Env:COMET_TRANSLATE_ONLY -ErrorAction SilentlyContinue
```

Repeat for e35b7de6a9d2bfd4. If no instructions moved, do not spend a gameplay run on LICM. Otherwise compare separately warmed cache directories for baseline CBV-on, LICM-on; capture total GPU ms and ms/dispatch, 80 commands each, same thread groups, actual loop counts and correct hair positions, then normal FPS/P95. The initial target is lower per-hash GPU ms with no extra dropped draws. To establish a dominant cause, capture native opcode histogram, actual iteration distribution from R1, DXIL loop census and GPU counters (registers/spills, occupancy, memory stalls if available). A hot loop with excessive selects and native indexed moves would justify a balanced indexed-register lowering experiment; memory-bound and arithmetic-bound cases need different remedies. Do not blindly alter the loop bound or omit solver passes.

All source changes remain staged only; the connector session cannot compile Rust or execute Windows/GPU. W2/R4 coordinate shared lower.rs and mod.rs hunks; no cache or renderer edits. Ask cache owner to include all translation switches in cache keys before permanent integration; use separate cache directories until then.
