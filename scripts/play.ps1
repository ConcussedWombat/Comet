# comet launcher: runs the game for normal play (no scripted presses, no capture).
param(
    [Parameter(Mandatory = $true)][string]$Game,
    [string]$Cache = "$env:LOCALAPPDATA\comet\cache",
    [string]$Data = "$env:LOCALAPPDATA\comet\data",
    [string]$Exe = "$PSScriptRoot\..\target\release\comet.exe",
    [ValidateSet("Inherit", "Baseline", "Relay", "LateWait")][string]$GpuProfile = "Inherit",
    [switch]$Benchmark
)
foreach ($v in "COMET_KEYS", "COMET_CAPTURE", "COMET_CAPTURE_FRAMES", "COMET_DUMP_TARGETS") {
    Remove-Item "Env:$v" -ErrorAction SilentlyContinue
}
if ($GpuProfile -ne "Inherit") {
    foreach ($v in "COMET_RELAY", "COMET_LATE_WAIT", "COMET_WIDE_BACK", "COMET_GPU_TIMING") {
        Remove-Item "Env:$v" -ErrorAction SilentlyContinue
    }
    if ($GpuProfile -eq "Baseline") { $env:COMET_LATE_WAIT = "0" }
    if ($GpuProfile -eq "Relay") { $env:COMET_RELAY = "2" }
    if ($GpuProfile -eq "LateWait") {
        $env:COMET_LATE_WAIT = "3"
        $env:COMET_WIDE_BACK = "1"
    }
}
if ($Benchmark) {
    $env:COMET_BENCHMARK = "1"
    $env:COMET_LOG = "info"
}
New-Item -ItemType Directory -Force $Cache, $Data | Out-Null
$env:COMET_CACHE_DIR = $Cache
$env:COMET_DATA_DIR = $Data
& $Exe run $Game --execute 2>&1 | Out-File "$env:TEMP\comet.log" -Encoding utf8
$code = $LASTEXITCODE
Write-Host "Exited with $code; log: $env:TEMP\comet.log"
exit $code
