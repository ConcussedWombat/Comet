# R4 — rendering completion items 4 and 5

Current batch-2 source (c5317cf and later docs-only commits). Keep COMET_R4_PSO and COMET_R4_WAVE at landed defaults. COMET_RELAY=0 for correctness. No game assets, no source push, no extra branch. One R4 chat.

## Item 4 — missing / late / malformed geometry

Acceptance: actual total "dropped ... without a host pipeline" <50/frame in warm yard and no floating head, missing limbs, dark boxes or giant triangle. This is a joint gate: item 2's disabled BVH builder accounts for about 343 drops/frame per owner; R2/W2 must eliminate that reason before <50 is reachable. Do not turn builder on by bypassing preflight, change counting, or count intentionally skipped draws as improvement.

**First candidate:** revised [worker-r4-priority-prewarm.patch.txt](worker-r4-priority-prewarm.patch.txt), commit [6a12901](https://github.com/Fyntra-Development/Comet/commit/6a1290118e7f384945c0db278d77700c5285b17e), patch blob 792328b1. Exact 32-hex PSO keys from actual first-use misses can now be selected with COMET_R4_GEOMETRY_KEYS (comma-separated), instead of all PSOs sharing a 16-hex shader hash. Existing COMET_R4_GEOMETRY_WAIT hash list still works. COMET_R4_PREWARM_PRIORITY=1 gives these specific PSOs a queue between immediately requested and ordinary early work. No forced waits or geometry skipping, no extra workers. Trace measures actual returned miss and late_frames/accepted on later request. Nine hunks checked against landed make.rs 1cc1efab. Previous trace 165 misses, 163 later resolutions (not necessarily accepted). Select only keys with later accepted=true and geometry relevance.

Example PowerShell on the Windows benchmark machine (use real path to run-9 trace or a fresh trace of same build, Relay off):
```powershell
$keys = Get-Content 'run9.log' | Where-Object { $_ -match 'R4 geometry first-use miss resolved on later request' -and $_ -match 'accepted=true' } | ForEach-Object { if ($_ -match 'key="?([0-9a-fA-F]{32})') { $Matches[1].ToLowerInvariant() } } | Sort-Object -Unique
# Filter to actual vertex/mesh and identified geometry-producer keys from the trace first.
# A/B with the same nonempty list, same warmed cache and scene:
$env:COMET_R4_GEOMETRY_KEYS = $keys -join ','
$env:COMET_R4_GEOMETRY_WAIT_MS = '0'
$env:COMET_R4_GEOMETRY_TRACE = '1'
Remove-Item Env:COMET_R4_PREWARM_PRIORITY -ErrorAction SilentlyContinue
# Baseline run; then candidate:
$env:COMET_R4_PREWARM_PRIORITY = '1'
```
Trace ordering depends on formatter; if log lines wrap, use the full original structured log. No relevant keys: do not enable indiscriminate prioritization. Expected metrics: selected and total first-use missed count down from baseline, accepted later-resolution lag down, "being made" drops down without increasing other dropped reasons or worsening startup P95. Run with depth bounds switch off in both arms (PSO keys preserve original name when off). This does not fix persistent bad vertex data or permanently refused PSOs.

**Next separate candidate:** previously staged [worker-r4-pso-retry.patch.txt](worker-r4-pso-retry.patch.txt) COMET_R4_PSO_RETRY=1, only after E_INVALIDARG and successful validation. The prior old patch and pso were alternatives; this revision extends landed pso. Expected refused per-hash drop reason -> zero only for recovered PSOs. Use actual post-batch-2 refused counts; old evidence was four compute and one mesh (5-6/frame), not 343. The 6b6db9f1a526daeb and 2fd01fca16993001 pixel shader errors (descriptor read only as data) persist as a separate translation problem in old trace. Current victory-fork lower.rs still contains that refusal; cannot claim a direct upstream port cures it. Need exact native/translated replay or resource-tracking owner assistance; do not synthesize descriptor dimensions or addresses.

**Persistent malformed geometry:** use existing [cull-probe](worker-r4-cull-probe.patch.txt) only for attribution, not acceptance. COMET_R4_WIREFRAME=one graphics hash, then unset; if triangle becomes edges/boxes outlines, identify its PSO and inspect exact mesh/vertex position, index/instance data, producer output, actual resident bytes, layout/stride, indirect records and mesh export coverage. Capture after all pipeline compilation has drained; persistent malformed geometry is not explained by late first use alone. Culling toggled separately; no global disabling or fake geometry. R1/draw owner to supply binding and vertex-buffer evidence. Actual frame-ops text and run-9 logs were not available here.

## Item 5 — ghostly background people

**New isolated correction:** [worker-r4-depth-bounds.patch.txt](worker-r4-depth-bounds.patch.txt), commit [71e321e](https://github.com/Fyntra-Development/Comet/commit/71e321ee8746057ec70d24c01a435d4773dc124e), patch blob 500c890b. COMET_R4_DEPTH_BOUNDS=1, off by default.

Credit Kaivyyy (#82) via [fxceless/victory-fork c96b4f2b](https://github.com/fxceless/victory-fork/commit/c96b4f2b8365bf43d54ab074940923ef8267f4c6) (GPL-3.0-or-later). That commit reports the first room lit by its lamp after restoring depth-bounds tests; it does not prove the ghost-character defect is the same. Comet currently drops DepthBoundsTestEnable from the title's subobject 21 and ignores OMSetDepthBounds entirely. Restore both ends together: store DESC1, retain original plain DESC/state identity when off, capability-check D3D12 OPTIONS2, build vertex/mesh depth-enabled PSOs using DEPTH_STENCIL1 stream, give these PSOs a distinct library identity, forward valid dynamic bounds to ID3D12GraphicsCommandList1 and replay across list cuts. Ordinary depth compare/write, stencil, blend, color masks and targets unchanged. No forced opacity. If GPU unsupported, preserve existing behavior and report support false. No unrelated Relay, texture or shader changes; list.rs and draw.rs edits limited to depth-bounds dynamic state only, coordinated with list/draw owners (do not overwrite their pending changes).

20 exact hunks (15 host, 4 draw, 1 list) replayed against current host/draw/list blobs; host hunks apply standalone and after cull and retry (also checked both pair orders). The cull probe copies the complete depth descriptor, including flag, without further changes. Types: existing Interface import, D3D12_* wildcard, AtomicU64/Ordering in draw.rs; non-Windows list call retains old ignored/logged path under cfg. Test r4_depth_bounds_descriptor_preserves_depth_state verifies subobject 21 flag, depth write/compare preservation, and reset by plain subobject 11. Need Windows build/clippy and runtime testing; no local Rust toolchain.

Compare independently:
- baseline all new R4 probes and COMET_R4_DEPTH_BOUNDS unset, Relay off;
- candidate COMET_R4_DEPTH_BOUNDS=1, same scene, no geometry overrides. In diagnostics set COMET_CAPTURE/COMET_CAPTURE_FRAMES outside checkout; check "R4 host depth bounds support" supported=true, "R4 depth-bounds PSO" enabled=true, and "R4 nontrivial depth bounds forwarded" for actual restrictive ranges (older capture first eight calls were 0..1; later unknown). Frame ops include R4 DEPTH BOUNDS per list. Compare ghost characters and indoor room lighting, normal opaque characters and transparency; no increase in refused PSOs/drops. If no restrictive ranges or unaffected ghost characters, do not attribute a fix to it.
- If ghost persists, use COMET_R4_STATE_HASHES for exact opaque/ghost PSO state on the existing cull-probe (no raster overrides). Check actual opaque/depth prepass drawn, active per-target blend, depth write/func, stencil, sample mask, alpha-to-coverage, DSV and dynamic blend factors. Compare to console material/LOD. Existing draw.rs forwards and replays OMSetBlendFactor, so no evidence to force a different blend factor. Request actual relevant frame-op text. A translucent pass may intentionally be translucent, and dithered LOD may need a missing earlier pass; do not globally force depth write or blend disabled.

Acceptance: background people render solid as intended with no new artifacts, no diagnostic overrides, normal alpha/material behavior preserved. Not yet verified.

## Build gate

Apply each candidate separately (or use a combined build with only one switch enabled per A/B):
```powershell
git apply --check docs/worker-r4-priority-prewarm.patch.txt
git apply docs/worker-r4-priority-prewarm.patch.txt
git apply --check docs/worker-r4-depth-bounds.patch.txt
git apply docs/worker-r4-depth-bounds.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-gpu --lib --locked r4_priority_preserves_requested_first_and_single_claim
cargo test -p comet-gpu --lib --locked r4_depth_bounds_descriptor_preserves_depth_state
cargo build --profile diag -p comet --features diagnostics --locked
```

Both switches off by default; exact-key parser alone is inert without selection. No cache-key list changes needed: these are PSO scheduling and state, no translation change. PSO state key explicitly distinguishes depth-bounds-enabled title state. No claim of rendering completion before benchmark owner verifies the stated gates.
