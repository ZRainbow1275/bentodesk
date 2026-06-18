$ErrorActionPreference = 'Stop'

$root = 'D:\Desktop\CREATOR FOUR'
$nano = Join-Path $root 'bentodesk-nano'
$sourceExe = Join-Path $nano 'target\x86_64-pc-windows-msvc\debug\bento-nano-shell.exe'
$stateDir = Join-Path $nano 'runtime-proof-0618-ws2-appearance-closure-state'
$proofDir = Join-Path $nano 'runtime-proof-0618-ws2-appearance-closure-try'
$proofExe = Join-Path $proofDir 'bento-nano-shell-ws2-proof.exe'
$itemRoot = Join-Path $stateDir 'items'
$vaultPath = Join-Path $stateDir 'vault.bin'
$stderrPath = Join-Path $proofDir 'stderr.log'
$stdoutPath = Join-Path $proofDir 'stdout.log'
$summaryPath = Join-Path $proofDir 'summary.json'
$themePickerPath = Join-Path $nano 'crates\bento-nano-app\src\theme_picker.rs'

function Write-Utf8NoBom([string]$path, [string]$content) {
  $encoding = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($path, $content, $encoding)
}

function Remove-DirInsideRepo([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) { return }
  $resolvedNano = (Resolve-Path -LiteralPath $nano).Path
  $resolvedPath = (Resolve-Path -LiteralPath $path).Path
  if (-not $resolvedPath.StartsWith($resolvedNano, [StringComparison]::OrdinalIgnoreCase)) {
    throw "refusing to remove path outside repo: $resolvedPath"
  }
  Remove-Item -LiteralPath $path -Recurse -Force
}

function Read-ProofText([string]$path) {
  if (Test-Path -LiteralPath $path) {
    $fs = [System.IO.File]::Open($path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    $reader = $null
    try {
      $reader = New-Object System.IO.StreamReader($fs, [System.Text.Encoding]::UTF8, $true)
      return $reader.ReadToEnd()
    } finally {
      if ($null -ne $reader) {
        $reader.Dispose()
      } else {
        $fs.Dispose()
      }
    }
  }
  return ''
}

function Read-ProofLines([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) { return @() }
  $text = Read-ProofText $path
  if ($text.Length -eq 0) { return @() }
  return @($text -split "`r?`n" | Where-Object { $_.Length -gt 0 })
}

function Get-FileSha256OrNull([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) { return $null }
  return (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
}

function Get-SettingValueFromVaultBody($body, [string]$key) {
  if ($null -eq $body -or $null -eq $body.kv) { return $null }
  $prop = $body.kv.PSObject.Properties[$key]
  if ($null -eq $prop) { return $null }
  $value = $prop.Value
  if ($null -eq $value) { return $null }
  if ($null -ne $value.PSObject.Properties['Str']) { return [string]$value.Str }
  if ($null -ne $value.PSObject.Properties['Bool']) { return [bool]$value.Bool }
  if ($null -ne $value.PSObject.Properties['Int']) { return [int]$value.Int }
  if ($null -ne $value.PSObject.Properties['Float']) { return [double]$value.Float }
  return $value.ToString()
}

function Get-VaultSnapshot {
  if (-not (Test-Path -LiteralPath $vaultPath)) {
    return [ordered]@{
      exists = $false
      mode_tag = $null
      mode = $null
      sha256 = $null
      bytes = 0
      plaintext_decoded = $false
      settings = [ordered]@{}
    }
  }

  $item = Get-Item -LiteralPath $vaultPath
  $hash = Get-FileSha256OrNull $vaultPath
  $record = [System.IO.File]::ReadAllText($vaultPath).TrimStart([char]0xFEFF) | ConvertFrom-Json
  $tag = [int]$record.mode_tag
  $mode = switch ($tag) {
    0 { 'None' }
    1 { 'Dpapi' }
    2 { 'Passphrase' }
    default { "Unknown:$tag" }
  }
  $settings = [ordered]@{}
  $plaintextDecoded = $false
  $plaintextBytes = 0
  $plaintextJsonPath = $null

  if ($tag -eq 0 -and $record.ciphertext_b64) {
    $bytes = [Convert]::FromBase64String([string]$record.ciphertext_b64)
    $plaintextBytes = $bytes.Length
    $json = [System.Text.Encoding]::UTF8.GetString($bytes)
    $plaintextJsonPath = Join-Path $proofDir 'vault-plaintext-last.json'
    Write-Utf8NoBom $plaintextJsonPath $json
    $body = $json | ConvertFrom-Json
    $settings['active_theme'] = Get-SettingValueFromVaultBody $body 'active_theme'
    $settings['theme.base_accent'] = Get-SettingValueFromVaultBody $body 'theme.base_accent'
    $settings['accent_color'] = Get-SettingValueFromVaultBody $body 'accent_color'
    $settings['zone_display_mode'] = Get-SettingValueFromVaultBody $body 'zone_display_mode'
    $settings['portable_mode'] = Get-SettingValueFromVaultBody $body 'general.portable_mode'
    $plaintextDecoded = $true
  }

  return [ordered]@{
    exists = $true
    mode_tag = $tag
    mode = $mode
    sha256 = $hash
    bytes = [int64]$item.Length
    ciphertext_b64_len = if ($record.ciphertext_b64) { [int]$record.ciphertext_b64.Length } else { 0 }
    plaintext_decoded = $plaintextDecoded
    plaintext_bytes = $plaintextBytes
    plaintext_json_path = $plaintextJsonPath
    settings = $settings
  }
}

function Wait-VaultSetting([string]$key, [object]$expected, [int]$timeoutMs = 4000) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $timeoutMs) {
    $snapshot = Get-VaultSnapshot
    if ($snapshot.exists -and $snapshot.plaintext_decoded) {
      $actual = $snapshot.settings[$key]
      if ($actual -eq $expected) { return $snapshot }
    }
    Start-Sleep -Milliseconds 120
  }
  return Get-VaultSnapshot
}

function Get-ThemePickerContract {
  $text = [System.IO.File]::ReadAllText($themePickerPath)
  $presetCount = if ($text -match 'pub const PRESET_COUNT:\s*usize\s*=\s*(\d+);') { [int]$Matches[1] } else { -1 }
  $accentCount = if ($text -match 'pub const ACCENT_SWATCH_COUNT:\s*usize\s*=\s*(\d+);') { [int]$Matches[1] } else { -1 }
  $groupCount = if ($text -match 'pub const THEME_GROUP_ORDER:\s*\[ThemeGroup;\s*(\d+)\]') { [int]$Matches[1] } else { -1 }
  $activeBorderDoc = ($text -match '2-DIP accent-blue border')
  $activeLabelDoc = ($text -match 'centred name label')
  return [ordered]@{
    source = $themePickerPath
    preset_count = $presetCount
    family_heading_count = $groupCount
    accent_swatch_count = $accentCount
    active_border_documented = [bool]$activeBorderDoc
    active_label_documented = [bool]$activeLabelDoc
    expected_preset_count = 17
    expected_family_heading_count = 4
    expected_accent_swatch_count = 12
    pass = ($presetCount -eq 17 -and $groupCount -eq 4 -and $accentCount -eq 12 -and $activeBorderDoc -and $activeLabelDoc)
  }
}

if (-not (Test-Path -LiteralPath $sourceExe)) {
  $env:CARGO_BUILD_JOBS = '1'
  $env:CARGO_INCREMENTAL = '0'
  Push-Location $root
  try {
    & cargo build --manifest-path bentodesk-nano/Cargo.toml -p bento-nano-shell --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
  } finally {
    Pop-Location
  }
}
if (-not (Test-Path -LiteralPath $sourceExe)) {
  throw "source exe not found: $sourceExe"
}

New-Item -ItemType Directory -Force -Path $proofDir | Out-Null
Get-ChildItem -LiteralPath $proofDir -File -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
Remove-DirInsideRepo $stateDir
New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
Copy-Item -LiteralPath $sourceExe -Destination $proofExe -Force

$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_INCREMENTAL = '0'
$previousItemRoot = $env:BENTODESK_NANO_BENCHMARK_ITEM_ROOT
$env:BENTODESK_NANO_BENCHMARK_ITEM_ROOT = $itemRoot
Push-Location $root
try {
  & cargo run --quiet --manifest-path bentodesk-nano/Cargo.toml -p bento-nano-platform --example seed_benchmark_scene --target x86_64-pc-windows-msvc -- $stateDir |
    Out-File -FilePath (Join-Path $proofDir '00-seed-benchmark-scene.txt') -Encoding utf8
  if ($LASTEXITCODE -ne 0) { throw "seed_benchmark_scene failed with exit code $LASTEXITCODE" }
} finally {
  Pop-Location
  if ($null -eq $previousItemRoot) {
    Remove-Item Env:\BENTODESK_NANO_BENCHMARK_ITEM_ROOT -ErrorAction SilentlyContinue
  } else {
    $env:BENTODESK_NANO_BENCHMARK_ITEM_ROOT = $previousItemRoot
  }
}

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class NativeProof0618Ws2 {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT lpPoint);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool InvalidateRect(IntPtr hWnd, IntPtr lpRect, bool bErase);
  [DllImport("user32.dll")] public static extern bool UpdateWindow(IntPtr hWnd);
  public const uint WM_HOTKEY = 0x0312;
  public const uint WM_LBUTTONDOWN = 0x0201;
  public const uint WM_LBUTTONUP = 0x0202;
  public const uint WM_MOUSEWHEEL = 0x020A;
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
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
  $cb = [NativeProof0618Ws2+EnumWindowsProc]{
    param([IntPtr]$hwnd, [IntPtr]$lparam)
    [uint32]$wpid = 0
    [void][NativeProof0618Ws2]::GetWindowThreadProcessId($hwnd, [ref]$wpid)
    if ($wpid -eq [uint32]$processId) {
      $class = New-Object System.Text.StringBuilder 256
      $title = New-Object System.Text.StringBuilder 256
      [void][NativeProof0618Ws2]::GetClassName($hwnd, $class, $class.Capacity)
      [void][NativeProof0618Ws2]::GetWindowText($hwnd, $title, $title.Capacity)
      $rect = New-Object NativeProof0618Ws2+RECT
      $client = New-Object NativeProof0618Ws2+RECT
      [void][NativeProof0618Ws2]::GetWindowRect($hwnd, [ref]$rect)
      [void][NativeProof0618Ws2]::GetClientRect($hwnd, [ref]$client)
      [void]$items.Add([pscustomobject]@{
        hwnd = $hwnd.ToInt64()
        class = $class.ToString()
        title = $title.ToString()
        visible = [NativeProof0618Ws2]::IsWindowVisible($hwnd)
        dpi = [NativeProof0618Ws2]::GetDpiForWindow($hwnd)
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
  [void][NativeProof0618Ws2]::EnumWindows($cb, [IntPtr]::Zero)
  return @($items.ToArray())
}

function Wait-Window([int]$processId, [string]$class, [int]$timeoutMs = 10000, [bool]$visibleOnly = $true) {
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

function Get-CurrentHitLine([int]$beforeCount) {
  $lines = Read-ProofLines $stderrPath
  $newLines = @()
  if ($lines.Count -gt $beforeCount) {
    $newLines = @($lines[$beforeCount..($lines.Count - 1)])
  }
  $hit = @($newLines | Where-Object { $_ -match 'settings: lbutton_down' } | Select-Object -Last 1)
  if ($hit.Count -gt 0) { return [string]$hit[0] }
  return $null
}

function Send-ClientClick($win, [double]$logicalX, [double]$logicalY, [string]$name, [int]$sleepMs = 280) {
  $beforeCount = (Read-ProofLines $stderrPath).Count
  $clientX = [int][Math]::Round($logicalX)
  $clientY = [int][Math]::Round($logicalY)
  [void][NativeProof0618Ws2]::SetForegroundWindow([IntPtr]$win.hwnd)
  $screenPoint = New-Object NativeProof0618Ws2+POINT
  $screenPoint.X = $clientX
  $screenPoint.Y = $clientY
  if ([NativeProof0618Ws2]::ClientToScreen([IntPtr]$win.hwnd, [ref]$screenPoint)) {
    [void][NativeProof0618Ws2]::SetCursorPos($screenPoint.X, $screenPoint.Y)
  } else {
    [void][NativeProof0618Ws2]::SetCursorPos([int]($win.rect.left + $clientX), [int]($win.rect.top + $clientY))
  }
  Start-Sleep -Milliseconds 60
  $lp = New-LParam $clientX $clientY
  [void][NativeProof0618Ws2]::SendMessageW([IntPtr]$win.hwnd, [NativeProof0618Ws2]::WM_LBUTTONDOWN, [UIntPtr]::Zero, $lp)
  Start-Sleep -Milliseconds 40
  [void][NativeProof0618Ws2]::SendMessageW([IntPtr]$win.hwnd, [NativeProof0618Ws2]::WM_LBUTTONUP, [UIntPtr]::Zero, $lp)
  Start-Sleep -Milliseconds $sleepMs
  $hitLine = Get-CurrentHitLine $beforeCount
  return [ordered]@{
    name = $name
    client_x = $clientX
    client_y = $clientY
    hit_line = $hitLine
  }
}

function Send-Wheel($win, [string]$direction, [int]$count, [int]$sleepMs = 90) {
  $events = @()
  $wParam = if ($direction -eq 'down') {
    [UIntPtr]([uint64]4287102976)
  } else {
    [UIntPtr]([uint64]7864320)
  }
  for ($i = 0; $i -lt $count; $i++) {
    [void][NativeProof0618Ws2]::SendMessageW([IntPtr]$win.hwnd, [NativeProof0618Ws2]::WM_MOUSEWHEEL, $wParam, [IntPtr]::Zero)
    Start-Sleep -Milliseconds $sleepMs
    $events += [ordered]@{ direction=$direction; index=($i + 1) }
  }
  return @($events)
}

function Force-Paint($win) {
  if (-not $win) { return }
  [void][NativeProof0618Ws2]::SetForegroundWindow([IntPtr]$win.hwnd)
  [void][NativeProof0618Ws2]::InvalidateRect([IntPtr]$win.hwnd, [IntPtr]::Zero, $false)
  [void][NativeProof0618Ws2]::UpdateWindow([IntPtr]$win.hwnd)
  Start-Sleep -Milliseconds 420
}

function Save-WindowShot($win, [string]$path) {
  if (-not $win) { return $false }
  $w = [Math]::Max(1, [int]$win.rect.width)
  $h = [Math]::Max(1, [int]$win.rect.height)
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  try {
    [void][NativeProof0618Ws2]::SetForegroundWindow([IntPtr]$win.hwnd)
    Start-Sleep -Milliseconds 120
    $g.CopyFromScreen([int]$win.rect.left, [int]$win.rect.top, 0, 0, [System.Drawing.Size]::new($w, $h))
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    return $true
  } finally {
    $g.Dispose()
    $bmp.Dispose()
  }
}

function Start-Target {
  Remove-Item -LiteralPath $stderrPath,$stdoutPath -Force -ErrorAction SilentlyContinue
  $previousStateDir = $env:BENTODESK_NANO_STATE_DIR
  $env:BENTODESK_NANO_STATE_DIR = $stateDir
  try {
    return Start-Process -FilePath $proofExe -WorkingDirectory $proofDir -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath -PassThru
  } finally {
    if ($null -eq $previousStateDir) {
      Remove-Item Env:\BENTODESK_NANO_STATE_DIR -ErrorAction SilentlyContinue
    } else {
      $env:BENTODESK_NANO_STATE_DIR = $previousStateDir
    }
  }
}

function Get-HitCount([string]$log, [string]$hitName) {
  return ([regex]::Matches($log, "hit=$([regex]::Escape($hitName))")).Count
}

function Get-ModeFromHitLine([string]$line) {
  if ($line -match 'hit=SetZoneDisplayMode\((Hover|Always|Click)\)') {
    return $Matches[1]
  }
  return $null
}

function Convert-ModeToWire([string]$mode) {
  switch ($mode) {
    'Hover' { return 'hover' }
    'Always' { return 'always' }
    'Click' { return 'click' }
    default { return $null }
  }
}

function Click-ModeCandidate($settingsWin, [int]$x, [int]$y, [string]$name) {
  $click = Send-ClientClick $settingsWin $x $y $name 360
  $mode = Get-ModeFromHitLine $click.hit_line
  $wire = Convert-ModeToWire $mode
  $vault = if ($wire) { Wait-VaultSetting 'zone_display_mode' $wire 4000 } else { Get-VaultSnapshot }
  return [ordered]@{
    name = $name
    x = $x
    y = $y
    hit_line = $click.hit_line
    mode = $mode
    wire = $wire
    vault_zone_display_mode = $vault.settings['zone_display_mode']
    vault_sha256 = $vault.sha256
    vault_plaintext_decoded = $vault.plaintext_decoded
  }
}

$stage = 'started'
$proc = $null
$main = $null
$settings = $null
$clicks = New-Object System.Collections.ArrayList
$wheels = New-Object System.Collections.ArrayList
$screenshots = New-Object System.Collections.ArrayList
$modeCalibration = New-Object System.Collections.ArrayList
$modeClicks = New-Object System.Collections.ArrayList
$processExitedAfterQuitHotkey = $false
$settingsClosedAfterSave = $false
$vaultBeforeMode = $null
$vaultAfterModeClicks = $null
$vaultAfterSave = $null
$stderrAll = ''
$stdoutAll = ''

try {
  $proc = Start-Target
  $main = Wait-Window $proc.Id 'BentoNanoShell' 10000
  if (-not $main) { throw 'main window not found' }
  Start-Sleep -Milliseconds 1200

  $stage = 'open-settings'
  [void][NativeProof0618Ws2]::PostMessageW([IntPtr]$main.hwnd, [NativeProof0618Ws2]::WM_HOTKEY, [UIntPtr]([uint64]16971), [IntPtr]::Zero)
  $settings = Wait-Window $proc.Id 'BentoAuxSets' 10000
  if (-not $settings) { throw 'settings window not found after hotkey 16971' }
  Start-Sleep -Milliseconds 900
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '01-settings-top.png') | Out-Null
  [void]$screenshots.Add('01-settings-top.png')

  $stage = 'appearance-theme'
  foreach ($event in Send-Wheel $settings 'down' 6) { [void]$wheels.Add($event) }
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '02-appearance-theme-grid.png') | Out-Null
  [void]$screenshots.Add('02-appearance-theme-grid.png')
  [void]$clicks.Add((Send-ClientClick $settings 454 231 'select-theme-card-6'))
  $themeVault = Wait-VaultSetting 'active_theme' 'ocean-blue' 4000
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '03-appearance-after-theme.png') | Out-Null
  [void]$screenshots.Add('03-appearance-after-theme.png')

  $stage = 'appearance-accent'
  foreach ($event in Send-Wheel $settings 'down' 5) { [void]$wheels.Add($event) }
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '04-appearance-accent-and-display-mode.png') | Out-Null
  [void]$screenshots.Add('04-appearance-accent-and-display-mode.png')
  [void]$clicks.Add((Send-ClientClick $settings 468 108 'select-accent-swatch-5'))
  $vaultBeforeMode = Get-VaultSnapshot

  $stage = 'zone-display-mode-calibration'
  $candidateXs = @(417, 499, 581)
  $candidateYs = @(156, 172, 188, 204, 220, 236, 252)
  $modeRowY = $null
  foreach ($y in $candidateYs) {
    foreach ($x in $candidateXs) {
      $result = Click-ModeCandidate $settings $x $y "calibrate-mode-x${x}-y${y}"
      [void]$modeCalibration.Add($result)
      if ($result.mode) {
        $modeRowY = $y
        break
      }
    }
    if ($null -ne $modeRowY) { break }
  }
  if ($null -eq $modeRowY) {
    throw 'could not find SetZoneDisplayMode hit during candidate calibration'
  }

  $stage = 'zone-display-mode-clicks'
  $targetModes = @(
    [ordered]@{ name='hover'; mode='Hover'; x=417 },
    [ordered]@{ name='always'; mode='Always'; x=499 },
    [ordered]@{ name='click'; mode='Click'; x=581 }
  )
  foreach ($target in $targetModes) {
    $result = Click-ModeCandidate $settings ([int]$target.x) ([int]$modeRowY) "set-zone-display-mode-$($target.name)"
    [void]$modeClicks.Add($result)
    if ($result.mode -ne $target.mode) {
      throw "expected SetZoneDisplayMode($($target.mode)) at x=$($target.x) y=$modeRowY, got $($result.mode)"
    }
    if ($result.vault_zone_display_mode -ne $target.name) {
      throw "vault zone_display_mode expected $($target.name), got $($result.vault_zone_display_mode)"
    }
  }
  $vaultAfterModeClicks = Get-VaultSnapshot
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '05-display-mode-after-clicks.png') | Out-Null
  [void]$screenshots.Add('05-display-mode-after-clicks.png')

  $stage = 'save'
  [void]$clicks.Add((Send-ClientClick $settings 578 556 'save-settings-ws2' 900))
  $settingsClosedAfterSave = [bool](Wait-Condition {
    $candidate = Get-WindowsForPid $proc.Id | Where-Object { $_.class -eq 'BentoAuxSets' -and $_.visible } | Select-Object -First 1
    return (-not $candidate)
  } 5000)
  if (-not $settingsClosedAfterSave) { throw 'settings did not close after SaveSettings click' }
  $vaultAfterSave = Wait-VaultSetting 'accent_color' '#22c55e' 4000
  Force-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '06-main-after-ws2-save.png') | Out-Null
  [void]$screenshots.Add('06-main-after-ws2-save.png')

  $stage = 'quit'
  [void][NativeProof0618Ws2]::PostMessageW([IntPtr]$main.hwnd, [NativeProof0618Ws2]::WM_HOTKEY, [UIntPtr]([uint64]16973), [IntPtr]::Zero)
  $processExitedAfterQuitHotkey = [bool](Wait-Condition { $proc.HasExited } 5000)
  if (-not $processExitedAfterQuitHotkey) { throw 'process did not exit after production quit hotkey' }
  $proc.WaitForExit(3000) | Out-Null

  $stderrAll = Read-ProofText $stderrPath
  $stdoutAll = Read-ProofText $stdoutPath
  $appearanceContract = Get-ThemePickerContract
  $screenshotFiles = @($screenshots.ToArray() | ForEach-Object {
    $shotPath = Join-Path $proofDir $_
    $item = Get-Item -LiteralPath $shotPath -ErrorAction SilentlyContinue
    [ordered]@{ name=$_; bytes=if ($item) { [int64]$item.Length } else { 0 } }
  })
  $nonBlankScreenshots = (@($screenshotFiles | Where-Object { $_.bytes -ge 12000 }).Count -eq $screenshotFiles.Count)

  $modeHitLines = @($modeClicks.ToArray() | ForEach-Object { $_.hit_line })
  $accepted = (
    $appearanceContract.pass -and
    (Get-HitCount $stderrAll 'SelectTheme') -ge 1 -and
    (Get-HitCount $stderrAll 'SelectAccent') -ge 1 -and
    ([regex]::Matches($stderrAll, 'hit=SetZoneDisplayMode\(Hover\)')).Count -ge 1 -and
    ([regex]::Matches($stderrAll, 'hit=SetZoneDisplayMode\(Always\)')).Count -ge 1 -and
    ([regex]::Matches($stderrAll, 'hit=SetZoneDisplayMode\(Click\)')).Count -ge 1 -and
    ([regex]::Matches($stderrAll, 'settings: scroll delta=')).Count -ge 11 -and
    $vaultAfterSave.exists -and
    $vaultAfterSave.plaintext_decoded -and
    $vaultAfterSave.settings['active_theme'] -eq 'ocean-blue' -and
    $vaultAfterSave.settings['accent_color'] -eq '#22c55e' -and
    $vaultAfterSave.settings['zone_display_mode'] -eq 'click' -and
    $nonBlankScreenshots -and
    $settingsClosedAfterSave -and
    $processExitedAfterQuitHotkey
  )

  $summary = [ordered]@{
    status = if ($accepted) { 'ok' } else { 'attention_required' }
    stage = 'completed'
    generated_at_utc = [DateTime]::UtcNow.ToString('o')
    ws_id = 'WS-2'
    proof_dir = $proofDir
    exe = $proofExe
    source_exe = $sourceExe
    state_dir = $stateDir
    no_mock_data = $true
    opened_via_hotkey_id = 16971
    quit_via_hotkey_id = 16973
    settings_window = Convert-WindowForJson $settings
    main_window = Convert-WindowForJson $main
    appearance_contract = [ordered]@{
      light_dark_toggle_expected = $false
      mirror_icon_toggle_expected = $false
      legacy_controls_dropped_by_research = $true
      research_source = '.trellis/tasks/05-29-nano-tauri-parity-plan/research/m1-settings-themepicker-spec.md:91'
      theme_picker_source_contract = $appearanceContract
      runtime_select_theme_hit = ((Get-HitCount $stderrAll 'SelectTheme') -ge 1)
      runtime_select_accent_hit = ((Get-HitCount $stderrAll 'SelectAccent') -ge 1)
      runtime_theme_saved = ($vaultAfterSave.settings['active_theme'] -eq 'ocean-blue')
      runtime_accent_saved = ($vaultAfterSave.settings['accent_color'] -eq '#22c55e')
      selected_theme_expected = 'ocean-blue'
      selected_accent_expected = '#22c55e'
      accent_persistence_key = 'accent_color'
    }
    zone_display_mode = [ordered]@{
      calibration = @($modeCalibration.ToArray())
      calibration_row_y = $modeRowY
      clicked_modes = @($modeClicks.ToArray())
      hit_lines = $modeHitLines
      final_expected = 'click'
      final_vault_value = $vaultAfterSave.settings['zone_display_mode']
      vault_sha_before_mode = $vaultBeforeMode.sha256
      vault_sha_after_mode_clicks = $vaultAfterModeClicks.sha256
      vault_sha_after_save = $vaultAfterSave.sha256
      hover_hit = (([regex]::Matches($stderrAll, 'hit=SetZoneDisplayMode\(Hover\)')).Count -ge 1)
      always_hit = (([regex]::Matches($stderrAll, 'hit=SetZoneDisplayMode\(Always\)')).Count -ge 1)
      click_hit = (([regex]::Matches($stderrAll, 'hit=SetZoneDisplayMode\(Click\)')).Count -ge 1)
      persisted_after_each_click = (@($modeClicks.ToArray() | Where-Object { $_.vault_zone_display_mode -eq $_.wire }).Count -eq 3)
    }
    vault = [ordered]@{
      path = $vaultPath
      before_mode = $vaultBeforeMode
      after_mode_clicks = $vaultAfterModeClicks
      after_save = $vaultAfterSave
    }
    hits = [ordered]@{
      SelectTheme = Get-HitCount $stderrAll 'SelectTheme'
      SelectAccent = Get-HitCount $stderrAll 'SelectAccent'
      SetZoneDisplayModeHover = ([regex]::Matches($stderrAll, 'hit=SetZoneDisplayMode\(Hover\)')).Count
      SetZoneDisplayModeAlways = ([regex]::Matches($stderrAll, 'hit=SetZoneDisplayMode\(Always\)')).Count
      SetZoneDisplayModeClick = ([regex]::Matches($stderrAll, 'hit=SetZoneDisplayMode\(Click\)')).Count
      SaveSettings = Get-HitCount $stderrAll 'SaveSettings'
      scroll_log_count = ([regex]::Matches($stderrAll, 'settings: scroll delta=')).Count
      stderr_available = ($stderrAll.Length -gt 8)
    }
    assertions = [ordered]@{
      accepted = [bool]$accepted
      settings_window_class = ($settings.class -eq 'BentoAuxSets')
      native_wheel_messages_sent = ($wheels.Count -ge 11)
      scroll_logs_seen = (([regex]::Matches($stderrAll, 'settings: scroll delta=')).Count -ge 11)
      appearance_source_contract_pass = [bool]$appearanceContract.pass
      theme_click_hit = ((Get-HitCount $stderrAll 'SelectTheme') -ge 1)
      accent_click_hit = ((Get-HitCount $stderrAll 'SelectAccent') -ge 1)
      zone_display_hover_hit = (([regex]::Matches($stderrAll, 'hit=SetZoneDisplayMode\(Hover\)')).Count -ge 1)
      zone_display_always_hit = (([regex]::Matches($stderrAll, 'hit=SetZoneDisplayMode\(Always\)')).Count -ge 1)
      zone_display_click_hit = (([regex]::Matches($stderrAll, 'hit=SetZoneDisplayMode\(Click\)')).Count -ge 1)
      active_theme_persisted = ($vaultAfterSave.settings['active_theme'] -eq 'ocean-blue')
      accent_color_persisted = ($vaultAfterSave.settings['accent_color'] -eq '#22c55e')
      zone_display_mode_persisted = ($vaultAfterSave.settings['zone_display_mode'] -eq 'click')
      nonblank_screenshots = [bool]$nonBlankScreenshots
      settings_closed_after_save = [bool]$settingsClosedAfterSave
      process_exited_after_quit_hotkey = [bool]$processExitedAfterQuitHotkey
    }
    clicks = @($clicks.ToArray())
    wheels = @($wheels.ToArray())
    screenshots = $screenshotFiles
    stdout_path = $stdoutPath
    stderr_path = $stderrPath
    remaining_blockers = @()
  }
  Write-Utf8NoBom $summaryPath ($summary | ConvertTo-Json -Depth 40)
  Write-Host "ws2_appearance_status=$($summary.status)"
  Write-Host "summary=$summaryPath"
  Write-Host "zone_display_mode=$($vaultAfterSave.settings['zone_display_mode'])"
  Write-Host "active_theme=$($vaultAfterSave.settings['active_theme'])"
  Write-Host "accent_color=$($vaultAfterSave.settings['accent_color'])"
} catch {
  $stderrAll = Read-ProofText $stderrPath
  $stdoutAll = Read-ProofText $stdoutPath
  $failure = [ordered]@{
    status = 'failed'
    stage = $stage
    generated_at_utc = [DateTime]::UtcNow.ToString('o')
    ws_id = 'WS-2'
    proof_dir = $proofDir
    error = $_.Exception.Message
    exe = $proofExe
    source_exe = $sourceExe
    state_dir = $stateDir
    stdout_path = $stdoutPath
    stderr_path = $stderrPath
    stdout_tail = if ($stdoutAll.Length -gt 4000) { $stdoutAll.Substring($stdoutAll.Length - 4000) } else { $stdoutAll }
    stderr_tail = if ($stderrAll.Length -gt 4000) { $stderrAll.Substring($stderrAll.Length - 4000) } else { $stderrAll }
  }
  Write-Utf8NoBom $summaryPath ($failure | ConvertTo-Json -Depth 20)
  if ($proc -and -not $proc.HasExited) {
    try {
      if ($main) {
        [void][NativeProof0618Ws2]::PostMessageW([IntPtr]$main.hwnd, [NativeProof0618Ws2]::WM_HOTKEY, [UIntPtr]([uint64]16973), [IntPtr]::Zero)
        Start-Sleep -Milliseconds 1000
      }
      if (-not $proc.HasExited) { $proc.Kill() }
    } catch {}
  }
  throw
}
