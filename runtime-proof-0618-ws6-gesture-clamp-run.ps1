$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$workspaceRoot = Split-Path -Parent $repoRoot
$sourceExe = Join-Path $repoRoot 'target\x86_64-pc-windows-msvc\debug\bento-nano-shell.exe'
$stateDir = Join-Path $repoRoot 'runtime-proof-0618-ws6-gesture-clamp-state'
$proofDir = Join-Path $repoRoot 'runtime-proof-0618-ws6-gesture-clamp-try'
$proofExe = Join-Path $proofDir 'bento-nano-shell-ws6-gesture-clamp-proof.exe'
$itemRoot = Join-Path $stateDir 'items'
$zonesPath = Join-Path $stateDir 'zones.bin'
$stderrPath = Join-Path $proofDir 'stderr.log'
$stdoutPath = Join-Path $proofDir 'stdout.log'
$summaryPath = Join-Path $proofDir 'summary.json'
$manifestPath = Join-Path $repoRoot 'Cargo.toml'
$targetTriple = 'x86_64-pc-windows-msvc'

$animationSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-animation-state-arbitration-try\summary.json'
$mergeDissolveSummaryPath = Join-Path $repoRoot 'runtime-proof-0608-merge-dissolve-scatter-try\summary.json'
$stackDragSummaryPath = Join-Path $repoRoot 'runtime-proof-0608-stack-drag-visual-try\summary.json'

function Assert-UnderPath([string]$Path, [string]$Parent) {
    $parentFull = [System.IO.Path]::GetFullPath($Parent).TrimEnd('\') + '\'
    $pathFull = [System.IO.Path]::GetFullPath($Path)
    if (-not $pathFull.StartsWith($parentFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing path outside proof workspace: $Path"
    }
}

Assert-UnderPath $stateDir $repoRoot
Assert-UnderPath $proofDir $repoRoot

function Write-Utf8NoBom {
    param(
        [string]$Path,
        [string]$Text
    )

    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

function Read-JsonPath {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }
    $text = [System.IO.File]::ReadAllText($Path).TrimStart([char]0xFEFF)
    return $text | ConvertFrom-Json
}

function Invoke-CargoCommand {
    param(
        [string]$Id,
        [string[]]$Arguments,
        [string]$Role
    )

    $logPath = Join-Path $proofDir "$Id.log"
    $env:CARGO_BUILD_JOBS = '1'
    $env:CARGO_INCREMENTAL = '0'

    $oldErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $output = & cargo @Arguments 2>&1
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $oldErrorActionPreference

    Write-Utf8NoBom $logPath (($output | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine)
    return [pscustomobject]@{
        id = $Id
        role = $Role
        command = "cargo $($Arguments -join ' ')"
        exit_code = [int]$exitCode
        passed = ($exitCode -eq 0)
        log = $logPath
    }
}

function Invoke-FocusedTest {
    param(
        [string]$Id,
        [string[]]$Arguments,
        [string]$Role
    )

    return Invoke-CargoCommand $Id $Arguments $Role
}

function Test-SourceContains {
    param(
        [string]$RelativePath,
        [string]$Pattern,
        [string]$Description
    )

    $path = Join-Path $repoRoot $RelativePath
    $text = if (Test-Path -LiteralPath $path) {
        [System.IO.File]::ReadAllText($path)
    } else {
        ''
    }
    return [pscustomobject]@{
        path = $RelativePath
        description = $Description
        pattern = $Pattern
        passed = [bool]([regex]::IsMatch($text, $Pattern, [Text.RegularExpressions.RegexOptions]::Singleline))
    }
}

function Clamp-Int([int]$Value, [int]$Min, [int]$Max) {
    $lo = [Math]::Min($Min, $Max)
    $hi = [Math]::Max($Min, $Max)
    if ($Value -lt $lo) { return $lo }
    if ($Value -gt $hi) { return $hi }
    return $Value
}

function Convert-RectForJson($Rect) {
    return [ordered]@{
        left = [int]$Rect.left
        top = [int]$Rect.top
        right = [int]$Rect.right
        bottom = [int]$Rect.bottom
        width = [int]($Rect.right - $Rect.left)
        height = [int]($Rect.bottom - $Rect.top)
    }
}

function Get-UnionBounds($Monitors) {
    $usable = @($Monitors | Where-Object {
        ($_.work.right - $_.work.left) -gt 0 -and ($_.work.bottom - $_.work.top) -gt 0
    })
    if ($usable.Count -eq 0) {
        return $null
    }
    $left = ($usable | ForEach-Object { $_.work.left } | Measure-Object -Minimum).Minimum
    $top = ($usable | ForEach-Object { $_.work.top } | Measure-Object -Minimum).Minimum
    $right = ($usable | ForEach-Object { $_.work.right } | Measure-Object -Maximum).Maximum
    $bottom = ($usable | ForEach-Object { $_.work.bottom } | Measure-Object -Maximum).Maximum
    return [ordered]@{
        left = [int]$left
        top = [int]$top
        right = [int]$right
        bottom = [int]$bottom
        width = [int]($right - $left)
        height = [int]($bottom - $top)
    }
}

function New-LParam([int]$X, [int]$Y) {
    return [IntPtr](((($Y -band 0xffff) -shl 16) -bor ($X -band 0xffff)))
}

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class NativeProof0618Ws6 {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  public delegate bool MonitorEnumProc(IntPtr hMonitor, IntPtr hdcMonitor, ref RECT lprcMonitor, IntPtr dwData);
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
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr dpiContext);
  [DllImport("user32.dll")] public static extern bool EnumDisplayMonitors(IntPtr hdc, IntPtr lprcClip, MonitorEnumProc lpfnEnum, IntPtr dwData);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfo(IntPtr hMonitor, ref MONITORINFO lpmi);
  public const uint SMTO_ABORTIFHUNG = 0x0002;
  public const uint WM_HOTKEY = 0x0312;
  public const uint WM_MOUSEMOVE = 0x0200;
  public const uint WM_LBUTTONDOWN = 0x0201;
  public const uint WM_LBUTTONUP = 0x0202;
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int left; public int top; public int right; public int bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct MONITORINFO { public int cbSize; public RECT rcMonitor; public RECT rcWork; public int dwFlags; }
}
"@

function Get-OsMonitors {
    # PER_MONITOR_AWARE_V2. The selected-stack shell is DPI aware, so the proof
    # process must opt into the same coordinate space before recording rcWork.
    [void][NativeProof0618Ws6]::SetProcessDpiAwarenessContext([IntPtr](-4))
    $items = New-Object System.Collections.ArrayList
    $cb = [NativeProof0618Ws6+MonitorEnumProc]{
        param([IntPtr]$hMonitor, [IntPtr]$hdcMonitor, [ref]$rect, [IntPtr]$data)
        $mi = New-Object NativeProof0618Ws6+MONITORINFO
        $mi.cbSize = [Runtime.InteropServices.Marshal]::SizeOf([type][NativeProof0618Ws6+MONITORINFO])
        [void][NativeProof0618Ws6]::GetMonitorInfo($hMonitor, [ref]$mi)
        [void]$items.Add([pscustomobject]@{
            index = [int]$items.Count
            screen = [pscustomobject]@{
                left = $mi.rcMonitor.left
                top = $mi.rcMonitor.top
                right = $mi.rcMonitor.right
                bottom = $mi.rcMonitor.bottom
            }
            work = [pscustomobject]@{
                left = $mi.rcWork.left
                top = $mi.rcWork.top
                right = $mi.rcWork.right
                bottom = $mi.rcWork.bottom
            }
            is_primary = (($mi.dwFlags -band 1) -ne 0)
        })
        return $true
    }
    [void][NativeProof0618Ws6]::EnumDisplayMonitors([IntPtr]::Zero, [IntPtr]::Zero, $cb, [IntPtr]::Zero)
    return @($items.ToArray())
}

function Convert-WindowForJson($Win) {
    if (-not $Win) { return $null }
    return [ordered]@{
        hwnd = [int64]$Win.hwnd
        class = [string]$Win.class
        title = [string]$Win.title
        visible = [bool]$Win.visible
        dpi = [int]$Win.dpi
        rect = [ordered]@{
            left = [int]$Win.rect.left
            top = [int]$Win.rect.top
            right = [int]$Win.rect.right
            bottom = [int]$Win.rect.bottom
            width = [int]$Win.rect.width
            height = [int]$Win.rect.height
        }
        client = [ordered]@{
            width = [int]$Win.client.width
            height = [int]$Win.client.height
        }
    }
}

function Get-WindowsForPid([int]$ProcessId) {
    $items = New-Object System.Collections.ArrayList
    $cb = [NativeProof0618Ws6+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$lparam)
        [uint32]$wpid = 0
        [void][NativeProof0618Ws6]::GetWindowThreadProcessId($hwnd, [ref]$wpid)
        if ($wpid -eq [uint32]$ProcessId) {
            $class = New-Object System.Text.StringBuilder 256
            $title = New-Object System.Text.StringBuilder 256
            [void][NativeProof0618Ws6]::GetClassName($hwnd, $class, $class.Capacity)
            [void][NativeProof0618Ws6]::GetWindowText($hwnd, $title, $title.Capacity)
            $rect = New-Object NativeProof0618Ws6+RECT
            $client = New-Object NativeProof0618Ws6+RECT
            [void][NativeProof0618Ws6]::GetWindowRect($hwnd, [ref]$rect)
            [void][NativeProof0618Ws6]::GetClientRect($hwnd, [ref]$client)
            [void]$items.Add([pscustomobject]@{
                hwnd = $hwnd.ToInt64()
                class = $class.ToString()
                title = $title.ToString()
                visible = [NativeProof0618Ws6]::IsWindowVisible($hwnd)
                dpi = [NativeProof0618Ws6]::GetDpiForWindow($hwnd)
                rect = [pscustomobject]@{
                    left = $rect.left
                    top = $rect.top
                    right = $rect.right
                    bottom = $rect.bottom
                    width = ($rect.right - $rect.left)
                    height = ($rect.bottom - $rect.top)
                }
                client = [pscustomobject]@{
                    width = ($client.right - $client.left)
                    height = ($client.bottom - $client.top)
                }
            })
        }
        return $true
    }
    [void][NativeProof0618Ws6]::EnumWindows($cb, [IntPtr]::Zero)
    return @($items.ToArray())
}

function Wait-Window([int]$ProcessId, [string]$Class, [int]$TimeoutMs = 10000) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.ElapsedMilliseconds -lt $TimeoutMs) {
        $win = Get-WindowsForPid $ProcessId |
            Where-Object { $_.class -eq $Class -and $_.visible } |
            Select-Object -First 1
        if ($win) { return $win }
        Start-Sleep -Milliseconds 100
    }
    return $null
}

function Send-ClientMessage($Win, [uint32]$Msg, [double]$ClientXValue, [double]$ClientYValue, [string]$Mode = 'send', [int]$SleepMs = 140) {
    $clientX = [int][Math]::Round($ClientXValue)
    $clientY = [int][Math]::Round($ClientYValue)
    [void][NativeProof0618Ws6]::SetForegroundWindow([IntPtr]$Win.hwnd)
    [void][NativeProof0618Ws6]::SetCursorPos([int]($Win.rect.left + $clientX), [int]($Win.rect.top + $clientY))
    Start-Sleep -Milliseconds 30
    $lp = New-LParam $clientX $clientY
    $sendResult = $null
    if ($Mode -eq 'post') {
        [void][NativeProof0618Ws6]::PostMessageW([IntPtr]$Win.hwnd, $Msg, [UIntPtr]::Zero, $lp)
    } else {
        $nativeResult = [UIntPtr]::Zero
        $sendResult = [NativeProof0618Ws6]::SendMessageTimeoutW(
            [IntPtr]$Win.hwnd,
            $Msg,
            [UIntPtr]::Zero,
            $lp,
            [NativeProof0618Ws6]::SMTO_ABORTIFHUNG,
            2500,
            [ref]$nativeResult
        )
        if ($sendResult -eq [IntPtr]::Zero) {
            throw "SendMessageTimeout timed out for msg=$Msg client=($clientX,$clientY)"
        }
    }
    Start-Sleep -Milliseconds $SleepMs
    return [ordered]@{
        msg = [int]$Msg
        client_x_value = [double]$ClientXValue
        client_y_value = [double]$ClientYValue
        client_x = $clientX
        client_y = $clientY
        dpi = [int]$Win.dpi
        input_coordinate_space = 'raw-client'
        mode = $Mode
        send_result = if ($sendResult -eq $null) { $null } else { $sendResult.ToInt64() }
    }
}

function Request-Paint($Win, [int]$SleepMs = 120) {
    if (-not $Win) { return }
    [void][NativeProof0618Ws6]::InvalidateRect([IntPtr]$Win.hwnd, [IntPtr]::Zero, $false)
    Start-Sleep -Milliseconds $SleepMs
}

function Save-WindowShot($Win, [string]$Path) {
    if (-not $Win) { return $false }
    $w = [Math]::Max(1, [int]$Win.rect.width)
    $h = [Math]::Max(1, [int]$Win.rect.height)
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    try {
        $g.CopyFromScreen([int]$Win.rect.left, [int]$Win.rect.top, 0, 0, [System.Drawing.Size]::new($w, $h))
        $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
        return $true
    } finally {
        $g.Dispose()
        $bmp.Dispose()
    }
}

function Dump-Example([string]$Example, [string]$FileName) {
    $path = Join-Path $proofDir $FileName
    Push-Location $workspaceRoot
    try {
        $output = & cargo run --quiet --manifest-path bentodesk-nano/Cargo.toml -p bento-nano-platform --example $Example --target $targetTriple -- $zonesPath 2>&1
        Write-Utf8NoBom $path (($output | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine)
        if ($LASTEXITCODE -ne 0) {
            throw "$Example failed with exit code $LASTEXITCODE"
        }
        return @($output | ForEach-Object { $_.ToString() })
    } finally {
        Pop-Location
    }
}

function Parse-ZoneGeometry([string[]]$Lines, [int]$Id) {
    foreach ($line in $Lines) {
        $cols = $line -split "`t", -1
        if ($cols.Count -lt 8) { continue }
        $rowId = $cols[0].TrimStart([char]0xFEFF)
        if ($rowId -eq [string]$Id) {
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

function Read-LiveMoves([string]$Text) {
    $moves = @()
    foreach ($m in [regex]::Matches($Text, 'drag: live_move zone_id=(?<zone>\d+) x=(?<x>-?\d+) y=(?<y>-?\d+) now_ms=(?<now>\d+)')) {
        $moves += [pscustomobject]@{
            zone_id = [int]$m.Groups['zone'].Value
            x = [int]$m.Groups['x'].Value
            y = [int]$m.Groups['y'].Value
            now_ms = [int]$m.Groups['now'].Value
        }
    }
    return @($moves)
}

function New-EdgeCaseResult {
    param(
        [string]$Name,
        [object]$Input,
        [object]$Move,
        [object]$Union,
        [object]$Zone
    )

    $maxX = [int]($Union.right - $Zone.w)
    $maxY = [int]($Union.bottom - $Zone.h)
    $xWithin = $Move -and $Move.x -ge $Union.left -and $Move.x -le $maxX
    $yWithin = $Move -and $Move.y -ge $Union.top -and $Move.y -le $maxY
    $pass = $false
    $expected = [ordered]@{
        x_min = [int]$Union.left
        y_min = [int]$Union.top
        x_max = [int]$maxX
        y_max = [int]$maxY
    }

    switch ($Name) {
        'left' {
            $expected['x'] = [int]$Union.left
            $pass = $Move -and $Move.x -eq $Union.left -and $yWithin
        }
        'top' {
            $expected['y'] = [int]$Union.top
            $pass = $Move -and $Move.y -eq $Union.top -and $xWithin
        }
        'right' {
            $expected['x'] = [int]$maxX
            $pass = $Move -and $Move.x -eq $maxX -and $yWithin
        }
        'bottom' {
            $expected['y'] = [int]$maxY
            $pass = $Move -and $Move.y -eq $maxY -and $xWithin
        }
        'right_bottom' {
            $expected['x'] = [int]$maxX
            $expected['y'] = [int]$maxY
            $pass = $Move -and $Move.x -eq $maxX -and $Move.y -eq $maxY
        }
    }

    return [ordered]@{
        name = $Name
        input = $Input
        clamped = if ($Move) { [ordered]@{ x = [int]$Move.x; y = [int]$Move.y; now_ms = [int]$Move.now_ms } } else { $null }
        expected = $expected
        passed = [bool]$pass
    }
}

function Post-Quit($Win) {
    [void][NativeProof0618Ws6]::PostMessageW([IntPtr]$Win.hwnd, [NativeProof0618Ws6]::WM_HOTKEY, [UIntPtr]([uint64]16973), [IntPtr]::Zero)
    Start-Sleep -Milliseconds 250
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
$screenshots = New-Object System.Collections.ArrayList
$processExitedAfterQuitHotkey = $false

try {
    New-Item -ItemType Directory -Force -Path $proofDir | Out-Null
    Get-ChildItem -LiteralPath $proofDir -Force -ErrorAction SilentlyContinue |
        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $stateDir) {
        Remove-Item -LiteralPath $stateDir -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $stateDir | Out-Null

    $build = Invoke-CargoCommand 'build-shell-debug' @(
        'build',
        '--manifest-path', $manifestPath,
        '-p', 'bento-nano-shell',
        '--target', $targetTriple
    ) 'build current debug shell executable used by runtime proof'
    if (-not $build.passed) { throw 'debug shell build failed' }
    if (-not (Test-Path -LiteralPath $sourceExe)) { throw "source exe not found: $sourceExe" }
    Copy-Item -LiteralPath $sourceExe -Destination $proofExe -Force

    $tests = @()
    $tests += Invoke-FocusedTest 'platform-monitor-smoke' @('test', '--manifest-path', $manifestPath, '-p', 'bento-nano-platform', '--test', 'monitor_smoke', '--target', $targetTriple, '--', '--test-threads=1') 'real Win32 monitor enumeration smoke'
    $tests += Invoke-FocusedTest 'platform-zone-monitor-smoke' @('test', '--manifest-path', $manifestPath, '-p', 'bento-nano-platform', '--test', 'zone_monitor_smoke', '--target', $targetTriple, '--', '--test-threads=1') 'zone centre to monitor mapping'
    $tests += Invoke-FocusedTest 'platform-union-bounds' @('test', '--manifest-path', $manifestPath, '-p', 'bento-nano-platform', 'union_bounds', '--lib', '--target', $targetTriple, '--', '--test-threads=1') 'strict union-bounds clamp, including single-monitor edges and two-monitor seam'
    $tests += Invoke-FocusedTest 'shell-drag-clamp-smoke' @('test', '--manifest-path', $manifestPath, '-p', 'bento-nano-shell', '--test', 'phase25_drag_clamp_smoke', '--target', $targetTriple, '--', '--test-threads=1') 'shell drag clamp integration and real enumerate_monitors round-trip'
    $tests += Invoke-FocusedTest 'shell-live-geometry' @('test', '--manifest-path', $manifestPath, '-p', 'bento-nano-shell', 'live_drag_geometry_updates_memory_before_dispatcher_drain', '--bin', 'bento-nano-shell', '--target', $targetTriple, '--', '--test-threads=1') 'live in-memory geometry updates before dispatcher drain'
    $tests += Invoke-FocusedTest 'app-zone-drag-merge' @('test', '--manifest-path', $manifestPath, '-p', 'bento-nano-app', 'zone_drag_merge', '--target', $targetTriple, '--', '--test-threads=1') 'F2 drag-over-merge ghost eligibility'
    $testsPassed = @($tests | Where-Object { -not $_.passed }).Count -eq 0
    if (-not $testsPassed) { throw 'focused WS-6 tests failed' }

    $sourceContracts = @()
    $sourceContracts += Test-SourceContains 'crates\bento-nano-shell\src\main.rs' 'clamp_rect_into_union_bounds\(\s*nx,\s*ny,\s*z\.w,\s*z\.h,\s*&slot\.state\.monitors' 'WM_MOUSEMOVE zone drag path calls strict union-bounds clamp with live monitor cache'
    $sourceContracts += Test-SourceContains 'crates\bento-nano-shell\src\main.rs' 'drag: live_move zone_id=\{\} x=\{cx\} y=\{cy\} now_ms=\{now_ms\}' 'drag proof log emits clamped live geometry'
    $sourceContracts += Test-SourceContains 'crates\bento-nano-shell\src\main.rs' 'Command::DissolveStack\(id\).*?dissolve_stack_scattered' 'F2 dissolve command routes to scatter/clamp implementation'
    $sourceContracts += Test-SourceContains 'crates\bento-nano-platform\src\monitor.rs' 'union_bounds_two_side_by_side_monitors_roam_the_full_union' 'two-monitor seam and outer-edge contract is locked in platform tests'
    $sourceContractsPassed = @($sourceContracts | Where-Object { -not $_.passed }).Count -eq 0
    if (-not $sourceContractsPassed) { throw 'WS-6 source contract checks failed' }

    $env:BENTODESK_NANO_BENCHMARK_ITEM_ROOT = $itemRoot
    Push-Location $workspaceRoot
    try {
        $seedOutput = & cargo run --quiet --manifest-path bentodesk-nano/Cargo.toml -p bento-nano-platform --example seed_benchmark_scene --target $targetTriple -- $stateDir 2>&1
        Write-Utf8NoBom (Join-Path $proofDir '00-seed-benchmark-scene.txt') (($seedOutput | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine)
        if ($LASTEXITCODE -ne 0) { throw "seed_benchmark_scene failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
        Remove-Item Env:\BENTODESK_NANO_BENCHMARK_ITEM_ROOT -ErrorAction SilentlyContinue
    }

    $geomBefore = Dump-Example 'dump_zone_geometry' '01-geometry-before.tsv'
    $zone1Before = Parse-ZoneGeometry $geomBefore 1
    if (-not $zone1Before) { throw 'zone 1 geometry before clamp drag not found' }
    $dragStartWrite = (Get-Item -LiteralPath $zonesPath).LastWriteTimeUtc

    $osMonitors = Get-OsMonitors
    $unionBounds = Get-UnionBounds $osMonitors
    if (-not $unionBounds) { throw 'OS monitor union bounds unavailable' }

    $proc = Start-Target
    $main = Wait-Window $proc.Id 'BentoNanoShell' 10000
    if (-not $main) { throw 'main window not found' }
    Start-Sleep -Milliseconds 1200
    Request-Paint $main 200
    Save-WindowShot $main (Join-Path $proofDir '02-before-edge-drag.png') | Out-Null
    [void]$screenshots.Add('02-before-edge-drag.png')

    $stage = 'edge-clamp-drag'
    $dpiScale = [Math]::Max(1.0, ([double]$main.dpi) / 96.0)
    $dragStartRawX = [int][Math]::Round(([double]$zone1Before.x + 56.0) * $dpiScale)
    $dragStartRawY = [int][Math]::Round(([double]$zone1Before.y + 24.0) * $dpiScale)
    [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0618Ws6]::WM_MOUSEMOVE) $dragStartRawX $dragStartRawY 'send' 160))
    [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0618Ws6]::WM_LBUTTONDOWN) $dragStartRawX $dragStartRawY 'send' 160))

    $edgeInputs = @(
        [ordered]@{ name = 'left'; x = -1200; y = 220 },
        [ordered]@{ name = 'top'; x = 220; y = -1200 },
        [ordered]@{ name = 'right'; x = [int]$main.client.width + 1800; y = 220 },
        [ordered]@{ name = 'bottom'; x = 320; y = [int]$main.client.height + 1800 },
        [ordered]@{ name = 'right_bottom'; x = [int]$main.client.width + 1800; y = [int]$main.client.height + 1800 }
    )

    foreach ($edge in $edgeInputs) {
        [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0618Ws6]::WM_MOUSEMOVE) $edge.x $edge.y 'send' 160))
        Request-Paint $main 80
        $shot = Join-Path $proofDir ("03-edge-{0}.png" -f $edge.name)
        Save-WindowShot $main $shot | Out-Null
        [void]$screenshots.Add((Split-Path -Leaf $shot))
    }

    $dragMidWrite = (Get-Item -LiteralPath $zonesPath).LastWriteTimeUtc
    [void]$clicks.Add((Send-ClientMessage $main ([NativeProof0618Ws6]::WM_LBUTTONUP) ([int]$main.client.width + 1800) ([int]$main.client.height + 1800) 'send' 750))
    Request-Paint $main 200
    Save-WindowShot $main (Join-Path $proofDir '04-after-edge-drag-release.png') | Out-Null
    [void]$screenshots.Add('04-after-edge-drag-release.png')
    $dragReleaseWrite = (Get-Item -LiteralPath $zonesPath).LastWriteTimeUtc
    $geomAfter = Dump-Example 'dump_zone_geometry' '05-geometry-after-edge-drag.tsv'
    $zone1After = Parse-ZoneGeometry $geomAfter 1
    if (-not $zone1After) { throw 'zone 1 geometry after clamp drag not found' }

    Post-Quit $main
    $processExitedAfterQuitHotkey = $proc.WaitForExit(4000)

    $stderrAll = if (Test-Path -LiteralPath $stderrPath) {
        [System.IO.File]::ReadAllText($stderrPath)
    } else {
        ''
    }
    $stdoutAll = if (Test-Path -LiteralPath $stdoutPath) {
        [System.IO.File]::ReadAllText($stdoutPath)
    } else {
        ''
    }
    $liveMoves = @(Read-LiveMoves $stderrAll | Where-Object { $_.zone_id -eq 1 })
    $maxX = [int]($unionBounds.right - $zone1Before.w)
    $maxY = [int]($unionBounds.bottom - $zone1Before.h)
    $edgeCases = @()
    foreach ($edge in $edgeInputs) {
        $move = switch ($edge.name) {
            'left' { $liveMoves | Where-Object { $_.x -eq $unionBounds.left } | Select-Object -First 1 }
            'top' { $liveMoves | Where-Object { $_.y -eq $unionBounds.top } | Select-Object -First 1 }
            'right' { $liveMoves | Where-Object { $_.x -eq $maxX } | Select-Object -First 1 }
            'bottom' { $liveMoves | Where-Object { $_.y -eq $maxY } | Select-Object -First 1 }
            'right_bottom' { $liveMoves | Where-Object { $_.x -eq $maxX -and $_.y -eq $maxY } | Select-Object -First 1 }
        }
        $edgeCases += New-EdgeCaseResult $edge.name $edge $move $unionBounds $zone1Before
    }
    $edgeCasesPassed = $edgeCases.Count -eq $edgeInputs.Count -and @($edgeCases | Where-Object { -not $_.passed }).Count -eq 0

    $noWriteDuringDrag = ($dragMidWrite -eq $dragStartWrite)
    $writeAfterRelease = ($dragReleaseWrite -gt $dragMidWrite)

    $runtimeStrictBounds = [ordered]@{
        inferred_from = 'OS EnumDisplayMonitors rcWork; same coordinate space as current selected-stack HWND on this host'
        monitors = @($osMonitors | ForEach-Object {
            [ordered]@{
                index = [int]$_.index
                is_primary = [bool]$_.is_primary
                screen = Convert-RectForJson $_.screen
                work = Convert-RectForJson $_.work
            }
        })
        union_bounds = $unionBounds
        host_monitor_count = [int]@($osMonitors).Count
        seam_case = if (@($osMonitors).Count -gt 1) { 'covered_by_host_topology' } else { 'not_applicable_single_monitor' }
        deterministic_two_monitor_seam_test = 'monitor::tests::union_bounds_two_side_by_side_monitors_roam_the_full_union'
        edge_cases = $edgeCases
        all_passed = [bool]$edgeCasesPassed
    }

    $animationSummary = Read-JsonPath $animationSummaryPath
    $mergeSummary = Read-JsonPath $mergeDissolveSummaryPath
    $stackDragSummary = Read-JsonPath $stackDragSummaryPath
    $f2Sanity = (
        ($null -ne $mergeSummary) -and
        ($mergeSummary.status -eq 'ok') -and
        ($mergeSummary.stage -eq 'completed') -and
        ($mergeSummary.stack.after_merge_zone5_members_4 -eq $true) -and
        ($mergeSummary.stack.after_dissolve_zone4_5_independent -eq $true) -and
        ($mergeSummary.geometry.released_zones_within_viewport -eq $true) -and
        ($mergeSummary.logs.stack_zone_5_4 -eq $true) -and
        ($mergeSummary.logs.dissolve_stack_5 -eq $true) -and
        ($mergeSummary.process_exited_after_quit_hotkey -eq $true)
    )
    $releaseBoundRuntime = (
        $noWriteDuringDrag -and
        $writeAfterRelease -and
        ($null -ne $animationSummary) -and
        ($animationSummary.status -eq 'ok') -and
        ($animationSummary.drag.no_write_during_drag -eq $true) -and
        ($animationSummary.drag.write_after_release -eq $true) -and
        ($animationSummary.drag.live_move_log_count -ge 10) -and
        ($animationSummary.process_exited_after_quit_hotkey -eq $true) -and
        ($null -ne $stackDragSummary) -and
        ($stackDragSummary.status -eq 'ok') -and
        ($stackDragSummary.drag.no_write_during_drag -eq $true) -and
        ($stackDragSummary.drag.write_after_release -eq $true)
    )

    $summaryStatusOk = $testsPassed `
        -and $sourceContractsPassed `
        -and $runtimeStrictBounds.all_passed `
        -and $noWriteDuringDrag `
        -and $writeAfterRelease `
        -and ($liveMoves.Count -ge $edgeInputs.Count) `
        -and $f2Sanity `
        -and $releaseBoundRuntime `
        -and $processExitedAfterQuitHotkey

    $summary = [ordered]@{
        status = if ($summaryStatusOk) { 'ok' } else { 'failed' }
        stage = 'completed'
        ws_id = 'WS-6'
        no_mock_data = $true
        exe = $proofExe
        source_exe = $sourceExe
        state_dir = $stateDir
        main_window = Convert-WindowForJson $main
        ws6_gesture_clamp = [ordered]@{
            accepted = [bool]$summaryStatusOk
            runtime_strict_edge_clamp_pass = [bool]$runtimeStrictBounds.all_passed
            no_write_during_drag = [bool]$noWriteDuringDrag
            write_after_release = [bool]$writeAfterRelease
            f2_merge_dissolve_sanity = [bool]$f2Sanity
            release_bound_runtime_chain = [bool]$releaseBoundRuntime
            host_seam_policy = $runtimeStrictBounds.seam_case
        }
        clamp = $runtimeStrictBounds
        drag = [ordered]@{
            before = $zone1Before
            after = $zone1After
            start_raw_client = [ordered]@{
                x = [int]$dragStartRawX
                y = [int]$dragStartRawY
                dpi_scale = [double]$dpiScale
            }
            live_move_log_count = [int]$liveMoves.Count
            live_moves = @($liveMoves)
            zones_bin_write_time_utc_before = $dragStartWrite.ToString('o')
            zones_bin_write_time_utc_mid_drag = $dragMidWrite.ToString('o')
            zones_bin_write_time_utc_after_release = $dragReleaseWrite.ToString('o')
            no_write_during_drag = [bool]$noWriteDuringDrag
            write_after_release = [bool]$writeAfterRelease
        }
        f2 = [ordered]@{
            merge_dissolve_summary = $mergeDissolveSummaryPath
            stack_drag_summary = $stackDragSummaryPath
            animation_summary = $animationSummaryPath
            sanity_pass = [bool]$f2Sanity
        }
        tests = @($tests)
        build = $build
        source_contracts = @($sourceContracts)
        clicks = @($clicks.ToArray())
        screenshots = @($screenshots.ToArray())
        logs = [ordered]@{
            stderr = $stderrPath
            stdout = $stdoutPath
            tray_registered = $stderrAll.Contains('tray: NIM_ADD registered')
            drag_proof_log_seen = ($liveMoves.Count -gt 0)
        }
        process_exited_after_quit_hotkey = [bool]$processExitedAfterQuitHotkey
    }

    Write-Utf8NoBom $summaryPath ($summary | ConvertTo-Json -Depth 30)
    if ($summary.status -ne 'ok') {
        throw 'runtime proof assertions failed; see summary.json'
    }
} catch {
    $message = $_.Exception.Message
    if (-not (Test-Path -LiteralPath $summaryPath)) {
        $partial = [ordered]@{
            status = 'failed'
            stage = $stage
            ws_id = 'WS-6'
            error = $message
            summary_path = $summaryPath
        }
        Write-Utf8NoBom $summaryPath ($partial | ConvertTo-Json -Depth 10)
    } else {
        Write-Utf8NoBom (Join-Path $proofDir 'failure.txt') $message
    }
    throw
} finally {
    if ($proc -and -not $proc.HasExited) {
        try { Post-Quit $main } catch {}
        if (-not $proc.WaitForExit(1000)) {
            try { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue } catch {}
        }
    }
}

Write-Host "ws6_gesture_clamp_status=ok"
Write-Host "summary=$summaryPath"
Write-Host "accepted=True"
