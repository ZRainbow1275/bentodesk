[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$')]
    [string] $Version,

    [Parameter(Mandatory)]
    [string] $AppExe,

    [Parameter(Mandatory)]
    [string] $OutputPath,

    [string] $MakeNsisPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$sourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))

function Assert-NoPrivateBuildPath([IO.FileInfo] $File) {
    $bytes = [IO.File]::ReadAllBytes($File.FullName)
    $ascii = [Text.Encoding]::ASCII.GetString($bytes)
    $unicode = [Text.Encoding]::Unicode.GetString($bytes)
    foreach ($privatePath in @($env:USERPROFILE, $sourceRoot)) {
        if (
            $privatePath -and
            ($ascii.IndexOf($privatePath, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or
             $unicode.IndexOf($privatePath, [StringComparison]::OrdinalIgnoreCase) -ge 0)
        ) {
            throw "$($File.Name) contains a private build-machine path: $privatePath"
        }
    }
}

$app = Get-Item -LiteralPath $AppExe
if ($app.VersionInfo.ProductVersion -cne $Version) {
    throw "BentoDesk.exe ProductVersion is '$($app.VersionInfo.ProductVersion)'; expected '$Version'"
}
Assert-NoPrivateBuildPath $app

if (-not $MakeNsisPath) {
    $command = Get-Command makensis.exe -ErrorAction SilentlyContinue
    if ($command) {
        $MakeNsisPath = $command.Source
    } else {
        $candidates = @(
            @(
                (Join-Path ${env:ProgramFiles(x86)} 'NSIS\makensis.exe'),
                (Join-Path $env:ProgramFiles 'NSIS\makensis.exe')
            ) | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) }
        )
        if ($candidates.Count -gt 0) {
            $MakeNsisPath = $candidates[0]
        }
    }
}
if (-not $MakeNsisPath -or -not (Test-Path -LiteralPath $MakeNsisPath -PathType Leaf)) {
    throw 'makensis.exe was not found. Install the pinned NSIS toolchain before building the Setup executable.'
}

$output = [IO.Path]::GetFullPath($OutputPath)
$outputDir = [IO.Path]::GetDirectoryName($output)
[IO.Directory]::CreateDirectory($outputDir) | Out-Null
Remove-Item -LiteralPath $output -Force -ErrorAction SilentlyContinue

$script = Join-Path $sourceRoot 'installer\BentoDesk.nsi'
$arguments = @(
    '/V4',
    '/WX',
    "/DVERSION=$Version",
    "/DAPP_EXE=$($app.FullName)",
    "/DOUT_FILE=$output",
    "/DSOURCE_ROOT=$sourceRoot",
    $script
)
& $MakeNsisPath @arguments
if ($LASTEXITCODE -ne 0) {
    throw "makensis.exe failed with exit code $LASTEXITCODE"
}

$setup = Get-Item -LiteralPath $output
$bytes = [IO.File]::ReadAllBytes($setup.FullName)
if ($bytes.Length -lt 2 -or $bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) {
    throw 'Setup output is not a PE executable'
}
if ($setup.VersionInfo.ProductVersion -cne $Version -or $setup.VersionInfo.FileVersion -cne "$Version.0") {
    throw "Setup metadata is '$($setup.VersionInfo.ProductVersion)' / '$($setup.VersionInfo.FileVersion)'"
}

Assert-NoPrivateBuildPath $setup

[pscustomobject]@{
    Path = $setup.FullName
    Size = $setup.Length
    SHA256 = (Get-FileHash -LiteralPath $setup.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    ProductVersion = $setup.VersionInfo.ProductVersion
}
