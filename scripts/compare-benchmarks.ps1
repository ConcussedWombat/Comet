# Compare the same measurement interval from two labeled benchmark logs.
param(
    [Parameter(Mandatory = $true)][string]$BaselineLog,
    [Parameter(Mandatory = $true)][string]$CandidateLog,
    [ValidateRange(0, 3600)][int]$SkipSeconds = 30,
    [ValidateRange(1, 3600)][int]$MeasureSeconds = 60
)
$ErrorActionPreference = "Stop"
$baseline = & "$PSScriptRoot\summarize-benchmark.ps1" -Log $BaselineLog -SkipSeconds $SkipSeconds -MeasureSeconds $MeasureSeconds
$candidate = & "$PSScriptRoot\summarize-benchmark.ps1" -Log $CandidateLog -SkipSeconds $SkipSeconds -MeasureSeconds $MeasureSeconds
if ([math]::Abs($baseline.Seconds - $candidate.Seconds) -gt ($MeasureSeconds * 0.1)) {
    Write-Warning "Measured durations differ by more than 10% of the requested interval; check that both runs cover the same scene."
}

$metrics = @(
    'AverageFps', 'MeanFrameMs', 'P50FrameMs', 'P95FrameMs', 'P99FrameMs', 'MaxFrameMs',
    'PrepareMsPerFrame', 'WaitMsPerFrame', 'ReadbackMsPerFrame',
    'UploadMBPerFrame', 'TextureMBPerFrame', 'ReadbackMBPerFrame', 'Seconds', 'Frames'
)
foreach ($metric in $metrics) {
    $before = $baseline.PSObject.Properties[$metric]
    $after = $candidate.PSObject.Properties[$metric]
    if ($null -eq $before -or $null -eq $after) { continue }
    $a = [double]$before.Value
    $b = [double]$after.Value
    $change = if ($a -ne 0) { [math]::Round(100 * ($b - $a) / $a, 2) } else { $null }
    [pscustomobject]@{
        Metric = $metric
        Baseline = $a
        Candidate = $b
        Delta = [math]::Round($b - $a, 2)
        DeltaPercent = $change
    }
}
