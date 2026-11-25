# Porting from VIctory-Belobug-AMD (workers B1-B5)

Source: https://github.com/Belobug/VIctory-Belobug-AMD at `4cbe9d7`
(2026-11-25), a fork of VIctory (https://github.com/Marvisak/VIctory) by
Marvisak and contributors. License GPL-3.0-or-later, the same as Comet, so
code may be ported **with credit**. Its README reports the world lit, with
textures, people, vehicles, vegetation and sky, at 3 to 5 fps on an RX 9070 XT.
We are at about 1 fps with black characters and missing walls.

## Rules

1. **Credit, every time.** Each ported patch's description and commit message
   says: `Ported from Belobug/VIctory-Belobug-AMD <commit> <path> (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug)`.
   Keep any copyright lines in ported code. Add one line per port to
   `CREDITS.md` (create it if missing).
2. **Port, don't paste.** VIctory's crates (`victory-gpu`, `victory-shader`,
   ...) differ from Comet's. Understand the change, then write it into
   Comet's structure (`comet-gpu`, `comet-shader`), with Comet's naming,
   tests and a `COMET_` switch. Improve it where you can: VIctory marks
   approximations with `shortcut:`; say whether ours is exact.
3. **Not their AMD-only workarounds** (driver compiler crashes, size-query
   crashes) unless the same failure shows on our NVIDIA card.
4. **Evidence rule of docs/missing-world.md applies:** name the shader hash /
   pipeline / pass, the log line that proves it runs, and request a
   before/after capture on the issue.
5. Default-off, staged in `patches/` as before. Translation switches go into
   the shader cache key list.

## Assignments

- **B1, lighting (black characters, dark rooms):** VIctory replaces the
  world's indirect diffuse light with a constant while the ray passes do not
  run (`VICTORY_AMBIENT`, 0.6 by default). Our builder is off too, so the
  same input is likely what leaves bodies black. Find where VIctory does it
  (grep `VICTORY_AMBIENT`), find the matching pass in Comet
  (docs/indirect-lighting-chain.md), port as `COMET_AMBIENT`. Improve: a
  per-room or sky-derived level instead of one constant, if the data is there.
- **B2, shader lanes:** quad reads for every read of another pixel in the
  same 2x2 quad, helper pixels included; a helper pixel's
  `v_readfirstlane` reads its own value (upstream VIctory `0601955`). Check
  whether Comet's wave64-on-wave32 split already does this; port what is
  missing. Also their doubled dynamic descriptor slots for heavy frames.
- **B3, missing geometry:** depth and stencil twins for depth textures
  written by compute; textures told apart by the mips their memory really
  holds; screen passes with pipeline workers of their own. Any of these can
  make walls/floor vanish or turn transparent. Map each to Comet's
  `resource`/`descriptor` code and port.
- **B4, speed:** hoisted clamps of indirect-draw runs (`VICTORY_HOIST_CLAMPS`,
  225 to about 80 ms GPU per frame at 9,000 draws). Comet clamps indirect
  arguments too: port the batching as `COMET_HOIST_CLAMPS`, measure with
  `COMET_GPU_TIMING`.
- **B5, survey:** diff VIctory-Belobug-AMD's history against upstream
  VIctory and list every rendering fix with its commit, what it fixes, and
  the Comet file it maps to; hand the list to B1-B4 and G1-G4. Also their
  skipped pipelines (distant-tree imposters) and which of our 22 failing
  shaders (docs/missing-world.md) they translate, and how.
