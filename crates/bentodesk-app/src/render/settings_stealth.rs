use super::*;
use crate::settings_panel::*;

impl Renderer {
    pub(super) fn draw_settings_stealth(
        &mut self,
        app: &AppState,
        context: SettingsRenderContext,
        scroll: f32,
        crash_restart_on: bool,
        safe_start_on: bool,
    ) -> Result<(bool, bool), RenderError> {
        let SettingsRenderContext {
            viewport,
            body,
            palette,
            title_color,
            label_color,
            accent_on,
            chip_bg,
            chip_border,
            chip_radius,
            btn_radius,
            ..
        } = context;
        let row_visible =
            |row: Rect, body: Rect| -> bool { row.bottom() > body.y && row.y < body.bottom() };
        let controls = palette.control_palette();
        // ── M1e — Stealth §7 card (`StealthModeCard.tsx`) ───────────────
        //
        // Sits after Startup in the Tauri body order. Reads the cached
        // `app.stealth_status` snapshot (refreshed by the shell on open +
        // Refresh/Reapply). Status pill kind/label derive via
        // `StatusLevel::from_status` (1:1 with Tauri `deriveLevel`). The
        // retry/error/OneDrive rows are conditional; the geometry helpers take
        // the same `has_retry`/`has_error` flags so paint matches hit-test.
        use crate::business::settings::stealth_mode_card::StatusLevel;
        let stealth_label =
            settings_stealth_label_rect(viewport, scroll, crash_restart_on, safe_start_on);
        if row_visible(stealth_label, body) {
            self.draw_settings_text(
                bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::STEALTH_GROUP_TITLE),
                stealth_label,
                label_color,
            )?;
        }
        // Snapshot the conditional flags + cloned fields out of the RefCell so
        // the borrow does not span the fallible paint calls below.
        let stealth_snapshot = app.stealth_status.borrow().clone();
        let (has_retry, has_error) = match &stealth_snapshot {
            Some(s) => (s.retry_count > 0, s.last_error.is_some()),
            None => (false, false),
        };
        // Helper to paint a `label | value` row (label left, value right).
        // Inlined per-row below to keep `self` borrows simple.
        let stealth_value_x_frac = 0.5_f32;
        // Row 0 — status (label + colored pill), always shown.
        let status_row =
            settings_stealth_status_row_rect(viewport, scroll, crash_restart_on, safe_start_on);
        if row_visible(status_row, body) {
            let label_rect = bentodesk_style::Rect {
                x: status_row.x,
                y: status_row.y + (status_row.height - 16.0) * 0.5,
                width: status_row.width * stealth_value_x_frac,
                height: 16.0,
            };
            self.draw_settings_text(
                bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::STEALTH_STATUS_LABEL),
                label_rect,
                label_color,
            )?;
            let pill = settings_stealth_pill_rect(status_row);
            let pill_radius = bentodesk_style::BorderRadius::all(pill.height * 0.5);
            // Keep status colour in the text and use a restrained tint for the
            // capsule surface. A near-solid fill made the small status pill read
            // like a primary action and amplified any text-alignment error.
            let (pill_bg, pill_fg, pill_label_id) = match stealth_snapshot.as_ref() {
                Some(s) => {
                    let level = StatusLevel::from_status(s);
                    let fg = match level {
                        StatusLevel::Applied => palette.accent_green,
                        StatusLevel::Pending => palette.accent_orange,
                        StatusLevel::Failed => palette.accent_red,
                    };
                    (with_alpha(fg, 0.18), fg, level.label_id())
                }
                None => (
                    controls.disabled_fill,
                    palette.text_muted,
                    bentodesk_style::i18n_zh_cn::ids::STEALTH_STATUS_PENDING,
                ),
            };
            self.fill_rounded_rect(pill, pill_bg, pill_radius)?;
            self.draw_settings_button_text(
                bentodesk_style::t(pill_label_id),
                pill,
                pill_fg,
                10.0,
                600,
            )?;
        }
        // Row 1 — schema version (label + value), always shown.
        let schema_row =
            settings_stealth_schema_row_rect(viewport, scroll, crash_restart_on, safe_start_on);
        if row_visible(schema_row, body) {
            let label_rect = bentodesk_style::Rect {
                x: schema_row.x,
                y: schema_row.y + (schema_row.height - 16.0) * 0.5,
                width: schema_row.width * stealth_value_x_frac,
                height: 16.0,
            };
            self.draw_settings_text(
                bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::STEALTH_SCHEMA_VERSION),
                label_rect,
                label_color,
            )?;
            let value_rect = bentodesk_style::Rect {
                x: schema_row.x + schema_row.width * stealth_value_x_frac,
                y: label_rect.y,
                width: schema_row.width * (1.0 - stealth_value_x_frac),
                height: 16.0,
            };
            let schema_text = match stealth_snapshot.as_ref() {
                Some(s) => smol_str::SmolStr::new(s.schema_version.as_str()),
                None => smol_str::SmolStr::new_static("—"),
            };
            self.draw_settings_row_value(schema_text.as_str(), value_rect, palette.text_muted)?;
        }
        // Row 2 — mirror health (label + 健康/异常), always shown.
        let mirror_row =
            settings_stealth_mirror_row_rect(viewport, scroll, crash_restart_on, safe_start_on);
        if row_visible(mirror_row, body) {
            let label_rect = bentodesk_style::Rect {
                x: mirror_row.x,
                y: mirror_row.y + (mirror_row.height - 16.0) * 0.5,
                width: mirror_row.width * stealth_value_x_frac,
                height: 16.0,
            };
            self.draw_settings_text(
                bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::STEALTH_MIRROR_HEALTHY),
                label_rect,
                label_color,
            )?;
            let value_rect = bentodesk_style::Rect {
                x: mirror_row.x + mirror_row.width * stealth_value_x_frac,
                y: label_rect.y,
                width: mirror_row.width * (1.0 - stealth_value_x_frac),
                height: 16.0,
            };
            let healthy = stealth_snapshot
                .as_ref()
                .map(|s| s.mirror_healthy)
                .unwrap_or(true);
            let mirror_id = if healthy {
                bentodesk_style::i18n_zh_cn::ids::STEALTH_MIRROR_HEALTHY_YES
            } else {
                bentodesk_style::i18n_zh_cn::ids::STEALTH_MIRROR_HEALTHY_NO
            };
            self.draw_settings_row_value(
                bentodesk_style::t(mirror_id),
                value_rect,
                palette.text_muted,
            )?;
        }
        // Row 3 — retry count (label + value), ONLY when retry_count > 0.
        if has_retry {
            let retry_row =
                settings_stealth_retry_row_rect(viewport, scroll, crash_restart_on, safe_start_on);
            if row_visible(retry_row, body) {
                let label_rect = bentodesk_style::Rect {
                    x: retry_row.x,
                    y: retry_row.y + (retry_row.height - 16.0) * 0.5,
                    width: retry_row.width * stealth_value_x_frac,
                    height: 16.0,
                };
                self.draw_settings_text(
                    bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::STEALTH_RETRY_COUNT),
                    label_rect,
                    label_color,
                )?;
                let value_rect = bentodesk_style::Rect {
                    x: retry_row.x + retry_row.width * stealth_value_x_frac,
                    y: label_rect.y,
                    width: retry_row.width * (1.0 - stealth_value_x_frac),
                    height: 16.0,
                };
                let retry_text = smol_str::SmolStr::new(
                    stealth_snapshot
                        .as_ref()
                        .map(|s| s.retry_count)
                        .unwrap_or(0)
                        .to_string(),
                );
                self.draw_settings_row_value(retry_text.as_str(), value_rect, palette.text_muted)?;
            }
        }
        // Row 4 — last-error block (label line + wrapped code), ONLY when set.
        if has_error {
            let err_block = settings_stealth_error_block_rect(
                viewport,
                scroll,
                crash_restart_on,
                safe_start_on,
                has_retry,
            );
            if row_visible(err_block, body) {
                let label_rect = bentodesk_style::Rect {
                    x: err_block.x,
                    y: err_block.y,
                    width: err_block.width,
                    height: 16.0,
                };
                self.draw_settings_text(
                    bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::STEALTH_LAST_ERROR),
                    label_rect,
                    label_color,
                )?;
                let err_rect = bentodesk_style::Rect {
                    x: err_block.x,
                    y: err_block.y + 18.0,
                    width: err_block.width,
                    height: err_block.height - 18.0,
                };
                if let Some(s) = stealth_snapshot.as_ref()
                    && let Some(err) = s.last_error.as_deref()
                {
                    self.draw_settings_text(err, err_rect, with_alpha(palette.accent_red, 0.9))?;
                }
            }
        }
        // Buttons row — [Refresh][Reapply], always shown.
        let stealth_btn_row = settings_stealth_buttons_row_rect(
            viewport,
            scroll,
            crash_restart_on,
            safe_start_on,
            has_retry,
            has_error,
        );
        if row_visible(stealth_btn_row, body) {
            let refresh_btn = settings_stealth_refresh_button_rect(stealth_btn_row);
            self.fill_rounded_rect(refresh_btn, chip_bg, btn_radius)?;
            self.stroke_rounded_rect(refresh_btn, chip_border, btn_radius, 1.0)?;
            self.draw_settings_button_text(
                bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::STEALTH_REFRESH_BTN),
                refresh_btn,
                title_color,
                12.0,
                500,
            )?;
            let reapply_btn = settings_stealth_reapply_button_rect(stealth_btn_row);
            self.fill_rounded_rect(reapply_btn, accent_on, btn_radius)?;
            self.draw_settings_button_text(
                bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::STEALTH_REAPPLY_BTN),
                reapply_btn,
                controls.on_accent,
                12.0,
                500,
            )?;
        }
        // OneDrive warning block — informational text only, ONLY when
        // retry_count > 0 (the backend notes OneDrive typically holds the
        // lock). No button: there is no OneDrive-exclusion probe / guide URL
        // in the native backend, so per §17 this stays text-only rather than a
        // dead button.
        if has_retry {
            let od_block = settings_stealth_onedrive_block_rect(
                viewport,
                scroll,
                crash_restart_on,
                safe_start_on,
                has_retry,
                has_error,
            );
            if row_visible(od_block, body) {
                let od_bg = with_alpha(palette.accent_orange, 0.12);
                self.fill_rounded_rect(od_block, od_bg, chip_radius)?;
                let text_rect = bentodesk_style::Rect {
                    x: od_block.x + 10.0,
                    y: od_block.y + 8.0,
                    width: (od_block.width - 20.0).max(0.0),
                    height: (od_block.height - 16.0).max(0.0),
                };
                self.draw_settings_text(
                    bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::STEALTH_ONEDRIVE_WARNING),
                    text_rect,
                    with_alpha(title_color, 0.92),
                )?;
            }
        }
        Ok((has_retry, has_error))
    }
}
