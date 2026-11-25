# M5 — sun-shadow map attribution (#110)

Baseline: main inspected 2026-11-25, draw.rs blob 05af84abe83493cb61bc6b13525061262f598e6a, host.rs blob 719172698443ecca20740942db28698da901f012. Evidence: [cell 999243c @ 68527ee](../../patches/notes/evidence-cell-999243c.md), frame 3004. Reference Belobug/VIctory-Belobug-AMD @ 4cbe9d7 (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug). Reference fxceless/victory-fork main resource.rs (GPL-3.0-or-later; hyphennn). No code copied.

## Measured vs unmeasured

- Frame 3004: shirt and frame albedo correct where drawn; final near-black; depth empty left/above window; 2,394 main-depth draws, 2,054 indirect, 271 mesh, 69 direct. These are MAIN depth, not a shadow-map inventory. The retail hard bar shadows are the target, not a measured attribution to any particular Comet shader.
- Main depth null-slot candidates: mesh 30332cf87c4e0eae (53 u9), 8563c39d693b36d0 (22 u9), ba8d52f4ea1a1bac + PS b37565055f2f7d3b (21 u8/u9). They are not established shadow producers. Missing geometry can also mean missing shadow casters; M1/N4 own this. Refused compute hashes in plan 2da4beb386509b77, 369cbe906ea72764, 2cc4c197ee29e291, 02384a60fda6f2d0, 6b82bc7d22016cb5 have real debug-layer id 622, “Shader wants wave lane count of 64 but maximum wave lane count for device is 32”; no attribution to sun shadows.
- docs/indirect-lighting-chain.md:7 reports direct sunlight in the yard in run 11, absent bounced sunlight inside. Direct sunlight is not evidence of correct shadow sampling. Do not confuse ambient, exposure, material-ID depth or scene depth with a shadow map.

## Reference vs Comet

| Reference file:line | Comet file:line | Difference / measured symptom |
|---|---|---|
| Belobug crates/victory-gpu/src/twin.rs:1–27, 65–77 | crates/comet-gpu/src/resource.rs:420–465, 529–541 | Belobug material-ID twins (118–120) explicitly exclude 16-bit shadow maps draws write. Comet includes single-plane R16/R32 twins for depth+UAV resources. Do not port B3's material twins as a sun fix. Dark lit surfaces compatible with material misclassification but not proof of absent sun shadows. |
| Belobug resource.rs:1219–1239; fxceless resource.rs:2069–2089 | resource.rs:1789–1807 | Both references strip simultaneous depth/UAV flags on host D3D12; Comet strips UAV and provides a twin. No demonstrated missing D16 resource flag port. |
| Belobug twin.rs:478–582 | host.rs:5490–5618 | Reference immediately copies material planes via fullscreen depth/stencil draw; Comet copies single-plane twin via copy states, defers stencil-bearing depth-plane draw, and publishes twin at close. Direct-indexed reads/writes not seen; host.rs:5607–5612 explicitly documents a captured frame where no table names shadow map for reading except next-map fill. Possible ordering/visibility issue, not measured this cell. B3 owns plane publication; M5 needs actual shadow identity before changing it. |
| Belobug resource.rs:356; descriptor/host.rs:2355–2364 | descriptor/host.rs:2069–2110, 2130–2195; draw.rs:3320–3344 | R16 typeless/typed views and depth readonly handling exist; twin UAV uses R16_UNORM. Need actual DSV/SRV subresource and comparison sampler identity; no mismatch proven. L8 owns sampler/readonly-DSV changes. |
| Belobug draw.rs:1407 | draw.rs:2330–2388, 810–876 | Existing Comet capture records successful operation hashes, whole target format, viewport/scissor and descriptor annotations, but not exact depth-view mip/slice in op text. A D16 array atlas can share resource identity across cascades; current whole-resource identity alone cannot prove a producer wrote the sampled slice. Add opt-in capture annotation, not an output-changing fix. |

## Capture request

Benchmark owner: matched frame facing the window, same camera, texture/mip/ambient/BVH/Relay settings, COMET_CAPTURE=<dir> COMET_CAPTURE_WORLD=1 COMET_CAPTURE_WORLD_ALSO=120,300 COMET_DUMP_TARGETS=1; with the staged diagnostic additionally COMET_M5_SHADOW_TRACE=1. Share text only, no assets. Include all depth-only f53/f55 (and f39/f40 if present) operations, `M5 shadow candidate` annotations with resource, DSV format/dimension/mip/slice range, executed shader hashes and counts; include clears, queue submission order, barriers and dropped PSO log lines. For candidate resource IDs identify later sampled SRV format/mip/slice, comparison sampler and consuming hash (L8 binding trace or GPU-side heap index evidence). Record depth occupancy and window-bar occluder coverage at producer completion, not solely end-of-frame overwritten data. Capture both direct-light output and final image. Compare scoreboard and matched visual output; no visual change proposed until actual failing producer/consumer is known.

Coordination: B3 #80, L8 #22, phase #88. Diagnostic only, no build/run by M5. No real sun-pass hash or executed M5 log yet; expected annotation is not execution evidence.
