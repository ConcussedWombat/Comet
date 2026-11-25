# R2 — non-ray-traced diffuse / deferred composite

## Scope and evidence

R2 now owns only rendering plan item 1's non-RT part. L1/L2/L3 own upstream lighting/RT, builder, slot 0x218; findings transferred to independent-review.md. Relay off for every correctness run. W3/R3 own texture staging and packed-span invalidation, R4 depth bounds/PSOs. Neither a lit yard nor a translated RT pass proves that a black material's diffuse inputs are valid.

Concrete non-RT candidate: lower.rs currently emits FLOAT32 typed loads for ordinary image reads, including reads of stencil material/lighting masks. The existing R2 UINT selector requires exact keys and chooses integer unconditionally at those sites; run-9 23k R2-IMAGE lines are not activation evidence. Previous diagnostic actually logged title stencil views 22 and 120 (120 maps to PC 47), associated with typeless host resource formats 19 and 44. A 2D stencil SRV returns integer data. The wrong typed load is not a valid substitute for that integer data; whether an affected deferred shader uses one is still unconfirmed. No global float-to-int conversion or lighting override.

## Patch

[worker-r2-stencil-srv.patch.txt](worker-r2-stencil-srv.patch.txt), revised commit 7c69ae5 / blob 7bd7a76. Nine hunks verified on current source. COMET_R2_STENCIL_SRV accepts exact comma-separated R2-IMAGE instruction keys (not 1). Non-RT pixel shader only, ordinary 2D SRV image_load/image_load_mip, no D16, declared SRV, static descriptor table address aligned to 32 bytes, no waterfall. New host user-data word 0x104 identifies an object SRV of Texture2D plane 1 with compatible live typeless resource (22/19, 47/44), not denied shader access. This is derived from the actual view for each draw. Selected shader generates mutually exclusive UINT32 textureLoad.i32 or original float32 textureLoad.f32 branches and merges VGPR dwords, preserving channels, coordinates, mip and lane guard. No speculative read of both types. Float branch remains for nonmatching views. Unknown formats and non-static/bindless sources remain on the old path. This does not fix null descriptors or late uploads. No RT compute or BVH changes. Existing manual COMET_R2_UINT_IMAGE_LOADS takes precedence, so unset for isolated run.

Cache key includes COMET_R2_STENCIL_SRV. One extra root dword per unique selected table entry (shared if same entry); limit initial trial to a single confirmed instruction, check combined graphics PSO root budget. A source format classification is not proof its actual GPU slot is resident or that its data is nonzero. Exact replay and DXIL validation must pass, no E_INVALIDARG or device loss.

## Find the actual non-RT input and select one site

1. Use existing diagnostic capture path with COMET_CAPTURE_WORLD=1, COMET_DUMP_TARGETS=1, COMET_R2_IMAGE_TRACE=1 and a short black-character scene (see worker-r2-diffuse-followup.md); capture locally, no assets in repository. Filter frame ops for the deferred G-buffer/material pixel draw, direct diffuse/skin/SSS pass, composite and their targets. Track target resource identity from writer to reader; compare lit vs black skin in same frame, look at base color and lighting buffers separately. End-of-frame dumps may be normalized or overwritten; confirm producer/consumer ordering and actual nonzero data where possible. If a pass is dropped, record exact pixel hash/refusal. If producer output nonzero but consumer reads null or zero, record shader, slot, view and resource format and whether bind happened. Check actual material stencil distribution; no assumption every pixel should have bit 3 set. If albedo is zero due to stale or invalidated texture, send W3/R3 actual material slot/resource (do not substitute ambient). If RT output is the first zero, hand to L1/L2.
2. Exact replay the non-RT candidate pixel container with COMET_TRANSLATE_ONLY and COMET_R2_IMAGE_TRACE=1; trace now includes table=Some((slot, dword)), and listed draw table records include view=[format,dimension,component mapping,pad,...,plane,...] under that trace switch. Correlate used kind and static table address, native instruction offset and bitwise consumers. Filter dim=1, not the noisy dim=0 1D case. Only nominate a key once the real view is 22/47 (including console 120); no arbitrary key or blanket 1.
3. Set COMET_R2_STENCIL_SRV to that exact key; replay again, expect R2-STENCIL-SRV: key=... emitted conditional typed load. Run with trace off for throughput. In world expect R2 stencil SRV integer path selected, with hash/slot/dword/binds (first 16 and powers of two). It counts selected host flags, not actual GPU invocations or nonzero values. Zero means no proven match. Check corresponding pixel output and diffuse contribution; do not interpret positive bind count as visual success.

## Build gate and test

```powershell
git apply --check docs/worker-r2-stencil-srv.patch.txt
git apply docs/worker-r2-stencil-srv.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-gpu --lib --locked r2_stencil_srv_checks_plane_dimension_and_view_family
cargo build --profile diag -p comet --features diagnostics --locked
```

Stop on any failure. R2 no compile or Windows/game access; test verifies classifier only, not GPU loads. Shared cache/mod/lower/draw source hunks are staged only. Coordinate W1/W3/cache owner; no edits to texture staging, BVH, builder or state creation. Previous interp-preserve patch independent; it still needs its own cache switch included if accepted.

A/B identical c5317cf-plus-candidate binary, same scene/time/camera and settings with Relay off; all other switches unchanged. A: unset COMET_R2_STENCIL_SRV and COMET_R2_UINT_IMAGE_LOADS. B: selected key, manual UINT selector still unset. Use separately warmed caches. Track selected-bind counter, target shader PSO validation, total/per-hash dropped draws, material mask and diffuse output. Visual goal: nonblack diffuse skin/clothing of previously black characters in lit scenes; preserve already correct hair/specular/rim/emissive and no device loss. If correct integer mask does not affect diffuse output, turn candidate off and follow producer/consumer chain; not evidence of full fix. The exact shader key and causal proof still require benchmark-owner runtime capture. No game assets, screenshots, raw captures or uncompiled application source committed.
