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
    # A synthetic executable prints the environment it receives and exits.
    $probe = Join-Path $scratch "probe.cmd"
    @('@echo off', 'echo adapter=%COMET_ADAPTER_INDEX%', 'echo benchmark=%COMET_BENCHMARK%', 'exit /b 7') |
        Set-Content -LiteralPath $probe -Encoding ASCII
    $label = "test-" + [guid]::NewGuid().ToString("N")
    $log = Join-Path ([IO.Path]::GetTempPath()) ("comet-" + $label + ".log")
    try {
        & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PSScriptRoot\play.ps1" -Game $missing -Cache (Join-Path $scratch "cache") -Data (Join-Path $scratch "data") -Exe $probe -AdapterIndex 2 -Benchmark -RunLabel $label
        if ($LASTEXITCODE -ne 7) { throw "Synthetic launcher exit code did not propagate." }
        $report = Get-Content -LiteralPath $log -Raw
        if ($report -notmatch 'adapter=2' -or $report -notmatch 'benchmark=1') { throw "Launcher did not pass the selected adapter and benchmark setting." }
        $oldAdapter = $env:COMET_ADAPTER_INDEX
        try {
            $env:COMET_ADAPTER_INDEX = "9"
            & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PSScriptRoot\play.ps1" -Game $missing -Cache (Join-Path $scratch "cache") -Data (Join-Path $scratch "data") -Exe $probe -AdapterIndex -1 -RunLabel $label
            if ($LASTEXITCODE -ne 7) { throw "Default adapter probe failed." }
            if ((Get-Content -LiteralPath $log -Raw) -match 'adapter=9') { throw "Default adapter did not clear the inherited selection." }
        } finally {
            if ($null -eq $oldAdapter) { Remove-Item Env:COMET_ADAPTER_INDEX -ErrorAction SilentlyContinue }
            else { $env:COMET_ADAPTER_INDEX = $oldAdapter }
        }
    } finally {
        Remove-Item -LiteralPath $log -Force -ErrorAction SilentlyContinue
    }
    Write-Host "Launcher exit-code regression test passed."
} finally {
    Remove-Item -LiteralPath $scratch -Recurse -Force
}
