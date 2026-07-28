use super::*;

impl Renderer {
    pub(super) fn draw_context_menu_row(
        &mut self,
        row: &popover::ContextMenuRow,
        rect: bentodesk_style::Rect,
        hovered: bool,
        palette: bentodesk_style::tokens::PaletteTauri,
        radius: f32,
    ) -> Result<(), RenderError> {
        if row.kind == popover::ContextMenuRowKind::Separator {
            let line = bentodesk_style::Rect {
                x: rect.x + 11.0,
                y: rect.y + (rect.height - 1.0) * 0.5,
                width: (rect.width - 22.0).max(0.0),
                height: 1.0,
            };
            self.fill_rounded_rect(
                line,
                with_alpha(palette.border_expanded, 0.22),
                BorderRadius::all(0.5),
            )?;
            return Ok(());
        }

        let row_body = bentodesk_style::Rect {
            x: rect.x + 5.0,
            y: rect.y + 1.0,
            width: (rect.width - 10.0).max(0.0),
            height: (rect.height - 2.0).max(0.0),
        };
        if hovered {
            let hover = if row.danger {
                with_alpha(palette.accent_red, 0.14)
            } else {
                // `surface_hover` already carries the theme's intentionally
                // subtle alpha. Replacing it with 0.92 turns light RGB tokens
                // into an opaque white bar and destroys label contrast.
                palette.surface_hover
            };
            self.fill_rounded_rect(row_body, hover, BorderRadius::all(radius))?;
        }

        let foreground = if row.danger {
            palette.accent_red
        } else {
            palette.text_primary
        };
        if let Some(icon) = row.icon {
            let icon_rect = bentodesk_style::Rect {
                x: rect.x + 12.0,
                y: rect.y + (rect.height - popover::CONTEXT_MENU_ICON_SIZE) * 0.5,
                width: popover::CONTEXT_MENU_ICON_SIZE,
                height: popover::CONTEXT_MENU_ICON_SIZE,
            };
            self.draw_icon_glyph(
                icon.as_str(),
                icon_rect,
                with_alpha(foreground, if hovered { 1.0 } else { 0.78 }),
            )?;
        }

        let arrow_reserve = if row.kind == popover::ContextMenuRowKind::Submenu {
            24.0
        } else {
            0.0
        };
        let label_rect = bentodesk_style::Rect {
            x: rect.x + 38.0,
            y: rect.y,
            width: (rect.width - 38.0 - 12.0 - arrow_reserve).max(0.0),
            height: rect.height,
        };
        self.draw_text_no_wrap_with_style(
            row.label.as_str(),
            label_rect,
            foreground,
            12.25,
            if hovered { 550 } else { 450 },
            1.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
        )?;
        if row.kind == popover::ContextMenuRowKind::Submenu {
            let arrow = bentodesk_style::Rect {
                x: rect.right() - 22.0,
                y: rect.y + (rect.height - 12.0) * 0.5,
                width: 12.0,
                height: 12.0,
            };
            self.draw_icon_glyph(
                IconKind::ArrowRight.as_str(),
                arrow,
                with_alpha(palette.text_muted, 0.90),
            )?;
        }
        Ok(())
    }

    pub(super) fn draw_context_menu_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let session = app.active_context_menu.borrow();
        let Some(session) = session.as_ref() else {
            return Ok(());
        };
        let palette = app.active_theme_tauri();
        let theme_radius = app.active_theme_radius_tauri();
        let card_radius = theme_radius.card.max(10.0);
        let row_radius = (card_radius - 2.0).max(4.0);
        // Keep the card dense enough for legibility without turning it into a
        // hard black rectangle floating above the desktop. The DComp surface
        // preserves this alpha, matching the glassier Tauri menu treatment.
        let surface = with_alpha(palette.surface_expanded, 0.96);

        for column in [
            popover::ContextMenuColumn::Main,
            popover::ContextMenuColumn::Submenu,
        ] {
            let Some(card) = popover::context_menu_card_rect(session, column) else {
                continue;
            };
            let shadow = bentodesk_style::Rect {
                x: card.x + 1.0,
                y: card.y + 3.0,
                width: card.width,
                height: card.height,
            };
            self.fill_rounded_rect(
                shadow,
                Color::rgba(0.0, 0.0, 0.0, 0.14),
                BorderRadius::all(card_radius + 1.0),
            )?;
            self.fill_rounded_rect(card, surface, BorderRadius::all(card_radius))?;
            self.stroke_rounded_rect(
                card,
                with_alpha(palette.border_expanded, 0.36),
                BorderRadius::all(card_radius),
                1.0,
            )?;
        }

        for row_index in 0..session.main_rows.len() {
            let hit = popover::ContextMenuHit {
                column: popover::ContextMenuColumn::Main,
                row: row_index,
            };
            if let Some(rect) = popover::context_menu_row_rect(session, hit) {
                self.draw_context_menu_row(
                    &session.main_rows[row_index],
                    rect,
                    session.hovered == Some(hit),
                    palette,
                    row_radius,
                )?;
            }
        }

        if session.submenu_open {
            let range = session.visible_submenu_range();
            for row_index in range.clone() {
                let hit = popover::ContextMenuHit {
                    column: popover::ContextMenuColumn::Submenu,
                    row: row_index,
                };
                if let Some(rect) = popover::context_menu_row_rect(session, hit) {
                    self.draw_context_menu_row(
                        &session.submenu_rows[row_index],
                        rect,
                        session.hovered == Some(hit),
                        palette,
                        row_radius,
                    )?;
                }
            }
            if let Some(card) =
                popover::context_menu_card_rect(session, popover::ContextMenuColumn::Submenu)
            {
                let max_start = session
                    .submenu_rows
                    .len()
                    .saturating_sub(popover::CONTEXT_MENU_MAX_SUBMENU_ROWS);
                if session.submenu_scroll > 0 {
                    self.fill_rounded_rect(
                        bentodesk_style::Rect {
                            x: card.right() - 4.0,
                            y: card.y + 8.0,
                            width: 2.0,
                            height: 12.0,
                        },
                        palette.accent_blue,
                        BorderRadius::all(1.0),
                    )?;
                }
                if session.submenu_scroll < max_start {
                    self.fill_rounded_rect(
                        bentodesk_style::Rect {
                            x: card.right() - 4.0,
                            y: card.bottom() - 20.0,
                            width: 2.0,
                            height: 12.0,
                        },
                        palette.accent_blue,
                        BorderRadius::all(1.0),
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn draw_tooltip_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let Some(session) = app.active_tooltip.borrow().clone() else {
            return Ok(());
        };
        // Wave E: Tauri SSoT tokens for the tooltip pill.
        use bentodesk_style::tokens as style_tokens;
        let descriptor = tooltip::Tooltip::from_tauri_tokens(
            session.text,
            app.active_theme_tauri(),
            // tooltip radius is global chrome (same for every theme, design §1.2)
            // — the per-theme `RadiusTauri` carries the global tooltip/minibar.
            app.active_theme_radius_tauri(),
            style_tokens::SPACING,
        );
        let pill = tooltip::tooltip_pill_rect(app.viewport);
        self.fill_rounded_rect(pill, descriptor.background, descriptor.border_radius)?;
        let text_rect = tooltip::tooltip_text_rect(app.viewport, &descriptor);
        self.draw_text(descriptor.text.as_str(), text_rect, descriptor.text_color)?;
        Ok(())
    }

    pub(super) fn draw_minibar_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let Some((zone_id, bar)) = app.active_minibar() else {
            return Ok(());
        };
        // Wave D: paint the MiniBar from Wave B Tauri SSoT tokens (gradient
        // top stop + 14 px radius — Wave A flagged gap).
        use bentodesk_style::tokens as style_tokens;
        let tauri_palette = app.active_theme_tauri();
        let bar = bar.with_tauri_tokens(
            tauri_palette,
            // minibar radius is global chrome (same for every theme, design §1.2).
            app.active_theme_radius_tauri(),
            style_tokens::SPACING,
        );
        let viewport = app.viewport;
        let panel = minibar::minibar_panel_rect(viewport);
        self.fill_rounded_rect_vertical_gradient(
            panel,
            tauri_palette.minibar_gradient_top,
            tauri_palette.minibar_gradient_bottom,
            bar.border_radius,
        )?;

        let icon_rect = minibar::minibar_icon_rect(viewport, &bar);
        self.draw_svg_fit(
            bar.icon_svg_path,
            icon_rect,
            bar.unpin_button.tint,
            bar.unpin_button.size,
        )?;

        let label_rect = minibar::minibar_label_rect(viewport, &bar);
        match app.zones.get(zone_id) {
            Some(zone) if zone.items.is_empty() => {
                self.draw_text(
                    if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                        "区域中暂无项目"
                    } else {
                        "Empty zone"
                    },
                    label_rect,
                    bar.unpin_button.tint,
                )?;
            }
            Some(zone) => {
                let capacity = minibar::minibar_item_capacity(viewport, &bar);
                for (index, item) in zone
                    .items
                    .iter()
                    .take(capacity.min(minibar::MINIBAR_SOURCE_MAX_ITEMS))
                    .enumerate()
                {
                    if let Some(item_rect) = minibar::minibar_item_rect(viewport, &bar, index) {
                        self.fill_rounded_rect(
                            item_rect,
                            bar.unpin_button.hover_background,
                            BorderRadius::all(8.0),
                        )?;
                        // M2 R4 (2026-05-29) — try the REAL extracted icon
                        // bitmap first (mirrors `draw_item_card`). Only when
                        // the cache misses / decode fails do we fall back to
                        // the extension-derived selected-stack line-art glyph.
                        // RC-4 Gap 1 — the 32×32 capsule is far too narrow for
                        // a full file name (the old "ite ite ite" symptom);
                        // the capsule is a glance affordance, the full name
                        // lives in the tray.
                        let icon_rect = bentodesk_style::Rect {
                            x: item_rect.x + 4.0,
                            y: item_rect.y + 4.0,
                            width: (item_rect.width - 8.0).max(0.0),
                            height: (item_rect.height - 8.0).max(0.0),
                        };
                        if !self.draw_item_bitmap(item.icon_hash.as_ref(), icon_rect, 1.0)? {
                            let kind = item_icon::fallback_icon_kind_for_item(
                                item.icon_hash.as_ref(),
                                item.path.as_ref(),
                            );
                            self.draw_icon_glyph(kind.as_str(), icon_rect, bar.unpin_button.tint)?;
                        }
                    }
                }
            }
            None => {
                self.draw_text(bar.label.as_str(), label_rect, bar.unpin_button.tint)?;
            }
        }

        let unpin_rect = minibar::minibar_unpin_rect(viewport, &bar);
        self.draw_svg_fit(
            bar.unpin_button.svg_path,
            unpin_rect,
            bar.unpin_button.tint,
            bar.unpin_button.size,
        )?;
        Ok(())
    }
}
