# L7 — GFX10 decoded opcode audit (static, main)

Scope: `crates/comet-shader/src/gfx10/opcodes.rs`, IR and structure handling, and emit/lower.rs (base lower blob f7333042ab801339ab9263f4ff6cace907cdfcc6). A decoder mnemonic does not establish an implementation. This is a source scan, not a per-encoding execution/validator sweep. Conditional support and explicit refusals are distinguished below.

## VALU: no general lowering found (excluding L7 staged opcodes)

- FP64: `v_cvt_i32_f64 v_cvt_f64_i32 v_cvt_f64_f32 v_cvt_u32_f64 v_cvt_f64_u32 v_trunc_f64 v_ceil_f64 v_rndne_f64 v_floor_f64 v_rcp_f64 v_rsq_f64 v_sqrt_f64 v_frexp_exp_i32_f64 v_frexp_mant_f64 v_fract_f64 v_fma_f64 v_div_fixup_f64 v_add_f64 v_mul_f64 v_min_f64 v_max_f64 v_ldexp_f64 v_div_scale_f64 v_trig_preop_f64`.
- FP32/FP16: `v_cvt_off_f32_i4 v_fmamk_f16 v_fmaak_f16 v_ldexp_f16 v_frexp_exp_i32_f32 v_frexp_mant_f32 v_frexp_mant_f16 v_frexp_exp_i16_f16 v_mullit_f32 v_div_fixup_f16 v_ldexp_f32 v_pk_fmac_f16`.
- Integer/packed: `v_mul_hi_i32_i24 v_mul_hi_u32_u24 v_sat_pk_u8_i16 v_lerp_u8 v_sad_u8 v_sad_hi_u8 v_sad_u16 v_sad_u32 v_mul_lo_i32 v_msad_u8 v_qsad_pk_u16_u8 v_mqsad_pk_u16_u8 v_mqsad_u32_u8 v_mad_i64_i32 v_add_nc_i16 v_sub_nc_i16 v_mad_u16 v_min3_i16 v_min3_u16 v_max3_i16 v_max3_u16 v_med3_i16 v_med3_u16 v_mad_i16 v_mad_u32_u16 v_mad_i32_i16 v_sub_nc_i32 v_add_nc_i32`.
- Packing: `v_cvt_pknorm_i16_f16 v_cvt_pknorm_u16_f16 v_cvt_pknorm_i16_f32 v_cvt_pknorm_u16_f32 v_cvt_pk_u16_u32 v_cvt_pk_i16_i32`.
- Dot products: `v_dot2c_f32_f16 v_dot4c_i32_i8 v_dot2_f32_f16 v_dot2_i32_i16 v_dot2_u32_u16 v_dot4_i32_i8 v_dot4_u32_u8 v_dot8_u32_u4`.
- Compare: all decoded `v_cmp{,x}_{f,lt,eq,le,gt,lg,ge,o,u,nge,nlg,ngt,nle,neq,nlt,tru,class}_f64`; also the decoded `v_cmp{,x}_{f,t}_{i64,u64}` cases (integer compare lowering handles other predicates).
- Indexed `v_movrelsd_2_b32 v_swaprel_b32`: no general direct emitter case found; audit their IR/indexed rewriting separately.
- Interpolation: `v_interp_p1lv_f16 v_interp_p2_f16` have no dedicated literal emitter cases; generic interpolation path and restrictions require separate review (L6). Do not infer from a literal-name search that all interpolation ops are unsupported.
- `v_nop v_pipeflush v_clrexcp` are intentionally dropped by IR; exception state is not modeled.

L7 staged candidates: `v_cvt_f32_f64 v_permlane16_b32 v_permlanex16_b32`. All are still gated and not validated on the title.

## SALU: no ordinary data lowering found

- `s_bfm_b64 s_bfe_u64 s_bfe_i64 s_absdiff_i32 s_pack_ll_b32_b16 s_pack_lh_b32_b16 s_pack_hh_b32_b16 s_cmovk_i32 s_getreg_b32 s_cmov_b32 s_cmov_b64 s_brev_b64 s_bcnt0_i32_b32 s_bcnt0_i32_b64 s_ff0_i32_b32 s_ff0_i32_b64 s_flbit_i32_b64 s_flbit_i32_i64 s_sext_i32_i8 s_sext_i32_i16 s_bitset0_b64 s_bitset1_b64 s_quadmask_b32 s_quadmask_b64 s_mov_fed_b32 s_bitreplicate_b64_b32`.
- `s_wqm_b32/b64` has structured mask semantics, not general data lowering. Saveexec/wrexec are handled via structure rather than assuming all literal names require separate cases.
- Indexed/control cases refused or restricted before ordinary lowering: `s_movrels_b32 s_movrels_b64 s_movreld_b32 s_movreld_b64 s_movrelsd_2_b32 s_subvector_loop_begin s_subvector_loop_end s_call_b64 s_rfe_b64 s_swappc_b64 s_setpc_b64`. Vertex fetch is a special path, not a general call implementation.
- MODE/hardware register state: `s_setreg_b32 s_setreg_imm32_b32 s_version` are refused; `s_round_mode s_denorm_mode` have no modeled floating-point state in the emitter. L7 conversion deliberately refuses these MODE writes, including unrelated setreg writes until proven safe.
- System/debug/control opcodes need separate handling: `s_setkill s_sethalt s_trap s_cbranch_cdbgsys s_cbranch_cdbguser s_cbranch_cdbgsys_or_user s_cbranch_cdbgsys_and_user`. Not ordinary SALU arithmetic.
- Waits, NOP, performance/trace, sleep and priority hints are intentionally dropped/handled elsewhere; SMEM is out of scope.

## Acceptance

Use synthetic encodings plus ISA reference vectors, inspect actual title shader MODE/EXEC and stage, validate emitted DXIL, compare hardware outputs, then test the three recorded shader offsets. This scan is not an assertion that all remaining names can be safely lowered by substituting a similarly named host instruction.

References: AMD RDNA2 Shader ISA (V_CVT_F32_F64, V_PERMLANE16_B32, V_PERMLANEX16_B32, MODE and modifiers): https://www.amd.com/content/dam/amd/en/documents/radeon-tech-docs/instruction-set-architectures/rdna2-shader-instruction-set-architecture.pdf ; upstream prior art: https://github.com/fxceless/victory-fork/commit/31714de (conversion pair operand handling adapted; inactive-lane shortcut not adopted).
