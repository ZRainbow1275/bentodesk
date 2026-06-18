$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$workspaceRoot = Split-Path -Parent $repoRoot
$proofDir = Join-Path $repoRoot 'runtime-proof-0618-ws5-animation-full-closure-try'
$summaryPath = Join-Path $proofDir 'summary.json'
$ws5RuntimeSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws5-animation-acceptance-try\summary.json'
$a3SummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws0-a3-auto-rebound-try\summary.json'
$cargoManifest = Join-Path $repoRoot 'Cargo.toml'

function Write-Utf8NoBom {
    param(
        [string]$Path,
        [string]$Text
    )
    $encoding = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

function Read-JsonPath {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }
    try {
        return (Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json)
    } catch {
        return $null
    }
}

function Invoke-CargoTestFilter {
    param(
        [string]$Filter,
        [string]$LogName
    )

    $logPath = Join-Path $proofDir $LogName
    $stdoutPath = Join-Path $proofDir ($LogName + '.stdout.tmp')
    $stderrPath = Join-Path $proofDir ($LogName + '.stderr.tmp')
    $env:CARGO_BUILD_JOBS = '1'
    $env:CARGO_INCREMENTAL = '0'

    $cargo = Get-Command cargo -ErrorAction Stop
    $manifestArg = '"' + $cargoManifest + '"'
    $argumentString = "test --manifest-path $manifestArg -p bento-nano-app --target x86_64-pc-windows-msvc $Filter"
    $process = Start-Process -FilePath $cargo.Source `
        -ArgumentList $argumentString `
        -WorkingDirectory $repoRoot `
        -NoNewWindow `
        -Wait `
        -PassThru `
        -RedirectStandardOutput $stdoutPath `
        -RedirectStandardError $stderrPath
    $exitCode = $process.ExitCode

    $stdout = if (Test-Path -LiteralPath $stdoutPath) { Get-Content -LiteralPath $stdoutPath -Raw -Encoding UTF8 } else { '' }
    $stderr = if (Test-Path -LiteralPath $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw -Encoding UTF8 } else { '' }
    Remove-Item -LiteralPath $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
    $text = $stderr + $stdout
    Write-Utf8NoBom $logPath $text

    $passed = 0
    foreach ($match in [regex]::Matches($text, 'test result: ok\. (\d+) passed')) {
        $passed += [int]$match.Groups[1].Value
    }

    $ok = ($exitCode -eq 0) -and ($passed -gt 0)
    return [pscustomobject]@{
        filter = $Filter
        log = $LogName
        exit_code = [int]$exitCode
        passed = [int]$passed
        ok = [bool]$ok
    }
}

function Test-A3Accepted {
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

function Test-Ws5RuntimeAccepted {
    param([object]$Summary)
    return (
        ($null -ne $Summary) -and
        ($Summary.status -eq 'ok') -and
        ($Summary.stage -eq 'completed') -and
        ($Summary.ws5_acceptance.accepted -eq $true) -and
        ($Summary.visual_review_required -eq $false) -and
        ($Summary.animation_state.state_arbitration_idle -eq $true) -and
        ($Summary.animation_state.item_state_arbitration_idle -eq $true) -and
        ($Summary.animation_state.highlight_suppressed_during_zone_drag -eq $true) -and
        ($Summary.animation_state.highlight_suppressed_during_item_drag -eq $true) -and
        ($Summary.visual_motion.continuous_drag_cadence_ok -eq $true) -and
        ($Summary.visual_motion.no_repeated_visual_frames -eq $true) -and
        ([int]$Summary.visual_motion.repeated_frame_delta_count -le 3) -and
        ([int]$Summary.visual_motion.non_repeated_frame_delta_count -ge 26) -and
        ($Summary.process_exited_after_quit_hotkey -eq $true)
    )
}

New-Item -ItemType Directory -Force -Path $proofDir | Out-Null
Get-ChildItem -LiteralPath $proofDir -Force -ErrorAction SilentlyContinue |
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue

$keyframeFilters = @(
    'pill_anim_duration_matches_tauri_spring_expand',
    'ease_out_back_pinned_samples',
    'ease_out_back_overshoots_past_one_midflight',
    'morph_pill_to_rect_with_back_curve_overshoots_then_settles',
    'ease_standard_pinned_samples_and_monotonic',
    'current_morph_rect_matches_old_inline_formula',
    'panel_sized_morph_follows_tauri_content_reveal_phase'
)

$hoverPressFilters = @(
    'pill_scale_for_combines_hover_and_press',
    'card_scale_hover_inflates_to_tauri_1_02',
    'card_scale_press_deflates_to_tauri_0_97',
    'card_press_duration_matches_tauri_80ms',
    'card_hover_duration_matches_tauri_transition_fast_150ms',
    'item_press_ramps_to_tauri_shrink_then_releases',
    'card_hover_lift_dy_zero_at_idle_minus_one_at_full_hover'
)

$keyframeResults = @()
for ($i = 0; $i -lt $keyframeFilters.Count; $i++) {
    $keyframeResults += Invoke-CargoTestFilter $keyframeFilters[$i] ('keyframe-{0:D2}-{1}.log' -f ($i + 1), $keyframeFilters[$i])
}

$hoverPressResults = @()
for ($i = 0; $i -lt $hoverPressFilters.Count; $i++) {
    $hoverPressResults += Invoke-CargoTestFilter $hoverPressFilters[$i] ('hover-press-{0:D2}-{1}.log' -f ($i + 1), $hoverPressFilters[$i])
}

$ws5Runtime = Read-JsonPath $ws5RuntimeSummaryPath
$a3Summary = Read-JsonPath $a3SummaryPath
$keyframeAccepted = @($keyframeResults | Where-Object { -not $_.ok }).Count -eq 0
$hoverPressAccepted = @($hoverPressResults | Where-Object { -not $_.ok }).Count -eq 0
$runtimeAccepted = Test-Ws5RuntimeAccepted $ws5Runtime
$a3Accepted = Test-A3Accepted $a3Summary
$accepted = $keyframeAccepted -and $hoverPressAccepted -and $runtimeAccepted -and $a3Accepted

$summary = [ordered]@{
    status = if ($accepted) { 'ok' } else { 'failed' }
    stage = 'completed'
    ws_id = 'WS-5'
    no_mock_data = $true
    visual_review_required = $false
    source_contract = [ordered]@{
        reference_runtime = 'bentodesk Tauri v1.3.0 local source contracts already ported into selected-stack tests'
        external_reference_basis = @(
            'CSS cubic-bezier maps input progress to output progress and allows y overshoot for bounce effects',
            'Animation parity can be machine tested by seeking/sampling explicit timing checkpoints and asserted computed state'
        )
        external_sources = @(
            'https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/easing-function/cubic-bezier',
            'https://www.w3.org/TR/css-easing-2/',
            'https://www.w3.org/TR/web-animations'
        )
    }
    keyframe_alignment = [ordered]@{
        accepted = [bool]$keyframeAccepted
        size_curve = 'cubic-bezier(0.34,1.56,0.64,1)'
        color_curve = 'cubic-bezier(0.25,0.1,0.25,1)'
        size_duration_ms = 500
        color_duration_ms = 300
        pinned_samples = [ordered]@{
            ease_out_back_0_25 = 0.816289
            ease_out_back_0_50 = 1.087401
            ease_out_back_0_70 = 1.075776
            css_ease_0_25 = 0.408511
            css_ease_0_50 = 0.802403
            css_ease_0_75 = 0.960459
        }
        tests = $keyframeResults
    }
    hover_press_delta = [ordered]@{
        accepted = [bool]$hoverPressAccepted
        pill_policy = 'geometry_identity_surface_tone_only'
        pill_hover_scale_delta = 0.0
        pill_press_scale_delta = 0.0
        item_hover_scale = 1.02
        item_press_scale = 0.97
        item_hover_duration_ms = 150
        item_press_duration_ms = 80
        item_hover_lift_dy = -1.0
        tests = $hoverPressResults
    }
    runtime_evidence = [ordered]@{
        accepted = [bool]$runtimeAccepted
        summary = $ws5RuntimeSummaryPath
        cadence_ok = if ($ws5Runtime) { [bool]$ws5Runtime.visual_motion.continuous_drag_cadence_ok } else { $false }
        state_arbitration_idle = if ($ws5Runtime) { [bool]$ws5Runtime.animation_state.state_arbitration_idle } else { $false }
        item_state_arbitration_idle = if ($ws5Runtime) { [bool]$ws5Runtime.animation_state.item_state_arbitration_idle } else { $false }
        process_exited_after_quit_hotkey = if ($ws5Runtime) { [bool]$ws5Runtime.process_exited_after_quit_hotkey } else { $false }
    }
    a3_auto_rebound = [ordered]@{
        accepted = [bool]$a3Accepted
        summary = $a3SummaryPath
        collapse_after_leave_ms = if ($a3Summary) { $a3Summary.a3_auto_rebound.collapse_after_leave_ms } else { $null }
        settled_after_leave_ms = if ($a3Summary) { $a3Summary.a3_auto_rebound.settled_after_leave_ms } else { $null }
        no_write_during_hover = if ($a3Summary) { [bool]$a3Summary.a3_auto_rebound.no_write_during_hover } else { $false }
    }
    ws5_full_closure = [ordered]@{
        accepted = [bool]$accepted
        closes_blockers = @(
            'Tauri/reference animation keyframe alignment is machine-compared for full open/close spring/color curve checkpoints.',
            'Pill and item hover/press delta acceptance is separately proven against current visual/source contract.'
        )
        retained_runtime_proofs = @(
            'continuous zone drag cadence and non-repeated visible deltas',
            'pointer-drag arbitration suppresses competing hover/morph/bloom/item-hover/highlight channels',
            'A3 hover enter/expand/leave/collapse auto-rebound'
        )
    }
}

$summaryJson = $summary | ConvertTo-Json -Depth 18
Write-Utf8NoBom $summaryPath $summaryJson

if (-not $accepted) {
    throw "WS-5 full closure proof failed; see $summaryPath"
}

Write-Output "ws5_full_closure_status=ok"
Write-Output "summary=$summaryPath"
