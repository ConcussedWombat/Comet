# R1 follow-up — selector, residual strands, solver cost

The benchmark owner confirmed c5317cf's opt-in hair binding trace was silent, while W1's landed CBV-row correction fixes the exploded hair. A few straight strands remain. Keep W1's correction enabled. No claim that R1 fixed the original artifact.

## Selector diagnosis

- `pipeline.rs::shader` interns the complete guest container and sets `Shader.hash = full.to_hex()[..16]` (BLAKE3). `Debug for Shader` prints stage plus that hash, and `pipeline/host.rs::HostPipeline::new` uses `format!("{shaders:?}")` for the timing name. The two known hashes are the correct format. W1's DXBC header digest is a DIFFERENT identity; don't substitute it here.
- In landed `draw.rs::bind`, stage is already available, but `tables` did another host pipeline lookup and selected from that. It also only logged if a concrete table entry resolved; the packed-root trace needed `stage.user_data` to be Some. Neither trace proves that compute dispatch was ever visited. Direct dispatch and ExecuteIndirect both call bind. `scripts/play.ps1` does not clear the two hair env vars; `-Benchmark` sets COMET_LOG=info. No static proof of the actual runtime reason for zero rows, and no current run-9 raw log available here.
- [worker-r1-hair-selector.patch.txt](worker-r1-hair-selector.patch.txt) (8150bd1) uses the actual stage passed to bind, exact timing-name fallback, and optional extra 16-digit timing hashes in `COMET_HAIR_HASHES` (comma separated; no broad wildcard). Logs switch configuration once even if both off, every distinct compute hash with selected=true/false when tracing, an early selected-stage marker before table preparation, actual root layout/used and indexed resource metadata, and direct/indirect dispatch records. It adds no shader lowering changes or cache-key changes.
- If configuration missing entirely in a run with known compute draws, check executable path/build and logging. If trace=false, check inherited process env. If census has neither known hash, check scene and compare current timing labels; only add hashes positively identified as hair. If selected=true with zero binding rows, examine the early stage metadata (root CBVs/SRVs/UAVs vs tables; indexed=0) rather than declaring selection failed. If reached but no dispatch marker, investigate binding failure/drop. If census/dispatch rows appear with both env vars on, old trace's conditional path was misleading.

## Outlier candidate

[worker-r1-hair-uav-order.patch.txt](worker-r1-hair-uav-order.patch.txt) (cbb272f; apply AFTER selector patch). Separate default-off switch `COMET_HAIR_UAV_ORDER=1` adds a global UAV barrier immediately before and after every selected direct or indirect compute dispatch. `list.rs::sync` currently ignores console sync hints; host list command order alone is not an explicit D3D12 UAV memory dependency. Existing ResourceBarrier translation adds some global barriers, so this is a falsifiable dependency-order hypothesis, not evidence that a particular hair dependency is missing. Uses the existing Windows D3D12 barrier layout seen in host.rs. No dispatch skipping, bounds/iteration clamps, visual filtering, or cross-owner source edits. May cost GPU time; not a speed optimization. Does not fix different overlapping shadow copies, within-dispatch shader races, or Relay CPU publication. Test Relay OFF first because batch 2 shows stale data with Relay on. If barriers remove the last outliers, narrow barriers to actual resources/dependencies before considering defaulting them.

## Build and A/B

On clean current main, apply in order (both independent runtime switches; same build):
```powershell
git apply --check docs/worker-r1-hair-selector.patch.txt
git apply docs/worker-r1-hair-selector.patch.txt
git apply --check docs/worker-r1-hair-uav-order.patch.txt
git apply docs/worker-r1-hair-uav-order.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-gpu hair_timing_label_is_exact --locked
cargo test -p comet-gpu hair_selected_view_offsets --locked
cargo build --release --locked -p comet
```

1. Selector diagnostic run: use `COMET_HAIR_BIND_TRACE=1`, keep `COMET_HAIR_BINDINGS=0`, `COMET_HAIR_UAV_ORDER=0`, Relay off, same save/scene. Expect R1 hair selector configuration and census even if no hair hash, then R1 hair stage reached, direct/indirect dispatch and binding rows where applicable. Trace is intentionally high volume; don't use it for throughput.
2. Original binding fix A/B independently: compare `COMET_HAIR_BINDINGS=0` and 1, order=0, trace only in short diagnostic run. Required backed table buffers should not be null; for CBV scratch slots do not infer null from buffer-range/kept-resource annotations. Record exact recovered cached null count and visible strand outliers.
3. Order A/B independently: binding fix off, `COMET_HAIR_UAV_ORDER=0` vs 1; trace off for throughput. Verify R1 hair UAV barriers recorded power-of-two counter (two per successful selected dispatch; a prepared but not executed command could add only a pre-barrier). If 80+80 direct dispatches/frame recur, expected 320 barriers/frame. Target: same dispatch count and zero crossing strands near heads across the same close-up and moving camera. Record each shader's GPU ms/frame in a separate timing capture and non-timing warmed FPS/P95; don't accept a slowdown with no visual change. If no difference, leave switch off.

## Worker 1: cost triage

- The new early-stage log reports actual host user-data mapping, packed word42/high byte when present, all root CBV addresses/lengths and known table CBV +168 record-time words/high bytes. These samples use the existing kernel read under nonblocking range mapping guard, not an unsafe pointer dereference; they may miss a busy/unreadable page. **They are record-time samples only**, not guaranteed to equal deferred upload bytes. Several CBVs may be unrelated; use native shader disassembly to identify the specific CBV and offset. If packed word42 absent, do not equate it with CBV +168. Ask benchmark owner to compare actual uploaded dword at submit with record-time sample if needed, in W5's host path. No retail shader bytes/addresses hardcoded in source; log locally and share aggregated diagnostics.
- For each exact solver: group dimensions, direct/indirect dispatch count, native workgroup size, high-byte histogram, GPU ms per dispatch, and count-weighted work. Compare W1 LICM trace with actual counts; prioritize loop body/invariant loads if count stays high and legitimate. If the count is corrupted or changes with CBV upload timing, fix binding/producer before lowering. No invented iteration cap.
- Keep W1's default CBV correction; compare shader cost only with identical correct visuals. Earlier diagnostic 193+161 ms/frame and 80+80 dispatches are distinct from the mixed skip A/B (~270 ms for hair plus RT), not a proven current hair-only cost. Current 15% throughput loss is runtime measured by benchmark owner, not R1.

Validation: both sequential patch hunk sets checked against fetched main source (9 + 4), no Rust compile, lint, GPU run, or outlier fix claimed. Reviewed types/imports/cfgs: HostPipeline, Stage, Shader.hash, UserData Debug, RootIndexedView Debug, existing title_memory_reading_range_now and host::read_into signatures, D3D12 barrier types from existing glob imports. Build gate still mandatory. Only docs staged on main; no application source pushed, new branches, PRs, or game assets. Coordinate with W1 on source-independent trace and W5 if submission-time capture is needed.
