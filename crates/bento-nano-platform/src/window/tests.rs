use super::{
    CS_DBLCLKS, CW_USEDEFAULT, WINDOW_CLASS_STYLE, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    WS_VISIBLE, WindowKind, default_size, ex_style_for, main_window_rect, scale_96_dpi_dimension,
};
use windows_sys::Win32::UI::WindowsAndMessaging::WS_OVERLAPPEDWINDOW;

#[test]
fn scale_96_dpi_dimension_keeps_96_dpi_identity() {
    assert_eq!(scale_96_dpi_dimension(480, 96), 480);
    assert_eq!(scale_96_dpi_dimension(360, 96), 360);
}

#[test]
fn window_class_requests_native_double_click_messages() {
    assert_ne!(WINDOW_CLASS_STYLE & CS_DBLCLKS, 0);
}

#[test]
fn scale_96_dpi_dimension_expands_scaled_desktops() {
    assert_eq!(scale_96_dpi_dimension(480, 144), 720);
    assert_eq!(scale_96_dpi_dimension(360, 144), 540);
}

#[test]
fn scale_96_dpi_dimension_preserves_sentinels() {
    assert_eq!(scale_96_dpi_dimension(CW_USEDEFAULT, 144), CW_USEDEFAULT);
    assert_eq!(scale_96_dpi_dimension(0, 144), 0);
}

/// Wave C — Main default size is the fullscreen 1920×1080 fallback for
/// `bento-nano-app::startup_layout_viewport` sentinel use; the real
/// runtime rect comes from `main_window_rect()`.
#[test]
fn main_default_size_is_fullscreen_fallback() {
    assert_eq!(default_size(WindowKind::Main), (1920, 1080));
}

#[test]
fn icon_picker_default_is_a_compact_dialog_not_a_desktop_slab() {
    assert_eq!(default_size(WindowKind::IconPicker), (480, 640));
}

/// Wave C — Main HWND must be a borderless `WS_POPUP` transparent overlay.
/// Stripping `WS_OVERLAPPEDWINDOW` eliminates the caption / sysmenu /
/// resize border so the entire client area is a hit-testable canvas.
#[test]
fn main_ex_style_is_popup_overlay_no_caption() {
    let (ex, style) = ex_style_for(WindowKind::Main);
    assert!(
        ex & WS_EX_NOREDIRECTIONBITMAP != 0,
        "Main must keep NoRedirectionBitmap per spec §4.1"
    );
    assert!(
        style & WS_POPUP != 0,
        "Main must be a popup window (no caption / no sysmenu)"
    );
    // The legacy `WS_OVERLAPPEDWINDOW` is `WS_OVERLAPPED | WS_CAPTION |
    // WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX`.
    // Wave C requires those bits cleared so no chrome paints on top of
    // the transparent canvas.
    const NON_OVERLAY_BITS: u32 = WS_OVERLAPPEDWINDOW & !WS_POPUP;
    assert_eq!(
        style & NON_OVERLAY_BITS,
        0,
        "Main must not request caption/sysmenu/resize-border bits",
    );
}

/// W13-A (2026-07-13) — the Tauri benchmark uses `alwaysOnTop: false`.
/// Main must remain in normal z-order so regular applications cover the
/// desktop surface.
#[test]
fn main_ex_style_is_not_topmost() {
    let (ex, _) = ex_style_for(WindowKind::Main);
    assert_eq!(
        ex & WS_EX_TOPMOST,
        0,
        "Main must not cover foreground applications"
    );
}

/// W13-B — blank desktop click-through is owned by the exact Main HWND
/// region. WS_EX_TRANSPARENT would also discard input inside that region.
#[test]
fn main_ex_style_uses_region_instead_of_transparent_window() {
    let (ex, _) = ex_style_for(WindowKind::Main);
    assert_eq!(
        ex & WS_EX_TRANSPARENT,
        0,
        "Main must receive mouse input inside its installed chrome region"
    );
}

#[test]
fn context_menu_is_focusable_owned_toolwindow_and_starts_hidden() {
    let (ex, style) = ex_style_for(WindowKind::ContextMenu);
    assert_ne!(ex & WS_EX_NOREDIRECTIONBITMAP, 0);
    assert_ne!(ex & WS_EX_TOOLWINDOW, 0);
    assert_eq!(ex & WS_EX_TOPMOST, 0);
    assert_eq!(ex & WS_EX_NOACTIVATE, 0);
    assert_ne!(style & WS_POPUP, 0);
    assert_eq!(style & WS_VISIBLE, 0);
}

/// V-1 (TL ruling 2026-05-21) — Settings is a Tauri-style borderless modal:
/// panel paints its own header per frame_060, so the OS caption / border /
/// sysmenu must not be on the HWND. Bare `WS_POPUP` keeps DComp while
/// stripping every non-popup chrome bit; Settings is not globally topmost.
#[test]
fn settings_ex_style_is_borderless_non_topmost() {
    let (ex, style) = ex_style_for(WindowKind::Settings);
    assert!(
        ex & WS_EX_NOREDIRECTIONBITMAP != 0,
        "Settings must keep NoRedirectionBitmap (DComp panel chrome)"
    );
    assert_eq!(
        ex & WS_EX_TOPMOST,
        0,
        "Settings must be occludable by unrelated foreground apps"
    );
    assert!(
        style & WS_POPUP != 0,
        "Settings must be WS_POPUP (no native chrome)"
    );
    // V-1 — explicit borderless: no caption, no sysmenu, no border, no resize
    const NON_POPUP_BITS: u32 = WS_OVERLAPPEDWINDOW & !WS_POPUP;
    assert_eq!(
        style & NON_POPUP_BITS,
        0,
        "Settings must not request caption / sysmenu / border / resize bits",
    );
    assert_eq!(
        style & WS_VISIBLE,
        0,
        "Settings must not flash before centring and renderer creation"
    );
}

#[test]
fn search_ex_style_is_borderless_non_topmost_and_starts_hidden() {
    let (ex, style) = ex_style_for(WindowKind::Search);
    assert_ne!(ex & WS_EX_NOREDIRECTIONBITMAP, 0);
    assert_eq!(ex & WS_EX_TOPMOST, 0);
    assert_ne!(style & WS_POPUP, 0);
    assert_eq!(style & WS_VISIBLE, 0);
    const NON_POPUP_BITS: u32 = WS_OVERLAPPEDWINDOW & !WS_POPUP;
    assert_eq!(style & NON_POPUP_BITS, 0);
}

#[test]
fn zone_editor_ex_style_is_borderless_non_topmost_and_starts_hidden() {
    let (ex, style) = ex_style_for(WindowKind::ZoneEditor);
    assert_ne!(
        ex & WS_EX_NOREDIRECTIONBITMAP,
        0,
        "ZoneEditor must remain on the native DComp path"
    );
    assert_eq!(
        ex & WS_EX_TOPMOST,
        0,
        "ZoneEditor must be occludable by unrelated foreground apps"
    );
    assert_ne!(
        style & WS_POPUP,
        0,
        "ZoneEditor must use a borderless popup HWND"
    );
    assert_eq!(
        style & WS_VISIBLE,
        0,
        "ZoneEditor must not flash before centring and renderer creation"
    );
    const NON_POPUP_BITS: u32 = WS_OVERLAPPEDWINDOW & !WS_POPUP;
    assert_eq!(
        style & NON_POPUP_BITS,
        0,
        "ZoneEditor must not request caption / sysmenu / border / resize bits",
    );
}

#[test]
fn every_self_painted_aux_window_is_borderless_non_topmost_and_starts_hidden() {
    const SELF_PAINTED: [WindowKind; 13] = [
        WindowKind::IconPicker,
        WindowKind::CapsulePicker,
        WindowKind::Settings,
        WindowKind::About,
        WindowKind::PalettePicker,
        WindowKind::RulesWizard,
        WindowKind::BulkManager,
        WindowKind::ZoneEditor,
        WindowKind::ItemFileRename,
        WindowKind::Suggestor,
        WindowKind::Timeline,
        WindowKind::SnapshotPicker,
        WindowKind::Search,
    ];
    const NON_POPUP_BITS: u32 = WS_OVERLAPPEDWINDOW & !WS_POPUP;
    for kind in SELF_PAINTED {
        let (ex, style) = ex_style_for(kind);
        assert_ne!(
            ex & WS_EX_NOREDIRECTIONBITMAP,
            0,
            "{kind:?} must remain on the DComp path"
        );
        assert_eq!(
            ex & WS_EX_TOPMOST,
            0,
            "{kind:?} must not cover unrelated applications"
        );
        assert_ne!(style & WS_POPUP, 0, "{kind:?} must be borderless");
        assert_eq!(
            style & NON_POPUP_BITS,
            0,
            "{kind:?} must not request native caption/border bits"
        );
        assert_eq!(
            style & WS_VISIBLE,
            0,
            "{kind:?} must not flash before centring and renderer creation"
        );
    }
}

/// Wave C — `main_window_rect()` always reports a usable rectangle. In a
/// CI/headless test harness `MonitorFromPoint` may report a zero work
/// area; the function falls back to `default_size(Main)` so the shell
/// can still call `CreateWindowExW` with sane dimensions.
#[test]
fn main_window_rect_is_non_empty() {
    let (_x, _y, w, h) = main_window_rect();
    assert!(w > 0, "main window rect width must be positive");
    assert!(h > 0, "main window rect height must be positive");
}

#[test]
fn main_ex_style_is_non_activating_tool_window() {
    let (ex, _) = ex_style_for(WindowKind::Main);
    assert!(
        ex & WS_EX_TOOLWINDOW != 0,
        "Main must stay out of Alt-Tab and the taskbar"
    );
    assert!(
        ex & WS_EX_NOACTIVATE != 0,
        "Main must never steal foreground focus from regular applications"
    );
}

/// MiniBar uses the selected-stack DComp renderer. Requesting Layered here
/// would route it onto the legacy redirection path and violates the
/// NoRedirectionBitmap/Layered mutex in spec 4.1.
#[test]
fn minibar_ex_style_uses_dcomp_no_redirection_bitmap() {
    let (ex, style) = ex_style_for(WindowKind::MiniBar);
    assert!(
        ex & WS_EX_NOREDIRECTIONBITMAP != 0,
        "MiniBar must keep NoRedirectionBitmap for DComp rendering"
    );
    assert_eq!(
        ex & WS_EX_LAYERED,
        0,
        "MiniBar must not request WS_EX_LAYERED with the DComp renderer"
    );
    assert!(
        ex & WS_EX_TOPMOST != 0,
        "MiniBar must stay topmost as a pinned desktop affordance"
    );
    assert!(
        ex & WS_EX_NOACTIVATE != 0,
        "MiniBar must not steal focus when clicked"
    );
    assert!(
        style & WS_POPUP != 0,
        "MiniBar must be a popup window without native chrome"
    );
}
