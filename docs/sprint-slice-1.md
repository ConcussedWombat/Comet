# Slice 1 — hair solver (Worker 1)

Scope: c47e03e0e3946741 / e35b7de6a9d2bfd4, shader emit/lower. First investigate user data offset 168 loop count and u4 binding; ask benchmark owner for sanitized disassembly, actual count histogram and resource binding (no title shader bytes in Git). Coordinate shared lowering changes with slice 2. No skipped dispatches or iteration clamps. Validate build/clippy and same-scene A/B before claiming completion.

## Diagnosis and validation handoff

The existing GPU timing capture attributes roughly 260 ms/frame to the two hair shaders. The note about the 7,000-instruction loop is from the benchmark owner's offline inspection, not a verified reproduction here.

### Verify the inputs before changing arithmetic

- At the translated user-data cbuffer, byte offset 168 is word 42 (row 10, component 2). For the solver whose disassembly shows the shift by 24, report `(u42 >> 24) & 255`. Do **not** infer that the other hair shader uses that count, or that 255 is invalid.
- `lower.rs:user` maps native parameter+dword to the host index, subtracting 16 for spilled layouts; word 42 is not necessarily native SGPR 42. `user_at` emits the load at entry, so it is already loop invariant.
- `draw.rs:op_text` annotates **every** user-data value with a heap lookup (even ordinary constants). For tables it divides the value by 32; for other types it uses the value unchanged. It also reconstructs the title's root values instead of the actual final `values` array passed to SetComputeRoot32BitConstants, which can contain regenerated table offsets. Consequently the reported null annotation for `u4=0xa67100` is a lead, not proof of a null UAV. Verify HostShader.user_data's root/slot/dword for index 4, the actual bound heap index or root address and shadow range, and whether the shader really uses that descriptor as a UAV. For index 42, compare the final bound word to the native parameter's actual value.
- Benchmark owner: [worker1-hair-log.py](worker1-hair-log.py) analyzes the existing slow-list notes without publishing retail shader bytes or paths. Run `python docs/worker1-hair-log.py <existing-diagnostic-log>`; `--self-test` passed locally. Notes are the six slowest commands per qualifying list, not an unbiased dispatch census. For a complete histogram, instrument the actual bound values in the existing diagnostics path, limited to these two hashes, and report aggregate counts and null classifications. Please share shader native/host disassembly excerpts around loop entry and backedge and the dominant instruction/resource types.

### Candidate A: fixed CBV row reuse (exact, not applied to the runtime)

[worker1-fixed-cbv-rows.patch](worker1-fixed-cbv-rows.patch) replaces repeated dynamic component selection and repeated CBV row loads for vector constant-buffer accesses where the only dynamic term is the existing 16-byte indexed stride. It does not assume extra offsets, change the count, widen accesses, modify resource bindings, or alter float semantics. For a four-dword aligned access the emitted row loads go from four to one; a crossing row needs two. The driver may already optimize some of this; **first check whether these shaders actually contain the affected accesses**. If not, defer this candidate and investigate the measured instruction mix. This touches a shared lowering helper, so Worker 2 should not apply an overlapping patch simultaneously.

The patch's three context hunks were checked against lower.rs blob c486ecd3f295be0b335ce229a2687f45c13ea1d3. A separate arithmetic model covered 320 low-nibble/row/count cases including unsigned address wrap. The candidate includes a Rust unit test. This is **not** a compiled Rust or DXIL validator result.

On the Windows benchmark machine, from current main, with no concurrent local changes:

```powershell
git apply --check docs/worker1-fixed-cbv-rows.patch
git apply docs/worker1-fixed-cbv-rows.patch
cargo build -p comet
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p comet-shader fixed_cbv_components_match_wrapping_byte_addresses --locked
```

Replay the two local shader containers with the existing offline translate tool (`COMET_TRANSLATE_ONLY=c47e03e0e3946741,e35b7de6a9d2bfd4` and `--validate --pipelines`). Confirm the compiler and cache use the candidate translation (fresh candidate cache if necessary), compare DXIL instruction/CBV row-load counts, and verify both output correctness and GPU per-shader timing with identical dispatch dimensions and inputs. Then use normal release mode for same-world FPS and P95; do not use timing-mode FPS as the acceptance result. If no gain, do not land the patch. Only the Windows owner should commit application source after the mandated build/clippy pass; this connector has no compiler or hardware. Main only, no skipped pipelines.

**Status:** input source mapping reviewed; runtime count/binding verification, exact shader replay, Windows build/clippy, and measured speedup remain pending. No claim that the 1 FPS issue is resolved.
