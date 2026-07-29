use super::*;

/// What the WM_NCHITTEST handler should return for a given client-space
/// point. Pure function so integration tests can exercise the rule
/// without a live HWND.
///
/// Rule (Ruling 3): top `TOOLBAR_HEIGHT` band returns `Caption` (drag),
/// unless the cursor lands on an `IconButton` (still `Client`). Below the
/// band returns `Client`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    Client,
    Caption,
    Transparent,
}

pub fn nchittest_kind(app: &AppState, win: &WindowState, x: f32, y: f32) -> HitKind {
    if y >= TOOLBAR_HEIGHT {
        return HitKind::Client;
    }
    match hit_test(win, x, y) {
        Some(id) if is_icon_button(app, id) => HitKind::Client,
        _ => HitKind::Caption,
    }
}

/// Shared hit-test for focusable D2D/DComp auxiliary panels. Their left header
/// area moves the single native HWND; the right edge stays client space for a
/// painted close control. Keeping this rule independent of widget-tree state
/// prevents a newly-opened panel from reverting to an immovable debug window
/// before its first interactive tree update.
pub fn auxiliary_panel_nchittest_kind(viewport: bentodesk_style::Size, x: f32, y: f32) -> HitKind {
    if x < 0.0 || y < 0.0 || x >= viewport.width || y >= viewport.height {
        return HitKind::Transparent;
    }
    const HEADER_HEIGHT: f32 = 52.0;
    const CLOSE_CONTROL_RESERVE: f32 = 96.0;
    if y < HEADER_HEIGHT && x < (viewport.width - CLOSE_CONTROL_RESERVE).max(0.0) {
        HitKind::Caption
    } else {
        HitKind::Client
    }
}

/// Borderless Bulk Manager hit-test. Its search input intentionally shares the
/// title row, so the generic auxiliary caption rule must exempt both the input
/// and close button. Otherwise Windows consumes a real click as HTCAPTION and
/// the client never receives the event that arms keyboard search.
pub fn bulk_manager_nchittest_kind(viewport: bentodesk_style::Size, x: f32, y: f32) -> HitKind {
    if x < 0.0 || y < 0.0 || x >= viewport.width || y >= viewport.height {
        return HitKind::Transparent;
    }
    if rect_contains(bulk_manager_panel::bulk_manager_search_rect(viewport), x, y)
        || rect_contains(bulk_manager_panel::bulk_manager_close_rect(viewport), x, y)
    {
        return HitKind::Client;
    }
    auxiliary_panel_nchittest_kind(viewport, x, y)
}

/// Borderless Settings hit-test: its painted header is the drag handle while
/// the close button remains a normal client control. The generic auxiliary
/// rule only treats the HWND's top 48 DIPs as a caption, which misses a centred
/// modal whose header starts below the window origin.
pub fn settings_nchittest_kind(viewport: bentodesk_style::Size, x: f32, y: f32) -> HitKind {
    if rect_contains(
        bentodesk_app::settings_panel::settings_close_button_rect_m1(viewport),
        x,
        y,
    ) {
        return HitKind::Client;
    }
    if rect_contains(
        bentodesk_app::settings_panel::settings_header_rect(viewport),
        x,
        y,
    ) {
        return HitKind::Caption;
    }
    HitKind::Client
}

/// Borderless global Search hit-test. Transparent host margin never blocks the
/// desktop; the painted header drags the real native popup, while the close
/// button and body remain client controls.
pub fn search_nchittest_kind(viewport: bentodesk_style::Size, x: f32, y: f32) -> HitKind {
    let panel = bentodesk_app::business::search_bar::search_panel_rect(viewport);
    if !rect_contains(panel, x, y) {
        return HitKind::Transparent;
    }
    if rect_contains(
        bentodesk_app::business::search_bar::search_close_rect(viewport),
        x,
        y,
    ) {
        return HitKind::Client;
    }
    if y < panel.y + bentodesk_app::business::search_bar::RUNTIME_HEADER_HEIGHT_PX {
        return HitKind::Caption;
    }
    HitKind::Client
}

/// Borderless About hit-test. The identity/header area is a native caption
/// drag handle, except for the painted close button which must keep receiving
/// normal client clicks.
pub fn about_nchittest_kind(viewport: bentodesk_style::Size, x: f32, y: f32) -> HitKind {
    let close = bentodesk_app::business::about::close_button_rect(viewport);
    if rect_contains(close, x, y) {
        return HitKind::Client;
    }
    let panel = bentodesk_app::business::about::panel_rect(viewport);
    if rect_contains(panel, x, y) && y <= panel.y + 144.0 {
        return HitKind::Caption;
    }
    HitKind::Client
}

/// Borderless ZoneEditor hit-test. The self-painted header is the native drag
/// handle, while its close button and every form control remain client input.
pub fn zone_editor_nchittest_kind(viewport: bentodesk_style::Size, x: f32, y: f32) -> HitKind {
    if rect_contains(
        bentodesk_app::zone_editor_geometry::zone_editor_close_rect(viewport),
        x,
        y,
    ) {
        return HitKind::Client;
    }
    if rect_contains(
        bentodesk_app::zone_editor_geometry::zone_editor_header_rect(viewport),
        x,
        y,
    ) {
        return HitKind::Caption;
    }
    HitKind::Client
}

/// Main desktop-overlay hit-test.
///
/// Unlike auxiliary dialogs, the main Ghost Layer covers the desktop work
/// area. Blank space must be click-through so Explorer desktop icons keep
/// behaving like the original Tauri stack's `setIgnoreCursorEvents(true)`
/// path. Real BentoDesk surfaces stay `Client`/`Caption`.
pub fn main_nchittest_kind(app: &AppState, win: &WindowState, x: f32, y: f32) -> HitKind {
    if x < 0.0 || y < 0.0 || x >= app.viewport.width || y >= app.viewport.height {
        return HitKind::Transparent;
    }
    if app.zone_drag.get().is_some()
        || app.zone_resize.get().is_some()
        || app.item_drag.borrow().is_some()
        || app.stack_tray_drag.get().is_some()
    {
        return HitKind::Client;
    }
    if stack_overlay_contains(app, x, y) {
        return HitKind::Client;
    }
    if app
        .active_context_menu
        .borrow()
        .as_ref()
        .is_some_and(|session| popover::context_menu_contains(session, x, y))
    {
        return HitKind::Client;
    }
    if hit_test_zone_resize_corner(app, x, y).is_some()
        || hit_test_zone_item(app, x, y).is_some()
        || hit_test_zone(app, x, y).is_some()
    {
        return HitKind::Client;
    }
    match hit_test(win, x, y) {
        Some(id) if is_icon_button(app, id) => HitKind::Client,
        _ => HitKind::Transparent,
    }
}

fn stack_overlay_contains(app: &AppState, x: f32, y: f32) -> bool {
    let stack_surface = app.stack_tray.borrow().clone();
    if let Some(state) = stack_surface.as_ref()
        && let Some(anchor) = app.zones.get(state.anchor_zone_id)
        && let Some(members) = app.zones.stack_member_ids(anchor.id)
    {
        let member_count = members.len();
        if state.is_management() {
            if stack_tray::stack_tray_hit_test(app.viewport, anchor, member_count, x, y).is_some() {
                return true;
            }
            let tray = stack_tray::stack_tray_rect(app.viewport, anchor, member_count);
            let selected_id = if members.contains(&state.selected_member_id) {
                state.selected_member_id
            } else {
                members[0]
            };
            if stack_tray::focused_preview_visible(anchor.id, selected_id)
                && rect_contains(stack_tray::focused_preview_rect(app.viewport, tray), x, y)
            {
                return true;
            }
        } else if let Some(member_index) = members
            .iter()
            .position(|member_id| *member_id == state.selected_member_id)
            && let Some(member) = app.zones.get(state.selected_member_id)
        {
            let petals = stack_tray::stack_bloom_petal_rects(app.viewport, anchor, member_count);
            if let Some(petal) = petals.get(member_index).copied()
                && stack_tray::focused_bloom_preview_contains(
                    app.viewport,
                    petal,
                    &petals,
                    member,
                    x,
                    y,
                )
            {
                return true;
            }
        }
    }

    // Management mode owns the stack surface and suppresses Bloom. A floating
    // petal preview does not: Tauri keeps the petals live while it is open.
    if app.selected_zone.get().is_some()
        || stack_surface
            .as_ref()
            .is_some_and(|state| state.is_management())
    {
        return false;
    }
    let Some(anchor_id) = app.stack_bloom_anchor.get() else {
        return false;
    };
    let Some(anchor) = app.zones.get(anchor_id) else {
        return false;
    };
    let Some(members) = app.zones.stack_member_ids(anchor.id) else {
        return false;
    };
    stack_tray::stack_bloom_hit_test(app.viewport, anchor, members.len(), x, y).is_some()
}
