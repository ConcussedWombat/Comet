# Summarize the exact throughput windows emitted by -Benchmark.
param(
    [string]$Log = "$env:TEMP\comet.log",
    [ValidateRange(0, 3600)][int]$SkipSeconds = 30,
    [ValidateRange(1, 3600)][int]$MeasureSeconds = 60
)
$ErrorActionPreference = "Stop"
$elapsed = 0.0
$seconds = 0.0
$frames = 0L
$rates = [System.Collections.Generic.List[double]]::new()
$phaseSamples = 0
$frameIntervals = [System.Collections.Generic.List[long]]::new()
$pacingSamples = 0
$prepareUs = 0.0
$waitUs = 0.0
$readbackUs = 0.0
$uploadBytes = 0.0
$textureBytes = 0.0
$readbackBytes = 0.0
foreach ($line in Get-Content -LiteralPath $Log) {
    if ($line -notmatch 'COMET_BENCHMARK frames=(\d+) elapsed_us=(\d+)') { continue }
    $n = [long]$Matches[1]
    $duration = [long]$Matches[2] / 1000000.0
    if ($duration -le 0 -or $n -le 0) { continue }
    $start = $elapsed
    $elapsed += $duration
    # Only whole windows within the chosen interval; do not estimate fractions.
    if ($start -lt $SkipSeconds) { continue }
    if ($elapsed -gt ($SkipSeconds + $MeasureSeconds)) { break }
    $frames += $n
    $seconds += $duration
    $rates.Add($n / $duration)
    # Pacing records are optional; accept only complete arrays.
    if ($line -match 'frame_us=\[([0-9, ]+)\]') {
        $values = @($Matches[1] -split ',' | ForEach-Object { $_.Trim() })
        if ($values.Count -eq $n -and @($values | Where-Object { $_ -notmatch '^[0-9]{1,18}$' }).Count -eq 0) {
            foreach ($value in $values) { $frameIntervals.Add([long]$value) }
            $pacingSamples++
        }
    }
    # Older logs still work. Never report partial phase totals as if they
    # described the full interval.
    if ($line -match 'prepare_us=(\d+) wait_us=(\d+) readback_us=(\d+) upload_bytes=(\d+) texture_bytes=(\d+) readback_bytes=(\d+)') {
        $phaseSamples++
        $prepareUs += [double]$Matches[1]
        $waitUs += [double]$Matches[2]
        $readbackUs += [double]$Matches[3]
        $uploadBytes += [double]$Matches[4]
        $textureBytes += [double]$Matches[5]
        $readbackBytes += [double]$Matches[6]
    }
}
if ($seconds -le 0 -or $seconds -lt ($MeasureSeconds * 0.8)) {
    throw "Not enough benchmark samples in the requested interval. Run with -Benchmark for longer."
}
$sorted = @($rates | Sort-Object)
$result = [ordered]@{
    Frames = $frames
    Seconds = [math]::Round($seconds, 3)
    Samples = $sorted.Count
    AverageFps = [math]::Round($frames / $seconds, 2)
    MeanFrameMs = [math]::Round(1000 * $seconds / $frames, 2)
    MinWindowFps = [math]::Round($sorted[0], 2)
    MaxWindowFps = [math]::Round($sorted[-1], 2)
}
if ($phaseSamples -eq $rates.Count) {
    $result["PrepareMsPerFrame"] = [math]::Round($prepareUs / 1000 / $frames, 2)
    $result["WaitMsPerFrame"] = [math]::Round($waitUs / 1000 / $frames, 2)
    $result["ReadbackMsPerFrame"] = [math]::Round($readbackUs / 1000 / $frames, 2)
    $result["UploadMBPerFrame"] = [math]::Round($uploadBytes / 1000000 / $frames, 2)
    $result["TextureMBPerFrame"] = [math]::Round($textureBytes / 1000000 / $frames, 2)
    $result["ReadbackMBPerFrame"] = [math]::Round($readbackBytes / 1000000 / $frames, 2)
}
if ($pacingSamples -eq $rates.Count -and $frameIntervals.Count -eq $frames) {
    $frameIntervals.Sort()
    # Nearest-rank percentiles of presented-frame intervals (not GPU latency).
    foreach ($percentile in @(50, 95, 99)) {
        $index = [int]([math]::Ceiling($frameIntervals.Count * $percentile / 100.0) - 1)
        $result["P${percentile}FrameMs"] = [math]::Round($frameIntervals[$index] / 1000.0, 2)
    }
    $result["MaxFrameMs"] = [math]::Round($frameIntervals[$frameIntervals.Count - 1] / 1000.0, 2)
}
[pscustomobject]$result
