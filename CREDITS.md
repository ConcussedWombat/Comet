# Credits

Comet acknowledges the following GPL-compatible upstream ports.

- Ported from Belobug/VIctory-Belobug-AMD e52106f crates/victory-gpu/src/{twin,draw}.rs (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug)
- Ported from Belobug/VIctory-Belobug-AMD 4284939 crates/victory-gpu/src/{resource.rs,descriptor/host.rs,texture.rs} (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug)
- Ported from Belobug/VIctory-Belobug-AMD 18e57db crates/victory-gpu/src/pipeline/{host,make}.rs (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug)
- Ported from Belobug/VIctory-Belobug-AMD 4cbe9d7 crates/victory-shader/src/emit/lower.rs (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug) — X1 static partial descriptor size.
- Ported from Belobug/VIctory-Belobug-AMD 4cbe9d7 crates/victory-gpu/src/resource.rs (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug) — X1 hypothetical mip budget probe.
- Ported/adapted from Belobug/VIctory-Belobug-AMD 00fc4a7 crates/victory-gpu/src/{resource,watch,descriptor/host}.rs (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug) — X1 full-card rotating texture scan.
- Ported from Belobug/VIctory-Belobug-AMD 4cbe9d7 crates/victory-gpu/src/ambient.rs (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug) — B1 stand-in indirect light (COMET_AMBIENT).
- Ported from fxceless/victory-fork a26859f crates/victory-gpu/src/{descriptor.rs,descriptor/host.rs} (GPL-3.0-or-later; pull request #305 by Flyander, after upstream Marvisak/VIctory 86a5ed6; fork changes by fxceless/hyphennn) — raw-byte descriptor heap sync (COMET_HEAP_SYNC).
- Ported from fxceless/victory-fork a803c87 crates/victory-gpu/src/{draw.rs,texture.rs,descriptor/host.rs} (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by fxceless/hyphennn) — textures picked by index looked at again after upload (COMET_HEAP_RECHECK).
- G5 staged clean-texture copy correctness experiment: design reference Belobug/VIctory-Belobug-AMD 01aadd193d57 crates/victory-gpu/src/texture.rs (submission-time uploads); comparative reference fxceless/victory-fork 465b94f54e2f crates/victory-gpu/src/draw.rs (pre-bake texture preparation). GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug and fxceless/hyphennn. Adapted to Comet's existing deferred staging, not a verbatim port.
- Ported/adapted from Belobug/VIctory-Belobug-AMD 4cbe9d7 crates/victory-gpu/src/texpool.rs (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by Belobug) — R3 bounded shared texture heap pilot (COMET_R3_TEXTURE_POOL), staged.
- Ported/adapted from fxceless/victory-fork 085ee7c crates/victory-gpu/src/tiles.rs (GPL-3.0-or-later; VIctory by Marvisak and contributors; fork changes by fxceless/hyphennn) — R3 reserved mip tiling probe and packed-tail planner (COMET_R3_RESERVED_PROBE), staged.
