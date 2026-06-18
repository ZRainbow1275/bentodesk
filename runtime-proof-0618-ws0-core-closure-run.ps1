$ErrorActionPreference = 'Stop'

$root = 'D:\Desktop\CREATOR FOUR'
$nano = Join-Path $root 'bentodesk-nano'
$sourceExe = Join-Path $nano 'target\x86_64-pc-windows-msvc\debug\bento-nano-shell.exe'
$proofDir = Join-Path $nano 'runtime-proof-0618-ws0-core-closure-try'
$stateDir = Join-Path $nano 'runtime-proof-0618-ws0-core-closure-state'
$proofExe = Join-Path $proofDir 'bento-nano-shell-ws0-core-proof.exe'
$summaryPath = Join-Path $proofDir 'summary.json'
$f3TemplatePath = Join-Path $nano 'runtime-proof-0505-ghost-explorer-icon-click-20260519-try\run-ghost-explorer-icon-click-proof.ps1'
$f3ScriptPath = Join-Path $proofDir 'generated-f3-current-click-through.ps1'
$f3StateDir = Join-Path $nano 'runtime-proof-0618-ws0-core-f3-click-state'
$f3ProofDir = Join-Path $proofDir 'f3-click-through'
$f3SummaryPath = Join-Path $f3ProofDir 'summary.json'
$ws2ScriptPath = Join-Path $nano 'runtime-proof-0618-ws2-appearance-closure-run.ps1'
$ws2SummaryPath = Join-Path $nano 'runtime-proof-0618-ws2-appearance-closure-try\summary.json'
$ws2StateDir = Join-Path $nano 'runtime-proof-0618-ws2-appearance-closure-state'
$a3SummaryPath = Join-Path $nano 'runtime-proof-0618-ws0-a3-auto-rebound-try\summary.json'
$r2RestoreDir = Join-Path $proofDir 'r2-restore-second-launch'
$r2RestoreStdout = Join-Path $r2RestoreDir 'stdout.log'
$r2RestoreStderr = Join-Path $r2RestoreDir 'stderr.log'

function Write-Utf8NoBom([string]$path, [string]$content) {
  $encoding = [System.Text.UTF8Encoding]::new($false)
  [System.IO.File]::WriteAllText($path, $content, $encoding)
}

function Remove-DirInsideNano([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) { return }
  $resolvedNano = (Resolve-Path -LiteralPath $nano).Path
  $resolvedPath = (Resolve-Path -LiteralPath $path).Path
  if (-not $resolvedPath.StartsWith($resolvedNano, [StringComparison]::OrdinalIgnoreCase)) {
    throw "refusing to remove path outside repo: $resolvedPath"
  }
  Remove-Item -LiteralPath $path -Recurse -Force
}

function ConvertTo-ProcessArgument([string]$value) {
  if ($value -notmatch '[\s"]') { return $value }
  return '"' + ($value -replace '"', '\"') + '"'
}

function Invoke-WorkspaceCargo {
  param(
    [Parameter(Mandatory=$true)][string[]]$Arguments,
    [Parameter(Mandatory=$true)][string]$OutputPath
  )

  $psi = [Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = 'cargo.exe'
  $psi.WorkingDirectory = $root
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.Environment['CARGO_BUILD_JOBS'] = '1'
  $psi.Environment['CARGO_INCREMENTAL'] = '0'
  $psi.Arguments = (($Arguments | ForEach-Object { ConvertTo-ProcessArgument $_ }) -join ' ')
  $p = [Diagnostics.Process]::Start($psi)
  $stdoutTask = $p.StandardOutput.ReadToEndAsync()
  $stderrTask = $p.StandardError.ReadToEndAsync()
  $p.WaitForExit()
  $combined = @()
  if (-not [string]::IsNullOrWhiteSpace($stderrTask.Result)) { $combined += $stderrTask.Result.TrimEnd() }
  if (-not [string]::IsNullOrWhiteSpace($stdoutTask.Result)) { $combined += $stdoutTask.Result.TrimEnd() }
  Write-Utf8NoBom $OutputPath (($combined -join [Environment]::NewLine) + [Environment]::NewLine)
  if ($p.ExitCode -ne 0) {
    throw "cargo $($Arguments -join ' ') failed with exit code $($p.ExitCode); see $OutputPath"
  }
}

function Read-JsonFile([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) { throw "json not found: $path" }
  return [System.IO.File]::ReadAllText($path, [System.Text.Encoding]::UTF8).TrimStart([char]0xFEFF) | ConvertFrom-Json
}

function Read-TextFile([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) { return '' }
  $fs = [System.IO.File]::Open($path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
  $reader = $null
  try {
    $reader = [System.IO.StreamReader]::new($fs, [System.Text.Encoding]::UTF8, $true)
    return $reader.ReadToEnd()
  } finally {
    if ($null -ne $reader) { $reader.Dispose() } else { $fs.Dispose() }
  }
}

function Quote-PowerShellLiteral([string]$value) {
  return "'" + ($value -replace "'", "''") + "'"
}

function New-F3CurrentScript {
  if (-not (Test-Path -LiteralPath $f3TemplatePath)) {
    throw "F3 template not found: $f3TemplatePath"
  }
  $template = [System.IO.File]::ReadAllText($f3TemplatePath, [System.Text.Encoding]::UTF8).TrimStart([char]0xFEFF)
  $template = $template.Replace(
    '$exe = Join-Path $nano ''target\x86_64-pc-windows-msvc\release\bento-nano-shell.exe''',
    ('$exe = ' + (Quote-PowerShellLiteral $proofExe))
  )
  $template = $template.Replace(
    '$stateDir = Join-Path $nano ''runtime-proof-0505-ghost-explorer-icon-click-state''',
    ('$stateDir = ' + (Quote-PowerShellLiteral $f3StateDir))
  )
  $template = $template.Replace(
    '$proofDir = Join-Path $nano ''runtime-proof-0505-ghost-explorer-icon-click-20260519-try''',
    ('$proofDir = ' + (Quote-PowerShellLiteral $f3ProofDir))
  )
  $template = $template.Replace(
    @'
  $scaleX = [Math]::Max(1.0, ([double]$main.rect.width / 480.0))
  $scaleY = [Math]::Max(1.0, ([double]$main.rect.height / 320.0))
'@,
    @'
  $scaleX = [Math]::Max(1.0, ([double]$main.rect.width / 1707.0))
  $scaleY = [Math]::Max(1.0, ([double]$main.rect.height / 912.0))
'@
  )
  Write-Utf8NoBom $f3ScriptPath $template
}

function Invoke-PowerShellScript([string]$scriptPath, [string]$stdoutPath) {
  $psi = [Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = 'powershell.exe'
  $psi.WorkingDirectory = $nano
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.Arguments = "-ExecutionPolicy Bypass -File " + (ConvertTo-ProcessArgument $scriptPath)
  $p = [Diagnostics.Process]::Start($psi)
  $stdoutTask = $p.StandardOutput.ReadToEndAsync()
  $stderrTask = $p.StandardError.ReadToEndAsync()
  $p.WaitForExit()
  $combined = @()
  if (-not [string]::IsNullOrWhiteSpace($stderrTask.Result)) { $combined += $stderrTask.Result.TrimEnd() }
  if (-not [string]::IsNullOrWhiteSpace($stdoutTask.Result)) { $combined += $stdoutTask.Result.TrimEnd() }
  Write-Utf8NoBom $stdoutPath (($combined -join [Environment]::NewLine) + [Environment]::NewLine)
  if ($p.ExitCode -ne 0) {
    throw "$scriptPath failed with exit code $($p.ExitCode); see $stdoutPath"
  }
}

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class NativeProof0618Ws0Core {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam);
  public const uint WM_HOTKEY = 0x0312;
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@

function Get-WindowsForPid([int]$processId) {
  $items = New-Object System.Collections.ArrayList
  $cb = [NativeProof0618Ws0Core+EnumWindowsProc]{
    param([IntPtr]$hwnd, [IntPtr]$lparam)
    [uint32]$wpid = 0
    [void][NativeProof0618Ws0Core]::GetWindowThreadProcessId($hwnd, [ref]$wpid)
    if ($wpid -eq [uint32]$processId) {
      $class = [System.Text.StringBuilder]::new(256)
      $title = [System.Text.StringBuilder]::new(256)
      [void][NativeProof0618Ws0Core]::GetClassName($hwnd, $class, $class.Capacity)
      [void][NativeProof0618Ws0Core]::GetWindowText($hwnd, $title, $title.Capacity)
      $rect = New-Object NativeProof0618Ws0Core+RECT
      [void][NativeProof0618Ws0Core]::GetWindowRect($hwnd, [ref]$rect)
      [void]$items.Add([pscustomobject]@{
        hwnd = $hwnd.ToInt64()
        class = $class.ToString()
        title = $title.ToString()
        visible = [NativeProof0618Ws0Core]::IsWindowVisible($hwnd)
        rect = [ordered]@{
          left = $rect.Left
          top = $rect.Top
          right = $rect.Right
          bottom = $rect.Bottom
          width = ($rect.Right - $rect.Left)
          height = ($rect.Bottom - $rect.Top)
        }
      })
    }
    return $true
  }
  [void][NativeProof0618Ws0Core]::EnumWindows($cb, [IntPtr]::Zero)
  return @($items.ToArray())
}

function Wait-Window([int]$processId, [string]$class, [int]$timeoutMs = 10000) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $timeoutMs) {
    $win = Get-WindowsForPid $processId | Where-Object { $_.class -eq $class -and $_.visible } | Select-Object -First 1
    if ($win) { return $win }
    Start-Sleep -Milliseconds 120
  }
  return $null
}

function Wait-Condition([scriptblock]$predicate, [int]$timeoutMs = 5000) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $timeoutMs) {
    $value = & $predicate
    if ($value) { return $value }
    Start-Sleep -Milliseconds 120
  }
  return $null
}

function Invoke-R2RestoreSecondLaunch {
  if (-not (Test-Path -LiteralPath (Join-Path $ws2StateDir 'vault.bin'))) {
    throw "WS2 vault not found for R2 restore proof: $ws2StateDir"
  }

  New-Item -ItemType Directory -Force -Path $r2RestoreDir | Out-Null
  Remove-Item -LiteralPath $r2RestoreStdout,$r2RestoreStderr -Force -ErrorAction SilentlyContinue

  $previousStateDir = $env:BENTODESK_NANO_STATE_DIR
  $env:BENTODESK_NANO_STATE_DIR = $ws2StateDir
  try {
    $proc = Start-Process -FilePath $proofExe -WorkingDirectory $r2RestoreDir -RedirectStandardOutput $r2RestoreStdout -RedirectStandardError $r2RestoreStderr -PassThru
  } finally {
    if ($null -eq $previousStateDir) {
      Remove-Item Env:\BENTODESK_NANO_STATE_DIR -ErrorAction SilentlyContinue
    } else {
      $env:BENTODESK_NANO_STATE_DIR = $previousStateDir
    }
  }

  $main = $null
  $exited = $false
  try {
    $main = Wait-Window $proc.Id 'BentoNanoShell' 10000
    if (-not $main) { throw 'R2 restore second launch main window not found' }
    Start-Sleep -Milliseconds 1800
    [void][NativeProof0618Ws0Core]::PostMessageW([IntPtr]([int64]$main.hwnd), [NativeProof0618Ws0Core]::WM_HOTKEY, [UIntPtr]([uint64]16973), [IntPtr]::Zero)
    $exited = [bool](Wait-Condition { $proc.HasExited } 7000)
    if (-not $exited) { throw 'R2 restore second launch did not exit through quit hotkey' }
    $proc.WaitForExit(3000) | Out-Null
  } finally {
    if ($proc -and -not $proc.HasExited) {
      Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
      $proc.WaitForExit(3000) | Out-Null
    }
  }

  $stderr = Read-TextFile $r2RestoreStderr
  $stdout = Read-TextFile $r2RestoreStdout
  return [ordered]@{
    state_dir = $ws2StateDir
    vault = Join-Path $ws2StateDir 'vault.bin'
    main_window = $main
    stderr = $r2RestoreStderr
    stdout = $r2RestoreStdout
    restore_log_seen = ($stderr -match 'zone_display_mode restored: click')
    restore_error_seen = ($stderr -match 'zone_display_mode restore skipped')
    process_exited_after_quit_hotkey = $exited
    stderr_excerpt = (($stderr -split "`r?`n") | Where-Object { $_ -match 'zone_display_mode|startup:|hotkey:' })
    stdout_len = $stdout.Length
  }
}

function Test-F3Accepted($f3) {
  return (
    $f3.status -eq 'ok' -and
    $f3.stage -eq 'completed' -and
    $f3.physical_double_click_opened_file -eq $true -and
    $f3.process_exited_after_quit_hotkey -eq $true -and
    $f3.obstruction_clearance.desktop_shell_reached -eq $true -and
    $f3.obstruction_clearance.main_reports_httransparent -eq $true -and
    $f3.probe_before_click.main_ws_ex_transparent -eq $true -and
    $f3.probe_before_click.main_nchittest_is_transparent -eq $true -and
    $f3.probe_before_click.window_from_point_is_main -eq $false
  )
}

function Test-Ws2R2Accepted($ws2) {
  return (
    $ws2.status -eq 'ok' -and
    $ws2.stage -eq 'completed' -and
    $ws2.no_mock_data -eq $true -and
    $ws2.assertions.accepted -eq $true -and
    $ws2.assertions.zone_display_hover_hit -eq $true -and
    $ws2.assertions.zone_display_always_hit -eq $true -and
    $ws2.assertions.zone_display_click_hit -eq $true -and
    $ws2.assertions.zone_display_mode_persisted -eq $true -and
    $ws2.vault.after_save.settings.zone_display_mode -eq 'click' -and
    $ws2.assertions.process_exited_after_quit_hotkey -eq $true
  )
}

function Test-A3Accepted($a3) {
  return (
    $a3.status -eq 'ok' -and
    $a3.stage -eq 'completed' -and
    $a3.a3_auto_rebound.accepted -eq $true -and
    $a3.process_exited_after_quit_hotkey -eq $true
  )
}

$stage = 'started'
$summary = [ordered]@{
  status = 'failed'
  stage = $stage
  ws_id = 'WS-0'
  no_mock_data = $true
  generated_at_utc = [DateTime]::UtcNow.ToString('o')
  proof_dir = $proofDir
  state_dir = $stateDir
  source_exe = $sourceExe
  exe = $proofExe
}

try {
  New-Item -ItemType Directory -Force -Path $proofDir | Out-Null
  Remove-DirInsideNano $stateDir
  Remove-DirInsideNano $f3StateDir
  New-Item -ItemType Directory -Force -Path $stateDir | Out-Null

  $stage = 'build-debug-shell'
  $summary.stage = $stage
  Invoke-WorkspaceCargo -Arguments @(
    'build', '--manifest-path', 'bentodesk-nano/Cargo.toml',
    '-p', 'bento-nano-shell',
    '--target', 'x86_64-pc-windows-msvc'
  ) -OutputPath (Join-Path $proofDir '00-cargo-build-debug-shell.txt')
  if (-not (Test-Path -LiteralPath $sourceExe)) { throw "source exe not found after build: $sourceExe" }
  Copy-Item -LiteralPath $sourceExe -Destination $proofExe -Force

  $stage = 'f3-current-click-through'
  $summary.stage = $stage
  New-F3CurrentScript
  Invoke-PowerShellScript $f3ScriptPath (Join-Path $proofDir '01-f3-current-click-through-output.txt')
  $f3 = Read-JsonFile $f3SummaryPath

  $stage = 'r2-refresh-current-settings-proof'
  $summary.stage = $stage
  Invoke-PowerShellScript $ws2ScriptPath (Join-Path $proofDir '02-ws2-r2-refresh-output.txt')
  $ws2 = Read-JsonFile $ws2SummaryPath

  $stage = 'r2-second-launch-restore'
  $summary.stage = $stage
  $r2Restore = Invoke-R2RestoreSecondLaunch

  $stage = 'a3-existing-runtime-proof'
  $summary.stage = $stage
  $a3 = Read-JsonFile $a3SummaryPath

  $f3Accepted = Test-F3Accepted $f3
  $r2Accepted = Test-Ws2R2Accepted $ws2
  $r2RestoreAccepted = (
    $r2Restore.restore_log_seen -eq $true -and
    $r2Restore.restore_error_seen -eq $false -and
    $r2Restore.process_exited_after_quit_hotkey -eq $true
  )
  $a3Accepted = Test-A3Accepted $a3
  $accepted = ($f3Accepted -and $r2Accepted -and $r2RestoreAccepted -and $a3Accepted)

  $summary.stage = if ($accepted) { 'completed' } else { 'failed-assertions' }
  $summary.status = if ($accepted) { 'ok' } else { 'failed' }
  $summary.ws0_core = [ordered]@{
    accepted = $accepted
    f3_click_through_current = $f3Accepted
    r2_picker_select_persist = $r2Accepted
    r2_second_launch_restore = $r2RestoreAccepted
    a3_auto_rebound = $a3Accepted
    m0_5_decision = if ($accepted) { 'not_needed' } else { 'unresolved' }
  }
  $summary.f3 = [ordered]@{
    summary = $f3SummaryPath
    status = $f3.status
    stage = $f3.stage
    desktop_shell_reached = $f3.obstruction_clearance.desktop_shell_reached
    main_reports_httransparent = $f3.obstruction_clearance.main_reports_httransparent
    main_ws_ex_transparent = $f3.probe_before_click.main_ws_ex_transparent
    main_nchittest_is_transparent = $f3.probe_before_click.main_nchittest_is_transparent
    window_from_point_class = $f3.probe_before_click.window_from_point.class
    physical_double_click_opened_file = $f3.physical_double_click_opened_file
    opened_process = $f3.opened_process
    process_exited_after_quit_hotkey = $f3.process_exited_after_quit_hotkey
    minimized_windows = $f3.obstruction_clearance.minimized_windows
  }
  $summary.r2 = [ordered]@{
    ws2_summary = $ws2SummaryPath
    status = $ws2.status
    stage = $ws2.stage
    settings_window_class = $ws2.settings_window.class
    opened_via_hotkey_id = $ws2.opened_via_hotkey_id
    quit_via_hotkey_id = $ws2.quit_via_hotkey_id
    hover_hit = $ws2.assertions.zone_display_hover_hit
    always_hit = $ws2.assertions.zone_display_always_hit
    click_hit = $ws2.assertions.zone_display_click_hit
    persisted_after_each_click = $ws2.zone_display_mode.persisted_after_each_click
    final_vault_zone_display_mode = $ws2.vault.after_save.settings.zone_display_mode
    second_launch_restore = $r2Restore
  }
  $summary.a3 = [ordered]@{
    summary = $a3SummaryPath
    status = $a3.status
    stage = $a3.stage
    accepted = $a3.a3_auto_rebound.accepted
    collapse_after_leave_ms = $a3.a3_auto_rebound.collapse_after_leave_ms
    settled_after_leave_ms = $a3.a3_auto_rebound.settled_after_leave_ms
    process_exited_after_quit_hotkey = $a3.process_exited_after_quit_hotkey
  }
  $summary.external_references = @(
    [ordered]@{
      topic = 'WindowFromPoint and WM_NCHITTEST proof design'
      url = 'https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-nchittest'
    },
    [ordered]@{
      topic = 'Layered window hit testing and WS_EX_TRANSPARENT'
      url = 'https://learn.microsoft.com/en-us/previous-versions/ms997507(v=msdn.10)'
    }
  )

  Write-Utf8NoBom $summaryPath ($summary | ConvertTo-Json -Depth 18)
  if (-not $accepted) {
    throw 'WS-0 core assertions failed; inspect summary.json'
  }
} catch {
  $summary.stage = $stage
  $summary.status = 'failed'
  $summary.error = $_.Exception.Message
  Write-Utf8NoBom $summaryPath ($summary | ConvertTo-Json -Depth 18)
  throw
}

Write-Output "ws0_core_closure_status=$($summary.status)"
Write-Output "summary=$summaryPath"
Write-Output "accepted=$($summary.ws0_core.accepted)"
