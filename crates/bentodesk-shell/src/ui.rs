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
//! pre-parity native scaffolding that survived only because nobody examined
//! the top-left corner during recent visual audits. The toolbar + status
//! mount calls are removed from this builder. The SVG path constants
//! (`PIN_PATH` etc.), event id table (`events::*`), and `nchittest_kind`
//! helpers stay because non-Main HWNDs (auxiliary settings / about /
//! picker windows) still depend on `nchittest_kind` for their own caption
//! drag-handle heuristics, and the keybinding dispatch table
//! (`action:toggle_pin` / `action:open_settings` / …) still references
//! the event id constants via `main.rs::toolbar_action_label`.
//!
//! All strings flow through the `bentodesk-style::i18n` table — no inline
//! literals in user-visible text positions.

use bentodesk_app::{
    AppState, WindowState,
    business::{bulk_manager_panel, highlight_overlay, popover, search_bar, stack_tray},
    expanded_zone_grid, zone_pill_geometry,
};
use bentodesk_layout::Direction;
use bentodesk_style::{BorderRadius, Color, Edges, Length, Rect, Shadow};
use bentodesk_tree::NodeId;
use bentodesk_widget::{ContainerNode, WidgetKind, WidgetNode};
use bentodesk_zone::{Zone, ZoneId, ZoneItemId};

/// Embedded SVG path data — compile-time constants, no runtime parsing.
/// Each path lives in 24×24 viewbox; the IconButton renders at 28×28 so a
/// 2px padding visually frames the glyph.
// All paths use only `M / L / Z` commands so the hand-rolled
// `bentodesk-platform::svg::build` parser (M/L/H/V/Z subset) accepts them.
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

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom()
}

mod nchittest;
mod settings_hit;
mod zone_hit;

pub use nchittest::*;
pub use settings_hit::*;
pub use zone_hit::*;

#[cfg(test)]
#[path = "ui/tests.rs"]
mod phase21_tests;
