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
$rates = @()
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
    $rates += $n / $duration
}
if ($seconds -le 0 -or $seconds -lt ($MeasureSeconds * 0.8)) {
    throw "Not enough benchmark samples in the requested interval. Run with -Benchmark for longer."
}
$sorted = @($rates | Sort-Object)
[pscustomobject]@{
    Frames = $frames
    Seconds = [math]::Round($seconds, 3)
    Samples = $sorted.Count
    AverageFps = [math]::Round($frames / $seconds, 2)
    MeanFrameMs = [math]::Round(1000 * $seconds / $frames, 2)
    MinWindowFps = [math]::Round($sorted[0], 2)
    MaxWindowFps = [math]::Round($sorted[-1], 2)
}
