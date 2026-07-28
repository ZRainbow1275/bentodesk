#requires -version 5.1
param([switch]$SkipBuild)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'ProofTools.psm1') -Force

function Send-ProofHotkey {
    param(
        [Parameter(Mandatory = $true)]$Window,
        [Parameter(Mandatory = $true)][int]$Id
    )

    if (-not [BentoDeskProofNative]::PostMessageW(
        [IntPtr]$Window.hwnd,
        0x0312,
        [UIntPtr]([uint64]$Id),
        [IntPtr]::Zero
    )) {
        throw "WM_HOTKEY id=$Id could not be posted"
    }
}

function Send-ProofKeyDown {
    param(
        [Parameter(Mandatory = $true)]$Window,
        [Parameter(Mandatory = $true)][int]$VirtualKey
    )

    $nativeResult = [UIntPtr]::Zero
    $sent = [BentoDeskProofNative]::SendMessageTimeoutW(
        [IntPtr]$Window.hwnd,
        0x0100,
        [UIntPtr]([uint64]$VirtualKey),
        [IntPtr]1,
        [BentoDeskProofNative]::SMTO_ABORTIFHUNG,
        2500,
        [ref]$nativeResult
    )
    if ($sent -eq [IntPtr]::Zero) {
        throw ('WM_KEYDOWN timed out for virtual key 0x{0:X2}' -f $VirtualKey)
    }
}

function Wait-ProofLogMatch {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Pattern,
        [int]$TimeoutMs = 5000
    )

    $timer = [Diagnostics.Stopwatch]::StartNew()
    while ($timer.ElapsedMilliseconds -lt $TimeoutMs) {
        if (Test-Path -LiteralPath $Path) {
            $match = Select-String -LiteralPath $Path -Pattern $Pattern |
                Select-Object -Last 1
            if ($match) {
                return $match.Line
            }
        }
        Start-Sleep -Milliseconds 50
    }
    return $null
}

function Wait-ProofWindowHidden {
    param(
        [Parameter(Mandatory = $true)][int]$TargetProcessId,
        [Parameter(Mandatory = $true)][string]$ClassName,
        [int]$TimeoutMs = 3000
    )

    $timer = [Diagnostics.Stopwatch]::StartNew()
    while ($timer.ElapsedMilliseconds -lt $TimeoutMs) {
        $visible = Get-ProofWindowsForPid -TargetProcessId $TargetProcessId |
            Where-Object { $_.class -eq $ClassName -and $_.visible } |
            Select-Object -First 1
        if (-not $visible) {
            return $true
        }
        Start-Sleep -Milliseconds 50
    }
    return $false
}

function Read-CapsuleRows {
    param([Parameter(Mandatory = $true)][string]$Path)

    $rows = New-Object System.Collections.ArrayList
    foreach ($line in [System.IO.File]::ReadAllLines($Path)) {
        if ($line -notmatch '^\d+\t') {
            continue
        }
        $fields = $line -split "`t"
        if ($fields.Count -ne 10) {
            throw "unexpected capsule dump row: $line"
        }
        [void]$rows.Add([pscustomobject]@{
            zone_id = [int]$fields[0]
            x = [int]$fields[1]
            y = [int]$fields[2]
            width = [int]$fields[3]
            height = [int]$fields[4]
            visible = ($fields[5] -eq 'true')
            stack_parent = if ($fields[6] -eq '-') { $null } else { [int]$fields[6] }
            stack_member_count = [int]$fields[7]
            capsule_size = $fields[8]
            capsule_shape = $fields[9]
        })
    }
    return @($rows.ToArray())
}

function Test-EqualTrackSpacing {
    param(
        [Parameter(Mandatory = $true)][double[]]$Centers,
        [double]$Tolerance = 3.0
    )

    if ($Centers.Count -lt 3) {
        return $false
    }
    $sorted = [double[]]@($Centers | Sort-Object)
    $deltas = for ($index = 1; $index -lt $sorted.Count; $index++) {
        $sorted[$index] - $sorted[$index - 1]
    }
    return (($deltas | Measure-Object -Maximum).Maximum -
        ($deltas | Measure-Object -Minimum).Minimum) -le $Tolerance
}

function Get-CoordinateClusterCount {
    param(
        [Parameter(Mandatory = $true)][double[]]$Values,
        [double]$Tolerance = 2.0
    )

    if ($Values.Count -eq 0) {
        return 0
    }
    $sorted = [double[]]@($Values | Sort-Object)
    $count = 1
    $clusterStart = $sorted[0]
    for ($index = 1; $index -lt $sorted.Count; $index++) {
        if ([Math]::Abs($sorted[$index] - $clusterStart) -gt $Tolerance) {
            $count++
            $clusterStart = $sorted[$index]
        }
    }
    return $count
}

function Test-GridAlignment {
    param(
        [Parameter(Mandatory = $true)]$Rows,
        [double]$Tolerance = 2.0
    )

    $columns = [int][Math]::Ceiling([Math]::Sqrt($Rows.Count))
    $expectedRows = [int][Math]::Ceiling($Rows.Count / [double]$columns)
    $xCenters = [double[]]@(
        $Rows | ForEach-Object { 2 * $_.x + $_.width }
    )
    $yCenters = [double[]]@(
        $Rows | ForEach-Object { 2 * $_.y + $_.height }
    )
    $coordinatePairs = @(
        $Rows |
            ForEach-Object {
                '{0},{1}' -f (2 * $_.x + $_.width), (2 * $_.y + $_.height)
            } |
            Sort-Object -Unique
    )
    return (
        (Get-CoordinateClusterCount -Values $xCenters -Tolerance $Tolerance) -eq $columns -and
        (Get-CoordinateClusterCount -Values $yCenters -Tolerance $Tolerance) -eq $expectedRows -and
        $coordinatePairs.Count -eq $Rows.Count
    )
}

$repo = Get-ProofRepoRoot
$run = New-ProofRunDirectory -Name 'layout-arrangements'
$runDirectory = $run.Directory
$stateDirectory = Join-Path $runDirectory 'state'
$itemRoot = Join-Path $stateDirectory 'items'
$binDirectory = Join-Path $runDirectory 'bin'
$stdoutPath = Join-Path $runDirectory 'stdout.log'
$stderrPath = Join-Path $runDirectory 'stderr.log'
$summaryPath = Join-Path $runDirectory 'summary.json'
$sourceExe = Join-Path $repo 'target\x86_64-pc-windows-msvc\release\BentoDesk.exe'
$proofExe = Join-Path $binDirectory 'BentoDesk.exe'
$dumpExe = Join-Path $repo 'target\x86_64-pc-windows-msvc\debug\examples\dump_zone_capsules.exe'
$commands = New-Object System.Collections.ArrayList
$stages = New-Object System.Collections.ArrayList
$process = $null
$mainWindow = $null
$failure = $null
$quitPosted = $false
$exitedThroughQuit = $false
$bulkOpenCount = 0
$seed = $null
$bulkClass = 'BentoAuxBlkMg'
$bulkHotkeyId = 16968

New-Item -ItemType Directory -Path $stateDirectory, $binDirectory -Force | Out-Null
[void](Assert-ProofPathUnder -Path $stateDirectory -Parent $runDirectory)
[void](Assert-ProofPathUnder -Path $proofExe -Parent $runDirectory)

$previousCargo = Set-ProofProcessEnvironment -Values @{
    CARGO_BUILD_JOBS = '1'
    CARGO_INCREMENTAL = '0'
}

try {
    if (-not $SkipBuild) {
        $build = Invoke-ProofCommand `
            -Name '01-release-build' `
            -FilePath 'cargo' `
            -Arguments @(
                'build', '--release',
                '-p', 'bento-nano-shell',
                '--bin', 'BentoDesk',
                '--target', 'x86_64-pc-windows-msvc'
            ) `
            -WorkingDirectory $repo `
            -LogDirectory $runDirectory
        [void]$commands.Add($build)
        if (-not $build.passed) {
            throw 'release build failed'
        }
    }
    $dumpBuild = Invoke-ProofCommand `
        -Name '02-build-capsule-dump' `
        -FilePath 'cargo' `
        -Arguments @(
            'build',
            '-p', 'bento-nano-shell',
            '--example', 'dump_zone_capsules',
            '--target', 'x86_64-pc-windows-msvc'
        ) `
        -WorkingDirectory $repo `
        -LogDirectory $runDirectory
    [void]$commands.Add($dumpBuild)
    if (-not $dumpBuild.passed -or -not (Test-Path -LiteralPath $dumpExe)) {
        throw 'capsule geometry dump helper build failed'
    }
    if (-not (Test-Path -LiteralPath $sourceExe)) {
        throw "release executable not found: $sourceExe"
    }

    $previousSeed = Set-ProofProcessEnvironment -Values @{
        BENTODESK_NANO_BENCHMARK_ITEM_ROOT = $itemRoot
        BENTODESK_NANO_BENCHMARK_REFERENCE_0602 = '1'
    }
    try {
        $seed = Invoke-ProofCommand `
            -Name '03-seed-layout-scene' `
            -FilePath 'cargo' `
            -Arguments @(
                'run', '--quiet',
                '-p', 'bento-nano-platform',
                '--example', 'seed_benchmark_scene',
                '--target', 'x86_64-pc-windows-msvc',
                '--', $stateDirectory
            ) `
            -WorkingDirectory $repo `
            -LogDirectory $runDirectory
        [void]$commands.Add($seed)
    } finally {
        Restore-ProofProcessEnvironment -Values $previousSeed
    }
    if (-not $seed.passed -or
        (Get-Content -LiteralPath $seed.log -Raw) -notmatch 'seeded 11 zones') {
        throw '11-zone layout scene seed failed'
    }

    Copy-Item -LiteralPath $sourceExe -Destination $proofExe
    $alreadyRunning = @(Get-Process -Name 'BentoDesk' -ErrorAction SilentlyContinue)
    if ($alreadyRunning.Count -ne 0) {
        throw "BentoDesk is already running (PID: $($alreadyRunning.Id -join ', ')); close it before isolated proof"
    }

    $process = Start-IsolatedBentoDesk `
        -Executable $proofExe `
        -WorkingDirectory $runDirectory `
        -StateDirectory $stateDirectory `
        -StdoutPath $stdoutPath `
        -StderrPath $stderrPath
    $mainWindow = Wait-ProofWindow `
        -TargetProcessId $process.Id `
        -ClassName 'BentoNanoShell' `
        -TimeoutMs 12000
    if (-not $mainWindow) {
        throw 'BentoDesk main window was not found'
    }
    Start-Sleep -Milliseconds 800

    # GetClientRect is DPI-virtualized into the same logical coordinate space
    # used by the target HWND and persisted Zone positions. Dividing by the
    # target DPI a second time would shrink the proof viewport incorrectly.
    $logicalWidth = [int]$mainWindow.client.width
    $logicalHeight = [int]$mainWindow.client.height
    if ($logicalWidth -le 0 -or $logicalHeight -le 0) {
        throw "invalid logical main viewport: ${logicalWidth}x${logicalHeight}"
    }

    $algorithmSpecs = @(
        [pscustomobject]@{ name = 'grid'; vk = 0x47 },
        [pscustomobject]@{ name = 'row'; vk = 0x52 },
        [pscustomobject]@{ name = 'column'; vk = 0x43 },
        [pscustomobject]@{ name = 'spiral'; vk = 0x50 },
        [pscustomobject]@{ name = 'organic'; vk = 0x4F }
    )
    foreach ($spec in $algorithmSpecs) {
        Send-ProofHotkey -Window $mainWindow -Id $bulkHotkeyId
        $bulkWindow = Wait-ProofWindow `
            -TargetProcessId $process.Id `
            -ClassName $bulkClass `
            -TimeoutMs 5000
        if (-not $bulkWindow) {
            throw "BulkManager did not open for $($spec.name)"
        }
        $bulkOpenCount++
        Send-ProofKeyDown -Window $bulkWindow -VirtualKey $spec.vk
        $logLine = Wait-ProofLogMatch `
            -Path $stderrPath `
            -Pattern ("bulk: BulkApplyLayout algorithm={0} changed=\d+ matched=11" -f $spec.name)
        if (-not $logLine) {
            throw "BulkManager did not dispatch $($spec.name) through the production key route"
        }
        Send-ProofKeyDown -Window $bulkWindow -VirtualKey 0x1B
        if (-not (Wait-ProofWindowHidden `
            -TargetProcessId $process.Id `
            -ClassName $bulkClass)) {
            throw "BulkManager did not close after $($spec.name)"
        }
        Start-Sleep -Milliseconds 120
        Request-ProofPaint -Window $mainWindow

        $dump = Invoke-ProofCommand `
            -Name ("layout-{0}-capsules" -f $spec.name) `
            -FilePath $dumpExe `
            -Arguments @($stateDirectory) `
            -WorkingDirectory $repo `
            -LogDirectory $runDirectory
        [void]$commands.Add($dump)
        if (-not $dump.passed) {
            throw "capsule geometry dump failed after $($spec.name)"
        }
        $capsules = @(Read-CapsuleRows -Path $dump.log)
        $visibleTopLevel = @(
            $capsules |
                Where-Object { $_.visible -and $null -eq $_.stack_parent } |
                Sort-Object zone_id
        )
        $insideViewport = $visibleTopLevel.Count -eq 11 -and @(
            $visibleTopLevel | Where-Object {
                $_.x -lt 0 -or
                $_.y -lt 0 -or
                $_.x + $_.width -gt $logicalWidth -or
                $_.y + $_.height -gt $logicalHeight
            }
        ).Count -eq 0

        $alignment = $true
        $distribution = $true
        if ($spec.name -eq 'row') {
            $tops = @($visibleTopLevel | ForEach-Object { $_.y })
            $alignment = (($tops | Measure-Object -Maximum).Maximum -
                ($tops | Measure-Object -Minimum).Minimum) -le 1
            $centers = [double[]]@(
                $visibleTopLevel | ForEach-Object { $_.x + $_.width / 2.0 }
            )
            $distribution = Test-EqualTrackSpacing -Centers $centers
        } elseif ($spec.name -eq 'column') {
            $lefts = @($visibleTopLevel | ForEach-Object { $_.x })
            $alignment = (($lefts | Measure-Object -Maximum).Maximum -
                ($lefts | Measure-Object -Minimum).Minimum) -le 1
            $centers = [double[]]@(
                $visibleTopLevel | ForEach-Object { $_.y + $_.height / 2.0 }
            )
            $distribution = Test-EqualTrackSpacing -Centers $centers
        } elseif ($spec.name -eq 'grid') {
            $alignment = Test-GridAlignment -Rows $visibleTopLevel
        } else {
            $distinct = @(
                $visibleTopLevel |
                    ForEach-Object { "$($_.x),$($_.y)" } |
                    Sort-Object -Unique
            ).Count
            $distribution = $distinct -ge 8
        }

        $screenshot = Save-ProofWindowShot `
            -Window $mainWindow `
            -Path (Join-Path $runDirectory ("layout-{0}.png" -f $spec.name))
        [void]$stages.Add([pscustomobject]@{
            algorithm = $spec.name
            virtual_key = ('0x{0:X2}' -f $spec.vk)
            log = $logLine
            visible_top_level_count = $visibleTopLevel.Count
            capsules = $visibleTopLevel
            inside_viewport = [bool]$insideViewport
            alignment = [bool]$alignment
            distribution = [bool]$distribution
            screenshot = $screenshot
            passed = [bool](
                $insideViewport -and
                $alignment -and
                $distribution -and
                $screenshot.nonblank
            )
        })
    }

    $quitPosted = Send-ProofQuitHotkey -Window $mainWindow
    $exitedThroughQuit = Wait-ProofProcessExit -TargetProcessId $process.Id -TimeoutMs 6000
    if (-not $exitedThroughQuit) {
        throw 'BentoDesk did not exit through production quit hotkey 16973'
    }
} catch {
    $failure = $_.Exception.Message
} finally {
    Restore-ProofProcessEnvironment -Values $previousCargo
    if ($process -and (Get-Process -Id $process.Id -ErrorAction SilentlyContinue)) {
        [void](Stop-ProofProcessExact -TargetProcessId $process.Id -Executable $proofExe)
    }
}

$stderrText = if (Test-Path -LiteralPath $stderrPath) {
    Get-Content -LiteralPath $stderrPath -Raw
} else {
    ''
}
$allStagesPassed = $stages.Count -eq 5 -and
    @($stages | Where-Object { -not $_.passed }).Count -eq 0
$runtimeAssertions = [ordered]@{
    production_bulk_hotkey = [bool](
        $bulkOpenCount -eq 5 -and
        ([regex]::Matches($stderrText, 'hotkey: id=16968 command=OpenBulkManager')).Count -eq 5
    )
    all_five_layout_commands = [bool](
        @(
            @('grid', 'row', 'column', 'spiral', 'organic') |
                Where-Object {
                    $stderrText -notmatch
                        ("bulk: BulkApplyLayout algorithm={0} changed=\d+ matched=11" -f $_)
                }
        ).Count -eq 0
    )
    all_capsules_inside_main_work_area = [bool](
        $stages.Count -eq 5 -and
        @($stages | Where-Object { -not $_.inside_viewport }).Count -eq 0
    )
    alignment_and_distribution = [bool](
        $stages.Count -eq 5 -and
        @($stages | Where-Object { -not $_.alignment -or -not $_.distribution }).Count -eq 0
    )
    screenshots_nonblank = [bool](
        $stages.Count -eq 5 -and
        @($stages | Where-Object { -not $_.screenshot.nonblank }).Count -eq 0
    )
    production_quit_hotkey = [bool]($quitPosted -and $exitedThroughQuit)
}
$allRuntimeAssertionsPassed =
    @($runtimeAssertions.Values | Where-Object { -not $_ }).Count -eq 0
$status = if (-not $failure -and $allStagesPassed -and $allRuntimeAssertionsPassed) {
    'ok'
} else {
    'failed'
}
$binary = if (Test-Path -LiteralPath $proofExe) {
    Get-Item -LiteralPath $proofExe
} else {
    $null
}
$summary = [ordered]@{
    status = $status
    run_id = $run.Id
    generated_utc = (Get-Date).ToUniversalTime().ToString('o')
    repo = $repo
    isolated_state_dir = $stateDirectory
    executable = [ordered]@{
        path = if ($binary) { $binary.FullName } else { $proofExe }
        bytes = if ($binary) { [int64]$binary.Length } else { $null }
        sha256 = if ($binary) {
            (Get-FileHash -Algorithm SHA256 -LiteralPath $proofExe).Hash.ToLowerInvariant()
        } else {
            $null
        }
    }
    process_id = if ($process) { $process.Id } else { $null }
    main_window = $mainWindow
    logical_viewport = [ordered]@{
        width = if ($mainWindow) {
            [int]$mainWindow.client.width
        } else {
            $null
        }
        height = if ($mainWindow) {
            [int]$mainWindow.client.height
        } else {
            $null
        }
    }
    scene = [ordered]@{
        zones = 11
        stack_anchors = 0
        seed_log = if ($seed) { $seed.log } else { $null }
    }
    bulk_manager = [ordered]@{
        class = $bulkClass
        hotkey_id = $bulkHotkeyId
        open_count = $bulkOpenCount
    }
    stages = @($stages)
    runtime_assertions = $runtimeAssertions
    commands = @($commands)
    quit_hotkey_posted = $quitPosted
    exited_through_quit_hotkey = $exitedThroughQuit
    failure = $failure
}
Write-ProofJson -Value $summary -Path $summaryPath -Depth 16
Write-Host "Layout arrangements proof: $summaryPath"

if ($status -ne 'ok') {
    $detail = if ($failure) { $failure } else { 'one or more layout proof assertions failed' }
    throw $detail
}
