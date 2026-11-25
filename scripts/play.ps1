# comet launcher: runs the game for normal play (no scripted presses, no capture).
param(
    [Parameter(Mandatory = $true)][string]$Game,
    [string]$Cache = "$env:LOCALAPPDATA\comet\cache",
    [string]$Data = "$env:LOCALAPPDATA\comet\data",
    [string]$Exe = "$PSScriptRoot\..\target\release\comet.exe",
    [ValidateSet("Inherit", "Baseline", "WideReadback", "Relay", "LateWait")][string]$GpuProfile = "Inherit",
    [switch]$Benchmark,
    [ValidateRange(-1, 2147483647)][int]$AdapterIndex = -1,
    [ValidatePattern("^[A-Za-z0-9_-]{1,48}$")][string]$RunLabel,
    [ValidateSet("Inherit", "Serial", "Parallel")][string]$CardHash = "Inherit",
    [ValidateSet("Inherit", "Serial", "Parallel")][string]$CardMerge = "Inherit",
    [ValidateSet("Inherit", "Baseline", "SmallBuffers")][string]$CardPlacement = "Inherit",
    [string]$DisplayMode = "Inherit"
)
foreach ($v in "COMET_KEYS", "COMET_CAPTURE", "COMET_CAPTURE_FRAMES", "COMET_DUMP_TARGETS") {
    Remove-Item "Env:$v" -ErrorAction SilentlyContinue
}
if ($GpuProfile -ne "Inherit") {
    foreach ($v in "COMET_RELAY", "COMET_LATE_WAIT", "COMET_WIDE_BACK", "COMET_GPU_TIMING") {
        Remove-Item "Env:$v" -ErrorAction SilentlyContinue
    }
    if ($GpuProfile -ne "Relay") { $env:COMET_RELAY = "0" }
    if ($GpuProfile -eq "Baseline") { $env:COMET_LATE_WAIT = "0" }
    if ($GpuProfile -eq "WideReadback") {
        $env:COMET_LATE_WAIT = "0"
        $env:COMET_WIDE_BACK = "1"
    }
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
if ($DisplayMode -eq "Default") {
    Remove-Item Env:COMET_DISPLAY_MODE -ErrorAction SilentlyContinue
} elseif ($DisplayMode -ne "Inherit") {
    if ($DisplayMode -notmatch '^([0-9]{1,4})x([0-9]{1,4})(?:@([0-9]{1,3}))?$') {
        throw "Display mode must be WxH or WxH@Hz, e.g. 1920x1080@60."
    }
    $width = [int]$Matches[1]
    $height = [int]$Matches[2]
    $refresh = if ($Matches.ContainsKey(3)) { [int]$Matches[3] } else { 60 }
    if ($width -lt 640 -or $width -gt 7680 -or $height -lt 360 -or $height -gt 4320 -or $refresh -lt 24 -or $refresh -gt 240) {
        throw "Display mode is outside Comet's supported range."
    }
    $env:COMET_DISPLAY_MODE = "{0}x{1}@{2}" -f $width, $height, $refresh
}
if ($CardHash -ne "Inherit") {
    $env:COMET_PARALLEL_CARD_HASH = if ($CardHash -eq "Parallel") { "1" } else { "0" }
}
if ($CardMerge -ne "Inherit") {
    $env:COMET_WIDE_MERGE = if ($CardMerge -eq "Parallel") { "1" } else { "0" }
}
# Hold the VRAM allocation cap fixed in the placement A/B. Inherit
# intentionally preserves any manually selected budget or threshold.
if ($CardPlacement -ne "Inherit") {
    $env:COMET_CARD_BUFFERS = "1"
    $env:COMET_CARD_BUFFERS_MIN_MB = if ($CardPlacement -eq "SmallBuffers") { "1" } else { "16" }
    $env:COMET_CARD_BUFFERS_MB = "512"
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
