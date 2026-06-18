$ErrorActionPreference = 'Stop'

$root = 'D:\Desktop\CREATOR FOUR'
$nano = Join-Path $root 'bentodesk-nano'
$sourceExe = Join-Path $nano 'target\x86_64-pc-windows-msvc\debug\bento-nano-shell.exe'
$stateDir = Join-Path $nano 'runtime-proof-0608-expanded-morph-visual-state'
$proofDir = Join-Path $nano 'runtime-proof-0608-expanded-morph-visual-try'
$proofExe = Join-Path $proofDir 'bento-nano-shell-proof.exe'
$itemRoot = Join-Path $stateDir 'items'
$zonesPath = Join-Path $stateDir 'zones.bin'
$stderrPath = Join-Path $proofDir 'stderr.log'
$stdoutPath = Join-Path $proofDir 'stdout.log'
$summaryPath = Join-Path $proofDir 'summary.json'

if (-not (Test-Path -LiteralPath $sourceExe)) {
  throw "source exe not found: $sourceExe"
}

New-Item -ItemType Directory -Force -Path $proofDir | Out-Null
Get-ChildItem -LiteralPath $proofDir -File -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -ne 'runtime-proof-0608-expanded-morph-visual-run.ps1' } |
  Remove-Item -Force -ErrorAction SilentlyContinue
if (Test-Path -LiteralPath $stateDir) { Remove-Item -LiteralPath $stateDir -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
Copy-Item -LiteralPath $sourceExe -Destination $proofExe -Force

$env:BENTODESK_NANO_BENCHMARK_ITEM_ROOT = $itemRoot
Push-Location $root
try {
  & cargo run --quiet --manifest-path bentodesk-nano/Cargo.toml -p bento-nano-platform --example seed_benchmark_scene --target x86_64-pc-windows-msvc -- $stateDir |
    Out-File -FilePath (Join-Path $proofDir '00-seed-benchmark-scene.txt') -Encoding utf8
} finally {
  Pop-Location
  Remove-Item Env:\BENTODESK_NANO_BENCHMARK_ITEM_ROOT -ErrorAction SilentlyContinue
}

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class NativeProof0608 {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool InvalidateRect(IntPtr hWnd, IntPtr lpRect, bool bErase);
  [DllImport("user32.dll")] public static extern bool UpdateWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdcBlt, uint nFlags);
  [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr hMenu);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetMenuStringW(IntPtr hMenu, uint uIDItem, StringBuilder lpString, int cchMax, uint flags);
  [DllImport("user32.dll")] public static extern uint GetMenuItemID(IntPtr hMenu, int nPos);
  public const uint WM_PAINT = 0x000F;
  public const uint WM_COMMAND = 0x0111;
  public const uint WM_HOTKEY = 0x0312;
  public const uint WM_KEYDOWN = 0x0100;
  public const uint WM_MOUSEMOVE = 0x0200;
  public const uint WM_LBUTTONDOWN = 0x0201;
  public const uint WM_LBUTTONUP = 0x0202;
  public const uint WM_RBUTTONUP = 0x0205;
  public const uint MN_GETHMENU = 0x01E1;
  public const uint MF_BYPOSITION = 0x00000400;
  public const uint PW_RENDERFULLCONTENT = 0x00000002;
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@

function Convert-WindowForJson($win) {
  if (-not $win) { return $null }
  return [ordered]@{
    hwnd = [int64]$win.hwnd
    class = [string]$win.class
    title = [string]$win.title
    visible = [bool]$win.visible
    dpi = [int]$win.dpi
    rect = [ordered]@{
      left = [int]$win.rect.left
      top = [int]$win.rect.top
      right = [int]$win.rect.right
      bottom = [int]$win.rect.bottom
      width = [int]$win.rect.width
      height = [int]$win.rect.height
    }
  }
}

function Get-WindowsForPid([int]$processId) {
  $items = New-Object System.Collections.ArrayList
  $cb = [NativeProof0608+EnumWindowsProc]{
    param([IntPtr]$hwnd, [IntPtr]$lparam)
    [uint32]$wpid = 0
    [void][NativeProof0608]::GetWindowThreadProcessId($hwnd, [ref]$wpid)
    if ($wpid -eq [uint32]$processId) {
      $class = New-Object System.Text.StringBuilder 256
      $title = New-Object System.Text.StringBuilder 256
      [void][NativeProof0608]::GetClassName($hwnd, $class, $class.Capacity)
      [void][NativeProof0608]::GetWindowText($hwnd, $title, $title.Capacity)
      $rect = New-Object NativeProof0608+RECT
      [void][NativeProof0608]::GetWindowRect($hwnd, [ref]$rect)
      [void]$items.Add([pscustomobject]@{
        hwnd = $hwnd.ToInt64()
        class = $class.ToString()
        title = $title.ToString()
        visible = [NativeProof0608]::IsWindowVisible($hwnd)
        dpi = [NativeProof0608]::GetDpiForWindow($hwnd)
        rect = [pscustomobject]@{
          left=$rect.Left; top=$rect.Top; right=$rect.Right; bottom=$rect.Bottom
          width=($rect.Right-$rect.Left); height=($rect.Bottom-$rect.Top)
        }
      })
    }
    return $true
  }
  [void][NativeProof0608]::EnumWindows($cb, [IntPtr]::Zero)
  return @($items.ToArray())
}

function Wait-Window([int]$processId, [string]$class, [int]$timeoutMs = 8000, [bool]$visibleOnly = $true) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $timeoutMs) {
    $win = Get-WindowsForPid $processId | Where-Object { $_.class -eq $class -and (-not $visibleOnly -or $_.visible) } | Select-Object -First 1
    if ($win) { return $win }
    Start-Sleep -Milliseconds 100
  }
  return $null
}

function Wait-Condition([scriptblock]$predicate, [int]$timeoutMs = 5000) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $timeoutMs) {
    $value = & $predicate
    if ($value) { return $value }
    Start-Sleep -Milliseconds 100
  }
  return $null
}

function New-LParam([int]$x, [int]$y) {
  return [IntPtr](((($y -band 0xffff) -shl 16) -bor ($x -band 0xffff)))
}

function Send-ClientMessage($win, [uint32]$msg, [double]$logicalX, [double]$logicalY, [string]$mode = 'send', [int]$sleepMs = 220) {
  $clientX = [int][Math]::Round($logicalX)
  $clientY = [int][Math]::Round($logicalY)
  [void][NativeProof0608]::SetForegroundWindow([IntPtr]$win.hwnd)
  [void][NativeProof0608]::SetCursorPos([int]($win.rect.left + $clientX), [int]($win.rect.top + $clientY))
  Start-Sleep -Milliseconds 60
  $lp = New-LParam $clientX $clientY
  if ($mode -eq 'post') {
    [void][NativeProof0608]::PostMessageW([IntPtr]$win.hwnd, $msg, [UIntPtr]::Zero, $lp)
  } else {
    [void][NativeProof0608]::SendMessageW([IntPtr]$win.hwnd, $msg, [UIntPtr]::Zero, $lp)
  }
  Start-Sleep -Milliseconds $sleepMs
  return [ordered]@{ msg=$msg; client_x=$clientX; client_y=$clientY; mode=$mode }
}

function Force-Paint($win) {
  if (-not $win) { return }
  [void][NativeProof0608]::InvalidateRect([IntPtr]$win.hwnd, [IntPtr]::Zero, $false)
  [void][NativeProof0608]::UpdateWindow([IntPtr]$win.hwnd)
  [void][NativeProof0608]::SendMessageW([IntPtr]$win.hwnd, [NativeProof0608]::WM_PAINT, [UIntPtr]::Zero, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 550
}

function Request-Paint($win) {
  if (-not $win) { return }
  [void][NativeProof0608]::InvalidateRect([IntPtr]$win.hwnd, [IntPtr]::Zero, $false)
  [void][NativeProof0608]::UpdateWindow([IntPtr]$win.hwnd)
  [void][NativeProof0608]::SendMessageW([IntPtr]$win.hwnd, [NativeProof0608]::WM_PAINT, [UIntPtr]::Zero, [IntPtr]::Zero)
}

function Capture-MainFrame($win, [string]$fileName, [int]$sleepMs, [Diagnostics.Stopwatch]$clock) {
  if ($sleepMs -gt 0) {
    Start-Sleep -Milliseconds $sleepMs
  }
  Request-Paint $win
  $path = Join-Path $proofDir $fileName
  $saved = Save-WindowShot $win $path
  return [ordered]@{
    file = $fileName
    saved = [bool]$saved
    elapsed_ms = [int]$clock.ElapsedMilliseconds
  }
}

function Save-WindowShot($win, [string]$path) {
  if (-not $win) { return $false }
  $w = [Math]::Max(1, [int]$win.rect.width)
  $h = [Math]::Max(1, [int]$win.rect.height)
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  try {
    $hdc = $g.GetHdc()
    try {
      $printed = [NativeProof0608]::PrintWindow([IntPtr]$win.hwnd, $hdc, [NativeProof0608]::PW_RENDERFULLCONTENT)
    } finally {
      $g.ReleaseHdc($hdc)
    }
    if (-not $printed) {
      $g.CopyFromScreen([int]$win.rect.left, [int]$win.rect.top, 0, 0, [System.Drawing.Size]::new($w, $h))
    }
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    return $true
  } finally {
    $g.Dispose()
    $bmp.Dispose()
  }
}

function Get-PopupMenuItems($popup) {
  if (-not $popup) { return @() }
  $menu = [NativeProof0608]::SendMessageW([IntPtr]$popup.hwnd, [NativeProof0608]::MN_GETHMENU, [UIntPtr]::Zero, [IntPtr]::Zero)
  if ($menu -eq [IntPtr]::Zero) { return @() }
  $count = [NativeProof0608]::GetMenuItemCount($menu)
  $items = @()
  for ($i = 0; $i -lt $count; $i++) {
    $label = New-Object System.Text.StringBuilder 256
    [void][NativeProof0608]::GetMenuStringW($menu, [uint32]$i, $label, $label.Capacity, [NativeProof0608]::MF_BYPOSITION)
    $items += [pscustomobject]@{
      index = $i
      id = [NativeProof0608]::GetMenuItemID($menu, $i)
      label = $label.ToString()
    }
  }
  return @($items)
}

function Send-ZoneMenuCommand($owner, $popup, $item) {
  [void][NativeProof0608]::SendMessageW([IntPtr]$owner.hwnd, [NativeProof0608]::WM_COMMAND, [UIntPtr]([uint64]$item.id), [IntPtr]::Zero)
  Start-Sleep -Milliseconds 250
  [void][NativeProof0608]::SendMessageW([IntPtr]$popup.hwnd, [NativeProof0608]::WM_KEYDOWN, [UIntPtr]([uint64]0x1B), [IntPtr]::Zero)
  Start-Sleep -Milliseconds 700
  return [ordered]@{ hwnd=[int64]$popup.hwnd; owner_hwnd=[int64]$owner.hwnd; label=[string]$item.label; id=[int]$item.id; method='visible-native-menu-wm-command' }
}

function Dump-Example([string]$example, [string]$fileName) {
  $path = Join-Path $proofDir $fileName
  Push-Location $root
  try {
    $output = & cargo run --quiet --manifest-path bentodesk-nano/Cargo.toml -p bento-nano-platform --example $example --target x86_64-pc-windows-msvc -- $zonesPath 2>&1
    $output | ForEach-Object { $_.ToString() } | Out-File -FilePath $path -Encoding utf8
    if ($LASTEXITCODE -ne 0) {
      throw "$example failed with exit code $LASTEXITCODE"
    }
    return @($output | ForEach-Object { $_.ToString() })
  } finally {
    Pop-Location
  }
}

function Parse-ZoneGeometry([string[]]$lines, [int]$id) {
  foreach ($line in $lines) {
    $cols = $line -split "`t", -1
    if ($cols.Count -lt 8) { continue }
    $rowId = $cols[0].TrimStart([char]0xFEFF)
    if ($rowId -eq [string]$id) {
      return [ordered]@{
        zone_id = [int]$rowId
        title = [string]$cols[1]
        x = [int]$cols[2]
        y = [int]$cols[3]
        w = [int]$cols[4]
        h = [int]$cols[5]
        visible = [bool]::Parse($cols[6])
        item_count = [int]$cols[7]
      }
    }
  }
  return $null
}

function Test-StackRow([string[]]$lines, [int]$id, [string]$parent, [string]$members) {
  foreach ($line in $lines) {
    $cols = $line -split "`t", -1
    if ($cols.Count -lt 5) { continue }
    $rowId = $cols[0].TrimStart([char]0xFEFF)
    if ($rowId -eq [string]$id -and $cols[3] -eq $parent -and $cols[4] -eq $members) {
      return $true
    }
  }
  return $false
}

function Post-Quit($win) {
  [void][NativeProof0608]::PostMessageW([IntPtr]$win.hwnd, [NativeProof0608]::WM_HOTKEY, [UIntPtr]([uint64]16973), [IntPtr]::Zero)
  Start-Sleep -Milliseconds 200
}

function Start-Target {
  $psi = [Diagnostics.ProcessStartInfo]::new($proofExe)
  $psi.WorkingDirectory = $proofDir
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.Environment['BENTODESK_NANO_STATE_DIR'] = $stateDir
  return [Diagnostics.Process]::Start($psi)
}

$stage = 'started'
$proc = $null
$main = $null
$clicks = New-Object System.Collections.ArrayList
$menuItems = @()
$menuCommand = $null
$stderrAll = ''
$stdoutAll = ''
$processExitedAfterQuitHotkey = $false

try {
  $stackBefore = Dump-Example 'dump_stack_summary' '01-stack-before.tsv'
  $geomBefore = Dump-Example 'dump_zone_geometry' '02-geometry-before.tsv'
  $zone4Before = Parse-ZoneGeometry $geomBefore 4
  if (-not $zone4Before) { throw 'benchmark zone 4 geometry not found' }
  $zoneFileBefore = Get-Item -LiteralPath $zonesPath

  $proc = Start-Target
  $main = Wait-Window $proc.Id 'BentoNanoShell' 10000
  if (-not $main) { throw 'main window not found' }
  Start-Sleep -Milliseconds 1200
  Force-Paint $main
  $frames = New-Object System.Collections.ArrayList
  $captureClock = [Diagnostics.Stopwatch]::StartNew()
  [void]$frames.Add((Capture-MainFrame $main '03-expanded-00-before-collapsed.png' 0 $captureClock))

  $stage = 'hover-expand-zone-4'
  $hoverX = [double]($zone4Before.x + 42)
  $hoverY = [double]($zone4Before.y + 24)
  $writeBeforeHover = (Get-Item -LiteralPath $zonesPath).LastWriteTimeUtc
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0608]::WM_MOUSEMOVE) $hoverX $hoverY 'send' 40))
  $captureClock.Restart()
  [void]$frames.Add((Capture-MainFrame $main '04-expanded-01-open-start.png' 0 $captureClock))
  [void]$frames.Add((Capture-MainFrame $main '05-expanded-02-open-mid-090ms.png' 90 $captureClock))
  [void]$frames.Add((Capture-MainFrame $main '06-expanded-03-open-mid-230ms.png' 140 $captureClock))
  [void]$frames.Add((Capture-MainFrame $main '07-expanded-04-open-stable.png' 360 $captureClock))

  $stage = 'leave-collapse-zone-4'
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0608]::WM_MOUSEMOVE) 24 24 'send' 40))
  $captureClock.Restart()
  [void]$frames.Add((Capture-MainFrame $main '08-expanded-05-close-start.png' 0 $captureClock))
  [void]$frames.Add((Capture-MainFrame $main '09-expanded-06-close-pre-delay-160ms.png' 160 $captureClock))
  [void]$frames.Add((Capture-MainFrame $main '10-expanded-07-close-mid-380ms.png' 220 $captureClock))
  [void]$frames.Add((Capture-MainFrame $main '11-expanded-08-close-stable.png' 360 $captureClock))
  $writeAfterHover = (Get-Item -LiteralPath $zonesPath).LastWriteTimeUtc
  $geomAfterHover = Dump-Example 'dump_zone_geometry' '12-geometry-after-hover.tsv'
  $zone4AfterHover = Parse-ZoneGeometry $geomAfterHover 4

  $frameSummary = Join-Path $proofDir 'expanded-frame-summary.tsv'
  @('file	elapsed_ms	saved') | Out-File -FilePath $frameSummary -Encoding utf8
  foreach ($frame in @($frames.ToArray())) {
    ("{0}`t{1}`t{2}" -f $frame.file, $frame.elapsed_ms, $frame.saved) |
      Out-File -FilePath $frameSummary -Encoding utf8 -Append
  }

  $stage = 'quit'
  Post-Quit $main
  $processExitedAfterQuitHotkey = [bool](Wait-Condition { $proc.HasExited } 5000)
  if (-not $processExitedAfterQuitHotkey) { throw 'process did not exit after production quit hotkey' }
  $proc.WaitForExit(3000) | Out-Null
  $stderrAll = $proc.StandardError.ReadToEnd()
  $stdoutAll = $proc.StandardOutput.ReadToEnd()
  $stderrAll | Out-File -FilePath $stderrPath -Encoding utf8
  $stdoutAll | Out-File -FilePath $stdoutPath -Encoding utf8

  $zone4StillUnstacked = Test-StackRow $stackBefore 4 '' ''
  $allFramesSaved = @($frames.ToArray()) | Where-Object { -not $_.saved } | Measure-Object | Select-Object -ExpandProperty Count
  $noWriteDuringHover = ($writeAfterHover -eq $writeBeforeHover)
  $logs = [ordered]@{
    startup_locale = $stderrAll.Contains('startup: locale=')
    acrylic_runtime = $stderrAll.Contains('startup: acrylic_runtime=')
    tray_registered = $stderrAll.Contains('tray: NIM_ADD registered')
  }

  $summary = [ordered]@{
    status = if ($zone4StillUnstacked -and $zone4Before -and $zone4AfterHover -and ($allFramesSaved -eq 0) -and $noWriteDuringHover -and $processExitedAfterQuitHotkey) { 'ok' } else { 'failed' }
    stage = 'completed'
    exe = $proofExe
    source_exe = $sourceExe
    state_dir = $stateDir
    main_window = Convert-WindowForJson $main
    expanded_morph = [ordered]@{
      zone_id = 4
      before = $zone4Before
      after = $zone4AfterHover
      stack_summary_unstacked = [bool]$zone4StillUnstacked
      hover_point = [ordered]@{ x = $hoverX; y = $hoverY }
      leave_point = [ordered]@{ x = 24; y = 24 }
      no_write_during_hover = [bool]$noWriteDuringHover
      zones_bin_write_time_utc_before_hover = $writeBeforeHover.ToString('o')
      zones_bin_write_time_utc_after_hover = $writeAfterHover.ToString('o')
      visual_review_required = $true
    }
    logs = $logs
    clicks = @($clicks.ToArray())
    frames = @($frames.ToArray())
    screenshots = @($frames.ToArray() | ForEach-Object { $_.file })
    process_exited_after_quit_hotkey = $processExitedAfterQuitHotkey
  }
  $summary | ConvertTo-Json -Depth 12 | Out-File -FilePath $summaryPath -Encoding utf8
  if ($summary.status -ne 'ok') { throw 'runtime proof assertions failed; see summary.json' }
} catch {
  $message = $_.Exception.Message
  $partialSummary = $null
  if (Test-Path -LiteralPath $summaryPath) {
    try {
      $partialSummary = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
    } catch {
      $partialSummary = $null
    }
  }
  if ($proc -and -not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    $proc.WaitForExit(3000) | Out-Null
  }
  if ($proc) {
    try {
      $stderrAll = $proc.StandardError.ReadToEnd()
      $stdoutAll = $proc.StandardOutput.ReadToEnd()
      $stderrAll | Out-File -FilePath $stderrPath -Encoding utf8
      $stdoutAll | Out-File -FilePath $stdoutPath -Encoding utf8
    } catch {}
  }
  $summary = [ordered]@{
    status = 'failed'
    stage = $stage
    error = $message
    exe = $proofExe
    source_exe = $sourceExe
    state_dir = $stateDir
    main_window = Convert-WindowForJson $main
    clicks = @($clicks.ToArray())
    partial_summary = $partialSummary
  }
  $summary | ConvertTo-Json -Depth 12 | Out-File -FilePath $summaryPath -Encoding utf8
  throw
}
