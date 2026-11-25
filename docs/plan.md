# The plan (final): catch the leaders, then pass them

Every worker reads this first. Evidence: patches/notes/evidence-*.md.
Porting rules and credit: docs/port-victory.md. Status markers:
docs/status.md.

## Goal

1. **Renderer retail-accurate** on two acceptance scenes (text descriptions
   below; the reference images stay on the benchmark owner's machine):
   the prologue cell and the "Gone Fishing" boat.
2. **Full-resolution textures on an 8 GB card** (the reference machine:
   RTX 5070 Laptop 8 GB, 32 GB RAM).
3. **FPS: 10, then 15, 20, 30** (30 is the stretch goal on this card).

## Phase 0: new base (benchmark owner)

Comet moves onto **DexNom/VIctory** (github.com/DexNom/VIctory,
GPL-3.0-or-later; VIctory by Marvisak and contributors; DexNom fork with
ray tracing, streaming and shadow pipelines), renamed to Comet's crates,
keeping Comet's name, launcher, history and credits. On top of it:

- hyphennn's NVIDIA work (fxceless/victory-fork): exact wave64-on-wave32
  forms, submit thread, copy queue, indirect gathering (10.3 fps on an
  RTX 4070);
- Comet's own measured wins: mip-skip for small cards, W5 present order,
  W6 exposure, L6 retry, M1 table preparation, heap recheck, heap sync,
  ambient (where the base lacks them).

**Gate:** the new base must beat today's main on the scoreboard on the
reference machine. Otherwise Comet stays and DexNom is ported fix by fix.
Until the gate passes, keep staging against current main.

**Gate result (2026-11-25, reference machine): Comet stays the base.**
DexNom 2e01ab4 built as published (release, diagnostics), same captures:
first run reached the world then fell to 0.0-0.1 fps and the title
crashed (null write); with a warm shader cache 0.3-0.6 fps and the cell
interior near-black (silhouette against a hazy window). Comet main at
the same moments: 0.85-0.9 fps, interior lit, no crashes. DexNom is tuned
for Radeon. Port DexNom fix by fix instead; first candidates: its
frosted/rippled window glass (rendered on DexNom, clear on Comet;
Atmosphere squad), its RT path (Ray tracing squad), its streaming and
marked-page copy-back (Materials, FPS).

## Acceptance scenes

**Cell (prologue, Lucia at the window; same camera as our captures):**
tiled wall left warm and sunlit; frosted, rippled window glass that
blurs and distorts the yard; warm sunset grading and a soft glow around
the window; orange INMATE shirt, dark brown hair with highlights; sharp
posters, photos and sketch papers; soft bounce light, no black shadows;
inmates in orange in the yard behind the glass.

**Boat ("Gone Fishing", Jason's first mission):** turquoise shallow
ocean with waves, sun glint and a foam wake; volumetric clouds, sun disc
and glare; distant islands with palms; white hull with decals and
outboard motors, reflections; characters in sunlight; HUD (tutorial box,
minimap, subtitles, compass bar); a plane and birds moving.

## Squads (50 workers, every system owned)

| Squad | Owns | Workers |
|---|---|---|
| Geometry & world | static world, walls/ceilings, GPU/meshlet culling, LODs, imposters, vegetation, moving vehicles | M1, M2, N4, G1, G3, G4, M10 |
| Materials & textures | albedo/normal/roughness, streaming, **full resolution on 8 GB** (see below) | N5, M9, R3, W3, P4 |
| Characters | skin, hair, eyes, GPU skinning, cloth, character materials | M4, R1, R2, G2 |
| Lighting | stand-in then real bounce light, sun shadows, local lamps, sky light, reflections | B1, M5, L1, L3, L9 |
| Ray tracing | builder, traversal, RT bounce/shadows/reflections on NVIDIA (DexNom --rt path) | M6, M7, L2, L10, G5 |
| Atmosphere & effects | volumetric fog, sky/clouds, particles/VFX, water, decals, glass refraction | L8, B3, N3, X1 |
| Post-processing | exposure, tone map, bloom, DOF, motion blur, TAA/FSR, grading, CA, grain | M3, L5, L6 |
| Shaders | 0 untranslated, 0 refused, 64-to-32-lane exact forms | N1, N2, M8, L4, L7, B2 |
| Present, video, UI | frame order/pacing, Bink cutscenes, menus, HUD | N7, W5 |
| FPS (8 now, 30 after the renderer milestones) | CPU submission, copy-back, page watching, PSO stutter, GPU cost per pass, internal resolution | B4, N6, P1, P2, W1, W4, R4, P5 |
| Test infra | same-camera captures, live switch toggles, offline shader suite, scoreboard bot, retail comparison score | W6, P3, W2 |
| Review, integration, coordination | review within the hour, merge conflicts, hourly standup | V1-V5, I1, I2, R5 |
| Playability, scouting | reach "Gone Fishing", missions, input, audio, saves; competitor watch | N8, B5 |

When a squad passes its acceptance rows, its workers move to FPS.

## Full-resolution textures on 8 GB (Materials squad, with Shaders)

The title keeps ~10.6 GB of textures mapped; the card gives us ~6.6 GB.
Full resolution fits if only what is actually sampled is resident:

1. **Sampler feedback** (D3D12 SM 6.5, supported on RTX): translated
   shaders write which texture and which mip they sample
   (WriteSamplerFeedback, or a per-index usage bitmap where feedback does
   not apply). The host learns the real working set per frame.
2. **Tiled (reserved) host textures**: map only the tiles of the mips
   that feedback asks for, from a shared heap pool (Belobug texpool).
   Full-resolution mip 0 only where it is seen up close.
3. **Residency priority**: cold textures demoted with
   SetResidencyPriority / Evict so the driver pages them to system RAM
   (32 GB) instead of failing; hot ones promoted.
4. **Render-target memory**: alias transient targets (2.7 GB today), and
   internal resolution control (also an FPS lever).
5. Keep BC compressed, no duplicate host copies.

Acceptance: textures waiting ~0, no lack-of-room passes, mip-skip off on
the cell and the boat, no fps loss.

## How every patch must look (or it is not reviewed)

1. One idea, staged as patches/<worker>-<topic>.patch.txt against
   current main, behind a default-off COMET_ switch (translation switches
   added to the shader cache key list).
2. **Compiles**: the build gate (Codex, on the reference machine) builds
   every new staged patch within the hour and posts the first error on
   the patch's issue. Fix it before asking for review.
3. **Tests**: a unit test for the logic (Comet has 600+; follow their
   style). Shader changes add a translator test.
4. **Evidence**: the symptom (from an evidence file), the reference
   commit (file:line), the log line that proves it ran, and the
   scoreboard number it should move.
5. **Credit** for every port: the CREDITS.md line goes at the top of the
   patch's note, not in the patch (the benchmark owner appends it when
   the patch lands). docs/queue.md lists what is on the card and which
   patches must be rebased.
6. **Review**: one reviewer from V1-V5 approves; I1/I2 rebase conflicts.
7. **Measurement**: the benchmark owner runs it on the card in a squad
   batch, lands it default-on if the scoreboard improves and nothing gets
   worse, otherwise returns it with the numbers.

## Cadence

- Workers: finding note, staged patch, capture request: 60-90 minutes a
  task; each worker keeps a queue of three tasks so none idles.
- Benchmark owner: a test-and-land cycle every ~45 minutes; squad batches,
  bisect only on regressions; evidence pack published after each run.
- R5: hourly standup on #44 (idle, blocked, rate-limited); stalled chats
  replaced the same hour with the same brief.
- Every 3 hours: competitor check (PortWatch; Belobug, hyphennn, DexNom
  repositories); new commits routed to squads.

## Scoreboard (posted after every batch)

black albedo; walls/ceiling in depth (cell); textures waiting;
untranslated and refused shaders; retail-comparison score per region of
the cell (wall, window, Lucia, posters); fps; device errors.
