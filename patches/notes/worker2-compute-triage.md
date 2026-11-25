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


## Batch 3 — one exact LDS-read-run candidate (W2)

Batch-1 MBCNT landed opt-in, but 1.57 vs 1.59 FPS all switches on/off showed no measurable benefit. Leave it off for this independent comparison. Batch 2 baseline is c5317cf; this patch was generated against then-current main after reading the Prior art section.

**Prior art checked:** [fxceless/victory-fork 578d459c](https://github.com/fxceless/victory-fork/commit/578d459c344aa44753e9a61ea6ba10a8c68f6ea6) introduced the split-wave LDS meeting scheme, already in Comet. Its comment explicitly requires a barrier between accesses of which one writes. The [16849898 cleanup](https://github.com/fxceless/victory-fork/commit/16849898f3282c4a7189b1557289c194bc720052) (~20 ms GPU/frame on their PC) is already in Comet; emit/bitcode/opt.rs, ir.rs and resource.rs were compared to the current fork and contain the same substantive logic. [90228a7d](https://github.com/fxceless/victory-fork/commit/90228a7dc66010281fdcfad10ac00c3a252be2f3) is already reflected in ir.rs; it is an irradiance-cache translation fix, not proof about the five heavy kernels. The upstream [3bff182e](https://github.com/fxceless/victory-fork/commit/3bff182ef4966ad6986fd8552dc9c7c58268df07) FIRSTLANE_OWN shortcut is not exact for arbitrary divergent values and is not ported. The fork's README still describes black shadowed regions and RT off by default. The meshlet culling view-kind fix concerns descriptor binding, not a demonstrated GPU-time reduction in any of W2's five hashes; leave that to its assigned owner.

**Candidate:** [worker2-lds-read-runs.patch.txt](worker2-lds-read-runs.patch.txt) (credit fxceless/VIctory's split-LDS design at 578d459c). This is a new narrow derivative optimization, not a verbatim upstream commit. When two consecutive unguarded ordinary LDS reads are in the same fully lowered block on an exact split-wave path, keep the first read's meeting and omit the second. Keep all write/atomic meetings, explicit barriers, wave-section and divergent-region code. No changes to native Wave32, unsplit Wave64, other shader stages, memory accesses, loop bounds, hair/RT or dispatches. The patch includes a synthetic 64-thread lowering regression, uses the same native DS encoding as valu_tests, and adds the switch to the shader cache key (the only comet-gpu hunk; benchmark owner owns integration of that shared line). All 10 old-context hunks matched current main on recheck. An independent linear access-order model enumerated 29,523 sequences of 1–9 read/write/other operations and preserved every pair involving a write; this is not Rust or GPU execution. Rust compilation, validator, PSO and runtime output remain unverified.

Enable only for selected exact target captures. The switch value is a comma-separated list of BLAKE3-prefix:DXBC-header-digest:DXIL-byte-count:FNV64 keys, as emitted by the prior W2 capture-key helper. Unset disables. The helper verifies hashes of the actual captures; a filename is not enough. A zero digest or an unassigned hash is refused. For convenience this self-contained Python snippet can be run locally in PowerShell (install the blake3 Python module if absent):

```powershell
$source = 'D:\local-comet-shader-capture'
$env:COMET_WORKER2_LDS_READ_RUNS = @'
from pathlib import Path
from blake3 import blake3
import struct, sys
wanted = {'5cf00dc39042edaf', '4115dd63f99a09d7', '3592426e3e9142d8', 'cb6adcc6da2613ff', 'e3730d3d043e0d90'}
keys, seen = {}, {}
for p in Path(sys.argv[1]).rglob('*.cdxil'):
    b = p.read_bytes()
    h = blake3(b).hexdigest()[:16]
    if len(b) < 32 or b[:4] != b'DXBC':
        continue
    total, count = struct.unpack_from('<II', b, 24)
    if total != len(b) or count > 256 or 32 + 4*count > total or not any(b[4:20]):
        continue
    for i in range(count):
        o, = struct.unpack_from('<I', b, 32 + i*4)
        if o + 8 > total or b[o:o+4] != b'DXIL':
            continue
        n, = struct.unpack_from('<I', b, o+4)
        if o+8+n > total:
            break
        f = 0xcbf29ce484222325
        for v in b[o+8:o+8+n]:
            f = ((f ^ v) * 0x100000001b3) & 0xffffffffffffffff
        k = f'{b[4:20].hex()}:{n}:{f:016x}'
        seen.setdefault(k, set()).add(h)
        if h in wanted:
            keys[h] = k
        break
if not keys or any(seen[k] != {h} for h, k in keys.items()):
    raise SystemExit('No usable exact W2 captures or ambiguous fingerprint')
print(','.join(f'{h}:{k}' for h,k in sorted(keys.items())))
'@ | python - $source
if ($LASTEXITCODE -ne 0 -or !$env:COMET_WORKER2_LDS_READ_RUNS) { throw 'W2 capture key failed' }
```

Apply only this patch to current main (do not apply the old MBCNT patch). Then:

```powershell
git apply --check docs/worker2-lds-read-runs.patch.txt
git apply docs/worker2-lds-read-runs.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-shader worker2_lds_read_runs_keep_write_boundaries --locked
cargo build --profile diag -p comet --features diagnostics --locked
```

Run exact offline replay with the switch set vs removed, keeping identical flags and target hash. Log `W2-LDS-READ-RUNS: digest=... sites=N` for selected translation attempts; final selected host DXIL census `barrier` should decrease if the candidate applied to the final translation. If zero sites or no final census decrease for all five, discard the candidate without a gameplay run. Otherwise use COMET_GPU_TIMING=1 in the same warmed scene to compare **the card's time by command (timing run), ms a frame and commands a frame** for 5cf00dc39042edaf (57 ms, 1 dispatch), 4115dd63f99a09d7 (45 ms, 144), 3592426e3e9142d8 (31 ms indirect), cb6adcc6da2613ff (24 ms), e3730d3d043e0d90 (23 ms). A lower per-hash ms/frame with identical dispatch counts, correct rendering and no device loss is the expected effect for affected shaders only. No quantitative savings claim without the exact capture and dynamic count. Use normal non-timing throughput separately. No source push, no branches or proprietary assets.
