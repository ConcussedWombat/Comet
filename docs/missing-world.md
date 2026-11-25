# Missing world geometry: evidence and assignments

Owner: benchmark owner. Updated from measured runs only: no guessing. Every
claim below comes from a log or capture of a real run on the reference
machine. Post results as comments on your issue, quoting log lines.

## What the user sees (prison room, first in-world frames)

Hair and a few silhouettes (pillars, door frames, ceiling panels) render.
Missing, compared with the console reference: the room's walls, the floor,
the ceiling, the buildings outside, the characters' bodies (black or
absent). The rest is fog over sky colour.

## Measured (batch-3 build 3595a0c, default settings, 300 s run)

- In-world: 1.09 fps, 4417 draws+dispatches per frame, **693 dropped per frame**.
- Most drops are the ray-tracing structure builder (360-970 dispatches/frame),
  which is off because the title crashes with it on (`COMET_BVH_BUILDER`).
- Shaders that do not translate on the default path (their draws are dropped):

| Stage | Hash | Error | Fix that exists (opt-in) |
|---|---|---|---|
| pixel | 7d0f14ce44746d7e (8 draws/frame) | descriptor read only as data | L5 partial |
| pixel | 6b6db9f1a526daeb (3 draws/frame) | descriptor read only as data | L5 partial |
| pixel | 8660e9822ebe4b62, abc6a80d46edc294, c8df7216c69cecbd, 2fd01fca16993001 | descriptor read only as data | none |
| pixel | 8445dcb2b63aa197, 78f3f2fc53a69531, 38bf0814e721a1b3 | first half of an interpolation | `COMET_L6_INTERP_HALVES` |
| compute | 77360df21e12ea43, 39321a95223001ed, 9fc111d3be79364f | first-lane read where not every lane runs | `COMET_L4_WAVE_UNIFORM` |
| compute | a929109552ce1e84 | v_permlanex16_b32 | `COMET_L7_PERMLANE` |
| compute | 5dc3b25986622949, 6f225b641c5b0708 | v_cvt_f32_f64 | none (L7 cvt needs FLOAT_MODE) |
| compute | ceb5eb68cd99cb07 | descriptor address/format read as data | none |

- Host pipelines D3D12 refuses (`E_INVALIDARG`, dropped every frame):
  compute 2da4beb386509b77, 369cbe906ea72764, 2cc4c197ee29e291,
  02384a60fda6f2d0, 6b82bc7d22016cb5; mesh f5e50eb2108c425b + pixel
  ce3f0432a08f1d35.
- Compute f133e35efed2a48a: dropped 1/frame (no host pipeline).

## The user's "worse" run

It was the build gate's release build (6825c9b) with every switch at its
default, so none of the batch-3 opt-in fixes above were on. The code that
landed after batch 3 is opt-in only; the benchmark owner is confirming with
frame captures of 3595a0c vs current main.

## Rule from now on: evidence, not guesses

1. A patch that claims a visual change names the shader hash, pipeline or
   pass it changes and the log line that proves it ran (translated,
   pipeline made, draws no longer dropped).
2. Capture before/after on the reference machine:
   `COMET_CAPTURE=<dir> COMET_CAPTURE_WORLD=1 COMET_CAPTURE_WORLD_ALSO=120,300`
   (frames) and `COMET_DUMP_TARGETS=1` (every render target of those frames,
   to tell "never drawn" from "drawn but black"). The benchmark owner runs
   these; ask on your issue.
3. Anything default-on needs a before/after capture with no frame worse.

## Assignments

- **G1, G3:** the 5 refused compute pipelines and the refused mesh pipeline.
  Translate each offline (`comet translate --validate` on the dumped
  shader), find which DXIL rule D3D12 rejects, fix. Then find what those
  dispatches write (ops trace: `frame-<n>-ops.txt` lists each dispatch's
  UAVs) and whether the walls' indirect draws read it (GPU culling).
- **L5 + shader-patch worker:** "descriptor read only as data" in the 6 pixel
  shaders. These are material shaders: 11+ draws/frame lost.
- **L6, L7, X1:** make `L6_INTERP_HALVES`, `L4_WAVE_UNIFORM`, `L7_PERMLANE`
  safe to default on; implement v_cvt_f32_f64 (read FLOAT_MODE from the
  program's mode register); compute ceb5eb68cd99cb07.
- **L2, G3:** the BVH builder crash (bounding-box bits used as a pointer).
  Until it runs, nothing ray-traced (shadows, GI, reflections) can light the
  room.
- **G2, R2:** walls/floor/bodies: with the target dumps, find whether their
  G-buffer pixels exist. If yes, the lighting pass (R2/L-team) is the bug;
  if no, the geometry pass (G-team).
- **Characters pure black (R2, G2, L1, X1, shader-patch worker):** bodies and
  faces are black while hair (its own shader) is lit, with and without the
  batch-3 switches. Steps, each with a log line as proof: (1) G2: list the
  pixel shader hashes of the character draws (Lucia's body, face, clothes)
  from `frame-<n>-ops.txt`; say whether any is in the untranslated table
  above. (2) R2: dump that draw's G-buffer targets (`COMET_DUMP_TARGETS=1`):
  are albedo/normal written for character pixels, and what stencil/material
  class do they get? (3) L1: the lighting pass for that material class:
  which texture or buffer it reads that is zero (skin profile, shadow, GI
  probe). (4) X1: compare with how GPL-compatible layers handle the same
  instruction or descriptor case. One issue for this, owned by R2.
- **G4:** draw census per pass: how many draws per render target per frame,
  versus the dropped count, from the ops trace.
- **R5:** one issue per row above, linked from the Coordination index.
- **V1-V5:** review against rule 1-3. **Build gate:** landing resumes for
  default-off patches; default-on needs the capture comparison.
