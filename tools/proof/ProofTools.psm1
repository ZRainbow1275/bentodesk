Set-StrictMode -Version Latest

$script:RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))

function Get-ProofRepoRoot {
    return $script:RepoRoot
}

function Assert-ProofPathUnder {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Parent
    )

    $parentFull = [System.IO.Path]::GetFullPath($Parent).TrimEnd('\') + '\'
    $pathFull = [System.IO.Path]::GetFullPath($Path)
    if (-not $pathFull.StartsWith($parentFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing path outside proof workspace: $pathFull"
    }
    return $pathFull
}

function New-ProofRunDirectory {
    param([Parameter(Mandatory = $true)][string]$Name)

    $artifactsRoot = Join-Path $script:RepoRoot 'artifacts\proof'
    $runId = '{0}-{1}-{2}' -f $Name, (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssfffZ'), $PID
    $runDirectory = Join-Path $artifactsRoot $runId
    [void](Assert-ProofPathUnder -Path $runDirectory -Parent $artifactsRoot)
    New-Item -ItemType Directory -Path $runDirectory -Force | Out-Null
    return [pscustomobject]@{
        Id = $runId
        Directory = [System.IO.Path]::GetFullPath($runDirectory)
        ArtifactsRoot = [System.IO.Path]::GetFullPath($artifactsRoot)
    }
}

function Write-ProofJson {
    param(
        [Parameter(Mandatory = $true)]$Value,
        [Parameter(Mandatory = $true)][string]$Path,
        [int]$Depth = 12
    )

    $Value | ConvertTo-Json -Depth $Depth | Set-Content -LiteralPath $Path -Encoding UTF8
}

function Invoke-ProofCommand {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$FilePath,
        [string[]]$Arguments = @(),
        [Parameter(Mandatory = $true)][string]$WorkingDirectory,
        [Parameter(Mandatory = $true)][string]$LogDirectory
    )

    $logPath = Join-Path $LogDirectory ("{0}.log" -f $Name)
    $stdoutPath = "$logPath.stdout.tmp"
    $stderrPath = "$logPath.stderr.tmp"
    $started = Get-Date
    $command = @($FilePath) + $Arguments
    $exitCode = 127
    $errorText = $null

    try {
        if (-not (Get-Command $FilePath -ErrorAction SilentlyContinue)) {
            $errorText = "command not found: $FilePath"
            $errorText | Set-Content -LiteralPath $logPath -Encoding UTF8
        } else {
            $argumentLine = @(
                foreach ($argumentValue in $Arguments) {
                    $argument = [string]$argumentValue
                    if ($argument.Contains('"')) {
                        throw "proof command argument contains an unsupported quote: $argument"
                    }
                    if ($argument.Length -eq 0 -or $argument -match '\s') {
                        # Windows argv parsing consumes trailing backslashes
                        # before a closing quote, so double that final run.
                        $escaped = $argument -replace '(\\+)$', '$1$1'
                        '"{0}"' -f $escaped
                    } else {
                        $argument
                    }
                }
            ) -join ' '

            # Keep native stdout/stderr out of Windows PowerShell's object
            # pipeline. Large Cargo test/doc runs otherwise retain thousands of
            # ErrorRecord/host objects and can grow the proof runner by GiBs.
            $process = Start-Process `
                -FilePath $FilePath `
                -ArgumentList $argumentLine `
                -WorkingDirectory $WorkingDirectory `
                -NoNewWindow `
                -PassThru `
                -RedirectStandardOutput $stdoutPath `
                -RedirectStandardError $stderrPath
            # `Start-Process -Wait` waits for the full descendant tree on
            # Windows. Rust may leave the Visual C++ telemetry helper alive;
            # the proof only owns and needs the exact Cargo process.
            $processHandle = $process.Handle
            $process.WaitForExit()
            $nativeExitCode = [uint32]0
            if (-not [BentoDeskProofNative]::GetExitCodeProcess(
                $processHandle,
                [ref]$nativeExitCode
            )) {
                throw [System.ComponentModel.Win32Exception]::new(
                    [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
                )
            }
            $exitCode = [int]$nativeExitCode
            $process.Dispose()

            $writer = [System.IO.StreamWriter]::new(
                $logPath,
                $false,
                [System.Text.UTF8Encoding]::new($false)
            )
            try {
                foreach ($partPath in @($stdoutPath, $stderrPath)) {
                    if (Test-Path -LiteralPath $partPath) {
                        $reader = [System.IO.StreamReader]::new($partPath, $true)
                        try {
                            while (($line = $reader.ReadLine()) -ne $null) {
                                $writer.WriteLine($line)
                            }
                        } finally {
                            $reader.Dispose()
                        }
                    }
                }
            } finally {
                $writer.Dispose()
            }
            Write-Host ("{0}: exit {1}; log={2}" -f $Name, $exitCode, $logPath)
        }
    } catch {
        $errorText = $_.Exception.Message
        $errorText | Add-Content -LiteralPath $logPath -Encoding UTF8
        $exitCode = 1
    } finally {
        foreach ($temporaryPath in @($stdoutPath, $stderrPath)) {
            if (Test-Path -LiteralPath $temporaryPath) {
                Remove-Item -LiteralPath $temporaryPath -Force
            }
        }
    }

    return [pscustomobject]@{
        name = $Name
        command = ($command -join ' ')
        exit_code = [int]$exitCode
        passed = ($exitCode -eq 0)
        duration_ms = [int64]((Get-Date) - $started).TotalMilliseconds
        log = [System.IO.Path]::GetFullPath($logPath)
        error = $errorText
    }
}

function Set-ProofProcessEnvironment {
    param([Parameter(Mandatory = $true)][hashtable]$Values)

    $previous = @{}
    foreach ($name in $Values.Keys) {
        $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        [Environment]::SetEnvironmentVariable($name, $Values[$name], 'Process')
    }
    return $previous
}

function Restore-ProofProcessEnvironment {
    param([Parameter(Mandatory = $true)][hashtable]$Values)

    foreach ($name in $Values.Keys) {
        [Environment]::SetEnvironmentVariable($name, $Values[$name], 'Process')
    }
}

function Start-IsolatedBentoDesk {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory,
        [Parameter(Mandatory = $true)][string]$StateDirectory,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath,
        [hashtable]$ExtraEnvironment = @{}
    )

    $environment = @{
        BENTODESK_STATE_DIR = $StateDirectory
        BENTODESK_NANO_STATE_DIR = $null
    }
    foreach ($entry in $ExtraEnvironment.GetEnumerator()) {
        $environment[$entry.Key] = $entry.Value
    }

    $previous = Set-ProofProcessEnvironment -Values $environment
    try {
        return Start-Process `
            -FilePath $Executable `
            -WorkingDirectory $WorkingDirectory `
            -PassThru `
            -RedirectStandardOutput $StdoutPath `
            -RedirectStandardError $StderrPath
    } finally {
        Restore-ProofProcessEnvironment -Values $previous
    }
}

if (-not ('BentoDeskProofNative' -as [type])) {
    Add-Type -AssemblyName System.Drawing
    Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class BentoDeskProofNative {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool GetExitCodeProcess(IntPtr hProcess, out uint lpExitCode);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern bool GetClientRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern bool ClientToScreen(IntPtr hWnd, ref POINT lpPoint);

    [DllImport("user32.dll")]
    public static extern uint GetDpiForWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int X, int Y);

    [DllImport("user32.dll")]
    public static extern bool SetWindowPos(
        IntPtr hWnd,
        IntPtr hWndInsertAfter,
        int X,
        int Y,
        int cx,
        int cy,
        uint uFlags
    );

    [DllImport("user32.dll")]
    public static extern bool PostMessageW(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr SendMessageTimeoutW(
        IntPtr hWnd,
        uint Msg,
        UIntPtr wParam,
        IntPtr lParam,
        uint fuFlags,
        uint uTimeout,
        out UIntPtr lpdwResult
    );

    [DllImport("user32.dll")]
    public static extern bool InvalidateRect(IntPtr hWnd, IntPtr lpRect, bool bErase);

    public const uint SMTO_ABORTIFHUNG = 0x0002;
    public const uint WM_HOTKEY = 0x0312;
    public const uint WM_MOUSEMOVE = 0x0200;
    public static readonly IntPtr HWND_TOPMOST = new IntPtr(-1);
    public const uint SWP_NOSIZE = 0x0001;
    public const uint SWP_NOMOVE = 0x0002;
    public const uint SWP_NOACTIVATE = 0x0010;
    public const uint SWP_SHOWWINDOW = 0x0040;

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct POINT {
        public int X;
        public int Y;
    }
}
'@
}

function Get-ProofWindowsForPid {
    param([Parameter(Mandatory = $true)][int]$TargetProcessId)

    $items = New-Object System.Collections.ArrayList
    $callback = [BentoDeskProofNative+EnumWindowsProc]{
        param([IntPtr]$Hwnd, [IntPtr]$LParam)

        [uint32]$windowPid = 0
        [void][BentoDeskProofNative]::GetWindowThreadProcessId($Hwnd, [ref]$windowPid)
        if ($windowPid -eq [uint32]$TargetProcessId) {
            $class = New-Object System.Text.StringBuilder 256
            $title = New-Object System.Text.StringBuilder 256
            [void][BentoDeskProofNative]::GetClassName($Hwnd, $class, $class.Capacity)
            [void][BentoDeskProofNative]::GetWindowText($Hwnd, $title, $title.Capacity)

            $rect = New-Object BentoDeskProofNative+RECT
            $client = New-Object BentoDeskProofNative+RECT
            $origin = New-Object BentoDeskProofNative+POINT
            [void][BentoDeskProofNative]::GetWindowRect($Hwnd, [ref]$rect)
            [void][BentoDeskProofNative]::GetClientRect($Hwnd, [ref]$client)
            [void][BentoDeskProofNative]::ClientToScreen($Hwnd, [ref]$origin)

            [void]$items.Add([pscustomobject]@{
                hwnd = $Hwnd.ToInt64()
                class = $class.ToString()
                title = $title.ToString()
                visible = [BentoDeskProofNative]::IsWindowVisible($Hwnd)
                dpi = [BentoDeskProofNative]::GetDpiForWindow($Hwnd)
                rect = [pscustomobject]@{
                    left = $rect.Left
                    top = $rect.Top
                    right = $rect.Right
                    bottom = $rect.Bottom
                    width = $rect.Right - $rect.Left
                    height = $rect.Bottom - $rect.Top
                }
                client = [pscustomobject]@{
                    left = $origin.X
                    top = $origin.Y
                    width = $client.Right - $client.Left
                    height = $client.Bottom - $client.Top
                }
            })
        }
        return $true
    }

    [void][BentoDeskProofNative]::EnumWindows($callback, [IntPtr]::Zero)
    return @($items.ToArray())
}

function Wait-ProofWindow {
    param(
        [Parameter(Mandatory = $true)][int]$TargetProcessId,
        [Parameter(Mandatory = $true)][string]$ClassName,
        [int]$TimeoutMs = 10000,
        [bool]$VisibleOnly = $true
    )

    $timer = [Diagnostics.Stopwatch]::StartNew()
    while ($timer.ElapsedMilliseconds -lt $TimeoutMs) {
        $window = Get-ProofWindowsForPid -TargetProcessId $TargetProcessId |
            Where-Object { $_.class -eq $ClassName -and (-not $VisibleOnly -or $_.visible) } |
            Select-Object -First 1
        if ($window) {
            return $window
        }
        Start-Sleep -Milliseconds 100
    }
    return $null
}

function New-ProofMouseLParam {
    param(
        [Parameter(Mandatory = $true)][int]$X,
        [Parameter(Mandatory = $true)][int]$Y
    )

    return [IntPtr](((($Y -band 0xffff) -shl 16) -bor ($X -band 0xffff)))
}

function Send-ProofMouseMove {
    param(
        [Parameter(Mandatory = $true)]$Window,
        [Parameter(Mandatory = $true)][int]$ClientX,
        [Parameter(Mandatory = $true)][int]$ClientY,
        [switch]$MoveSystemCursor,
        [int]$SleepMs = 0
    )

    if ($MoveSystemCursor) {
        [void][BentoDeskProofNative]::SetForegroundWindow([IntPtr]$Window.hwnd)
        [void][BentoDeskProofNative]::SetCursorPos(
            [int]($Window.client.left + $ClientX),
            [int]($Window.client.top + $ClientY)
        )
        Start-Sleep -Milliseconds 35
    }
    $nativeResult = [UIntPtr]::Zero
    $sent = [BentoDeskProofNative]::SendMessageTimeoutW(
        [IntPtr]$Window.hwnd,
        [BentoDeskProofNative]::WM_MOUSEMOVE,
        [UIntPtr]::Zero,
        (New-ProofMouseLParam -X $ClientX -Y $ClientY),
        [BentoDeskProofNative]::SMTO_ABORTIFHUNG,
        2500,
        [ref]$nativeResult
    )
    if ($sent -eq [IntPtr]::Zero) {
        throw "WM_MOUSEMOVE timed out at client=($ClientX,$ClientY)"
    }
    if ($SleepMs -gt 0) {
        Start-Sleep -Milliseconds $SleepMs
    }
}

function Set-ProofWindowInputForeground {
    param([Parameter(Mandatory = $true)]$Window)

    $raised = [BentoDeskProofNative]::SetWindowPos(
        [IntPtr]$Window.hwnd,
        [BentoDeskProofNative]::HWND_TOPMOST,
        0,
        0,
        0,
        0,
        [BentoDeskProofNative]::SWP_NOMOVE -bor
            [BentoDeskProofNative]::SWP_NOSIZE -bor
            [BentoDeskProofNative]::SWP_NOACTIVATE -bor
            [BentoDeskProofNative]::SWP_SHOWWINDOW
    )
    if (-not $raised) {
        throw 'SetWindowPos(HWND_TOPMOST) failed for the isolated proof window'
    }
    Start-Sleep -Milliseconds 100
}

function Request-ProofPaint {
    param(
        [Parameter(Mandatory = $true)]$Window,
        [int]$SleepMs = 80
    )

    [void][BentoDeskProofNative]::InvalidateRect([IntPtr]$Window.hwnd, [IntPtr]::Zero, $false)
    if ($SleepMs -gt 0) {
        Start-Sleep -Milliseconds $SleepMs
    }
}

function Save-ProofWindowShot {
    param(
        [Parameter(Mandatory = $true)]$Window,
        [Parameter(Mandatory = $true)][string]$Path
    )

    $width = [Math]::Max(1, [int]$Window.rect.width)
    $height = [Math]::Max(1, [int]$Window.rect.height)
    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen(
            [int]$Window.rect.left,
            [int]$Window.rect.top,
            0,
            0,
            [System.Drawing.Size]::new($width, $height)
        )
        $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }

    $sample = New-Object System.Drawing.Bitmap $Path
    try {
        $colors = New-Object 'System.Collections.Generic.HashSet[int]'
        $minimum = 255
        $maximum = 0
        $stepX = [Math]::Max(1, [int]($sample.Width / 64))
        $stepY = [Math]::Max(1, [int]($sample.Height / 36))
        for ($y = 0; $y -lt $sample.Height; $y += $stepY) {
            for ($x = 0; $x -lt $sample.Width; $x += $stepX) {
                $pixel = $sample.GetPixel($x, $y)
                [void]$colors.Add($pixel.ToArgb())
                $luma = [int](0.2126 * $pixel.R + 0.7152 * $pixel.G + 0.0722 * $pixel.B)
                $minimum = [Math]::Min($minimum, $luma)
                $maximum = [Math]::Max($maximum, $luma)
            }
        }
        return [pscustomobject]@{
            path = [System.IO.Path]::GetFullPath($Path)
            width = $sample.Width
            height = $sample.Height
            sampled_colors = $colors.Count
            luminance_range = $maximum - $minimum
            nonblank = ($colors.Count -ge 8 -and ($maximum - $minimum) -ge 16)
        }
    } finally {
        $sample.Dispose()
    }
}

function Send-ProofQuitHotkey {
    param([Parameter(Mandatory = $true)]$Window)

    return [BentoDeskProofNative]::PostMessageW(
        [IntPtr]$Window.hwnd,
        [BentoDeskProofNative]::WM_HOTKEY,
        [UIntPtr]([uint64]16973),
        [IntPtr]::Zero
    )
}

function Wait-ProofProcessExit {
    param(
        [Parameter(Mandatory = $true)][int]$TargetProcessId,
        [int]$TimeoutMs = 5000
    )

    $timer = [Diagnostics.Stopwatch]::StartNew()
    while ($timer.ElapsedMilliseconds -lt $TimeoutMs) {
        if (-not (Get-Process -Id $TargetProcessId -ErrorAction SilentlyContinue)) {
            return $true
        }
        Start-Sleep -Milliseconds 100
    }
    return $false
}

function Get-ExactExecutableProcesses {
    param([Parameter(Mandatory = $true)][string]$Executable)

    $expected = [System.IO.Path]::GetFullPath($Executable)
    $matches = New-Object System.Collections.ArrayList
    foreach ($process in Get-Process -ErrorAction SilentlyContinue) {
        try {
            if ($process.Path -and
                [System.IO.Path]::GetFullPath($process.Path).Equals(
                    $expected,
                    [System.StringComparison]::OrdinalIgnoreCase
                )) {
                [void]$matches.Add($process)
            }
        } catch {
            continue
        }
    }
    return @($matches.ToArray())
}

function Stop-ProofProcessExact {
    param(
        [Parameter(Mandatory = $true)][int]$TargetProcessId,
        [Parameter(Mandatory = $true)][string]$Executable
    )

    $process = Get-Process -Id $TargetProcessId -ErrorAction SilentlyContinue
    if (-not $process) {
        return $false
    }
    $actual = $null
    try {
        $actual = [System.IO.Path]::GetFullPath($process.Path)
    } catch {
        throw "refusing to terminate PID $TargetProcessId because its executable path is unavailable"
    }
    $expected = [System.IO.Path]::GetFullPath($Executable)
    if (-not $actual.Equals($expected, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing to terminate PID $TargetProcessId at unexpected path: $actual"
    }
    Stop-Process -Id $TargetProcessId -Force
    return $true
}

Export-ModuleMember -Function @(
    'Get-ProofRepoRoot',
    'Assert-ProofPathUnder',
    'New-ProofRunDirectory',
    'Write-ProofJson',
    'Invoke-ProofCommand',
    'Set-ProofProcessEnvironment',
    'Restore-ProofProcessEnvironment',
    'Start-IsolatedBentoDesk',
    'Get-ProofWindowsForPid',
    'Wait-ProofWindow',
    'Set-ProofWindowInputForeground',
    'Send-ProofMouseMove',
    'Request-ProofPaint',
    'Save-ProofWindowShot',
    'Send-ProofQuitHotkey',
    'Wait-ProofProcessExit',
    'Get-ExactExecutableProcesses',
    'Stop-ProofProcessExact'
)
