//! Phase 2.1 misc behaviour tests covering Rulings B / C / D semantics that
//! don't require a live HWND.

#![forbid(unsafe_op_in_unsafe_fn)]

use std::borrow::Cow;

use bentodesk_app::{AppState, Command, EventDispatcher};
use bentodesk_platform::WindowKind;
use bentodesk_shell::ui;
use bentodesk_style::{EN_US, Size, ZH_CN, current_locale_is, init_locale, set_locale};
use bentodesk_zone::{Zone, ZoneId};

#[test]
fn dispatcher_carries_zone_delete_command_with_id() {
    // Ruling D — DeleteZone is the right-click outcome on a zone body.
    let d = EventDispatcher::new();
    assert!(d.push(Command::DeleteZone(ZoneId(42))));
    let mut buf: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    let n = d.drain_into(&mut buf);
    assert_eq!(n, 1);
    let got_id = match buf.first() {
        Some(Command::DeleteZone(id)) => Some(*id),
        _ => None,
    };
    assert_eq!(got_id, Some(ZoneId(42)));
}

#[test]
fn dispatcher_close_and_toggle_locale_are_distinct_variants() {
    // Ruling C — settings panel emits CloseSettings AND ToggleLocale.
    let d = EventDispatcher::new();
    let _ = d.push(Command::CloseSettings);
    let _ = d.push(Command::ToggleLocale);
    let mut buf: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    let _ = d.drain_into(&mut buf);
    assert_eq!(buf.len(), 2);
    let kind0 = match buf.first() {
        Some(Command::CloseSettings) => "close",
        _ => "other",
    };
    let kind1 = match buf.get(1) {
        Some(Command::ToggleLocale) => "locale",
        _ => "other",
    };
    assert_eq!(kind0, "close");
    assert_eq!(kind1, "locale");
}

#[test]
fn dispatcher_show_window_and_show_tray_menu_distinct() {
    // Ruling B — tray icon left-click → ShowWindow(Main); right-click →
    // ShowTrayMenu. T-013 widened ShowWindow to carry a `WindowKind`; the
    // tray-icon producer always passes `Main` (only the main HWND has a
    // tray surface).
    let d = EventDispatcher::new();
    let _ = d.push(Command::ShowWindow(WindowKind::Main));
    let _ = d.push(Command::ShowTrayMenu);
    let mut buf: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    let _ = d.drain_into(&mut buf);
    let kinds: Vec<&'static str> = buf
        .iter()
        .map(|c| match c {
            Command::ShowWindow(WindowKind::Main) => "show",
            Command::ShowTrayMenu => "menu",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["show", "menu"]);
}

#[test]
fn locale_toggle_swaps_zh_and_en_in_place() {
    // Phase 2.1 / Ruling C — locale-switch button flips zh-CN ⇄ en-US.
    init_locale(&ZH_CN);
    assert!(current_locale_is(&ZH_CN));
    set_locale(&EN_US);
    assert!(current_locale_is(&EN_US));
    set_locale(&ZH_CN);
    assert!(current_locale_is(&ZH_CN));
}

#[test]
fn g3_settings_panel_labels_differ_across_zh_and_en() {
    // G3 wave — every Settings-panel StringId migrated off a hard-coded
    // English literal must resolve to a different glyph set in zh-CN versus
    // en-US, otherwise the Ctrl+, panel keeps speaking English in the
    // baseline locale (the after-ctrl-comma.png regression).
    //
    // Compare the two static tables directly. `t(id)` is separately covered in
    // bentodesk-style; using the process-global locale pointer here races the
    // locale-toggle test when this integration binary runs in parallel.
    use bentodesk_style::i18n_zh_cn::ids;

    let migrated = [
        // Row labels — the screenshot's left column.
        ids::SETTINGS_UPDATES,
        ids::SETTINGS_AUTO_DOWNLOAD,
        ids::SETTINGS_STEALTH_STORAGE,
        ids::SETTINGS_VAULT_ENCRYPTION,
        ids::SETTINGS_THEME_HEADING,
        ids::SETTINGS_VAULT,
        ids::SETTINGS_KEYS,
        ids::SETTINGS_PLUGINS,
        ids::SETTINGS_PERSISTENCE_HINT,
        // Right-column buttons.
        ids::BTN_CHECK,
        ids::BTN_ON,
        ids::BTN_OFF,
        ids::BTN_SKIP,
        ids::BTN_IMPORT,
        ids::BTN_BACKUP,
        ids::BTN_LIST,
        ids::BTN_RESTORE,
        ids::BTN_BUNDLE,
        ids::BTN_DIAG,
        ids::BTN_RECOVER,
        // Enum-driven cycle buttons.
        ids::UPDATE_FREQ_DAILY,
        ids::UPDATE_FREQ_WEEKLY,
        ids::UPDATE_FREQ_MANUAL,
        ids::ZONE_MODE_HOVER,
        ids::ZONE_MODE_ALWAYS,
        ids::ZONE_MODE_CLICK,
        // Wave I-α / R14 (2026-05-25) — picker row caption added so the
        // 3-radio Zone Display Mode picker can render `默认显示模式` / `Default
        // display mode` as the row label without doubling up "模式:" on each
        // radio. Pin guards id 140 against table-padding regressions in both
        // locales.
        ids::SETTINGS_ZONE_DISPLAY_MODE_LABEL,
        // Updater + encryption state machine.
        ids::UPDATER_IDLE,
        ids::UPDATER_CHECKING,
        ids::ENCRYPTION_MODE_NONE,
        ids::ENCRYPTION_TYPE_PASS,
        ids::ENCRYPTION_UNLOCK,
        ids::THEME_DEFAULT,
        // Plugins + keybindings modals.
        ids::PLUGINS_REGISTRY_HINT,
        ids::PLUGINS_REFRESH,
        ids::PLUGINS_REMOVE,
        ids::KEYBINDINGS_TITLE,
        ids::KEYBINDINGS_RECORDING,
        ids::KEYBINDINGS_RECORD,
        ids::KEYBINDINGS_RESET,
    ];

    let zh: Vec<&'static str> = migrated.iter().map(|id| ZH_CN.get(*id)).collect();
    let en: Vec<&'static str> = migrated.iter().map(|id| EN_US.get(*id)).collect();

    // Every migrated id must yield a non-empty translation in both locales.
    for (idx, (z, e)) in zh.iter().zip(en.iter()).enumerate() {
        assert!(
            !z.is_empty(),
            "zh-CN slot {idx} (id={:?}) is empty — table padding regression",
            migrated[idx]
        );
        assert!(
            !e.is_empty(),
            "en-US slot {idx} (id={:?}) is empty — table padding regression",
            migrated[idx]
        );
    }

    // Count distinct-across-locales pairs. The Dpapi mode legitimately
    // shares its glyph across both tables (Win32 API name), so we expect
    // strictly more than half of the migrated ids to differ — well above
    // the wave brief's "≥ 5" threshold.
    let differing: usize = zh.iter().zip(en.iter()).filter(|(z, e)| z != e).count();
    assert!(
        differing >= 5,
        "expected ≥ 5 ids to differ across zh-CN/en-US, only {differing} did"
    );
    assert!(
        differing >= migrated.len() / 2,
        "expected most migrated ids to differ; only {differing}/{} did",
        migrated.len()
    );
}

#[test]
fn zone_drag_mousemove_updates_geometry_but_not_dirty() {
    // Q1 ruling — mousemove updates (x,y) on a dragged zone but **does not**
    // flip `dirty`. The save granularity is per-gesture (LBUTTONUP).
    let mut app = AppState::new();
    app.viewport = Size {
        width: 480.0,
        height: 320.0,
    };
    app.zones
        .add(Zone::new(ZoneId(1), Cow::Borrowed("z"), 50, 50, 100, 100));
    app.zone_drag.set(Some((ZoneId(1), 5, 7)));

    // Mimic handle_mouse_move's drag branch: mutate zone WITHOUT mark_dirty.
    if let Some((id, dx, dy)) = app.zone_drag.get()
        && let Some(z) = app.zones.get_mut(id)
    {
        // mouse at (200, 150) → new top-left = (200-5, 150-7) = (195, 143)
        z.x = 200 - dx;
        z.y = 150 - dy;
    }
    let z = match app.zones.get(ZoneId(1)) {
        Some(z) => z,
        None => return,
    };
    assert_eq!((z.x, z.y), (195, 143));
    assert!(!app.dirty.get(), "mousemove must NOT mark dirty (Q1)");

    // Then handle_lbutton_up's branch: drop drag state, mark dirty once.
    app.zone_drag.set(None);
    app.mark_dirty();
    assert!(app.dirty.get(), "LBUTTONUP marks dirty exactly once");
}

#[test]
fn zone_resize_state_clamps_to_minimum_dims() {
    // Ruling D — resize must clamp to 80×60 minimum so a user can't shrink
    // a zone past usability.
    let mut app = AppState::new();
    app.viewport = Size {
        width: 480.0,
        height: 320.0,
    };
    app.zones
        .add(Zone::new(ZoneId(2), Cow::Borrowed("z"), 100, 100, 200, 150));
    app.zone_resize.set(Some((ZoneId(2), 200, 150)));

    // Simulate mouse pulled inside the zone — would naively shrink to
    // (10, 5) but clamping must protect the floor.
    if let Some((id, _, _)) = app.zone_resize.get()
        && let Some(z) = app.zones.get_mut(id)
    {
        let new_w = (110_i32 - z.x).max(80);
        let new_h = (105_i32 - z.y).max(60);
        z.w = new_w;
        z.h = new_h;
    }
    let z = match app.zones.get(ZoneId(2)) {
        Some(z) => z,
        None => return,
    };
    assert!(z.w >= 80, "width must clamp to 80, got {}", z.w);
    assert!(z.h >= 60, "height must clamp to 60, got {}", z.h);
}

#[test]
fn settings_hit_outside_panel_returns_outside_close_command_target() {
    // Ruling C — click-outside dismisses (`Outside` → `CloseSettings`).
    let mut app = AppState::new();
    app.viewport = Size {
        width: 800.0,
        height: 600.0,
    };
    let h = ui::settings_hit(&app, 5.0, 5.0);
    assert_eq!(h, ui::SettingsHit::Outside);
}

#[test]
fn appstate_dirty_starts_false_and_mark_dirty_flips() {
    let app = AppState::new();
    assert!(!app.dirty.get(), "fresh state must be clean");
    app.mark_dirty();
    assert!(app.dirty.get());
    app.dirty.set(false);
    assert!(!app.dirty.get());
}

#[test]
fn refcell_panics_on_nested_borrow_mut_simulating_wndproc_reentry() {
    // Sanity check — verifies the rollback's safety story.
    //
    // The previous unsafe `ui_get_zone_mut` upgraded `&AppState` to
    // `&mut Zone` via raw-pointer cast. When a pump-message Win32 API
    // (TrackPopupMenu / HTCAPTION move loop / MessageBox / SetWindowPos /
    // Shell_NotifyIconW) re-entered the wndproc while the original `&mut`
    // was still live, the second handler's raw-ptr would alias — silent UB.
    //
    // The new safe path borrows through `RefCell<AppState>`. Re-entry
    // panics deterministically instead. This test simulates that scenario.
    use std::cell::RefCell;
    use std::panic;

    let cell: RefCell<AppState> = RefCell::new({
        let mut a = AppState::new();
        a.viewport = Size {
            width: 480.0,
            height: 320.0,
        };
        a.zones
            .add(Zone::new(ZoneId(1), Cow::Borrowed("z"), 0, 0, 100, 100));
        a
    });

    // Mute the default panic hook so RUST_BACKTRACE noise doesn't pollute
    // CI output during the expected catch_unwind.
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let outer = cell.borrow_mut();
    let nested = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        // Re-entrant wndproc call that also tries to borrow_mut.
        let _inner = cell.borrow_mut();
    }));
    drop(outer);

    panic::set_hook(prev_hook);

    assert!(
        nested.is_err(),
        "nested borrow_mut MUST panic — RefCell prevents the silent \
         aliasing UB the raw-ptr design used to allow"
    );
}
