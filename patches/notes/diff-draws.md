# N4: draws, indirect execution and culling inputs

Baseline: Comet main source blobs draw.rs `61c35c9a6afd501bb389ec0c97e4d917705aaba5`, indirect.rs `0675fb9187635de86fb4db6740eeabab26380a6d`; source inspected after ff08f59. References pinned to Belobug/VIctory-Belobug-AMD 4cbe9d7 (B), fxceless/victory-fork a26859f (H); paths below relative to each crate's src/. Both GPL-3.0-or-later. Plan e8150c7, issue #102. This is a first behavioral audit of core paths, not an exhaustive line-by-line whole-module equivalence proof.

## Priority: missing depth in the cell

Measured by benchmark owner after Phase 1 and ambient: walls/floor absent from depth although no corresponding draws are dropped. A CPU record is not evidence that its GPU-generated arguments are nonzero. No measured Comet culling hash attribution yet.

H README says meshlet culling's scene buffers were read with the wrong view kind; H descriptor/host.rs:2222–2242,2299–2303,2409 converts structured/raw views; C descriptor/host.rs:1789–1809,1850–1854,1937 already has the underlying conversion. Do not blindly duplicate or bypass the conversion. H descriptor/host.rs:2176 and 2318/2419 vs C:1869/1947: unaligned buffer-view fallback and descriptor-byte mirrors are a promising separate N3 audit (H README reports 3–6 views/frame and ~40,000 unannounced records). H README also reports alias guard improved wall incidence (4/4 vs 2/5); N6 owns that. These are reference measurements only.

H draw.rs:2359–2424 counts 9 culling hashes and main-pass scissors; C draw.rs:1256–1263 lacks equivalent. H reports run 182 frame 1529 had 1,386 draws under scissor 531 0 1789 1337; absent-room run 187 frame 1414 lacked that rectangle. Reference only. Port a default-off census of recorded work (rather than attempted binds), distinguishing direct/indirect compute and main-pass command counts; exact scissor/count signature and culling presence can test whether title already decided room absent. Do not interpret counters as GPU execution. G4 #32 already covers refused PSO capture; use together rather than replace.

Hashes named by H draw.rs:2367–2377:
- 55679a0547649d0b tree; 89d08e561897d4e5 tree imposter; 9209777e8face9dd grass
- 17af15c4015cb4b6 prop; fdd56d97a806bcca prop_nocull; 42f717f1b4d18e94 simple; 1c4b3c0547e3e6fa slod
- 9146291b38ea9ce1 meshlet_args; 42683eebec081520 depth_pyramid_cpu_occlusion.
Compare to refused CS hashes in docs/missing-world.md (2da4beb386509b77, 369cbe906ea72764, 2cc4c197ee29e291, 02384a60fda6f2d0, 6b82bc7d22016cb5): no equality with above. A producer dependency is possible but not demonstrated.

## Behavior inventory (reference → Comet; symptom and proposed disposition)

| Reference path:line | Comet path:line | Difference / disposition |
|---|---|---|
| B draw.rs:1586–1688 | draw.rs:3000–3136 | B hoists clamps by run or inline; C has opt-in speculative multi-record window (also H draw.rs:2862–3000). B4 #79 owns hoisting, avoid overlap. Window off by default; test only if enabled. Both preserve original count buffer, clamp to scratch. |
| B draw.rs:1529–1584,1695–1733 | draw.rs:3031–3052,3141–3148 | B GPU-expands unindexed indirect rectangle vertex data/records; C uses pipeline GS or first triangle fallback, drops unsupported buffered/indexed rectangles. Tiled light/coverage, not established missing depth cause. Port after matching rectangle pipeline/op evidence; N2 owns GS. |
| B draw.rs:870–961 | draw.rs:2191–2264 | B dispatch threads conversion, dimension-fit validation, null-UAV skip, twin begin/resolve, work accounting vs C direct groups, builder gate and ambient hook. Verify API dispatch-thread flag at call sites before port; no confirmed culling dispatch exceeds direct limit. Null-UAV skip explicitly loses writes, do not import. B3 owns depth twins, L1 ambient. |
| B draw.rs:1739–1825 | draw.rs:3141–3206 | B indirect dispatch resolves twins and counts write/work; C binds then records, builder failure gate in wrapper 2982–2997. B3/N7 own twin/queue effects. |
| B draw.rs:208–255,526–568,702–869,1110–1321 | draw.rs:667–695,1058–1095,1256–1529,2381–2768 | Different root sizing, memory readiness, table allocation/binding and null checks; C closer to H (root SRV/UAV host-copy and indexed tables), plus hair/strict builder options and table hashes. N3/N6 to verify actual scene bindings; no wholesale substitution. |
| B draw.rs:365–495,604–620 | draw.rs:794–874,927–977 | B rebinds targets when list is split/hoisted and applies depth bounds; C replay and R4 capability check. Coordinate B4/N2; no evidence of depth-bounds rejection. |
| H draw.rs:88–96,1523–1590,1671–1994 | draw.rs:560–610,1931–2152 | H early prioritized texture pass, full-card retry/rotation/recheck and card accounting; C total time budget option and mip skip. N5 owns; likely texture loading/material symptoms rather than established missing wall depth. |
| H draw.rs:943,2359–2424 | draw.rs:1256–1263,2191–2264,2346–2371,2834–2972,3000–3206 | Missing culling/scissor census. Port diagnostic, default off, count after host recording; no render change. |
| H draw.rs:2082–2315,3223–3326 | draw.rs:2269–2330,3360–3407 | H more detailed slot/bytes/memory hashes and slot tracing. G4 selected pass capture already stages raw bindings. N3/N6 can extend as evidence requires. |
| H draw.rs:684–726,916–921 | draw.rs:927–973,1230–1235 | Similar depth-read transitions/replay; C capability probe differs. No new culling fix identified. |
| H draw.rs:2025–2081,2862–3070 | draw.rs:2191–2264,2982–3206 | C adds ambient target, hair tracing, builder gate; H and C share mesh scissor, indirect-window refresh/count, stage-store eligibility and mesh clamp path. Preserve C changes. |
| B indirect.rs:30–117,342–443 | indirect.rs:30–133,247–433 | B bounded 256 atomic retries and hoisted scratch offsets; C ahead-window garbage guard, mesh counters, optional post-wait omission. B bounded retry can discard contested records; do not port as geometry fix. B4 hoist and P1 wait owners. |
| H indirect.rs:72–90; H host.rs:5129–5131,5815–5816 | indirect.rs:72–83; host.rs:5997–6001,6865–6868 | H logs largest requested compute X and truncations, README reports ~77,500 requested vs 65,535 executed. C clamps same but lacks compute telemetry. Potential incomplete culling producer; not yet attributed. Port gated compute telemetry separately after coordinating host budget layout with B4; cannot fix by issuing a second dispatch from zero (would repeat GroupID). |
| B primitive.rs:18–107,269–379 | primitive.rs:24–60 | B specialized position/color fast path, generic fallback; C generic same rectangle approximation as H primitive.rs:24–60. Perf-only candidate after actual CPU rectangle profile; not evidence of walls. |
| B draw.rs:1385–1518 vs H draw.rs:2714–2857 | draw.rs:2834–2975 | Direct draw/rectangle expansion broadly same; C tracks windows and diagnostic mesh filtering. Indexed rects require GS. No missing geometry attribution. |
| H indirect.rs:1342–1830 | indirect.rs:685–1252 | H extra bare command cost benchmark; C has WARP post-wait tests; tests do not prove title culling output. Retain tests. |

## Capture / acceptance

Ask owner on #102: matched cell shots, same settings and mip skip, ambient and builder state. COMET_CAPTURE=<dir> COMET_CAPTURE_WORLD=1 COMET_CAPTURE_WORLD_ALSO=120,300 COMET_DUMP_TARGETS=1; repeat with COMET_N4_WALLS_TRACE=1. Include frame ops for 9146291b38ea9ce1 / 42683eebec081520 and their consumers: argument/count address and stride, producer UAV and scissor. Collect actual GPU-side arguments/count where available; do not infer from CPU stale memory. Expected new marker N4 walls census (not yet observed); no behavior change, compare depth and count by frame, use non-instrumented run for FPS. Refused PSO evidence via G4 separately. A real correction requires a verified bad producer/view/argument, not bypassing culling or relaxing bounds blindly.

Port credit: fxceless/victory-fork a26859f crates/victory-gpu/src/draw.rs (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by fxceless/hyphennn). Belobug audit only; B4's hoist work not duplicated.
