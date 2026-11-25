# R1 — exploded hair: host binding experiment

**For benchmark owner and Worker 1:** staged source candidate only, [worker-r1-hair-bindings.patch.txt](worker-r1-hair-bindings.patch.txt) (latest patch revision 63a82e6). Main application source is unchanged. No branch, no PR, no shader lowering, no game assets.

## Static findings

- `draw.rs::tables` prepares statically named resources on ordinary tables, but evaluates `RootIndexedView` only when a table is relocated (CBV or heap capacity). The bulk heap pass explicitly refreshes once per frame and does not register UAV writes. If either hair shader uses one of those indexed resources, the solver can see stale constraints/positions and its writes can fail to reach title memory or the next frame.
- `descriptor/host.rs::mirror` records the descriptor key before knowing whether `make` succeeded. A null buffer slot with unchanged record words will not be retried even if backing becomes available. Fresh table cache has an analogous record-only key.
- The diagnostic `u4=0xa67100 [heap 340872: null]` is not sufficient proof that the actual UAV is null: `op_text` divides a table byte offset by 32 and describes its BASE slot, not every accessed entry.
- Graphics IA buffers use `shadow::find_any` and refresh before binding; graphics SRVs use `readable`. Neither proves which draw renders hair, nor that a different overlapping shadow sees same-list UAV writes. The new trace correlates candidate UAV ranges with overlapping IA and graphics table SRV consumers. Root-address-only, arbitrary dynamically indexed graphics reads and actual GPU position contents are not exhaustively traced. The exact missing data/vertex shader and iteration-count correctness are still runtime questions. No fabricated addresses, count clamping, skipped dispatches or changes to vertex geometry.

## Candidate

`COMET_HAIR_BINDINGS=1` for c47e03e0e3946741 and e35b7de6a9d2bfd4 only: resolve known root-constant indexed descriptors on ordinary tables, handle indices beyond host capacity using existing fresh-table path, mirror exact buffer slots and call existing refresh / GPU write registration. Retry failed cached buffer descriptors; include selected buffer backing IDs in fresh-table cache keys. Does not guess missing resource records or make unsupported views valid.

`COMET_HAIR_BIND_TRACE=1` separately logs actual named/selected slots, host index, backing status, title address/length, store classification, actual packed host root data and candidate graphics consumers. It is a diagnostic capture, not a throughput setting. The packed root values are NOT automatically the guest dword at +168; W1 must map that offset through the exact shader's layout/CBV disassembly.

## Validation queue

Apply in a clean local main working tree (no new branch):
```powershell
git apply --check docs/worker-r1-hair-bindings.patch.txt
git apply docs/worker-r1-hair-bindings.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-gpu hair_selected_view_offsets --locked
cargo build --release --locked -p comet
```

All 14 unified diff hunks checked against fetched main source by exact old context and line counts (0 discrepancies); no Rust compilation or runtime executed here. Test covers offset arithmetic including wrapping and exact hash scope, not actual GPU descriptor lifetimes. Review direct descriptor mutation under Relay; compare baseline synchronous if an anomaly appears. Do not integrate before build/clippy succeeds.

1. Short same-scene trace A/B, both runs `COMET_HAIR_BIND_TRACE=1`, A hair fix unset, B `COMET_HAIR_BINDINGS=1`. Inspect `hair binding` rows for selected/fixed slots with valid nonempty title range but `backed=false`; target zero for actually required buffers after creation. Look for `hair binding recovered cached null buffer`; compare range addresses and lengths between consecutive frames. Check `hair solver root data` indexed/used/stores values; if indexed=0 this path cannot explain the issue. `hair vertex consumer candidate` is range overlap, not proof of a hair draw.
2. Worker 1: correlate actual title CBV/constant layout for dword +168, report high byte on each solver dispatch, and match named/selected UAV ranges and load/store instructions. Do not clamp legitimate iterations or assume a null table base is a missing actual UAV. Run W1 lowering patch separately, then combined only after both pass. If real required slots are backed but hair still explodes, check shader load/store stride, previous-position producer, constraint buffer content and matching vertex fetch/position output; report text diagnostics only.
3. Throughput A/B with trace and COMET_GPU_TIMING OFF, same warmed save, resolution, Relay flags, build, frame cap and scene. Record FPS, frame median/P95, GPU wait, readback ms/bytes and dispatch count. Optional separate timing capture compare the two named shaders' total GPU ms/frame; reported skip A/B approximately 260 ms is a hypothesis/target, not a guarantee. New correct writeback may increase readback; do not claim a speedup from skipped draws. Aim for same dispatch counts, no new device removals and no red lines across screen: hair should remain attached and move naturally over many frames. Do not call this fixed until visual verification. Native cap 30 FPS; 5 FPS immediate gate.
4. Rollback: unset COMET_HAIR_BINDINGS and COMET_HAIR_BIND_TRACE. No default behavior change.

R1 scope only draw/descriptor binding. W1 owns lowering, W4 owns shadow/copy-back internals, W5 owns Relay; no edits to their files. Please link actual validation results in independent-review.md.
