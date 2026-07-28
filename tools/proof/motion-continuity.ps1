#requires -version 5.1
param([switch]$SkipBuild)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'ProofTools.psm1') -Force

function Get-LogLineCount {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return 0
    }
    return @(Get-Content -LiteralPath $Path).Count
}

function ConvertFrom-AnimStateLine {
    param([string]$Line, [int]$LineNumber)

    $values = [ordered]@{ line_number = $LineNumber; raw = $Line }
    foreach ($match in [regex]::Matches($Line, '(\w+)=([^ ]+)')) {
        $values[$match.Groups[1].Value] = $match.Groups[2].Value
    }
    foreach ($name in @('now_ms', 'duration_ms', 'zone', 'occupancy')) {
        if ($values.Contains($name)) {
            $values[$name] = [uint64]$values[$name]
        }
    }
    foreach ($name in @('value', 'from', 'to', 'pill_morph_value', 'stack_bloom_progress')) {
        if ($values.Contains($name)) {
            $values[$name] = [double]::Parse(
                [string]$values[$name],
                [Globalization.CultureInfo]::InvariantCulture
            )
        }
    }
    return [pscustomobject]$values
}

function Get-StageRows {
    param($AllRows, $Stage)
    return @($AllRows | Where-Object {
        $_.line_number -gt $Stage.start_line -and $_.line_number -le $Stage.end_line
    })
}

function Test-Monotonic {
    param([double[]]$Values, [bool]$Increasing)
    if ($Values.Count -lt 4) {
        return $false
    }
    for ($index = 1; $index -lt $Values.Count; $index++) {
        if ($Increasing -and $Values[$index] + 0.003 -lt $Values[$index - 1]) {
            return $false
        }
        if (-not $Increasing -and $Values[$index] - 0.003 -gt $Values[$index - 1]) {
            return $false
        }
    }
    return $true
}

function Get-Percentile {
    param([double[]]$Values, [double]$Percentile)
    if ($Values.Count -eq 0) {
        return $null
    }
    $sorted = @($Values | Sort-Object)
    $index = [Math]::Min(
        $sorted.Count - 1,
        [Math]::Max(0, [int][Math]::Ceiling($Percentile * $sorted.Count) - 1)
    )
    return [double]$sorted[$index]
}

$repo = Get-ProofRepoRoot
$run = New-ProofRunDirectory -Name 'motion-continuity'
$runDirectory = $run.Directory
$stateDirectory = Join-Path $runDirectory 'state'
$itemRoot = Join-Path $stateDirectory 'items'
$binDirectory = Join-Path $runDirectory 'bin'
$stdoutPath = Join-Path $runDirectory 'stdout.log'
$stderrPath = Join-Path $runDirectory 'stderr.log'
$summaryPath = Join-Path $runDirectory 'summary.json'
$sourceExe = Join-Path $repo 'target\x86_64-pc-windows-msvc\release\BentoDesk.exe'
$proofExe = Join-Path $binDirectory 'BentoDesk.exe'
$commands = New-Object System.Collections.ArrayList
$stages = New-Object System.Collections.ArrayList
$screenshots = New-Object System.Collections.ArrayList
$process = $null
$mainWindow = $null
$failure = $null
$quitPosted = $false
$exitedThroughQuit = $false

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
            -Arguments @('build', '--release', '-p', 'bentodesk-shell', '--bin', 'BentoDesk') `
            -WorkingDirectory $repo `
            -LogDirectory $runDirectory
        [void]$commands.Add($build)
        if (-not $build.passed) {
            throw 'release build failed'
        }
    }
    if (-not (Test-Path -LiteralPath $sourceExe)) {
        throw "release executable not found: $sourceExe"
    }

    $previousSeed = Set-ProofProcessEnvironment -Values @{
        BENTODESK_BENCHMARK_ITEM_ROOT = $itemRoot
    }
    try {
        $seed = Invoke-ProofCommand `
            -Name '02-seed-benchmark-scene' `
            -FilePath 'cargo' `
            -Arguments @(
                'run', '--quiet',
                '-p', 'bentodesk-platform',
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
    if (-not $seed.passed) {
        throw 'benchmark scene seed failed'
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
        -StderrPath $stderrPath `
        -ExtraEnvironment @{ BENTODESK_ANIM_PROOF_LOG = '1' }
    $mainWindow = Wait-ProofWindow -TargetProcessId $process.Id -ClassName 'BentoDeskShell' -TimeoutMs 12000
    if (-not $mainWindow) {
        throw 'BentoDesk main window was not found'
    }
    Start-Sleep -Milliseconds 1000
    # BentoDesk correctly lives below ordinary application windows. This
    # isolated motion-only process is temporarily raised so WindowFromPoint and
    # the real cursor agree; runtime-performance separately verifies production
    # startup remains WS_EX_TOPMOST=false.
    Set-ProofWindowInputForeground -Window $mainWindow

    $logicalWidth = [int][Math]::Floor($mainWindow.client.width * 96.0 / $mainWindow.dpi)
    $logicalHeight = [int][Math]::Floor($mainWindow.client.height * 96.0 / $mainWindow.dpi)
    $leaveX = [Math]::Max(8, $logicalWidth - 24)
    $leaveY = [Math]::Max(8, $logicalHeight - 24)
    Send-ProofMouseMove `
        -Window $mainWindow `
        -ClientX $leaveX `
        -ClientY $leaveY `
        -MoveSystemCursor `
        -SleepMs 500
    Request-ProofPaint -Window $mainWindow
    [void]$screenshots.Add((Save-ProofWindowShot `
        -Window $mainWindow `
        -Path (Join-Path $runDirectory '00-collapsed.png')))

    $start = Get-LogLineCount -Path $stderrPath
    Send-ProofMouseMove -Window $mainWindow -ClientX 106 -ClientY 356 -MoveSystemCursor -SleepMs 430
    Request-ProofPaint -Window $mainWindow
    [void]$screenshots.Add((Save-ProofWindowShot `
        -Window $mainWindow `
        -Path (Join-Path $runDirectory '01-expanded.png')))
    [void]$stages.Add([pscustomobject]@{
        name = 'full-expand'
        start_line = $start
        end_line = Get-LogLineCount -Path $stderrPath
    })

    $start = Get-LogLineCount -Path $stderrPath
    Send-ProofMouseMove -Window $mainWindow -ClientX $leaveX -ClientY $leaveY -MoveSystemCursor -SleepMs 520
    Request-ProofPaint -Window $mainWindow
    [void]$screenshots.Add((Save-ProofWindowShot `
        -Window $mainWindow `
        -Path (Join-Path $runDirectory '02-collapsed-after-full.png')))
    [void]$stages.Add([pscustomobject]@{
        name = 'full-collapse'
        start_line = $start
        end_line = Get-LogLineCount -Path $stderrPath
    })

    $start = Get-LogLineCount -Path $stderrPath
    Send-ProofMouseMove -Window $mainWindow -ClientX 106 -ClientY 356 -MoveSystemCursor -SleepMs 120
    Send-ProofMouseMove -Window $mainWindow -ClientX $leaveX -ClientY $leaveY -MoveSystemCursor -SleepMs 220
    Send-ProofMouseMove -Window $mainWindow -ClientX 106 -ClientY 356 -MoveSystemCursor -SleepMs 90
    Request-ProofPaint -Window $mainWindow -SleepMs 20
    [void]$screenshots.Add((Save-ProofWindowShot `
        -Window $mainWindow `
        -Path (Join-Path $runDirectory '03-reversal-resumed.png')))
    Start-Sleep -Milliseconds 340
    [void]$stages.Add([pscustomobject]@{
        name = 'rapid-reversal'
        start_line = $start
        end_line = Get-LogLineCount -Path $stderrPath
    })

    Send-ProofMouseMove -Window $mainWindow -ClientX $leaveX -ClientY $leaveY -MoveSystemCursor -SleepMs 520
    $start = Get-LogLineCount -Path $stderrPath
    Send-ProofMouseMove -Window $mainWindow -ClientX 106 -ClientY 356 -MoveSystemCursor -SleepMs 120
    Send-ProofMouseMove -Window $mainWindow -ClientX 466 -ClientY 356 -MoveSystemCursor -SleepMs 90
    Request-ProofPaint -Window $mainWindow -SleepMs 20
    [void]$screenshots.Add((Save-ProofWindowShot `
        -Window $mainWindow `
        -Path (Join-Path $runDirectory '04-multi-zone-overlap.png')))
    Start-Sleep -Milliseconds 340
    Request-ProofPaint -Window $mainWindow
    [void]$screenshots.Add((Save-ProofWindowShot `
        -Window $mainWindow `
        -Path (Join-Path $runDirectory '05-zone-five-expanded.png')))
    [void]$stages.Add([pscustomobject]@{
        name = 'rapid-multi-zone-handoff'
        start_line = $start
        end_line = Get-LogLineCount -Path $stderrPath
    })

    Send-ProofMouseMove -Window $mainWindow -ClientX $leaveX -ClientY $leaveY -MoveSystemCursor -SleepMs 520
    $start = Get-LogLineCount -Path $stderrPath
    Send-ProofMouseMove -Window $mainWindow -ClientX 120 -ClientY 84 -MoveSystemCursor -SleepMs 620
    [void]$stages.Add([pscustomobject]@{
        name = 'stack-enter'
        start_line = $start
        end_line = Get-LogLineCount -Path $stderrPath
    })
    Request-ProofPaint -Window $mainWindow
    [void]$screenshots.Add((Save-ProofWindowShot `
        -Window $mainWindow `
        -Path (Join-Path $runDirectory '06-stack-bloom.png')))

    $start = Get-LogLineCount -Path $stderrPath
    Send-ProofMouseMove -Window $mainWindow -ClientX $leaveX -ClientY $leaveY -MoveSystemCursor -SleepMs 320
    [void]$stages.Add([pscustomobject]@{
        name = 'stack-exit'
        start_line = $start
        end_line = Get-LogLineCount -Path $stderrPath
    })

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
    if ($process) {
        $process.WaitForExit()
    }
}

$stderrLines = if (Test-Path -LiteralPath $stderrPath) {
    [System.IO.File]::ReadAllLines($stderrPath)
} else {
    [string[]]@()
}
$stderrLines = [string[]]@($stderrLines)
$animRows = New-Object System.Collections.ArrayList
$morphSegments = New-Object System.Collections.ArrayList
$morphTicks = New-Object System.Collections.ArrayList
for ($index = 0; $index -lt $stderrLines.Count; $index++) {
    if ($stderrLines[$index] -match '^anim_state: ') {
        [void]$animRows.Add((ConvertFrom-AnimStateLine -Line $stderrLines[$index] -LineNumber ($index + 1)))
    } elseif ($stderrLines[$index] -match '^pill_morph_segment: ') {
        [void]$morphSegments.Add((ConvertFrom-AnimStateLine -Line $stderrLines[$index] -LineNumber ($index + 1)))
    } elseif ($stderrLines[$index] -match '^pill_morph_tick: ') {
        [void]$morphTicks.Add((ConvertFrom-AnimStateLine -Line $stderrLines[$index] -LineNumber ($index + 1)))
    }
}

$expandStage = $stages | Where-Object { $_.name -eq 'full-expand' } | Select-Object -First 1
$collapseStage = $stages | Where-Object { $_.name -eq 'full-collapse' } | Select-Object -First 1
$reversalStage = $stages | Where-Object { $_.name -eq 'rapid-reversal' } | Select-Object -First 1
$multiStage = $stages | Where-Object { $_.name -eq 'rapid-multi-zone-handoff' } | Select-Object -First 1
$stackEnterStage = $stages | Where-Object { $_.name -eq 'stack-enter' } | Select-Object -First 1
$stackExitStage = $stages | Where-Object { $_.name -eq 'stack-exit' } | Select-Object -First 1

$expandSegments = if ($expandStage) { Get-StageRows -AllRows $morphSegments -Stage $expandStage } else { @() }
$collapseSegments = if ($collapseStage) { Get-StageRows -AllRows $morphSegments -Stage $collapseStage } else { @() }
$reversalSegments = if ($reversalStage) { Get-StageRows -AllRows $morphSegments -Stage $reversalStage } else { @() }
$multiSegments = if ($multiStage) { Get-StageRows -AllRows $morphSegments -Stage $multiStage } else { @() }
$expandTicks = if ($expandStage) { @(Get-StageRows -AllRows $morphTicks -Stage $expandStage | Where-Object { $_.zone -eq 4 }) } else { @() }
$collapseTicks = if ($collapseStage) { @(Get-StageRows -AllRows $morphTicks -Stage $collapseStage | Where-Object { $_.zone -eq 4 }) } else { @() }
$reversalTicks = if ($reversalStage) { @(Get-StageRows -AllRows $morphTicks -Stage $reversalStage | Where-Object { $_.zone -eq 4 }) } else { @() }
$multiTicks = if ($multiStage) { @(Get-StageRows -AllRows $morphTicks -Stage $multiStage) } else { @() }
$stackEnterRows = if ($stackEnterStage) { Get-StageRows -AllRows $animRows -Stage $stackEnterStage } else { @() }
$stackExitRows = if ($stackExitStage) { Get-StageRows -AllRows $animRows -Stage $stackExitStage } else { @() }

$expandStart = $expandSegments | Where-Object {
    $_.zone -eq 4 -and $_.from -le 0.001 -and $_.to -ge 0.999
} | Select-Object -First 1
$expandTerminal = $expandTicks | Where-Object { $_.value -ge 0.999 } | Select-Object -First 1
$expandDuration = if ($expandStart -and $expandTerminal) {
    [int64]($expandTerminal.now_ms - $expandStart.now_ms)
} else {
    $null
}

$collapseStart = $collapseSegments | Where-Object {
    $_.zone -eq 4 -and $_.from -ge 0.999 -and $_.to -le 0.001
} | Select-Object -First 1
$collapseTerminal = $collapseTicks | Where-Object { $_.value -le 0.001 } | Select-Object -First 1
$collapseDuration = if ($collapseStart -and $collapseTerminal) {
    [int64]($collapseTerminal.now_ms - $collapseStart.now_ms)
} else {
    $null
}

$reversalExpandEvents = @($reversalSegments | Where-Object {
    $_.zone -eq 4 -and $_.to -ge 0.999
})
$resumeEvent = $reversalExpandEvents | Select-Object -Last 1
$collapseBeforeResume = if ($resumeEvent) {
    $reversalTicks |
        Where-Object {
            $_.line_number -lt $resumeEvent.line_number -and
            $_.zone -eq 4
        } |
        Select-Object -Last 1
} else {
    $null
}
$reversalBoundaryDelta = if ($resumeEvent -and $collapseBeforeResume) {
    [Math]::Abs($resumeEvent.from - $collapseBeforeResume.value)
} else {
    $null
}

$multiCollapseA = $multiSegments | Where-Object {
    $_.zone -eq 4 -and $_.to -le 0.001
} | Select-Object -First 1
$multiExpandB = $multiSegments | Where-Object {
    $_.zone -eq 5 -and $_.to -ge 0.999
} | Select-Object -First 1
$multiTerminalB = $multiTicks | Where-Object {
    $_.zone -eq 5 -and $_.value -ge 0.999
} | Select-Object -First 1
$multiOverlapTimes = @(
    $multiTicks |
        Where-Object { $_.active -eq 'true' } |
        Group-Object now_ms |
        Where-Object { @($_.Group | Select-Object -ExpandProperty zone -Unique).Count -ge 2 }
)

$allMorphTicks = @($morphTicks)
$tickTimes = @($allMorphTicks | Select-Object -ExpandProperty now_ms -Unique)
$tickIntervals = New-Object System.Collections.ArrayList
for ($index = 1; $index -lt $tickTimes.Count; $index++) {
    $delta = [int64]($tickTimes[$index] - $tickTimes[$index - 1])
    if ($delta -gt 0 -and $delta -le 100) {
        [void]$tickIntervals.Add([double]$delta)
    }
}
$terminalDuplicates = 0
foreach ($zoneGroup in @($allMorphTicks | Group-Object zone)) {
    $zoneTicks = @($zoneGroup.Group)
    for ($index = 1; $index -lt $zoneTicks.Count; $index++) {
        $previous = $zoneTicks[$index - 1]
        $current = $zoneTicks[$index]
        $segmentBetween = @($morphSegments | Where-Object {
            $_.zone -eq $current.zone -and
            $_.line_number -gt $previous.line_number -and
            $_.line_number -le $current.line_number
        }).Count -gt 0
        if (
            -not $segmentBetween -and
            $current.now_ms -gt $previous.now_ms -and
            (
                ($previous.value -eq 0.0 -and $current.value -eq 0.0) -or
                ($previous.value -eq 1.0 -and $current.value -eq 1.0)
            )
        ) {
            $terminalDuplicates++
        }
    }
}

$stackEnterTicks = @($stackEnterRows | Where-Object {
    $_.phase -eq 'stack_bloom_tick' -and $_.stack_bloom_leaving -eq 'false'
})
$stackExitTicks = @($stackExitRows | Where-Object {
    $_.phase -eq 'stack_bloom_tick' -and $_.stack_bloom_leaving -eq 'true'
})
$stackEnterDuration = if ($stackEnterTicks.Count -ge 2) {
    [int64]($stackEnterTicks[-1].now_ms - $stackEnterTicks[0].now_ms)
} else {
    $null
}
$stackExitDuration = if ($stackExitTicks.Count -ge 2) {
    [int64]($stackExitTicks[-1].now_ms - $stackExitTicks[0].now_ms)
} else {
    $null
}

$assertions = [ordered]@{
    full_expand_segment_240_ms = [bool]($expandStart -and $expandStart.duration_ms -eq 240)
    full_expand_wall_time_220_to_320_ms = [bool]($expandDuration -ge 220 -and $expandDuration -le 320)
    full_expand_monotonic = [bool](Test-Monotonic -Values @(
        $expandTicks | ForEach-Object { $_.value }
    ) -Increasing $true)
    full_expand_exact_endpoint = [bool]($expandTerminal)
    full_collapse_segment_240_ms = [bool]($collapseStart -and $collapseStart.duration_ms -eq 240)
    full_collapse_wall_time_220_to_320_ms = [bool]($collapseDuration -ge 220 -and $collapseDuration -le 320)
    full_collapse_monotonic = [bool](Test-Monotonic -Values @(
        $collapseTicks | ForEach-Object { $_.value }
    ) -Increasing $false)
    full_collapse_exact_endpoint = [bool]($collapseTerminal)
    reversal_continuity_delta_le_008 = [bool]($null -ne $reversalBoundaryDelta -and $reversalBoundaryDelta -le 0.08)
    reversal_segment_uses_remaining_distance = [bool](
        $resumeEvent -and
        $resumeEvent.duration_ms -ge 50 -and
        $resumeEvent.duration_ms -lt 240
    )
    multi_zone_handoff_segments_both_survive = [bool]($multiCollapseA -and $multiExpandB)
    multi_zone_handoff_has_active_overlap = [bool]($multiOverlapTimes.Count -ge 1)
    multi_zone_handoff_reaches_zone_five_endpoint = [bool]($multiTerminalB)
    no_duplicate_terminal_tick = ($terminalDuplicates -eq 0)
    frame_tick_median_le_20_ms = [bool](
        $tickIntervals.Count -ge 20 -and
        (Get-Percentile -Values @($tickIntervals) -Percentile 0.5) -le 20
    )
    frame_tick_p95_le_35_ms = [bool](
        $tickIntervals.Count -ge 20 -and
        (Get-Percentile -Values @($tickIntervals) -Percentile 0.95) -le 35
    )
    stack_enter_le_480_ms = [bool]($null -ne $stackEnterDuration -and $stackEnterDuration -le 480)
    stack_exit_le_140_ms = [bool]($null -ne $stackExitDuration -and $stackExitDuration -le 140)
    screenshots_nonblank = [bool](
        $screenshots.Count -ge 7 -and @($screenshots | Where-Object { -not $_.nonblank }).Count -eq 0
    )
    production_quit_hotkey = [bool]($quitPosted -and $exitedThroughQuit)
}
$allAssertionsPassed = @($assertions.Values | Where-Object { -not $_ }).Count -eq 0
$failedAssertions = @(
    $assertions.GetEnumerator() |
        Where-Object { -not $_.Value } |
        ForEach-Object { $_.Key }
)
$status = if (-not $failure -and $allAssertionsPassed) { 'ok' } else { 'failed' }

$summary = [ordered]@{
    status = $status
    run_id = $run.Id
    generated_utc = (Get-Date).ToUniversalTime().ToString('o')
    repo = $repo
    isolated_state_dir = $stateDirectory
    process_id = if ($process) { $process.Id } else { $null }
    main_window = $mainWindow
    input_boundary = 'isolated proof HWND is temporarily promoted to HWND_TOPMOST so real cursor reconciliation can exercise a desktop-layer app while other applications are open; runtime-performance separately verifies production WS_EX_TOPMOST=false'
    measurements = [ordered]@{
        full_expand_segment_ms = if ($expandStart) { $expandStart.duration_ms } else { $null }
        full_expand_wall_ms = $expandDuration
        full_collapse_segment_ms = if ($collapseStart) { $collapseStart.duration_ms } else { $null }
        full_collapse_wall_ms = $collapseDuration
        reversal_boundary_delta = $reversalBoundaryDelta
        reversal_resume_duration_ms = if ($resumeEvent) { $resumeEvent.duration_ms } else { $null }
        multi_zone_overlap_tick_count = $multiOverlapTimes.Count
        morph_tick_count = $allMorphTicks.Count
        tick_interval_count = $tickIntervals.Count
        tick_median_ms = Get-Percentile -Values @($tickIntervals) -Percentile 0.5
        tick_p95_ms = Get-Percentile -Values @($tickIntervals) -Percentile 0.95
        duplicate_terminal_ticks = $terminalDuplicates
        stack_enter_ms = $stackEnterDuration
        stack_exit_ms = $stackExitDuration
    }
    assertions = $assertions
    failed_assertions = $failedAssertions
    stages = @($stages)
    screenshots = @($screenshots)
    anim_state_log_count = $animRows.Count
    morph_segment_log_count = $morphSegments.Count
    morph_tick_log_count = $morphTicks.Count
    commands = @($commands)
    failure = $failure
}
Write-ProofJson -Value $summary -Path $summaryPath
Write-Host "Motion proof: $summaryPath"

if ($status -ne 'ok') {
    $detail = if ($failure) { $failure } else { "failed assertions: $($failedAssertions -join ', ')" }
    throw "motion continuity proof failed: $detail"
}
