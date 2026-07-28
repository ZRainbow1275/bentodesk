use super::*;

impl Renderer {
    /// Dedicated entry point for the `WindowKind::Settings` HWND. The HWND
    /// has its own 800×600 viewport (vs the Main HWND's primary-monitor work
    /// area), so painting the entire main UI tree + zones underneath the
    /// modal scrim leaks Main-window geometry into the Settings frame and
    /// causes overlap (button rects positioned for the Main viewport land
    /// outside the Settings panel chrome). Render only the scrim + panel +
    /// any open sub-modals, keeping the Settings HWND's frame self-contained.
    pub(super) fn draw_settings_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        self.draw_settings_panel(app)
    }

    /// Phase 2.1 Ruling C — draw the modal settings overlay. Triggered by
    /// `app.settings_open == true`. Three layers:
    ///   1. Full-viewport α=0.30 black scrim so the underlying UI fades.
    ///   2. Centred 320×200 rounded panel with translucent dark fill.
    ///   3. Title + real settings rows + close button.
    pub(super) fn draw_settings_panel(&mut self, app: &AppState) -> Result<(), RenderError> {
        use crate::settings_panel::{
            SETTINGS_PANEL_RADIUS, SETTINGS_ROW_PAD_X, SettingsBodyFlags, settings_body_rect,
            settings_cancel_button_rect, settings_close_button_rect_m1, settings_footer_rect,
            settings_header_rect, settings_panel_fills_host, settings_panel_rect_m1,
            settings_save_button_rect,
        };
        // Round-2 M1 — Tauri 1.2.4 frame_060/065/070/075 dark redesign.
        //
        // Three layers paint in order:
        //   1. Full-viewport α=0.55 scrim so the underlying desk fades hard.
        //   2. Dark dialog card (400 × min(700, viewport.h-padding), radius 14).
        //   3. Sticky 48-DIP header + scrollable body + sticky 56-DIP footer.
        //
        // Body content for M1: 5 toggle rows + language chip row.
        // K1 modal-opener arms (keybindings/plugins/theme picker) remain alive
        // as orphan paint paths gated on their own `*_open` Cells. They never
        // fire from M1 hit-test but compile-clean per Ruling B.
        // M6a — read the live theme palette so the whole Settings paint (panel
        // / header / footer / labels / accent / track) re-skins with the
        // active theme. Bound once; `PaletteTauri: Copy` (§10).
        let palette = app.active_theme_tauri();
        // P1 (#7 fix wave 2026-06-01) — wall-clock sampled ONCE per Settings
        // paint (same `GetTickCount` pattern `draw_zones` uses for the pill
        // animator, allocation-free §10). Threaded into the §2/§10 text-field
        // caret blink so a focused caret toggles at the Windows ~530ms cadence.
        // The frame-pump keeps redrawing while a field is focused (the shell
        // ORs `settings_focused_field != None` into `any_active`), so this value
        // advances frame to frame.
        // SAFETY: `GetTickCount` is total + thread-safe.
        let settings_now_ms =
            unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
        // P1 — caret is ON for the first half of each ~1060ms blink period.
        let caret_on = settings_caret_on(settings_now_ms);
        // Tauri's modal uses the active theme's expanded glass token. Auxiliary
        // HWNDs deliberately do not sample Main's monitor-sized backdrop: a
        // movable panel would otherwise drag a detached wallpaper snapshot.
        // Therefore the native fallback must be fully opaque. Even a 0.96
        // surface leaves high-contrast foreground-window edges visible as a
        // sharp vertical seam through the Settings card when Acrylic is
        // unavailable.
        let panel_bg = palette.surface_expanded;
        let has_panel_backdrop = self.backdrop_brush.is_some();
        let panel_fallback_bg = opaque_auxiliary_surface(panel_bg);
        let title_color = palette.text_primary;
        let label_color = palette.text_secondary;
        let accent_on = palette.accent_blue;
        // Native controls share one polarity-aware semantic derivation.  A
        // literal white overlay works on dark themes but disappears on Order,
        // Editorial, Neo and Frosted surfaces.
        let controls = palette.control_palette();
        let track_off = controls.track_off;
        let chip_bg = controls.fill;
        let chip_border = controls.border;
        let toggle_knob_color = controls.knob;
        let divider_color = controls.divider;
        let panel_radius = bento_nano_style::BorderRadius::all(SETTINGS_PANEL_RADIUS);
        // M6b — per-theme card radius for the Settings chip surfaces.
        let chip_radius_tokens = app.active_theme_radius_tauri();
        let chip_radius = bento_nano_style::BorderRadius::all(chip_radius_tokens.card);
        let btn_radius = bento_nano_style::BorderRadius::all(8.0);

        // RC-4 Gap 2 — derive a layout viewport from backbuffer + base_scale.
        let base_scale = self.base_scale.max(0.01);
        let viewport = bento_nano_style::Size {
            width: (self.width as f32 / base_scale).max(1.0),
            height: (self.height as f32 / base_scale).max(1.0),
        };

        // A synthetic wide overlay viewport still receives the reference scrim.
        // The production Settings HWND is the card itself; painting a rectangular
        // scrim there filled the transparent rounded corners with a black slab.
        if !settings_panel_fills_host(viewport) {
            let scrim_rect = bento_nano_style::Rect {
                x: 0.0,
                y: 0.0,
                width: viewport.width,
                height: viewport.height,
            };
            self.fill_rounded_rect(
                scrim_rect,
                with_alpha(bento_nano_style::Color::BLACK, 0.50),
                bento_nano_style::BorderRadius::ZERO,
            )?;
        }

        let panel = settings_panel_rect_m1(viewport);

        let open_progress = app.settings_open_animation_progress_at(settings_now_ms);
        let open_eased = crate::state::settings_open_animation_ease(open_progress);
        let open_scale = crate::state::settings_open_animation_scale(open_eased);
        let open_transform_active = (open_scale - 1.0).abs() > f32::EPSILON;
        if open_transform_active {
            let open_transform = scale_about_rect_center_matrix(base_scale, panel, open_scale);
            self.set_logical_transform_override(Some(open_transform))?;
        }

        let settings_paint = (|| -> Result<(), RenderError> {
            // 2) Panel card — blur the desktop snapshot, reapply the overlay's
            // 50% dimming inside the clipped card, then add the theme glass.
            // This mirrors CSS backdrop-filter ordering and avoids both sharp
            // text bleed and the old opaque black slab.
            if has_panel_backdrop {
                self.fill_frosted_rect(
                    panel,
                    with_alpha(bento_nano_style::Color::BLACK, 0.50),
                    panel_radius,
                )?;
                self.fill_rounded_rect(panel, panel_bg, panel_radius)?;
            } else {
                self.fill_rounded_rect(panel, panel_fallback_bg, panel_radius)?;
            }
            let panel_border = bento_nano_style::Rect {
                x: panel.x + 0.5,
                y: panel.y + 0.5,
                width: (panel.width - 1.0).max(0.0),
                height: (panel.height - 1.0).max(0.0),
            };
            self.stroke_rounded_rect(
                panel_border,
                palette.border_expanded,
                bento_nano_style::BorderRadius::all((SETTINGS_PANEL_RADIUS - 0.5).max(0.0)),
                1.0,
            )?;

            // 3) Header (sticky, 52 DIP) — title + close ×.
            let header = settings_header_rect(viewport);
            let title_rect = bento_nano_style::Rect {
                x: header.x + SETTINGS_ROW_PAD_X,
                y: header.y + (header.height - 20.0) * 0.5,
                width: header.width * 0.5,
                height: 20.0,
            };
            self.draw_text_no_wrap_with_style(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_TITLE),
                title_rect,
                title_color,
                16.0,
                600,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Leading,
                    v: dwrite::VAlign::Center,
                },
            )?;
            let close_rect = settings_close_button_rect_m1(viewport);
            let close_chrome = panel_header_button_chrome(
                palette,
                PanelHeaderButtonKind::Close,
                app.settings_close_hover.get(),
            );
            if let Some(background) = close_chrome.background {
                self.fill_rounded_rect(close_rect, background, BorderRadius::all(8.0))?;
            }
            self.draw_icon_glyph(
                IconKind::X.as_str(),
                centered_square_rect(close_rect, 16.0),
                close_chrome.glyph,
            )?;
            let header_hairline = bento_nano_style::Rect {
                x: header.x,
                y: header.bottom() - 1.0,
                width: header.width,
                height: 1.0,
            };
            self.fill_rounded_rect(header_hairline, divider_color, BorderRadius::ZERO)?;

            // 4) Body — paint rows scrolled by `app.scroll_offset_y`.
            //
            // M1b (S-02): clip the whole body band so partial rows at the top/bottom
            // edge are masked by the sticky header/footer instead of bleeding past
            // them (rows fully offscreen still early-skip via `row_visible`, but a
            // row straddling the edge now clips at the pixel boundary).
            //
            // CRITICAL — the body paint propagates with `?`, so a naive
            // `push; …?; pop` would leak the clip on the first D2D error and
            // corrupt the device context. We capture the body paint into a closure
            // result and ALWAYS run `pop_clip()` before propagating, keeping the
            // push/pop balanced across every early return. (No Drop guard: a
            // fallible pop in Drop is disallowed; this stays `?`-clean + panic-free.)
            let body = settings_body_rect(viewport);
            let settings_context = SettingsRenderContext {
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
                settings_now_ms,
                caret_on,
            };
            self.push_clip(body)?;
            let body_paint = (|| -> Result<(), RenderError> {
                let scroll = app.scroll_offset_y.get();

                let source_count =
                    self.draw_settings_general_paths(app, settings_context, scroll)?;
                let (scroll, crash_restart_on, safe_start_on) = self
                    .draw_settings_performance_startup(
                        app,
                        settings_context,
                        scroll,
                        source_count,
                    )?;
                let (has_retry, has_error) = self.draw_settings_stealth(
                    app,
                    settings_context,
                    scroll,
                    crash_restart_on,
                    safe_start_on,
                )?;
                let updater_flags = SettingsBodyFlags::new(
                    crash_restart_on,
                    safe_start_on,
                    has_retry,
                    has_error,
                    crate::business::settings::updater_card::updater_height_kind(
                        &app.settings_updater_status.borrow(),
                    ),
                );
                let updater_flags =
                    self.draw_settings_updater(app, settings_context, scroll, updater_flags)?;
                let backup_flags =
                    self.draw_settings_backup(app, settings_context, scroll, updater_flags)?;
                self.draw_settings_encryption(app, settings_context, scroll, backup_flags)?;
                let plugin_flags =
                    self.draw_settings_plugins(app, settings_context, scroll, backup_flags)?;
                self.draw_settings_appearance(
                    app,
                    settings_context,
                    scroll,
                    plugin_flags,
                    source_count,
                )?;

                Ok(())
            })();
            // Balance the body clip BEFORE propagating any body-paint error so the
            // device context is never left with a dangling PushAxisAlignedClip.
            self.pop_clip()?;
            body_paint?;

            // 5) Footer (sticky, 56 DIP) — [取消] [保存(accent)]. Painted AFTER the
            // body clip is popped so the sticky footer is never masked by it.
            let footer = settings_footer_rect(viewport);
            let footer_hairline = bento_nano_style::Rect {
                x: footer.x,
                y: footer.y,
                width: footer.width,
                height: 1.0,
            };
            self.fill_rounded_rect(footer_hairline, divider_color, BorderRadius::ZERO)?;
            let cancel_btn = settings_cancel_button_rect(viewport);
            if let Some(error) = app.settings_save_error.borrow().as_ref() {
                self.draw_text_with_style(
                    error.as_str(),
                    bento_nano_style::Rect {
                        x: footer.x + SETTINGS_ROW_PAD_X,
                        y: footer.y + 8.0,
                        width: (cancel_btn.x - footer.x - SETTINGS_ROW_PAD_X * 2.0).max(0.0),
                        height: footer.height - 16.0,
                    },
                    with_alpha(palette.accent_red, 0.98),
                    10.5,
                    500,
                    1.25,
                )?;
            }
            self.fill_rounded_rect(cancel_btn, controls.fill, btn_radius)?;
            self.stroke_rounded_rect(cancel_btn, controls.border, btn_radius, 1.0)?;
            self.draw_settings_button_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTING_CANCEL),
                cancel_btn,
                label_color,
                13.0,
                500,
            )?;
            // M1a 2026-05-29 — Save dims to ~0.4 alpha when no toggle has been
            // touched since the panel opened, mirroring Tauri `disabled={!dirty()}`
            // at `SettingsPanel.tsx:799`. The hit-tester treats the dimmed button
            // as a no-op (`SaveSettings` dispatch arm short-circuits when
            // `!settings_dirty`); Cancel stays always-active.
            let save_btn = settings_save_button_rect(viewport);
            let dirty = app.settings_dirty.get();
            let save_fill = if dirty {
                accent_on
            } else {
                controls.disabled_fill
            };
            let save_text = if dirty {
                controls.on_accent
            } else {
                controls.disabled_text
            };
            self.fill_rounded_rect(save_btn, save_fill, btn_radius)?;
            self.stroke_rounded_rect(
                save_btn,
                if dirty {
                    accent_on
                } else {
                    controls.disabled_border
                },
                btn_radius,
                1.0,
            )?;
            self.draw_settings_button_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTING_SAVE),
                save_btn,
                save_text,
                13.0,
                500,
            )?;

            // K1 modal-opener paint paths — orphan-alive per Ruling B. They never
            // fire from M1 hit-test (no SettingsHit→Open* arms) but compile and
            // can still surface via keyboard shortcuts.
            if app.settings_keybindings_open.get() {
                self.draw_keybindings_modal(app)?;
            }
            // M1h (2026-05-29) — the plugins MODAL gate (`if app.settings_plugins_open
            // { self.draw_plugins_modal(app) }`) was removed: the Plugins surface is
            // now an always-inline §11 section painted inside the scrollable body
            // (see the M1h block in the body-paint closure above). `draw_plugins_modal`
            // + `settings_plugins_open` were deleted.
            // M6-UI (2026-05-29) — the Wave J1b swatch-popup paint
            // (`if app.theme_picker_open { paint_into(ThemePickerAdapter, …) }`)
            // was removed: §3 Appearance is now an always-inline grid painted by
            // the M6-UI block inside the scrollable body-paint closure above
            // (group headings + 17 ThemeCards + accent swatch row), re-skinning
            // live off `app.active_theme_tauri()`.

            Ok(())
        })();
        if open_transform_active {
            self.set_logical_transform_override(None)?;
        }
        settings_paint
    }
}
