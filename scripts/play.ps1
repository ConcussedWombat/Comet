# comet launcher: runs the game for normal play (no scripted presses, no capture).
param(
    [Parameter(Mandatory = $true)][string]$Game,
    [string]$Cache = "$env:LOCALAPPDATA\comet\cache",
    [string]$Data = "$env:LOCALAPPDATA\comet\data",
    [string]$Exe = "$PSScriptRoot\..\target\release\comet.exe"
)
foreach ($v in "COMET_KEYS", "COMET_CAPTURE", "COMET_CAPTURE_FRAMES", "COMET_DUMP_TARGETS") {
    Remove-Item "Env:$v" -ErrorAction SilentlyContinue
}
New-Item -ItemType Directory -Force $Cache, $Data | Out-Null
$env:COMET_CACHE_DIR = $Cache
$env:COMET_DATA_DIR = $Data
& $Exe run $Game --execute 2>&1 | Out-File "$env:TEMP\comet.log" -Encoding utf8
Write-Host "Exited with $LASTEXITCODE; log: $env:TEMP\comet.log"
