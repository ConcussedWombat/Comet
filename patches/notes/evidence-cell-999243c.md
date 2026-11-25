# Evidence: Lucia's cell, current main (999243c), reference machine

Captured by the benchmark owner (COMET_CAPTURE_WORLD + COMET_DUMP_TARGETS),
frame 3004 (player in control, facing the window). Text only: no game
assets. Ask on your issue for more of the same.

## What the buffers show

- **Albedo (G-buffer RT0) is right where geometry is drawn**: orange
  "INMATE" shirt, white window frame, yard and inmates.
- **The final image is near-black on the same surfaces** (shirt, frame):
  lighting/exposure, not materials. Facing the bright window, auto-exposure
  adapts to the sky; retail lights the interior with bounced sunlight (ray
  traced); our stand-in constant (COMET_AMBIENT 0.6) is not enough.
- **Depth is empty left of the window and above it**: those wall pieces are
  NOT drawn (other views draw the back wall).
- Scoreboard this run: black albedo late 40-49%, textures waiting 25,534,
  0.81 fps. The texture recheck run reached 9-21% (run-to-run variance is
  large: camera, streaming).

## Draws into the main depth buffer this frame

2,394 draws from 229 pipelines: 2,054 indirect (counts written by the
title's GPU culling), 271 mesh, 69 direct.

Pipelines drawing with **null descriptor slots** (a mesh shader that reads
its vertex data through a null slot draws nothing): the prime suspects for
the missing wall pieces.

```
  53  mesh 30332cf87c4e0eae  null user-data slots: {'u9': 53}
  22  mesh 8563c39d693b36d0  null user-data slots: {'u9': 22}
  21  mesh ba8d52f4ea1a1bac, pixel b37565055f2f7d3b  null user-data slots: {'u8': 15, 'u9': 6}
  19  vertex 1d2c970773ce2183, pixel e070816c3b889df0  null user-data slots: {'u8': 18, 'u10': 1}
  18  vertex 81749e9de9a48b7b, pixel a09dcf112c9345a7  null user-data slots: {'u8': 18}
  18  vertex 5661fc67ffeefd94, pixel b611cae927f41d36  null user-data slots: {'u8': 18}
  15  mesh e135eaf39576263a, pixel 8ba01719b71d1d35  null user-data slots: {'u8': 15}
  12  mesh c7ede3ef60c9fa72, pixel 3f1c61d954b46f31  null user-data slots: {'u8': 4, 'u9': 4, 'u10': 4}
  10  vertex 05cd62db489af324, pixel dc8af7c2fbf4bfa7  null user-data slots: {'u9': 10}
   7  mesh 8c8f0165ddd5199d, pixel 4cde906781d9b043  null user-data slots: {'u8': 4, 'u9': 2, 'u10': 1}
   6  mesh 3d8783a44ba8d08e  null user-data slots: {'u9': 6}
   6  mesh f698acc9d4fa6bba, pixel 9a326171ff144fc9  null user-data slots: {'u8': 2, 'u9': 2, 'u10': 2}
   6  vertex ed734c953c0eac78, pixel 4eb407bd8e8d0223  null user-data slots: {'u9': 4, 'u8': 2}
   6  mesh 04b9be3756e8f41c, pixel 1af4a50c7fb40c97  null user-data slots: {'u8': 2, 'u9': 2, 'u10': 2}
   4  mesh 4c8d9b5bcc24f746  null user-data slots: {'u9': 4}
```

Pipelines on **Belobug's VICTORY_SKIP_PIPELINES** list (distant-tree
imposters whose wind variants read volumes nothing fills and "throw their
quads across the screen"; likely our stray giant triangles) drawing here:
mesh c89489afd89ee7f0 (21x), mesh 45f081396c2c59ba (17x), mesh
63083c777948927a (13x).

## Other measured facts (today)

- Refused pipelines: D3D12 debug layer message 622 "Shader wants wave lane
  count of 64 but maximum wave lane count for device is 32". N1's
  n1-exact-wave32 turned refused 6 -> 1 but untranslated 16 -> 23 ("no safe
  two-half form"): not landed.
- Ray-tracing builder (COMET_BVH_BUILDER=1, L4_WAVE_UNIFORM=1, raw memory
  lists/directory, QUARANTINE=1): runs, no crash for 315 s, drops 9/frame;
  0.12 fps (prepare 4.3 s, readback 2.7 s); picture dark then washes out
  white (inf/NaN in lighting).
- Textures: skip 1 + X1 pressure scan fills the card again (17,563
  waiting); X1 released 0 slots: the title keeps ~10.6 GB of texture memory
  mapped. Residency must follow GPU use (shader-side usage feedback).
