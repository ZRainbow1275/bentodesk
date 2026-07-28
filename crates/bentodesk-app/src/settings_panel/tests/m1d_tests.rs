//! M1d 2026-05-29 — Performance §5 + Startup management §6 geometry.
//! Replaces the deleted m3 advanced/overlay tests.
use super::*;

fn vp() -> Size {
    Size {
        width: 800.0,
        height: 600.0,
    }
}

#[test]
fn perf_label_sits_below_m2_textarea() {
    // The Performance §5 label roots at the FIXED 4-card reserve baseline
    // (scroll 0, no reflow delta), so it must clear the §2 watch textarea
    // computed at the same full reserve (count = cap). G3 parity
    // (2026-06-01): §3 Appearance + §4 DisplayMode now sit BETWEEN §2 Paths
    // and §5 Performance, so the perf label clears the textarea by even more
    // than pre-G3 (the `>=` still holds — and now with extra slack).
    let v = vp();
    let textarea = settings_watch_textarea_rect(v, 0.0, SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
    let label = settings_performance_label_rect(v, 0.0);
    assert!(label.y >= textarea.bottom());
    // The §4 DisplayMode picker (the section directly above Performance)
    // must end at-or-above the perf label — pin the new adjacency.
    let picker = settings_zone_display_mode_picker_row_rect(v, 0.0);
    assert!(
        label.y >= picker.bottom(),
        "Performance §5 label (y={}) must sit below the §4 DisplayMode \
             picker row (bottom={})",
        label.y,
        picker.bottom(),
    );
}

#[test]
fn m1i_perf_reflows_via_reserve_delta() {
    // M1i fidelity — folding the reserve delta into scroll shifts the
    // Performance label (and everything below it) UP by exactly the delta,
    // proving the single-base-offset reflow reaches the lower sections.
    let v = vp();
    let base = settings_performance_label_rect(v, 0.0);
    let delta = settings_sources_reserve_delta(2);
    let reflowed = settings_performance_label_rect(v, delta);
    assert!(delta > 0.0);
    assert!((base.y - reflowed.y - delta).abs() < 0.01);
}

#[test]
fn perf_slider_rows_stack_vertically() {
    let v = vp();
    let r0 = settings_performance_slider_row_rect(v, 0.0, 0);
    let r1 = settings_performance_slider_row_rect(v, 0.0, 1);
    let r2 = settings_performance_slider_row_rect(v, 0.0, 2);
    assert!((r1.y - r0.bottom()).abs() < 0.01);
    assert!((r2.y - r1.bottom()).abs() < 0.01);
    assert_eq!(r0.height, SETTINGS_SLIDER_ROW_H);
}

#[test]
fn perf_slider_track_sits_on_lower_line_full_width() {
    let v = vp();
    for index in 0..SETTINGS_PERF_ROW_COUNT {
        let row = settings_performance_slider_row_rect(v, 0.0, index);
        let track = settings_performance_slider_rect(v, 0.0, index);
        // Track on the lower line (below the label/value line).
        assert!(track.y > row.y + row.height * 0.4);
        assert!(track.bottom() <= row.bottom() + 0.01);
        assert!((track.x - row.x).abs() < 0.01);
        assert!((track.width - row.width).abs() < 0.01);
    }
}

#[test]
fn perf_row_count_pinned() {
    assert_eq!(SETTINGS_PERF_ROW_COUNT, 3);
}

#[test]
fn startup_label_sits_below_performance_section() {
    let v = vp();
    let last_perf = settings_performance_slider_row_rect(v, 0.0, SETTINGS_PERF_ROW_COUNT - 1);
    let startup = settings_startup_label_rect(v, 0.0);
    assert!(startup.y >= last_perf.bottom() + SETTINGS_SECTION_GAP - 0.01);
}

#[test]
fn startup_always_rows_stack_with_desc_gaps() {
    let v = vp();
    let label = settings_startup_label_rect(v, 0.0);
    let high = settings_startup_high_priority_row_rect(v, 0.0);
    let crash = settings_crash_restart_row_rect(v, 0.0);
    assert!((high.y - label.bottom()).abs() < 0.01);
    // crash row sits a full row + a desc-line below high priority.
    assert!((crash.y - (high.bottom() + SETTINGS_DESC_H)).abs() < 0.01);
}

#[test]
fn startup_crash_steppers_only_chain_when_enabled() {
    let v = vp();
    let retries = settings_crash_max_retries_row_rect(v, 0.0);
    let window = settings_crash_window_row_rect(v, 0.0);
    // window stepper sits directly below the retries stepper.
    assert!((window.y - retries.bottom()).abs() < 0.01);
    // Steppers' − value + pack right-to-left.
    let plus = settings_stepper_plus_rect(retries);
    let value = settings_stepper_value_rect(retries);
    let minus = settings_stepper_minus_rect(retries);
    let input = settings_stepper_input_rect(retries);
    assert!(minus.right() <= value.x + 0.01);
    assert!(value.right() <= plus.x + 0.01);
    assert!(plus.right() <= retries.right() + 0.01);
    assert_eq!(plus.width, SETTINGS_NUM_BTN_W);
    assert_eq!(value.width, SETTINGS_NUM_VALUE_W);
    assert_eq!(input.width, 72.0);
    assert_eq!(input.height, 30.0);
    assert_eq!(input.x, minus.x);
    assert_eq!(input.right(), plus.right());
    assert_eq!(input.right(), retries.right());
}

#[test]
fn safe_start_row_reflows_with_crash_restart_flag() {
    let v = vp();
    let off = settings_safe_start_row_rect(v, 0.0, false);
    let on = settings_safe_start_row_rect(v, 0.0, true);
    // Net effect of showing the two crash steppers is +2 stepper rows: the
    // crash-restart desc-clearing gap (SETTINGS_DESC_H) is present in BOTH
    // branches (OFF adds it directly; ON spends it on the retries-row gap),
    // so it cancels and the delta is exactly two row heights.
    assert!(on.y > off.y);
    assert!((on.y - off.y - SETTINGS_ROW_H_M1 * 2.0).abs() < 0.01);
}

#[test]
fn hibernate_slider_sits_below_safe_start_when_shown() {
    let v = vp();
    let safe = settings_safe_start_row_rect(v, 0.0, true);
    let slider_row = settings_hibernate_slider_row_rect(v, 0.0, true);
    assert!((slider_row.y - (safe.bottom() + SETTINGS_DESC_H)).abs() < 0.01);
    assert_eq!(slider_row.height, SETTINGS_SLIDER_ROW_H);
    let track = settings_hibernate_slider_rect(v, 0.0, true);
    assert!(track.bottom() <= slider_row.bottom() + 0.01);
}

#[test]
fn content_height_grows_with_conditional_rows() {
    let v = vp();
    // Both gates off → shortest. Crash on → +2 stepper rows. Hibernate on
    // → + slider row + desc. All on → tallest.
    let k = UpdaterHeightKind::StatusOnly;
    let none =
        settings_body_content_height(v, &SettingsBodyFlags::new(false, false, false, false, k));
    let crash =
        settings_body_content_height(v, &SettingsBodyFlags::new(true, false, false, false, k));
    let hib =
        settings_body_content_height(v, &SettingsBodyFlags::new(false, true, false, false, k));
    let both =
        settings_body_content_height(v, &SettingsBodyFlags::new(true, true, false, false, k));
    assert!(crash > none, "crash steppers must add height");
    assert!(hib > none, "hibernate slider must add height");
    assert!(both > crash);
    assert!(both > hib);
    // Crash adds a net 2 stepper rows (the desc-clearing gap cancels
    // between the two branches — see safe_start_row_reflows test).
    assert!((crash - none - SETTINGS_ROW_H_M1 * 2.0).abs() < 0.01);
}

#[test]
fn content_height_exceeds_m2_total() {
    let v = vp();
    // `SettingsBodyFlags::new` defaults source_row_count to 0, so measure
    // the M2 block at the same count for an apples-to-apples comparison.
    let m2 = settings_m2_content_height(v, 0);
    let total = settings_body_content_height(
        v,
        &SettingsBodyFlags::new(true, true, false, false, UpdaterHeightKind::StatusOnly),
    );
    assert!(total > m2);
}

#[test]
fn scroll_offset_shifts_performance_label_up() {
    let v = vp();
    let r_at_0 = settings_performance_label_rect(v, 0.0);
    let r_at_50 = settings_performance_label_rect(v, 50.0);
    assert!((r_at_50.y + 50.0 - r_at_0.y).abs() < 0.01);
}

// ── M1e — Stealth §7 geometry ──────────────────────────────────────

#[test]
fn m1e_stealth_title_sits_below_startup_section() {
    let v = vp();
    // With both Startup gates on, the Stealth title must clear the
    // hibernate slider row (the lowest Startup element).
    let startup_bottom = settings_hibernate_slider_row_rect(v, 0.0, true).bottom();
    let title = settings_stealth_label_rect(v, 0.0, true, true);
    assert!(
        (title.y - (startup_bottom + SETTINGS_SECTION_GAP)).abs() < 0.01,
        "stealth title must start a section gap below the last Startup row \
             (startup_bottom={}, title.y={})",
        startup_bottom,
        title.y,
    );
    assert_eq!(title.height, SETTINGS_SECTION_LABEL_H);
}

#[test]
fn m1e_stealth_base_rows_stack_in_order() {
    let v = vp();
    let title = settings_stealth_label_rect(v, 0.0, true, true);
    let status = settings_stealth_status_row_rect(v, 0.0, true, true);
    let schema = settings_stealth_schema_row_rect(v, 0.0, true, true);
    let mirror = settings_stealth_mirror_row_rect(v, 0.0, true, true);
    assert!((status.y - title.bottom()).abs() < 0.01);
    assert!((schema.y - status.bottom()).abs() < 0.01);
    assert!((mirror.y - schema.bottom()).abs() < 0.01);
    // The status pill right-anchors inside the status row.
    let pill = settings_stealth_pill_rect(status);
    assert!(pill.right() <= status.right() + 0.01);
    assert!(pill.x >= status.x + status.width * 0.5);
    assert_eq!(pill.width, SETTINGS_STEALTH_PILL_W);
}

#[test]
fn m1e_error_block_reflows_when_retry_row_present() {
    let v = vp();
    // Without a retry row, the error block hangs off the mirror row.
    let mirror = settings_stealth_mirror_row_rect(v, 0.0, true, true);
    let err_no_retry = settings_stealth_error_block_rect(v, 0.0, true, true, false);
    assert!((err_no_retry.y - mirror.bottom()).abs() < 0.01);
    // With a retry row, the error block sits a full retry row lower.
    let retry = settings_stealth_retry_row_rect(v, 0.0, true, true);
    let err_with_retry = settings_stealth_error_block_rect(v, 0.0, true, true, true);
    assert!((err_with_retry.y - retry.bottom()).abs() < 0.01);
    assert!(err_with_retry.y > err_no_retry.y);
}

#[test]
fn m1e_buttons_paired_refresh_left_reapply_right() {
    let v = vp();
    let row = settings_stealth_buttons_row_rect(v, 0.0, true, true, false, false);
    let refresh = settings_stealth_refresh_button_rect(row);
    let reapply = settings_stealth_reapply_button_rect(row);
    assert_eq!(refresh.y, reapply.y);
    assert_eq!(refresh.width, reapply.width);
    assert!(refresh.right() < reapply.x);
    assert!(reapply.right() <= row.right() + 0.01);
}

#[test]
fn m1e_onedrive_block_only_below_buttons() {
    let v = vp();
    let buttons = settings_stealth_buttons_row_rect(v, 0.0, true, true, true, false);
    let onedrive = settings_stealth_onedrive_block_rect(v, 0.0, true, true, true, false);
    assert!(onedrive.y > buttons.bottom());
    assert_eq!(onedrive.height, SETTINGS_STEALTH_ONEDRIVE_H);
}

#[test]
fn m1e_stealth_content_height_grows_with_retry_and_error() {
    // Pure additive helper — base < +retry, base < +error, both tallest.
    let base = settings_stealth_content_height(false, false);
    let retry = settings_stealth_content_height(true, false);
    let error = settings_stealth_content_height(false, true);
    let both = settings_stealth_content_height(true, true);
    assert!(retry > base, "retry row + OneDrive block must add height");
    assert!(error > base, "last-error block must add height");
    assert!(both > retry);
    assert!(both > error);
    // The error branch adds exactly the error block height.
    assert!(
        (error - base - SETTINGS_STEALTH_ERROR_BLOCK_H).abs() < 0.01,
        "error-only delta must equal the error block height",
    );
    // The retry branch adds a retry row + the OneDrive block (+ its gap).
    assert!(
        (retry - base - SETTINGS_STEALTH_ROW_H - 8.0 - SETTINGS_STEALTH_ONEDRIVE_H).abs() < 0.01,
        "retry-only delta must equal retry row + OneDrive block + gap",
    );
}

#[test]
fn m1e_body_content_height_includes_stealth() {
    let v = vp();
    // The full body height with stealth conditionals on must exceed the
    // height with them off (the Stealth card grows).
    let k = UpdaterHeightKind::StatusOnly;
    let off = settings_body_content_height(v, &SettingsBodyFlags::new(true, true, false, false, k));
    let on = settings_body_content_height(v, &SettingsBodyFlags::new(true, true, true, true, k));
    assert!(on > off, "stealth retry+error rows must grow the body");
}

#[test]
fn m1e_clamp_scroll_honours_stealth_flags() {
    let v = vp();
    let k = UpdaterHeightKind::StatusOnly;
    let f_off = SettingsBodyFlags::new(true, true, false, false, k);
    let f_on = SettingsBodyFlags::new(true, true, true, true, k);
    // Taller content (stealth rows on) ⇒ a larger max-scroll clamp.
    let max_off = settings_body_max_scroll(settings_body_content_height(v, &f_off), v);
    let max_on = settings_body_max_scroll(settings_body_content_height(v, &f_on), v);
    let clamped_off = settings_clamp_scroll(0.0, 99999.0, v, &f_off);
    let clamped_on = settings_clamp_scroll(0.0, 99999.0, v, &f_on);
    assert!((clamped_off - max_off).abs() < 0.01);
    assert!((clamped_on - max_on).abs() < 0.01);
    assert!(clamped_on >= clamped_off);
}

// ── M1f — Updater §8 geometry ──────────────────────────────────────

/// All five flag combos used by M1f tests share the both-startup-gates-on
/// baseline (matches the M1e tests) so the Updater section sits at a stable
/// Y; only `updater_kind` varies.
fn flags(kind: UpdaterHeightKind) -> SettingsBodyFlags {
    SettingsBodyFlags::new(true, true, false, false, kind)
}

#[test]
fn m1f_updater_title_sits_below_stealth_section() {
    let v = vp();
    // With no stealth retry, the Stealth section ends at its buttons row.
    let stealth_bottom =
        settings_stealth_buttons_row_rect(v, 0.0, true, true, false, false).bottom();
    let title = settings_updater_label_rect(v, 0.0, true, true, false, false);
    assert!(
        (title.y - (stealth_bottom + SETTINGS_SECTION_GAP)).abs() < 0.01,
        "updater title must start a section gap below the last Stealth row \
             (stealth_bottom={}, title.y={})",
        stealth_bottom,
        title.y,
    );
    assert_eq!(title.height, SETTINGS_SECTION_LABEL_H);
}

#[test]
fn m1f_updater_title_reflows_when_stealth_retry_present() {
    let v = vp();
    // A stealth retry adds the OneDrive block, pushing the updater title
    // lower. Updater kind is irrelevant to the title Y.
    let no_retry = settings_updater_label_rect(v, 0.0, true, true, false, false);
    let with_retry = settings_updater_label_rect(v, 0.0, true, true, true, false);
    assert!(with_retry.y > no_retry.y);
}

#[test]
fn m1f_status_row_and_pill_anchor() {
    let v = vp();
    let f = flags(UpdaterHeightKind::StatusOnly);
    let title = settings_updater_label_rect(v, 0.0, true, true, false, false);
    let status = settings_updater_status_row_rect(v, 0.0, &f);
    assert!((status.y - title.bottom()).abs() < 0.01);
    let pill = settings_updater_pill_rect(status);
    assert!(pill.right() <= status.right() + 0.01);
    assert!(pill.x >= status.x + status.width * 0.5);
    assert_eq!(pill.width, SETTINGS_UPDATER_PILL_W);
}

#[test]
fn m1f_middle_block_height_tracks_status_family() {
    let v = vp();
    let status_only =
        settings_updater_middle_block_rect(v, 0.0, &flags(UpdaterHeightKind::StatusOnly));
    let versioned =
        settings_updater_middle_block_rect(v, 0.0, &flags(UpdaterHeightKind::Versioned));
    let downloading =
        settings_updater_middle_block_rect(v, 0.0, &flags(UpdaterHeightKind::Downloading));
    let error = settings_updater_middle_block_rect(v, 0.0, &flags(UpdaterHeightKind::Error));
    assert_eq!(status_only.height, 0.0);
    assert_eq!(versioned.height, SETTINGS_UPDATER_ROW_H);
    assert_eq!(downloading.height, SETTINGS_UPDATER_PROGRESS_H);
    assert_eq!(error.height, SETTINGS_UPDATER_ERROR_H);
    // The progress track sits inside the downloading block, full width.
    let track =
        settings_updater_progress_track_rect(v, 0.0, &flags(UpdaterHeightKind::Downloading));
    assert!(track.y >= downloading.y);
    assert!(track.bottom() <= downloading.bottom() + 0.01);
    assert!((track.width - downloading.width).abs() < 0.01);
    assert_eq!(track.height, SETTINGS_UPDATER_PROGRESS_TRACK_H);
}

#[test]
fn m1f_buttons_left_pack_in_column_order() {
    let v = vp();
    let row = settings_updater_buttons_row_rect(v, 0.0, &flags(UpdaterHeightKind::Versioned));
    let b0 = settings_updater_button_rect(row, 0);
    let b1 = settings_updater_button_rect(row, 1);
    let b2 = settings_updater_button_rect(row, 2);
    assert_eq!(b0.y, b1.y);
    assert!(b0.right() <= b1.x + 0.01);
    assert!(b1.right() <= b2.x + 0.01);
    assert!((b0.x - row.x).abs() < 0.01);
    assert_eq!(b0.width, SETTINGS_UPDATER_BTN_W);
}

#[test]
fn m1f_buttons_row_reflows_with_middle_block() {
    let v = vp();
    // The buttons row sits lower when a middle block is present.
    let no_block = settings_updater_buttons_row_rect(v, 0.0, &flags(UpdaterHeightKind::StatusOnly));
    let with_progress =
        settings_updater_buttons_row_rect(v, 0.0, &flags(UpdaterHeightKind::Downloading));
    assert!(with_progress.y > no_block.y);
    assert!((with_progress.y - no_block.y - SETTINGS_UPDATER_PROGRESS_H).abs() < 0.01);
}

#[test]
fn m1f_prefs_rows_stack_below_buttons() {
    let v = vp();
    let f = flags(UpdaterHeightKind::StatusOnly);
    let buttons = settings_updater_buttons_row_rect(v, 0.0, &f);
    let freq = settings_updater_frequency_row_rect(v, 0.0, &f);
    let auto = settings_updater_auto_download_row_rect(v, 0.0, &f);
    assert!(freq.y >= buttons.bottom());
    assert!((auto.y - freq.bottom()).abs() < 0.01);
    // Chip right-anchors in the frequency row; toggle hit right-anchors in
    // the auto-download row.
    let chip = settings_updater_frequency_chip_rect(freq);
    assert!((chip.right() - freq.right()).abs() < 0.01);
    let hit = settings_updater_auto_download_hit_rect(auto);
    assert!((hit.right() - auto.right()).abs() < 0.01);
    assert_eq!(hit.width, SETTINGS_TOP_TOGGLE_HIT_W);
}

#[test]
fn m1f_content_height_tracks_status_family() {
    let status_only = settings_updater_content_height(UpdaterHeightKind::StatusOnly);
    let versioned = settings_updater_content_height(UpdaterHeightKind::Versioned);
    let downloading = settings_updater_content_height(UpdaterHeightKind::Downloading);
    let error = settings_updater_content_height(UpdaterHeightKind::Error);
    assert!(versioned > status_only);
    assert!(downloading > status_only);
    assert!(error > status_only);
    // Each family adds exactly its middle-block height over StatusOnly.
    assert!((versioned - status_only - SETTINGS_UPDATER_ROW_H).abs() < 0.01);
    assert!((downloading - status_only - SETTINGS_UPDATER_PROGRESS_H).abs() < 0.01);
    assert!((error - status_only - SETTINGS_UPDATER_ERROR_H).abs() < 0.01);
}

#[test]
fn m1f_body_content_height_includes_updater() {
    let v = vp();
    // Body height with the updater downloading (progress block) must exceed
    // the idle (status-only) height — proving the updater feeds the body.
    let idle = settings_body_content_height(v, &flags(UpdaterHeightKind::StatusOnly));
    let dl = settings_body_content_height(v, &flags(UpdaterHeightKind::Downloading));
    assert!(dl > idle, "updater progress block must grow the body");
}

#[test]
fn m1f_flags_round_trip_through_height_fn() {
    let v = vp();
    // The Copy struct's fields drive the same height as the equivalent
    // legacy-style bools would: build two flag sets that differ only in
    // updater_kind and confirm the delta equals the middle-block delta.
    let a = SettingsBodyFlags::new(true, false, true, false, UpdaterHeightKind::StatusOnly);
    let b = SettingsBodyFlags::new(true, false, true, false, UpdaterHeightKind::Error);
    let ha = settings_body_content_height(v, &a);
    let hb = settings_body_content_height(v, &b);
    assert!((hb - ha - SETTINGS_UPDATER_ERROR_H).abs() < 0.01);
    // Round-trip the struct itself (Copy + Eq).
    let c = a;
    assert_eq!(a, c);
    assert_ne!(a, b);
}

// ── M1g — Backup §9 geometry ───────────────────────────────────────

/// Backup flag baseline: both startup gates on (stable Updater Y) + the
/// updater idle (StatusOnly) so only the backup row count varies.
fn backup_flags(backup_rows: usize) -> SettingsBodyFlags {
    SettingsBodyFlags::new(true, true, false, false, UpdaterHeightKind::StatusOnly)
        .with_backup_rows(backup_rows)
}

#[test]
fn m1g_backup_title_sits_below_updater_section() {
    let v = vp();
    let f = backup_flags(0);
    let updater_bottom = settings_updater_auto_download_row_rect(v, 0.0, &f).bottom();
    let title = settings_backup_label_rect(v, 0.0, &f);
    // Title clears the Updater section by exactly the section gap.
    assert!((title.y - updater_bottom - SETTINGS_SECTION_GAP).abs() < 0.01);
    assert_eq!(title.height, SETTINGS_SECTION_LABEL_H);
}

#[test]
fn m1g_rows_stack_in_order_title_desc_actions_status_list() {
    let v = vp();
    let f = backup_flags(2).with_backup_status(true);
    let title = settings_backup_label_rect(v, 0.0, &f);
    let desc = settings_backup_description_rect(v, 0.0, &f);
    let actions = settings_backup_actions_row_rect(v, 0.0, &f);
    let status = settings_backup_status_rect(v, 0.0, &f);
    let entry0 = settings_backup_entry_row_rect(v, 0.0, &f, 0);
    assert!((desc.y - title.bottom()).abs() < 0.01);
    assert!((actions.y - desc.bottom()).abs() < 0.01);
    assert!(status.y >= actions.bottom());
    assert!(entry0.y >= status.bottom());

    let no_status = backup_flags(2);
    let no_status_actions = settings_backup_actions_row_rect(v, 0.0, &no_status);
    let no_status_entry = settings_backup_entry_row_rect(v, 0.0, &no_status, 0);
    assert!(
        (no_status_entry.y - no_status_actions.bottom() - SETTINGS_BACKUP_CONTENT_GAP).abs() < 0.01
    );
}

#[test]
fn m1g_create_and_refresh_buttons_pack_left_inside_actions_row() {
    let v = vp();
    let row = settings_backup_actions_row_rect(v, 0.0, &backup_flags(0));
    let create = settings_backup_create_button_rect(row);
    let refresh = settings_backup_refresh_button_rect(row);
    assert!((create.x - row.x).abs() < 0.01);
    assert!(create.right() <= refresh.x);
    assert_eq!(create.width, SETTINGS_BACKUP_CREATE_BTN_W);
    assert_eq!(refresh.width, SETTINGS_BACKUP_REFRESH_BTN_W);
    assert!(refresh.right() <= row.right());
}

#[test]
fn m1g_entry_rows_stack_with_gap_and_restore_button_right_anchors() {
    let v = vp();
    let f = backup_flags(3);
    let r0 = settings_backup_entry_row_rect(v, 0.0, &f, 0);
    let r1 = settings_backup_entry_row_rect(v, 0.0, &f, 1);
    let r2 = settings_backup_entry_row_rect(v, 0.0, &f, 2);
    assert!(r0.y < r1.y);
    assert!(r1.y < r2.y);
    // Adjacent rows are one row-height + gap apart.
    assert!(
        (r1.y - r0.y - SETTINGS_BACKUP_ENTRY_ROW_H - SETTINGS_BACKUP_ENTRY_ROW_GAP).abs() < 0.01
    );
    let restore = settings_backup_restore_button_rect(r0);
    assert!(restore.right() <= r0.right());
    assert!(restore.x >= r0.x);
    assert_eq!(restore.width, SETTINGS_BACKUP_RESTORE_BTN_W);
}

#[test]
fn m1g_content_height_grows_with_visible_row_count() {
    // 0 (empty placeholder) / 1 / cap rows — height is monotone up to cap.
    let h0 = settings_backup_content_height(0);
    let h1 = settings_backup_content_height(1);
    let h_cap = settings_backup_content_height(SETTINGS_BACKUP_ROW_VISIBLE_MAX);
    assert!(h1 >= h0, "one entry row ≥ the empty placeholder slot");
    assert!(h_cap > h1, "more rows must grow the section");
    // Over-cap saturates at the cap height (the cap is applied inside).
    let h_over = settings_backup_content_height(SETTINGS_BACKUP_ROW_VISIBLE_MAX + 10);
    assert!((h_over - h_cap).abs() < 0.01);
}

#[test]
fn m1g_body_content_height_includes_backup_rows() {
    let v = vp();
    // Body height with 3 backup rows must exceed the empty-list body — the
    // variable list feeds the body via SettingsBodyFlags::backup_row_count.
    let empty = settings_body_content_height(v, &backup_flags(0));
    let full = settings_body_content_height(v, &backup_flags(SETTINGS_BACKUP_ROW_VISIBLE_MAX));
    assert!(full > empty, "backup list rows must grow the body");
}

#[test]
fn m1g_with_backup_rows_only_changes_backup_field() {
    let base = SettingsBodyFlags::new(true, true, false, false, UpdaterHeightKind::StatusOnly);
    let with = base.with_backup_rows(2);
    assert_eq!(base.backup_row_count, 0);
    assert_eq!(with.backup_row_count, 2);
    assert_eq!(with.crash_restart_enabled, base.crash_restart_enabled);
    assert_eq!(with.updater_kind, base.updater_kind);
}
