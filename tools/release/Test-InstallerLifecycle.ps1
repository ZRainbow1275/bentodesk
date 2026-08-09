[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $SetupPath,

    [Parameter(Mandatory)]
    [ValidatePattern('^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$')]
    [string] $ExpectedVersion,

    [string] $PreviousSetupPath,

    [switch] $GitHubHostedWindowsRunner
)

if (
    -not $GitHubHostedWindowsRunner.IsPresent -or
    $env:GITHUB_ACTIONS -cne 'true' -or
    $env:RUNNER_ENVIRONMENT -cne 'github-hosted' -or
    $env:RUNNER_OS -cne 'Windows'
) {
    throw 'Installer lifecycle testing requires -GitHubHostedWindowsRunner on a GitHub-hosted Windows runner'
}

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Invoke-NativeProcess {
    param(
        [Parameter(Mandatory)] [string] $FilePath,
        [Parameter(Mandatory)] [string[]] $Arguments
    )

    $process = Start-Process -FilePath $FilePath -ArgumentList $Arguments -Wait -PassThru
    return $process.ExitCode
}

function Assert-PathExists {
    param([Parameter(Mandatory)] [string] $LiteralPath)
    if (-not (Test-Path -LiteralPath $LiteralPath)) {
        throw "Expected path is missing: $LiteralPath"
    }
}

function Assert-PathMissing {
    param([Parameter(Mandatory)] [string] $LiteralPath)
    if (Test-Path -LiteralPath $LiteralPath) {
        throw "Unexpected path remains: $LiteralPath"
    }
}

$setup = Get-Item -LiteralPath $SetupPath
if ($setup.VersionInfo.ProductVersion -cne $ExpectedVersion) {
    throw "Setup ProductVersion is '$($setup.VersionInfo.ProductVersion)'; expected '$ExpectedVersion'"
}
$previousSetup = if ($PreviousSetupPath) { Get-Item -LiteralPath $PreviousSetupPath } else { $null }
if ($previousSetup -and [Version]$previousSetup.VersionInfo.ProductVersion -ge [Version]$ExpectedVersion) {
    throw "Previous Setup version '$($previousSetup.VersionInfo.ProductVersion)' is not older than '$ExpectedVersion'"
}

$sourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$proofModule = Join-Path $sourceRoot 'tools\proof\ProofTools.psm1'
$tempRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$testRoot = Join-Path $tempRoot "bentodesk-installer-lifecycle-$PID"
$rejectedRoot = Join-Path $testRoot 'rejected'
$installRoot = Join-Path $testRoot 'installed'
$alternateInstallRoot = Join-Path $testRoot 'alternate-install-root'
$runtimeState = Join-Path $testRoot 'runtime-state'
$userManagedRoot = Join-Path $testRoot 'user-managed'
$userSentinel = Join-Path $userManagedRoot 'must-survive.txt'
$forgedRoot = Join-Path $testRoot 'forged-install-root'
$forgedPortableState = Join-Path $forgedRoot 'BentoDeskData'
$forgedSentinel = Join-Path $forgedPortableState 'must-survive.txt'
$ancestorTarget = Join-Path $testRoot 'ancestor-target'
$ancestorTargetState = Join-Path $ancestorTarget 'BentoDeskData'
$ancestorLink = Join-Path $testRoot 'ancestor-parent-link'
$ancestorState = Join-Path $ancestorLink 'BentoDeskData'
$ancestorSentinel = Join-Path $ancestorTargetState 'must-survive.txt'
$roamingState = Join-Path $env:APPDATA 'BentoDesk'
$productRegistry = 'HKCU:\Software\BentoDesk'
$uninstallRegistry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\BentoDesk'
$desktopShortcut = Join-Path ([Environment]::GetFolderPath('Desktop')) 'BentoDesk.lnk'
$startMenuDirectory = Join-Path ([Environment]::GetFolderPath('Programs')) 'BentoDesk'
$startMenuShortcut = Join-Path $startMenuDirectory 'BentoDesk.lnk'

foreach ($protectedPath in @(
    $roamingState,
    $productRegistry,
    $uninstallRegistry,
    $desktopShortcut,
    $startMenuDirectory
)) {
    if (Test-Path -LiteralPath $protectedPath) {
        throw "Installer lifecycle test requires a clean disposable user profile; found: $protectedPath"
    }
}

[IO.Directory]::CreateDirectory($testRoot) | Out-Null
[IO.Directory]::CreateDirectory($userManagedRoot) | Out-Null
[IO.File]::WriteAllText($userSentinel, 'user-managed data')

$rejectedExit = Invoke-NativeProcess -FilePath $setup.FullName -Arguments @(
    '/S',
    "/D=$rejectedRoot"
)
if ($rejectedExit -cne 2) {
    throw "Silent install without legal acceptance returned $rejectedExit instead of 2"
}
Assert-PathMissing $rejectedRoot

$acceptedArguments = @(
    '/S',
    '/ACCEPTAGREEMENT',
    '/ACCEPTPRIVACY',
    "/D=$installRoot"
)
$initialSetup = if ($previousSetup) { $previousSetup } else { $setup }
$installExit = Invoke-NativeProcess -FilePath $initialSetup.FullName -Arguments $acceptedArguments
if ($installExit -cne 0) {
    throw "Accepted silent install returned $installExit"
}

$installedExe = Join-Path $installRoot 'BentoDesk.exe'
$uninstaller = Join-Path $installRoot 'Uninstall.exe'
$priorUpgradeTested = $false
$upgradeSentinel = Join-Path $roamingState 'prior-version-upgrade-must-preserve.txt'
if ($previousSetup) {
    if ((Get-Item -LiteralPath $installedExe).VersionInfo.ProductVersion -cne $previousSetup.VersionInfo.ProductVersion) {
        throw 'Previous Setup installed the wrong BentoDesk.exe version'
    }
    [IO.Directory]::CreateDirectory($roamingState) | Out-Null
    [IO.File]::WriteAllText($upgradeSentinel, 'prior-version state')
    $crossRootExit = Invoke-NativeProcess -FilePath $setup.FullName -Arguments @(
        '/S',
        '/ACCEPTAGREEMENT',
        '/ACCEPTPRIVACY',
        "/D=$alternateInstallRoot"
    )
    if ($crossRootExit -cne 5) {
        throw "Cross-root upgrade returned $crossRootExit instead of 5"
    }
    Assert-PathMissing $alternateInstallRoot
    if ((Get-Item -LiteralPath $installedExe).VersionInfo.ProductVersion -cne $previousSetup.VersionInfo.ProductVersion) {
        throw 'Rejected cross-root upgrade changed the existing installation'
    }
    $upgradeExit = Invoke-NativeProcess -FilePath $setup.FullName -Arguments $acceptedArguments
    if ($upgradeExit -cne 0) {
        throw "Prior-version upgrade returned $upgradeExit"
    }
    Assert-PathExists $upgradeSentinel
    $priorUpgradeTested = $true
}
foreach ($requiredPath in @(
    $installedExe,
    $uninstaller,
    (Join-Path $installRoot 'LICENSE'),
    (Join-Path $installRoot 'legal\UserAgreement.en.txt'),
    (Join-Path $installRoot 'legal\UserAgreement.zh-CN.txt'),
    (Join-Path $installRoot 'legal\PrivacyPolicy.en.txt'),
    (Join-Path $installRoot 'legal\PrivacyPolicy.zh-CN.txt'),
    (Join-Path $installRoot 'legal\Remove-BentoDeskData.ps1'),
    $desktopShortcut,
    $startMenuShortcut
)) {
    Assert-PathExists $requiredPath
}
if ((Get-Item -LiteralPath $installedExe).VersionInfo.ProductVersion -cne $ExpectedVersion) {
    throw 'Installed BentoDesk.exe has the wrong ProductVersion'
}
$installedMetadata = [ordered]@{
    agreement = Get-ItemPropertyValue -LiteralPath $productRegistry -Name AcceptedAgreementRevision
    privacy = Get-ItemPropertyValue -LiteralPath $productRegistry -Name AcceptedPrivacyRevision
    publisher = Get-ItemPropertyValue -LiteralPath $uninstallRegistry -Name Publisher
    contact = Get-ItemPropertyValue -LiteralPath $uninstallRegistry -Name Contact
}
$expectedPublisher = -join @([char]0x65B9, [char]0x5BD2)
if (
    $installedMetadata.agreement -cne '2026-08-02' -or
    $installedMetadata.privacy -cne '2026-08-02' -or
    $installedMetadata.publisher -cne $expectedPublisher -or
    $installedMetadata.contact -cne 'hybridrevis@gmail.com'
) {
    throw "Installed legal or author registry metadata is incomplete: $($installedMetadata | ConvertTo-Json -Compress)"
}

$ownershipParent = Join-Path $testRoot 'portable-ownership'
$ownershipState = Join-Path $ownershipParent 'BentoDeskData'
$ownershipMarker = Join-Path $ownershipParent '.bentodesk-portable'
$ownershipSentinel = Join-Path $ownershipState 'must-survive-without-owner.txt'
[IO.Directory]::CreateDirectory($ownershipState) | Out-Null
[IO.File]::WriteAllText($ownershipSentinel, 'unowned portable data')
$purgeHelper = Join-Path $installRoot 'legal\Remove-BentoDeskData.ps1'
$ownershipArguments = @{
    RoamingRoot = Join-Path $testRoot 'missing-roaming\BentoDesk'
    RoamingParent = Join-Path $testRoot 'missing-roaming'
    PortableRoot = $ownershipState
    PortableParent = $ownershipParent
    PortableMarker = $ownershipMarker
}
& $purgeHelper @ownershipArguments
Assert-PathExists $ownershipSentinel
[IO.File]::WriteAllText($ownershipMarker, 'not a BentoDesk marker')
& $purgeHelper @ownershipArguments
Assert-PathExists $ownershipSentinel
[IO.File]::WriteAllText($ownershipMarker, "BentoDesk portable mode`n")
& $purgeHelper @ownershipArguments
Assert-PathMissing $ownershipState
Assert-PathMissing $ownershipMarker

[IO.Directory]::CreateDirectory($ancestorTargetState) | Out-Null
[IO.File]::WriteAllText($ancestorSentinel, 'ancestor target must survive')
New-Item -ItemType Junction -Path $ancestorLink -Target $ancestorTarget | Out-Null
$ancestorBlocked = $false
try {
    & (Join-Path $installRoot 'legal\Remove-BentoDeskData.ps1') `
        -RoamingRoot (Join-Path $testRoot 'missing-roaming\BentoDesk') `
        -RoamingParent (Join-Path $testRoot 'missing-roaming') `
        -PortableRoot $ancestorState `
        -PortableParent $ancestorLink `
        -PortableMarker (Join-Path $ancestorLink '.bentodesk-portable')
} catch {
    $ancestorBlocked = $true
}
if (-not $ancestorBlocked) {
    throw 'Purge through an ancestor junction was not rejected'
}
Assert-PathExists $ancestorSentinel
[IO.Directory]::Delete($ancestorLink, $false)

$repairSentinel = Join-Path $roamingState 'same-version-repair-must-preserve.txt'
[IO.Directory]::CreateDirectory($roamingState) | Out-Null
[IO.File]::WriteAllText($repairSentinel, 'repair state')
$sameVersionRepairExit = Invoke-NativeProcess -FilePath $setup.FullName -Arguments $acceptedArguments
if ($sameVersionRepairExit -cne 0) {
    throw "Same-version repair returned $sameVersionRepairExit"
}
foreach ($repairPath in @($installedExe, $uninstaller, $repairSentinel, $desktopShortcut, $startMenuShortcut)) {
    Assert-PathExists $repairPath
}
$uninstallEntries = @(
    Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' |
        Where-Object { $_.PSChildName -ceq 'BentoDesk' }
)
if ($uninstallEntries.Count -cne 1) {
    throw "Same-version repair left $($uninstallEntries.Count) BentoDesk uninstall entries"
}

Import-Module $proofModule -Force
$process = $null
try {
    $stdout = Join-Path $testRoot 'installed.stdout.log'
    $stderr = Join-Path $testRoot 'installed.stderr.log'
    $process = Start-IsolatedBentoDesk `
        -Executable $installedExe `
        -WorkingDirectory $installRoot `
        -StateDirectory $runtimeState `
        -StdoutPath $stdout `
        -StderrPath $stderr
    $window = Wait-ProofWindow `
        -TargetProcessId $process.Id `
        -ClassName 'BentoDeskShell' `
        -TimeoutMs 12000
    if (-not $window) {
        throw 'Installed BentoDesk did not create its native main window'
    }
    if (-not (Send-ProofQuitHotkey -Window $window)) {
        throw 'Installed BentoDesk rejected the production quit hotkey'
    }
    if (-not (Wait-ProofProcessExit -TargetProcessId $process.Id -TimeoutMs 8000)) {
        throw 'Installed BentoDesk did not exit through the production quit path'
    }
} finally {
    if ($process -and (Get-Process -Id $process.Id -ErrorAction SilentlyContinue)) {
        Stop-ProofProcessExact -TargetProcessId $process.Id -Executable $installedExe | Out-Null
    }
}

[IO.Directory]::CreateDirectory($roamingState) | Out-Null
[IO.File]::WriteAllText((Join-Path $roamingState 'preserve.txt'), 'roaming state')
$portableState = Join-Path $installRoot 'BentoDeskData'
[IO.Directory]::CreateDirectory($portableState) | Out-Null
$realSentinel = Join-Path $portableState 'must-survive-on-forged-root.txt'
[IO.File]::WriteAllText($realSentinel, 'real state')
[IO.File]::WriteAllText((Join-Path $installRoot '.bentodesk-portable'), "BentoDesk portable mode`n")
New-Item -ItemType Junction -Path (Join-Path $portableState 'outside-link') -Target $userManagedRoot |
    Out-Null
[IO.Directory]::CreateDirectory($forgedPortableState) | Out-Null
[IO.File]::WriteAllText($forgedSentinel, 'forged state')

$preserveExit = Invoke-NativeProcess -FilePath $uninstaller -Arguments @('/S')
if ($preserveExit -cne 0) {
    throw "Default uninstall returned $preserveExit"
}
foreach ($preservedPath in @($roamingState, $portableState, $userSentinel)) {
    Assert-PathExists $preservedPath
}
foreach ($removedPath in @($installedExe, $uninstaller, $productRegistry, $uninstallRegistry, $desktopShortcut, $startMenuShortcut)) {
    Assert-PathMissing $removedPath
}

$unownedShortcutBytes = [Text.Encoding]::UTF8.GetBytes('user-owned BentoDesk shortcut sentinel')
[IO.File]::WriteAllBytes($desktopShortcut, $unownedShortcutBytes)
$reinstallExit = Invoke-NativeProcess -FilePath $setup.FullName -Arguments $acceptedArguments
if ($reinstallExit -cne 0) {
    throw "Reinstall before purge returned $reinstallExit"
}
$shortcutAfterReinstall = [IO.File]::ReadAllBytes($desktopShortcut)
if (
    [Convert]::ToBase64String($unownedShortcutBytes) -cne
    [Convert]::ToBase64String($shortcutAfterReinstall)
) {
    throw 'Reinstall overwrote an unowned desktop shortcut'
}
if ((Get-ItemPropertyValue -LiteralPath $productRegistry -Name DesktopShortcutOwned) -cne 0) {
    throw 'Reinstall incorrectly claimed ownership of a pre-existing desktop shortcut'
}
$optOutRepairExit = Invoke-NativeProcess -FilePath $setup.FullName -Arguments $acceptedArguments
if ($optOutRepairExit -cne 0) {
    throw "Opt-out repair returned $optOutRepairExit"
}
if (
    [Convert]::ToBase64String($unownedShortcutBytes) -cne
    [Convert]::ToBase64String([IO.File]::ReadAllBytes($desktopShortcut))
) {
    throw 'Same-version repair overwrote an unowned desktop shortcut'
}
$uninstaller = Join-Path $installRoot 'Uninstall.exe'
$unconfirmedPurgeExit = Invoke-NativeProcess -FilePath $uninstaller -Arguments @(
    '/S',
    '/PURGEUSERDATA',
    '/CONFIRMPURGE=WRONG',
    "_?=$installRoot"
)
if ($unconfirmedPurgeExit -cne 3) {
    throw "Unconfirmed silent purge returned $unconfirmedPurgeExit instead of 3"
}
foreach ($preservedPath in @($installedExe, $uninstaller, $roamingState, $portableState, $userSentinel)) {
    Assert-PathExists $preservedPath
}

$forgedRootExit = Invoke-NativeProcess -FilePath $uninstaller -Arguments @(
    '/S',
    '/PURGEUSERDATA',
    '/CONFIRMPURGE=DELETE-BENTODESK-LOCAL-STATE',
    "_?=$forgedRoot"
)
if ($forgedRootExit -cne 4) {
    throw "Purge with a forged _?= root returned $forgedRootExit instead of 4"
}
foreach ($preservedPath in @(
    $installedExe,
    $uninstaller,
    $roamingState,
    $realSentinel,
    $forgedSentinel,
    $userSentinel
)) {
    Assert-PathExists $preservedPath
}

$purgeExit = Invoke-NativeProcess -FilePath $uninstaller -Arguments @(
    '/S',
    '/PURGEUSERDATA',
    '/CONFIRMPURGE=DELETE-BENTODESK-LOCAL-STATE'
)
if ($purgeExit -cne 0) {
    throw "Explicit purge uninstall returned $purgeExit"
}
foreach ($removedPath in @(
    $roamingState,
    $portableState,
    $installRoot,
    $productRegistry,
    $uninstallRegistry,
    $startMenuDirectory
)) {
    Assert-PathMissing $removedPath
}
Assert-PathExists $userSentinel
Assert-PathExists $forgedSentinel
if (
    [Convert]::ToBase64String($unownedShortcutBytes) -cne
    [Convert]::ToBase64String([IO.File]::ReadAllBytes($desktopShortcut))
) {
    throw 'Uninstall deleted or changed an unowned desktop shortcut'
}
[IO.File]::Delete($desktopShortcut)

[pscustomobject]@{
    Status = 'ok'
    Version = $ExpectedVersion
    SilentLegalGateExitCode = $rejectedExit
    PriorVersionUpgradePreservedState = $priorUpgradeTested
    SameVersionRepairPreservedState = $true
    DefaultUninstallPreservedState = $true
    UnconfirmedSilentPurgeRejected = $true
    ForgedInstallRootPurgeRejected = $true
    AncestorReparsePurgeRejected = $ancestorBlocked
    UnownedDesktopShortcutPreserved = $true
    CrossRootUpgradeRejected = [bool]$previousSetup
    ExplicitPurgeRemovedOnlyBentoDeskState = $true
}
