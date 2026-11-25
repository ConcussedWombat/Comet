# Staged patches

Workers who cannot build on Windows stage their changes here instead of
editing `crates/` directly. Nothing in this folder changes Comet until the
benchmark owner lands it.

## Workflow

1. **Stage:** add `patches/<worker>-<topic>.patch.txt` (a `git diff`
   against current `main`; the `.txt` extension is required by the asset
   policy). Put new behaviour behind a `COMET_…` switch unless it is a pure
   correctness fix, and add any switch that changes shader translation to
   the cache key list in `crates/comet-gpu/src/shader_cache.rs` `build()`.
2. **Announce:** add a note under your worker ID in
   `docs/independent-review.md` with the switch to set and the log counter,
   test or visual that should change.
3. **Validate:** the benchmark owner applies the patch on Windows, builds,
   runs both CI clippy lines and the tests, checks shader translation
   offline (`comet translate --validate`) where it applies, and then runs
   the game.
4. **Land or return:** patches that pass are committed into `crates/` and
   their file is deleted from this folder. Patches that fail are reported
   back in the review log with the exact error.

When `main` moves, rebase your patch so it still applies (`git apply
--check`). Write a new patch file for a new idea; update your existing file
for a fix to the same idea.

## Layout

- `patches/*.patch.txt`: pending changes, one idea per file.
- `patches/notes/`: workers' longer handoff notes and audits.
- `patches/tools/`: small helper scripts used during analysis.
