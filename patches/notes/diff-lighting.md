# Lighting module diff — L1, post-f2573a8 main

Base observed 2487b2870bc8 and subsequent staging commits. References: Belobug/VIctory-Belobug-AMD cd9f78c, fxceless/victory-fork 6a2d9f9, 90228a7, 310c6eb, a803c87 (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug and fxceless/hyphennn). Follow docs/port-victory.md. This note and the native resolve diagnostic do not port upstream code.

## Measured evidence and exactness priority

Frame 3004, 999243c: correct RT0 shirt/window/yard where geometry exists, near-black final output; owner subsequently measured exposure readback f2573a8 brightening 77->89 at same FPS. This is not proof of actual GI. Quarantine builder run lasted 315 s, 9 drops/frame, 0.12 FPS, dark then white; quarantined stores are not valid native BVH publication. Missing walls now partially resolved by raw descriptor sync; texture recheck restored shirt and hair. Runs differ. No new runtime measurement by L1.

## Module comparison

| Reference | Comet | Difference / action |
|---|---|---|
| Belobug ambient.rs:30-49, LIGHT=(1,176), constant 0.6 | ambient.rs:14-39, SLOT/DWORD, 0.6; draw.rs:1113-1153 | Landed B1 ff08f59, exact typed backed UAV check, direct dispatch only. Comet disables fallback for builder-on experiments, including quarantine; retain that safety. |
| Belobug ambient.rs:56-126 and 197-205, cd9f78c | ambient.rs:45-57,119-171 | Upstream tint/live file/depth-neighbourhood shading (DEPTH=(0,464), guessed near/FOV) replaces resolve with synthetic ambient occlusion; Comet neutral packed clear. **Not a retail-accurate port**; reject as solution under current directive. |
| Belobug twin.rs:5-14 (material mask producers, 16-bit depth, tiled lighting) | resource.rs / host.rs twins | Classifier is independent from GI; B3/L8 own exact twin comparison; require same material-class pixel before and after classifier/depth read. No competing patch. |
| fxceless docs/fork-ray-tracing.md and 6a2d9f9 | comet-shader, memory.rs, pipeline.rs | Reference translates 59/64 trace programs; explicitly not end-to-end RT. Comet already includes parked-mask change 90228a7; N1/M8/L4 own exact remaining wave forms. L2/G3/M7 own builder corruption and real publication. |
| fxceless 310c6eb (live texture reupload while full) and a803c87 | draw.rs recheck then room gate, descriptor/host.rs | Recheck landed default-on but changed fingerprints can still be refused at card limit. Rebased exact live-key/resource admission (default off), separate from GI; coordinate N5/W3. |

Known exact named passes: f22a76fcb967ee59 screen-probe trace; 34fb51b8a0312e82 temporal resolve (B1 #98). Existing B1 binding path validates resolve marker, slot 1 dword 176 and actual retained R32_UINT mip0 texture. Native output must be sampled **before** B1 overwrites it or later pass reuses it. End-of-frame dump alone is insufficient. Diagnostic COMET_L1_RESOLVE_PROBE=1 with capture flags and synchronous mode records immediate post-dispatch copy, reports packed RGB9E5 zero count/peak/mean after completion; no forced light, no shader rewrite; added barriers perturb timing/ordering and only 16 captures allowed. It reports whole-target statistics, not pixel-specific attribution, and RGB9E5 has no NaN encoding: white could arise elsewhere. If trace hit output is invalid first, resolve probe alone cannot isolate upstream producer. Do not use quarantine as acceptance.

## Owner dependencies

Request matched frame ops and dumps on current build, same camera and texture/exposure flags: builder OFF ambient OFF, builder OFF ambient default control; separately the next already-authorized builder-on/quarantine diagnostic, then only after L2/G3 safety clearance nonquarantined builder. Relay and late wait OFF for probe. Provide L1 RESOLVE PROBE lines, f22a76fcb967ee59 dispatch and output resource/counters, 34fb51b8a0312e82 dispatch and exact UAV ID, named gather/shade/temporal input and output slots, actual consumer and pre/post exposure light. Provide actual classifier hash and material-class depth/stencil where skin remains dark (B3/R2/G2). Request local translate --list on the remaining nine failures with name/hash/error, including any irradiance-cache/trace/shadow programs. No title assets, no invented values. Success: correct nonconstant bounced lighting, shadows and reflections, valid BVH producer/consumer with quarantine OFF, matching retail images; no stand-in during acceptance. FPS optimization waits until accurate.
