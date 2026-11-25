# M6 — DexNom experimental RT launch mapping

Reference: DexNom/VIctory d71f197 (apps/victory/src/play_launcher.rs, Setup-DexNomRuntime.ps1) and 8cca4d2 (docs/fork-ray-tracing.md, BENCHMARK.md, README.md), GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by DexNom. Static audit on Comet main. No title assets, build or runtime results from M6.

## Exact shortcut

At 8cca4d2 apps/victory/src/play_launcher.rs:17–27,83–90, --rt sets exactly:
- VICTORY_BVH_BUILDER=1
- VICTORY_BVH_QUARANTINE=0
- VICTORY_ACTIVATE_RAYS=1
- VICTORY_RT_RESOLVE=1

No wave, memory or intersection-counter switches are set by the shortcut. The diagnostic counter was enabled separately in the measured City runs.

## Source mapping

| DexNom @8cca4d2 (unless indicated) | Comet main | Behavior / mapping |
|---|---|---|
| apps/victory/src/play_launcher.rs:83–90; pipeline.rs:312–345 | crates/comet-gpu/src/pipeline.rs:318–346 | COMET_RT=1 requests COMET_BVH_BUILDER behavior. Preserve all-host-PSO and dropped-stage guards; no claim of successful execution. |
| crates/victory-gpu/src/pipeline.rs:276–287 | crates/comet-gpu/src/pipeline.rs:285–297 | Quarantine is already off by default. Explicit COMET_BVH_QUARANTINE=1 remains a safety override for separate diagnostics, unlike the launcher which forces 0. Unset/0 for parity. |
| crates/victory-gpu/src/pipeline/make.rs:50–79 | crates/comet-gpu/src/pipeline/make.rs:75–78,88–122; pipeline.rs:462–481 | CS_ScreenProbes_ActivateRays (39f0e9aeb156a61c) excluded upstream only if VICTORY_ACTIVATE_RAYS=0. Comet has no corresponding name-based exclusion; normal PSO creation already allows it. Manual skip list can still suppress it. |
| crates/victory-gpu/src/pipeline/make.rs:81–97 | crates/comet-gpu/src/pipeline/make.rs:75–78; pipeline.rs:462–481 | CS_RaytracingResolve (~1,000 material raytracingresolve_dispatch programs) excluded upstream only if VICTORY_RT_RESOLVE=0 AND builder off. Comet does not have this exclusion; no new switch required. |
| crates/victory-shader/src/emit/mod.rs:127–134; docs/fork-ray-tracing.md:19–37,56–58 | docs/indirect-lighting-chain.md:17–24; crates/comet-gpu/src/draw.rs:2256–2263,3050–3070 | f22a76fcb967ee59 (CS_ScreenProbes_TraceRays_RayBinning_BVH_Mixed_ShaderBinning_WithFar) is the selected traversal counter hash in reference, historically translated in Comet. No forced dispatch: title scheduling, normal binding, pipeline readiness, no bypass of skip/guard logic. 64 traversal programs, irradiance-cache shading and shadow masks use normal compute. Reference does not enumerate all 64 hashes, all material resolve hashes, or which individual variants actually ran in City. Five formerly refused shadow compute PSOs created on Radeon, not proven on RTX. |
| crates/victory-gpu/src/memory.rs:414–440 | crates/comet-gpu/src/memory.rs:241–250,327–373 | Reference tracking integrated, not a launcher flag. Comet equivalent preset requests COMET_L2_RAW_MEMORY_LISTS=1 and COMET_L2_RAW_MEMORY_DIRECTORY=1 behavior, directory snapshot plus list refresh/writeback catchup. Not equivalent to DexNom precise guest-write tracking or expanded directory; M7/N6 own those ports. |
| docs/fork-ray-tracing.md:56–58 | crates/comet-shader/src/emit/lower.rs:552–557 | COMET_L4_WAVE_UNIFORM default on, no translation changes or new cache key. Do not force risky COMET_WAVE_WIDE or change explicit wave overrides. For parity leave L4 unset or 1. |
| No constant stand-in in DexNom | crates/comet-gpu/src/ambient.rs:14–40; draw.rs:1345–1349 | COMET_RT suppresses B1 just like builder requested, even when preflight fails. B1 CS_ScreenProbes_TemporalResolve UAV (root 1,dword 176) is distinct from CS_RaytracingResolve material shading. Do not overwrite title resolve results. |

Comet snapshots: pipeline.rs fc423fb5, pipeline/make.rs 521c19ad, draw.rs 05af84ab, memory.rs d1ad4cbf, ambient.rs de697190, lower.rs 660db91b. Preset changes only runtime GPU gates; no global environment mutation, no shader translation/cache changes. Explicit diagnostic skip list, quarantine, and L4 overrides remain authoritative; clear them for clean parity runs. L2 strict roots is a separate optional experiment, not silently enabled.

## Evidence and capture

DexNom BENCHMARK.md @8cca4d2: RX 7900 XTX City, 6,074,512 cumulative active node visits and 848 finite in-range triangle candidates by frame 1204 (one selected shader); after precise write tracking, five million cumulative in-range candidates by frame 1265. Mean frame 3469 -> 969 ms; FPS 0.29 -> 1.03, guest memory compared 12793 -> 2655 MB. These are reference measurements only, not a Comet forecast. Per-pass lighting and bounce count unverified. Comet evidence-cell-999243c @68527ee: quarantine builder runs 315 s, 9 drops/frame, 0.12 FPS, dark then white (inf/NaN). Preset alone cannot fix malformed nodes, missing pages or failed PSOs.

M7 #115 and L2 #16/#89: coordinate builder correctness and recording-time raw directory views; no duplicate source ports. Benchmark owner: review first, then fresh process COMET_RT=1 with quarantine unset/0 only after L2/M7 approve. Compare legacy explicit switches against preset with same build, fixed camera, relay off, no skip hashes, L4 default-on, matched frame captures COMET_CAPTURE=<dir> COMET_CAPTURE_WORLD=1 COMET_CAPTURE_WORLD_ALSO=120,300 COMET_DUMP_TARGETS=1. Preserve safe control. Expected new log: "M6 experimental RT requested"; existing log "the ray-tracing structure builder runs: all of its programs have host pipelines" is only preflight, not geometry proof. Quote actual hash, PSO, dispatch/drop, valid node/header counts, finite hit/irradiance outputs before asserting lighting. Record 39f0e9aeb156a61c and f22a76fcb967ee59; enumerate material resolve and all irradiance/shadow variants with translate --validate --list and actual ops. Stop on device loss or invalid pointers. No game assets. No default-on landing.
