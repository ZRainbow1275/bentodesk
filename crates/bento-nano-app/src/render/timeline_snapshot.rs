use super::*;

impl Renderer {
    pub(super) fn draw_timeline_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
        // Wave E: Tauri SSoT tokens for the Timeline panel.
        let chrome = timeline_panel::TimelinePanelChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = timeline_panel::timeline_panel_rect(viewport);
        let shadow_rect = timeline_panel::timeline_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        // M6c — timeline panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "桌面时间线"
            } else {
                "Desktop Timeline"
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
                "选择记录可预览布局；使用上方按钮保存、固定、恢复或删除。"
            } else {
                "Select a checkpoint to preview it, then save, pin, restore, or delete."
            },
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 50.0,
                width: panel.width - 36.0,
                height: 24.0,
            },
            chrome.muted_color,
        )?;

        let state = app.timeline_panel.borrow();
        let status = if let Some(error) = state.error() {
            smol_str::SmolStr::new(if zh {
                format!("错误：{error}")
            } else {
                format!("Error: {error}")
            })
        } else if let Some(status) = state.status() {
            status.clone()
        } else {
            smol_str::SmolStr::new(if zh {
                format!("已载入 {} 条时间线记录", state.entries().len())
            } else {
                format!("Loaded {} checkpoints", state.entries().len())
            })
        };
        self.draw_text(
            status.as_str(),
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 80.0,
                width: panel.width - 36.0,
                height: 22.0,
            },
            if state.error().is_some() {
                chrome.error_color
            } else {
                chrome.muted_color
            },
        )?;

        let has_entries = !state.entries().is_empty();
        let action_palette = app.active_theme_tauri();
        for spec in timeline_panel::TIMELINE_ACTION_BUTTONS {
            let rect = timeline_panel::timeline_button_rect(viewport, *spec);
            let enabled = !matches!(
                spec.hit,
                timeline_panel::TimelinePointerHit::Pin
                    | timeline_panel::TimelinePointerHit::Restore
                    | timeline_panel::TimelinePointerHit::Delete
            ) || has_entries;
            let emphasis = if !enabled {
                AuxiliaryActionEmphasis::Disabled
            } else {
                match spec.hit {
                    timeline_panel::TimelinePointerHit::Save => AuxiliaryActionEmphasis::Primary,
                    timeline_panel::TimelinePointerHit::Delete => AuxiliaryActionEmphasis::Danger,
                    _ => AuxiliaryActionEmphasis::Secondary,
                }
            };
            let action = auxiliary_action_chrome(action_palette, emphasis);
            self.fill_rounded_rect(rect, action.fill, chrome.button_radius)?;
            self.stroke_rounded_rect(rect, action.border, chrome.button_radius, 1.0)?;
            self.draw_text_no_wrap(
                timeline_action_label(spec.hit, zh),
                bento_nano_style::Rect {
                    x: rect.x + 8.0,
                    y: rect.y + 6.0,
                    width: rect.width - 16.0,
                    height: 16.0,
                },
                action.text,
            )?;
        }

        if !has_entries {
            let center_y = panel.y + panel.height * 0.56;
            self.draw_icon_glyph(
                IconKind::Camera.as_str(),
                bento_nano_style::Rect {
                    x: panel.x + (panel.width - 34.0) * 0.5,
                    y: center_y - 48.0,
                    width: 34.0,
                    height: 34.0,
                },
                chrome.muted_color,
            )?;
            self.draw_text_aligned(
                if zh {
                    "还没有时间线记录"
                } else {
                    "No timeline checkpoints yet"
                },
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: center_y,
                    width: panel.width - 36.0,
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
                    "选择“保存”记录当前区域布局，之后可随时预览和恢复。"
                } else {
                    "Select Save to capture the current layout for preview and restore."
                },
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: center_y + 30.0,
                    width: panel.width - 36.0,
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

        let list_w = panel.width * 0.56;
        for (index, entry) in state
            .entries()
            .iter()
            .take(timeline_panel::RUNTIME_VISIBLE_ROW_LIMIT)
            .enumerate()
        {
            let row = timeline_panel::timeline_row_rect(viewport, index);
            let bg = if index == state.cursor_index() {
                chrome.selected_background
            } else {
                chrome.row_background
            };
            self.fill_rounded_rect(row, bg, chrome.row_radius)?;
            let line = smol_str::SmolStr::new(if zh {
                format!(
                    "{}　· {} 个区域　· {} 个项目",
                    entry.captured_at, entry.zone_count, entry.item_count
                )
            } else {
                format!(
                    "{}  · {} zones  · {} items",
                    entry.captured_at, entry.zone_count, entry.item_count
                )
            });
            if entry.pinned {
                self.draw_icon_glyph(
                    IconKind::Pin.as_str(),
                    bento_nano_style::Rect {
                        x: row.x + 10.0,
                        y: row.y + 5.0,
                        width: 12.0,
                        height: 12.0,
                    },
                    chrome.body_color,
                )?;
            }
            self.draw_text(
                line.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 28.0,
                    y: row.y + 4.0,
                    width: row.width - 38.0,
                    height: 17.0,
                },
                chrome.body_color,
            )?;
            let delta = if entry.delta_summary.is_empty() {
                if zh { "无变化" } else { "no change" }
            } else {
                entry.delta_summary.as_str()
            };
            self.draw_text(
                delta,
                bento_nano_style::Rect {
                    x: row.x + 10.0,
                    y: row.y + 21.0,
                    width: row.width - 20.0,
                    height: 15.0,
                },
                chrome.muted_color,
            )?;
        }

        let detail_x = panel.x + list_w + 12.0;
        let detail_w = panel.width - (detail_x - panel.x) - 18.0;
        if let Some(active) = state.active() {
            let detail = smol_str::SmolStr::new(if zh {
                format!(
                    "当前记录\n{} · {} 个区域 · {}",
                    if active.pinned {
                        "已固定"
                    } else {
                        "未固定"
                    },
                    active.snapshot.zones.len(),
                    active.snapshot.captured_at
                )
            } else {
                format!(
                    "Selected checkpoint\n{} · {} zones · {}",
                    if active.pinned {
                        "Pinned"
                    } else {
                        "Not pinned"
                    },
                    active.snapshot.zones.len(),
                    active.snapshot.captured_at
                )
            });
            self.draw_text(
                detail.as_str(),
                bento_nano_style::Rect {
                    x: detail_x,
                    y: panel.y + timeline_panel::RUNTIME_ROW_TOP_PX,
                    width: detail_w,
                    height: 72.0,
                },
                chrome.body_color,
            )?;
            let thumbnail_rect = timeline_detail_thumbnail_rect(panel, detail_x, detail_w);
            // Wave E: Tauri SSoT tokens for the inline snapshot thumbnail.
            let thumbnail_chrome = snapshot_picker::SnapshotThumbnailChrome::from_tauri_tokens(
                app.active_theme_tauri(),
                app.active_theme_radius_tauri(),
            );
            self.draw_snapshot_thumbnail(&active.snapshot, thumbnail_rect, thumbnail_chrome)?;
        }
        Ok(())
    }

    pub(super) fn draw_snapshot_picker_window(
        &mut self,
        app: &AppState,
    ) -> Result<(), RenderError> {
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
        // Wave E: Tauri SSoT tokens for the Snapshot picker panel.
        use bento_nano_style::tokens as style_tokens;
        let chrome = snapshot_picker::SnapshotPickerChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = snapshot_picker::snapshot_picker_panel_rect(viewport);
        let action_palette = app.active_theme_tauri();
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
        let close_rect = snapshot_picker::snapshot_picker_close_rect(viewport);
        let close_chrome =
            auxiliary_action_chrome(action_palette, AuxiliaryActionEmphasis::Secondary);
        self.fill_rounded_rect(close_rect, close_chrome.fill, chrome.button_radius)?;
        self.draw_icon_glyph(
            "x",
            centered_square_rect(close_rect, 14.0),
            close_chrome.text,
        )?;
        // M6c — snapshot picker panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "布局快照"
            } else {
                "Layout Snapshots"
            },
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 14.0,
                width: (close_rect.x - panel.x - 30.0).max(120.0),
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        self.fill_rounded_rect(
            bento_nano_style::Rect {
                x: panel.x + 1.0,
                y: panel.y + 51.0,
                width: (panel.width - 2.0).max(0.0),
                height: 1.0,
            },
            with_alpha(chrome.body_color, 0.08),
            BorderRadius::ZERO,
        )?;
        let helper_line_h =
            style_tokens::TYPOGRAPHY.sm.size_px * style_tokens::TYPOGRAPHY.sm.line_height;
        self.draw_text(
            if zh {
                "选择快照查看预览，再载入或删除；也可保存当前布局。"
            } else {
                "Select a snapshot to preview, load, or delete; save the current layout anytime."
            },
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 60.0,
                width: panel.width - 36.0,
                height: helper_line_h,
            },
            chrome.muted_color,
        )?;

        let state = app.snapshot_picker.borrow();
        let status = if let Some(error) = state.error() {
            smol_str::SmolStr::new(if zh {
                format!("错误：{error}")
            } else {
                format!("Error: {error}")
            })
        } else if let Some(status) = state.status() {
            status.clone()
        } else {
            smol_str::SmolStr::new(if zh {
                format!("已载入 {} 个布局快照", state.entries().len())
            } else {
                format!("Loaded {} snapshots", state.entries().len())
            })
        };
        let status_y = panel.y + 82.0;
        self.draw_text(
            status.as_str(),
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: status_y,
                width: panel.width - 36.0,
                height: 22.0,
            },
            if state.error().is_some() {
                chrome.error_color
            } else {
                chrome.muted_color
            },
        )?;

        let has_entries = !state.entries().is_empty();
        for spec in snapshot_picker::SNAPSHOT_PICKER_ACTION_BUTTONS {
            let rect = snapshot_picker::snapshot_picker_button_rect(viewport, *spec);
            let enabled = !matches!(
                spec.hit,
                snapshot_picker::SnapshotPickerPointerHit::Load
                    | snapshot_picker::SnapshotPickerPointerHit::Delete
            ) || has_entries;
            let emphasis = if !enabled {
                AuxiliaryActionEmphasis::Disabled
            } else {
                match spec.hit {
                    snapshot_picker::SnapshotPickerPointerHit::Save => {
                        AuxiliaryActionEmphasis::Primary
                    }
                    snapshot_picker::SnapshotPickerPointerHit::Delete => {
                        AuxiliaryActionEmphasis::Danger
                    }
                    _ => AuxiliaryActionEmphasis::Secondary,
                }
            };
            let action = auxiliary_action_chrome(action_palette, emphasis);
            self.fill_rounded_rect(rect, action.fill, chrome.button_radius)?;
            self.stroke_rounded_rect(rect, action.border, chrome.button_radius, 1.0)?;
            self.draw_text_no_wrap_with_style(
                snapshot_action_label(spec.hit, zh),
                rect,
                action.text,
                12.0,
                550,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }

        if !has_entries {
            let center_y = panel.y + panel.height * 0.56;
            self.draw_icon_glyph(
                IconKind::Camera.as_str(),
                bento_nano_style::Rect {
                    x: panel.x + (panel.width - 32.0) * 0.5,
                    y: center_y - 46.0,
                    width: 32.0,
                    height: 32.0,
                },
                chrome.muted_color,
            )?;
            self.draw_text_aligned(
                if zh {
                    "还没有布局快照"
                } else {
                    "No layout snapshots yet"
                },
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: center_y,
                    width: panel.width - 36.0,
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
                    "选择“保存”即可创建第一份快照。"
                } else {
                    "Select Save to create your first snapshot."
                },
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: center_y + 30.0,
                    width: panel.width - 36.0,
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

        for (index, snapshot) in state
            .entries()
            .iter()
            .take(snapshot_picker::RUNTIME_VISIBLE_ROW_LIMIT)
            .enumerate()
        {
            let row = snapshot_picker::snapshot_picker_row_rect(viewport, index);
            let bg = if index == state.cursor_index() {
                chrome.selected_background
            } else {
                chrome.row_background
            };
            self.fill_rounded_rect(row, bg, chrome.row_radius)?;
            let preview_rect = snapshot_row_preview_rect(row);
            self.draw_snapshot_thumbnail(snapshot, preview_rect, chrome.thumbnail_chrome)?;
            let title = if snapshot.name.trim().is_empty() {
                snapshot.id.as_str()
            } else {
                snapshot.name.as_str()
            };
            let text_width = (preview_rect.x - row.x - 22.0).max(48.0);
            self.draw_text(
                title,
                bento_nano_style::Rect {
                    x: row.x + 10.0,
                    y: row.y + 4.0,
                    width: text_width,
                    height: 18.0,
                },
                chrome.body_color,
            )?;
            let meta = snapshot_picker::meta_line(
                snapshot,
                snapshot.captured_at.as_str(),
                if zh { "区域" } else { "Zones" },
            );
            let confirm = state.row_action().is_awaiting_for(snapshot.id.as_str());
            let meta_text = if confirm {
                smol_str::SmolStr::new(if zh {
                    format!("{meta}　·　再次选择删除以确认")
                } else {
                    format!("{meta}  ·  Select Delete again to confirm")
                })
            } else {
                meta
            };
            self.draw_text(
                meta_text.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 10.0,
                    y: row.y + 24.0,
                    width: text_width,
                    height: 16.0,
                },
                if confirm {
                    chrome.error_color
                } else {
                    chrome.muted_color
                },
            )?;
        }
        Ok(())
    }

    pub(super) fn draw_snapshot_thumbnail(
        &mut self,
        snapshot: &DesktopSnapshot,
        rect: bento_nano_style::Rect,
        chrome: snapshot_picker::SnapshotThumbnailChrome,
    ) -> Result<(), RenderError> {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        self.fill_rounded_rect(rect, chrome.border_color, chrome.border_radius)?;
        let content_bg = inset_rect(rect, 1.0);
        self.fill_rounded_rect(content_bg, chrome.background_color, chrome.content_radius)?;

        let mut drew_any = false;
        for zone in &snapshot.zones {
            let Some(zone_rect) = snapshot_zone_thumbnail_rect(zone, rect) else {
                continue;
            };
            let fill = zone
                .accent_color
                .as_deref()
                .and_then(parse_hex_color)
                .unwrap_or(chrome.fallback_zone_color);
            self.fill_rounded_rect(zone_rect, fill, chrome.zone_radius)?;
            drew_any = true;
        }

        if !drew_any {
            self.draw_text(
                if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                    "暂无区域"
                } else {
                    "No zones"
                },
                inset_rect(rect, 8.0),
                chrome.empty_text_color,
            )?;
        }
        Ok(())
    }
}
