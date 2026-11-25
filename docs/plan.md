# The plan: catch Belobug, then pass it

One plan for every worker. Evidence: docs/missing-world.md. Porting rules
and credit: docs/port-victory.md.

## Where we are (measured on the reference machine, 2026-11-25)

| | Comet now | Belobug (VIctory-Belobug-AMD, its README) |
|---|---|---|
| World picture | Textures load, stand-in light on: faces, bunks, cell lit; walls/floor around Lucia not drawn; some materials no colour | "lit, with textures, people, vehicles, vegetation, water and sky" |
| In-world fps | 0.9 (RTX 5070 Laptop 8 GB) | 3-5 (RX 9070 XT 16 GB) |
| Playable | not measured | reported 77.4% |

Landed today: Phase 1 texture mip-skip (e82716d), B1 stand-in light
(ff08f59). Black albedo 81% -> ~40%; textures waiting 15,350 -> ~2,500.

**Key fact:** Belobug's renderer and Comet's share a lineage and a layout
(`victory-gpu/src/{descriptor/host,pipeline/make,twin,shadow,watch,capture,...}.rs`
vs `comet-gpu/src/...`). We can compare them module by module. Every
module where Belobug draws something we do not is a concrete, checkable
difference, not a guess.

## Strategy

1. **Diff, don't guess.** For each renderer module, list what Belobug
   does that Comet does not (or does differently), with the Belobug file
   and line. Port the differences that explain a measured symptom first.
2. **One owner per module**, so ports do not collide.
3. **Measure every landing** on the reference machine (scoreboard below).
   The benchmark owner builds, captures and lands in batches; nothing
   default-on lands without a before/after capture.
4. **Credit every port** (docs/port-victory.md, CREDITS.md). Port into
   Comet's structure; do not paste.

## Module owners (Belobug file -> Comet file)

| Module | Belobug | Comet | Owner (+ helpers) |
|---|---|---|---|
| Shader translation (instructions, quad reads, helper lanes, wave64) | victory-shader/src/{emit,gfx10,dxil,...} | comet-shader/src | **N1** (+ L4, L5, L6, L7, B2) |
| Pipelines (make, host, library, depth bounds) | pipeline/{make,host,library,depth_bounds_test}.rs | pipeline/ | **N2** (+ R4, G1) |
| Descriptors and views | descriptor/host.rs, descriptor.rs | descriptor/ | **N3** (+ R1, G2) |
| Draws, indirect, culling inputs | draw.rs, indirect.rs, primitive.rs | draw.rs, indirect.rs | **N4** (+ G4, B4) |
| Depth twins and material classes | twin.rs | resource.rs twins, host.rs planes | **B3** (+ L8, P1) |
| Textures and memory | texture.rs, texpool.rs, resource.rs, memory.rs | texture.rs, resource.rs | **N5** (+ R3, P4, W3) |
| Copy-back and title memory | shadow.rs, watch.rs, memwait.rs | shadow.rs, watch.rs | **N6** (+ W4, W5) |
| Queues, frames, present | queue.rs, frame.rs, dxgi.rs, compose.rs | queue.rs, host.rs present | **N7** (+ W6, the flicker workers) |
| Lighting chain, ambient, ray tracing | ambient.rs (+ what runs instead of rays) | ambient.rs, BVH builder | **L1** (+ L2, L3, L9, L10, G3, B1) |
| Playability (input, audio, saves, missions) | victory-hle | comet-hle | **N8** (+ X1) |

N1-N8 are new workers (deployed 2026-11-25). Each module owner's first
deliverable is `patches/notes/diff-<module>.md`: every behavioural
difference, Belobug file:line, Comet file:line, which measured symptom it
could explain, and a proposed port. B5 keeps the whole-repo survey
(patches/notes/victory-survey.md) and hands items to owners.

## What to fix first (measured symptoms, in order)

1. **Walls and floor around Lucia are not drawn** (absent from the depth
   buffer; Comet drops no draw for them). Owners N4, N2, B3. Check what
   Belobug does differently in culling inputs, refused compute pipelines
   (2da4beb386509b77, 369cbe906ea72764, 2cc4c197ee29e291,
   02384a60fda6f2d0, 6b82bc7d22016cb5), depth pyramid / twins.
2. **Materials with zero colour** (bunks: normal maps present, albedo 0).
   Owners N3, B3, N1.
3. **Shaders that do not translate** (22 in docs/missing-world.md).
   Owner N1.
4. **Speed** 0.9 -> 3+ fps: hoisted clamps (B4), GPU wait, copy-back.
   Owners N4, N6, P team.
5. **Real ray tracing** (BVH builder crash) to replace the stand-in light.
   Owners L2, G3, B1.
6. **Playability checklist** measured the way Belobug reports it. Owner N8.

## Scoreboard (benchmark owner posts after every landing batch)

| Metric | Before today | Now | Target |
|---|---|---|---|
| Drawn pixels with black albedo | 81% | ~40% | < 10% |
| Walls/floor in depth (cell frame) | no | no | yes |
| Textures waiting | 15,350 | ~2,500 | ~0 |
| Shaders that do not translate | 22 | 22 | 0 |
| In-world fps | 0.9 | 0.9 | 3+, then 5+ |
| Menu: presents that jump back | 15/30 | fix measured 0/30 | 0 |

## Process

- Stage default-off patches in `patches/`, one idea per file, rebased on
  main, with the switch, the symptom, the Belobug reference and the log
  line that proves it runs.
- V1-V5 review against this plan. I1/I2 resolve conflicts between staged
  patches (several present/twin patches touch the same code).
- The benchmark owner tests and lands in batches and posts the scoreboard
  on the Coordination index (#44). R5 keeps issues current and reports idle
  workers.
- The build gate lands reviewed default-off patches.
