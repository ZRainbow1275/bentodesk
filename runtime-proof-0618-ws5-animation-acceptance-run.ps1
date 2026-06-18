$ErrorActionPreference = 'Stop'

$root = 'D:\Desktop\CREATOR FOUR'
$nano = Join-Path $root 'bentodesk-nano'
$sourceExe = Join-Path $nano 'target\x86_64-pc-windows-msvc\debug\bento-nano-shell.exe'
$stateDir = Join-Path $nano 'runtime-proof-0618-ws5-animation-acceptance-state'
$proofDir = Join-Path $nano 'runtime-proof-0618-ws5-animation-acceptance-try'
$proofExe = Join-Path $proofDir 'bento-nano-shell-ws5-animation-proof.exe'
$itemRoot = Join-Path $stateDir 'items'
$zonesPath = Join-Path $stateDir 'zones.bin'
$stderrPath = Join-Path $proofDir 'stderr.log'
$stdoutPath = Join-Path $proofDir 'stdout.log'
$summaryPath = Join-Path $proofDir 'summary.json'
$stateDumpPath = Join-Path $proofDir 'state-dumps.jsonl'
$frameTimingPath = Join-Path $proofDir 'frame-timing.csv'
$pixelAssertionsPath = Join-Path $proofDir 'pixel-assertions.json'
$cadenceMetricsPath = Join-Path $proofDir 'cadence-metrics.json'

function Assert-UnderPath([string]$path, [string]$parent) {
  $parentFull = [System.IO.Path]::GetFullPath($parent).TrimEnd('\') + '\'
  $pathFull = [System.IO.Path]::GetFullPath($path)
  if (-not $pathFull.StartsWith($parentFull, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "refusing path outside proof workspace: $path"
  }
}

Assert-UnderPath $stateDir $nano
Assert-UnderPath $proofDir $nano

if (-not (Test-Path -LiteralPath $sourceExe)) {
  throw "source exe not found: $sourceExe"
}

$oldProofProcess = Get-Process -Name 'bento-nano-shell-ws5-animation-proof' -ErrorAction SilentlyContinue
if ($oldProofProcess) {
  throw 'bento-nano-shell-ws5-animation-proof is already running; stop the old proof process before rerunning'
}

New-Item -ItemType Directory -Force -Path $proofDir | Out-Null
Get-ChildItem -LiteralPath $proofDir -Force -ErrorAction SilentlyContinue |
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
if (Test-Path -LiteralPath $stateDir) {
  Remove-Item -LiteralPath $stateDir -Recurse -Force
}
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
public static class NativeProof0618Anim {
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
  [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr SendMessageTimeoutW(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
  [DllImport("user32.dll")] public static extern bool InvalidateRect(IntPtr hWnd, IntPtr lpRect, bool bErase);
  public const uint SMTO_ABORTIFHUNG = 0x0002;
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
  $cb = [NativeProof0618Anim+EnumWindowsProc]{
    param([IntPtr]$hwnd, [IntPtr]$lparam)
    [uint32]$wpid = 0
    [void][NativeProof0618Anim]::GetWindowThreadProcessId($hwnd, [ref]$wpid)
    if ($wpid -eq [uint32]$processId) {
      $class = New-Object System.Text.StringBuilder 256
      $title = New-Object System.Text.StringBuilder 256
      [void][NativeProof0618Anim]::GetClassName($hwnd, $class, $class.Capacity)
      [void][NativeProof0618Anim]::GetWindowText($hwnd, $title, $title.Capacity)
      $rect = New-Object NativeProof0618Anim+RECT
      $client = New-Object NativeProof0618Anim+RECT
      [void][NativeProof0618Anim]::GetWindowRect($hwnd, [ref]$rect)
      [void][NativeProof0618Anim]::GetClientRect($hwnd, [ref]$client)
      [void]$items.Add([pscustomobject]@{
        hwnd = $hwnd.ToInt64()
        class = $class.ToString()
        title = $title.ToString()
        visible = [NativeProof0618Anim]::IsWindowVisible($hwnd)
        dpi = [NativeProof0618Anim]::GetDpiForWindow($hwnd)
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
  [void][NativeProof0618Anim]::EnumWindows($cb, [IntPtr]::Zero)
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

function Send-ClientMessage($win, [uint32]$msg, [double]$clientXValue, [double]$clientYValue, [string]$mode = 'send', [int]$sleepMs = 120) {
  $clientX = [int][Math]::Round($clientXValue)
  $clientY = [int][Math]::Round($clientYValue)
  [void][NativeProof0618Anim]::SetForegroundWindow([IntPtr]$win.hwnd)
  [void][NativeProof0618Anim]::SetCursorPos([int]($win.rect.left + $clientX), [int]($win.rect.top + $clientY))
  Start-Sleep -Milliseconds 25
  $lp = New-LParam $clientX $clientY
  $sendResult = $null
  if ($mode -eq 'post') {
    [void][NativeProof0618Anim]::PostMessageW([IntPtr]$win.hwnd, $msg, [UIntPtr]::Zero, $lp)
  } else {
    $nativeResult = [UIntPtr]::Zero
    $sendResult = [NativeProof0618Anim]::SendMessageTimeoutW(
      [IntPtr]$win.hwnd,
      $msg,
      [UIntPtr]::Zero,
      $lp,
      [NativeProof0618Anim]::SMTO_ABORTIFHUNG,
      2500,
      [ref]$nativeResult
    )
    if ($sendResult -eq [IntPtr]::Zero) {
      throw "SendMessageTimeout timed out for msg=$msg client=($clientX,$clientY)"
    }
  }
  Start-Sleep -Milliseconds $sleepMs
  return [ordered]@{
    msg=$msg
    client_x_value=[double]$clientXValue
    client_y_value=[double]$clientYValue
    client_x=$clientX
    client_y=$clientY
    dpi=[int]$win.dpi
    input_coordinate_space='raw-client'
    mode=$mode
    send_result=if ($sendResult -eq $null) { $null } else { $sendResult.ToInt64() }
  }
}

function Request-Paint($win, [int]$sleepMs = 120) {
  if (-not $win) { return }
  [void][NativeProof0618Anim]::InvalidateRect([IntPtr]$win.hwnd, [IntPtr]::Zero, $false)
  Start-Sleep -Milliseconds $sleepMs
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
      before = [System.IO.Path]::GetFileName($beforePath)
      after = [System.IO.Path]::GetFileName($afterPath)
      logical_rect = [ordered]@{ x = [double]$logicalRect.x; y = [double]$logicalRect.y; width = [double]$logicalRect.width; height = [double]$logicalRect.height }
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

function Save-DragFrame($win, [string]$path, [string]$previousPath, [double]$scale, [hashtable]$scanRect) {
  Request-Paint $win 16
  Save-WindowShot $win $path | Out-Null
  if (-not $previousPath -or -not (Test-Path -LiteralPath $previousPath)) {
    return [ordered]@{ attempts = 1; delta = $null }
  }
  $delta = Analyze-ImageDelta $previousPath $path $scale $scanRect
  return [ordered]@{ attempts = 1; delta = $delta }
}

function Get-Stats($values) {
  $items = @($values | Where-Object { $null -ne $_ } | ForEach-Object { [double]$_ })
  if ($items.Count -eq 0) {
    return [ordered]@{ count=0; min=$null; max=$null; mean=$null; stddev=$null; p95=$null }
  }
  $sorted = @($items | Sort-Object)
  $mean = ($items | Measure-Object -Average).Average
  $variance = 0.0
  foreach ($value in $items) {
    $variance += [Math]::Pow($value - $mean, 2)
  }
  $stddev = [Math]::Sqrt($variance / [double]$items.Count)
  $p95Index = [Math]::Min($sorted.Count - 1, [Math]::Max(0, [int][Math]::Ceiling($sorted.Count * 0.95) - 1))
  return [ordered]@{
    count = [int]$items.Count
    min = [Math]::Round([double]$sorted[0], 2)
    max = [Math]::Round([double]$sorted[$sorted.Count - 1], 2)
    mean = [Math]::Round([double]$mean, 2)
    stddev = [Math]::Round([double]$stddev, 2)
    p95 = [Math]::Round([double]$sorted[$p95Index], 2)
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

function Parse-ZoneGeometry($lines, [int]$zoneId) {
  foreach ($line in $lines) {
    if ($line -match "^$zoneId`t(?<title>.*?)`t(?<x>-?\d+)`t(?<y>-?\d+)`t(?<w>-?\d+)`t(?<h>-?\d+)") {
      return [ordered]@{
        zone_id = $zoneId
        title = $Matches.title
        x = [int]$Matches.x
        y = [int]$Matches.y
        w = [int]$Matches.w
        h = [int]$Matches.h
      }
    }
  }
  return $null
}

function Parse-AnimStateLine([string]$line) {
  $row = [ordered]@{ raw = $line }
  foreach ($m in [regex]::Matches($line, '(\w+)=([^ ]+)')) {
    $row[$m.Groups[1].Value] = $m.Groups[2].Value
  }
  return $row
}

function Assert-DragStateIdle($states) {
  foreach ($s in $states) {
    if ($s.active_drag -ne 'zone') { return $false }
    if ($s.zone_drag -eq 'none') { return $false }
    if ($s.hovered_zone -ne 'none') { return $false }
    if ($s.pill_anim_zone -ne 'none') { return $false }
    if ($s.stack_bloom_anchor -ne 'none') { return $false }
    if ($s.hover_scheduler_pending -ne 'false') { return $false }
    if ($s.item_hover_active -ne 'false') { return $false }
    if ($s.highlight_targets -ne '0') { return $false }
    if ($s.highlight_pulses -ne '0') { return $false }
  }
  return $true
}

function Assert-ItemDragStateIdle($states) {
  foreach ($s in $states) {
    if ($s.active_drag -ne 'item') { return $false }
    if ($s.hovered_zone -ne 'none') { return $false }
    if ($s.stack_bloom_anchor -ne 'none') { return $false }
    if ($s.hover_scheduler_pending -ne 'false') { return $false }
    if ($s.highlight_targets -ne '0') { return $false }
    if ($s.highlight_pulses -ne '0') { return $false }
  }
  return $true
}

function Test-MotionMonotonic($moves) {
  if ($moves.Count -lt 10) { return $false }
  for ($i = 1; $i -lt $moves.Count; $i++) {
    if ($moves[$i].x -lt $moves[$i - 1].x) { return $false }
    if ($moves[$i].y -lt $moves[$i - 1].y) { return $false }
    if ($moves[$i].x -eq $moves[$i - 1].x -and $moves[$i].y -eq $moves[$i - 1].y) { return $false }
  }
  return $true
}

function Get-UniqueSequentialValues($values) {
  $items = New-Object System.Collections.ArrayList
  foreach ($value in $values) {
    if ($items.Count -eq 0 -or [int]$items[$items.Count - 1] -ne [int]$value) {
      [void]$items.Add([int]$value)
    }
  }
  return @($items.ToArray())
}

function Post-Quit($win) {
  [void][NativeProof0618Anim]::PostMessageW([IntPtr]$win.hwnd, [NativeProof0618Anim]::WM_HOTKEY, [UIntPtr]([uint64]16973), [IntPtr]::Zero)
  Start-Sleep -Milliseconds 200
}

function Start-Target {
  $prevStateDir = [Environment]::GetEnvironmentVariable('BENTODESK_NANO_STATE_DIR', 'Process')
  $prevDragProofLog = [Environment]::GetEnvironmentVariable('BENTODESK_NANO_DRAG_PROOF_LOG', 'Process')
  $prevAnimProofLog = [Environment]::GetEnvironmentVariable('BENTODESK_NANO_ANIM_PROOF_LOG', 'Process')
  try {
    [Environment]::SetEnvironmentVariable('BENTODESK_NANO_STATE_DIR', $stateDir, 'Process')
    [Environment]::SetEnvironmentVariable('BENTODESK_NANO_DRAG_PROOF_LOG', '1', 'Process')
    [Environment]::SetEnvironmentVariable('BENTODESK_NANO_ANIM_PROOF_LOG', '1', 'Process')
    return Start-Process `
      -FilePath $proofExe `
      -WorkingDirectory $proofDir `
      -PassThru `
      -RedirectStandardOutput $stdoutPath `
      -RedirectStandardError $stderrPath
  } finally {
    [Environment]::SetEnvironmentVariable('BENTODESK_NANO_STATE_DIR', $prevStateDir, 'Process')
    [Environment]::SetEnvironmentVariable('BENTODESK_NANO_DRAG_PROOF_LOG', $prevDragProofLog, 'Process')
    [Environment]::SetEnvironmentVariable('BENTODESK_NANO_ANIM_PROOF_LOG', $prevAnimProofLog, 'Process')
  }
}

$stage = 'started'
$proc = $null
$main = $null
$clicks = New-Object System.Collections.ArrayList
$screenshots = New-Object System.Collections.ArrayList
$dragFrameImages = New-Object System.Collections.ArrayList
$dragFrameTimings = New-Object System.Collections.ArrayList
$stderrAll = ''
$stdoutAll = ''
$processExitedAfterQuitHotkey = $false

try {
  $geomBefore = Dump-Example 'dump_zone_geometry' '01-geometry-before.tsv'
  $zone1Before = Parse-ZoneGeometry $geomBefore 1
  if (-not $zone1Before) { throw 'zone 1 geometry before drag not found' }

  $proc = Start-Target
  $main = Wait-Window $proc.Id 'BentoNanoShell' 10000
  if (-not $main) { throw 'main window not found' }
  Start-Sleep -Milliseconds 1200
  Request-Paint $main
  $shot = Join-Path $proofDir '00-baseline-collapsed.png'
  Save-WindowShot $main $shot | Out-Null
  [void]$screenshots.Add('00-baseline-collapsed.png')

  $stage = 'hover-bloom'
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0618Anim]::WM_MOUSEMOVE) 120 84 'send' 650))
  Request-Paint $main
  $shot = Join-Path $proofDir '01-hover-bloom-pre-drag.png'
  Save-WindowShot $main $shot | Out-Null
  [void]$screenshots.Add('01-hover-bloom-pre-drag.png')

  $stage = 'zone-drag'
  $dragStartWrite = (Get-Item -LiteralPath $zonesPath).LastWriteTimeUtc
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0618Anim]::WM_LBUTTONDOWN) 120 84 'send' 140))
  Request-Paint $main
  $shot = Join-Path $proofDir '02-lbuttondown-pre-threshold.png'
  Save-WindowShot $main $shot | Out-Null
  [void]$screenshots.Add('02-lbuttondown-pre-threshold.png')

  $dragPath = New-Object System.Collections.ArrayList
  for ($step = 1; $step -le 30; $step++) {
    $x = [int](120 + ($step * 6))
    $y = [int](84 + ($step * 4))
    [void]$dragPath.Add(@($x, $y))
  }
  $scale = ([double]$main.dpi) / 96.0
  $dragScanRect = @{ x = 40.0; y = 50.0; width = 390.0; height = 190.0 }
  $dragClock = [Diagnostics.Stopwatch]::StartNew()
  $frameIndex = 0
  foreach ($point in $dragPath) {
    $frameIndex++
    $inputAt = $dragClock.ElapsedMilliseconds
    [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0618Anim]::WM_MOUSEMOVE) $point[0] $point[1] 'send' 8))
    $captureAt = $dragClock.ElapsedMilliseconds
    $name = "03-drag-frame-{0:D2}.png" -f $frameIndex
    $path = Join-Path $proofDir $name
    $previousPath = if ($dragFrameImages.Count -gt 0) { Join-Path $proofDir $dragFrameImages[$dragFrameImages.Count - 1] } else { $null }
    $capture = Save-DragFrame $main $path $previousPath $scale $dragScanRect
    [void]$dragFrameImages.Add($name)
    [void]$screenshots.Add($name)
    [void]$dragFrameTimings.Add([ordered]@{
      seq = $frameIndex
      input_x = [int]$point[0]
      input_y = [int]$point[1]
      input_elapsed_ms = [int]$inputAt
      capture_elapsed_ms = [int]$captureAt
      capture_attempts = [int]$capture.attempts
      image = $name
    })
  }
  Request-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '04-drag-in-flight.png') | Out-Null
  [void]$screenshots.Add('04-drag-in-flight.png')
  $dragMidWrite = (Get-Item -LiteralPath $zonesPath).LastWriteTimeUtc

  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0618Anim]::WM_LBUTTONUP) 275 180 'send' 900))
  Request-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '05-drag-after-release.png') | Out-Null
  [void]$screenshots.Add('05-drag-after-release.png')
  $dragReleaseWrite = (Get-Item -LiteralPath $zonesPath).LastWriteTimeUtc
  $geomAfterDrag = Dump-Example 'dump_zone_geometry' '06-geometry-after-drag.tsv'
  $zone1AfterDrag = Parse-ZoneGeometry $geomAfterDrag 1

  $stage = 'zone4-morph-and-item-drag'
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0618Anim]::WM_MOUSEMOVE) 106 356 'send' 1200))
  Request-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '07-zone4-expanded-morph-stable.png') | Out-Null
  [void]$screenshots.Add('07-zone4-expanded-morph-stable.png')

  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0618Anim]::WM_LBUTTONDOWN) 261 427 'send' 220))
  $itemPath = @(
    @(276, 433),
    @(302, 444),
    @(335, 458)
  )
  foreach ($point in $itemPath) {
    [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0618Anim]::WM_MOUSEMOVE) $point[0] $point[1] 'send' 130))
  }
  Request-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '08-item-drag-in-flight.png') | Out-Null
  [void]$screenshots.Add('08-item-drag-in-flight.png')
  [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0618Anim]::WM_LBUTTONUP) 335 458 'send' 900))
  Request-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '09-item-drag-after-release.png') | Out-Null
  [void]$screenshots.Add('09-item-drag-after-release.png')
  $itemsAfter = Dump-Example 'dump_zone_item_grid' '10-item-grid-after.tsv'

  $stage = 'quit'
  Post-Quit $main
  $processExitedAfterQuitHotkey = [bool](Wait-Condition { $proc.HasExited } 5000)
  if (-not $processExitedAfterQuitHotkey) { throw 'process did not exit after production quit hotkey' }
  $proc.WaitForExit(3000) | Out-Null
  $stderrAll = if (Test-Path -LiteralPath $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw } else { '' }
  $stdoutAll = if (Test-Path -LiteralPath $stdoutPath) { Get-Content -LiteralPath $stdoutPath -Raw } else { '' }

  $stateRows = @()
  foreach ($m in [regex]::Matches($stderrAll, '(?m)^anim_state: .*$')) {
    $row = Parse-AnimStateLine $m.Value
    $stateRows += [pscustomobject]$row
    ($row | ConvertTo-Json -Compress -Depth 8) | Out-File -FilePath $stateDumpPath -Append -Encoding utf8
  }

  $liveMoves = @()
  foreach ($m in [regex]::Matches($stderrAll, 'drag: live_move zone_id=(?<zone>\d+) x=(?<x>-?\d+) y=(?<y>-?\d+) now_ms=(?<now>\d+)')) {
    $liveMoves += [pscustomobject]@{
      zone_id = [int]$m.Groups['zone'].Value
      x = [int]$m.Groups['x'].Value
      y = [int]$m.Groups['y'].Value
      now_ms = [int]$m.Groups['now'].Value
    }
  }

  'seq,input_x,input_y,input_elapsed_ms,capture_elapsed_ms,capture_attempts,image' | Out-File -FilePath $frameTimingPath -Encoding utf8
  foreach ($row in $dragFrameTimings) {
    "$($row.seq),$($row.input_x),$($row.input_y),$($row.input_elapsed_ms),$($row.capture_elapsed_ms),$($row.capture_attempts),$($row.image)" |
      Out-File -FilePath $frameTimingPath -Append -Encoding utf8
  }

  $scale = ([double]$main.dpi) / 96.0
  $dragScanRect = @{ x = 40.0; y = 50.0; width = 390.0; height = 190.0 }
  $frameDeltas = @()
  for ($i = 1; $i -lt $dragFrameImages.Count; $i++) {
    $before = Join-Path $proofDir $dragFrameImages[$i - 1]
    $after = Join-Path $proofDir $dragFrameImages[$i]
    $frameDeltas += [pscustomobject](Analyze-ImageDelta $before $after $scale $dragScanRect)
  }
  $frameDeltas | ConvertTo-Json -Depth 10 | Out-File -FilePath $pixelAssertionsPath -Encoding utf8

  $zoneDragStatesForCadence = @($stateRows | Where-Object { $_.phase -eq 'zone_drag_live' })
  $captureIntervals = @()
  for ($i = 1; $i -lt $dragFrameTimings.Count; $i++) {
    $captureIntervals += ([int]$dragFrameTimings[$i].capture_elapsed_ms - [int]$dragFrameTimings[$i - 1].capture_elapsed_ms)
  }
  $liveMoveIntervals = @()
  for ($i = 1; $i -lt $liveMoves.Count; $i++) {
    $liveMoveIntervals += ([int]$liveMoves[$i].now_ms - [int]$liveMoves[$i - 1].now_ms)
  }
  $captureIntervalStats = Get-Stats $captureIntervals
  $liveMoveIntervalStats = Get-Stats $liveMoveIntervals
  $zoneDragTickTimes = Get-UniqueSequentialValues @($zoneDragStatesForCadence | ForEach-Object { [int]$_.now_ms })
  $zoneDragTickIntervals = @()
  for ($i = 1; $i -lt $zoneDragTickTimes.Count; $i++) {
    $zoneDragTickIntervals += ([int]$zoneDragTickTimes[$i] - [int]$zoneDragTickTimes[$i - 1])
  }
  $zoneDragTickStats = Get-Stats $zoneDragTickIntervals
  $pfdMs = if ($zoneDragTickStats.count -gt 0) {
    [Math]::Round(([double]$zoneDragTickStats.mean + 2.0 * [double]$zoneDragTickStats.stddev), 2)
  } else {
    $null
  }
  $jankIntervals = @($zoneDragTickIntervals | Where-Object { $_ -gt 50 })
  $jankPercent = if ($zoneDragTickIntervals.Count -gt 0) { [Math]::Round(($jankIntervals.Count * 100.0 / [double]$zoneDragTickIntervals.Count), 2) } else { 100.0 }

  $hoverBloomSeen = ($stateRows | Where-Object {
    $_.phase -eq 'hover_changed' -and $_.active_drag -eq 'none' -and $_.hovered_zone -eq '1' -and $_.stack_bloom_anchor -eq '1' -and $_.hover_scheduler_pending -eq 'false'
  }).Count -ge 1
  $zone4NormalHoverSeen = ($stateRows | Where-Object {
    $_.phase -eq 'hover_changed' -and $_.hovered_zone -eq '4' -and $_.stack_bloom_anchor -eq 'none' -and $_.active_drag -eq 'none'
  }).Count -ge 1
  $zoneDragStates = $zoneDragStatesForCadence
  $itemDragStates = @($stateRows | Where-Object { $_.phase -eq 'item_drag_live' })
  $tickSkipSeen = ($stateRows | Where-Object { $_.phase -eq 'tick_pointer_drag_skip' -and $_.active_drag -eq 'zone' }).Count -ge 1
  $releaseCleared = @($stateRows | Where-Object { $_.phase -eq 'lbutton_up_after_clear' -and $_.active_drag -eq 'none' }).Count -ge 2
  $zoneReleaseCleared = @($stateRows | Where-Object { $_.phase -eq 'lbutton_up_after_clear' -and $_.active_drag -eq 'none' -and $_.zone_drag -eq 'none' }).Count -ge 1
  $zoneDragHighlightSuppressed = $zoneDragStates.Count -ge 10 -and @($zoneDragStates | Where-Object { $_.highlight_targets -ne '0' -or $_.highlight_pulses -ne '0' }).Count -eq 0
  $itemDragHighlightSuppressed = $itemDragStates.Count -ge 1 -and @($itemDragStates | Where-Object { $_.highlight_targets -ne '0' -or $_.highlight_pulses -ne '0' }).Count -eq 0
  $stateArbitrationIdle = $zoneDragStates.Count -ge 10 -and (Assert-DragStateIdle $zoneDragStates)
  $itemStateArbitrationIdle = $itemDragStates.Count -ge 1 -and (Assert-ItemDragStateIdle $itemDragStates)
  $motionMonotonic = Test-MotionMonotonic $liveMoves
  $liveMoveLogCount = $liveMoves.Count
  $noWriteDuringDrag = ($dragMidWrite -eq $dragStartWrite)
  $writeAfterRelease = ($dragReleaseWrite -gt $dragMidWrite)
  $dragMoved = $zone1AfterDrag -and (($zone1AfterDrag.x -ne $zone1Before.x) -or ($zone1AfterDrag.y -ne $zone1Before.y))
  $nonRepeatedFrameDeltas = @($frameDeltas | Where-Object { $_.changed_ratio -ge 0.003 -and $_.mean_rgb_delta -ge 0.4 })
  $repeatedFrameDeltaCount = $frameDeltas.Count - $nonRepeatedFrameDeltas.Count
  $noRepeatedVisualFrames = $frameDeltas.Count -ge 10 -and $repeatedFrameDeltaCount -le 1
  $continuousCadenceOk = $dragFrameImages.Count -ge 30 `
    -and $frameDeltas.Count -ge 29 `
    -and $nonRepeatedFrameDeltas.Count -ge 26 `
    -and $repeatedFrameDeltaCount -le 3 `
    -and $zoneDragTickStats.count -ge 30 `
    -and $zoneDragTickStats.mean -le 18.0 `
    -and $zoneDragTickStats.p95 -le 20.0 `
    -and $pfdMs -le 22.0 `
    -and $jankPercent -eq 0.0
  $cadenceMetrics = [ordered]@{
    proof_kind = 'continuous drag internal frame-pump cadence plus screenshot delta proof'
    external_reference_basis = 'frame interval, perceived frame duration, jank/outlier count, repeated-frame ratio, and pixel delta variance'
    measurement_boundary = 'frame-pump cadence uses app anim_state tick timestamps during active zone drag; screenshot capture cadence is reported but not used as a vsync/FPS claim because CopyFromScreen+PNG is slow'
    frame_count = [int]$dragFrameImages.Count
    frame_delta_count = [int]$frameDeltas.Count
    capture_interval_ms = $captureIntervalStats
    live_move_interval_ms = $liveMoveIntervalStats
    zone_drag_tick_interval_ms = $zoneDragTickStats
    perceived_frame_duration_ms = $pfdMs
    jank_threshold_ms = 50
    jank_interval_count = [int]$jankIntervals.Count
    jank_percent = $jankPercent
    repeated_frame_delta_count = [int]$repeatedFrameDeltaCount
    non_repeated_frame_delta_count = [int]$nonRepeatedFrameDeltas.Count
    thresholds = [ordered]@{
      min_frame_count = 30
      min_frame_delta_count = 29
      min_non_repeated_frame_deltas = 26
      max_repeated_frame_deltas = 3
      min_zone_drag_tick_intervals = 30
      max_zone_drag_tick_mean_ms = 18.0
      max_zone_drag_tick_p95_ms = 20.0
      max_perceived_frame_duration_ms = 22.0
      max_jank_percent = 0.0
    }
    passed = [bool]$continuousCadenceOk
  }
  $cadenceMetrics | ConvertTo-Json -Depth 12 | Out-File -FilePath $cadenceMetricsPath -Encoding utf8
  $itemDownLog = [regex]::Match($stderrAll, 'items: drag-proof lbutton_down item zone_id=(\d+) item_id=(\d+)')
  $itemPathReached = $itemDownLog.Success -and [int]$itemDownLog.Groups[1].Value -eq 4

  $ws5AcceptanceOk = $stateArbitrationIdle `
    -and $tickSkipSeen `
    -and $zoneReleaseCleared `
    -and $noRepeatedVisualFrames `
    -and $continuousCadenceOk `
    -and $noWriteDuringDrag `
    -and $writeAfterRelease `
    -and $dragMoved `
    -and $processExitedAfterQuitHotkey
  $summaryStatusOk = $ws5AcceptanceOk

  $summary = [ordered]@{
    status = if ($summaryStatusOk) { 'ok' } else { 'failed' }
    stage = 'completed'
    exe = $proofExe
    source_exe = $sourceExe
    state_dir = $stateDir
    visual_review_required = $false
    main_window = Convert-WindowForJson $main
    ws5_acceptance = [ordered]@{
      accepted = [bool]$ws5AcceptanceOk
      scope = 'WS-5 continuous zone-drag runtime cadence and visual delta proof'
      measurement_boundary = 'Internal frame-pump cadence is measured from anim_state tick timestamps while active_drag=zone; screenshot capture proves non-repeated visible deltas but is not used as a vsync/FPS timer.'
      non_closing_limitations = @(
        'Does not close full WS-5: A3 auto-rebound proof, Tauri keyframe alignment, and hover/press delta acceptance still require dedicated gates.',
        'Copied item-drag and zone4 hover probes are retained as auxiliary diagnostics only and do not gate this cadence proof.'
      )
    }
    animation_state = [ordered]@{
      state_dump_jsonl = 'state-dumps.jsonl'
      anim_state_log_count = [int]$stateRows.Count
      hover_bloom_seen = [bool]$hoverBloomSeen
      zone4_normal_hover_seen = [bool]$zone4NormalHoverSeen
      zone_drag_state_count = [int]$zoneDragStates.Count
      state_arbitration_idle = [bool]$stateArbitrationIdle
      item_drag_state_count = [int]$itemDragStates.Count
      item_state_arbitration_idle = [bool]$itemStateArbitrationIdle
      highlight_suppressed_during_zone_drag = [bool]$zoneDragHighlightSuppressed
      highlight_suppressed_during_item_drag = [bool]$itemDragHighlightSuppressed
      tick_skip_seen = [bool]$tickSkipSeen
      release_cleared = [bool]$releaseCleared
      zone_release_cleared = [bool]$zoneReleaseCleared
    }
    drag = [ordered]@{
      before = $zone1Before
      after = $zone1AfterDrag
      moved = [bool]$dragMoved
      live_move_log_count = [int]$liveMoveLogCount
      live_moves = @($liveMoves)
      motion_monotonic = [bool]$motionMonotonic
      zones_bin_write_time_utc_before = $dragStartWrite.ToString('o')
      zones_bin_write_time_utc_mid_drag = $dragMidWrite.ToString('o')
      zones_bin_write_time_utc_after_release = $dragReleaseWrite.ToString('o')
      no_write_during_drag = [bool]$noWriteDuringDrag
      write_after_release = [bool]$writeAfterRelease
    }
    visual_motion = [ordered]@{
      frame_timing_csv = 'frame-timing.csv'
      pixel_assertions_json = 'pixel-assertions.json'
      cadence_metrics_json = 'cadence-metrics.json'
      drag_frame_count = [int]$dragFrameImages.Count
      frame_delta_count = [int]$frameDeltas.Count
      non_repeated_frame_delta_count = [int]$nonRepeatedFrameDeltas.Count
      repeated_frame_delta_count = [int]$repeatedFrameDeltaCount
      no_repeated_visual_frames = [bool]$noRepeatedVisualFrames
      continuous_drag_cadence_ok = [bool]$continuousCadenceOk
      capture_interval_ms = $captureIntervalStats
      live_move_interval_ms = $liveMoveIntervalStats
      zone_drag_tick_interval_ms = $zoneDragTickStats
      perceived_frame_duration_ms = $pfdMs
      jank_percent = $jankPercent
      drag_scan_rect = $dragScanRect
    }
    item_drag = [ordered]@{
      lbutton_down_log = if ($itemDownLog.Success) { $itemDownLog.Value } else { $null }
      path_reached = [bool]$itemPathReached
      item_grid_after_rows = [int]$itemsAfter.Count
    }
    logs = [ordered]@{
      tray_registered = $stderrAll.Contains('tray: NIM_ADD registered')
      animation_proof_log_seen = ($stateRows.Count -gt 0)
      drag_proof_log_seen = ($liveMoveLogCount -gt 0)
    }
    clicks = @($clicks.ToArray())
    screenshots = @($screenshots.ToArray())
    dumps = [ordered]@{
      geometry_before = '01-geometry-before.tsv'
      geometry_after_drag = '06-geometry-after-drag.tsv'
      item_grid_after = '10-item-grid-after.tsv'
    }
    process_exited_after_quit_hotkey = $processExitedAfterQuitHotkey
  }
  $summary | ConvertTo-Json -Depth 16 | Out-File -FilePath $summaryPath -Encoding utf8
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
  try {
    if (Test-Path -LiteralPath $stderrPath) { $stderrAll = Get-Content -LiteralPath $stderrPath -Raw }
  } catch {}
  try {
    if (Test-Path -LiteralPath $stdoutPath) { $stdoutAll = Get-Content -LiteralPath $stdoutPath -Raw }
  } catch {}
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
  $summary | ConvertTo-Json -Depth 16 | Out-File -FilePath $summaryPath -Encoding utf8
  throw
}
