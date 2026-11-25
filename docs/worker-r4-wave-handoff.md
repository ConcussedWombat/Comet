# R4 exact-wave follow-up

Use [worker-r4-handoff.md](worker-r4-handoff.md) for the preferred PSO retry candidate (COMET_R4_PSO_RETRY=1). Another R4 worker prepared it concurrently; it includes equivalent compute stream retry and is preferred to my earlier COMET_R4_PSO=1 candidate. **Do not apply both PSO patches.** My earlier [worker-r4-pso.patch.txt](worker-r4-pso.patch.txt) remains an alternative, not an additional patch.

New separate candidate [worker-r4-exact-wave.patch.txt](worker-r4-exact-wave.patch.txt) ([1d28c3f](https://github.com/Fyntra-Development/Comet/commit/1d28c3f4b90ac1350b82f40ee6529bbc10c98176)) adds COMET_R4_WAVE=1 (off by default), only for f133e35efed2a48a and the specific 0x1eb4 refusal. Ordinary translation preferred; retries existing exact widening for each applicable control-flow form, only for 64 threads on a wave32 host, requires SDK DXIL validation, never caches the experimental variant as an ordinary shader translation. No approximate half-wave substitution or guard removal. Prior widening experiments lost the device: group size and validator success are necessary but not sufficient for execution safety. Test with the benchmark owner's watchdog, abort on device loss/stall; keep off until validated. Existing offline CLI is unchanged by this runtime flag. First follow the isolated replay instructions in the primary handoff to determine actual group size and retry errors; do not enable global COMET_WAVE_WIDE=1 in gameplay.

Statically replayed all 4 exact-wave hunks after the preferred 7-hunk PSO retry patch against fetched main source. They also replay after my original 9-hunk PSO alternative. No source push or compilation. On clean current main:

```powershell
git apply --check docs/worker-r4-pso-retry.patch.txt
git apply docs/worker-r4-pso-retry.patch.txt
git apply --check docs/worker-r4-exact-wave.patch.txt
git apply docs/worker-r4-exact-wave.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-gpu --lib --locked r4_mesh_canonicalizes_only_inactive_fields
cargo build --profile diag -p comet --features diagnostics --locked
```

Test PSO retry alone first. Then test COMET_R4_WAVE=1 separately if offline replay indicates the exact one-wave group is applicable. Expect the f133 per-hash drop reason to disappear (0.4/frame in earlier startup interval; not present in later steady intervals), plus R4 exact wave retry validated and a real host pipeline creation. No promise of a steady-state FPS change. PSO recovery target 5–6 fewer dropped operations/frame (430 -> 424–425, other causes unchanged). Pixel 6b6db9f1a526daeb at 0x49dc is a translator failure, not PSO; needs shader resource owner, 9/frame in late capture. RT builder off remains separate. Do not force incomplete builder stages on.

Read-only geometry audit: draw.rs root_view no longer caps SRV refresh at 1 MiB by default (historically truncated 1452/2021 mesh streams). Ensure COMET_ROOT_VIEW_MB and COMET_ROOT_UAV_MB unset, COMET_ROOT_SRV_DIRECT unset; do not disable bounds. Existing op_text diagnostic records title and host root addresses, remaining lengths and heap tables. R1/draw owner: correlate with declared R4 resources after restored mesh PSO, inspect geometry/LOD producer completion, actual sizes, strides and indirect arguments. No binding source changes outside assignment, no game assets/screenshots. No Rust toolchain, Windows or game available here. Both candidates unbuilt, no measured visual or FPS change. Independent reviewer and benchmark owner: queue exact-wave only after offline evidence and mandatory build gate.
