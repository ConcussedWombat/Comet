# Known-good configuration

The best picture measured on the reference machine (RTX 5070 Laptop 8 GB,
32 GB RAM). Nothing may land that makes this configuration look worse.

## 2026-11-25: the "proven set"

```
COMET_R3_TEXTURE_POOL=1           R3: host textures placed in shared heaps (Belobug 4cbe9d7)
COMET_STAGING_POOL=1              staging buffers as slices of pooled upload buffers
COMET_ALIAS_VIEWS=1               N6: other views of written memory kept in step
COMET_M9_TEXTURE_FEEDBACK=1       M9: texture usage feedback (costs GPU time; under test)
COMET_N5_DEXNOM_SPARSE_TEXTURES=1 N5: sparse texture residency (DexNom 921d8d3)
```

Seen with it: Lucia's orange INMATE shirt, the cell lit and warm, walls
and posters present, sharp face close-ups in cutscenes, textures loaded
about twice as fast (50,267 in 274 s in the world against ~28,000 in
315 s). Still wrong: black eyes (Z15), bright hair (Z7), ghost doubles
(Z8), heavy blur (Z9), black wall patches (Z10), haze (Z11), cutscene
stand-ins (Z12), sparkly window glass (Z14).

Cost: 0.76 fps against ~1.05 for each switch alone (GPU wait 913 ms a
frame against ~600). Suspected: M9's feedback writes now that both halves
are on; measured next without it.

Made worse when added (do not turn on with this set until fixed):

- `COMET_AMBIENT_SKY=1` (L12), `COMET_L11_INTEGER_BLEND=1` (L11),
  `COMET_G7_FRESH_BUFFER_OWNER=1` (G7) together: hair bright blond,
  stair-stepped shirt edges, grey noise over posters, black walls.
- `COMET_N5_FULL_RECHECK=1` (N5), `COMET_M4_CHARACTER_FIRST=1`,
  `COMET_M4_BAKE_PREFLIGHT=1` (M4) together: the world was reached only
  late (24 s measured); to be run again alone.
- All of the above at once: 0.70 fps, a long black stretch, haze.
