# Staged patches

**Coordination has moved to [GitHub Issues — Coordination index #44](https://github.com/Fyntra-Development/Comet/issues/44).** Find your worker issue there. `docs/independent-review.md` is read-only history; do not append to it.

Workers who cannot build on Windows stage their changes here instead of editing `crates/` directly. Nothing here changes Comet until the Windows Codex build gate lands it.

## Workflow

1. **Stage:** add `patches/<worker>-<topic>.patch.txt` (a `git diff` against current `main`; `.txt` is required by the asset policy). Gate new behavior with a default-off `COMET_…` switch unless a pure correctness fix. Add translation-changing switches to `crates/comet-gpu/src/shader_cache.rs` `build()`.
2. **Announce:** comment `STAGED patches/<file>: switch, expected change` on your own issue, with exact revision, base, expected metric and tests.
3. **Review:** V1–V5 comment on the patch's issue. One `APPROVE <file> (default-off)` suffices only if the default path is unchanged; otherwise two distinct plain `APPROVE <file>` decisions are required. Resolve all `CHANGES` and refresh review after revisions. Prioritize second approvals, then resubmissions, then new patches. L9 reviews rendering exactness.
4. **Integrate:** I1/I2 rebase stale hunks or coordinate combined patches with owners; comment `REBASED` or `FRESH` on the relevant issue.
5. **Land or return:** the Windows build gate applies approved patches, builds, runs both CI Clippy lines and tests (GPU lib serial until isolation is fixed), then commits passing changes and deletes their patch files. Comment `LANDED <file> <commit>` or `BUILD-FAIL <file>: error` on the patch's issue. Append a single `LANDED <file> <commit>` line to `docs/status.md` for each landing.
6. **DONE:** close your task issue with a comment describing what landed or was abandoned. Open a new issue for a new task. R5 tracks initial-round closures and writes milestone markers to `docs/status.md`; benchmark owner runs the game at those markers.

Main only; no new branches, PRs or game assets. Refer to workers by ID and issue number, not @mentions (shared GitHub account). When main moves, rebase your patch so it still applies (`git apply --check`). Update the existing file for a revision of the same idea.

## Layout

- `patches/*.patch.txt`: staged changes, one idea per file.
- `patches/notes/`: longer handoffs and audits.
- `patches/tools/`: small helper scripts.
- `docs/status.md`: machine-read landing and milestone markers.
