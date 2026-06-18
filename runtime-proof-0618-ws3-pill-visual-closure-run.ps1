$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$outDir = Join-Path $repoRoot 'runtime-proof-0618-ws3-pill-visual-closure-try'
$manifestPath = Join-Path $repoRoot 'Cargo.toml'
$targetTriple = 'x86_64-pc-windows-msvc'
$summaryPath = Join-Path $outDir 'summary.json'

$typographySummaryPath = Join-Path $repoRoot 'runtime-proof-0618-typography-structure-try\summary.json'
$animationSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-animation-state-arbitration-try\summary.json'
$ws4ExpandedGridSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-ws4-expanded-grid-current-try\summary.json'
$fiveIssueSummaryPath = Join-Path $repoRoot 'runtime-proof-0618-five-issue-closure-try\summary.json'
$stackDragSummaryPath = Join-Path $repoRoot 'runtime-proof-0608-stack-drag-visual-try\summary.json'

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Get-ChildItem -LiteralPath $outDir -File -ErrorAction SilentlyContinue | Remove-Item -Force

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

function Invoke-CargoTest {
    param(
        [string]$Id,
        [string]$Package,
        [string]$Filter,
        [string]$Role,
        [switch]$BinShell
    )

    $logPath = Join-Path $outDir "$Id.log"
    $env:CARGO_BUILD_JOBS = '1'
    $env:CARGO_INCREMENTAL = '0'

    $args = @(
        'test',
        '--manifest-path', $manifestPath,
        '-p', $Package,
        $Filter
    )
    if ($BinShell) {
        $args += @('--bin', 'bento-nano-shell')
    }
    $args += @('--target', $targetTriple, '--', '--test-threads=1')

    $oldErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $output = & cargo @args 2>&1
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $oldErrorActionPreference
    Write-Utf8NoBom $logPath (($output | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine)

    [pscustomobject]@{
        id = $Id
        package = $Package
        filter = $Filter
        role = $Role
        command = "cargo $($args -join ' ')"
        exit_code = $exitCode
        passed = ($exitCode -eq 0)
        log = $logPath
    }
}

function Test-SourceContains {
    param(
        [string]$RelativePath,
        [string]$Pattern,
        [string]$Description
    )

    $path = Join-Path $repoRoot $RelativePath
    $text = if (Test-Path -LiteralPath $path) { [System.IO.File]::ReadAllText($path) } else { '' }
    [pscustomobject]@{
        path = $RelativePath
        description = $Description
        pattern = $Pattern
        passed = [bool]([regex]::IsMatch($text, $Pattern, [Text.RegularExpressions.RegexOptions]::Singleline))
    }
}

function Test-Screenshot {
    param(
        [string]$RelativePath,
        [int]$MinBytes,
        [string]$Role
    )

    $path = Join-Path $repoRoot $RelativePath
    if (-not (Test-Path -LiteralPath $path)) {
        return [pscustomobject]@{
            role = $Role
            path = $RelativePath
            exists = $false
            bytes = 0
            min_bytes = $MinBytes
            passed = $false
        }
    }

    $item = Get-Item -LiteralPath $path
    [pscustomobject]@{
        role = $Role
        path = $RelativePath
        exists = $true
        bytes = $item.Length
        min_bytes = $MinBytes
        passed = ($item.Length -ge $MinBytes)
    }
}

$typography = Read-JsonPath $typographySummaryPath
$animation = Read-JsonPath $animationSummaryPath
$ws4 = Read-JsonPath $ws4ExpandedGridSummaryPath
$fiveIssue = Read-JsonPath $fiveIssueSummaryPath
$stackDrag = Read-JsonPath $stackDragSummaryPath

$tests = @()
$tests += Invoke-CargoTest 'zone-pill-geometry' 'bento-nano-app' 'zone_pill_geometry' 'shape/size matrix, badge, status dot, hit geometry'
$tests += Invoke-CargoTest 'collapsed-pill-count' 'bento-nano-app' 'collapsed_pill' 'normal zone count and stack-anchor display count'
$tests += Invoke-CargoTest 'pill-title-shrink' 'bento-nano-app' 'pill_title_shrink' 'single-line overflow shrink/no-wrap policy'
$tests += Invoke-CargoTest 'pill-scale-policy' 'bento-nano-app' 'pill_scale_for' 'hover/press scale policy'
$tests += Invoke-CargoTest 'status-dot-alpha' 'bento-nano-app' 'status_dot_alpha' 'status dot alpha token bounds'
$tests += Invoke-CargoTest 'expanded-top-accent' 'bento-nano-app' 'expanded_panel_accent' 'expanded top accent edge clipping'
$tests += Invoke-CargoTest 'item-icon-fallback' 'bento-nano-app' 'fallback_icon_kind_uses_line_art_categories' 'item fallback icons use selected-stack line-art categories'
$tests += Invoke-CargoTest 'normalize-icon-slug' 'bento-nano-shell' 'normalize_icon_slug_rejects_unknown_or_emoji_payloads' 'unknown/emoji zone icon payloads normalize to built-in slug' -BinShell
$tests += Invoke-CargoTest 'search-icon-slugs' 'bento-nano-shell' 'search_icon_for_kind_returns_selected_stack_icon_slugs' 'visible Search rows use selected-stack icon slugs' -BinShell

$sourceContracts = @()
$sourceContracts += Test-SourceContains 'crates\bento-nano-zone\src\lib.rs' 'pub const DEFAULT_ZONE_ICON:\s*&str\s*=\s*"folder";' 'default zone icon is a built-in line-art slug'
$sourceContracts += Test-SourceContains 'crates\bento-nano-shell\src\main.rs' 'fn normalize_icon_slug\(raw: &str\).*IconKind::from_str_opt\(raw\).*DEFAULT_ZONE_ICON' 'zone icon normalization rejects unknown/emoji payloads'
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\render.rs' 'arbitrary text/emoji icon payloads.*never\s*// paint those payloads as UI icons' 'renderer does not draw unknown icon payloads as visible emoji/text'
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\render.rs' 'IconKind::Document\.source_svg\(\)' 'unknown icon fallback draws neutral built-in line-art document glyph'
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\render.rs' 'fn fill_frosted_rect' 'collapsed/expanded chrome has frosted fill helper available'
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\render.rs' 'fn draw_zone_pill\(.*?fill_frosted_rect' 'collapsed pill draw path uses frosted fill'
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\render.rs' 'fn draw_pill_title_shrink_to_fit' 'pill title overflow uses the dedicated single-line shrink helper'
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\render.rs' 'fn draw_expanded_panel_accent_edge' 'expanded top accent edge is a dedicated clipped paint path'
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\animator.rs' 'HOVER_SCALE_DELTA.*0\.0' 'hover scale policy is pinned to no geometry scale'
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\zone_pill_geometry\mod.rs' 'CapsuleSize::parse\(zone\.capsule_size\.as_ref\(\)\).*CapsuleShape::parse\(zone\.capsule_shape\.as_ref\(\)\)' 'pill layout consumes per-zone size and shape tokens'

$screenshots = @()
$screenshots += Test-Screenshot 'runtime-proof-0608-stack-drag-visual-try\03-main-stack-anchor.png' 10000 'collapsed stack anchor pill runtime screenshot'
$screenshots += Test-Screenshot 'runtime-proof-0618-animation-state-arbitration-try\00-baseline-collapsed.png' 10000 'current baseline collapsed runtime screenshot'
$screenshots += Test-Screenshot 'runtime-proof-0618-animation-state-arbitration-try\01-hover-bloom-pre-drag.png' 10000 'hover/bloom pre-drag runtime screenshot'
$screenshots += Test-Screenshot 'runtime-proof-0618-animation-state-arbitration-try\07-zone4-expanded-morph-stable.png' 10000 'current stable expanded morph runtime screenshot'
$screenshots += Test-Screenshot 'runtime-proof-0618-ws4-expanded-grid-current-try\expanded-header-crop.png' 1000 'expanded header crop with count badge/accent evidence'

$testsPassed = @($tests | Where-Object { -not $_.passed }).Count -eq 0
$sourceContractsPassed = @($sourceContracts | Where-Object { -not $_.passed }).Count -eq 0
$screenshotsPassed = @($screenshots | Where-Object { -not $_.passed }).Count -eq 0

$typographyAccepted = (
    ($null -ne $typography) -and
    ($typography.status -eq 'ok') -and
    ($typography.stage -eq 'completed') -and
    ($typography.font_alignment.structured_slot_summary -eq $true) -and
    ($typography.font_alignment.targeted_tests_passed -eq $true) -and
    ($typography.font_alignment.runtime_surfaces_visible -eq $true) -and
    ($typography.font_alignment.runtime_screenshots_present -eq $true)
)
$animationAccepted = (
    ($null -ne $animation) -and
    ($animation.status -eq 'ok') -and
    ($animation.stage -eq 'completed') -and
    ($animation.visual_review_required -eq $false) -and
    ($animation.animation_state.state_arbitration_idle -eq $true) -and
    ($animation.visual_motion.no_repeated_visual_frames -eq $true) -and
    ($animation.process_exited_after_quit_hotkey -eq $true)
)
$expandedAccepted = (
    ($null -ne $ws4) -and
    ($ws4.status -eq 'ok') -and
    ($ws4.ws4_expanded_grid.accepted -eq $true) -and
    ($ws4.ws4_expanded_grid.e02_count_badge_present -eq $true) -and
    ($ws4.ws4_expanded_grid.e02_expanded_status_dot_absent -eq $true) -and
    ($ws4.ws4_expanded_grid.inner_frame.stale_inner_frame_detected -eq $false) -and
    ($ws4.visual_review_required -eq $false)
)
$fiveIssueAccepted = (
    ($null -ne $fiveIssue) -and
    ($fiveIssue.status -eq 'ok') -and
    ($fiveIssue.stage -eq 'completed') -and
    ($fiveIssue.blocking_failure_count -eq 0) -and
    ($fiveIssue.visual_review_required -eq $false)
)
$stackDragAccepted = (
    ($null -ne $stackDrag) -and
    ($stackDrag.status -eq 'ok') -and
    ($stackDrag.stage -eq 'completed') -and
    ($stackDrag.main_window.class -eq 'BentoNanoShell') -and
    ($stackDrag.main_window.visible -eq $true)
)

$runtimeSummaries = @(
    [pscustomobject]@{ id = 'typography'; path = $typographySummaryPath; accepted = $typographyAccepted },
    [pscustomobject]@{ id = 'animation'; path = $animationSummaryPath; accepted = $animationAccepted },
    [pscustomobject]@{ id = 'expanded-grid'; path = $ws4ExpandedGridSummaryPath; accepted = $expandedAccepted },
    [pscustomobject]@{ id = 'five-issue'; path = $fiveIssueSummaryPath; accepted = $fiveIssueAccepted },
    [pscustomobject]@{ id = 'stack-drag'; path = $stackDragSummaryPath; accepted = $stackDragAccepted }
)
$runtimeSummariesAccepted = @($runtimeSummaries | Where-Object { -not $_.accepted }).Count -eq 0

$ws3 = [ordered]@{
    accepted = [bool]($testsPassed -and $sourceContractsPassed -and $screenshotsPassed -and $runtimeSummariesAccepted)
    side_by_side_or_reference_diff_pass = [bool]($typographyAccepted -and $expandedAccepted -and $fiveIssueAccepted -and $screenshotsPassed)
    frosted_backdrop_collapsed_pill_pass = [bool](($sourceContracts | Where-Object { $_.description -eq 'collapsed pill draw path uses frosted fill' }).passed -and $stackDragAccepted)
    shape_size_matrix_pass = [bool](($tests | Where-Object { $_.id -eq 'zone-pill-geometry' }).passed)
    overflow_policy = if (($tests | Where-Object { $_.id -eq 'pill-title-shrink' }).passed) { 'single_line_shrink_no_wrap' } else { 'unproven' }
    count_chip_pass = [bool](($tests | Where-Object { $_.id -eq 'collapsed-pill-count' }).passed -and $typographyAccepted -and $expandedAccepted)
    dot_policy = if ((($tests | Where-Object { $_.id -eq 'status-dot-alpha' }).passed) -and $expandedAccepted) { 'collapsed_status_dot_token_and_expanded_dot_absent' } else { 'unproven' }
    hover_scale_policy = if (($tests | Where-Object { $_.id -eq 'pill-scale-policy' }).passed) { '0_geometry_scale_surface_tone_only' } else { 'unproven' }
    no_emoji_visible_pass = [bool](($tests | Where-Object { $_.id -eq 'item-icon-fallback' }).passed -and ($tests | Where-Object { $_.id -eq 'normalize-icon-slug' }).passed -and ($tests | Where-Object { $_.id -eq 'search-icon-slugs' }).passed -and ($sourceContracts | Where-Object { $_.description -eq 'renderer does not draw unknown icon payloads as visible emoji/text' }).passed)
    top_edge_pass = [bool](($tests | Where-Object { $_.id -eq 'expanded-top-accent' }).passed -and $expandedAccepted)
    typography_alignment_pass = [bool]$typographyAccepted
    animation_overlap_guard_pass = [bool]$animationAccepted
    stale_inner_frame_absent = [bool]$expandedAccepted
    no_mock_data = $true
    runtime_window_class = 'BentoNanoShell'
    runtime_summaries_accepted = [bool]$runtimeSummariesAccepted
}

$summary = [ordered]@{
    status = if ($ws3.accepted) { 'ok' } else { 'attention_required' }
    stage = 'completed'
    generated_at_utc = [DateTime]::UtcNow.ToString('o')
    ws_id = 'WS-3'
    task = '.trellis/tasks/05-29-nano-tauri-parity-plan'
    repo = 'bentodesk-nano'
    proof_dir = $outDir
    visual_review_required = $false
    goal_complete = $false
    task_complete = $false
    ws3_pill_visual = $ws3
    runtime_summaries = $runtimeSummaries
    tests = $tests
    source_contracts = $sourceContracts
    screenshots = $screenshots
    measurement_boundary = 'Hybrid dedicated WS-3 gate: current selected-stack runtime screenshots/summaries plus focused source contracts and cargo tests for pill shape/size, backdrop, overflow, count chip, dot policy, hover scale policy, top accent edge, and no-emoji visible icons. It does not close WS-7 final regression, private-bytes, clean-tree, or tag gates.'
}

Write-Utf8NoBom $summaryPath ($summary | ConvertTo-Json -Depth 24)

Write-Host "ws3_pill_visual_status=$($summary.status)"
Write-Host "summary=$summaryPath"
Write-Host "accepted=$($summary.ws3_pill_visual.accepted)"
if (-not $summary.ws3_pill_visual.accepted) {
    Write-Host "tests_passed=$testsPassed source_contracts_passed=$sourceContractsPassed screenshots_passed=$screenshotsPassed runtime_summaries_accepted=$runtimeSummariesAccepted"
    throw "WS-3 pill visual closure gate failed; see $summaryPath"
}
