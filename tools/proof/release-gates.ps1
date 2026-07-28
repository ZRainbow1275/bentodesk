#requires -version 5.1

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'ProofTools.psm1') -Force

function New-SourceGateResult {
    param(
        [string]$Name,
        [bool]$Passed,
        $Details
    )
    return [pscustomobject]@{
        name = $Name
        passed = $Passed
        details = $Details
    }
}

$repo = Get-ProofRepoRoot
$run = New-ProofRunDirectory -Name 'release-gates'
$runDirectory = $run.Directory
$summaryPath = Join-Path $runDirectory 'summary.json'
$releaseExe = Join-Path $repo 'target\x86_64-pc-windows-msvc\release\BentoDesk.exe'
$commands = New-Object System.Collections.ArrayList
$sourceGates = New-Object System.Collections.ArrayList

$previousCargo = Set-ProofProcessEnvironment -Values @{
    CARGO_BUILD_JOBS = '1'
    CARGO_INCREMENTAL = '0'
}

try {
    $commandSpecs = @(
        @('01-fmt', 'cargo', @('fmt', '--all', '--', '--check')),
        @('02-tests', 'cargo', @('test', '--workspace', '--all-targets')),
        @('03-clippy', 'cargo', @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')),
        @('04-doc', 'cargo', @('doc', '--workspace', '--no-deps')),
        @('05-deny', 'cargo', @('deny', 'check')),
        @('06-audit', 'cargo', @('audit')),
        @('07-release-build', 'cargo', @('build', '--release', '-p', 'bentodesk-shell', '--bin', 'BentoDesk'))
    )
    foreach ($spec in $commandSpecs) {
        $result = Invoke-ProofCommand `
            -Name $spec[0] `
            -FilePath $spec[1] `
            -Arguments $spec[2] `
            -WorkingDirectory $repo `
            -LogDirectory $runDirectory
        [void]$commands.Add($result)
    }
} finally {
    Restore-ProofProcessEnvironment -Values $previousCargo
}

$productionRustFiles = @(
    Get-ChildItem -LiteralPath (Join-Path $repo 'crates') -Recurse -File -Filter '*.rs' |
        Where-Object {
            $_.FullName -notmatch '[\\/](tests|examples)[\\/]' -and
            $_.Name -ne 'tests.rs'
        }
)
$oversized = New-Object System.Collections.ArrayList
foreach ($file in $productionRustFiles) {
    $lineCount = 0
    foreach ($line in [System.IO.File]::ReadLines($file.FullName)) {
        $lineCount++
    }
    if ($lineCount -gt 800) {
        [void]$oversized.Add([pscustomobject]@{
            path = $file.FullName.Substring($repo.Length + 1)
            lines = $lineCount
        })
    }
}
[void]$sourceGates.Add((New-SourceGateResult `
    -Name 'production-rust-modules-at-most-800-lines' `
    -Passed ($oversized.Count -eq 0) `
    -Details @($oversized)))

$rootProofPile = @(
    Get-ChildItem -LiteralPath $repo -File -Filter 'runtime-proof-*' -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty Name
)
[void]$sourceGates.Add((New-SourceGateResult `
    -Name 'no-root-runtime-proof-pile' `
    -Passed ($rootProofPile.Count -eq 0) `
    -Details $rootProofPile))

$lockText = Get-Content -LiteralPath (Join-Path $repo 'Cargo.lock') -Raw
$lockPackages = @(
    [regex]::Matches($lockText, '(?m)^name = "([^"]+)"$') |
        ForEach-Object { $_.Groups[1].Value } |
        Sort-Object -Unique
)
$forbiddenPackages = @(
    'tauri', 'tauri-runtime', 'tauri-runtime-wry', 'wry', 'webview2-com',
    'tokio', 'async-std', 'smol', 'egui', 'eframe', 'iced', 'slint', 'gtk',
    'gtk4', 'qt_core', 'qt_widgets'
)
$foundForbiddenPackages = @($lockPackages | Where-Object { $_ -in $forbiddenPackages })
[void]$sourceGates.Add((New-SourceGateResult `
    -Name 'no-webview-async-runtime-or-third-party-gui-dependencies' `
    -Passed ($foundForbiddenPackages.Count -eq 0) `
    -Details $foundForbiddenPackages))

$productionTextFiles = @(
    $productionRustFiles
    Get-ChildItem -LiteralPath $repo -File -Filter 'Cargo.toml'
    Get-ChildItem -LiteralPath (Join-Path $repo 'crates') -Recurse -File -Filter 'Cargo.toml'
)
$forbiddenSpawnPattern = 'Command::new\s*\(\s*["''](?:powershell(?:\.exe)?|pwsh(?:\.exe)?|cmd(?:\.exe)?|node(?:\.exe)?|npm(?:\.cmd)?|pnpm(?:\.cmd)?|msedge(?:\.exe)?|chrome(?:\.exe)?)["'']'
$forbiddenSpawnRegex = [regex]::new($forbiddenSpawnPattern)
$forbiddenSpawns = New-Object System.Collections.ArrayList
foreach ($file in $productionTextFiles) {
    $lineNumber = 0
    foreach ($line in [System.IO.File]::ReadLines($file.FullName)) {
        $lineNumber++
        if ($forbiddenSpawnRegex.IsMatch($line)) {
            [void]$forbiddenSpawns.Add([pscustomobject]@{
                path = $file.FullName.Substring($repo.Length + 1)
                line = $lineNumber
                text = $line.Trim()
            })
        }
    }
}
[void]$sourceGates.Add((New-SourceGateResult `
    -Name 'no-browser-node-or-shell-process-spawn' `
    -Passed ($forbiddenSpawns.Count -eq 0) `
    -Details @($forbiddenSpawns)))

$debtPattern = '^\s*#\s*!?\[\s*allow\(dead_code\)\s*\]|^\s*(?:todo|unimplemented)!\s*\(|Status:\s*scaffolding|future implementation'
$debtRegex = [regex]::new($debtPattern)
$debtMarkers = New-Object System.Collections.ArrayList
foreach ($file in $productionRustFiles) {
    $lineNumber = 0
    foreach ($line in [System.IO.File]::ReadLines($file.FullName)) {
        $lineNumber++
        if ($debtRegex.IsMatch($line)) {
            [void]$debtMarkers.Add([pscustomobject]@{
                path = $file.FullName.Substring($repo.Length + 1)
                line = $lineNumber
                text = $line.Trim()
            })
        }
    }
}
[void]$sourceGates.Add((New-SourceGateResult `
    -Name 'no-unexplained-dead-code-or-scaffolding-markers' `
    -Passed ($debtMarkers.Count -eq 0) `
    -Details @($debtMarkers)))

$binary = if (Test-Path -LiteralPath $releaseExe) { Get-Item -LiteralPath $releaseExe } else { $null }
$binaryInfo = if ($binary) { [Diagnostics.FileVersionInfo]::GetVersionInfo($releaseExe) } else { $null }
$binaryGate = [bool]($binary -and $binary.Length -le 2621440)
[void]$sourceGates.Add((New-SourceGateResult `
    -Name 'release-executable-at-most-2-5-mib' `
    -Passed $binaryGate `
    -Details ([ordered]@{
        path = $releaseExe
        bytes = if ($binary) { [int64]$binary.Length } else { $null }
        limit_bytes = 2621440
        sha256 = if ($binary) {
            (Get-FileHash -Algorithm SHA256 -LiteralPath $releaseExe).Hash.ToLowerInvariant()
        } else {
            $null
        }
        product_name = if ($binaryInfo) { $binaryInfo.ProductName } else { $null }
        product_version = if ($binaryInfo) { $binaryInfo.ProductVersion } else { $null }
        file_version = if ($binaryInfo) { $binaryInfo.FileVersion } else { $null }
    })))

$commandsPassed = @($commands | Where-Object { -not $_.passed }).Count -eq 0
$sourcesPassed = @($sourceGates | Where-Object { -not $_.passed }).Count -eq 0
$status = if ($commandsPassed -and $sourcesPassed) { 'ok' } else { 'failed' }
$summary = [ordered]@{
    status = $status
    run_id = $run.Id
    generated_utc = (Get-Date).ToUniversalTime().ToString('o')
    repo = $repo
    rust_toolchain = (& rustc --version)
    cargo_version = (& cargo --version)
    cargo_config = [System.IO.File]::ReadAllText((Join-Path $repo '.cargo\config.toml'))
    commands = @($commands)
    source_gates = @($sourceGates)
}
Write-ProofJson -Value $summary -Path $summaryPath
Write-Host "Release gates: $summaryPath"

if ($status -ne 'ok') {
    throw 'one or more release gates failed; inspect summary.json and per-gate logs'
}
