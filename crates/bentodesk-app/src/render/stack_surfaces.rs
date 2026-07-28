use super::*;

impl Renderer {
    pub(super) fn draw_stack_tray_overlay(&mut self, app: &AppState) -> Result<(), RenderError> {
        use bentodesk_style::i18n_zh_cn::ids;

        let Some(state) = app.stack_tray.borrow().clone() else {
            return Ok(());
        };
        let Some(anchor) = app.zones.get(state.anchor_zone_id) else {
            return Ok(());
        };
        let Some(member_ids) = app.zones.stack_member_ids(anchor.id) else {
            return Ok(());
        };
        // Wave D: consume Wave B Tauri-token SSoT for the tray panel chrome
        // instead of the legacy `bentodesk-theme` palette.
        let chrome = stack_tray::StackTrayChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let member_count = member_ids.len();
        if state.is_bloom_preview() {
            let Some(member_index) = member_ids
                .iter()
                .position(|member_id| *member_id == state.selected_member_id)
            else {
                return Ok(());
            };
            let Some(preview_zone) = app.zones.get(state.selected_member_id) else {
                return Ok(());
            };
            let petals = stack_tray::stack_bloom_petal_rects(app.viewport, anchor, member_count);
            let Some(petal) = petals.get(member_index).copied() else {
                return Ok(());
            };
            let preview =
                stack_tray::focused_bloom_preview_rect(app.viewport, petal, &petals, preview_zone);
            return self.draw_focused_preview_overlay(app, preview_zone, preview, chrome);
        }
        let tray = stack_tray::stack_tray_rect(app.viewport, anchor, member_count);
        let tray_shadow = stack_tray::panel_shadow_rect(tray, chrome.panel_shadow);
        self.fill_rounded_rect(tray_shadow, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(tray, chrome.panel_background, chrome.panel_radius)?;

        self.draw_text_no_wrap_with_style(
            bentodesk_style::t(ids::STACK_MEMBERS_LABEL),
            stack_tray::stack_tray_header_title_rect(app.viewport, anchor, member_count),
            chrome.text_primary,
            stack_tray::TRAY_TITLE_FONT_PX,
            stack_tray::TRAY_TITLE_FONT_WEIGHT,
            stack_tray::TRAY_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        let count_badge =
            stack_tray::stack_tray_header_count_rect(app.viewport, anchor, member_count);
        if count_badge.width > 0.0 && count_badge.height > 0.0 {
            self.fill_rounded_rect(
                count_badge,
                with_alpha(chrome.text_accent, 0.30),
                bentodesk_style::BorderRadius::all(count_badge.height * 0.5),
            )?;
            let count_label = format_small_count(member_count);
            let count_text_rect = bentodesk_style::Rect {
                x: count_badge.x + stack_tray::TRAY_HEADER_COUNT_BADGE_PAD_X_PX,
                y: count_badge.y,
                width: (count_badge.width - stack_tray::TRAY_HEADER_COUNT_BADGE_PAD_X_PX * 2.0)
                    .max(0.0),
                height: count_badge.height,
            };
            self.draw_text_no_wrap_with_style(
                count_label.as_str(),
                count_text_rect,
                chrome.text_primary,
                stack_tray::TRAY_COUNT_FONT_PX,
                stack_tray::TRAY_COUNT_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }

        let dissolve = stack_tray::stack_tray_dissolve_rect(app.viewport, anchor, member_count);
        self.fill_rounded_rect(dissolve, chrome.danger_background, chrome.button_radius)?;
        self.draw_icon_glyph(
            "trash",
            centered_square_rect(dissolve, 14.0),
            chrome.text_primary,
        )?;
        let close = stack_tray::stack_tray_close_rect(app.viewport, anchor, member_count);
        self.fill_rounded_rect(close, chrome.button_background, chrome.button_radius)?;
        self.draw_icon_glyph("x", centered_square_rect(close, 13.0), chrome.text_primary)?;

        let selected_id = if member_ids.contains(&state.selected_member_id) {
            state.selected_member_id
        } else {
            member_ids[0]
        };
        let drag_state = app.stack_tray_drag.get();
        for (row_index, member_id) in member_ids
            .iter()
            .copied()
            .take(stack_tray::TRAY_VISIBLE_ROW_LIMIT)
            .enumerate()
        {
            let Some(member) = app.zones.get(member_id) else {
                continue;
            };
            let row_rect =
                stack_tray::stack_tray_row_rect(app.viewport, anchor, member_count, row_index);
            self.fill_rounded_rect(
                row_rect,
                if drag_state.is_some_and(|drag| {
                    drag.anchor_zone_id == anchor.id && drag.member_id == member_id
                }) {
                    chrome.dragged_background
                } else if member_id == selected_id {
                    chrome.selected_background
                } else {
                    chrome.row_background
                },
                chrome.row_radius,
            )?;
            let icon_rect = bentodesk_style::Rect {
                x: row_rect.x + 8.0,
                y: row_rect.y + 8.0,
                width: 28.0,
                height: 22.0,
            };
            self.fill_rounded_rect(icon_rect, chrome.button_background, chrome.button_radius)?;
            self.draw_icon_glyph(member.icon.as_ref(), icon_rect, chrome.text_primary)?;
            self.draw_text_no_wrap_with_style(
                member.display_title(),
                bentodesk_style::Rect {
                    x: row_rect.x + 44.0,
                    y: row_rect.y + 6.0,
                    width: (row_rect.width - 128.0).max(0.0),
                    height: 17.0,
                },
                chrome.text_primary,
                stack_tray::TRAY_MEMBER_NAME_FONT_PX,
                stack_tray::TRAY_MEMBER_NAME_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
            let item_count = member.items.len();
            let item_label = format_small_count(item_count);
            let meta_count = stack_tray::stack_tray_member_meta_count_rect(row_rect);
            self.draw_text_no_wrap_with_style(
                item_label.as_str(),
                meta_count,
                chrome.text_muted,
                stack_tray::TRAY_MEMBER_META_FONT_PX,
                stack_tray::TRAY_MEMBER_META_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
            let detach =
                stack_tray::stack_tray_detach_rect(app.viewport, anchor, member_count, row_index);
            self.fill_rounded_rect(detach, chrome.button_background, chrome.button_radius)?;
            self.draw_icon_glyph(
                "arrow_right",
                centered_square_rect(detach, 13.0),
                chrome.text_primary,
            )?;
        }

        let status_rect = stack_tray::stack_tray_status_rect(tray);
        if member_count > stack_tray::TRAY_VISIBLE_ROW_LIMIT {
            let hidden = member_count - stack_tray::TRAY_VISIBLE_ROW_LIMIT;
            let hidden_label = format_small_count(hidden);
            self.draw_text_no_wrap_with_style(
                "+",
                stack_tray::stack_tray_status_prefix_rect(status_rect),
                chrome.text_muted,
                stack_tray::TRAY_STATUS_FONT_PX,
                stack_tray::TRAY_STATUS_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
            self.draw_text_no_wrap_with_style(
                hidden_label.as_str(),
                stack_tray::stack_tray_status_count_rect(status_rect),
                chrome.text_muted,
                stack_tray::TRAY_STATUS_FONT_PX,
                stack_tray::TRAY_STATUS_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
            self.draw_text_no_wrap_with_style(
                bentodesk_style::t(ids::STACK_MORE_MEMBERS),
                stack_tray::stack_tray_status_suffix_rect(status_rect),
                chrome.text_muted,
                stack_tray::TRAY_STATUS_FONT_PX,
                stack_tray::TRAY_STATUS_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
        } else if app.stack_tray_drag.get().is_some() {
            self.draw_text_no_wrap_with_style(
                bentodesk_style::t(ids::STACK_REORDER_HINT),
                status_rect,
                chrome.text_accent,
                stack_tray::TRAY_STATUS_FONT_PX,
                stack_tray::TRAY_STATUS_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
        } else if let Some(status) = state.status.as_ref() {
            self.draw_text_no_wrap_with_style(
                status.as_str(),
                status_rect,
                chrome.text_accent,
                stack_tray::TRAY_STATUS_FONT_PX,
                stack_tray::TRAY_STATUS_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
        }

        if !stack_tray::focused_preview_visible(anchor.id, selected_id) {
            return Ok(());
        }
        let Some(preview_zone) = app.zones.get(selected_id) else {
            return Ok(());
        };
        let preview = stack_tray::focused_preview_rect(app.viewport, tray);
        let preview_shadow = stack_tray::panel_shadow_rect(preview, chrome.panel_shadow);
        self.fill_rounded_rect(
            preview_shadow,
            chrome.panel_shadow.color,
            chrome.panel_radius,
        )?;
        self.fill_rounded_rect(preview, chrome.preview_background, chrome.panel_radius)?;
        self.draw_text_no_wrap_with_style(
            bentodesk_style::t(ids::FOCUSED_PREVIEW_TITLE),
            bentodesk_style::Rect {
                x: preview.x + 16.0,
                y: preview.y + 12.0,
                width: preview.width - 32.0,
                height: 18.0,
            },
            chrome.text_accent,
            stack_tray::PREVIEW_EYEBROW_FONT_PX,
            stack_tray::PREVIEW_EYEBROW_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            preview_zone.display_title(),
            bentodesk_style::Rect {
                x: preview.x + 16.0,
                y: preview.y + 36.0,
                width: preview.width - 32.0,
                height: 18.0,
            },
            chrome.text_primary,
            stack_tray::PREVIEW_TITLE_FONT_PX,
            stack_tray::PREVIEW_TITLE_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        let preview_w = format_small_count(preview_zone.w as usize);
        let preview_h = format_small_count(preview_zone.h as usize);
        let preview_count = format_small_count(preview_zone.items.len());
        self.draw_text_no_wrap_with_style(
            preview_w.as_str(),
            stack_tray::focused_preview_meta_number_rect(preview, 0),
            chrome.text_muted,
            stack_tray::PREVIEW_META_FONT_PX,
            stack_tray::PREVIEW_META_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            "×",
            stack_tray::focused_preview_meta_mark_rect(preview, 0),
            chrome.text_muted,
            stack_tray::PREVIEW_META_FONT_PX,
            stack_tray::PREVIEW_META_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            preview_h.as_str(),
            stack_tray::focused_preview_meta_number_rect(preview, 1),
            chrome.text_muted,
            stack_tray::PREVIEW_META_FONT_PX,
            stack_tray::PREVIEW_META_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            bentodesk_style::t(ids::STACK_DIMENSION_SEPARATOR),
            stack_tray::focused_preview_meta_mark_rect(preview, 1),
            chrome.text_muted,
            stack_tray::PREVIEW_META_FONT_PX,
            stack_tray::PREVIEW_META_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            preview_count.as_str(),
            stack_tray::focused_preview_meta_number_rect(preview, 2),
            chrome.text_muted,
            stack_tray::PREVIEW_META_FONT_PX,
            stack_tray::PREVIEW_META_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            bentodesk_style::t(ids::BULK_MANAGER_COL_ITEMS),
            stack_tray::focused_preview_meta_suffix_rect(preview),
            chrome.text_muted,
            stack_tray::PREVIEW_META_FONT_PX,
            stack_tray::PREVIEW_META_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        if preview_zone.items.is_empty() {
            self.draw_text_no_wrap_with_style(
                bentodesk_style::t(ids::FOCUSED_PREVIEW_EMPTY),
                bentodesk_style::Rect {
                    x: preview.x + 16.0,
                    y: preview.y + 92.0,
                    width: preview.width - 32.0,
                    height: 18.0,
                },
                chrome.text_muted,
                stack_tray::PREVIEW_EMPTY_FONT_PX,
                stack_tray::PREVIEW_EMPTY_FONT_WEIGHT,
                stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
        } else {
            for (idx, item) in preview_zone.items.iter().take(4).enumerate() {
                let y = preview.y + 88.0 + idx as f32 * 24.0;
                let row = bentodesk_style::Rect {
                    x: preview.x + 16.0,
                    y,
                    width: preview.width - 32.0,
                    height: 20.0,
                };
                self.fill_rounded_rect(row, chrome.row_background, chrome.preview_item_radius)?;
                self.draw_text_no_wrap_with_style(
                    item.name.as_ref(),
                    bentodesk_style::Rect {
                        x: row.x + 8.0,
                        y: row.y + 2.0,
                        width: row.width - 16.0,
                        height: 15.0,
                    },
                    chrome.text_primary,
                    stack_tray::PREVIEW_ITEM_FONT_PX,
                    stack_tray::PREVIEW_ITEM_FONT_WEIGHT,
                    stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
                    dwrite::TextAlign::DEFAULT,
                )?;
            }
        }
        Ok(())
    }

    // α5 (S2, 2026-05-24): no longer called from the Main HWND paint loop
    // (the unconditional call at :470 leaked a 4 DIP blue strip across the
    // top of the desktop overlay). Kept as `dead_code`-tolerant in case a
    // future Settings header or accent-callout reuses it; `cargo test` still
    // pins the math at :1235/1283/1303/1391 via the consumer accessors.
    pub(super) fn draw_inline_zone_search(
        &mut self,
        app: &AppState,
        panel: bentodesk_style::Rect,
        query: &str,
    ) -> Result<(), RenderError> {
        let pal = app.active_theme_tauri();
        // SAFETY: GetTickCount is total and thread-safe.
        let now_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
        let reveal = app
            .zone_search_animation_progress_at(now_ms)
            .clamp(0.0, 1.0);
        if reveal <= f32::EPSILON {
            return Ok(());
        }
        let final_input = search_bar::zone_inline_rect(panel);
        let input = bentodesk_style::Rect {
            x: final_input.right() - final_input.width * reveal,
            width: final_input.width * reveal,
            ..final_input
        };
        self.fill_rounded_rect(
            input,
            fade_color(pal.surface_subtle, reveal),
            bentodesk_style::BorderRadius::all(8.0),
        )?;
        self.stroke_rounded_rect(
            input,
            with_alpha(pal.accent_blue, 0.78 * reveal),
            bentodesk_style::BorderRadius::all(8.0),
            1.0,
        )?;
        self.draw_icon_glyph(
            IconKind::Search.as_str(),
            bentodesk_style::Rect {
                x: input.x + 10.0,
                y: input.y + 11.0,
                width: 14.0,
                height: 14.0,
            },
            fade_color(pal.text_muted, reveal),
        )?;
        self.draw_text_no_wrap_with_style(
            if query.is_empty() {
                if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                    "搜索项目…"
                } else {
                    "Search items…"
                }
            } else {
                query
            },
            bentodesk_style::Rect {
                x: input.x + 32.0,
                y: input.y,
                width: (input.width - 66.0).max(0.0),
                height: input.height,
            },
            if query.is_empty() {
                fade_color(pal.text_muted, reveal)
            } else {
                fade_color(pal.text_primary, reveal)
            },
            12.0,
            400,
            1.4,
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
        )?;
        if !query.is_empty() && reveal > 0.55 {
            let clear = search_bar::zone_inline_clear_rect(panel);
            self.draw_icon_glyph(
                IconKind::X.as_str(),
                inset_rect(clear, 4.0),
                fade_color(pal.text_muted, reveal),
            )?;
        }
        Ok(())
    }

    pub(super) fn draw_focused_preview_overlay(
        &mut self,
        app: &AppState,
        zone: &Zone,
        preview: bentodesk_style::Rect,
        chrome: stack_tray::StackTrayChrome,
    ) -> Result<(), RenderError> {
        let pal = app.active_theme_tauri();
        let palette = app.active_theme_palette();
        let radius = app.active_theme_radius_tauri();
        self.fill_frosted_rect(preview, chrome.preview_background, chrome.panel_radius)?;
        self.stroke_rounded_rect(preview, pal.border_expanded, chrome.panel_radius, 1.0)?;
        self.fill_rounded_rect(
            bentodesk_style::Rect {
                x: preview.x,
                y: preview.y,
                width: preview.width,
                height: 2.0,
            },
            pal.accent_blue,
            bentodesk_style::BorderRadius::all(radius.expanded),
        )?;

        let icon_rect = bentodesk_style::Rect {
            x: preview.x + 14.0,
            y: preview.y + 10.0,
            width: 28.0,
            height: 28.0,
        };
        self.fill_rounded_rect(icon_rect, pal.surface_subtle, chrome.button_radius)?;
        self.draw_icon_glyph(
            zone.icon.as_ref(),
            centered_square_rect(icon_rect, 18.0),
            pal.text_primary,
        )?;
        self.draw_text_no_wrap_with_style(
            zone.display_title(),
            bentodesk_style::Rect {
                x: icon_rect.right() + 10.0,
                y: preview.y + 10.0,
                width: (preview.width - 136.0).max(48.0),
                height: 28.0,
            },
            pal.text_primary,
            13.0,
            600,
            1.35,
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
        )?;
        let search = stack_tray::focused_bloom_preview_search_rect(preview);
        let close = stack_tray::focused_bloom_preview_close_rect(preview);
        self.draw_icon_glyph(
            IconKind::Search.as_str(),
            centered_square_rect(search, 16.0),
            pal.text_muted,
        )?;
        self.draw_icon_glyph(
            IconKind::X.as_str(),
            centered_square_rect(close, 16.0),
            pal.text_muted,
        )?;
        self.fill_rounded_rect(
            bentodesk_style::Rect {
                x: preview.x,
                y: preview.y + 47.0,
                width: preview.width,
                height: 1.0,
            },
            with_alpha(bentodesk_style::Color::WHITE, 0.05),
            bentodesk_style::BorderRadius::ZERO,
        )?;

        let search_active = app.zone_search_target.get() == Some(zone.id);
        let search_reveal = if search_active {
            // SAFETY: GetTickCount is total and thread-safe.
            app.zone_search_animation_progress_at(unsafe {
                windows_sys::Win32::System::SystemInformation::GetTickCount()
            })
        } else {
            0.0
        };
        let search_item_offset = search_bar::ZONE_INLINE_ITEM_OFFSET_Y_PX * search_reveal;
        let search_state = app.search_bar.borrow();
        let search_query = search_state.query.as_str();
        if search_active {
            self.draw_inline_zone_search(app, preview, search_query)?;
        }

        let item_chrome = item_card::ItemCardChrome::from_tokens(
            palette,
            app.active_theme_radius(),
            pal.surface_subtle,
            item_label_text_color_for_reference(pal),
            pal.text_primary,
            pal.surface_hover,
            pal.border_hover,
        );
        // SAFETY: `GetTickCount` is total and thread-safe. One sample keeps all
        // preview cards on the same hover/press frame.
        let anim_now_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
        let item_hover = app.item_hover.get();
        let item_drag = app.item_drag.borrow();
        let item_label_group_px = {
            let mut label_flow_slot = 0;
            item_label_group_font_size(zone.items.iter().filter_map(|item| {
                if search_active
                    && !search_bar::zone_item_matches_query(item.name.as_ref(), search_query)
                {
                    return None;
                }
                let (rect, next_slot) = highlight_overlay::item_card_rect_for_flow_slot_in_panel(
                    zone,
                    preview,
                    label_flow_slot,
                    item.is_wide,
                    search_item_offset,
                );
                label_flow_slot = next_slot;
                (rect.width > 0.0 && rect.height > 0.0).then_some((
                    item_label_visible_name(item.name.as_ref()),
                    (rect.width - 8.0).max(0.0),
                ))
            }))
        };
        let mut flow_slot = 0;
        let mut visible_item_count = 0usize;
        for item in &zone.items {
            if search_active
                && !search_bar::zone_item_matches_query(item.name.as_ref(), search_query)
            {
                continue;
            }
            visible_item_count += 1;
            let (rect, next_slot) = highlight_overlay::item_card_rect_for_flow_slot_in_panel(
                zone,
                preview,
                flow_slot,
                item.is_wide,
                search_item_offset,
            );
            flow_slot = next_slot;
            if rect.width <= 0.0 || rect.height <= 0.0 {
                continue;
            }
            let is_dragged_source = item_drag
                .as_ref()
                .is_some_and(|drag| drag.zone_id == zone.id && drag.item_id == item.id);
            let card_key = (zone.id, item.id);
            let (hover_raw, press_t) = if is_dragged_source {
                (0.0, 0.0)
            } else {
                item_hover.sample(card_key, anim_now_ms)
            };
            let hover_t = if is_dragged_source || item.file_missing {
                0.0
            } else {
                hover_raw
            };
            let item_scale = if is_dragged_source {
                1.0
            } else {
                item_card::card_scale_for(hover_raw, press_t)
            };
            self.draw_item_card(
                item,
                rect,
                if is_dragged_source {
                    item_chrome.drag_source_background
                } else if item.file_missing {
                    item_chrome.missing_background
                } else {
                    item_chrome.normal_background
                },
                &item_chrome,
                hover_t,
                !is_dragged_source && item_hover.press_held(card_key),
                item_scale,
                item_label_group_px,
                1.0,
            )?;
        }
        if search_active && visible_item_count == 0 {
            self.draw_text_no_wrap_with_style(
                bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::SEARCH_EMPTY),
                bentodesk_style::Rect {
                    x: preview.x + expanded_zone_grid::HEADER_INSET_X,
                    y: preview.y + item_grid::ITEM_GRID_TOP_OFFSET_PX + search_item_offset,
                    width: (preview.width - expanded_zone_grid::HEADER_INSET_X * 2.0).max(0.0),
                    height: 28.0,
                },
                pal.text_muted,
                12.0,
                400,
                1.4,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }
        Ok(())
    }
}
