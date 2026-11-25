# R4 — terrain / hedge pipeline handoff

Read the 10-worker assignment. Source candidate: [worker-r4-pso-retry.patch.txt](worker-r4-pso-retry.patch.txt), revision [fa41d9c](https://github.com/Fyntra-Development/Comet/commit/fa41d9c779fdda102fc3f7c82fc0d169f96ea091). **COMET_R4_PSO_RETRY=1**, off by default. Only `crates/comet-gpu/src/pipeline/host.rs` is changed by application of the patch. All seven hunks replayed against blob `20a15abb1ac44fd09bef6c483f1d054f7592e449`. No application source pushed, no new branch, no PR. No Rust toolchain or Windows/game here; compile, clippy and runtime acceptance remain open.

**Do not apply both R4 patches.** A second candidate [worker-r4-pso.patch.txt](worker-r4-pso.patch.txt) appeared on main during this work and touches the same host file. Prefer this retry-only alternative for the first isolated A/B: it validates only after E_INVALIDARG, logs each shader's declared resource bindings, uses distinct cache keys for actual signed bytes, tries equivalent compute stream creation if the regular descriptor is still refused, and canonicalizes only unused mesh RTV/blend fields. Active RTs, depth/stencil, input resource ABI, stage code and topology are not replaced or faked. Any DXIL validator rejection stops the retry; a successful PSO still needs real shader output checked. Coordinate with the other R4 candidate owner before merging.

## Baseline and exact metric

The saved diagnostic capture (03:05:09 and 03:05:14 UTC, same scene) reports:
- compute 02384a60fda6f2d0, 2cc4c197ee29e291, 2da4beb386509b77, 369cbe906ea72764: each 1 dropped/frame, translated then E_INVALIDARG;
- mesh f5e50eb2108c425b + pixel ce3f0432a08f1d35: 1/frame (occasionally 2);
- pixel 6b6db9f1a526daeb: 9/frame, **translation**, not PSO creation, fails at 0x49dc ("a descriptor read only as data is not supported"). This needs a shader resource-tracking owner; the current assignment restricts my shader changes to f133.
- f133e35efed2a48a: compute translation fails at 0x1eb4 ("a lane mask exchanged where not every lane runs"), 0.4/frame in one earlier interval; not observed in the above steady intervals.
- RT builder off: 755 or 820/frame in these intervals, scene-dependent; separate R2 / builder gate. Do not force builder stages to run.

Conditional target for this patch: four compute refusal counters 1.0 -> 0 each and mesh refusal 1.0 -> 0 (2 -> 0 when twice drawn), total **5–6 fewer dropped draws/dispatches per frame** if all five create. This is a target, not a measured or guaranteed change. At a comparable 430/frame baseline, 425–424/frame with other causes unchanged. Pixel and f133 not fixed by this PSO patch; even restoring them only removes their own counts unless downstream changes are measured. The dominant RT-builder-off line should remain unchanged here. A restored producer can feed many geometry/LOD draws without a large drop-count change; verify images and buffer contents separately.

## Windows gate

Apply on a clean current main worktree. Do not apply the competing R4 patch simultaneously.

```powershell
git apply --check docs/worker-r4-pso-retry.patch.txt
git apply docs/worker-r4-pso-retry.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-gpu --lib --locked r4_mesh_canonicalizes_only_inactive_fields
cargo build --release --locked -p comet
```

Run same warm scene and other flags with COMET_R4_PSO_RETRY unset vs 1. Report same per-hash drop reasons, counts, FPS/P95, device loss and terrain/hedge correctness; keep GPU timing off for throughput. For diagnosis only use a separate diagnostics build (`cargo build --release --locked -p comet --features diagnostics`) and COMET_D3D12_DEBUG=1. Capture the R4 PSO shader bindings, refused compute/mesh descriptor, DXIL valid/invalid, signed/stream/mesh PSO retry accepted lines and D3D12 message IDs/text. No debug-layer errors were present in the saved log, so the precise rejection root cause is not yet known.

## f133 exact replay and streamed geometry

Do not run a globally widened Wave64 translation in the game: previous widen experiments lost the GPU, and the shader translator's widening guard only permits a single 64-thread console wave when exchange barriers are necessary. Do not remove all-lane guards or accept approximate half-wave geometry producers. Use existing cache-keyed wave switches offline to discover the shape, with local captures outside the repo:

```powershell
# Set before a diagnostic capture if these shaders aren't already saved:
$env:COMET_SHADER_SAVE = 'D:\local-comet-shader-capture'
# After capture, use actual local directory. Do not upload the bytes.
Remove-Item Env:COMET_SHADER_SAVE -ErrorAction SilentlyContinue
$env:COMET_HOST_MAX_WAVE = '32'
$env:COMET_WAVE_WHY = '1'
$env:COMET_WAVE_LOCAL = 'idiom'
$env:COMET_TRANSLATE_ONLY = '02384a60fda6f2d0,2cc4c197ee29e291,2da4beb386509b77,369cbe906ea72764,f5e50eb2108c425b,ce3f0432a08f1d35,6b6db9f1a526daeb,f133e35efed2a48a'
.\target\release\comet.exe translate 'D:\local-comet-shader-capture' --list --validate --pipelines --jobs 1
$env:COMET_TRANSLATE_ONLY = 'f133e35efed2a48a'
$env:COMET_WAVE_WIDE = '1'
.\target\release\comet.exe translate 'D:\local-comet-shader-capture' --list --validate --pipelines --jobs 1
Remove-Item Env:COMET_WAVE_WIDE,Env:COMET_WAVE_LOCAL,Env:COMET_WAVE_WHY,Env:COMET_TRANSLATE_ONLY,Env:COMET_HOST_MAX_WAVE -ErrorAction SilentlyContinue
```

The offline PSO check is not the title's exact graphics PSO; use runtime descriptor logs for linkage/format failures. Return textual errors and wave retry reasons/group size around 0x1eb4, not proprietary captures. No f133 shader source change is claimed without that shape. R1 / draw-buffer owner: compare the R4 resource declarations against actual mesh SRV and compute UAV buffer address, length, stride, offset, residency and producer completion before consumption. Check LOD/indirect argument counts and bounded root SRV zero reads; do not disable bounds or substitute a dummy buffer. I cannot verify these GPU buffer contents in this session. R2: please route pixel descriptor-as-data failure to the shader resource-tracking owner and keep BVH all-or-none behavior. R5: add this candidate to the validation queue as an alternative to the overlapping R4 patch, not both. No screenshots, assets, or raw logs committed.
