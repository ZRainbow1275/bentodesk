use super::*;

impl Renderer {
    /// Draw all zones from `app.zones`. Each zone is a translucent rounded
    /// rectangle with its title at top-left. Zones live in their own
    /// collection (Ruling 2) and rendering walks the list directly — no
    /// widget-tree mount.
    pub(super) fn draw_zones(&mut self, app: &AppState) -> Result<(), RenderError> {
        // V-8 — wall-clock used to sample the pill animator. We read
        // `GetTickCount` once per frame so all pills share the same phase
        // (the breathing dot looks broken if each pill samples a different
        // `now`). Allocation-free per spec §10.
        // SAFETY: `GetTickCount` is total + thread-safe.
        let anim_now_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
        let palette = app.active_theme_palette();
        // M6a — live Tauri-parity palette for this frame. Bound ONCE here and
        // threaded into the pill / morph paint helpers so the whole zone
        // surface re-skins with the active theme (§10: Copy, no re-borrow).
        let pal = app.active_theme_tauri();
        // M6b — active theme's Tauri-parity shadow stacks (Copy, bound once §10).
        // The expanded-panel drop band + the collapsed-pill zen halo both read
        // their per-theme stack from here so e.g. `terminal`'s green glow and
        // the Angular `none` themes' empty stacks paint correctly.
        let shadow_tauri = app.active_theme_shadow_tauri();
        // M6c — active theme's effect channel (Copy, bound once §10). Only
        // `cyberpunk` (Neon) consumes it here, layering an ADDITIVE bloom on
        // top of the M6b box-shadow; every other theme no-ops at the variant
        // match.
        let effect = app.active_theme_effect_tauri();
        let zone_chrome =
            zone_surface_geometry::ZoneSurfaceChrome::from_radius(app.active_theme_radius());
        let item_chrome = item_card::ItemCardChrome::from_tokens(
            palette,
            app.active_theme_radius(),
            pal.surface_subtle,
            // Current Tauri ItemCard.css uses the secondary-text token so the
            // uniform label rail does not compete with the panel title.
            item_label_text_color_for_reference(pal),
            pal.text_primary,
            pal.surface_hover,
            pal.border_hover,
        );
        // P1.3 / P2.1 (2026-06-02 real-blur inversion) — the idle expanded
        // panel tint is `--surface-expanded` rgba(12,12,18,0.82), the token
        // Tauri's `.bento-zone--expanded { background: var(--surface-expanded) }`
        // actually uses (NOT `--surface-dialog`, which is reserved for the
        // Dialog/Settings primitive). The V-9 round-4 ruling that pinned 0.92
        // (`surface_dialog`) PREDATES the real D2D gaussian+saturation backdrop
        // (`screencap::capture_primary_workarea_blurred`): at 0.92α only ~8% of
        // the blur shows through, masking even a correct frost — exactly the
        // "完全不一样" delta. With a real backdrop the 0.82α is correct: it lets
        // the required ~18% wallpaper-bleed through so the blur(24px) saturate(1.7)
        // reads. The palette test (`surface_dialog.a > surface_expanded.a`) still
        // holds. `zone_fill_active` (active-drag accent) stays near-opaque.
        let zone_fill_idle = pal.surface_expanded;
        let zone_fill_active = with_alpha(palette.accent, 0.92);
        let expanded_panel_aux = expanded_panel_aux_chrome(pal);
        let zone_live_folder_fill = expanded_panel_aux.live_folder_fill;
        let zone_live_folder_text = expanded_panel_aux.live_folder_text;
        // #4 (2026-06-02) — the stack-anchor halo / "Stack ×N" badge / "Peek:"
        // row fills were removed (no Tauri reference); their colour bindings
        // (stack_shadow / stack_wrapper_halo / stack_badge_fill / stack_peek_fill)
        // went with them.
        let zone_drop_target_glow = with_alpha(palette.accent_hover, 0.30);
        let drop_preview_fill = with_alpha(palette.accent, 0.20);
        let drop_preview_core = with_alpha(palette.accent_hover, 0.34);
        // The expanded panel and morph endpoint share the authored Tauri
        // surface radius. `RadiusTokens::lg` is the widget scale (8 DIP in the
        // default theme), not `.bento-zone--expanded`'s 16-DIP radius.
        let radius = BorderRadius::all(app.active_theme_radius_tauri().expanded);
        let active_id = app
            .zone_drag
            .get()
            .map(|t| t.0)
            .or_else(|| app.zone_resize.get().map(|t| t.0));
        let zone_search_target = app.zone_search_target.get();
        let zone_search_query = app.search_bar.borrow().query.clone();
        // #5 (2026-06-02) — `active_id` (drag OR resize) drives the active-fill
        // tint / drop-target highlight; it must NOT force the expanded body.
        // A RESIZE can only be armed on an already-expanded panel
        // (`hit_test_zone_resize_corner` gates on `zone_pill_body_visible`),
        // so only a resize may force `pill_body_visible`. A DRAG of a
        // COLLAPSED pill must keep it a pill that follows the cursor (Tauri
        // drags the capsule itself) — forcing the body there made the pill
        // "disappear" into its mostly-empty 480×432 expanded body. The hit /
        // chrome SSoTs (`effective_zone_hit_rect` / `effective_zone_chrome_rect`)
        // already key off `zone_pill_body_visible`, so dropping the drag-force
        // restores paint == hit. The resize-force itself now lives inside the
        // shared `AppState::zone_pill_body_visible` SSoT.
        let item_drag = active_item_drag_visual(app);
        let drag_target_id =
            item_drag.and_then(|drag| hit_test_render_zone(app, drag.last_x, drag.last_y));
        let dragged_item_wide = item_drag
            .and_then(|drag| {
                app.zones
                    .item(drag.zone_id, drag.item_id)
                    .map(|item| item.is_wide)
            })
            .unwrap_or(false);
        // Z-order — three fixed passes. Expanded/morphing zones form the normal
        // TOP layer, collapsed pills are the BOTTOM layer, and the actively
        // moved capsule is painted last. This matches Tauri's `Z_ZONE_DRAG`
        // contract: the complete 70%-opaque source capsule stays above every
        // candidate until mouse-up; stack scoring runs only on release and never
        // changes paint order or adds an early merge ring. With the
        // dense 4×4 grid a panel's 480×432 footprint overlaps the pills of zones
        // a row below it, so a single zone-order pass let a later pill overpaint
        // an earlier expanded panel (the bright count badges bled through the
        // dark frosted surface — a Tauri `.bento-zone--expanded` z-index break).
        // Fix: iterate three fixed passes — no per-frame Vec / heap allocation.
        // `zone_draw_layer` preserves the shared `AppState::zone_on_top` SSoT
        // for idle zones and adds only the transient drag override.
        for draw_layer in [0_u8, 1, 2] {
            for zone in app.zones.iter() {
                if !zone.is_visible() || zone.is_stacked_child() {
                    continue;
                }
                if zone_draw_layer(app, zone) != draw_layer {
                    continue;
                }
                // Wave C (05-20 visual parity) — collapsed pill render path.
                // #4 (2026-06-02): a COLLAPSED stack anchor renders as the compact
                // stack pill too (count badge = member count); every zone whose
                // `body_visible_for_mode` is false renders as a Tauri-style capsule
                // pill at `(zone.x, zone.y)` consuming the Wave B token SSoT in
                // `zone_pill_geometry`.
                //
                // #4 / R1 — a stack anchor's HOVER affordance is the bloom, NOT a
                // panel expand (Tauri `StackWrapper.tsx` has no hover-to-expand
                // state). So an anchor's body is visible only when it is explicitly
                // selected (a focused member) or being dragged — never on mere
                // hover — so the collapsed pill + bloom can co-exist without the
                // panel popping underneath them.
                // (Shared SSoT — `zone_on_top` above keys off the SAME predicate.)
                let pill_body_visible = app.zone_pill_body_visible(zone);
                // Wave G2 — morphing capsule. When the hover transition is
                // in-flight for this zone, paint an intermediate rounded-rect
                // instead of snapping between collapsed pill and expanded body.
                // Stack anchors don't run the pill↔panel morph (they toggle between
                // the compact pill and the focused-member panel without it).
                let pill_anim_active = app.zone_pill_morph_in_flight(zone);
                if pill_anim_active {
                    let count = zone.items.len();
                    let pill_layout = zone_pill_geometry::pill_layout_for_zone(zone, count);
                    let expanded_rect = bentodesk_style::Rect {
                        x: zone.x as f32,
                        y: zone.y as f32,
                        width: zone.w as f32,
                        height: zone.h as f32,
                    };
                    let raw = app.zone_pill_anim_progress.get();
                    // The shared monotonic `current_morph_rect` is the single
                    // structural visual state, so paint == hit
                    // geometry (effective_zone_chrome_rect / effective_zone_hit_rect
                    // call the same helper). `draw_zone_pill_morph` re-derives the
                    // rect from the same `morph` via `morph_pill_to_rect`, so the
                    // returned rect here is discarded but stays bit-identical.
                    let (morph, _morph_rect) = zone_pill_geometry::current_morph_rect(
                        pill_layout.rect,
                        expanded_rect,
                        app.zone_pill_anim_from_morph.get(),
                        raw,
                        app.zone_pill_anim_expanding.get(),
                    );
                    // V21-C9 — still sample the V-8 PillHover channel at the
                    // morph boundary, but keep the collapsed endpoint at the
                    // exact Tauri `surface_zen` token. Tauri has no hover
                    // background rule for `.bento-zone--zen`; hover feedback is
                    // shape/shadow/transform-specific, not a base-fill brighten.
                    let hover_t = {
                        let anim = app.pill_animator.borrow();
                        anim.sample(zone.id, animator::AnimChannel::PillHover, anim_now_ms)
                    };
                    // One visual state: the same morph drives surface geometry,
                    // identity placement, hit/chrome bounds, and expanded-only
                    // content. A second delayed alpha timeline made the shell
                    // arrive before its contents and read as a detached layer.
                    self.draw_zone_pill_morph(
                        app,
                        zone,
                        &pill_layout,
                        expanded_rect,
                        morph,
                        hover_t,
                        pal,
                        &item_chrome,
                        effect,
                    )?;
                    continue;
                }
                if !pill_body_visible {
                    if let Some(member_ids) = app.zones.stack_member_ids(zone.id) {
                        let layout = zone_pill_geometry::stack_capsule_layout_for_zone(
                            zone,
                            member_ids.len(),
                        );
                        let (hover_t, press_t, emerge_progress) = {
                            let anim = app.pill_animator.borrow();
                            (
                                anim.sample(zone.id, animator::AnimChannel::PillHover, anim_now_ms),
                                anim.sample(zone.id, animator::AnimChannel::PillPress, anim_now_ms),
                                1.0 - anim.sample(
                                    zone.id,
                                    animator::AnimChannel::StackEmerge,
                                    anim_now_ms,
                                ),
                            )
                        };
                        self.draw_stack_capsule(
                            app,
                            zone,
                            member_ids.as_slice(),
                            &layout,
                            hover_t,
                            press_t,
                            emerge_progress,
                            pal,
                            shadow_tauri.zen,
                            effect,
                        )?;
                        continue;
                    }
                    let count = collapsed_pill_display_count(app, zone);
                    let layout = zone_pill_geometry::pill_layout_for_zone(zone, count);
                    // V-8 — sample hover / press channels at paint time. The
                    // animator borrow is released before any further mutation
                    // (the pill paint helpers are read-only on app state).
                    let (hover_t, press_t) = {
                        let anim = app.pill_animator.borrow();
                        (
                            anim.sample(zone.id, animator::AnimChannel::PillHover, anim_now_ms),
                            anim.sample(zone.id, animator::AnimChannel::PillPress, anim_now_ms),
                        )
                    };
                    self.draw_zone_pill(
                        zone,
                        &layout,
                        count,
                        hover_t,
                        press_t,
                        zone_drag_visual_opacity(app, zone.id),
                        anim_now_ms,
                        pal,
                        shadow_tauri.zen,
                        effect,
                    )?;
                    continue;
                }
                let rect = bentodesk_style::Rect {
                    x: zone.x as f32,
                    y: zone.y as f32,
                    width: zone.w as f32,
                    height: zone.h as f32,
                };
                // Wave I2 — expanded body chrome (panel shadow / header band /
                // divider / count badge). M2 (05-29): the footer thumbnail strip
                // (E-01) was deleted — Tauri's BentoPanel has no footer node.
                // #4 (2026-06-02) — a focused stack member (incl. the anchor) now
                // renders as the NORMAL expanded panel, so the shadow is no longer
                // suppressed for anchors (the bespoke anchor halo + double-shadow
                // that this guard avoided double-stamping was removed below).
                let expanded_layout = expanded_zone_grid::expanded_zone_layout(zone);
                {
                    // M6b — per-theme `expanded` stack under the panel band so the
                    // expanded surface lifts off the desktop backdrop. `draw_shadow_stack`
                    // grows the panel base rect per layer (the Angular `none` themes
                    // paint nothing here; tinted Rounded themes carry their L2 colour).
                    self.draw_shadow_stack(expanded_layout.panel, shadow_tauri.expanded, radius)?;
                    // M6c — the `cyberpunk` neon `filter: drop-shadow` bloom on the
                    // expanded panel (`.bento-zone-expanded`), ADDITIVE on top of
                    // the M6b box-shadow above and UNDER the surface fill below.
                    if let bentodesk_style::tokens::EffectTauri::Neon(n) = effect {
                        self.draw_neon_glow(expanded_layout.panel, n.expanded, radius)?;
                    }
                }
                // #4 (2026-06-02) — the per-anchor wrapper halo + double drop-shadow
                // were REMOVED: they have no Tauri reference counterpart and were
                // part of the bug-screenshot pile-up. A focused stack member now
                // renders as the NORMAL expanded panel (the `!zone.is_stack_anchor()`
                // shadow above), and a collapsed anchor renders as the compact pill.
                if Some(zone.id) == drag_target_id {
                    let glow_rect = bentodesk_style::Rect {
                        x: rect.x - 3.0,
                        y: rect.y - 3.0,
                        width: rect.width + 6.0,
                        height: rect.height + 6.0,
                    };
                    self.fill_rounded_rect(
                        glow_rect,
                        zone_drop_target_glow,
                        zone_chrome.drop_target_radius,
                    )?;
                }
                let fill = if Some(zone.id) == active_id {
                    zone_fill_active
                } else {
                    zone_fill_idle
                };
                // Frosted-backdrop (2026-06-02 real-blur inversion) — the settled
                // expanded panel surface is real acrylic: [blurred+saturated desktop
                // clipped to the panel rect] + [ONE tint] (`surface_expanded` 82%
                // idle / `accent@0.92` active), matching Tauri's panel chrome over
                // `backdrop-filter: blur(24px) saturate(1.7)`. The idle tint dropped
                // from `surface_dialog` 0.92 → `surface_expanded` 0.82 (P1.3): at
                // 0.92 the blur was masked, the dominant "完全不一样" delta. Frosting
                // under the active-drag accent tint is intentional (Tauri's accent
                // panels also sit over the blur). Degrades to the flat tint when no
                // backdrop. The M6b shadow stack + accent edge are unchanged.
                self.fill_frosted_rect(rect, fill, radius)?;
                // P2.2 — the 1px white-12% panel hairline (Tauri
                // `.bento-zone--expanded { border: 1px solid rgba(255,255,255,0.12) }`
                // = `--border-expanded`) that native never painted. Stroked AFTER the
                // frosted fill and BEFORE the accent top-edge below so the 2px accent
                // bar layers over the hairline (CSS border-top paints over the box
                // border). `stroke_rounded_rect` short-circuits on `color.a <= 0.0`.
                self.stroke_rounded_rect(rect, pal.border_expanded, radius, 1.0)?;
                if let Some(accent) = zone.accent_color.as_deref().and_then(parse_hex_color) {
                    self.draw_expanded_panel_accent_edge(rect, radius, accent)?;
                }
                let body_visible = pill_body_visible;
                self.draw_expanded_panel_header(app, zone, &expanded_layout, pal, 1.0, true)?;
                let zone_search_active = zone_search_target == Some(zone.id);
                let zone_search_reveal = if zone_search_active {
                    app.zone_search_animation_progress_at(anim_now_ms)
                } else {
                    0.0
                };
                if zone_search_active {
                    self.draw_inline_zone_search(app, rect, zone_search_query.as_str())?;
                }
                // V-11 (2026-05-21, round 2): the expanded-zone right-bottom
                // display-mode chip ("Hover"/"Always"/"Click") was deleted.
                // Tauri 1.2.4 baseline never paints a display-mode label on the
                // zone surface — the mode is toggled exclusively through the
                // Settings panel's ZoneDisplay row (SettingsHit::CycleZoneDisplayMode,
                // dispatched at bentodesk-shell/src/main.rs:11465 and :12907).
                // The `ZoneSurfaceChrome::display_chip_radius` token + the
                // `effective_zone_display_mode` accessor on AppState are kept for
                // log/test parity; M4 owns the K1 dead_code sweep for the now-
                // unused chrome field.
                // #4 (2026-06-02) — the "Stack ×N" badge + "Peek: <member>" sub-row
                // were REMOVED. They have no Tauri reference counterpart and were
                // part of the bug-screenshot pile-up. Stack membership is now
                // conveyed by the collapsed pill's count badge; a focused member
                // uses the normal expanded panel.
                if !body_visible {
                    continue;
                }
                // V-9 round 2 (2026-05-21) — expanded-body status dot removed.
                // User flagged it as a stray blue ring above each pill ("4" / "10").
                // Tauri 1.2.4 expanded panel has no top-right indicator; the
                // collapsed pill keeps its Wave H2 dot since that one matches
                // baseline.
                if let Some(path) = zone.live_folder_path.as_deref() {
                    let live_text = live_folder_badge_text(path);
                    // M2③ cascade — live-folder badge sits just below the 48-DIP
                    // header band (was y+34 under the legacy 30-DIP header).
                    let live_rect = bentodesk_style::Rect {
                        x: rect.x + 8.0,
                        y: rect.y + item_grid::ITEM_GRID_TOP_OFFSET_PX + 4.0,
                        width: (rect.width - 16.0).max(0.0),
                        height: 16.0,
                    };
                    self.fill_rounded_rect(
                        live_rect,
                        zone_live_folder_fill,
                        zone_chrome.live_badge_radius,
                    )?;
                    self.draw_text(
                        live_text.as_str(),
                        bentodesk_style::Rect {
                            x: live_rect.x + 6.0,
                            y: live_rect.y + 2.0,
                            width: (live_rect.width - 12.0).max(0.0),
                            height: 12.0,
                        },
                        zone_live_folder_text,
                    )?;
                }
                let item_top_offset = search_bar::ZONE_INLINE_ITEM_OFFSET_Y_PX * zone_search_reveal;
                let item_scroll_max = if zone_search_active {
                    highlight_overlay::item_flow_max_scroll(
                        zone,
                        item_top_offset,
                        zone.items
                            .iter()
                            .filter(|item| {
                                search_bar::zone_item_matches_query(
                                    item.name.as_ref(),
                                    zone_search_query.as_str(),
                                )
                            })
                            .map(|item| item.is_wide),
                    )
                } else {
                    highlight_overlay::item_flow_max_scroll(
                        zone,
                        item_top_offset,
                        zone.items.iter().map(|item| item.is_wide),
                    )
                };
                let item_scroll = app.zone_content_scroll_offset(zone.id).min(item_scroll_max);
                let content_clip = highlight_overlay::item_content_clip_rect(zone, item_top_offset);
                self.push_clip(content_clip)?;
                let content_result = (|| -> Result<(), RenderError> {
                    let item_label_group_px = {
                        let mut label_flow_slot = 0;
                        item_label_group_font_size(zone.items.iter().filter_map(|item| {
                            if zone_search_active
                                && !search_bar::zone_item_matches_query(
                                    item.name.as_ref(),
                                    zone_search_query.as_str(),
                                )
                            {
                                return None;
                            }
                            let card_rect = if zone_search_active {
                                let (card, next_slot) =
                                    highlight_overlay::item_card_rect_for_flow_slot_scrolled(
                                        zone,
                                        label_flow_slot,
                                        item.is_wide,
                                        item_top_offset,
                                        item_scroll,
                                    );
                                label_flow_slot = next_slot;
                                card
                            } else {
                                highlight_overlay::item_card_rect_for_item_scrolled(
                                    zone,
                                    item,
                                    item_scroll,
                                )
                            };
                            (card_rect.width > 0.0).then_some((
                                item_label_visible_name(item.name.as_ref()),
                                (card_rect.width - 8.0).max(0.0),
                            ))
                        }))
                    };
                    let mut search_flow_slot = 0;
                    let mut visible_item_count = 0usize;
                    for item in &zone.items {
                        if zone_search_active
                            && !search_bar::zone_item_matches_query(
                                item.name.as_ref(),
                                zone_search_query.as_str(),
                            )
                        {
                            continue;
                        }
                        visible_item_count += 1;
                        let card_rect = if zone_search_active {
                            let (card, next_slot) =
                                highlight_overlay::item_card_rect_for_flow_slot_scrolled(
                                    zone,
                                    search_flow_slot,
                                    item.is_wide,
                                    item_top_offset,
                                    item_scroll,
                                );
                            search_flow_slot = next_slot;
                            card
                        } else {
                            highlight_overlay::item_card_rect_for_item_scrolled(
                                zone,
                                item,
                                item_scroll,
                            )
                        };
                        if card_rect.width <= 0.0
                            || card_rect.bottom() <= content_clip.y
                            || card_rect.y >= content_clip.bottom()
                        {
                            continue;
                        }
                        let is_dragged_source = item_drag
                            .map(|drag| drag.zone_id == zone.id && drag.item_id == item.id)
                            .unwrap_or(false);
                        let item_fill = if is_dragged_source {
                            item_chrome.drag_source_background
                        } else if item.file_missing {
                            item_chrome.missing_background
                        } else {
                            item_chrome.normal_background
                        };
                        // M3-A2 — sample the live per-item hover/press ramp and compose
                        // the Tauri scale(1.02)/scale(0.97). The dragged source card
                        // never scales (it's the muted placeholder under the ghost),
                        // so it stays at identity. `item_hover` is `Copy` in a `Cell`,
                        // so this is a single read + a few muls per card (§10 hot path).
                        //
                        // M3-A3 — Tauri removes the entire `:hover` rule on a
                        // `aria-disabled` (missing-file) card, and a drag-source card
                        // shows its muted placeholder bg, never the hover chrome. So we
                        // zero `hover_t` for both: only a present, non-dragged card
                        // lifts / lerps its bg-border-shadow.
                        let card_key = (zone.id, item.id);
                        let item_hover = app.item_hover.get();
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
                        // FIX 1 — drop the translateY lift only while the pointer is
                        // actively held (Tauri `:active` scale-only override). On
                        // release the lift returns while the press scale ramps out.
                        let press_held = !is_dragged_source && item_hover.press_held(card_key);
                        self.draw_item_card(
                            item,
                            card_rect,
                            item_fill,
                            &item_chrome,
                            hover_t,
                            press_held,
                            item_scale,
                            item_label_group_px,
                            1.0,
                        )?;
                    }
                    if zone_search_active && visible_item_count == 0 {
                        self.draw_text_no_wrap_with_style(
                            bentodesk_style::t(bentodesk_style::i18n_zh_cn::ids::SEARCH_EMPTY),
                            bentodesk_style::Rect {
                                x: rect.x + expanded_zone_grid::HEADER_INSET_X,
                                y: rect.y + item_grid::ITEM_GRID_TOP_OFFSET_PX + item_top_offset,
                                width: (rect.width - expanded_zone_grid::HEADER_INSET_X * 2.0)
                                    .max(0.0),
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
                    if Some(zone.id) == drag_target_id {
                        if let Some(preview) = drop_preview_rect_for_zone(
                            zone,
                            item_drag,
                            dragged_item_wide,
                            item_scroll,
                            item_top_offset,
                        ) {
                            // Drag preview is a target affordance, not card chrome. Paint it
                            // after resident cards so occupied cells cannot cover the target core,
                            // but before the floating ghost so the dragged item remains topmost.
                            self.fill_rounded_rect(
                                preview,
                                drop_preview_fill,
                                item_chrome.card_radius,
                            )?;
                            let core = inset_rect(preview, 4.0);
                            self.fill_rounded_rect(
                                core,
                                drop_preview_core,
                                zone_chrome.drop_preview_core_radius,
                            )?;
                        }
                    }
                    Ok(())
                })();
                let pop_result = self.pop_clip();
                content_result?;
                pop_result?;

                // Tauri uses a transparent track and a narrow content thumb.
                // Keep it subtle but visible so clipped rows advertise that
                // the panel can be scrolled instead of looking truncated.
                if item_scroll_max > 0.0 && content_clip.height > 8.0 {
                    let track = bentodesk_style::Rect {
                        x: rect.right() - 5.0,
                        y: content_clip.y + 4.0,
                        width: 3.0,
                        height: (content_clip.height - 8.0).max(0.0),
                    };
                    let viewport_ratio =
                        content_clip.height / (content_clip.height + item_scroll_max);
                    let thumb_height = (track.height * viewport_ratio)
                        .clamp(18.0_f32.min(track.height), track.height);
                    let travel = (track.height - thumb_height).max(0.0);
                    let progress = item_scroll / item_scroll_max;
                    self.fill_rounded_rect(
                        bentodesk_style::Rect {
                            x: track.x,
                            y: track.y + travel * progress,
                            width: track.width,
                            height: thumb_height,
                        },
                        with_alpha(pal.text_primary, 0.22),
                        bentodesk_style::BorderRadius::all(2.0),
                    )?;
                }
                // M2 E-01 (2026-05-29) — the 16×16 sub-zone footer thumbnail
                // strip was DELETED. Tauri's `BentoPanel` renders header + grid
                // only with no footer node; the strip was an additive native
                // divergence visible only on stack anchors. Removed for 1:1.
            }
        }
        // Z-order (2026-06-02) — the hover-bloom is drawn AFTER both layers, so
        // it stays above every pill AND every panel. It is a hover affordance on
        // a COLLAPSED stack anchor and is gated to frames where no zone is
        // expanded/selected (so it never co-renders with a panel) — keeping it
        // last preserves the current visual intent (top of the whole zone stack)
        // and matches the hit side, where `push_stack_overlay_rects` is pushed
        // before the per-zone rects so the bloom petals win the hit-test.
        // #4 / R1 (2026-06-02) — the hover-bloom is a real Tauri feature
        // (`StackWrapper.tsx` hover-bloom), so it is GATED, not deleted. It
        // fans out ONLY when (a) the cursor hovers a stack anchor, (b) the
        // explicit management tray is closed (`stack_tray` is None — they are
        // mutually exclusive surfaces), and (c) no zone is expanded/selected
        // (so it can never co-render with an expanded panel). Step 5 separately
        // ensures the bloom trigger only arms for actual stack anchors.
        // `selected_zone.is_none()` means no member is focused (no expanded
        // anchor panel), so the bloom can never co-render with the focused-
        // member panel. The anchor's own hover does NOT expand it (see the
        // `pill_body_visible` anchor rule above), so the collapsed pill + bloom
        // are the only surfaces shown while hovering.
        self.draw_stack_bloom_overlay(app, anim_now_ms)?;
        if let Some(drag) = item_drag {
            if let Some((zone, item)) = source_drag_item(app, drag) {
                let source_rect = item_card_rect_for_item(zone, item);
                let ghost_rect = drag_ghost_rect(app, drag, source_rect);
                let shadow_rect = bentodesk_style::Rect {
                    x: ghost_rect.x + 4.0,
                    y: ghost_rect.y + 6.0,
                    width: ghost_rect.width,
                    height: ghost_rect.height,
                };
                self.fill_rounded_rect(
                    shadow_rect,
                    item_chrome.ghost_shadow,
                    item_chrome.card_radius,
                )?;
                self.draw_item_card(
                    item,
                    ghost_rect,
                    if item.file_missing {
                        item_chrome.missing_background
                    } else {
                        item_chrome.ghost_background
                    },
                    &item_chrome,
                    // M3-A2/A3 — the floating drag ghost is not a hover target;
                    // it keeps identity scale + zero hover_t (no lift / bg-border
                    // -shadow lerp; the ghost has its own lift/shadow treatment)
                    // so hover/press chrome stays on the live grid.
                    0.0,
                    false,
                    1.0,
                    item_label_font_size_for_width(
                        item_label_visible_name(item.name.as_ref()),
                        (ghost_rect.width - 8.0).max(0.0),
                    ),
                    1.0,
                )?;
            }
        }
        // V-11 (2026-05-21): bottom-left `item_operation_status` chip removed.
        // Tauri 1.2.4 baseline never painted a status pill on item open/copy/etc;
        // the `AppState::item_operation_status` cell + `ZoneSurfaceChrome::
        // item_status_radius` token are kept for log/test parity (and a possible
        // future toast surface) but are no longer rendered. M4 owns the dead_code
        // sweep for the now-unused field.
        Ok(())
    }
}
