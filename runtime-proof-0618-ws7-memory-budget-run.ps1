$ErrorActionPreference = 'Stop'

$root = 'D:\Desktop\CREATOR FOUR'
$nano = Join-Path $root 'bentodesk-nano'
$targetTriple = 'x86_64-pc-windows-msvc'
$exe = Join-Path $nano "target\$targetTriple\release\bento-nano-shell.exe"
$proofDir = Join-Path $nano 'runtime-proof-0618-ws7-memory-budget-try'
$stateDir = Join-Path $nano 'runtime-proof-0618-ws7-memory-budget-state'
$itemRoot = Join-Path $stateDir 'items'
$summaryPath = Join-Path $proofDir 'summary.json'
$stdoutPath = Join-Path $proofDir 'bento-nano-shell-stdout.log'
$stderrPath = Join-Path $proofDir 'bento-nano-shell-stderr.log'

function Write-Utf8NoBom([string]$path, [string]$content) {
  $encoding = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($path, $content, $encoding)
}

function Write-Json($value, [string]$name) {
  Write-Utf8NoBom (Join-Path $proofDir $name) ($value | ConvertTo-Json -Depth 16)
}

New-Item -ItemType Directory -Force -Path $proofDir | Out-Null
Get-ChildItem -LiteralPath $proofDir -File -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -ne 'runtime-proof-0618-ws7-memory-budget-run.ps1' } |
  Remove-Item -Force -ErrorAction SilentlyContinue

$resolvedNano = [IO.Path]::GetFullPath($nano)
foreach ($target in @($stateDir, $itemRoot, $proofDir)) {
  $resolvedTarget = [IO.Path]::GetFullPath($target)
  if (-not $resolvedTarget.StartsWith($resolvedNano, [StringComparison]::OrdinalIgnoreCase)) {
    throw "refusing cleanup outside nano workspace: $resolvedTarget"
  }
}
if (Test-Path -LiteralPath $stateDir) {
  Remove-Item -LiteralPath $stateDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $stateDir | Out-Null

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class NativeWs7MemoryProof {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdcBlt, uint nFlags);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr dpiContext);
  public const uint WM_HOTKEY = 0x0312;
  public const uint PW_RENDERFULLCONTENT = 0x00000002;
  public static readonly IntPtr DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = new IntPtr(-4);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@

[void][NativeWs7MemoryProof]::SetThreadDpiAwarenessContext([NativeWs7MemoryProof]::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)

function Get-WindowsForPid([int]$processId) {
  $items = New-Object System.Collections.ArrayList
  $callback = [NativeWs7MemoryProof+EnumWindowsProc]{
    param([IntPtr]$hwnd, [IntPtr]$lparam)
    [uint32]$windowPid = 0
    [void][NativeWs7MemoryProof]::GetWindowThreadProcessId($hwnd, [ref]$windowPid)
    if ($windowPid -eq [uint32]$processId) {
      $class = New-Object System.Text.StringBuilder 256
      $title = New-Object System.Text.StringBuilder 256
      [void][NativeWs7MemoryProof]::GetClassName($hwnd, $class, $class.Capacity)
      [void][NativeWs7MemoryProof]::GetWindowText($hwnd, $title, $title.Capacity)
      $rect = New-Object NativeWs7MemoryProof+RECT
      [void][NativeWs7MemoryProof]::GetWindowRect($hwnd, [ref]$rect)
      [void]$items.Add([pscustomobject]@{
        hwnd = $hwnd.ToInt64()
        class = $class.ToString()
        title = $title.ToString()
        visible = [NativeWs7MemoryProof]::IsWindowVisible($hwnd)
        rect = [pscustomobject]@{
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
  [void][NativeWs7MemoryProof]::EnumWindows($callback, [IntPtr]::Zero)
  return @($items.ToArray())
}

function Wait-Window([int]$processId, [string]$class, [int]$timeoutMs = 15000) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $timeoutMs) {
    $win = Get-WindowsForPid $processId |
      Where-Object { $_.class -eq $class -and $_.visible } |
      Select-Object -First 1
    if ($win) { return $win }
    Start-Sleep -Milliseconds 150
  }
  return $null
}

function Save-WindowShot($window, [string]$name) {
  if (-not $window) { return $false }
  $path = Join-Path $proofDir $name
  $rect = New-Object NativeWs7MemoryProof+RECT
  [void][NativeWs7MemoryProof]::GetWindowRect([IntPtr]$window.hwnd, [ref]$rect)
  $width = [Math]::Max(1, [int]($rect.Right - $rect.Left))
  $height = [Math]::Max(1, [int]($rect.Bottom - $rect.Top))
  $bitmap = New-Object System.Drawing.Bitmap $width, $height
  $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
  try {
    $hdc = $graphics.GetHdc()
    try {
      $printed = [NativeWs7MemoryProof]::PrintWindow([IntPtr]$window.hwnd, $hdc, [NativeWs7MemoryProof]::PW_RENDERFULLCONTENT)
    } finally {
      $graphics.ReleaseHdc($hdc)
    }
    if (-not $printed) {
      $graphics.CopyFromScreen([int]$rect.Left, [int]$rect.Top, 0, 0, [System.Drawing.Size]::new($width, $height))
    }
    $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    return $true
  } finally {
    $graphics.Dispose()
    $bitmap.Dispose()
  }
}

function Get-ProcessSample([Diagnostics.Process]$process, [string]$name) {
  $process.Refresh()
  return [ordered]@{
    label = $name
    sample = $name
    pid = $process.Id
    path = $process.Path
    responding = $process.Responding
    private_bytes = [int64]$process.PrivateMemorySize64
    private_mb = [Math]::Round($process.PrivateMemorySize64 / 1MB, 2)
    working_set = [int64]$process.WorkingSet64
    working_set_mb = [Math]::Round($process.WorkingSet64 / 1MB, 2)
    utc = [DateTime]::UtcNow.ToString('o')
  }
}

function Run-CargoCommand([string[]]$arguments, [string]$logName, [hashtable]$extraEnv = $null) {
  $logPath = Join-Path $proofDir $logName
  $stderrLogPath = Join-Path $proofDir ([IO.Path]::GetFileNameWithoutExtension($logName) + '.stderr.log')
  $previous = @{}
  $previousErrorAction = $ErrorActionPreference
  if ($extraEnv) {
    foreach ($key in $extraEnv.Keys) {
      $previous[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
      [Environment]::SetEnvironmentVariable($key, [string]$extraEnv[$key], 'Process')
    }
  }
  Push-Location $root
  try {
    Remove-Item -LiteralPath $logPath,$stderrLogPath -Force -ErrorAction SilentlyContinue
    $global:ErrorActionPreference = 'Continue'
    & cargo @arguments > $logPath 2> $stderrLogPath
    $exitCode = $LASTEXITCODE
    $global:ErrorActionPreference = $previousErrorAction
    if ($exitCode -ne 0) {
      throw "cargo command failed ($exitCode): cargo $($arguments -join ' '); stdout=$logPath; stderr=$stderrLogPath"
    }
  } finally {
    $global:ErrorActionPreference = $previousErrorAction
    Pop-Location
    if ($extraEnv) {
      foreach ($key in $extraEnv.Keys) {
        [Environment]::SetEnvironmentVariable($key, $previous[$key], 'Process')
      }
    }
  }
  return $logPath
}

function Read-ProofText([string]$path) {
  if (Test-Path -LiteralPath $path) {
    return Get-Content -LiteralPath $path -Raw
  }
  return ''
}

$stage = 'started'
$process = $null
$main = $null
$minibar = $null
$sample10 = $null
$sample30 = $null
$sample60 = $null
$processExitedAfterQuitHotkey = $false

try {
  $stage = 'build-release'
  Run-CargoCommand @(
    'build',
    '--manifest-path', 'bentodesk-nano/Cargo.toml',
    '-p', 'bento-nano-shell',
    '--release',
    '--target', $targetTriple
  ) '00-release-build.log' @{ CARGO_BUILD_JOBS='1'; CARGO_INCREMENTAL='0' } | Out-Null
  if (-not (Test-Path -LiteralPath $exe)) {
    throw "release executable missing after build: $exe"
  }
  $binarySize = (Get-Item -LiteralPath $exe).Length

  $stage = 'seed'
  Run-CargoCommand @(
    'run', '--quiet',
    '--manifest-path', 'bentodesk-nano/Cargo.toml',
    '-p', 'bento-nano-platform',
    '--example', 'seed_benchmark_scene',
    '--target', $targetTriple,
    '--', $stateDir
  ) '01-seed-benchmark-scene.txt' @{ CARGO_BUILD_JOBS='1'; CARGO_INCREMENTAL='0'; BENTODESK_NANO_BENCHMARK_ITEM_ROOT=$itemRoot } | Out-Null
  Run-CargoCommand @(
    'run', '--quiet',
    '--manifest-path', 'bentodesk-nano/Cargo.toml',
    '-p', 'bento-nano-backend',
    '--example', 'seed_minibar_pins',
    '--target', $targetTriple,
    '--', $stateDir, '1'
  ) '02-seed-minibar-pins.txt' @{ CARGO_BUILD_JOBS='1'; CARGO_INCREMENTAL='0' } | Out-Null
  Run-CargoCommand @(
    'run', '--quiet',
    '--manifest-path', 'bentodesk-nano/Cargo.toml',
    '-p', 'bento-nano-platform',
    '--example', 'dump_zones',
    '--target', $targetTriple,
    '--', $stateDir
  ) '03-zones-before-launch.tsv' @{ CARGO_BUILD_JOBS='1'; CARGO_INCREMENTAL='0' } | Out-Null
  Run-CargoCommand @(
    'run', '--quiet',
    '--manifest-path', 'bentodesk-nano/Cargo.toml',
    '-p', 'bento-nano-platform',
    '--example', 'dump_zone_items',
    '--target', $targetTriple,
    '--', $stateDir
  ) '04-items-before-launch.tsv' @{ CARGO_BUILD_JOBS='1'; CARGO_INCREMENTAL='0' } | Out-Null

  $zoneRows = @(Get-Content -LiteralPath (Join-Path $proofDir '03-zones-before-launch.tsv') | Where-Object { $_ -match '^\d+\t' })
  $itemRows = @(Get-Content -LiteralPath (Join-Path $proofDir '04-items-before-launch.tsv') | Where-Object { $_ -match '^\d+\t' })
  if ($zoneRows.Count -ne 5) { throw "expected 5 benchmark zones, got $($zoneRows.Count)" }
  if ($itemRows.Count -ne 50) { throw "expected 50 benchmark items, got $($itemRows.Count)" }

  $stage = 'launch-release'
  Remove-Item -LiteralPath $stdoutPath,$stderrPath -Force -ErrorAction SilentlyContinue
  $old = Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $exe }
  foreach ($oldProcess in $old) {
    Stop-Process -Id $oldProcess.ProcessId -Force -ErrorAction SilentlyContinue
  }
  Start-Sleep -Milliseconds 500

  $previousState = $env:BENTODESK_NANO_STATE_DIR
  $env:BENTODESK_NANO_STATE_DIR = $stateDir
  try {
    $process = Start-Process -FilePath $exe `
      -WorkingDirectory (Split-Path $exe) `
      -RedirectStandardOutput $stdoutPath `
      -RedirectStandardError $stderrPath `
      -PassThru
  } finally {
    if ($null -eq $previousState) {
      Remove-Item Env:\BENTODESK_NANO_STATE_DIR -ErrorAction SilentlyContinue
    } else {
      $env:BENTODESK_NANO_STATE_DIR = $previousState
    }
  }

  $stage = 'wait-windows'
  $main = Wait-Window $process.Id 'BentoNanoShell' 15000
  if (-not $main) { throw 'main BentoNanoShell HWND not visible' }
  $minibar = Wait-Window $process.Id 'BentoAuxMbar' 15000
  if (-not $minibar) { throw 'MiniBar BentoAuxMbar HWND not visible' }
  Save-WindowShot $main '05-main-benchmark.png' | Out-Null
  Save-WindowShot $minibar '06-minibar-benchmark.png' | Out-Null

  $stage = 'sample-memory'
  Start-Sleep -Seconds 10
  $sample10 = Get-ProcessSample $process 't10'
  Write-Json $sample10 '07-process-t10.json'
  Start-Sleep -Seconds 20
  $sample30 = Get-ProcessSample $process 't30'
  Write-Json $sample30 '08-process-t30.json'
  Start-Sleep -Seconds 30
  $sample60 = Get-ProcessSample $process 't60'
  Write-Json $sample60 '09-process-t60.json'

  $processCount = @(Get-Process -Name 'bento-nano-shell' -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -eq $exe }).Count
  if ($processCount -ne 1) {
    throw "expected exactly one selected-stack process, got $processCount"
  }

  $stderrText = Read-ProofText $stderrPath
  foreach ($needle in @('startup: locale=zh-CN', 'startup: acrylic_feature=on', 'startup: acrylic_runtime=')) {
    if (-not $stderrText.Contains($needle)) {
      throw "memory proof missing startup diagnostic: $needle"
    }
  }
  if (-not $stderrText.Contains('tray: NIM_ADD registered; NIM_SETVERSION=4')) {
    throw 'memory proof missing tray registration log'
  }

  $stage = 'quit'
  [void][NativeWs7MemoryProof]::PostMessageW([IntPtr]$main.hwnd, [NativeWs7MemoryProof]::WM_HOTKEY, [UIntPtr]([uint64]16973), [IntPtr]::Zero)
  $processExitedAfterQuitHotkey = $process.WaitForExit(8000)
  if (-not $processExitedAfterQuitHotkey) {
    throw 'process did not exit after production QuitApp hotkey'
  }

  $samples = @($sample10, $sample30, $sample60)
  $maxPrivateMb = ($samples | ForEach-Object { [double]$_.private_mb } | Measure-Object -Maximum).Maximum
  $accepted = (
    $binarySize -le 2621440 -and
    $zoneRows.Count -eq 5 -and
    $itemRows.Count -eq 50 -and
    $main -and
    $minibar -and
    $processCount -eq 1 -and
    $maxPrivateMb -le 25.0 -and
    $processExitedAfterQuitHotkey
  )

  $summary = [ordered]@{
    status = if ($accepted) { 'ok' } else { 'failed' }
    stage = 'completed'
    ws_id = 'WS-7'
    proof_kind = 'current-private-bytes-budget'
    no_mock_data = $true
    accepted = [bool]$accepted
    proof_dir = $proofDir
    state_dir = $stateDir
    selected_stack_exe = $exe
    release_binary_bytes = [int64]$binarySize
    release_binary_under_2_5mb = ($binarySize -le 2621440)
    private_bytes_threshold_mb = 25.0
    max_private_mb = [Math]::Round([double]$maxPrivateMb, 2)
    memory_samples = $samples
    benchmark = [ordered]@{
      zones = [int]$zoneRows.Count
      items = [int]$itemRows.Count
      main_visible = [bool]$main
      main_class = $main.class
      main_title = $main.title
      main_rect = [ordered]@{ width=[int]$main.rect.width; height=[int]$main.rect.height }
      minibar_visible = [bool]$minibar
      minibar_class = $minibar.class
      minibar_title = $minibar.title
      minibar_rect = [ordered]@{ width=[int]$minibar.rect.width; height=[int]$minibar.rect.height }
      process_count = [int]$processCount
      chinese_locale_logged = $stderrText.Contains('startup: locale=zh-CN')
      acrylic_feature_logged = $stderrText.Contains('startup: acrylic_feature=on')
      acrylic_runtime_logged = $stderrText.Contains('startup: acrylic_runtime=')
      tray_registered = $stderrText.Contains('tray: NIM_ADD registered; NIM_SETVERSION=4')
      t10 = $sample10
      t30 = $sample30
      t60 = $sample60
      private_mb_under_25_at_t10 = ($sample10.private_mb -le 25.0)
      private_mb_under_25_at_t30 = ($sample30.private_mb -le 25.0)
      private_mb_under_25_at_t60 = ($sample60.private_mb -le 25.0)
    }
    screenshots = @(
      '05-main-benchmark.png',
      '06-minibar-benchmark.png'
    )
    stdout = $stdoutPath
    stderr = $stderrPath
    process_exited_after_quit_hotkey = $processExitedAfterQuitHotkey
  }
  Write-Json $summary 'summary.json'
  if (-not $accepted) { throw 'WS-7 memory proof failed; see summary.json' }
} catch {
  $message = $_.Exception.Message
  if ($process -and -not $process.HasExited) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    $process.WaitForExit(3000) | Out-Null
  }
  $summary = [ordered]@{
    status = 'failed'
    stage = $stage
    ws_id = 'WS-7'
    proof_kind = 'current-private-bytes-budget'
    no_mock_data = $true
    accepted = $false
    error = $message
    proof_dir = $proofDir
    state_dir = $stateDir
    selected_stack_exe = $exe
    main_window = $main
    minibar_window = $minibar
    memory_samples = @($sample10, $sample30, $sample60)
    stdout = $stdoutPath
    stderr = $stderrPath
    process_exited_after_quit_hotkey = $processExitedAfterQuitHotkey
  }
  Write-Json $summary 'summary.json'
  throw
} finally {
  if ($process -and -not $process.HasExited) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    $process.WaitForExit(3000) | Out-Null
  }
}
