# R2 — black skin / indirect diffuse handoff

R2 owns only comet-shader BVH/ray tracing and device slot 0x218. Main only; no application source pushed. See [candidate patch](worker-r2-black-faces.patch.txt) (9ff69b1). The eight unified diff hunks were checked against main. An independent numerical model of closed parallel slabs matched 19,600 signed-zero/face/extent cases (392 differed from the previous finite-inverse approximation). This is not a compiled Rust test or a GPU/console comparison.

## Root-cause assessment

The title's light uses its own compute traversal and Xbox-native BVH nodes, not DXR state objects. Missing indirect light is the leading explanation of dark skin/shadows. The all-or-none builder is disabled, including when any required PSO is missing. Do NOT bypass it: partial builders have previously corrupted title memory. Missing raw-address pages also return zero/misses. The reported 105 ms f22a76fcb967ee59 pass being executed is not evidence that it has valid geometry or irradiance.

Slot 0x218 requests ID3D12PipelineState per device.rs; it is not identified as CreateStateObject. The diagnostic log shows repeated apparent pointer/length pairs at qwords 1/2, 3/4, 5/6, 7/8; the native layout and semantics remain unproven. In one earlier capture the call counter reached at least 256, so distinguish call count from the 29 attributed pipeline creations. Do not guess a PC description or return S_OK with a dummy object.

## Candidate and gate

COMET_R2_BVH_PARALLEL_SLABS=1 changes only the BVH slab test when a direction component is +/-0: a ray on either face is within a closed slab and has no finite t bound on that axis. The old path is preserved by default. This is an experimental console-semantics correction; it could change traversal/shadows, not a demonstrated fix for black faces. It cannot replace a missing builder or PSO. CPU and hardware oracle tests cover both signs, faces, outside rays, extent and both node formats.

COMET_R2_SLOT218_PROBE=1 adds four bounded header classifications per readable sampled descriptor, first 32 calls and powers of two. No raw shader bytes are printed or committed by the new probe; E_NOTIMPL and null out are preserved. A DXBC header is only a candidate, not proof of a PSO layout. Send pair/size/kind/declared fields to R2; if there is no container header, identify the packet layout from local call-site disassembly. Do not share asset bytes.

On the benchmark owner's clean local main:

```powershell
git apply --check docs/worker-r2-black-faces.patch.txt
git apply docs/worker-r2-black-faces.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
Remove-Item Env:COMET_R2_BVH_PARALLEL_SLABS -ErrorAction SilentlyContinue
cargo test -p comet-shader --all-features --locked bvh::tests
cargo test -p comet-shader --all-features --locked intersect_tests
$env:COMET_R2_BVH_PARALLEL_SLABS='1'
cargo test -p comet-shader --all-features --locked bvh::tests
cargo test -p comet-shader --all-features --locked intersect_tests
cargo build --release --locked -p comet
```

Stop on any failed command. The new BVH switch affects translated shader bytes; since shader_cache.rs is not R2-owned, use two separate local cache directories (-Cache on play.ps1), warmed independently on the same executable, and do not reuse one directory across switch settings. Keep all other flags, display, scene, Relay and title state identical. Run the slot probe separately; disable it for throughput runs. For timing captures use COMET_GPU_TIMING=1 with -GpuProfile Inherit (the other profiles clear this variable).

## Exact acceptance metrics

- Probe: new log message `R2 slot218 descriptor probe`, four records per readable sampled descriptor; compare kind and declared across pairs. Existing unsupported 0x218 calls will NOT decrease from this diagnostic alone.
- Boundary correction: synthetic closed-slab false misses must go to zero in the added test; in-game f22a76fcb967ee59 GPU ms/frame is monitored versus the 105 ms diagnostic baseline. No decrease is promised: correct additional traversal can increase cost. Compare same-view skin/shadow appearance; avoid claiming any whole-face fix from this alone.
- Full lighting fix: `dropped .../frame without a host pipeline: a stage of the ray-tracing structure builder, which is off` must fall from ~343/frame to zero in the same scene, with the affirmative builder-runs message and no subsequent builder-off event. Repeated refused or untranslated stages must be eliminated, not skipped. Slot 0x218 unsupported count should fall only once real equivalent PSOs can be constructed. Separately check f22a76fcb967ee59, frame P95 and no crashes/corruption. The expected visual result once indirect light actually works is visible skin tones, shading and clothing folds in shadow, without artificial ambient/color overrides.

Benchmark owner and R4: first run existing COMET_BVH_BUILDER=1 with COMET_BVH_QUARANTINE=1 as a short diagnostic on a disposable test state, using COMET_WAVE_WHY=1. Record missing/of or failed shader hashes and why Wave64 was retained. Quarantine is not a visual acceptance run (CPU results are withheld); do not disable it until every stage passes and memory safety is reviewed. If no builder runs, capture the exact refused hashes and shader replay text only. R4 owns host PSO creation; R2 owns ray tracing lowering, not overlapping files. Coordinate with worker 2 on f22a76fcb967ee59; do not skip it. Native ceiling 30 FPS, 15 intermediate, immediate 5 FPS target. The full visual fix is open until this evidence is available.
