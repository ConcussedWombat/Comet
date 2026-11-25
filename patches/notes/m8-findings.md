# M8: Wave64 cross-half exact forms

Evidence: [cell frame 3004, 999243c](https://github.com/Fyntra-Development/Comet/blob/68527ee/patches/notes/evidence-cell-999243c.md): D3D12 #622 "Shader wants wave lane count of 64 but maximum wave lane count for device is 32"; N1 test refused 6 -> 1 but untranslated 16 -> 23 ("no safe two-half form"). Refused compute hashes 2da4beb386509b77, 369cbe906ea72764, 2cc4c197ee29e291, 02384a60fda6f2d0, 6b82bc7d22016cb5; mesh f5e50eb2108c425b. No per-op hash mapping yet. Missing depth left/above window is measured, causal link not established. Shader owner N1 #99, L4 #18, PSO tracker #92.

## Source comparison (line numbers at audited revisions)

| Operation | hyphennn 6a2d9f9 crates/victory-shader/src/emit/lower.rs | Belobug 4cbe9d7 same path | Comet main crates/comet-shader/src/emit/lower.rs | Finding / symptom |
|---|---|---|---|---|
| Mask as data | 2945–2994 | 3081 | 3323–3375 | Existing ballot halves exchange; requires convergent defining block, non-EXEC masks otherwise refuse. Can cause N1 rejection; no hash attribution. |
| Lane index / exchange | 3525–3592 | 12649 | 3924–3992, 4065–4080 | Existing LDS pair protocol; reuse group barriers and disjoint reserved LDS. |
| readlane | 7187–7234 | 8215 | 7717–7764 | Existing selected writer and two barriers; requires every lane at instruction. Missing lane/index normalization needs independent review, not silently approximated. |
| writelane | 7235 | 8231 | 7765–7783 | Already console-lane select (not a cross-half shuffle); preserve old VGPR on all other lanes. |
| mbcnt | 8279 | 9765 | 8881–8952 | Already full low/high prefix count with console lane; masks may fail upstream. |
| ds_bpermute | 4257–4289 | 4667–4699 | 4735–4769 | Both references also refuse split-wave permute. Source outside EXEC should be zero. M8 original extension, not a purported upstream fix. Potential shadow-mask/culling relevance; no proof any of the six hashes contains it. |
| Widening | 432–436 (needs_every_lane) | wave.rs 424–521 | emit/mod.rs 556–615; lower.rs 474–475 | N1 exact retry staged, existing widening restricted to one console wave; avoid multi-wave group barriers without group-uniform proof. |
| Mesh | — | — | 4391–4400 | Separate unsupported stage; this compute patch cannot fix f5e50eb2108c425b. |

## First patch design

COMET_M8_BPERMUTE=1 (default off, shader-cache key). For a single 64-thread compute group only, with real group waits and all lanes at the block, reserve 64 dwords after the title LDS and existing translator scratch, budget <= 8192 dwords. Every lane writes its source (zero if outside EXEC) to its unique console-lane slot. Unconditional LDS group barrier, read arbitrary index 0..63, unconditional barrier before scratch reuse. Reject local-wave approximate retries and nonconvergent blocks (report need for widening). No multi-wave groups, scatter ds_permute, or mesh claims. No change to existing mask/mbcnt paths. Credit: adapted the exchange pattern from fxceless/victory-fork 6a2d9f9 lower.rs (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by hyphennn), not copied; compared Belobug/VIctory-Belobug-AMD 4cbe9d7.

## Requested capture

Benchmark owner: COMET_WAVE_WHY=1 COMET_WAVE_LOCAL=idiom; compare N1 exact-only arm vs N1 + M8. Capture matched camera at cell window using COMET_CAPTURE=<dir> COMET_CAPTURE_WORLD=1 COMET_CAPTURE_WORLD_ALSO=120,300 COMET_DUMP_TARGETS=1. Record per-hash stage, thread shape, rejection reason and opcode, DXIL validator/PSO acceptance, missing depth, black albedo, dropped draws and FPS; log expected M8-BPERMUTE emitted (not yet observed). Request a trace of any remaining no-safe-form shader. No benchmark or build performed by M8.
