$ErrorActionPreference = 'Stop'

$root = 'D:\Desktop\CREATOR FOUR'
$nano = Join-Path $root 'bentodesk-nano'
$morphScript = Join-Path $nano 'runtime-proof-0608-expanded-morph-visual-run.ps1'
$morphDir = Join-Path $nano 'runtime-proof-0608-expanded-morph-visual-try'
$morphSummaryPath = Join-Path $morphDir 'summary.json'
$proofDir = Join-Path $nano 'runtime-proof-0617-expanded-inner-frame-current-try'
$summaryPath = Join-Path $proofDir 'summary.json'
$referenceVideo = Get-ChildItem -LiteralPath (Join-Path $root 'resource') -Filter '*.mp4' |
  Where-Object { $_.Name -like '*2026-06-02 130741.mp4' } |
  Select-Object -First 1 -ExpandProperty FullName

if (-not (Test-Path -LiteralPath $morphScript)) {
  throw "expanded morph proof script not found: $morphScript"
}

New-Item -ItemType Directory -Force -Path $proofDir | Out-Null
Get-ChildItem -LiteralPath $proofDir -File -ErrorAction SilentlyContinue | Remove-Item -Force

& powershell -NoProfile -ExecutionPolicy Bypass -File $morphScript
if ($LASTEXITCODE -ne 0) {
  throw "expanded morph proof failed with exit code $LASTEXITCODE"
}

if (-not (Test-Path -LiteralPath $morphSummaryPath)) {
  throw "expanded morph proof summary not found: $morphSummaryPath"
}

$morphSummary = Get-Content -LiteralPath $morphSummaryPath -Raw -Encoding UTF8 | ConvertFrom-Json
if ($morphSummary.status -ne 'ok' -or $morphSummary.stage -ne 'completed') {
  throw "expanded morph proof did not complete cleanly"
}
if (-not $morphSummary.expanded_morph.no_write_during_hover) {
  throw "expanded morph proof wrote state during hover"
}

Add-Type -AssemblyName System.Drawing

function Get-Luma([System.Drawing.Color]$c) {
  return [double](0.2126 * $c.R + 0.7152 * $c.G + 0.0722 * $c.B)
}

function Copy-BitmapCrop([System.Drawing.Bitmap]$source, [System.Drawing.Rectangle]$rect, [string]$path) {
  $crop = New-Object System.Drawing.Bitmap $rect.Width, $rect.Height
  $g = [System.Drawing.Graphics]::FromImage($crop)
  try {
    $g.DrawImage($source, 0, 0, $rect, [System.Drawing.GraphicsUnit]::Pixel)
    $crop.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  } finally {
    $g.Dispose()
    $crop.Dispose()
  }
}

function Analyze-InnerFrame([string]$imagePath, [string]$label, [object]$zone, [double]$scale) {
  if (-not (Test-Path -LiteralPath $imagePath)) {
    throw "frame not found: $imagePath"
  }

  $bmp = [System.Drawing.Bitmap]::new($imagePath)
  try {
    $panelLeft = [int][Math]::Round([double]$zone.x * $scale)
    $panelTop = [int][Math]::Round([double]$zone.y * $scale)
    $panelWidth = [int][Math]::Round([double]$zone.w * $scale)
    $panelHeight = [int][Math]::Round([double]$zone.h * $scale)

    $bodyLeft = $panelLeft + [int][Math]::Round(5.5 * $scale)
    $bodyTop = $panelTop + [int][Math]::Round(56.7 * $scale)
    $bodyRight = $panelLeft + $panelWidth - [int][Math]::Round(6.0 * $scale)
    $bodyBottom = $panelTop + $panelHeight - [int][Math]::Round(6.0 * $scale)

    $bodyLeft = [Math]::Max(1, [Math]::Min($bmp.Width - 2, $bodyLeft))
    $bodyTop = [Math]::Max(1, [Math]::Min($bmp.Height - 2, $bodyTop))
    $bodyRight = [Math]::Max($bodyLeft + 1, [Math]::Min($bmp.Width - 2, $bodyRight))
    $bodyBottom = [Math]::Max($bodyTop + 1, [Math]::Min($bmp.Height - 2, $bodyBottom))

    $bodyWidth = $bodyRight - $bodyLeft
    $bodyHeight = $bodyBottom - $bodyTop
    $threshold = 22.0
    $minVerticalRun = [int][Math]::Ceiling($bodyHeight * 0.70)
    $minHorizontalRun = [int][Math]::Ceiling($bodyWidth * 0.70)
    $longVerticalEdges = New-Object System.Collections.ArrayList
    $longHorizontalEdges = New-Object System.Collections.ArrayList

    for ($x = $bodyLeft; $x -lt $bodyRight; $x++) {
      $currentRun = 0
      $maxRun = 0
      for ($y = $bodyTop; $y -lt $bodyBottom; $y++) {
        $left = Get-Luma $bmp.GetPixel($x - 1, $y)
        $center = Get-Luma $bmp.GetPixel($x, $y)
        $right = Get-Luma $bmp.GetPixel($x + 1, $y)
        $contrast = [Math]::Max([Math]::Abs($center - $left), [Math]::Abs($right - $center))
        if ($contrast -ge $threshold) {
          $currentRun++
          if ($currentRun -gt $maxRun) { $maxRun = $currentRun }
        } else {
          $currentRun = 0
        }
      }
      if ($maxRun -ge $minVerticalRun) {
        [void]$longVerticalEdges.Add([ordered]@{ x = $x; longest_run = $maxRun })
      }
    }

    for ($y = $bodyTop; $y -lt $bodyBottom; $y++) {
      $currentRun = 0
      $maxRun = 0
      for ($x = $bodyLeft; $x -lt $bodyRight; $x++) {
        $top = Get-Luma $bmp.GetPixel($x, $y - 1)
        $center = Get-Luma $bmp.GetPixel($x, $y)
        $bottom = Get-Luma $bmp.GetPixel($x, $y + 1)
        $contrast = [Math]::Max([Math]::Abs($center - $top), [Math]::Abs($bottom - $center))
        if ($contrast -ge $threshold) {
          $currentRun++
          if ($currentRun -gt $maxRun) { $maxRun = $currentRun }
        } else {
          $currentRun = 0
        }
      }
      if ($maxRun -ge $minHorizontalRun) {
        [void]$longHorizontalEdges.Add([ordered]@{ y = $y; longest_run = $maxRun })
      }
    }

    $stableCopy = Join-Path $proofDir ("{0}.png" -f $label)
    Copy-Item -LiteralPath $imagePath -Destination $stableCopy -Force
    $cropPath = Join-Path $proofDir ("{0}-body-scan.png" -f $label)
    Copy-BitmapCrop $bmp ([System.Drawing.Rectangle]::new($bodyLeft, $bodyTop, $bodyWidth, $bodyHeight)) $cropPath

    $detected = (($longVerticalEdges.Count -gt 0) -or ($longHorizontalEdges.Count -gt 0))
    return [ordered]@{
      label = $label
      source_image = $imagePath
      copied_image = $stableCopy
      body_scan_crop = $cropPath
      image_width = [int]$bmp.Width
      image_height = [int]$bmp.Height
      panel_bbox_px = @($panelLeft, $panelTop, $panelWidth, $panelHeight)
      body_scan_rect_px = @($bodyLeft, $bodyTop, $bodyWidth, $bodyHeight)
      luma_edge_threshold = $threshold
      min_vertical_run_px = $minVerticalRun
      min_horizontal_run_px = $minHorizontalRun
      long_vertical_edge_columns = @($longVerticalEdges.ToArray())
      long_vertical_edge_count = [int]$longVerticalEdges.Count
      long_horizontal_edge_rows = @($longHorizontalEdges.ToArray())
      long_horizontal_edge_count = [int]$longHorizontalEdges.Count
      stale_inner_frame_detected = [bool]$detected
    }
  } finally {
    $bmp.Dispose()
  }
}

$scale = 1.0
if ($morphSummary.main_window -and $morphSummary.main_window.dpi -gt 0) {
  $scale = [double]$morphSummary.main_window.dpi / 96.0
}

$zone = $morphSummary.expanded_morph.before
if (-not $zone) {
  throw "expanded morph summary has no zone geometry"
}

$frames = @(
  @{ label = 'open-mid-230ms'; file = '06-expanded-03-open-mid-230ms.png' },
  @{ label = 'open-stable'; file = '07-expanded-04-open-stable.png' }
)

$analyses = @()
foreach ($frame in $frames) {
  $path = Join-Path $morphDir $frame.file
  $analyses += Analyze-InnerFrame $path $frame.label $zone $scale
}

$detectedCount = @($analyses | Where-Object { $_.stale_inner_frame_detected }).Count
$summary = [ordered]@{
  status = if ($detectedCount -eq 0) { 'ok' } else { 'failed' }
  stage = 'completed'
  proof_dir = $proofDir
  source_morph_summary = $morphSummaryPath
  reference_video = $referenceVideo
  source_exe = $morphSummary.source_exe
  state_dir = $morphSummary.state_dir
  main_window = $morphSummary.main_window
  expanded_morph = $morphSummary.expanded_morph
  dpi_scale = $scale
  scanned_frame_count = [int]$analyses.Count
  stale_inner_frame_detected = [bool]($detectedCount -gt 0)
  analyses = $analyses
  process_exited_after_quit_hotkey = $morphSummary.process_exited_after_quit_hotkey
}

$summary | ConvertTo-Json -Depth 12 | Out-File -FilePath $summaryPath -Encoding utf8
if ($summary.status -ne 'ok') {
  throw "current expanded inner-frame scan failed; see $summaryPath"
}
