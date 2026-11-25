# The plan (all workers): measured root cause first

Read this before anything else. Evidence: docs/missing-world.md. Porting
source: docs/port-victory.md.

## What the dumps proved (run of main, 2026-11-25, reference machine)

- Walls, floor, ceiling, bunks and characters **are drawn**: they are in the
  depth buffer and the normal G-buffer target.
- Their **albedo is black**: 81% of drawn pixels have zero base colour
  (G-buffer RT0). Only hair and a few objects have colour.
- Why: the card runs out of room for textures. Textures picked by index:
  263 loaded at world start, 19,856 (6.4 GB) after 2 min, then **stuck, with
  12,700+ waiting for the rest of the run**. A texture that never loads
  reads as zero: black characters, black walls; alpha-tested vegetation,
  fences and cut-outs likely discard entirely (to verify).
- The reference GPU has 8 GB. The console has 16 GB unified and streams.
  Comet loads textures and almost never lets them go
  (`resource::room`, `draw.rs heap_textures`).

## Scoreboard (benchmark owner posts it after every landing)

| Metric | Now | Target |
|---|---|---|
| Drawn pixels with black albedo | 81% | < 10% |
| Textures waiting (log: "textures picked by index") | 12,700 | ~0 |
| Draws/dispatches dropped per frame | ~600-900 | < 50 |
| In-world fps | 1.1 | 5+ |

A landing that makes any number worse is reverted. Claims without a log line
or capture are not reviewed.

## Phase 1: texture memory (critical path; benchmark owner leads and builds)

Owner: benchmark owner. Workers stage patches or analysis for:
- **Evict by use:** let go of host texture copies not used by a draw for N
  frames (LRU), not only when nothing names them. (benchmark owner, W4, P4)
- **Draw-first loading:** upload textures the current frame's draws read
  before recently written slots. (benchmark owner, W3, G5)
- **Only mapped mips:** upload the mip levels the title has streamed, not the
  whole chain; re-upload when more mips map. (R3, B3)
- **Stay compressed, no duplicates:** prove BC textures stay BC on the card
  (bytes per texture vs. format), and that one title texture is not uploaded
  twice (placed vs object copies). (R3, I1)
- **Where the 6.4 GB goes:** breakdown by format, size and kind from
  `copystat` lines. (P1, P3)

## Phase 2: light

- Stand-in for the missing ray-traced indirect light, ported from
  VIctory-Belobug-AMD (`COMET_AMBIENT`). (B1, L1, R2)
- Lighting chain checks once textures load: shadows, sky, exposure.
  (L3, L8, L9, L10)
- BVH builder crash, so the real ray-traced light runs. (L2, G3, W5)

## Phase 3: shaders and geometry that are still missing

After Phase 1 the benchmark owner re-dumps; what is still missing is
attributed by shader hash.
- 6 pixel shaders "descriptor read only as data" (likely terrain or
  vegetation materials). (L5, X1, B2)
- "first half of an interpolation", permlane, v_cvt_f32_f64, wave-uniform
  reads: make the opt-in fixes default-safe. (L4, L6, L7)
- 5 refused compute PSOs and 1 refused mesh PSO (possible GPU culling). (G1, G4)
- Character material bindings and draw attribution. (G2, R1)
- Survey of every VIctory fix, mapped to Comet files. (B5)

## Phase 4: speed

- Hoisted indirect clamps (`COMET_HOIST_CLAMPS`). (B4)
- GPU wait (~570 ms/frame) and copy-back (~120 ms/frame) reduction.
  (P2, P5, W1, W2, W6, R4)

## Review, landing, coordination

- V1-V5: review every patch against the evidence rule; default-on needs the
  scoreboard run.
- I2: integration of conflicting patches.
- R5: one issue per Phase item, labelled P1-P4, linked from the
  Coordination index; keeps every worker busy and reports idle workers.
- Build gate: lands approved default-off patches; does not run the game
  while the benchmark owner runs it.
