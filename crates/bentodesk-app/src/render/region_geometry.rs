use super::*;

// M6-UI (2026-05-29) — the Wave J1b `ThemePickerAdapter` (the
// `RendererLike` bridge that forwarded the popup `paint_into` onto the
// renderer) was removed alongside the popup. §3 Appearance now paints inline
// in `draw_settings_panel`'s body closure using the renderer's own
// `fill_rounded_rect` / `stroke_rounded_rect` / `draw_text` directly, so no
// adapter trait object is needed.

/// Phase 2.3.1b — pure-scale 3×2 matrix used as the per-frame base
/// transform. Free function so caller sites avoid an extra `&self` borrow
/// when they only need the matrix value (e.g., between back-to-back SVG
/// transform restores).
#[inline]
pub(super) fn base_scale_matrix(scale: f32) -> windows::Foundation::Numerics::Matrix3x2 {
    windows::Foundation::Numerics::Matrix3x2 {
        M11: scale,
        M12: 0.0,
        M21: 0.0,
        M22: scale,
        M31: 0.0,
        M32: 0.0,
    }
}

/// Mc-2b — pure staleness predicate for the paint-entry generation self-heal.
/// Returns `true` when the renderer's cached device generation no longer
/// matches the platform's current generation, i.e. the device chain was
/// rebuilt (by this or another window's recovery) since this renderer last
/// built its device-derived COM. Free function so the decision is unit-testable
/// without a GPU-backed `Renderer`.
#[inline]
pub(super) fn renderer_is_stale(cached_gen: u64, current_gen: u64) -> bool {
    cached_gen != current_gen
}

/// P0 click-through shadow/glow margin in logical DIP.
///
/// Each chrome rect is inflated by this amount before the window region is
/// built so soft drop-shadows / hover glows are NOT hard-clipped by the OS at
/// the region edge (which would read as a sharp rectangular cut through the
/// shadow). Derived from the dominant painted pill shadow
/// (`SHADOW.zen.outer()`: `offset_y 8 + blur 32`): the visible falloff reaches
/// roughly `offset + blur/2 = 8 + 16 = 24` DIP past the surface. The expanded
/// panel's larger shadow (`offset 16 + blur 48`) extends further but is purely
/// decorative — a faint clip there is acceptable, whereas widening the region
/// to its full 64-DIP reach would re-arm the desktop to catch clicks well
/// outside the visible panel. 24 DIP is the balance: covers the common pill
/// shadow fully, keeps the click-through margin tight.
pub(super) const CHROME_REGION_SHADOW_MARGIN_DIP: f32 = 24.0;

/// P0 click-through (CLICKTHROUGH-FIX-VALIDATED.md, 2026-06-02) — the union of
/// every currently-PAINTED interactive surface on the Main overlay, in logical
/// DIP. This is the single source of truth for the Main HWND window region (see
/// [`Renderer::apply_main_click_through_region`]): blank areas fall OUTSIDE the
/// region so clicks reach the desktop natively, painted chrome stays
/// interactive.
///
/// The set MUST mirror `bentodesk-shell::ui::main_nchittest_kind` (which
/// classifies each client point `Client`/`Caption` vs `Transparent`): every
/// rect here corresponds to a case where that fn returns NON-`Transparent`.
/// Item cards, resize corners, and `PanelHeader` buttons are all geometric
/// SUBSETS of their owning zone's body/pill rect, so unioning the zone rects
/// already covers them — no need to enumerate the sub-rects.
///
/// Each rect is inflated by [`CHROME_REGION_SHADOW_MARGIN_DIP`]. Pure /
/// allocation-lean: one stack `SmallVec`, no heap beyond a spill on a very
/// large zone count. Returns rects in DIP; the caller converts to physical px.
pub(super) fn chrome_region_rects(app: &AppState) -> SmallVec<[bentodesk_style::Rect; 16]> {
    use bentodesk_style::Rect;
    let mut out: SmallVec<[Rect; 16]> = SmallVec::new();
    let vp = app.viewport;
    let full = Rect {
        x: 0.0,
        y: 0.0,
        width: vp.width.max(0.0),
        height: vp.height.max(0.0),
    };

    // Any in-flight drag/resize routes EVERY
    // point to `Client` so the gesture keeps receiving moves even over blank
    // desktop. Cover the full viewport for the duration of the drag.
    if app.zone_drag.get().is_some()
        || app.zone_resize.get().is_some()
        || app.item_drag.borrow().is_some()
        || app.stack_tray_drag.get().is_some()
    {
        push_inflated(&mut out, full, 0.0);
        return out;
    }

    // Stack overlay (open tray + focused preview, or a hovered-anchor bloom).
    // Mirrors `ui::stack_overlay_contains`.
    push_stack_overlay_rects(app, &mut out, full);

    // App-rendered context menu sits above zones on the Main surface. Include
    // its compact bounding box (including the submenu bridge) without the Zone
    // shadow inflation so every visible row receives production pointer input.
    if let Some(session) = app.active_context_menu.borrow().as_ref() {
        push_clamped_inflated(&mut out, popover::context_menu_bounds(session), full, 0.0);
    }

    // Per-zone painted surface — pill / in-flight morph / expanded body.
    // Mirrors `ui::effective_zone_hit_rect` + the `hit_test_zone` visibility
    // filter (skip hidden zones + stacked children).
    for zone in app.zones.iter() {
        if !zone.is_visible() || zone.is_stacked_child() {
            continue;
        }
        let rect = effective_zone_chrome_rect(app, zone);
        // Belt-and-suspenders (ROOT-CAUSE-corrupt-zone-geometry.md): clamp the
        // ZONE BODY rect to the viewport BEFORE inflating so an oversized /
        // corrupt zone can never make the whole window catch clicks. The
        // shadow-margin inflate may then extend slightly past the viewport —
        // that's fine (only the painted soft shadow), but the body itself can
        // never exceed the window region.
        push_clamped_inflated(&mut out, rect, full, CHROME_REGION_SHADOW_MARGIN_DIP);
    }

    out
}

/// Intersect `rect` with `bounds` (both DIP). Returns the overlapping rectangle,
/// or `None` when they do not overlap (or the intersection is degenerate). Pure /
/// allocation-free — the click-through region clamp depends on this so an
/// oversized zone can never push a rect beyond the window.
#[inline]
pub(super) fn intersect_with_viewport(
    rect: bentodesk_style::Rect,
    bounds: bentodesk_style::Rect,
) -> Option<bentodesk_style::Rect> {
    let left = rect.x.max(bounds.x);
    let top = rect.y.max(bounds.y);
    let right = rect.right().min(bounds.right());
    let bottom = rect.bottom().min(bounds.bottom());
    if right <= left || bottom <= top {
        return None;
    }
    Some(bentodesk_style::Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// Clamp `rect` to the viewport `bounds`, THEN inflate by `margin` and push it
/// (mirrors [`push_inflated`] but with the pre-inflate viewport clamp). A rect
/// fully outside the viewport is dropped entirely. The skip-degenerate guard in
/// `push_inflated` still applies because the clamped rect is forwarded through it.
#[inline]
pub(super) fn push_clamped_inflated(
    out: &mut SmallVec<[bentodesk_style::Rect; 16]>,
    rect: bentodesk_style::Rect,
    bounds: bentodesk_style::Rect,
    margin: f32,
) {
    if let Some(clamped) = intersect_with_viewport(rect, bounds) {
        push_inflated(out, clamped, margin);
    }
}

/// Painted chrome rect for one zone — the DIP rectangle the renderer is
/// currently drawing. Re-implements `bentodesk-shell::ui::effective_zone_hit_rect`
/// in the `bentodesk-app` layer (the shell depends on app, not the reverse, so
/// the helper can't be imported; both sides consume the same `zone_pill_geometry`
/// SSoT so they stay in lockstep). Three cases: pill-morph in flight, collapsed
/// pill, expanded body. Pure / allocation-free.
pub(super) fn effective_zone_chrome_rect(app: &AppState, zone: &Zone) -> bentodesk_style::Rect {
    use bentodesk_style::Rect;
    // #4 / R1 (2026-06-02) — a stack anchor's body is visible only when it is
    // explicitly selected (a focused member), NOT on hover (hover shows the
    // bloom). #5 (2026-06-02) — only a RESIZE (armable solely on an already-
    // expanded panel) may force the expanded body; a DRAG keeps a collapsed pill
    // a pill. Both rules now live in the shared `AppState::zone_pill_body_visible`
    // SSoT, the SAME predicate the paint side (`draw_zones`) and the z-layering
    // (`zone_on_top`) key off, so paint == hit geometry can't drift.
    let body_visible = app.zone_pill_body_visible(zone);
    let stack_member_count = app.zones.stack_member_ids(zone.id).map(|m| m.len());
    let count = stack_member_count.unwrap_or_else(|| zone.items.len());
    let pill_layout = zone_pill_geometry::pill_layout_for_zone(zone, count);
    let expanded_rect = Rect {
        x: zone.x as f32,
        y: zone.y as f32,
        width: zone.w as f32,
        height: zone.h as f32,
    };

    // Case 1 — pill morph in flight (mirrors effective_zone_hit_rect case 1).
    // Anchors don't morph (the paint-side pill_anim_active also excludes them).
    // #2 step 8 (2026-06-02) — shared `current_morph_rect` SSoT so paint == hit.
    if app.zone_pill_morph_in_flight(zone) {
        let raw = app.zone_pill_anim_progress.get();
        let (_morph, rect) = zone_pill_geometry::current_morph_rect(
            pill_layout.rect,
            expanded_rect,
            app.zone_pill_anim_from_morph.get(),
            raw,
            app.zone_pill_anim_expanding.get(),
        );
        return rect;
    }

    if !body_visible {
        if let Some(member_count) = stack_member_count {
            return zone_pill_geometry::stack_capsule_layout_for_zone(zone, member_count).rect;
        }
        return pill_layout.rect;
    }

    // Case 3 — expanded body (focused stack member uses the normal panel).
    expanded_rect
}

/// Push the stack-overlay chrome rects (open tray + focused preview, or a
/// hovered-anchor bloom) into `out`. Mirrors `ui::stack_overlay_contains` so
/// the region covers exactly the points that function returns `Client` for.
pub(super) fn push_stack_overlay_rects(
    app: &AppState,
    out: &mut SmallVec<[bentodesk_style::Rect; 16]>,
    full: bentodesk_style::Rect,
) {
    let vp = app.viewport;
    // Open tray — tray body plus the focused preview pane only after a real
    // member is selected; the default anchor management view stays compact.
    if let Some(state) = app.stack_tray.borrow().clone() {
        if let Some(anchor) = app.zones.get(state.anchor_zone_id) {
            if let Some(members) = app.zones.stack_member_ids(anchor.id) {
                let member_count = members.len();
                if state.is_management() {
                    let tray = stack_tray::stack_tray_rect(vp, anchor, member_count);
                    push_clamped_inflated(out, tray, full, CHROME_REGION_SHADOW_MARGIN_DIP);
                    let selected_id = if members.contains(&state.selected_member_id) {
                        state.selected_member_id
                    } else {
                        members[0]
                    };
                    if stack_tray::focused_preview_visible(anchor.id, selected_id) {
                        push_clamped_inflated(
                            out,
                            stack_tray::focused_preview_rect(vp, tray),
                            full,
                            CHROME_REGION_SHADOW_MARGIN_DIP,
                        );
                    }
                } else if let Some(member_index) = members
                    .iter()
                    .position(|member_id| *member_id == state.selected_member_id)
                    && let Some(preview_zone) = app.zones.get(state.selected_member_id)
                {
                    let petals = stack_tray::stack_bloom_petal_rects(vp, anchor, member_count);
                    if let Some(petal) = petals.get(member_index).copied() {
                        push_clamped_inflated(
                            out,
                            stack_tray::focused_bloom_preview_rect(
                                vp,
                                petal,
                                &petals,
                                preview_zone,
                            ),
                            full,
                            CHROME_REGION_SHADOW_MARGIN_DIP,
                        );
                    }
                }
            }
        }
    }

    // Hovered-anchor bloom — the fan of petal rects shown while the cursor is
    // over a stack anchor. #4 / R1 (2026-06-02): mirror the render-side gate so
    // the click-through region never registers petal hit targets on a frame
    // where the bloom is NOT painted (tray open or a member focused/selected) —
    // no invisible dead click targets.
    let bloom_allowed = stack_surface_allows_bloom(app);
    if let Some(anchor_id) = app.stack_bloom_anchor.get().filter(|_| bloom_allowed) {
        if let Some(anchor) = app.zones.get(anchor_id) {
            if let Some(members) = app.zones.stack_member_ids(anchor.id) {
                let petals = if app.stack_bloom_leaving.get()
                    && app.stack_bloom_anchor.get() == Some(anchor.id)
                {
                    stack_tray::stack_bloom_exit_petal_rects_at(
                        vp,
                        anchor,
                        members.len(),
                        app.stack_bloom_progress.get(),
                    )
                } else {
                    stack_tray::stack_bloom_petal_rects(vp, anchor, members.len())
                };
                for petal in petals {
                    push_clamped_inflated(out, petal, full, CHROME_REGION_SHADOW_MARGIN_DIP);
                }
            }
        }
    }
}

/// Inflate `rect` by `margin` DIP on every side and push it onto `out`, skipping
/// degenerate (non-positive area) rects so the region never gains an empty part.
#[inline]
pub(super) fn push_inflated(
    out: &mut SmallVec<[bentodesk_style::Rect; 16]>,
    rect: bentodesk_style::Rect,
    margin: f32,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    out.push(bentodesk_style::Rect {
        x: rect.x - margin,
        y: rect.y - margin,
        width: rect.width + margin * 2.0,
        height: rect.height + margin * 2.0,
    });
}
