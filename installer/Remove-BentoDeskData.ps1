[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $RoamingRoot,
    [Parameter(Mandatory)] [string] $RoamingParent,
    [Parameter(Mandatory)] [string] $PortableRoot,
    [Parameter(Mandatory)] [string] $PortableParent,
    [Parameter(Mandatory)] [string] $PortableMarker
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-NoReparseAncestors {
    param([Parameter(Mandatory)] [string] $Path)

    $fullPath = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    $volumeRoot = [IO.Path]::GetPathRoot($fullPath)
    if (-not $volumeRoot) {
        throw "Refusing a path without a volume root: $fullPath"
    }
    $current = $volumeRoot
    $relative = $fullPath.Substring($volumeRoot.Length)
    foreach ($segment in $relative.Split(
        [char[]]@([IO.Path]::DirectorySeparatorChar),
        [StringSplitOptions]::RemoveEmptyEntries
    )) {
        $current = Join-Path $current $segment
        if (-not (Test-Path -LiteralPath $current)) {
            break
        }
        $item = Get-Item -LiteralPath $current -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Refusing to delete through a reparse-point ancestor: $current"
        }
    }
}

function Remove-SafeTree {
    param(
        [Parameter(Mandatory)] [string] $Root,
        [Parameter(Mandatory)] [string] $AllowedParent
    )

    $rootPath = [IO.Path]::GetFullPath($Root).TrimEnd('\')
    $parentPath = [IO.Path]::GetFullPath($AllowedParent).TrimEnd('\')
    if (-not [string]::Equals(
        [IO.Path]::GetDirectoryName($rootPath),
        $parentPath,
        [StringComparison]::OrdinalIgnoreCase
    )) {
        throw "Refusing to delete outside the exact allowed parent: $rootPath"
    }
    if (-not (Test-Path -LiteralPath $rootPath)) {
        return
    }
    Assert-NoReparseAncestors -Path $parentPath

    $item = Get-Item -LiteralPath $rootPath -Force
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        if ($item.PSIsContainer) {
            [IO.Directory]::Delete($rootPath, $false)
        } else {
            [IO.File]::Delete($rootPath)
        }
        return
    }
    if ($item.PSIsContainer) {
        foreach ($child in Get-ChildItem -LiteralPath $rootPath -Force) {
            Remove-SafeTree -Root $child.FullName -AllowedParent $rootPath
        }
    }
    Remove-Item -LiteralPath $rootPath -Force
}

function Remove-OwnedPortableState {
    param(
        [Parameter(Mandatory)] [string] $Root,
        [Parameter(Mandatory)] [string] $AllowedParent,
        [Parameter(Mandatory)] [string] $Marker
    )

    $parentPath = [IO.Path]::GetFullPath($AllowedParent).TrimEnd('\')
    $markerPath = [IO.Path]::GetFullPath($Marker).TrimEnd('\')
    if (
        -not [string]::Equals(
            [IO.Path]::GetDirectoryName($markerPath),
            $parentPath,
            [StringComparison]::OrdinalIgnoreCase
        ) -or
        -not [string]::Equals(
            [IO.Path]::GetFileName($markerPath),
            '.bentodesk-portable',
            [StringComparison]::Ordinal
        )
    ) {
        throw "Refusing an unexpected portable marker path: $markerPath"
    }
    Assert-NoReparseAncestors -Path $parentPath
    if (-not (Test-Path -LiteralPath $markerPath -PathType Leaf)) {
        return
    }
    $markerItem = Get-Item -LiteralPath $markerPath -Force
    if (($markerItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        return
    }
    if ([IO.File]::ReadAllText($markerPath) -cne "BentoDesk portable mode`n") {
        return
    }

    Remove-SafeTree -Root $Root -AllowedParent $parentPath
    Remove-Item -LiteralPath $markerPath -Force
}

Remove-SafeTree -Root $RoamingRoot -AllowedParent $RoamingParent
Remove-OwnedPortableState `
    -Root $PortableRoot `
    -AllowedParent $PortableParent `
    -Marker $PortableMarker
