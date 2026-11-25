# M6 — ray-trace program findings (2026-11-25)

Static audit only, main snapshots: emit/mod.rs abceb89440d4bd4001a3b5af3ebc7a3b2c35c8bf; emit/lower.rs 365862e22d67ba57c64fe7954360c9fe62b85ea3; apps/comet/src/translate.rs cf5967c81fe4ec8caf6f5c38ac54082a934084d9. Evidence: patches/notes/evidence-cell-999243c.md @68527ee; docs/plan.md. No title assets, Windows DXIL validator, game run or 64-program manifest available here. No measured Comet 59/64 claim.

| Reference file:line (fxceless/victory-fork 6a2d9f9 unless stated) | Comet file:line | Finding and measured association |
|---|---|---|
| docs/fork-ray-tracing.md:53–56,69 | docs/fork-ray-tracing.md:69–79,102–105; docs/indirect-lighting-chain.md:11,20–21 | 59/64 and five shadow-mask refusals are upstream claims; no Comet per-hash validation inventory. Correct albedo but near-black final on frame 3004 supports missing lighting/exposure, not any particular translation refusal. |
| crates/victory-shader/src/emit/mod.rs:344,366–378 | crates/comet-shader/src/emit/mod.rs:436–475 | Reference wave-wide retry defaults on; Comet split-wave retry requires COMET_WAVE_WIDE=1 because runs 121–124 lost device in particle Sprite/Trail. Potential coverage difference, not established for trace hashes. Never globally enable. |
| crates/victory-shader/src/emit/mod.rs:461–509 | crates/comet-shader/src/emit/mod.rs:556–609 | Widening already present; one console wave/group required for split wave32 barriers. COMET_WAVE_WIDE=1 not proof of safe execution. |
| crates/victory-shader/src/emit/own_bits.rs:16–35; emit/lower.rs:1888–1900 | crates/comet-shader/src/emit/own_bits.rs:16–35; emit/lower.rs:1810–1814,2056–2058 | Own-bit loop proof already ported. Exact only if omitted rounds have no effects; do not blanket replace wave64 ORs with half-wave OR. |
| crates/victory-shader/src/emit/lower.rs:1946–2025 | crates/comet-shader/src/emit/lower.rs:2278–2327 | Balanced half sum exists; do not replace with ordinary f32 reduction. No identified trace hash exercising this. |
| Belobug/VIctory-Belobug-AMD 4cbe9d7 crates/victory-shader/src/emit/mod.rs:1036–1138 | crates/comet-shader/src/emit/mod.rs:367–382,556–609 | Reference adapter tests distinguish translated versus host-accepted; NVIDIA wave32 is not AMD wave64. Measured D3D12 message 622 is host rejection, not validator failure. |
| docs/fork-ray-tracing.md:14–34,38–52,57–62 | docs/indirect-lighting-chain.md:9,17,30–35 | Title traversal uses ordinary compute and node tests, not DXR; builder readiness is distinct from tracing. Quarantine run 315s, 9 drops/frame, 0.12 fps, dark then white, no proof of valid BVH or rays. L2 owns. |

Known hashes: f22a76fcb967ee59 (CS_ScreenProbes_TraceRays_RayBinning_BVH_Mixed_ShaderBinning_WithFar) historically translated/host PSO, ~105 ms dispatch, output unverified (docs/indirect-lighting-chain.md:20). Builder-only 77360df21e12ea43, 39321a95223001ed, 9fc111d3be79364f historically "first-lane read where not every lane runs"; exclude from 64 trace tally. Other refused compute 2da4beb386509b77, 369cbe906ea72764, 2cc4c197ee29e291, 02384a60fda6f2d0, 6b82bc7d22016cb5 and mesh f5e50eb2108c425b: message 622 "Shader wants wave lane count of 64 but maximum wave lane count for device is 32"; pass identity unverified. No shadow-mask hashes or irradiance hash available.

## Benchmark-owner request

Using your local diagnostic executable and shaders.rpf, PowerShell, no game assets shared:

```powershell
$env:COMET_HOST_WAVE='32'
$env:COMET_HOST_MAX_WAVE='32'
$env:COMET_WAVE_LOCAL='idiom'
$env:COMET_WAVE_WHY='1'
Remove-Item Env:COMET_TRANSLATE_ONLY -ErrorAction SilentlyContinue
& $comet translate $shaderPack --validate --list --jobs 1 2>&1 | Tee-Object m6-strict.txt
```

Repeat with original runtime settings (unset WAVE_LOCAL if default), record full manifest with exact program name, container #hash, code ~hash, stage, wave/group dimensions, error/offset or validated, and count duplicate containers separately. CLI apps/comet/src/main.rs:162–166 and translate.rs:163–175,320–350,399–489,634–665. A successful IR build is not validation; validator and pipeline statuses separate. Use --pipelines in separate run on selected hashes with COMET_TRANSLATE_ONLY and only offline PSO creation; compare COMET_WAVE_WIDE=1 on selected failing hashes offline, not in-game. Run one hash/process for WAVE64-RETRY correlation. Share text only; do not commit dumps. No approximate wave form counts as retail-accurate; compare strict vs default and classify every discrepancy. Return exactly 64 trace programs and all irradiance shading variants, with explicit unresolved rows if names are missing.

N1 #99: owns translator; coordinate exact retry fixes and cache keys, avoid duplicate port. L2 #16/#89: builder and raw pages; capture valid hierarchy before tracing acceptance. For safe matched before/after after owner approves: COMET_CAPTURE=<dir> COMET_CAPTURE_WORLD=1 COMET_CAPTURE_WORLD_ALSO=120,300 COMET_DUMP_TARGETS=1, same scene/settings, quarantine off only after L2 safety approval, record dispatch/drop and finite hit/irradiance outputs. No candidate behavior fix until exact failing trace hash and reason; never substitute fake irradiance or bypass barrier guards.

Reference credit: GPL-3.0-or-later; VIctory by Marvisak and contributors, fork changes by hyphennn (fxceless/victory-fork 6a2d9f9) and Belobug (4cbe9d7). No new code port here.
