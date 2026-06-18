$ErrorActionPreference = 'Stop'

$root = 'D:\Desktop\CREATOR FOUR'
$nano = Join-Path $root 'bentodesk-nano'
$sourceExe = Join-Path $nano 'target\x86_64-pc-windows-msvc\debug\bento-nano-shell.exe'
$stateDir = Join-Path $nano 'runtime-proof-0617-item-drag-preview-layering-state'
$proofDir = Join-Path $nano 'runtime-proof-0617-item-drag-preview-layering-try'
$proofExe = Join-Path $proofDir 'bento-nano-shell.exe'
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
  Where-Object { $_.Name -ne 'runtime-proof-0617-item-drag-preview-layering-run.ps1' } |
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
public static class NativeProof0617ItemDrag {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr SendMessageTimeoutW(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
  [DllImport("user32.dll")] public static extern bool InvalidateRect(IntPtr hWnd, IntPtr lpRect, bool bErase);
  [DllImport("user32.dll")] public static extern bool UpdateWindow(IntPtr hWnd);
  public const uint SMTO_ABORTIFHUNG = 0x0002;
  public const uint WM_PAINT = 0x000F;
  public const uint WM_HOTKEY = 0x0312;
  public const uint WM_MOUSEMOVE = 0x0200;
  public const uint WM_LBUTTONDOWN = 0x0201;
  public const uint WM_LBUTTONUP = 0x0202;
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
    client = [ordered]@{
      width = [int]$win.client.width
      height = [int]$win.client.height
    }
  }
}

function Get-WindowsForPid([int]$processId) {
  $items = New-Object System.Collections.ArrayList
  $cb = [NativeProof0617ItemDrag+EnumWindowsProc]{
    param([IntPtr]$hwnd, [IntPtr]$lparam)
    [uint32]$wpid = 0
    [void][NativeProof0617ItemDrag]::GetWindowThreadProcessId($hwnd, [ref]$wpid)
    if ($wpid -eq [uint32]$processId) {
      $class = New-Object System.Text.StringBuilder 256
      $title = New-Object System.Text.StringBuilder 256
      [void][NativeProof0617ItemDrag]::GetClassName($hwnd, $class, $class.Capacity)
      [void][NativeProof0617ItemDrag]::GetWindowText($hwnd, $title, $title.Capacity)
      $rect = New-Object NativeProof0617ItemDrag+RECT
      $client = New-Object NativeProof0617ItemDrag+RECT
      [void][NativeProof0617ItemDrag]::GetWindowRect($hwnd, [ref]$rect)
      [void][NativeProof0617ItemDrag]::GetClientRect($hwnd, [ref]$client)
      [void]$items.Add([pscustomobject]@{
        hwnd = $hwnd.ToInt64()
        class = $class.ToString()
        title = $title.ToString()
        visible = [NativeProof0617ItemDrag]::IsWindowVisible($hwnd)
        dpi = [NativeProof0617ItemDrag]::GetDpiForWindow($hwnd)
        rect = [pscustomobject]@{
          left=$rect.Left; top=$rect.Top; right=$rect.Right; bottom=$rect.Bottom
          width=($rect.Right-$rect.Left); height=($rect.Bottom-$rect.Top)
        }
        client = [pscustomobject]@{
          width=($client.Right-$client.Left); height=($client.Bottom-$client.Top)
        }
      })
    }
    return $true
  }
  [void][NativeProof0617ItemDrag]::EnumWindows($cb, [IntPtr]::Zero)
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

function Send-ClientMessage($win, [uint32]$msg, [double]$clientXValue, [double]$clientYValue, [string]$mode = 'send', [int]$sleepMs = 180) {
  $displayScale = [Math]::Max(1.0, ([double]$win.dpi) / 96.0)
  $clientX = [int][Math]::Round($clientXValue)
  $clientY = [int][Math]::Round($clientYValue)
  [void][NativeProof0617ItemDrag]::SetForegroundWindow([IntPtr]$win.hwnd)
  [void][NativeProof0617ItemDrag]::SetCursorPos([int]($win.rect.left + $clientX), [int]($win.rect.top + $clientY))
  Start-Sleep -Milliseconds 40
  $lp = New-LParam $clientX $clientY
  $timedOut = $false
  $sendResult = $null
  if ($mode -eq 'post') {
    [void][NativeProof0617ItemDrag]::PostMessageW([IntPtr]$win.hwnd, $msg, [UIntPtr]::Zero, $lp)
  } else {
    $nativeResult = [UIntPtr]::Zero
    $sendResult = [NativeProof0617ItemDrag]::SendMessageTimeoutW(
      [IntPtr]$win.hwnd,
      $msg,
      [UIntPtr]::Zero,
      $lp,
      [NativeProof0617ItemDrag]::SMTO_ABORTIFHUNG,
      2500,
      [ref]$nativeResult
    )
    if ($sendResult -eq [IntPtr]::Zero) {
      $timedOut = $true
    }
  }
  Start-Sleep -Milliseconds $sleepMs
  if ($timedOut) {
    throw "SendMessageTimeout timed out for msg=$msg client=($clientX,$clientY) mode=$mode"
  }
  return [ordered]@{
    msg=$msg
    client_x_value=[double]$clientXValue
    client_y_value=[double]$clientYValue
    client_x=$clientX
    client_y=$clientY
    dpi=[int]$win.dpi
    display_scale=[double]$displayScale
    input_coordinate_space='raw-client'
    mode=$mode
    send_result=if ($sendResult -eq $null) { $null } else { $sendResult.ToInt64() }
  }
}

function Request-Paint($win) {
  if (-not $win) { return }
  [void][NativeProof0617ItemDrag]::InvalidateRect([IntPtr]$win.hwnd, [IntPtr]::Zero, $false)
  Start-Sleep -Milliseconds 180
}

function Save-WindowShot($win, [string]$path) {
  if (-not $win) { return $false }
  $w = [Math]::Max(1, [int]$win.rect.width)
  $h = [Math]::Max(1, [int]$win.rect.height)
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  try {
    $g.CopyFromScreen([int]$win.rect.left, [int]$win.rect.top, 0, 0, [System.Drawing.Size]::new($w, $h))
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    return $true
  } finally {
    $g.Dispose()
    $bmp.Dispose()
  }
}

function Analyze-ImageDelta([string]$beforePath, [string]$afterPath, [double]$scale, [hashtable]$logicalRect) {
  $before = [System.Drawing.Bitmap]::new($beforePath)
  $after = [System.Drawing.Bitmap]::new($afterPath)
  try {
    $x0 = [Math]::Max(0, [int][Math]::Round($logicalRect.x * $scale))
    $y0 = [Math]::Max(0, [int][Math]::Round($logicalRect.y * $scale))
    $w = [Math]::Min($before.Width - $x0, [int][Math]::Round($logicalRect.width * $scale))
    $h = [Math]::Min($before.Height - $y0, [int][Math]::Round($logicalRect.height * $scale))
    if ($w -le 0 -or $h -le 0) { throw "invalid scan rect ${x0},${y0} ${w}x${h}" }
    [int64]$sum = 0
    [int]$changed = 0
    [int]$maxDelta = 0
    for ($yy = $y0; $yy -lt ($y0 + $h); $yy++) {
      for ($xx = $x0; $xx -lt ($x0 + $w); $xx++) {
        $a = $before.GetPixel($xx, $yy)
        $b = $after.GetPixel($xx, $yy)
        $delta = [Math]::Abs([int]$a.R - [int]$b.R) + [Math]::Abs([int]$a.G - [int]$b.G) + [Math]::Abs([int]$a.B - [int]$b.B)
        $sum += $delta
        if ($delta -ge 18) { $changed++ }
        if ($delta -gt $maxDelta) { $maxDelta = $delta }
      }
    }
    $pixels = $w * $h
    return [ordered]@{
      logical_rect = [ordered]@{
        x = [double]$logicalRect.x
        y = [double]$logicalRect.y
        width = [double]$logicalRect.width
        height = [double]$logicalRect.height
      }
      physical_rect = [ordered]@{ x = $x0; y = $y0; width = $w; height = $h }
      pixels = $pixels
      changed_pixels = $changed
      changed_ratio = [Math]::Round(($changed / [double]$pixels), 4)
      mean_rgb_delta = [Math]::Round(($sum / [double]$pixels), 2)
      max_rgb_delta = $maxDelta
    }
  } finally {
    $before.Dispose()
    $after.Dispose()
  }
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

function Post-Quit($win) {
  [void][NativeProof0617ItemDrag]::PostMessageW([IntPtr]$win.hwnd, [NativeProof0617ItemDrag]::WM_HOTKEY, [UIntPtr]([uint64]16973), [IntPtr]::Zero)
  Start-Sleep -Milliseconds 200
}

function Start-Target {
  $prevStateDir = [Environment]::GetEnvironmentVariable('BENTODESK_NANO_STATE_DIR', 'Process')
  $prevDragProofLog = [Environment]::GetEnvironmentVariable('BENTODESK_NANO_DRAG_PROOF_LOG', 'Process')
  try {
    [Environment]::SetEnvironmentVariable('BENTODESK_NANO_STATE_DIR', $stateDir, 'Process')
    [Environment]::SetEnvironmentVariable('BENTODESK_NANO_DRAG_PROOF_LOG', '1', 'Process')
    return Start-Process `
      -FilePath $proofExe `
      -WorkingDirectory $proofDir `
      -PassThru `
      -RedirectStandardOutput $stdoutPath `
      -RedirectStandardError $stderrPath
  } finally {
    [Environment]::SetEnvironmentVariable('BENTODESK_NANO_STATE_DIR', $prevStateDir, 'Process')
    [Environment]::SetEnvironmentVariable('BENTODESK_NANO_DRAG_PROOF_LOG', $prevDragProofLog, 'Process')
  }
}

$stage = 'started'
$proc = $null
$main = $null
$clicks = New-Object System.Collections.ArrayList
$stderrAll = ''
$stdoutAll = ''
$processExitedAfterQuitHotkey = $false

try {
  $itemsBefore = Dump-Example 'dump_zone_item_grid' '01-item-grid-before.tsv'

  $proc = Start-Target
  $main = Wait-Window $proc.Id 'BentoNanoShell' 10000
  if (-not $main) { throw 'main window not found' }
  Start-Sleep -Milliseconds 1200
  Request-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '02-main-collapsed.png') | Out-Null

  $stage = 'open-zone-4'
  $zone4Select = [ordered]@{ zone_id = 4; x = 106; y = 356 }
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0617ItemDrag]::WM_MOUSEMOVE) $zone4Select.x $zone4Select.y 'send' 1100))
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0617ItemDrag]::WM_LBUTTONDOWN) $zone4Select.x $zone4Select.y 'send' 120))
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0617ItemDrag]::WM_LBUTTONUP) $zone4Select.x $zone4Select.y 'send' 500))
  Request-Paint $main
  $beforeDragPath = Join-Path $proofDir '03-zone4-expanded-before-item-drag.png'
  Save-WindowShot $main $beforeDragPath | Out-Null

  $stage = 'item-drag'
  $dragStartWrite = (Get-Item -LiteralPath $zonesPath).LastWriteTimeUtc
  $source = [ordered]@{ zone_id = 4; expected_item_id = 2; x = 261; y = 427 }
  $target = [ordered]@{ zone_id = 4; occupied_item_hint = 3; x = 335; y = 458 }
  $previewScan = @{ x = 306.0; y = 392.0; width = 58.0; height = 40.0 }

  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0617ItemDrag]::WM_MOUSEMOVE) $source.x $source.y 'send' 220))
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0617ItemDrag]::WM_LBUTTONDOWN) $source.x $source.y 'send' 260))
  $dragPath = @(
    @(276, 433),
    @(302, 444),
    @(335, 458)
  )
  foreach ($point in $dragPath) {
    [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0617ItemDrag]::WM_MOUSEMOVE) $point[0] $point[1] 'send' 180))
  }
  Start-Sleep -Milliseconds 500
  Request-Paint $main
  $inFlightPath = Join-Path $proofDir '04-item-drag-occupied-target-in-flight.png'
  Save-WindowShot $main $inFlightPath | Out-Null
  $dragMidWrite = (Get-Item -LiteralPath $zonesPath).LastWriteTimeUtc

  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0617ItemDrag]::WM_LBUTTONUP) $target.x $target.y 'send' 900))
  Request-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '05-item-drag-after-release.png') | Out-Null
  $dragReleaseWrite = (Get-Item -LiteralPath $zonesPath).LastWriteTimeUtc
  $itemsAfter = Dump-Example 'dump_zone_item_grid' '06-item-grid-after.tsv'

  $stage = 'quit'
  Post-Quit $main
  $processExitedAfterQuitHotkey = [bool](Wait-Condition { $proc.HasExited } 5000)
  if (-not $processExitedAfterQuitHotkey) { throw 'process did not exit after production quit hotkey' }
  $proc.WaitForExit(3000) | Out-Null
  $stderrAll = if (Test-Path -LiteralPath $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw } else { '' }
  $stdoutAll = if (Test-Path -LiteralPath $stdoutPath) { Get-Content -LiteralPath $stdoutPath -Raw } else { '' }

  $scale = ([double]$main.dpi) / 96.0
  $scan = Analyze-ImageDelta $beforeDragPath $inFlightPath $scale $previewScan
  $itemDownLog = [regex]::Match($stderrAll, 'items: drag-proof lbutton_down item zone_id=(\d+) item_id=(\d+)')
  $mouseMoveLogCount = ([regex]::Matches($stderrAll, 'items: drag-proof mouse_move')).Count
  $startedExternal = $stderrAll.Contains('items: drag-proof starting_external')
  $noWriteDuringDrag = ($dragMidWrite -eq $dragStartWrite)
  $writeAfterRelease = ($dragReleaseWrite -gt $dragMidWrite)
  $previewVisible = ($scan.changed_ratio -ge 0.10 -and $scan.mean_rgb_delta -ge 8.0 -and $scan.max_rgb_delta -ge 35)
  $itemDownMatches = $itemDownLog.Success -and [int]$itemDownLog.Groups[1].Value -eq 4

  $summary = [ordered]@{
    status = if ($itemDownMatches -and $mouseMoveLogCount -ge 3 -and -not $startedExternal -and $noWriteDuringDrag -and $previewVisible -and $processExitedAfterQuitHotkey) { 'ok' } else { 'failed' }
    stage = 'completed'
    exe = $proofExe
    source_exe = $sourceExe
    state_dir = $stateDir
    main_window = Convert-WindowForJson $main
    item_drag = [ordered]@{
      selected_zone = $zone4Select
      source = $source
      target = $target
      drag_path = $dragPath
      lbutton_down_log = if ($itemDownLog.Success) { $itemDownLog.Value } else { $null }
      mousemove_log_count = [int]$mouseMoveLogCount
      started_external = [bool]$startedExternal
      zones_bin_write_time_utc_before = $dragStartWrite.ToString('o')
      zones_bin_write_time_utc_mid_drag = $dragMidWrite.ToString('o')
      zones_bin_write_time_utc_after_release = $dragReleaseWrite.ToString('o')
      no_write_during_drag = [bool]$noWriteDuringDrag
      write_after_release = [bool]$writeAfterRelease
    }
    preview_visibility = [ordered]@{
      before_image = '03-zone4-expanded-before-item-drag.png'
      in_flight_image = '04-item-drag-occupied-target-in-flight.png'
      scan = $scan
      visible = [bool]$previewVisible
    }
    logs = [ordered]@{
      item_lbutton_down = [bool]$itemDownLog.Success
      item_lbutton_down_zone4 = [bool]$itemDownMatches
      mousemove_count = [int]$mouseMoveLogCount
      external_drag_started = [bool]$startedExternal
      tray_registered = $stderrAll.Contains('tray: NIM_ADD registered')
    }
    clicks = @($clicks.ToArray())
    dumps = [ordered]@{
      item_grid_before = '01-item-grid-before.tsv'
      item_grid_after = '06-item-grid-after.tsv'
      before_row_count = [int]$itemsBefore.Count
      after_row_count = [int]$itemsAfter.Count
    }
    screenshots = @(
      '02-main-collapsed.png',
      '03-zone4-expanded-before-item-drag.png',
      '04-item-drag-occupied-target-in-flight.png',
      '05-item-drag-after-release.png'
    )
    process_exited_after_quit_hotkey = $processExitedAfterQuitHotkey
  }
  $summary | ConvertTo-Json -Depth 14 | Out-File -FilePath $summaryPath -Encoding utf8
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
    if ($main) { Post-Quit $main }
    if (-not $proc.WaitForExit(2000)) {
      $proc.Kill()
    }
  }
  if ($proc) {
    try {
      if (Test-Path -LiteralPath $stderrPath) { $stderrAll = Get-Content -LiteralPath $stderrPath -Raw }
    } catch {}
    try {
      if (Test-Path -LiteralPath $stdoutPath) { $stdoutAll = Get-Content -LiteralPath $stdoutPath -Raw }
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
    partial_summary = $partialSummary
  }
  $summary | ConvertTo-Json -Depth 14 | Out-File -FilePath $summaryPath -Encoding utf8
  throw
}
