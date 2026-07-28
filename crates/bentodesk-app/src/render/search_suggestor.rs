use super::*;

impl Renderer {
    pub(super) fn draw_search_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        // Wave E: source visual chrome from the Wave B Tauri SSoT
        // (`bentodesk_style::tokens::PALETTE_DARK / RADIUS / SHADOW`) so the
        // selected-stack runtime panels render against the same tokens the
        // Tauri 1.2.4 baseline used. Legacy `from_tokens` constructor is
        // retained for back-compat callers (theme palette mutation tests).
        let chrome = search_bar::SearchBarChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = search_bar::search_panel_rect(viewport);
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        self.stroke_rounded_rect(
            panel,
            app.active_theme_tauri().border_expanded,
            chrome.panel_radius,
            1.0,
        )?;
        use bentodesk_style::i18n_zh_cn::ids;
        // M6c — search panel title (`h2`).
        self.draw_text_chromatic_title(
            bentodesk_style::t(ids::SEARCH_TITLE),
            bentodesk_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: panel.width - 76.0,
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        let close = search_bar::search_close_rect(viewport);
        self.draw_icon_glyph(
            IconKind::X.as_str(),
            centered_square_rect(close, 16.0),
            chrome.muted_color,
        )?;

        let state = app.search_bar.borrow();
        let input = search_bar::search_input_rect(viewport);
        self.fill_rounded_rect(input, chrome.input_background, chrome.input_radius)?;
        self.stroke_rounded_rect(
            input,
            with_alpha(app.active_theme_tauri().accent_blue, 0.70),
            chrome.input_radius,
            1.0,
        )?;
        self.draw_icon_glyph(
            IconKind::Search.as_str(),
            bentodesk_style::Rect {
                x: input.x + 14.0,
                y: input.y + 15.0,
                width: 18.0,
                height: 18.0,
            },
            chrome.muted_color,
        )?;
        let query_text = if state.query.is_empty() {
            bentodesk_style::t(ids::SEARCH_PLACEHOLDER)
        } else {
            state.query.as_str()
        };
        self.draw_text(
            query_text,
            bentodesk_style::Rect {
                x: input.x + 42.0,
                y: input.y + 12.0,
                width: input.width - 56.0,
                height: 24.0,
            },
            if state.query.is_empty() {
                chrome.muted_color
            } else {
                chrome.body_color
            },
        )?;

        let status = if state.query.is_empty() {
            smol_str::SmolStr::new_static(bentodesk_style::t(ids::SEARCH_IDLE_HINT))
        } else if state.results.is_empty() {
            smol_str::SmolStr::new_static(bentodesk_style::t(ids::SEARCH_EMPTY))
        } else {
            smol_str::SmolStr::new(format!(
                "{}{}",
                state.visible_count(),
                bentodesk_style::t(ids::SEARCH_RESULTS_SUFFIX)
            ))
        };
        self.draw_text(
            status.as_str(),
            bentodesk_style::Rect {
                x: input.x,
                y: input.bottom() + 8.0,
                width: input.width,
                height: 22.0,
            },
            chrome.muted_color,
        )?;

        if state.results.is_empty() {
            self.draw_icon_glyph(
                IconKind::Search.as_str(),
                bentodesk_style::Rect {
                    x: panel.x + (panel.width - 28.0) * 0.5,
                    y: input.bottom() + 70.0,
                    width: 28.0,
                    height: 28.0,
                },
                with_alpha(chrome.muted_color, 0.75),
            )?;
            return Ok(());
        }

        for (index, hit) in state
            .results
            .iter()
            .take(search_bar::MAX_VISIBLE_RESULTS)
            .enumerate()
        {
            let row = search_bar::search_row_rect(viewport, index);
            let row_bg = if state.selected_index() == Some(index) {
                chrome.selected_background
            } else {
                chrome.row_background
            };
            self.fill_rounded_rect(row, row_bg, chrome.row_radius)?;
            let icon_rect = bentodesk_style::Rect {
                x: row.x + 12.0,
                y: row.y + 9.0,
                width: 28.0,
                height: 28.0,
            };
            self.draw_icon_glyph(hit.icon.as_str(), icon_rect, chrome.body_color)?;
            self.draw_text(
                hit.name.as_str(),
                bentodesk_style::Rect {
                    x: row.x + 58.0,
                    y: row.y + 6.0,
                    width: row.width - 180.0,
                    height: 18.0,
                },
                chrome.body_color,
            )?;
            self.draw_text(
                hit.breadcrumb.as_str(),
                bentodesk_style::Rect {
                    x: row.x + 58.0,
                    y: row.y + 25.0,
                    width: row.width - 180.0,
                    height: 16.0,
                },
                chrome.muted_color,
            )?;
            let kind_label = match &hit.kind {
                bentodesk_backend::search::SearchItemKind::File => {
                    bentodesk_style::t(ids::SEARCH_KIND_FILE)
                }
                bentodesk_backend::search::SearchItemKind::Folder => {
                    bentodesk_style::t(ids::SEARCH_KIND_FOLDER)
                }
                bentodesk_backend::search::SearchItemKind::Zone => {
                    bentodesk_style::t(ids::SEARCH_KIND_ZONE)
                }
                bentodesk_backend::search::SearchItemKind::Setting => {
                    bentodesk_style::t(ids::SEARCH_KIND_SETTING)
                }
                bentodesk_backend::search::SearchItemKind::Action => {
                    bentodesk_style::t(ids::SEARCH_KIND_ACTION)
                }
            };
            self.draw_text(
                kind_label,
                bentodesk_style::Rect {
                    x: row.right() - 112.0,
                    y: row.y + 14.0,
                    width: 100.0,
                    height: 18.0,
                },
                chrome.muted_color,
            )?;
        }
        Ok(())
    }

    pub(super) fn draw_suggestor_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);
        // Wave E: Tauri SSoT tokens for the Smart-group suggestor panel.
        // Confidence-badge colours route through the dedicated Tauri tone
        // helper so badges use `accent_green` / `accent_orange` / `text_muted`
        // per Wave A `search-bar-and-suggestor.md`.
        use bentodesk_style::tokens as style_tokens;
        // M6a — live theme palette for the suggestor panel chrome.
        let palette = app.active_theme_tauri();
        let chrome = smart_group_suggestor::SmartGroupSuggestorChrome::from_tauri_tokens(
            palette,
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = smart_group_suggestor::suggestor_panel_rect(viewport);
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        // M6c — smart-group suggestor panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "智能分组建议"
            } else {
                "Smart grouping"
            },
            bentodesk_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: panel.width - 110.0,
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        let close = smart_group_suggestor::suggestor_close_rect(viewport);
        self.fill_rounded_rect(close, chrome.close_background, chrome.close_radius)?;
        self.draw_icon_glyph("x", centered_square_rect(close, 14.0), chrome.muted_color)?;
        self.fill_rounded_rect(
            bentodesk_style::Rect {
                x: panel.x + 1.0,
                y: panel.y + 51.0,
                width: (panel.width - 2.0).max(0.0),
                height: 1.0,
            },
            with_alpha(chrome.body_color, 0.08),
            BorderRadius::ZERO,
        )?;
        let line_height =
            style_tokens::TYPOGRAPHY.sm.size_px * style_tokens::TYPOGRAPHY.sm.line_height;
        let helper_top = panel.y + 58.0;
        self.draw_text(
            if zh {
                "选择建议查看匹配文件，按需调整范围后应用。"
            } else {
                "Select a suggestion, review its files, then refine and apply."
            },
            bentodesk_style::Rect {
                x: panel.x + 18.0,
                y: helper_top,
                width: panel.width - 36.0,
                height: line_height,
            },
            chrome.muted_color,
        )?;

        let state = app.suggestor.borrow();
        let status = app.suggestor_status.borrow().clone().unwrap_or_else(|| {
            smol_str::SmolStr::new(if zh {
                format!("已生成 {} 条分组建议", state.entries().len())
            } else {
                format!("Loaded {} suggestions", state.entries().len())
            })
        });
        let status_top = panel.y + smart_group_suggestor::RUNTIME_STATUS_TOP_PX;
        self.draw_text(
            status.as_str(),
            bentodesk_style::Rect {
                x: panel.x + 18.0,
                y: status_top,
                width: panel.width - 36.0,
                height: line_height,
            },
            chrome.muted_color,
        )?;

        if state.entries().is_empty() {
            self.draw_text(
                if zh {
                    "当前桌面扫描暂未生成可用的分组建议。"
                } else {
                    "The current Desktop scan did not produce any grouping suggestions."
                },
                bentodesk_style::Rect {
                    x: panel.x + 18.0,
                    y: panel.y + smart_group_suggestor::RUNTIME_ROW_TOP_PX,
                    width: panel.width - 36.0,
                    height: 24.0,
                },
                chrome.body_color,
            )?;
            return Ok(());
        }

        for (index, entry) in state
            .entries()
            .iter()
            .take(smart_group_suggestor::MAX_VISIBLE_SUGGESTIONS)
            .enumerate()
        {
            let row = smart_group_suggestor::suggestor_row_rect(viewport, index);
            let row_bg = if index == state.selected_index() {
                chrome.selected_background
            } else {
                chrome.row_background
            };
            self.fill_rounded_rect(row, row_bg, chrome.row_radius)?;
            self.stroke_rounded_rect(
                row,
                with_alpha(chrome.body_color, 0.06),
                chrome.row_radius,
                1.0,
            )?;
            let icon_rect = bentodesk_style::Rect {
                x: row.x + 12.0,
                y: row.y + ((row.height - smart_group_suggestor::ROW_ICON_SIZE_PX) * 0.5),
                width: smart_group_suggestor::ROW_ICON_SIZE_PX,
                height: smart_group_suggestor::ROW_ICON_SIZE_PX,
            };
            self.draw_icon_glyph(entry.suggestion.icon.as_str(), icon_rect, chrome.body_color)?;
            let apply = smart_group_suggestor::suggestor_apply_rect(viewport, index);
            let dismiss = smart_group_suggestor::suggestor_dismiss_rect(viewport, index);
            let badge = bentodesk_style::Rect {
                x: apply.x - 82.0,
                y: row.y + 17.0,
                width: 72.0,
                height: 20.0,
            };
            // Wave F carry-over #2: title must respect badge's left edge.
            // Drop the .max(96.0) floor so we never paint into the badge;
            // route through no-wrap so an over-wide title is character-trimmed
            // inside its box instead of stamping a fragment across the badge.
            let text_width = (badge.x - (row.x + 50.0) - 12.0).max(0.0);
            self.draw_text_no_wrap_with_style(
                localized_suggestor_group_name(entry.suggestion.name.as_str(), zh),
                bentodesk_style::Rect {
                    x: row.x + 50.0,
                    y: row.y + 6.0,
                    width: text_width,
                    height: 19.0,
                },
                chrome.body_color,
                12.5,
                600,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Leading,
                    v: dwrite::VAlign::Center,
                },
            )?;
            let summary = localized_suggestor_rule_summary(&entry.suggestion, zh);
            let meta = smol_str::SmolStr::new(if zh {
                format!(
                    "已选择 {}/{}　· {}",
                    entry.selected_path_count(),
                    entry.total_path_count(),
                    summary
                )
            } else {
                format!(
                    "{}/{} selected · {}",
                    entry.selected_path_count(),
                    entry.total_path_count(),
                    summary
                )
            });
            self.draw_text_no_wrap_with_style(
                meta.as_str(),
                bentodesk_style::Rect {
                    x: row.x + 50.0,
                    y: row.y + 29.0,
                    width: text_width,
                    height: 17.0,
                },
                chrome.muted_color,
                10.0,
                450,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Leading,
                    v: dwrite::VAlign::Center,
                },
            )?;

            let tone = smart_group_suggestor::confidence_tone(entry.suggestion.confidence);
            let (badge_bg, badge_text) =
                smart_group_suggestor::tone_colors_from_tauri_palette(tone, palette);
            self.fill_rounded_rect(badge, badge_bg, chrome.badge_radius)?;
            let confidence = smol_str::SmolStr::new(format!(
                "{} {}%",
                confidence_tone_label(tone, zh),
                (entry.suggestion.confidence * 100.0).round() as i32
            ));
            self.draw_text_no_wrap_with_style(
                confidence.as_str(),
                badge,
                badge_text,
                10.0,
                550,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;

            self.fill_rounded_rect(apply, chrome.action_background, chrome.action_radius)?;
            let apply_text = if state.applying_id() == Some(&entry.id) {
                if zh { "应用中" } else { "Applying" }
            } else {
                if zh { "应用" } else { "Apply" }
            };
            self.draw_text_no_wrap_with_style(
                apply_text,
                apply,
                palette.readable_text_on(chrome.action_background),
                11.0,
                600,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
            self.fill_rounded_rect(
                dismiss,
                with_alpha(chrome.danger_background, 0.12),
                chrome.action_radius,
            )?;
            self.stroke_rounded_rect(
                dismiss,
                with_alpha(chrome.danger_background, 0.28),
                chrome.action_radius,
                1.0,
            )?;
            self.draw_icon_glyph(
                "x",
                centered_square_rect(dismiss, 12.0),
                chrome.danger_background,
            )?;
        }

        if let Some(entry) = state.selected_entry() {
            let preview = smart_group_suggestor::suggestor_preview_rect(viewport);
            self.fill_rounded_rect(preview, chrome.preview_background, chrome.preview_radius)?;
            self.stroke_rounded_rect(
                preview,
                with_alpha(chrome.body_color, 0.08),
                chrome.preview_radius,
                1.0,
            )?;
            let title = smol_str::SmolStr::new(format!(
                "{}：{}/{} {}",
                if zh {
                    "本次整理范围"
                } else {
                    "Files to organize"
                },
                entry.selected_path_count(),
                entry.total_path_count(),
                if zh { "项已选择" } else { "selected" }
            ));
            self.draw_text(
                title.as_str(),
                bentodesk_style::Rect {
                    x: preview.x + 8.0,
                    y: preview.y + 8.0,
                    width: preview.width - 128.0,
                    height: 16.0,
                },
                chrome.body_color,
            )?;

            let all = smart_group_suggestor::suggestor_select_all_rect(viewport);
            self.fill_rounded_rect(all, chrome.close_background, chrome.preview_button_radius)?;
            self.stroke_rounded_rect(
                all,
                with_alpha(chrome.body_color, 0.12),
                chrome.preview_button_radius,
                1.0,
            )?;
            self.draw_text_no_wrap_with_style(
                if zh { "全选" } else { "All" },
                all,
                chrome.body_color,
                10.0,
                550,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
            let none = smart_group_suggestor::suggestor_select_none_rect(viewport);
            self.fill_rounded_rect(none, chrome.close_background, chrome.preview_button_radius)?;
            self.stroke_rounded_rect(
                none,
                with_alpha(chrome.body_color, 0.12),
                chrome.preview_button_radius,
                1.0,
            )?;
            self.draw_text_no_wrap_with_style(
                if zh { "清空" } else { "None" },
                none,
                chrome.muted_color,
                10.0,
                550,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;

            for offset in 0..entry.preview_file_count() {
                let Some(path_index) = entry.preview_path_index(offset) else {
                    continue;
                };
                let Some(path) = entry.suggestion.matching_files.get(path_index) else {
                    continue;
                };
                let rect = smart_group_suggestor::suggestor_preview_file_rect(viewport, offset);
                let focused = path_index == entry.focused_path_index();
                let checked = entry.is_path_selected(path_index);
                let marker = match (focused, checked) {
                    (true, true) => "› ✓",
                    (true, false) => "› ○",
                    (false, true) => "  ✓",
                    (false, false) => "  ○",
                };
                let label = smol_str::SmolStr::new(format!(
                    "{} {}",
                    marker,
                    smart_group_suggestor::path_basename(path)
                ));
                self.draw_text_no_wrap(
                    label.as_str(),
                    bentodesk_style::Rect {
                        x: rect.x,
                        y: rect.y + 1.0,
                        width: rect.width,
                        height: rect.height,
                    },
                    if checked {
                        chrome.body_color
                    } else {
                        chrome.muted_color
                    },
                )?;
            }
        }
        Ok(())
    }
}
