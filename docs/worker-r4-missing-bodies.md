# R4 — missing bodies / limbs, beach and indoor scenes

New assignment from [rendering evidence update](independent-review.md): floating head with body missing and limbs/torso absent. Hair is substantially fixed in batch 1 (W1/R1), and zero diffuse lighting is a separate R2 issue. Keep the two defects distinct: a black but silhouette-visible body is not a missing draw. No game/screenshots/captures in git.

## Candidate A: first-use producer recovery

[worker-r4-geometry-first-use.patch.txt](worker-r4-geometry-first-use.patch.txt) ([0bb5c6a](https://github.com/Fyntra-Development/Comet/commit/0bb5c6a01f26ce0f469fad5e82c5f93d2373e238)), changes only pipeline/make.rs. COMET_R4_GEOMETRY_TRACE=1 logs first-use compilation misses and later request resolution by exact PSO key and shader hashes. COMET_R4_GEOMETRY_WAIT=<comma-separated 16-digit hashes> prewarms selected producers/draws at creation and, on their first request only, gives them a bounded extra shared wait if the regular budget expired. COMET_R4_GEOMETRY_WAIT_MS defaults to 250, capped at 500; unset/empty hash list leaves normal behavior. No synchronous/unbounded compiler wait, no shader substitutions or bypass of refused PSOs. Extra time may hurt loading and first-world frame; test only selected implicated producers. Test covers exact hash parse, shared reservation, same-frame refunds and stale-frame rejection. All five diff hunks checked against blob c6fb185b333dc94e7ebb4f2ef40f11c47c9cb5b3.

Evidence: older diagnostic run reports 4.0 and 4.5/frame dropped for "being made" at 03:04:04 and 03:04:10, then zero in later intervals. A one-time skinning/LOD producer missed at startup can leave its buffers uninitialized even after the host pipeline finishes, but this has not been established for the beach body. No reason to expect a steady-state drop-count gain from this candidate. With tracing on, select only a first-use miss tied to the affected geometry; if no relevant miss exists, do not enable the wait. Expected metric for selected recoverable misses: R4 geometry first use missed -> zero, and corresponding "dropped ... without a host pipeline: being made" reduced by the number of restored first-use operations. Preserve actual geometry and other draw counts.

## Candidate B: rasterizer culling isolation

[worker-r4-cull-probe.patch.txt](worker-r4-cull-probe.patch.txt) ([a0c8624](https://github.com/Fyntra-Development/Comet/commit/a0c86247f227f993310c00387ecd0e0e8b80d47b)), changes pipeline/host.rs only, prepared AFTER the preferred PSO retry and exact-wave patches; do not apply the alternative overlapping PSO patch. The previous mesh host state string is empty, so COMET_R4_GEOMETRY_TRACE=1 also supplies the mesh PSO state to existing frame op listings. COMET_R4_CULL_PROBE=<exact vertex/mesh shader hash list> changes only CullMode to NONE for selected graphics PSOs. Maintains depth/stencil tests, front winding convention, depth clipping, sample state, active targets and shader bytes. Uses a separate effective PSO cache key. Diagnostic ONLY: rendering normally culled backfaces is not a permanent fix. Expected result if wrong winding/backface culling is causal: previously missing limbs appear while draw/dispatch counts and drop reasons remain unchanged. If not, roll back immediately; do not claim missing parts recovered by accepting extra backfaces. Test covers unchanged depth bias, clipping and winding. All 12 hunks statically replayed after the previous seven PSO and two host-side exact-wave hunks.

## Validate and attribute

On a clean local main worktree, apply A; for B apply preferred PSO retry, exact-wave (keep both runtime switches off during culling test), then B. Both candidates may coexist, but test independently.

```powershell
git apply --check docs/worker-r4-geometry-first-use.patch.txt
git apply docs/worker-r4-geometry-first-use.patch.txt
git apply --check docs/worker-r4-pso-retry.patch.txt
git apply docs/worker-r4-pso-retry.patch.txt
git apply --check docs/worker-r4-exact-wave.patch.txt
git apply docs/worker-r4-exact-wave.patch.txt
git apply --check docs/worker-r4-cull-probe.patch.txt
git apply docs/worker-r4-cull-probe.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-gpu --lib --locked r4_geometry_wait_is_exact_and_budget_shared
cargo test -p comet-gpu --lib --locked r4_cull_probe_preserves_other_rasterizer_fields
cargo build --profile diag -p comet --features diagnostics --locked
```

Use COMET_R4_GEOMETRY_TRACE=1 and existing COMET_CAPTURE/COMET_CAPTURE_FRAMES (outside checkout, directly launch diagnostics executable; play.ps1 clears capture flags). Obtain text-only frame op listing for the actual beach/indoor scene. Share the relevant pipeline hashes, first-use status, effective graphics state, mesh group counts and root buffer extent/stride/residency. Capture only locally, never commit images or retail shader bytes. Select exact hashes from the listing, not every mesh PSO. Test A with trace on in both arms and same warmed scene, selected wait unset vs exact list. Test B with trace on and cull probe unset vs exact list. Keep unrelated switches identical. Record FPS/P95, first-use miss count and "being made" drops; for B observe silhouette restoration and unchanged counts. A diagnostic run that disables normal culling must never be treated as a valid performance benchmark.

## Read-only culling / skinning audit, handoffs

Translator lower.rs already documents two historical causes of empty meshlet output: heap SRV treated as CBV in BatchCullMeshlets / BatchCullPreSkinnedMeshlets / BatchCullBatchCSMeshlets (around 5545) and incorrect mip/offset gathers from the depth pyramid (around 9630). It still approximates the sampler address mode there and meshes with static maximum counts and degenerate unused primitives (around 4197), including an output-size shortcut and bounds-based primitive cull (around 10542). These are credible suspects if no-cull does nothing and group counts are valid; they are not confirmed causes of the new defect. R1 / shader owner: verify actual producer shader hashes, indirect group counts and skinning output against console; compare descriptor kind/stride/base, shadow range and actual resident bytes, then inspect the specific culling producer and mesh emitted vertex/primitive counts. Do not disable root bounds, mutate unrelated lowering or force invalid primitives. I have not edited these out-of-scope paths.

Earlier capture (not run 7) shows nonzero aggregate indirect mesh groups (e.g. 7,730/frame at 03:04:10), so the global group count does not identify the missing individual character. Pixel 6b6db9f1a526daeb still needs descriptor-as-data shader resource tracking. The five known refused PSOs and f133 exact-wave experiment remain independently queued. R2 owns diffuse/RT builder. New run-7 log/frame text was not available in the accessible library; only benchmark-owner descriptions. No application source pushed, no Rust/Windows/game execution here, no assertion that limbs are restored. Mandatory compile and runtime validation pending.
