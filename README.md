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

### Display mode

Comet reports a virtual 4K/60 Hz console display by default. To test whether the title scales its rendering workload with a lower resolution, set:

```powershell
$env:COMET_DISPLAY_MODE = "1920x1080@60"
.\scripts\play.ps1 -Game "D:\game\game.exe"
```

The desktop launcher includes virtual display presets, and the play script accepts `-DisplayMode 1920x1080@60` or `-DisplayMode Default`. Other examples are `2560x1440@60` and `1920x1080@120`. The mode is a hint to the title, not a forced render resolution, frame rate or Windows display change. Actual performance must be measured. Remove the override with `Remove-Item Env:COMET_DISPLAY_MODE`.

### Benchmarking

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

Run for at least 100 seconds and exit normally. Use `-RunLabel baseline`, `-RunLabel relay` and `-RunLabel latewait` to retain separate logs under `%TEMP%` (for example, `comet-baseline.log`). Pass the matching path to `summarize-benchmark.ps1 -Log`. Without a label, summarize before the next run because the log is overwritten. The launcher also accepts `-AdapterIndex 1` to select a hardware adapter or `-AdapterIndex -1` to restore the Windows default. Repeat with `Relay` and `LateWait`; report average FPS, mean frame time, GPU and driver, rendering issues and crashes. Min/max values represent sample-window throughput, not individual-frame lows or highs. Do not enable GPU timing for these comparisons: it synchronizes submissions.

The equivalent experimental environment settings are `COMET_RELAY=2`, or `COMET_LATE_WAIT=3` with `COMET_WIDE_BACK=1`.

Benchmark summaries also report preparation, GPU-wait and readback milliseconds per frame, and uploaded buffer, texture and readback MB per frame, when every selected sample includes those counters. These are accumulated subsystem costs; asynchronous workers can overlap, so the costs need not add up to frame time. Old logs still report throughput alone. New benchmark logs also contain individual presented-frame intervals, summarized as P50, P95, P99 and maximum frame time. These quantify pacing and stutter, not GPU execution latency.

To compare two labeled benchmark runs, use the same warm-up and measurement interval:

```powershell
.\scripts\compare-benchmarks.ps1 -BaselineLog "$env:TEMP\comet-baseline.log" -CandidateLog "$env:TEMP\comet-relay.log" -SkipSeconds 30 -MeasureSeconds 60
```

The comparison lists signed differences and percentages (positive FPS and negative frame time are improvements). Missing metrics are omitted and percentages with a zero baseline are left blank. Pipe to `Export-Csv -NoTypeInformation` to save results. These measurements do not substitute for checking image quality and stability.

### Parallel CPU readback experiment

Choose `-GpuProfile WideReadback` to batch snapshots and CPU comparisons across existing workers while retaining synchronous GPU fence waits. Exact dirty bounds are calculated on those workers; the merge keeps its original order. This isolates CPU readback from Relay/LateWait and is experimental, not the default.

```powershell
.\scripts\play.ps1 -Game "D:\game\game.exe" -GpuProfile Baseline -CardHash Serial -Benchmark -RunLabel cpu-base
.\scripts\play.ps1 -Game "D:\game\game.exe" -GpuProfile WideReadback -CardHash Serial -Benchmark -RunLabel cpu-wide
```

Run each command separately with the same warm cache and in-world scene, and compare the resulting logs. No title compute or rendering work is skipped. Check correctness and frame pacing in addition to throughput.

For a separate test of CPU merging of **card-resident** readbacks, keep the GPU profile and hash settings identical and compare `-CardMerge Serial` against `-CardMerge Parallel`. The latter sets `COMET_WIDE_MERGE=1`, an existing experimental worker path for synchronous card readback; it does not alter the GPU hash algorithm. Use separate named logs, verify rendered output and stability, and return to Serial if there is a regression. This is independent of `WideReadback`, which batches snapshots and comparisons of system-memory buffers. Inherit preserves the environment (the runtime defaults to serial merging).

### CPU readback and VRAM placement experiment

The frame log now breaks down CPU-scanned buffer MiB/frame by why the
buffer stayed in system memory (the line starts with `CPU-scanned buffer MiB/frame by VRAM fallback reason`). If `below_min` dominates, compare a lower size threshold; if `cap` dominates, compare a larger explicit allocation cap, subject to the GPU's available VRAM. The runtime still checks the local video-memory budget before admitting each buffer.

Keep the GPU profile, scene, driver, warmed cache and other settings fixed. Start a fresh process for every run because buffer placement happens at creation. For example, on a machine with sufficient VRAM:

```powershell
$env:COMET_CARD_BUFFERS = "1"
$env:COMET_CARD_BUFFERS_MIN_MB = "16"
$env:COMET_CARD_BUFFERS_MB = "512"
.\scripts\play.ps1 -Game "D:\game\game.exe" -GpuProfile Baseline -Benchmark -RunLabel card-base

$env:COMET_CARD_BUFFERS_MIN_MB = "1"
.\scripts\play.ps1 -Game "D:\game\game.exe" -GpuProfile Baseline -Benchmark -RunLabel card-min1

# Only if the cap dominates and there is sufficient local VRAM:
$env:COMET_CARD_BUFFERS_MB = "1024"
.\scripts\play.ps1 -Game "D:\game\game.exe" -GpuProfile Baseline -Benchmark -RunLabel card-min1-cap1024

Remove-Item Env:COMET_CARD_BUFFERS, Env:COMET_CARD_BUFFERS_MIN_MB, Env:COMET_CARD_BUFFERS_MB
```

Compare each run's CPU readback time, GPU busy time, local memory budget, texture backlog, visual correctness and crashes. Moving work to the GPU can be slower if it is already saturated or pushes textures out of VRAM; these are experiments, not recommended default settings. Repeat each run before drawing conclusions.

### GPU memory hashing

Comet now uses a 64-thread group per 64 KiB GPU memory piece by default.
Each lane uses four independent hash chains and coalesced loads, rather
than a single thread walking the whole piece serially. It retains the
same 128-bit hash storage and readback granularity. On the recorded
0.4 FPS world sample, 41,719 pieces were compared per frame (about
2.55 GiB of hash input), making the serial path a particularly poor
fit. Gameplay performance and correctness of the new default remain
unverified; a shader/PSO creation failure falls back to serial. Set
`COMET_PARALLEL_CARD_HASH=0` or choose `-CardHash Serial` for the old
path; `-CardHash Parallel` forces the new path. Inherit keeps your
environment settings.

To A/B test, keep the same GPU profile and warmed cache, restart Comet
for each run, and use the same in-world scene:

```powershell
$env:COMET_PARALLEL_CARD_HASH = "0"
.\scripts\play.ps1 -Game "D:\game\game.exe" -GpuProfile Relay -Benchmark
.\scripts\summarize-benchmark.ps1 -SkipSeconds 30 -MeasureSeconds 60
$env:COMET_PARALLEL_CARD_HASH = "1"
.\scripts\play.ps1 -Game "D:\game\game.exe" -GpuProfile Relay -Benchmark
.\scripts\summarize-benchmark.ps1 -SkipSeconds 30 -MeasureSeconds 60
Remove-Item Env:COMET_PARALLEL_CARD_HASH
```

Check the log for `card compare ready` with `parallel_hash=true` to
verify the experimental shader was selected. If the shader or pipeline
cannot be created, Comet falls back to the serial version. Compare FPS,
frame time, rendering, stability and the card-copy/readback statistics;
do not treat higher FPS with incorrect graphics or memory as a success.

### Diagnose slow in-world gameplay

The targeted game is capped at 30 FPS (33.3 ms per presented frame).
Stable 30 FPS in-world is the final performance goal; 15 FPS (66.7 ms)
is an intermediate milestone. A 1 FPS scene takes about 1,000 ms, so
identify the largest measured bottleneck first. A 60 or 120 Hz display
mode does not remove the game's frame-rate cap.

Current builds join overlapping GPU hash runs by default and compare each
64 KiB piece only once per submission. Adjacent GPU readbacks also share
copy commands where their source and destination are contiguous. Compare
the log's hash/compare passes, unique pieces and readback copy commands
before and after these changes.

For an experimental faster run with the same warmed cache and save:

```powershell
.\scripts\play.ps1 -Game "D:\game\game.exe" -GpuProfile Relay -CardHash Parallel -DisplayMode 1280x720@60 -Benchmark -RunLabel fast
.\scripts\summarize-benchmark.ps1 -Log "$env:TEMP\comet-fast.log" -SkipSeconds 30 -MeasureSeconds 60
```

This combines several experimental options; it is not a validated 15 FPS
preset. Compare the same scene with Baseline, then vary one setting at a
time if investigating the cause. The display mode is only a title hint.
If the log reports substantial `bindless texture scheduling` time,
experiment separately with `$env:COMET_HEAP_TEXTURE_BUDGET_MS = "16"`
before launching. It caps the background scheduling loop between texture
operations (default 250 ms, 1–250 accepted); it is not a hard frame cap
and does not skip explicit draw texture reads. Watch for missing/late
textures and growing queue depth. Restore with
`Remove-Item Env:COMET_HEAP_TEXTURE_BUDGET_MS`.

If texture refreshes spend substantial time waiting on title-memory remaps,
try `$env:COMET_TEXTURE_SHORT_LOCK = "1"` in a separate A/B run. Changed
fully mapped textures are copied to private scratch so hashing the snapshot
and detiling do not hold up the streaming thread. Partly mapped textures
and the kernel-copy path now release the mapping lock before conversion
without an extra copy. The optional full-texture snapshot trades extra CPU
memory bandwidth for shorter lock holds; measure before enabling it by
default. Restore with `Remove-Item Env:COMET_TEXTURE_SHORT_LOCK`.

When reporting 1 FPS, include the in-world `fps, ms/frame` line,
`bindless texture scheduling`, `card readback per frame`, the GPU
memory hash/compare line, and relay/settle statistics if enabled. Include
GPU/driver, display mode, profile, cache warm-up and scene; never upload
game assets or proprietary shader dumps.

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
