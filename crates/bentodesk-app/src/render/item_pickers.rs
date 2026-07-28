use super::*;

impl Renderer {
    pub(super) fn draw_item_file_rename_window(
        &mut self,
        app: &AppState,
    ) -> Result<(), RenderError> {
        let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);
        let chrome = item_file_rename_geometry::ItemFileRenameChrome::from_tokens(
            app.active_theme_palette(),
            app.active_theme_radius(),
            app.active_theme_shadow(),
        );
        let viewport = app.viewport;
        let panel = item_file_rename_geometry::item_file_rename_panel_rect(viewport);
        let shadow_rect = item_file_rename_geometry::item_file_rename_panel_shadow_rect(
            panel,
            chrome.panel_shadow,
        );
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        // The rename form is a movable auxiliary HWND. Keep only its rounded
        // outer corners transparent; a translucent card exposes sharp desktop
        // and foreground-window seams through the text fields.
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        self.stroke_rounded_rect(
            panel,
            with_alpha(chrome.body_color, 0.12),
            chrome.panel_radius,
            1.0,
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
            width: panel.width - 36.0,
            height: 26.0,
        };
        // M6c — file rename panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh { "重命名文件" } else { "Rename file" },
            title_rect,
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;

        let session = app.item_file_rename.borrow();
        let current_path = session
            .as_ref()
            .map(|entry| entry.current_path.as_str())
            .unwrap_or(if zh {
                "未选择任何项目"
            } else {
                "No item selected"
            });
        let path_rect = item_file_rename_geometry::item_file_rename_path_rect(viewport);
        self.draw_text(current_path, path_rect, chrome.muted_color)?;

        let label_rect = bentodesk_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 84.0,
            width: panel.width - 36.0,
            height: 18.0,
        };
        self.draw_text(
            if zh { "新文件名" } else { "New file name" },
            label_rect,
            chrome.muted_color,
        )?;

        let input_rect = item_file_rename_geometry::item_file_rename_input_rect(viewport);
        self.fill_rounded_rect(input_rect, chrome.accent_color, chrome.input_radius)?;
        self.fill_rounded_rect(
            inset_rect(input_rect, 2.0),
            chrome.input_background,
            chrome.input_inner_radius,
        )?;
        let draft = session
            .as_ref()
            .map(|entry| entry.draft_name.as_str())
            .unwrap_or("");
        let draft_rect = bentodesk_style::Rect {
            x: input_rect.x + 12.0,
            y: input_rect.y + 9.0,
            width: input_rect.width - 24.0,
            height: 20.0,
        };
        self.draw_text(draft, draft_rect, chrome.body_color)?;

        let status = session
            .as_ref()
            .and_then(|entry| entry.status.as_ref())
            .map(|text| (text.as_str(), chrome.error_color))
            .unwrap_or((
                if zh {
                    "按 Enter 确认重命名，按 Esc 取消。"
                } else {
                    "Enter to rename; Esc to cancel."
                },
                chrome.muted_color,
            ));
        let status_rect = item_file_rename_geometry::item_file_rename_status_rect(viewport);
        self.draw_text(status.0, status_rect, status.1)?;
        Ok(())
    }

    pub(super) fn draw_icon_picker_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);
        // Wave D: consume Wave B Tauri-token SSoT.
        let chrome = icon_picker::IconPickerChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = picker_geometry::picker_panel(viewport);
        let shadow_rect = picker_geometry::picker_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        let close_rect = picker_geometry::icon_picker_close_rect(viewport);
        let title_rect = bentodesk_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 16.0,
            width: (close_rect.x - panel.x - 28.0).max(120.0),
            height: 28.0,
        };
        // M6c — icon picker panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "选择区域图标"
            } else {
                "Icon picker"
            },
            title_rect,
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        self.fill_rounded_rect(
            close_rect,
            with_alpha(chrome.body_color, 0.06),
            chrome.slot_radius,
        )?;
        self.draw_icon_glyph(
            IconKind::X.as_str(),
            centered_square_rect(close_rect, 14.0),
            chrome.muted_color,
        )?;

        let session = app.icon_picker.borrow();
        let selected_icon = session
            .as_ref()
            .map(|s| s.selected_icon.as_str())
            .unwrap_or("");
        let selected_icon_label = if selected_icon.is_empty() {
            if zh { "未选择" } else { "No selection" }
        } else {
            localized_icon_wire_label(selected_icon, zh)
        };
        let target_label = match session.as_ref().and_then(|s| s.zone_id) {
            Some(_) if zh => "应用到当前区域",
            Some(_) => "Target: zone icon",
            None if session.is_some() && zh => "应用到批量管理器中的已选区域",
            None if session.is_some() => "Target: BulkManager selection",
            None if zh => "尚未选择应用目标",
            None => "Target: none",
        };

        let target_rect = bentodesk_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 58.0,
            width: panel.width - 36.0,
            height: 22.0,
        };
        self.draw_text(target_label, target_rect, chrome.muted_color)?;

        let chip_rect = picker_geometry::icon_picker_selected_rect(viewport);
        self.fill_rounded_rect(chip_rect, chrome.accent_color, chrome.chip_radius)?;
        self.fill_rounded_rect(
            inset_rect(chip_rect, 2.0),
            chrome.chip_background,
            chrome.chip_inner_radius,
        )?;
        let selected_rect = bentodesk_style::Rect {
            x: chip_rect.x + 12.0,
            y: chip_rect.y + 10.0,
            width: chip_rect.width - 24.0,
            height: 24.0,
        };
        self.draw_text(selected_icon_label, selected_rect, chrome.body_color)?;

        for (index, kind) in ALL_ICON_KINDS.iter().enumerate() {
            let slot_rect = picker_geometry::icon_picker_slot_rect(viewport, index);
            let selected = kind.matches_wire(selected_icon);
            let border_color = if selected {
                chrome.accent_color
            } else {
                chrome.chip_background
            };
            self.fill_rounded_rect(slot_rect, border_color, chrome.slot_radius)?;
            self.fill_rounded_rect(
                inset_rect(slot_rect, 2.0),
                chrome.chip_background,
                chrome.slot_inner_radius,
            )?;
            let icon_rect = bentodesk_style::Rect {
                x: slot_rect.x + (slot_rect.width - 22.0) * 0.5,
                y: slot_rect.y + 6.0,
                width: 22.0,
                height: 22.0,
            };
            self.draw_svg_document_stroke_fit(
                kind.source_svg(),
                icon_rect,
                chrome.body_color,
                22.0,
            )?;
            let slug_rect = bentodesk_style::Rect {
                x: slot_rect.x + 3.0,
                y: slot_rect.y + 32.0,
                width: slot_rect.width - 6.0,
                height: 18.0,
            };
            self.draw_text_no_wrap_with_style(
                icon_kind_label(*kind, zh),
                slug_rect,
                chrome.body_color,
                9.5,
                450,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }

        let hint_rect = picker_geometry::icon_picker_hint_rect(viewport, ALL_ICON_KINDS.len());
        self.draw_text(
            if zh {
                "单击图标即可保存；方向键可切换，Esc 取消。"
            } else {
                "Click an icon to save. F2 or Right cycles icon. Enter saves. Esc cancels."
            },
            hint_rect,
            chrome.muted_color,
        )?;
        if session.is_none() {
            let warning_rect = bentodesk_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 184.0,
                width: panel.width - 36.0,
                height: 24.0,
            };
            self.draw_text(
                if zh {
                    "请从区域菜单打开图标选择器。"
                } else {
                    "Open from a zone to commit the selected icon."
                },
                warning_rect,
                chrome.warning_color,
            )?;
        }
        Ok(())
    }

    pub(super) fn draw_palette_picker_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);
        // Wave D: consume Wave B Tauri-token SSoT.
        let chrome = palette_picker::PalettePickerChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = picker_geometry::picker_panel(viewport);
        let shadow_rect = picker_geometry::picker_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        let title_rect = bentodesk_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 16.0,
            width: panel.width - 36.0,
            height: 28.0,
        };
        // M6c — palette picker panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "选择强调色"
            } else {
                "Palette picker"
            },
            title_rect,
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;

        let session = app.palette_picker.borrow();
        let target_label = match session.as_ref().map(|s| s.target) {
            Some(PaletteTarget::ZoneAccent(_)) if zh => "应用到当前区域",
            Some(PaletteTarget::ZoneAccent(_)) => "Target: zone accent",
            Some(PaletteTarget::ThemeBase) if zh => "应用到当前主题",
            Some(PaletteTarget::ThemeBase) => "Target: theme base accent",
            Some(PaletteTarget::BulkManagerSelectedAccent) if zh => "应用到批量管理器中的已选区域",
            Some(PaletteTarget::BulkManagerSelectedAccent) => "Target: BulkManager selection",
            None if zh => "尚未选择应用目标",
            None => "Target: none",
        };
        let selected_accent = session
            .as_ref()
            .and_then(|s| s.selected_accent.as_deref())
            .unwrap_or(if zh { "未设置" } else { "None" });

        let target_rect = bentodesk_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 58.0,
            width: panel.width - 36.0,
            height: 22.0,
        };
        self.draw_text(target_label, target_rect, chrome.muted_color)?;

        let selected = session.as_ref().and_then(|s| s.selected_accent.as_deref());
        for (index, swatch) in palette_picker::swatch_table().iter().enumerate() {
            let swatch_rect = picker_geometry::palette_picker_swatch_rect(viewport, index);
            let is_selected = selected == Some(swatch.hex.as_str());
            let border = if is_selected {
                chrome.warning_color
            } else {
                chrome.chip_background
            };
            self.fill_rounded_rect(swatch_rect, border, chrome.swatch_radius)?;
            if let Some(color) = parse_hex_color(swatch.hex.as_str()) {
                self.fill_rounded_rect(
                    inset_rect(swatch_rect, 3.0),
                    color,
                    chrome.swatch_inner_radius,
                )?;
            }
        }
        let clear_rect = picker_geometry::palette_picker_clear_rect(viewport);
        let clear_border = if selected.is_none() {
            chrome.warning_color
        } else {
            chrome.chip_background
        };
        self.fill_rounded_rect(clear_rect, clear_border, chrome.clear_radius)?;
        self.fill_rounded_rect(
            inset_rect(clear_rect, 2.0),
            chrome.chip_background,
            chrome.clear_inner_radius,
        )?;
        let clear_text_rect = bentodesk_style::Rect {
            x: clear_rect.x + 8.0,
            y: clear_rect.y + 5.0,
            width: clear_rect.width - 16.0,
            height: 20.0,
        };
        self.draw_text(
            if zh { "清除" } else { "Clear" },
            clear_text_rect,
            chrome.body_color,
        )?;

        let value_rect = picker_geometry::palette_picker_value_rect(viewport);
        self.draw_text(selected_accent, value_rect, chrome.body_color)?;

        let hint_rect = picker_geometry::palette_picker_hint_rect(viewport);
        self.draw_text(
            if zh {
                "单击色块即可保存；选择“清除”可恢复默认，Esc 取消。"
            } else {
                "Click a swatch or Clear to save. F3/Right cycles. Esc cancels."
            },
            hint_rect,
            chrome.muted_color,
        )?;
        if session.as_ref().map(|s| s.target).is_none() {
            let warning_rect = bentodesk_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 184.0,
                width: panel.width - 36.0,
                height: 24.0,
            };
            self.draw_text(
                if zh {
                    "请先从区域或设置页面打开颜色选择器。"
                } else {
                    "No palette target is active."
                },
                warning_rect,
                chrome.warning_color,
            )?;
        }
        Ok(())
    }
}
