use super::*;

impl Renderer {
    pub(super) fn draw_capsule_picker_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
        // Wave D: consume Wave B Tauri-token SSoT.
        let chrome = capsule_picker::CapsulePickerChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = capsule_picker::capsule_picker_panel_rect(viewport);
        let shadow_rect =
            capsule_picker::capsule_picker_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        // M6c — capsule picker panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "场景胶囊"
            } else {
                "Context Capsules"
            },
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: panel.width - 36.0,
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        self.draw_text(
            if zh {
                "保存当前桌面布局，并在需要时一键恢复。"
            } else {
                "Save the current Desktop layout and restore it whenever you need it."
            },
            capsule_picker::capsule_picker_hint_rect(viewport),
            chrome.muted_color,
        )?;

        let state = app.capsule_picker.borrow();
        let action_palette = app.active_theme_tauri();
        for (index, hit) in capsule_picker::CAPSULE_PICKER_ACTIONS
            .iter()
            .copied()
            .enumerate()
        {
            let rect = capsule_picker::capsule_picker_action_rect(viewport, index);
            let enabled = !state.is_busy()
                && (!matches!(hit, CapsulePickerHit::Restore | CapsulePickerHit::Delete)
                    || !state.entries().is_empty());
            let emphasis = if !enabled {
                AuxiliaryActionEmphasis::Disabled
            } else {
                match hit {
                    CapsulePickerHit::Capture => AuxiliaryActionEmphasis::Primary,
                    CapsulePickerHit::Delete => AuxiliaryActionEmphasis::Danger,
                    CapsulePickerHit::Restore
                    | CapsulePickerHit::Close
                    | CapsulePickerHit::Hint
                    | CapsulePickerHit::Error
                    | CapsulePickerHit::Empty
                    | CapsulePickerHit::Row(_) => AuxiliaryActionEmphasis::Secondary,
                }
            };
            let action = auxiliary_action_chrome(action_palette, emphasis);
            self.fill_rounded_rect(rect, action.fill, chrome.row_radius)?;
            self.stroke_rounded_rect(rect, action.border, chrome.row_radius, 1.0)?;
            self.draw_text_aligned(
                capsule_action_label(hit, zh),
                rect,
                action.text,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }
        if let Some(error) = state.last_error() {
            self.draw_text(
                error,
                capsule_picker::capsule_picker_error_rect(viewport),
                chrome.error_color,
            )?;
        }
        if state.entries().is_empty() {
            let empty = capsule_picker::capsule_picker_empty_rect(viewport);
            self.draw_icon_glyph(
                IconKind::Bookmark.as_str(),
                bento_nano_style::Rect {
                    x: empty.x + (empty.width - 32.0) * 0.5,
                    y: empty.y,
                    width: 32.0,
                    height: 32.0,
                },
                chrome.muted_color,
            )?;
            self.draw_text_aligned(
                if zh {
                    "还没有场景胶囊"
                } else {
                    "No context capsules yet"
                },
                bento_nano_style::Rect {
                    x: empty.x,
                    y: empty.y + 42.0,
                    width: empty.width,
                    height: 24.0,
                },
                chrome.body_color,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
            self.draw_text_aligned(
                if zh {
                    "选择“保存当前”即可记录这组桌面布局。"
                } else {
                    "Select Save current to capture this Desktop layout."
                },
                bento_nano_style::Rect {
                    x: empty.x,
                    y: empty.y + 72.0,
                    width: empty.width,
                    height: 24.0,
                },
                chrome.muted_color,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
            return Ok(());
        }

        for (index, entry) in state
            .entries()
            .iter()
            .take(capsule_picker::CAPSULE_VISIBLE_ROW_LIMIT)
            .enumerate()
        {
            let row = capsule_picker::capsule_picker_row_rect(viewport, index);
            let bg = if index == state.selected_index() {
                chrome.selected_background
            } else {
                chrome.row_background
            };
            self.fill_rounded_rect(row, bg, chrome.row_radius)?;
            let icon = IconKind::from_str_opt(entry.icon.as_str()).unwrap_or(IconKind::Bookmark);
            self.draw_icon_glyph(
                icon.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 10.0,
                    y: row.y + 10.0,
                    width: 20.0,
                    height: 20.0,
                },
                chrome.body_color,
            )?;
            self.draw_text_no_wrap(
                entry.name.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 42.0,
                    y: row.y + 5.0,
                    width: row.width - 52.0,
                    height: 18.0,
                },
                chrome.body_color,
            )?;
            self.draw_text_no_wrap(
                entry.captured_at.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 42.0,
                    y: row.y + 22.0,
                    width: row.width - 52.0,
                    height: 16.0,
                },
                chrome.muted_color,
            )?;
        }
        Ok(())
    }

    pub(super) fn draw_bulk_manager_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
        // Wave E: Tauri SSoT tokens for the BulkManager panel.
        use bento_nano_style::tokens as style_tokens;
        let action_palette = app.active_theme_tauri();
        let chrome = bulk_manager_panel::BulkManagerChrome::from_tauri_tokens(
            action_palette,
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = bulk_manager_panel::bulk_manager_panel_rect(viewport);
        let search_rect = bulk_manager_panel::bulk_manager_search_rect(viewport);
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        // M6c — bulk manager panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "批量管理区域"
            } else {
                "Bulk Manager"
            },
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: (search_rect.x - panel.x - 30.0).max(160.0),
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        let close_rect = bulk_manager_panel::bulk_manager_close_rect(viewport);
        let close_chrome =
            auxiliary_action_chrome(action_palette, AuxiliaryActionEmphasis::Secondary);
        self.fill_rounded_rect(close_rect, close_chrome.fill, chrome.button_radius)?;
        self.stroke_rounded_rect(close_rect, close_chrome.border, chrome.button_radius, 1.0)?;
        self.draw_icon_glyph(
            "x",
            centered_square_rect(close_rect, 14.0),
            close_chrome.text,
        )?;

        let bulk_line_height =
            style_tokens::TYPOGRAPHY.sm.size_px * style_tokens::TYPOGRAPHY.sm.line_height;
        self.draw_text(
            if zh {
                "搜索与排序区域；单击一行即可加入或移出批量选择。"
            } else {
                "Search and sort zones; click a row to toggle its batch selection."
            },
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + bulk_manager_panel::RUNTIME_HELPER_TOP_PX,
                width: panel.width - 36.0,
                height: bulk_line_height,
            },
            chrome.muted_color,
        )?;

        let state = app.bulk_manager.borrow();
        let search_fill = if state.search_focused() {
            chrome.cursor_background
        } else {
            chrome.row_background
        };
        self.fill_rounded_rect(search_rect, search_fill, chrome.search_radius)?;
        if state.search_focused() {
            self.stroke_rounded_rect(
                search_rect,
                with_alpha(action_palette.accent_blue, 0.88),
                chrome.search_radius,
                1.5,
            )?;
        }
        let search_body = if state.search().is_empty() {
            if zh {
                "搜索区域…"
            } else {
                "Search zones..."
            }
        } else {
            state.search()
        };
        let search_text = if zh {
            smol_str::SmolStr::new(search_body)
        } else {
            smol_str::SmolStr::new(format!("Search: {search_body}"))
        };
        self.draw_text(
            search_text.as_str(),
            bento_nano_style::Rect {
                x: search_rect.x + 10.0,
                y: search_rect.y + 7.0,
                width: search_rect.width - 20.0,
                height: 18.0,
            },
            if state.search().is_empty() {
                chrome.muted_color
            } else {
                chrome.body_color
            },
        )?;
        if state.search_focused() && state.search().is_empty() {
            self.fill_rounded_rect(
                bento_nano_style::Rect {
                    x: search_rect.x + 10.0,
                    y: search_rect.y + 9.0,
                    width: 1.5,
                    height: search_rect.height - 18.0,
                },
                action_palette.accent_blue,
                BorderRadius::ZERO,
            )?;
        }
        let rows = state.visible_rows();
        let row_window_start =
            bulk_manager_panel::bulk_manager_visible_window_start(state.cursor_index(), rows.len());
        let row_window_summary = localized_visible_range(
            row_window_start,
            rows.len(),
            bulk_manager_panel::RUNTIME_VISIBLE_ROW_LIMIT,
            zh,
        );
        let selected_count = state.selected().len();
        let base_status_text = app.bulk_manager_status.borrow().clone().unwrap_or_else(|| {
            if zh {
                smol_str::SmolStr::new(format!(
                    "共 {} 个区域，已选择 {} 个",
                    rows.len(),
                    selected_count
                ))
            } else {
                smol_str::SmolStr::new(format!(
                    "{} zones listed, {} selected",
                    rows.len(),
                    selected_count
                ))
            }
        });
        let base_status_text = if let Some(summary) = row_window_summary {
            smol_str::SmolStr::new(format!("{base_status_text} — {summary}"))
        } else {
            base_status_text
        };
        let edit_status = state.text_edit().map(|edit| {
            let draft = if edit.draft.is_empty() {
                bulk_text_edit_placeholder(edit.field, zh)
            } else {
                edit.draft.as_str()
            };
            if zh {
                smol_str::SmolStr::new(format!(
                    "编辑{}：{}　F2 切换字段 · Enter 应用 · Esc 取消",
                    bulk_text_edit_field_label(edit.field, true),
                    draft
                ))
            } else {
                smol_str::SmolStr::new(format!(
                    "Edit {}: {}    F2 field · Enter apply · Esc cancel",
                    bulk_text_edit_field_label(edit.field, false),
                    draft
                ))
            }
        });
        let status_text = edit_status.as_ref().unwrap_or(&base_status_text);
        let status_top = panel.y + bulk_manager_panel::RUNTIME_STATUS_TOP_PX;
        let status_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: status_top,
            width: panel.width - 36.0,
            height: bulk_line_height,
        };
        if edit_status.is_some() {
            self.fill_rounded_rect(status_rect, chrome.cursor_background, chrome.edit_radius)?;
        }
        self.draw_text(
            status_text.as_str(),
            if edit_status.is_some() {
                inset_rect(status_rect, 4.0)
            } else {
                status_rect
            },
            if edit_status.is_some() {
                chrome.body_color
            } else {
                chrome.muted_color
            },
        )?;
        // Quiet separators make the dense toolbar read as three command
        // groups (selection, visibility, layout) instead of fifteen unrelated
        // pill buttons.
        for (x, y) in [
            (panel.x + 133.0, panel.y + 108.0),
            (panel.x + 249.0, panel.y + 108.0),
            (panel.x + 554.0, panel.y + 138.0),
        ] {
            self.fill_rounded_rect(
                bento_nano_style::Rect {
                    x,
                    y,
                    width: 1.0,
                    height: 16.0,
                },
                with_alpha(chrome.body_color, 0.12),
                BorderRadius::ZERO,
            )?;
        }
        for spec in bulk_manager_panel::BULK_MANAGER_ACTION_BUTTONS {
            let rect = bulk_manager_panel::bulk_manager_button_rect(viewport, *spec);
            let enabled = bulk_manager_panel::bulk_manager_action_enabled(
                spec.hit,
                !rows.is_empty(),
                selected_count > 0,
            );
            let emphasis = if !enabled {
                AuxiliaryActionEmphasis::Disabled
            } else {
                match spec.hit {
                    bulk_manager_panel::BulkManagerPointerHit::Delete => {
                        AuxiliaryActionEmphasis::Danger
                    }
                    _ => AuxiliaryActionEmphasis::Secondary,
                }
            };
            let action = auxiliary_action_chrome(action_palette, emphasis);
            if enabled {
                self.fill_rounded_rect(rect, action.fill, chrome.button_radius)?;
                self.stroke_rounded_rect(rect, action.border, chrome.button_radius, 1.0)?;
            }
            // RC-4 Gap 3 — `draw_text_no_wrap` keeps the 4-letter button
            // labels ("Show", "Move", "Close") on a single line and trims
            // with an ellipsis if the layout box is too narrow, instead of
            // wrapping them into "Sho/w", "Mov", "Clos/e" against the wide
            // YaHei UI fallback Latin metrics. Shrink the horizontal pad
            // from 7 px to SPACING.xs (4 px) each side to give the run an
            // extra 6 px of room — enough for every label in the table to
            // measure clean at the spec'd width without column changes.
            self.draw_text_no_wrap_with_style(
                bulk_manager_action_label(spec.hit, zh),
                rect,
                if enabled {
                    action.text
                } else {
                    with_alpha(chrome.muted_color, 0.42)
                },
                11.5,
                if enabled { 550 } else { 450 },
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }
        let sort_band = bento_nano_style::Rect {
            x: panel.x + bulk_manager_panel::RUNTIME_PANEL_INSET_PX,
            y: panel.y + bulk_manager_panel::RUNTIME_SORT_HEADER_TOP_PX,
            width: panel.width - bulk_manager_panel::RUNTIME_PANEL_INSET_PX * 2.0,
            height: bulk_manager_panel::RUNTIME_SORT_HEADER_HEIGHT_PX,
        };
        self.fill_rounded_rect(sort_band, chrome.row_background, chrome.sort_radius)?;
        for key in bulk_manager_panel::SortKey::ALL {
            let rect = bulk_manager_panel::bulk_manager_sort_header_rect(viewport, *key);
            let active = state.sort_key() == *key;
            let suffix = if active {
                match state.sort_direction() {
                    bulk_manager_panel::SortDirection::Ascending => " ↑",
                    bulk_manager_panel::SortDirection::Descending => " ↓",
                }
            } else {
                ""
            };
            let label =
                smol_str::SmolStr::new(format!("{}{}", bulk_manager_sort_label(*key, zh), suffix));
            // RC-4 Gap 3 — same no-wrap protection as the action buttons.
            self.draw_text_no_wrap_with_style(
                label.as_str(),
                bento_nano_style::Rect {
                    x: rect.x + 8.0,
                    width: (rect.width - 16.0).max(0.0),
                    ..rect
                },
                if active {
                    action_palette.accent_blue
                } else {
                    chrome.muted_color
                },
                11.5,
                if active { 600 } else { 500 },
                1.0,
                dwrite::TextAlign {
                    h: if matches!(
                        key,
                        bulk_manager_panel::SortKey::Items | bulk_manager_panel::SortKey::Size
                    ) {
                        dwrite::HAlign::Center
                    } else {
                        dwrite::HAlign::Leading
                    },
                    v: dwrite::VAlign::Center,
                },
            )?;
        }
        if rows.is_empty() {
            self.draw_text(
                if zh {
                    "暂无可批量管理的区域。"
                } else {
                    "No zones available for bulk operations."
                },
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: panel.y + bulk_manager_panel::RUNTIME_ROW_TOP_PX,
                    width: panel.width - 36.0,
                    height: 24.0,
                },
                chrome.body_color,
            )?;
            return Ok(());
        }

        for (display_index, row_data) in rows
            .iter()
            .skip(row_window_start)
            .take(bulk_manager_panel::RUNTIME_VISIBLE_ROW_LIMIT)
            .enumerate()
        {
            let index = row_window_start + display_index;
            let row = bulk_manager_panel::bulk_manager_row_rect(viewport, display_index);
            let bg = if state.is_selected(row_data.id) {
                chrome.selected_background
            } else if index == state.cursor_index() {
                chrome.cursor_background
            } else {
                chrome.row_background
            };
            self.fill_rounded_rect(row, bg, chrome.row_radius)?;
            let selected = state.is_selected(row_data.id);
            let name_cell = bulk_manager_panel::bulk_manager_row_cell_rect(
                viewport,
                display_index,
                bulk_manager_panel::SortKey::Name,
            );
            let checkbox = bento_nano_style::Rect {
                x: name_cell.x + 9.0,
                y: name_cell.y + (name_cell.height - 14.0) * 0.5,
                width: 14.0,
                height: 14.0,
            };
            if selected {
                self.fill_rounded_rect(
                    checkbox,
                    action_palette.accent_blue,
                    BorderRadius::all(4.0),
                )?;
                self.draw_text_no_wrap_with_style(
                    "✓",
                    checkbox,
                    action_palette.readable_text_on(action_palette.accent_blue),
                    10.0,
                    700,
                    1.0,
                    dwrite::TextAlign {
                        h: dwrite::HAlign::Center,
                        v: dwrite::VAlign::Center,
                    },
                )?;
            } else {
                self.stroke_rounded_rect(
                    checkbox,
                    if index == state.cursor_index() {
                        action_palette.accent_blue
                    } else {
                        with_alpha(chrome.muted_color, 0.5)
                    },
                    BorderRadius::all(4.0),
                    1.0,
                )?;
            }
            let name_x = checkbox.right() + 9.0;
            self.draw_text_no_wrap_with_style(
                row_data.display_name.as_str(),
                bento_nano_style::Rect {
                    x: name_x,
                    y: name_cell.y + 3.0,
                    width: (name_cell.right() - name_x - 8.0).max(0.0),
                    height: 18.0,
                },
                chrome.body_color,
                12.0,
                550,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Leading,
                    v: dwrite::VAlign::Center,
                },
            )?;
            let row_meta = smol_str::SmolStr::new(if zh {
                format!(
                    "{} · {}",
                    if row_data.visible { "显示" } else { "隐藏" },
                    if row_data.locked {
                        "已锁定"
                    } else {
                        "未锁定"
                    }
                )
            } else {
                format!(
                    "{} · {}",
                    if row_data.visible {
                        "Visible"
                    } else {
                        "Hidden"
                    },
                    if row_data.locked {
                        "Locked"
                    } else {
                        "Unlocked"
                    }
                )
            });
            self.draw_text_no_wrap_with_style(
                row_meta.as_str(),
                bento_nano_style::Rect {
                    x: name_x,
                    y: name_cell.y + 20.0,
                    width: (name_cell.right() - name_x - 8.0).max(0.0),
                    height: 14.0,
                },
                chrome.muted_color,
                9.5,
                450,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Leading,
                    v: dwrite::VAlign::Center,
                },
            )?;

            let items_cell = bulk_manager_panel::bulk_manager_row_cell_rect(
                viewport,
                display_index,
                bulk_manager_panel::SortKey::Items,
            );
            let item_count = smol_str::SmolStr::new(row_data.item_count.to_string());
            self.draw_text_no_wrap_with_style(
                item_count.as_str(),
                items_cell,
                chrome.body_color,
                12.0,
                500,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;

            let accent_cell = bulk_manager_panel::bulk_manager_row_cell_rect(
                viewport,
                display_index,
                bulk_manager_panel::SortKey::Accent,
            );
            let parsed_accent = parse_hex_color(row_data.accent_hex.as_str());
            let accent_text = if row_data.accent_hex.is_empty() {
                if zh { "默认" } else { "Default" }
            } else {
                row_data.accent_hex.as_str()
            };
            let accent_text_x = if let Some(color) = parsed_accent {
                let swatch = bento_nano_style::Rect {
                    x: accent_cell.x + 8.0,
                    y: accent_cell.y + (accent_cell.height - 12.0) * 0.5,
                    width: 12.0,
                    height: 12.0,
                };
                self.fill_rounded_rect(swatch, color, BorderRadius::all(6.0))?;
                swatch.right() + 6.0
            } else {
                accent_cell.x + 8.0
            };
            self.draw_text_no_wrap(
                accent_text,
                bento_nano_style::Rect {
                    x: accent_text_x,
                    y: accent_cell.y + 7.0,
                    width: (accent_cell.right() - accent_text_x - 6.0).max(0.0),
                    height: 18.0,
                },
                chrome.body_color,
            )?;

            let size_cell = bulk_manager_panel::bulk_manager_row_cell_rect(
                viewport,
                display_index,
                bulk_manager_panel::SortKey::Size,
            );
            let size_text = smol_str::SmolStr::new(format!(
                "{}×{}%",
                row_data.width_percent, row_data.height_percent
            ));
            self.draw_text_no_wrap_with_style(
                size_text.as_str(),
                size_cell,
                chrome.body_color,
                12.0,
                500,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }
        Ok(())
    }
}
