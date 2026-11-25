# N8 playability differential — first audit (2026-11-25)

Owner: #106; helper X1 #45. Plan e8150c7; Comet main observed 71c03dc47d4db41bd03433fdd4217af5996277ff (gameinput.rs blob 88c93a458d7ee4077ad9bf9592fce96761f2a2e9). References: Belobug/VIctory-Belobug-AMD 4cbe9d7 (B), fxceless/victory-fork a26859f (F), Zettrox/ztrx default branch read 2026-11-25 (Z). All reference README claims are self-reported, not matched tests. B/F GPL-3.0-or-later; see docs/port-victory.md. No shader or gameplay measurements made by N8.

## PortWatch-style checkpoint ledger (not a percentage)
| Checkpoint | Comet evidence | B 4cbe9d7 README | F a26859f README | Z README |
|---|---|---|---|---|
| Boot and first-boot screens | world reached; individual screens unmeasured | works | boots | works |
| Menus / start | plan 15/30 backward presents before fix, 0/30 measured fix | 30 fps | 30 fps | works |
| Intro video / audio | unmeasured | inherited upstream menu/video sound claim | opening cutscene plays, audio unreported | intro works, audio unreported |
| World / player control | prison/cell capture; 0.9 fps; input unmeasured | lit world, 3–5 fps, driving reported | prologue scene, pad/HUD/pause work, 2–5 fps latest | 1–3 fps, cell to boat chase |
| Prologue cleared | unknown | unknown in pinned README | unknown | progressed through cafeteria, robbery, police chase, boat, binoculars; completion not explicit |
| First mission cleared / Gone Fishing | unknown | missions reported by owner; no named completion in pinned README | unknown | user reports Gone Fishing; pinned milestone name not verified by README |
| Save / quit / reload | unknown live | saves load/store | checkpoint autosaves and reloads | not stated |
| Crash-free run | no matched duration; builder crash separate #89 | one driver timeout | no device loss in normal runs claimed | 90+ min one session |
| Audio during play / pad disconnect/reconnect | unknown | unknown live | unknown live | unknown live |

Do not convert unknown into fail or infer 77.4% from source; B README explicitly says not playable at 3–5 fps, so plan's percentage is a separate reported metric without denominator. Define completion as observed checkpoint transition, not screenshot. Log checkpoint name, time since boot, game build/profile, Comet commit, switches, input device, elapsed/exit reason, frame count, median and p95 frame time. Record pass/fail/not-tested/blocked independently for each. Use disposable local save directories; compare same starting checkpoint, cache warmth, hardware and flags. No save manipulation or automated mission skips. Three runs if possible, 90 min or last milestone/fatal error. Report crash count/exposure minutes and exact fatal HRESULT/RVA plus surrounding log; do not claim crash-free beyond observed duration.

## Behavioral differential (initial targeted pass)
All file paths below relative to crates/{victory,comet}-hle/src/modules/ respectively. B/F/Z source inspected by connector. Line anchors refer to the snapshots above, not future main. Normalized brand-only changes ignored.

| Reference file:line | Comet file:line | Observed difference; symptom/evidence and proposal |
|---|---|---|
| F gameinput.rs:60–62 (256); B/Z same 32 as Comet | gameinput.rs:60–62, 478–495, 625–670 | 32 changed readings/device vs 256, at 8 ms poll cadence; retained window can be ~0.256 vs ~2.048 s under continuous change (not guaranteed). At 0.9 fps a slow consumer can lag; REFERENCE_TOO_OLD 0x838A0004 is explicit. No measured input failures yet. Stage opt-in 256 and trace too-old requests, compare same manual inputs and checkpoint. |
| B gameinput.rs:66–81 | gameinput.rs:66–81,118–137 | O maps to B in B, not Comet; Backspace still works. No measured blocker. Optional alias later only with evidence. |
| B mod.rs:17–18 and gameinput.rs:1153–1181 | mod.rs:15–18, gameinput.rs:1136–1163 | B polls direct DualSense HID in slot 0 when XInput unavailable, Comet XInput or keyboard only. No pad-specific failure reported. Port HID module only after checking device report, hotplug, wireless/USB lifecycle and ownership; don't regress Comet scripted pad buttons. |
| B gameinput.rs:97–106,127–129,925–946,1210–1221 | gameinput.rs:114–115,912–929 | B preserves four motors through Source, but host drops triggers just like Comet. No gameplay advantage yet; don't port API shape alone. |
| B gameinput.rs:1159–1162,1224–1249 | gameinput.rs:1149–1162 | Comet additionally overlays COMET_KEYS onto real pad slot zero; B lacks that overlay. Preserve Comet behavior. |
| B/F/Z gameinput.rs device constructor (B 211, F/Z 197) | gameinput.rs:195–201,1420–1429 | Fixed-length victory brand prefix vs length-derived comet prefix; Comet has regression test for distinct device IDs. Keep. |
| B audio.rs:474 | audio.rs:479–480,1071–1076 | B adds blocks before modulo; Comet reduces first, avoiding overflow. Comet also logs large allocations (117–121). F/Z audio identical after brand normalization. No audio port justified. |
| B/F/Z xgamesave.rs:184–202,260,304–310 | xgamesave.rs:184–232,265–270 and legacy migration | Comet stricter canonical version selection, ignores cleanup failure, validates committed data before deleting old data, flushes fallback copy, distinguishes corruption vs missing. Preserve newer durability; B uses default config path at 365–369; compare per-user namespace in follow-up before any save changes. No live failure. |
| B xuser.rs:157–166 | xuser.rs:157–173,333–339 | Comet returns empty modern suffix rather than name for every component; F/Z identical to Comet. No port. |
| B/F/Z dsp.rs, storage.rs, xgameui.rs, xgameruntime.rs, speech.rs, media.rs, apu.rs | corresponding Comet files | Identical normalized or comment-only differences; no behavior difference found in inspected files. Speech/reverb limitations are common, not shown mission blockers. |
| F dstorage.rs:445–485,1048–1143 | dstorage.rs:925–933,1086–1130 | F traces file/offset and page writes, retries physical-destination publication (1097–1137; 12 default). B/Z lack that retry. Comet has a pre-existing DirectStorage test failure per #44, but no mission-associated failed-read trace. Request exact failing page HRESULT/offset and coordinate W5/N6/P4 before a retry port; bounded retry must not write a stale/reassigned physical mapping. |
| B dstorage.rs:121–160,219–225,398–434,623–635,640–651; F/Z analogous | dstorage.rs:120–160,226–234,408–445,634–645,764,925 | Comet has stricter null checks, captures last error, explicit scheduler failure and cancellation, scratch reuse; B counts extra staging intermediate/page destination buffers (623–635). Detailed scheduler/lifetime review needed; no downgrade of Comet correctness. |

Scope limit: initial audit only; not exhaustive across every runtime submodule, DirectStorage internals, HID, save migration, nested audio codecs, lifecycle/files/kernel, or repository history. Need complete tree-level comparison and benchmark logs before claiming every behavioral difference. No measured HLE cause for geometry, lighting, shader drops or mission progression. The known refused compute hashes 2da4beb386509b77, 369cbe906ea72764, 2cc4c197ee29e291, 02384a60fda6f2d0, 6b82bc7d22016cb5 are GPU owners' work, not input attribution.

## Capture request
Benchmark owner: matched baseline and opt-in runs, first boot with fresh disposable profile and second reload, keyboard and XInput separately; measure the checkpoints above. Existing logs: "GameInputCreate", "input device", trace "current reading"; compare input edge behavior and capture any REFERENCE_TOO_OLD with patch instrumentation. Log actual lines, not invented output. Use COMET_CAPTURE=<dir> COMET_CAPTURE_WORLD=1 COMET_CAPTURE_WORLD_ALSO=120,300 COMET_DUMP_TARGETS=1 for matched visual evidence; input/audio/save are judged separately. No shader hash for HLE change (not a shader/pipeline change). Request on #106.
