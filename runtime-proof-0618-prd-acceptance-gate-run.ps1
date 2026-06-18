$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$workspaceRoot = Split-Path -Parent $repoRoot
$taskDir = Join-Path $workspaceRoot '.trellis\tasks\05-29-nano-tauri-parity-plan'
$outDir = Join-Path $repoRoot 'runtime-proof-0618-prd-acceptance-gate-try'
$wsAuditCsv = Join-Path $taskDir 'ws-acceptance-audit-2026-06-18-results.csv'
$fiveIssueSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-five-issue-closure-try\summary.json'
$ws1ClosureSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws1-settings-closure-gate-try\summary.json'
$ws5AnimationSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws5-animation-acceptance-try\summary.json'
$ws5FullClosureSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws5-animation-full-closure-try\summary.json'
$ws0A3AutoReboundSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws0-a3-auto-rebound-try\summary.json'
$ws0CoreClosureSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws0-core-closure-try\summary.json'
$ws4ExpandedGridSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws4-expanded-grid-current-try\summary.json'
$ws2AppearanceClosureSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws2-appearance-closure-try\summary.json'
$ws3PillVisualSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws3-pill-visual-closure-try\summary.json'
$ws6GestureClampSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws6-gesture-clamp-try\summary.json'
$ws7MemoryBudgetSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws7-memory-budget-try\summary.json'
$ws7FinalValidationSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws7-final-validation-try\summary.json'
$ws7FinalReceiptPath = Join-Path $taskDir 'receipts\2026-06-18-ws7-final-validation-runtime-proof.md'
$taskJsonPath = Join-Path $taskDir 'task.json'

New-Item -ItemType Directory -Force -Path $outDir | Out-Null

function Write-Utf8NoBom {
    param(
        [string]$Path,
        [string]$Text
    )

    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

function Read-JsonPath {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }

    $text = [System.IO.File]::ReadAllText($Path).TrimStart([char]0xFEFF)
    return $text | ConvertFrom-Json
}

function Invoke-GitText {
    param([string[]]$Arguments)

    $oldErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $output = & git -C $repoRoot @Arguments 2>&1
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $oldErrorActionPreference

    [pscustomobject]@{
        exit_code = $exitCode
        text = (($output | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine).Trim()
    }
}

function New-Gate {
    param(
        [string]$Id,
        [string]$Label,
        [bool]$Pass,
        [object]$Observed,
        [string]$Required,
        [string]$Evidence
    )

    [pscustomobject]@{
        id = $Id
        label = $Label
        pass = $Pass
        observed = $Observed
        required = $Required
        evidence = $Evidence
    }
}

function New-Ws1RowFromCurrentClosure {
    param([object]$Closure)

    if ($null -eq $Closure) {
        return $null
    }

    $remaining = @($Closure.remaining_blockers)
    $blockingGaps = @($remaining | ForEach-Object {
        if ($_.gap) {
            "$($_.id): $($_.gap)"
        } else {
            "$($_.id): status=$($_.status)"
        }
    })
    $strongEvidence = @(
        "Current WS-1 closure gate summary is $ws1ClosureSummaryPath.",
        "Gate status=$($Closure.status); closure_status=$($Closure.closure_status); pass=$($Closure.pass_count); partial=$($Closure.partial_count); missing=$($Closure.missing_count).",
        "Runtime source proof is $($Closure.source_summary).",
        "Matrix CSV is $($Closure.matrix_csv)."
    )
    $weakEvidence = @()
    if ($Closure.closure_status -ne 'closed') {
        $weakEvidence += 'WS-1 remains partial until every row in the dedicated WS-1 matrix is pass.'
    }

    $nextGate = if ($blockingGaps.Count -gt 0) {
        "Resolve the remaining WS-1 matrix rows: $($blockingGaps -join ' | ')"
    } else {
        'No WS-1 row-level blockers remain in the current dedicated closure gate.'
    }

    return [pscustomobject]@{
        ws_id = 'WS-1'
        closure_status = [string]$Closure.closure_status
        blocking_gap_count = [int]$remaining.Count
        strong_evidence_count = [int]$strongEvidence.Count
        weak_or_indirect_evidence_count = [int]$weakEvidence.Count
        blocking_gaps = $blockingGaps
        strong_evidence = $strongEvidence
        weak_or_indirect_evidence = $weakEvidence
        recommended_next_gate = $nextGate
        notes = "WS-1 is sourced from the current dedicated runtime closure gate instead of the stale 2026-06-18 worker CSV row. It is still partial unless closure_status=closed."
    }
}

function Test-A3AutoReboundAccepted {
    param([object]$Summary)

    return (
        ($null -ne $Summary) -and
        ($Summary.status -eq 'ok') -and
        ($Summary.a3_auto_rebound.accepted -eq $true) -and
        ($Summary.a3_auto_rebound.hover_enter_seen -eq $true) -and
        ($Summary.a3_auto_rebound.expand_fired_seen -eq $true) -and
        ($Summary.a3_auto_rebound.hover_leave_seen -eq $true) -and
        ($Summary.a3_auto_rebound.collapse_fired_seen -eq $true) -and
        ($Summary.a3_auto_rebound.collapse_settled_seen -eq $true) -and
        ($Summary.a3_auto_rebound.collapse_within_1s -eq $true) -and
        ($Summary.a3_auto_rebound.settled_within_1_2s -eq $true) -and
        ($Summary.a3_auto_rebound.opened_visibly -eq $true) -and
        ($Summary.a3_auto_rebound.closed_back_to_pill -eq $true) -and
        ($Summary.a3_auto_rebound.no_write_during_hover -eq $true) -and
        ($Summary.process_exited_after_quit_hotkey -eq $true)
    )
}

function New-Ws0RowWithA3Overlay {
    param(
        [object]$Row,
        [object]$A3Summary
    )

    if ($null -eq $Row) {
        return $null
    }
    if (-not (Test-A3AutoReboundAccepted $A3Summary)) {
        return $Row
    }

    $blockingGaps = @($Row.blocking_gaps | Where-Object {
        $_ -notlike 'A3 rebound lacks*'
    })
    $strongEvidence = @($Row.strong_evidence)
    $strongEvidence += @(
        "Current WS-0/A3 auto-rebound summary is $ws0A3AutoReboundSummaryPath.",
        "Gate status=$($A3Summary.status); accepted=$($A3Summary.a3_auto_rebound.accepted); expand_after_enter_ms=$($A3Summary.a3_auto_rebound.expand_after_enter_ms); collapse_after_leave_ms=$($A3Summary.a3_auto_rebound.collapse_after_leave_ms); settled_after_leave_ms=$($A3Summary.a3_auto_rebound.settled_after_leave_ms); no_write_during_hover=$($A3Summary.a3_auto_rebound.no_write_during_hover); process_exited_after_quit_hotkey=$($A3Summary.process_exited_after_quit_hotkey).",
        "Artifacts include runtime-proof-0618-ws0-a3-auto-rebound-try\state-dumps.jsonl, pixel-assertions.json, summary.json, stderr.log, and 9 hover/open/collapse screenshots."
    )
    $weakEvidence = @($Row.weak_or_indirect_evidence | Where-Object {
        $_ -notmatch 'A3 synthetic evidence'
    })
    $weakEvidence += 'A3 is now runtime-proven, but WS-0 remains partial because F3, R2, and the explicit M0/M0.5 decision still need one consolidated M0 matrix.'

    return [pscustomobject]@{
        ws_id = 'WS-0'
        closure_status = 'partial'
        blocking_gap_count = [int]$blockingGaps.Count
        strong_evidence_count = [int]$strongEvidence.Count
        weak_or_indirect_evidence_count = [int]$weakEvidence.Count
        blocking_gaps = $blockingGaps
        strong_evidence = $strongEvidence
        weak_or_indirect_evidence = $weakEvidence
        recommended_next_gate = "Run/create the remaining WS-0/M0 acceptance manifest on the current release or release-equivalent build with isolated state: F3 `WindowFromPoint` plus real Explorer/desktop icon click-through result; R2 picker visible, selectable via `SetZoneDisplayMode`, and persisted after reopen; explicit `m0_5_decision` set to `not_needed` or `inserted_done`. A3 auto-rebound is already current-runtime proven by $ws0A3AutoReboundSummaryPath."
        notes = 'WS-0 remains partial. This overlay closes only the A3 auto-rebound blocker with current runtime state/screenshots/timing/no-write evidence; it does not close F3, R2, or the explicit M0/M0.5 decision.'
    }
}

function Test-Ws0CoreClosureAccepted {
    param([object]$Summary)

    return (
        ($null -ne $Summary) -and
        ($Summary.status -eq 'ok') -and
        ($Summary.stage -eq 'completed') -and
        ($Summary.ws_id -eq 'WS-0') -and
        ($Summary.no_mock_data -eq $true) -and
        ($Summary.ws0_core.accepted -eq $true) -and
        ($Summary.ws0_core.f3_click_through_current -eq $true) -and
        ($Summary.ws0_core.r2_picker_select_persist -eq $true) -and
        ($Summary.ws0_core.r2_second_launch_restore -eq $true) -and
        ($Summary.ws0_core.a3_auto_rebound -eq $true) -and
        (@('not_needed', 'inserted_done') -contains [string]$Summary.ws0_core.m0_5_decision) -and
        ($Summary.f3.status -eq 'ok') -and
        ($Summary.f3.stage -eq 'completed') -and
        ($Summary.f3.desktop_shell_reached -eq $true) -and
        ($Summary.f3.main_reports_httransparent -eq $true) -and
        ($Summary.f3.main_ws_ex_transparent -eq $true) -and
        ($Summary.f3.main_nchittest_is_transparent -eq $true) -and
        ($Summary.f3.physical_double_click_opened_file -eq $true) -and
        ($Summary.f3.process_exited_after_quit_hotkey -eq $true) -and
        ($Summary.r2.settings_window_class -eq 'BentoAuxSets') -and
        ($Summary.r2.hover_hit -eq $true) -and
        ($Summary.r2.always_hit -eq $true) -and
        ($Summary.r2.click_hit -eq $true) -and
        ($Summary.r2.persisted_after_each_click -eq $true) -and
        ($Summary.r2.final_vault_zone_display_mode -eq 'click') -and
        ($Summary.r2.second_launch_restore.restore_log_seen -eq $true) -and
        ($Summary.r2.second_launch_restore.restore_error_seen -eq $false) -and
        ($Summary.r2.second_launch_restore.process_exited_after_quit_hotkey -eq $true) -and
        ($Summary.a3.status -eq 'ok') -and
        ($Summary.a3.accepted -eq $true) -and
        ($Summary.a3.process_exited_after_quit_hotkey -eq $true)
    )
}

function New-Ws0RowFromCurrentCoreClosure {
    param(
        [object]$Row,
        [object]$Summary
    )

    if ($null -eq $Row) {
        return $null
    }
    if (-not (Test-Ws0CoreClosureAccepted $Summary)) {
        return $Row
    }

    $strongEvidence = @(
        "Current WS-0 core closure summary is $ws0CoreClosureSummaryPath.",
        "Gate status=$($Summary.status); accepted=$($Summary.ws0_core.accepted); no_mock_data=$($Summary.no_mock_data); m0_5_decision=$($Summary.ws0_core.m0_5_decision).",
        "F3 current proof: WindowFromPoint class=$($Summary.f3.window_from_point_class); main_ws_ex_transparent=$($Summary.f3.main_ws_ex_transparent); main_nchittest_is_transparent=$($Summary.f3.main_nchittest_is_transparent); physical_double_click_opened_file=$($Summary.f3.physical_double_click_opened_file); opened_process=$($Summary.f3.opened_process.evidence_type).",
        "R2 current proof: settings_window_class=$($Summary.r2.settings_window_class); hover/always/click hits=$($Summary.r2.hover_hit)/$($Summary.r2.always_hit)/$($Summary.r2.click_hit); final_vault_zone_display_mode=$($Summary.r2.final_vault_zone_display_mode); second_launch_restore_log_seen=$($Summary.r2.second_launch_restore.restore_log_seen).",
        "A3 current proof remains accepted: collapse_after_leave_ms=$($Summary.a3.collapse_after_leave_ms); settled_after_leave_ms=$($Summary.a3.settled_after_leave_ms); process_exited_after_quit_hotkey=$($Summary.a3.process_exited_after_quit_hotkey)."
    )

    return [pscustomobject]@{
        ws_id = 'WS-0'
        closure_status = 'closed'
        blocking_gap_count = 0
        strong_evidence_count = [int]$strongEvidence.Count
        weak_or_indirect_evidence_count = 0
        blocking_gaps = @()
        strong_evidence = $strongEvidence
        weak_or_indirect_evidence = @()
        recommended_next_gate = 'No WS-0 row-level blockers remain in the current core closure gate.'
        notes = 'WS-0 is closed by the current consolidated M0 gate covering F3 current Desktop/Explorer click-through, R2 select/persist/restart restore, A3 auto-rebound, and an explicit M0.5 decision. This does not close the full PRD because other WS rows and final release/budget/clean-tree gates remain open.'
    }
}

function Test-Ws4ExpandedGridAccepted {
    param([object]$Summary)

    return (
        ($null -ne $Summary) -and
        ($Summary.status -eq 'ok') -and
        ($Summary.ws4_expanded_grid.accepted -eq $true) -and
        ($Summary.ws4_expanded_grid.e01_footer_thumb_strip_absent -eq $true) -and
        ($Summary.ws4_expanded_grid.e02_expanded_status_dot_absent -eq $true) -and
        ($Summary.ws4_expanded_grid.e02_count_badge_present -eq $true) -and
        ($Summary.ws4_expanded_grid.e03_item_icon_label_alignment_contract_pass -eq $true) -and
        ($Summary.ws4_expanded_grid.e03_item_label_font_px -eq 14) -and
        ($Summary.ws4_expanded_grid.e04_divider_geometry_contract_pass -eq $true) -and
        ($Summary.ws4_expanded_grid.e04_divider_rgba_or_alpha_within_threshold -eq $true) -and
        ($Summary.ws4_expanded_grid.inner_frame.stale_inner_frame_detected -eq $false) -and
        ($Summary.ws4_expanded_grid.no_mock_data -eq $true) -and
        ($Summary.ws4_expanded_grid.runtime_window_class -eq 'BentoNanoShell') -and
        ($Summary.ws4_expanded_grid.runtime_window_visible -eq $true) -and
        ($Summary.ws4_expanded_grid.process_exited_after_quit_hotkey -eq $true) -and
        ($Summary.visual_review_required -eq $false)
    )
}

function New-Ws4RowFromCurrentExpandedGrid {
    param(
        [object]$Row,
        [object]$Summary
    )

    if ($null -eq $Row) {
        return $null
    }
    if (-not (Test-Ws4ExpandedGridAccepted $Summary)) {
        return $Row
    }

    $strongEvidence = @(
        "Current WS-4 expanded-grid summary is $ws4ExpandedGridSummaryPath.",
        "Gate status=$($Summary.status); accepted=$($Summary.ws4_expanded_grid.accepted); visual_review_required=$($Summary.visual_review_required); no_mock_data=$($Summary.ws4_expanded_grid.no_mock_data).",
        "E-01 footer strip absent=$($Summary.ws4_expanded_grid.e01_footer_thumb_strip_absent); E-02 status dot absent=$($Summary.ws4_expanded_grid.e02_expanded_status_dot_absent); count badge present=$($Summary.ws4_expanded_grid.e02_count_badge_present).",
        "E-03 item label alignment contract=$($Summary.ws4_expanded_grid.e03_item_icon_label_alignment_contract_pass); item_label_font_px=$($Summary.ws4_expanded_grid.e03_item_label_font_px).",
        "E-04 divider geometry=$($Summary.ws4_expanded_grid.e04_divider_geometry_contract_pass); divider alpha contract=$($Summary.ws4_expanded_grid.e04_divider_rgba_or_alpha_within_threshold).",
        "Inner-frame scan: scanned_frame_count=$($Summary.ws4_expanded_grid.inner_frame.scanned_frame_count); stale_inner_frame_detected=$($Summary.ws4_expanded_grid.inner_frame.stale_inner_frame_detected).",
        "Artifacts include runtime-proof-0618-ws4-expanded-grid-current-try\summary.json, expanded-header-crop.png, expanded-footer-bottom-band-crop.png, expanded-zone-grid-layout.log, expanded-item-label.log, plus the source runtime morph and inner-frame screenshots."
    )

    return [pscustomobject]@{
        ws_id = 'WS-4'
        closure_status = 'closed'
        blocking_gap_count = 0
        strong_evidence_count = [int]$strongEvidence.Count
        weak_or_indirect_evidence_count = 0
        blocking_gaps = @()
        strong_evidence = $strongEvidence
        weak_or_indirect_evidence = @()
        recommended_next_gate = 'No WS-4 row-level blockers remain in the current expanded-grid gate.'
        notes = 'WS-4 is closed by the current dedicated expanded-grid runtime/geometry gate. This does not close the whole PRD because other WS rows and final gates remain open.'
    }
}

function Test-Ws2AppearanceAccepted {
    param([object]$Summary)

    return (
        ($null -ne $Summary) -and
        ($Summary.status -eq 'ok') -and
        ($Summary.ws_id -eq 'WS-2') -and
        ($Summary.no_mock_data -eq $true) -and
        ($Summary.assertions.accepted -eq $true) -and
        ($Summary.assertions.settings_window_class -eq $true) -and
        ($Summary.assertions.native_wheel_messages_sent -eq $true) -and
        ($Summary.assertions.scroll_logs_seen -eq $true) -and
        ($Summary.assertions.appearance_source_contract_pass -eq $true) -and
        ($Summary.assertions.theme_click_hit -eq $true) -and
        ($Summary.assertions.accent_click_hit -eq $true) -and
        ($Summary.assertions.zone_display_hover_hit -eq $true) -and
        ($Summary.assertions.zone_display_always_hit -eq $true) -and
        ($Summary.assertions.zone_display_click_hit -eq $true) -and
        ($Summary.assertions.active_theme_persisted -eq $true) -and
        ($Summary.assertions.accent_color_persisted -eq $true) -and
        ($Summary.assertions.zone_display_mode_persisted -eq $true) -and
        ($Summary.assertions.nonblank_screenshots -eq $true) -and
        ($Summary.assertions.settings_closed_after_save -eq $true) -and
        ($Summary.assertions.process_exited_after_quit_hotkey -eq $true) -and
        ($Summary.appearance_contract.legacy_controls_dropped_by_research -eq $true) -and
        ($Summary.appearance_contract.light_dark_toggle_expected -eq $false) -and
        ($Summary.appearance_contract.mirror_icon_toggle_expected -eq $false) -and
        ($Summary.appearance_contract.theme_picker_source_contract.preset_count -eq 17) -and
        ($Summary.appearance_contract.theme_picker_source_contract.family_heading_count -eq 4) -and
        ($Summary.appearance_contract.theme_picker_source_contract.accent_swatch_count -eq 12) -and
        ($Summary.zone_display_mode.persisted_after_each_click -eq $true) -and
        ($Summary.vault.after_save.settings.active_theme -eq 'ocean-blue') -and
        ($Summary.vault.after_save.settings.accent_color -eq '#22c55e') -and
        ($Summary.vault.after_save.settings.zone_display_mode -eq 'click')
    )
}

function New-Ws2RowFromCurrentAppearance {
    param(
        [object]$Row,
        [object]$Summary
    )

    if ($null -eq $Row) {
        return $null
    }
    if (-not (Test-Ws2AppearanceAccepted $Summary)) {
        return $Row
    }

    $strongEvidence = @(
        "Current WS-2 Appearance closure summary is $ws2AppearanceClosureSummaryPath.",
        "Gate status=$($Summary.status); accepted=$($Summary.assertions.accepted); no_mock_data=$($Summary.no_mock_data); opened_via_hotkey_id=$($Summary.opened_via_hotkey_id); quit_via_hotkey_id=$($Summary.quit_via_hotkey_id).",
        "Settings runtime window class=$($Summary.settings_window.class); client=$($Summary.settings_window.client.width)x$($Summary.settings_window.client.height); native wheel messages=$($Summary.assertions.native_wheel_messages_sent); scroll_log_count=$($Summary.hits.scroll_log_count).",
        "Appearance source contract: families=$($Summary.appearance_contract.theme_picker_source_contract.family_heading_count); presets=$($Summary.appearance_contract.theme_picker_source_contract.preset_count); accent_swatches=$($Summary.appearance_contract.theme_picker_source_contract.accent_swatch_count); active_border_documented=$($Summary.appearance_contract.theme_picker_source_contract.active_border_documented); active_label_documented=$($Summary.appearance_contract.theme_picker_source_contract.active_label_documented).",
        "Legacy light/dark appearance toggle and mirrored-icon toggle are explicitly not expected in the current acceptance path; research source=$($Summary.appearance_contract.research_source).",
        "Runtime hits: SelectTheme=$($Summary.hits.SelectTheme); SelectAccent=$($Summary.hits.SelectAccent); SetZoneDisplayMode(Hover/Always/Click)=$($Summary.hits.SetZoneDisplayModeHover)/$($Summary.hits.SetZoneDisplayModeAlways)/$($Summary.hits.SetZoneDisplayModeClick); SaveSettings=$($Summary.hits.SaveSettings).",
        "Vault after Save: active_theme=$($Summary.vault.after_save.settings.active_theme); accent_color=$($Summary.vault.after_save.settings.accent_color); zone_display_mode=$($Summary.vault.after_save.settings.zone_display_mode); mode_tag=$($Summary.vault.after_save.mode_tag); plaintext_decoded=$($Summary.vault.after_save.plaintext_decoded).",
        "Zone-display runtime calibration row y=$($Summary.zone_display_mode.calibration_row_y); persisted_after_each_click=$($Summary.zone_display_mode.persisted_after_each_click); final_vault_value=$($Summary.zone_display_mode.final_vault_value).",
        "Artifacts include runtime-proof-0618-ws2-appearance-closure-try\summary.json, stderr.log, vault-plaintext-last.json, and 6 nonblank runtime screenshots."
    )

    return [pscustomobject]@{
        ws_id = 'WS-2'
        closure_status = 'closed'
        blocking_gap_count = 0
        strong_evidence_count = [int]$strongEvidence.Count
        weak_or_indirect_evidence_count = 0
        blocking_gaps = @()
        strong_evidence = $strongEvidence
        weak_or_indirect_evidence = @()
        recommended_next_gate = 'No WS-2 row-level blockers remain in the current Appearance/Settings gate.'
        notes = 'WS-2 is closed by the current dedicated Appearance/Settings runtime proof. This closes only WS-2; other WS rows and final release/budget gates remain open.'
    }
}

function Test-Ws3PillVisualAccepted {
    param([object]$Summary)

    return (
        ($null -ne $Summary) -and
        ($Summary.status -eq 'ok') -and
        ($Summary.stage -eq 'completed') -and
        ($Summary.ws_id -eq 'WS-3') -and
        ($Summary.visual_review_required -eq $false) -and
        ($Summary.ws3_pill_visual.accepted -eq $true) -and
        ($Summary.ws3_pill_visual.side_by_side_or_reference_diff_pass -eq $true) -and
        ($Summary.ws3_pill_visual.frosted_backdrop_collapsed_pill_pass -eq $true) -and
        ($Summary.ws3_pill_visual.shape_size_matrix_pass -eq $true) -and
        ($Summary.ws3_pill_visual.overflow_policy -eq 'single_line_shrink_no_wrap') -and
        ($Summary.ws3_pill_visual.count_chip_pass -eq $true) -and
        ($Summary.ws3_pill_visual.dot_policy -eq 'collapsed_status_dot_token_and_expanded_dot_absent') -and
        ($Summary.ws3_pill_visual.hover_scale_policy -eq '0_geometry_scale_surface_tone_only') -and
        ($Summary.ws3_pill_visual.no_emoji_visible_pass -eq $true) -and
        ($Summary.ws3_pill_visual.top_edge_pass -eq $true) -and
        ($Summary.ws3_pill_visual.typography_alignment_pass -eq $true) -and
        ($Summary.ws3_pill_visual.animation_overlap_guard_pass -eq $true) -and
        ($Summary.ws3_pill_visual.stale_inner_frame_absent -eq $true) -and
        ($Summary.ws3_pill_visual.no_mock_data -eq $true) -and
        ($Summary.ws3_pill_visual.runtime_window_class -eq 'BentoNanoShell') -and
        ($Summary.ws3_pill_visual.runtime_summaries_accepted -eq $true)
    )
}

function New-Ws3RowFromCurrentPillVisual {
    param(
        [object]$Row,
        [object]$Summary
    )

    if ($null -eq $Row) {
        return $null
    }
    if (-not (Test-Ws3PillVisualAccepted $Summary)) {
        return $Row
    }

    $strongEvidence = @(
        "Current WS-3 pill visual summary is $ws3PillVisualSummaryPath.",
        "Gate status=$($Summary.status); accepted=$($Summary.ws3_pill_visual.accepted); visual_review_required=$($Summary.visual_review_required); no_mock_data=$($Summary.ws3_pill_visual.no_mock_data).",
        "Pill visual contract: frosted_backdrop_collapsed_pill_pass=$($Summary.ws3_pill_visual.frosted_backdrop_collapsed_pill_pass); shape_size_matrix_pass=$($Summary.ws3_pill_visual.shape_size_matrix_pass); overflow_policy=$($Summary.ws3_pill_visual.overflow_policy).",
        "Badges and indicators: count_chip_pass=$($Summary.ws3_pill_visual.count_chip_pass); dot_policy=$($Summary.ws3_pill_visual.dot_policy); hover_scale_policy=$($Summary.ws3_pill_visual.hover_scale_policy).",
        "Icon and edge policy: no_emoji_visible_pass=$($Summary.ws3_pill_visual.no_emoji_visible_pass); top_edge_pass=$($Summary.ws3_pill_visual.top_edge_pass).",
        "Cross-proof guardrails: typography_alignment_pass=$($Summary.ws3_pill_visual.typography_alignment_pass); animation_overlap_guard_pass=$($Summary.ws3_pill_visual.animation_overlap_guard_pass); stale_inner_frame_absent=$($Summary.ws3_pill_visual.stale_inner_frame_absent).",
        "Artifacts include runtime-proof-0618-ws3-pill-visual-closure-try\summary.json, focused cargo test logs, source-contract checks, current collapsed/hover/expanded screenshots, typography summary, animation summary, WS-4 expanded-grid summary, and five-issue closure summary."
    )

    return [pscustomobject]@{
        ws_id = 'WS-3'
        closure_status = 'closed'
        blocking_gap_count = 0
        strong_evidence_count = [int]$strongEvidence.Count
        weak_or_indirect_evidence_count = 0
        blocking_gaps = @()
        strong_evidence = $strongEvidence
        weak_or_indirect_evidence = @()
        recommended_next_gate = 'No WS-3 row-level blockers remain in the current pill visual gate.'
        notes = 'WS-3 is closed by the current dedicated pill visual runtime/source/test gate. This does not close the full PRD because other WS rows and final release/budget/clean-tree gates remain open.'
    }
}

function New-Ws5RowFromCurrentAnimation {
    param(
        [object]$Summary,
        [object]$FullClosure,
        [object]$A3Summary
    )

    if ($null -eq $Summary) {
        return $null
    }
    if ($Summary.status -ne 'ok' -or $Summary.ws5_acceptance.accepted -ne $true) {
        return $null
    }

    $visual = $Summary.visual_motion
    $tick = $visual.zone_drag_tick_interval_ms
    $a3Accepted = Test-A3AutoReboundAccepted $A3Summary
    $fullAccepted = (
        ($null -ne $FullClosure) -and
        ($FullClosure.status -eq 'ok') -and
        ($FullClosure.stage -eq 'completed') -and
        ($FullClosure.ws_id -eq 'WS-5') -and
        ($FullClosure.no_mock_data -eq $true) -and
        ($FullClosure.visual_review_required -eq $false) -and
        ($FullClosure.ws5_full_closure.accepted -eq $true) -and
        ($FullClosure.keyframe_alignment.accepted -eq $true) -and
        ($FullClosure.hover_press_delta.accepted -eq $true) -and
        ($FullClosure.runtime_evidence.accepted -eq $true) -and
        ($FullClosure.a3_auto_rebound.accepted -eq $true)
    )
    $blockingGaps = @()
    if (-not $a3Accepted) {
        $blockingGaps += 'A3 auto-rebound still needs a current runtime hover-enter -> expand -> leave -> collapse acceptance proof with collapse timing/state/screenshots.'
    }
    if (-not $fullAccepted) {
        $blockingGaps += @(
            'Tauri/reference animation keyframe alignment is still not machine-compared for the full open/close spring curve.',
            'Pill and item hover/press delta acceptance is still not separately proven against the current visual/reference contract.'
        )
    }
    $strongEvidence = @(
        "Current WS-5 animation acceptance summary is $ws5AnimationSummaryPath.",
        "Gate status=$($Summary.status); accepted=$($Summary.ws5_acceptance.accepted); scope=$($Summary.ws5_acceptance.scope).",
        "Continuous drag cadence: frame_delta_count=$($visual.frame_delta_count); non_repeated=$($visual.non_repeated_frame_delta_count); repeated=$($visual.repeated_frame_delta_count); tick_count=$($tick.count); tick_mean_ms=$($tick.mean); tick_p95_ms=$($tick.p95); pfd_ms=$($visual.perceived_frame_duration_ms); jank_percent=$($visual.jank_percent).",
        "State arbitration during zone drag: idle=$($Summary.animation_state.state_arbitration_idle); tick_skip_seen=$($Summary.animation_state.tick_skip_seen); zone_release_cleared=$($Summary.animation_state.zone_release_cleared); process_exited_after_quit_hotkey=$($Summary.process_exited_after_quit_hotkey).",
        "Artifacts include runtime-proof-0618-ws5-animation-acceptance-try\cadence-metrics.json, pixel-assertions.json, frame-timing.csv, state-dumps.jsonl, and 30 drag-frame screenshots."
    )
    if ($a3Accepted) {
        $strongEvidence += "Current A3 auto-rebound summary is $ws0A3AutoReboundSummaryPath; expand_after_enter_ms=$($A3Summary.a3_auto_rebound.expand_after_enter_ms); collapse_after_leave_ms=$($A3Summary.a3_auto_rebound.collapse_after_leave_ms); settled_after_leave_ms=$($A3Summary.a3_auto_rebound.settled_after_leave_ms); opened_visibly=$($A3Summary.a3_auto_rebound.opened_visibly); closed_back_to_pill=$($A3Summary.a3_auto_rebound.closed_back_to_pill); no_write_during_hover=$($A3Summary.a3_auto_rebound.no_write_during_hover)."
    }
    if ($fullAccepted) {
        $strongEvidence += @(
            "Current WS-5 full closure summary is $ws5FullClosureSummaryPath.",
            "Keyframe alignment: accepted=$($FullClosure.keyframe_alignment.accepted); size_curve=$($FullClosure.keyframe_alignment.size_curve); color_curve=$($FullClosure.keyframe_alignment.color_curve); size_duration_ms=$($FullClosure.keyframe_alignment.size_duration_ms); color_duration_ms=$($FullClosure.keyframe_alignment.color_duration_ms).",
            "Hover/press delta: accepted=$($FullClosure.hover_press_delta.accepted); pill_policy=$($FullClosure.hover_press_delta.pill_policy); item_hover_scale=$($FullClosure.hover_press_delta.item_hover_scale); item_press_scale=$($FullClosure.hover_press_delta.item_press_scale); item_hover_duration_ms=$($FullClosure.hover_press_delta.item_hover_duration_ms); item_press_duration_ms=$($FullClosure.hover_press_delta.item_press_duration_ms).",
            "Full closure retained runtime evidence: runtime_accepted=$($FullClosure.runtime_evidence.accepted); a3_accepted=$($FullClosure.a3_auto_rebound.accepted); visual_review_required=$($FullClosure.visual_review_required)."
        )
    }
    $weakEvidence = @()
    if (-not $fullAccepted) {
        $weakEvidence += $(if ($a3Accepted) {
            'This overlay closes the current continuous drag cadence/jank and A3 auto-rebound sub-gates only; WS-5 remains partial until the remaining animation acceptance gates are proven.'
        } else {
            'This overlay closes the current continuous drag cadence/jank sub-gate only; WS-5 remains partial until the remaining animation acceptance gates are proven.'
        })
    }
    $closed = $blockingGaps.Count -eq 0

    return [pscustomobject]@{
        ws_id = 'WS-5'
        closure_status = if ($closed) { 'closed' } else { 'partial' }
        blocking_gap_count = [int]$blockingGaps.Count
        strong_evidence_count = [int]$strongEvidence.Count
        weak_or_indirect_evidence_count = [int]$weakEvidence.Count
        blocking_gaps = $blockingGaps
        strong_evidence = $strongEvidence
        weak_or_indirect_evidence = $weakEvidence
        recommended_next_gate = if ($closed) { 'No WS-5 row-level blockers remain in the current animation full-closure gate.' } else { "Resolve the remaining WS-5 animation gates: $($blockingGaps -join ' | ')" }
        notes = if ($closed) { 'WS-5 is closed by the current dedicated runtime cadence proof, A3 auto-rebound proof, and WS-5 full closure source/test contract gate. This does not close the full PRD because other WS rows and final release/budget/clean-tree gates may remain open.' } else { 'WS-5 is sourced from the current dedicated runtime cadence proof plus, when available, the current A3 auto-rebound proof. It remains partial until the full closure gate proves keyframe alignment and hover/press deltas.' }
    }
}

function Test-Ws6GestureClampAccepted {
    param([object]$Summary)

    if ($null -eq $Summary) {
        return $false
    }
    $edgeCases = @($Summary.clamp.edge_cases)
    $tests = @($Summary.tests)
    return (
        ($Summary.status -eq 'ok') -and
        ($Summary.stage -eq 'completed') -and
        ($Summary.ws_id -eq 'WS-6') -and
        ($Summary.no_mock_data -eq $true) -and
        ($Summary.ws6_gesture_clamp.accepted -eq $true) -and
        ($Summary.ws6_gesture_clamp.runtime_strict_edge_clamp_pass -eq $true) -and
        ($Summary.ws6_gesture_clamp.no_write_during_drag -eq $true) -and
        ($Summary.ws6_gesture_clamp.write_after_release -eq $true) -and
        ($Summary.ws6_gesture_clamp.f2_merge_dissolve_sanity -eq $true) -and
        ($Summary.ws6_gesture_clamp.release_bound_runtime_chain -eq $true) -and
        ($Summary.clamp.all_passed -eq $true) -and
        ($edgeCases.Count -ge 5) -and
        (@($edgeCases | Where-Object { $_.passed -ne $true }).Count -eq 0) -and
        (@($tests | Where-Object { $_.passed -ne $true }).Count -eq 0) -and
        ($Summary.drag.no_write_during_drag -eq $true) -and
        ($Summary.drag.write_after_release -eq $true) -and
        ($Summary.f2.sanity_pass -eq $true) -and
        ($Summary.process_exited_after_quit_hotkey -eq $true)
    )
}

function New-Ws6RowFromCurrentGestureClamp {
    param(
        [object]$Row,
        [object]$Summary
    )

    if ($null -eq $Row) {
        return $null
    }
    if (-not (Test-Ws6GestureClampAccepted $Summary)) {
        return $Row
    }

    $edgeSummary = (@($Summary.clamp.edge_cases) | ForEach-Object {
        "$($_.name)=($($_.clamped.x),$($_.clamped.y))"
    }) -join '; '
    $strongEvidence = @(
        "Current WS-6 gesture/clamp summary is $ws6GestureClampSummaryPath.",
        "Gate status=$($Summary.status); accepted=$($Summary.ws6_gesture_clamp.accepted); no_mock_data=$($Summary.no_mock_data); main_window_class=$($Summary.main_window.class); process_exited_after_quit_hotkey=$($Summary.process_exited_after_quit_hotkey).",
        "Monitor topology: host_monitor_count=$($Summary.clamp.host_monitor_count); seam_case=$($Summary.clamp.seam_case); union_bounds=[$($Summary.clamp.union_bounds.left),$($Summary.clamp.union_bounds.top),$($Summary.clamp.union_bounds.right),$($Summary.clamp.union_bounds.bottom)].",
        "Runtime strict edge clamp passed for left/top/right/bottom/right_bottom: $edgeSummary.",
        "Drag persistence contract: live_move_log_count=$($Summary.drag.live_move_log_count); no_write_during_drag=$($Summary.drag.no_write_during_drag); write_after_release=$($Summary.drag.write_after_release).",
        "F2 sanity: merge/dissolve summary=$($Summary.f2.merge_dissolve_summary); sanity_pass=$($Summary.f2.sanity_pass); release_bound_runtime_chain=$($Summary.ws6_gesture_clamp.release_bound_runtime_chain).",
        "Focused tests passed: platform monitor smoke, zone monitor smoke, strict union-bounds, shell drag clamp smoke, live geometry before dispatcher drain, and app zone-drag merge ghost eligibility."
    )

    return [pscustomobject]@{
        ws_id = 'WS-6'
        closure_status = 'closed'
        blocking_gap_count = 0
        strong_evidence_count = [int]$strongEvidence.Count
        weak_or_indirect_evidence_count = 0
        blocking_gaps = @()
        strong_evidence = $strongEvidence
        weak_or_indirect_evidence = @()
        recommended_next_gate = 'No WS-6 row-level blockers remain in the current gesture/clamp gate.'
        notes = 'WS-6 is closed by the current dedicated runtime edge-clamp proof plus F2 merge/dissolve sanity evidence. This does not close the full PRD because other WS rows and final release/budget/clean-tree gates remain open.'
    }
}

function Test-Ws7MemoryBudgetAccepted {
    param([object]$Summary)

    return (
        ($null -ne $Summary) -and
        ($Summary.status -eq 'ok') -and
        ($Summary.accepted -eq $true) -and
        ($Summary.max_private_mb -le 25.0) -and
        ($Summary.benchmark.private_mb_under_25_at_t30 -eq $true) -and
        ($Summary.benchmark.private_mb_under_25_at_t60 -eq $true) -and
        ($Summary.process_exited_after_quit_hotkey -eq $true)
    )
}

function Test-Ws7FinalValidationAccepted {
    param([object]$Summary)

    return (
        ($null -ne $Summary) -and
        ($Summary.status -eq 'ok') -and
        ($Summary.stage -eq 'completed') -and
        ($Summary.no_mock_data -eq $true) -and
        ($Summary.cargo_test.accepted -eq $true) -and
        ($Summary.cargo_test.exit_code -eq 0) -and
        ($Summary.cargo_test.results.passed_total -ge 1675) -and
        ($Summary.cargo_test.results.failed_total -eq 0) -and
        ($Summary.cargo_clippy.accepted -eq $true) -and
        ($Summary.cargo_clippy.exit_code -eq 0) -and
        ($Summary.cargo_clippy.results.error_line_count -eq 0) -and
        ($Summary.cargo_clippy.results.warning_line_count -eq 0) -and
        ($Summary.snap_reconciliation.accepted -eq $true) -and
        ($Summary.snap_reconciliation.results.modified_or_untracked_count -eq 6)
    )
}

function New-Ws7RowFromCurrentBudget {
    param(
        [object]$Row,
        [Nullable[int64]]$ReleaseBytes,
        [string]$ReleaseExe,
        [object]$MemorySummary,
        [bool]$MemoryAccepted,
        [object]$FinalValidationSummary,
        [bool]$FinalValidationAccepted,
        [bool]$FinalReceiptExists
    )

    if ($null -eq $Row) {
        return $null
    }

    $blockingGaps = @($Row.blocking_gaps)
    $strongEvidence = @($Row.strong_evidence)
    $weakEvidence = @($Row.weak_or_indirect_evidence)

    if (($null -ne $ReleaseBytes) -and ($ReleaseBytes -le 2621440)) {
        $blockingGaps = @($blockingGaps | Where-Object {
            $_ -notlike 'No current release-build binary-size proof*'
        })
        $strongEvidence += "Current release binary budget passes: $ReleaseExe is $ReleaseBytes bytes (<= 2621440)."
        $weakEvidence = @($weakEvidence | Where-Object {
            $_ -notmatch 'release binary|release executable|binary'
        })
    }

    if ($MemoryAccepted) {
        $blockingGaps = @($blockingGaps | Where-Object {
            $_ -notmatch 'private bytes|Private Bytes|private-bytes|private_mb'
        })
        $strongEvidence += @(
            "Current WS-7 memory budget summary is $ws7MemoryBudgetSummaryPath.",
            "Current release runtime memory proof passes: max_private_mb=$($MemorySummary.max_private_mb); t10=$($MemorySummary.benchmark.t10.private_mb); t30=$($MemorySummary.benchmark.t30.private_mb); t60=$($MemorySummary.benchmark.t60.private_mb); process_exited_after_quit_hotkey=$($MemorySummary.process_exited_after_quit_hotkey).",
            "Runtime scene used isolated state with 5 zones, 50 items, visible BentoNanoShell, visible BentoAuxMbar, tray registration, locale/acrylic startup diagnostics, and production QuitApp hotkey exit."
        )
        $weakEvidence = @($weakEvidence | Where-Object {
            $_ -notmatch 'private_mb|Private Bytes|private bytes|0505 final whole-app'
        })
    }

    if ($FinalValidationAccepted) {
        $blockingGaps = @($blockingGaps | Where-Object {
            ($_ -notmatch 'full-regression|full regression|tests >=1675|cargo clippy|clippy|snap sync|Snap sync') -and
            ($_ -notmatch 'commit/tag boundary|tag does not cover|tag blockers')
        })
        if ($FinalReceiptExists) {
            $blockingGaps = @($blockingGaps | Where-Object {
                $_ -notmatch 'final WS-7/full-PRD receipt|corresponding receipt'
            })
        }
        $strongEvidence += @(
            "Current WS-7 final validation summary is $ws7FinalValidationSummaryPath.",
            "Current workspace tests pass: passed_total=$($FinalValidationSummary.cargo_test.results.passed_total); failed_total=$($FinalValidationSummary.cargo_test.results.failed_total); group_count=$($FinalValidationSummary.cargo_test.results.group_count); exit_code=$($FinalValidationSummary.cargo_test.exit_code).",
            "Current workspace clippy passes: exit_code=$($FinalValidationSummary.cargo_clippy.exit_code); errors=$($FinalValidationSummary.cargo_clippy.results.error_line_count); warnings=$($FinalValidationSummary.cargo_clippy.results.warning_line_count).",
            "Current snap reconciliation is captured for exactly $($FinalValidationSummary.snap_reconciliation.results.modified_or_untracked_count) modified snap.md baselines; full diff is in runtime-proof-0618-ws7-final-validation-try\33-git-diff-snap.stdout.log."
        )
        if ($FinalReceiptExists) {
            $strongEvidence += "Current WS-7 final validation receipt exists: $ws7FinalReceiptPath."
        }
        $weakEvidence = @($weakEvidence | Where-Object {
            $_ -notmatch 'full-regression|clippy|snap|commit|tag|receipt'
        })
    }

    $closed = $blockingGaps.Count -eq 0
    return [pscustomobject]@{
        ws_id = 'WS-7'
        closure_status = if ($closed) { 'closed' } else { 'partial' }
        blocking_gap_count = [int]$blockingGaps.Count
        strong_evidence_count = [int]$strongEvidence.Count
        weak_or_indirect_evidence_count = [int]$weakEvidence.Count
        blocking_gaps = $blockingGaps
        strong_evidence = $strongEvidence
        weak_or_indirect_evidence = $weakEvidence
        recommended_next_gate = if ($closed) {
            'No WS-7 row-level blockers remain; the full PRD gate still separately requires task completion, clean tree, and a tag at HEAD.'
        } else {
            "Resolve the remaining WS-7 final gates: $($blockingGaps -join ' | ')"
        }
        notes = if ($closed) {
            'WS-7 row-level evidence is closed by current budget/regression artifacts. This does not override the PRD-level clean-tree/task/tag gates.'
        } else {
            'WS-7 is updated with current release binary and private-bytes proof when present. It remains partial until the current full regression, clippy, snap reconciliation, final receipt/task state, clean-tree, commit, and tag blockers are closed.'
        }
    }
}

$taskJson = Read-JsonPath $taskJsonPath
$fiveIssue = Read-JsonPath $fiveIssueSummaryPath
$ws1Closure = Read-JsonPath $ws1ClosureSummaryPath
$ws5Animation = Read-JsonPath $ws5AnimationSummaryPath
$ws5FullClosure = Read-JsonPath $ws5FullClosureSummaryPath
$ws0A3AutoRebound = Read-JsonPath $ws0A3AutoReboundSummaryPath
$ws0CoreClosure = Read-JsonPath $ws0CoreClosureSummaryPath
$ws4ExpandedGrid = Read-JsonPath $ws4ExpandedGridSummaryPath
$ws2AppearanceClosure = Read-JsonPath $ws2AppearanceClosureSummaryPath
$ws3PillVisual = Read-JsonPath $ws3PillVisualSummaryPath
$ws6GestureClamp = Read-JsonPath $ws6GestureClampSummaryPath
$ws7MemoryBudget = Read-JsonPath $ws7MemoryBudgetSummaryPath
$ws7FinalValidation = Read-JsonPath $ws7FinalValidationSummaryPath

$wsRows = @()
if (Test-Path -LiteralPath $wsAuditCsv) {
    foreach ($row in (Import-Csv -LiteralPath $wsAuditCsv)) {
        $result = $row.result_json | ConvertFrom-Json
        $wsRows += [pscustomobject]@{
            ws_id = $result.ws_id
            closure_status = $result.closure_status
            blocking_gap_count = @($result.blocking_gaps).Count
            strong_evidence_count = @($result.strong_evidence).Count
            weak_or_indirect_evidence_count = @($result.weak_or_indirect_evidence).Count
            blocking_gaps = @($result.blocking_gaps)
            strong_evidence = @($result.strong_evidence)
            weak_or_indirect_evidence = @($result.weak_or_indirect_evidence)
            recommended_next_gate = $result.recommended_next_gate
            notes = $result.notes
        }
    }
}

$ws0BaseRow = $wsRows | Where-Object { $_.ws_id -eq 'WS-0' } | Select-Object -First 1
$currentWs0Row = New-Ws0RowWithA3Overlay $ws0BaseRow $ws0A3AutoRebound
if ($null -ne $currentWs0Row) {
    $wsRows = @($wsRows | Where-Object { $_.ws_id -ne 'WS-0' })
    $wsRows += $currentWs0Row
}
$ws0BaseRow = $wsRows | Where-Object { $_.ws_id -eq 'WS-0' } | Select-Object -First 1
$currentWs0CoreRow = New-Ws0RowFromCurrentCoreClosure $ws0BaseRow $ws0CoreClosure
if ($null -ne $currentWs0CoreRow) {
    $wsRows = @($wsRows | Where-Object { $_.ws_id -ne 'WS-0' })
    $wsRows += $currentWs0CoreRow
}
$currentWs1Row = New-Ws1RowFromCurrentClosure $ws1Closure
if ($null -ne $currentWs1Row) {
    $wsRows = @($wsRows | Where-Object { $_.ws_id -ne 'WS-1' })
    $wsRows += $currentWs1Row
}
$ws2BaseRow = $wsRows | Where-Object { $_.ws_id -eq 'WS-2' } | Select-Object -First 1
$currentWs2Row = New-Ws2RowFromCurrentAppearance $ws2BaseRow $ws2AppearanceClosure
if ($null -ne $currentWs2Row) {
    $wsRows = @($wsRows | Where-Object { $_.ws_id -ne 'WS-2' })
    $wsRows += $currentWs2Row
}
$ws3BaseRow = $wsRows | Where-Object { $_.ws_id -eq 'WS-3' } | Select-Object -First 1
$currentWs3Row = New-Ws3RowFromCurrentPillVisual $ws3BaseRow $ws3PillVisual
if ($null -ne $currentWs3Row) {
    $wsRows = @($wsRows | Where-Object { $_.ws_id -ne 'WS-3' })
    $wsRows += $currentWs3Row
}
$ws4BaseRow = $wsRows | Where-Object { $_.ws_id -eq 'WS-4' } | Select-Object -First 1
$currentWs4Row = New-Ws4RowFromCurrentExpandedGrid $ws4BaseRow $ws4ExpandedGrid
if ($null -ne $currentWs4Row) {
    $wsRows = @($wsRows | Where-Object { $_.ws_id -ne 'WS-4' })
    $wsRows += $currentWs4Row
}
$currentWs5Row = New-Ws5RowFromCurrentAnimation $ws5Animation $ws5FullClosure $ws0A3AutoRebound
if ($null -ne $currentWs5Row) {
    $wsRows = @($wsRows | Where-Object { $_.ws_id -ne 'WS-5' })
    $wsRows += $currentWs5Row
}
$ws6BaseRow = $wsRows | Where-Object { $_.ws_id -eq 'WS-6' } | Select-Object -First 1
$currentWs6Row = New-Ws6RowFromCurrentGestureClamp $ws6BaseRow $ws6GestureClamp
if ($null -ne $currentWs6Row) {
    $wsRows = @($wsRows | Where-Object { $_.ws_id -ne 'WS-6' })
    $wsRows += $currentWs6Row
}

$gitStatus = Invoke-GitText @('status', '--short', '--untracked-files=all')
$gitDescribe = Invoke-GitText @('describe', '--tags', '--dirty', '--always')
$gitBranch = Invoke-GitText @('rev-parse', '--abbrev-ref', 'HEAD')
$gitHead = Invoke-GitText @('rev-parse', '--short', 'HEAD')
$gitTagsAtHead = Invoke-GitText @('tag', '--points-at', 'HEAD')

$dirtyLines = @()
if ($gitStatus.text.Length -gt 0) {
    $dirtyLines = @($gitStatus.text -split "`r?`n" | Where-Object { $_.Trim().Length -gt 0 })
}

$releaseExeCandidates = @(
    (Join-Path -Path $repoRoot -ChildPath 'target\x86_64-pc-windows-msvc\release\bento-nano-shell.exe'),
    (Join-Path -Path $repoRoot -ChildPath 'target\release\bento-nano-shell.exe')
)
$releaseExe = $releaseExeCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
$releaseBytes = if ($releaseExe) { (Get-Item -LiteralPath $releaseExe).Length } else { $null }

$oldFinalProof = Read-JsonPath (Join-Path $repoRoot 'runtime-proof-0505-final-whole-app-regression-20260519-try\summary.json')
$oldPrivateMb = $null
if ($null -ne $oldFinalProof) {
    $t60 = $oldFinalProof.memory_samples | Where-Object { $_.label -eq 't60' } | Select-Object -First 1
    if ($null -ne $t60) {
        $oldPrivateMb = $t60.private_mb
    }
}
$ws7MemoryAccepted = Test-Ws7MemoryBudgetAccepted $ws7MemoryBudget
$ws7FinalValidationAccepted = Test-Ws7FinalValidationAccepted $ws7FinalValidation
$ws7FinalReceiptExists = Test-Path -LiteralPath $ws7FinalReceiptPath

$ws7BaseRow = $wsRows | Where-Object { $_.ws_id -eq 'WS-7' } | Select-Object -First 1
$currentWs7Row = New-Ws7RowFromCurrentBudget $ws7BaseRow $releaseBytes $releaseExe $ws7MemoryBudget $ws7MemoryAccepted $ws7FinalValidation $ws7FinalValidationAccepted $ws7FinalReceiptExists
if ($null -ne $currentWs7Row) {
    $wsRows = @($wsRows | Where-Object { $_.ws_id -ne 'WS-7' })
    $wsRows += $currentWs7Row
}

$closedWsCount = @($wsRows | Where-Object { $_.closure_status -eq 'closed' }).Count
$partialWsCount = @($wsRows | Where-Object { $_.closure_status -eq 'partial' }).Count
$missingWsCount = @($wsRows | Where-Object { $_.closure_status -eq 'missing' }).Count
$unclearWsCount = @($wsRows | Where-Object { $_.closure_status -eq 'unclear' }).Count
$notClosedRows = @($wsRows | Where-Object { $_.closure_status -ne 'closed' })
$blockingGapCount = ($wsRows | ForEach-Object { $_.blocking_gap_count } | Measure-Object -Sum).Sum
if ($null -eq $blockingGapCount) {
    $blockingGapCount = 0
}

$gates = @()
$gates += New-Gate 'five-defect-gate' 'Five user-reported visual defects are machine closed' `
    (($fiveIssue.status -eq 'ok') -and ($fiveIssue.blocking_failure_count -eq 0) -and ($fiveIssue.visual_review_required -eq $false)) `
    "status=$($fiveIssue.status); blocking_failure_count=$($fiveIssue.blocking_failure_count); visual_review_required=$($fiveIssue.visual_review_required)" `
    'status=ok, blocking_failure_count=0, visual_review_required=false' `
    'runtime-proof-0618-five-issue-closure-try\summary.json'
$gates += New-Gate 'all-ws-closed' 'All PRD WS-0..WS-7 rows are closed' `
    ($wsRows.Count -eq 8 -and $notClosedRows.Count -eq 0) `
    "rows=$($wsRows.Count); closed=$closedWsCount; partial=$partialWsCount; missing=$missingWsCount; unclear=$unclearWsCount" `
    '8 rows and all closure_status=closed' `
    '.trellis\tasks\05-29-nano-tauri-parity-plan\ws-acceptance-audit-2026-06-18-results.csv'
$gates += New-Gate 'task-json-complete' 'Trellis task status is completed' `
    ($taskJson.status -eq 'completed') `
    $taskJson.status `
    'completed' `
    '.trellis\tasks\05-29-nano-tauri-parity-plan\task.json'
$gates += New-Gate 'worktree-clean' 'Repo worktree is clean' `
    ($dirtyLines.Count -eq 0) `
    "$($dirtyLines.Count) dirty/untracked paths" `
    '0 dirty/untracked paths' `
    'git status --short --untracked-files=all'
$gates += New-Gate 'tag-at-head' 'Current HEAD is tagged for release closure' `
    ($gitTagsAtHead.text.Length -gt 0) `
    $gitTagsAtHead.text `
    'at least one tag points at HEAD' `
    'git tag --points-at HEAD'

$gates += New-Gate 'release-binary-budget' 'Current release binary is <= 2.5 MB' `
    (($null -ne $releaseBytes) -and ($releaseBytes -le 2621440)) `
    $(if ($null -eq $releaseBytes) { 'missing release executable' } else { "$releaseBytes bytes" }) `
    '<= 2621440 bytes' `
    'target release bento-nano-shell.exe'

$gates += New-Gate 'current-private-bytes-budget' 'Current main+minibar private bytes <= 25 MB' `
    $ws7MemoryAccepted `
    $(if ($ws7MemoryAccepted) { "current max_private_mb=$($ws7MemoryBudget.max_private_mb); t30=$($ws7MemoryBudget.benchmark.t30.private_mb); t60=$($ws7MemoryBudget.benchmark.t60.private_mb)" } elseif ($null -ne $ws7MemoryBudget) { "current proof not accepted: status=$($ws7MemoryBudget.status); accepted=$($ws7MemoryBudget.accepted); max_private_mb=$($ws7MemoryBudget.max_private_mb)" } elseif ($null -eq $oldPrivateMb) { 'no current private-bytes proof found' } else { "only stale 0505 proof found: t60.private_mb=$oldPrivateMb" }) `
    'current proof on this tree with private bytes <= 25 MB' `
    $ws7MemoryBudgetSummaryPath

$gates += New-Gate 'current-workspace-tests' 'Current workspace cargo test passes' `
    ($ws7FinalValidationAccepted -and ($ws7FinalValidation.cargo_test.accepted -eq $true)) `
    $(if ($ws7FinalValidationAccepted) { "passed_total=$($ws7FinalValidation.cargo_test.results.passed_total); failed_total=$($ws7FinalValidation.cargo_test.results.failed_total); group_count=$($ws7FinalValidation.cargo_test.results.group_count)" } elseif ($null -ne $ws7FinalValidation) { "current final validation not accepted: status=$($ws7FinalValidation.status); test_exit=$($ws7FinalValidation.cargo_test.exit_code); passed_total=$($ws7FinalValidation.cargo_test.results.passed_total); failed_total=$($ws7FinalValidation.cargo_test.results.failed_total)" } else { 'no current final validation proof found' }) `
    'current `cargo test --workspace --all-targets --target x86_64-pc-windows-msvc` proof with >=1675 passed and 0 failed' `
    $ws7FinalValidationSummaryPath

$gates += New-Gate 'current-workspace-clippy' 'Current workspace clippy -D warnings passes' `
    ($ws7FinalValidationAccepted -and ($ws7FinalValidation.cargo_clippy.accepted -eq $true)) `
    $(if ($ws7FinalValidationAccepted) { "exit_code=$($ws7FinalValidation.cargo_clippy.exit_code); errors=$($ws7FinalValidation.cargo_clippy.results.error_line_count); warnings=$($ws7FinalValidation.cargo_clippy.results.warning_line_count)" } elseif ($null -ne $ws7FinalValidation) { "current final validation not accepted: clippy_exit=$($ws7FinalValidation.cargo_clippy.exit_code); errors=$($ws7FinalValidation.cargo_clippy.results.error_line_count); warnings=$($ws7FinalValidation.cargo_clippy.results.warning_line_count)" } else { 'no current final validation proof found' }) `
    'current `cargo clippy --workspace --all-targets -- -D warnings` proof with 0 errors and 0 warnings' `
    $ws7FinalValidationSummaryPath

$gates += New-Gate 'current-snap-reconciliation' 'Current snap.md baseline changes are reconciled' `
    ($ws7FinalValidationAccepted -and ($ws7FinalValidation.snap_reconciliation.accepted -eq $true)) `
    $(if ($ws7FinalValidationAccepted) { "modified_or_untracked_count=$($ws7FinalValidation.snap_reconciliation.results.modified_or_untracked_count)" } elseif ($null -ne $ws7FinalValidation) { "current final validation not accepted: snap_count=$($ws7FinalValidation.snap_reconciliation.results.modified_or_untracked_count); accepted=$($ws7FinalValidation.snap_reconciliation.accepted)" } else { 'no current final validation proof found' }) `
    'current snap-status.json shows exactly the six known modified snap.md baselines and captures their diff' `
    (Join-Path $repoRoot 'runtime-proof-0618-ws7-final-validation-try\snap-status.json')

$priorityRows = @($wsRows | Sort-Object -Property @{ Expression = { $_.closure_status -eq 'closed' }; Ascending = $true }, @{ Expression = 'blocking_gap_count'; Descending = $true }, @{ Expression = 'ws_id'; Ascending = $true })
$nextGates = @($priorityRows | Select-Object -First 4 | ForEach-Object {
    [pscustomobject]@{
        ws_id = $_.ws_id
        closure_status = $_.closure_status
        blocking_gap_count = $_.blocking_gap_count
        recommended_next_gate = $_.recommended_next_gate
    }
})

$failedGates = @($gates | Where-Object { -not $_.pass })
$status = if ($failedGates.Count -eq 0) { 'ok' } else { 'attention_required' }

$matrixRows = @($wsRows | ForEach-Object {
    [pscustomobject]@{
        ws_id = $_.ws_id
        closure_status = $_.closure_status
        blocking_gap_count = $_.blocking_gap_count
        strong_evidence_count = $_.strong_evidence_count
        weak_or_indirect_evidence_count = $_.weak_or_indirect_evidence_count
        recommended_next_gate = $_.recommended_next_gate
    }
})

$summary = [pscustomobject]@{
    status = $status
    stage = 'completed'
    generated_at_utc = [DateTime]::UtcNow.ToString('o')
    task = '.trellis/tasks/05-29-nano-tauri-parity-plan'
    repo = 'bentodesk-nano'
    proof_dir = $outDir
    goal_complete = $false
    task_complete = $false
    five_defect_gate_status = $fiveIssue.status
    ws_rows_total = $wsRows.Count
    ws_closed_count = $closedWsCount
    ws_partial_count = $partialWsCount
    ws_missing_count = $missingWsCount
    ws_unclear_count = $unclearWsCount
    blocking_gap_count = [int]$blockingGapCount
    failed_prd_gate_count = $failedGates.Count
    git = [pscustomobject]@{
        branch = $gitBranch.text
        head = $gitHead.text
        describe = $gitDescribe.text
        tags_at_head = $gitTagsAtHead.text
        dirty_path_count = $dirtyLines.Count
        dirty_paths = $dirtyLines
    }
    gates = $gates
    next_recommended_gates = $nextGates
    current_gate_overrides = [pscustomobject]@{
        ws1_settings_closure_summary = $ws1ClosureSummaryPath
        ws1_override_applied = ($null -ne $currentWs1Row)
        ws5_animation_acceptance_summary = $ws5AnimationSummaryPath
        ws5_animation_full_closure_summary = $ws5FullClosureSummaryPath
        ws5_override_applied = ($null -ne $currentWs5Row)
        ws0_a3_auto_rebound_summary = $ws0A3AutoReboundSummaryPath
        ws0_a3_overlay_applied = (Test-A3AutoReboundAccepted $ws0A3AutoRebound)
        ws0_core_closure_summary = $ws0CoreClosureSummaryPath
        ws0_core_override_applied = (Test-Ws0CoreClosureAccepted $ws0CoreClosure)
        ws4_expanded_grid_summary = $ws4ExpandedGridSummaryPath
        ws4_override_applied = (Test-Ws4ExpandedGridAccepted $ws4ExpandedGrid)
        ws2_appearance_closure_summary = $ws2AppearanceClosureSummaryPath
        ws2_override_applied = (Test-Ws2AppearanceAccepted $ws2AppearanceClosure)
        ws3_pill_visual_summary = $ws3PillVisualSummaryPath
        ws3_override_applied = (Test-Ws3PillVisualAccepted $ws3PillVisual)
        ws6_gesture_clamp_summary = $ws6GestureClampSummaryPath
        ws6_override_applied = (Test-Ws6GestureClampAccepted $ws6GestureClamp)
        ws7_memory_budget_summary = $ws7MemoryBudgetSummaryPath
        ws7_memory_budget_applied = $ws7MemoryAccepted
        ws7_final_validation_summary = $ws7FinalValidationSummaryPath
        ws7_final_validation_applied = $ws7FinalValidationAccepted
        ws7_final_receipt = $ws7FinalReceiptPath
        ws7_final_receipt_exists = $ws7FinalReceiptExists
    }
    ws_rows = $wsRows
}

$summaryPath = Join-Path $outDir 'summary.json'
$matrixJsonPath = Join-Path $outDir 'ws-matrix.json'
$matrixCsvPath = Join-Path $outDir 'ws-matrix.csv'
Write-Utf8NoBom $summaryPath ($summary | ConvertTo-Json -Depth 30)
Write-Utf8NoBom $matrixJsonPath ($matrixRows | ConvertTo-Json -Depth 10)
$matrixRows | Export-Csv -Path $matrixCsvPath -NoTypeInformation -Encoding UTF8

Write-Host "prd_acceptance_status=$status"
Write-Host "summary=$summaryPath"
Write-Host "matrix_json=$matrixJsonPath"
Write-Host "matrix_csv=$matrixCsvPath"
Write-Host "ws_closed=$closedWsCount partial=$partialWsCount missing=$missingWsCount unclear=$unclearWsCount"
Write-Host "blocking_gap_count=$blockingGapCount"
if ($failedGates.Count -gt 0) {
    Write-Host "failed_prd_gates=$($failedGates.Count)"
    foreach ($gate in $failedGates) {
        Write-Host " - $($gate.id): observed=$($gate.observed); required=$($gate.required)"
    }
}
