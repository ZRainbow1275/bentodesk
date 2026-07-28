#requires -version 5.1
param([switch]$SkipBuild)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'ProofTools.psm1') -Force

$repo = Get-ProofRepoRoot
$run = New-ProofRunDirectory -Name 'runtime-performance'
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
$samples = New-Object System.Collections.ArrayList
$process = $null
$mainWindow = $null
$launchClock = $null
$screenshot = $null
$failure = $null
$quitPosted = $false
$exitedThroughQuit = $false
$seed = $null

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
    $seedText = Get-Content -LiteralPath $seed.log -Raw
    if ($seedText -notmatch 'seeded 5 zones / 50 items') {
        throw 'benchmark seed did not prove the strict 5-zone / 50-item scene'
    }

    Copy-Item -LiteralPath $sourceExe -Destination $proofExe
    $alreadyRunning = @(Get-Process -Name 'BentoDesk' -ErrorAction SilentlyContinue)
    if ($alreadyRunning.Count -ne 0) {
        throw "BentoDesk is already running (PID: $($alreadyRunning.Id -join ', ')); close it before isolated proof"
    }

    $launchClock = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-IsolatedBentoDesk `
        -Executable $proofExe `
        -WorkingDirectory $runDirectory `
        -StateDirectory $stateDirectory `
        -StdoutPath $stdoutPath `
        -StderrPath $stderrPath
    $mainWindow = Wait-ProofWindow -TargetProcessId $process.Id -ClassName 'BentoDeskShell' -TimeoutMs 12000
    if (-not $mainWindow) {
        throw 'BentoDesk main window was not found'
    }

    foreach ($targetSeconds in @(10, 30, 60)) {
        while ($launchClock.Elapsed.TotalSeconds -lt $targetSeconds) {
            if (-not (Get-Process -Id $process.Id -ErrorAction SilentlyContinue)) {
                throw "BentoDesk exited before t${targetSeconds}"
            }
            Start-Sleep -Milliseconds 200
        }
        $current = Get-Process -Id $process.Id -ErrorAction Stop
        $sameExecutable = @(Get-ExactExecutableProcesses -Executable $proofExe)
        [void]$samples.Add([pscustomobject]@{
            target_seconds = $targetSeconds
            elapsed_ms = [int64]$launchClock.ElapsedMilliseconds
            private_bytes = [int64]$current.PrivateMemorySize64
            private_mib = [Math]::Round($current.PrivateMemorySize64 / 1MB, 2)
            working_set_bytes = [int64]$current.WorkingSet64
            working_set_mib = [Math]::Round($current.WorkingSet64 / 1MB, 2)
            same_executable_process_count = $sameExecutable.Count
        })
        if ($targetSeconds -eq 10) {
            Request-ProofPaint -Window $mainWindow
            $screenshot = Save-ProofWindowShot `
                -Window $mainWindow `
                -Path (Join-Path $runDirectory 'main-window-t10.png')
        }
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
$binary = if (Test-Path -LiteralPath $proofExe) { Get-Item -LiteralPath $proofExe } else { $null }
$binaryHash = if ($binary) { (Get-FileHash -Algorithm SHA256 -LiteralPath $proofExe).Hash.ToLowerInvariant() } else { $null }
$memoryPassed = $samples.Count -eq 3 -and @(
    $samples | Where-Object { $_.target_seconds -in @(30, 60) -and $_.private_bytes -gt 25MB }
).Count -eq 0
$singleProcessPassed = $samples.Count -eq 3 -and @(
    $samples | Where-Object { $_.same_executable_process_count -ne 1 }
).Count -eq 0
$logAssertions = [ordered]@{
    locale = [bool]($stderrText -match 'startup: locale=(zh-CN|en-US)')
    acrylic_feature = [bool]($stderrText -match 'startup: acrylic_feature=(on|off)')
    acrylic_runtime = [bool]($stderrText -match 'startup: acrylic_runtime=(available|unavailable|unknown)')
    production_not_topmost = [bool](
        $stderrText -match 'v10_audit: post_attach_t\+0ms .* WS_EX_TOPMOST=false'
    )
    tray_registered = [bool]($stderrText -match 'tray: NIM_ADD registered')
    hotkeys_registered = [bool]($stderrText -match 'hotkey: registered_global count=\d+')
    quit_hotkey = [bool]($stderrText -match 'hotkey: id=16973 command=QuitApp')
}
$allLogsPassed = @($logAssertions.Values | Where-Object { -not $_ }).Count -eq 0
$status = if (
    -not $failure -and
    $memoryPassed -and
    $singleProcessPassed -and
    $binary -and
    $binary.Length -le 2621440 -and
    $screenshot -and
    $screenshot.nonblank -and
    $allLogsPassed -and
    $quitPosted -and
    $exitedThroughQuit
) { 'ok' } else { 'failed' }
$failedGates = New-Object System.Collections.ArrayList
if (-not $memoryPassed) { [void]$failedGates.Add('memory_gate_25_mib') }
if (-not $singleProcessPassed) { [void]$failedGates.Add('single_process_gate') }
if (-not $binary -or $binary.Length -gt 2621440) { [void]$failedGates.Add('binary_size_gate') }
if (-not $screenshot -or -not $screenshot.nonblank) { [void]$failedGates.Add('screenshot_nonblank') }
foreach ($entry in $logAssertions.GetEnumerator()) {
    if (-not $entry.Value) { [void]$failedGates.Add("runtime_log:$($entry.Key)") }
}
if (-not $quitPosted -or -not $exitedThroughQuit) { [void]$failedGates.Add('production_quit_hotkey') }

$summary = [ordered]@{
    status = $status
    run_id = $run.Id
    generated_utc = (Get-Date).ToUniversalTime().ToString('o')
    repo = $repo
    isolated_state_dir = $stateDirectory
    strict_scene = [ordered]@{
        zones = 5
        items = 50
        seed_log = if ($commands.Count -ge 1) { $seed.log } else { $null }
    }
    executable = [ordered]@{
        path = if ($binary) { $binary.FullName } else { $proofExe }
        bytes = if ($binary) { [int64]$binary.Length } else { $null }
        limit_bytes = 2621440
        sha256 = $binaryHash
    }
    process_id = if ($process) { $process.Id } else { $null }
    main_window = $mainWindow
    memory_samples = @($samples)
    memory_gate_25_mib = $memoryPassed
    single_process_gate = $singleProcessPassed
    screenshot = $screenshot
    runtime_log_assertions = $logAssertions
    failed_gates = @($failedGates)
    quit_hotkey_posted = $quitPosted
    exited_through_quit_hotkey = $exitedThroughQuit
    commands = @($commands)
    failure = $failure
}
Write-ProofJson -Value $summary -Path $summaryPath
Write-Host "Runtime proof: $summaryPath"

if ($status -ne 'ok') {
    $detail = if ($failure) { $failure } else { "failed gates: $(@($failedGates) -join ', ')" }
    throw "runtime performance proof failed: $detail"
}
