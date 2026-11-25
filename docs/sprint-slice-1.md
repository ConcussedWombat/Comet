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

**Revised patch [c224278](https://github.com/Fyntra-Development/Comet/commit/c224278904a90af6585e5f9e8a3839a8b7d7c768):** now defaults off. Set `COMET_HAIR_CBV_ROWS` to a comma-separated list of the two captured DXBC header digests (32 hex characters each, not the 16-digit BLAKE3 log hashes). Only compute containers whose nonzero header digest matches can enter the alternate lowering. No changes to GPU cache or shared runtime files; use separate cache directories, because the cache does not include this env value. The alternate path logs `HAIR-CBV-ROWS: sites=N row_loads_removed_before_opt=M` on successful lowering; these are static sites, not dynamic instruction counts. A zero count rules this candidate out for the affected shader. For an eligible load, the address is unchanged and the three component comparisons/selects per dword disappear; the generic driver may already fold some of them. No evidence yet that this explains most of the runtime gap.

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
Remove-Item Env:COMET_HAIR_CBV_ROWS -ErrorAction SilentlyContinue
# Use the diagnostics-enabled release executable as built by the owner.
.\target\release\comet.exe translate $cap --list --validate --pipelines --jobs 1
$env:COMET_HAIR_CBV_ROWS = $digests -join ','
.\target\release\comet.exe translate $cap --list --validate --pipelines --jobs 1
Remove-Item Env:COMET_TRANSLATE_ONLY -ErrorAction SilentlyContinue
```

Record the emitted HAIR-CBV-ROWS line for each hash and baseline/candidate DXIL census. If sites=0 for both, do not spend a gameplay A/B on this patch; revert locally and provide the exact native/host loop disassembly and opcode counts. A row count decrease before the optimizer does not guarantee a decrease in final DXIL or native instructions. Confirm validation and PSO creation for both shaders. Inspect other shaders for duplicate header digests if using a broader local capture set; do not enable if the identity is ambiguous.

For gameplay, use **separate warmed `COMET_CACHE_DIR` paths** (e.g. `D:\comet-w1-baseline` and `D:\comet-w1-candidate`) with the same executable, scene, and other flags. Set `COMET_HAIR_CBV_ROWS` only in the candidate process. Do not reuse either cache for a different digest selection. This deliberately avoids colliding with Worker 2's cache-key patch; before permanent integration, ask the cache owner to include the selection in the cache key.

In a diagnostics build, set `COMET_GPU_TIMING=1` for both runs. For each comparable frame, both hashes must still have the same dispatch count (~80 each), same dimensions, and comparable actual loop-count distribution. Compare per-hash **total GPU milliseconds and milliseconds per dispatch**: baseline 193 ms / 2.41 ms and 161 ms / 2.01 ms respectively. A first meaningful target is at least 10% lower for an affected shader (<=173.7 ms / <=2.17 ms, or <=144.9 ms / <=1.81 ms), not a claimed gain. Stretch goal is an order of magnitude, which this isolated CBV fix alone may not achieve. Record P50/P95 of repeated dispatch timing where possible. No new dropped draws/dispatches; preserve correct positions, finite vertex output and identical simulation iteration count. Then run normal release-mode warm-world FPS/P95 with timing off; do not use diagnostic-mode FPS as acceptance. If the candidate gives no material improvement, do not land it.

### R1 / W2 coordination

R1: please log actual final bound word 42 (not reconstructed title state) and its `HostShader.user_data` slot/root/dword mapping for c47e; check u4's actual descriptor kind, bound heap index, base/range and validity on both hashes. Check input contents at the solver entry, nonfinite/implausible output positions after each solver and before vertex fetch, and readback/upload ordering. Compare input/output after your binding fix before concluding the shader math or loop count is wrong. Do not clamp count or skip solver work. I will keep my changes in lowering only. If the actual loop high byte is excessive relative to the native dispatch's expected count, that is a binding/data correctness lead, not authorization to reduce it.

W2: my candidate touches lower.rs and emit/mod.rs, not shader_cache.rs; the hunk locations do not coincide with your mbcnt hunk at the time of writing. Apply/test independently first. Coordinate a cache-key addition only before permanent integration; no cross-worker source overwrite. 15 fps intermediate, 30 fps title ceiling. No screenshots, captures or other game assets in the repo.

**Status:** input source mapping reviewed; runtime count/binding verification, exact shader replay, Windows build/clippy, and measured speedup remain pending. No claim that the 1 FPS issue is resolved.

The candidate files use `.txt` suffixes to comply with the existing asset-policy extension allowlist. Python can execute a text-named script directly; `git apply` does not depend on the extension. The cache fingerprints the executable, but not COMET_HAIR_CBV_ROWS; use separately warmed cache directories even with the same candidate build.
