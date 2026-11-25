# Synthetic logs verify duration weighting and warm-up exclusion.
$ErrorActionPreference = "Stop"
$log = Join-Path ([IO.Path]::GetTempPath()) ("comet-benchmark-test-" + [guid]::NewGuid() + ".log")
try {
    @(
        "COMET_BENCHMARK frames=100 elapsed_us=1000000",
        "COMET_BENCHMARK frames=4 elapsed_us=2000000",
        "COMET_BENCHMARK frames=6 elapsed_us=1000000",
        "COMET_BENCHMARK frames=99 elapsed_us=1000000"
    ) | Set-Content -LiteralPath $log
    $r = & "$PSScriptRoot\summarize-benchmark.ps1" -Log $log -SkipSeconds 1 -MeasureSeconds 3
    if ($r.Frames -ne 10 -or $r.Seconds -ne 3 -or $r.AverageFps -ne 3.33 -or $r.MinWindowFps -ne 2 -or $r.MaxWindowFps -ne 6) {
        throw "Benchmark summary did not weight samples by duration."
    }
    if ($null -ne $r.PSObject.Properties["WaitMsPerFrame"]) { throw "Legacy logs must not report unavailable phase data." }
    @(
        "COMET_BENCHMARK frames=4 elapsed_us=2000000 prepare_us=40000 wait_us=80000 readback_us=12000 upload_bytes=4000000 texture_bytes=8000000 readback_bytes=2000000",
        "COMET_BENCHMARK frames=6 elapsed_us=1000000 prepare_us=60000 wait_us=120000 readback_us=18000 upload_bytes=6000000 texture_bytes=12000000 readback_bytes=3000000"
    ) | Set-Content -LiteralPath $log
    $r = & "$PSScriptRoot\summarize-benchmark.ps1" -Log $log -SkipSeconds 0 -MeasureSeconds 3
    if ($r.PrepareMsPerFrame -ne 10 -or $r.WaitMsPerFrame -ne 20 -or $r.ReadbackMsPerFrame -ne 3 -or $r.UploadMBPerFrame -ne 1 -or $r.TextureMBPerFrame -ne 2 -or $r.ReadbackMBPerFrame -ne 0.5) {
        throw "Phase totals were not weighted by frame count."
    }
    @(
        "COMET_BENCHMARK frames=4 elapsed_us=1000000",
        "COMET_BENCHMARK frames=6 elapsed_us=1000000 prepare_us=60000 wait_us=120000 readback_us=18000 upload_bytes=6000000 texture_bytes=12000000 readback_bytes=3000000"
    ) | Set-Content -LiteralPath $log
    $r = & "$PSScriptRoot\summarize-benchmark.ps1" -Log $log -SkipSeconds 0 -MeasureSeconds 2
    if ($null -ne $r.PSObject.Properties["WaitMsPerFrame"]) { throw "Mixed logs must not report incomplete phase totals." }
    @(
        "COMET_BENCHMARK frames=2 elapsed_us=1000000 frame_us=[100000, 900000]",
        "COMET_BENCHMARK frames=3 elapsed_us=1000000 frame_us=[200000, 300000, 500000]"
    ) | Set-Content -LiteralPath $log
    $r = & "$PSScriptRoot\summarize-benchmark.ps1" -Log $log -SkipSeconds 0 -MeasureSeconds 2
    if ($r.P50FrameMs -ne 300 -or $r.P95FrameMs -ne 900 -or $r.P99FrameMs -ne 900 -or $r.MaxFrameMs -ne 900) {
        throw "Frame pacing percentiles are incorrect."
    }
    @(
        "COMET_BENCHMARK frames=2 elapsed_us=1000000 frame_us=[100000]",
        "COMET_BENCHMARK frames=3 elapsed_us=1000000 frame_us=[200000, 300000, 500000]"
    ) | Set-Content -LiteralPath $log
    $r = & "$PSScriptRoot\summarize-benchmark.ps1" -Log $log -SkipSeconds 0 -MeasureSeconds 2
    if ($null -ne $r.PSObject.Properties["P99FrameMs"]) { throw "Incomplete pacing data must not be reported." }
    $candidateLog = "$log.candidate"
    try {
        @(
            "COMET_BENCHMARK frames=2 elapsed_us=1000000 prepare_us=0 wait_us=1000 readback_us=1000 upload_bytes=0 texture_bytes=0 readback_bytes=0",
            "COMET_BENCHMARK frames=2 elapsed_us=1000000 prepare_us=0 wait_us=1000 readback_us=1000 upload_bytes=0 texture_bytes=0 readback_bytes=0"
        ) | Set-Content -LiteralPath $log
        @(
            "COMET_BENCHMARK frames=4 elapsed_us=1000000 prepare_us=0 wait_us=1000 readback_us=1000 upload_bytes=0 texture_bytes=0 readback_bytes=0",
            "COMET_BENCHMARK frames=4 elapsed_us=1000000 prepare_us=0 wait_us=1000 readback_us=1000 upload_bytes=0 texture_bytes=0 readback_bytes=0"
        ) | Set-Content -LiteralPath $candidateLog
        $comparison = @(& "$PSScriptRoot\compare-benchmarks.ps1" -BaselineLog $log -CandidateLog $candidateLog -SkipSeconds 0 -MeasureSeconds 2)
        $fps = $comparison | Where-Object Metric -eq 'AverageFps'
        $frameTime = $comparison | Where-Object Metric -eq 'MeanFrameMs'
        $upload = $comparison | Where-Object Metric -eq 'UploadMBPerFrame'
        if ($fps.Baseline -ne 2 -or $fps.Candidate -ne 4 -or $fps.DeltaPercent -ne 100 -or $frameTime.DeltaPercent -ne -50) {
            throw "Benchmark comparison arithmetic is incorrect."
        }
        if ($null -eq $upload -or $null -ne $upload.DeltaPercent -or @($comparison | Where-Object Metric -eq 'P99FrameMs').Count -ne 0) {
            throw "Benchmark comparison must omit missing metrics and avoid division by zero."
        }
    } finally {
        Remove-Item -LiteralPath $candidateLog -Force -ErrorAction SilentlyContinue
    }
    Write-Host "Benchmark regression test passed."
} finally {
    Remove-Item -LiteralPath $log -Force
}
