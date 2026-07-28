use super::*;

impl Renderer {
    pub(super) fn draw_debug_overlay(&mut self, app: &AppState) -> Result<(), RenderError> {
        let (fps, rss_mb, frame_us) = {
            let state = app.debug_overlay.borrow();
            if !state.visible {
                return Ok(());
            }
            (state.fps(), state.last_rss_mb, state.last_frame_us)
        };
        let chrome = debug_overlay::DebugOverlayChrome::from_tokens(
            app.active_theme_palette(),
            app.active_theme_radius(),
            app.active_theme_spacing(),
            app.active_theme_shadow(),
        );
        let panel = Rect {
            x: (app.viewport.width - debug_overlay::OVERLAY_WIDTH - debug_overlay::EDGE_MARGIN)
                .max(debug_overlay::EDGE_MARGIN),
            y: debug_overlay::EDGE_MARGIN,
            width: debug_overlay::OVERLAY_WIDTH,
            height: debug_overlay::OVERLAY_HEIGHT,
        };
        let shadow = debug_overlay::panel_shadow_rect(panel, chrome.shadow);
        self.fill_rounded_rect(shadow, chrome.shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(panel, chrome.panel, chrome.panel_radius)?;
        let text_width = panel.width - chrome.text_inset_x * 2.0;
        self.draw_text(
            "Debug Overlay",
            Rect {
                x: panel.x + chrome.text_inset_x,
                y: panel.y + chrome.title_top,
                width: text_width,
                height: chrome.title_height,
            },
            chrome.title,
        )?;
        let fps_line = format!("FPS: {fps:>3}");
        let rss_line = format!("RSS: {rss_mb:>4.1} MB");
        let frame_line = format!("Frame: {:>5.2} ms", frame_us as f32 / 1000.0);
        self.draw_text(
            &fps_line,
            Rect {
                x: panel.x + chrome.text_inset_x,
                y: panel.y + chrome.metric_first_top,
                width: text_width,
                height: chrome.metric_row_height,
            },
            chrome.body,
        )?;
        self.draw_text(
            &rss_line,
            Rect {
                x: panel.x + chrome.text_inset_x,
                y: panel.y + chrome.metric_first_top + chrome.metric_row_gap,
                width: text_width,
                height: chrome.metric_row_height,
            },
            chrome.body,
        )?;
        self.draw_text(
            &frame_line,
            Rect {
                x: panel.x + chrome.text_inset_x,
                y: panel.y + chrome.metric_first_top + chrome.metric_row_gap * 2.0,
                width: text_width,
                height: chrome.metric_row_height,
            },
            chrome.muted,
        )
    }

    pub(super) fn draw_highlight_overlay(&mut self, app: &AppState) -> Result<(), RenderError> {
        let overlay = app.highlight_overlay.borrow();
        if !overlay.has_targets() {
            return Ok(());
        }
        // Wave E: Tauri SSoT tokens for highlight overlay accents.
        // M6a — re-skin from the live theme palette (bound once per fn, §10).
        let pal = app.active_theme_tauri();
        let fill = highlight_overlay::fill_color_from_tauri_palette(pal);
        let outline = highlight_overlay::outline_color_from_tauri_palette(pal);
        let radius =
            highlight_overlay::target_radius_from_tauri_tokens(app.active_theme_radius_tauri());
        for target in overlay.targets().iter().copied() {
            let paint = highlight_overlay::paint_rect(target);
            if paint.width <= 0.0 || paint.height <= 0.0 {
                continue;
            }
            if overlay.show_outline() {
                self.fill_rounded_rect(paint, outline, radius)?;
                let inner = inset_rect(paint, highlight_overlay::OUTLINE_WIDTH_PX);
                self.fill_rounded_rect(inner, fill, radius)?;
            } else {
                self.fill_rounded_rect(paint, fill, radius)?;
            }
        }
        if !overlay.pulses().is_empty() {
            let phase = overlay.current_pulse_phase();
            let halo = highlight_overlay::pulse_halo_color_from_tauri_palette(pal, phase);
            let core = highlight_overlay::pulse_core_color_from_tauri_palette(pal);
            for target in overlay.pulses() {
                let halo_rect = highlight_overlay::pulse_halo_rect(target, phase);
                if halo_rect.width > 0.0 && halo_rect.height > 0.0 {
                    self.fill_rounded_rect(
                        halo_rect,
                        halo,
                        BorderRadius::all(halo_rect.width * 0.5),
                    )?;
                }
                let core_rect = highlight_overlay::pulse_core_rect(target);
                if core_rect.width > 0.0 && core_rect.height > 0.0 {
                    self.fill_rounded_rect(
                        core_rect,
                        core,
                        BorderRadius::all(core_rect.width * 0.5),
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn draw_node(
        &mut self,
        node: &WidgetNode,
        rect: bentodesk_style::Rect,
    ) -> Result<(), RenderError> {
        match node {
            WidgetNode::Container(c) => {
                self.fill_rounded_rect(rect, c.background, c.radius)?;
            }
            WidgetNode::Button(b) => {
                self.fill_rounded_rect(rect, b.background, b.radius)?;
                if !b.label.is_empty() {
                    self.draw_text(&b.label, rect, b.label_color)?;
                }
            }
            WidgetNode::Text(t) => {
                self.draw_text_with_style(
                    t.resolved(),
                    rect,
                    t.color,
                    t.font_size_pt,
                    t.font_weight,
                    t.line_height,
                )?;
            }
            WidgetNode::Image(img) => {
                if let ImageSource::SvgPath(path) = &img.source {
                    if !path.is_empty() {
                        self.draw_svg(path.as_str(), rect, img.tint)?;
                    }
                } else if let ImageSource::File(path) = &img.source {
                    self.draw_image_file(path.as_str(), rect)?;
                }
            }
            WidgetNode::BentoCard(card) => {
                // Shadow rendering hooks into D2D's shadow effect in PHASE_2;
                // for now we draw the rounded fill so the card geometry is
                // visible in the spike. Spec §17 — shadow is non-lever
                // visual polish and stays out of Phase 1.2's binary budget.
                self.fill_rounded_rect(rect, card.background, card.border_radius)?;
            }
            WidgetNode::Toolbar(_) => {
                // Toolbar is a flex container with no own visual — children
                // are dispatched by the outer iter loop. Nothing to draw
                // here, intentionally.
            }
            WidgetNode::IconButton(ib) => {
                // Hover background — interpolate alpha by hover_progress.
                let p = ib.hover_progress();
                if p > 0.0 {
                    let bg = bentodesk_style::Color {
                        a: ib.hover_background.a * p,
                        ..ib.hover_background
                    };
                    self.fill_rounded_rect(rect, bg, ib.hover_radius)?;
                }
                // SVG glyph — `svg_path` is a 24×24 viewbox path. `draw_svg`
                // applies scale-to-fit using the icon's source viewbox.
                if !ib.svg_path.is_empty() {
                    self.draw_svg_fit(ib.svg_path, rect, ib.tint, 24.0)?;
                }
            }
            WidgetNode::ScrollContainer(_) => {
                // Container with no own visual — content clipping happens
                // when the layout engine grows clip-rect support
                // (PHASE_2). Children are dispatched by the outer iter
                // loop, so the static frame is correct today.
            }
            WidgetNode::Checkbox(c) => {
                let p = c.fill_progress();
                let bg = bentodesk_style::Color {
                    r: c.box_color.r + (c.box_color_checked.r - c.box_color.r) * p,
                    g: c.box_color.g + (c.box_color_checked.g - c.box_color.g) * p,
                    b: c.box_color.b + (c.box_color_checked.b - c.box_color.b) * p,
                    a: c.box_color.a + (c.box_color_checked.a - c.box_color.a) * p,
                };
                self.fill_rounded_rect(rect, bg, c.radius)?;
            }
            WidgetNode::Toggle(t) => {
                let p = t.thumb_anim.current();
                let bg = bentodesk_style::Color {
                    r: t.track_off.r + (t.track_on.r - t.track_off.r) * p,
                    g: t.track_off.g + (t.track_on.g - t.track_off.g) * p,
                    b: t.track_off.b + (t.track_on.b - t.track_off.b) * p,
                    a: t.track_off.a + (t.track_on.a - t.track_off.a) * p,
                };
                self.fill_rounded_rect(rect, bg, t.track_radius)?;
                let thumb_x = rect.x
                    + bentodesk_widget::toggle::THUMB_INSET_PX
                    + (rect.width
                        - bentodesk_widget::toggle::THUMB_DIAMETER_PX
                        - 2.0 * bentodesk_widget::toggle::THUMB_INSET_PX)
                        * p;
                let thumb_rect = bentodesk_style::Rect {
                    x: thumb_x,
                    y: rect.y + bentodesk_widget::toggle::THUMB_INSET_PX,
                    width: bentodesk_widget::toggle::THUMB_DIAMETER_PX,
                    height: bentodesk_widget::toggle::THUMB_DIAMETER_PX,
                };
                self.fill_rounded_rect(thumb_rect, t.thumb, t.thumb_radius)?;
            }
            WidgetNode::Radio(r) => {
                let selected = r.is_selected();
                let ring = if selected { r.ring_selected } else { r.ring };
                self.fill_rounded_rect(rect, ring, r.radius)?;
                let dot_progress = r.dot_progress();
                if dot_progress > 0.0 {
                    let dot_d = (rect.width * 0.5).max(0.0) * dot_progress;
                    let inset = (rect.width - dot_d) * 0.5;
                    let dot = bentodesk_style::Rect {
                        x: rect.x + inset,
                        y: rect.y + inset,
                        width: dot_d,
                        height: dot_d,
                    };
                    self.fill_rounded_rect(dot, r.dot, r.dot_radius_for_diameter(dot_d))?;
                }
            }
            WidgetNode::Slider(s) => {
                self.fill_rounded_rect(rect, s.track_color, s.track_radius)?;
                let value = (*s.value.get()).clamp(0.0, 1.0);
                let fill_rect = bentodesk_style::Rect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width * value,
                    height: rect.height,
                };
                self.fill_rounded_rect(fill_rect, s.fill_color, s.track_radius)?;
                let thumb_x =
                    rect.x + rect.width * value - bentodesk_widget::slider::THUMB_DIAMETER_PX * 0.5;
                let thumb_y =
                    rect.y + rect.height * 0.5 - bentodesk_widget::slider::THUMB_DIAMETER_PX * 0.5;
                let thumb = bentodesk_style::Rect {
                    x: thumb_x,
                    y: thumb_y,
                    width: bentodesk_widget::slider::THUMB_DIAMETER_PX,
                    height: bentodesk_widget::slider::THUMB_DIAMETER_PX,
                };
                self.fill_rounded_rect(thumb, s.thumb_color, s.thumb_radius)?;
            }
            WidgetNode::Input(i) => {
                let border = if i.focused { i.border_focus } else { i.border };
                self.fill_rounded_rect(rect, border, i.radius)?;
                self.fill_rounded_rect(rect, i.background, i.radius)?;
                let text_str = i.text.get().clone();
                if !text_str.is_empty() {
                    self.draw_text(text_str.as_str(), rect, i.text_color)?;
                } else if !i.placeholder.is_empty() {
                    self.draw_text(i.placeholder.as_str(), rect, i.placeholder_color)?;
                }
            }
            WidgetNode::Dropdown(d) => {
                let border = if d.popup.visible {
                    d.border_focus
                } else {
                    d.border
                };
                self.fill_rounded_rect(rect, border, d.radius)?;
                self.fill_rounded_rect(rect, d.background, d.radius)?;
                if let Some(label) = d.selected_label() {
                    self.draw_text(label, rect, d.text)?;
                }
            }
            WidgetNode::Tab(t) => {
                self.fill_rounded_rect(rect, t.header_color, BorderRadius::ZERO)?;
                let underline_x = rect.x + t.underline_anim.current();
                let underline_w = t.active_underline_width();
                let underline = bentodesk_style::Rect {
                    x: underline_x,
                    y: rect.y + rect.height - bentodesk_widget::tab::UNDERLINE_THICKNESS_PX,
                    width: underline_w,
                    height: bentodesk_widget::tab::UNDERLINE_THICKNESS_PX,
                };
                self.fill_rounded_rect(underline, t.underline_color, t.underline_radius)?;
            }
            WidgetNode::Collapsible(_) => {
                // Header + body are children dispatched by the outer loop;
                // the collapsible itself owns no fill — only the height
                // animation, which the layout engine reads directly.
            }
            WidgetNode::Modal(m) => {
                let alpha = m.fade_progress();
                if alpha > 0.0 {
                    let scrim = bentodesk_style::Color {
                        a: m.scrim.a * alpha,
                        ..m.scrim
                    };
                    self.fill_rounded_rect(rect, scrim, BorderRadius::ZERO)?;
                }
            }
            WidgetNode::Popup(_)
            | WidgetNode::Tooltip(_)
            | WidgetNode::ContextMenu(_)
            | WidgetNode::DragPreview(_) => {
                // Overlay primitives — they live in their own HWNDs (T-011
                // Window factory). The main-window render walk does not
                // paint them; per-window renderers handle their geometry.
            }
            WidgetNode::List(_)
            | WidgetNode::Grid(_)
            | WidgetNode::VirtualList(_)
            | WidgetNode::VirtualGrid(_)
            | WidgetNode::Row(_)
            | WidgetNode::Column(_)
            | WidgetNode::GridLayout(_) => {
                // Pure layout containers — children dispatched by the outer
                // iter loop. No own fill.
            }
            WidgetNode::SvgIcon(s) => {
                self.draw_svg_fit(s.source.as_str(), rect, s.tint, s.size)?;
            }
            WidgetNode::FileIcon(f) => {
                if !f.is_pending() {
                    // PHASE_2: pull bitmap from platform icon cache by
                    // `f.cache_hash`. Until the platform cache lands the
                    // background placeholder is correct.
                }
                if f.background.a > 0.0 {
                    self.fill_rounded_rect(rect, f.background, f.border_radius)?;
                }
            }
        }
        Ok(())
    }
}
