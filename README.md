<p align="center">
  <img src="docs/comet.svg" alt="Comet" width="680">
</p>

<h3 align="center">Xbox Series compatibility layer for Windows</h3>

<p align="center">
  <a href="#getting-started">Getting started</a> ·
  <a href="#current-state">Status</a> ·
  <a href="#performance">Performance</a> ·
  <a href="#development">Development</a>
</p>

Comet runs an Xbox Series title's x86-64 game code directly on Windows, translating console APIs and GPU shaders to their PC equivalents. The current focus is getting the graphics right and improving frame times on NVIDIA hardware.

Comet is experimental. Bring your own legally obtained game files; this repository does not distribute the game or its assets. It is not affiliated with the game's developer or publisher.

## Getting started

**Requirements**

- Windows 10 or 11 (64-bit), with an AVX2-capable CPU.
- Direct3D 12 GPU with shader model 6.6 and mesh shaders. NVIDIA RTX 20 series or newer is the current target; other vendors are experimental.
- Several GB of free Windows commit memory.
- To build from source: Rust (pinned in [rust-toolchain.toml](https://github.com/Fyntra-Development/Comet/blob/main/rust-toolchain.toml)) and the MSVC C++ build tools.

Download a Windows build from [Releases](https://github.com/Fyntra-Development/Comet/releases), if available, or build from source:

```powershell
git clone https://github.com/Fyntra-Development/Comet.git
cd Comet
cargo build --release -p comet
```

Check your hardware before launching a game:

```powershell
.\target\release\comet.exe doctor
```

The doctor reports the selected adapter, VRAM, AVX2, shader model, mesh shaders and wave size. A passing result is not a guarantee of gameplay compatibility. To select a specific GPU, set `$env:COMET_ADAPTER_INDEX = "1"` (replace 1 with the index reported by the doctor). Remove the variable to restore the default selection. Software adapters are refused.

### Launch

From the repository or extracted release folder, open the desktop launcher:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\launcher.ps1
```

Or launch directly from PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\play.ps1 -Game "D:\game\game.exe"
```

The launcher accepts custom game, shader cache and save locations. With the script, use `-Cache` and `-Data` to override the default folders under `%LOCALAPPDATA%\comet`. Use an XInput controller (DualShock/DualSense via DS4Windows also works), or the keyboard while the game window is focused.

## Current state

The following results come from upstream testing on Windows 11 with an RTX 3050 (8 GB). They are not new benchmarks of Comet.

| Area | Upstream result |
| :--- | :--- |
| Startup | Boot, first-boot screens, intro and opening credits work. |
| Gameplay | Prologue playable from the cell through the boat chase. |
| Stability | Over 90 minutes without a crash in one session. |
| Performance | 1–3 FPS in the world; the first run is slower while shaders are translated. |
| Graphics | Missing ray-traced lighting, dark shadows, overexposure, some block artifacts and incorrect boat height. |

Ray-tracing shader translation and BVH traversal are in development. Lighting is not working end to end yet. See the [ray-tracing notes](https://github.com/Fyntra-Development/Comet/blob/main/docs/fork-ray-tracing.md) for the implementation and remaining gaps.

## Performance

Shader translation is cached between runs. Warm the cache before comparing performance, and keep the same save, scene, GPU and driver.

The default launcher inherits your settings. To test a specific GPU submission path:

| Profile | Submission path |
| :--- | :--- |
| `Baseline` | Synchronous submission |
| `Relay` | Experimental relay stage 2 |
| `LateWait` | Experimental late wait stage 3 and wide copy-back |

```powershell
.\scripts\play.ps1 -Game "D:\game\game.exe" -GpuProfile Baseline -Benchmark
.\scripts\summarize-benchmark.ps1 -SkipSeconds 30 -MeasureSeconds 60
```

Run for at least 100 seconds and exit normally. Summarize before the next run, since the log is overwritten. Repeat with `Relay` and `LateWait`; report average FPS, mean frame time, GPU and driver, rendering issues and crashes. Min/max values represent sample-window throughput, not individual-frame lows or highs. Do not enable GPU timing for these comparisons: it synchronizes submissions.

The equivalent experimental environment settings are `COMET_RELAY=2`, or `COMET_LATE_WAIT=3` with `COMET_WIDE_BACK=1`.

## Development

The project is split into Rust crates:

| Crate | Purpose |
| :--- | :--- |
| [comet-loader](https://github.com/Fyntra-Development/Comet/tree/main/crates/comet-loader) | Maps and starts native title code. |
| [comet-hle](https://github.com/Fyntra-Development/Comet/tree/main/crates/comet-hle) | Implements console runtime APIs. |
| [comet-gpu](https://github.com/Fyntra-Development/Comet/tree/main/crates/comet-gpu) | Translates D3D12.X commands and resources to host D3D12. |
| [comet-shader](https://github.com/Fyntra-Development/Comet/tree/main/crates/comet-shader) | Translates native GPU programs to host shaders. |
| [comet-formats](https://github.com/Fyntra-Development/Comet/tree/main/crates/comet-formats) | Texture layouts and audio formats. |
| [comet-pipeline](https://github.com/Fyntra-Development/Comet/tree/main/crates/comet-pipeline) | Conversion and content-addressed caching. |

Read the [architecture notes](https://github.com/Fyntra-Development/Comet/blob/main/docs/architecture.md) for the full layout and [asset policy](https://github.com/Fyntra-Development/Comet/blob/main/docs/asset-policy.md) before contributing. Do not commit game files, shader dumps, extracted data or game screenshots.

```powershell
cargo xtask install-hooks
cargo xtask policy
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

### Troubleshooting

- If `crashes.log` reports error 1455, increase the Windows paging file.
- If changing GPUs, run the doctor again before testing.
- Launch logs are written to `%TEMP%\comet.log`.

For extra debugging support, build with:

```powershell
cargo build --profile diag -p comet --features diagnostics
```

| Environment variable | Purpose |
| :--- | :--- |
| `COMET_CAPTURE`, `COMET_CAPTURE_FRAMES` | Save specified frames to a directory (diagnostics build). |
| `COMET_KEYS` | Script key presses, e.g. `30:Enter,35:X` (diagnostics build). |
| `COMET_D3D12_DRED=1` | Report commands in flight on GPU failure. |
| `COMET_PIPELINE_WAIT_MS=24` | Shorten the wait for pipelines still being built. |

The normal play script clears capture and scripted-input variables; run the diagnostics executable directly when using those options.

### Migrating from ZTRX

Use `comet.exe`, `comet.toml` and the `COMET_` environment variable prefix. Existing saves can be moved to `%LOCALAPPDATA%\comet\data` or kept in place with `-Data`. Shader caches may need to be rebuilt.

## Credits & license

Comet is maintained by Fyntra Development and is a fork of [ZTRX](https://github.com/Zettrox/ztrx), based on VIctory (Marvisak) and victory-fork (fxceless). Original authors and license notices are retained.

Licensed under [GPL-3.0-or-later](LICENSE). See the asset policy for the BCPack format-table exception.
