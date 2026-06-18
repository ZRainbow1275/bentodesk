$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$workspaceRoot = Split-Path -Parent $repoRoot
$taskDir = Join-Path $workspaceRoot '.trellis\tasks\05-29-nano-tauri-parity-plan'
$receiptDir = Join-Path $taskDir 'receipts'
$outDir = Join-Path $repoRoot 'runtime-proof-0618-five-issue-closure-try'

New-Item -ItemType Directory -Force -Path $outDir | Out-Null

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

function Test-Receipt {
    param([string]$Name)

    $path = Join-Path $receiptDir $Name
    return Test-Path -LiteralPath $path
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

        if ($current -is [System.Collections.IDictionary]) {
            if (-not $current.Contains($part)) {
                return $null
            }
            $current = $current[$part]
            continue
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

function Test-JsonAtLeast {
    param(
        [object]$Object,
        [string]$Path,
        [int]$Minimum
    )

    $actual = Get-Field $Object $Path
    if ($null -eq $actual) {
        return $false
    }

    return [int]$actual -ge $Minimum
}

function New-Check {
    param(
        [string]$Id,
        [string]$Label,
        [string]$Severity,
        [bool]$Pass,
        [string]$Evidence,
        [object]$Observed,
        [string]$Required
    )

    [pscustomobject]@{
        id = $Id
        label = $Label
        severity = $Severity
        pass = $Pass
        evidence = $Evidence
        observed = $Observed
        required = $Required
    }
}

function New-Defect {
    param(
        [string]$Id,
        [string]$Defect,
        [string]$EvidenceStrength,
        [object[]]$Checks,
        [string[]]$Receipts,
        [string[]]$Summaries,
        [string[]]$RemainingGaps
    )

    $blockingFailures = @($Checks | Where-Object { $_.severity -eq 'blocking' -and -not $_.pass })
    if ($blockingFailures.Count -gt 0) {
        $closureStatus = 'not_closed'
    } elseif ($RemainingGaps.Count -gt 0) {
        $closureStatus = 'machine_evidence_with_remaining_review'
    } elseif ($EvidenceStrength -eq 'receipt-backed') {
        $closureStatus = 'receipt_backed_needs_structured_runtime_summary'
    } else {
        $closureStatus = 'machine_closed'
    }

    [pscustomobject]@{
        id = $Id
        user_reported_defect = $Defect
        evidence_strength = $EvidenceStrength
        closure_status = $closureStatus
        blocking_check_count = @($Checks | Where-Object { $_.severity -eq 'blocking' }).Count
        passed_check_count = @($Checks | Where-Object { $_.pass }).Count
        failed_check_count = @($Checks | Where-Object { -not $_.pass }).Count
        receipts = $Receipts
        summaries = $Summaries
        remaining_gaps = $RemainingGaps
        checks = $Checks
    }
}

function Write-Utf8NoBom {
    param(
        [string]$Path,
        [string]$Text
    )

    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

$animationSummaryPath = 'runtime-proof-0618-animation-state-arbitration-try\summary.json'
$mergeSummaryPath = 'runtime-proof-0608-merge-dissolve-scatter-try\summary.json'
$stackDragSummaryPath = 'runtime-proof-0608-stack-drag-visual-try\summary.json'
$morphSummaryPath = 'runtime-proof-0608-expanded-morph-visual-try\summary.json'
$innerFrameSummaryPath = 'runtime-proof-0617-expanded-inner-frame-current-try\summary.json'
$itemDragSummaryPath = 'runtime-proof-0617-item-drag-preview-layering-try\summary.json'
$settingsSummaryPath = 'runtime-proof-0608-settings-appearance-try\summary.json'
$typographySummaryPath = 'runtime-proof-0618-typography-structure-try\summary.json'

$animation = Read-JsonFile $animationSummaryPath
$merge = Read-JsonFile $mergeSummaryPath
$stackDrag = Read-JsonFile $stackDragSummaryPath
$morph = Read-JsonFile $morphSummaryPath
$innerFrame = Read-JsonFile $innerFrameSummaryPath
$itemDrag = Read-JsonFile $itemDragSummaryPath
$settings = Read-JsonFile $settingsSummaryPath
$typography = Read-JsonFile $typographySummaryPath

$defects = @()

$fontReceipts = @(
    '2026-06-08-zone-chrome-typography-runtime-proof.md',
    '2026-06-08-stack-tray-item-typography-runtime-proof.md',
    '2026-06-08-stack-tray-count-badge-runtime-proof.md'
)
$fontChecks = @()
foreach ($receipt in $fontReceipts) {
    $fontChecks += New-Check "font-receipt-$receipt" "Typography receipt exists: $receipt" 'blocking' (Test-Receipt $receipt) "receipts\$receipt" (Test-Receipt $receipt) 'file exists'
}
$fontChecks += New-Check 'font-structure-summary-ok' 'Structured typography proof summary passed' 'blocking' ((Test-JsonEquals $typography 'status' 'ok') -and (Test-JsonEquals $typography 'stage' 'completed')) $typographySummaryPath "$(Get-Field $typography 'status')/$(Get-Field $typography 'stage')" 'status=ok and stage=completed'
$fontChecks += New-Check 'font-structured-slot-summary' 'Typography proof records structured role/slot evidence' 'blocking' (Test-JsonEquals $typography 'font_alignment.structured_slot_summary' $true) $typographySummaryPath (Get-Field $typography 'font_alignment.structured_slot_summary') 'true'
$fontChecks += New-Check 'font-targeted-tests' 'Typography targeted tests passed' 'blocking' (Test-JsonEquals $typography 'font_alignment.targeted_tests_passed' $true) $typographySummaryPath (Get-Field $typography 'font_alignment.targeted_tests_passed') 'true'
$fontChecks += New-Check 'font-runtime-surfaces' 'Typography runtime surfaces were visible in selected-stack summaries' 'blocking' (Test-JsonEquals $typography 'font_alignment.runtime_surfaces_visible' $true) $typographySummaryPath (Get-Field $typography 'font_alignment.runtime_surfaces_visible') 'true'
$fontChecks += New-Check 'font-runtime-screenshots' 'Typography runtime screenshots are present and non-empty' 'blocking' (Test-JsonEquals $typography 'font_alignment.runtime_screenshots_present' $true) $typographySummaryPath (Get-Field $typography 'font_alignment.runtime_screenshots_present') 'true'
$fontChecks += New-Check 'font-dwrite-current-contract' 'DWrite character-spacing contract is documented in spec ledger' 'blocking' (Test-Receipt '2026-06-17-five-issue-visual-continuation-proof.md') 'receipts\2026-06-17-five-issue-visual-continuation-proof.md' (Test-Receipt '2026-06-17-five-issue-visual-continuation-proof.md') 'file exists'
$defects += New-Defect `
    -Id 'defect-1-font-alignment' `
    -Defect 'Font alignment mismatch' `
    -EvidenceStrength 'machine-summary' `
    -Checks $fontChecks `
    -Receipts $fontReceipts `
    -Summaries @($typographySummaryPath, $morphSummaryPath, $stackDragSummaryPath) `
    -RemainingGaps @()

$animationChecks = @()
$animationChecks += New-Check 'anim-summary-ok' '0618 animation arbitration summary passed' 'blocking' ((Test-JsonEquals $animation 'status' 'ok') -and (Test-JsonEquals $animation 'stage' 'completed')) $animationSummaryPath "$(Get-Field $animation 'status')/$(Get-Field $animation 'stage')" 'status=ok and stage=completed'
$animationChecks += New-Check 'anim-no-visual-review' '0618 animation proof did not defer visual review' 'blocking' (Test-JsonEquals $animation 'visual_review_required' $false) $animationSummaryPath (Get-Field $animation 'visual_review_required') 'false'
$animationChecks += New-Check 'anim-zone-idle' 'Zone drag competing animation channels stayed idle' 'blocking' (Test-JsonEquals $animation 'animation_state.state_arbitration_idle' $true) $animationSummaryPath (Get-Field $animation 'animation_state.state_arbitration_idle') 'true'
$animationChecks += New-Check 'anim-item-idle' 'Item drag competing animation channels stayed idle' 'blocking' (Test-JsonEquals $animation 'animation_state.item_state_arbitration_idle' $true) $animationSummaryPath (Get-Field $animation 'animation_state.item_state_arbitration_idle') 'true'
$animationChecks += New-Check 'anim-highlight-zone-suppressed' 'Search/highlight overlay suppressed during zone drag' 'blocking' (Test-JsonEquals $animation 'animation_state.highlight_suppressed_during_zone_drag' $true) $animationSummaryPath (Get-Field $animation 'animation_state.highlight_suppressed_during_zone_drag') 'true'
$animationChecks += New-Check 'anim-highlight-item-suppressed' 'Search/highlight overlay suppressed during item drag' 'blocking' (Test-JsonEquals $animation 'animation_state.highlight_suppressed_during_item_drag' $true) $animationSummaryPath (Get-Field $animation 'animation_state.highlight_suppressed_during_item_drag') 'true'
$animationChecks += New-Check 'anim-state-dumps-rich' 'Animation state dump count is non-trivial' 'warning' (Test-JsonAtLeast $animation 'animation_state.anim_state_log_count' 100) $animationSummaryPath (Get-Field $animation 'animation_state.anim_state_log_count') '>=100'
$animationChecks += New-Check 'anim-morph-summary-ok' 'Expanded morph runtime proof exists for content reveal arbitration' 'blocking' ((Test-JsonEquals $morph 'status' 'ok') -and (Test-JsonEquals $morph 'stage' 'completed')) $morphSummaryPath "$(Get-Field $morph 'status')/$(Get-Field $morph 'stage')" 'status=ok and stage=completed'
$animationChecks += New-Check 'anim-item-preview-ok' 'Item drag preview layering runtime proof passed' 'blocking' ((Test-JsonEquals $itemDrag 'status' 'ok') -and (Test-JsonEquals $itemDrag 'stage' 'completed')) $itemDragSummaryPath "$(Get-Field $itemDrag 'status')/$(Get-Field $itemDrag 'stage')" 'status=ok and stage=completed'
$defects += New-Defect `
    -Id 'defect-2-overlapping-animation-states' `
    -Defect 'Multiple animation states visible at once' `
    -EvidenceStrength 'machine-summary' `
    -Checks $animationChecks `
    -Receipts @('2026-06-08-expanded-morph-grid-runtime-proof.md', '2026-06-08-stack-drag-visual-runtime-proof.md', '2026-06-08-drag-motion-arbitration-runtime-proof.md', '2026-06-17-item-drag-preview-layering-runtime-proof.md', '2026-06-18-animation-state-arbitration-runtime-proof.md') `
    -Summaries @($animationSummaryPath, $morphSummaryPath, $itemDragSummaryPath) `
    -RemainingGaps @()

$innerFrameChecks = @()
$innerFrameChecks += New-Check 'inner-video-receipt' 'Reference-video extraction receipt exists' 'blocking' (Test-Receipt '2026-06-08-expanded-inner-frame-video-proof.md') 'receipts\2026-06-08-expanded-inner-frame-video-proof.md' (Test-Receipt '2026-06-08-expanded-inner-frame-video-proof.md') 'file exists'
$innerFrameChecks += New-Check 'inner-current-receipt' 'Current inner-frame runtime receipt exists' 'blocking' (Test-Receipt '2026-06-17-expanded-inner-frame-current-proof.md') 'receipts\2026-06-17-expanded-inner-frame-current-proof.md' (Test-Receipt '2026-06-17-expanded-inner-frame-current-proof.md') 'file exists'
$innerFrameChecks += New-Check 'inner-summary-ok' '0617 inner-frame scan summary passed' 'blocking' ((Test-JsonEquals $innerFrame 'status' 'ok') -and (Test-JsonEquals $innerFrame 'stage' 'completed')) $innerFrameSummaryPath "$(Get-Field $innerFrame 'status')/$(Get-Field $innerFrame 'stage')" 'status=ok and stage=completed'
$innerFrameChecks += New-Check 'inner-stale-frame-absent' 'Long stale inner-frame edges were not detected' 'blocking' (Test-JsonEquals $innerFrame 'stale_inner_frame_detected' $false) $innerFrameSummaryPath (Get-Field $innerFrame 'stale_inner_frame_detected') 'false'
$innerFrameChecks += New-Check 'inner-scanned-both-frames' 'At least two current frames were scanned' 'blocking' (Test-JsonAtLeast $innerFrame 'scanned_frame_count' 2) $innerFrameSummaryPath (Get-Field $innerFrame 'scanned_frame_count') '>=2'
$innerFrameChecks += New-Check 'inner-process-exited' 'Inner-frame proof exited through production quit path' 'blocking' (Test-JsonEquals $innerFrame 'process_exited_after_quit_hotkey' $true) $innerFrameSummaryPath (Get-Field $innerFrame 'process_exited_after_quit_hotkey') 'true'
$innerFrameChecks += New-Check 'inner-reference-video-linked' 'Inner-frame proof links the user reference video' 'blocking' ([bool](Get-Field $innerFrame 'reference_video')) $innerFrameSummaryPath (Get-Field $innerFrame 'reference_video') 'non-empty reference_video'
$defects += New-Defect `
    -Id 'defect-3-expanded-inner-frame' `
    -Defect 'Expanded panel appears to have an inner frame' `
    -EvidenceStrength 'machine-summary' `
    -Checks $innerFrameChecks `
    -Receipts @('2026-06-08-expanded-panel-accent-edge-runtime-proof.md', '2026-06-08-expanded-inner-frame-video-proof.md', '2026-06-17-expanded-inner-frame-current-proof.md') `
    -Summaries @($morphSummaryPath, $innerFrameSummaryPath) `
    -RemainingGaps @()

$mergeChecks = @()
$mergeChecks += New-Check 'merge-summary-ok' 'Merge/dissolve scatter runtime summary passed' 'blocking' ((Test-JsonEquals $merge 'status' 'ok') -and (Test-JsonEquals $merge 'stage' 'completed')) $mergeSummaryPath "$(Get-Field $merge 'status')/$(Get-Field $merge 'stage')" 'status=ok and stage=completed'
$mergeChecks += New-Check 'merge-members' 'After merge, zone 5 had expected member relationship' 'blocking' (Test-JsonEquals $merge 'stack.after_merge_zone5_members_4' $true) $mergeSummaryPath (Get-Field $merge 'stack.after_merge_zone5_members_4') 'true'
$mergeChecks += New-Check 'merge-dissolve-independent' 'After dissolve, zones 4 and 5 were independent' 'blocking' (Test-JsonEquals $merge 'stack.after_dissolve_zone4_5_independent' $true) $mergeSummaryPath (Get-Field $merge 'stack.after_dissolve_zone4_5_independent') 'true'
$mergeChecks += New-Check 'merge-not-overlapped' 'Released zones were not exactly overlapped' 'blocking' (Test-JsonEquals $merge 'geometry.released_zones_not_overlapped_exactly' $true) $mergeSummaryPath (Get-Field $merge 'geometry.released_zones_not_overlapped_exactly') 'true'
$mergeChecks += New-Check 'merge-within-viewport' 'Released zones stayed inside viewport' 'blocking' (Test-JsonEquals $merge 'geometry.released_zones_within_viewport' $true) $mergeSummaryPath (Get-Field $merge 'geometry.released_zones_within_viewport') 'true'
$mergeChecks += New-Check 'merge-open-tray-log' 'Open StackTray producer was reached' 'blocking' (Test-JsonEquals $merge 'logs.open_stack_tray_5' $true) $mergeSummaryPath (Get-Field $merge 'logs.open_stack_tray_5') 'true'
$mergeChecks += New-Check 'merge-dissolve-log' 'Dissolve Stack producer was reached' 'blocking' (Test-JsonEquals $merge 'logs.dissolve_stack_5' $true) $mergeSummaryPath (Get-Field $merge 'logs.dissolve_stack_5') 'true'
$mergeChecks += New-Check 'merge-quit-hotkey' 'Process exited through production quit hotkey' 'blocking' (Test-JsonEquals $merge 'process_exited_after_quit_hotkey' $true) $mergeSummaryPath (Get-Field $merge 'process_exited_after_quit_hotkey') 'true'
$defects += New-Defect `
    -Id 'defect-4-chaotic-merge-display' `
    -Defect 'Merge-after display is visually chaotic' `
    -EvidenceStrength 'machine-summary' `
    -Checks $mergeChecks `
    -Receipts @('2026-06-08-stack-dissolve-scatter-runtime-proof.md', '2026-06-08-merge-drag-ghost-runtime-proof.md', '2026-06-08-focused-preview-viewport-proof.md') `
    -Summaries @($mergeSummaryPath) `
    -RemainingGaps @()

$dragChecks = @()
$dragChecks += New-Check 'drag-stack-summary-ok' 'Stack drag visual runtime summary passed' 'blocking' ((Test-JsonEquals $stackDrag 'status' 'ok') -and (Test-JsonEquals $stackDrag 'stage' 'completed')) $stackDragSummaryPath "$(Get-Field $stackDrag 'status')/$(Get-Field $stackDrag 'stage')" 'status=ok and stage=completed'
$dragChecks += New-Check 'drag-animation-summary-ok' '0618 drag arbitration runtime summary passed' 'blocking' ((Test-JsonEquals $animation 'status' 'ok') -and (Test-JsonEquals $animation 'stage' 'completed')) $animationSummaryPath "$(Get-Field $animation 'status')/$(Get-Field $animation 'stage')" 'status=ok and stage=completed'
$dragChecks += New-Check 'drag-live-move-count' 'Drag emitted enough live move frames' 'blocking' (Test-JsonAtLeast $animation 'drag.live_move_log_count' 12) $animationSummaryPath (Get-Field $animation 'drag.live_move_log_count') '>=12'
$dragChecks += New-Check 'drag-motion-monotonic' 'Drag motion was monotonic' 'blocking' (Test-JsonEquals $animation 'drag.motion_monotonic' $true) $animationSummaryPath (Get-Field $animation 'drag.motion_monotonic') 'true'
$dragChecks += New-Check 'drag-no-write-midflight' 'zones.bin was not rewritten during drag' 'blocking' (Test-JsonEquals $animation 'drag.no_write_during_drag' $true) $animationSummaryPath (Get-Field $animation 'drag.no_write_during_drag') 'true'
$dragChecks += New-Check 'drag-write-after-release' 'zones.bin was rewritten after release' 'blocking' (Test-JsonEquals $animation 'drag.write_after_release' $true) $animationSummaryPath (Get-Field $animation 'drag.write_after_release') 'true'
$dragChecks += New-Check 'drag-no-repeated-visual-frames' 'Captured drag frames had non-repeated adjacent deltas' 'blocking' (Test-JsonEquals $animation 'visual_motion.no_repeated_visual_frames' $true) $animationSummaryPath (Get-Field $animation 'visual_motion.no_repeated_visual_frames') 'true'
$dragChecks += New-Check 'drag-frame-count' 'Captured drag frame count is sufficient' 'blocking' (Test-JsonAtLeast $animation 'visual_motion.drag_frame_count' 12) $animationSummaryPath (Get-Field $animation 'visual_motion.drag_frame_count') '>=12'
$dragChecks += New-Check 'drag-item-proof-ok' 'Item drag proof stayed release-bound' 'blocking' ((Test-JsonEquals $itemDrag 'item_drag.no_write_during_drag' $true) -and (Test-JsonEquals $itemDrag 'item_drag.write_after_release' $true)) $itemDragSummaryPath "no_write=$(Get-Field $itemDrag 'item_drag.no_write_during_drag'); write_after=$(Get-Field $itemDrag 'item_drag.write_after_release')" 'no_write_during_drag=true and write_after_release=true'
$defects += New-Defect `
    -Id 'defect-5-unsmooth-drag-motion' `
    -Defect 'Drag animation is strange or not smooth' `
    -EvidenceStrength 'machine-summary' `
    -Checks $dragChecks `
    -Receipts @('2026-06-08-stack-drag-visual-runtime-proof.md', '2026-06-08-drag-motion-arbitration-runtime-proof.md', '2026-06-17-item-drag-preview-layering-runtime-proof.md', '2026-06-18-animation-state-arbitration-runtime-proof.md') `
    -Summaries @($stackDragSummaryPath, $animationSummaryPath, $itemDragSummaryPath) `
    -RemainingGaps @()

$settingsChecks = @()
$settingsChecks += New-Check 'settings-summary-ok' 'Settings/Appearance runtime summary passed' 'blocking' ((Test-JsonEquals $settings 'status' 'ok') -and (Test-JsonEquals $settings 'stage' 'completed')) $settingsSummaryPath "$(Get-Field $settings 'status')/$(Get-Field $settings 'stage')" 'status=ok and stage=completed'
$settingsChecks += New-Check 'settings-window-class' 'Settings opened as BentoAuxSets' 'blocking' (Test-JsonEquals $settings 'settings_window.class' 'BentoAuxSets') $settingsSummaryPath (Get-Field $settings 'settings_window.class') 'BentoAuxSets'
$settingsChecks += New-Check 'settings-native-wheel' 'Native mouse wheel path was exercised' 'blocking' (Test-JsonEquals $settings 'assertions.native_wheel_messages_sent' $true) $settingsSummaryPath (Get-Field $settings 'assertions.native_wheel_messages_sent') 'true'
$settingsChecks += New-Check 'settings-vault-written' 'Settings first save wrote vault.bin' 'blocking' (Test-JsonEquals $settings 'assertions.vault_created_after_first_save' $true) $settingsSummaryPath (Get-Field $settings 'assertions.vault_created_after_first_save') 'true'
$settingsChecks += New-Check 'settings-backup-created' 'Settings backup file exists' 'blocking' (Test-JsonEquals $settings 'assertions.backup_file_created' $true) $settingsSummaryPath (Get-Field $settings 'assertions.backup_file_created') 'true'
$settingsChecks += New-Check 'settings-plugin-disabled' 'Plugin registry persisted disabled state after toggle' 'blocking' (Test-JsonEquals $settings 'assertions.plugin_disabled_in_registry' $true) $settingsSummaryPath (Get-Field $settings 'assertions.plugin_disabled_in_registry') 'true'
$settingsChecks += New-Check 'settings-backup-log-boundary' 'Backup creation stderr marker was present' 'warning' (Test-JsonEquals $settings 'logs.backup_created' $true) $settingsSummaryPath (Get-Field $settings 'logs.backup_created') 'true'
$settingsGaps = @()
if (-not (Test-JsonEquals $settings 'logs.backup_created' $true)) {
    $settingsGaps += 'Settings proof has backup_file_created=true but logs.backup_created=false; keep the file assertion as proof and treat the missing stderr marker as a diagnostic gap.'
}
$relatedParityRows = @(
    [pscustomobject]@{
        id = 'related-ws1-ws2-settings-appearance'
        scope = 'WS-1/WS-2 related parity proof, not one of the five visible defects'
        closure_status = if (@($settingsChecks | Where-Object { $_.severity -eq 'blocking' -and -not $_.pass }).Count -eq 0) { 'machine_evidence_with_log_boundary' } else { 'not_closed' }
        summaries = @($settingsSummaryPath)
        receipts = @('2026-06-08-ws1-ws2-settings-runtime-proof.md')
        remaining_gaps = $settingsGaps
        checks = $settingsChecks
    }
)

$allDefectChecks = @($defects | ForEach-Object { $_.checks })
$blockingFailures = @($allDefectChecks | Where-Object { $_.severity -eq 'blocking' -and -not $_.pass })
$defectsWithReview = @($defects | Where-Object { $_.remaining_gaps.Count -gt 0 -or $_.closure_status -match 'needs_structured|remaining_review' })
$status = if ($blockingFailures.Count -eq 0 -and $defectsWithReview.Count -eq 0) {
    'ok'
} elseif ($blockingFailures.Count -eq 0) {
    'partial'
} else {
    'attention_required'
}

$matrixRows = @()
foreach ($defect in $defects) {
    $matrixRows += [pscustomobject]@{
        id = $defect.id
        defect = $defect.user_reported_defect
        closure_status = $defect.closure_status
        evidence_strength = $defect.evidence_strength
        blocking_checks = $defect.blocking_check_count
        passed_checks = $defect.passed_check_count
        failed_checks = $defect.failed_check_count
        remaining_gap_count = $defect.remaining_gaps.Count
        summaries = ($defect.summaries -join ';')
        receipts = ($defect.receipts -join ';')
    }
}

$summary = [pscustomobject]@{
    status = $status
    stage = 'completed'
    generated_at_utc = [DateTime]::UtcNow.ToString('o')
    task = '.trellis/tasks/05-29-nano-tauri-parity-plan'
    repo = 'bentodesk-nano'
    proof_dir = $outDir
    goal_complete = $false
    task_complete = $false
    visual_review_required = ($defectsWithReview.Count -gt 0)
    blocking_failure_count = $blockingFailures.Count
    defects_with_review_or_structured_gap = @($defectsWithReview | ForEach-Object { $_.id })
    defect_count = $defects.Count
    related_parity_rows = $relatedParityRows
    defects = $defects
}

$summaryPath = Join-Path $outDir 'summary.json'
$matrixJsonPath = Join-Path $outDir 'matrix.json'
$matrixCsvPath = Join-Path $outDir 'matrix.csv'

Write-Utf8NoBom $summaryPath ($summary | ConvertTo-Json -Depth 20)
Write-Utf8NoBom $matrixJsonPath ($matrixRows | ConvertTo-Json -Depth 8)
$matrixRows | Export-Csv -Path $matrixCsvPath -NoTypeInformation -Encoding UTF8

Write-Host "five_issue_closure_status=$status"
Write-Host "summary=$summaryPath"
Write-Host "matrix_json=$matrixJsonPath"
Write-Host "matrix_csv=$matrixCsvPath"
if ($blockingFailures.Count -gt 0) {
    Write-Host "blocking_failures=$($blockingFailures.Count)"
    foreach ($failure in $blockingFailures) {
        Write-Host " - $($failure.id): observed=$($failure.observed); required=$($failure.required)"
    }
}
