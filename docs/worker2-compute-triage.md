# Worker 2 — measured non-hair compute passes

The benchmark owner's 5 FPS sprint assigns f22a76fcb967ee59 (screen-probe ray binning, 105 ms/frame), 5cf00dc39042edaf (57 ms), 4115dd63f99a09d7 (45 ms, 144 dispatches), and 3592426e3e9142d8 (31 ms). Those four reported command totals sum to 238 ms; the broader slice is estimated at ~300 ms. Worker 1 owns the hair solver. Do not assume the ray-binning pass executes BVH intersection simply because its label contains BVH.

## Exact shader replay — existing Windows benchmark owner

Use the existing local shader capture and the current diagnostics executable. Use actual paths outside the repository and the wave size and descriptor view limit from the measured launch. The example uses the recorded 32-lane host. Run in a separate PowerShell session to avoid changing settings for gameplay.

```powershell
$exe = (Resolve-Path '.\target\diag\comet.exe').Path
$source = 'D:\local-comet-shader-capture'
$out = 'D:\worker2-compute-triage'
New-Item -ItemType Directory -Force $out | Out-Null
$hashes = @('f22a76fcb967ee59','5cf00dc39042edaf','4115dd63f99a09d7','3592426e3e9142d8')
$env:COMET_HOST_WAVE = '32'
$env:COMET_HOST_MAX_WAVE = '32'
$env:COMET_WAVE_WHY = '1'
foreach ($hash in $hashes) {
    $shader = Join-Path $source "$hash.compute.cdxil"
    if (!(Test-Path -LiteralPath $shader)) {
        Write-Warning "Missing $hash"
        continue
    }
    $dir = Join-Path $out $hash
    New-Item -ItemType Directory -Force $dir | Out-Null
    & $exe shader-info $shader --disasm > (Join-Path $dir 'native.txt')
    if ($LASTEXITCODE -ne 0) { throw "disasm $hash failed" }
    $env:COMET_TRANSLATE_ONLY = $hash
    $env:COMET_SHADER_CENSUS = Join-Path $dir 'census.tsv'
    $env:COMET_EMIT_DUMP = $dir
    Remove-Item -LiteralPath $env:COMET_SHADER_CENSUS -ErrorAction SilentlyContinue
    & $exe translate $shader --list --validate --pipelines --jobs 1 *> (Join-Path $dir 'translate.txt')
    if ($LASTEXITCODE -ne 0) { Write-Warning "translate $hash failed; retain diagnostic" }
    # The disassembly format contains opcode names and label_XXXX branch targets.
    Select-String -Path (Join-Path $dir 'native.txt') -Pattern 'v_mbcnt_|v_readfirstlane|ds_bpermute|s_barrier|image_bvh|atomic|s_(c)?branch' |
        Measure-Object | Select-Object Count
}
```

Verify each census has an emitted program and each capture filename corresponds to the intended hash. If captures are missing, enable COMET_SHADER_SAVE only during the next already planned diagnostic run; do not ask the user for another separate run. Keep retail disassembly, DXIL and shader bytes local. Share per-hash group and dispatch dimensions, console/host wave mode, census totals, relevant opcode counts, backward-branch offsets, inferred loop counts with their source, validator/PSO outcome, resource bound ranges/null slots and aggregate GPU timing. These are static instruction counts until dynamic timing or invocation counts establish otherwise. Direct branches that jump backward can enclose conditional blocks, so their textual spans do not establish actual executed work.

## Candidate A: exact Wave32 mbcnt lowering (staged, not applied)

[worker2-wave32-mbcnt.patch](worker2-wave32-mbcnt.patch), commit 1f65b05, removes redundant saturation and mask operations in an explicitly declared native Wave32 compute shader. For lane 0..31 the high-half count is always zero, the low-half count is popcount(mask & ((1 << lane) - 1)); for all-ones the result equals the lane number. Bias addition is wrapping u32. The original path remains for Wave64, split waves, mesh, graphics and disabled normal feature-usage recalculation. The opt-in COMET_WORKER2_MBCNT=1 is included in the shader cache key. The candidate changes no dispatches, rendering policy or atomic ordering.

An independent JavaScript arithmetic model matched 166,400 cases (32 lanes, 520 masks, five biases, both halves). All four unified-diff hunks were checked against current source and their declared lengths. The synthetic GPU regression in the patch exercises both paths on multiple and partial waves, dynamic and constant masks, wraparound, DXIL validation when available, and actual output on a Wave32-capable D3D12 adapter. **No Rust compilation or GPU execution here.** A driver may already fold the original expressions. This is a conditional candidate only: if the measured target kernels do not use these instructions in meaningful work, discard it rather than spending scarce benchmark time. It is not evidence of a frame-rate gain.

Coordinate the shared lower.rs and shader_cache.rs hunks with Worker 1. On a clean local main working tree, if the opcode evidence warrants testing:

```powershell
git apply --check docs/worker2-wave32-mbcnt.patch
git apply docs/worker2-wave32-mbcnt.patch
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-shader worker2_wave32_mbcnt_matches_scalar_rank -- --nocapture
```

Do not push candidate source if any command fails. Replay with COMET_WORKER2_MBCNT=0 and =1 in separate processes, holding other flags fixed. Compare census and correctness, then target command durations using the diagnostics build. Run normal release throughput separately with the same candidate executable and separately warmed caches. Compare world FPS/P95, exact hash timings, identical dispatch counts, dropped draws, and visible rendering correctness. If no measurable gain or correctness regression, keep it off or discard.

## Remaining work

Highest priority is measured, exact-program attribution: if the 105 ms pass contains a long memory-dependent loop, identify the induction value and validate the bound and actual bound buffers; if barriers/atomics dominate, determine the synchronization and data dependencies before rewriting. Then inspect the other three measured hashes. No approximate wave semantics, lower loop caps, or skipped shaders count toward the 5 FPS milestone. No source push until required Windows build/clippy gate, no extra remote branch. Connector worker cannot access the retail capture or run Rust/Windows tests, so the benchmark owner's diagnostic summaries and build results are the required handoff.
