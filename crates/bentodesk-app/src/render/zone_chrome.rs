use super::*;

impl Renderer {
    /// Paint the stack-specific collapsed capsule from Tauri `StackCapsule.tsx`.
    ///
    /// This is intentionally separate from `draw_zone_pill`: stack anchors have
    /// their own 220x52 grid with overlapped member peeks, a top-member icon
    /// bubble, title, and member-count badge. The anchor zone remains the
    /// command/hit root; the visual top zone follows Tauri's sorted stack order.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_stack_capsule(
        &mut self,
        app: &AppState,
        anchor: &Zone,
        member_ids: &[ZoneId],
        layout: &StackCapsuleLayout,
        hover_t: f32,
        press_t: f32,
        emerge_progress: f32,
        pal: bentodesk_style::tokens::PaletteTauri,
        shadow_zen: bentodesk_style::ShadowStack,
        effect: bentodesk_style::tokens::EffectTauri,
    ) -> Result<(), RenderError> {
        let hover_t = hover_t.clamp(0.0, 1.0);
        let is_locked = stack_capsule_is_locked(app, anchor, member_ids);
        let has_preview = stack_capsule_has_preview(app, anchor.id);
        let bloom = if is_locked {
            stack_capsule_bloom_visual(0.0, member_ids.len(), false)
        } else {
            stack_capsule_bloom_visual_for_app(app, anchor.id, member_ids.len())
        };
        let emerge = stack_capsule_presented_emerge_visual(emerge_progress);
        let visual_scale = bloom.scale * emerge.scale;
        let capsule_opacity = bloom.opacity
            * emerge.opacity
            * stack_capsule_locked_opacity(is_locked)
            * zone_drag_visual_opacity(app, anchor.id);
        let visual_dy = stack_capsule_hover_translate_y(hover_t) * (1.0 - bloom.recede_t);
        let base_rect = translate_rect(layout.rect, 0.0, visual_dy);
        let visual_rect = animator::scale_rect_centered(base_rect, visual_scale);
        let visual_radius = scale_border_radius(layout.radius, visual_scale);
        let child_rect = |rect| {
            scale_rect_about_center(
                translate_rect(rect, 0.0, visual_dy),
                base_rect,
                visual_scale,
            )
        };
        if let bentodesk_style::tokens::EffectTauri::Neon(n) = effect {
            self.draw_neon_glow(visual_rect, n.collapsed, visual_radius)?;
        }
        self.draw_shadow_stack(
            visual_rect,
            scale_shadow_stack(
                fade_shadow_stack(
                    stack_capsule_visual_shadow_stack(
                        shadow_zen,
                        hover_t,
                        bloom.recede_t,
                        has_preview,
                    ),
                    capsule_opacity,
                ),
                visual_scale,
            ),
            visual_radius,
        )?;
        let surface_color = collapsed_zen_surface_color(pal, hover_t);
        self.fill_frosted_rect_with_group_opacity(
            visual_rect,
            surface_color,
            visual_radius,
            capsule_opacity,
        )?;
        let (sheen_start, sheen_end) = stack_capsule_glass_sheen_colors();
        self.fill_rounded_rect_linear_gradient(
            visual_rect,
            fade_color(sheen_start, capsule_opacity),
            fade_color(sheen_end, capsule_opacity),
            visual_radius,
            stack_capsule_sheen_gradient_props(visual_rect),
        )?;
        self.stroke_rounded_rect(
            visual_rect,
            fade_color(
                stack_capsule_bloom_border_color(pal, hover_t, bloom.recede_t),
                capsule_opacity,
            ),
            visual_radius,
            1.0,
        )?;

        let chip_fill = fade_color(with_alpha(pal.text_primary, 0.08), capsule_opacity);
        let chip_border = fade_color(with_alpha(pal.text_primary, 0.10), capsule_opacity);
        let content_color = fade_color(pal.text_primary, capsule_opacity);
        let badge_chrome = stack_capsule_badge_chrome(pal, is_locked);
        let peek_start = member_ids.len().saturating_sub(layout.peek_visible_count);
        let mut slot = 0;
        while slot < layout.peek_visible_count {
            let peek_rect = child_rect(layout.peek_icons[slot]);
            let peek_radius = scale_border_radius(layout.peek_radius, visual_scale);
            self.fill_rounded_rect(peek_rect, chip_fill, peek_radius)?;
            self.stroke_rounded_rect(peek_rect, chip_border, peek_radius, 1.0)?;
            if let Some(member) = member_ids
                .get(peek_start + slot)
                .and_then(|member_id| app.zones.get(*member_id))
            {
                self.draw_icon_glyph(
                    member.icon.as_ref(),
                    centered_square_rect(peek_rect, 12.0 * visual_scale),
                    content_color,
                )?;
            }
            slot += 1;
        }

        let top_zone = member_ids
            .last()
            .and_then(|member_id| app.zones.get(*member_id))
            .unwrap_or(anchor);
        let icon_bubble = child_rect(layout.icon_bubble);
        let icon_glyph = child_rect(layout.icon_glyph);
        let badge = child_rect(layout.badge);
        let mut label_layout = layout.label;
        let preview_label =
            bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::STACK_PREVIEW_ACTIVE);
        let preview_indicator_layout =
            if stack_capsule_show_preview_indicator(has_preview, bloom.recede_t) {
                let width = stack_capsule_preview_indicator_width(preview_label);
                let x = (layout.badge.x - zone_pill_geometry::STACK_CAPSULE_GAP_PX - width)
                    .max(layout.label.x);
                label_layout.width =
                    (x - zone_pill_geometry::STACK_CAPSULE_GAP_PX - label_layout.x).max(0.0);
                Some(Rect {
                    x,
                    y: layout.rect.y
                        + (layout.rect.height - STACK_CAPSULE_PREVIEW_INDICATOR_HEIGHT) * 0.5,
                    width: (layout.badge.x - zone_pill_geometry::STACK_CAPSULE_GAP_PX - x).max(0.0),
                    height: STACK_CAPSULE_PREVIEW_INDICATOR_HEIGHT,
                })
            } else {
                None
            };
        let label_text = translate_rect(label_layout, 0.0, visual_dy);
        let badge_text = translate_rect(layout.badge, 0.0, visual_dy);
        let text_transform =
            stack_capsule_bloom_text_transform(self.base_scale, base_rect, visual_scale);
        self.fill_rounded_rect(
            icon_bubble,
            chip_fill,
            scale_border_radius(layout.icon_radius, visual_scale),
        )?;
        self.draw_icon_glyph(top_zone.icon.as_ref(), icon_glyph, content_color)?;
        self.draw_stack_capsule_title_shrink_to_fit_transformed(
            top_zone.display_title(),
            label_text,
            content_color,
            text_transform,
        )?;
        if let Some(indicator) = preview_indicator_layout {
            let indicator = child_rect(indicator);
            self.fill_rounded_rect(
                indicator,
                fade_color(STACK_CAPSULE_PREVIEW_INDICATOR_FILL, capsule_opacity),
                BorderRadius::all(indicator.height * 0.5),
            )?;
            self.draw_text_no_wrap_with_style_transformed(
                preview_label,
                indicator,
                fade_color(STACK_CAPSULE_PREVIEW_INDICATOR_TEXT, capsule_opacity),
                STACK_CAPSULE_PREVIEW_INDICATOR_FONT_PX,
                STACK_CAPSULE_PREVIEW_INDICATOR_FONT_WEIGHT,
                1.2,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
                text_transform,
            )?;
        }

        self.fill_rounded_rect(
            badge,
            fade_color(badge_chrome.fill, capsule_opacity),
            scale_border_radius(layout.badge_radius, visual_scale),
        )?;
        let count_str = format_small_count(member_ids.len());
        self.draw_text_no_wrap_with_style_transformed(
            count_str.as_str(),
            badge_text,
            fade_color(badge_chrome.text, capsule_opacity),
            zone_pill_geometry::STACK_CAPSULE_BADGE_FONT_PX,
            zone_pill_geometry::STACK_CAPSULE_BADGE_FONT_WEIGHT,
            1.2,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
            text_transform,
        )?;
        let _ = press_t;
        Ok(())
    }

    /// Paint the complete expanded PanelHeader layer at `opacity`.
    ///
    /// The settled panel and the in-flight Bento layer share this exact path so
    /// the final spring frame cannot pop from a title-only proxy to the real
    /// icon/badge/search/close chrome. Tauri keeps the Bento layer mounted and
    /// changes only its opacity; Native mirrors that contract here.
    pub(super) fn draw_expanded_panel_header(
        &mut self,
        app: &AppState,
        zone: &Zone,
        layout: &expanded_zone_grid::ExpandedZoneLayout,
        pal: bentodesk_style::tokens::PaletteTauri,
        opacity: f32,
        draw_identity: bool,
    ) -> Result<(), RenderError> {
        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return Ok(());
        }

        if draw_identity {
            let pill = zone_pill_geometry::pill_layout_for_zone(zone, zone.items.len());
            let identity = morph_zen_content_to_header(pill, layout, 1.0);
            self.draw_zone_pill_content(zone, &identity, zone.items.len(), opacity, pal)?;
        }

        let search_btn = layout.header_search_btn;
        let close_btn = layout.header_close_btn;
        let search_chrome = panel_header_button_chrome(
            pal,
            PanelHeaderButtonKind::Search,
            app.is_panel_header_button_hovered(zone.id, PanelHeaderButtonKind::Search),
        );
        let close_chrome = panel_header_button_chrome(
            pal,
            PanelHeaderButtonKind::Close,
            app.is_panel_header_button_hovered(zone.id, PanelHeaderButtonKind::Close),
        );
        let button_radius =
            bentodesk_style::BorderRadius::all(expanded_zone_grid::HEADER_BTN_RADIUS);
        if let Some(background) = search_chrome.background {
            self.fill_rounded_rect(search_btn, fade_color(background, opacity), button_radius)?;
        }
        if let Some(background) = close_chrome.background {
            self.fill_rounded_rect(close_btn, fade_color(background, opacity), button_radius)?;
        }
        let glyph_size = expanded_zone_grid::HEADER_BTN_GLYPH_SIZE;
        let glyph_inset = |button: bentodesk_style::Rect| bentodesk_style::Rect {
            x: button.x + (button.width - glyph_size) * 0.5,
            y: button.y + (button.height - glyph_size) * 0.5,
            width: glyph_size,
            height: glyph_size,
        };
        self.draw_icon_glyph(
            IconKind::Search.as_str(),
            glyph_inset(search_btn),
            fade_color(search_chrome.glyph, opacity),
        )?;
        self.draw_icon_glyph(
            IconKind::X.as_str(),
            glyph_inset(close_btn),
            fade_color(close_chrome.glyph, opacity),
        )?;
        self.fill_rounded_rect(
            layout.divider,
            with_alpha(bentodesk_style::Color::WHITE, 0.05 * opacity),
            bentodesk_style::BorderRadius::ZERO,
        )?;
        Ok(())
    }

    /// Paint only the collapsed Zen layer's icon/title/count row.
    /// Surface, shadow, and border remain owned by the outer pill/morph paths.
    pub(super) fn draw_zone_pill_content(
        &mut self,
        zone: &Zone,
        layout: &ZonePillLayout,
        display_count: usize,
        opacity: f32,
        pal: bentodesk_style::tokens::PaletteTauri,
    ) -> Result<(), RenderError> {
        use crate::business::zen_capsule::CapsuleSize;

        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return Ok(());
        }
        let size = CapsuleSize::parse(zone.capsule_size.as_ref());
        let display_title = zone.display_title();
        self.draw_icon_glyph(
            zone.icon.as_ref(),
            layout.icon,
            fade_color(pal.text_primary, opacity),
        )?;
        let uses_visible_glyph_content_metrics =
            zone_pill_geometry::pill_uses_visible_glyph_content_metrics(
                size,
                zone.icon.as_ref(),
                display_title,
            );
        let title_font_px = zone_pill_geometry::pill_title_font_px_for_text(
            size,
            uses_visible_glyph_content_metrics,
            display_title,
        );
        let title_tracking_px = zone_pill_geometry::pill_title_tracking_px_for(
            size,
            uses_visible_glyph_content_metrics,
        );
        let title_color = with_alpha(
            pal.text_primary,
            zone_pill_geometry::pill_title_alpha_for(size, uses_visible_glyph_content_metrics),
        );
        let title_rect = bentodesk_style::Rect {
            x: layout.label.x,
            y: layout.rect.y,
            width: layout.label.width,
            height: layout.rect.height,
        };
        self.draw_pill_title_ellipsis(
            display_title,
            title_rect,
            fade_color(title_color, opacity),
            title_font_px,
            title_tracking_px,
        )?;

        let badge_fill = tauri_badge_fill(zone.accent_color.as_deref(), pal.badge_bg);
        self.fill_rounded_rect(
            layout.badge,
            fade_color(badge_fill, opacity),
            layout.badge_radius,
        )?;
        let count_str = format_small_count(display_count);
        let (badge_pad_x, _) = size.badge_padding_xy();
        let badge_text_rect = bentodesk_style::Rect {
            x: layout.badge.x + badge_pad_x,
            // DirectWrite centers the line box, while rounded count badges are
            // judged by the visible digit ink. A half-DIP optical nudge keeps
            // the rasterized numeral centered at both 100% and 150% DPI.
            y: layout.badge.y + 0.5,
            width: (layout.badge.width - badge_pad_x * 2.0).max(0.0),
            height: layout.badge.height,
        };
        self.draw_text_no_wrap_with_style(
            count_str.as_str(),
            badge_text_rect,
            fade_color(pal.text_primary, opacity),
            size.badge_font_px(),
            size.badge_font_weight(),
            // DWrite vertically centres the uniform line box, not the visible
            // digit ink. A 1.4 line box lifts the 10/11-DIP numeral by ~2 DIP
            // inside this fixed badge; a tight line box restores optical centre.
            1.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
        )?;
        Ok(())
    }

    /// Wave C (05-20 visual parity) — collapsed zone pill render path.
    /// Tauri shows ordinary zones as a rounded capsule (icon + name + count
    /// badge) by default. The live `pal: PaletteTauri` is threaded in from
    /// `draw_zones` so the pill re-skins with the active theme. The paint
    /// inputs are genuinely distinct, so the arity is allowed rather than
    /// bundled.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_zone_pill(
        &mut self,
        zone: &Zone,
        layout: &ZonePillLayout,
        display_count: usize,
        hover_t: f32,
        press_t: f32,
        opacity: f32,
        anim_now_ms: u32,
        pal: bentodesk_style::tokens::PaletteTauri,
        shadow_zen: bentodesk_style::ShadowStack,
        effect: bentodesk_style::tokens::EffectTauri,
    ) -> Result<(), RenderError> {
        // M6a — the live theme palette is passed in by `draw_zones` (bound
        // once per frame). Read `pal.X` instead of the static `PALETTE_DARK`
        // so the collapsed pill re-skins with the active theme.
        // Frosted-backdrop (2026-06-01) — `ACRYLIC_FALLBACK` is no longer used
        // here: the collapsed pill's old `ACRYLIC_FALLBACK` + `surface_zen`
        // double layer is replaced by one `fill_frosted_rect` (blur + single
        // tint), so the import is dropped to stay warning-clean.
        use crate::business::zen_capsule::{CapsuleShape, CapsuleSize};
        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return Ok(());
        }
        // G5 (2026-06-01) — resolve the per-zone capsule size + shape once so the
        // chrome / label / badge below can branch on them (Tauri ZenCapsule).
        let size = CapsuleSize::parse(zone.capsule_size.as_ref());
        let shape = CapsuleShape::parse(zone.capsule_shape.as_ref());
        let is_minimal = matches!(shape, CapsuleShape::Minimal);
        // V-8 — compose hover + press into the final scale multiplier and
        // expand the pill rect about its center. Persisted geometry tokens
        // are NEVER mutated (hard constraint) — `scale_rect_centered`
        // returns a fresh `Rect` for paint only.
        //
        // Fix 8 (G5, VERIFIED) — `pill_scale_for` is a no-op: `HOVER_SCALE_DELTA`
        // and `PRESS_SCALE_DELTA` are both 0.0 (V-12 disabled pill scale), so
        // this returns EXACTLY 1.0 for any hover/press and `scaled_rect` ==
        // `layout.rect`. Tauri's ZenCapsule has no scale transform, so this
        // matches; left in place per V-12 (no scale re-enable).
        let scale = animator::pill_scale_for(hover_t, press_t);
        let scaled_rect = animator::scale_rect_centered(layout.rect, scale);
        let scaled_radius = layout.radius;
        // V21-C1 (2026-06-21) — Tauri's collapsed `.bento-zone--zen` carries
        // `box-shadow: var(--shadow-zen)`. Restore that contract through the
        // same feathered, allocation-free ShadowStack painter used by expanded
        // panels, keyed to the active theme rather than the static dark token.
        if is_minimal {
            // G5 (2026-06-01) — `minimal` shape (Tauri BentoZone.css:92-99
            // `.bento-zone--shape-minimal`): TRANSPARENT background, NO
            // backdrop blur, NO shadow/glow, just a 1px DASHED border at
            // rgba(255,255,255,0.2). Skip the acrylic + surface fills + neon
            // glow entirely and stroke a dashed outline instead of the solid
            // `border-zen` hairline. Corner radius is the resolved 8px.
            let dashed_border = Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.2,
            };
            self.stroke_rounded_rect_dashed(
                scaled_rect,
                fade_color(dashed_border, opacity),
                scaled_radius,
                1.0,
            )?;
        } else {
            // M6c — the `cyberpunk` neon `filter: drop-shadow` bloom on the
            // collapsed pill (`.bento-zone`), painted UNDER the glass+surface
            // fill and alongside the restored theme shadow.
            // Paint the exact geometry returned by `pill_layout_for_zone`.
            // The former medium/large-only 4-DIP vertical inset made the
            // settled pill 8 DIP shorter than morph t=0 and visibly swapped
            // the shell at both ends of the transition.
            let chrome_rect = scaled_rect;
            let chrome_radius = ordinary_zone_pill_chrome_radius(chrome_rect, scaled_radius);
            if let bentodesk_style::tokens::EffectTauri::Neon(n) = effect {
                self.draw_neon_glow(
                    chrome_rect,
                    [
                        fade_shadow(n.collapsed[0], opacity),
                        fade_shadow(n.collapsed[1], opacity),
                    ],
                    chrome_radius,
                )?;
            }
            self.draw_shadow_stack(
                chrome_rect,
                fade_shadow_stack(ordinary_zone_pill_shadow_stack(size, shadow_zen), opacity),
                chrome_radius,
            )?;
            // Frosted-backdrop (2026-06-01, real acrylic) — the collapsed pill
            // surface is now [blurred-desktop backdrop clipped to the capsule] +
            // [ONE `surface_zen` tint]. The old double layer (`ACRYLIC_FALLBACK`
            // + `surface_color`) over the SHARP wallpaper read as murk; Tauri
            // paints this same `surface_zen` 55% alpha OVER `blur(20px)`.
            // `fill_frosted_rect` degrades to the single tint when no backdrop.
            // V21-C9 — Tauri's ordinary `.bento-zone--zen` has no hover
            // background rule. Keep the base tint exactly `surface_zen`; hover
            // feedback belongs to stack-specific shadow/transform paths, not a
            // hidden RGB brighten on every ordinary capsule.
            let surface_color = collapsed_zen_surface_color(pal, hover_t);
            self.fill_frosted_rect_with_group_opacity(
                chrome_rect,
                surface_color,
                chrome_radius,
                opacity,
            )?;
            // M2 S2a (2026-05-29) — Tauri's `.zen-capsule` carries a 1px solid
            // `var(--border-zen)` = `rgba(255,255,255,0.1)` outline. native drew
            // no stroke at all; added here so the capsule reads as glass with a
            // hairline edge. Pure-paint via the existing `stroke_rounded_rect`.
            self.stroke_rounded_rect(
                chrome_rect,
                fade_color(pal.border_zen, opacity),
                chrome_radius,
                1.0,
            )?;
        }
        // M2 S2b (2026-05-29) — the under-icon accent stripe was REMOVED.
        // Tauri's collapsed ZenCapsule has no such stripe (the 2px accent
        // border-top belongs to the EXPANDED body only). The zone accent is
        // still consulted below to tint the count badge (Tauri
        // `var(--zone-accent, --badge-bg)`).
        self.draw_zone_pill_content(zone, layout, display_count, opacity, pal)?;
        // V-9 round 3 (2026-05-21) — Wave H2 top-right status dot removed.
        //
        // G5 (2026-06-01), fix 7 — the V-14 HOVER-gated green dot that painted
        // over the badge on hover has ALSO been removed. Tauri's ZenCapsule has
        // NO hover badge change (ZenCapsule.css:10 only transitions
        // `background`); the count badge stays visible on hover. The v1.2.4
        // "reference frames 005-008" the old comment cited do not reproduce in
        // the live v1.3.0 source. No separate always-on status dot is painted
        // here (the geometry contract exposes only icon, title, and badge).
        let _ = (anim_now_ms, press_t, hover_t);
        Ok(())
    }

    /// Wave G2 — paint the in-flight capsule morph. `morph = 0` reproduces
    /// the collapsed pill chrome, `morph = 1` reproduces the expanded zone
    /// surface; values in between paint the lerped rect at lerped corner
    /// radius + lerped fill alpha. Glyph + label + count badge fade in
    /// proportional to `morph` so the transient frame doesn't show truncated
    /// text. Allocation-free hot-path per spec §10.
    ///
    /// Matches the sibling `draw_zone_pill` arity allowance: the inputs
    /// (zone / layout / expanded rect / two independently-eased animation
    /// channels / palette) are all genuinely distinct paint
    /// data. Geometry, tint, border, identity, actions and item cards consume
    /// the same monotonic `morph`; no second structural paint timeline exists.
    // #2 step 7 (2026-06-02) — `hover_t` (the V-8 PillHover channel sample, 0..1)
    // is threaded in so the +8% surface brighten the collapsed pill carries is
    // continuous across the pill→morph hand-off rather than snapping away. The
    // params are independent paint primitives; bundling adds indirection on a
    // hot per-zone call site, so allow the count.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_zone_pill_morph(
        &mut self,
        app: &AppState,
        zone: &Zone,
        pill_layout: &ZonePillLayout,
        expanded_rect: bentodesk_style::Rect,
        morph: f32,
        hover_t: f32,
        pal: bentodesk_style::tokens::PaletteTauri,
        item_chrome: &item_card::ItemCardChrome,
        effect: bentodesk_style::tokens::EffectTauri,
    ) -> Result<(), RenderError> {
        // M6a — live theme palette passed in by `draw_zones` (§10).
        // Frosted-backdrop (2026-06-01) — `ACRYLIC_FALLBACK` dropped from the
        // import: the morph's old `ACRYLIC_FALLBACK` + flat `surface_zen` double
        // layer is replaced by one `fill_frosted_rect` (blur + a single tint
        // lerped zen→dialog), so the token is no longer referenced here.
        use crate::business::zen_capsule::CapsuleSize;
        // Geometry, identity, expanded content and hit bounds consume the same
        // monotonic morph. Keeping the clamp here makes endpoint math explicit
        // and protects against malformed persisted/transient state.
        let morph_clamped = morph.clamp(0.0, 1.0);
        let pill_rect = pill_layout.rect;
        let rect = zone_pill_geometry::morph_pill_to_rect(pill_rect, expanded_rect, morph);
        // Capsule radius → expanded surface radius (RADIUS.expanded = 16 px,
        // matches the legacy zone chrome rounding). M2② — the morph START
        // radius reads the pill layout's OWN per-shape radius
        // (`pill_layout.radius`, resolved from `zone.capsule_shape`) instead of
        // the hardcoded `RADIUS.capsule`, so a rounded/minimal/circle capsule
        // uncurls from the radius it was actually painted at (no radius pop at
        // morph t=0) and stays consistent with the collapsed pill.
        let expanded_radius = app.active_theme_radius_tauri().expanded;
        let radius_px = zone_pill_geometry::morph_pill_radius(
            pill_layout.radius.top_left,
            expanded_radius,
            morph,
        );
        let border_radius = BorderRadius::all(radius_px);
        // M6c — the `cyberpunk` neon bloom during the capsule<->panel morph,
        // painted UNDER the shadow band + surface fill. The glow lerps from the
        // collapsed (`.bento-zone`) layers to the expanded (`.bento-zone-expanded`)
        // layers by the clamped morph fraction so the bloom grows in lockstep
        // with the surface, with no pop at either endpoint (§10: stack-`f32`
        // lerp, 2 grown fills).
        if let bentodesk_style::tokens::EffectTauri::Neon(n) = effect {
            let morph_layers = [
                lerp_neon_layer(n.collapsed[0], n.expanded[0], morph_clamped),
                lerp_neon_layer(n.collapsed[1], n.expanded[1], morph_clamped),
            ];
            self.draw_neon_glow(rect, morph_layers, border_radius)?;
        }
        // Use the same shadow path as both settled endpoints. W13-B suppresses
        // fake blurred geometry there; the former direct fills bypassed that
        // fix and looked like a dark animation plate behind the real Zone.
        let shadows = app.active_theme_shadow_tauri();
        let collapsed_shadow = ordinary_zone_pill_shadow_stack(
            CapsuleSize::parse(zone.capsule_size.as_ref()),
            shadows.zen,
        );
        self.draw_shadow_stack(
            rect,
            lerp_shadow_stack(collapsed_shadow, shadows.expanded, morph_clamped),
            border_radius,
        )?;
        // Frosted-backdrop (2026-06-01) — real-acrylic morph surface: [blurred
        // desktop clipped to the morphing rect] + [ONE tint], replacing the old
        // `ACRYLIC_FALLBACK` + flat `surface_zen` double layer.
        //
        // Cross-fade the real settled endpoint colors along the same morph as
        // geometry. A separate 300ms tint channel was visible as a plate-layer
        // transition after the shell had already changed shape.
        // V21-C9 — the collapsed endpoint is the exact `surface_zen` token
        // even during hover. Tauri animates the background token itself; it
        // does not add an extra hover-brightened endpoint before the morph.
        let surface_zen = collapsed_zen_surface_color(pal, hover_t);
        let morph_tint = lerp_color(surface_zen, pal.surface_expanded, morph_clamped);
        self.fill_frosted_rect(rect, morph_tint, border_radius)?;
        self.stroke_rounded_rect(
            rect,
            lerp_color(pal.border_zen, pal.border_expanded, morph_clamped),
            border_radius,
            1.0,
        )?;
        if let Some(accent) = tauri_zone_accent_color(zone.accent_color.as_deref()) {
            self.draw_expanded_panel_accent_edge(
                rect,
                border_radius,
                with_alpha(accent, accent.a * morph_clamped),
            )?;
        }

        let morph_layout =
            expanded_zone_grid::expanded_zone_layout_for_rect(rect, zone.items.len());
        let live_zen_layout = zone_pill_geometry::pill_content_layout_in_rect(*pill_layout, rect);
        let identity_layout =
            morph_zen_content_to_header(live_zen_layout, &morph_layout, morph_clamped);
        // Tauri's outer `.bento-zone` owns `overflow: hidden`. Keep every child
        // on the same live morph surface too: during collapse the cards reflow
        // faster than their opacity reaches zero, and without this clip they can
        // briefly paint below the already-shrunken shell like a detached layer.
        self.push_clip(rect)?;
        let content_result = (|| -> Result<(), RenderError> {
            // Icon, title and count are one persistent identity row. Only
            // expanded-only actions/cards fade; the identity itself moves from the
            // capsule slots into the final header slots without a duplicate copy.
            self.draw_zone_pill_content(zone, &identity_layout, zone.items.len(), 1.0, pal)?;
            self.draw_expanded_panel_header(app, zone, &morph_layout, pal, morph_clamped, false)?;

            if morph_clamped > 0.0 {
                let item_label_group_px =
                    item_label_group_font_size(zone.items.iter().filter_map(|item| {
                        let card_rect =
                            highlight_overlay::item_card_rect_for_item_in_panel(zone, item, rect);
                        (card_rect.width > 0.0 && card_rect.height > 0.0).then_some((
                            item_label_visible_name(item.name.as_ref()),
                            (card_rect.width - 8.0).max(0.0),
                        ))
                    }));
                for item in &zone.items {
                    let card_rect =
                        highlight_overlay::item_card_rect_for_item_in_panel(zone, item, rect);
                    if card_rect.width <= 0.0 || card_rect.height <= 0.0 {
                        continue;
                    }
                    let item_fill = if item.file_missing {
                        item_chrome.missing_background
                    } else {
                        item_chrome.normal_background
                    };
                    // Tauri keeps BentoPanel mounted while the capsule is
                    // collapsed, so `.item-enter` runs only on the initial DOM
                    // mount. Replaying it on every hover expansion made the
                    // shell arrive first and the cards look like a second layer.
                    // The persistent Bento layer's shared alpha is the complete
                    // per-expand reveal contract.
                    self.draw_item_card(
                        item,
                        card_rect,
                        item_fill,
                        item_chrome,
                        0.0,
                        false,
                        1.0,
                        item_label_group_px,
                        morph_clamped,
                    )?;
                }
            }
            Ok(())
        })();
        let pop_result = self.pop_clip();
        content_result.and(pop_result)
    }
}
