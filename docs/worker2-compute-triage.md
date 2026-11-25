# Worker 2 — measured top non-hair compute passes

**Maintainer redirect:** withdraw the generic lane-rank/cleanup candidate from future validation. The batch-1 result was confounded; it is not an individual W2 measurement. Hair belongs to W1 and ray-binning f22a76fcb967ee59 to R2. No source push or new branch.

The benchmark owner's per-command times, correlated with first-bind container sizes in the earlier comet-diagtime.log (03:04:02–03:04:09 UTC):

| Hash | ms/frame | Dispatches/frame | Container bytes | First question |
| --- | ---: | ---: | ---: | --- |
| 5cf00dc39042edaf | 57 | 1 | 23140 | Long single dispatch: grid, loop trip counts, wave and memory operations |
| 4115dd63f99a09d7 | 45 | 144 | 1120 | Small container; 0.3125 ms/dispatch: grid, bandwidth, contention or synchronization |
| 3592426e3e9142d8 | 31 | indirect | 23248 | Actual indirect grid and group dimensions, then loops and memory |
| cb6adcc6da2613ff | 24 | unknown | 5964 | Texture/load work and grid |
| e3730d3d043e0d90 | 23 | unknown | 4864 | Buffer/atomic work and grid |

The first two account for 102/180 ms (56.7%). All five translated and built host compute PSOs in the earlier diagnostic log. Container size is not native instruction count; the earlier log predates corrected per-command timing. Do not infer the cause from hash, size or timing alone.

## Exact replay, highest measured time first

Benchmark owner: reuse existing local captures and diagnostics executable. No extra gameplay run if captures exist. Use the actual wave settings of the measured run. Keep proprietary shader bytes/disassembly local; send aggregate counts, relevant loop-bound sources and resource/dispatch dimensions.

```powershell
$exe = (Resolve-Path '.\target\diag\comet.exe').Path
$source = 'D:\local-comet-shader-capture'
$out = 'D:\worker2-top-compute'
$env:COMET_HOST_MAX_WAVE = '32'
$env:COMET_WAVE_WHY = '1'
Remove-Item Env:COMET_WORKER2_MBCNT -ErrorAction SilentlyContinue
Remove-Item Env:COMET_WORKER2_CLEANUP -ErrorAction SilentlyContinue
$hashes = @('5cf00dc39042edaf','4115dd63f99a09d7','3592426e3e9142d8','cb6adcc6da2613ff','e3730d3d043e0d90')
foreach ($hash in $hashes) {
    $shader = Join-Path $source "$hash.compute.cdxil"
    if (!(Test-Path -LiteralPath $shader)) { Write-Warning "Missing $hash"; continue }
    $dir = Join-Path $out $hash
    New-Item -ItemType Directory -Force $dir | Out-Null
    $env:COMET_TRANSLATE_ONLY = $hash
    $env:COMET_SHADER_CENSUS = Join-Path $dir 'census.tsv'
    Remove-Item -LiteralPath $env:COMET_SHADER_CENSUS -ErrorAction SilentlyContinue
    & $exe shader-info $shader --disasm > (Join-Path $dir 'native.txt')
    if ($LASTEXITCODE -ne 0) { throw "disasm $hash failed" }
    & $exe translate $shader --list --validate --pipelines --jobs 1 *> (Join-Path $dir 'translate.txt')
    if ($LASTEXITCODE -ne 0) { Write-Warning "translate $hash failed" }
    if (Test-Path -LiteralPath $env:COMET_SHADER_CENSUS) {
        Get-Content -LiteralPath $env:COMET_SHADER_CENSUS
    } else { Write-Warning "No emitted census for $hash" }
}
```

Verify the exact hash and emitted census for each. Share console/host wave mode, group and dispatch dimensions, native and emitted static counts, loop-bound sources, and validation/PSO outcome. For the next planned COMET_GPU_TIMING=1 run, retain the card's time by command rows and direct dispatch grid notes; actual indirect arguments if available. Timing mode changes scheduling. Compare normal warmed FPS/P95 separately.

**Implementation gate:** identify the expensive sequence in each exact shader before modifying lowering. No approximate waves, dropped work, reduced loop caps or speculative cleanup patches. Preserve output and dispatch counts. Stage a hash-scoped COMET_ opt-in only after evidence supports it. Required build/Clippy gate before source push. Native 30 FPS ceiling, 15 FPS intermediate. No verified shader-specific fix yet.

Historical candidate: [worker2-wave32-mbcnt.patch at 1f65b05](https://github.com/Fyntra-Development/Comet/blob/1f65b05e878136a3b6b827df0a6b092b7a6c730a/docs/worker2-wave32-mbcnt.patch) is withdrawn.
