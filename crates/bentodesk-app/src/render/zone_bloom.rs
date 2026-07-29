use super::*;

impl Renderer {
    pub(super) fn draw_stack_bloom_overlay(
        &mut self,
        app: &AppState,
        anim_now_ms: u32,
    ) -> Result<(), RenderError> {
        let palette = app.active_theme_palette();
        let pal = app.active_theme_tauri();
        let zone_chrome =
            zone_surface_geometry::ZoneSurfaceChrome::from_radius(app.active_theme_radius());
        let bloom_allowed = stack_surface_allows_bloom(app);
        // `stack_bloom_anchor` is the sole structural owner. A plain
        // `hovered_zone` is deliberately insufficient: pointer drop creates
        // the model relation first and then explicitly arms this owner, while
        // context-menu stacking stays collapsed until a real hover/click.
        if let Some(anchor_id) = app.stack_bloom_anchor.get().filter(|_| bloom_allowed)
            && let Some(anchor) = app.zones.get(anchor_id)
            && let Some(member_ids) = app.zones.stack_member_ids(anchor.id)
        {
            let frames = if app.stack_bloom_leaving.get()
                && app.stack_bloom_anchor.get() == Some(anchor.id)
            {
                stack_tray::stack_bloom_exit_frames_at(
                    app.viewport,
                    anchor,
                    member_ids.len(),
                    app.stack_bloom_progress.get(),
                )
            } else {
                let reveal_progress = if app.stack_bloom_anchor.get() == Some(anchor.id) {
                    app.stack_bloom_progress.get()
                } else {
                    1.0
                };
                stack_tray::stack_bloom_frames_at(
                    app.viewport,
                    anchor,
                    member_ids.len(),
                    reveal_progress,
                )
            };
            let petal_size = stack_tray::stack_bloom_petal_size(member_ids.len());
            let bloom_interaction = app.stack_bloom_interaction.get();
            let overflow_count = stack_tray::stack_bloom_overflow_count(member_ids.len());
            let member_frame_count = frames.len().saturating_sub(usize::from(overflow_count > 0));
            for (member_id, frame) in member_ids
                .iter()
                .copied()
                .take(member_frame_count)
                .zip(frames.iter().copied().take(member_frame_count))
            {
                let Some(member) = app.zones.get(member_id) else {
                    continue;
                };
                let active = bloom_interaction.active_member == Some(member_id);
                let active_t = if active {
                    stack_bloom_active_transition_t(
                        anim_now_ms,
                        bloom_interaction.active_member_started_ms,
                    )
                } else {
                    0.0
                };
                let active_scale = 1.0 + (STACK_BLOOM_ACTIVE_SCALE - 1.0) * active_t;
                let petal_rect = animator::scale_rect_centered(frame.rect, active_scale);
                if frame.connector.width > 0.5 && frame.connector.height > 0.5 {
                    self.fill_rounded_rect(
                        frame.connector,
                        with_alpha(palette.accent, 0.16 * frame.alpha),
                        zone_chrome.bloom_connector_radius,
                    )?;
                }
                let accent = member
                    .accent_color
                    .as_deref()
                    .and_then(parse_hex_color)
                    .unwrap_or(pal.accent_blue);
                // W14 — do not fake CSS blur with a second opaque
                // offset tile. That hard duplicate is the black slab
                // visible around every Bloom petal; ordinary blurred
                // layers follow the shared W13-B suppression contract.
                if active_t > 0.0 {
                    let (pulse_spread, pulse_alpha) = stack_bloom_active_pulse(
                        anim_now_ms,
                        bloom_interaction.active_member_started_ms,
                        member_ids.len() > 8,
                    );
                    let pulse_rect = bentodesk_style::Rect {
                        x: petal_rect.x - pulse_spread,
                        y: petal_rect.y - pulse_spread,
                        width: petal_rect.width + pulse_spread * 2.0,
                        height: petal_rect.height + pulse_spread * 2.0,
                    };
                    self.fill_rounded_rect(
                        pulse_rect,
                        with_alpha(accent, pulse_alpha * active_t * frame.alpha),
                        BorderRadius::all(16.0 * active_scale + pulse_spread),
                    )?;
                    let ring_spread = 1.5;
                    let ring_rect = bentodesk_style::Rect {
                        x: petal_rect.x - ring_spread,
                        y: petal_rect.y - ring_spread,
                        width: petal_rect.width + ring_spread * 2.0,
                        height: petal_rect.height + ring_spread * 2.0,
                    };
                    self.fill_rounded_rect(
                        ring_rect,
                        with_alpha(accent, active_t * frame.alpha),
                        BorderRadius::all(16.0 * active_scale + ring_spread),
                    )?;
                }
                // Match Tauri's fixed Bloom cards: both the desktop
                // backdrop and the theme tint participate in the same
                // entry/exit opacity. Painting only a translucent tint
                // leaves Explorer labels razor-sharp through the petal;
                // fading only the tint leaves a hard backdrop slab.
                self.fill_frosted_rect_with_group_opacity(
                    petal_rect,
                    pal.surface_expanded,
                    zone_chrome.bloom_petal_radius,
                    frame.alpha,
                )?;
                self.fill_rounded_rect_linear_gradient(
                    petal_rect,
                    with_alpha(bentodesk_style::Color::WHITE, 0.14 * frame.alpha),
                    with_alpha(bentodesk_style::Color::WHITE, 0.04 * frame.alpha),
                    zone_chrome.bloom_petal_radius,
                    stack_capsule_sheen_gradient_props(petal_rect),
                )?;
                self.stroke_rounded_rect(
                    petal_rect,
                    lerp_color(
                        with_alpha(bentodesk_style::Color::WHITE, 0.22 * frame.alpha),
                        with_alpha(accent, frame.alpha),
                        active_t,
                    ),
                    zone_chrome.bloom_border_radius,
                    1.0 + 0.5 * active_t,
                )?;
                let content_scale = frame.scale * active_scale;
                let icon_side = (petal_size.icon_size * content_scale).clamp(
                    18.0,
                    (petal_rect.width.min(petal_rect.height) - 16.0).max(18.0),
                );
                let content = stack_tray::stack_bloom_petal_content_layout(
                    petal_rect,
                    icon_side,
                    content_scale,
                );
                let icon_rect = content.icon_rect;
                let icon_radius = BorderRadius::all(icon_rect.width.min(icon_rect.height) * 0.5);
                self.fill_rounded_rect(
                    icon_rect,
                    with_alpha(accent, (0.78 + 0.22 * active_t) * frame.alpha),
                    icon_radius,
                )?;
                self.stroke_rounded_rect(
                    icon_rect,
                    lerp_color(
                        with_alpha(bentodesk_style::Color::WHITE, 0.14 * frame.alpha),
                        with_alpha(accent, 0.60 * frame.alpha),
                        active_t,
                    ),
                    icon_radius,
                    1.0,
                )?;
                self.draw_icon_glyph(
                    member.icon.as_ref(),
                    centered_square_rect(icon_rect, (icon_side * 0.60).max(12.0)),
                    with_alpha(pal.text_primary, frame.alpha),
                )?;
                self.draw_stack_bloom_petal_name(
                    member.display_title(),
                    content.title_rect,
                    with_alpha(pal.text_primary, 0.92 * frame.alpha),
                )?;
            }
            if overflow_count > 0
                && let Some(frame) = frames.last().copied()
            {
                let overflow_rect = frame.rect;
                self.fill_frosted_rect_with_group_opacity(
                    overflow_rect,
                    pal.surface_zen,
                    zone_chrome.bloom_petal_radius,
                    frame.alpha,
                )?;
                self.fill_rounded_rect_linear_gradient(
                    overflow_rect,
                    with_alpha(bentodesk_style::Color::WHITE, 0.06 * frame.alpha),
                    with_alpha(bentodesk_style::Color::WHITE, 0.02 * frame.alpha),
                    zone_chrome.bloom_petal_radius,
                    stack_capsule_sheen_gradient_props(overflow_rect),
                )?;
                self.stroke_rounded_rect(
                    overflow_rect,
                    with_alpha(bentodesk_style::Color::WHITE, 0.12 * frame.alpha),
                    zone_chrome.bloom_border_radius,
                    1.0,
                )?;
                let count = format_small_count(overflow_count);
                let token_width = 44.0 * frame.scale;
                let plus_width = 14.0 * frame.scale;
                let token_rect = bentodesk_style::Rect {
                    x: overflow_rect.x + (overflow_rect.width - token_width) * 0.5,
                    y: overflow_rect.y,
                    width: token_width,
                    height: overflow_rect.height,
                };
                self.draw_text_no_wrap_with_style(
                    "+",
                    bentodesk_style::Rect {
                        width: plus_width,
                        ..token_rect
                    },
                    with_alpha(pal.text_primary, 0.70 * frame.alpha),
                    18.0 * frame.scale,
                    700,
                    1.2,
                    dwrite::TextAlign {
                        h: dwrite::HAlign::Center,
                        v: dwrite::VAlign::Center,
                    },
                )?;
                self.draw_text_no_wrap_with_style(
                    count.as_str(),
                    bentodesk_style::Rect {
                        x: token_rect.x + plus_width,
                        width: token_rect.width - plus_width,
                        ..token_rect
                    },
                    with_alpha(pal.text_primary, 0.70 * frame.alpha),
                    18.0 * frame.scale,
                    700,
                    1.2,
                    dwrite::TextAlign {
                        h: dwrite::HAlign::Center,
                        v: dwrite::VAlign::Center,
                    },
                )?;
            }
        }
        Ok(())
    }
}
