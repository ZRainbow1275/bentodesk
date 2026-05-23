//! Phase 1.4 integration tests — verify the Main HWND tree builder
//! produces a well-formed widget tree and that layout fits the 480×320
//! viewport.
//!
//! Wave G1 (2026-05-20): root changed from `BentoCard` to a fully
//! transparent `Container` — Tauri baseline parity (video review of
//! `屏幕录制 2026-05-20 161936.mp4` confirmed the original never painted
//! a full-screen scrim; only per-pill translucent dark + Settings modal
//! paint their own surfaces on top of the wallpaper). DWM Mica was
//! removed in the same wave because it leaked light-theme cream through
//! the alpha=0 surface and produced a whiteboard look.

use bento_nano_app::{AppState, WindowState};
use bento_nano_shell::ui;
use bento_nano_style::{Size, ZH_CN};
use bento_nano_widget::{WidgetKind, WidgetNode};

#[test]
fn mount_main_tree_produces_well_formed_tree() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 480.0,
        height: 320.0,
    };
    let root = ui::mount_main_tree(&mut app);

    // Wave G1 — root must be a fully transparent Container so the
    // desktop wallpaper shows through. Pills + Settings modal paint
    // their own translucent dark surfaces on top.
    let root_node = app.tree.get(root);
    assert!(
        root_node.is_ok(),
        "root must resolve: {:?}",
        root_node.err()
    );
    let root_kind = match root_node {
        Ok(n) => n.kind(),
        Err(_) => return,
    };
    assert_eq!(root_kind, WidgetKind::Container);
    if let Ok(WidgetNode::Container(c)) = app.tree.get(root) {
        assert!(
            c.background.a <= 0.0,
            "main root container must have transparent background (alpha=0), got {:?}",
            c.background,
        );
    } else {
        panic!("expected Container root, got {:?}", app.tree.get(root).map(|n| n.kind()));
    }

    // V-6 Round-2 (2026-05-21) — the legacy 5-IconButton Toolbar + "就绪"
    // status TextNode were removed from `mount_main_tree` because they
    // were being painted at top-left of the transparent desktop overlay
    // (live hand-test regression: PIN star + SETTINGS gear glyphs +
    // Chinese status text leaked through the ghost layer). The Tauri
    // 1.2.4 baseline never paints a top-left toolbar on the desktop
    // overlay. The tree now contains only the transparent Root + Body
    // containers; IconButton/Text/Toolbar leaves are intentionally
    // absent. Walk descendants and assert the new minimal shape.
    let mut iconbutton_count = 0usize;
    let mut text_count = 0usize;
    let mut toolbar_count = 0usize;
    let mut container_count = 0usize;
    let mut visited: smallvec::SmallVec<[bento_nano_tree::NodeId; 16]> = smallvec::SmallVec::new();
    visited.push(root);
    let mut i = 0;
    while i < visited.len() {
        let id = visited[i];
        i += 1;
        let kind = match app.tree.get(id) {
            Ok(n) => n.kind(),
            Err(_) => continue,
        };
        match kind {
            WidgetKind::IconButton => iconbutton_count += 1,
            WidgetKind::Text => text_count += 1,
            WidgetKind::Toolbar => toolbar_count += 1,
            WidgetKind::Container => container_count += 1,
            _ => {}
        }
        if let Ok(children) = app.tree.children(id) {
            for c in children {
                visited.push(*c);
            }
        }
    }
    assert_eq!(
        iconbutton_count, 0,
        "main tree must NOT mount any IconButtons (V-6 R2 — toolbar removed)"
    );
    assert_eq!(
        text_count, 0,
        "main tree must NOT mount the status TextNode (V-6 R2 — \"就绪\" removed)"
    );
    assert_eq!(
        toolbar_count, 0,
        "main tree must NOT mount any Toolbar (V-6 R2 — toolbar removed)"
    );
    assert_eq!(
        container_count, 2,
        "main tree must mount exactly 2 transparent Containers (Root + Body)"
    );
}

#[test]
fn mount_main_tree_layout_fits_window_bounds() {
    let mut app = AppState::new();
    let viewport = Size {
        width: 480.0,
        height: 320.0,
    };
    app.viewport = viewport;
    let _ = ui::mount_main_tree(&mut app);

    let mut win = WindowState::new();
    let res_call = win.layout.layout(&app.tree, viewport);
    assert!(
        res_call.is_ok(),
        "layout must succeed: {:?}",
        res_call.err()
    );
    let res = match res_call {
        Ok(r) => r,
        Err(_) => return,
    };
    assert!(
        !res.is_empty(),
        "layout must produce at least the root rect"
    );
    for (id, rect) in res.iter() {
        assert!(
            rect.x >= 0.0 && rect.y >= 0.0,
            "node {:?} has negative origin: {:?}",
            id,
            rect
        );
        assert!(
            rect.right() <= viewport.width + 0.5,
            "node {:?} exceeds viewport width: right={} viewport.w={}",
            id,
            rect.right(),
            viewport.width
        );
        assert!(
            rect.bottom() <= viewport.height + 0.5,
            "node {:?} exceeds viewport height: bottom={} viewport.h={}",
            id,
            rect.bottom(),
            viewport.height
        );
    }
}

#[test]
fn icon_button_paths_use_only_supported_svg_commands() {
    // The hand-rolled `bento-nano-platform::svg` parser accepts only
    // M / L / H / V / Z (case-insensitive). A path that slips a curve in
    // would fail at first paint — catch it at test time instead.
    let supported = |c: u8| {
        matches!(
            c,
            b'M' | b'm' | b'L' | b'l' | b'H' | b'h' | b'V' | b'v' | b'Z' | b'z'
        )
    };
    for (name, path) in [
        ("PIN", ui::PIN_PATH),
        ("SETTINGS", ui::SETTINGS_PATH),
        ("HIDE", ui::HIDE_PATH),
        ("ADD", ui::ADD_PATH),
        ("EXIT", ui::EXIT_PATH),
    ] {
        for c in path.bytes() {
            if c.is_ascii_alphabetic() {
                assert!(
                    supported(c),
                    "{name} path uses unsupported command '{}'",
                    c as char
                );
            }
        }
    }
}

#[test]
fn nchittest_returns_caption_in_toolbar_band_when_no_icon_button() {
    // Ruling 3 (post V-6 R2) — the top `TOOLBAR_HEIGHT` band still acts as
    // a drag-handle for non-Main HWNDs whose `mount_*` builders attach a
    // real toolbar; `ui::nchittest_kind` is no longer used by the Main
    // HWND (it switched to `main_nchittest_kind` which delegates blank
    // space to `HTTRANSPARENT`). For the V-6 R2 Main tree (transparent
    // Root + Body, no leaves), `nchittest_kind` still falls into the
    // `y < TOOLBAR_HEIGHT` arm and returns `Caption` because no
    // IconButton hits the cursor — which is the documented Ruling 3
    // semantics, just with an empty body underneath.
    let mut app = AppState::new();
    let viewport = Size {
        width: 480.0,
        height: 320.0,
    };
    app.viewport = viewport;
    let _ = ui::mount_main_tree(&mut app);

    let mut win = WindowState::new();
    let _ = win.run_layout(&app);

    // Pick a point inside the `TOOLBAR_HEIGHT` band (y < 36) — Ruling 3
    // returns `Caption` because no IconButton overlaps.
    let kind = ui::nchittest_kind(&app, &win, 400.0, 8.0);
    assert_eq!(kind, ui::HitKind::Caption);
}

// V-6 Round-2 (2026-05-21) — `nchittest_returns_client_on_iconbutton`
// retired. `mount_main_tree` no longer attaches any IconButton to the
// Main HWND tree (the legacy toolbar was painting at top-left of the
// transparent desktop overlay — pre-parity scaffolding removed). The
// `is_icon_button` predicate stays live for non-Main HWND builders that
// still attach toolbar widgets to their own trees, but no Main-HWND
// IconButton fixture exists here to assert against.
#[test]
fn _retired_nchittest_returns_client_on_iconbutton_v6_r2() {}

#[test]
fn nchittest_returns_client_below_toolbar_band() {
    let mut app = AppState::new();
    let viewport = Size {
        width: 480.0,
        height: 320.0,
    };
    app.viewport = viewport;
    let _ = ui::mount_main_tree(&mut app);

    let mut win = WindowState::new();
    let _ = win.run_layout(&app);

    // Below the TOOLBAR_HEIGHT (36) — anywhere in the body should be
    // HTCLIENT regardless of what's painted there.
    let kind = ui::nchittest_kind(&app, &win, 240.0, 200.0);
    assert_eq!(kind, ui::HitKind::Client);
}

#[test]
fn appstate_alloc_zone_id_is_monotonic_starting_at_one() {
    let app = AppState::new();
    let a = app.alloc_zone_id();
    let b = app.alloc_zone_id();
    let c = app.alloc_zone_id();
    assert_eq!(a, bento_nano_zone::ZoneId(1));
    assert_eq!(b, bento_nano_zone::ZoneId(2));
    assert_eq!(c, bento_nano_zone::ZoneId(3));
}

#[test]
fn appstate_pin_toggle_flips_cell() {
    // PIN consumer flips `is_pinned` between calls — emulate the
    // dispatcher drain to verify the Cell-based state machine.
    let app = AppState::new();
    assert!(!app.is_pinned.get());
    app.is_pinned.set(!app.is_pinned.get());
    assert!(app.is_pinned.get());
    app.is_pinned.set(!app.is_pinned.get());
    assert!(!app.is_pinned.get());
}

#[test]
fn appstate_settings_open_default_is_false_and_flips() {
    let app = AppState::new();
    assert!(!app.settings_open.get());
    app.settings_open.set(true);
    assert!(app.settings_open.get());
}

// V-6 Round-2 (2026-05-21) — `mount_main_tree_status_text_resolves_to_active_locale`
// retired. `mount_main_tree` no longer attaches the "就绪" / "Ready" status
// `TextNode` to the Main HWND tree (the status text was painting at the
// left edge of the transparent desktop overlay — pre-parity scaffolding
// that survived earlier visual audits). `STATUS_READY` stays defined in
// `bento-nano-style::i18n_{zh,en}` and continues to resolve correctly when
// other surfaces choose to display it (the keybindings_section + minibar
// pill chrome still reference status strings); the assertion just no
// longer has a Main-HWND TextNode to inspect.
#[test]
fn _retired_mount_main_tree_status_text_resolves_to_active_locale_v6_r2() {
    // Sanity-touch the locale init so `bento_nano_style::ZH_CN` stays in
    // the test crate's used-symbol surface — keeps the import that other
    // tests in this module still rely on.
    bento_nano_style::init_locale(&ZH_CN);
}
