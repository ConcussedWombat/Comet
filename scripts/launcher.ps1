# A Windows desktop launcher for Comet. Uses the same play script as the CLI.
param([string]$Exe = "$PSScriptRoot\..\target\release\comet.exe")
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()

$form = New-Object System.Windows.Forms.Form
$form.Text = "Comet"
$form.ClientSize = New-Object System.Drawing.Size(560, 428)
$form.FormBorderStyle = "FixedDialog"
$form.MaximizeBox = $false
$form.StartPosition = "CenterScreen"

function Add-Label([string]$Text, [int]$X, [int]$Y) {
    $label = New-Object System.Windows.Forms.Label
    $label.Text = $Text
    $label.AutoSize = $true
    $label.Location = New-Object System.Drawing.Point($X, $Y)
    $form.Controls.Add($label)
}
function Add-TextBox([string]$Value, [int]$Y) {
    $box = New-Object System.Windows.Forms.TextBox
    $box.Text = $Value
    $box.Location = New-Object System.Drawing.Point(18, $Y)
    $box.Width = 432
    $form.Controls.Add($box)
    return $box
}
function Add-Button([string]$Text, [int]$X, [int]$Y, [int]$Width) {
    $button = New-Object System.Windows.Forms.Button
    $button.Text = $Text
    $button.Location = New-Object System.Drawing.Point($X, $Y)
    $button.Width = $Width
    $form.Controls.Add($button)
    return $button
}
# Quote Windows command-line arguments without using the shell.
function Quote-Argument([string]$Value) {
    return '"' + [regex]::Replace([regex]::Replace($Value, '(\\*)"', '$1$1\"'), '(\\+)$', '$1$1') + '"'
}

Add-Label "Game executable (your own copy)" 18 15
$game = Add-TextBox "" 35
$browse = Add-Button "Browse..." 460 33 82
$browse.Add_Click({
    $dialog = New-Object System.Windows.Forms.OpenFileDialog
    try {
        $dialog.Filter = "Executables (*.exe)|*.exe|All files (*.*)|*.*"
        if ($dialog.ShowDialog() -eq "OK") { $game.Text = $dialog.FileName }
    } finally { $dialog.Dispose() }
})
Add-Label "Shader cache" 18 66
$cache = Add-TextBox "$env:LOCALAPPDATA\comet\cache" 86
Add-Label "Save data" 18 117
$data = Add-TextBox "$env:LOCALAPPDATA\comet\data" 137

Add-Label "GPU profile" 18 169
$profile = New-Object System.Windows.Forms.ComboBox
$profile.Location = New-Object System.Drawing.Point(18, 189)
$profile.Width = 180
$profile.DropDownStyle = "DropDownList"
[void]$profile.Items.AddRange([object[]]@("Inherit", "Baseline", "Relay", "LateWait"))
$profile.SelectedIndex = 0
$form.Controls.Add($profile)
$benchmark = New-Object System.Windows.Forms.CheckBox
$benchmark.Text = "Record benchmark samples"
$benchmark.AutoSize = $true
$benchmark.Location = New-Object System.Drawing.Point(220, 190)
$form.Controls.Add($benchmark)

Add-Label "DXGI adapter (-1 = Windows default)" 18 221
$adapter = New-Object System.Windows.Forms.NumericUpDown
$adapter.Location = New-Object System.Drawing.Point(18, 241)
$adapter.Width = 100
$adapter.Minimum = -1
$adapter.Maximum = [int]::MaxValue
$adapter.Value = -1
$form.Controls.Add($adapter)
$doctor = Add-Button "Check hardware" 140 239 130
$doctor.Add_Click({
    $proc = $null
    try {
        if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) { throw "Build Comet first: cargo build --release -p comet" }
        $info = New-Object System.Diagnostics.ProcessStartInfo
        $info.FileName = (Resolve-Path -LiteralPath $Exe).Path
        $info.Arguments = "doctor"
        $info.UseShellExecute = $false
        $info.CreateNoWindow = $true
        $info.RedirectStandardOutput = $true
        $info.RedirectStandardError = $true
        if ($adapter.Value -lt 0) {
            $info.EnvironmentVariables.Remove("COMET_ADAPTER_INDEX")
        } else {
            $info.EnvironmentVariables["COMET_ADAPTER_INDEX"] = ([int]$adapter.Value).ToString([Globalization.CultureInfo]::InvariantCulture)
        }
        $proc = [System.Diagnostics.Process]::Start($info)
        $stdout = $proc.StandardOutput.ReadToEndAsync()
        $stderr = $proc.StandardError.ReadToEndAsync()
        if (-not $proc.WaitForExit(15000)) {
            $proc.Kill()
            throw "Hardware check timed out."
        }
        $report = $stdout.GetAwaiter().GetResult() + [Environment]::NewLine + $stderr.GetAwaiter().GetResult()
        [void][System.Windows.Forms.MessageBox]::Show($report, "Comet hardware check (exit $($proc.ExitCode))")
    } catch {
        [void][System.Windows.Forms.MessageBox]::Show($_.Exception.Message, "Comet hardware check")
    } finally {
        if ($null -ne $proc) { $proc.Dispose() }
    }
})

Add-Label "Virtual display (title hint)" 18 276
$display = New-Object System.Windows.Forms.ComboBox
$display.Location = New-Object System.Drawing.Point(18, 296)
$display.Width = 180
$display.DropDownStyle = "DropDownList"
[void]$display.Items.AddRange([object[]]@("Inherit", "Default", "1280x720@60", "1920x1080@60", "2560x1440@60", "3840x2160@60", "1920x1080@120"))
$display.SelectedIndex = 0
$form.Controls.Add($display)
Add-Label "GPU memory hash (experimental)" 220 276
$hash = New-Object System.Windows.Forms.ComboBox
$hash.Location = New-Object System.Drawing.Point(220, 296)
$hash.Width = 180
$hash.DropDownStyle = "DropDownList"
[void]$hash.Items.AddRange([object[]]@("Inherit", "Serial", "Parallel"))
$hash.SelectedIndex = 0
$form.Controls.Add($hash)

$status = New-Object System.Windows.Forms.Label
$status.Text = "Ready. Experimental profiles may be unstable."
$status.Location = New-Object System.Drawing.Point(18, 340)
$status.Size = New-Object System.Drawing.Size(520, 32)
$form.Controls.Add($status)
$launch = Add-Button "Play" 18 379 110
$logs = Add-Button "Open log folder" 140 379 135
$logs.Add_Click({ Start-Process explorer.exe -ArgumentList (Quote-Argument ([IO.Path]::GetTempPath())) })
$script:child = $null
$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 500
$timer.Add_Tick({
    if ($null -ne $script:child -and $script:child.HasExited) {
        $status.Text = "Comet exited with code $($script:child.ExitCode). See comet.log."
        $script:child.Dispose()
        $script:child = $null
        $launch.Enabled = $true
        $timer.Stop()
    }
})
$launcher = Join-Path $PSScriptRoot "play.ps1"
$launch.Add_Click({
    try {
        if (-not (Test-Path -LiteralPath $game.Text -PathType Leaf)) { throw "Choose a valid game executable." }
        if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) { throw "Build Comet first: cargo build --release -p comet" }
        if ([string]::IsNullOrWhiteSpace($cache.Text) -or [string]::IsNullOrWhiteSpace($data.Text)) {
            throw "Choose cache and save directories."
        }
        $args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $launcher,
            "-Game", $game.Text, "-Cache", $cache.Text, "-Data", $data.Text,
            "-Exe", $Exe, "-GpuProfile", $profile.SelectedItem.ToString(),
            "-AdapterIndex", ([int]$adapter.Value).ToString([Globalization.CultureInfo]::InvariantCulture),
            "-DisplayMode", $display.SelectedItem.ToString(), "-CardHash", $hash.SelectedItem.ToString())
        if ($benchmark.Checked) { $args += "-Benchmark" }
        $info = New-Object System.Diagnostics.ProcessStartInfo
        $info.FileName = "powershell.exe"
        $info.Arguments = ($args | ForEach-Object { Quote-Argument $_ }) -join " "
        $info.UseShellExecute = $false
        $info.CreateNoWindow = $true
        $script:child = [System.Diagnostics.Process]::Start($info)
        $launch.Enabled = $false
        $status.Text = "Comet is running. Log: $env:TEMP\comet.log"
        $timer.Start()
    } catch { $status.Text = $_.Exception.Message }
})
$form.Add_FormClosed({ $timer.Stop(); $timer.Dispose(); if ($null -ne $script:child) { $script:child.Dispose() } })
[void]$form.ShowDialog()
$form.Dispose()
