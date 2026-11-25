# Regression test: no game or GPU is needed.
param([string]$Exe = "$PSScriptRoot\..\target\release\comet.exe")
$ErrorActionPreference = "Stop"
$Exe = (Resolve-Path -LiteralPath $Exe).Path
$scratch = Join-Path ([IO.Path]::GetTempPath()) ("comet-launch-test-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    $missing = Join-Path $scratch "missing.exe"
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $Exe run $missing --execute 2>&1 | Out-Null
        $expected = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previous
    }
    if ($expected -eq 0) { throw "Expected the missing title to fail." }
    & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PSScriptRoot\play.ps1" -Game $missing -Cache (Join-Path $scratch "cache") -Data (Join-Path $scratch "data") -Exe $Exe
    $actual = $LASTEXITCODE
    if ($actual -ne $expected) { throw "Launcher returned $actual; expected $expected." }
    Write-Host "Launcher exit-code regression test passed."
} finally {
    Remove-Item -LiteralPath $scratch -Recurse -Force
}
