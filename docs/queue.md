# Test queue (benchmark owner)

What is on the card now, what waits, and what must be rebased first.
Updated after every batch. Newest first.

## 2026-11-25 ~23:30 UTC: one shared base (main ef7e45b)

Every recent Stage 1-4 and new-worker patch is now ON MAIN, switches off:
n8-stream-stats/complete, m9-feedback-host/translator,
n5-dexnom-sparse-textures, n5-full-texture-recheck, m4-bake-preflight,
r3-shared-texture-heaps, n6-alias-views, l12-sky-ambient,
l11-integer-rtv-blend, g7-fresh-buffer-owner, c1-mrt-attribution,
g6-draw-evidence, g7-table-trace, l11-tile-trace, l12-probe-trace,
l13-builder-exact-only, and the staging pool (COMET_STAGING_POOL).

**Everyone: rebase your open patches onto main ef7e45b or later before
asking for a run.** Do not restage the landed patches above; send follow-ups
as new patches. The known-good switch set is in docs/known-good.md: a patch
must not make that picture worse.

Not yet on main (conflicted with the above): g6-lod-trace, l13-rt-census.
G6 and L13: rebase them.

**On the card now:** the known-good set without M9 (does M9's feedback
cost the 0.3 fps the set lost?). Next: N5 full recheck alone, M4 alone,
L12 sky ambient alone (with the known-good set).

**Rule (unchanged):** no CREDITS.md edits in patches; credit line at the
top of the patch's note, the benchmark owner appends it on landing.
