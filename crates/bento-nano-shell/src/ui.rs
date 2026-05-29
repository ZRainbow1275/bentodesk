//! BentoDesk Main HWND UI tree builder.
//!
//! Constructs the production widget tree mounted on the shell's Main
//! `WM_PAINT`. The structure follows the existing Tauri BentoDesk visual
//! language:
//!
//! ```text
//! Root: ContainerNode (TRANSPARENT — paints nothing, desktop shows through)
//! └── Body: ContainerNode (TRANSPARENT, empty — pills + Settings modal
//!     paint themselves directly via the renderer; no inline toolbar /
//!     status chrome lives on the desktop overlay anymore).
//! ```
//!
//! Wave G1 (2026-05-20): the root was previously a `BentoCard` with a
//! dark 78%-alpha background and `Length::Auto`. Layout stretched the root
//! to the full viewport (the Main HWND covers the entire desktop work
//! area), so the renderer painted a dark gray fill over the transparent
//! backbuffer — DComp then composited that "whiteboard" over the desktop
//! wallpaper. PrintWindow proofs masked the bug by compositing over a
//! black backdrop. Switching the root to a `Container` with
//! `Color::TRANSPARENT` makes `fill_rounded_rect` short-circuit (alpha
//! check at the top of the function), so empty regions paint nothing and
//! the desktop shows through — matching the Tauri reference behavior
//! where `<body>` has no background.
//!
//! V-6 Round-2 (2026-05-21) live hand-test regression fix: the legacy
//! 5-IconButton Toolbar + "就绪" status TextNode were still being mounted
//! on the Main tree body and `Renderer::draw_node` was painting them at
//! the top-left of the transparent desktop overlay — the PIN star glyph +
//! SETTINGS gear glyph were leaking through as white SVG outlines and the
//! Chinese "就绪" status text was leaking at the left edge of every frame.
//! The Tauri 1.2.4 baseline never painted a top-left toolbar / status row
//! on the desktop overlay (visual parity Wave G + H confirmed pills +
//! settings modal are the only painted surfaces); the toolbar mount was
//! pre-parity nano scaffolding that survived only because nobody examined
//! the top-left corner during recent visual audits. The toolbar + status
//! mount calls are removed from this builder. The SVG path constants
//! (`PIN_PATH` etc.), event id table (`events::*`), and `nchittest_kind`
//! helpers stay because non-Main HWNDs (auxiliary settings / about /
//! picker windows) still depend on `nchittest_kind` for their own caption
//! drag-handle heuristics, and the keybinding dispatch table
//! (`action:toggle_pin` / `action:open_settings` / …) still references
//! the event id constants via `main.rs::toolbar_action_label`.
//!
//! All strings flow through the `bento-nano-style::i18n` table — no inline
//! literals in user-visible text positions.

use bento_nano_app::{
    AppState, WindowState, zone_pill_geometry,
    business::{item_grid, stack_tray},
};
use bento_nano_layout::Direction;
use bento_nano_style::{BorderRadius, Color, Edges, Length, Rect, Shadow};
use bento_nano_tree::NodeId;
use bento_nano_widget::{ContainerNode, WidgetKind, WidgetNode};
use bento_nano_zone::{Zone, ZoneId, ZoneItemId};

/// Wave C (05-20 visual parity) — effective hit-test rectangle for `zone`.
///
/// V-13 (2026-05-21) — the hit-rect MUST mirror the rect the renderer is
/// currently painting (paint–hit parity to within 1 DIP). Three cases,
/// matching `Renderer::draw_zones` precisely:
///
/// 1. **Pill-morph in-flight** (`zone_pill_anim_zone == this zone` and
///    progress is strictly between 0 and 1, and not a stack anchor) — the
///    renderer paints `morph_pill_to_rect(pill, expanded, eased)` so the
///    hit-rect lerps in lockstep. Without this, a single mouse-move tick
///    after hover starts snapped the hit-rect to the full expanded body
///    while the visual was still a pill — clicks/hover registered in the
///    invisible "phantom" zone box surrounding the pill.
/// 2. **Collapsed pill** (body not visible per mode and not a stack
///    anchor) — pill rect from `zone_pill_geometry` is the only clickable
///    region.
/// 3. **Expanded body** (body visible per mode, OR stack anchor with its
///    own chrome) — full stored `(x, y, w, h)` rectangle is authoritative.
///
/// Pure / allocation-free.
fn effective_zone_hit_rect(app: &AppState, zone: &Zone) -> Rect {
    let body_visible = app.zone_body_visible_for_mode(zone);
    let count = zone.items.len();
    let pill_layout = zone_pill_geometry::pill_layout_for_zone(zone, count);
    let expanded_rect = Rect {
        x: zone.x as f32,
        y: zone.y as f32,
        width: zone.w as f32,
        height: zone.h as f32,
    };

    // V-13 case 1 — pill morph in flight. Mirrors render.rs:1252-1287.
    if app.zone_pill_anim_zone.get() == Some(zone.id) && !zone.is_stack_anchor() {
        let raw = app.zone_pill_anim_progress.get();
        if raw > 0.0 && raw < 1.0 {
            let eased = zone_pill_geometry::ease_out_cubic_progress(raw);
            // `expanding` true → morph from pill (0) → expanded (1); false
            // reverses the morph so the shrink animation tracks correctly.
            let morph = if app.zone_pill_anim_expanding.get() {
                eased
            } else {
                1.0 - eased
            };
            return zone_pill_geometry::morph_pill_to_rect(pill_layout.rect, expanded_rect, morph);
        }
    }

    // V-13 case 2 — collapsed pill (no morph, body hidden).
    if !body_visible && !zone.is_stack_anchor() {
        return pill_layout.rect;
    }

    // V-13 case 3 — expanded body (or stack anchor chrome).
    expanded_rect
}

/// Embedded SVG path data — compile-time constants, no runtime parsing.
/// Each path lives in 24×24 viewbox; the IconButton renders at 28×28 so a
/// 2px padding visually frames the glyph.
// All paths use only `M / L / Z` commands so the hand-rolled
// `bento-nano-platform::svg::build` parser (M/L/H/V/Z subset) accepts them.
// Adding a curve here would make the parser reject the icon — keep the
// glyph polygon-shaped or extend the parser first.
/// Toolbar height in DIPs. WM_NCHITTEST uses this to decide which client
/// rows act as a drag handle (Ruling 3 — top `TOOLBAR_HEIGHT` band returns
/// `HTCAPTION` unless the cursor is on an `IconButton`).
pub const TOOLBAR_HEIGHT: f32 = 36.0;

pub const PIN_PATH: &str = "M12 2 L13 8 L19 8 L14 12 L16 19 L12 16 L8 19 L10 12 L5 8 L11 8 Z";
/// 8-tooth gear — outer star polygon + inner square stand-in for the hub.
/// Approximation good enough for a 24×24 toolbar glyph; arc-based version
/// lands when the parser learns elliptical arcs (PHASE_2).
pub const SETTINGS_PATH: &str = concat!(
    "M12 2 L14 5 L17 4 L17 7 L20 8 L19 11 L22 12 L19 13 L20 16 L17 17 L17 20 ",
    "L14 19 L12 22 L10 19 L7 20 L7 17 L4 16 L5 13 L2 12 L5 11 L4 8 L7 7 L7 4 L10 5 Z ",
    "M9 9 L15 9 L15 15 L9 15 Z"
);
pub const HIDE_PATH: &str = "M3 12 L21 12";
pub const EXIT_PATH: &str = "M5 5 L19 19 M19 5 L5 19";
/// "+" glyph — vertical + horizontal cross. Used by the toolbar's
/// add-zone button (Phase 2 / Ruling 4).
pub const ADD_PATH: &str = "M12 5 L12 19 M5 12 L19 12";

/// Toolbar event ids — match `IconButton::on_click_event`. Zero is reserved
/// as "no event" by `IconButton::click`, so ids start at 1.
pub mod events {
    pub const PIN: u32 = 1;
    pub const SETTINGS: u32 = 2;
    pub const HIDE: u32 = 3;
    pub const EXIT: u32 = 4;
    /// Toolbar "+" — append a new zone at default geometry. Phase 2 / Ruling 4.
    pub const ADD_ZONE: u32 = 5;
}

/// Build the entire Main HWND widget tree onto `app`. Returns the root
/// node id.
///
/// Layout is established here, but values come from one place — the
/// constants below — so adjusting the visual is a single-file edit.
///
/// The root is a fully transparent `Container`. Wave G1 video review of
/// `屏幕录制 2026-05-20 161936.mp4` confirms the Tauri baseline never
/// painted a full-screen scrim — the desktop wallpaper shows through
/// everywhere and only individual zone pills + the Settings modal paint
/// their own translucent dark surfaces on top. DWM Mica was removed in
/// the same wave (it leaked light-theme cream through the alpha=0
/// surface and made the overlay look like a whiteboard).
pub fn mount_main_tree(app: &mut AppState) -> NodeId {
    // 1) Root — fully transparent. `fill_rounded_rect` short-circuits at
    //    alpha=0 (see `Renderer::fill_rounded_rect`), so this layer is a
    //    no-op paint that still acts as the layout anchor for the
    //    toolbar + status children.
    let root_container = ContainerNode {
        direction: Direction::Column,
        width: Length::Auto,
        height: Length::Auto,
        padding: Edges::ZERO,
        background: Color::TRANSPARENT,
        radius: BorderRadius::ZERO,
        shadow: Shadow::NONE,
    };
    let root = app.mount_root(WidgetNode::Container(root_container));

    // 2) Body — empty transparent container.
    //
    // V-6 Round-2 (2026-05-21) — the body used to host a 5-IconButton
    // Toolbar + "就绪" status TextNode. Those children were painted at
    // top-left of the transparent desktop overlay (the PIN star glyph,
    // the SETTINGS gear glyph, and the Chinese status text were visible
    // through every Chrome / File Explorer foreground window during live
    // hand-test on 2026-05-21). The Tauri 1.2.4 baseline never paints a
    // top-left toolbar on the desktop overlay, so the body now stays
    // empty — pills + the Settings modal (Ctrl+,) cover every action the
    // toolbar formerly exposed, and the global hotkey table keeps the
    // event ids reachable. The `body` Container itself is preserved so
    // existing root-then-body tree walks (`AppState::mount_root` returns
    // the root, callers descend via `children`) still encounter the same
    // shape — just without the visible IconButton + Text leaves.
    let body = ContainerNode {
        direction: Direction::Column,
        width: Length::Auto,
        height: Length::Auto,
        padding: Edges::ZERO,
        background: Color::TRANSPARENT,
        radius: BorderRadius::ZERO,
        shadow: Shadow::NONE,
    };
    let _body_id = match app.add_child(root, "body", WidgetNode::Container(body)) {
        Ok(id) => id,
        // Allocation is the only way `append_child` can fail; on OOM we
        // bail with the partially-built tree so paint can still progress.
        Err(_) => return root,
    };

    root
}

/// Hit-test the tree against a viewport-local point. Walks the layout
/// result computed by the most recent `WindowState::run_layout` and returns
/// the topmost node containing the point. Out-of-bounds returns `None`.
pub fn hit_test(win: &WindowState, x: f32, y: f32) -> Option<NodeId> {
    // Iterate in reverse so the topmost (last-drawn) node wins. The layout
    // engine emits in tree-traversal order, so reverse iteration is a
    // close-enough approximation of paint order for the flat layouts we
    // build today.
    let result = win.layout.last_result()?;
    let mut hit: Option<NodeId> = None;
    for (id, rect) in result.iter() {
        if x >= rect.x && x < rect.right() && y >= rect.y && y < rect.bottom() {
            hit = Some(*id);
        }
    }
    hit
}

/// Convenience predicate — true when the node id is an `IconButton`. Used
/// by the wndproc to short-circuit hover / click routing for non-button
/// hits.
pub fn is_icon_button(app: &AppState, id: NodeId) -> bool {
    matches!(
        app.tree.get(id).map(|n| n.kind()),
        Ok(WidgetKind::IconButton)
    )
}

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
    if app.settings_open.get() || app.about_open.get() {
        return HitKind::Client;
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
    if let Some(state) = app.stack_tray.borrow().clone() {
        if let Some(anchor) = app.zones.get(state.anchor_zone_id) {
            if let Some(members) = app.zones.stack_member_ids(anchor.id) {
                let member_count = members.len();
                if stack_tray::stack_tray_hit_test(app.viewport, anchor, member_count, x, y)
                    .is_some()
                {
                    return true;
                }
                let tray = stack_tray::stack_tray_rect(app.viewport, anchor, member_count);
                if rect_contains(stack_tray::focused_preview_rect(app.viewport, tray), x, y) {
                    return true;
                }
            }
        }
    }

    let Some(anchor_id) = app
        .hovered_zone
        .get()
        .and_then(|zone_id| app.zones.stack_anchor_for(zone_id))
    else {
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

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom()
}

// -----------------------------------------------------------------------------
// Phase 2.1 Ruling D — zone hit-testing helpers.
// -----------------------------------------------------------------------------

/// DIP edge length of the bottom-right resize corner box. Spec: 12 DIP square
/// is the canonical "easy to grab without crowding the zone body".
pub const ZONE_RESIZE_CORNER: f32 = 12.0;

/// Topmost (= last drawn = highest z) zone whose body contains `(x, y)`.
/// Reverse iteration so newer zones win over older ones — matches paint
/// order in `Renderer::draw_zones`.
pub fn hit_test_zone(app: &AppState, x: f32, y: f32) -> Option<ZoneId> {
    for z in app.zones.iter().rev() {
        if !z.is_visible() || z.is_stacked_child() {
            continue;
        }
        let rect = effective_zone_hit_rect(app, z);
        if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
            return Some(z.id);
        }
    }
    None
}

/// Topmost item card under `(x, y)`, returning its owning zone, item id,
/// and effective filesystem path. Geometry mirrors `Renderer::draw_zones` so
/// drag-out hit-testing stays aligned with what the user sees.
pub fn hit_test_zone_item(app: &AppState, x: f32, y: f32) -> Option<(ZoneId, ZoneItemId, String)> {
    for z in app.zones.iter().rev() {
        if !z.is_visible() || z.is_stacked_child() {
            continue;
        }
        // Wave C — collapsed pill mode hides the item grid, so items
        // attached to a non-expanded non-anchor zone are not hit-testable.
        // Stack anchors keep the legacy expanded chrome and remain reachable.
        if !app.zone_body_visible_for_mode(z) && !z.is_stack_anchor() {
            continue;
        }
        let zx = z.x as f32;
        let zy = z.y as f32;
        let zr = zx + z.w as f32;
        let zb = zy + z.h as f32;
        if x < zx || x >= zr || y < zy || y >= zb {
            continue;
        }
        let columns = z.grid_columns.max(1) as f32;
        let gap = item_grid::ITEM_GRID_COLUMN_GAP_PX;
        let cell_w = ((z.w as f32 - 16.0) - gap * (columns - 1.0)).max(44.0) / columns;
        for item in z.items.iter().rev() {
            let span = item_grid::column_span_for(item.is_wide) as f32;
            let ix = zx + 8.0 + item.x as f32 * (cell_w + gap);
            let iy = zy
                + 30.0
                + item.y as f32
                    * (item_grid::ITEM_GRID_ROW_HEIGHT_PX + item_grid::ITEM_GRID_ROW_GAP_PX);
            let iw = (cell_w * span + gap * (span - 1.0)).min((zr - 8.0 - ix).max(0.0));
            let ih = item_grid::ITEM_GRID_ROW_HEIGHT_PX.min((zb - 8.0 - iy).max(0.0));
            if iw > 0.0 && ih > 0.0 && x >= ix && x < ix + iw && y >= iy && y < iy + ih {
                return Some((z.id, item.id, item.path.to_string()));
            }
        }
    }
    None
}

/// Grid coordinate under `(x, y)` inside `zone_id`, using the same geometry
/// constants as [`hit_test_zone_item`] and `Renderer::draw_zones`. The shell
/// uses this on item mouse-up so a dragged card can produce a real
/// `Command::MoveItem` instead of only an Explorer drag-out.
pub fn item_grid_position_for_point(
    app: &AppState,
    zone_id: ZoneId,
    x: f32,
    y: f32,
) -> Option<(i32, i32)> {
    let z = app.zones.get(zone_id)?;
    let columns = z.grid_columns.max(1) as i32;
    let gap = item_grid::ITEM_GRID_COLUMN_GAP_PX;
    let columns_f = columns as f32;
    let cell_w = ((z.w as f32 - 16.0) - gap * (columns_f - 1.0)).max(44.0) / columns_f;
    let col_stride = cell_w + gap;
    let row_stride = item_grid::ITEM_GRID_ROW_HEIGHT_PX + item_grid::ITEM_GRID_ROW_GAP_PX;
    let raw_col = ((x - z.x as f32 - 8.0) / col_stride).floor() as i32;
    let raw_row = ((y - z.y as f32 - 30.0) / row_stride).floor() as i32;
    Some((raw_col.clamp(0, columns - 1), raw_row.max(0)))
}

/// Topmost zone whose bottom-right `ZONE_RESIZE_CORNER` square contains
/// `(x, y)`. Distinct from `hit_test_zone`: a click in the corner triggers
/// resize, anywhere else inside the body triggers drag.
pub fn hit_test_zone_resize_corner(app: &AppState, x: f32, y: f32) -> Option<ZoneId> {
    for z in app.zones.iter().rev() {
        if !z.is_visible() || z.is_stacked_child() {
            continue;
        }
        // Wave C — collapsed pills have no resize handle (they auto-size to
        // their badge + label content). Only expanded zones surface the
        // bottom-right resize corner.
        if !app.zone_body_visible_for_mode(z) && !z.is_stack_anchor() {
            continue;
        }
        let zr = (z.x + z.w) as f32;
        let zb = (z.y + z.h) as f32;
        let cx = zr - ZONE_RESIZE_CORNER;
        let cy = zb - ZONE_RESIZE_CORNER;
        if x >= cx && x < zr && y >= cy && y < zb {
            return Some(z.id);
        }
    }
    None
}

// -----------------------------------------------------------------------------
// Phase 2.1 Ruling C — settings panel hit-tester.
//
// Geometry constants live in `bento_nano_app::settings_panel` so the renderer
// and this hit-tester share a single source of truth. We re-export the rect
// helpers here for callers that already pull `ui::*`.
// -----------------------------------------------------------------------------

// Round-2 M1 — only the modal helpers + the J1b active-theme chip rect stay
// in the active import set. The Wave K1 row helpers are orphan-alive in
// `bento-nano-app::settings_panel` but no longer referenced from this hit
// tester; their `pub use` re-exports were dropped here to keep the symbol
// surface trim.
// M1h (2026-05-29) — the plugins modal-rect helpers (`settings_plugin_row_rect`
// / `_toggle_rect` / `_uninstall_rect`, `settings_plugins_close_rect` /
// `_install_rect` / `_modal_rect` / `_refresh_rect`) were dropped from this
// re-export when the Plugins surface moved inline; the inline §11 hit-test uses
// the fully-qualified `bento_nano_app::settings_panel::settings_plugin_*` paths
// (same convention as the Backup §9 hits).
pub use bento_nano_app::settings_panel::{
    SETTINGS_BACKUP_ENTRY_VISIBLE_MAX, SETTINGS_CLOSE_BTN_H, SETTINGS_CLOSE_BTN_W,
    SETTINGS_PANEL_HEIGHT, SETTINGS_PANEL_PADDING, SETTINGS_PANEL_WIDTH, SETTINGS_SWITCH_BTN_H,
    SETTINGS_SWITCH_BTN_W, settings_active_theme_rect, settings_keybinding_record_rect,
    settings_keybinding_reset_rect, settings_keybindings_close_rect,
    settings_keybindings_modal_rect,
};

/// What part of the settings overlay was clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHit {
    /// Locale-switch button row.
    SwitchLocale,
    /// Open the keybindings recorder/reset modal.
    OpenKeybindings,
    /// Close the keybindings recorder/reset modal.
    CloseKeybindings,
    // M1h (2026-05-29) — `OpenPlugins` / `ClosePlugins` / `RefreshPlugins` were
    // removed: the Plugins surface is inline (no modal to open/close) and Tauri
    // has no Refresh affordance in the §11 section (the list refreshes on
    // Settings open). Install / Toggle / Uninstall below stay.
    /// Select a `.bdplugin`/zip archive for install through the selected-stack
    /// safe archive extraction and plugin registry path.
    InstallPlugin,
    /// Toggle the visible plugin row at this row index.
    TogglePlugin(usize),
    /// Uninstall the visible plugin row at this row index.
    UninstallPlugin(usize),
    /// Start recording the visible keybinding row at this row index.
    RecordKeybinding(usize),
    /// Reset the visible keybinding row at this row index.
    ResetKeybinding(usize),
    /// Updater cadence cycle button row.
    CycleUpdateFrequency,
    /// Updater lifecycle check-now button.
    CheckForUpdates,
    /// Updater auto-download toggle row.
    ToggleUpdateAutoDownload,
    /// Updater download/install stateful action button.
    RunUpdateAction,
    /// Updater skip-current-version button.
    SkipCurrentUpdate,
    /// Stealth storage master switch row.
    ToggleStealthEnabled,
    /// Config-vault encryption mode row.
    CycleEncryptionMode,
    /// Theme base accent picker row.
    OpenThemeBasePalette,
    /// Native JSON theme import row.
    ImportTheme,
    /// Full active-theme cycle row.
    CycleActiveTheme,
    /// Process default zone display-mode cycle button.
    CycleZoneDisplayMode,
    /// α4 (Wave I-α, 2026-05-25) — pick a specific zone-display mode
    /// (Hover / Always / Click) from the 3-radio picker that replaces the
    /// orphan cycle button. Dispatches `Command::SetSetting` like the
    /// cycle button used to, but with the explicit chosen mode wire string
    /// instead of `zone_display_mode.next()`.
    SetZoneDisplayMode(bento_nano_app::ZoneDisplayMode),
    /// Create a config-vault backup now.
    CreateSettingsBackup,
    /// List real config-vault backup files.
    ListSettingsBackups,
    /// Restore the newest real config-vault backup.
    RestoreLatestSettingsBackup,
    /// Restore the visible backup entry at this newest-first list index.
    RestoreSettingsBackup(usize),
    /// Capture a synchronized recovery bundle for the current layout.
    CreateRecoveryBundle,
    /// Export a validated recovery diagnostics report.
    ExportRecoveryDiagnostics,
    /// Restore the current layout from the latest recovery bundle.
    RestoreRecoveryBundle,
    /// Bottom close button.
    Close,
    /// Anywhere inside the panel chrome but not on a button — eat the click.
    Body,
    /// Outside the panel rect — dismiss the panel (Ruling C: click-outside).
    Outside,
    /// Wave J1b — theme picker swatch popup thumbnail (`0..PRESET_COUNT`).
    PickerThumbnail(u8),
    /// Wave J1b — theme picker accent-row dot/label.
    PickerAccent,
    /// Wave J1b — theme picker Reset footer button.
    PickerReset,
    /// Wave J1b — theme picker Save footer button.
    PickerSave,
    /// Wave J1b — click landed outside the open picker → close it; eat the
    /// click so it does not bubble onto an underlying settings chip.
    ClosePicker,
    // ------------------------------------------------------------------
    // Round-2 M1 — dark Settings shell hits. New variants only; the K1
    // variants above stay orphan-alive (Ruling B) so the existing dispatch
    // arms in `bento-nano-shell::main` continue to link.
    // ------------------------------------------------------------------
    /// Top-section row 0: 桌面嵌入设 (desktop embed).
    ToggleDesktopEmbed,
    /// Top-section row 1: 开机启动 (run at startup).
    ToggleAutostart,
    /// Top-section row 2: 显示在任务栏 (show in taskbar).
    ToggleShowInTaskbar,
    /// Top-section row 3: 智能自动布局 (smart auto-layout).
    ToggleSmartLayout,
    /// Top-section row 4: 便携模式 (portable mode — restart required).
    /// M1a 2026-05-29: renamed from `ToggleSpeedMode` to reach Tauri 1:1
    /// parity with `SettingsPanel.tsx:294` (bound field `portable_mode`).
    TogglePortableMode,
    /// Open the locale chooser (currently flips locale; M5 promotes this to
    /// a popup once the dropdown menu lands). Distinct from `SwitchLocale`
    /// only by intent — they dispatch identically in M1.
    OpenLocaleMenu,
    /// Sticky-footer Cancel button — discards in-memory changes and closes.
    CancelSettings,
    /// Sticky-footer Save button — closes (real persistence wires in a
    /// later wave).
    SaveSettings,
    /// Wheel/keyboard scroll delta against the body (positive = scroll down
    /// in DIPs). The wheel handler dispatches this so the wheel-routing path
    /// stays single-purpose; mouse hit-test never emits it directly.
    ScrollBodyDelta(i32),
    /// M1i 2026-05-29 — 桌面源 §2 refresh (`↻`) button: re-run
    /// `desktop_sources::all_desktop_dirs` and repopulate the cached
    /// `AppState::desktop_sources` read-only list, then redraw. Replaces the
    /// two per-card cosmetic enable toggles (`ToggleSourcePrimary` /
    /// `ToggleSourcePublic`), which were removed as a deliberate Tauri-parity
    /// change — the Tauri `desktop-source-card` has no toggle, only a 已监视
    /// badge.
    RefreshDesktopSources,
    /// Round-2 M2 — 桌面路径 input click. M2 logs + redraws; live keyboard
    /// editing lands in a later wave.
    EditDesktopPath,
    /// Round-2 M2 — 监控值 textarea click. Same M2-stub story as
    /// `EditDesktopPath`; keyboard binding wires in a later wave.
    EditWatchValues,

    // ------------------------------------------------------------------
    // M1d 2026-05-29 — Performance §5 + Startup management §6. These
    // replace the deleted bespoke 高级 / 未来集成验证 hits. Every variant
    // mutates real AppState, is Save-gated, and is reverted by Cancel.
    // ------------------------------------------------------------------
    /// M1d — Performance slider drag. `index` selects the slider
    /// (0=expand_delay, 1=collapse_delay, 2=icon_cache) and the quantized
    /// client `x` lets the dispatcher map track-x→stepped value. Quantizing
    /// to i32 keeps `SettingsHit` `Eq` derivable.
    DragPerformanceSlider { index: u8, x_q: i32 },
    /// M1d — Startup §6 toggle: 高优先级启动 (`startup_high_priority`).
    ToggleStartupHighPriority,
    /// M1d — Startup §6 toggle: 崩溃自动重启 (`crash_restart_enabled`).
    /// Gates the two crash steppers below.
    ToggleCrashRestart,
    /// M1d — Startup §6 stepper `+`: 最大重试次数 (`crash_max_retries`).
    IncCrashMaxRetries,
    /// M1d — Startup §6 stepper `−`: 最大重试次数 (`crash_max_retries`).
    DecCrashMaxRetries,
    /// M1d — Startup §6 stepper `+`: 崩溃窗口（秒）(`crash_window_secs`).
    IncCrashWindowSecs,
    /// M1d — Startup §6 stepper `−`: 崩溃窗口（秒）(`crash_window_secs`).
    DecCrashWindowSecs,
    /// M1d — Startup §6 toggle: 休眠安全恢复
    /// (`safe_start_after_hibernation`). Gates the hibernate slider below.
    ToggleSafeStartHibernation,
    /// M1d — Startup §6 hibernate slider drag (恢复延迟 ms). Carries the
    /// quantized client `x` for the dispatcher's track-x→value map.
    DragHibernateDelay(i32),

    // ------------------------------------------------------------------
    // M1e 2026-05-29 — Stealth §7 card (`StealthModeCard.tsx`). Both
    // variants dispatch to a REAL `bento_nano_backend::stealth` call (no
    // no-op arms): Refresh re-reads `stealth::status()`; Reapply builds a
    // `StealthConfig` + calls `reapply_hidden_on_startup`.
    // ------------------------------------------------------------------
    /// M1e — Stealth §7 Refresh button: re-read `stealth::status()` into the
    /// cached `app.stealth_status` snapshot and redraw.
    RefreshStealth,
    /// M1e — Stealth §7 Reapply button (重新应用): build the live
    /// `StealthConfig` and call `stealth::reapply_hidden_on_startup`, then
    /// refresh the cached status.
    ReapplyStealth,
}

/// Resolve a click point against the settings overlay layout.
///
/// Round-2 M1 — the panel is now the dark shell; only the top 5 toggle rows
/// + language row + sticky footer + close × hit-test. Wave K1 row helpers
/// stay live as orphans so the existing match arms in `main.rs` still link
/// (Ruling B), but no rect emits them in M1.
pub fn settings_hit(app: &AppState, x: f32, y: f32) -> SettingsHit {
    let vp = app.viewport;
    if app.settings_keybindings_open.get() {
        let modal = settings_keybindings_modal_rect(vp);
        if x >= modal.x && x < modal.right() && y >= modal.y && y < modal.bottom() {
            let close = settings_keybindings_close_rect(vp);
            if x >= close.x && x < close.right() && y >= close.y && y < close.bottom() {
                return SettingsHit::CloseKeybindings;
            }
            for row_index in
                0..bento_nano_app::business::settings::keybindings_section::keybinding_rows().len()
            {
                let record = settings_keybinding_record_rect(vp, row_index);
                if x >= record.x && x < record.right() && y >= record.y && y < record.bottom() {
                    return SettingsHit::RecordKeybinding(row_index);
                }
                let reset = settings_keybinding_reset_rect(vp, row_index);
                if x >= reset.x && x < reset.right() && y >= reset.y && y < reset.bottom() {
                    return SettingsHit::ResetKeybinding(row_index);
                }
            }
        }
        return SettingsHit::Body;
    }
    // M1h (2026-05-29) — the plugins MODAL hit block was removed: the Plugins
    // surface is now an always-inline §11 section hit-tested at the body level
    // (Install / per-card Toggle / per-card Uninstall) near the end of this
    // function, after the Backup §9 hits. `settings_plugins_open` +
    // `OpenPlugins` / `ClosePlugins` / `RefreshPlugins` were deleted.
    // Wave J1b — theme picker swatch popup. When open the picker layer sits
    // above every Row 5+ chip in the Settings panel, so it must hit-test
    // first; otherwise clicks on the popup would punch through to the
    // active-theme chip / vault chips beneath it.
    if app.theme_picker_open.get() {
        let chip = settings_active_theme_rect(vp);
        let origin = bento_nano_app::theme_picker::theme_picker_popup_origin(chip);
        let layout = bento_nano_app::theme_picker::theme_picker_layout(origin, vp);
        match bento_nano_app::theme_picker::hit_test(&layout, x, y) {
            Some(bento_nano_app::theme_picker::ThemePickerHit::Thumbnail(i)) => {
                return SettingsHit::PickerThumbnail(i);
            }
            Some(bento_nano_app::theme_picker::ThemePickerHit::Accent) => {
                return SettingsHit::PickerAccent;
            }
            Some(bento_nano_app::theme_picker::ThemePickerHit::Reset) => {
                return SettingsHit::PickerReset;
            }
            Some(bento_nano_app::theme_picker::ThemePickerHit::Save) => {
                return SettingsHit::PickerSave;
            }
            None => {
                // Outside the popup → close it and eat the click so the
                // user does not also trigger the underlying chip in the
                // same gesture (matches the Tauri 1.2.4 baseline behavior).
                return SettingsHit::ClosePicker;
            }
        }
    }
    // Round-2 M1 — dark shell hit routing.
    let panel = bento_nano_app::settings_panel::settings_panel_rect_m1(vp);
    if x < panel.x || x >= panel.right() || y < panel.y || y >= panel.bottom() {
        return SettingsHit::Outside;
    }
    // Header close × — sticky, hit-tested before any scrolled content.
    let close_m1 = bento_nano_app::settings_panel::settings_close_button_rect_m1(vp);
    if x >= close_m1.x && x < close_m1.right() && y >= close_m1.y && y < close_m1.bottom() {
        return SettingsHit::Close;
    }
    // Footer Cancel + Save — sticky.
    let cancel_btn = bento_nano_app::settings_panel::settings_cancel_button_rect(vp);
    if x >= cancel_btn.x
        && x < cancel_btn.right()
        && y >= cancel_btn.y
        && y < cancel_btn.bottom()
    {
        return SettingsHit::CancelSettings;
    }
    let save_btn = bento_nano_app::settings_panel::settings_save_button_rect(vp);
    if x >= save_btn.x && x < save_btn.right() && y >= save_btn.y && y < save_btn.bottom() {
        return SettingsHit::SaveSettings;
    }
    // Body scroll area — anything outside the body rect (i.e. inside footer
    // padding) eats as `Body` so phantom drags do not leak to underlying
    // surfaces.
    let body = bento_nano_app::settings_panel::settings_body_rect(vp);
    if x < body.x || x >= body.right() || y < body.y || y >= body.bottom() {
        return SettingsHit::Body;
    }
    let scroll_y = app.scroll_offset_y.get();
    // Top 5 toggles — index 0..=4.
    for index in 0..bento_nano_app::settings_panel::SETTINGS_TOP_TOGGLE_COUNT {
        let hit = bento_nano_app::settings_panel::settings_top_toggle_hit_rect(vp, scroll_y, index);
        if x >= hit.x && x < hit.right() && y >= hit.y && y < hit.bottom() {
            return match index {
                0 => SettingsHit::ToggleDesktopEmbed,
                1 => SettingsHit::ToggleAutostart,
                2 => SettingsHit::ToggleShowInTaskbar,
                3 => SettingsHit::ToggleSmartLayout,
                4 => SettingsHit::TogglePortableMode,
                _ => SettingsHit::Body,
            };
        }
    }
    // Language dropdown chip.
    let lang_chip = bento_nano_app::settings_panel::settings_language_chip_rect(vp, scroll_y);
    if x >= lang_chip.x && x < lang_chip.right() && y >= lang_chip.y && y < lang_chip.bottom() {
        return SettingsHit::OpenLocaleMenu;
    }
    // α4 (Wave I-α, 2026-05-25) — zone-display-mode picker radios. Three
    // right-anchored radio hit-boxes sit on the row directly below the
    // language chip; each radio dispatches `SetZoneDisplayMode(mode)`.
    // Index → mode mapping mirrors the renderer's `modes` array.
    for index in 0..bento_nano_app::settings_panel::SETTINGS_ZONE_DISPLAY_MODE_COUNT {
        let hit = bento_nano_app::settings_panel::settings_zone_display_mode_radio_rect(
            vp, scroll_y, index,
        );
        if x >= hit.x && x < hit.right() && y >= hit.y && y < hit.bottom() {
            let mode = match index {
                0 => bento_nano_app::ZoneDisplayMode::Hover,
                1 => bento_nano_app::ZoneDisplayMode::Always,
                2 => bento_nano_app::ZoneDisplayMode::Click,
                _ => return SettingsHit::Body,
            };
            return SettingsHit::SetZoneDisplayMode(mode);
        }
    }
    // M1i fidelity — the §2 source list reflows to the LIVE source count;
    // the hit geometry must read the same count the renderer paints.
    let source_count = app.desktop_sources.borrow().len();
    // M1i fidelity — 桌面源 §2 refresh (`↻`) button. Now the LAST child of the
    // list, right-anchored BELOW the live card stack (not on the heading row).
    // Click re-resolves the desktop sources and repopulates the read-only list.
    // The source cards themselves are display-only (no per-card hit-box).
    let refresh = bento_nano_app::settings_panel::settings_sources_refresh_button_rect(
        vp, scroll_y, source_count,
    );
    if x >= refresh.x && x < refresh.right() && y >= refresh.y && y < refresh.bottom() {
        return SettingsHit::RefreshDesktopSources;
    }
    // Round-2 M2 — 桌面路径 input box (reflows below the live source stack).
    let path_box =
        bento_nano_app::settings_panel::settings_desktop_path_input_rect(vp, scroll_y, source_count);
    if x >= path_box.x && x < path_box.right() && y >= path_box.y && y < path_box.bottom() {
        return SettingsHit::EditDesktopPath;
    }
    // Round-2 M2 — 监控值 textarea (reflows below the live source stack).
    let watch_box =
        bento_nano_app::settings_panel::settings_watch_textarea_rect(vp, scroll_y, source_count);
    if x >= watch_box.x && x < watch_box.right() && y >= watch_box.y && y < watch_box.bottom() {
        return SettingsHit::EditWatchValues;
    }
    // M1i fidelity — single-base-offset reflow (mirrors the renderer's `scroll`
    // shadow in `render.rs`). Everything from Performance §5 downward roots at
    // the fixed 4-card source reserve; fold the live reserve delta into
    // `scroll_y` so the hit geometry shifts UP by the height of the missing
    // source cards in lockstep with what is painted.
    let scroll_y =
        scroll_y + bento_nano_app::settings_panel::settings_sources_reserve_delta(source_count);
    // M1d — Performance §5: 3 SliderRows (no conditionals). The slider track
    // band sits on the lower line of each row; a click anywhere on it starts
    // a drag carrying the quantized client x for the dispatcher's
    // track-x→value map.
    for index in 0..bento_nano_app::settings_panel::SETTINGS_PERF_ROW_COUNT {
        let track = bento_nano_app::settings_panel::settings_performance_slider_rect(
            vp, scroll_y, index,
        );
        if x >= track.x && x < track.right() && y >= track.y && y < track.bottom() {
            return SettingsHit::DragPerformanceSlider {
                index,
                x_q: x.round() as i32,
            };
        }
    }
    // M1d — Startup §6. Two always-on toggles, two conditional steppers
    // (crash_restart), one always-on toggle, one conditional slider
    // (hibernation). The two gating bools are read from AppState so the
    // hit-test geometry matches whatever rows are currently painted.
    let crash_restart_on = app.crash_restart_enabled.get();
    let safe_start_on = app.safe_start_after_hibernation.get();
    // 高优先级启动 toggle (row 0).
    let high_row =
        bento_nano_app::settings_panel::settings_startup_high_priority_row_rect(vp, scroll_y);
    let high_hit = bento_nano_app::settings_panel::settings_startup_toggle_hit_rect(high_row);
    if x >= high_hit.x && x < high_hit.right() && y >= high_hit.y && y < high_hit.bottom() {
        return SettingsHit::ToggleStartupHighPriority;
    }
    // 崩溃自动重启 toggle (row 1).
    let crash_row =
        bento_nano_app::settings_panel::settings_crash_restart_row_rect(vp, scroll_y);
    let crash_hit = bento_nano_app::settings_panel::settings_startup_toggle_hit_rect(crash_row);
    if x >= crash_hit.x && x < crash_hit.right() && y >= crash_hit.y && y < crash_hit.bottom() {
        return SettingsHit::ToggleCrashRestart;
    }
    // Crash steppers (rows 2/3) — only when crash_restart_on.
    if crash_restart_on {
        let retries_row =
            bento_nano_app::settings_panel::settings_crash_max_retries_row_rect(vp, scroll_y);
        let r_plus = bento_nano_app::settings_panel::settings_stepper_plus_rect(retries_row);
        if x >= r_plus.x && x < r_plus.right() && y >= r_plus.y && y < r_plus.bottom() {
            return SettingsHit::IncCrashMaxRetries;
        }
        let r_minus = bento_nano_app::settings_panel::settings_stepper_minus_rect(retries_row);
        if x >= r_minus.x && x < r_minus.right() && y >= r_minus.y && y < r_minus.bottom() {
            return SettingsHit::DecCrashMaxRetries;
        }
        let window_row =
            bento_nano_app::settings_panel::settings_crash_window_row_rect(vp, scroll_y);
        let w_plus = bento_nano_app::settings_panel::settings_stepper_plus_rect(window_row);
        if x >= w_plus.x && x < w_plus.right() && y >= w_plus.y && y < w_plus.bottom() {
            return SettingsHit::IncCrashWindowSecs;
        }
        let w_minus = bento_nano_app::settings_panel::settings_stepper_minus_rect(window_row);
        if x >= w_minus.x && x < w_minus.right() && y >= w_minus.y && y < w_minus.bottom() {
            return SettingsHit::DecCrashWindowSecs;
        }
    }
    // 休眠安全恢复 toggle (row 4) — Y depends on crash_restart_on.
    let safe_row = bento_nano_app::settings_panel::settings_safe_start_row_rect(
        vp,
        scroll_y,
        crash_restart_on,
    );
    let safe_hit = bento_nano_app::settings_panel::settings_startup_toggle_hit_rect(safe_row);
    if x >= safe_hit.x && x < safe_hit.right() && y >= safe_hit.y && y < safe_hit.bottom() {
        return SettingsHit::ToggleSafeStartHibernation;
    }
    // 恢复延迟 hibernate slider (row 5) — only when safe_start_on.
    if safe_start_on {
        let track = bento_nano_app::settings_panel::settings_hibernate_slider_rect(
            vp,
            scroll_y,
            crash_restart_on,
        );
        if x >= track.x && x < track.right() && y >= track.y && y < track.bottom() {
            return SettingsHit::DragHibernateDelay(x.round() as i32);
        }
    }

    // M1e — Stealth §7 buttons ([Refresh][Reapply]). The buttons-row Y depends
    // on the conditional retry/error rows above it, so read the same cached
    // `stealth_status` snapshot the renderer paints from (so paint geometry
    // and hit geometry agree). Only the two buttons are interactive — the
    // status/value rows and the OneDrive text block are non-interactive.
    let (stealth_has_retry, stealth_has_error) = match &*app.stealth_status.borrow() {
        Some(s) => (s.retry_count > 0, s.last_error.is_some()),
        None => (false, false),
    };
    let stealth_btn_row = bento_nano_app::settings_panel::settings_stealth_buttons_row_rect(
        vp,
        scroll_y,
        crash_restart_on,
        safe_start_on,
        stealth_has_retry,
        stealth_has_error,
    );
    let refresh_btn =
        bento_nano_app::settings_panel::settings_stealth_refresh_button_rect(stealth_btn_row);
    if x >= refresh_btn.x && x < refresh_btn.right() && y >= refresh_btn.y && y < refresh_btn.bottom()
    {
        return SettingsHit::RefreshStealth;
    }
    let reapply_btn =
        bento_nano_app::settings_panel::settings_stealth_reapply_button_rect(stealth_btn_row);
    if x >= reapply_btn.x && x < reapply_btn.right() && y >= reapply_btn.y && y < reapply_btn.bottom()
    {
        return SettingsHit::ReapplyStealth;
    }

    // M1f — Updater §8 actions/prefs. The card's row Ys depend on the
    // Startup+Stealth gating flags AND the live updater status family (which
    // drives the version/progress/error middle-block height), so build the
    // same `SettingsBodyFlags` the renderer paints from (so paint geometry and
    // hit geometry agree). Interactive: 3 action buttons + frequency chip +
    // auto-download toggle. The status/version/progress/error blocks are
    // non-interactive. Action-button column indices match the renderer: col 0
    // = 检查更新 (always), col 1 = 下载/安装并重启 (gated), col 2 = 跳过此版本 (gated).
    let updater_status = app.settings_updater_status.borrow();
    let updater_kind = bento_nano_app::business::settings::updater_card::updater_height_kind(
        &updater_status,
    );
    let updater_flags = bento_nano_app::settings_panel::SettingsBodyFlags::new(
        crash_restart_on,
        safe_start_on,
        stealth_has_retry,
        stealth_has_error,
        updater_kind,
    );
    let upd_btn_row = bento_nano_app::settings_panel::settings_updater_buttons_row_rect(
        vp,
        scroll_y,
        &updater_flags,
    );
    // Col 0 — 检查更新 (always).
    let check_btn = bento_nano_app::settings_panel::settings_updater_button_rect(upd_btn_row, 0);
    if x >= check_btn.x && x < check_btn.right() && y >= check_btn.y && y < check_btn.bottom() {
        return SettingsHit::CheckForUpdates;
    }
    // Col 1 — 下载 (Available) or 安装并重启 (Ready) → RunUpdateAction.
    if bento_nano_app::business::settings::updater_card::updater_show_download(&updater_status)
        || bento_nano_app::business::settings::updater_card::updater_show_install(&updater_status)
    {
        let action_btn =
            bento_nano_app::settings_panel::settings_updater_button_rect(upd_btn_row, 1);
        if x >= action_btn.x && x < action_btn.right() && y >= action_btn.y && y < action_btn.bottom()
        {
            return SettingsHit::RunUpdateAction;
        }
    }
    // Col 2 — 跳过此版本 (Available/Ready) → SkipCurrentUpdate.
    if bento_nano_app::business::settings::updater_card::updater_show_skip(&updater_status) {
        let skip_btn =
            bento_nano_app::settings_panel::settings_updater_button_rect(upd_btn_row, 2);
        if x >= skip_btn.x && x < skip_btn.right() && y >= skip_btn.y && y < skip_btn.bottom() {
            return SettingsHit::SkipCurrentUpdate;
        }
    }
    // 检查频率 cycling chip → CycleUpdateFrequency.
    let upd_freq_row = bento_nano_app::settings_panel::settings_updater_frequency_row_rect(
        vp,
        scroll_y,
        &updater_flags,
    );
    let freq_chip =
        bento_nano_app::settings_panel::settings_updater_frequency_chip_rect(upd_freq_row);
    if x >= freq_chip.x && x < freq_chip.right() && y >= freq_chip.y && y < freq_chip.bottom() {
        return SettingsHit::CycleUpdateFrequency;
    }
    // 后台静默下载 toggle → ToggleUpdateAutoDownload.
    let upd_auto_row = bento_nano_app::settings_panel::settings_updater_auto_download_row_rect(
        vp,
        scroll_y,
        &updater_flags,
    );
    let auto_hit =
        bento_nano_app::settings_panel::settings_updater_auto_download_hit_rect(upd_auto_row);
    if x >= auto_hit.x && x < auto_hit.right() && y >= auto_hit.y && y < auto_hit.bottom() {
        return SettingsHit::ToggleUpdateAutoDownload;
    }

    // M1g — Backup §9 buttons. The card's row Ys depend on the same
    // Startup+Stealth+Updater flags as the renderer PLUS the variable backup
    // row count (capped), so build the same `SettingsBodyFlags` the renderer
    // paints from via `with_backup_rows` (so paint geometry and hit geometry
    // agree). Interactive: 立即备份 (always) + 刷新 (always) + per-row 恢复
    // (one per visible entry). The title/description/status/empty rows are
    // non-interactive. The per-row 恢复 carries the newest-first list index;
    // the dispatch arm maps index → entry → backup_id.
    let backup_entries = app.settings_backup_entries.borrow();
    let backup_visible = bento_nano_app::business::settings::backup_card::backup_visible_row_count(
        &backup_entries,
    );
    let backup_flags = updater_flags.with_backup_rows(backup_visible);
    let backup_actions = bento_nano_app::settings_panel::settings_backup_actions_row_rect(
        vp,
        scroll_y,
        &backup_flags,
    );
    let create_btn =
        bento_nano_app::settings_panel::settings_backup_create_button_rect(backup_actions);
    if x >= create_btn.x && x < create_btn.right() && y >= create_btn.y && y < create_btn.bottom() {
        return SettingsHit::CreateSettingsBackup;
    }
    let refresh_btn =
        bento_nano_app::settings_panel::settings_backup_refresh_button_rect(backup_actions);
    if x >= refresh_btn.x && x < refresh_btn.right() && y >= refresh_btn.y && y < refresh_btn.bottom()
    {
        return SettingsHit::ListSettingsBackups;
    }
    // Per-row 恢复 buttons — only the visible (non-empty, capped) entries.
    if !bento_nano_app::business::settings::backup_card::backup_list_is_empty(&backup_entries) {
        for entry_index in 0..backup_visible {
            let entry_row = bento_nano_app::settings_panel::settings_backup_entry_row_rect(
                vp,
                scroll_y,
                &backup_flags,
                entry_index,
            );
            let restore_btn =
                bento_nano_app::settings_panel::settings_backup_restore_button_rect(entry_row);
            if x >= restore_btn.x
                && x < restore_btn.right()
                && y >= restore_btn.y
                && y < restore_btn.bottom()
            {
                return SettingsHit::RestoreSettingsBackup(entry_index);
            }
        }
    }

    // M1h — Plugins §11 inline section. The card Ys depend on the same
    // Startup+Stealth+Updater+Backup flags as the renderer PLUS the variable
    // plugin row count (capped), so build the same `SettingsBodyFlags` the
    // renderer paints from via `with_plugin_rows` (paint geometry == hit
    // geometry). Interactive: 安装插件... (always) + per-card enable toggle +
    // per-card 卸载. The title/author/desc/empty rows are non-interactive. The
    // per-card toggle/uninstall carry the list index; the dispatch arms map
    // index → entry → plugin id (and toggle flips the current enabled state).
    let plugin_entries = app.settings_plugin_entries.borrow();
    let plugin_visible =
        bento_nano_app::business::settings::plugins_section::plugin_visible_row_count(
            &plugin_entries,
        );
    let plugin_flags = backup_flags.with_plugin_rows(plugin_visible);
    let plugin_install = bento_nano_app::settings_panel::settings_plugins_install_button_rect(
        vp,
        scroll_y,
        &plugin_flags,
    );
    if x >= plugin_install.x
        && x < plugin_install.right()
        && y >= plugin_install.y
        && y < plugin_install.bottom()
    {
        return SettingsHit::InstallPlugin;
    }
    // Per-card enable toggle + 卸载 — only the visible (non-empty, capped) cards.
    if !bento_nano_app::business::settings::plugins_section::plugin_list_is_empty(&plugin_entries) {
        for card_index in 0..plugin_visible {
            let card = bento_nano_app::settings_panel::settings_plugin_card_rect(
                vp,
                scroll_y,
                &plugin_flags,
                card_index,
            );
            let toggle = bento_nano_app::settings_panel::settings_plugin_toggle_hit_rect(card);
            if x >= toggle.x && x < toggle.right() && y >= toggle.y && y < toggle.bottom() {
                return SettingsHit::TogglePlugin(card_index);
            }
            let uninstall =
                bento_nano_app::settings_panel::settings_plugin_uninstall_button_rect(card);
            if x >= uninstall.x
                && x < uninstall.right()
                && y >= uninstall.y
                && y < uninstall.bottom()
            {
                return SettingsHit::UninstallPlugin(card_index);
            }
        }
    }

    SettingsHit::Body
}

#[cfg(test)]
mod phase21_tests {
    use super::*;
    use bento_nano_app::{AppState, SettingsPluginEntry};
    use bento_nano_style::Size;
    use bento_nano_zone::Zone;
    use std::borrow::Cow;

    fn app_with_zones(zs: Vec<Zone>) -> AppState {
        let mut app = AppState::new();
        // Round-2 M1 — Settings panel needs a viewport tall enough to host
        // header (48) + body (≥ 6 rows * 44 = 264) + footer (56) + top margin
        // (16 * 2 = 32). The Wave K1 baseline used 480×320 which collapses the
        // M1 body so the bottom toggle hits fall outside `body_rect` and
        // settings_hit returns Body. 800×600 matches the production panel
        // dimensions and Tauri reference frames.
        app.viewport = Size {
            width: 800.0,
            height: 600.0,
        };
        for z in zs {
            app.zones.add(z);
        }
        app
    }

    fn app_and_window_with_minibar(zs: Vec<Zone>) -> (AppState, WindowState) {
        let mut app = app_with_zones(zs);
        let mut win = WindowState::new();
        let _ = mount_main_tree(&mut app);
        win.run_layout(&app).expect("main tree layout");
        (app, win)
    }

    // V-6 Round-2 (2026-05-21) — `toolbar_point_for_event` helper retired
    // alongside `mount_main_tree`'s toolbar removal. No Main-HWND
    // IconButton exists to look up anymore.

    #[test]
    fn main_nchittest_empty_desktop_space_is_transparent() {
        let (app, win) = app_and_window_with_minibar(Vec::new());

        assert_eq!(
            main_nchittest_kind(&app, &win, 420.0, 280.0),
            HitKind::Transparent
        );
    }

    // V-6 Round-2 (2026-05-21) — `main_nchittest_keeps_toolbar_buttons_clickable`
    // retired. `mount_main_tree` no longer attaches IconButtons to the
    // Main HWND tree (the legacy toolbar painted at top-left of the
    // transparent desktop overlay — pre-parity scaffolding removed). The
    // remaining hit-test surface that needs to stay clickable on the Main
    // HWND is zones / settings modal / about modal — those continue to be
    // covered by `main_nchittest_keeps_real_zone_surfaces_clickable` +
    // `main_nchittest_keeps_modal_overlay_dismissal_clickable` below.
    #[test]
    fn _retired_main_nchittest_keeps_toolbar_buttons_clickable_v6_r2() {}

    #[test]
    fn main_nchittest_keeps_real_zone_surfaces_clickable() {
        let zone = Zone::new(ZoneId(21), Cow::Borrowed("zone"), 120, 120, 180, 120);
        let (app, win) = app_and_window_with_minibar(vec![zone]);

        assert_eq!(
            main_nchittest_kind(&app, &win, 140.0, 150.0),
            HitKind::Client
        );
        assert_eq!(
            main_nchittest_kind(&app, &win, 420.0, 280.0),
            HitKind::Transparent
        );
    }

    #[test]
    fn main_nchittest_keeps_modal_overlay_dismissal_clickable() {
        let (app, win) = app_and_window_with_minibar(Vec::new());
        app.settings_open.set(true);

        assert_eq!(main_nchittest_kind(&app, &win, 0.0, 0.0), HitKind::Client);
    }

    #[test]
    fn hit_test_zone_returns_topmost_when_overlapping() {
        let app = app_with_zones(vec![
            Zone::new(ZoneId(1), Cow::Borrowed("a"), 0, 0, 100, 100),
            Zone::new(ZoneId(2), Cow::Borrowed("b"), 50, 50, 100, 100),
        ]);
        // Inside the overlap region — id 2 wins (drawn last).
        assert_eq!(hit_test_zone(&app, 75.0, 75.0), Some(ZoneId(2)));
        // Only id 1 covers (10, 10).
        assert_eq!(hit_test_zone(&app, 10.0, 10.0), Some(ZoneId(1)));
        // Empty space.
        assert_eq!(hit_test_zone(&app, 400.0, 300.0), None);
    }

    #[test]
    fn hit_test_zone_skips_hidden_zones() {
        let mut hidden = Zone::new(ZoneId(2), Cow::Borrowed("hidden"), 50, 50, 100, 100);
        hidden.set_visible(false);
        let app = app_with_zones(vec![
            Zone::new(ZoneId(1), Cow::Borrowed("visible"), 0, 0, 100, 100),
            hidden,
        ]);

        // Wave C — visible zone collapses to its pill rect (≤96×36 at the
        // zone origin); the resize corner is only surfaced for expanded
        // zones, so the legacy `(145, 145)` corner is no longer a hit.
        assert_eq!(hit_test_zone(&app, 20.0, 20.0), Some(ZoneId(1)));
        assert_eq!(hit_test_zone_resize_corner(&app, 145.0, 145.0), None);
    }

    #[test]
    fn hit_test_zone_skips_stacked_children() {
        let mut app = app_with_zones(vec![
            Zone::new(ZoneId(1), Cow::Borrowed("anchor"), 0, 0, 150, 150),
            Zone::new(ZoneId(2), Cow::Borrowed("child"), 50, 50, 150, 150),
        ]);
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));

        assert_eq!(hit_test_zone(&app, 75.0, 75.0), Some(ZoneId(1)));
        assert_eq!(hit_test_zone_resize_corner(&app, 195.0, 195.0), None);
    }

    #[test]
    fn hit_test_zone_item_skips_hidden_zone_items() {
        let mut hidden = Zone::new(ZoneId(8), Cow::Borrowed("hidden"), 10, 10, 240, 180);
        hidden.set_visible(false);
        hidden
            .add_item(
                Cow::Owned("C:/Users/HP/Desktop/Hidden.lnk".to_owned()),
                Cow::Borrowed("hash"),
            )
            .expect("item id");
        let app = app_with_zones(vec![hidden]);

        assert_eq!(hit_test_zone_item(&app, 24.0, 48.0), None);
    }

    #[test]
    fn hit_test_zone_item_returns_item_under_visible_card() {
        let mut zone = Zone::new(ZoneId(1), Cow::Borrowed("z"), 10, 10, 240, 180);
        let item_id = zone
            .add_item(
                Cow::Owned("C:/Users/HP/Desktop/App.lnk".to_owned()),
                Cow::Borrowed("hash"),
            )
            .expect("item id");
        let app = app_with_zones(vec![zone]);
        // Wave C — items are reachable only when the zone is expanded.
        app.set_zone_display_mode(bento_nano_app::ZoneDisplayMode::Always);

        let hit = hit_test_zone_item(&app, 24.0, 48.0).expect("item hit");
        assert_eq!(hit.0, ZoneId(1));
        assert_eq!(hit.1, item_id);
        assert_eq!(hit.2, "C:/Users/HP/Desktop/App.lnk");
        assert_eq!(hit_test_zone_item(&app, 400.0, 300.0), None);
    }

    #[test]
    fn hit_test_zone_item_uses_zone_grid_columns() {
        let mut zone = Zone::new(ZoneId(3), Cow::Borrowed("z"), 10, 10, 240, 180);
        zone.set_grid_columns(2);
        let _first = zone
            .add_item(
                Cow::Owned("C:/Users/HP/Desktop/Left.lnk".to_owned()),
                Cow::Borrowed("left"),
            )
            .expect("first item");
        let second = zone
            .add_item(
                Cow::Owned("C:/Users/HP/Desktop/Right.lnk".to_owned()),
                Cow::Borrowed("right"),
            )
            .expect("second item");
        let app = app_with_zones(vec![zone]);
        app.set_zone_display_mode(bento_nano_app::ZoneDisplayMode::Always);

        let hit = hit_test_zone_item(&app, 150.0, 48.0).expect("right-column item hit");
        assert_eq!(hit.0, ZoneId(3));
        assert_eq!(hit.1, second);
        assert_eq!(hit.2, "C:/Users/HP/Desktop/Right.lnk");
    }

    #[test]
    fn hit_test_zone_resize_corner_only_in_bottom_right_box() {
        let app = app_with_zones(vec![Zone::new(
            ZoneId(7),
            Cow::Borrowed("z"),
            100,
            100,
            200,
            100,
        )]);
        // Wave C — resize corner only exists on expanded zones.
        app.set_zone_display_mode(bento_nano_app::ZoneDisplayMode::Always);
        // Inside body but outside corner.
        assert_eq!(hit_test_zone_resize_corner(&app, 150.0, 150.0), None);
        // Inside the 12×12 corner box (right=300, bottom=200).
        assert_eq!(
            hit_test_zone_resize_corner(&app, 295.0, 195.0),
            Some(ZoneId(7))
        );
        // Edge boundary excluded (`<` not `<=`).
        assert_eq!(hit_test_zone_resize_corner(&app, 300.0, 200.0), None);
    }

    // Wave C (05-20 visual parity) — pill hit-test + DPI-rect tests.

    #[test]
    fn hit_test_zone_uses_pill_rect_when_collapsed() {
        let zone = Zone::new(ZoneId(42), Cow::Borrowed("Docs"), 100, 100, 240, 180);
        let app = app_with_zones(vec![zone]);
        // Default ZoneDisplayMode::Hover, no hover/select → collapsed pill.
        let layout = bento_nano_app::zone_pill_geometry::pill_layout_for_zone(
            app.zones.get(ZoneId(42)).expect("zone"),
            0,
        );
        let inside_x = layout.rect.x + layout.rect.width * 0.5;
        let inside_y = layout.rect.y + layout.rect.height * 0.5;
        assert_eq!(hit_test_zone(&app, inside_x, inside_y), Some(ZoneId(42)));
        // Beyond the pill but within the legacy 240×180 rect → no hit.
        assert_eq!(hit_test_zone(&app, 100.0 + 180.0, 100.0 + 100.0), None);
    }

    #[test]
    fn hit_test_zone_uses_full_rect_when_expanded() {
        let zone = Zone::new(ZoneId(43), Cow::Borrowed("Docs"), 100, 100, 240, 180);
        let app = app_with_zones(vec![zone]);
        app.set_zone_display_mode(bento_nano_app::ZoneDisplayMode::Always);
        // Far corner of expanded rect is now reachable.
        assert_eq!(hit_test_zone(&app, 100.0 + 200.0, 100.0 + 150.0), Some(ZoneId(43)));
    }

    // V-13 (2026-05-21) — during the pill→expanded morph the hit-rect MUST
    // mirror the painted rect, not snap to the full expanded zone box.
    //
    // Real flow on first hover: tick N enters pill (hovered=None, anim=None,
    // pill hit-rect). On the same tick, `update_zone_pill_hover` sets
    // `zone_pill_anim_zone = Some(zone), expanding = true, progress = 0.0`.
    // Tick N+1 (a few ms later) has progress ~0.05 — the renderer paints
    // `morph_pill_to_rect(pill, expanded, eased(0.05))`, basically still
    // pill-sized. Pre-fix, `effective_zone_hit_rect` saw `body_visible=true`
    // (because `hovered_zone == zone.id`) and returned the FULL 240×180
    // box, so clicks/hover triggered in the invisible "phantom" rectangle
    // around the visible pill. Post-fix, case 1 fires and the hit-rect
    // tracks the morphed rect within 1 DIP.
    #[test]
    fn hit_test_zone_morph_just_started_uses_pill_sized_rect() {
        let zone = Zone::new(ZoneId(44), Cow::Borrowed("Docs"), 100, 100, 240, 180);
        let app = app_with_zones(vec![zone]);
        // Tick N+1 state: hovered_zone set, morph kicked off with tiny progress.
        app.hovered_zone.set(Some(ZoneId(44)));
        app.zone_pill_anim_zone.set(Some(ZoneId(44)));
        app.zone_pill_anim_expanding.set(true);
        app.zone_pill_anim_progress.set(0.05);
        // Cursor far outside the pill but inside the legacy 240×180 box:
        // must NOT hit. (Pre-fix this returned Some(ZoneId(44)).)
        assert_eq!(hit_test_zone(&app, 100.0 + 200.0, 100.0 + 150.0), None);
        // Center of the actual pill rect still hits.
        let layout = bento_nano_app::zone_pill_geometry::pill_layout_for_zone(
            app.zones.get(ZoneId(44)).expect("zone"),
            0,
        );
        let cx = layout.rect.x + layout.rect.width * 0.5;
        let cy = layout.rect.y + layout.rect.height * 0.5;
        assert_eq!(hit_test_zone(&app, cx, cy), Some(ZoneId(44)));
    }

    #[test]
    fn hit_test_zone_morph_in_flight_uses_interpolated_rect() {
        let zone = Zone::new(ZoneId(45), Cow::Borrowed("Docs"), 100, 100, 240, 180);
        let app = app_with_zones(vec![zone]);
        // Hover + morph half-way through expanding. Renderer paints
        // morph_pill_to_rect(pill, expanded, eased(0.5)). Hit-rect must
        // mirror that exactly (paint–hit parity within 1 DIP).
        app.hovered_zone.set(Some(ZoneId(45)));
        app.zone_pill_anim_zone.set(Some(ZoneId(45)));
        app.zone_pill_anim_expanding.set(true);
        app.zone_pill_anim_progress.set(0.5);
        let layout = bento_nano_app::zone_pill_geometry::pill_layout_for_zone(
            app.zones.get(ZoneId(45)).expect("zone"),
            0,
        );
        let expanded = bento_nano_style::Rect {
            x: 100.0,
            y: 100.0,
            width: 240.0,
            height: 180.0,
        };
        let eased = bento_nano_app::zone_pill_geometry::ease_out_cubic_progress(0.5);
        let morphed =
            bento_nano_app::zone_pill_geometry::morph_pill_to_rect(layout.rect, expanded, eased);
        // Inside the morphed rect → hit.
        let cx = morphed.x + morphed.width * 0.5;
        let cy = morphed.y + morphed.height * 0.5;
        assert_eq!(hit_test_zone(&app, cx, cy), Some(ZoneId(45)));
        // Just outside the morphed rect (1 DIP beyond right edge) →
        // no hit. This is the paint-hit parity guarantee.
        assert_eq!(
            hit_test_zone(&app, morphed.right() + 1.0, cy),
            None
        );
    }

    #[test]
    fn hit_test_zone_morph_complete_uses_full_rect() {
        let zone = Zone::new(ZoneId(46), Cow::Borrowed("Docs"), 100, 100, 240, 180);
        let app = app_with_zones(vec![zone]);
        // Morph finished at progress=1.0, hover still active — renderer
        // paints the full expanded chrome, so hit-rect is full rect.
        app.hovered_zone.set(Some(ZoneId(46)));
        app.zone_pill_anim_zone.set(Some(ZoneId(46)));
        app.zone_pill_anim_expanding.set(true);
        app.zone_pill_anim_progress.set(1.0);
        // Far corner of full expanded rect → hit.
        assert_eq!(hit_test_zone(&app, 100.0 + 200.0, 100.0 + 150.0), Some(ZoneId(46)));
    }

    #[test]
    fn hit_test_zone_item_skipped_in_collapsed_pill_mode() {
        let mut zone = Zone::new(ZoneId(44), Cow::Borrowed("z"), 10, 10, 240, 180);
        let _item = zone
            .add_item(
                Cow::Owned("C:/Users/HP/Desktop/A.lnk".to_owned()),
                Cow::Borrowed("hash-a"),
            )
            .expect("item");
        let app = app_with_zones(vec![zone]);
        // Hover-default + not hovered → pill mode — items not hit-testable.
        assert!(hit_test_zone_item(&app, 24.0, 48.0).is_none());
    }

    #[test]
    fn item_grid_position_for_point_clamps_to_visible_columns() {
        let app = app_with_zones(vec![Zone::new(
            ZoneId(9),
            Cow::Borrowed("grid"),
            10,
            20,
            240,
            180,
        )]);

        assert_eq!(
            item_grid_position_for_point(&app, ZoneId(9), 24.0, 52.0),
            Some((0, 0))
        );
        assert_eq!(
            item_grid_position_for_point(&app, ZoneId(9), 500.0, 200.0),
            Some((3, 1))
        );
        assert_eq!(
            item_grid_position_for_point(&app, ZoneId(99), 24.0, 52.0),
            None
        );
    }

    #[test]
    fn item_grid_position_for_point_uses_zone_grid_columns() {
        let mut zone = Zone::new(ZoneId(10), Cow::Borrowed("grid"), 10, 20, 240, 180);
        zone.set_grid_columns(2);
        let app = app_with_zones(vec![zone]);

        assert_eq!(
            item_grid_position_for_point(&app, ZoneId(10), 220.0, 52.0),
            Some((1, 0))
        );
    }

    #[test]
    fn settings_hit_outside_returns_outside() {
        let app = app_with_zones(vec![]);
        // (0,0) is outside the centred Settings panel on a 480×320 viewport.
        assert_eq!(settings_hit(&app, 0.0, 0.0), SettingsHit::Outside);
    }

    #[test]
    fn settings_hit_resolves_buttons_and_body() {
        // Round-2 M1 — the dark shell only routes the new variants. Wave K1
        // rect helpers (locale "switch" chip, encryption mode, zone display,
        // theme chip, vault chips, backup entries, recovery actions, etc.)
        // are intentionally orphan-alive per Ruling B and no longer fire.
        let app = app_with_zones(vec![]);

        // Top 5 toggles map to their ToggleX variants in order.
        let scroll_y = 0.0;
        let r0 = bento_nano_app::settings_panel::settings_top_toggle_hit_rect(
            app.viewport,
            scroll_y,
            0,
        );
        assert_eq!(
            settings_hit(&app, r0.x + r0.width * 0.5, r0.y + r0.height * 0.5),
            SettingsHit::ToggleDesktopEmbed
        );
        let r1 = bento_nano_app::settings_panel::settings_top_toggle_hit_rect(
            app.viewport,
            scroll_y,
            1,
        );
        assert_eq!(
            settings_hit(&app, r1.x + r1.width * 0.5, r1.y + r1.height * 0.5),
            SettingsHit::ToggleAutostart
        );
        let r2 = bento_nano_app::settings_panel::settings_top_toggle_hit_rect(
            app.viewport,
            scroll_y,
            2,
        );
        assert_eq!(
            settings_hit(&app, r2.x + r2.width * 0.5, r2.y + r2.height * 0.5),
            SettingsHit::ToggleShowInTaskbar
        );
        let r3 = bento_nano_app::settings_panel::settings_top_toggle_hit_rect(
            app.viewport,
            scroll_y,
            3,
        );
        assert_eq!(
            settings_hit(&app, r3.x + r3.width * 0.5, r3.y + r3.height * 0.5),
            SettingsHit::ToggleSmartLayout
        );
        let r4 = bento_nano_app::settings_panel::settings_top_toggle_hit_rect(
            app.viewport,
            scroll_y,
            4,
        );
        assert_eq!(
            settings_hit(&app, r4.x + r4.width * 0.5, r4.y + r4.height * 0.5),
            SettingsHit::TogglePortableMode
        );

        // Language chip → OpenLocaleMenu.
        let lang = bento_nano_app::settings_panel::settings_language_chip_rect(
            app.viewport,
            scroll_y,
        );
        assert_eq!(
            settings_hit(&app, lang.x + lang.width * 0.5, lang.y + lang.height * 0.5),
            SettingsHit::OpenLocaleMenu
        );

        // Footer Cancel + Save.
        let cancel = bento_nano_app::settings_panel::settings_cancel_button_rect(app.viewport);
        assert_eq!(
            settings_hit(
                &app,
                cancel.x + cancel.width * 0.5,
                cancel.y + cancel.height * 0.5
            ),
            SettingsHit::CancelSettings
        );
        let save = bento_nano_app::settings_panel::settings_save_button_rect(app.viewport);
        assert_eq!(
            settings_hit(
                &app,
                save.x + save.width * 0.5,
                save.y + save.height * 0.5
            ),
            SettingsHit::SaveSettings
        );

        // Close × in the sticky header.
        let close = bento_nano_app::settings_panel::settings_close_button_rect_m1(app.viewport);
        assert_eq!(
            settings_hit(&app, close.x + close.width * 0.5, close.y + close.height * 0.5),
            SettingsHit::Close
        );

        // Inside the body chrome but not over a control → Body.
        let body = bento_nano_app::settings_panel::settings_body_rect(app.viewport);
        assert_eq!(
            settings_hit(&app, body.x + 4.0, body.y + body.height - 4.0),
            SettingsHit::Body
        );

        // Outside the panel rect → Outside.
        let panel = bento_nano_app::settings_panel::settings_panel_rect_m1(app.viewport);
        assert_eq!(
            settings_hit(&app, panel.x - 5.0, panel.y - 5.0),
            SettingsHit::Outside
        );
    }

    // Round-2 M1 — the K1 `settings_hit_resolves_visible_backup_entry_restore`
    // test was retired with the K1 vault row. The dispatch variant
    // `SettingsHit::RestoreSettingsBackup` stays orphan-alive until M4's
    // 设置备份 section re-introduces the hit path.
    #[test]
    fn _retired_settings_hit_resolves_visible_backup_entry_restore_in_round_2_m1() {}

    /// M1g — reachability: with backup entries seeded, clicking 立即备份 /
    /// 刷新 / per-row 恢复 resolves to the (previously orphan) backup
    /// `SettingsHit` variants. Proves the paint→hit chain is wired — after
    /// this chunk no backup button is painted-but-unwired. Builds the SAME
    /// `SettingsBodyFlags` (idle updater + capped backup count) the hit-tester
    /// derives so the sampled button centres line up with production geometry.
    #[test]
    fn m1g_settings_hit_resolves_backup_create_refresh_and_per_row_restore() {
        use bento_nano_app::SettingsBackupEntry;
        use smol_str::SmolStr;

        let app = app_with_zones(vec![]);
        // Seed two real-shaped entries so the per-row restore path is live.
        app.settings_backup_entries.replace(vec![
            SettingsBackupEntry {
                id: SmolStr::new_static("1748467200-100"),
                file_name: SmolStr::new_static("vault-1748467200-100.bin"),
                size_bytes: 4096,
            },
            SettingsBackupEntry {
                id: SmolStr::new_static("1748460000-100"),
                file_name: SmolStr::new_static("vault-1748460000-100.bin"),
                size_bytes: 8192,
            },
        ]);

        // Rebuild the EXACT flags the hit-tester derives: the live Startup
        // gating bools (both default true in AppState::new) + idle updater
        // (StatusOnly) + the capped visible backup row count. Reading them off
        // `app` (rather than hardcoding) is what makes the test's button rects
        // line up with production geometry.
        let entries = app.settings_backup_entries.borrow();
        let visible =
            bento_nano_app::business::settings::backup_card::backup_visible_row_count(&entries);
        let flags = bento_nano_app::settings_panel::SettingsBodyFlags::new(
            app.crash_restart_enabled.get(),
            app.safe_start_after_hibernation.get(),
            false,
            false,
            bento_nano_app::settings_panel::UpdaterHeightKind::StatusOnly,
        )
        .with_backup_rows(visible);
        drop(entries);

        // The Backup card sits at the bottom of the scrollable content; scroll
        // to the clamped max so its rows fall inside the visible body (the
        // hit-tester early-returns `Body`/`Outside` for any y outside the body
        // rect). Assert the whole backup section is in view so the geometry —
        // not scroll bookkeeping — is what's under test.
        let content_h =
            bento_nano_app::settings_panel::settings_body_content_height(app.viewport, &flags);
        let max_scroll =
            bento_nano_app::settings_panel::settings_body_max_scroll(content_h, app.viewport);
        app.scroll_offset_y.set(max_scroll);
        // M1i fidelity — `settings_hit` folds the §2 source reserve delta into
        // the scroll for all perf-and-below geometry; this test populates no
        // desktop sources (count 0), so apply the matching fold to the rects we
        // compare against, exactly as production paint/hit does.
        let scroll_y = max_scroll
            + bento_nano_app::settings_panel::settings_sources_reserve_delta(
                flags.source_row_count,
            );
        let body = bento_nano_app::settings_panel::settings_body_rect(app.viewport);
        let label = bento_nano_app::settings_panel::settings_backup_label_rect(
            app.viewport,
            scroll_y,
            &flags,
        );
        assert!(
            label.y >= body.y && label.y < body.bottom(),
            "backup section must scroll into the visible body (label.y={}, body=[{}, {}])",
            label.y,
            body.y,
            body.bottom(),
        );

        let actions = bento_nano_app::settings_panel::settings_backup_actions_row_rect(
            app.viewport,
            scroll_y,
            &flags,
        );
        let create = bento_nano_app::settings_panel::settings_backup_create_button_rect(actions);
        assert_eq!(
            settings_hit(&app, create.x + create.width * 0.5, create.y + create.height * 0.5),
            SettingsHit::CreateSettingsBackup,
        );
        let refresh = bento_nano_app::settings_panel::settings_backup_refresh_button_rect(actions);
        assert_eq!(
            settings_hit(&app, refresh.x + refresh.width * 0.5, refresh.y + refresh.height * 0.5),
            SettingsHit::ListSettingsBackups,
        );
        // Per-row 恢复 — index 0 and index 1 each route to their own index.
        for entry_index in 0..visible {
            let row = bento_nano_app::settings_panel::settings_backup_entry_row_rect(
                app.viewport,
                scroll_y,
                &flags,
                entry_index,
            );
            let restore =
                bento_nano_app::settings_panel::settings_backup_restore_button_rect(row);
            assert_eq!(
                settings_hit(
                    &app,
                    restore.x + restore.width * 0.5,
                    restore.y + restore.height * 0.5,
                ),
                SettingsHit::RestoreSettingsBackup(entry_index),
                "per-row restore must carry the newest-first list index",
            );
        }
    }

    /// M1g — empty list: with no backup entries there is no per-row restore
    /// hit (the empty-placeholder row is non-interactive), but 立即备份 /
    /// 刷新 stay reachable.
    #[test]
    fn m1g_settings_hit_empty_backup_list_has_no_restore_but_keeps_create_refresh() {
        let app = app_with_zones(vec![]);
        assert!(app.settings_backup_entries.borrow().is_empty());

        let flags = bento_nano_app::settings_panel::SettingsBodyFlags::new(
            app.crash_restart_enabled.get(),
            app.safe_start_after_hibernation.get(),
            false,
            false,
            bento_nano_app::settings_panel::UpdaterHeightKind::StatusOnly,
        )
        .with_backup_rows(0);
        // Scroll the bottom Backup card into the visible body (see the sibling
        // reachability test for why).
        let content_h =
            bento_nano_app::settings_panel::settings_body_content_height(app.viewport, &flags);
        let max_scroll =
            bento_nano_app::settings_panel::settings_body_max_scroll(content_h, app.viewport);
        app.scroll_offset_y.set(max_scroll);
        // M1i fidelity — `settings_hit` folds the §2 source reserve delta into
        // the scroll for all perf-and-below geometry; this test populates no
        // desktop sources (count 0), so apply the matching fold to the rects we
        // compare against, exactly as production paint/hit does.
        let scroll_y = max_scroll
            + bento_nano_app::settings_panel::settings_sources_reserve_delta(
                flags.source_row_count,
            );
        let actions = bento_nano_app::settings_panel::settings_backup_actions_row_rect(
            app.viewport,
            scroll_y,
            &flags,
        );
        let create = bento_nano_app::settings_panel::settings_backup_create_button_rect(actions);
        assert_eq!(
            settings_hit(&app, create.x + create.width * 0.5, create.y + create.height * 0.5),
            SettingsHit::CreateSettingsBackup,
        );
        // The empty-placeholder row's centre must NOT produce a restore hit —
        // it eats as Body (non-interactive).
        let empty_row = bento_nano_app::settings_panel::settings_backup_entry_row_rect(
            app.viewport,
            scroll_y,
            &flags,
            0,
        );
        let hit = settings_hit(
            &app,
            empty_row.x + empty_row.width * 0.5,
            empty_row.y + empty_row.height * 0.5,
        );
        assert_ne!(hit, SettingsHit::RestoreSettingsBackup(0));
    }

    /// α4 (Wave I-α, 2026-05-25) — clicking each of the three zone-display
    /// radio hit-boxes routes to the matching `SetZoneDisplayMode(mode)`
    /// variant. Each hit-box centre is sampled so the test exercises the
    /// hit-tester (not the geometry, which has its own settings_panel.rs
    /// tests).
    #[test]
    fn alpha4_three_radio_hit_boxes_route_to_set_zone_display_mode() {
        let app = app_with_zones(vec![]);
        let scroll_y = 0.0;

        let r_hover = bento_nano_app::settings_panel::settings_zone_display_mode_radio_rect(
            app.viewport,
            scroll_y,
            0,
        );
        assert_eq!(
            settings_hit(
                &app,
                r_hover.x + r_hover.width * 0.5,
                r_hover.y + r_hover.height * 0.5,
            ),
            SettingsHit::SetZoneDisplayMode(bento_nano_app::ZoneDisplayMode::Hover)
        );

        let r_always = bento_nano_app::settings_panel::settings_zone_display_mode_radio_rect(
            app.viewport,
            scroll_y,
            1,
        );
        assert_eq!(
            settings_hit(
                &app,
                r_always.x + r_always.width * 0.5,
                r_always.y + r_always.height * 0.5,
            ),
            SettingsHit::SetZoneDisplayMode(bento_nano_app::ZoneDisplayMode::Always)
        );

        let r_click = bento_nano_app::settings_panel::settings_zone_display_mode_radio_rect(
            app.viewport,
            scroll_y,
            2,
        );
        assert_eq!(
            settings_hit(
                &app,
                r_click.x + r_click.width * 0.5,
                r_click.y + r_click.height * 0.5,
            ),
            SettingsHit::SetZoneDisplayMode(bento_nano_app::ZoneDisplayMode::Click)
        );
    }

    #[test]
    fn settings_hit_routes_keybindings_modal_buttons_first() {
        let app = app_with_zones(vec![]);
        app.settings_keybindings_open.set(true);

        let record = settings_keybinding_record_rect(app.viewport, 0);
        assert_eq!(
            settings_hit(
                &app,
                record.x + record.width * 0.5,
                record.y + record.height * 0.5
            ),
            SettingsHit::RecordKeybinding(0)
        );

        let reset = settings_keybinding_reset_rect(app.viewport, 1);
        assert_eq!(
            settings_hit(
                &app,
                reset.x + reset.width * 0.5,
                reset.y + reset.height * 0.5
            ),
            SettingsHit::ResetKeybinding(1)
        );

        let close = settings_keybindings_close_rect(app.viewport);
        assert_eq!(
            settings_hit(
                &app,
                close.x + close.width * 0.5,
                close.y + close.height * 0.5
            ),
            SettingsHit::CloseKeybindings
        );

        let modal = settings_keybindings_modal_rect(app.viewport);
        assert_eq!(
            settings_hit(&app, modal.x + 8.0, modal.y + modal.height - 8.0),
            SettingsHit::Body
        );
    }

    /// M1h — reachability: with plugin entries seeded, clicking the full-width
    /// 安装插件... button / per-card enable toggle / per-card 卸载 resolves to
    /// `InstallPlugin` / `TogglePlugin(idx)` / `UninstallPlugin(idx)` against
    /// the INLINE §11 geometry (no modal). Proves the paint→hit chain is wired
    /// after the modal→inline move — no plugin control is painted-but-unwired.
    /// Builds the SAME `SettingsBodyFlags` (idle updater + empty backup +
    /// capped plugin count) the hit-tester derives so the sampled centres line
    /// up with production geometry, then scrolls the bottom Plugins section
    /// into the visible body.
    #[test]
    fn m1h_settings_hit_resolves_inline_plugin_install_toggle_and_per_card_uninstall() {
        let app = app_with_zones(vec![]);
        // Seed two real-shaped entries so the per-card toggle/uninstall paths
        // are live (different kinds + enabled states for good measure).
        app.settings_plugin_entries.replace(vec![
            SettingsPluginEntry {
                id: smol_str::SmolStr::new_static("com.test.theme"),
                name: smol_str::SmolStr::new_static("Theme"),
                version: smol_str::SmolStr::new_static("1.0.0"),
                plugin_type: smol_str::SmolStr::new_static("theme"),
                author: smol_str::SmolStr::new_static("Acme"),
                description: smol_str::SmolStr::new_static("A theme plugin"),
                enabled: true,
            },
            SettingsPluginEntry {
                id: smol_str::SmolStr::new_static("com.test.widget"),
                name: smol_str::SmolStr::new_static("Widget"),
                version: smol_str::SmolStr::new_static("2.0.0"),
                plugin_type: smol_str::SmolStr::new_static("widget"),
                author: smol_str::SmolStr::new_static("Acme"),
                description: smol_str::SmolStr::new_static("A widget plugin"),
                enabled: false,
            },
        ]);

        // Rebuild the EXACT flags the hit-tester derives: live Startup gating
        // bools + idle updater + empty backup list + capped visible plugin
        // count. Reading them off `app` keeps the sampled rects production-true.
        let entries = app.settings_plugin_entries.borrow();
        let visible =
            bento_nano_app::business::settings::plugins_section::plugin_visible_row_count(&entries);
        let flags = bento_nano_app::settings_panel::SettingsBodyFlags::new(
            app.crash_restart_enabled.get(),
            app.safe_start_after_hibernation.get(),
            false,
            false,
            bento_nano_app::settings_panel::UpdaterHeightKind::StatusOnly,
        )
        .with_backup_rows(0)
        .with_plugin_rows(visible);
        drop(entries);

        // Plugins sits LAST in the body; scroll to the clamped max so its rows
        // fall inside the visible body (the hit-tester early-returns for any y
        // outside the body rect).
        let content_h =
            bento_nano_app::settings_panel::settings_body_content_height(app.viewport, &flags);
        let max_scroll =
            bento_nano_app::settings_panel::settings_body_max_scroll(content_h, app.viewport);
        app.scroll_offset_y.set(max_scroll);
        // M1i fidelity — `settings_hit` folds the §2 source reserve delta into
        // the scroll for all perf-and-below geometry; this test populates no
        // desktop sources (count 0), so apply the matching fold to the rects we
        // compare against, exactly as production paint/hit does.
        let scroll_y = max_scroll
            + bento_nano_app::settings_panel::settings_sources_reserve_delta(
                flags.source_row_count,
            );
        let body = bento_nano_app::settings_panel::settings_body_rect(app.viewport);
        let label =
            bento_nano_app::settings_panel::settings_plugins_label_rect(app.viewport, scroll_y, &flags);
        assert!(
            label.y >= body.y && label.y < body.bottom(),
            "plugins section must scroll into the visible body (label.y={}, body=[{}, {}])",
            label.y,
            body.y,
            body.bottom(),
        );

        // 安装插件... full-width button → InstallPlugin.
        let install = bento_nano_app::settings_panel::settings_plugins_install_button_rect(
            app.viewport,
            scroll_y,
            &flags,
        );
        assert_eq!(
            settings_hit(&app, install.x + install.width * 0.5, install.y + install.height * 0.5),
            SettingsHit::InstallPlugin,
        );

        // Per-card enable toggle + 卸载 — each routes to its own card index.
        for card_index in 0..visible {
            let card = bento_nano_app::settings_panel::settings_plugin_card_rect(
                app.viewport,
                scroll_y,
                &flags,
                card_index,
            );
            let toggle = bento_nano_app::settings_panel::settings_plugin_toggle_hit_rect(card);
            assert_eq!(
                settings_hit(&app, toggle.x + toggle.width * 0.5, toggle.y + toggle.height * 0.5),
                SettingsHit::TogglePlugin(card_index),
                "per-card toggle must carry the list index",
            );
            let uninstall =
                bento_nano_app::settings_panel::settings_plugin_uninstall_button_rect(card);
            assert_eq!(
                settings_hit(
                    &app,
                    uninstall.x + uninstall.width * 0.5,
                    uninstall.y + uninstall.height * 0.5,
                ),
                SettingsHit::UninstallPlugin(card_index),
                "per-card uninstall must carry the list index",
            );
        }
    }

    /// M1h — empty list: with no plugins there is no per-card toggle/uninstall
    /// hit (the empty-placeholder row is non-interactive), but the full-width
    /// 安装插件... button stays reachable.
    #[test]
    fn m1h_settings_hit_empty_plugin_list_keeps_install_but_has_no_card_hits() {
        let app = app_with_zones(vec![]);
        assert!(app.settings_plugin_entries.borrow().is_empty());

        let flags = bento_nano_app::settings_panel::SettingsBodyFlags::new(
            app.crash_restart_enabled.get(),
            app.safe_start_after_hibernation.get(),
            false,
            false,
            bento_nano_app::settings_panel::UpdaterHeightKind::StatusOnly,
        )
        .with_backup_rows(0)
        .with_plugin_rows(0);
        let content_h =
            bento_nano_app::settings_panel::settings_body_content_height(app.viewport, &flags);
        let max_scroll =
            bento_nano_app::settings_panel::settings_body_max_scroll(content_h, app.viewport);
        app.scroll_offset_y.set(max_scroll);
        // M1i fidelity — `settings_hit` folds the §2 source reserve delta into
        // the scroll for all perf-and-below geometry; this test populates no
        // desktop sources (count 0), so apply the matching fold to the rects we
        // compare against, exactly as production paint/hit does.
        let scroll_y = max_scroll
            + bento_nano_app::settings_panel::settings_sources_reserve_delta(
                flags.source_row_count,
            );
        let install = bento_nano_app::settings_panel::settings_plugins_install_button_rect(
            app.viewport,
            scroll_y,
            &flags,
        );
        assert_eq!(
            settings_hit(&app, install.x + install.width * 0.5, install.y + install.height * 0.5),
            SettingsHit::InstallPlugin,
        );
        // The empty-placeholder row's centre must NOT produce a plugin hit — it
        // eats as Body (non-interactive).
        let empty_row = bento_nano_app::settings_panel::settings_plugin_empty_row_rect(
            app.viewport,
            scroll_y,
            &flags,
        );
        let empty_hit = settings_hit(
            &app,
            empty_row.x + empty_row.width * 0.5,
            empty_row.y + empty_row.height * 0.5,
        );
        assert_ne!(empty_hit, SettingsHit::TogglePlugin(0));
        assert_ne!(empty_hit, SettingsHit::UninstallPlugin(0));
    }
}
