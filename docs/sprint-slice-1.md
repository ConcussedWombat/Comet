# Slice 1 — hair solver (Worker 1)

Scope: c47e03e0e3946741 / e35b7de6a9d2bfd4, shader emit/lower. First investigate user data offset 168 loop count and u4 binding; ask benchmark owner for sanitized disassembly, actual count histogram and resource binding (no title shader bytes in Git). Coordinate shared lowering changes with slice 2. No skipped dispatches or iteration clamps. Validate build/clippy and same-scene A/B before claiming completion.

## Diagnosis and validation handoff

The existing diagnostic capture attributes 193 ms / 80 dispatches to c47e03e0e3946741 and 161 ms / 80 dispatches to e35b7de6a9d2bfd4 (354 ms summed command time, 2.41 and 2.01 ms/dispatch respectively). The separate skip A/B in the review also skipped f22a76fcb967ee59, so its ~270 ms frame-time delta is not an isolated hair measurement. The note about the 7,000-instruction loop is from the benchmark owner's offline inspection, not a verified reproduction here. At 254 iterations that is up to ~1.78 million DXIL instructions per lane on a path executing the entire body; actual active lanes, branch paths, instruction mix and driver ISA may be very different. No LDS/barriers in the observed host shader means replacing barriers is not a supported diagnosis.

### Verify the inputs before changing arithmetic

- At the translated user-data cbuffer, byte offset 168 is word 42 (row 10, component 2). For the solver whose disassembly shows the shift by 24, report `(u42 >> 24) & 255`. Do **not** infer that the other hair shader uses that count, or that 255 is invalid.
- `lower.rs:user` maps native parameter+dword to the host index, subtracting 16 for spilled layouts; word 42 is not necessarily native SGPR 42. `user_at` emits the load at entry, so it is already loop invariant.
- `draw.rs:op_text` annotates **every** user-data value with a heap lookup (even ordinary constants). For tables it divides the value by 32; for other types it uses the value unchanged. It also reconstructs the title's root values instead of the actual final `values` array passed to SetComputeRoot32BitConstants, which can contain regenerated table offsets. Consequently the reported null annotation for `u4=0xa67100` is a lead, not proof of a null UAV. Verify HostShader.user_data's root/slot/dword for index 4, the actual bound heap index or root address and shadow range, and whether the shader really uses that descriptor as a UAV. For index 42, compare the final bound word to the native parameter's actual value.
- Benchmark owner: [worker1-hair-log.py.txt](worker1-hair-log.py.txt) analyzes the existing slow-list notes without publishing retail shader bytes or paths. Run `python docs/worker1-hair-log.py.txt <existing-diagnostic-log>`; `--self-test` passed locally. Notes are the six slowest commands per qualifying list, not an unbiased dispatch census. For a complete histogram, instrument the actual bound values in the existing diagnostics path, limited to these two hashes, and report aggregate counts and null classifications. Please share shader native/host disassembly excerpts around loop entry and backedge and the dominant instruction/resource types.

## Landed correctness fix and measured cost

Batch 1 landed the original c05d0b9 CBV lowering by maintainer direction. **Do not apply the old worker1-fixed-cbv-rows.patch.txt again.** Its later staged rollback/digest version ([47c917f](https://github.com/Fyntra-Development/Comet/commit/47c917fa5eb3373bbcd933689137e4cd78340b97)) was not the landed version. The existing CBV row reuse remains unchanged and enabled on main.

Benchmark owner, same in-world 20–140 s window, Relay: 1.21 FPS reference, 1.16 without W1, 0.99 with W1. The W1 delta is ~15% FPS or ~148 ms/frame versus the without-W1 batch. Hair no longer explodes; preserve the fix. This is frame throughput evidence, not proof that the extra time is directly in either hair shader. Capture post-fix per-hash GPU timing, dispatch dimensions/counts, other passes and readback, to distinguish added valid work from translation overhead.

## Candidate B — solver hot-loop census and arithmetic LICM (rebased)

[worker1-hair-loop-licm.patch.txt](worker1-hair-loop-licm.patch.txt) ([ad0f6bd](https://github.com/Fyntra-Development/Comet/commit/ad0f6bd09e0b069879a0b4c09e18817cb3ea8738)) applies **directly to current post-batch-1 main** (base 1dd0b6fb87f4f7837291337173500dc236069a1e). No other patch is required. The landed CBV implementation is untouched. The mode is passed through the existing lower/run/finish path, and exact digest selection is self-contained. Both COMET_HAIR_LOOP and COMET_HAIR_LOOP_DIGESTS are now included in the shader-cache key, per benchmark owner's cache-key direction. This is the only non-shader file hunk; coordinate with the cache owner and R2/R4 when applying their cache-key edits. No renderer changes.

The normal DXIL optimizer has no LICM. This patch adds a natural-loop census after the normal optimization and an optional conservative arithmetic LICM experiment. Only selected compute digests receive it.

- `COMET_HAIR_LOOP_DIGESTS`: comma-separated exact nonzero DXBC header digests from the two captures (NOT the 16-digit BLAKE3 log hashes).
- `COMET_HAIR_LOOP=trace`: no code motion; prints `HAIR-NATIVE` opcode histogram and `HAIR-LOOP` per natural loop: body ops, phis, CBV loads, other memory/effect calls, wave/quad ops, selects, full eligible arithmetic count and moved=0. Replay one hash at a time. Counters are static, not dynamic hardware ISA. Check especially select inflation and the 7k body: v_movrels_b32, v_movreld_b32, v_movrelsd_b32 lower to chains of compare/select per VGPR.
- `COMET_HAIR_LOOP=1`: same trace, moves up to 32 eligible arithmetic instructions per loop to a unique unconditional preheader. Requires operands available there and source block dominating all latches/exits. No memory, handles, wave/quad, phi, branches, iteration guards/counts or FP reassociation changed. Extra live values may worsen register pressure. Zero moved rules this candidate out; even 32 in a 7k-op loop is <0.5% of static body. This is an initial optimization and diagnostic, not a demonstrated dominant speedup.
- Unset or `0`: original post-CBV lowering and no loop diagnostic. Normal shader optimization must stay enabled.

### Build gate and exact A/B

```powershell
git apply --check docs/worker1-hair-loop-licm.patch.txt
git apply docs/worker1-hair-loop-licm.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-shader hair_loop_ --locked
```

Use the diagnostics-enabled release executable. Capture files are local only, outside Git; if no distinct nonzero header digests, stop instead of broadening selection.

```powershell
$cap = 'D:\local-comet-shader-capture' # actual local capture directory
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
$env:COMET_HAIR_LOOP_DIGESTS = $digests -join ','
foreach ($hash in $hashes) {
    $env:COMET_TRANSLATE_ONLY = $hash
    $env:COMET_HAIR_LOOP = 'trace'
    .\target\release\comet.exe translate $cap --list --validate --pipelines --jobs 1
    $env:COMET_HAIR_LOOP = '1'
    .\target\release\comet.exe translate $cap --list --validate --pipelines --jobs 1
}
Remove-Item Env:COMET_TRANSLATE_ONLY -ErrorAction SilentlyContinue
```

If moved=0, do not spend a gameplay run on LICM; send the census. Otherwise compare trace/off vs LICM-on on same binary and warmed cache per mode (new cache keys separate them). For `COMET_GPU_TIMING=1` compare per-hash GPU ms/frame and ms/dispatch, same ~80 commands each, dimensions, actual iteration distribution and no new dropped draws. Historical pre-CBV numbers 193 ms / 2.41 ms and 161 ms / 2.01 ms per dispatch are context only; establish fresh post-CBV baseline first. Initial target >=10% lower per affected hash relative to current baseline; eventual goal ~10x reduction. Then normal release timing-off FPS/P95, same 20–140 s scene. Keep correct strand positions. No iteration clamp or dispatch skipping.

R1: actual bound u42 mapping/loop histogram, u4 descriptor validity and strand outliers. If hot loops have excessive selects and native relative register moves, prioritize indexed-VGPR lowering; if memory-bound, target memory/descriptor overhead, not arithmetic. Request local native/host loop excerpts and GPU register/spill/occupancy/stall counters, not retail bytes. W2/R4: coordinate shared lower.rs/mod.rs and cache-key contexts.

Static verification: all 10 hunks matched the pinned main source and sequential line counts. No Rust/Windows execution available in worker environment; Windows build-gate owner must compile/lint/test. No application source pushed, no branches/PRs/assets. Priority is solver performance without regressing the landed hair correction.
