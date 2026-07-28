use super::*;
use crate::settings_panel::*;
use crate::state::ZoneDisplayMode;

impl Renderer {
    pub(super) fn draw_settings_appearance(
        &mut self,
        app: &AppState,
        context: SettingsRenderContext,
        scroll: f32,
        plugin_flags: SettingsBodyFlags,
        source_count: usize,
    ) -> Result<(), RenderError> {
        let SettingsRenderContext {
            viewport,
            body,
            palette,
            title_color,
            label_color,
            accent_on,
            chip_bg,
            chip_border,
            settings_now_ms,
            ..
        } = context;
        let row_visible =
            |row: Rect, body: Rect| -> bool { row.bottom() > body.y && row.y < body.bottom() };
        // ── M6-UI / G3 parity — §3 Appearance inline theme grid (`SettingsPanel.tsx:396-536`) ──
        //
        // G3 parity (2026-06-01): §3 Appearance now flows between §2 Paths and
        // §4 DisplayMode (Tauri body order General → Paths → **Appearance** →
        // DisplayMode → Performance), no longer LAST after Plugins. The geometry
        // helpers (`settings_appearance_label_rect` et al.) re-anchor off the §2
        // 监控值 textarea bottom, so this paint block lands at its new position
        // automatically (paint==hit SSoT) even though it stays here in source
        // order. The grid geometry (group headings + 17 ThemeCards + accent
        // swatch row) is owned by `theme_picker::appearance_layout`; the section
        // anchor + content width come from `settings_panel`. Selecting a card re-skins
        // the app live (the active card draws a 2-DIP accent-blue border + a
        // 10%-blue fill tint, compared against `app.active_theme_id`). The
        // accent swatch row is the editable accent picker (Control B MVP).
        //
        // Developer Options (custom-theme textarea + Import/Export) is DEFERRED
        // (no nano keyboard/text-input infra + no JSON theme parser) — see the
        // M6-UI carve-out note; no dead toggle is painted.
        use crate::settings_panel::{
            settings_appearance_grid_origin, settings_appearance_inner_width,
            settings_appearance_label_rect, settings_appearance_picker_label_rect,
        };
        use crate::theme_picker::{
            self as tp, AppearanceLayout, BUILTIN_THEMES, SWATCH_BLOCK_RADIUS, SWATCH_INNER_GAP,
            THEME_CARD_BORDER, THEME_CARD_RADIUS, THEME_GROUP_ORDER,
        };
        // Live theme id (the active card highlight) — borrowed once.
        let active_theme_id = app.active_theme_id.borrow().clone();
        let appearance_hover = app.settings_appearance_hover.get();
        let accent_value = app.settings_accent_editor_value();
        // Group title — 外观 / Appearance.
        let appearance_label = settings_appearance_label_rect(viewport, scroll, &plugin_flags);
        if row_visible(appearance_label, body) {
            self.draw_settings_group_title(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_APPEARANCE),
                appearance_label,
                label_color,
            )?;
        }
        // "选择主题 / Choose Theme" picker label.
        let picker_label = settings_appearance_picker_label_rect(viewport, scroll, &plugin_flags);
        if row_visible(picker_label, body) {
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::THEME_PICKER_LABEL),
                picker_label,
                label_color,
            )?;
        }
        // Grid layout — body-width-driven, Copy, allocation-free.
        let appearance_origin = settings_appearance_grid_origin(viewport, scroll, &plugin_flags);
        let appearance_inner_w = settings_appearance_inner_width(viewport);
        let appearance: AppearanceLayout =
            tp::appearance_layout(appearance_origin, appearance_inner_w);
        // surface_subtle = rgba(white, 0.04) card bg (live theme). Active card
        // overrides to accent-blue@0.10 + a 2-DIP accent-blue rounded border.
        let card_radius = bento_nano_style::BorderRadius::all(THEME_CARD_RADIUS);
        let swatch_radius = bento_nano_style::BorderRadius::all(SWATCH_BLOCK_RADIUS);
        // Group headings — Tauri `.theme-group__title`: UPPERCASE,
        // letter-spacing 1px, font-size 10px, weight 600, color text-muted.
        // `draw_text_tracked` upper-cases (no-op for CJK) + applies the 1-DIP
        // per-glyph tracking via DWrite SetCharacterSpacing (both locales).
        for (group_pos, group) in THEME_GROUP_ORDER.iter().enumerate() {
            let heading = appearance.group_headings[group_pos];
            if row_visible(heading, body) {
                self.draw_text_tracked(
                    bento_nano_style::t(group.heading_id()),
                    heading,
                    palette.text_muted,
                    10.0,
                    600,
                    1.0,
                )?;
            }
        }
        // 17 ThemeCards (walk the preset table; rects indexed by preset id).
        for preset in BUILTIN_THEMES.iter() {
            let i = preset.id as usize;
            let card = appearance.cards[i];
            if !row_visible(card, body) {
                continue;
            }
            let is_active = preset.theme_id == active_theme_id.as_str();
            let is_hovered = appearance_hover == Some(tp::AppearanceHit::Card(preset.id));
            let selection_progress =
                app.theme_card_selection_progress_at(preset.id, is_active, settings_now_ms);
            let card_chrome = settings_theme_card_chrome(palette, selection_progress, is_hovered);
            // Card surface.
            self.fill_rounded_rect(card, card_chrome.fill, card_radius)?;
            // Active card border — 2-DIP accent-blue. Tauri's CSS `border` is a
            // fully-inset border-box; D2D strokes centred on the geometric edge,
            // so the rect is inset by half the stroke width (1 DIP) on all sides
            // and the radius shrinks to stay concentric — no bleed past the card.
            if let Some(border_color) = card_chrome.border {
                let inset = THEME_CARD_BORDER * 0.5;
                let border_rect = bento_nano_style::Rect {
                    x: card.x + inset,
                    y: card.y + inset,
                    width: (card.width - THEME_CARD_BORDER).max(0.0),
                    height: (card.height - THEME_CARD_BORDER).max(0.0),
                };
                let border_radius =
                    bento_nano_style::BorderRadius::all((THEME_CARD_RADIUS - inset).max(0.0));
                self.stroke_rounded_rect(
                    border_rect,
                    border_color,
                    border_radius,
                    THEME_CARD_BORDER,
                )?;
            }
            // 40×40 swatch block — 4 quadrant fills (3-DIP gutter == gap:3px).
            let block = appearance.swatch_blocks[i];
            // Block pad behind the quadrants (rounded clip silhouette).
            self.fill_rounded_rect(block, palette.surface_subtle, swatch_radius)?;
            // Quadrants — Tauri `.theme-card__swatches { border-radius:8;
            // overflow:hidden }` masks SHARP-cornered quadrants behind an 8-DIP
            // rounded square. No rounded-clip primitive exists (PushAxisAlignedClip
            // is rectangular), so each corner quadrant rounds ONLY its single
            // OUTER corner to 8 (TL→top-left, TR→top-right, BL→bottom-left,
            // BR→bottom-right) and stays square at the inner centre cross — the
            // visible-correct per-corner approximation via `fill_partial_rounded_rect`.
            const QUADRANT_OUTER_CORNER: [[bool; 4]; 4] = [
                [true, false, false, false], // 0 = TL
                [false, true, false, false], // 1 = TR
                [false, false, false, true], // 2 = BL
                [false, false, true, false], // 3 = BR
            ];
            let quads = tp::thumbnail_swatch_quadrants(block, SWATCH_INNER_GAP);
            let mut q = 0usize;
            while q < 4 {
                self.fill_partial_rounded_rect(
                    quads[q],
                    preset.swatch_colors[q],
                    SWATCH_BLOCK_RADIUS,
                    QUADRANT_OUTER_CORNER[q],
                )?;
                q += 1;
            }
            // Name label below the swatch — Tauri `.theme-card__label`:
            // text-align:center, 10px, color text-secondary, single line.
            let label_rect = bento_nano_style::Rect {
                x: card.x,
                y: block.bottom() + crate::theme_picker::THEME_CARD_SWATCH_LABEL_GAP,
                width: card.width,
                height: crate::theme_picker::CARD_LABEL_HEIGHT,
            };
            // #1 step 13 (2026-06-02) — was the lone `draw_text_centered` helper;
            // now folded into the unified styled path with explicit center/center.
            self.draw_text_no_wrap_with_style(
                bento_nano_style::t(preset.name_id),
                label_rect,
                palette.text_secondary,
                10.0,
                400,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }
        // Accent row — Tauri's single compact colour input, backed by
        // Nano's existing native ChooseColorW producer.
        if row_visible(appearance.accent_row, body) {
            let accent_picker = appearance.accent_picker;
            let accent_label_rect = bento_nano_style::Rect {
                x: appearance.accent_row.x,
                y: appearance.accent_row.y,
                width: (accent_picker.x - appearance.accent_row.x - 8.0).max(0.0),
                height: appearance.accent_row.height,
            };
            self.draw_settings_text_no_wrap(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_ACCENT_COLOR),
                accent_label_rect,
                label_color,
            )?;
            let accent_picker_hovered = appearance_hover == Some(tp::AppearanceHit::AccentPicker);
            let accent_picker_radius = bento_nano_style::BorderRadius::all(6.0);
            self.fill_rounded_rect(
                accent_picker,
                if accent_picker_hovered {
                    with_alpha(accent_on, 0.10)
                } else {
                    chip_bg
                },
                accent_picker_radius,
            )?;
            self.stroke_rounded_rect(
                accent_picker,
                if accent_picker_hovered {
                    with_alpha(accent_on, 0.72)
                } else {
                    chip_border
                },
                accent_picker_radius,
                if accent_picker_hovered { 1.5 } else { 1.0 },
            )?;
            let preview = bento_nano_style::Rect {
                x: accent_picker.x + 3.0,
                y: accent_picker.y + 3.0,
                width: accent_picker.width - 6.0,
                height: accent_picker.height - 6.0,
            };
            let preview_color = parse_hex_color(accent_value.as_str())
                .unwrap_or_else(|| with_alpha(palette.text_muted, 0.35));
            self.fill_rounded_rect(
                preview,
                preview_color,
                bento_nano_style::BorderRadius::all(4.0),
            )?;
        }

        // ── §4 Zone Display Mode ──
        // Tauri `SettingsPanel.tsx:538-598` uses a left explanatory
        // label/hint and a right-aligned vertical stack of three full
        // option cards. The shared settings-panel rects also drive
        // hit-testing, so the complete card remains clickable.
        let display_mode_label =
            crate::settings_panel::settings_display_mode_label_rect(viewport, scroll);
        if row_visible(display_mode_label, body) {
            self.draw_settings_group_title(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_DISPLAY_MODE),
                display_mode_label,
                palette.text_muted,
            )?;
        }
        let picker_row = settings_zone_display_mode_picker_row_rect(viewport, scroll);
        if row_visible(picker_row, body) {
            let copy_label = settings_display_mode_copy_label_rect(viewport, scroll);
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_DISPLAY_MODE_LABEL),
                copy_label,
                label_color,
            )?;
            let hint = settings_display_mode_hint_rect(viewport, scroll);
            self.draw_text_with_style(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_DISPLAY_MODE_HINT),
                hint,
                palette.text_muted,
                11.0,
                400,
                1.25,
            )?;
            let modes = [
                ZoneDisplayMode::Hover,
                ZoneDisplayMode::Always,
                ZoneDisplayMode::Click,
            ];
            let current = app.zone_display_mode.get();
            let radius_outer = BorderRadius::all(SETTINGS_RADIO_OUTER_D * 0.5);
            let radius_inner = BorderRadius::all(SETTINGS_RADIO_INNER_D * 0.5);
            let option_radius = BorderRadius::all(8.0);
            for index in 0..SETTINGS_ZONE_DISPLAY_MODE_COUNT {
                let mode = modes[index as usize];
                let option = crate::settings_panel::settings_zone_display_mode_radio_rect(
                    viewport, scroll, index,
                );
                if mode == current {
                    self.fill_rounded_rect(option, with_alpha(accent_on, 0.10), option_radius)?;
                    self.stroke_rounded_rect(
                        option,
                        with_alpha(accent_on, 0.35),
                        option_radius,
                        1.0,
                    )?;
                }
                let outer = settings_zone_display_mode_radio_outer_rect(viewport, scroll, index);
                let ring_color = if mode == current {
                    accent_on
                } else {
                    chip_border
                };
                self.stroke_rounded_rect(outer, ring_color, radius_outer, 1.0)?;
                if mode == current {
                    let inner =
                        settings_zone_display_mode_radio_inner_rect(viewport, scroll, index);
                    self.fill_rounded_rect(inner, accent_on, radius_inner)?;
                }
                // Full Tauri option copy via StringId 77/78/79.
                let label_id = match mode {
                    ZoneDisplayMode::Hover => bento_nano_style::i18n_zh_cn::ids::ZONE_MODE_HOVER,
                    ZoneDisplayMode::Always => bento_nano_style::i18n_zh_cn::ids::ZONE_MODE_ALWAYS,
                    ZoneDisplayMode::Click => bento_nano_style::i18n_zh_cn::ids::ZONE_MODE_CLICK,
                };
                let label = settings_zone_display_mode_radio_label_rect(viewport, scroll, index);
                self.draw_text_no_wrap_with_style(
                    bento_nano_style::t(label_id),
                    label,
                    title_color,
                    crate::settings_panel::SETTINGS_TEXT_LABEL_SIZE,
                    crate::settings_panel::SETTINGS_TEXT_LABEL_WEIGHT,
                    crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
                    dwrite::TextAlign::DEFAULT,
                )?;
            }
        }

        // Native D2D equivalent of Tauri's 4px WebKit scrollbar. The
        // thumb uses the exact live body flags so its size and position
        // remain truthful when source/backup/plugin rows change.
        let scrollbar_flags = plugin_flags.with_source_rows(source_count);
        let content_h = settings_body_content_height(viewport, &scrollbar_flags);
        if let Some(thumb) =
            settings_scrollbar_thumb_rect(viewport, content_h, app.scroll_offset_y.get())
        {
            self.fill_rounded_rect(
                thumb,
                with_alpha(palette.text_primary, 0.24),
                BorderRadius::all(SETTINGS_SCROLLBAR_W * 0.5),
            )?;
        }
        Ok(())
    }
}
