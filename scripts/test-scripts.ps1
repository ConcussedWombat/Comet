# Syntax-check every PowerShell script without loading Windows Forms or running the title.
$ErrorActionPreference = "Stop"
Get-ChildItem -LiteralPath $PSScriptRoot -Filter "*.ps1" -File | ForEach-Object {
    $tokens = $null
    $errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile($_.FullName, [ref]$tokens, [ref]$errors)
    if ($errors.Count -gt 0) { throw ("Invalid PowerShell in " + $_.Name + ": " + ($errors -join "; ")) }
}
Write-Host "PowerShell syntax checks passed."
