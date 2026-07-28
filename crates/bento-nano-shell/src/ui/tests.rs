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

/// M5 cleanup (2026-05-31) — compute the `scroll_offset_y` that brings a
/// section to the TOP of the visible body, given the scroll-space Y its
/// label lands at when unscrolled. After the Wave J1b/M6-UI §3 Appearance
/// grid was appended BELOW the Backup §9 / Plugins §11 sections, those two
/// sections are no longer the bottom of the scrollable content — so the old
/// `scroll_offset_y = max_scroll` no longer reveals them (it now reveals the
/// trailing Appearance grid). Instead, scroll precisely so the target
/// section's label sits at the body top, then clamp to the legal range. This
/// derives the offset from live geometry (no hardcoded magic numbers) so the
/// sampled button centres line up with production paint/hit (which share the
/// same `scroll + reserve_delta` fold).
///
/// `label_at_unscrolled_y` is the section label's `.y` computed with
/// `scroll_offset_y == 0` (i.e. `scroll_y == reserve_delta(source_count)`).
/// Returns the `scroll_offset_y` to store in `app.scroll_offset_y` so the
/// section's label aligns with `body.y`.
fn scroll_offset_to_top_of_body(
    viewport: bento_nano_style::Size,
    flags: &bento_nano_app::settings_panel::SettingsBodyFlags,
    label_at_unscrolled_y: f32,
) -> f32 {
    let body = bento_nano_app::settings_panel::settings_body_rect(viewport);
    // At scroll_offset_y == 0 the label lands at `label_at_unscrolled_y`;
    // every unit of scroll shifts it up by one. To move it to `body.y` we
    // scroll by exactly the surplus distance below the body top.
    let want = (label_at_unscrolled_y - body.y).max(0.0);
    // Clamp to the legal scroll range so the stored offset is production-true.
    let content_h = bento_nano_app::settings_panel::settings_body_content_height(viewport, flags);
    let max_scroll = bento_nano_app::settings_panel::settings_body_max_scroll(content_h, viewport);
    want.min(max_scroll)
}

fn app_and_window_with_minibar(zs: Vec<Zone>) -> (AppState, WindowState) {
    let mut app = app_with_zones(zs);
    let mut win = WindowState::new();
    let _ = mount_main_tree(&mut app);
    win.run_layout(&app).expect("main tree layout");
    (app, win)
}

include!("tests/01_main_nchittest_empty_desktop_space_is_transparent.rs");
include!("tests/02_hit_test_zone_uses_pill_rect_when_collapsed.rs");
include!("tests/03_m1g_settings_hit_empty_backup_list_has_no_restore_but_ke.rs");
include!("tests/04_settings_hit_compact_accent_picker_resolves_after_scroll.rs");
