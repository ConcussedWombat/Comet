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
    Write-Host "Benchmark regression test passed."
} finally {
    Remove-Item -LiteralPath $log -Force
}
