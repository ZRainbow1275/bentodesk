# Theme B — stress.ps1
#
# Generates `-Count` desktop shortcuts to seed a fully-populated
# desktop for BentoDesk performance benchmarking. Each LNK points at
# `notepad.exe` (ubiquitous and cheap to extract an icon from) but
# names are randomised so the grouping heuristics exercise their
# category paths.
#
# Usage:
#   pwsh -File scripts\bench\stress.ps1 -Count 1000
#   pwsh -File scripts\bench\stress.ps1 -Cleanup
#
# The `-Cleanup` switch removes every LNK this script previously
# generated (identified by the `BENTODESK_BENCH_` prefix).

param(
    [int]$Count = 1000,
    [switch]$Cleanup
)

$ErrorActionPreference = 'Stop'
$desktop = [Environment]::GetFolderPath('Desktop')
$prefix  = 'BENTODESK_BENCH_'

if ($Cleanup) {
    $removed = 0
    Get-ChildItem -LiteralPath $desktop -Filter "$prefix*.lnk" -File | ForEach-Object {
        Remove-Item -LiteralPath $_.FullName -Force
        $removed++
    }
    Write-Host "Removed $removed benchmark shortcuts from $desktop"
    return
}

Write-Host "Generating $Count shortcuts in $desktop ..."

$shell = New-Object -ComObject WScript.Shell
$categories = @('Docs','Code','Media','Work','Personal','Tools','Archive','Notes','Design','Build')
$target = Join-Path $env:WINDIR 'System32\notepad.exe'

$sw = [System.Diagnostics.Stopwatch]::StartNew()
for ($i = 0; $i -lt $Count; $i++) {
    $cat = $categories[$i % $categories.Count]
    $name = ('{0}{1}_{2:D4}.lnk' -f $prefix, $cat, $i)
    $lnkPath = Join-Path $desktop $name
    $sc = $shell.CreateShortcut($lnkPath)
    $sc.TargetPath = $target
    $sc.Description = "BentoDesk benchmark shortcut #$i ($cat)"
    $sc.Save()
}
$sw.Stop()

Write-Host "Created $Count shortcuts in $($sw.Elapsed.TotalSeconds.ToString('0.00'))s"
Write-Host "Run '-Cleanup' to delete them when finished."
