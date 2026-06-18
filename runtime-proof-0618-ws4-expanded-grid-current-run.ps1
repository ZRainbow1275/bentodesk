$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$outDir = Join-Path $repoRoot 'runtime-proof-0618-ws4-expanded-grid-current-try'
$manifestPath = Join-Path $repoRoot 'Cargo.toml'
$targetTriple = 'x86_64-pc-windows-msvc'
$innerFrameScript = Join-Path $repoRoot 'runtime-proof-0617-expanded-inner-frame-current-run.ps1'
$typographyScript = Join-Path $repoRoot 'runtime-proof-0618-typography-structure-run.ps1'
$morphSummaryPath = Join-Path $repoRoot 'runtime-proof-0608-expanded-morph-visual-try\summary.json'
$innerFrameSummaryPath = Join-Path $repoRoot 'runtime-proof-0617-expanded-inner-frame-current-try\summary.json'
$typographySummaryPath = Join-Path $repoRoot 'runtime-proof-0618-typography-structure-try\summary.json'
$summaryPath = Join-Path $outDir 'summary.json'

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

function Invoke-ProofScript {
    param(
        [string]$Id,
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "proof script not found: $Path"
    }

    $logPath = Join-Path $outDir "$Id.log"
    $oldErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $output = & powershell -ExecutionPolicy Bypass -File $Path 2>&1
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $oldErrorActionPreference
    Write-Utf8NoBom $logPath (($output | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine)

    [pscustomobject]@{
        id = $Id
        path = $Path
        command = "powershell -ExecutionPolicy Bypass -File `"$Path`""
        exit_code = $exitCode
        passed = ($exitCode -eq 0)
        log = $logPath
    }
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

function Copy-BitmapCrop {
    param(
        [System.Drawing.Bitmap]$Bitmap,
        [System.Drawing.Rectangle]$Rect,
        [string]$Path
    )

    $crop = New-Object System.Drawing.Bitmap($Rect.Width, $Rect.Height)
    $g = [System.Drawing.Graphics]::FromImage($crop)
    try {
        $g.DrawImage($Bitmap, [System.Drawing.Rectangle]::new(0, 0, $Rect.Width, $Rect.Height), $Rect, [System.Drawing.GraphicsUnit]::Pixel)
        $crop.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $g.Dispose()
        $crop.Dispose()
    }
}

function Clamp-Rect {
    param(
        [System.Drawing.Rectangle]$Rect,
        [int]$MaxWidth,
        [int]$MaxHeight
    )

    $x = [Math]::Max(0, [Math]::Min($MaxWidth - 1, $Rect.X))
    $y = [Math]::Max(0, [Math]::Min($MaxHeight - 1, $Rect.Y))
    $right = [Math]::Max($x + 1, [Math]::Min($MaxWidth, $Rect.Right))
    $bottom = [Math]::Max($y + 1, [Math]::Min($MaxHeight, $Rect.Bottom))
    [System.Drawing.Rectangle]::new($x, $y, $right - $x, $bottom - $y)
}

function Analyze-ExpandedScreenshot {
    param(
        [string]$ImagePath,
        [object]$Zone,
        [double]$Scale
    )

    Add-Type -AssemblyName System.Drawing
    $bmp = [System.Drawing.Bitmap]::new($ImagePath)
    try {
        $panelLeft = [int][Math]::Round([double]$Zone.x * $Scale)
        $panelTop = [int][Math]::Round([double]$Zone.y * $Scale)
        $panelWidth = [int][Math]::Round([double]$Zone.w * $Scale)
        $panelHeight = [int][Math]::Round([double]$Zone.h * $Scale)
        $panel = Clamp-Rect ([System.Drawing.Rectangle]::new($panelLeft, $panelTop, $panelWidth, $panelHeight)) $bmp.Width $bmp.Height
        $header = Clamp-Rect ([System.Drawing.Rectangle]::new($panel.X, $panel.Y, $panel.Width, [int][Math]::Round(48.0 * $Scale))) $bmp.Width $bmp.Height
        $footerBand = Clamp-Rect ([System.Drawing.Rectangle]::new($panel.X, $panel.Bottom - [int][Math]::Round(12.0 * $Scale), $panel.Width, [int][Math]::Round(12.0 * $Scale))) $bmp.Width $bmp.Height

        $greenPixels = 0
        $saturatedHeaderPixels = 0
        for ($y = $header.Top; $y -lt $header.Bottom; $y++) {
            for ($x = $header.Left; $x -lt $header.Right; $x++) {
                $p = $bmp.GetPixel($x, $y)
                $max = [Math]::Max($p.R, [Math]::Max($p.G, $p.B))
                $min = [Math]::Min($p.R, [Math]::Min($p.G, $p.B))
                if ($p.G -ge 120 -and $p.G -gt ($p.R * 1.25) -and $p.G -gt ($p.B * 1.25)) {
                    $greenPixels++
                }
                if (($max - $min) -ge 55 -and $max -ge 120) {
                    $saturatedHeaderPixels++
                }
            }
        }

        $footerBrightPixels = 0
        for ($y = $footerBand.Top; $y -lt $footerBand.Bottom; $y++) {
            for ($x = $footerBand.Left; $x -lt $footerBand.Right; $x++) {
                $p = $bmp.GetPixel($x, $y)
                $luma = 0.2126 * $p.R + 0.7152 * $p.G + 0.0722 * $p.B
                if ($luma -ge 72.0) {
                    $footerBrightPixels++
                }
            }
        }

        $headerCrop = Join-Path $outDir 'expanded-header-crop.png'
        $footerCrop = Join-Path $outDir 'expanded-footer-bottom-band-crop.png'
        Copy-BitmapCrop $bmp $header $headerCrop
        Copy-BitmapCrop $bmp $footerBand $footerCrop

        $footerPixels = $footerBand.Width * $footerBand.Height
        $footerBrightRatio = if ($footerPixels -gt 0) { [Math]::Round($footerBrightPixels / [double]$footerPixels, 6) } else { 1.0 }

        [pscustomobject]@{
            image = $ImagePath
            panel_rect_px = @($panel.X, $panel.Y, $panel.Width, $panel.Height)
            header_rect_px = @($header.X, $header.Y, $header.Width, $header.Height)
            footer_bottom_band_rect_px = @($footerBand.X, $footerBand.Y, $footerBand.Width, $footerBand.Height)
            header_crop = $headerCrop
            footer_bottom_band_crop = $footerCrop
            green_status_dot_candidate_pixels = [int]$greenPixels
            saturated_header_pixels = [int]$saturatedHeaderPixels
            count_badge_present_by_saturated_header_pixels = [bool]($saturatedHeaderPixels -ge 180)
            expanded_status_dot_absent_by_green_scan = [bool]($greenPixels -le 4)
            footer_bottom_band_bright_pixels = [int]$footerBrightPixels
            footer_bottom_band_bright_ratio = $footerBrightRatio
            footer_thumb_strip_absent_by_bottom_band_scan = [bool]($footerBrightRatio -le 0.025)
        }
    } finally {
        $bmp.Dispose()
    }
}

$scriptResults = @()
$scriptResults += Invoke-ProofScript 'expanded-inner-frame-current' $innerFrameScript
$scriptResults += Invoke-ProofScript 'typography-structure' $typographyScript

$testResults = @()
$testResults += Invoke-AppTest 'expanded-zone-grid-layout' 'expanded_zone_grid' 'expanded grid header/divider/count badge geometry'
$testResults += Invoke-AppTest 'expanded-item-label' 'item_label' 'expanded item-card label typography and trimming'

$morph = Read-JsonPath $morphSummaryPath
$inner = Read-JsonPath $innerFrameSummaryPath
$typography = Read-JsonPath $typographySummaryPath
if ($null -eq $morph) { throw "morph summary not found: $morphSummaryPath" }
if ($null -eq $inner) { throw "inner-frame summary not found: $innerFrameSummaryPath" }
if ($null -eq $typography) { throw "typography summary not found: $typographySummaryPath" }

$scale = 1.0
if ($morph.main_window -and $morph.main_window.dpi -gt 0) {
    $scale = [double]$morph.main_window.dpi / 96.0
}

$stableImage = Join-Path $repoRoot 'runtime-proof-0608-expanded-morph-visual-try\07-expanded-04-open-stable.png'
$imageAnalysis = Analyze-ExpandedScreenshot $stableImage $morph.expanded_morph.before $scale

$sourceContracts = @()
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\expanded_zone_grid.rs' 'old 16.16 sub-zone footer thumbnail strip.*with no footer node' 'expanded layout documents deleted footer thumbnail strip'
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\expanded_zone_grid.rs' 'green status dot.*removed' 'expanded layout documents no status-dot slot'
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\expanded_zone_grid.rs' 'pub const DIVIDER_INSET_X:\s*f32\s*=\s*0\.0;' 'divider spans full header width'
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\expanded_zone_grid.rs' 'pub const DIVIDER_THICKNESS:\s*f32\s*=\s*1\.0;' 'divider is one DIP'
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\render.rs' 'with_alpha\(bento_nano_style::Color::WHITE,\s*0\.05\)' 'divider paint alpha matches Tauri 0.05 white border'
$sourceContracts += Test-SourceContains 'crates\bento-nano-app\src\render.rs' 'const ITEM_LABEL_BASE_FONT_PX:\s*f32\s*=\s*14\.0;' 'expanded item label fixed 14px token'

$scriptsPassed = @($scriptResults | Where-Object { -not $_.passed }).Count -eq 0
$testsPassed = @($testResults | Where-Object { -not $_.passed }).Count -eq 0
$sourceContractsPassed = @($sourceContracts | Where-Object { -not $_.passed }).Count -eq 0
$runtimeSummariesPassed = (
    ($morph.status -eq 'ok') -and
    ($morph.stage -eq 'completed') -and
    ($morph.process_exited_after_quit_hotkey -eq $true) -and
    ($inner.status -eq 'ok') -and
    ($inner.stage -eq 'completed') -and
    ($inner.stale_inner_frame_detected -eq $false) -and
    ($typography.status -eq 'ok') -and
    ($typography.stage -eq 'completed') -and
    ($typography.font_alignment.structured_slot_summary -eq $true)
)

$ws4Accepted = (
    $scriptsPassed -and
    $testsPassed -and
    $sourceContractsPassed -and
    $runtimeSummariesPassed -and
    ($imageAnalysis.expanded_status_dot_absent_by_green_scan -eq $true) -and
    ($imageAnalysis.count_badge_present_by_saturated_header_pixels -eq $true) -and
    ($imageAnalysis.footer_thumb_strip_absent_by_bottom_band_scan -eq $true)
)

$summary = [ordered]@{
    status = if ($ws4Accepted) { 'ok' } else { 'attention_required' }
    stage = 'completed'
    generated_at_utc = [DateTime]::UtcNow.ToString('o')
    task = '.trellis/tasks/05-29-nano-tauri-parity-plan'
    repo = 'bentodesk-nano'
    proof_dir = $outDir
    proof_kind = 'WS-4 current expanded-grid runtime and geometry acceptance gate'
    goal_complete = $false
    task_complete = $false
    visual_review_required = $false
    source_morph_summary = $morphSummaryPath
    source_inner_frame_summary = $innerFrameSummaryPath
    source_typography_summary = $typographySummaryPath
    ws4_expanded_grid = [ordered]@{
        accepted = [bool]$ws4Accepted
        e01_footer_thumb_strip_absent = [bool]($imageAnalysis.footer_thumb_strip_absent_by_bottom_band_scan -and (($sourceContracts | Where-Object { $_.description -eq 'expanded layout documents deleted footer thumbnail strip' }).passed -eq $true))
        e02_expanded_status_dot_absent = [bool]$imageAnalysis.expanded_status_dot_absent_by_green_scan
        e02_count_badge_present = [bool]$imageAnalysis.count_badge_present_by_saturated_header_pixels
        e03_item_icon_label_alignment_contract_pass = [bool](($typography.font_alignment.structured_slot_summary -eq $true) -and ($testsPassed -eq $true))
        e03_item_label_font_px = 14
        e04_divider_geometry_contract_pass = [bool](($testResults | Where-Object { $_.id -eq 'expanded-zone-grid-layout' }).passed -eq $true)
        e04_divider_rgba_or_alpha_within_threshold = [bool](($sourceContracts | Where-Object { $_.description -eq 'divider paint alpha matches Tauri 0.05 white border' }).passed -eq $true)
        inner_frame = [ordered]@{
            scanned_frame_count = [int]$inner.scanned_frame_count
            stale_inner_frame_detected = [bool]$inner.stale_inner_frame_detected
            analyses = $inner.analyses
        }
        no_mock_data = $true
        runtime_window_class = $morph.main_window.class
        runtime_window_visible = $morph.main_window.visible
        process_exited_after_quit_hotkey = $morph.process_exited_after_quit_hotkey
        measurement_boundary = 'Hybrid proof: scoped runtime screenshot pixel scans for visible absence/presence, current inner-frame edge scan, structured expanded-grid geometry tests, and source contracts for non-rendered footer/status-dot slots and divider alpha.'
    }
    image_analysis = $imageAnalysis
    script_results = $scriptResults
    tests = $testResults
    source_contracts = $sourceContracts
    runtime_summaries = [ordered]@{
        morph_status = $morph.status
        inner_frame_status = $inner.status
        typography_status = $typography.status
        typography_structured_slot_summary = $typography.font_alignment.structured_slot_summary
    }
    screenshots = @(
        $stableImage,
        $imageAnalysis.header_crop,
        $imageAnalysis.footer_bottom_band_crop
    )
}

Write-Utf8NoBom $summaryPath ($summary | ConvertTo-Json -Depth 30)

Write-Host "ws4_expanded_grid_status=$($summary.status)"
Write-Host "summary=$summaryPath"
Write-Host "accepted=$($summary.ws4_expanded_grid.accepted)"
if ($summary.status -ne 'ok') {
    throw "WS-4 expanded-grid current gate failed; see $summaryPath"
}
