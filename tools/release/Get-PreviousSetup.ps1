[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$')]
    [string] $Repository,

    [Parameter(Mandatory)]
    [ValidatePattern('^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$')]
    [string] $PreviousVersion,

    [Parameter(Mandatory)]
    [string] $OutputPath
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

$output = [IO.Path]::GetFullPath($OutputPath)
$work = Join-Path ([IO.Path]::GetDirectoryName($output)) "previous-$PreviousVersion"
$portableName = "BentoDesk-$PreviousVersion-windows-x64-portable.zip"
$portable = Join-Path $work $portableName
$checksums = Join-Path $work 'SHA256SUMS.txt'
$stage = Join-Path $work 'stage'

if (Test-Path -LiteralPath $work) {
    Remove-Item -LiteralPath $work -Recurse -Force
}
[IO.Directory]::CreateDirectory($work) | Out-Null

gh release download "v$PreviousVersion" `
    --repo $Repository `
    --pattern $portableName `
    --pattern 'SHA256SUMS.txt' `
    --dir $work

$checksumLine = @(
    Get-Content -LiteralPath $checksums |
        Where-Object { $_ -match "^[0-9a-fA-F]{64}\s+\*?$([regex]::Escape($portableName))$" }
)
if ($checksumLine.Count -cne 1) {
    throw "Published checksums contain no unique entry for $portableName"
}
$expectedHash = $checksumLine[0].Substring(0, 64).ToLowerInvariant()
$actualHash = (Get-FileHash -LiteralPath $portable -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualHash -cne $expectedHash) {
    throw "Published portable hash mismatch: $actualHash != $expectedHash"
}

Expand-Archive -LiteralPath $portable -DestinationPath $stage
$previousExe = Join-Path $stage 'BentoDesk.exe'
if (-not (Test-Path -LiteralPath $previousExe -PathType Leaf)) {
    throw "Published portable archive is missing BentoDesk.exe"
}
if ((Get-Item -LiteralPath $previousExe).VersionInfo.ProductVersion -cne $PreviousVersion) {
    throw "Published BentoDesk.exe does not identify as $PreviousVersion"
}

& (Join-Path $PSScriptRoot 'Build-Installer.ps1') `
    -Version $PreviousVersion `
    -AppExe $previousExe `
    -OutputPath $output

if ((Get-Item -LiteralPath $output).VersionInfo.ProductVersion -cne $PreviousVersion) {
    throw "Previous-version Setup fixture does not identify as $PreviousVersion"
}
