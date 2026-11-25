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
    Write-Host "Benchmark regression test passed."
} finally {
    Remove-Item -LiteralPath $log -Force
}
