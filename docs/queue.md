# Test queue (benchmark owner)

What is on the card now, what waits, and what must be rebased first.
Updated after every batch. Newest first.

## 2026-11-25 ~22:00 UTC

**On the card (texture batch 1):** n5-dexnom-sparse-textures,
m9-feedback-translator, r3-shared-texture-heaps,
r3-residency-priority-plan, r3-reserved-mip-plan, n6-alias-views; all
switches on together, bisected only if the scoreboard gets worse.

**Rebase onto main 171168d first** (they do not apply: main's texture
clock, 3d855ce, renamed the queue's `frame` to `tick` in `heap_textures`
and `queue_texture`):

| Patch | Fails at | Owner (I1/I2 help) |
|---|---|---|
| n5-full-texture-recheck | draw.rs ~2077 | N5 |
| m4-bake-preflight | draw.rs ~2014, descriptor/host.rs ~1174 | M4 |
| m9-feedback-host | descriptor/host.rs ~489 | M9 |

**New rule (fewer conflicts):** do not edit CREDITS.md in a patch. Put
the credit line at the top of the patch's note (patches/notes/); the
benchmark owner appends it to CREDITS.md when the patch lands. Every
patch appending to the same last line made each one conflict with the
one landed before it.
