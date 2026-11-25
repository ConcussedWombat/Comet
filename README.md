# Comet

**Comet** runs the Xbox Series build of the game natively on Windows PCs with
NVIDIA graphics cards, from game files you supply yourself.

**This repository contains no game files or assets.** You need your own copy
of the game. Not affiliated with or endorsed by its developer or publisher.

## Upstream baseline

Reported by upstream on Windows 11 with an **NVIDIA GeForce RTX 3050** (8 GB):

| | |
|---|---|
| Boot, first-boot screens, intro, opening credits | works |
| Prologue | playable from the cell to the boat chase: cell, cafeteria, robbery, police chase (wanted level, minimap), boat, binoculars |
| Stability | 1 h 30 min+ in one session without a crash |
| Frame rate | 1-3 fps in the world (the first run is slower while shaders translate; they are cached after that) |
| Known problems | anything in shadow is black and bright scenes overexpose (the game's ray-traced lighting does not run yet); some blocky artifacts; boats sit low in the water |

## Requirements

- Windows 10/11 x64, a CPU with AVX2
- A Direct3D 12 GPU with shader model 6.6 and mesh shaders (NVIDIA RTX 20 series or newer)
- Rust (the version in `rust-toolchain.toml`)
- Several GB of free Windows commit memory (raise the paging file if `crashes.log` reports error 1455)

## Build

```powershell
cargo build --release -p comet
```

## Play

```powershell
powershell -ExecutionPolicy Bypass -File scripts\play.ps1 -Game "<your game folder>\<the game's exe>"
```

`scripts\play.ps1` keeps the shader cache and saves in folders you choose
(`-Cache`, `-Data`) and presses no buttons for you. Controller: XInput pads
(a DualShock/DualSense through DS4Windows works). With no pad connected the
keyboard acts as one while the game window has focus.

## Migrating from ZTRX

The executable is now `comet.exe`, configuration uses `comet.toml`, and environment variables use the `COMET_` prefix. Default caches and saves now live in `%LOCALAPPDATA%\comet`. To keep existing saves, point `-Data` at your previous data directory or copy it to the new one. Existing caches may be rebuilt.

## Faster settings (optional, off by default)

`COMET_RELAY=2`, or `COMET_LATE_WAIT=3 COMET_WIDE_BACK=1` cut frame
time on upstream's test machines. Set them in PowerShell before running the script.

## Performance profiles and benchmarks

The default launcher preserves your existing settings. Use an explicit profile to compare the conservative path against upstream's experimental asynchronous paths:

- `-GpuProfile Baseline`: synchronous submission.
- `-GpuProfile Relay`: experimental relay stage 2.
- `-GpuProfile LateWait`: experimental late wait stage 3 and wide copy-back.

Run the same save and scene on the same machine with each profile. Warm the shader cache first. Play for at least 100 seconds, exit normally, and summarize before launching another run (the log is overwritten).

```powershell
.\scripts\play.ps1 -Game "D:\game\game.exe" -GpuProfile Baseline -Benchmark
.\scripts\summarize-benchmark.ps1 -SkipSeconds 30 -MeasureSeconds 60
```

Repeat with Relay and LateWait. Report average FPS, mean frame time, visible rendering issues, crashes, GPU and driver version. The min/max values are sample-window throughput, not individual-frame lows or highs. Do not use GPU timing mode for these comparisons; it deliberately synchronizes submissions.

## Debugging switches

| Switch | Effect |
|---|---|
| `COMET_CAPTURE=<dir>` + `COMET_CAPTURE_FRAMES=<list>` | save those frames as PNG (diagnostics build) |
| `COMET_KEYS=30:Enter,35:X` | press keys at those seconds (diagnostics build) |
| `COMET_D3D12_DRED=1` | on a GPU failure, report the commands in flight |
| `COMET_PIPELINE_WAIT_MS=24` | shorter per-frame wait for pipelines still being built |

Diagnostics build: `cargo build --profile diag -p comet --features diagnostics`

## License

GPL-3.0-or-later (see `LICENSE`). Comet is maintained by Fyntra Development and is a fork of [ZTRX](https://github.com/Zettrox/ztrx), based on VIctory (Marvisak) and victory-fork (fxceless). Original authors and license notices are retained. Rebranded November 24, 2026.
