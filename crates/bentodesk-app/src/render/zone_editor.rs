use super::*;

impl Renderer {
    pub(super) fn draw_zone_editor_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        use crate::business::zone_editor::{ACCENT_PALETTE, CapsuleShapeChoice, CapsuleSizeChoice};

        let tauri_palette = app.active_theme_tauri();
        let chrome = zone_editor_geometry::ZoneEditorChrome::from_tauri_tokens(
            tauri_palette,
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = zone_editor_geometry::zone_editor_panel(viewport);
        // ZoneEditor is a compact top-level native dialog, not a wallpaper
        // surface. Its card must remain opaque: preserving the theme token's
        // translucent alpha here made desktop icons and labels legible through
        // every form row, visually resembling a broken browser overlay.
        self.fill_rounded_rect(
            panel,
            with_alpha(chrome.panel_background, 1.0),
            chrome.panel_radius,
        )?;
        self.stroke_rounded_rect(
            panel,
            with_alpha(chrome.body_color, 0.12),
            chrome.panel_radius,
            1.0,
        )?;
        let title_rect = bentodesk_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 16.0,
            width: 190.0_f32.min(panel.width - 90.0),
            height: 28.0,
        };
        // M6c — zone editor panel title (`h2`).
        self.draw_text_chromatic_title(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                "编辑区域"
            } else {
                "Edit zone"
            },
            title_rect,
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        let close_rect = zone_editor_geometry::zone_editor_close_rect(viewport);
        self.fill_rounded_rect(
            close_rect,
            with_alpha(chrome.body_color, 0.05),
            chrome.row_radius,
        )?;
        self.draw_icon_glyph(
            "x",
            centered_square_rect(close_rect, 14.0),
            chrome.muted_color,
        )?;
        let header = zone_editor_geometry::zone_editor_header_rect(viewport);
        self.fill_rounded_rect(
            bentodesk_style::Rect {
                x: header.x + 1.0,
                y: header.bottom() - 1.0,
                width: (header.width - 2.0).max(0.0),
                height: 1.0,
            },
            with_alpha(chrome.body_color, 0.08),
            BorderRadius::ZERO,
        )?;

        let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);
        let session = app.zone_editor.borrow();
        let label_rect = bentodesk_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 54.0,
            width: panel.width - 36.0,
            height: 14.0,
        };
        self.draw_text(
            if zh { "区域名称" } else { "Zone name" },
            label_rect,
            chrome.muted_color,
        )?;

        let input_rect = zone_editor_geometry::zone_editor_name_input_rect(viewport);
        self.fill_rounded_rect(input_rect, chrome.input_background, chrome.input_radius)?;
        if session.is_some() {
            self.stroke_rounded_rect(input_rect, chrome.accent_color, chrome.input_radius, 1.5)?;
        }

        let selected_size = session
            .as_ref()
            .map(|entry| CapsuleSizeChoice::parse(entry.draft_capsule_size.as_str()))
            .unwrap_or_default();
        let selected_shape = session
            .as_ref()
            .map(|entry| CapsuleShapeChoice::parse(entry.draft_capsule_shape.as_str()))
            .unwrap_or_default();
        let draft = session
            .as_ref()
            .map(|s| s.draft_name.as_str())
            .unwrap_or(if zh {
                "尚未选择区域"
            } else {
                "No zone selected"
            });
        let draft_rect = inset_rect(input_rect, 10.0);
        self.draw_text_no_wrap_with_style(
            draft,
            draft_rect,
            chrome.body_color,
            14.0,
            500,
            1.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
        )?;

        let icon_chip_rect = zone_editor_geometry::zone_editor_icon_rect(viewport);
        let icon_label_rect = bentodesk_style::Rect {
            x: panel.x + 18.0,
            y: icon_chip_rect.y + 3.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text(
            if zh { "图标" } else { "Icon" },
            icon_label_rect,
            chrome.muted_color,
        )?;
        self.fill_rounded_rect(icon_chip_rect, chrome.input_background, chrome.row_radius)?;
        let icon_value_rect = bentodesk_style::Rect {
            x: icon_chip_rect.x + 10.0,
            y: icon_chip_rect.y + 4.0,
            width: icon_chip_rect.width - 20.0,
            height: icon_chip_rect.height - 8.0,
        };
        let icon_value = session
            .as_ref()
            .map(|s| s.draft_icon.as_str())
            .unwrap_or("folder");
        self.draw_icon_glyph(
            icon_value,
            bentodesk_style::Rect {
                x: icon_chip_rect.x + 8.0,
                y: icon_chip_rect.y + 4.0,
                width: 18.0,
                height: 18.0,
            },
            chrome.body_color,
        )?;
        self.draw_text(
            localized_icon_wire_label(icon_value, zh),
            bentodesk_style::Rect {
                x: icon_value_rect.x + 24.0,
                width: (icon_value_rect.width - 24.0).max(0.0),
                ..icon_value_rect
            },
            chrome.body_color,
        )?;

        let accent_row_rect = zone_editor_geometry::zone_editor_accent_rect(viewport);
        let accent_label_rect = bentodesk_style::Rect {
            x: panel.x + 18.0,
            y: accent_row_rect.y + 3.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text(
            if zh { "强调色" } else { "Accent" },
            accent_label_rect,
            chrome.muted_color,
        )?;
        let selected_accent = session
            .as_ref()
            .and_then(|entry| entry.draft_accent_color.as_deref());
        let custom_selected = selected_accent.is_some_and(|hex| !ACCENT_PALETTE.contains(&hex));
        for index in 0..(ACCENT_PALETTE.len() + 2) {
            let Some(visual) =
                zone_editor_geometry::zone_editor_accent_option_visual_rect(viewport, index)
            else {
                continue;
            };
            let selected = if index == 0 {
                selected_accent.is_none()
            } else if index <= ACCENT_PALETTE.len() {
                selected_accent == Some(ACCENT_PALETTE[index - 1])
            } else {
                custom_selected
            };
            let border = if selected {
                chrome.accent_color
            } else {
                with_alpha(chrome.body_color, 0.16)
            };
            self.fill_rounded_rect(visual, border, chrome.swatch_radius)?;
            let inner = inset_rect(visual, 2.0);
            if index == 0 {
                self.fill_rounded_rect(inner, chrome.input_background, chrome.swatch_inner_radius)?;
                self.draw_text_no_wrap_with_style(
                    "×",
                    inner,
                    chrome.muted_color,
                    12.0,
                    500,
                    1.0,
                    dwrite::TextAlign {
                        h: dwrite::HAlign::Center,
                        v: dwrite::VAlign::Center,
                    },
                )?;
            } else if index <= ACCENT_PALETTE.len() {
                if let Some(color) = parse_hex_color(ACCENT_PALETTE[index - 1]) {
                    self.fill_rounded_rect(inner, color, chrome.swatch_inner_radius)?;
                }
            } else {
                self.fill_rounded_rect(inner, chrome.input_background, chrome.swatch_inner_radius)?;
                self.draw_text_no_wrap_with_style(
                    "+",
                    inner,
                    chrome.body_color,
                    12.0,
                    600,
                    1.0,
                    dwrite::TextAlign {
                        h: dwrite::HAlign::Center,
                        v: dwrite::VAlign::Center,
                    },
                )?;
            }
        }

        let grid_value_rect = zone_editor_geometry::zone_editor_grid_rect(viewport);
        let grid_label_rect = bentodesk_style::Rect {
            x: panel.x + 18.0,
            y: grid_value_rect.y + 3.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text(
            if zh { "网格列数" } else { "Grid" },
            grid_label_rect,
            chrome.muted_color,
        )?;
        let selected_columns = session
            .as_ref()
            .map(|entry| entry.draft_grid_columns)
            .unwrap_or(4);
        for columns in crate::business::zone_editor::GRID_COLUMNS_MIN
            ..=crate::business::zone_editor::GRID_COLUMNS_MAX
        {
            let Some(option) =
                zone_editor_geometry::zone_editor_grid_option_rect(viewport, columns)
            else {
                continue;
            };
            let selected = columns == selected_columns;
            self.fill_rounded_rect(
                option,
                if selected {
                    with_alpha(chrome.accent_color, 0.18)
                } else {
                    chrome.input_background
                },
                chrome.row_radius,
            )?;
            if selected {
                self.stroke_rounded_rect(
                    option,
                    with_alpha(chrome.accent_color, 0.82),
                    chrome.row_radius,
                    1.0,
                )?;
            }
            self.draw_text_no_wrap_with_style(
                grid_columns_label(columns, zh),
                option,
                if selected {
                    chrome.body_color
                } else {
                    chrome.muted_color
                },
                11.5,
                if selected { 600 } else { 450 },
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }

        let capsule_size_row = zone_editor_geometry::zone_editor_capsule_size_rect(viewport);
        let capsule_size_label_rect = bentodesk_style::Rect {
            x: panel.x + 18.0,
            y: capsule_size_row.y + 3.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text(
            if zh { "宽度" } else { "Width" },
            capsule_size_label_rect,
            chrome.muted_color,
        )?;
        for (index, size) in CapsuleSizeChoice::ALL.iter().copied().enumerate() {
            let Some(option) =
                zone_editor_geometry::zone_editor_capsule_size_option_rect(viewport, index)
            else {
                continue;
            };
            let selected = size == selected_size;
            self.fill_rounded_rect(
                option,
                if selected {
                    with_alpha(chrome.accent_color, 0.18)
                } else {
                    chrome.input_background
                },
                chrome.row_radius,
            )?;
            if selected {
                self.stroke_rounded_rect(
                    option,
                    with_alpha(chrome.accent_color, 0.82),
                    chrome.row_radius,
                    1.0,
                )?;
            }
            let label = match (zh, size) {
                (true, CapsuleSizeChoice::Small) => "小 · 120",
                (true, CapsuleSizeChoice::Medium) => "中 · 160",
                (true, CapsuleSizeChoice::Large) => "大 · 200",
                (false, CapsuleSizeChoice::Small) => "Small · 120",
                (false, CapsuleSizeChoice::Medium) => "Medium · 160",
                (false, CapsuleSizeChoice::Large) => "Large · 200",
            };
            self.draw_text_no_wrap_with_style(
                label,
                option,
                if selected {
                    chrome.body_color
                } else {
                    chrome.muted_color
                },
                11.5,
                if selected { 600 } else { 450 },
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }

        let capsule_shape_row = zone_editor_geometry::zone_editor_capsule_shape_rect(viewport);
        let capsule_shape_label_rect = bentodesk_style::Rect {
            x: panel.x + 18.0,
            y: capsule_shape_row.y + 3.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text(
            if zh { "边角" } else { "Corners" },
            capsule_shape_label_rect,
            chrome.muted_color,
        )?;
        for (index, shape) in CapsuleShapeChoice::ALL.iter().copied().enumerate() {
            let Some(option) =
                zone_editor_geometry::zone_editor_capsule_shape_option_rect(viewport, index)
            else {
                continue;
            };
            let selected = shape == selected_shape;
            let option_radius = match shape {
                CapsuleShapeChoice::Pill | CapsuleShapeChoice::Circle => {
                    BorderRadius::all(option.height * 0.5)
                }
                CapsuleShapeChoice::Rounded => chrome.row_radius,
                CapsuleShapeChoice::Minimal => BorderRadius::all(8.0),
                CapsuleShapeChoice::Square => BorderRadius::ZERO,
            };
            self.fill_rounded_rect(
                option,
                if selected {
                    with_alpha(chrome.accent_color, 0.18)
                } else {
                    chrome.input_background
                },
                option_radius,
            )?;
            if selected {
                self.stroke_rounded_rect(
                    option,
                    with_alpha(chrome.accent_color, 0.82),
                    option_radius,
                    1.0,
                )?;
            }
            let label = match (zh, shape) {
                (true, CapsuleShapeChoice::Pill) => "胶囊",
                (true, CapsuleShapeChoice::Rounded) => "圆角",
                (true, CapsuleShapeChoice::Circle) => "圆形",
                (true, CapsuleShapeChoice::Minimal) => "极简",
                (true, CapsuleShapeChoice::Square) => "方角",
                (false, CapsuleShapeChoice::Pill) => "Pill",
                (false, CapsuleShapeChoice::Rounded) => "Rounded",
                (false, CapsuleShapeChoice::Circle) => "Circle",
                (false, CapsuleShapeChoice::Minimal) => "Minimal",
                (false, CapsuleShapeChoice::Square) => "Square",
            };
            self.draw_text_no_wrap_with_style(
                label,
                option,
                if selected {
                    chrome.body_color
                } else {
                    chrome.muted_color
                },
                11.5,
                if selected { 600 } else { 450 },
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }

        self.fill_rounded_rect(
            bentodesk_style::Rect {
                x: panel.x + 1.0,
                y: panel.bottom() - 64.0,
                width: (panel.width - 2.0).max(0.0),
                height: 1.0,
            },
            with_alpha(chrome.body_color, 0.08),
            BorderRadius::ZERO,
        )?;
        let save_rect = zone_editor_geometry::zone_editor_save_rect(viewport);
        self.fill_rounded_rect(save_rect, chrome.accent_color, chrome.button_radius)?;
        self.draw_text_no_wrap_with_style(
            if zh { "保存" } else { "Save" },
            save_rect,
            tauri_palette.readable_text_on(chrome.accent_color),
            13.0,
            600,
            1.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
        )?;
        let cancel_rect = zone_editor_geometry::zone_editor_cancel_rect(viewport);
        self.fill_rounded_rect(cancel_rect, chrome.input_background, chrome.button_radius)?;
        self.draw_text_no_wrap_with_style(
            if zh { "取消" } else { "Cancel" },
            cancel_rect,
            chrome.body_color,
            13.0,
            500,
            1.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
        )?;
        Ok(())
    }
}
