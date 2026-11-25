<p align="center">
  <img src="docs/comet.svg" alt="Comet" width="480">
</p>

<p align="center"><strong>Xbox Series compatibility layer for Windows</strong></p>

<p align="center">
  <a href="#status">Status</a> ·
  <a href="#getting-started">Getting started</a> ·
  <a href="#benchmarking">Benchmarking</a> ·
  <a href="#development">Development</a>
</p>

Comet runs an Xbox Series title’s x86-64 code on Windows by translating console APIs and GPU shaders to their PC equivalents. Development currently targets NVIDIA GPUs.

## Status

**Experimental.** The table reports the latest Windows validation and earlier diagnostic samples where noted.

| Area | Verified result | Scope |
| :--- | :--- | :--- |
| **Gameplay** | In-world prologue reached | Validated in the prison-yard scene on Windows. |
| **Frame rate** | **0.95 FPS** with Relay off | 120-second in-world sample. That is **6.3%** of the 15 FPS milestone and **3.2%** of the title’s 30 FPS cap. |
| **Shader translation** | **5/5 selected compute shaders (100% of the sample)** translated and built host pipelines | Earlier diagnostic of five high-cost compute passes. Overall shader translation coverage is unmeasured. [Compute shader notes](docs/worker2-compute-triage.md) |
| **Lighting** | Visible lighting restored | The tested yard and some faces lit up with Relay off. |
| **Geometry** | **163/165 (98.8%)** first-use pipeline misses resolved on a later request | Counts pipeline misses in one diagnostic run. Some geometry can still appear late. |
| **Textures** | Compressed texture residency and BCPack validation fixes enabled by default | Texture artifacts remain under investigation. |
| **Hair** | Severe strand artifacts corrected in the tested scene | The constant-buffer fix is enabled by default. [Hair validation notes](docs/independent-review.md#batch-1-landed-benchmark-owner-runtime-verified) |
| **GPU submission** | **Relay is off by default** | Relay reached **1.22 FPS** in the same window and produced stale, dark rendering. |
| **Build validation** | **17/17 staged patch versions** passed both Windows builds and both Clippy modes | Workspace test runs reproduced a pre-existing DirectStorage close-queue failure. [Build-gate results](docs/independent-review.md#build-gate) |

[Windows validation and benchmark notes](docs/independent-review.md#batch-2-landed-benchmark-owner-runtime-verified)

## Getting started

### Requirements

- 64-bit Windows 10 or 11 and an AVX2-capable CPU.
- An NVIDIA RTX 20 Series or newer GPU with Direct3D 12, shader model 6.6, and mesh shader support. Other vendors are experimental.
- Several GB of available Windows commit memory.
- The pinned [Rust toolchain](rust-toolchain.toml) and MSVC C++ build tools.
- Your own legally obtained Xbox Series title files.

### Build and launch

```powershell
git clone https://github.com/Fyntra-Development/Comet.git
cd Comet
cargo build --release -p comet --locked
.\target\release\comet.exe doctor
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\launcher.ps1
```

The doctor checks the selected adapter, VRAM, AVX2, shader model, mesh shaders, and wave size. The launcher lets you select your title executable, shader cache, save location, and display mode. Use an XInput controller or the keyboard while the game window is focused.

To launch from PowerShell, enter the full path to your title executable when prompted.

```powershell
$game = (Read-Host 'Full path to the Xbox Series executable').Trim('"')
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\play.ps1 -Game $game
```

The play script accepts `-Cache` and `-Data` to override the default folders under `%LOCALAPPDATA%\comet`. Use `-AdapterIndex` with the index reported by the doctor if you want to select a GPU explicitly.

### Troubleshooting

- For Windows error 1455 in `crashes.log`, check the paging file and available commit memory.
- If you change GPUs, run the doctor again before testing.
- The play script writes `comet.log` in the Windows temporary directory. Set `-RunLabel` to keep separate logs.
- If you are migrating from ZTRX, use `comet.exe`, `comet.toml`, and the `COMET_` environment prefix. Move saves to `%LOCALAPPDATA%\comet\data` or select their existing location with `-Data`. Shader caches may need to be rebuilt.

## Benchmarking

Warm the shader cache, then keep the build, saved scene, GPU driver, and display mode fixed. Run for at least 100 seconds, exit normally, and use the same measurement window for each run.

```powershell
$game = (Read-Host 'Full path to the Xbox Series executable').Trim('"')
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\play.ps1 -Game $game -GpuProfile Baseline -Benchmark -RunLabel baseline
$baseline = Join-Path ([IO.Path]::GetTempPath()) 'comet-baseline.log'
.\scripts\summarize-benchmark.ps1 -Log $baseline -SkipSeconds 30 -MeasureSeconds 60
```

For an A/B run, repeat with one changed setting and a new `-RunLabel`. Compare the labeled logs with the same `-SkipSeconds` and `-MeasureSeconds`. Report FPS, mean frame time, P95 when available, rendering changes, and stability. GPU timing changes submission behavior, so run it separately. [Benchmark comparison script](scripts/compare-benchmarks.ps1)

<details>
<summary>GPU profiles and tuning options</summary>

| Option | What it changes |
| :--- | :--- |
| `-GpuProfile Baseline` | Synchronous GPU submission. Use this as the reference. |
| `-GpuProfile Relay` | Experimental overlapping submission and copy-back. Check for stale or dark rendering. |
| `-GpuProfile LateWait` | Experimental late wait stage 3 with wide copy-back. |
| `-GpuProfile WideReadback` | Batches system-memory snapshots and comparisons across CPU workers with synchronous GPU waits. |
| `-DisplayMode 1920x1080@60` | Reports a virtual 1080p display to the title. Use `Default` to restore the virtual 4K/60 Hz mode. The title selects its render resolution and frame rate. |
| `-CardHash Serial` or `Parallel` | Compares GPU memory using serial or parallel hashing. Parallel hashing is the default and falls back to serial if shader or pipeline creation fails. |
| `-CardMerge Serial` or `Parallel` | Compares CPU merge modes for synchronous card-resident readback. |
| `-CardPlacement Baseline` or `SmallBuffers` | Compares a 16 MiB or 1 MiB minimum for card buffers. Both profiles use a 512 MiB cap and still enforce the local VRAM budget. |

Relay remains off by default. Set `COMET_RELAY=2` to enable it, or set `COMET_RELAY_DEFAULT=1` to rehearse the proposed default with the explicit Relay and late-wait settings unset. Set `COMET_RELAY=0` to override that rehearsal. Host and shadow share the same mode selection. When Relay is enabled, `COMET_RELAY_SAFE=1` is the default and waits for overlapping staging and shadow accesses. For diagnostics, `COMET_RELAY_SAFE=2` serializes flight landing and `COMET_RELAY_SAFE=0` reproduces unsafe overlap. LateWait uses `COMET_RELAY=0 COMET_LATE_WAIT=3 COMET_WIDE_BACK=1`.

If the logs identify a specific bottleneck, test one change at a time. For texture scheduling or refresh, the opt-ins include `COMET_HEAP_TEXTURE_BUDGET_MS=16`, `COMET_TEXTURE_SHORT_LOCK=1`, and `COMET_WIDE_TEXTURES=1`. For page mapping, test `COMET_BATCH_PAGE_ALIASES=1` or `COMET_REUSE_PAGE_ALIASES=1`. The synchronous `COMET_RETAIN_CARD_WRITES=1` experiment limits reuploads of GPU-produced data. Check visual correctness and stability alongside timing and memory counters. [Performance investigations and validation runs](docs/independent-review.md)

</details>

## Development

| Crate | Purpose |
| :--- | :--- |
| [comet-loader](crates/comet-loader) | Maps and starts native title code. |
| [comet-hle](crates/comet-hle) | Implements console runtime APIs. |
| [comet-gpu](crates/comet-gpu) | Translates D3D12.X commands and resources to host D3D12. |
| [comet-shader](crates/comet-shader) | Translates native GPU programs to host shaders. |
| [comet-formats](crates/comet-formats) | Handles texture layouts and audio formats. |
| [comet-pipeline](crates/comet-pipeline) | Handles conversion and content-addressed caching. |

Read the [architecture notes](docs/architecture.md) for the project layout and the [ray-tracing notes](docs/fork-ray-tracing.md) for implementation details. Keep game files, extracted data, shader dumps, and game screenshots out of the repository. [Asset policy](docs/asset-policy.md)

```powershell
cargo xtask install-hooks
cargo xtask policy
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

<details>
<summary>Diagnostics build</summary>

```powershell
cargo build --profile diag -p comet --features diagnostics --locked
```

The diagnostics build supports frame capture via `COMET_CAPTURE` and `COMET_CAPTURE_FRAMES`, and scripted input via `COMET_KEYS`. Run the diagnostics executable directly when using these settings. Enable `COMET_D3D12_DRED=1` to report commands in flight on GPU failure.

</details>

## Credits and license

Comet is maintained by Fyntra Development and is a fork of [ZTRX](https://github.com/Zettrox/ztrx), based on VIctory (Marvisak) and victory-fork (fxceless). Original authors and license notices are retained. The project is independent of the title’s developer and publisher.

Comet is licensed under [GPL-3.0-or-later](LICENSE). See the [asset policy](docs/asset-policy.md) for the BCPack format-table exception.