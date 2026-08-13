#requires -version 5.1
[CmdletBinding()]
param(
    [Alias('Executable', 'CandidateExe')][string]$ExecutablePath,
    [switch]$SkipBuild,
    [Alias('DumpExecutable')][string]$DumpExecutablePath,
    [Alias('ItemGridDumpExecutable')][string]$ItemGridDumpExecutablePath,
    [Alias('ZoneItemsDumpExecutable')][string]$ZoneItemsDumpExecutablePath,
    [string]$Legacy209State,
    [string]$Legacy209ExpectedSha256,
    [string]$Legacy209ProducerExe,
    [string]$Legacy209ProducerExpectedSha256,
    [string]$Legacy210State,
    [string]$Legacy210ExpectedSha256,
    [string]$Legacy210ProducerExe,
    [string]$Legacy210ProducerExpectedSha256,
    [string]$StateSeed,
    [string]$StateSeedExpectedSha256,
    [string]$ProofOutputRoot,
    [string]$BuildTargetDirectory,
    [string]$ExpectedProductVersion = '2.1.0',
    [int]$ExpectedDpi = 144,
    [int]$ExpectedViewportWidth = 1707,
    [int]$ExpectedViewportHeight = 912,
    [int]$ExpectedTaskbarDip = 48
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'ProofTools.psm1') -Force
$script:LiveGeometryCache = @{}

# Mouse and keyboard claims use real SendInput on the interactive desktop.
if (-not ('BentoDeskProofInput' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class BentoDeskProofInput {
    const uint MOUSE = 0, KEYBOARD = 1, MOVE = 1, DOWN = 2, UP = 4;
    const uint ABSOLUTE = 0x8000, VIRTUALDESK = 0x4000, KEYUP = 2, UNICODE = 4;
    [StructLayout(LayoutKind.Sequential)] struct MOUSEINPUT {
        public int dx; public int dy; public uint data; public uint flags;
        public uint time; public UIntPtr extra;
    }
    [StructLayout(LayoutKind.Sequential)] struct KEYINPUT {
        public ushort vk; public ushort scan; public uint flags; public uint time;
        public UIntPtr extra;
    }
    [StructLayout(LayoutKind.Explicit)] struct INPUT {
        [FieldOffset(0)] public uint type;
        [FieldOffset(8)] public MOUSEINPUT mouse;
        [FieldOffset(8)] public KEYINPUT key;
    }
    [StructLayout(LayoutKind.Sequential)] struct POINT { public int x; public int y; }
    [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] inputs, int size);
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] static extern bool GetCursorPos(out POINT point);
    [DllImport("user32.dll")] static extern int GetSystemMetrics(int index);
    static bool Send(INPUT input) {
        return SendInput(1, new INPUT[] { input }, Marshal.SizeOf(typeof(INPUT))) == 1;
    }
    public static bool Move(int x, int y) {
        SetCursorPos(x, y);
        int vx = GetSystemMetrics(76), vy = GetSystemMetrics(77);
        int vw = Math.Max(2, GetSystemMetrics(78)), vh = Math.Max(2, GetSystemMetrics(79));
        return Send(new INPUT { type = MOUSE, mouse = new MOUSEINPUT {
            dx = (int)Math.Round((x - vx) * 65535.0 / (vw - 1)),
            dy = (int)Math.Round((y - vy) * 65535.0 / (vh - 1)),
            flags = MOVE | ABSOLUTE | VIRTUALDESK
        }});
    }
    public static bool MoveAndButtonDown(int x, int y) {
        SetCursorPos(x, y);
        int vx = GetSystemMetrics(76), vy = GetSystemMetrics(77);
        int vw = Math.Max(2, GetSystemMetrics(78)), vh = Math.Max(2, GetSystemMetrics(79));
        var inputs = new INPUT[] {
            new INPUT { type = MOUSE, mouse = new MOUSEINPUT {
                dx = (int)Math.Round((x - vx) * 65535.0 / (vw - 1)),
                dy = (int)Math.Round((y - vy) * 65535.0 / (vh - 1)),
                flags = MOVE | ABSOLUTE | VIRTUALDESK
            }},
            new INPUT { type = MOUSE, mouse = new MOUSEINPUT { flags = DOWN }}
        };
        return SendInput(2, inputs, Marshal.SizeOf(typeof(INPUT))) == 2;
    }
    public static bool Button(bool down) {
        return Send(new INPUT { type = MOUSE, mouse = new MOUSEINPUT {
            flags = down ? DOWN : UP
        }});
    }
    public static bool Key(int vk, bool up) {
        return Send(new INPUT { type = KEYBOARD, key = new KEYINPUT {
            vk = (ushort)vk, flags = up ? KEYUP : 0
        }});
    }
    public static bool Text(string value) {
        foreach (char character in value) {
            if (!Send(new INPUT { type = KEYBOARD, key = new KEYINPUT {
                scan = character, flags = UNICODE
            }}) || !Send(new INPUT { type = KEYBOARD, key = new KEYINPUT {
                scan = character, flags = UNICODE | KEYUP
            }})) return false;
        }
        return true;
    }
}
'@
}
if (-not ('BentoDeskProofDisplay' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class BentoDeskProofDisplay {
    [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint flags);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern bool GetMonitorInfoW(IntPtr monitor, ref MONITORINFO info);
    [StructLayout(LayoutKind.Sequential)] public struct RECT {
        public int Left; public int Top; public int Right; public int Bottom;
    }
    [StructLayout(LayoutKind.Sequential)] public struct MONITORINFO {
        public int cbSize; public RECT rcMonitor; public RECT rcWork; public uint dwFlags;
    }
}
'@
}

function Get-Field {
    param([AllowNull()]$Object, [Parameter(Mandatory = $true)][string[]]$Names)
    if ($null -eq $Object) { return $null }
    foreach ($name in $Names) {
        $property = $Object.PSObject.Properties[$name]
        if ($property -and $null -ne $property.Value) { return $property.Value }
    }
    return $null
}
function Convert-Scalar {
    param([AllowNull()]$Value)
    if ($null -eq $Value) { return $null }
    if ($Value -is [bool] -or $Value -is [int] -or $Value -is [long] -or $Value -is [double]) { return $Value }
    $text = ([string]$Value).Trim()
    if ($text.Length -eq 0 -or $text -eq '-') { return $null }
    if ($text -eq 'true') { return $true }
    if ($text -eq 'false') { return $false }
    $number = 0.0
    if ([double]::TryParse($text, [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$number)) { return $number }
    return $text
}
function Convert-FiniteDouble {
    param([AllowNull()]$Value)
    $scalar = Convert-Scalar $Value
    if ($null -eq $scalar -or $scalar -is [bool]) { return $null }
    $number = 0.0
    if (-not [double]::TryParse(
        ([string]$scalar),
        [Globalization.NumberStyles]::Float,
        [Globalization.CultureInfo]::InvariantCulture,
        [ref]$number
    )) { return $null }
    if ([double]::IsNaN($number) -or [double]::IsInfinity($number)) { return $null }
    return $number
}
function Read-SharedText {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return '' }
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    try {
        $reader = New-Object IO.StreamReader($stream, [Text.Encoding]::UTF8, $true)
        try { return $reader.ReadToEnd() } finally { $reader.Dispose() }
    } finally { $stream.Dispose() }
}
function Get-Coordinate {
    param($Row, [string]$Kind, [string]$Axis, [string[]]$Aliases = @())
    $value = Get-Field $Row -Names (@(($Kind + '_' + $Axis), ($Kind + $Axis)) + $Aliases)
    if ($null -ne $value) { return Convert-Scalar $value }
    $nested = Get-Field $Row -Names @($Kind, ($Kind + '_rect'))
    if ($nested) {
        $names = @($Axis)
        if ($Axis -eq 'x') { $names += 'left' }
        if ($Axis -eq 'y') { $names += 'top' }
        if ($Axis -eq 'width') { $names += 'w' }
        if ($Axis -eq 'height') { $names += 'h' }
        return Convert-Scalar (Get-Field $nested -Names $names)
    }
    return $null
}
function Convert-DumpRow {
    param($Row)
    $id = Convert-Scalar (Get-Field $Row -Names @('zone_id', 'id', 'zone'))
    if ($null -eq $id) { return $null }
    $cx = Get-Coordinate $Row 'capsule' 'x' @('x', 'rect_x')
    $cy = Get-Coordinate $Row 'capsule' 'y' @('y', 'rect_y')
    $cw = Get-Coordinate $Row 'capsule' 'width' @('width', 'w')
    $ch = Get-Coordinate $Row 'capsule' 'height' @('height', 'h')
    $hx = Get-Field $Row -Names @('persisted_x', 'home_x', 'zone_x', 'origin_x', 'x')
    $hy = Get-Field $Row -Names @('persisted_y', 'home_y', 'zone_y', 'origin_y', 'y')
    if ($null -eq $hx) { $hx = $cx }
    if ($null -eq $hy) { $hy = $cy }
    $px = Get-Coordinate $Row 'panel' 'x' @('resolved_panel_x', 'expanded_x')
    $py = Get-Coordinate $Row 'panel' 'y' @('resolved_panel_y', 'expanded_y')
    $pw = Get-Coordinate $Row 'panel' 'width' @('resolved_panel_width', 'expanded_width')
    $ph = Get-Coordinate $Row 'panel' 'height' @('resolved_panel_height', 'expanded_height')
    $ex = Get-Coordinate $Row 'effective' 'x' @('effective_rect_x', 'surface_x')
    $ey = Get-Coordinate $Row 'effective' 'y' @('effective_rect_y', 'surface_y')
    $ew = Get-Coordinate $Row 'effective' 'width' @('effective_rect_width', 'surface_width')
    $eh = Get-Coordinate $Row 'effective' 'height' @('effective_rect_height', 'surface_height')
    $right = Convert-Scalar (Get-Field $Row -Names @('anchor_right', 'right_anchor', 'anchored_right'))
    $bottom = Convert-Scalar (Get-Field $Row -Names @('anchor_bottom', 'bottom_anchor', 'anchored_bottom'))
    $parent = Convert-Scalar (Get-Field $Row -Names @('stack_parent', 'parent_id'))
    $parentNumber = Convert-FiniteDouble $parent
    if ($null -eq $parent -or ([string]$parent) -eq '-' -or $parentNumber -eq 0) { $parent = $null }
    elseif ($null -eq $parentNumber) { $parent = $null }
    else { $parent = $parentNumber }
    $members = Convert-Scalar (Get-Field $Row -Names @('stack_member_count', 'member_count', 'stack_members'))
    if ($null -eq $members) { $members = 0 }
    $visible = Convert-Scalar (Get-Field $Row -Names @('visible', 'is_visible'))
    if ($null -eq $visible) { $visible = $true }
    $stack = Convert-Scalar (Get-Field $Row -Names @('is_stack_anchor', 'stack_anchor_visible'))
    if ($null -eq $stack) { $stack = ([int]$members -gt 0) }
    $extended = ($null -ne $hx -and $null -ne $hy -and $null -ne $cx -and $null -ne $cy -and $null -ne $cw -and $null -ne $ch -and $null -ne $right -and $null -ne $bottom -and $null -ne $px -and $null -ne $py -and $null -ne $pw -and $null -ne $ph -and $null -ne $ex -and $null -ne $ey -and $null -ne $ew -and $null -ne $eh)
    return [pscustomobject][ordered]@{
        zone_id = [int]$id; visible = [bool]$visible
        stack_parent = if ($null -eq $parent) { $null } else { [int]$parent }
        stack_member_count = [int]$members; is_stack_anchor = [bool]$stack
        capsule_size = [string](Get-Field $Row -Names @('capsule_size', 'size'))
        capsule_shape = [string](Get-Field $Row -Names @('capsule_shape', 'shape'))
        home_x = if ($null -ne $hx) { [double](Convert-Scalar $hx) } else { $null }
        home_y = if ($null -ne $hy) { [double](Convert-Scalar $hy) } else { $null }
        capsule_x = if ($null -ne $cx) { [double]$cx } else { $null }
        capsule_y = if ($null -ne $cy) { [double]$cy } else { $null }
        capsule_width = if ($null -ne $cw) { [double]$cw } else { $null }
        capsule_height = if ($null -ne $ch) { [double]$ch } else { $null }
        anchor_right = $right; anchor_bottom = $bottom
        panel_x = if ($null -ne $px) { [double]$px } else { $null }
        panel_y = if ($null -ne $py) { [double]$py } else { $null }
        panel_width = if ($null -ne $pw) { [double]$pw } else { $null }
        panel_height = if ($null -ne $ph) { [double]$ph } else { $null }
        effective_x = if ($null -ne $ex) { [double]$ex } else { $null }
        effective_y = if ($null -ne $ey) { [double]$ey } else { $null }
        effective_width = if ($null -ne $ew) { [double]$ew } else { $null }
        effective_height = if ($null -ne $eh) { [double]$eh } else { $null }
        extended = [bool]$extended; raw = $Row
    }
}
function Read-Dump {
    param([string]$Path)
    $lines = [IO.File]::ReadAllLines($Path)
    foreach ($line in @($lines | Where-Object { $_.TrimStart().StartsWith('{') })) {
        try {
            $json = $line | ConvertFrom-Json
            $items = Get-Field $json -Names @('zones', 'capsules', 'rows', 'items')
            if (-not $items) { $items = $json }
            $rows = New-Object System.Collections.ArrayList
            foreach ($item in @($items)) { $row = Convert-DumpRow $item; if ($row) { [void]$rows.Add($row) } }
            if ($rows.Count -gt 0) { return [pscustomobject][ordered]@{ rows = @($rows.ToArray()); extended = (@($rows | Where-Object { -not $_.extended }).Count -eq 0); format = 'json'; path = [IO.Path]::GetFullPath($Path) } }
        } catch { }
    }
    $header = -1; $headers = @()
    for ($i = 0; $i -lt $lines.Count; $i++) { if ($lines[$i] -match '(^|	)zone_id(	|$)') { $header = $i; $headers = @($lines[$i] -split ([char]9)); break } }
    if ($header -lt 0) { throw "dump header not found: $Path" }
    $rows = New-Object System.Collections.ArrayList
    for ($i = $header + 1; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -notmatch '^\s*\d+[	]') { continue }
        $values = @($lines[$i] -split ([char]9)); $obj = [ordered]@{}
        for ($j = 0; $j -lt $headers.Count; $j++) {
            $value = if ($j -lt $values.Count) { $values[$j] } else { $null }
            $obj[$headers[$j]] = Convert-Scalar $value
        }
        $row = Convert-DumpRow ([pscustomobject]$obj); if ($row) { [void]$rows.Add($row) }
    }
    if ($rows.Count -eq 0) { throw "dump has no rows: $Path" }
    return [pscustomobject][ordered]@{ rows = @($rows.ToArray()); extended = (@($rows | Where-Object { -not $_.extended }).Count -eq 0); format = 'tsv'; headers = $headers; path = [IO.Path]::GetFullPath($Path) }
}
function Invoke-Dump {
    param([string]$DumpExecutable, [string]$StateDirectory, [int]$Width, [int]$Height, [string]$Label, [string]$LogDirectory, [System.Collections.ArrayList]$Commands)
    $attempts = @(@($StateDirectory, $Width.ToString(), $Height.ToString()), @($StateDirectory, '--viewport-width', $Width.ToString(), '--viewport-height', $Height.ToString()), @($StateDirectory)); $legacy = $null
    for ($i = 0; $i -lt $attempts.Count; $i++) {
        $result = Invoke-ProofCommand -Name ('{0}-{1}' -f $Label, $i + 1) -FilePath $DumpExecutable -Arguments $attempts[$i] -WorkingDirectory (Get-ProofRepoRoot) -LogDirectory $LogDirectory; [void]$Commands.Add($result); if (-not $result.passed) { continue }
        try { $dump = Read-Dump $result.log } catch { continue }; $dump | Add-Member NoteProperty command $result -Force; $dump | Add-Member NoteProperty interface_attempt ($i + 1) -Force
        if ($dump.extended) { return $dump }; if ($null -eq $legacy) { $legacy = $dump }
    }
    if ($legacy) { return $legacy }; throw "all dump_zone_capsules interfaces failed for $StateDirectory"
}
function Get-Row { param($Rows, [int]$Id); return @($Rows | Where-Object { $_.zone_id -eq $Id } | Select-Object -First 1) }
function Get-Rect {
    param([AllowNull()]$Row, [string]$Kind = 'capsule')
    if ($null -eq $Row) { return $null }
    $x = Get-Field $Row -Names @(($Kind + '_x'))
    $y = Get-Field $Row -Names @(($Kind + '_y'))
    $w = Get-Field $Row -Names @(($Kind + '_width'))
    $h = Get-Field $Row -Names @(($Kind + '_height'))
    if ($null -eq $x -or $null -eq $y -or $null -eq $w -or $null -eq $h) { return $null }
    return [pscustomobject]@{ x = [double]$x; y = [double]$y; width = [double]$w; height = [double]$h; right = [double]$x + [double]$w; bottom = [double]$y + [double]$h }
}
function Inside {
    param($Rect, [int]$Width, [int]$Height)
    $epsilon = 0.001
    return [bool]($Rect -and $Rect.x -ge -$epsilon -and $Rect.y -ge -$epsilon -and $Rect.right -le $Width + $epsilon -and $Rect.bottom -le $Height + $epsilon)
}
function Meta { param([string]$Path); if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "binary not found: $Path" }; $item = Get-Item -LiteralPath $Path; return [pscustomobject][ordered]@{ path = [IO.Path]::GetFullPath($Path); bytes = [int64]$item.Length; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant(); product_version = [string]$item.VersionInfo.ProductVersion; file_version = [string]$item.VersionInfo.FileVersion } }
function Metrics {
    param($Window)
    $handle = [BentoDeskProofDisplay]::MonitorFromWindow([IntPtr]$Window.hwnd, 2); if ($handle -eq [IntPtr]::Zero) { throw 'MonitorFromWindow failed' }; $info = New-Object BentoDeskProofDisplay+MONITORINFO; $info.cbSize = [Runtime.InteropServices.Marshal]::SizeOf($info); if (-not [BentoDeskProofDisplay]::GetMonitorInfoW($handle, [ref]$info)) { throw 'GetMonitorInfoW failed' }
    $dpi = [Math]::Max(96, [int]$Window.dpi); $scale = [double]$dpi / 96.0; $m = $info.rcMonitor; $w = $info.rcWork; $ww = $w.Right - $w.Left; $wh = $w.Bottom - $w.Top; $lw = [Math]::Round($ww / $scale); $lh = [Math]::Round($wh / $scale); $rw = [double]$Window.client.width; $rh = [double]$Window.client.height; $logical = [Math]::Abs($rw - $lw) -lt [Math]::Abs($rw / $scale - $lw); if ($logical) { $cw = [Math]::Round($rw); $ch = [Math]::Round($rh); $conversion = 'GetClientRect-logical' } else { $cw = [Math]::Round($rw / $scale); $ch = [Math]::Round($rh / $scale); $conversion = 'client-device-to-logical' }
    $left = $w.Left - $m.Left; $top = $w.Top - $m.Top; $right = $m.Right - $w.Right; $bottom = $m.Bottom - $w.Bottom; $edge = if ($bottom -ge $left -and $bottom -ge $top -and $bottom -ge $right) { 'bottom' } elseif ($top -ge $left -and $top -ge $right) { 'top' } elseif ($left -ge $right) { 'left' } else { 'right' }
    return [pscustomobject][ordered]@{ dpi = $dpi; scale = $scale; conversion = $conversion; client_observed = [ordered]@{ left = $Window.client.left; top = $Window.client.top; right = $Window.client.left + $rw; bottom = $Window.client.top + $rh; width = $rw; height = $rh }; logical_viewport = [ordered]@{ width = [int]$cw; height = [int]$ch }; monitor_device = [ordered]@{ left = $m.Left; top = $m.Top; right = $m.Right; bottom = $m.Bottom }; work_device = [ordered]@{ left = $w.Left; top = $w.Top; right = $w.Right; bottom = $w.Bottom; width = $ww; height = $wh }; work_logical = [ordered]@{ width = $lw; height = $lh }; taskbar = [ordered]@{ edge = $edge; left_device = $left; top_device = $top; right_device = $right; bottom_device = $bottom; left_logical = [Math]::Round($left / $scale); top_logical = [Math]::Round($top / $scale); right_logical = [Math]::Round($right / $scale); bottom_logical = [Math]::Round($bottom / $scale) } }
}
function Move-Input { param($Window, [double]$X, [double]$Y); $scale = [Math]::Max(1.0, [double]$Window.dpi / 96.0); $sx = [int]$Window.client.left + [int][Math]::Round($X * $scale); $sy = [int]$Window.client.top + [int][Math]::Round($Y * $scale); if (-not [BentoDeskProofInput]::Move($sx, $sy)) { throw "SendInput mouse move failed at ($X,$Y)" } }
function Move-Down-Input { param($Window, [double]$X, [double]$Y); $scale = [Math]::Max(1.0, [double]$Window.dpi / 96.0); $sx = [int]$Window.client.left + [int][Math]::Round($X * $scale); $sy = [int]$Window.client.top + [int][Math]::Round($Y * $scale); if (-not [BentoDeskProofInput]::MoveAndButtonDown($sx, $sy)) { throw "SendInput atomic mouse move/down failed at ($X,$Y)" } }
function Click-Input { param($Window, [double]$X, [double]$Y); Move-Input $Window $X $Y; Start-Sleep -Milliseconds 45; if (-not [BentoDeskProofInput]::Button($true)) { throw 'SendInput left down failed' }; Start-Sleep -Milliseconds 45; if (-not [BentoDeskProofInput]::Button($false)) { throw 'SendInput left up failed' } }
function Drag-Input { param($Window, [double]$StartX, [double]$StartY, [double]$EndX, [double]$EndY, [int]$Steps = 24); Move-Down-Input $Window $StartX $StartY; for ($i = 1; $i -le $Steps; $i++) { $t = [double]$i / $Steps; Move-Input $Window ($StartX + ($EndX - $StartX) * $t) ($StartY + ($EndY - $StartY) * $t); Start-Sleep -Milliseconds 18 }; if (-not [BentoDeskProofInput]::Button($false)) { throw 'SendInput left up failed' }; Start-Sleep -Milliseconds 250; return [pscustomobject][ordered]@{ real_system_cursor = $true; atomic_move_down = $true; message_sequence = @('SendInput:MOUSEMOVE+LEFTDOWN', 'SendInput:MOUSEMOVE', 'SendInput:LEFTUP'); move_count = $Steps } }
function Key-Input { param([int]$Key, [bool]$Up); if (-not [BentoDeskProofInput]::Key($Key, $Up)) { throw "SendInput key failed: $Key" } }
function Chord-Input { param([int]$Modifier, [int]$Key); Key-Input $Modifier $false; Start-Sleep -Milliseconds 30; Key-Input $Key $false; Start-Sleep -Milliseconds 35; Key-Input $Key $true; Key-Input $Modifier $true; Start-Sleep -Milliseconds 90 }
function Text-Input { param([string]$Value); if (-not [BentoDeskProofInput]::Text($Value)) { throw "SendInput Unicode text failed: $Value" }; Start-Sleep -Milliseconds 150 }
function Write-ZoneDisplayModeVault {
    param([string]$State, [ValidateSet('click', 'always')][string]$Mode)
    $inner = [Text.Encoding]::UTF8.GetBytes(('{{"kv":{{"zone_display_mode":{{"Str":"{0}"}}}}}}' -f $Mode))
    $record = [ordered]@{ version = 1; mode_tag = 0; salt_b64 = ''; nonce_b64 = ''; tag_b64 = ''; ciphertext_b64 = [Convert]::ToBase64String($inner) }
    $path = Join-Path $State 'vault.bin'
    [IO.File]::WriteAllText($path, ($record | ConvertTo-Json -Compress), (New-Object Text.UTF8Encoding($false)))
    return [pscustomobject][ordered]@{ mode = $Mode; path = [IO.Path]::GetFullPath($path); sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant() }
}
function EdgeTarget {
    param([string]$Edge, [int]$Slot, [int]$Count, $Rect, [int]$Width, [int]$Height, $Rows, [int]$ZoneId)
    $maxX = [Math]::Max(0, $Width - [int]$Rect.width)
    $maxY = [Math]::Max(0, $Height - [int]$Rect.height)
    $preferred = [double]($Slot + 1) / ($Count + 1)
    $candidates = foreach ($index in 1..31) {
        $fraction = [double]$index / 32
        $target = switch ($Edge) {
            'top' { [pscustomobject]@{ x = [Math]::Floor($maxX * $fraction); y = 0 } }
            'right' { [pscustomobject]@{ x = $maxX; y = [Math]::Floor($maxY * $fraction) } }
            'bottom' { [pscustomobject]@{ x = [Math]::Floor($maxX * $fraction); y = $maxY } }
            default { [pscustomobject]@{ x = 0; y = [Math]::Floor($maxY * $fraction) } }
        }
        $safe = $true
        foreach ($other in @($Rows | Where-Object { $_.zone_id -ne $ZoneId -and $_.visible -and $null -eq $_.stack_parent })) {
            $otherRect = Get-Rect $other
            if (-not $otherRect) { continue }
            $interWidth = [Math]::Max(0, [Math]::Min($target.x + $Rect.width, $otherRect.right) - [Math]::Max($target.x, $otherRect.x))
            $interHeight = [Math]::Max(0, [Math]::Min($target.y + $Rect.height, $otherRect.bottom) - [Math]::Max($target.y, $otherRect.y))
            $overlap = ($interWidth * $interHeight) / [Math]::Max(1, [Math]::Min($Rect.width * $Rect.height, $otherRect.width * $otherRect.height))
            $dx = ($target.x + $Rect.width / 2) - ($otherRect.x + $otherRect.width / 2)
            $dy = ($target.y + $Rect.height / 2) - ($otherRect.y + $otherRect.height / 2)
            $distance = [Math]::Sqrt($dx * $dx + $dy * $dy)
            $proximity = 0.8 * (($Rect.width + $Rect.height + $otherRect.width + $otherRect.height) / 4)
            if ($overlap -ge 0.30 -or $distance -le $proximity) { $safe = $false; break }
        }
        if ($safe) { $target | Add-Member NoteProperty preference_distance ([Math]::Abs($fraction - $preferred)); $target }
    }
    $selected = @($candidates | Sort-Object preference_distance | Select-Object -First 1)
    if ($selected.Count -ne 1) { throw "no stack-safe $Edge target for zone $ZoneId" }
    return $selected[0]
}
function QuadTarget { param([string]$Quad, $Rect, [int]$Width, [int]$Height); $maxX = [Math]::Max(0, $Width - [int]$Rect.width); $maxY = [Math]::Max(0, $Height - [int]$Rect.height); switch ($Quad) { 'left-top' { return [pscustomobject]@{ x = 20; y = 20 } }; 'right-top' { return [pscustomobject]@{ x = [Math]::Max(0, $maxX - 20); y = 20 } }; 'left-bottom' { return [pscustomobject]@{ x = 20; y = [Math]::Max(0, $maxY - 20) } }; default { return [pscustomobject]@{ x = [Math]::Max(0, $maxX - 20); y = [Math]::Max(0, $maxY - 20) } } } }
function AwayTarget { param($Rect, [int]$Width, [int]$Height); return [pscustomobject]@{ x = if (($Rect.x + $Rect.width / 2) -lt $Width / 2) { $Width - 8 } else { 8 }; y = if (($Rect.y + $Rect.height / 2) -lt $Height / 2) { $Height - 8 } else { 8 } } }
function Direction {
    param($Row, [string]$Quad, [int]$Width, [int]$Height)
    $panel = Get-Rect $Row 'panel'
    $capsule = Get-Rect $Row 'capsule'
    if (-not $panel -or -not $capsule) { return [pscustomobject]@{ available = $false; passed = $false; panel = $panel } }
    $wantRight = $Quad -match 'left'
    $wantBottom = $Quad -match 'top'
    $anchorRight = [bool](Get-Field $Row -Names @('anchor_right'))
    $anchorBottom = [bool](Get-Field $Row -Names @('anchor_bottom'))
    $fixedX = if ($wantRight) { [Math]::Abs($panel.x - $capsule.x) -le 1 } else { [Math]::Abs($panel.right - $capsule.right) -le 1 }
    $fixedY = if ($wantBottom) { [Math]::Abs($panel.y - $capsule.y) -le 1 } else { [Math]::Abs($panel.bottom - $capsule.bottom) -le 1 }
    $inside = Inside $panel $Width $Height
    return [pscustomobject][ordered]@{
        available = $true
        passed = ($anchorRight -eq (-not $wantRight) -and $anchorBottom -eq (-not $wantBottom) -and $fixedX -and $fixedY -and $inside)
        panel = $panel; capsule = $capsule
        expected = ('{0},{1}' -f $(if ($wantRight) { 'right' } else { 'left' }), $(if ($wantBottom) { 'down' } else { 'up' }))
        anchor_right = $anchorRight; anchor_bottom = $anchorBottom
        fixed_x_edge = $fixedX; fixed_y_edge = $fixedY; inside_viewport = $inside
    }
}
function RepairCount { param([string]$Path); if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }; $m = [regex]::Matches((Read-SharedText $Path), 'layout: startup geometry repaired_zones=(\d+)'); if ($m.Count -eq 0) { return $null }; return [int]$m[$m.Count - 1].Groups[1].Value }

function Get-SourceFingerprint {
    param([string]$Repo, [string]$ManifestPath)
    $headOutput = @(& git -C $Repo rev-parse HEAD 2>&1)
    if ($LASTEXITCODE -ne 0 -or $headOutput.Count -ne 1) { throw 'git rev-parse HEAD failed' }
    $paths = @(& git -C $Repo ls-files --cached --others --exclude-standard 2>&1)
    if ($LASTEXITCODE -ne 0) { throw 'git ls-files failed' }
    $entries = New-Object System.Collections.ArrayList
    foreach ($relative in @($paths | Sort-Object -Unique)) {
        $full = Join-Path $Repo ([string]$relative)
        if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { throw "source file disappeared: $relative" }
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $full).Hash.ToLowerInvariant()
        [void]$entries.Add("$hash`t$relative")
    }
    [IO.File]::WriteAllText($ManifestPath, (($entries.ToArray() -join "`n") + "`n"), (New-Object Text.UTF8Encoding($false)))
    return [pscustomobject][ordered]@{
        head = ([string]$headOutput[0]).Trim()
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $ManifestPath).Hash.ToLowerInvariant()
        file_count = $entries.Count
        manifest = [IO.Path]::GetFullPath($ManifestPath)
    }
}

function Read-LiveGeometry {
    param([string]$Path)
    $full = [IO.Path]::GetFullPath($Path)
    if (-not (Test-Path -LiteralPath $full -PathType Leaf)) {
        return [pscustomobject]@{ row_count = 0L; rows = @(); phase_rows = @(); morph_rows = @(); source = $full }
    }
    $entry = $script:LiveGeometryCache[$full]
    $stream = [IO.File]::Open($full, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    try {
        if ($null -eq $entry -or $stream.Length -lt $entry.offset) {
            $entry = [pscustomobject]@{ offset = 0L; trailing = ''; row_count = 0L; latest = @{}; phase_latest = @{}; morph_latest = @{} }
        }
        [void]$stream.Seek([long]$entry.offset, [IO.SeekOrigin]::Begin)
        $reader = New-Object IO.StreamReader($stream, (New-Object Text.UTF8Encoding($false)), $false, 4096, $true)
        try { $chunk = $reader.ReadToEnd() } finally { $reader.Dispose() }
        $entry.offset = $stream.Position
    } finally { $stream.Dispose() }

    $text = $entry.trailing + $chunk
    $lines = @($text -split '\r?\n')
    if ($text.Length -gt 0 -and -not $text.EndsWith("`n")) {
        $entry.trailing = $lines[-1]
        $lines = if ($lines.Count -gt 1) { @($lines[0..($lines.Count - 2)]) } else { @() }
    } else {
        $entry.trailing = ''
    }
    foreach ($line in $lines) {
        $match = [regex]::Match($line, 'zone_geometry_live:\s+(?<body>.*)$')
        if (-not $match.Success) { continue }
        $values = [ordered]@{}
        foreach ($field in [regex]::Matches($match.Groups['body'].Value, '(?<key>[a-z_]+)=(?<value>[^\s]+)')) {
            $values[$field.Groups['key'].Value] = Convert-Scalar $field.Groups['value'].Value
        }
        $valueObject = [pscustomobject]$values
        $phase = [string](Get-Field $valueObject -Names @('phase'))
        $numberNames = @(
            'now_ms', 'zone', 'stack_parent', 'stack_members',
            'viewport_x', 'viewport_y', 'viewport_width', 'viewport_height',
            'home_x', 'home_y', 'stored_width', 'stored_height',
            'capsule_x', 'capsule_y', 'capsule_width', 'capsule_height',
            'panel_x', 'panel_y', 'panel_width', 'panel_height', 'morph',
            'effective_x', 'effective_y', 'effective_width', 'effective_height'
        )
        $numbers = @{}
        $validNumbers = $true
        foreach ($name in $numberNames) {
            $number = Convert-FiniteDouble (Get-Field $valueObject -Names @($name))
            if ($null -eq $number) { $validNumbers = $false; break }
            $numbers[$name] = $number
        }
        $visible = Get-Field $valueObject -Names @('visible')
        $anchorRight = Get-Field $valueObject -Names @('anchor_right')
        $anchorBottom = Get-Field $valueObject -Names @('anchor_bottom')
        $now = $numbers['now_ms']; $morph = $numbers['morph']
        $viewportWidth = $numbers['viewport_width']; $viewportHeight = $numbers['viewport_height']
        if (
            [string]::IsNullOrWhiteSpace($phase) -or
            -not $validNumbers -or
            $visible -isnot [bool] -or $anchorRight -isnot [bool] -or $anchorBottom -isnot [bool] -or
            $null -eq $now -or $now -lt 0 -or $now -gt [uint32]::MaxValue -or $now -ne [Math]::Floor($now) -or
            $numbers['zone'] -lt 1 -or $numbers['zone'] -ne [Math]::Floor($numbers['zone']) -or
            $numbers['stack_parent'] -lt 0 -or $numbers['stack_parent'] -ne [Math]::Floor($numbers['stack_parent']) -or
            $numbers['stack_members'] -lt 0 -or $numbers['stack_members'] -ne [Math]::Floor($numbers['stack_members']) -or
            $null -eq $morph -or $morph -lt 0 -or $morph -gt 1 -or
            $null -eq $viewportWidth -or $viewportWidth -le 0 -or
            $null -eq $viewportHeight -or $viewportHeight -le 0 -or
            $numbers['capsule_width'] -le 0 -or $numbers['capsule_height'] -le 0 -or
            $numbers['panel_width'] -le 0 -or $numbers['panel_height'] -le 0 -or
            $numbers['effective_width'] -le 0 -or $numbers['effective_height'] -le 0
        ) { continue }
        $row = Convert-DumpRow $valueObject
        if (-not $row -or -not $row.extended) { continue }
        $row | Add-Member NoteProperty phase $phase -Force
        $row | Add-Member NoteProperty now_ms ([uint32]$now) -Force
        $row | Add-Member NoteProperty morph $morph -Force
        $row | Add-Member NoteProperty viewport_width $viewportWidth -Force
        $row | Add-Member NoteProperty viewport_height $viewportHeight -Force
        $row | Add-Member NoteProperty candidate_live $true -Force
        $entry.row_count++
        $row | Add-Member NoteProperty live_sequence ([long]$entry.row_count) -Force
        $entry.latest[[int]$row.zone_id] = $row
        $entry.phase_latest[('{0}:{1}' -f [int]$row.zone_id, [string]$row.phase)] = $row
        if ($row.morph -le 0.01) { $entry.morph_latest[('{0}:collapsed' -f [int]$row.zone_id)] = $row }
        if ($row.morph -ge 0.99) { $entry.morph_latest[('{0}:expanded' -f [int]$row.zone_id)] = $row }
    }
    $script:LiveGeometryCache[$full] = $entry
    return [pscustomobject]@{
        row_count = [long]$entry.row_count
        rows = @($entry.latest.Values | Sort-Object zone_id)
        phase_rows = @($entry.phase_latest.Values | Sort-Object live_sequence)
        morph_rows = @($entry.morph_latest.Values | Sort-Object live_sequence)
        source = $full
    }
}

function Get-LiveSnapshot {
    param([string]$Path)
    $cache = Read-LiveGeometry $Path
    return [pscustomobject][ordered]@{
        row_count = $cache.row_count
        zone_count = @($cache.rows).Count
        extended = (@($cache.rows).Count -gt 0 -and @($cache.rows | Where-Object { -not $_.extended }).Count -eq 0)
        rows = @($cache.rows)
        source = $cache.source
    }
}

function Wait-NewLiveRow {
    param([string]$Path, [int]$ZoneId, [int]$AfterCount, [int]$TimeoutMs = 2000)
    $timer = [Diagnostics.Stopwatch]::StartNew()
    do {
        $cache = Read-LiveGeometry $Path
        $match = @($cache.rows | Where-Object { $_.zone_id -eq $ZoneId -and $_.live_sequence -gt $AfterCount })
        if ($match.Count -eq 1) { return $match[0] }
        Start-Sleep -Milliseconds 50
    } while ($timer.ElapsedMilliseconds -lt $TimeoutMs)
    return $null
}
function Wait-NewLivePhase {
    param([string]$Path, [int]$ZoneId, [string]$Phase, [int]$AfterCount, [int]$TimeoutMs = 2000)
    $timer = [Diagnostics.Stopwatch]::StartNew()
    do {
        $cache = Read-LiveGeometry $Path
        $match = @($cache.phase_rows | Where-Object { $_.zone_id -eq $ZoneId -and $_.phase -eq $Phase -and $_.live_sequence -gt $AfterCount })
        if ($match.Count -eq 1) { return $match[0] }
        Start-Sleep -Milliseconds 50
    } while ($timer.ElapsedMilliseconds -lt $TimeoutMs)
    return $null
}
function Wait-LiveMorph {
    param([string]$Path, [int]$ZoneId, [int]$AfterCount, [double]$Minimum, [double]$Maximum, [int]$TimeoutMs = 3000)
    $timer = [Diagnostics.Stopwatch]::StartNew()
    do {
        $cache = Read-LiveGeometry $Path
        $match = @($cache.morph_rows | Where-Object { $_.zone_id -eq $ZoneId -and $_.live_sequence -gt $AfterCount -and $_.morph -ge $Minimum -and $_.morph -le $Maximum })
        if ($match.Count -eq 1) { return $match[0] }
        Start-Sleep -Milliseconds 50
    } while ($timer.ElapsedMilliseconds -lt $TimeoutMs)
    return $null
}
function Wait-StableLiveMorph {
    param([string]$Path, [int]$ZoneId, [int]$AfterCount, [double]$Minimum, [double]$Maximum)
    $first = Wait-LiveMorph $Path $ZoneId $AfterCount $Minimum $Maximum
    if (-not $first) { return $null }
    Start-Sleep -Milliseconds 650
    $cache = Read-LiveGeometry $Path
    $latest = @($cache.rows | Where-Object {
        $_.zone_id -eq $ZoneId -and $_.live_sequence -ge $first.live_sequence -and
        $_.morph -ge $Minimum -and $_.morph -le $Maximum
    })
    $collapsedAfter = @($cache.morph_rows | Where-Object {
        $_.zone_id -eq $ZoneId -and $_.live_sequence -gt $first.live_sequence -and $_.morph -le 0.01
    })
    if ($latest.Count -eq 1 -and $collapsedAfter.Count -eq 0) { return $latest[0] }
    return $null
}

function Rects-Near {
    param($Left, $Right, [double]$Tolerance = 1.0)
    if (-not $Left -or -not $Right) { return $false }
    return [bool]([Math]::Abs($Left.x - $Right.x) -le $Tolerance -and [Math]::Abs($Left.y - $Right.y) -le $Tolerance -and [Math]::Abs($Left.width - $Right.width) -le $Tolerance -and [Math]::Abs($Left.height - $Right.height) -le $Tolerance)
}

function Compare-LiveAndDumpSnapshot {
    param($Live, $Dump)
    $liveIds = @($Live.rows | ForEach-Object { [int]$_.zone_id } | Sort-Object -Unique)
    $dumpIds = @($Dump.rows | ForEach-Object { [int]$_.zone_id } | Sort-Object -Unique)
    $sameIds = ($liveIds.Count -eq $dumpIds.Count -and @(Compare-Object $liveIds $dumpIds).Count -eq 0)
    $rectsMatch = $sameIds
    if ($sameIds) {
        foreach ($id in $dumpIds) {
            $liveRow = Get-Row $Live.rows $id
            $dumpRow = Get-Row $Dump.rows $id
            if (-not (Rects-Near (Get-Rect $liveRow) (Get-Rect $dumpRow)) -or
                -not (Rects-Near (Get-Rect $liveRow 'panel') (Get-Rect $dumpRow 'panel'))) {
                $rectsMatch = $false
                break
            }
        }
    }
    return [pscustomobject][ordered]@{
        same_zone_ids = $sameIds
        rects_match = $rectsMatch
        live_zone_ids = $liveIds
        dump_zone_ids = $dumpIds
        passed = ($sameIds -and $rectsMatch)
    }
}

function Assert-ProofParserFixtures {
    param([string]$Directory)
    $path = Join-Path $Directory 'parser-fixture.log'
    $valid = 'zone_geometry_live: phase=fixture now_ms=1 zone=1 visible=true stack_parent=0 stack_members=0 viewport_x=0 viewport_y=0 viewport_width=100 viewport_height=80 home_x=0 home_y=0 stored_width=20 stored_height=20 capsule_x=0 capsule_y=0 capsule_width=10 capsule_height=10 anchor_right=false anchor_bottom=false panel_x=0 panel_y=0 panel_width=20 panel_height=20 morph=0 effective_x=0 effective_y=0 effective_width=10 effective_height=10'
    $required = @('phase', 'now_ms', 'zone', 'visible', 'stack_parent', 'stack_members', 'viewport_x', 'viewport_y', 'viewport_width', 'viewport_height', 'home_x', 'home_y', 'stored_width', 'stored_height', 'capsule_x', 'capsule_y', 'capsule_width', 'capsule_height', 'anchor_right', 'anchor_bottom', 'panel_x', 'panel_y', 'panel_width', 'panel_height', 'morph', 'effective_x', 'effective_y', 'effective_width', 'effective_height')
    $invalid = foreach ($field in $required) { [regex]::Replace($valid, (' {0}=[^ ]+' -f [regex]::Escape($field)), '', 1) }
    $invalid += $valid.Replace(' visible=true ', ' visible=maybe ')
    $invalid += $valid.Replace(' morph=0 ', ' morph=NaN ')
    [IO.File]::WriteAllLines($path, @($invalid) + $valid, (New-Object Text.UTF8Encoding($false)))
    $parsed = Read-LiveGeometry $path
    $row = @($parsed.rows | Select-Object -First 1)
    if (
        $parsed.row_count -ne 1 -or $row.Count -ne 1 -or $null -ne $row[0].stack_parent -or
        @($parsed.morph_rows).Count -ne 1 -or
        -not (Inside (Get-Rect $row[0]) 100 80) -or
        (Inside ([pscustomobject]@{ x = -0.5; y = 0.0; right = 9.5; bottom = 10.0 }) 100 80)
    ) { throw 'Live geometry parser fail-closed fixture failed' }
    return [pscustomobject][ordered]@{ passed = $true; path = [IO.Path]::GetFullPath($path); accepted_rows = $parsed.row_count }
}

function DragMatrix {
    param($Window, [string]$State, [string]$Dump, $Viewport, [string]$RunDir, [System.Collections.ArrayList]$Commands, [System.Collections.ArrayList]$Shots, [string]$Stderr)
    $initial = Invoke-Dump $Dump $State $Viewport.width $Viewport.height 'drag-initial' $RunDir $Commands
    $ordinary = @($initial.rows | Where-Object { $_.visible -and -not $_.is_stack_anchor -and $null -eq $_.stack_parent })
    $variants = @('small/pill', 'medium/pill', 'large/minimal', 'large/pill', 'small/circle'); $selected = New-Object System.Collections.ArrayList
    foreach ($variant in $variants) { $r = $ordinary | Where-Object { ('{0}/{1}' -f $_.capsule_size, $_.capsule_shape).ToLowerInvariant() -eq $variant } | Select-Object -First 1; if ($r) { [void]$selected.Add($r) } }
    $observed = @($selected | ForEach-Object { ('{0}/{1}' -f $_.capsule_size, $_.capsule_shape).ToLowerInvariant() }); $missing = @($variants | Where-Object { $observed -notcontains $_ }); $stack = @($initial.rows | Where-Object { $_.visible -and $_.is_stack_anchor } | Select-Object -First 1)
    if ($missing.Count -gt 0 -or $stack.Count -eq 0) { throw ('fixture lacks required variants/stack: {0}' -f ($missing -join ',')) }
    $targets = @($selected.ToArray() | Sort-Object capsule_x, capsule_y) + @($stack); $edges = @('top', 'right', 'bottom', 'left'); $stages = New-Object System.Collections.ArrayList
    for ($i = 0; $i -lt $targets.Count; $i++) {
        foreach ($edge in $edges) {
            $id = [int]$targets[$i].zone_id
            $dumpState = Invoke-Dump $Dump $State $Viewport.width $Viewport.height ('drag-{0}-{1}-before' -f $id, $edge) $RunDir $Commands
            $row = Get-Row $dumpState.rows $id
            $rect = Get-Rect $row
            if (-not $rect) { throw "capsule rect missing for $id" }
            $target = EdgeTarget $edge $i $targets.Count $rect $Viewport.width $Viewport.height $dumpState.rows $id
            $beforeCount = (Read-LiveGeometry $Stderr).row_count
            $gesture = Drag-Input $Window ($rect.x + $rect.width / 2) ($rect.y + $rect.height / 2) ($target.x + $rect.width / 2) ($target.y + $rect.height / 2)
            $latched = Wait-NewLivePhase $Stderr $id 'zone_drag_latched' $beforeCount
            $live = Wait-NewLivePhase $Stderr $id 'zone_drag_live' $beforeCount
            $liveRect = Get-Rect $live
            $inside = Inside $liveRect $Viewport.width $Viewport.height
            $touch = if ($liveRect) {
                switch ($edge) {
                    'top' { [Math]::Abs($liveRect.y) -le 2 }
                    'right' { [Math]::Abs($liveRect.right - $Viewport.width) -le 2 }
                    'bottom' { [Math]::Abs($liveRect.bottom - $Viewport.height) -le 2 }
                    default { [Math]::Abs($liveRect.x) -le 2 }
                }
            } else { $false }
            $after = Invoke-Dump $Dump $State $Viewport.width $Viewport.height ('drag-{0}-{1}-after' -f $id, $edge) $RunDir $Commands
            $afterRow = Get-Row $after.rows $id
            $dumpRect = Get-Rect $afterRow
            $crosscheck = Rects-Near $liveRect $dumpRect
            $roleStable = if ($targets[$i].is_stack_anchor) {
                $afterRow.is_stack_anchor -and $null -eq $afterRow.stack_parent
            } else {
                -not $afterRow.is_stack_anchor -and $null -eq $afterRow.stack_parent
            }
            $shot = Save-ProofWindowShot $Window (Join-Path $RunDir ('drag-{0}-{1}.png' -f $id, $edge))
            [void]$Shots.Add($shot)
            [void]$stages.Add([pscustomobject][ordered]@{
                zone_id = $id; edge = $edge; gesture = $gesture
                candidate_drag_latched = $latched; candidate_live = $live; capsule = $liveRect
                inside_viewport = $inside; touches_requested_edge = $touch
                offline_dump_capsule = $dumpRect; live_dump_crosscheck = $crosscheck
                surface_role_stable = $roleStable
                dump_extended = $after.extended; screenshot = $shot
                passed = ($latched -and $live -and $live.extended -and $inside -and $touch -and $crosscheck -and $roleStable -and $after.extended -and $shot.nonblank)
            })
            if (-not ($i -eq $targets.Count - 1 -and $edge -eq $edges[-1])) {
                $away = AwayTarget $liveRect $Viewport.width $Viewport.height
                $collapsedLive = Wait-LiveMorph $Stderr $id $beforeCount 0.0 0.01
                if (-not ($collapsedLive -and $collapsedLive.morph -le 0.01 -and (Rects-Near (Get-Rect $collapsedLive) (Get-Rect $collapsedLive 'effective')))) {
                    throw "zone $id did not collapse between drag stages"
                }
                Move-Input $Window $away.x $away.y
            }
        }
    }
    return [pscustomobject][ordered]@{
        status = if (@($stages | Where-Object { -not $_.passed }).Count -eq 0) { 'ok' } else { 'failed' }
        required_variants = $variants; observed_variants = $observed; missing_variants = $missing
        stack_anchor_id = $stack[0].zone_id; stage_count = $stages.Count; stages = @($stages.ToArray())
        real_mouse_path = (@($stages | Where-Object { -not $_.gesture.real_system_cursor }).Count -eq 0)
        candidate_live = (@($stages | Where-Object { -not $_.candidate_drag_latched -or -not $_.candidate_live -or -not $_.candidate_live.extended }).Count -eq 0)
        dump_extended = (@($stages | Where-Object { -not $_.dump_extended -or -not $_.live_dump_crosscheck }).Count -eq 0)
    }
}
function QuadrantMatrix {
    param($Window, [string]$State, [string]$Dump, $Viewport, [string]$RunDir, [System.Collections.ArrayList]$Commands, [System.Collections.ArrayList]$Shots, [string]$Stderr)
    $quads = @('left-top', 'right-top', 'left-bottom', 'right-bottom')
    $dumpState = Invoke-Dump $Dump $State $Viewport.width $Viewport.height 'quadrants-initial' $RunDir $Commands
    $free = @($dumpState.rows | Where-Object { $_.visible -and -not $_.is_stack_anchor -and $null -eq $_.stack_parent } | Select-Object -First 4)
    if ($free.Count -lt 4) { throw "four quadrant fixtures required, got $($free.Count)" }
    $placements = New-Object System.Collections.ArrayList

    for ($i = 0; $i -lt 4; $i++) {
        $id = [int]$free[$i].zone_id
        $row = Get-Row $dumpState.rows $id
        $rect = Get-Rect $row
        $target = QuadTarget $quads[$i] $rect $Viewport.width $Viewport.height
        $beforeCount = (Read-LiveGeometry $Stderr).row_count
        $gesture = Drag-Input $Window ($rect.x + $rect.width / 2) ($rect.y + $rect.height / 2) ($target.x + $rect.width / 2) ($target.y + $rect.height / 2)
        $latched = Wait-NewLivePhase $Stderr $id 'zone_drag_latched' $beforeCount
        $dragLive = Wait-NewLivePhase $Stderr $id 'zone_drag_live' $beforeCount
        if (-not $latched -or -not $dragLive) { throw "candidate emitted no drag phases after quadrant placement for zone $id" }
        $dumpState = Invoke-Dump $Dump $State $Viewport.width $Viewport.height ('quadrant-place-{0}' -f $id) $RunDir $Commands
        $away = AwayTarget (Get-Rect $dragLive) $Viewport.width $Viewport.height
        Move-Input $Window $away.x $away.y
        $collapsedLive = Wait-LiveMorph $Stderr $id ([long](Get-Field $dragLive -Names @('live_sequence'))) 0.0 0.01
        $collapsed = [bool]($collapsedLive -and $collapsedLive.morph -le 0.01 -and (Rects-Near (Get-Rect $collapsedLive) (Get-Rect $collapsedLive 'effective')))
        if (-not $collapsed) { throw "zone $id did not collapse after quadrant placement" }
        [void]$placements.Add([pscustomobject][ordered]@{ zone_id = $id; quadrant = $quads[$i]; gesture = $gesture; candidate_drag_latched = $latched; candidate_drag_live = $dragLive; collapsed_candidate_live = $collapsedLive })
    }

    $stages = New-Object System.Collections.ArrayList
    for ($i = 0; $i -lt 4; $i++) {
        Set-ProofWindowInputForeground $Window
        $id = [int]$free[$i].zone_id
        $quad = $quads[$i]
        $dumpState = Invoke-Dump $Dump $State $Viewport.width $Viewport.height ('quadrant-{0}-before' -f $quad) $RunDir $Commands
        $rect = Get-Rect (Get-Row $dumpState.rows $id)
        $beforeExpand = (Read-LiveGeometry $Stderr).row_count
        Move-Input $Window ($rect.x + $rect.width / 2) ($rect.y + $rect.height / 2)
        Start-Sleep -Milliseconds 850
        $expandedLive = Wait-NewLiveRow $Stderr $id $beforeExpand
        $direction = Direction $expandedLive $quad $Viewport.width $Viewport.height
        $expandedPanel = Get-Rect $expandedLive 'panel'
        $expandedEffective = Get-Rect $expandedLive 'effective'
        $expandedMorph = [bool]($expandedLive -and $expandedLive.morph -ge 0.99)
        $expandedEffectiveMatches = Rects-Near $expandedPanel $expandedEffective

        $expandedDump = Invoke-Dump $Dump $State $Viewport.width $Viewport.height ('quadrant-{0}-expanded-crosscheck' -f $quad) $RunDir $Commands
        $dumpPanel = Get-Rect (Get-Row $expandedDump.rows $id) 'panel'
        $panelCrosscheck = Rects-Near $expandedPanel $dumpPanel
        $expandedShot = Save-ProofWindowShot $Window (Join-Path $RunDir ('quadrant-{0}-expanded.png' -f $quad))
        [void]$Shots.Add($expandedShot)

        $inlineClick = $false
        $inlineLog = $false
        if ($direction.available) {
            Click-Input $Window ($direction.panel.x + $direction.panel.width - 62) ($direction.panel.y + 24)
            Start-Sleep -Milliseconds 250
            $inlineClick = $true
            $text = Read-SharedText $Stderr
            $inlineLog = $text -match ('search: OpenZoneSearch zone={0} inline=true' -f $id)
        }

        Set-ProofWindowInputForeground $Window
        Chord-Input 0x11 0x4B
        Start-Sleep -Milliseconds 250
        $aux = @(Get-ProofWindowsForPid -TargetProcessId $Window.process_id | Where-Object { $_.visible -and $_.class -eq 'BentoAuxSearch' })
        $text = Read-SharedText $Stderr
        $globalSearch = ($aux.Count -eq 1 -and $text -match 'search: OpenSearch shown hwnd=\d+')
        $keyboardShot = Save-ProofWindowShot $(if ($aux.Count -eq 1) { $aux[0] } else { $Window }) (Join-Path $RunDir ('quadrant-{0}-keyboard.png' -f $quad))
        [void]$Shots.Add($keyboardShot)
        if ($aux.Count -eq 1) { Set-ProofWindowInputForeground $aux[0] }
        Key-Input 0x1B $false
        Key-Input 0x1B $true

        $away = if ($quad -eq 'left-top') {
            [pscustomobject]@{ x = $Viewport.width - 8; y = $Viewport.height - 8 }
        } elseif ($quad -eq 'right-top') {
            [pscustomobject]@{ x = 8; y = $Viewport.height - 8 }
        } elseif ($quad -eq 'left-bottom') {
            [pscustomobject]@{ x = $Viewport.width - 8; y = 8 }
        } else {
            [pscustomobject]@{ x = 8; y = 8 }
        }
        $collapseBaseline = Read-LiveGeometry $Stderr
        $baselineRow = Get-Row $collapseBaseline.rows $id
        if (-not ($baselineRow -and $baselineRow.morph -ge 0.99 -and (Rects-Near (Get-Rect $baselineRow 'panel') (Get-Rect $baselineRow 'effective')))) {
            throw "zone $id was not expanded immediately before collapse"
        }
        Move-Input $Window $away.x $away.y
        $collapsedLive = Wait-LiveMorph $Stderr $id $collapseBaseline.row_count 0.0 0.01
        if (-not $collapsedLive) { throw "zone $id did not emit a fresh collapsed endpoint" }
        $collapsedCapsule = Get-Rect $collapsedLive
        $collapsedEffective = Get-Rect $collapsedLive 'effective'
        $collapsed = [bool]($collapsedLive -and $collapsedLive.morph -le 0.01 -and (Rects-Near $collapsedCapsule $collapsedEffective))
        $collapsedShot = Save-ProofWindowShot $Window (Join-Path $RunDir ('quadrant-{0}-collapsed.png' -f $quad))
        [void]$Shots.Add($collapsedShot)
        $after = Invoke-Dump $Dump $State $Viewport.width $Viewport.height ('quadrant-{0}-after' -f $quad) $RunDir $Commands
        $collapsedCrosscheck = Rects-Near $collapsedCapsule (Get-Rect (Get-Row $after.rows $id))
        $different = (Get-FileHash -Algorithm SHA256 -LiteralPath $expandedShot.path).Hash -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $collapsedShot.path).Hash

        [void]$stages.Add([pscustomobject][ordered]@{
            zone_id = $id; quadrant = $quad
            expanded_candidate_live = $expandedLive; direction = $direction
            expanded_morph_complete = $expandedMorph
            expanded_effective_matches_panel = $expandedEffectiveMatches
            live_dump_panel_crosscheck = $panelCrosscheck
            inline_search_click = $inlineClick; inline_search_log = $inlineLog
            global_search_class = if ($aux.Count -eq 1) { $aux[0].class } else { $null }
            keyboard_search_opened = $globalSearch
            expanded_screenshot = $expandedShot; keyboard_screenshot = $keyboardShot
            collapsed_candidate_live = $collapsedLive; collapsed_to_capsule = $collapsed
            collapsed_screenshot = $collapsedShot
            expanded_collapsed_frames_differ = $different
            live_dump_capsule_crosscheck = $collapsedCrosscheck
            dump_extended = ($expandedDump.extended -and $after.extended)
            passed = ($direction.available -and $direction.passed -and $expandedMorph -and $expandedEffectiveMatches -and $panelCrosscheck -and $inlineClick -and $inlineLog -and $globalSearch -and $collapsed -and $collapsedCrosscheck -and $different -and $expandedShot.nonblank -and $keyboardShot.nonblank -and $collapsedShot.nonblank -and $expandedDump.extended -and $after.extended)
        })
    }

    return [pscustomobject][ordered]@{
        status = if (@($stages | Where-Object { -not $_.passed }).Count -eq 0) { 'ok' } else { 'failed' }
        placement_drags = @($placements.ToArray())
        stages = @($stages.ToArray())
        directional = (@($stages | Where-Object { -not $_.direction.passed -or -not $_.expanded_morph_complete -or -not $_.collapsed_to_capsule }).Count -eq 0)
        physical_search_entries = (@($stages | Where-Object { -not $_.inline_search_log -or -not $_.keyboard_search_opened }).Count -eq 0)
        candidate_live = (@($stages | Where-Object { -not $_.expanded_candidate_live -or -not $_.collapsed_candidate_live }).Count -eq 0)
        dump_extended = (@($stages | Where-Object { -not $_.dump_extended -or -not $_.live_dump_panel_crosscheck -or -not $_.live_dump_capsule_crosscheck }).Count -eq 0)
    }
}
function ClickMatrix {
    param($Window, [string]$State, [string]$Dump, $Viewport, [string]$RunDir, [System.Collections.ArrayList]$Commands, [System.Collections.ArrayList]$Shots, [string]$Stderr, [object[]]$Targets)
    $stages = New-Object System.Collections.ArrayList
    $modeRestored = (Read-SharedText $Stderr) -match 'zone_display_mode restored: click'
    foreach ($target in @($Targets | Sort-Object zone_id)) {
        Set-ProofWindowInputForeground $Window
        $id = [int]$target.zone_id
        $dumpState = Invoke-Dump $Dump $State $Viewport.width $Viewport.height ('click-{0}-before' -f $id) $RunDir $Commands
        $rect = Get-Rect (Get-Row $dumpState.rows $id)
        $before = (Read-LiveGeometry $Stderr).row_count
        Click-Input $Window ($rect.x + $rect.width / 2) ($rect.y + $rect.height / 2)
        $live = Wait-LiveMorph $Stderr $id $before 0.99 1.01
        $direction = Direction $live $target.quadrant $Viewport.width $Viewport.height
        $effectiveMatches = Rects-Near (Get-Rect $live 'panel') (Get-Rect $live 'effective')
        $shot = Save-ProofWindowShot $Window (Join-Path $RunDir ('click-{0}-{1}.png' -f $id, $target.quadrant))
        [void]$Shots.Add($shot)
        $collapseBaseline = Read-LiveGeometry $Stderr
        $baselineRow = Get-Row $collapseBaseline.rows $id
        if (-not $baselineRow) { throw "click zone $id had no live row immediately before Header Close" }
        $panel = Get-Rect $baselineRow 'panel'
        if (-not ($panel -and (Get-Field $baselineRow -Names @('morph')) -ge 0.99 -and (Rects-Near $panel (Get-Rect $baselineRow 'effective')))) {
            throw "click zone $id was not expanded immediately before Header Close"
        }
        $headerHeight = [Math]::Min(48.0, [Math]::Max(0.0, $panel.height))
        $buttonSize = [Math]::Min(28.0, $headerHeight)
        $closeButton = [pscustomobject][ordered]@{
            x = $panel.right - 16.0 - $buttonSize
            y = $panel.y + ($headerHeight - $buttonSize) / 2.0
            width = $buttonSize
            height = $buttonSize
        }
        Click-Input $Window ($closeButton.x + $closeButton.width / 2.0) ($closeButton.y + $closeButton.height / 2.0)
        $collapsed = Wait-LiveMorph $Stderr $id $collapseBaseline.row_count 0.0 0.01
        $collapsedToCapsule = [bool]($collapsed -and (Rects-Near (Get-Rect $collapsed) (Get-Rect $collapsed 'effective')))
        if (-not $collapsedToCapsule) { throw "click zone $id did not emit a fresh collapsed endpoint after Header Close" }
        [void]$stages.Add([pscustomobject][ordered]@{
            zone_id = $id; quadrant = $target.quadrant; candidate_live = $live
            direction = $direction; effective_matches_panel = $effectiveMatches
            header_close_button = $closeButton; header_close_real_mouse = $true
            collapse_baseline_row_count = [long]$collapseBaseline.row_count
            collapsed_candidate_live = $collapsed; collapsed_to_capsule = $collapsedToCapsule
            screenshot = $shot
            passed = ($modeRestored -and $live -and $direction.passed -and $effectiveMatches -and $collapsedToCapsule -and $shot.nonblank)
        })
    }
    return [pscustomobject][ordered]@{
        status = if ($modeRestored -and @($stages | Where-Object { -not $_.passed }).Count -eq 0) { 'ok' } else { 'failed' }
        mode_restored = $modeRestored; real_mouse_path = $true; stages = @($stages.ToArray())
    }
}
function AlwaysMatrix {
    param($Window, $Viewport, [string]$RunDir, [System.Collections.ArrayList]$Shots, [string]$Stderr, [object[]]$Targets)
    $stages = New-Object System.Collections.ArrayList
    $modeRestored = (Read-SharedText $Stderr) -match 'zone_display_mode restored: always'
    foreach ($target in @($Targets | Sort-Object zone_id)) {
        $id = [int]$target.zone_id
        $before = (Read-LiveGeometry $Stderr).row_count
        $current = Get-Row (Get-LiveSnapshot $Stderr).rows $id
        $capsule = Get-Rect $current
        Move-Input $Window ($capsule.x + $capsule.width / 2) ($capsule.y + $capsule.height / 2)
        $live = Wait-LiveMorph $Stderr $id $before 0.99 1.01
        $direction = Direction $live $target.quadrant $Viewport.width $Viewport.height
        $effectiveMatches = Rects-Near (Get-Rect $live 'panel') (Get-Rect $live 'effective')
        $shot = Save-ProofWindowShot $Window (Join-Path $RunDir ('always-{0}-{1}.png' -f $id, $target.quadrant))
        [void]$Shots.Add($shot)
        [void]$stages.Add([pscustomobject][ordered]@{
            zone_id = $id; quadrant = $target.quadrant; candidate_live = $live
            direction = $direction; effective_matches_panel = $effectiveMatches; screenshot = $shot
            passed = ($modeRestored -and $live -and $direction.passed -and $effectiveMatches -and $shot.nonblank)
        })
    }
    return [pscustomobject][ordered]@{
        status = if ($modeRestored -and @($stages | Where-Object { -not $_.passed }).Count -eq 0) { 'ok' } else { 'failed' }
        mode_restored = $modeRestored; real_mouse_path = $true; stages = @($stages.ToArray())
    }
}
function KeyboardMatrix {
    param($Window, $Viewport, [string]$RunDir, [System.Collections.ArrayList]$Shots, [string]$Stderr, [object[]]$Targets)
    $stages = New-Object System.Collections.ArrayList
    Move-Input $Window ($Viewport.width / 2) ($Viewport.height / 2)
    Start-Sleep -Milliseconds 850
    foreach ($target in @($Targets | Sort-Object zone_id)) {
        Set-ProofWindowInputForeground $Window
        $id = [int]$target.zone_id
        $before = (Read-LiveGeometry $Stderr).row_count
        Chord-Input 0x11 0xDD
        $live = Wait-StableLiveMorph $Stderr $id $before 0.99 1.01
        if (-not $live) { throw "keyboard FocusNextZone did not remain visibly expanded for zone $id" }
        $direction = Direction $live $target.quadrant $Viewport.width $Viewport.height
        $effectiveMatches = Rects-Near (Get-Rect $live 'panel') (Get-Rect $live 'effective')
        $hotkeyLog = (Read-SharedText $Stderr) -match 'hotkey: id=\d+ command=FocusNextZone'
        $shot = Save-ProofWindowShot $Window (Join-Path $RunDir ('keyboard-{0}-{1}.png' -f $id, $target.quadrant))
        [void]$Shots.Add($shot)
        [void]$stages.Add([pscustomobject][ordered]@{
            zone_id = $id; quadrant = $target.quadrant; candidate_live = $live
            direction = $direction; effective_matches_panel = $effectiveMatches
            focus_next_hotkey_log = $hotkeyLog; screenshot = $shot
            passed = ($live -and $direction.passed -and $effectiveMatches -and $hotkeyLog -and $shot.nonblank)
        })
    }
    return [pscustomobject][ordered]@{
        status = if (@($stages | Where-Object { -not $_.passed }).Count -eq 0) { 'ok' } else { 'failed' }
        real_keyboard_path = $true; stages = @($stages.ToArray())
    }
}
function FreeSearchMatrix {
    param($Window, $Viewport, [string]$RunDir, [System.Collections.ArrayList]$Shots, [string]$Stderr, [object[]]$Targets)
    $requiredQuadrants = @('left-top', 'right-top', 'left-bottom', 'right-bottom')
    $orderedTargets = @($Targets | Sort-Object zone_id)
    $observedQuadrants = @($orderedTargets | ForEach-Object { [string]$_.quadrant } | Sort-Object -Unique)
    if ($orderedTargets.Count -ne 4 -or @(Compare-Object $requiredQuadrants $observedQuadrants).Count -ne 0) {
        throw ('free Zone Search requires four distinct quadrants; observed={0}' -f ($observedQuadrants -join ','))
    }
    $stages = New-Object System.Collections.ArrayList
    foreach ($target in $orderedTargets) {
        $id = [int]$target.zone_id
        Move-Input $Window ($Viewport.width / 2) ($Viewport.height / 2)
        Start-Sleep -Milliseconds 850
        Set-ProofWindowInputForeground $Window
        $before = (Read-LiveGeometry $Stderr).row_count
        Chord-Input 0x11 0x4B
        Start-Sleep -Milliseconds 250
        $aux = @(Get-ProofWindowsForPid -TargetProcessId $Window.process_id | Where-Object { $_.visible -and $_.class -eq 'BentoAuxSearch' })
        if ($aux.Count -ne 1) { throw "free Zone Search did not open exactly one BentoAuxSearch window" }
        Set-ProofWindowInputForeground $aux[0]
        Text-Input ('Proof Zone {0}' -f $id)
        Set-ProofWindowInputForeground $aux[0]
        Key-Input 0x0D $false
        Key-Input 0x0D $true
        $live = Wait-StableLiveMorph $Stderr $id $before 0.99 1.01
        if (-not $live) { throw "free Zone Search did not remain visibly expanded for zone $id" }
        $direction = Direction $live $target.quadrant $Viewport.width $Viewport.height
        $effectiveMatches = Rects-Near (Get-Rect $live 'panel') (Get-Rect $live 'effective')
        $text = Read-SharedText $Stderr
        $activationLog = $text -match ('search: ActivateSearchResult id=zone:{0} kind=Zone' -f $id)
        $hidden = @(Get-ProofWindowsForPid -TargetProcessId $Window.process_id | Where-Object { $_.visible -and $_.class -eq 'BentoAuxSearch' }).Count -eq 0
        $shot = Save-ProofWindowShot $Window (Join-Path $RunDir ('free-search-{0}-{1}.png' -f $id, $target.quadrant))
        [void]$Shots.Add($shot)
        [void]$stages.Add([pscustomobject][ordered]@{
            zone_id = $id; quadrant = $target.quadrant; query = ('Proof Zone {0}' -f $id)
            candidate_live = $live; direction = $direction; effective_matches_panel = $effectiveMatches
            activation_log = $activationLog; search_window_hidden_after_activation = $hidden
            screenshot = $shot
            passed = ($live -and $direction.passed -and $effectiveMatches -and $activationLog -and $hidden -and $shot.nonblank)
        })
    }
    return [pscustomobject][ordered]@{
        status = if ($stages.Count -eq 4 -and @($stages | Where-Object { -not $_.passed }).Count -eq 0) { 'ok' } else { 'failed' }
        required_quadrants = $requiredQuadrants; observed_quadrants = $observedQuadrants
        stage_count = $stages.Count; real_unicode_keyboard_path = $true; stages = @($stages.ToArray())
    }
}
function Session {
    param(
        [string]$Label, [string]$Executable, [string]$State, [string]$Dump,
        [string]$RunDir, [System.Collections.ArrayList]$Commands,
        [System.Collections.ArrayList]$Shots,
        [ValidateSet('None', 'Drag', 'Quadrants', 'Click', 'Always', 'Keyboard', 'FreeSearch')][string]$Exercise = 'None',
        [object[]]$Targets = @()
    )
    $stdout = Join-Path $RunDir ($Label + '-stdout.log')
    $stderr = Join-Path $RunDir ($Label + '-stderr.log')
    $process = $null
    $window = $null
    $metrics = $null
    $dumpState = $null
    $liveSnapshot = $null
    $initialCrosscheck = $null
    $exerciseResult = $null
    $failure = $null
    $exited = $false
    $repair = $null

    try {
        $process = Start-IsolatedBentoDesk -Executable $Executable -WorkingDirectory $RunDir -StateDirectory $State -StdoutPath $stdout -StderrPath $stderr -ExtraEnvironment @{ BENTODESK_ANIM_PROOF_LOG = '1' }
        $window = Wait-ProofWindow -TargetProcessId $process.Id -ClassName 'BentoDeskShell' -TimeoutMs 15000
        if (-not $window) { throw "$Label did not create BentoDeskShell" }
        $window | Add-Member NoteProperty process_id $process.Id -Force
        Start-Sleep -Milliseconds 1100
        Set-ProofWindowInputForeground $window
        $metrics = Metrics $window
        if ($ExpectedDpi -gt 0 -and $metrics.dpi -ne $ExpectedDpi) { throw "DPI expected $ExpectedDpi observed $($metrics.dpi)" }
        if ($ExpectedViewportWidth -gt 0 -and $metrics.logical_viewport.width -ne $ExpectedViewportWidth) { throw "viewport width expected $ExpectedViewportWidth observed $($metrics.logical_viewport.width)" }
        if ($ExpectedViewportHeight -gt 0 -and $metrics.logical_viewport.height -ne $ExpectedViewportHeight) { throw "viewport height expected $ExpectedViewportHeight observed $($metrics.logical_viewport.height)" }
        if ($metrics.logical_viewport.width -ne [int]$metrics.work_logical.width -or $metrics.logical_viewport.height -ne [int]$metrics.work_logical.height) { throw "logical viewport does not match work area rounded to nearest DIP" }
        if ($metrics.taskbar.edge -ne 'bottom') { throw "bottom taskbar required; observed edge=$($metrics.taskbar.edge)" }
        if ($ExpectedTaskbarDip -gt 0 -and $metrics.taskbar.bottom_logical -ne $ExpectedTaskbarDip) { throw "bottom taskbar expected $ExpectedTaskbarDip DIP; observed $($metrics.taskbar.bottom_logical)" }

        $dumpState = Invoke-Dump $Dump $State $metrics.logical_viewport.width $metrics.logical_viewport.height ($Label + '-initial') $RunDir $Commands
        $liveSnapshot = Get-LiveSnapshot $stderr
        if (-not $liveSnapshot.extended) { throw "$Label candidate emitted no complete live geometry snapshot" }
        $initialCrosscheck = Compare-LiveAndDumpSnapshot $liveSnapshot $dumpState
        if (-not $initialCrosscheck.passed) { throw "$Label candidate live/dump zone set or rect cross-check failed" }
        $repair = RepairCount $stderr
        [void]$Shots.Add((Save-ProofWindowShot $window (Join-Path $RunDir ($Label + '-initial.png'))))

        if ($Exercise -ne 'None') {
            $viewport = [pscustomobject]@{
                width = [int]$metrics.logical_viewport.width
                height = [int]$metrics.logical_viewport.height
            }
            $exerciseResult = switch ($Exercise) {
                'Drag' { [pscustomobject][ordered]@{ drag = DragMatrix $window $State $Dump $viewport $RunDir $Commands $Shots $stderr } }
                'Quadrants' { [pscustomobject][ordered]@{ quadrants = QuadrantMatrix $window $State $Dump $viewport $RunDir $Commands $Shots $stderr } }
                'Click' { [pscustomobject][ordered]@{ click = ClickMatrix $window $State $Dump $viewport $RunDir $Commands $Shots $stderr $Targets } }
                'Always' { [pscustomobject][ordered]@{ always = AlwaysMatrix $window $viewport $RunDir $Shots $stderr $Targets } }
                'Keyboard' { [pscustomobject][ordered]@{ keyboard = KeyboardMatrix $window $viewport $RunDir $Shots $stderr $Targets } }
                'FreeSearch' { [pscustomobject][ordered]@{ free_search = FreeSearchMatrix $window $viewport $RunDir $Shots $stderr $Targets } }
            }
        }

        Set-ProofWindowInputForeground $window
        Chord-Input 0x11 0x51
        $exited = Wait-ProofProcessExit -TargetProcessId $process.Id -TimeoutMs 8000
        if (-not $exited) { throw "$Label did not exit through physical Ctrl+Q" }
    } catch {
        $failure = $_.Exception.Message
    } finally {
        if ($process -and (Get-Process -Id $process.Id -ErrorAction SilentlyContinue)) {
            [void](Stop-ProofProcessExact -TargetProcessId $process.Id -Executable $Executable)
        }
        if ($process) { $process.WaitForExit() }
    }

    $residue = @(Get-ExactExecutableProcesses -Executable $Executable)
    $zonePath = Join-Path $State 'zones.bin'
    $item = if (Test-Path -LiteralPath $zonePath) { Get-Item -LiteralPath $zonePath } else { $null }
    return [pscustomobject][ordered]@{
        label = $Label
        process_id = if ($process) { $process.Id } else { $null }
        stdout = [IO.Path]::GetFullPath($stdout)
        stderr = [IO.Path]::GetFullPath($stderr)
        display = $metrics
        input_foreground = if ($window) {
            [ordered]@{
                verified = [bool](Get-Field $window -Names @('foreground_verified'))
                hwnd = Get-Field $window -Names @('foreground_hwnd')
                process_id = Get-Field $window -Names @('foreground_process_id')
            }
        } else { $null }
        logical_viewport = if ($metrics) { $metrics.logical_viewport } else { $null }
        repair_count = $repair
        initial_dump = $dumpState
        initial_live = $liveSnapshot
        initial_live_dump_crosscheck = $initialCrosscheck
        exercise = $exerciseResult
        quit_physical = ($null -ne $process -and $exited)
        quit_exited = $exited
        state_hash = if ($item) { (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash.ToLowerInvariant() } else { $null }
        state_bytes = if ($item) { [int64]$item.Length } else { $null }
        exact_path_residue = $residue.Count
        failure = $failure
        passed = (-not $failure -and $exited -and (Get-Field $liveSnapshot -Names @('extended')) -and (Get-Field $initialCrosscheck -Names @('passed')) -and $residue.Count -eq 0)
    }
}

$repo = Get-ProofRepoRoot
$run = New-ProofRunDirectory -Name 'zone-workarea-directional' -ArtifactsRoot $ProofOutputRoot
$root = $run.Directory
$bin = Join-Path $root 'bin'
$buildTarget = if ([string]::IsNullOrWhiteSpace($BuildTargetDirectory)) {
    Join-Path $root 'build-target'
} else {
    [IO.Path]::GetFullPath($BuildTargetDirectory)
}
$summaryPath = Join-Path $root 'summary.json'
$commands = New-Object System.Collections.ArrayList
$shots = New-Object System.Collections.ArrayList
$fixtures = New-Object System.Collections.ArrayList
$legacyProducers = New-Object System.Collections.ArrayList
$results = New-Object System.Collections.ArrayList
$failures = New-Object System.Collections.ArrayList
$sourceMeta = $null
$proofMeta = $null
$dumpSourceMeta = $null
$dumpMeta = $null
$itemGridSourceMeta = $null
$itemGridMeta = $null
$zoneItemsSourceMeta = $null
$zoneItemsMeta = $null
$sourceBefore = $null
$sourceAfterBuild = $null
$sourceFinal = $null
$freshSourceBuild = $false
$parserFixture = $null
$fixtureHashesOk = $false
$legacyProducerIdentityOk = $false
$proofExe = Join-Path $bin 'BentoDesk.exe'
$proofDumpExe = Join-Path $bin 'dump_zone_capsules.exe'
$proofItemGridExe = Join-Path $bin 'dump_zone_item_grid.exe'
$proofZoneItemsExe = Join-Path $bin 'dump_zone_items.exe'
$dumpExe = $null
$itemGridExe = $null
$zoneItemsExe = $null
$previousCargo = $null
$customBinaryInputs = [bool]($PSBoundParameters.ContainsKey('ExecutablePath') -or $PSBoundParameters.ContainsKey('DumpExecutablePath') -or $PSBoundParameters.ContainsKey('ItemGridDumpExecutablePath') -or $PSBoundParameters.ContainsKey('ZoneItemsDumpExecutablePath'))

try {
    New-Item -ItemType Directory -Path $bin -Force | Out-Null
    [void](Assert-ProofPathUnder $bin $root)
    [void](Assert-ProofPathUnder $proofExe $root)
    [void](Assert-ProofPathUnder $proofDumpExe $root)
    [void](Assert-ProofPathUnder $proofItemGridExe $root)
    [void](Assert-ProofPathUnder $proofZoneItemsExe $root)
    $parserFixture = Assert-ProofParserFixtures $root
    $sourceBefore = Get-SourceFingerprint $repo (Join-Path $root 'source-before.tsv')

    $environment = @{ CARGO_BUILD_JOBS = '1'; CARGO_INCREMENTAL = '0' }
    if (-not $SkipBuild) {
        if ($customBinaryInputs) { throw 'custom binary paths are diagnostic-only and require -SkipBuild' }
        if ($PSBoundParameters.ContainsKey('BuildTargetDirectory') -and
            (Test-Path -LiteralPath $buildTarget) -and
            @(Get-ChildItem -LiteralPath $buildTarget -Force).Count -gt 0) {
            throw 'external build target must be absent or empty for a fresh proof build'
        }
        New-Item -ItemType Directory -Path $buildTarget -Force | Out-Null
        if (-not $PSBoundParameters.ContainsKey('BuildTargetDirectory')) {
            [void](Assert-ProofPathUnder $buildTarget $root)
        }
        $environment.CARGO_TARGET_DIR = $buildTarget
    }
    $previousCargo = Set-ProofProcessEnvironment $environment

    if (-not $SkipBuild) {
        $build = Invoke-ProofCommand -Name 'release-build' -FilePath 'cargo' -Arguments @('build', '--locked', '--release', '--target', 'x86_64-pc-windows-msvc', '-p', 'bentodesk-shell', '--bin', 'BentoDesk') -WorkingDirectory $repo -LogDirectory $root
        [void]$commands.Add($build)
        if (-not $build.passed) { throw 'release build failed' }
        $dumpBuild = Invoke-ProofCommand -Name 'capsule-dump-build' -FilePath 'cargo' -Arguments @('build', '--locked', '--release', '--target', 'x86_64-pc-windows-msvc', '-p', 'bentodesk-shell', '--example', 'dump_zone_capsules') -WorkingDirectory $repo -LogDirectory $root
        [void]$commands.Add($dumpBuild)
        if (-not $dumpBuild.passed) { throw 'dump_zone_capsules build failed' }
        $itemGridBuild = Invoke-ProofCommand -Name 'item-dump-build' -FilePath 'cargo' -Arguments @('build', '--locked', '--release', '--target', 'x86_64-pc-windows-msvc', '-p', 'bentodesk-platform', '--example', 'dump_zone_item_grid', '--example', 'dump_zone_items') -WorkingDirectory $repo -LogDirectory $root
        [void]$commands.Add($itemGridBuild)
        if (-not $itemGridBuild.passed) { throw 'item dump helper build failed' }
        $ExecutablePath = Join-Path $buildTarget 'x86_64-pc-windows-msvc\release\BentoDesk.exe'
        $dumpExe = Join-Path $buildTarget 'x86_64-pc-windows-msvc\release\examples\dump_zone_capsules.exe'
        $itemGridExe = Join-Path $buildTarget 'x86_64-pc-windows-msvc\release\examples\dump_zone_item_grid.exe'
        $zoneItemsExe = Join-Path $buildTarget 'x86_64-pc-windows-msvc\release\examples\dump_zone_items.exe'
        $freshSourceBuild = $true
    } else {
        if (-not $ExecutablePath) { $ExecutablePath = Join-Path $repo 'target\x86_64-pc-windows-msvc\release\BentoDesk.exe' }
        $dumpExe = if ($DumpExecutablePath) { [IO.Path]::GetFullPath($DumpExecutablePath) } else { Join-Path $repo 'target\x86_64-pc-windows-msvc\debug\examples\dump_zone_capsules.exe' }
        $itemGridExe = if ($ItemGridDumpExecutablePath) { [IO.Path]::GetFullPath($ItemGridDumpExecutablePath) } else { Join-Path $repo 'target\x86_64-pc-windows-msvc\debug\examples\dump_zone_item_grid.exe' }
        $zoneItemsExe = if ($ZoneItemsDumpExecutablePath) { [IO.Path]::GetFullPath($ZoneItemsDumpExecutablePath) } else { Join-Path $repo 'target\x86_64-pc-windows-msvc\debug\examples\dump_zone_items.exe' }
    }

    $sourceAfterBuild = Get-SourceFingerprint $repo (Join-Path $root 'source-after-build.tsv')
    if ($sourceBefore.head -ne $sourceAfterBuild.head -or $sourceBefore.sha256 -ne $sourceAfterBuild.sha256) { throw 'source tree changed while building proof binaries' }

    $sourceMeta = Meta $ExecutablePath
    $expectedFileVersion = if ($ExpectedProductVersion) { "$ExpectedProductVersion.0" } else { $null }
    if ($ExpectedProductVersion -and $sourceMeta.product_version -ne $ExpectedProductVersion) { throw "source PE ProductVersion mismatch: $($sourceMeta.product_version)" }
    if ($expectedFileVersion -and $sourceMeta.file_version -ne $expectedFileVersion) { throw "source PE FileVersion mismatch: $($sourceMeta.file_version)" }
    Copy-Item -LiteralPath $sourceMeta.path -Destination $proofExe -Force
    $proofMeta = Meta $proofExe
    if ($sourceMeta.sha256 -ne $proofMeta.sha256 -or $sourceMeta.bytes -ne $proofMeta.bytes) { throw 'proof executable hash/size mismatch' }

    $dumpSourceMeta = Meta $dumpExe
    Copy-Item -LiteralPath $dumpSourceMeta.path -Destination $proofDumpExe -Force
    $dumpMeta = Meta $proofDumpExe
    if ($dumpSourceMeta.sha256 -ne $dumpMeta.sha256 -or $dumpSourceMeta.bytes -ne $dumpMeta.bytes) { throw 'proof dump executable hash/size mismatch' }
    $dumpExe = $proofDumpExe

    $itemGridSourceMeta = Meta $itemGridExe
    Copy-Item -LiteralPath $itemGridSourceMeta.path -Destination $proofItemGridExe -Force
    $itemGridMeta = Meta $proofItemGridExe
    if ($itemGridSourceMeta.sha256 -ne $itemGridMeta.sha256 -or $itemGridSourceMeta.bytes -ne $itemGridMeta.bytes) { throw 'proof item-grid dump executable hash/size mismatch' }

    $zoneItemsSourceMeta = Meta $zoneItemsExe
    Copy-Item -LiteralPath $zoneItemsSourceMeta.path -Destination $proofZoneItemsExe -Force
    $zoneItemsMeta = Meta $proofZoneItemsExe
    if ($zoneItemsSourceMeta.sha256 -ne $zoneItemsMeta.sha256 -or $zoneItemsSourceMeta.bytes -ne $zoneItemsMeta.bytes) { throw 'proof zone-items dump executable hash/size mismatch' }

    $producerInputs = @(
        [pscustomobject]@{ label = 'legacy-2.0.9'; executable = $Legacy209ProducerExe; expected_hash = $Legacy209ProducerExpectedSha256; expected_version = '2.0.9' },
        [pscustomobject]@{ label = 'legacy-2.0.10'; executable = $Legacy210ProducerExe; expected_hash = $Legacy210ProducerExpectedSha256; expected_version = '2.0.10' }
    )
    foreach ($producer in $producerInputs) {
        if (-not $producer.executable) { throw "missing required $($producer.label) producer executable" }
        if (-not $producer.expected_hash -or $producer.expected_hash -notmatch '^[0-9a-fA-F]{64}$') { throw "missing or invalid producer SHA-256 for $($producer.label)" }
        $meta = Meta $producer.executable
        $expectedHash = ([string]$producer.expected_hash).ToLowerInvariant()
        if ($meta.sha256 -ne $expectedHash) { throw "$($producer.label) producer SHA-256 mismatch: $($meta.sha256)" }
        if ($meta.product_version -ne $producer.expected_version -or $meta.file_version -ne "$($producer.expected_version).0") { throw "$($producer.label) producer PE version mismatch" }
        [void]$legacyProducers.Add([pscustomobject][ordered]@{
            label = $producer.label; executable = $meta
            expected_sha256 = $expectedHash; hash_matches = $true
            expected_product_version = $producer.expected_version
            expected_file_version = "$($producer.expected_version).0"
        })
    }
    $legacyProducerIdentityOk = ($legacyProducers.Count -eq 2 -and @($legacyProducers | Where-Object { -not $_.hash_matches }).Count -eq 0)

    $fixtureInputs = @(
        [pscustomobject]@{ label = 'legacy-2.0.9'; source = $Legacy209State; expected = $Legacy209ExpectedSha256; producer = @($legacyProducers | Where-Object { $_.label -eq 'legacy-2.0.9' })[0] },
        [pscustomobject]@{ label = 'legacy-2.0.10'; source = $Legacy210State; expected = $Legacy210ExpectedSha256; producer = @($legacyProducers | Where-Object { $_.label -eq 'legacy-2.0.10' })[0] },
        [pscustomobject]@{ label = 'custom'; source = $StateSeed; expected = $StateSeedExpectedSha256; producer = $null }
    )
    foreach ($input in $fixtureInputs) {
        if (-not $input.source) { throw "missing required $($input.label) state fixture" }
        if (-not $input.expected -or $input.expected -notmatch '^[0-9a-fA-F]{64}$') { throw "missing or invalid expected SHA-256 for $($input.label)" }
        $zones = [IO.Path]::GetFullPath($input.source)
        if (Test-Path -LiteralPath $zones -PathType Container) { $zones = Join-Path $zones 'zones.bin' }
        if (-not (Test-Path -LiteralPath $zones -PathType Leaf)) { throw "state fixture zones.bin not found: $zones" }
        $item = Get-Item -LiteralPath $zones
        $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $zones).Hash.ToLowerInvariant()
        $expectedHash = ([string]$input.expected).ToLowerInvariant()
        if ($actualHash -ne $expectedHash) { throw "$($input.label) fixture SHA-256 mismatch: $actualHash" }
        [void]$fixtures.Add([pscustomobject][ordered]@{
            label = $input.label
            source_input = [IO.Path]::GetFullPath($input.source)
            source_zones = $zones
            expected_hash = $expectedHash
            source_hash = $actualHash
            hash_matches = $true
            producer = $input.producer
            source_bytes = [int64]$item.Length
            proof_directory = Join-Path $root $input.label
        })
    }
    $fixtureHashesOk = ($fixtures.Count -eq 3 -and @($fixtures | Where-Object { -not $_.hash_matches }).Count -eq 0)

    foreach ($fixture in $fixtures) {
        $runDir = $fixture.proof_directory
        $stateDir = Join-Path $runDir 'state'
        New-Item -ItemType Directory -Path $runDir, $stateDir -Force | Out-Null
        [void](Assert-ProofPathUnder $runDir $root)
        [void](Assert-ProofPathUnder $stateDir $runDir)
        $isolatedZones = Join-Path $stateDir 'zones.bin'
        Copy-Item -LiteralPath $fixture.source_zones -Destination $isolatedZones -Force
        $initialCopyHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $isolatedZones).Hash.ToLowerInvariant()
        if ($initialCopyHash -ne $fixture.source_hash) { throw "$($fixture.label) isolated fixture copy mismatch" }

        $quadrantSession = $null
        $quadrantState = $null
        $quadrantCopyHash = $null
        $activationSessions = New-Object System.Collections.ArrayList
        if ($fixture.label -eq 'custom') {
            $quadrantState = Join-Path $runDir 'quadrant-state'
            New-Item -ItemType Directory -Path $quadrantState -Force | Out-Null
            [void](Assert-ProofPathUnder $quadrantState $runDir)
            Copy-Item -LiteralPath $fixture.source_zones -Destination (Join-Path $quadrantState 'zones.bin') -Force
            $quadrantCopyHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $quadrantState 'zones.bin')).Hash.ToLowerInvariant()
            if ($quadrantCopyHash -ne $fixture.source_hash) { throw 'custom quadrant fixture copy mismatch' }
            $first = Session 'first' $proofExe $stateDir $dumpExe $runDir $commands $shots -Exercise Drag
            if (-not (Get-Field $first -Names @('passed'))) {
                throw ('prerequisite session failed: label={0}; failure={1}' -f (Get-Field $first -Names @('label')), (Get-Field $first -Names @('failure')))
            }
            $quadrantSession = Session 'quadrants' $proofExe $quadrantState $dumpExe $runDir $commands $shots -Exercise Quadrants
            if (-not (Get-Field $quadrantSession -Names @('passed'))) {
                throw ('prerequisite session failed: label={0}; failure={1}' -f (Get-Field $quadrantSession -Names @('label')), (Get-Field $quadrantSession -Names @('failure')))
            }
            $quadrantExercise = Get-Field (Get-Field $quadrantSession -Names @('exercise')) -Names @('quadrants')
            $targets = @(Get-Field $quadrantExercise -Names @('placement_drags') | Where-Object { $null -ne $_ })
            if ($targets.Count -ne 4) { throw "four persisted quadrant targets required for activation proof, got $($targets.Count)" }
            foreach ($activation in @(
                [pscustomobject]@{ label = 'click'; exercise = 'Click'; vault_mode = 'click' },
                [pscustomobject]@{ label = 'always'; exercise = 'Always'; vault_mode = 'always' },
                [pscustomobject]@{ label = 'keyboard'; exercise = 'Keyboard'; vault_mode = $null },
                [pscustomobject]@{ label = 'free-search'; exercise = 'FreeSearch'; vault_mode = $null }
            )) {
                $activationState = Join-Path $runDir ($activation.label + '-state')
                New-Item -ItemType Directory -Path $activationState -Force | Out-Null
                [void](Assert-ProofPathUnder $activationState $runDir)
                $activationZones = Join-Path $activationState 'zones.bin'
                Copy-Item -LiteralPath (Join-Path $quadrantState 'zones.bin') -Destination $activationZones -Force
                $expectedActivationHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $quadrantState 'zones.bin')).Hash.ToLowerInvariant()
                $activationHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $activationZones).Hash.ToLowerInvariant()
                if ($activationHash -ne $expectedActivationHash) { throw "$($activation.label) quadrant state copy mismatch" }
                $vault = if ($activation.vault_mode) { Write-ZoneDisplayModeVault $activationState $activation.vault_mode } else { $null }
                $session = Session $activation.label $proofExe $activationState $dumpExe $runDir $commands $shots -Exercise $activation.exercise -Targets $targets
                if (-not (Get-Field $session -Names @('passed'))) {
                    throw ('activation session failed: label={0}; failure={1}' -f (Get-Field $session -Names @('label')), (Get-Field $session -Names @('failure')))
                }
                $matrixResult = Get-Field (Get-Field $session -Names @('exercise')) -Names @($activation.exercise.ToLowerInvariant().Replace('freesearch', 'free_search'))
                if (-not $matrixResult -or (Get-Field $matrixResult -Names @('status')) -ne 'ok') {
                    throw ('activation matrix failed: label={0}; status={1}' -f $activation.label, (Get-Field $matrixResult -Names @('status')))
                }
                [void]$activationSessions.Add([pscustomobject][ordered]@{
                    label = $activation.label; exercise = $activation.exercise
                    state_dir = $activationState; source_state_sha256 = $expectedActivationHash
                    initial_copy_sha256 = $activationHash; vault = $vault
                    session = $session; matrix = $matrixResult
                })
            }
        } else {
            $first = Session 'first' $proofExe $stateDir $dumpExe $runDir $commands $shots
        }
        $restart = Session 'restart' $proofExe $stateDir $dumpExe $runDir $commands $shots
        $firstHash = Get-Field $first -Names @('state_hash')
        $restartHash = Get-Field $restart -Names @('state_hash')
        $firstRepair = Get-Field $first -Names @('repair_count')
        $restartRepair = Get-Field $restart -Names @('repair_count')
        $stable = [bool]($firstHash -and $restartHash -and $firstHash -eq $restartHash)
        $firstRepairRecorded = ($null -ne $firstRepair)
        $legacyRepairRequired = ($fixture.label -eq 'legacy-2.0.9' -or $fixture.label -eq 'legacy-2.0.10')
        $expectedFirstRepair = ($firstRepairRecorded -and (-not $legacyRepairRequired -or $firstRepair -gt 0))
        $firstOutputChanged = [bool]($firstHash -and $firstHash -ne $initialCopyHash)
        $firstChangeRequirementMet = [bool](-not $legacyRepairRequired -or $firstOutputChanged)
        $zeroRepair = ($null -ne $restartRepair -and $restartRepair -eq 0)
        $activationPassed = ($activationSessions.Count -eq $(if ($fixture.label -eq 'custom') { 4 } else { 0 }) -and @($activationSessions | Where-Object { -not $_.session.passed -or $_.session.repair_count -ne 0 -or $_.matrix.status -ne 'ok' }).Count -eq 0)
        $quadrantPassed = ($null -eq $quadrantSession -or ($quadrantSession.passed -and $quadrantSession.repair_count -eq 0 -and $activationPassed))
        $residueZero = ($first.exact_path_residue -eq 0 -and $restart.exact_path_residue -eq 0 -and ($null -eq $quadrantSession -or $quadrantSession.exact_path_residue -eq 0) -and @($activationSessions | Where-Object { $_.session.exact_path_residue -ne 0 }).Count -eq 0)
        $exercise = if ($quadrantSession) {
            [pscustomobject][ordered]@{
                drag = Get-Field (Get-Field $first -Names @('exercise')) -Names @('drag')
                quadrants = Get-Field (Get-Field $quadrantSession -Names @('exercise')) -Names @('quadrants')
                click = Get-Field (Get-Field (@($activationSessions | Where-Object { $_.label -eq 'click' })[0].session) -Names @('exercise')) -Names @('click')
                always = Get-Field (Get-Field (@($activationSessions | Where-Object { $_.label -eq 'always' })[0].session) -Names @('exercise')) -Names @('always')
                keyboard = Get-Field (Get-Field (@($activationSessions | Where-Object { $_.label -eq 'keyboard' })[0].session) -Names @('exercise')) -Names @('keyboard')
                free_search = Get-Field (Get-Field (@($activationSessions | Where-Object { $_.label -eq 'free-search' })[0].session) -Names @('exercise')) -Names @('free_search')
            }
        } else { $null }
        [void]$results.Add([pscustomobject][ordered]@{
            label = $fixture.label
            source = [ordered]@{
                input = $fixture.source_input; zones = $fixture.source_zones
                expected_sha256 = $fixture.expected_hash; sha256 = $fixture.source_hash
                bytes = $fixture.source_bytes; isolated_copy_sha256 = $initialCopyHash
                quadrant_copy_sha256 = $quadrantCopyHash
                producer = $fixture.producer
            }
            isolated_state_dir = $stateDir
            quadrant_state_dir = $quadrantState
            first = $first
            quadrant_session = $quadrantSession
            activation_sessions = @($activationSessions.ToArray())
            restart = $restart
            exercise = $exercise
            first_repair_recorded = $firstRepairRecorded
            expected_first_repair = $expectedFirstRepair
            first_output_changed_from_input = $firstOutputChanged
            first_change_requirement_met = $firstChangeRequirementMet
            first_to_restart_hash_stable = $stable
            restart_zero_repeat_repair = $zeroRepair
            exact_path_residue_zero = $residueZero
            passed = ($first.passed -and $quadrantPassed -and $restart.passed -and $expectedFirstRepair -and $firstChangeRequirementMet -and $stable -and $zeroRepair -and $residueZero)
        })
    }

    $sourceFinal = Get-SourceFingerprint $repo (Join-Path $root 'source-final.tsv')
    if ($sourceBefore.head -ne $sourceFinal.head -or $sourceBefore.sha256 -ne $sourceFinal.sha256) { throw 'source tree changed during runtime proof' }
} catch {
    [void]$failures.Add($_.Exception.Message)
} finally {
    if ($previousCargo) { Restore-ProofProcessEnvironment $previousCargo }
}

$required = @($results | Where-Object { $_.label -eq 'legacy-2.0.9' -or $_.label -eq 'legacy-2.0.10' })
$matrix = @($results | Where-Object { $_.label -eq 'custom' })
$matrixExercise = if ($matrix.Count -eq 1) { Get-Field $matrix[0] -Names @('exercise') } else { $null }
$dragResult = Get-Field $matrixExercise -Names @('drag')
$quadrantResult = Get-Field $matrixExercise -Names @('quadrants')
$clickResult = Get-Field $matrixExercise -Names @('click')
$alwaysResult = Get-Field $matrixExercise -Names @('always')
$keyboardResult = Get-Field $matrixExercise -Names @('keyboard')
$freeSearchResult = Get-Field $matrixExercise -Names @('free_search')
$allDrag = [bool]($dragResult -and (Get-Field $dragResult -Names @('status')) -eq 'ok' -and (Get-Field $dragResult -Names @('candidate_live')))
$allQuadrants = [bool]($quadrantResult -and (Get-Field $quadrantResult -Names @('status')) -eq 'ok' -and (Get-Field $quadrantResult -Names @('candidate_live')))
$allClick = [bool]($clickResult -and (Get-Field $clickResult -Names @('status')) -eq 'ok')
$allAlways = [bool]($alwaysResult -and (Get-Field $alwaysResult -Names @('status')) -eq 'ok')
$allKeyboard = [bool]($keyboardResult -and (Get-Field $keyboardResult -Names @('status')) -eq 'ok')
$allFreeSearch = [bool]($freeSearchResult -and (Get-Field $freeSearchResult -Names @('status')) -eq 'ok')
$allActivation = [bool]($allClick -and $allAlways -and $allKeyboard -and $allFreeSearch)
$initialDumpsExtended = ($results.Count -eq 3)
$initialLiveExtended = ($results.Count -eq 3)
$display = New-Object System.Collections.ArrayList
foreach ($result in $results) {
    foreach ($sessionName in @('first', 'restart')) {
        $session = Get-Field $result -Names @($sessionName)
        $dump = Get-Field $session -Names @('initial_dump')
        $live = Get-Field $session -Names @('initial_live')
        $crosscheck = Get-Field $session -Names @('initial_live_dump_crosscheck')
        if (-not (Get-Field $dump -Names @('extended')) -or -not (Get-Field $crosscheck -Names @('passed'))) { $initialDumpsExtended = $false }
        if (-not (Get-Field $live -Names @('extended')) -or -not (Get-Field $crosscheck -Names @('passed'))) { $initialLiveExtended = $false }
    }
    $first = Get-Field $result -Names @('first')
    $observedDisplay = Get-Field $first -Names @('display')
    if ($observedDisplay) { [void]$display.Add($observedDisplay) }
}
$matrixDumpsExtended = [bool]($dragResult -and $quadrantResult -and (Get-Field $dragResult -Names @('dump_extended')) -and (Get-Field $quadrantResult -Names @('dump_extended')))
$allDump = [bool]($initialDumpsExtended -and $matrixDumpsExtended)
$allLive = [bool]($initialLiveExtended -and $allDrag -and $allQuadrants -and $allActivation)
$allShots = ($shots.Count -gt 0 -and @($shots | Where-Object { -not $_.nonblank }).Count -eq 0)
$allSessions = ($required.Count -eq 2 -and $matrix.Count -eq 1 -and @($results | Where-Object { -not $_.passed }).Count -eq 0)
$displayOk = ($results.Count -eq 3 -and $display.Count -eq 3)
$displayOk = $displayOk -and @($display | Where-Object { $_.logical_viewport.width -ne [int]$_.work_logical.width -or $_.logical_viewport.height -ne [int]$_.work_logical.height }).Count -eq 0
$displayOk = $displayOk -and @($display | Where-Object { $_.taskbar.edge -ne 'bottom' }).Count -eq 0
if ($ExpectedDpi -gt 0) { $displayOk = $displayOk -and @($display | Where-Object { $_.dpi -ne $ExpectedDpi }).Count -eq 0 }
if ($ExpectedViewportWidth -gt 0) { $displayOk = $displayOk -and @($display | Where-Object { $_.logical_viewport.width -ne $ExpectedViewportWidth }).Count -eq 0 }
if ($ExpectedViewportHeight -gt 0) { $displayOk = $displayOk -and @($display | Where-Object { $_.logical_viewport.height -ne $ExpectedViewportHeight }).Count -eq 0 }
if ($ExpectedTaskbarDip -gt 0) { $displayOk = $displayOk -and @($display | Where-Object { $_.taskbar.bottom_logical -ne $ExpectedTaskbarDip }).Count -eq 0 }

$sourceFrozen = [bool]($sourceBefore -and $sourceAfterBuild -and $sourceFinal -and $sourceBefore.head -eq $sourceAfterBuild.head -and $sourceBefore.head -eq $sourceFinal.head -and $sourceBefore.sha256 -eq $sourceAfterBuild.sha256 -and $sourceBefore.sha256 -eq $sourceFinal.sha256)
$expectedFileVersion = if ($ExpectedProductVersion) { "$ExpectedProductVersion.0" } else { $null }
$claims = [ordered]@{
    proof_parser_fail_closed = [bool]($parserFixture -and $parserFixture.passed)
    fresh_locked_same_tree_build = [bool]($freshSourceBuild -and -not $SkipBuild -and -not $customBinaryInputs)
    source_tree_frozen = $sourceFrozen
    exact_hash_candidate = [bool]($sourceMeta -and $proofMeta -and $sourceMeta.sha256 -eq $proofMeta.sha256 -and $sourceMeta.bytes -eq $proofMeta.bytes)
    expected_pe_version = [bool]($sourceMeta -and (-not $ExpectedProductVersion -or ($sourceMeta.product_version -eq $ExpectedProductVersion -and $sourceMeta.file_version -eq $expectedFileVersion)))
    dump_helper_same_source_copy = [bool]($dumpSourceMeta -and $dumpMeta -and $dumpSourceMeta.sha256 -eq $dumpMeta.sha256 -and $dumpSourceMeta.bytes -eq $dumpMeta.bytes -and $sourceFrozen)
    item_grid_dump_helper_same_source_copy = [bool]($itemGridSourceMeta -and $itemGridMeta -and $itemGridSourceMeta.sha256 -eq $itemGridMeta.sha256 -and $itemGridSourceMeta.bytes -eq $itemGridMeta.bytes -and $sourceFrozen)
    zone_items_dump_helper_same_source_copy = [bool]($zoneItemsSourceMeta -and $zoneItemsMeta -and $zoneItemsSourceMeta.sha256 -eq $zoneItemsMeta.sha256 -and $zoneItemsSourceMeta.bytes -eq $zoneItemsMeta.bytes -and $sourceFrozen)
    fixture_source_hashes_exact = [bool]$fixtureHashesOk
    legacy_producer_binary_identity_exact = [bool]$legacyProducerIdentityOk
    isolated_state_dir = [bool]($results.Count -gt 0 -and @($results | Where-Object { -not ([IO.Path]::GetFullPath($_.isolated_state_dir).StartsWith([IO.Path]::GetFullPath($root), [StringComparison]::OrdinalIgnoreCase)) }).Count -eq 0)
    current_display_workarea_dpi_taskbar = [bool]$displayOk
    real_system_cursor_wm_drag_path = [bool]($allDrag -and (Get-Field $dragResult -Names @('real_mouse_path')))
    four_edges_candidate_live_capsule_containment = [bool]$allDrag
    hover_directional_four_quadrants_candidate_live = [bool]($allQuadrants -and (Get-Field $quadrantResult -Names @('directional')))
    click_directional_four_quadrants_candidate_live = $allClick
    always_directional_four_quadrants_candidate_live = $allAlways
    keyboard_directional_four_quadrants_candidate_live = $allKeyboard
    free_zone_search_activation_candidate_live = $allFreeSearch
    physical_inline_and_global_search = [bool]($allQuadrants -and (Get-Field $quadrantResult -Names @('physical_search_entries')))
    candidate_live_home_capsule_anchor_panel_effective = [bool]$allLive
    offline_dump_same_source_crosscheck = [bool]$allDump
    screenshots_nonblank = [bool]$allShots
    first_restart_readback_and_zero_repeat_repair = [bool]$allSessions
    exact_path_residue_zero = [bool]($results.Count -gt 0 -and @($results | Where-Object { -not $_.exact_path_residue_zero }).Count -eq 0)
    stderr_recorded = [bool]($results.Count -gt 0 -and @($results | Where-Object { -not (Test-Path -LiteralPath $_.first.stderr) -or -not (Test-Path -LiteralPath $_.restart.stderr) -or ($_.quadrant_session -and -not (Test-Path -LiteralPath $_.quadrant_session.stderr)) -or @($_.activation_sessions | Where-Object { -not (Test-Path -LiteralPath $_.session.stderr) }).Count -gt 0 }).Count -eq 0)
}
$failedClaims = @($claims.GetEnumerator() | Where-Object { -not $_.Value } | ForEach-Object { $_.Key })
$status = if ($failures.Count -eq 0 -and $failedClaims.Count -eq 0) { 'ok' } else { 'failed' }
$summary = [ordered]@{
    status = $status
    run_id = $run.Id
    generated_utc = (Get-Date).ToUniversalTime().ToString('o')
    repo = $repo
    source_tree = [ordered]@{ before = $sourceBefore; after_build = $sourceAfterBuild; final = $sourceFinal }
    build = [ordered]@{ skip_build = [bool]$SkipBuild; fresh_locked_isolated_target = $freshSourceBuild; custom_binary_inputs = $customBinaryInputs; target_dir = $buildTarget }
    parser_fixture = $parserFixture
    candidate = [ordered]@{ source = $sourceMeta; proof = $proofMeta; copied_to = $proofExe; expected_product_version = $ExpectedProductVersion; expected_file_version = $expectedFileVersion }
    dump_zone_capsules = [ordered]@{ source = $dumpSourceMeta; proof = $dumpMeta; copied_to = $proofDumpExe; role = 'same-tree offline persistence cross-check only' }
    dump_zone_item_grid = [ordered]@{ source = $itemGridSourceMeta; proof = $itemGridMeta; copied_to = $proofItemGridExe; role = 'same-tree item persistence readback only' }
    dump_zone_items = [ordered]@{ source = $zoneItemsSourceMeta; proof = $zoneItemsMeta; copied_to = $proofZoneItemsExe; role = 'same-tree item identity and icon migration readback only' }
    legacy_producers = @($legacyProducers.ToArray())
    run_directory = $root
    fixtures = @($results.ToArray())
    screenshots = @($shots.ToArray())
    commands = @($commands.ToArray())
    claims = $claims
    failed_claims = $failedClaims
    failures = @($failures.ToArray())
    final_candidate_boundary = @(
        'Runtime geometry claims come from the candidate process stderr; dump_zone_capsules is only a same-tree persistence cross-check.',
        'This script physically covers drag, Hover, Click, Always, keyboard direction, Inline Search, global Ctrl+K Search, free-Zone Search activation, collapse, and restart.',
        'Four-quadrant close/search header, wheel/max-scroll, Search/Suggestor highlight, and drop-target/preview seams remain bound to the same source tree by automated production-path tests.',
        'Any later product source, version resource, manifest, or release-workflow change invalidates this candidate receipt.'
    )
}
Write-ProofJson -Value $summary -Path $summaryPath -Depth 24
Write-Host ('Zone workarea/directional proof: {0}' -f $summaryPath)
if ($status -ne 'ok') {
    [Console]::Error.WriteLine('zone workarea/directional proof failed: {0}' -f ($failedClaims -join ', '))
    exit 1
}
exit 0
