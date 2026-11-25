# Competitor watch: commits routed to squads

Checked every ~3 hours (plan.md, Cadence). All three repositories are
GPL-3.0-or-later; port with a CREDITS.md line (docs/port-victory.md).
Newest check first.

## 2026-11-25 ~21:30 UTC

| Commit | What | Squad / worker |
|---|---|---|
| fxceless/victory-fork f5c03f3 (PR #999) | textures shaders write that the title reads on the CPU copied back into title memory (`VICTORY_TEXTURE_BACK=1`); with it the tester got past the sinking boat | Playability N8 (reach "Gone Fishing"), with Atmosphere N3 (water) |
| fxceless/victory-fork c36971b | small buffers' copies in card memory (`VICTORY_CARD_BUFFERS_MIN_KB`), changed runs by masks, scalar buffer loads four dwords a call (`VICTORY_SHADER_OPT` bit 64), copy-back measuring step | FPS B4/P1 (buffers), Shaders M8 (scalar loads) |
| fxceless/victory-fork 79cfdd9 | windows outside buffers as one buffer in a GPU upload heap (`VICTORY_WINDOW_UPLOAD`) | FPS W1 |
| Belobug e48eb83, 4f62c2a | ambient light follows the sky dome, on by default | Lighting B1 (with `b1-ambient-shade-tint`) |
| Belobug ae75a66, 7263339 | clean-up passes over translated shaders; compiles not held endlessly | Shaders N2, B2 |
| Belobug 6986a57 | screen-probe ray activation program left out without rays | Ray tracing L10 |
| DexNom 921d8d3 | pixel quad sampling repair, ray buffer views, sparse texture recovery | already with N5 (Stage 3), M7 |
