# W6 — rendering completion item 6: exposure metering

Page-fault candidate is already staged for batch 3; no changes to watch.rs in this new candidate.

Patch: [worker6-exposure-readback.patch.txt](worker6-exposure-readback.patch.txt), revision [139f59b](https://github.com/Fyntra-Development/Comet/commit/139f59b72417bc17267cb1d9ad5369e834544e15), blob 8b154fc1dadfe6d9227ad66044cfb1a53a0e711c. Base c5317cf: list.rs blob 9bf6bea02e1eb7e3ef50d457987500852cf699cb, host.rs blob d752bcbcecfaf115cbc85996e6705046d23bfd0b, both unchanged on main at staging. Four hunks replayed. Source changes only to list.rs read_back and host.rs read_backs logging; coordinate host.rs hunk with W5. No changes to shader cache/translation, mapping locks, W5 flight scheduling, R2 diffuse/RT, W3/R3 uploads or R4 PSOs.

## Exact pass and evidence

The exposure metering/luminance feedback pass copies **5x1 and 1x1 RGBA32F (80 and 16 bytes)** to title memory every frame; list.rs copy_texture_region and host.rs read_backs identify this from run 90. capture.rs also identifies tiny luminance/exposure float targets and logs their texels with target dumps. Exact producer and tone-map consumer shader hashes are not present in the shared review; use captured ops to identify them, not guessed hashes.

The existing read_back forwards a source box to GPU CopyTextureRegion but publishes every byte of the destination footprint to title memory, even when the box only writes a subset. Nonzero destination corners are refused altogether. For a single RGBA32F row, a sub-box of one texel into a 5-texel footprint can therefore overwrite 64 untouched bytes with data not written by the GPU. This is a static defect; whether the title actually uses these copy shapes is not yet known.

COMET_W6_EXPOSURE_READBACK=1 (default off) limits publication to the exact source-box byte range for the two known source dimensions/format (RGBA32F, height 1, single slice, subresource 0, destination one row of 16 or 80 bytes), and supports valid x offsets in that row. The GPU still copies to the beginning of the temporary host footprint. The title gets only written bytes at its requested offset; remaining bytes are preserved. Full matching rows are unchanged. All other copies retain old rules. Invalid selected boxes/offsets are rejected. No artificial exposure, clamp, shader skip, or gamma adjustment.

COMET_W6_EXPOSURE_TRACE=1 (both builds, default off) logs W6-EXPOSURE recording with source resource, box, x, title destination address and byte count; W6-EXPOSURE published with same address/byte count and float values from completed host readback, or read back map failed. The host trace uses the small-row shape only, so match recording entries by destination/byte count/order before treating values as RGBA32F. Recording entry alone is not proof of GPU completion. A source pointer is process-local; correlate within a single capture. No raw title data files published.

## Windows gate and A/B

```powershell
git apply --check docs/worker6-exposure-readback.patch.txt
git apply docs/worker6-exposure-readback.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-gpu --lib --locked exposure_readback_window_preserves_uncopied_texels
cargo build --profile diag -p comet --features diagnostics
$env:COMET_RELAY="0"
$env:COMET_READ_BACK="1"
$env:COMET_W6_EXPOSURE_TRACE="1"
$env:COMET_W6_EXPOSURE_READBACK="0"
# Run same indoor and outdoor sequence; then repeat with fix=1.
```

Keep all lighting/texture switches, scene, save, resolution, and cache constant. Use existing capture on the two chosen frames with COMET_CAPTURE=<local folder>, COMET_CAPTURE_FRAMES=<frame numbers>, COMET_DUMP_TARGETS=1 on the diagnostic build. In frame-*-ops.txt, find "read back of" for source resource (from recording trace); correlate prior target writers, dispatch/draw names/hashes and following tone-map consumer. Compare target-texel messages to W6 published values (note target dumps are at present, which may be after a subsequent write). Only share text summaries of hashes, finite/zero/NaN counts, extents, and values; never commit screenshots, shaders, game assets or raw capture files.

Metric: zero publication of untouched texels for eligible partial copies, no valid metering destination-corner refusal, successful readbacks, and correct/non-stale finite luminance feedback. Visual acceptance: same indoor shot no longer crushed black and bright exterior no longer overexposed, while highlights and contrast remain natural and no new artifacts. Not a throughput patch. If both boxes are full/null, expected behavior is byte-identical and this correction cannot explain the exposure defect. If values are wrong already in producer target, trace the exact producer's UAV/SRV formats, shader and dependencies next; if valid on GPU but stale in title memory, check CPU publication and waits with W5. Do not enable by default or claim exposure is fixed without actual evidence.

Static checks: four hunks verified against c5317cf/current source, 1,568 byte-window model cases preserve all bytes outside the valid destination extent. Regression added for full, partial and invalid boxes and offsets. Rust/Clippy and in-world validation pending Windows build gate. 
