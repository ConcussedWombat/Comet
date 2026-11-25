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
    @('@echo off', 'echo adapter=%COMET_ADAPTER_INDEX%', 'echo benchmark=%COMET_BENCHMARK%', 'echo display=%COMET_DISPLAY_MODE%', 'echo hash=%COMET_PARALLEL_CARD_HASH%', 'echo cardmerge=%COMET_WIDE_MERGE%', 'echo cardenabled=%COMET_CARD_BUFFERS%', 'echo cardmin=%COMET_CARD_BUFFERS_MIN_MB%', 'echo cardcap=%COMET_CARD_BUFFERS_MB%', 'echo wide=%COMET_WIDE_BACK%', 'echo late=%COMET_LATE_WAIT%', 'echo relay=%COMET_RELAY%', 'exit /b 7') |
        Set-Content -LiteralPath $probe -Encoding ASCII
    $label = "test-" + [guid]::NewGuid().ToString("N")
    $log = Join-Path ([IO.Path]::GetTempPath()) ("comet-" + $label + ".log")
    try {
        & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PSScriptRoot\play.ps1" -Game $missing -Cache (Join-Path $scratch "cache") -Data (Join-Path $scratch "data") -Exe $probe -AdapterIndex 2 -Benchmark -DisplayMode '1920x1080@120' -CardHash Parallel -CardMerge Parallel -RunLabel $label
        if ($LASTEXITCODE -ne 7) { throw "Synthetic launcher exit code did not propagate." }
        $report = Get-Content -LiteralPath $log -Raw
        if ($report -notmatch 'adapter=2' -or $report -notmatch 'benchmark=1') { throw "Launcher did not pass the selected adapter and benchmark setting." }
        if ($report -notmatch 'display=1920x1080@120' -or $report -notmatch 'hash=1' -or $report -notmatch 'cardmerge=1') { throw "Launcher did not pass display and hash settings." }
        & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PSScriptRoot\play.ps1" -Game $missing -Cache (Join-Path $scratch "cache") -Data (Join-Path $scratch "data") -Exe $probe -GpuProfile WideReadback -RunLabel $label
        if ($LASTEXITCODE -ne 7) { throw "Wide readback probe failed." }
        $report = Get-Content -LiteralPath $log -Raw
        if ($report -notmatch 'wide=1' -or $report -notmatch 'late=0' -or $report -match 'relay=[12]') {
            throw "WideReadback must enable parallel CPU readback without async submission."
        }
        & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PSScriptRoot\play.ps1" -Game $missing -Cache (Join-Path $scratch "cache") -Data (Join-Path $scratch "data") -Exe $probe -GpuProfile Baseline -RunLabel $label
        if ($LASTEXITCODE -ne 7) { throw "Baseline probe failed." }
        if ((Get-Content -LiteralPath $log -Raw) -match 'wide=1') { throw "Baseline must not enable wide readback." }
        foreach ($placement in "Baseline", "SmallBuffers") {
            & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PSScriptRoot\play.ps1" -Game $missing -Cache (Join-Path $scratch "cache") -Data (Join-Path $scratch "data") -Exe $probe -CardPlacement $placement -RunLabel $label
            if ($LASTEXITCODE -ne 7) { throw "Card placement probe failed." }
            $report = Get-Content -LiteralPath $log -Raw
            $minimum = if ($placement -eq "Baseline") { "16" } else { "1" }
            if ($report -notmatch '(?m)^cardenabled=1\r?$' -or $report -notmatch "(?m)^cardmin=$minimum\r?$" -or $report -notmatch '(?m)^cardcap=512\r?$') {
                throw "Card placement must change only the size threshold, with a fixed cap."
            }
        }
        $oldAdapter = $env:COMET_ADAPTER_INDEX
        $oldMerge = $env:COMET_WIDE_MERGE
        $oldDisplay = $env:COMET_DISPLAY_MODE
        $oldHash = $env:COMET_PARALLEL_CARD_HASH
        try {
            $env:COMET_ADAPTER_INDEX = "9"
            $env:COMET_DISPLAY_MODE = "1280x720@60"
            $env:COMET_PARALLEL_CARD_HASH = "1"
            $env:COMET_WIDE_MERGE = "1"
            & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PSScriptRoot\play.ps1" -Game $missing -Cache (Join-Path $scratch "cache") -Data (Join-Path $scratch "data") -Exe $probe -AdapterIndex -1 -DisplayMode Default -CardHash Serial -CardMerge Serial -RunLabel $label
            if ($LASTEXITCODE -ne 7) { throw "Default adapter probe failed." }
            $report = Get-Content -LiteralPath $log -Raw
            if ($report -match 'adapter=9') { throw "Default adapter did not clear the inherited selection." }
            if ($report -match 'display=1280x720@60') { throw "Default display must clear the override." }
            if ($report -notmatch 'cardmerge=0') { throw "Serial card merge did not override inherited setting." }
            if ($report -notmatch 'hash=0') { throw "Serial hashing must explicitly clear the parallel setting." }
            & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PSScriptRoot\play.ps1" -Game $missing -Cache (Join-Path $scratch "cache") -Data (Join-Path $scratch "data") -Exe $probe -DisplayMode Inherit -CardHash Inherit -CardMerge Inherit -RunLabel $label
            if ($LASTEXITCODE -ne 7) { throw "Inherited display probe failed." }
            $report = Get-Content -LiteralPath $log -Raw
            if ($report -notmatch 'display=1280x720@60' -or $report -notmatch 'hash=1' -or $report -notmatch 'cardmerge=1') { throw "Inherit must preserve the environment." }
            foreach ($invalid in '1920x1080@0', '0x1080', '1920x1080@60Hz', '1920x1080@241') {
                $previous = $ErrorActionPreference
                $ErrorActionPreference = "Continue"
                try {
                    & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PSScriptRoot\play.ps1" -Game $missing -Cache (Join-Path $scratch "cache") -Data (Join-Path $scratch "data") -Exe $probe -DisplayMode $invalid -RunLabel $label 2>&1 | Out-Null
                    if ($LASTEXITCODE -eq 0 -or $LASTEXITCODE -eq 7) { throw "Invalid display mode was accepted: $invalid" }
                } finally {
                    $ErrorActionPreference = $previous
                }
            }
        } finally {
            if ($null -eq $oldAdapter) { Remove-Item Env:COMET_ADAPTER_INDEX -ErrorAction SilentlyContinue }
            else { $env:COMET_ADAPTER_INDEX = $oldAdapter }
            if ($null -eq $oldDisplay) { Remove-Item Env:COMET_DISPLAY_MODE -ErrorAction SilentlyContinue }
            else { $env:COMET_DISPLAY_MODE = $oldDisplay }
            if ($null -eq $oldHash) { Remove-Item Env:COMET_PARALLEL_CARD_HASH -ErrorAction SilentlyContinue }
            else { $env:COMET_PARALLEL_CARD_HASH = $oldHash }
            if ($null -eq $oldMerge) { Remove-Item Env:COMET_WIDE_MERGE -ErrorAction SilentlyContinue }
            else { $env:COMET_WIDE_MERGE = $oldMerge }
        }
    } finally {
        Remove-Item -LiteralPath $log -Force -ErrorAction SilentlyContinue
    }
    Write-Host "Launcher exit-code regression test passed."
} finally {
    Remove-Item -LiteralPath $scratch -Recurse -Force
}
