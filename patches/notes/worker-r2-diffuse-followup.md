# R2 follow-up — diffuse term is zero (run 7)

Read the benchmark owner's updated rendering evidence: specular, emissive, sky and water remain visible; skin, arms and clothing are black indoors and at sunset. This is stronger evidence of a missing diffuse term than the earlier hair issue. The f22a76fcb967ee59 shader was translated and host PSO was logged in the older diagtime capture, so its 105 ms execution does not prove correct output, and the pass itself is not simply absent. The all-or-none builder and slot 0x218 remain independent known gaps; do not force a partial builder.

## Concrete second candidate: typed integer image loads

[worker-r2-diffuse-uint-load.patch.txt](worker-r2-diffuse-uint-load.patch.txt), 03f0ffa, touches only shader BVH helper and image lowering. Current lower.rs emits textureLoad.f32 with FLOAT32 component annotation for every normal SRV image load, and bitcasts the returned floats to VGPR dwords. For actual UINT texture views (e.g. the title's material/stencil mask), this is not a type-correct D3D12 operation. A mask lost to a mismatched view/load could zero the diffuse gate even while specular remains visible. The older log shows real stencil SRVs (console 120 -> PC 47, and PC 22); this proves those integer views exist, not that f22 or the diffuse composite reads them at a specific instruction. Do not select a site on speculation.

COMET_R2_IMAGE_TRACE=1 logs `R2-IMAGE: key=... op=... dim=... mask=... used=... selected=false` for each eligible load during translation. COMET_R2_UINT_IMAGE_LOADS accepts comma-separated exact keys from that log, for sites demonstrated to read a UINT SRV consistently. The key includes stage, native code and resource/layout metadata fingerprint, size and native instruction offset. Selected sites use UINT32 handle annotation and textureLoad.i32, retaining the original coordinate, mip, channel mask, and lane guard. No change to samples, D16, UAV stores/loads, float resources, or unspecified sites. Log `R2-UINT-LOAD: key=... emitted integer SRV load`. This is a conditional correction, not a verified game fix or a global integer override. The two patch hunks match current main; no compiler or Windows execution here.

## Capture the exact first zero-producing pass (one short diagnostic, not a throughput benchmark)

Existing capture.rs already logs per-command resource bindings and dumps end-of-frame render targets and texture UAVs. No need for a new host or draw patch. Use the diagnostics executable directly (play.ps1 removes COMET_CAPTURE, COMET_CAPTURE_FRAMES and COMET_DUMP_TARGETS). With the same scene/settings and owner-approved overlap mode, use a local folder outside the repository:

```powershell
$exe = (Resolve-Path '.\target\diag\comet.exe').Path
$env:COMET_CAPTURE = Join-Path $env:TEMP 'comet-r2-diffuse'
$env:COMET_CAPTURE_WORLD = '1'
$env:COMET_DUMP_TARGETS = '1'
$env:COMET_SHADER_SAVE = Join-Path $env:TEMP 'comet-r2-shaders'
New-Item -ItemType Directory -Force $env:COMET_CAPTURE,$env:COMET_SHADER_SAVE | Out-Null
& $exe run $Game --execute *> (Join-Path $env:COMET_CAPTURE 'r2.log')
```

Capture only a short representative black-face scene. Existing capture-world selects frames 2,6,30,80 after first 500 draws; optionally COMET_CAPTURE_WORLD_ALSO selects a known scene offset. Capture is intrusive; if Relay makes dumps unreliable use synchronous Baseline for diagnosis only. Never commit screenshots, shader bytes or raw frame ops. Inspect the relevant `frame-*-ops.txt` locally: search f22a76fcb967ee59, note its actually bound SRVs/UAVs (including descriptor format, nulls, root ranges), then follow output resource identities to the next gather/apply and composite consumers. The heap annotations may describe non-descriptor words, and list recording order is not execution order; validate descriptor role from shader resource metadata and submission dependencies. End-of-frame images can be overwritten or normalized; they are not per-pass numeric readbacks. Use them only to nominate the first suspect buffer, and confirm zero/nonzero at producer/consumer if the owner has an appropriate local capture method. Inspect the logged stencil-plane histogram and bit 3 occupancy in the indoor scene. Do not assume all zero stencil means diffuse is necessarily zero, or that correcting stencil alone restores lighting.

For the exact candidate program(s) captured locally, enable COMET_R2_IMAGE_TRACE=1 for offline replay; use the diagnostic executable with shader-info --disasm and translate --list --validate --pipelines --jobs 1, COMET_TRANSLATE_ONLY set to the exact capture hash. Find the material/lighting mask loads, cross-reference native offsets, bound resource formats, and the downstream AND/compare/gating logic. Select a key only when all executions of that site are known to use UINT. If no confirmed UINT mismatch, discard this candidate rather than stacking it blindly. If the first zero is a missing UAV/producer, report the exact hash, register/slot, resource format and dimensions, before/after nonzero count, and drop reason to its owner. No raw asset bytes required.

## Build and A/B

```powershell
git apply --check docs/worker-r2-diffuse-uint-load.patch.txt
git apply docs/worker-r2-diffuse-uint-load.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-shader --all-features --locked r2_image_selection
cargo build --profile diag -p comet --features diagnostics --locked
```

Stop on failures. Coordinate W1/W2 lower.rs changes; the R2 hunk is at the image load, not CBV/wave lowering. Use separate warmed cache directories for selector off/on because it changes emitted code and R2 does not own shader_cache.rs. First replay the exact shader and validate its PSO. Next compare only one selected site or a proven set with identical scene and other switches; clear capture flags before normal throughput A/B. No forced ambient, skipped passes or substitute lighting.

Acceptance: selected-site log appears only for nominated keys, exact replay and PSO validation pass, the material/diffuse gate and diffuse output become nonzero where reference lighting is expected, and faces/arms/clothing acquire diffuse shading without altered specular/emissive. Track f22a76fcb967ee59 GPU ms/frame (105 ms old diagnostic baseline), dropped draws, and no device loss. This candidate alone does not change builder-off drops or slot 0x218 errors. If no visual improvement or no UINT evidence, keep off. Full fix remains open. Main only; handoff text committed, no application source push.
