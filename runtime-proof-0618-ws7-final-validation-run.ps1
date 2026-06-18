$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$proofDir = Join-Path $repoRoot 'runtime-proof-0618-ws7-final-validation-try'
$summaryPath = Join-Path $proofDir 'summary.json'
$targetTriple = 'x86_64-pc-windows-msvc'

function Write-Utf8NoBom {
    param(
        [string]$Path,
        [string]$Text
    )

    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

function Write-JsonFile {
    param(
        [string]$Path,
        [object]$Value
    )

    Write-Utf8NoBom $Path ($Value | ConvertTo-Json -Depth 24)
}

function Read-TextFile {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return ''
    }

    return [System.IO.File]::ReadAllText($Path)
}

function Get-LogTail {
    param(
        [string]$Path,
        [int]$MaxLines = 80
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return @()
    }

    return @(Get-Content -LiteralPath $Path -Tail $MaxLines | ForEach-Object { $_.ToString() })
}

function Invoke-LoggedCommand {
    param(
        [string]$Id,
        [string]$Executable,
        [string[]]$Arguments,
        [string]$WorkingDirectory = $repoRoot,
        [hashtable]$Environment = $null
    )

    $stdoutPath = Join-Path $proofDir "$Id.stdout.log"
    $stderrPath = Join-Path $proofDir "$Id.stderr.log"
    Remove-Item -LiteralPath $stdoutPath,$stderrPath -Force -ErrorAction SilentlyContinue

    $previousEnv = @{}
    if ($Environment) {
        foreach ($key in $Environment.Keys) {
            $previousEnv[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
            [Environment]::SetEnvironmentVariable($key, [string]$Environment[$key], 'Process')
        }
    }

    $previousErrorAction = $global:ErrorActionPreference
    $started = [DateTime]::UtcNow
    Push-Location $WorkingDirectory
    try {
        $global:ErrorActionPreference = 'Continue'
        & $Executable @Arguments > $stdoutPath 2> $stderrPath
        $exitCode = $LASTEXITCODE
    } finally {
        $global:ErrorActionPreference = $previousErrorAction
        Pop-Location
        if ($Environment) {
            foreach ($key in $Environment.Keys) {
                [Environment]::SetEnvironmentVariable($key, $previousEnv[$key], 'Process')
            }
        }
    }

    $finished = [DateTime]::UtcNow
    return [ordered]@{
        id = $Id
        executable = $Executable
        arguments = $Arguments
        working_directory = $WorkingDirectory
        exit_code = [int]$exitCode
        started_utc = $started.ToString('o')
        finished_utc = $finished.ToString('o')
        duration_seconds = [Math]::Round(($finished - $started).TotalSeconds, 2)
        stdout = $stdoutPath
        stderr = $stderrPath
        stdout_tail = @()
        stderr_tail = @()
    }
}

function Parse-CargoTestResults {
    param(
        [string]$StdoutPath,
        [string]$StderrPath
    )

    $text = (Read-TextFile $StdoutPath) + [Environment]::NewLine + (Read-TextFile $StderrPath)
    $matches = [regex]::Matches(
        $text,
        'test result: (?<status>[a-zA-Z_]+)\. (?<passed>\d+) passed; (?<failed>\d+) failed; (?<ignored>\d+) ignored; (?<measured>\d+) measured; (?<filtered>\d+) filtered out'
    )

    $groups = @()
    $passed = 0
    $failed = 0
    $ignored = 0
    $measured = 0
    $filtered = 0
    foreach ($match in $matches) {
        $row = [ordered]@{
            status = $match.Groups['status'].Value
            passed = [int]$match.Groups['passed'].Value
            failed = [int]$match.Groups['failed'].Value
            ignored = [int]$match.Groups['ignored'].Value
            measured = [int]$match.Groups['measured'].Value
            filtered_out = [int]$match.Groups['filtered'].Value
        }
        $groups += [pscustomobject]$row
        $passed += $row.passed
        $failed += $row.failed
        $ignored += $row.ignored
        $measured += $row.measured
        $filtered += $row.filtered_out
    }

    return [ordered]@{
        group_count = [int]$groups.Count
        passed_total = [int]$passed
        failed_total = [int]$failed
        ignored_total = [int]$ignored
        measured_total = [int]$measured
        filtered_out_total = [int]$filtered
        groups = $groups
    }
}

function Parse-ClippyResults {
    param(
        [string]$StdoutPath,
        [string]$StderrPath
    )

    $text = (Read-TextFile $StdoutPath) + [Environment]::NewLine + (Read-TextFile $StderrPath)
    return [ordered]@{
        error_line_count = [int]([regex]::Matches($text, '(?m)^error(\[|:)')).Count
        warning_line_count = [int]([regex]::Matches($text, '(?m)^warning(\[|:)')).Count
    }
}

function Parse-SnapStatus {
    param(
        [string]$StatusStdoutPath,
        [string]$DiffNameStdoutPath,
        [string]$DiffNumstatStdoutPath
    )

    $statusLines = @((Read-TextFile $StatusStdoutPath) -split '\r?\n' | Where-Object { $_.Trim().Length -gt 0 })
    $diffNameLines = @((Read-TextFile $DiffNameStdoutPath) -split '\r?\n' | Where-Object { $_.Trim().Length -gt 0 })
    $numstatLines = @((Read-TextFile $DiffNumstatStdoutPath) -split '\r?\n' | Where-Object { $_.Trim().Length -gt 0 })

    $statusEntries = @()
    foreach ($line in $statusLines) {
        if ($line.Length -lt 4) {
            continue
        }
        $path = $line.Substring(3).Trim()
        $statusEntries += [pscustomobject]@{
            code = $line.Substring(0, 2)
            path = $path
        }
    }

    $diffEntries = @()
    foreach ($line in $diffNameLines) {
        $parts = $line -split "`t"
        if ($parts.Count -ge 2) {
            $diffEntries += [pscustomobject]@{
                status = $parts[0]
                path = $parts[$parts.Count - 1]
            }
        }
    }

    $numstatEntries = @()
    foreach ($line in $numstatLines) {
        $parts = $line -split "`t"
        if ($parts.Count -ge 3) {
            $numstatEntries += [pscustomobject]@{
                added = $parts[0]
                deleted = $parts[1]
                path = $parts[2]
            }
        }
    }

    return [ordered]@{
        status_entries = $statusEntries
        modified_or_untracked_count = [int]$statusEntries.Count
        diff_name_entries = $diffEntries
        diff_numstat_entries = $numstatEntries
    }
}

$resolvedRepo = [IO.Path]::GetFullPath($repoRoot)
$resolvedProof = [IO.Path]::GetFullPath($proofDir)
if (-not $resolvedProof.StartsWith($resolvedRepo, [StringComparison]::OrdinalIgnoreCase)) {
    throw "refusing to write proof outside repo: $resolvedProof"
}

New-Item -ItemType Directory -Force -Path $proofDir | Out-Null
Get-ChildItem -LiteralPath $proofDir -Force -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force

$commands = @()
$stage = 'started'

try {
    $stage = 'tool-versions'
    $commands += [pscustomobject](Invoke-LoggedCommand '00-cargo-version' 'cargo' @('--version'))
    $commands += [pscustomobject](Invoke-LoggedCommand '01-rustc-version' 'rustc' @('--version'))

    $lowMemoryCargoEnv = @{
        CARGO_BUILD_JOBS = '1'
        CARGO_INCREMENTAL = '0'
    }

    $stage = 'cargo-test-workspace'
    $testCommand = [pscustomobject](Invoke-LoggedCommand `
        '10-cargo-test-workspace-all-targets' `
        'cargo' `
        @(
            'test',
            '--manifest-path', (Join-Path $repoRoot 'Cargo.toml'),
            '--workspace',
            '--all-targets',
            '--target', $targetTriple
        ) `
        $repoRoot `
        $lowMemoryCargoEnv)
    $commands += $testCommand
    $testResults = Parse-CargoTestResults $testCommand.stdout $testCommand.stderr

    $stage = 'cargo-clippy-workspace'
    $clippyCommand = [pscustomobject](Invoke-LoggedCommand `
        '20-cargo-clippy-workspace-all-targets' `
        'cargo' `
        @(
            'clippy',
            '--manifest-path', (Join-Path $repoRoot 'Cargo.toml'),
            '--workspace',
            '--all-targets',
            '--target', $targetTriple,
            '--',
            '-D',
            'warnings'
        ) `
        $repoRoot `
        $lowMemoryCargoEnv)
    $commands += $clippyCommand
    $clippyResults = Parse-ClippyResults $clippyCommand.stdout $clippyCommand.stderr

    $stage = 'snap-status'
    $snapStatusCommand = [pscustomobject](Invoke-LoggedCommand `
        '30-git-status-snap' `
        'git' `
        @('-C', $repoRoot, 'status', '--short', '--untracked-files=all', '--', '*.snap.md') `
        $repoRoot)
    $commands += $snapStatusCommand

    $snapNameCommand = [pscustomobject](Invoke-LoggedCommand `
        '31-git-diff-name-status-snap' `
        'git' `
        @('-C', $repoRoot, 'diff', '--name-status', '--', '*.snap.md') `
        $repoRoot)
    $commands += $snapNameCommand

    $snapNumstatCommand = [pscustomobject](Invoke-LoggedCommand `
        '32-git-diff-numstat-snap' `
        'git' `
        @('-C', $repoRoot, 'diff', '--numstat', '--', '*.snap.md') `
        $repoRoot)
    $commands += $snapNumstatCommand

    $snapDiffCommand = [pscustomobject](Invoke-LoggedCommand `
        '33-git-diff-snap' `
        'git' `
        @('-C', $repoRoot, 'diff', '--', '*.snap.md') `
        $repoRoot)
    $commands += $snapDiffCommand

    $snapResults = Parse-SnapStatus $snapStatusCommand.stdout $snapNameCommand.stdout $snapNumstatCommand.stdout
    Write-JsonFile (Join-Path $proofDir 'snap-status.json') $snapResults

    $stage = 'git-status-all'
    $gitStatusCommand = [pscustomobject](Invoke-LoggedCommand `
        '40-git-status-all' `
        'git' `
        @('-C', $repoRoot, 'status', '--short', '--untracked-files=all') `
        $repoRoot)
    $commands += $gitStatusCommand

    $testAccepted = (($testCommand.exit_code -eq 0) -and ($testResults.failed_total -eq 0) -and ($testResults.passed_total -gt 0))
    $clippyAccepted = (($clippyCommand.exit_code -eq 0) -and ($clippyResults.error_line_count -eq 0))
    $snapAccepted = (
        ($snapStatusCommand.exit_code -eq 0) -and
        ($snapNameCommand.exit_code -eq 0) -and
        ($snapNumstatCommand.exit_code -eq 0) -and
        ($snapDiffCommand.exit_code -eq 0) -and
        ($snapResults.modified_or_untracked_count -eq 6)
    )

    $summary = [ordered]@{
        status = if ($testAccepted -and $clippyAccepted -and $snapAccepted) { 'ok' } else { 'failed' }
        stage = 'completed'
        repo_root = $repoRoot
        target_triple = $targetTriple
        no_mock_data = $true
        low_memory_cargo_env = $lowMemoryCargoEnv
        cargo_test = [ordered]@{
            accepted = $testAccepted
            command_id = $testCommand.id
            exit_code = $testCommand.exit_code
            results = $testResults
        }
        cargo_clippy = [ordered]@{
            accepted = $clippyAccepted
            command_id = $clippyCommand.id
            exit_code = $clippyCommand.exit_code
            results = $clippyResults
        }
        snap_reconciliation = [ordered]@{
            accepted = $snapAccepted
            status_command_id = $snapStatusCommand.id
            diff_name_command_id = $snapNameCommand.id
            diff_numstat_command_id = $snapNumstatCommand.id
            diff_command_id = $snapDiffCommand.id
            results = $snapResults
            note = 'accepted means the current tree has exactly the six known modified snap.md baselines and their full diff is captured in the proof directory; it does not imply the git worktree is clean.'
        }
        commands = $commands
        summary_json = $summaryPath
    }
} catch {
    $summary = [ordered]@{
        status = 'failed'
        stage = $stage
        repo_root = $repoRoot
        target_triple = $targetTriple
        no_mock_data = $true
        error = $_.Exception.Message
        commands = $commands
        summary_json = $summaryPath
    }
}

Write-JsonFile $summaryPath $summary
Write-Output "summary: $summaryPath"
Write-Output "status: $($summary.status)"
if ($summary.status -ne 'ok') {
    exit 1
}
