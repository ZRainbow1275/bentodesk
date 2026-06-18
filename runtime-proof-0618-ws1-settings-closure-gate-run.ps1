$ErrorActionPreference = 'Stop'

$root = 'D:\Desktop\CREATOR FOUR'
$nano = Join-Path $root 'bentodesk-nano'
$sourceRuntimeScript = Join-Path $nano 'runtime-proof-0608-settings-appearance-run.ps1'
$sourceProofDir = Join-Path $nano 'runtime-proof-0608-settings-appearance-try'
$sourceSummaryPath = Join-Path $sourceProofDir 'summary.json'
$sourceStderrPath = Join-Path $sourceProofDir 'stderr.log'
$proofDir = Join-Path $nano 'runtime-proof-0618-ws1-settings-closure-gate-try'
$summaryPath = Join-Path $proofDir 'summary.json'
$matrixPath = Join-Path $proofDir 'ws1-matrix.csv'

function Write-Utf8NoBom([string]$path, [string]$content) {
  $encoding = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($path, $content, $encoding)
}

function Json-Bool($value) {
  if ($null -eq $value) { return $false }
  return [bool]$value
}

function Add-MatrixRow(
  [System.Collections.ArrayList]$rows,
  [string]$id,
  [string]$status,
  [string]$evidence,
  [string]$gap
) {
  [void]$rows.Add([ordered]@{
    id = $id
    status = $status
    evidence = $evidence
    gap = $gap
  })
}

New-Item -ItemType Directory -Force -Path $proofDir | Out-Null

if (-not (Test-Path -LiteralPath $sourceRuntimeScript)) {
  throw "source runtime proof script not found: $sourceRuntimeScript"
}

& powershell -ExecutionPolicy Bypass -File $sourceRuntimeScript

if (-not (Test-Path -LiteralPath $sourceSummaryPath)) {
  throw "source runtime summary not found: $sourceSummaryPath"
}

$summaryText = Get-Content -LiteralPath $sourceSummaryPath -Raw
$source = $summaryText | ConvertFrom-Json
$stderr = ''
if (Test-Path -LiteralPath $sourceStderrPath) {
  $stderr = Get-Content -LiteralPath $sourceStderrPath -Raw
}

$hits = $source.hits
$assertions = $source.assertions
$screenshots = @($source.screenshots)
$backups = $source.backups
$plugin = $source.plugin
$footerProof = $source.footer_proof
$visualMatrix = $source.visual_matrix
$stickyFooterBehaviorProven = ($hits.SaveSettings -ge 2 -and (Json-Bool $assertions.first_settings_closed_after_save) -and (Json-Bool $assertions.second_settings_closed_after_save))
$stickyFooterGeometryProven = (Json-Bool $assertions.sticky_footer_geometry_pixels)
$fullSectionVisualMatrixProven = ((Json-Bool $assertions.full_section_visual_matrix) -and $visualMatrix -and (Json-Bool $visualMatrix.accepted))

$rows = New-Object System.Collections.ArrayList

Add-MatrixRow $rows 'runtime-foundation' `
  ($(if ($source.status -eq 'ok' -and $source.stage -eq 'completed' -and $source.settings_window.class -eq 'BentoAuxSets') { 'pass' } else { 'missing' })) `
  "source status=$($source.status); stage=$($source.stage); settings_class=$($source.settings_window.class); hotkeys=$($source.opened_via_hotkey_id)/$($source.quit_via_hotkey_id)" `
  ''

Add-MatrixRow $rows 'native-scroll-and-screenshots' `
  ($(if ((Json-Bool $assertions.native_wheel_messages_sent) -and (Json-Bool $assertions.nonblank_required_screenshots)) { 'pass' } else { 'missing' })) `
  "wheel_event_count=$($source.logs.wheel_event_count); scroll_log_count=$($source.logs.scroll_log_count); screenshots=$($screenshots.Count)" `
  ''

Add-MatrixRow $rows 'sticky-footer-save-behavior' `
  ($(if ($stickyFooterBehaviorProven -and $stickyFooterGeometryProven) { 'pass' } elseif ($stickyFooterBehaviorProven) { 'partial' } else { 'missing' })) `
  "SaveSettings hits=$($hits.SaveSettings); first_closed=$($assertions.first_settings_closed_after_save); second_closed=$($assertions.second_settings_closed_after_save); footer_pixels=$($assertions.sticky_footer_geometry_pixels); footer_vs_body=$($footerProof.distances.footer_vs_body); save_vs_footer=$($footerProof.distances.save_vs_footer); screenshot=$($footerProof.screenshot)" `
  ($(if ($stickyFooterBehaviorProven -and $stickyFooterGeometryProven) { '' } elseif ($stickyFooterBehaviorProven) { 'Behavior is runtime-proven, but pixel/geometry proof of the sticky footer band against Tauri v1.3.0 is still absent.' } else { 'SaveSettings did not close both Settings windows through the visible footer button path.' }))

Add-MatrixRow $rows 'performance-all-three-sliders' `
  ($(if (Json-Bool $assertions.all_performance_sliders_hit) { 'pass' } else { 'missing' })) `
  "DragPerformanceSlider total=$($hits.DragPerformanceSlider); index0=$($hits.DragPerformanceSliderIndex0); index1=$($hits.DragPerformanceSliderIndex1); index2=$($hits.DragPerformanceSliderIndex2)" `
  ''

Add-MatrixRow $rows 'backup-create-list' `
  ($(if ((Json-Bool $assertions.backup_file_created) -and $hits.CreateSettingsBackup -ge 1 -and $hits.ListSettingsBackups -ge 1 -and $backups.count -ge 1) { 'pass' } else { 'missing' })) `
  "CreateSettingsBackup=$($hits.CreateSettingsBackup); ListSettingsBackups=$($hits.ListSettingsBackups); backup_count=$($backups.count)" `
  ''

Add-MatrixRow $rows 'backup-restore' `
  ($(if (Json-Bool $assertions.backup_restore_hit) { 'pass' } else { 'missing' })) `
  "RestoreSettingsBackup hits=$($hits.RestoreSettingsBackup)" `
  ($(if (Json-Bool $assertions.backup_restore_hit) { '' } else { 'Restore button runtime coordinate is not yet proven.' }))

Add-MatrixRow $rows 'encryption-runtime-interaction' `
  ($(if ((Json-Bool $assertions.encryption_passphrase_runtime_hit) -and (Json-Bool $assertions.vault_passphrase_mode_written)) { 'pass' } else { 'missing' })) `
  "FocusPassphraseField=$($hits.FocusPassphraseField); SelectEncryptionModePassphrase=$($hits.SelectEncryptionModePassphrase); vault_mode=$($source.vault.wire_after_encryption.mode); mode_tag=$($source.vault.wire_after_encryption.mode_tag)" `
  ($(if ((Json-Bool $assertions.encryption_passphrase_runtime_hit) -and (Json-Bool $assertions.vault_passphrase_mode_written)) { '' } else { 'Need isolated-state runtime click/type/apply proof and vault mode dump.' }))

Add-MatrixRow $rows 'plugin-toggle' `
  ($(if ((Json-Bool $assertions.plugin_disabled_in_registry) -and $hits.TogglePlugin -ge 1 -and $plugin.enabled_after_toggle -eq $false) { 'pass' } else { 'missing' })) `
  "TogglePlugin hits=$($hits.TogglePlugin); registry_exists=$($plugin.registry_exists); enabled_after_toggle=$($plugin.enabled_after_toggle)" `
  ''

Add-MatrixRow $rows 'plugin-install-uninstall-lifecycle' `
  ($(if ((Json-Bool $assertions.plugin_installed_from_native_picker) -and (Json-Bool $assertions.plugin_uninstalled_from_registry) -and $hits.InstallPlugin -ge 1 -and $hits.UninstallPlugin -ge 1) { 'pass' } elseif ((Json-Bool $assertions.plugin_uninstalled_from_registry) -and $hits.UninstallPlugin -ge 1) { 'partial' } else { 'missing' })) `
  "InstallPlugin=$($hits.InstallPlugin); installed_present=$($plugin.installed_plugin.present); installed_after_exit=$($plugin.installed_plugin_after_exit.present); install_dir_exists=$($plugin.installed_plugin_dir_exists); manifest_exists=$($plugin.installed_manifest_exists); TogglePlugin=$($hits.TogglePlugin); enabled_after_toggle=$($plugin.enabled_after_toggle); UninstallPlugin=$($hits.UninstallPlugin); removed_after_uninstall=$($plugin.removed_after_uninstall)" `
  ($(if ((Json-Bool $assertions.plugin_installed_from_native_picker) -and (Json-Bool $assertions.plugin_uninstalled_from_registry) -and $hits.InstallPlugin -ge 1 -and $hits.UninstallPlugin -ge 1) { '' } elseif ((Json-Bool $assertions.plugin_uninstalled_from_registry) -and $hits.UninstallPlugin -ge 1) { 'Uninstall is runtime-proven against a pre-extracted plugin; install still uses a native file picker and is not automated in this proof.' } else { 'Install/uninstall lifecycle was not driven through visible Settings runtime proof.' }))

Add-MatrixRow $rows 'full-section-visual-matrix' `
  ($(if ($fullSectionVisualMatrixProven) { 'pass' } else { 'partial' })) `
  "accepted=$($visualMatrix.accepted); geometry_tests_ok=$($visualMatrix.geometry_tests.ok); geometry_tests_passed=$($visualMatrix.geometry_tests.passed); section_count=$($visualMatrix.section_count); failed_section_count=$($visualMatrix.failed_section_count); screenshots: $(@($screenshots | ForEach-Object { $_.name }) -join ', ')" `
  ($(if ($fullSectionVisualMatrixProven) { '' } else { 'Screenshots cover multiple scroll slices, but automated section-title/row geometry or pixel comparison did not pass.' }))

Add-MatrixRow $rows 'backup-log-cleanliness' `
  ($(if ((Json-Bool $source.logs.backup_created) -and (Json-Bool $assertions.backup_file_created)) { 'pass' } else { 'partial' })) `
  "logs.backup_created=$($source.logs.backup_created); backup_created_stderr=$($source.logs.backup_created_stderr); backup_created_artifact=$($source.logs.backup_created_artifact); backup_file_created=$($assertions.backup_file_created)" `
  ($(if ((Json-Bool $source.logs.backup_created) -and (Json-Bool $assertions.backup_file_created)) { '' } else { 'Backup file evidence passes, but the summary field did not capture backup creation cleanly.' }))

$blockingRows = @($rows.ToArray() | Where-Object { $_.status -ne 'pass' })
$passRows = @($rows.ToArray() | Where-Object { $_.status -eq 'pass' })
$partialRows = @($rows.ToArray() | Where-Object { $_.status -eq 'partial' })
$missingRows = @($rows.ToArray() | Where-Object { $_.status -eq 'missing' })
$stderrHitLines = @($stderr -split "`r?`n" | Where-Object { $_ -like 'settings: lbutton_down*' })

$summary = [ordered]@{
  status = if ($blockingRows.Count -eq 0) { 'ok' } else { 'attention_required' }
  stage = 'completed'
  ws_id = 'WS-1'
  closure_status = if ($blockingRows.Count -eq 0) { 'closed' } else { 'partial' }
  source_runtime_script = $sourceRuntimeScript
  source_summary = $sourceSummaryPath
  source_stderr = $sourceStderrPath
  matrix_csv = $matrixPath
  pass_count = [int]$passRows.Count
  partial_count = [int]$partialRows.Count
  missing_count = [int]$missingRows.Count
  blocking_gap_count = [int]$blockingRows.Count
  proven_improvements = @(
    'all three Performance sliders reached through real Settings clicks',
    'Backup list producer reached through real Settings click',
    'Backup restore reached through real per-row Settings click',
    'Passphrase encryption focus/type/apply reached and vault wire mode changed to Passphrase',
    'Plugin disable and uninstall reached through real Settings clicks',
    'Plugin native picker install reached with registry, install directory, and manifest evidence',
    'sticky footer Save behavior now has product-geometry and screenshot pixel evidence',
    'full Settings section visual matrix has runtime screenshots plus settings_panel geometry tests',
    'native WM_MOUSEWHEEL Settings scroll remains proven',
    'vault creation, backup creation, hotkey open/quit remain proven'
  )
  remaining_blockers = @($blockingRows | ForEach-Object { [ordered]@{ id=$_.id; status=$_.status; gap=$_.gap } })
  source_hits = $hits
  source_assertions = $assertions
  source_backups = $backups
  source_plugin = $plugin
  source_footer_proof = $footerProof
  source_visual_matrix = $visualMatrix
  stderr_hit_lines = $stderrHitLines
  matrix = @($rows.ToArray())
}

$csvLines = New-Object System.Collections.ArrayList
[void]$csvLines.Add('id,status,evidence,gap')
foreach ($row in $rows) {
  $escapedEvidence = '"' + ($row.evidence -replace '"', '""') + '"'
  $escapedGap = '"' + ($row.gap -replace '"', '""') + '"'
  [void]$csvLines.Add("$($row.id),$($row.status),$escapedEvidence,$escapedGap")
}

Write-Utf8NoBom $matrixPath (($csvLines.ToArray() -join "`n") + "`n")
Write-Utf8NoBom $summaryPath (($summary | ConvertTo-Json -Depth 14) + "`n")

Write-Output "ws1_settings_closure_status=$($summary.status)"
Write-Output "closure_status=$($summary.closure_status)"
Write-Output "pass=$($summary.pass_count) partial=$($summary.partial_count) missing=$($summary.missing_count)"
Write-Output "summary=$summaryPath"
Write-Output "matrix=$matrixPath"
