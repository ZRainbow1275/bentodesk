$ErrorActionPreference = 'Stop'

$root = 'D:\Desktop\CREATOR FOUR'
$nano = Join-Path $root 'bentodesk-nano'
$sourceExe = Join-Path $nano 'target\x86_64-pc-windows-msvc\debug\bento-nano-shell.exe'
$stateDir = Join-Path $nano 'runtime-proof-0608-settings-appearance-state'
$proofDir = Join-Path $nano 'runtime-proof-0608-settings-appearance-try'
$proofExe = Join-Path $proofDir 'bento-nano-shell-settings-proof.exe'
$itemRoot = Join-Path $stateDir 'items'
$vaultPath = Join-Path $stateDir 'vault.bin'
$backupsDir = Join-Path $stateDir 'backups'
$registryPath = Join-Path $stateDir 'plugins\registry.json'
$stderrPath = Join-Path $proofDir 'stderr.log'
$stdoutPath = Join-Path $proofDir 'stdout.log'
$summaryPath = Join-Path $proofDir 'summary.json'
$installPluginId = 'com.test.installed-theme'
$installPluginArchive = Join-Path $proofDir 'runtime-installed-theme.zip'
$attemptNativePluginInstall = $true

function Write-Utf8NoBom([string]$path, [string]$content) {
  $encoding = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($path, $content, $encoding)
}

if (-not (Test-Path -LiteralPath $sourceExe)) {
  throw "source exe not found: $sourceExe"
}

New-Item -ItemType Directory -Force -Path $proofDir | Out-Null
Get-ChildItem -LiteralPath $proofDir -File -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -ne 'runtime-proof-0608-settings-appearance-run.ps1' } |
  Remove-Item -Force -ErrorAction SilentlyContinue
if (Test-Path -LiteralPath $stateDir) {
  $resolvedNano = (Resolve-Path -LiteralPath $nano).Path
  $resolvedState = (Resolve-Path -LiteralPath $stateDir).Path
  if (-not $resolvedState.StartsWith($resolvedNano, [StringComparison]::OrdinalIgnoreCase)) {
    throw "refusing to remove state dir outside nano repo: $resolvedState"
  }
  Remove-Item -LiteralPath $stateDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
Copy-Item -LiteralPath $sourceExe -Destination $proofExe -Force

$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_INCREMENTAL = '0'
$env:BENTODESK_NANO_BENCHMARK_ITEM_ROOT = $itemRoot
Push-Location $root
try {
  & cargo run --quiet --manifest-path bentodesk-nano/Cargo.toml -p bento-nano-platform --example seed_benchmark_scene --target x86_64-pc-windows-msvc -- $stateDir |
    Out-File -FilePath (Join-Path $proofDir '00-seed-benchmark-scene.txt') -Encoding utf8
} finally {
  Pop-Location
  Remove-Item Env:\BENTODESK_NANO_BENCHMARK_ITEM_ROOT -ErrorAction SilentlyContinue
}

$pluginDir = Join-Path $stateDir 'plugins\com.test.runtime-theme'
New-Item -ItemType Directory -Force -Path $pluginDir | Out-Null
$manifest = [ordered]@{
  id = 'com.test.runtime-theme'
  name = 'Runtime Theme'
  version = '1.0.0'
  type = 'theme'
  author = 'Runtime Proof'
  description = 'Runtime proof plugin'
  min_app_version = $null
  icon = $null
}
$theme = [ordered]@{
  id = 'runtime-theme'
  name = 'Runtime Theme'
  is_builtin = $false
  colors = [ordered]@{
    accent = '#22c55e'
    background = '#101820'
    text = '#f8fafc'
    border = '#334155'
  }
  capsule = [ordered]@{
    shape = 'rounded'
    size = 'medium'
    blur_radius = 18.0
  }
  animation = [ordered]@{
    expand_duration_ms = 180
    collapse_duration_ms = 260
  }
  glassmorphism = [ordered]@{
    blur = 20.0
    opacity = 0.82
    saturation = 1.2
  }
}
Write-Utf8NoBom (Join-Path $pluginDir 'manifest.json') ($manifest | ConvertTo-Json -Depth 8)
Write-Utf8NoBom (Join-Path $pluginDir 'theme.json') ($theme | ConvertTo-Json -Depth 8)

$installPluginSrc = Join-Path $proofDir 'runtime-installed-theme-src'
New-Item -ItemType Directory -Force -Path $installPluginSrc | Out-Null
$installManifest = [ordered]@{
  id = $installPluginId
  name = 'Runtime Installed Theme'
  version = '1.0.0'
  type = 'theme'
  author = 'Runtime Proof'
  description = 'Runtime proof plugin installed through native file picker'
  min_app_version = $null
  icon = $null
}
$installTheme = [ordered]@{
  id = 'runtime-installed-theme'
  name = 'Runtime Installed Theme'
  is_builtin = $false
  colors = [ordered]@{
    accent = '#06b6d4'
    background = '#111827'
    text = '#f9fafb'
    border = '#374151'
  }
  capsule = [ordered]@{
    shape = 'rounded'
    size = 'medium'
    blur_radius = 18.0
  }
  animation = [ordered]@{
    expand_duration_ms = 180
    collapse_duration_ms = 260
  }
  glassmorphism = [ordered]@{
    blur = 20.0
    opacity = 0.82
    saturation = 1.2
  }
}
Write-Utf8NoBom (Join-Path $installPluginSrc 'manifest.json') ($installManifest | ConvertTo-Json -Depth 8)
Write-Utf8NoBom (Join-Path $installPluginSrc 'theme.json') ($installTheme | ConvertTo-Json -Depth 8)
Compress-Archive -Path (Join-Path $installPluginSrc '*') -DestinationPath $installPluginArchive -Force

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class NativeProof0608Settings {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  public delegate bool EnumChildWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr hWnd, EnumChildWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool SetWindowTextW(IntPtr hWnd, string lpString);
  [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr hDlg, int nIDDlgItem);
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
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdcBlt, uint nFlags);
  public const uint WM_PAINT = 0x000F;
  public const uint WM_SIZE = 0x0005;
  public const uint WM_CHAR = 0x0102;
  public const uint WM_COMMAND = 0x0111;
  public const uint BM_CLICK = 0x00F5;
  public const uint WM_HOTKEY = 0x0312;
  public const uint WM_LBUTTONDOWN = 0x0201;
  public const uint WM_LBUTTONUP = 0x0202;
  public const uint WM_MOUSEWHEEL = 0x020A;
  public const uint PW_RENDERFULLCONTENT = 0x00000002;
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
      width = if ($win.client) { [int]$win.client.width } else { 0 }
      height = if ($win.client) { [int]$win.client.height } else { 0 }
    }
  }
}

function Get-WindowsForPid([int]$processId) {
  $items = New-Object System.Collections.ArrayList
  $cb = [NativeProof0608Settings+EnumWindowsProc]{
    param([IntPtr]$hwnd, [IntPtr]$lparam)
    [uint32]$wpid = 0
    [void][NativeProof0608Settings]::GetWindowThreadProcessId($hwnd, [ref]$wpid)
    if ($wpid -eq [uint32]$processId) {
      $class = New-Object System.Text.StringBuilder 256
      $title = New-Object System.Text.StringBuilder 256
      [void][NativeProof0608Settings]::GetClassName($hwnd, $class, $class.Capacity)
      [void][NativeProof0608Settings]::GetWindowText($hwnd, $title, $title.Capacity)
      $rect = New-Object NativeProof0608Settings+RECT
      $client = New-Object NativeProof0608Settings+RECT
      [void][NativeProof0608Settings]::GetWindowRect($hwnd, [ref]$rect)
      [void][NativeProof0608Settings]::GetClientRect($hwnd, [ref]$client)
      [void]$items.Add([pscustomobject]@{
        hwnd = $hwnd.ToInt64()
        class = $class.ToString()
        title = $title.ToString()
        visible = [NativeProof0608Settings]::IsWindowVisible($hwnd)
        dpi = [NativeProof0608Settings]::GetDpiForWindow($hwnd)
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
  [void][NativeProof0608Settings]::EnumWindows($cb, [IntPtr]::Zero)
  return @($items.ToArray())
}

function Get-ChildWindowsForHwnd($hwnd) {
  $items = New-Object System.Collections.ArrayList
  $cb = [NativeProof0608Settings+EnumChildWindowsProc]{
    param([IntPtr]$child, [IntPtr]$lParam)
    $classBuilder = New-Object System.Text.StringBuilder 256
    $textBuilder = New-Object System.Text.StringBuilder 512
    [void][NativeProof0608Settings]::GetClassName($child, $classBuilder, $classBuilder.Capacity)
    [void][NativeProof0608Settings]::GetWindowText($child, $textBuilder, $textBuilder.Capacity)
    [void]$items.Add([pscustomobject]@{
      hwnd = $child.ToInt64()
      class = $classBuilder.ToString()
      title = $textBuilder.ToString()
      ctrl_id = [NativeProof0608Settings]::GetDlgCtrlID($child)
      visible = [NativeProof0608Settings]::IsWindowVisible($child)
    })
    return $true
  }
  [void][NativeProof0608Settings]::EnumChildWindows([IntPtr]$hwnd, $cb, [IntPtr]::Zero)
  return @($items.ToArray())
}

function Get-HwndText([IntPtr]$hwnd) {
  $textBuilder = New-Object System.Text.StringBuilder 1024
  [void][NativeProof0608Settings]::GetWindowText($hwnd, $textBuilder, $textBuilder.Capacity)
  return $textBuilder.ToString()
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

function Send-ClientClick($win, [double]$logicalX, [double]$logicalY, [string]$name, [int]$sleepMs = 260) {
  $clientX = [int][Math]::Round($logicalX)
  $clientY = [int][Math]::Round($logicalY)
  [void][NativeProof0608Settings]::SetForegroundWindow([IntPtr]$win.hwnd)
  $screenPoint = New-Object NativeProof0608Settings+POINT
  $screenPoint.X = $clientX
  $screenPoint.Y = $clientY
  if ([NativeProof0608Settings]::ClientToScreen([IntPtr]$win.hwnd, [ref]$screenPoint)) {
    [void][NativeProof0608Settings]::SetCursorPos($screenPoint.X, $screenPoint.Y)
  } else {
    [void][NativeProof0608Settings]::SetCursorPos([int]($win.rect.left + $clientX), [int]($win.rect.top + $clientY))
  }
  Start-Sleep -Milliseconds 60
  $lp = New-LParam $clientX $clientY
  [void][NativeProof0608Settings]::SendMessageW([IntPtr]$win.hwnd, [NativeProof0608Settings]::WM_LBUTTONDOWN, [UIntPtr]::Zero, $lp)
  Start-Sleep -Milliseconds 40
  [void][NativeProof0608Settings]::SendMessageW([IntPtr]$win.hwnd, [NativeProof0608Settings]::WM_LBUTTONUP, [UIntPtr]::Zero, $lp)
  Start-Sleep -Milliseconds $sleepMs
  return [ordered]@{ name=$name; client_x=$clientX; client_y=$clientY }
}

function Post-ClientClick($win, [double]$logicalX, [double]$logicalY, [string]$name, [int]$sleepMs = 260) {
  $clientX = [int][Math]::Round($logicalX)
  $clientY = [int][Math]::Round($logicalY)
  [void][NativeProof0608Settings]::SetForegroundWindow([IntPtr]$win.hwnd)
  $screenPoint = New-Object NativeProof0608Settings+POINT
  $screenPoint.X = $clientX
  $screenPoint.Y = $clientY
  if ([NativeProof0608Settings]::ClientToScreen([IntPtr]$win.hwnd, [ref]$screenPoint)) {
    [void][NativeProof0608Settings]::SetCursorPos($screenPoint.X, $screenPoint.Y)
  } else {
    [void][NativeProof0608Settings]::SetCursorPos([int]($win.rect.left + $clientX), [int]($win.rect.top + $clientY))
  }
  Start-Sleep -Milliseconds 60
  $lp = New-LParam $clientX $clientY
  [void][NativeProof0608Settings]::PostMessageW([IntPtr]$win.hwnd, [NativeProof0608Settings]::WM_LBUTTONDOWN, [UIntPtr]::Zero, $lp)
  Start-Sleep -Milliseconds 40
  [void][NativeProof0608Settings]::PostMessageW([IntPtr]$win.hwnd, [NativeProof0608Settings]::WM_LBUTTONUP, [UIntPtr]::Zero, $lp)
  Start-Sleep -Milliseconds $sleepMs
  return [ordered]@{ name=$name; client_x=$clientX; client_y=$clientY; posted=$true }
}

function Send-Wheel($win, [string]$direction, [int]$count, [int]$sleepMs = 90) {
  $events = @()
  $wParam = if ($direction -eq 'down') {
    [UIntPtr]([uint64]4287102976)
  } else {
    [UIntPtr]([uint64]7864320)
  }
  for ($i = 0; $i -lt $count; $i++) {
    [void][NativeProof0608Settings]::SendMessageW([IntPtr]$win.hwnd, [NativeProof0608Settings]::WM_MOUSEWHEEL, $wParam, [IntPtr]::Zero)
    Start-Sleep -Milliseconds $sleepMs
    $events += [ordered]@{ direction=$direction; index=($i + 1) }
  }
  return @($events)
}

function Send-Text($win, [string]$text, [int]$sleepMs = 35) {
  foreach ($ch in $text.ToCharArray()) {
    [void][NativeProof0608Settings]::SendMessageW([IntPtr]$win.hwnd, [NativeProof0608Settings]::WM_CHAR, [UIntPtr]([uint64][int][char]$ch), [IntPtr]::Zero)
    Start-Sleep -Milliseconds $sleepMs
  }
}

function Get-PluginRegistryState([string]$pluginId) {
  if (-not (Test-Path -LiteralPath $registryPath)) {
    return [ordered]@{ registry_exists=$false; plugin_count=0; present=$false; id=$pluginId; enabled=$null }
  }
  $registry = Get-Content -LiteralPath $registryPath -Raw | ConvertFrom-Json
  $plugins = @()
  if ($registry -and $registry.plugins) {
    $plugins = @($registry.plugins)
  }
  $plugin = @($plugins | Where-Object { $_.id -eq $pluginId } | Select-Object -First 1)
  return [ordered]@{
    registry_exists = $true
    plugin_count = [int]$plugins.Count
    present = [bool]$plugin
    id = $pluginId
    enabled = if ($plugin) { [bool]$plugin.enabled } else { $null }
  }
}

function Get-VaultWireRecord([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) {
    return [ordered]@{ exists=$false; mode_tag=$null; mode=$null; sha256=$null; bytes=0; ciphertext_b64_len=0 }
  }
  $item = Get-Item -LiteralPath $path
  $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
  try {
    $record = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    $tag = [int]$record.mode_tag
    $mode = switch ($tag) {
      0 { 'None' }
      1 { 'Dpapi' }
      2 { 'Passphrase' }
      default { "Unknown:$tag" }
    }
    return [ordered]@{
      exists = $true
      mode_tag = $tag
      mode = $mode
      sha256 = $hash
      bytes = [int64]$item.Length
      ciphertext_b64_len = if ($record.ciphertext_b64) { [int]$record.ciphertext_b64.Length } else { 0 }
      salt_b64_len = if ($record.salt_b64) { [int]$record.salt_b64.Length } else { 0 }
      nonce_b64_len = if ($record.nonce_b64) { [int]$record.nonce_b64.Length } else { 0 }
      tag_b64_len = if ($record.tag_b64) { [int]$record.tag_b64.Length } else { 0 }
    }
  } catch {
    return [ordered]@{
      exists = $true
      mode_tag = $null
      mode = 'unreadable-json'
      sha256 = $hash
      bytes = [int64]$item.Length
      ciphertext_b64_len = 0
    }
  }
}

function Submit-OpenFileDialog($processId, [string]$path) {
  $dialogPath = [System.IO.Path]::GetFileName($path)
  $automationResult = [ordered]@{ attempted=$false; set_value=$false; invoked_open=$false; error=$null }
  $dialog = Wait-Window $processId '#32770' 5000
  if (-not $dialog) {
    throw 'plugin install file dialog not found'
  }
  [void][NativeProof0608Settings]::SetForegroundWindow([IntPtr]$dialog.hwnd)
  Start-Sleep -Milliseconds 250
  $children = @(Get-ChildWindowsForHwnd $dialog.hwnd)
  $children | ConvertTo-Json -Depth 4 | Out-File -FilePath (Join-Path $proofDir 'plugin-install-dialog-children.json') -Encoding utf8
  $fileControlInfos = @($children | Where-Object { $_.ctrl_id -eq 1148 -and $_.visible -and ($_.class -eq 'ComboBoxEx32' -or $_.class -eq 'ComboBox' -or $_.class -eq 'Edit') })
  $fileEditInfo = @($fileControlInfos | Where-Object { $_.class -eq 'Edit' } | Select-Object -First 1)
  $openButtonInfo = @($children | Where-Object { $_.class -eq 'Button' -and $_.ctrl_id -eq 1 -and $_.visible } | Select-Object -First 1)
  $openButton = if ($openButtonInfo) { [IntPtr]([int64]$openButtonInfo.hwnd) } else { [IntPtr]::Zero }
  if ($fileControlInfos.Count -gt 0) {
    foreach ($info in $fileControlInfos) {
      [void][NativeProof0608Settings]::SetWindowTextW([IntPtr]([int64]$info.hwnd), $dialogPath)
    }
    if ($fileEditInfo) {
      $fileEditHwnd = [IntPtr]([int64]$fileEditInfo.hwnd)
      [void][NativeProof0608Settings]::SetWindowTextW($fileEditHwnd, '')
      foreach ($ch in $dialogPath.ToCharArray()) {
        [void][NativeProof0608Settings]::SendMessageW($fileEditHwnd, [NativeProof0608Settings]::WM_CHAR, [UIntPtr]([uint64][int][char]$ch), [IntPtr]::Zero)
      }
      $comboInfos = @($fileControlInfos | Where-Object { $_.class -eq 'ComboBox' -or $_.class -eq 'ComboBoxEx32' })
      foreach ($comboInfo in $comboInfos) {
        [void][NativeProof0608Settings]::SendMessageW([IntPtr]$dialog.hwnd, [NativeProof0608Settings]::WM_COMMAND, [UIntPtr]([uint64]((5 -shl 16) -bor 1148)), [IntPtr]([int64]$comboInfo.hwnd))
      }
    }
    Start-Sleep -Milliseconds 300
  }

  if ($openButton -ne [IntPtr]::Zero) {
    [void][NativeProof0608Settings]::SendMessageW($openButton, [NativeProof0608Settings]::BM_CLICK, [UIntPtr]::Zero, [IntPtr]::Zero)
  } else {
    [void][NativeProof0608Settings]::SendMessageW([IntPtr]$dialog.hwnd, [NativeProof0608Settings]::WM_COMMAND, [UIntPtr]([uint64]1), [IntPtr]::Zero)
  }

  $closed = [bool](Wait-Condition {
    $candidate = Get-WindowsForPid $processId | Where-Object { $_.class -eq '#32770' -and $_.visible } | Select-Object -First 1
    return (-not $candidate)
  } 2500)

  if (-not $closed) {
    $automationResult.attempted = $true
    try {
      if ($fileEditInfo) {
        $editElement = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]([int64]$fileEditInfo.hwnd))
        $valuePattern = $editElement.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
        $valuePattern.SetValue($dialogPath)
        $automationResult.set_value = $true
      }
      if ($openButton -ne [IntPtr]::Zero) {
        $buttonElement = [System.Windows.Automation.AutomationElement]::FromHandle($openButton)
        $invokePattern = $buttonElement.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
        $invokePattern.Invoke()
        $automationResult.invoked_open = $true
      }
    } catch {
      $automationResult.error = $_.Exception.Message
    }
    $automationResult | ConvertTo-Json -Depth 5 | Out-File -FilePath (Join-Path $proofDir 'plugin-install-dialog-automation.json') -Encoding utf8
    $closed = [bool](Wait-Condition {
      $candidate = Get-WindowsForPid $processId | Where-Object { $_.class -eq '#32770' -and $_.visible } | Select-Object -First 1
      return (-not $candidate)
    } 5000)
  }

  if (-not $closed) {
    [void][NativeProof0608Settings]::SetForegroundWindow([IntPtr]$dialog.hwnd)
    $wscriptShell = New-Object -ComObject WScript.Shell
    [void]$wscriptShell.AppActivate($processId)
    Start-Sleep -Milliseconds 250
    Set-Clipboard -Value $dialogPath
    Start-Sleep -Milliseconds 150
    $wscriptShell.SendKeys('%n')
    Start-Sleep -Milliseconds 150
    $wscriptShell.SendKeys('^a')
    Start-Sleep -Milliseconds 100
    $wscriptShell.SendKeys('^v')
    Start-Sleep -Milliseconds 150
    $wscriptShell.SendKeys('{ENTER}')
    $closed = [bool](Wait-Condition {
      $candidate = Get-WindowsForPid $processId | Where-Object { $_.class -eq '#32770' -and $_.visible } | Select-Object -First 1
      return (-not $candidate)
    } 5000)
  }

  if (-not $closed) {
    [void][NativeProof0608Settings]::SendMessageW([IntPtr]$dialog.hwnd, [NativeProof0608Settings]::WM_COMMAND, [UIntPtr]([uint64]1), [IntPtr]::Zero)
    $closed = [bool](Wait-Condition {
      $candidate = Get-WindowsForPid $processId | Where-Object { $_.class -eq '#32770' -and $_.visible } | Select-Object -First 1
      return (-not $candidate)
    } 4000)
  }
  if (-not $closed) {
    [System.Windows.Forms.SendKeys]::SendWait('{ESC}')
    throw 'plugin install file dialog did not close after file path submission'
  }
  return [ordered]@{ hwnd=$dialog.hwnd; class=$dialog.class; title=$dialog.title; selected_path=$path; closed=$closed; automation=$automationResult }
}

function Force-Paint($win) {
  if (-not $win) { return }
  [void][NativeProof0608Settings]::SetForegroundWindow([IntPtr]$win.hwnd)
  [void][NativeProof0608Settings]::InvalidateRect([IntPtr]$win.hwnd, [IntPtr]::Zero, $false)
  [void][NativeProof0608Settings]::UpdateWindow([IntPtr]$win.hwnd)
  Start-Sleep -Milliseconds 450
}

function Save-WindowShot($win, [string]$path) {
  if (-not $win) { return $false }
  $w = [Math]::Max(1, [int]$win.rect.width)
  $h = [Math]::Max(1, [int]$win.rect.height)
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  try {
    [void][NativeProof0608Settings]::SetForegroundWindow([IntPtr]$win.hwnd)
    Start-Sleep -Milliseconds 120
    $g.CopyFromScreen([int]$win.rect.left, [int]$win.rect.top, 0, 0, [System.Drawing.Size]::new($w, $h))
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    return $true
  } finally {
    $g.Dispose()
    $bmp.Dispose()
  }
}

function New-RectProof([int]$x, [int]$y, [int]$width, [int]$height) {
  return [ordered]@{
    x = $x
    y = $y
    width = $width
    height = $height
    right = $x + $width
    bottom = $y + $height
  }
}

function Test-PointInRect([int]$x, [int]$y, $rect) {
  return ($x -ge $rect.x -and $x -lt $rect.right -and $y -ge $rect.y -and $y -lt $rect.bottom)
}

function Get-AverageColor([System.Drawing.Bitmap]$bmp, [int]$x, [int]$y, [int]$width, [int]$height) {
  $r = 0.0
  $g = 0.0
  $b = 0.0
  $n = 0
  for ($yy = $y; $yy -lt ($y + $height); $yy++) {
    for ($xx = $x; $xx -lt ($x + $width); $xx++) {
      if ($xx -ge 0 -and $yy -ge 0 -and $xx -lt $bmp.Width -and $yy -lt $bmp.Height) {
        $color = $bmp.GetPixel($xx, $yy)
        $r += $color.R
        $g += $color.G
        $b += $color.B
        $n += 1
      }
    }
  }
  if ($n -le 0) { throw "empty sample rect x=$x y=$y w=$width h=$height" }
  return [ordered]@{
    r = [Math]::Round($r / $n, 2)
    g = [Math]::Round($g / $n, 2)
    b = [Math]::Round($b / $n, 2)
    samples = $n
  }
}

function Get-ColorDistance($a, $b) {
  return [Math]::Round(
    [Math]::Sqrt(
      [Math]::Pow(([double]$a.r - [double]$b.r), 2) +
      [Math]::Pow(([double]$a.g - [double]$b.g), 2) +
      [Math]::Pow(([double]$a.b - [double]$b.b), 2)
    ),
    2
  )
}

function Analyze-StickyFooterShot([string]$screenshotName) {
  $shotPath = Join-Path $proofDir $screenshotName
  if (-not (Test-Path -LiteralPath $shotPath)) {
    return [ordered]@{ passed=$false; screenshot=$screenshotName; error='screenshot missing' }
  }

  $bmp = [System.Drawing.Bitmap]::new($shotPath)
  try {
    # Mirrors settings_panel.rs for the 800x600 BentoAuxSets client:
    # panel=(160,16,480,568), footer=(160,528,480,56), save=(536,540,84,32).
    $panel = New-RectProof 160 16 480 568
    $footer = New-RectProof 160 528 480 56
    $body = New-RectProof 160 64 480 464
    $save = New-RectProof 536 540 84 32
    $cancel = New-RectProof 444 540 84 32
    $saveClick = [ordered]@{ x=578; y=556 }

    $footerSampleRect = New-RectProof 230 548 120 20
    $bodySampleRect = New-RectProof 230 505 120 20
    $saveSampleRect = New-RectProof 566 548 24 18
    $cancelSampleRect = New-RectProof 474 548 24 18

    $footerSample = Get-AverageColor $bmp $footerSampleRect.x $footerSampleRect.y $footerSampleRect.width $footerSampleRect.height
    $bodySample = Get-AverageColor $bmp $bodySampleRect.x $bodySampleRect.y $bodySampleRect.width $bodySampleRect.height
    $saveSample = Get-AverageColor $bmp $saveSampleRect.x $saveSampleRect.y $saveSampleRect.width $saveSampleRect.height
    $cancelSample = Get-AverageColor $bmp $cancelSampleRect.x $cancelSampleRect.y $cancelSampleRect.width $cancelSampleRect.height

    $footerVsBody = Get-ColorDistance $footerSample $bodySample
    $saveVsFooter = Get-ColorDistance $saveSample $footerSample
    $cancelVsFooter = Get-ColorDistance $cancelSample $footerSample
    $passed = (
      $bmp.Width -eq 800 -and
      $bmp.Height -eq 600 -and
      (Test-PointInRect $saveClick.x $saveClick.y $footer) -and
      (Test-PointInRect $saveClick.x $saveClick.y $save) -and
      $body.bottom -eq $footer.y -and
      $footerVsBody -ge 8.0 -and
      $saveVsFooter -ge 20.0
    )

    return [ordered]@{
      passed = [bool]$passed
      screenshot = $screenshotName
      screenshot_path = $shotPath
      viewport = [ordered]@{ width=$bmp.Width; height=$bmp.Height }
      expected_geometry_source = 'settings_panel.rs: settings_panel_rect_m1/settings_footer_rect/settings_save_button_rect'
      panel_rect = $panel
      body_rect = $body
      footer_rect = $footer
      save_button_rect = $save
      cancel_button_rect = $cancel
      save_click = $saveClick
      save_click_in_footer = (Test-PointInRect $saveClick.x $saveClick.y $footer)
      save_click_in_save_button = (Test-PointInRect $saveClick.x $saveClick.y $save)
      sample_rects = [ordered]@{
        footer = $footerSampleRect
        body_above_footer = $bodySampleRect
        save_button = $saveSampleRect
        cancel_button = $cancelSampleRect
      }
      samples = [ordered]@{
        footer = $footerSample
        body_above_footer = $bodySample
        save_button = $saveSample
        cancel_button = $cancelSample
      }
      distances = [ordered]@{
        footer_vs_body = $footerVsBody
        save_vs_footer = $saveVsFooter
        cancel_vs_footer = $cancelVsFooter
      }
      thresholds = [ordered]@{
        footer_vs_body_min = 8.0
        save_vs_footer_min = 20.0
      }
    }
  } finally {
    $bmp.Dispose()
  }
}

function Get-LuminanceRange([System.Drawing.Bitmap]$bmp, [int]$x, [int]$y, [int]$width, [int]$height, [int]$step = 8) {
  $min = 255.0
  $max = 0.0
  $n = 0
  for ($yy = $y; $yy -lt ($y + $height); $yy += $step) {
    for ($xx = $x; $xx -lt ($x + $width); $xx += $step) {
      if ($xx -ge 0 -and $yy -ge 0 -and $xx -lt $bmp.Width -and $yy -lt $bmp.Height) {
        $color = $bmp.GetPixel($xx, $yy)
        $lum = (0.2126 * [double]$color.R) + (0.7152 * [double]$color.G) + (0.0722 * [double]$color.B)
        if ($lum -lt $min) { $min = $lum }
        if ($lum -gt $max) { $max = $lum }
        $n += 1
      }
    }
  }
  if ($n -le 0) { throw "empty luminance sample rect x=$x y=$y w=$width h=$height" }
  return [ordered]@{
    min = [Math]::Round($min, 2)
    max = [Math]::Round($max, 2)
    range = [Math]::Round($max - $min, 2)
    samples = $n
    step = $step
  }
}

function Invoke-VisualMatrixGeometryTests {
  $stdout = Join-Path $proofDir 'visual-matrix-settings-panel-tests.stdout.log'
  $stderr = Join-Path $proofDir 'visual-matrix-settings-panel-tests.stderr.log'
  $previousJobs = $env:CARGO_BUILD_JOBS
  $previousIncremental = $env:CARGO_INCREMENTAL
  try {
    $env:CARGO_BUILD_JOBS = '1'
    $env:CARGO_INCREMENTAL = '0'
    Remove-Item -LiteralPath $stdout,$stderr -Force -ErrorAction SilentlyContinue
    $procCargo = Start-Process `
      -FilePath 'cargo' `
      -WorkingDirectory $nano `
      -ArgumentList @('test', '-p', 'bento-nano-app', '--target', 'x86_64-pc-windows-msvc', 'settings_panel') `
      -RedirectStandardOutput $stdout `
      -RedirectStandardError $stderr `
      -WindowStyle Hidden `
      -Wait `
      -PassThru
    $text = (Read-ProofText $stdout) + "`n" + (Read-ProofText $stderr)
    $match = [regex]::Match($text, 'test result: ok\. ([0-9]+) passed; 0 failed; 0 ignored; 0 measured; ([0-9]+) filtered out')
    $passed = if ($match.Success) { [int]$match.Groups[1].Value } else { 0 }
    return [ordered]@{
      ok = ($procCargo.ExitCode -eq 0 -and $passed -ge 100)
      command = 'cargo test -p bento-nano-app --target x86_64-pc-windows-msvc settings_panel'
      exit_code = [int]$procCargo.ExitCode
      passed = [int]$passed
      expected_min_passed = 100
      stdout = $stdout
      stderr = $stderr
      required_geometry_coverage = @(
        'm1_header_sticky_at_top_of_panel',
        'm1_body_sits_between_header_and_footer',
        'm1_footer_sticky_at_bottom_of_panel',
        'm2_source_rows_stack_vertically_below_label',
        'g3_m2_sources_section_sits_above_display_mode_picker',
        'perf_label_sits_below_m2_textarea',
        'startup_label_sits_below_performance_section',
        'm1e_stealth_title_sits_below_startup_section',
        'm1f_updater_title_sits_below_stealth_section',
        'm1g_rows_stack_in_order_title_desc_actions_status_list',
        'm7_encryption_rows_stack_in_order',
        'm1h_install_button_full_width_below_title',
        'm1h_plugin_cards_stack_vertically_below_install'
      )
    }
  } catch {
    return [ordered]@{
      ok = $false
      command = 'cargo test -p bento-nano-app --target x86_64-pc-windows-msvc settings_panel'
      exit_code = $null
      passed = 0
      expected_min_passed = 100
      stdout = $stdout
      stderr = $stderr
      error = $_.Exception.Message
    }
  } finally {
    if ($null -eq $previousJobs) {
      Remove-Item Env:\CARGO_BUILD_JOBS -ErrorAction SilentlyContinue
    } else {
      $env:CARGO_BUILD_JOBS = $previousJobs
    }
    if ($null -eq $previousIncremental) {
      Remove-Item Env:\CARGO_INCREMENTAL -ErrorAction SilentlyContinue
    } else {
      $env:CARGO_INCREMENTAL = $previousIncremental
    }
  }
}

function Analyze-FullSectionVisualMatrix($geometryTests) {
  $sections = @(
    [ordered]@{ section='general'; screenshot='03-settings-top.png' },
    [ordered]@{ section='paths'; screenshot='04-settings-paths-visible.png' },
    [ordered]@{ section='appearance_theme'; screenshot='05-settings-appearance-theme-grid.png' },
    [ordered]@{ section='appearance_theme_after_select'; screenshot='06-settings-appearance-after-theme.png' },
    [ordered]@{ section='appearance_accent'; screenshot='07-settings-appearance-accent-row.png' },
    [ordered]@{ section='performance_startup'; screenshot='08-settings-performance-startup.png' },
    [ordered]@{ section='backup'; screenshot='09-settings-backup.png' },
    [ordered]@{ section='backup_after_restore'; screenshot='09b-settings-backup-after-restore.png' },
    [ordered]@{ section='encryption_passphrase'; screenshot='09c-settings-encryption-passphrase.png' },
    [ordered]@{ section='plugin_card'; screenshot='10-settings-plugin-card.png' },
    [ordered]@{ section='plugin_toggle'; screenshot='11-settings-after-plugin-toggle.png' },
    [ordered]@{ section='plugin_uninstall'; screenshot='11b-settings-after-plugin-uninstall.png' },
    [ordered]@{ section='plugin_native_install'; screenshot='11c-settings-after-plugin-install.png' }
  )
  $rows = New-Object System.Collections.ArrayList
  foreach ($section in $sections) {
    $shotPath = Join-Path $proofDir $section.screenshot
    $item = Get-Item -LiteralPath $shotPath -ErrorAction SilentlyContinue
    if (-not $item) {
      [void]$rows.Add([ordered]@{
        section = $section.section
        screenshot = $section.screenshot
        passed = $false
        error = 'missing screenshot'
      })
      continue
    }
    $bmp = [System.Drawing.Bitmap]::new($shotPath)
    try {
      $bodyRange = Get-LuminanceRange $bmp 170 74 460 430 8
      $footerRange = Get-LuminanceRange $bmp 180 536 440 40 8
      $passed = (
        $item.Length -gt 5000 -and
        $bmp.Width -eq 800 -and
        $bmp.Height -eq 600 -and
        $bodyRange.range -ge 8.0 -and
        $footerRange.range -ge 4.0
      )
      [void]$rows.Add([ordered]@{
        section = $section.section
        screenshot = $section.screenshot
        screenshot_path = $shotPath
        passed = [bool]$passed
        bytes = [int64]$item.Length
        viewport = [ordered]@{ width=$bmp.Width; height=$bmp.Height }
        body_luminance = $bodyRange
        footer_luminance = $footerRange
      })
    } finally {
      $bmp.Dispose()
    }
  }
  $failed = @($rows.ToArray() | Where-Object { -not $_.passed })
  return [ordered]@{
    accepted = ($failed.Count -eq 0 -and $geometryTests -and [bool]$geometryTests.ok)
    source_contract = 'settings_panel geometry tests plus runtime section screenshots'
    geometry_tests = $geometryTests
    section_count = [int]$rows.Count
    required_section_count = [int]$sections.Count
    failed_section_count = [int]$failed.Count
    sections = @($rows.ToArray())
  }
}

function Post-Quit($win) {
  [void][NativeProof0608Settings]::PostMessageW([IntPtr]$win.hwnd, [NativeProof0608Settings]::WM_HOTKEY, [UIntPtr]([uint64]16973), [IntPtr]::Zero)
  Start-Sleep -Milliseconds 200
}

function Read-ProofText([string]$path) {
  if (Test-Path -LiteralPath $path) {
    return Get-Content -LiteralPath $path -Raw
  }
  return ''
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

$stage = 'started'
$proc = $null
$main = $null
$settings = $null
$clicks = New-Object System.Collections.ArrayList
$wheels = New-Object System.Collections.ArrayList
$screenshots = New-Object System.Collections.ArrayList
$stderrAll = ''
$stdoutAll = ''
$processExitedAfterQuitHotkey = $false
$firstSettingsClosedAfterSave = $false
$secondSettingsClosedAfterSave = $false
$firstVaultAfterSave = $null
$vaultBeforeSave = $null
$vaultAfterExit = $null
$vaultWireAfterFirstSave = $null
$vaultWireAfterEncryption = $null
$pluginAfterToggle = $null
$installedPluginAfterInstall = $null
$installedPluginAfterExit = $null
$pluginInstallDialog = $null
$footerProof = $null
$visualMatrixGeometryTests = Invoke-VisualMatrixGeometryTests
$visualMatrixProof = $null

try {
  $proc = Start-Target
  $main = Wait-Window $proc.Id 'BentoNanoShell' 10000
  if (-not $main) { throw 'main window not found' }
  Start-Sleep -Milliseconds 1200

  $stage = 'open-settings'
  [void][NativeProof0608Settings]::PostMessageW([IntPtr]$main.hwnd, [NativeProof0608Settings]::WM_HOTKEY, [UIntPtr]([uint64]16971), [IntPtr]::Zero)
  $settings = Wait-Window $proc.Id 'BentoAuxSets' 10000
  if (-not $settings) { throw 'settings window not found after hotkey 16971' }
  Start-Sleep -Milliseconds 900
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '03-settings-top.png') | Out-Null
  [void]$screenshots.Add('03-settings-top.png')

  $stage = 'top-and-path'
  [void]$clicks.Add((Send-ClientClick $settings 590 262 'toggle-portable-mode'))
  foreach ($event in Send-Wheel $settings 'down' 2) { [void]$wheels.Add($event) }
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '04-settings-paths-visible.png') | Out-Null
  [void]$screenshots.Add('04-settings-paths-visible.png')
  [void]$clicks.Add((Send-ClientClick $settings 250 330 'focus-desktop-path'))
  Send-Text $settings '\Proof'

  $stage = 'appearance-theme'
  foreach ($event in Send-Wheel $settings 'down' 4) { [void]$wheels.Add($event) }
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '05-settings-appearance-theme-grid.png') | Out-Null
  [void]$screenshots.Add('05-settings-appearance-theme-grid.png')
  [void]$clicks.Add((Send-ClientClick $settings 343 231 'select-theme-card'))
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '06-settings-appearance-after-theme.png') | Out-Null
  [void]$screenshots.Add('06-settings-appearance-after-theme.png')

  $stage = 'appearance-accent'
  foreach ($event in Send-Wheel $settings 'down' 5) { [void]$wheels.Add($event) }
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '07-settings-appearance-accent-row.png') | Out-Null
  [void]$screenshots.Add('07-settings-appearance-accent-row.png')
  [void]$clicks.Add((Send-ClientClick $settings 468 108 'select-accent-swatch'))

  $stage = 'performance-startup'
  [void]$clicks.Add((Send-ClientClick $settings 400 295 'drag-performance-slider-expand'))
  [void]$clicks.Add((Send-ClientClick $settings 420 339 'drag-performance-slider-collapse'))
  [void]$clicks.Add((Send-ClientClick $settings 440 383 'drag-performance-slider-icon-cache'))
  [void]$clicks.Add((Send-ClientClick $settings 590 464 'toggle-startup-high-priority'))
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '08-settings-performance-startup.png') | Out-Null
  [void]$screenshots.Add('08-settings-performance-startup.png')
  $footerProof = Analyze-StickyFooterShot '08-settings-performance-startup.png'

  $stage = 'first-save'
  if (Test-Path -LiteralPath $vaultPath) { $vaultBeforeSave = Get-Item -LiteralPath $vaultPath }
  [void]$clicks.Add((Send-ClientClick $settings 578 556 'save-settings-first-pass' 900))
  $firstSettingsClosedAfterSave = [bool](Wait-Condition {
    $candidate = Get-WindowsForPid $proc.Id | Where-Object { $_.class -eq 'BentoAuxSets' -and $_.visible } | Select-Object -First 1
    return (-not $candidate)
  } 5000)
  if (Test-Path -LiteralPath $vaultPath) { $firstVaultAfterSave = Get-Item -LiteralPath $vaultPath }
  if (-not $firstSettingsClosedAfterSave) { throw 'settings did not close after first SaveSettings click' }
  if (-not $firstVaultAfterSave) { throw 'vault.bin was not created after first SaveSettings click' }
  $vaultWireAfterFirstSave = Get-VaultWireRecord $vaultPath

  $stage = 'reopen-settings'
  [void][NativeProof0608Settings]::PostMessageW([IntPtr]$main.hwnd, [NativeProof0608Settings]::WM_HOTKEY, [UIntPtr]([uint64]16971), [IntPtr]::Zero)
  $settings = Wait-Window $proc.Id 'BentoAuxSets' 10000
  if (-not $settings) { throw 'settings window not found after second hotkey 16971' }
  Start-Sleep -Milliseconds 900

  $stage = 'backup'
  foreach ($event in Send-Wheel $settings 'down' 9) { [void]$wheels.Add($event) }
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '09-settings-backup.png') | Out-Null
  [void]$screenshots.Add('09-settings-backup.png')
  [void]$clicks.Add((Send-ClientClick $settings 232 200 'create-settings-backup' 850))
  [void]$clicks.Add((Send-ClientClick $settings 334 200 'list-settings-backups' 650))
  [void]$clicks.Add((Send-ClientClick $settings 590 263 'restore-settings-backup' 900))
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '09b-settings-backup-after-restore.png') | Out-Null
  [void]$screenshots.Add('09b-settings-backup-after-restore.png')

  $stage = 'encryption'
  [void]$clicks.Add((Send-ClientClick $settings 590 476 'focus-encryption-passphrase' 250))
  Send-Text $settings 'correct horse runtime proof'
  [void]$clicks.Add((Send-ClientClick $settings 549 421 'apply-encryption-passphrase' 1400))
  $vaultWireAfterEncryption = Get-VaultWireRecord $vaultPath
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '09c-settings-encryption-passphrase.png') | Out-Null
  [void]$screenshots.Add('09c-settings-encryption-passphrase.png')

  $stage = 'plugins'
  foreach ($event in Send-Wheel $settings 'down' 8) { [void]$wheels.Add($event) }
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '10-settings-plugin-card.png') | Out-Null
  [void]$screenshots.Add('10-settings-plugin-card.png')
  [void]$clicks.Add((Send-ClientClick $settings 590 422 'toggle-runtime-plugin' 650))
  $pluginAfterToggle = Get-PluginRegistryState 'com.test.runtime-theme'
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '11-settings-after-plugin-toggle.png') | Out-Null
  [void]$screenshots.Add('11-settings-after-plugin-toggle.png')
  [void]$clicks.Add((Send-ClientClick $settings 578 482 'uninstall-runtime-plugin' 900))
  Force-Paint $settings
  Save-WindowShot $settings (Join-Path $proofDir '11b-settings-after-plugin-uninstall.png') | Out-Null
  [void]$screenshots.Add('11b-settings-after-plugin-uninstall.png')
  if ($attemptNativePluginInstall) {
    [void]$clicks.Add((Post-ClientClick $settings 400 380 'install-runtime-plugin' 400))
    $pluginInstallDialog = Submit-OpenFileDialog $proc.Id $installPluginArchive
    $installedPluginAfterInstall = Wait-Condition {
      $state = Get-PluginRegistryState $installPluginId
      if ($state.present) { return $state }
      return $null
    } 7000
    if (-not $installedPluginAfterInstall) { throw 'installed plugin did not appear in registry after native file picker install' }
    Force-Paint $settings
    Save-WindowShot $settings (Join-Path $proofDir '11c-settings-after-plugin-install.png') | Out-Null
    [void]$screenshots.Add('11c-settings-after-plugin-install.png')
  }

  $stage = 'save'
  [void]$clicks.Add((Send-ClientClick $settings 578 556 'save-settings-second-pass' 900))
  $secondSettingsClosedAfterSave = [bool](Wait-Condition {
    $candidate = Get-WindowsForPid $proc.Id | Where-Object { $_.class -eq 'BentoAuxSets' -and $_.visible } | Select-Object -First 1
    return (-not $candidate)
  } 5000)
  Force-Paint $main
  Save-WindowShot $main (Join-Path $proofDir '12-main-after-settings-save.png') | Out-Null
  [void]$screenshots.Add('12-main-after-settings-save.png')

  $stage = 'quit'
  Post-Quit $main
  $processExitedAfterQuitHotkey = [bool](Wait-Condition { $proc.HasExited } 5000)
  if (-not $processExitedAfterQuitHotkey) { throw 'process did not exit after production quit hotkey' }
  $proc.WaitForExit(3000) | Out-Null
  $stderrAll = Read-ProofText $stderrPath
  $stdoutAll = Read-ProofText $stdoutPath

  if (Test-Path -LiteralPath $vaultPath) { $vaultAfterExit = Get-Item -LiteralPath $vaultPath }
  $backupFiles = @()
  if (Test-Path -LiteralPath $backupsDir) {
    $backupFiles = @(Get-ChildItem -LiteralPath $backupsDir -Filter 'vault-*.bin' -File -ErrorAction SilentlyContinue)
  }
  $pluginAfterExit = Get-PluginRegistryState 'com.test.runtime-theme'
  $installedPluginAfterExit = Get-PluginRegistryState $installPluginId

  $hits = [ordered]@{
    TogglePortableMode = Get-HitCount $stderrAll 'TogglePortableMode'
    EditDesktopPath = Get-HitCount $stderrAll 'EditDesktopPath'
    SelectTheme = Get-HitCount $stderrAll 'SelectTheme'
    SelectAccent = Get-HitCount $stderrAll 'SelectAccent'
    DragPerformanceSlider = Get-HitCount $stderrAll 'DragPerformanceSlider'
    DragPerformanceSliderIndex0 = ([regex]::Matches($stderrAll, 'hit=DragPerformanceSlider \{ index: 0,')).Count
    DragPerformanceSliderIndex1 = ([regex]::Matches($stderrAll, 'hit=DragPerformanceSlider \{ index: 1,')).Count
    DragPerformanceSliderIndex2 = ([regex]::Matches($stderrAll, 'hit=DragPerformanceSlider \{ index: 2,')).Count
    ToggleStartupHighPriority = Get-HitCount $stderrAll 'ToggleStartupHighPriority'
    CreateSettingsBackup = Get-HitCount $stderrAll 'CreateSettingsBackup'
    ListSettingsBackups = Get-HitCount $stderrAll 'ListSettingsBackups'
    RestoreSettingsBackup = Get-HitCount $stderrAll 'RestoreSettingsBackup'
    FocusPassphraseField = Get-HitCount $stderrAll 'FocusPassphraseField'
    SelectEncryptionModePassphrase = Get-HitCount $stderrAll 'SelectEncryptionModePassphrase'
    InstallPlugin = Get-HitCount $stderrAll 'InstallPlugin'
    TogglePlugin = Get-HitCount $stderrAll 'TogglePlugin'
    UninstallPlugin = Get-HitCount $stderrAll 'UninstallPlugin'
    SaveSettings = Get-HitCount $stderrAll 'SaveSettings'
    stderr_available = ($stderrAll.Length -gt 8)
  }
  $scrollLogCount = ([regex]::Matches($stderrAll, 'settings: scroll delta=')).Count
  $screenshotFiles = @($screenshots.ToArray() | ForEach-Object {
    $shotPath = Join-Path $proofDir $_
    $item = Get-Item -LiteralPath $shotPath -ErrorAction SilentlyContinue
    [ordered]@{ name=$_; bytes=if ($item) { [int64]$item.Length } else { 0 } }
  })
  $requiredShotNames = @(
    '03-settings-top.png',
    '04-settings-paths-visible.png',
    '05-settings-appearance-theme-grid.png',
    '07-settings-appearance-accent-row.png',
    '08-settings-performance-startup.png',
    '09-settings-backup.png',
    '10-settings-plugin-card.png',
    '11-settings-after-plugin-toggle.png',
    '11b-settings-after-plugin-uninstall.png',
    '11c-settings-after-plugin-install.png'
  )
  $nonBlankRequiredScreenshots = $true
  foreach ($name in $requiredShotNames) {
    $shot = $screenshotFiles | Where-Object { $_.name -eq $name } | Select-Object -First 1
    if (-not $shot -or $shot.bytes -le 5000) {
      $nonBlankRequiredScreenshots = $false
    }
  }
  $logs = [ordered]@{
    settings_opened_by_hotkey_id = 16971
    scroll_log_count = $scrollLogCount
    wheel_event_count = @($wheels.ToArray()).Count
    backup_created = ($stderrAll.Contains('settings backup created') -or $backupFiles.Count -ge 1)
    backup_created_stderr = $stderrAll.Contains('settings backup created')
    backup_created_artifact = ($backupFiles.Count -ge 1)
    plugin_installed = $stderrAll.Contains("plugins: InstallPlugin installed id=$installPluginId")
    plugin_toggled = $stderrAll.Contains('plugins: TogglePlugin id=com.test.runtime-theme')
    plugin_uninstalled = $stderrAll.Contains('plugins: UninstallPlugin removed id=com.test.runtime-theme')
    tray_registered = $stderrAll.Contains('tray: NIM_ADD registered')
  }
  $vault = [ordered]@{
    path = $vaultPath
    exists = [bool]$vaultAfterExit
    size_before_save = if ($vaultBeforeSave) { [int64]$vaultBeforeSave.Length } else { 0 }
    size_after_first_save = if ($firstVaultAfterSave) { [int64]$firstVaultAfterSave.Length } else { 0 }
    size_after = if ($vaultAfterExit) { [int64]$vaultAfterExit.Length } else { 0 }
    last_write_before_save_utc = if ($vaultBeforeSave) { $vaultBeforeSave.LastWriteTimeUtc.ToString('o') } else { $null }
    last_write_after_first_save_utc = if ($firstVaultAfterSave) { $firstVaultAfterSave.LastWriteTimeUtc.ToString('o') } else { $null }
    last_write_after_utc = if ($vaultAfterExit) { $vaultAfterExit.LastWriteTimeUtc.ToString('o') } else { $null }
    wire_after_first_save = $vaultWireAfterFirstSave
    wire_after_encryption = $vaultWireAfterEncryption
  }
  $plugin = [ordered]@{
    registry_path = $registryPath
    registry_exists = [bool]$pluginAfterExit.registry_exists
    id = 'com.test.runtime-theme'
    enabled_after_toggle = if ($pluginAfterToggle) { $pluginAfterToggle.enabled } else { $null }
    present_after_toggle = if ($pluginAfterToggle) { $pluginAfterToggle.present } else { $false }
    present_after_uninstall = [bool]$pluginAfterExit.present
    removed_after_uninstall = ($pluginAfterToggle -and $pluginAfterToggle.present -and -not $pluginAfterExit.present)
    install_archive_path = $installPluginArchive
    install_id = $installPluginId
    installed_plugin_dir = (Join-Path $stateDir "plugins\$installPluginId")
    installed_plugin_dir_exists = (Test-Path -LiteralPath (Join-Path $stateDir "plugins\$installPluginId") -PathType Container)
    installed_manifest_exists = (Test-Path -LiteralPath (Join-Path $stateDir "plugins\$installPluginId\manifest.json") -PathType Leaf)
    install_dialog = $pluginInstallDialog
    installed_plugin = $installedPluginAfterInstall
    installed_plugin_after_exit = $installedPluginAfterExit
    after_toggle = $pluginAfterToggle
    after_exit = $pluginAfterExit
  }
  $visualMatrixProof = Analyze-FullSectionVisualMatrix $visualMatrixGeometryTests
  $assertions = [ordered]@{
    settings_window_class = ($settings -and $settings.class -eq 'BentoAuxSets')
    first_settings_closed_after_save = $firstSettingsClosedAfterSave
    second_settings_closed_after_save = $secondSettingsClosedAfterSave
    native_wheel_messages_sent = (@($wheels.ToArray()).Count -ge 10)
    all_performance_sliders_hit = ($hits.DragPerformanceSliderIndex0 -ge 1 -and $hits.DragPerformanceSliderIndex1 -ge 1 -and $hits.DragPerformanceSliderIndex2 -ge 1)
    nonblank_required_screenshots = $nonBlankRequiredScreenshots
    vault_written = ($vault.exists -and $vault.size_after -gt 0)
    vault_created_after_first_save = ([bool]$firstVaultAfterSave -and $vault.size_after_first_save -gt 0)
    backup_file_created = ($backupFiles.Count -ge 1)
    backup_restore_hit = ($hits.RestoreSettingsBackup -ge 1)
    encryption_passphrase_runtime_hit = ($hits.FocusPassphraseField -ge 1 -and $hits.SelectEncryptionModePassphrase -ge 1)
    vault_passphrase_mode_written = ($vault.wire_after_encryption -and $vault.wire_after_encryption.mode_tag -eq 2 -and $vault.wire_after_encryption.sha256 -ne $vault.wire_after_first_save.sha256)
    plugin_disabled_in_registry = ($plugin.registry_exists -and $plugin.id -eq 'com.test.runtime-theme' -and $plugin.enabled_after_toggle -eq $false)
    plugin_uninstalled_from_registry = ($plugin.removed_after_uninstall -and $hits.UninstallPlugin -ge 1)
    plugin_installed_from_native_picker = ($plugin.installed_plugin -and $plugin.installed_plugin.present -and $plugin.installed_plugin_after_exit -and $plugin.installed_plugin_after_exit.present -and $plugin.installed_plugin_dir_exists -and $plugin.installed_manifest_exists -and $hits.InstallPlugin -ge 1)
    sticky_footer_geometry_pixels = ($footerProof -and $footerProof.passed)
    full_section_visual_matrix = ($visualMatrixProof -and $visualMatrixProof.accepted)
    process_exited_after_quit_hotkey = $processExitedAfterQuitHotkey
  }
  $summaryStatus = if (
    $assertions.settings_window_class -and
    $assertions.first_settings_closed_after_save -and
    $assertions.second_settings_closed_after_save -and
    $assertions.native_wheel_messages_sent -and
    $assertions.nonblank_required_screenshots -and
    $assertions.vault_written -and
    $assertions.vault_created_after_first_save -and
    $assertions.backup_file_created -and
    $assertions.backup_restore_hit -and
    $assertions.encryption_passphrase_runtime_hit -and
    $assertions.vault_passphrase_mode_written -and
    $assertions.plugin_disabled_in_registry -and
    $assertions.plugin_uninstalled_from_registry -and
    $assertions.plugin_installed_from_native_picker -and
    $assertions.sticky_footer_geometry_pixels -and
    $assertions.full_section_visual_matrix -and
    $assertions.process_exited_after_quit_hotkey
  ) { 'ok' } else { 'failed' }

  $summary = [ordered]@{
    status = $summaryStatus
    stage = 'completed'
    exe = $proofExe
    source_exe = $sourceExe
    state_dir = $stateDir
    opened_via_hotkey_id = 16971
    quit_via_hotkey_id = 16973
    main_window = Convert-WindowForJson $main
    settings_window = Convert-WindowForJson $settings
    hits = $hits
    logs = $logs
    assertions = $assertions
    vault = $vault
    backups = [ordered]@{
      dir = $backupsDir
      count = [int]$backupFiles.Count
      files = @($backupFiles | ForEach-Object { [ordered]@{ name=$_.Name; bytes=[int64]$_.Length } })
    }
    plugin = $plugin
    footer_proof = $footerProof
    visual_matrix = $visualMatrixProof
    clicks = @($clicks.ToArray())
    wheels = @($wheels.ToArray())
    screenshots = $screenshotFiles
  }
  $summary | ConvertTo-Json -Depth 12 | Out-File -FilePath $summaryPath -Encoding utf8
  if ($summary.status -ne 'ok') { throw 'runtime proof assertions failed; see summary.json and stderr.log' }
} catch {
  $message = $_.Exception.Message
  if ($proc -and -not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    $proc.WaitForExit(3000) | Out-Null
  }
  if ($proc) {
    try {
      $stderrAll = Read-ProofText $stderrPath
      $stdoutAll = Read-ProofText $stdoutPath
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
    settings_window = Convert-WindowForJson $settings
    clicks = @($clicks.ToArray())
    wheels = @($wheels.ToArray())
    screenshots = @($screenshots.ToArray())
    visual_matrix_geometry_tests = $visualMatrixGeometryTests
  }
  $summary | ConvertTo-Json -Depth 12 | Out-File -FilePath $summaryPath -Encoding utf8
  throw
}
