$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$outDir = Join-Path $repoRoot 'runtime-proof-0618-typography-structure-try'
$manifestPath = Join-Path $repoRoot 'Cargo.toml'
$targetTriple = 'x86_64-pc-windows-msvc'

New-Item -ItemType Directory -Force -Path $outDir | Out-Null

function Write-Utf8NoBom {
    param(
        [string]$Path,
        [string]$Text
    )

    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

function Read-JsonFile {
    param([string]$RelativePath)

    $path = Join-Path $repoRoot $RelativePath
    if (-not (Test-Path -LiteralPath $path)) {
        return $null
    }

    $text = [System.IO.File]::ReadAllText($path)
    $text = $text.TrimStart([char]0xFEFF)
    return $text | ConvertFrom-Json
}

function Get-Field {
    param(
        [object]$Object,
        [string]$Path
    )

    $current = $Object
    foreach ($part in ($Path -split '\.')) {
        if ($null -eq $current) {
            return $null
        }

        $property = $current.PSObject.Properties[$part]
        if ($null -eq $property) {
            return $null
        }
        $current = $property.Value
    }

    return $current
}

function Test-JsonEquals {
    param(
        [object]$Object,
        [string]$Path,
        [object]$Expected
    )

    $actual = Get-Field $Object $Path
    return $actual -eq $Expected
}

function Invoke-AppTest {
    param(
        [string]$Id,
        [string]$Filter,
        [string]$Role
    )

    $logPath = Join-Path $outDir "$Id.log"
    $env:CARGO_BUILD_JOBS = '1'
    $env:CARGO_INCREMENTAL = '0'

    $oldErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $output = & cargo test --manifest-path $manifestPath -p bento-nano-app $Filter --target $targetTriple -- --test-threads=1 2>&1
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $oldErrorActionPreference
    Write-Utf8NoBom $logPath (($output | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine)

    [pscustomobject]@{
        id = $Id
        role = $Role
        filter = $Filter
        command = "cargo test --manifest-path $manifestPath -p bento-nano-app $Filter --target $targetTriple -- --test-threads=1"
        exit_code = $exitCode
        passed = ($exitCode -eq 0)
        log = $logPath
    }
}

function Test-Screenshot {
    param(
        [string]$RelativePath,
        [int]$MinBytes
    )

    $path = Join-Path $repoRoot $RelativePath
    if (-not (Test-Path -LiteralPath $path)) {
        return [pscustomobject]@{
            path = $RelativePath
            exists = $false
            length = 0
            min_bytes = $MinBytes
            pass = $false
        }
    }

    $item = Get-Item -LiteralPath $path
    [pscustomobject]@{
        path = $RelativePath
        exists = $true
        length = $item.Length
        min_bytes = $MinBytes
        pass = ($item.Length -ge $MinBytes)
    }
}

$morphSummaryPath = 'runtime-proof-0608-expanded-morph-visual-try\summary.json'
$stackDragSummaryPath = 'runtime-proof-0608-stack-drag-visual-try\summary.json'
$morph = Read-JsonFile $morphSummaryPath
$stackDrag = Read-JsonFile $stackDragSummaryPath

$testResults = @()
$testResults += Invoke-AppTest 'pill-centerline' 'pill_icon_label_badge_share_vertical_centerline' 'collapsed pill icon/title/badge vertical centerline'
$testResults += Invoke-AppTest 'pill-title-linebox' 'pill_label_height_tracks_capsule_title_font_tier' 'collapsed pill title line box follows capsule font tier'
$testResults += Invoke-AppTest 'morph-header-title' 'morph_header_title' 'in-flight morph header title role and slot'
$testResults += Invoke-AppTest 'expanded-item-label' 'item_label' 'expanded item-card label fixed 14px token and trim'
$testResults += Invoke-AppTest 'stack-tray-typography' 'stack_tray_typography_matches_tauri_compact_roles' 'StackTray and focused preview compact typography roles'
$testResults += Invoke-AppTest 'stack-tray-count-badge' 'stack_tray_header_count_clears_action_buttons' 'StackTray header numeric count badge clearance'

$screenshotChecks = @()
$screenshotChecks += Test-Screenshot 'runtime-proof-0608-expanded-morph-visual-try\05-expanded-02-open-mid-090ms.png' 10000
$screenshotChecks += Test-Screenshot 'runtime-proof-0608-expanded-morph-visual-try\06-expanded-03-open-mid-230ms.png' 10000
$screenshotChecks += Test-Screenshot 'runtime-proof-0608-expanded-morph-visual-try\07-expanded-04-open-stable.png' 10000
$screenshotChecks += Test-Screenshot 'runtime-proof-0608-stack-drag-visual-try\03-main-stack-anchor.png' 10000
$screenshotChecks += Test-Screenshot 'runtime-proof-0608-stack-drag-visual-try\09-stack-tray-open.png' 10000

$runtimeSummaries = @(
    [pscustomobject]@{
        path = $morphSummaryPath
        status_ok = ((Test-JsonEquals $morph 'status' 'ok') -and (Test-JsonEquals $morph 'stage' 'completed'))
        main_window_class = Get-Field $morph 'main_window.class'
        main_window_visible = Get-Field $morph 'main_window.visible'
        dpi = Get-Field $morph 'main_window.dpi'
        screenshot_count = @((Get-Field $morph 'screenshots')).Count
    },
    [pscustomobject]@{
        path = $stackDragSummaryPath
        status_ok = ((Test-JsonEquals $stackDrag 'status' 'ok') -and (Test-JsonEquals $stackDrag 'stage' 'completed'))
        main_window_class = Get-Field $stackDrag 'main_window.class'
        main_window_visible = Get-Field $stackDrag 'main_window.visible'
        dpi = Get-Field $stackDrag 'main_window.dpi'
        screenshot_count = @((Get-Field $stackDrag 'screenshots')).Count
    }
)

$roles = @(
    [pscustomobject]@{
        id = 'collapsed_pill_title'
        surface = 'main collapsed pill'
        proof_tests = @('pill-centerline', 'pill-title-linebox')
        runtime_screenshots = @('runtime-proof-0608-stack-drag-visual-try\03-main-stack-anchor.png')
        structured_contract = 'icon/title/badge share vertical centerline; title line box follows CapsuleSize title font tier'
    },
    [pscustomobject]@{
        id = 'morph_header_title'
        surface = 'in-flight capsule-to-panel morph'
        proof_tests = @('morph-header-title')
        runtime_screenshots = @('runtime-proof-0608-expanded-morph-visual-try\05-expanded-02-open-mid-090ms.png', 'runtime-proof-0608-expanded-morph-visual-try\06-expanded-03-open-mid-230ms.png')
        structured_contract = '14px / weight 500 / line-height 1.4 / DWrite tracking 0.3px on the settled header slot'
    },
    [pscustomobject]@{
        id = 'expanded_item_label'
        surface = 'settled expanded item grid'
        proof_tests = @('expanded-item-label')
        runtime_screenshots = @('runtime-proof-0608-expanded-morph-visual-try\07-expanded-04-open-stable.png')
        structured_contract = 'fixed 14px label token with DirectWrite no-wrap trimming instead of width-driven shrink'
    },
    [pscustomobject]@{
        id = 'stack_tray_compact_roles'
        surface = 'StackTray and focused preview'
        proof_tests = @('stack-tray-typography', 'stack-tray-count-badge')
        runtime_screenshots = @('runtime-proof-0608-stack-drag-visual-try\09-stack-tray-open.png')
        structured_contract = 'compact role font sizes plus numeric count badge that clears action buttons'
    }
)

$targetedTestsPassed = @($testResults | Where-Object { -not $_.passed }).Count -eq 0
$runtimeSummariesOk = @($runtimeSummaries | Where-Object { -not $_.status_ok -or $_.main_window_class -ne 'BentoNanoShell' -or $_.main_window_visible -ne $true }).Count -eq 0
$screenshotsPresent = @($screenshotChecks | Where-Object { -not $_.pass }).Count -eq 0
$structuredSlotSummary = $targetedTestsPassed -and $runtimeSummariesOk -and $screenshotsPresent
$status = if ($structuredSlotSummary) { 'ok' } else { 'attention_required' }

$summary = [pscustomobject]@{
    status = $status
    stage = 'completed'
    generated_at_utc = [DateTime]::UtcNow.ToString('o')
    task = '.trellis/tasks/05-29-nano-tauri-parity-plan'
    repo = 'bentodesk-nano'
    proof_dir = $outDir
    proof_kind = 'structured typography role/slot runtime gate'
    goal_complete = $false
    task_complete = $false
    font_alignment = [pscustomobject]@{
        structured_slot_summary = $structuredSlotSummary
        targeted_tests_passed = $targetedTestsPassed
        runtime_surfaces_visible = $runtimeSummariesOk
        runtime_screenshots_present = $screenshotsPresent
        ocr_or_pixel_font_diff_performed = $false
        no_mock_data = $true
        isolated_runtime_summaries_used = $true
        roles = $roles
    }
    runtime_summaries = $runtimeSummaries
    screenshots = $screenshotChecks
    tests = $testResults
}

$summaryPath = Join-Path $outDir 'summary.json'
Write-Utf8NoBom $summaryPath ($summary | ConvertTo-Json -Depth 20)

Write-Host "typography_structure_status=$status"
Write-Host "summary=$summaryPath"
if (-not $structuredSlotSummary) {
    Write-Host "targeted_tests_passed=$targetedTestsPassed"
    Write-Host "runtime_surfaces_visible=$runtimeSummariesOk"
    Write-Host "runtime_screenshots_present=$screenshotsPresent"
}
