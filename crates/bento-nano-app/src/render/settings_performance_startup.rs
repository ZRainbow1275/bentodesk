use super::*;
use crate::settings_panel::*;
use crate::widgets::toggle_switch::toggle_switch_in_rect;

impl Renderer {
    pub(super) fn draw_settings_performance_startup(
        &mut self,
        app: &AppState,
        context: SettingsRenderContext,
        scroll: f32,
        source_count: usize,
    ) -> Result<(f32, bool, bool), RenderError> {
        let SettingsRenderContext {
            viewport,
            body,
            title_color,
            label_color,
            accent_on,
            track_off,
            chip_bg,
            chip_border,
            toggle_knob_color,
            btn_radius,
            ..
        } = context;
        let row_visible =
            |row: Rect, body: Rect| -> bool { row.bottom() > body.y && row.y < body.bottom() };
        // ── M1d sections — Performance §5 + Startup management §6 ────────
        //
        // Replaces the deleted bespoke 高级 / 未来集成验证 blocks with the two
        // genuine Tauri sections (`SettingsPanel.tsx:601-698`). Performance =
        // 3 SliderRows (no conditionals). Startup = 2 toggles + 2 conditional
        // steppers (crash_restart) + 1 toggle + 1 conditional slider
        // (hibernation). The hit-tester in `bento-nano-shell::ui::settings_hit`
        // + the dispatch arms in `main.rs` route every control fully through
        // paint→hit→dispatch→persist→snapshot.
        let slider_track_radius = bento_nano_style::BorderRadius::all(2.0);
        let slider_thumb_radius =
            bento_nano_style::BorderRadius::all(SETTINGS_SLIDER_THUMB_D * 0.5);

        // Read the two gating bools once so paint matches geometry exactly.
        let crash_restart_on = app.crash_restart_enabled.get();
        let safe_start_on = app.safe_start_after_hibernation.get();

        // M1i fidelity — single-base-offset reflow. The Performance §5 group and
        // EVERY section below it (Startup/Stealth/Updater/Backup/Plugins) root
        // at `settings_perf_origin_y_offset`, which is pinned at the fixed
        // 4-card source reserve. Folding the live reserve delta into `scroll`
        // shifts the whole lower body UP by the height of the missing source
        // cards (Tauri's flex column) — shadowing `scroll` here propagates the
        // shift to all perf-and-below geometry fns without touching their
        // signatures. The hit-tester applies the identical fold (`ui.rs`).
        let scroll = scroll + settings_sources_reserve_delta(source_count);

        // Performance group title.
        let perf_label = settings_performance_label_rect(viewport, scroll);
        if row_visible(perf_label, body) {
            self.draw_settings_group_title(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_PERFORMANCE),
                perf_label,
                label_color,
            )?;
        }

        // Performance SliderRows. Each: label + tabular "{v}{unit}" on the top
        // line, full-width track band + filled segment + thumb on the lower
        // line (matches Tauri `.slider-row`, `SettingsPanel.tsx:848-871`).
        let perf_rows: [(u16, i32, i32, &'static str); 3] = [
            (
                bento_nano_style::i18n_zh_cn::ids::SETTING_EXPAND_DELAY.0,
                crate::state::EXPAND_DELAY_MIN_MS,
                crate::state::EXPAND_DELAY_MAX_MS,
                "ms",
            ),
            (
                bento_nano_style::i18n_zh_cn::ids::SETTING_COLLAPSE_DELAY.0,
                crate::state::COLLAPSE_DELAY_MIN_MS,
                crate::state::COLLAPSE_DELAY_MAX_MS,
                "ms",
            ),
            (
                bento_nano_style::i18n_zh_cn::ids::SETTING_ICON_CACHE_SIZE.0,
                crate::state::ICON_CACHE_MIN,
                crate::state::ICON_CACHE_MAX,
                "",
            ),
        ];
        for index in 0..SETTINGS_PERF_ROW_COUNT {
            let row = settings_performance_slider_row_rect(viewport, scroll, index);
            if !row_visible(row, body) {
                continue;
            }
            let (label_id, min, max, unit) = perf_rows[index as usize];
            let raw = match index {
                0 => app.expand_delay_ms.get(),
                1 => app.collapse_delay_ms.get(),
                _ => app.icon_cache_size.get(),
            };
            let value = raw.clamp(min, max);
            // Top line: label (left) + value (right, tabular).
            let label_rect = bento_nano_style::Rect {
                x: row.x,
                y: row.y + 4.0,
                width: row.width * 0.6,
                height: 16.0,
            };
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::StringId(label_id)),
                label_rect,
                label_color,
            )?;
            let value_text = if unit.is_empty() {
                smol_str::SmolStr::new(value.to_string())
            } else {
                smol_str::SmolStr::new(format!("{value}{unit}"))
            };
            let value_rect = bento_nano_style::Rect {
                x: row.x + row.width * 0.6,
                y: row.y + 4.0,
                width: row.width * 0.4,
                height: 16.0,
            };
            self.draw_text_no_wrap_with_style(
                value_text.as_str(),
                value_rect,
                title_color,
                crate::settings_panel::SETTINGS_TEXT_VALUE_SIZE,
                crate::settings_panel::SETTINGS_TEXT_VALUE_WEIGHT,
                crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Trailing,
                    v: dwrite::VAlign::Near,
                },
            )?;
            // Lower line: slider track + filled segment + thumb.
            let track = settings_performance_slider_rect(viewport, scroll, index);
            let track_band = bento_nano_style::Rect {
                x: track.x,
                y: track.y + (track.height - 4.0) * 0.5,
                width: track.width,
                height: 4.0,
            };
            self.fill_rounded_rect(track_band, track_off, slider_track_radius)?;
            let span = (max - min).max(1) as f32;
            let frac = ((value - min) as f32 / span).clamp(0.0, 1.0);
            let filled = bento_nano_style::Rect {
                x: track_band.x,
                y: track_band.y,
                width: track_band.width * frac,
                height: track_band.height,
            };
            self.fill_rounded_rect(filled, accent_on, slider_track_radius)?;
            let thumb_d = track.height;
            let thumb = bento_nano_style::Rect {
                x: track.x + track.width * frac - thumb_d * 0.5,
                y: track.y,
                width: thumb_d,
                height: thumb_d,
            };
            // Tauri `.settings-slider::-webkit-slider-thumb` uses the
            // active accent, while only toggle-switch thumbs are white.
            self.fill_rounded_rect(thumb, accent_on, slider_thumb_radius)?;
        }

        // Startup management group title.
        let startup_label = settings_startup_label_rect(viewport, scroll);
        if row_visible(startup_label, body) {
            self.draw_settings_group_title(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_STARTUP),
                startup_label,
                label_color,
            )?;
        }

        // Reusable toggle-row paint: label (left) + desc caption + rocker.
        // Returns the toggle hit-box so the caller can drop it (unused here).
        // We inline rather than closure to keep `self` borrows simple.
        // Row 0 — 高优先级启动 (always).
        let high_row = settings_startup_high_priority_row_rect(viewport, scroll);
        if row_visible(high_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: high_row.x,
                y: high_row.y + (high_row.height - 16.0) * 0.5,
                width: high_row.width * 0.6,
                height: 16.0,
            };
            self.draw_settings_text(
                bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::SETTING_STARTUP_HIGH_PRIORITY,
                ),
                label_rect,
                label_color,
            )?;
            let on = app.startup_high_priority.get();
            let switch = toggle_switch_in_rect(settings_startup_toggle_hit_rect(high_row));
            self.fill_rounded_rect(
                switch.track,
                if on { accent_on } else { track_off },
                BorderRadius::all(switch.track_radius()),
            )?;
            self.fill_rounded_rect(
                switch.knob(on),
                toggle_knob_color,
                BorderRadius::all(switch.knob_radius()),
            )?;
        }
        // Row 0 desc caption.
        let high_desc = bento_nano_style::Rect {
            x: high_row.x,
            y: high_row.bottom() + 1.0,
            width: high_row.width,
            height: 14.0,
        };
        if row_visible(high_desc, body) {
            self.draw_settings_text(
                bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::SETTING_STARTUP_HIGH_PRIORITY_DESC,
                ),
                high_desc,
                with_alpha(label_color, 0.7),
            )?;
        }

        // Row 1 — 崩溃自动重启 (always, gates the steppers).
        let crash_row = settings_crash_restart_row_rect(viewport, scroll);
        if row_visible(crash_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: crash_row.x,
                y: crash_row.y + (crash_row.height - 16.0) * 0.5,
                width: crash_row.width * 0.6,
                height: 16.0,
            };
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTING_CRASH_RESTART),
                label_rect,
                label_color,
            )?;
            let switch = toggle_switch_in_rect(settings_startup_toggle_hit_rect(crash_row));
            self.fill_rounded_rect(
                switch.track,
                if crash_restart_on {
                    accent_on
                } else {
                    track_off
                },
                BorderRadius::all(switch.track_radius()),
            )?;
            self.fill_rounded_rect(
                switch.knob(crash_restart_on),
                toggle_knob_color,
                BorderRadius::all(switch.knob_radius()),
            )?;
        }
        let crash_desc = bento_nano_style::Rect {
            x: crash_row.x,
            y: crash_row.bottom() + 1.0,
            width: crash_row.width,
            height: 14.0,
        };
        if row_visible(crash_desc, body) {
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTING_CRASH_RESTART_DESC),
                crash_desc,
                with_alpha(label_color, 0.7),
            )?;
        }

        // Rows 2/3 — crash number inputs, ONLY when crash_restart_on.
        // The 72×30 shell matches Tauri `.settings-row__number-input`;
        // the existing side targets retain decrement/increment behaviour.
        if crash_restart_on {
            let stepper_rows: [(u16, Rect, i32); 2] = [
                (
                    bento_nano_style::i18n_zh_cn::ids::SETTING_CRASH_MAX_RETRIES.0,
                    settings_crash_max_retries_row_rect(viewport, scroll),
                    app.crash_max_retries.get(),
                ),
                (
                    bento_nano_style::i18n_zh_cn::ids::SETTING_CRASH_WINDOW_SECS.0,
                    settings_crash_window_row_rect(viewport, scroll),
                    app.crash_window_secs.get(),
                ),
            ];
            for (label_id, row, value) in stepper_rows {
                if !row_visible(row, body) {
                    continue;
                }
                let label_rect = bento_nano_style::Rect {
                    x: row.x,
                    y: row.y + (row.height - 16.0) * 0.5,
                    width: row.width * 0.6,
                    height: 16.0,
                };
                self.draw_settings_text(
                    bento_nano_style::t(bento_nano_style::StringId(label_id)),
                    label_rect,
                    label_color,
                )?;
                let val_rect = settings_stepper_value_rect(row);
                let input_rect = settings_stepper_input_rect(row);
                self.fill_rounded_rect(input_rect, chip_bg, btn_radius)?;
                self.stroke_rounded_rect(input_rect, chip_border, btn_radius, 1.0)?;
                let buf = smol_str::SmolStr::new(value.to_string());
                self.draw_text_no_wrap_with_style(
                    buf.as_str(),
                    val_rect,
                    title_color,
                    crate::settings_panel::SETTINGS_TEXT_LABEL_SIZE,
                    crate::settings_panel::SETTINGS_TEXT_LABEL_WEIGHT,
                    crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
                    dwrite::TextAlign {
                        h: dwrite::HAlign::Center,
                        v: dwrite::VAlign::Center,
                    },
                )?;
            }
        }

        // Row 4 — 休眠安全恢复 (always, gates the hibernate slider). Its Y
        // depends on whether the crash steppers are present.
        let safe_row = settings_safe_start_row_rect(viewport, scroll, crash_restart_on);
        if row_visible(safe_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: safe_row.x,
                y: safe_row.y + (safe_row.height - 16.0) * 0.5,
                width: safe_row.width * 0.6,
                height: 16.0,
            };
            self.draw_settings_text(
                bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::SETTING_SAFE_START_HIBERNATION,
                ),
                label_rect,
                label_color,
            )?;
            let switch = toggle_switch_in_rect(settings_startup_toggle_hit_rect(safe_row));
            self.fill_rounded_rect(
                switch.track,
                if safe_start_on { accent_on } else { track_off },
                BorderRadius::all(switch.track_radius()),
            )?;
            self.fill_rounded_rect(
                switch.knob(safe_start_on),
                toggle_knob_color,
                BorderRadius::all(switch.knob_radius()),
            )?;
        }
        let safe_desc = bento_nano_style::Rect {
            x: safe_row.x,
            y: safe_row.bottom() + 1.0,
            width: safe_row.width,
            height: 14.0,
        };
        if row_visible(safe_desc, body) {
            self.draw_settings_text(
                bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::SETTING_SAFE_START_HIBERNATION_DESC,
                ),
                safe_desc,
                with_alpha(label_color, 0.7),
            )?;
        }

        // Row 5 — 恢复延迟 SliderRow, ONLY when safe_start_on.
        if safe_start_on {
            let row = settings_hibernate_slider_row_rect(viewport, scroll, crash_restart_on);
            if row_visible(row, body) {
                let value = app.hibernate_resume_delay_ms.get().clamp(
                    crate::state::HIBERNATE_DELAY_MIN_MS,
                    crate::state::HIBERNATE_DELAY_MAX_MS,
                );
                let label_rect = bento_nano_style::Rect {
                    x: row.x,
                    y: row.y + 4.0,
                    width: row.width * 0.6,
                    height: 16.0,
                };
                self.draw_settings_text(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTING_HIBERNATE_DELAY),
                    label_rect,
                    label_color,
                )?;
                let value_text = smol_str::SmolStr::new(format!("{value}ms"));
                let value_rect = bento_nano_style::Rect {
                    x: row.x + row.width * 0.6,
                    y: row.y + 4.0,
                    width: row.width * 0.4,
                    height: 16.0,
                };
                self.draw_text_no_wrap_with_style(
                    value_text.as_str(),
                    value_rect,
                    title_color,
                    crate::settings_panel::SETTINGS_TEXT_VALUE_SIZE,
                    crate::settings_panel::SETTINGS_TEXT_VALUE_WEIGHT,
                    crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
                    dwrite::TextAlign {
                        h: dwrite::HAlign::Trailing,
                        v: dwrite::VAlign::Near,
                    },
                )?;
                let track = settings_hibernate_slider_rect(viewport, scroll, crash_restart_on);
                let track_band = bento_nano_style::Rect {
                    x: track.x,
                    y: track.y + (track.height - 4.0) * 0.5,
                    width: track.width,
                    height: 4.0,
                };
                self.fill_rounded_rect(track_band, track_off, slider_track_radius)?;
                let span = (crate::state::HIBERNATE_DELAY_MAX_MS
                    - crate::state::HIBERNATE_DELAY_MIN_MS)
                    .max(1) as f32;
                let frac =
                    ((value - crate::state::HIBERNATE_DELAY_MIN_MS) as f32 / span).clamp(0.0, 1.0);
                let filled = bento_nano_style::Rect {
                    x: track_band.x,
                    y: track_band.y,
                    width: track_band.width * frac,
                    height: track_band.height,
                };
                self.fill_rounded_rect(filled, accent_on, slider_track_radius)?;
                let thumb_d = track.height;
                let thumb = bento_nano_style::Rect {
                    x: track.x + track.width * frac - thumb_d * 0.5,
                    y: track.y,
                    width: thumb_d,
                    height: thumb_d,
                };
                self.fill_rounded_rect(thumb, accent_on, slider_thumb_radius)?;
            }
        }
        Ok((scroll, crash_restart_on, safe_start_on))
    }
}
