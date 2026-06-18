$ErrorActionPreference = 'Stop'

$root = 'D:\Desktop\CREATOR FOUR'
$nano = Join-Path $root 'bentodesk-nano'
$sourceExe = Join-Path $nano 'target\x86_64-pc-windows-msvc\debug\bento-nano-shell.exe'
$stateDir = Join-Path $nano 'runtime-proof-0608-merge-dissolve-scatter-state'
$proofDir = Join-Path $nano 'runtime-proof-0608-merge-dissolve-scatter-try'
$proofExe = Join-Path $proofDir 'bento-nano-shell-proof.exe'
$itemRoot = Join-Path $stateDir 'items'
$zonesPath = Join-Path $stateDir 'zones.bin'
$stderrPath = Join-Path $proofDir 'stderr.log'
$stdoutPath = Join-Path $proofDir 'stdout.log'
$summaryPath = Join-Path $proofDir 'summary.json'

function Assert-UnderNano([string]$path) {
  $fullPath = [System.IO.Path]::GetFullPath($path)
  $fullNano = [System.IO.Path]::GetFullPath($nano).TrimEnd('\')
  if (-not $fullPath.StartsWith($fullNano + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "refusing to touch path outside nano workspace: $fullPath"
  }
}

if (-not (Test-Path -LiteralPath $sourceExe)) {
  throw "source exe not found: $sourceExe"
}

Assert-UnderNano $stateDir
Assert-UnderNano $proofDir
if (Test-Path -LiteralPath $proofDir) {
  Remove-Item -LiteralPath $proofDir -Recurse -Force
}
if (Test-Path -LiteralPath $stateDir) {
  Remove-Item -LiteralPath $stateDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $proofDir | Out-Null
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
public static class NativeProof0608Merge {
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
  $cb = [NativeProof0608Merge+EnumWindowsProc]{
    param([IntPtr]$hwnd, [IntPtr]$lparam)
    [uint32]$wpid = 0
    [void][NativeProof0608Merge]::GetWindowThreadProcessId($hwnd, [ref]$wpid)
    if ($wpid -eq [uint32]$processId) {
      $class = New-Object System.Text.StringBuilder 256
      $title = New-Object System.Text.StringBuilder 256
      [void][NativeProof0608Merge]::GetClassName($hwnd, $class, $class.Capacity)
      [void][NativeProof0608Merge]::GetWindowText($hwnd, $title, $title.Capacity)
      $rect = New-Object NativeProof0608Merge+RECT
      [void][NativeProof0608Merge]::GetWindowRect($hwnd, [ref]$rect)
      [void]$items.Add([pscustomobject]@{
        hwnd = $hwnd.ToInt64()
        class = $class.ToString()
        title = $title.ToString()
        visible = [NativeProof0608Merge]::IsWindowVisible($hwnd)
        dpi = [NativeProof0608Merge]::GetDpiForWindow($hwnd)
        rect = [pscustomobject]@{
          left=$rect.Left; top=$rect.Top; right=$rect.Right; bottom=$rect.Bottom
          width=($rect.Right-$rect.Left); height=($rect.Bottom-$rect.Top)
        }
      })
    }
    return $true
  }
  [void][NativeProof0608Merge]::EnumWindows($cb, [IntPtr]::Zero)
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
  [void][NativeProof0608Merge]::SetForegroundWindow([IntPtr]$win.hwnd)
  [void][NativeProof0608Merge]::SetCursorPos([int]($win.rect.left + $clientX), [int]($win.rect.top + $clientY))
  Start-Sleep -Milliseconds 60
  $lp = New-LParam $clientX $clientY
  if ($mode -eq 'post') {
    [void][NativeProof0608Merge]::PostMessageW([IntPtr]$win.hwnd, $msg, [UIntPtr]::Zero, $lp)
  } else {
    [void][NativeProof0608Merge]::SendMessageW([IntPtr]$win.hwnd, $msg, [UIntPtr]::Zero, $lp)
  }
  Start-Sleep -Milliseconds $sleepMs
  return [ordered]@{ msg=$msg; client_x=$clientX; client_y=$clientY; mode=$mode }
}

function Post-ClientMessageThenMoveCursor($win, [uint32]$msg, [double]$logicalX, [double]$logicalY, [double]$safeLogicalX, [double]$safeLogicalY, [int]$sleepMs = 350) {
  $clientX = [int][Math]::Round($logicalX)
  $clientY = [int][Math]::Round($logicalY)
  $safeClientX = [int][Math]::Round($safeLogicalX)
  $safeClientY = [int][Math]::Round($safeLogicalY)
  [void][NativeProof0608Merge]::SetForegroundWindow([IntPtr]$win.hwnd)
  [void][NativeProof0608Merge]::SetCursorPos([int]($win.rect.left + $clientX), [int]($win.rect.top + $clientY))
  Start-Sleep -Milliseconds 60
  $lp = New-LParam $clientX $clientY
  [void][NativeProof0608Merge]::PostMessageW([IntPtr]$win.hwnd, $msg, [UIntPtr]::Zero, $lp)
  [void][NativeProof0608Merge]::SetCursorPos([int]($win.rect.left + $safeClientX), [int]($win.rect.top + $safeClientY))
  Start-Sleep -Milliseconds $sleepMs
  return [ordered]@{
    msg = $msg
    client_x = $clientX
    client_y = $clientY
    mode = 'post-move-cursor'
    safe_client_x = $safeClientX
    safe_client_y = $safeClientY
  }
}

function Force-Paint($win) {
  if (-not $win) { return }
  [void][NativeProof0608Merge]::InvalidateRect([IntPtr]$win.hwnd, [IntPtr]::Zero, $false)
  [void][NativeProof0608Merge]::UpdateWindow([IntPtr]$win.hwnd)
  [void][NativeProof0608Merge]::SendMessageW([IntPtr]$win.hwnd, [NativeProof0608Merge]::WM_PAINT, [UIntPtr]::Zero, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 550
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
      $printed = [NativeProof0608Merge]::PrintWindow([IntPtr]$win.hwnd, $hdc, [NativeProof0608Merge]::PW_RENDERFULLCONTENT)
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
  $menu = [NativeProof0608Merge]::SendMessageW([IntPtr]$popup.hwnd, [NativeProof0608Merge]::MN_GETHMENU, [UIntPtr]::Zero, [IntPtr]::Zero)
  if ($menu -eq [IntPtr]::Zero) { return @() }
  $count = [NativeProof0608Merge]::GetMenuItemCount($menu)
  $items = @()
  for ($i = 0; $i -lt $count; $i++) {
    $label = New-Object System.Text.StringBuilder 256
    [void][NativeProof0608Merge]::GetMenuStringW($menu, [uint32]$i, $label, $label.Capacity, [NativeProof0608Merge]::MF_BYPOSITION)
    $items += [pscustomobject]@{
      index = $i
      id = [NativeProof0608Merge]::GetMenuItemID($menu, $i)
      label = $label.ToString()
    }
  }
  return @($items)
}

function Send-ZoneMenuCommand($owner, $popup, $item) {
  [void][NativeProof0608Merge]::SendMessageW([IntPtr]$owner.hwnd, [NativeProof0608Merge]::WM_COMMAND, [UIntPtr]([uint64]$item.id), [IntPtr]::Zero)
  Start-Sleep -Milliseconds 250
  [void][NativeProof0608Merge]::SendMessageW([IntPtr]$popup.hwnd, [NativeProof0608Merge]::WM_KEYDOWN, [UIntPtr]([uint64]0x1B), [IntPtr]::Zero)
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

function Parse-StackRow([string[]]$lines, [int]$id) {
  foreach ($line in $lines) {
    $cols = $line -split "`t", -1
    if ($cols.Count -lt 5) { continue }
    $rowId = $cols[0].TrimStart([char]0xFEFF)
    if ($rowId -eq [string]$id) {
      return [ordered]@{
        zone_id = [int]$rowId
        title = [string]$cols[1]
        visible = [bool]::Parse($cols[2])
        stack_parent = [string]$cols[3]
        stack_members = [string]$cols[4]
      }
    }
  }
  return $null
}

function Test-StackRow([string[]]$lines, [int]$id, [string]$parent, [string]$members) {
  $row = Parse-StackRow $lines $id
  return [bool]($row -and $row.stack_parent -eq $parent -and $row.stack_members -eq $members)
}

function Test-WithinViewport($geom, [int]$viewportW, [int]$viewportH) {
  return [bool]($geom -and $geom.x -ge 0 -and $geom.y -ge 0 -and ($geom.x + $geom.w) -le $viewportW -and ($geom.y + $geom.h) -le $viewportH)
}

function Clamp-Double([double]$value, [double]$min, [double]$max) {
  return [Math]::Min([Math]::Max($value, $min), $max)
}

function Get-DissolveClick($anchor, [int]$memberCount, [int]$viewportW, [int]$viewportH) {
  $trayWidth = 340.0
  $trayMinHeight = 168.0
  $trayHeaderHeight = 42.0
  $trayRowStride = 42.0
  $trayInset = 14.0
  $trayGap = 12.0
  $trayMargin = 14.0
  $trayCloseWidth = 54.0
  $trayDissolveWidth = 76.0
  $trayButtonHeight = 24.0

  $visibleRows = [Math]::Min($memberCount, 6)
  $height = [Math]::Max($trayHeaderHeight + $visibleRows * $trayRowStride + $trayInset, $trayMinHeight)
  $anchorRight = [double]($anchor.x + $anchor.w)
  $rightCandidate = $anchorRight + $trayGap
  $leftCandidate = [double]$anchor.x - $trayGap - $trayWidth
  if ($rightCandidate + $trayWidth + $trayMargin -le $viewportW) {
    $trayX = $rightCandidate
  } else {
    $trayX = $leftCandidate
  }
  $maxX = [Math]::Max($viewportW - $trayWidth - $trayMargin, $trayMargin)
  $maxY = [Math]::Max($viewportH - $height - $trayMargin, $trayMargin)
  $trayX = Clamp-Double $trayX $trayMargin $maxX
  $trayY = Clamp-Double ([double]$anchor.y) $trayMargin $maxY

  $dissolveX = $trayX + $trayWidth - $trayInset - $trayCloseWidth - $trayGap - $trayDissolveWidth
  $dissolveY = $trayY + 9.0
  return [ordered]@{
    x = [int][Math]::Round($dissolveX + $trayDissolveWidth / 2.0)
    y = [int][Math]::Round($dissolveY + $trayButtonHeight / 2.0)
    tray = [ordered]@{ x=$trayX; y=$trayY; width=$trayWidth; height=$height }
    dissolve = [ordered]@{ x=$dissolveX; y=$dissolveY; width=$trayDissolveWidth; height=$trayButtonHeight }
  }
}

function Post-Quit($win) {
  [void][NativeProof0608Merge]::PostMessageW([IntPtr]$win.hwnd, [NativeProof0608Merge]::WM_HOTKEY, [UIntPtr]([uint64]16973), [IntPtr]::Zero)
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
$dissolveClick = $null
$stderrAll = ''
$stdoutAll = ''
$processExitedAfterQuitHotkey = $false
$stackBefore = @()
$stackAfterMerge = @()
$stackAfterDissolve = @()
$geomBefore = @()
$geomAfterMerge = @()
$geomAfterDissolve = @()

try {
  $stackBefore = Dump-Example 'dump_stack_summary' '01-stack-before.tsv'
  $geomBefore = Dump-Example 'dump_zone_geometry' '02-geometry-before.tsv'
  $zone4Before = Parse-ZoneGeometry $geomBefore 4
  $zone5Before = Parse-ZoneGeometry $geomBefore 5

  $proc = Start-Target
  $main = Wait-Window $proc.Id 'BentoNanoShell' 10000
  if (-not $main) { throw 'main window not found' }
  Start-Sleep -Milliseconds 1200
  Force-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '03-main-before-merge.png') | Out-Null

  $stage = 'merge-zone-4-onto-zone-5'
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0608Merge]::WM_LBUTTONDOWN) 120 360 'send' 120))
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0608Merge]::WM_MOUSEMOVE) 500 400 'send' 350))
  Force-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '04-zone4-over-zone5-in-flight.png') | Out-Null
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0608Merge]::WM_LBUTTONUP) 500 400 'send' 900))
  Force-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '05-after-merge-stack.png') | Out-Null
  $stackAfterMerge = Dump-Example 'dump_stack_summary' '06-stack-after-merge.tsv'
  $geomAfterMerge = Dump-Example 'dump_zone_geometry' '07-geometry-after-merge.tsv'
  $zone4AfterMerge = Parse-ZoneGeometry $geomAfterMerge 4
  $zone5AfterMerge = Parse-ZoneGeometry $geomAfterMerge 5

  $stage = 'open-stack-tray'
  # Zone 5 is a collapsed stack anchor after the merge. Its clickable region is
  # the painted pill, not the full stored body rect; keep the right-click inside
  # the medium pill (x=424..~626, y=332..380 for the benchmark scene).
  # The app uses TrackPopupMenu(TPM_RETURNCMD). With a synthetic WM_RBUTTONUP
  # and the cursor left at the menu origin, Windows can immediately return the
  # row under the cursor (observed: Auto organize, id=3) before the proof can
  # enumerate the menu HWND. Move the cursor away after posting while preserving
  # the lParam hit point, then select Open stack tray through the real WM_COMMAND
  # path below.
  [void]$clicks.Add((Post-ClientMessageThenMoveCursor $main ([NativeProof0608Merge]::WM_RBUTTONUP) 500 356 24 24 350))
  $popup = Wait-Window $proc.Id '#32768' 5000
  if (-not $popup) { throw 'zone context popup not found' }
  Save-WindowShot $popup (Join-Path $proofDir '08-zone5-context-menu.png') | Out-Null
  $menuItems = Get-PopupMenuItems $popup
  $menuItems | ConvertTo-Json -Depth 8 | Out-File -FilePath (Join-Path $proofDir '08-zone5-context-menu-items.json') -Encoding utf8
  $openStack = $menuItems | Where-Object { $_.id -eq 12 -or $_.label -eq 'Open stack tray' } | Select-Object -First 1
  if (-not $openStack) { throw 'Open stack tray menu item not found' }
  $menuCommand = Send-ZoneMenuCommand $main $popup $openStack
  Force-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '09-stack-tray-open-before-dissolve.png') | Out-Null

  $stage = 'click-dissolve'
  $viewportW = [int]$main.rect.width
  $viewportH = [int]$main.rect.height
  $dissolveClick = Get-DissolveClick $zone5AfterMerge 2 $viewportW $viewportH
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0608Merge]::WM_LBUTTONDOWN) $dissolveClick.x $dissolveClick.y 'send' 120))
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0608Merge]::WM_LBUTTONUP) $dissolveClick.x $dissolveClick.y 'send' 1000))
  Force-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '10-after-dissolve-scattered.png') | Out-Null
  $stackAfterDissolve = Dump-Example 'dump_stack_summary' '11-stack-after-dissolve.tsv'
  $geomAfterDissolve = Dump-Example 'dump_zone_geometry' '12-geometry-after-dissolve.tsv'
  $zone4AfterDissolve = Parse-ZoneGeometry $geomAfterDissolve 4
  $zone5AfterDissolve = Parse-ZoneGeometry $geomAfterDissolve 5

  $stage = 'quit'
  Post-Quit $main
  $processExitedAfterQuitHotkey = [bool](Wait-Condition { $proc.HasExited } 5000)
  if (-not $processExitedAfterQuitHotkey) { throw 'process did not exit after production quit hotkey' }
  $proc.WaitForExit(3000) | Out-Null
  $stderrAll = $proc.StandardError.ReadToEnd()
  $stdoutAll = $proc.StandardOutput.ReadToEnd()
  $stderrAll | Out-File -FilePath $stderrPath -Encoding utf8
  $stdoutAll | Out-File -FilePath $stdoutPath -Encoding utf8

  $initialStackOk = (Test-StackRow $stackBefore 1 '' '2,3') -and (Test-StackRow $stackBefore 4 '' '') -and (Test-StackRow $stackBefore 5 '' '')
  $mergeStackOk = (Test-StackRow $stackAfterMerge 5 '' '4') -and (Test-StackRow $stackAfterMerge 4 '5' '')
  $dissolveStackOk = (Test-StackRow $stackAfterDissolve 4 '' '') -and (Test-StackRow $stackAfterDissolve 5 '' '')
  $scatterOk = $zone4AfterDissolve -and $zone5AfterDissolve -and (($zone4AfterDissolve.x -ne $zone5AfterDissolve.x) -or ($zone4AfterDissolve.y -ne $zone5AfterDissolve.y))
  $viewportOk = (Test-WithinViewport $zone4AfterDissolve $viewportW $viewportH) -and (Test-WithinViewport $zone5AfterDissolve $viewportW $viewportH)
  $logs = [ordered]@{
    stack_zone_5_4 = $stderrAll.Contains('stack: StackZone anchor=5 child=4')
    open_stack_tray_5 = $stderrAll.Contains('stack: OpenStackTray anchor=5')
    dissolve_stack_5 = $stderrAll.Contains('stack: DissolveStack anchor=5')
    zone_menu_command_5 = $stderrAll.Contains('zone_menu: wm_command=12 zone_id=5')
    tray_registered = $stderrAll.Contains('tray: NIM_ADD registered')
  }

  $summary = [ordered]@{
    status = if ($initialStackOk -and $mergeStackOk -and $dissolveStackOk -and $scatterOk -and $viewportOk -and $logs.stack_zone_5_4 -and $logs.open_stack_tray_5 -and $logs.dissolve_stack_5) { 'ok' } else { 'failed' }
    stage = 'completed'
    exe = $proofExe
    source_exe = $sourceExe
    state_dir = $stateDir
    main_window = Convert-WindowForJson $main
    menu_command = $menuCommand
    dissolve_click = $dissolveClick
    stack = [ordered]@{
      initial_zone1_members_2_3_and_4_5_independent = [bool]$initialStackOk
      after_merge_zone5_members_4 = [bool]$mergeStackOk
      after_dissolve_zone4_5_independent = [bool]$dissolveStackOk
    }
    geometry = [ordered]@{
      zone4_before = $zone4Before
      zone5_before = $zone5Before
      zone4_after_merge = $zone4AfterMerge
      zone5_after_merge = $zone5AfterMerge
      zone4_after_dissolve = $zone4AfterDissolve
      zone5_after_dissolve = $zone5AfterDissolve
      released_zones_not_overlapped_exactly = [bool]$scatterOk
      released_zones_within_viewport = [bool]$viewportOk
    }
    logs = $logs
    clicks = @($clicks.ToArray())
    screenshots = @(
      '03-main-before-merge.png',
      '04-zone4-over-zone5-in-flight.png',
      '05-after-merge-stack.png',
      '08-zone5-context-menu.png',
      '09-stack-tray-open-before-dissolve.png',
      '10-after-dissolve-scattered.png'
    )
    process_exited_after_quit_hotkey = $processExitedAfterQuitHotkey
  }
  $summary | ConvertTo-Json -Depth 12 | Out-File -FilePath $summaryPath -Encoding utf8
  if ($summary.status -ne 'ok') { throw 'runtime proof assertions failed; see summary.json' }
} catch {
  $message = $_.Exception.Message
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
    menu_command = $menuCommand
    dissolve_click = $dissolveClick
    clicks = @($clicks.ToArray())
  }
  $summary | ConvertTo-Json -Depth 12 | Out-File -FilePath $summaryPath -Encoding utf8
  throw
}
