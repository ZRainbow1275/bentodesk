use super::*;
use crate::settings_panel::*;
use crate::widgets::toggle_switch::toggle_switch_in_rect;

impl Renderer {
    pub(super) fn draw_settings_general_paths(
        &mut self,
        app: &AppState,
        context: SettingsRenderContext,
        scroll: f32,
    ) -> Result<usize, RenderError> {
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
            ..
        } = context;
        let row_visible =
            |row: Rect, body: Rect| -> bool { row.bottom() > body.y && row.y < body.bottom() };
        let general_label = settings_general_label_rect(viewport, scroll);
        if row_visible(general_label, body) {
            self.draw_settings_group_title(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_GENERAL),
                general_label,
                palette.text_muted,
            )?;
        }

        // Toggle row labels by index (0..=4). M1a 2026-05-29: row 4 text was
        // retargeted to Tauri "智能自动分组" (still id 116, const name
        // unchanged); row 5 swapped from the bespoke speed-mode id 117 to the
        // new Tauri "便携模式" id 141 (`SETTING_PORTABLE_MODE`).
        let toggle_labels: [u16; 5] = [
            bento_nano_style::i18n_zh_cn::ids::SETTING_DESKTOP_EMBED.0,
            bento_nano_style::i18n_zh_cn::ids::SETTING_AUTOSTART.0,
            bento_nano_style::i18n_zh_cn::ids::SETTING_SHOW_IN_TASKBAR.0,
            bento_nano_style::i18n_zh_cn::ids::SETTING_SMART_LAYOUT.0,
            bento_nano_style::i18n_zh_cn::ids::SETTING_PORTABLE_MODE.0,
        ];

        for index in 0..SETTINGS_TOP_TOGGLE_COUNT {
            let row = settings_top_toggle_row_rect(viewport, scroll, index);
            if !row_visible(row, body) {
                continue;
            }
            // Row label.
            let label_rect = bento_nano_style::Rect {
                x: row.x,
                y: row.y + (row.height - 16.0) * 0.5,
                width: row.width * 0.6,
                height: 16.0,
            };
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::StringId(toggle_labels[index as usize])),
                label_rect,
                label_color,
            )?;
            // Toggle.
            let hit = settings_top_toggle_hit_rect(viewport, scroll, index);
            let on = match index {
                0 => app.setting_desktop_embed.get(),
                1 => app.setting_autostart.get(),
                2 => app.setting_show_in_taskbar.get(),
                3 => app.setting_smart_layout.get(),
                4 => app.setting_portable_mode.get(),
                _ => false,
            };
            let switch = toggle_switch_in_rect(hit);
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

        // Language row.
        let locale_row = settings_language_row_rect(viewport, scroll);
        if row_visible(locale_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: locale_row.x,
                y: locale_row.y + (locale_row.height - 16.0) * 0.5,
                width: locale_row.width * 0.45,
                height: 16.0,
            };
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTING_LANGUAGE),
                label_rect,
                label_color,
            )?;
            let chip = settings_language_chip_rect(viewport, scroll);
            self.fill_rounded_rect(chip, chip_bg, chip_radius)?;
            let chip_hairline = bento_nano_style::Rect {
                x: chip.x,
                y: chip.y,
                width: chip.width,
                height: 1.0,
            };
            self.fill_rounded_rect(chip_hairline, chip_border, BorderRadius::ZERO)?;
            let locale_label = if bento_nano_style::current_locale_is(&bento_nano_style::EN_US) {
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::LOCALE_LABEL_EN_US)
            } else {
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::LOCALE_LABEL_ZH_CN)
            };
            self.draw_settings_text_no_wrap(
                locale_label,
                settings_language_chip_label_rect(viewport, scroll),
                title_color,
            )?;
            self.draw_settings_text_no_wrap(
                "▾",
                settings_language_chevron_rect(viewport, scroll),
                label_color,
            )?;
        }

        // §4 DisplayMode group (G3 parity 2026-06-01) — promoted out of the
        // General band into its own `settings-group` between §3 Appearance and
        // §5 Performance. Because §4 roots at the FIXED source-reserve baseline
        // (it anchors off §3 Appearance, like Performance §5), it must paint with
        // the reserve-FOLDED `scroll` — so the paint block lives AFTER the fold,
        // adjacent to the §3 Appearance block near the end of this closure (paint
        // ==hit SSoT; see the `§4 DisplayMode` block below the Appearance grid).

        // ── Round-2 M2 sections ──────────────────────────────────────────

        let paths_label = settings_paths_label_rect(viewport, scroll);
        if row_visible(paths_label, body) {
            self.draw_settings_group_title(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_PATHS),
                paths_label,
                palette.text_muted,
            )?;
        }

        // 桌面源 label (M1i fidelity — Tauri `.settings-row__label` ABOVE the
        // `.desktop-source-list`; refresh button is now the list's LAST child,
        // painted after the cards below, `SettingsPanel.tsx:317-361`).
        let source_count = app.desktop_sources.borrow().len();
        let sources_label = settings_sources_label_rect(viewport, scroll);
        if row_visible(sources_label, body) {
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SECTION_DESKTOP_SOURCES),
                sources_label,
                label_color,
            )?;
        }

        // M1i fidelity — `.desktop-source-card` geometry/typography translated
        // 1:1 from `SettingsPanel.css:665-770`:
        //   card  : radius 8, bg white@4%, border 1px solid border_zen,
        //           padding 8/10, icon→body gap 10, inter-card gap 6
        //   icon  : 28×28 CIRCLE, white initial, font 12 semibold, per-kind bg
        //           @0.75 (User=blue Public=green OneDrive=sky Custom=purple)
        //   body  : label 13 medium text_primary, path 11 MONOSPACE text_muted
        //           with ellipsis trim, internal gap 2
        //   badge : green@0.18 bg, accent_green text, 9px semibold UPPERCASE,
        //           padding 2/8, radius 10, AUTO width right-aligned, centred
        // The list snapshot is owned by AppState and refreshed on open /
        // RefreshDesktopSources, never built per-frame (architecture §10).
        const CARD_PAD_X: f32 = 10.0;
        const ICON_SIZE: f32 = 28.0;
        const ICON_BODY_GAP: f32 = 10.0;
        const BODY_GAP: f32 = 2.0;
        const LABEL_LINE_H: f32 = 16.0;
        const PATH_LINE_H: f32 = 14.0;
        let card_radius = bento_nano_style::BorderRadius::all(8.0);
        let card_bg = palette.neutral_overlay(0.04);
        let card_border = palette.border_zen;
        let sources = app.desktop_sources.borrow();
        let visible_sources = sources.len().min(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
        for index in 0..visible_sources {
            let row = settings_source_row_rect(viewport, scroll, index as u8);
            if !row_visible(row, body) {
                continue;
            }
            let (kind, path_text, watched) = &sources[index];
            // Card surface + 1px hairline border (Tauri `border: 1px solid
            // var(--border-zen)` — the nano card previously had NO stroke).
            self.fill_rounded_rect(row, card_bg, card_radius)?;
            self.stroke_rounded_rect(row, card_border, card_radius, 1.0)?;
            // 28×28 CIRCLE with the kind initial (was a 24×24 rounded square).
            // A square fill_rounded_rect with radius = half-side is a true
            // circle. Per-kind LITERAL rgba @0.75 (palette.accent_purple is
            // 139,92,246 — NOT the 168,85,247 Tauri purple — so Custom uses a
            // literal; OneDrive's sky 14,165,233 has no palette token either).
            let icon_rect = bento_nano_style::Rect {
                x: row.x + CARD_PAD_X,
                y: row.y + (row.height - ICON_SIZE) * 0.5,
                width: ICON_SIZE,
                height: ICON_SIZE,
            };
            let (icon_bg, icon_glyph, kind_label_id) = match kind {
                bento_nano_backend::desktop_sources::DesktopSourceKind::User => (
                    bento_nano_style::Color::from_u8(59, 130, 246, 191), // 0.75
                    "U",
                    bento_nano_style::i18n_zh_cn::ids::SOURCE_PRIMARY_LABEL,
                ),
                bento_nano_backend::desktop_sources::DesktopSourceKind::Public => (
                    bento_nano_style::Color::from_u8(34, 197, 94, 191),
                    "P",
                    bento_nano_style::i18n_zh_cn::ids::SOURCE_PUBLIC_LABEL,
                ),
                bento_nano_backend::desktop_sources::DesktopSourceKind::OneDrive => (
                    bento_nano_style::Color::from_u8(14, 165, 233, 191), // sky (fixed)
                    "O",
                    bento_nano_style::i18n_zh_cn::ids::SOURCE_ONEDRIVE_LABEL,
                ),
                bento_nano_backend::desktop_sources::DesktopSourceKind::Custom => (
                    bento_nano_style::Color::from_u8(168, 85, 247, 191), // purple (fixed)
                    "C",
                    bento_nano_style::i18n_zh_cn::ids::SOURCE_CUSTOM_LABEL,
                ),
            };
            self.fill_rounded_rect(
                icon_rect,
                icon_bg,
                bento_nano_style::BorderRadius::all(ICON_SIZE * 0.5),
            )?;
            self.draw_text_no_wrap_with_style(
                icon_glyph,
                icon_rect,
                bento_nano_style::Color::WHITE,
                12.0,
                600,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
            // Body column (flex:1, gap 2): label line on top, path line below,
            // the pair vertically centred against the icon.
            let body_x = icon_rect.right() + ICON_BODY_GAP;
            // Reserve room on the right for the badge so the path never runs
            // under it (Tauri's flex `min-width:0` body shrinks for the badge).
            let badge_reserve: f32 = if *watched { 76.0 } else { 0.0 };
            let body_w = (row.right() - CARD_PAD_X - badge_reserve - body_x).max(1.0);
            let block_h = LABEL_LINE_H + BODY_GAP + PATH_LINE_H;
            let body_top = row.y + (row.height - block_h) * 0.5;
            let label_rect = bento_nano_style::Rect {
                x: body_x,
                y: body_top,
                width: body_w,
                height: LABEL_LINE_H,
            };
            self.draw_text_with_style(
                bento_nano_style::t(kind_label_id),
                label_rect,
                title_color,
                13.0,
                500,
                1.0,
            )?;
            // Path line — REAL resolved path, MONOSPACE, ellipsis-trimmed.
            let path_rect = bento_nano_style::Rect {
                x: body_x,
                y: body_top + LABEL_LINE_H + BODY_GAP,
                width: body_w,
                height: PATH_LINE_H,
            };
            self.draw_text_monospace_ellipsis(
                path_text.as_str(),
                path_rect,
                palette.text_muted,
                11.0,
            )?;
            // Watched badge — translucent green tint, accent_green text, auto
            // width right-aligned, vertically centred (was a solid-green fill
            // with WHITE text in a fixed 56×22 rect).
            if *watched {
                let badge_text =
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SOURCE_WATCHED_BADGE);
                let badge_upper = badge_text.to_uppercase();
                // Auto width: shrink-to-fit the text + 8px padding each side.
                // CJK glyphs ≈ font_size wide, Latin ≈ font_size*0.62, plus the
                // 0.8px letter-spacing Tauri applies per glyph.
                const BADGE_FONT: f32 = 9.0;
                const BADGE_PAD_X: f32 = 8.0;
                const BADGE_LETTER_SPACING: f32 = 0.8;
                let glyph_count = badge_upper.chars().count() as f32;
                let text_w: f32 = badge_upper
                    .chars()
                    .map(|c| {
                        if (c as u32) > 0x2E80 {
                            BADGE_FONT
                        } else {
                            BADGE_FONT * 0.62
                        }
                    })
                    .sum::<f32>()
                    + BADGE_LETTER_SPACING * glyph_count;
                let badge_w = text_w + BADGE_PAD_X * 2.0;
                let badge_h: f32 = 16.0; // 2px pad + ~12 line box
                let badge_rect = bento_nano_style::Rect {
                    x: row.right() - CARD_PAD_X - badge_w,
                    y: row.y + (row.height - badge_h) * 0.5,
                    width: badge_w,
                    height: badge_h,
                };
                let badge_bg = with_alpha(palette.accent_green, 0.18);
                self.fill_rounded_rect(
                    badge_rect,
                    badge_bg,
                    bento_nano_style::BorderRadius::all(10.0),
                )?;
                self.draw_text_no_wrap_with_style(
                    badge_upper.as_str(),
                    badge_rect,
                    palette.accent_green,
                    BADGE_FONT,
                    600,
                    1.0,
                    dwrite::TextAlign {
                        h: dwrite::HAlign::Center,
                        v: dwrite::VAlign::Center,
                    },
                )?;
            }
        }
        drop(sources);

        // M1i fidelity — empty `.desktop-source-empty` placeholder (italic,
        // 11px, text_muted) when no desktop sources resolve. nano's refresh is
        // synchronous (no async loading frame), so Tauri's "…" loading glyph is
        // N/A by construction — there is never a loading state to paint.
        if visible_sources == 0 {
            let label = settings_sources_label_rect(viewport, scroll);
            let empty_rect = bento_nano_style::Rect {
                x: label.x + 4.0,
                y: label.bottom() + 6.0,
                width: (label.width - 8.0).max(1.0),
                height: 12.0,
            };
            if row_visible(empty_rect, body) {
                // No italic system face is loaded; the muted tone + xs size
                // reads as the de-emphasised placeholder Tauri renders italic.
                self.draw_text_with_style(
                    bento_nano_style::t(
                        bento_nano_style::i18n_zh_cn::ids::SOURCE_EMPTY_PLACEHOLDER,
                    ),
                    empty_rect,
                    palette.text_muted,
                    11.0,
                    400,
                    1.0,
                )?;
            }
        }

        // M1i fidelity — refresh (`↻`) button: LAST child of the list,
        // right-anchored BELOW the cards / placeholder (`align-self:flex-end`).
        // Secondary-button style: chip_bg fill, radius, centred 14px glyph.
        let refresh_btn = settings_sources_refresh_button_rect(viewport, scroll, source_count);
        if row_visible(refresh_btn, body) {
            self.fill_rounded_rect(
                refresh_btn,
                chip_bg,
                bento_nano_style::BorderRadius::all(6.0),
            )?;
            self.stroke_rounded_rect(
                refresh_btn,
                chip_border,
                bento_nano_style::BorderRadius::all(6.0),
                1.0,
            )?;
            // U+21BB CLOCKWISE OPEN CIRCLE ARROW — the refresh glyph, centred.
            self.draw_text_no_wrap_with_style(
                "\u{21BB}",
                refresh_btn,
                title_color,
                14.0,
                400,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }

        // 桌面路径 label + input (reflows below the live source stack).
        let path_label = settings_desktop_path_label_rect(viewport, scroll, source_count);
        if row_visible(path_label, body) {
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SECTION_DESKTOP_PATH),
                path_label,
                label_color,
            )?;
        }
        // Input/textarea boxes keep the radius-10 surface the M2 layout shipped.
        let input_box_radius = bento_nano_style::BorderRadius::all(10.0);
        let path_input = settings_desktop_path_input_rect(viewport, scroll, source_count);
        if row_visible(path_input, body) {
            self.fill_rounded_rect(path_input, chip_bg, input_box_radius)?;
            let path_text = app.desktop_path_draft.borrow();
            let text_rect = bento_nano_style::Rect {
                x: path_input.x + 12.0,
                y: path_input.y + (path_input.height - 16.0) * 0.5,
                width: (path_input.width - 24.0).max(0.0),
                height: 16.0,
            };
            self.draw_settings_text_no_wrap(path_text.as_str(), text_rect, title_color)?;
            drop(path_text);
        }

        // 监控值 label + textarea (reflows below the live source stack).
        let watch_label = settings_watch_label_rect(viewport, scroll, source_count);
        if row_visible(watch_label, body) {
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SECTION_WATCH_VALUES),
                watch_label,
                label_color,
            )?;
        }
        let watch_area = settings_watch_textarea_rect(viewport, scroll, source_count);
        if row_visible(watch_area, body) {
            self.fill_rounded_rect(watch_area, chip_bg, input_box_radius)?;
            let watch_text = app.watch_paths_draft.borrow();
            if watch_text.is_empty() {
                // Hint placeholder.
                let hint_rect = bento_nano_style::Rect {
                    x: watch_area.x + 12.0,
                    y: watch_area.y + 10.0,
                    width: (watch_area.width - 24.0).max(0.0),
                    height: 16.0,
                };
                self.draw_settings_text(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::WATCH_HINT_LINE_EACH),
                    hint_rect,
                    label_color,
                )?;
            } else {
                let text_rect = bento_nano_style::Rect {
                    x: watch_area.x + 12.0,
                    y: watch_area.y + 10.0,
                    width: (watch_area.width - 24.0).max(0.0),
                    height: (watch_area.height - 20.0).max(0.0),
                };
                self.draw_settings_text(watch_text.as_str(), text_rect, title_color)?;
            }
            drop(watch_text);
        }
        Ok(source_count)
    }
}
