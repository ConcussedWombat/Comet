# Offline-only six-material-shader validation. No game launch or game assets
# copied into Git. Run with a Windows diagnostic Comet built from the reviewed
# patches (L7 evidence, I1/L5 combined, L7 packed index, X1 waterfall, B2 lanes).
# Do not interpret an uncompiled switch as a negative test; record build ID.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Comet,
    [Parameter(Mandatory=$true)][string]$ShaderDir,
    [Parameter(Mandatory=$true)][string]$BuildId,
    [Parameter(Mandatory=$true)][string]$OutDir,
    [ValidateSet('baseline','l5','packed','waterfall','lanes','all')]
    [string[]]$Cases = @('baseline','l5','packed','waterfall','lanes','all')
)
$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path -LiteralPath $Comet).Path
$root = (Resolve-Path -LiteralPath $ShaderDir).Path
$out = [System.IO.Path]::GetFullPath($OutDir)
New-Item -ItemType Directory -Path $out -Force | Out-Null
$hashes = @(
    '7d0f14ce44746d7e','6b6db9f1a526daeb','8660e9822ebe4b62',
    'abc6a80d46edc294','c8df7216c69cecbd','2fd01fca16993001'
)
$variants = @{
    baseline = @()
    l5 = @('COMET_L5_SRD','COMET_L5_PIXEL_SRD')
    packed = @('COMET_L5_SRD','COMET_L5_PIXEL_SRD','COMET_L7_SRD_INDEX')
    waterfall = @('COMET_X1_WATERFALL_SIZES')
    lanes = @('COMET_B2_PIXEL_LANES')
    all = @('COMET_L5_SRD','COMET_L5_PIXEL_SRD','COMET_L7_SRD_INDEX',
        'COMET_X1_WATERFALL_SIZES','COMET_B2_PIXEL_LANES')
}
$controlled = @('COMET_L5_SRD','COMET_L5_PIXEL_SRD','COMET_L7_SRD_INDEX',
    'COMET_X1_WATERFALL_SIZES','COMET_B2_PIXEL_LANES','COMET_L5_SRD_SNAPSHOT',
    'COMET_L7_SRD_PROVENANCE','COMET_L5_SRD_TRACE','COMET_TRANSLATE_ONLY',
    'COMET_HOST_MAX_WAVE','COMET_DUMP_MATCH','COMET_FAIL_DUMP','COMET_PAIRS')
# Other COMET switches are intentionally inherited and recorded, not changed:
# compare runs on the same compiled binary and environment.
$saved = @{}
foreach ($name in $controlled) {
    $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
$metadata = [ordered]@{
    build = $BuildId
    executable_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $exe).Hash
    cases = $Cases
    inherited_comet_switches = @(
        Get-ChildItem Env:COMET_* | Where-Object {
            $_.Name -notin $controlled -and $_.Value -match '^[01]$'
        } | ForEach-Object { "$($_.Name)=$($_.Value)" }
    )
}
$metadata | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $out 'matrix-build.json')
$rows = @()
try {
    foreach ($case in $Cases) {
        foreach ($name in $controlled) {
            [Environment]::SetEnvironmentVariable($name, $null, 'Process')
        }
        foreach ($name in $variants[$case]) {
            [Environment]::SetEnvironmentVariable($name, '1', 'Process')
        }
        $env:COMET_HOST_MAX_WAVE = '32'
        $env:COMET_L7_SRD_PROVENANCE = '1'
        $env:COMET_L5_SRD_TRACE = '1'
        foreach ($hash in $hashes) {
            $env:COMET_TRANSLATE_ONLY = $hash
            $stem = "$case-$hash"
            $stdout = Join-Path $out "$stem.stdout.txt"
            $stderr = Join-Path $out "$stem.stderr.txt"
            # Separate streams preserve validator status and avoid PowerShell
            # treating native stderr diagnostics as terminating PS errors.
            & $exe translate --validate --list --jobs 1 $root 1> $stdout 2> $stderr
            $exit = $LASTEXITCODE
            $lines = @(Get-Content -LiteralPath $stdout)
            $diag = @(Get-Content -LiteralPath $stderr)
            $named = @($lines | Where-Object { $_ -match "#$hash\b.*\bPs\b" })
            $validated = @($named | Where-Object { $_ -match '\bvalidated\s*$' })
            $notValidated = @($named | Where-Object {
                $_ -notmatch '\bvalidated\s*$' -and $_ -notmatch '\bduplicate\s*$'
            })
            $status = if ($exit -ne 0) { 'process-failed' }
                elseif ($validated.Count -gt 0 -and $notValidated.Count -eq 0) { 'validated' }
                elseif ($validated.Count -gt 0) { 'partial' }
                elseif ($named.Count -gt 0) { 'not-validated' }
                else { 'not-found-or-no-PS-status' }
            $trace = @($diag | Where-Object {
                $_ -match 'L7-SRD-(SITE|DEF|USE)|L7-SRD packed-index|L5-SRD|L5-PIXEL-SRD'
            })
            $rows += [pscustomobject]@{
                Build = $BuildId; Case = $case; Hash = $hash; Status = $status
                ValidatedPS = $validated.Count; OtherPS = $notValidated.Count
                Exit = $exit; TraceLines = $trace.Count
                Switches = ($variants[$case] -join ',')
            }
            $rows | Export-Csv -LiteralPath (Join-Path $out 'matrix.csv') -NoTypeInformation
            Write-Host "$case $hash $status (exit $exit, trace $($trace.Count))"
        }
    }
} finally {
    foreach ($name in $controlled) {
        [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process')
    }
}
# Review the exact per-hash stdout and stderr, not just this matrix. No
# validated status means actual GPU draw, albedo or geometry was recovered.
$rows | Format-Table Case,Hash,Status,ValidatedPS,OtherPS,TraceLines -AutoSize
