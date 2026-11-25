# comet launcher: runs the game for normal play (no scripted presses, no capture).
param(
    [Parameter(Mandatory = $true)][string]$Game,
    [string]$Cache = "$env:LOCALAPPDATA\comet\cache",
    [string]$Data = "$env:LOCALAPPDATA\comet\data",
    [string]$Exe = "$PSScriptRoot\..\target\release\comet.exe",
    [ValidateSet("Inherit", "Baseline", "Relay", "LateWait")][string]$GpuProfile = "Inherit",
    [switch]$Benchmark,
    [ValidateRange(-1, 2147483647)][int]$AdapterIndex = -1,
    [ValidatePattern("^[A-Za-z0-9_-]{1,48}$")][string]$RunLabel
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
if ($PSBoundParameters.ContainsKey("AdapterIndex")) {
    if ($AdapterIndex -lt 0) {
        Remove-Item Env:COMET_ADAPTER_INDEX -ErrorAction SilentlyContinue
    } else {
        $env:COMET_ADAPTER_INDEX = $AdapterIndex.ToString([Globalization.CultureInfo]::InvariantCulture)
    }
}
if ($Benchmark) {
    $env:COMET_BENCHMARK = "1"
    $env:COMET_LOG = "info"
}
New-Item -ItemType Directory -Force $Cache, $Data | Out-Null
$env:COMET_CACHE_DIR = $Cache
$env:COMET_DATA_DIR = $Data
$logName = if ($RunLabel) { "comet-$RunLabel.log" } else { "comet.log" }
$log = Join-Path ([IO.Path]::GetTempPath()) $logName
& $Exe run $Game --execute 2>&1 | Out-File -LiteralPath $log -Encoding utf8
$code = $LASTEXITCODE
Write-Host "Exited with $code; log: $log"
exit $code
