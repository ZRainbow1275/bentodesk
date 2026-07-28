#requires -version 5.1
param([switch]$SkipBuild)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'ProofTools.psm1') -Force

function ConvertFrom-ProofLine {
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
    foreach ($name in @('value', 'from', 'to')) {
        if ($values.Contains($name)) {
            $values[$name] = [double]::Parse(
                [string]$values[$name],
                [Globalization.CultureInfo]::InvariantCulture
            )
        }
    }
    return [pscustomobject]$values
}

function Send-ProofClick {
    param(
        [Parameter(Mandatory = $true)]$Window,
        [Parameter(Mandatory = $true)][int]$ClientX,
        [Parameter(Mandatory = $true)][int]$ClientY
    )

    Send-ProofMouseMove `
        -Window $Window `
        -ClientX $ClientX `
        -ClientY $ClientY `
        -MoveSystemCursor `
        -SleepMs 35
    $scale = [Math]::Max(1.0, [double]$Window.dpi / 96.0)
    $deviceX = [int][Math]::Round($ClientX * $scale)
    $deviceY = [int][Math]::Round($ClientY * $scale)
    $lParam = [IntPtr](((($deviceY -band 0xffff) -shl 16) -bor ($deviceX -band 0xffff)))
    foreach ($event in @(
        [pscustomobject]@{ message = [uint32]0x0201; wparam = [uint64]1 },
        [pscustomobject]@{ message = [uint32]0x0202; wparam = [uint64]0 }
    )) {
        $nativeResult = [UIntPtr]::Zero
        $sent = [BentoDeskProofNative]::SendMessageTimeoutW(
            [IntPtr]$Window.hwnd,
            $event.message,
            [UIntPtr]$event.wparam,
            $lParam,
            [BentoDeskProofNative]::SMTO_ABORTIFHUNG,
            2500,
            [ref]$nativeResult
        )
        if ($sent -eq [IntPtr]::Zero) {
            throw "mouse click message timed out at client=($ClientX,$ClientY)"
        }
        Start-Sleep -Milliseconds 35
    }
}

function Get-LogLineCount {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return 0
    }
    return @(Get-Content -LiteralPath $Path).Count
}

$repo = Get-ProofRepoRoot
$run = New-ProofRunDirectory -Name 'click-collapse'
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
$screenshots = New-Object System.Collections.ArrayList
$process = $null
$mainWindow = $null
$failure = $null
$quitPosted = $false
$exitedThroughQuit = $false
$stageStart = 0
$stageEnd = 0
$leaveX = $null
$leaveY = $null

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
        BENTODESK_BENCHMARK_ZONE_DISPLAY_MODE = 'click'
    }
    try {
        $seed = Invoke-ProofCommand `
            -Name '02-seed-click-scene' `
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
        throw 'click scene seed failed'
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
        -SleepMs 420
    Request-ProofPaint -Window $mainWindow
    [void]$screenshots.Add((Save-ProofWindowShot `
        -Window $mainWindow `
        -Path (Join-Path $runDirectory '00-click-collapsed.png')))

    $stageStart = Get-LogLineCount -Path $stderrPath
    Send-ProofClick -Window $mainWindow -ClientX 106 -ClientY 356
    Start-Sleep -Milliseconds 420
    Request-ProofPaint -Window $mainWindow
    [void]$screenshots.Add((Save-ProofWindowShot `
        -Window $mainWindow `
        -Path (Join-Path $runDirectory '01-click-expanded.png')))

    Send-ProofMouseMove `
        -Window $mainWindow `
        -ClientX $leaveX `
        -ClientY $leaveY `
        -MoveSystemCursor `
        -SleepMs 650
    Request-ProofPaint -Window $mainWindow
    [void]$screenshots.Add((Save-ProofWindowShot `
        -Window $mainWindow `
        -Path (Join-Path $runDirectory '02-click-auto-collapsed.png')))
    $stageEnd = Get-LogLineCount -Path $stderrPath

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

$lines = if (Test-Path -LiteralPath $stderrPath) {
    [string[]]@(Get-Content -LiteralPath $stderrPath)
} else {
    [string[]]@()
}
$segments = New-Object System.Collections.ArrayList
$ticks = New-Object System.Collections.ArrayList
$states = New-Object System.Collections.ArrayList
for ($index = 0; $index -lt $lines.Count; $index++) {
    $lineNumber = $index + 1
    if ($lineNumber -le $stageStart -or $lineNumber -gt $stageEnd) {
        continue
    }
    if ($lines[$index] -match '^pill_morph_segment: ') {
        [void]$segments.Add((ConvertFrom-ProofLine -Line $lines[$index] -LineNumber $lineNumber))
    } elseif ($lines[$index] -match '^pill_morph_tick: ') {
        [void]$ticks.Add((ConvertFrom-ProofLine -Line $lines[$index] -LineNumber $lineNumber))
    } elseif ($lines[$index] -match '^anim_state: ') {
        [void]$states.Add((ConvertFrom-ProofLine -Line $lines[$index] -LineNumber $lineNumber))
    }
}

$expand = $segments | Where-Object {
    $_.zone -eq 4 -and $_.from -le 0.001 -and $_.to -ge 0.999
} | Select-Object -First 1
$collapse = $segments | Where-Object {
    $_.zone -eq 4 -and $_.from -ge 0.999 -and $_.to -le 0.001
} | Select-Object -First 1
$expandEndpoint = $ticks | Where-Object {
    $_.zone -eq 4 -and $_.value -ge 0.999
} | Select-Object -First 1
$collapseEndpoint = if ($collapse) {
    $ticks | Where-Object {
        $_.zone -eq 4 -and $_.line_number -gt $collapse.line_number -and $_.value -le 0.001
    } | Select-Object -First 1
} else {
    $null
}
$selectionCleared = $states | Where-Object {
    $_.phase -eq 'hover_collapse_fired' -and $_.selected_zone -eq 'none'
} | Select-Object -First 1
$expandedHash = if ($screenshots.Count -ge 2) {
    (Get-FileHash -Algorithm SHA256 -LiteralPath $screenshots[1].path).Hash
} else {
    $null
}
$collapsedHash = if ($screenshots.Count -ge 3) {
    (Get-FileHash -Algorithm SHA256 -LiteralPath $screenshots[2].path).Hash
} else {
    $null
}

$assertions = [ordered]@{
    real_click_started_full_expand = [bool]($expand -and $expand.duration_ms -eq 240)
    click_expand_reached_endpoint = [bool]$expandEndpoint
    leave_started_full_collapse = [bool]($collapse -and $collapse.duration_ms -eq 240)
    leave_collapse_reached_endpoint = [bool]$collapseEndpoint
    click_selection_cleared_after_leave = [bool]$selectionCleared
    expanded_and_collapsed_frames_differ = [bool](
        $expandedHash -and $collapsedHash -and $expandedHash -ne $collapsedHash
    )
    screenshots_nonblank = [bool](
        $screenshots.Count -eq 3 -and @($screenshots | Where-Object { -not $_.nonblank }).Count -eq 0
    )
    production_quit_hotkey = [bool]($quitPosted -and $exitedThroughQuit)
}
$failedAssertions = @(
    $assertions.GetEnumerator() |
        Where-Object { -not $_.Value } |
        ForEach-Object { $_.Key }
)
$status = if (-not $failure -and $failedAssertions.Count -eq 0) { 'ok' } else { 'failed' }
$summary = [ordered]@{
    status = $status
    run_id = $run.Id
    generated_utc = (Get-Date).ToUniversalTime().ToString('o')
    repo = $repo
    isolated_state_dir = $stateDirectory
    seeded_display_mode = 'click'
    process_id = if ($process) { $process.Id } else { $null }
    main_window = $mainWindow
    input = [ordered]@{
        message_sequence = @('WM_MOUSEMOVE', 'WM_LBUTTONDOWN', 'WM_LBUTTONUP', 'WM_MOUSEMOVE')
        click_client = [ordered]@{ x = 106; y = 356 }
        leave_client = [ordered]@{ x = $leaveX; y = $leaveY }
    }
    measurements = [ordered]@{
        expand_segment_ms = if ($expand) { $expand.duration_ms } else { $null }
        collapse_segment_ms = if ($collapse) { $collapse.duration_ms } else { $null }
        morph_segment_count = $segments.Count
        morph_tick_count = $ticks.Count
    }
    assertions = $assertions
    failed_assertions = $failedAssertions
    screenshots = @($screenshots)
    commands = @($commands)
    failure = $failure
}
Write-ProofJson -Value $summary -Path $summaryPath
Write-Host "Click collapse proof: $summaryPath"

if ($status -ne 'ok') {
    $detail = if ($failure) { $failure } else { "failed assertions: $($failedAssertions -join ', ')" }
    throw "click collapse proof failed: $detail"
}
