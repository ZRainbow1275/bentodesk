use super::*;
use crate::settings_panel::*;
use crate::state::SettingsUpdaterStatus;
use crate::widgets::toggle_switch::toggle_switch_in_rect;

impl Renderer {
    pub(super) fn draw_settings_updater(
        &mut self,
        app: &AppState,
        context: SettingsRenderContext,
        scroll: f32,
        updater_flags: SettingsBodyFlags,
    ) -> Result<SettingsBodyFlags, RenderError> {
        let SettingsRenderContext {
            viewport,
            body,
            palette,
            title_color,
            label_color,
            accent_on,
            track_off,
            chip_bg,
            chip_border,
            toggle_knob_color,
            chip_radius,
            btn_radius,
            ..
        } = context;
        let row_visible =
            |row: Rect, body: Rect| -> bool { row.bottom() > body.y && row.y < body.bottom() };
        let controls = palette.control_palette();
        // ── M1f — Updater §8 card (`UpdaterCard.tsx`) ───────────────────
        //
        // Sits after Stealth in the Tauri body order. Reads the live
        // `app.settings_updater_status` snapshot (drained from the
        // UpdateEvent channel by the shell event loop). Status → pill kind +
        // label, version-block / progress-bar / error-line visibility, and
        // action-button visibility all derive from the lib helpers in
        // `business::settings::updater_card` (1:1 with Tauri `statusPillLabel`
        // + the three `<Show when=…>` gates). The conditional middle block's
        // height is captured as `UpdaterHeightKind`, threaded through the same
        // `SettingsBodyFlags` the hit-tester + scroll-clamp use so paint and
        // hit geometry agree.
        use crate::business::settings::updater_card as upd;
        let updater_status = app.settings_updater_status.borrow();
        let updater_label = settings_updater_label_rect(
            viewport,
            scroll,
            updater_flags.crash_restart_enabled,
            updater_flags.safe_start_after_hibernation,
            updater_flags.stealth_has_retry,
            updater_flags.stealth_has_error,
        );
        if row_visible(updater_label, body) {
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_CARD_TITLE),
                updater_label,
                label_color,
            )?;
        }
        // Row 0 — status (label + colored pill), always shown.
        let upd_value_x_frac = 0.5_f32;
        let upd_status_row = settings_updater_status_row_rect(viewport, scroll, &updater_flags);
        if row_visible(upd_status_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: upd_status_row.x,
                y: upd_status_row.y + (upd_status_row.height - 16.0) * 0.5,
                width: upd_status_row.width * upd_value_x_frac,
                height: 16.0,
            };
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_STATUS_LABEL),
                label_rect,
                label_color,
            )?;
            // Status text owns the semantic colour; the pill keeps only a soft
            // tint so it remains a status indicator rather than a primary CTA.
            let pill = settings_updater_pill_rect(upd_status_row);
            let pill_radius = bento_nano_style::BorderRadius::all(pill.height * 0.5);
            let (pill_bg, pill_fg) = match upd::UpdaterPillKind::from_status(&updater_status) {
                upd::UpdaterPillKind::UpToDate | upd::UpdaterPillKind::Ready => {
                    let fg = palette.accent_green;
                    (with_alpha(fg, 0.16), fg)
                }
                upd::UpdaterPillKind::Busy | upd::UpdaterPillKind::Active => {
                    let fg = palette.accent_blue;
                    (with_alpha(fg, 0.16), fg)
                }
                upd::UpdaterPillKind::Skipped => (controls.disabled_fill, palette.text_muted),
                upd::UpdaterPillKind::Error => {
                    let fg = palette.accent_red;
                    (with_alpha(fg, 0.16), fg)
                }
            };
            self.fill_rounded_rect(pill, pill_bg, pill_radius)?;
            self.draw_settings_button_text(
                bento_nano_style::t(upd::updater_status_label_id(&updater_status)),
                pill,
                pill_fg,
                11.0,
                600,
            )?;
        }
        // Middle block — version line (Available/Ready/Installing/Skipped),
        // progress bar (Downloading), or error line (Error). Mutually
        // exclusive; StatusOnly paints nothing (zero-height block).
        let upd_middle = settings_updater_middle_block_rect(viewport, scroll, &updater_flags);
        if upd_middle.height > 0.0 && row_visible(upd_middle, body) {
            match updater_flags.updater_kind {
                UpdaterHeightKind::Versioned => {
                    let label_rect = bento_nano_style::Rect {
                        x: upd_middle.x,
                        y: upd_middle.y + (upd_middle.height - 16.0) * 0.5,
                        width: upd_middle.width * upd_value_x_frac,
                        height: 16.0,
                    };
                    self.draw_settings_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::UPDATER_AVAILABLE_VERSION,
                        ),
                        label_rect,
                        label_color,
                    )?;
                    let value_rect = bento_nano_style::Rect {
                        x: upd_middle.x + upd_middle.width * upd_value_x_frac,
                        y: label_rect.y,
                        width: upd_middle.width * (1.0 - upd_value_x_frac),
                        height: 16.0,
                    };
                    if let Some(version) = upd::updater_visible_version(&updater_status) {
                        self.draw_settings_row_value(
                            version.as_str(),
                            value_rect,
                            palette.text_muted,
                        )?;
                    }
                }
                UpdaterHeightKind::Downloading => {
                    // Track + filled portion. When the total is unknown the
                    // fraction is None → paint a muted full-width track only
                    // (indeterminate cue), never a panic / divide-by-zero.
                    let track =
                        settings_updater_progress_track_rect(viewport, scroll, &updater_flags);
                    let track_radius = bento_nano_style::BorderRadius::all(track.height * 0.5);
                    self.fill_rounded_rect(
                        track,
                        with_alpha(palette.surface_subtle, 0.85),
                        track_radius,
                    )?;
                    if let Some(frac) = upd::updater_progress_fraction(&updater_status) {
                        let fill = bento_nano_style::Rect {
                            x: track.x,
                            y: track.y,
                            width: (track.width * frac).max(0.0),
                            height: track.height,
                        };
                        self.fill_rounded_rect(fill, accent_on, track_radius)?;
                    }
                }
                UpdaterHeightKind::Error => {
                    if let SettingsUpdaterStatus::Error(message) = &*updater_status {
                        self.draw_settings_text(
                            message.as_str(),
                            upd_middle,
                            with_alpha(palette.accent_red, 0.9),
                        )?;
                    }
                }
                UpdaterHeightKind::StatusOnly => {}
            }
        }
        // Action buttons row — 检查更新 (always, col 0), then state-gated
        // 下载 / 安装并重启 (col 1) + 跳过此版本 (col 2). The column indices match
        // the hit-tester so paint and hit agree.
        let upd_btn_row = settings_updater_buttons_row_rect(viewport, scroll, &updater_flags);
        if row_visible(upd_btn_row, body) {
            // Col 0 — 检查更新 (always).
            let check_btn = settings_updater_button_rect(upd_btn_row, 0);
            let updater_action_bg = with_alpha(accent_on, 0.18);
            let updater_action_border = with_alpha(accent_on, 0.38);
            self.fill_rounded_rect(check_btn, updater_action_bg, btn_radius)?;
            self.stroke_rounded_rect(check_btn, updater_action_border, btn_radius, 1.0)?;
            self.draw_settings_button_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_CHECK_NOW),
                check_btn,
                accent_on,
                12.0,
                500,
            )?;
            // Col 1 — 下载 (Available) or 安装并重启 (Ready), accent-filled.
            if upd::updater_show_download(&updater_status) {
                let dl_btn = settings_updater_button_rect(upd_btn_row, 1);
                self.fill_rounded_rect(dl_btn, updater_action_bg, btn_radius)?;
                self.stroke_rounded_rect(dl_btn, updater_action_border, btn_radius, 1.0)?;
                self.draw_settings_button_text(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_DOWNLOAD),
                    dl_btn,
                    accent_on,
                    12.0,
                    500,
                )?;
            } else if upd::updater_show_install(&updater_status) {
                let install_btn = settings_updater_button_rect(upd_btn_row, 1);
                self.fill_rounded_rect(install_btn, updater_action_bg, btn_radius)?;
                self.stroke_rounded_rect(install_btn, updater_action_border, btn_radius, 1.0)?;
                self.draw_settings_button_text(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_INSTALL_RESTART),
                    install_btn,
                    accent_on,
                    12.0,
                    500,
                )?;
            }
            // Col 2 — 跳过此版本 (Available/Ready), neutral chip.
            if upd::updater_show_skip(&updater_status) {
                let skip_btn = settings_updater_button_rect(upd_btn_row, 2);
                self.fill_rounded_rect(skip_btn, chip_bg, btn_radius)?;
                self.stroke_rounded_rect(skip_btn, chip_border, btn_radius, 1.0)?;
                self.draw_settings_button_text(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_SKIP_VERSION),
                    skip_btn,
                    title_color,
                    12.0,
                    500,
                )?;
            }
        }
        // Prefs row — 检查频率 cycling chip (Daily/Weekly/Manual).
        let upd_freq_row = settings_updater_frequency_row_rect(viewport, scroll, &updater_flags);
        if row_visible(upd_freq_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: upd_freq_row.x,
                y: upd_freq_row.y + (upd_freq_row.height - 16.0) * 0.5,
                width: upd_freq_row.width * upd_value_x_frac,
                height: 16.0,
            };
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_FREQUENCY),
                label_rect,
                label_color,
            )?;
            let chip = settings_updater_frequency_chip_rect(upd_freq_row);
            self.fill_rounded_rect(chip, chip_bg, chip_radius)?;
            let freq_id = match app.update_check_frequency.get() {
                bento_nano_backend::updater::UpdateCheckFrequency::Daily => {
                    bento_nano_style::i18n_zh_cn::ids::UPDATER_FREQ_DAILY
                }
                bento_nano_backend::updater::UpdateCheckFrequency::Weekly => {
                    bento_nano_style::i18n_zh_cn::ids::UPDATER_FREQ_WEEKLY
                }
                bento_nano_backend::updater::UpdateCheckFrequency::Manual => {
                    bento_nano_style::i18n_zh_cn::ids::UPDATER_FREQ_MANUAL
                }
            };
            self.draw_settings_button_text(
                bento_nano_style::t(freq_id),
                chip,
                title_color,
                12.0,
                500,
            )?;
        }
        // Prefs row — 后台静默下载 toggle.
        let upd_auto_row =
            settings_updater_auto_download_row_rect(viewport, scroll, &updater_flags);
        if row_visible(upd_auto_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: upd_auto_row.x,
                y: upd_auto_row.y + (upd_auto_row.height - 16.0) * 0.5,
                width: upd_auto_row.width * 0.7,
                height: 16.0,
            };
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_AUTO_DOWNLOAD),
                label_rect,
                label_color,
            )?;
            let auto_on = app.update_auto_download.get();
            let hit = settings_updater_auto_download_hit_rect(upd_auto_row);
            let switch = toggle_switch_in_rect(hit);
            self.fill_rounded_rect(
                switch.track,
                if auto_on { accent_on } else { track_off },
                BorderRadius::all(switch.track_radius()),
            )?;
            self.fill_rounded_rect(
                switch.knob(auto_on),
                toggle_knob_color,
                BorderRadius::all(switch.knob_radius()),
            )?;
        }
        drop(updater_status);
        Ok(updater_flags)
    }
}
