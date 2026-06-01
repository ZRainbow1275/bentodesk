//! Settings overlay geometry (Wave K1 — Tauri 1.2.4 visual parity refactor;
//! Round-2 M1 adds the dark-shell on top of the K1 surface).
//!
//! ## Round-2 M1 (2026-05-21)
//!
//! Tauri 1.2.4 reference frame `frame_060.png` shows the Settings dialog as
//! a **dark, scrollable, vertically stacked** card with a sticky header and
//! a sticky `[Cancel] [Save]` footer. The Wave K1 light panel is being
//! retired surface-by-surface across M1→M4.
//!
//! - **M1** (this wave): dark shell, header + scrollable body + footer
//!   plumbing, top 5 toggle rows + 语言 row only. K1 row helpers remain in
//!   this module marked `#[allow(dead_code)]` so the existing `SettingsHit`
//!   dispatch arms in `bento-nano-shell` still link (Ruling B,
//!   orphan-alive); they no longer paint or hit-test.
//! - **M2** adds 桌面源 list + 桌面路径 input + 监控值 textarea.
//! - **M3** adds the 高级洗脑启动 / 磁吸 / 重叠版本 / 装备状态 mid-section.
//! - **M4** adds 应用更新 + 设置备份 + 设置加密 + 插件 inline and finally
//!   retires the K1 orphans (`#[allow(dead_code)]` must be empty by M4
//!   sign-off per Ruling 1).
//!
//! Each new helper here is a pure function of `(viewport, scroll_offset_y,
//! [index])` — no `AppState` reads inside the geometry layer (spec §10).
//!
//! Original K1 doc:
//!
//! Panel layout constants live here so both the renderer (`Renderer::
//! draw_settings_panel`) and the shell's hit-tester (`ui::settings_hit`)
//! agree on every rect without round-tripping through a depended-twice
//! crate. Keeping them in `bento-nano-app` (which both `bento-nano-shell`
//! and the renderer can see) avoids a dependency inversion.
//!
//! ## Wave K1 redesign (2026-05-20)
//!
//! Pre-K1 the panel was 440 × 330 with a dark `surface_dialog` background
//! and a tight 6-row text-button grid. The Tauri 1.2.4 baseline shows a
//! **vertical, light, translucent** panel with iOS-style rocker switches
//! for boolean rows + dropdown chips for enum rows.
//!
//! Geometry strategy:
//!   * Rect-helper **function names are preserved** so the hit-tester
//!     (`ui::settings_hit`) and the tooltip hit dispatcher stay byte-stable.
//!   * Each row is `SETTINGS_ROW_STEP_Y = 44` DIPs tall, indexed top-down.
//!   * Boolean rows (stealth / auto-download) render a 36 × 20 rocker on
//!     the right; the rect helper returns the **rocker hit-box** (38 × 24)
//!     so the click target stays Fitts-friendly even though the painted
//!     track is smaller.
//!   * Enum rows (encryption mode, locale, zone display mode, theme,
//!     update frequency) render a flat rounded-rect dropdown chip on the
//!     right that shows the current selection.
//!   * Vault row (backup / restore / recovery) compresses six small action
//!     chips into a single row — one row per backup file follows below.
//!   * Modal openers (keybindings, plugins) sit in their own row near the
//!     bottom (no longer in the header title-bar like pre-K1).
//!
//! Modal rects (plugins / keybindings) are untouched — only the main
//! Settings panel chrome was redesigned.

use bento_nano_style::{Rect, Shadow, Size, tokens as style_tokens};

/// Panel size in DIPs — 360×580 vertical card matching Tauri 1.2.4 frame_060.
///
/// The Settings HWND is 800 × 600 (see `bento_nano_platform::window::
/// default_size_for_kind`), so this panel still fits with margins after the
/// top-anchor offset.
pub const SETTINGS_PANEL_WIDTH: f32 = 360.0;
/// Actual panel height; vertical card hosts header + 10 section rows + action
/// row + bottom close button.
pub const SETTINGS_PANEL_HEIGHT: f32 = 560.0;
/// Inner padding around the panel chrome.
pub const SETTINGS_PANEL_PADDING: f32 = 16.0;

/// First section row Y inside the panel (relative to panel origin).
/// Below this is the header band (title + close icon + divider).
pub const SETTINGS_ROW_START_Y: f32 = 56.0;
/// Distance between consecutive section row top edges.
pub const SETTINGS_ROW_STEP_Y: f32 = 44.0;
/// Each section row's visible height (also the dropdown chip height + 4 DIP
/// vertical breathing room). Reused as the hit-box for boolean rows.
pub const SETTINGS_ROW_H: f32 = 40.0;

/// Header band height (title + close + bottom hairline).
pub const SETTINGS_HEADER_H: f32 = 48.0;

/// Wave K1 — iOS-style rocker switch track size (38×22 DIPs).
pub const SETTINGS_TOGGLE_TRACK_W: f32 = 38.0;
pub const SETTINGS_TOGGLE_TRACK_H: f32 = 22.0;
/// Knob diameter — 16 DIPs at off, slides 16 DIPs on the on edge.
pub const SETTINGS_TOGGLE_KNOB_D: f32 = 16.0;

/// Wave K1 — Hit-box for a toggle row "control" position. Larger than the
/// painted track so the click target is comfortable.
pub const SETTINGS_SWITCH_BTN_W: f32 = 60.0;
pub const SETTINGS_SWITCH_BTN_H: f32 = 28.0;

/// Wave K1 — Dropdown chip dimensions (label + chevron, flat rounded rect).
pub const SETTINGS_DROPDOWN_CHIP_W: f32 = 130.0;
pub const SETTINGS_DROPDOWN_CHIP_H: f32 = 28.0;
/// Wave K1 — Width of the chevron column inside the dropdown chip.
pub const SETTINGS_DROPDOWN_CHEVRON_W: f32 = 14.0;

/// Updater row inline action buttons (Check / Run / Skip).
pub const SETTINGS_UPDATE_ACTION_BTN_W: f32 = 56.0;
pub const SETTINGS_UPDATE_SKIP_BTN_W: f32 = 46.0;
pub const SETTINGS_UPDATE_ACTION_GAP: f32 = 6.0;

/// Theme row inline buttons (Import) + the theme-base swatch.
pub const SETTINGS_THEME_IMPORT_BTN_W: f32 = 50.0;
pub const SETTINGS_ACTIVE_THEME_BTN_W: f32 = SETTINGS_DROPDOWN_CHIP_W;
/// Theme-base accent swatch (square indicator).
pub const SETTINGS_THEME_BASE_SWATCH_W: f32 = 28.0;

/// Zone display-mode dropdown chip — sized identically to other enum chips
/// so the panel reads as a clean vertical column.
pub const SETTINGS_ZONE_DISPLAY_MODE_BTN_W: f32 = SETTINGS_DROPDOWN_CHIP_W;

/// Vault row — six small action chips packed across one row.
pub const SETTINGS_BACKUP_BTN_W: f32 = 48.0;
pub const SETTINGS_BACKUP_BTN_GAP: f32 = 4.0;
pub const SETTINGS_BACKUP_ENTRY_VISIBLE_MAX: usize = 3;
pub const SETTINGS_BACKUP_ENTRY_H: f32 = 18.0;
pub const SETTINGS_BACKUP_ENTRY_GAP: f32 = 6.0;

// M1h (2026-05-29) — the pre-K1 plugins modal-opener button constants
// (`SETTINGS_PLUGINS_BTN_W/H`) were removed with `settings_plugins_open_rect`
// when the Plugins surface moved inline. The keybindings opener uses its own
// `SETTINGS_KEYBINDINGS_BTN_W/H` below and is unaffected.

/// M1h (2026-05-29) — the plugin lifecycle MODAL geometry constants
/// (`SETTINGS_PLUGINS_MODAL_W/H`, `_ROW_START_Y/STEP_Y/H`, `_INSTALL/REFRESH/
/// TOGGLE/REMOVE/CLOSE_BTN_W`, `_BTN_GAP`) were removed: the Plugins surface is
/// now an inline §11 section (see the M1h block lower in this file). The
/// visible-row cap survives — it still bounds the inline plugin-card list so
/// paint / hit / scroll agree.
pub const SETTINGS_PLUGINS_ROW_VISIBLE_MAX: usize = 5;

/// Keybindings modal geometry (untouched by K1).
pub const SETTINGS_KEYBINDINGS_BTN_W: f32 = 64.0;
pub const SETTINGS_KEYBINDINGS_BTN_H: f32 = SETTINGS_SWITCH_BTN_H;
pub const SETTINGS_KEYBINDINGS_MODAL_W: f32 = 420.0;
pub const SETTINGS_KEYBINDINGS_MODAL_H: f32 = 310.0;
pub const SETTINGS_KEYBINDINGS_ROW_START_Y: f32 = 44.0;
pub const SETTINGS_KEYBINDINGS_ROW_STEP_Y: f32 = 26.0;
pub const SETTINGS_KEYBINDINGS_ROW_H: f32 = 26.0;
pub const SETTINGS_KEYBINDINGS_RECORD_BTN_W: f32 = 64.0;
pub const SETTINGS_KEYBINDINGS_RESET_BTN_W: f32 = 54.0;
pub const SETTINGS_KEYBINDINGS_BTN_GAP: f32 = 6.0;
pub const SETTINGS_KEYBINDINGS_CLOSE_BTN_W: f32 = 32.0;
pub const SETTINGS_KEYBINDINGS_CLOSE_BTN_H: f32 = 24.0;

/// Header close-icon (X) hit-box — top-right of the panel chrome.
pub const SETTINGS_CLOSE_BTN_W: f32 = 32.0;
pub const SETTINGS_CLOSE_BTN_H: f32 = 28.0;

/// RC-5 Gap B — y-anchor for the Settings panel inside its HWND.
///
/// Wave F shipped a centred panel, which on the 1200×900 host HWND left a
/// ~190 DIP modal-scrim void between the OS title bar and the panel chrome
/// (`y = (900-330)/2 = 285`). Users read that void as "the panel got lost"
/// rather than "centered modal".
///
/// Switching to a top-anchored offset (one `SPACING.lg` below the title
/// bar — the same 16 DIP gap the dialog body uses everywhere else) pulls
/// the panel up to the chrome edge so it reads as deliberately anchored.
pub const SETTINGS_PANEL_TOP_MARGIN: f32 = style_tokens::SPACING.lg;

// =============================================================================
// Row index map — section rows from top to bottom inside the panel chrome.
//
// Helper rects below reuse this index so adding a row only touches the index
// constants. The hit-tester picks rects by name, not by index.
// =============================================================================

const ROW_INDEX_STEALTH: u32 = 0;
const ROW_INDEX_UPDATE_AUTO: u32 = 1;
const ROW_INDEX_ENCRYPTION: u32 = 2;
const ROW_INDEX_LOCALE: u32 = 3;
const ROW_INDEX_ZONE_DISPLAY: u32 = 4;
const ROW_INDEX_THEME: u32 = 5;
const ROW_INDEX_UPDATER: u32 = 6;
const ROW_INDEX_VAULT: u32 = 7;
const ROW_INDEX_MODALS: u32 = 8;

/// Number of section rows in the main panel column (excludes the backup
/// entry strip below `ROW_INDEX_VAULT`). Used by the renderer to compute
/// where the close button can sit without overlapping content.
pub const SETTINGS_SECTION_ROW_COUNT: u32 = 9;

/// Compute the panel's absolute rect for `viewport`. X is centred; Y
/// anchors to the top of the HWND with [`SETTINGS_PANEL_TOP_MARGIN`] of
/// headroom (RC-5 Gap B). Both axes saturate to `0.0` when the viewport
/// is too small to host the panel + margin.
pub fn settings_panel_rect(viewport: Size) -> Rect {
    let x = ((viewport.width - SETTINGS_PANEL_WIDTH) * 0.5).max(0.0);
    let y = if viewport.height >= SETTINGS_PANEL_HEIGHT + SETTINGS_PANEL_TOP_MARGIN {
        SETTINGS_PANEL_TOP_MARGIN
    } else {
        0.0
    };
    Rect {
        x,
        y,
        width: SETTINGS_PANEL_WIDTH,
        height: SETTINGS_PANEL_HEIGHT,
    }
}

/// Approximate the Settings panel drop-shadow bounds from active theme tokens.
pub fn settings_panel_shadow_rect(panel: Rect, shadow: Shadow) -> Rect {
    let spread = shadow.blur.max(0.0);
    Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    }
}

/// Header close icon (X) rect — top-right of the panel chrome.
///
/// Pre-K1 the close button lived at the bottom-centre of the panel; the
/// Tauri 1.2.4 baseline anchors it inside the header band so users can
/// dismiss the panel without scrolling past the row stack.
pub fn settings_close_button_rect(viewport: Size) -> Rect {
    let p = settings_panel_rect(viewport);
    Rect {
        x: p.x + p.width - SETTINGS_PANEL_PADDING - SETTINGS_CLOSE_BTN_W,
        y: p.y + 10.0,
        width: SETTINGS_CLOSE_BTN_W,
        height: SETTINGS_CLOSE_BTN_H,
    }
}

/// Compute the absolute Y of section row `index` inside the panel chrome.
fn section_row_y(viewport: Size, index: u32) -> f32 {
    let p = settings_panel_rect(viewport);
    p.y + SETTINGS_ROW_START_Y + SETTINGS_ROW_STEP_Y * index as f32
}

/// Compute the right-anchored control rect for section row `index` using
/// the supplied control width.
fn section_row_control_rect(viewport: Size, index: u32, control_w: f32, control_h: f32) -> Rect {
    let p = settings_panel_rect(viewport);
    let row_y = section_row_y(viewport, index);
    let y_offset = (SETTINGS_ROW_H - control_h) * 0.5;
    Rect {
        x: p.x + p.width - SETTINGS_PANEL_PADDING - control_w,
        y: row_y + y_offset,
        width: control_w,
        height: control_h,
    }
}

/// Full row rect (panel-wide minus padding) — used by the renderer for the
/// label area on the left half of each section row.
pub fn settings_section_row_rect(viewport: Size, index: u32) -> Rect {
    let p = settings_panel_rect(viewport);
    Rect {
        x: p.x + SETTINGS_PANEL_PADDING,
        y: section_row_y(viewport, index),
        width: p.width - SETTINGS_PANEL_PADDING * 2.0,
        height: SETTINGS_ROW_H,
    }
}

/// Title-bar keybindings button — Wave K1 moves it into the modal-openers
/// row at the bottom of the column. Width matches the plugin opener so the
/// two read as a paired action group.
pub fn settings_keybindings_open_rect(viewport: Size) -> Rect {
    let row = settings_section_row_rect(viewport, ROW_INDEX_MODALS);
    Rect {
        x: row.x,
        y: row.y + (SETTINGS_ROW_H - SETTINGS_KEYBINDINGS_BTN_H) * 0.5,
        width: SETTINGS_KEYBINDINGS_BTN_W,
        height: SETTINGS_KEYBINDINGS_BTN_H,
    }
}

// M1h (2026-05-29) — `settings_plugins_open_rect` (the K1 modal-opener button
// that sat next to the keybindings opener) was removed: Tauri has no "open
// plugins" affordance — the Plugins §11 section is always inline in the
// scrollable body. The keybindings opener is unaffected.

// =============================================================================
// Boolean toggle rows — return the rocker-switch hit-box.
// =============================================================================

/// Locale dropdown chip rect (Wave K1 — re-uses the legacy "switch button"
/// name so the hit-tester does not need to change). The chip sits in the
/// language row and reads as `[中文 ▾]` or `[English ▾]` at runtime.
pub fn settings_switch_button_rect(viewport: Size) -> Rect {
    section_row_control_rect(
        viewport,
        ROW_INDEX_LOCALE,
        SETTINGS_DROPDOWN_CHIP_W,
        SETTINGS_DROPDOWN_CHIP_H,
    )
}

/// Stealth-enabled rocker rect.
pub fn settings_stealth_enabled_rect(viewport: Size) -> Rect {
    section_row_control_rect(
        viewport,
        ROW_INDEX_STEALTH,
        SETTINGS_SWITCH_BTN_W,
        SETTINGS_SWITCH_BTN_H,
    )
}

/// Updater auto-download rocker rect.
pub fn settings_update_auto_download_rect(viewport: Size) -> Rect {
    section_row_control_rect(
        viewport,
        ROW_INDEX_UPDATE_AUTO,
        SETTINGS_SWITCH_BTN_W,
        SETTINGS_SWITCH_BTN_H,
    )
}

// =============================================================================
// Enum/dropdown rows — chip on the right of the row.
// =============================================================================

/// Config-vault encryption mode dropdown chip.
pub fn settings_encryption_mode_rect(viewport: Size) -> Rect {
    section_row_control_rect(
        viewport,
        ROW_INDEX_ENCRYPTION,
        SETTINGS_DROPDOWN_CHIP_W,
        SETTINGS_DROPDOWN_CHIP_H,
    )
}

/// Zone display-mode dropdown chip.
pub fn settings_zone_display_mode_rect(viewport: Size) -> Rect {
    section_row_control_rect(
        viewport,
        ROW_INDEX_ZONE_DISPLAY,
        SETTINGS_ZONE_DISPLAY_MODE_BTN_W,
        SETTINGS_DROPDOWN_CHIP_H,
    )
}

/// Update frequency dropdown chip.
pub fn settings_update_frequency_rect(viewport: Size) -> Rect {
    section_row_control_rect(
        viewport,
        ROW_INDEX_UPDATER,
        SETTINGS_DROPDOWN_CHIP_W,
        SETTINGS_DROPDOWN_CHIP_H,
    )
}

/// Updater "Check now" inline action button — sits to the left of the
/// frequency dropdown chip.
pub fn settings_update_check_now_rect(viewport: Size) -> Rect {
    let freq = settings_update_frequency_rect(viewport);
    Rect {
        x: freq.x - SETTINGS_UPDATE_ACTION_GAP - SETTINGS_UPDATE_ACTION_BTN_W,
        y: freq.y,
        width: SETTINGS_UPDATE_ACTION_BTN_W,
        height: freq.height,
    }
}

/// Updater stateful action button (Download/Install/Wait) — to the left of
/// the Check button.
pub fn settings_update_action_rect(viewport: Size) -> Rect {
    let check = settings_update_check_now_rect(viewport);
    Rect {
        x: check.x - SETTINGS_UPDATE_ACTION_GAP - SETTINGS_UPDATE_ACTION_BTN_W,
        y: check.y,
        width: SETTINGS_UPDATE_ACTION_BTN_W,
        height: check.height,
    }
}

/// Updater skip-current-version button — to the left of the action button.
pub fn settings_update_skip_rect(viewport: Size) -> Rect {
    let action = settings_update_action_rect(viewport);
    Rect {
        x: action.x - SETTINGS_UPDATE_ACTION_GAP - SETTINGS_UPDATE_SKIP_BTN_W,
        y: action.y,
        width: SETTINGS_UPDATE_SKIP_BTN_W,
        height: action.height,
    }
}

// =============================================================================
// Theme row — dropdown chip + import action + accent swatch.
// =============================================================================

/// Active theme dropdown chip (right edge of the theme row).
pub fn settings_active_theme_rect(viewport: Size) -> Rect {
    section_row_control_rect(
        viewport,
        ROW_INDEX_THEME,
        SETTINGS_ACTIVE_THEME_BTN_W,
        SETTINGS_DROPDOWN_CHIP_H,
    )
}

/// Theme JSON import action — to the left of the active-theme chip.
pub fn settings_theme_import_rect(viewport: Size) -> Rect {
    let active = settings_active_theme_rect(viewport);
    Rect {
        x: active.x - SETTINGS_UPDATE_ACTION_GAP - SETTINGS_THEME_IMPORT_BTN_W,
        y: active.y,
        width: SETTINGS_THEME_IMPORT_BTN_W,
        height: active.height,
    }
}

/// Theme-base accent swatch — to the left of the import action.
pub fn settings_theme_base_rect(viewport: Size) -> Rect {
    let import = settings_theme_import_rect(viewport);
    Rect {
        x: import.x - SETTINGS_UPDATE_ACTION_GAP - SETTINGS_THEME_BASE_SWATCH_W,
        y: import.y,
        width: SETTINGS_THEME_BASE_SWATCH_W,
        height: import.height,
    }
}

// =============================================================================
// Vault row — six small action chips packed across the row.
// =============================================================================

fn settings_backup_action_rect(viewport: Size, button_index: u32) -> Rect {
    let p = settings_panel_rect(viewport);
    let row_y = section_row_y(viewport, ROW_INDEX_VAULT);
    let total_w = SETTINGS_BACKUP_BTN_W * 6.0 + SETTINGS_BACKUP_BTN_GAP * 5.0;
    let chip_h = SETTINGS_DROPDOWN_CHIP_H;
    let y_offset = (SETTINGS_ROW_H - chip_h) * 0.5;
    Rect {
        x: p.x + p.width - SETTINGS_PANEL_PADDING - total_w
            + (SETTINGS_BACKUP_BTN_W + SETTINGS_BACKUP_BTN_GAP) * button_index as f32,
        y: row_y + y_offset,
        width: SETTINGS_BACKUP_BTN_W,
        height: chip_h,
    }
}

pub fn settings_backup_now_rect(viewport: Size) -> Rect {
    settings_backup_action_rect(viewport, 0)
}
pub fn settings_backup_list_rect(viewport: Size) -> Rect {
    settings_backup_action_rect(viewport, 1)
}
pub fn settings_backup_restore_rect(viewport: Size) -> Rect {
    settings_backup_action_rect(viewport, 2)
}
pub fn settings_recovery_create_rect(viewport: Size) -> Rect {
    settings_backup_action_rect(viewport, 3)
}
pub fn settings_recovery_diagnostics_rect(viewport: Size) -> Rect {
    settings_backup_action_rect(viewport, 4)
}
pub fn settings_recovery_restore_rect(viewport: Size) -> Rect {
    settings_backup_action_rect(viewport, 5)
}

/// Visible settings backup entry rect — sits under the vault row, three
/// across, before the modal-openers row.
pub fn settings_backup_entry_rect(viewport: Size, entry_index: usize) -> Rect {
    let p = settings_panel_rect(viewport);
    let total_gap = SETTINGS_BACKUP_ENTRY_GAP * (SETTINGS_BACKUP_ENTRY_VISIBLE_MAX as f32 - 1.0);
    let entry_w = (p.width - SETTINGS_PANEL_PADDING * 2.0 - total_gap)
        / SETTINGS_BACKUP_ENTRY_VISIBLE_MAX as f32;
    let vault = settings_backup_now_rect(viewport);
    Rect {
        x: p.x
            + SETTINGS_PANEL_PADDING
            + (entry_w + SETTINGS_BACKUP_ENTRY_GAP) * entry_index as f32,
        y: vault.bottom() + 4.0,
        width: entry_w,
        height: SETTINGS_BACKUP_ENTRY_H,
    }
}

// =============================================================================
// Modal rects — untouched by Wave K1.
// =============================================================================

pub fn settings_keybindings_modal_rect(viewport: Size) -> Rect {
    Rect {
        x: ((viewport.width - SETTINGS_KEYBINDINGS_MODAL_W) * 0.5).max(0.0),
        y: ((viewport.height - SETTINGS_KEYBINDINGS_MODAL_H) * 0.5).max(0.0),
        width: SETTINGS_KEYBINDINGS_MODAL_W,
        height: SETTINGS_KEYBINDINGS_MODAL_H,
    }
}

// M1h (2026-05-29) — the plugin lifecycle MODAL geometry
// (`settings_plugins_modal_rect` / `_close_rect` / `_refresh_rect` /
// `_install_rect` / `settings_plugin_row_rect` / `_toggle_rect` /
// `_uninstall_rect`) was removed: the Plugins surface is now an inline §11
// section of the scrollable Settings body (Tauri parity — `SettingsPanel.tsx:
// 709-781`). The inline geometry lives in the M1h block alongside the Backup
// §9 helpers below (`settings_plugins_label_rect` … `settings_plugins_content_height`).

pub fn settings_keybinding_row_rect(viewport: Size, row_index: usize) -> Rect {
    let modal = settings_keybindings_modal_rect(viewport);
    Rect {
        x: modal.x + SETTINGS_PANEL_PADDING,
        y: modal.y
            + SETTINGS_KEYBINDINGS_ROW_START_Y
            + SETTINGS_KEYBINDINGS_ROW_STEP_Y * row_index as f32,
        width: modal.width - SETTINGS_PANEL_PADDING * 2.0,
        height: SETTINGS_KEYBINDINGS_ROW_H,
    }
}

pub fn settings_keybinding_record_rect(viewport: Size, row_index: usize) -> Rect {
    let row = settings_keybinding_row_rect(viewport, row_index);
    Rect {
        x: row.right()
            - SETTINGS_KEYBINDINGS_RESET_BTN_W
            - SETTINGS_KEYBINDINGS_BTN_GAP
            - SETTINGS_KEYBINDINGS_RECORD_BTN_W,
        y: row.y + 2.0,
        width: SETTINGS_KEYBINDINGS_RECORD_BTN_W,
        height: SETTINGS_SWITCH_BTN_H,
    }
}

pub fn settings_keybinding_reset_rect(viewport: Size, row_index: usize) -> Rect {
    let row = settings_keybinding_row_rect(viewport, row_index);
    Rect {
        x: row.right() - SETTINGS_KEYBINDINGS_RESET_BTN_W,
        y: row.y + 2.0,
        width: SETTINGS_KEYBINDINGS_RESET_BTN_W,
        height: SETTINGS_SWITCH_BTN_H,
    }
}

pub fn settings_keybindings_close_rect(viewport: Size) -> Rect {
    let modal = settings_keybindings_modal_rect(viewport);
    Rect {
        x: modal.x + modal.width - SETTINGS_PANEL_PADDING - SETTINGS_KEYBINDINGS_CLOSE_BTN_W,
        y: modal.y + 12.0,
        width: SETTINGS_KEYBINDINGS_CLOSE_BTN_W,
        height: SETTINGS_KEYBINDINGS_CLOSE_BTN_H,
    }
}

// =============================================================================
// Round-2 M1 — Dark Settings shell.
//
// Tauri 1.2.4 `frame_060.png` baseline. The Wave K1 light panel above is
// kept alive as orphan (Ruling B) — render.rs no longer paints those rows
// but their rect helpers stay so the dispatch graph compiles.
// =============================================================================

/// Round-2 M1 — panel width in DIPs. TL Ruling 5 2026-05-21: 400→420 so the
/// panel reads ≥420 DIP across all DPI (was 400; with the K1 paint-leak fixed
/// the visual perception of "tiny" came from leak debris, not real geometry,
/// but a 20-DIP bump still leaves room for M3 slider track + value chip pairs).
pub const SETTINGS_PANEL_WIDTH_M1: f32 = 480.0;
/// Round-2 M1 — maximum panel height. TL A-path 2026-05-21: 700→580 so the
/// modal fits inside the 800×600 Settings aux HWND client area with breathing
/// room around the 8-DIP drop shadow. Smaller viewports still clamp via the
/// `min(available_h)` in `settings_panel_rect_m1`.
pub const SETTINGS_PANEL_HEIGHT_MAX: f32 = 580.0;
/// Round-2 M1 — panel corner radius. Tauri `--radius-expanded: 16px`
/// (`SettingsPanel.css`); M1b parity bump 14→16 (was 14 per frame_060).
pub const SETTINGS_PANEL_RADIUS: f32 = 16.0;
/// V-5 (TL re-issue 2026-05-21) — alpha for the 8-DIP outer drop-shadow ring
/// painted around the panel. The shadow is a hard-edged `fill_rounded_rect`
/// (no D2D gaussian blur on the hot path per spec §10) so ANY non-zero alpha
/// reads as a visible halo / "mask ring" against the wallpaper — Tauri 1.2.4
/// achieves the lifted-modal look via CSS gaussian blur which has no D2D
/// equivalent on our render path. Re-issued V-5 contract: "panel 外只露
/// 桌面 wallpaper, 不出现任何 BentoDesk-painted overlay 圈" → alpha = 0.0.
/// `fill_rounded_rect` in `render.rs` early-returns at `color.a <= 0.0`, so
/// this also short-circuits the paint call (zero allocation, zero D2D cost).
/// Re-introducing a non-zero value would resurrect the mask-ring regression;
/// the unit test below pins this at exactly 0.0. Pre-fix v1 was 0.45, v2 was
/// 0.15 (still produced a faint visible ring per V-5 re-audit).
pub const SETTINGS_PANEL_SHADOW_ALPHA: f32 = 0.0;
/// Round-2 M1 — sticky header band height (title + close ×).
pub const SETTINGS_HEADER_H_M1: f32 = 48.0;
/// Round-2 M1 — sticky footer band height ([取消] [保存]).
pub const SETTINGS_FOOTER_H: f32 = 56.0;
/// Round-2 M1 — single row height in the scrollable body.
pub const SETTINGS_ROW_H_M1: f32 = 44.0;
/// Round-2 M1 — horizontal padding inside body rows.
pub const SETTINGS_ROW_PAD_X: f32 = 20.0;
/// Round-2 M1 — vertical gap between logical sections inside the body.
pub const SETTINGS_SECTION_GAP: f32 = 24.0;
/// Round-2 M1 — top-toggle track width (matches the Wave K1 toggle, 38 DIP).
pub const SETTINGS_TOP_TOGGLE_HIT_W: f32 = 60.0;
/// Round-2 M1 — top-toggle row right-anchored hit-box height.
pub const SETTINGS_TOP_TOGGLE_HIT_H: f32 = 28.0;
/// Round-2 M1 — language dropdown chip width.
pub const SETTINGS_LANGUAGE_CHIP_W: f32 = 130.0;
/// Round-2 M1 — language dropdown chip height.
pub const SETTINGS_LANGUAGE_CHIP_H: f32 = 28.0;
/// Round-2 M1 — chevron column width inside the language chip.
pub const SETTINGS_LANGUAGE_CHEVRON_W: f32 = 14.0;
/// Round-2 M1 — header close-× hit-box size (square).
pub const SETTINGS_CLOSE_X_SIZE: f32 = 28.0;
/// Round-2 M1 — footer button width (Cancel + Save share a width).
pub const SETTINGS_FOOTER_BTN_W: f32 = 84.0;
/// Round-2 M1 — footer button height.
pub const SETTINGS_FOOTER_BTN_H: f32 = 32.0;
/// Round-2 M1 — gap between Cancel and Save in the footer.
pub const SETTINGS_FOOTER_BTN_GAP: f32 = 8.0;

/// Round-2 M1 — number of top-section toggle rows (5 toggles + 1 language
/// row living inside the same logical section). Pinned by a test below.
pub const SETTINGS_TOP_TOGGLE_COUNT: u8 = 5;

/// α4 (Wave I-α, 2026-05-25) — zone-display-mode picker row geometry.
/// Tauri 1.2.4 baseline (`SettingsPanel.tsx:555-595`) renders a 3-radio
/// horizontal group (Hover / Always / Click) immediately below the language
/// row. Each radio is a 12-DIP outer circle plus a 6-DIP inner dot when
/// selected, alongside an inline label. Three radios, two inter-radio gaps,
/// and leading/trailing padding pack into the right-anchored ~260-DIP
/// cluster, matching the language-chip horizontal anchor for vertical
/// alignment.
///
/// Picker row sits between the language row (M1) and the M2 sources
/// section; `settings_m2_origin_y_offset` is bumped by one row to clear it.
/// Number of choices in the zone-display-mode picker.
pub const SETTINGS_ZONE_DISPLAY_MODE_COUNT: u8 = 3;
/// Outer circle diameter (DIP) of one radio.
pub const SETTINGS_RADIO_OUTER_D: f32 = 14.0;
/// Inner dot diameter (DIP) when a radio is selected.
pub const SETTINGS_RADIO_INNER_D: f32 = 6.0;
/// Per-radio hit-box width (outer circle + 4-DIP gap + label).
pub const SETTINGS_RADIO_W: f32 = 78.0;
/// Per-radio hit-box height (matches the language-chip height so the row
/// reads as a single horizontal control band).
pub const SETTINGS_RADIO_H: f32 = 28.0;
/// Horizontal gap between adjacent radios.
pub const SETTINGS_RADIO_GAP: f32 = 4.0;

/// Round-2 M2 — height of a section label band (the dim header text above
/// 桌面源 / 桌面路径 / 监控值).
pub const SETTINGS_SECTION_LABEL_H: f32 = 24.0;

/// Round-2 M2 / M1i fidelity (2026-05-29) — height of one 桌面源 card row.
/// Tauri `.desktop-source-card` is `padding: 8px 10px` around a 28-DIP icon /
/// two-line body (13 + 11 px text + 2 px internal gap). 8 (top pad) + 28
/// (icon, the tallest flex child) + 8 (bottom pad) = 44.
pub const SETTINGS_SOURCE_ROW_H: f32 = 44.0;

/// M1i 2026-05-29 — number of source-card slots the §2 Paths layout caps the
/// list at (and the max cards the renderer paints). The list is dynamic, fed
/// from `desktop_sources::all_desktop_dirs`; the realistic Windows ceiling is
/// User + Public + OneDrive + Custom = 4. M1i fidelity fix — the list now
/// reflows to the LIVE count (Tauri's flex column): every section below it
/// shifts up/down by the height of the missing/extra cards. The renderer
/// paints `min(live_count, MAX)` cards. Mirrors the visible-cap rhythm of
/// [`SETTINGS_PLUGINS_ROW_VISIBLE_MAX`] / [`SETTINGS_BACKUP_ROW_VISIBLE_MAX`].
pub const SETTINGS_SOURCE_ROW_VISIBLE_MAX: u8 = 4;

/// Round-2 M2 / M1i fidelity — vertical gap between two source cards. Tauri
/// `.desktop-source-list { gap: 6px }`.
pub const SETTINGS_SOURCE_GAP: f32 = 6.0;

/// M1i fidelity — the `.desktop-source-empty` placeholder height (one italic
/// 11-px line, `padding: 6px 4px`). 6 + ~12 (line box) + 6 = 24.
pub const SETTINGS_SOURCE_EMPTY_H: f32 = 24.0;

/// M1i fidelity — the `.desktop-source-refresh` button row. The button is the
/// LAST child of the list (`align-self: flex-end`), below the cards. Tauri
/// `padding: 4px 10px; min-width: 32px; font-size: 14px`. 4 + ~16 + 4 = 24
/// tall.
pub const SETTINGS_SOURCE_REFRESH_BTN_W: f32 = 36.0;
pub const SETTINGS_SOURCE_REFRESH_BTN_H: f32 = 24.0;
/// M1i fidelity — gap between the last card (or empty placeholder) and the
/// right-anchored refresh button.
pub const SETTINGS_SOURCE_REFRESH_GAP: f32 = 6.0;

/// Round-2 M2 — text input row (single-line) used by 桌面路径.
pub const SETTINGS_INPUT_ROW_H: f32 = 40.0;

/// Round-2 M2 — multi-line textarea used by 监控值. 4 visible lines worth
/// of dark surface — enough to hint at scroll without taking over.
pub const SETTINGS_TEXTAREA_H: f32 = 96.0;

/// Round-2 M1 — logical sections inside the Settings dialog body.
///
/// M1 only paints `TopToggles + Language`; M2/M3/M4 add the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSectionId {
    /// Top 5 toggle rows: desktop_embed / autostart / show_in_taskbar /
    /// smart_layout / speed_mode.
    TopToggles,
    /// Language dropdown row (语言 / Language [中文 ▾]).
    Language,
}

/// Round-2 M1 — compute the dark Settings panel rect for the supplied
/// viewport. Centred horizontally, top-anchored with
/// [`SETTINGS_PANEL_TOP_MARGIN`] headroom, clamps to the available height.
pub fn settings_panel_rect_m1(viewport: Size) -> Rect {
    let panel_w = SETTINGS_PANEL_WIDTH_M1.min(viewport.width);
    let available_h = (viewport.height - SETTINGS_PANEL_TOP_MARGIN * 2.0).max(0.0);
    let panel_h = SETTINGS_PANEL_HEIGHT_MAX.min(available_h);
    Rect {
        x: ((viewport.width - panel_w) * 0.5).max(0.0),
        y: SETTINGS_PANEL_TOP_MARGIN,
        width: panel_w,
        height: panel_h,
    }
}

/// Round-2 M1 — sticky header band (title + close ×, NOT scrolled).
pub fn settings_header_rect(viewport: Size) -> Rect {
    let p = settings_panel_rect_m1(viewport);
    Rect {
        x: p.x,
        y: p.y,
        width: p.width,
        height: SETTINGS_HEADER_H_M1,
    }
}

/// Round-2 M1 — sticky footer band (Cancel + Save, NOT scrolled).
pub fn settings_footer_rect(viewport: Size) -> Rect {
    let p = settings_panel_rect_m1(viewport);
    Rect {
        x: p.x,
        y: p.bottom() - SETTINGS_FOOTER_H,
        width: p.width,
        height: SETTINGS_FOOTER_H,
    }
}

/// Round-2 M1 — scroll-clipped body rect (between header and footer).
pub fn settings_body_rect(viewport: Size) -> Rect {
    let p = settings_panel_rect_m1(viewport);
    let body_top = p.y + SETTINGS_HEADER_H_M1;
    let body_bottom = p.bottom() - SETTINGS_FOOTER_H;
    Rect {
        x: p.x,
        y: body_top,
        width: p.width,
        height: (body_bottom - body_top).max(0.0),
    }
}

/// Round-2 M1 — header close-× hit-box (sticky, NOT scrolled).
pub fn settings_close_button_rect_m1(viewport: Size) -> Rect {
    let header = settings_header_rect(viewport);
    Rect {
        x: header.right() - SETTINGS_ROW_PAD_X - SETTINGS_CLOSE_X_SIZE,
        y: header.y + (header.height - SETTINGS_CLOSE_X_SIZE) * 0.5,
        width: SETTINGS_CLOSE_X_SIZE,
        height: SETTINGS_CLOSE_X_SIZE,
    }
}

/// Round-2 M1 — footer Save button (right-aligned, accent fill).
pub fn settings_save_button_rect(viewport: Size) -> Rect {
    let footer = settings_footer_rect(viewport);
    Rect {
        x: footer.right() - SETTINGS_ROW_PAD_X - SETTINGS_FOOTER_BTN_W,
        y: footer.y + (footer.height - SETTINGS_FOOTER_BTN_H) * 0.5,
        width: SETTINGS_FOOTER_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// Round-2 M1 — footer Cancel button (left of Save).
pub fn settings_cancel_button_rect(viewport: Size) -> Rect {
    let save = settings_save_button_rect(viewport);
    Rect {
        x: save.x - SETTINGS_FOOTER_BTN_GAP - SETTINGS_FOOTER_BTN_W,
        y: save.y,
        width: SETTINGS_FOOTER_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// Round-2 M1 — content-space origin for body content. Subtract this from
/// the absolute paint Y of a body row to get its position in scroll space.
fn settings_body_content_origin(viewport: Size, scroll_offset_y: f32) -> f32 {
    settings_body_rect(viewport).y - scroll_offset_y
}

/// Round-2 M1 — full row rect for the top-section toggle at `index`
/// (`0..SETTINGS_TOP_TOGGLE_COUNT`). Honours scroll offset.
pub fn settings_top_toggle_row_rect(viewport: Size, scroll_offset_y: f32, index: u8) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + SETTINGS_ROW_H_M1 * index as f32,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_ROW_H_M1,
    }
}

/// Round-2 M1 — right-anchored toggle hit-box inside a top-section row.
pub fn settings_top_toggle_hit_rect(viewport: Size, scroll_offset_y: f32, index: u8) -> Rect {
    let row = settings_top_toggle_row_rect(viewport, scroll_offset_y, index);
    Rect {
        x: row.right() - SETTINGS_TOP_TOGGLE_HIT_W,
        y: row.y + (row.height - SETTINGS_TOP_TOGGLE_HIT_H) * 0.5,
        width: SETTINGS_TOP_TOGGLE_HIT_W,
        height: SETTINGS_TOP_TOGGLE_HIT_H,
    }
}

/// Round-2 M1 — full row rect for the language dropdown row. Sits directly
/// below the 5 toggle rows.
pub fn settings_language_row_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + SETTINGS_ROW_H_M1 * SETTINGS_TOP_TOGGLE_COUNT as f32,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_ROW_H_M1,
    }
}

/// Round-2 M1 — language dropdown chip rect (right-anchored inside the
/// language row).
pub fn settings_language_chip_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let row = settings_language_row_rect(viewport, scroll_offset_y);
    Rect {
        x: row.right() - SETTINGS_LANGUAGE_CHIP_W,
        y: row.y + (row.height - SETTINGS_LANGUAGE_CHIP_H) * 0.5,
        width: SETTINGS_LANGUAGE_CHIP_W,
        height: SETTINGS_LANGUAGE_CHIP_H,
    }
}

/// Round-2 M1 — chevron sub-rect on the right of the language chip.
pub fn settings_language_chevron_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let chip = settings_language_chip_rect(viewport, scroll_offset_y);
    Rect {
        x: chip.right() - SETTINGS_LANGUAGE_CHEVRON_W - 6.0,
        y: chip.y + (chip.height - 16.0) * 0.5,
        width: SETTINGS_LANGUAGE_CHEVRON_W,
        height: 16.0,
    }
}

/// Round-2 M1 — label sub-rect inside the language chip (chevron excluded).
pub fn settings_language_chip_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let chip = settings_language_chip_rect(viewport, scroll_offset_y);
    Rect {
        x: chip.x + 10.0,
        y: chip.y + (chip.height - 16.0) * 0.5,
        width: (chip.width - SETTINGS_LANGUAGE_CHEVRON_W - 14.0).max(0.0),
        height: 16.0,
    }
}

/// G3 parity (2026-06-01) — the §4 DisplayMode group title rect (`显示模式 /
/// Display Mode`). The zone-display-mode picker was promoted out of the General
/// band into its own `settings-group` §4 between §3 Appearance and §5
/// Performance, matching Tauri's body order (`SettingsPanel.tsx:538-598`
/// `<section class="settings-group"><h3>{settingsGroupDisplayMode}</h3>`).
/// Anchors off the §3 Appearance section bottom (its last element is the accent
/// swatch row, whose bottom is `appearance_content_height − section_gap` below
/// the Appearance title) plus a section gap. The `flags` arg is unused (the
/// section roots at the fixed source-reserve baseline like Appearance §5) but
/// kept for signature symmetry with the Appearance helpers.
///
/// G3 reuses the existing bilingual `SETTINGS_ZONE_DISPLAY_MODE_LABEL`
/// (StringId 140 — "默认显示模式" / "Default display mode") as the group title
/// rather than minting a new StringId (spec §8 — no new strings unless
/// required). The renderer paints the group title here and the radios below;
/// the per-row caption that previously sat to the left of the radios is dropped
/// (the group title now carries the section name).
pub fn settings_display_mode_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    // §3 Appearance roots at `settings_appearance_origin_y_offset`; its content
    // height (title + picker label + grid + trailing gap) minus the trailing gap
    // is the section bottom. §4 DisplayMode sits one section gap below it.
    let appearance_bottom = settings_appearance_origin_y_offset()
        + settings_appearance_content_height(viewport)
        - SETTINGS_SECTION_GAP;
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + appearance_bottom + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// α4 (Wave I-α, 2026-05-25) / G3 parity (2026-06-01) — full row rect for the
/// zone-display-mode picker (the 3 radios). Now sits directly below the §4
/// DisplayMode group title ([`settings_display_mode_label_rect`]) as a standalone
/// section between §3 Appearance and §5 Performance — promoted out of the
/// General band per Tauri parity. Honours scroll offset the same way every other
/// body row does.
pub fn settings_zone_display_mode_picker_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
) -> Rect {
    let label = settings_display_mode_label_rect(viewport, scroll_offset_y);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// α4 — sub-rect for radio `index` (0 = Hover, 1 = Always, 2 = Click).
/// Three radios right-anchor as a single 78×3 + 4×2 = 242-DIP cluster
/// aligned with the language-chip column above. The hit-box height matches
/// the row breathing room (28 DIP) so a tall click still lands cleanly.
pub fn settings_zone_display_mode_radio_rect(
    viewport: Size,
    scroll_offset_y: f32,
    index: u8,
) -> Rect {
    let row = settings_zone_display_mode_picker_row_rect(viewport, scroll_offset_y);
    let cluster_w = SETTINGS_RADIO_W * SETTINGS_ZONE_DISPLAY_MODE_COUNT as f32
        + SETTINGS_RADIO_GAP * (SETTINGS_ZONE_DISPLAY_MODE_COUNT - 1) as f32;
    let cluster_x = row.right() - cluster_w;
    Rect {
        x: cluster_x + (SETTINGS_RADIO_W + SETTINGS_RADIO_GAP) * index as f32,
        y: row.y + (row.height - SETTINGS_RADIO_H) * 0.5,
        width: SETTINGS_RADIO_W,
        height: SETTINGS_RADIO_H,
    }
}

/// α4 — outer-circle paint rect for radio `index`. Sits inside the
/// hit-box, left-anchored, vertically centred.
pub fn settings_zone_display_mode_radio_outer_rect(
    viewport: Size,
    scroll_offset_y: f32,
    index: u8,
) -> Rect {
    let hit = settings_zone_display_mode_radio_rect(viewport, scroll_offset_y, index);
    Rect {
        x: hit.x,
        y: hit.y + (hit.height - SETTINGS_RADIO_OUTER_D) * 0.5,
        width: SETTINGS_RADIO_OUTER_D,
        height: SETTINGS_RADIO_OUTER_D,
    }
}

/// α4 — inner-dot paint rect for radio `index` (only painted when this
/// mode is the current `zone_display_mode`).
pub fn settings_zone_display_mode_radio_inner_rect(
    viewport: Size,
    scroll_offset_y: f32,
    index: u8,
) -> Rect {
    let outer = settings_zone_display_mode_radio_outer_rect(viewport, scroll_offset_y, index);
    Rect {
        x: outer.x + (SETTINGS_RADIO_OUTER_D - SETTINGS_RADIO_INNER_D) * 0.5,
        y: outer.y + (SETTINGS_RADIO_OUTER_D - SETTINGS_RADIO_INNER_D) * 0.5,
        width: SETTINGS_RADIO_INNER_D,
        height: SETTINGS_RADIO_INNER_D,
    }
}

/// α4 — label rect for radio `index`. Sits to the right of the outer
/// circle with a 4-DIP gap; vertically centred inside the hit-box.
pub fn settings_zone_display_mode_radio_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    index: u8,
) -> Rect {
    let hit = settings_zone_display_mode_radio_rect(viewport, scroll_offset_y, index);
    let outer = settings_zone_display_mode_radio_outer_rect(viewport, scroll_offset_y, index);
    let label_x = outer.right() + 4.0;
    Rect {
        x: label_x,
        y: hit.y + (hit.height - 16.0) * 0.5,
        width: (hit.right() - label_x).max(0.0),
        height: 16.0,
    }
}

/// G3 parity (2026-06-01) — height the §4 DisplayMode group contributes to
/// `settings_body_content_height`: the group title + the picker (radio) row +
/// a trailing section gap. PURE — no global state. Mirrors the term rhythm of
/// the other section content-height helpers so the scroll clamp stays exact.
pub fn settings_display_mode_content_height() -> f32 {
    SETTINGS_SECTION_LABEL_H + SETTINGS_ROW_H_M1 + SETTINGS_SECTION_GAP
}

/// M1f — which Updater §8 status family is live, for height purposes only.
/// The updater card height depends on whether the version block, the progress
/// bar, and/or the error line are shown — and those three are mutually
/// determined by the status family. Collapsing the status to this 4-way
/// discriminant keeps [`SettingsBodyFlags`] `Copy` + tiny (no SmolStr) while
/// still driving an exact dynamic height. Pure data — geometry reads it, never
/// global state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpdaterHeightKind {
    /// Idle / Checking / UpToDate — status row only, no extra blocks.
    StatusOnly,
    /// Available / Ready / Installing / Skipped — adds the version block.
    Versioned,
    /// Downloading — adds the progress bar (no version block).
    Downloading,
    /// Error — adds the wrapped error line (no version block).
    Error,
}

/// M1d + M1e + M1f — the conditional-row gating flags fed into the dynamic
/// body-height + scroll-clamp geometry. Bundling them in one `Copy` struct
/// keeps `settings_body_content_height` / `settings_clamp_scroll` (and the
/// shell call sites) under clippy's `too_many_arguments` threshold as more
/// sections light up. Geometry stays PURE: every flag is passed in, nothing
/// inside reads global state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SettingsBodyFlags {
    /// Startup §6 — crash-auto-restart toggle on (shows the 2 stepper rows).
    pub crash_restart_enabled: bool,
    /// Startup §6 — safe-start-after-hibernation on (shows the resume slider).
    pub safe_start_after_hibernation: bool,
    /// Stealth §7 — `retry_count > 0` (shows the retry row + OneDrive block).
    pub stealth_has_retry: bool,
    /// Stealth §7 — `last_error.is_some()` (shows the error block).
    pub stealth_has_error: bool,
    /// Updater §8 — which status family is live (drives version/progress/error
    /// block visibility, hence the card height).
    pub updater_kind: UpdaterHeightKind,
    /// Backup §9 — number of backup rows the list paints (already capped at
    /// [`SETTINGS_BACKUP_ROW_VISIBLE_MAX`] by the caller via
    /// `backup_card::backup_visible_row_count`). The list is variable-length,
    /// so its height grows one [`SETTINGS_BACKUP_ROW_H`] per visible row; an
    /// empty list shows the single `backupEmpty` placeholder row instead.
    pub backup_row_count: usize,
    /// Plugins §11 — number of plugin cards the list paints (already capped at
    /// [`SETTINGS_PLUGINS_ROW_VISIBLE_MAX`] by the caller via
    /// `plugins_section::plugin_visible_row_count`). Like the backup list this
    /// is variable-length: each visible plugin card adds one
    /// [`SETTINGS_PLUGIN_CARD_H`] (+ inter-card gap); an empty list shows the
    /// single `pluginEmpty` placeholder row instead.
    pub plugin_row_count: usize,
    /// Paths §2 — number of dynamic desktop-source cards the §2 list paints
    /// (already capped at [`SETTINGS_SOURCE_ROW_VISIBLE_MAX`] by the caller).
    /// M1i fidelity (2026-05-29) — this section sits MID-body and now REFLOWS:
    /// the live count drives the source-block height
    /// ([`settings_sources_content_height`]) and, via
    /// [`settings_sources_reserve_delta`], shifts every section below it
    /// up/down (Tauri's flex column). The count threads into
    /// [`settings_body_content_height`] so the scroll clamp matches the live
    /// height, exactly like `with_backup_rows` / `with_plugin_rows`.
    pub source_row_count: usize,
}

impl SettingsBodyFlags {
    /// Convenience constructor used by tests + call sites that only vary the
    /// Startup/Stealth bools (updater idle, no backups). Keeps the common case
    /// terse; `with_backup_rows` layers the variable backup-list count on top.
    pub const fn new(
        crash_restart_enabled: bool,
        safe_start_after_hibernation: bool,
        stealth_has_retry: bool,
        stealth_has_error: bool,
        updater_kind: UpdaterHeightKind,
    ) -> Self {
        Self {
            crash_restart_enabled,
            safe_start_after_hibernation,
            stealth_has_retry,
            stealth_has_error,
            updater_kind,
            backup_row_count: 0,
            plugin_row_count: 0,
            source_row_count: 0,
        }
    }

    /// M1g — return a copy with the Backup §9 visible-row count set. Builder
    /// style keeps `new()`'s arity fixed (no clippy `too_many_arguments`
    /// regression) while letting the shell + renderer feed the live capped
    /// count into the dynamic body height + scroll clamp.
    pub const fn with_backup_rows(mut self, backup_row_count: usize) -> Self {
        self.backup_row_count = backup_row_count;
        self
    }

    /// M1h — return a copy with the Plugins §11 visible-row count set. Same
    /// builder rationale as [`Self::with_backup_rows`]: keeps `new()`'s arity
    /// fixed while feeding the live capped plugin-card count into the dynamic
    /// body height + scroll clamp so paint / hit / scroll all agree.
    pub const fn with_plugin_rows(mut self, plugin_row_count: usize) -> Self {
        self.plugin_row_count = plugin_row_count;
        self
    }

    /// M1i — return a copy with the Paths §2 source-card count set. Same
    /// builder rationale as [`Self::with_plugin_rows`]: keeps `new()`'s arity
    /// fixed while feeding the live capped desktop-source count through the
    /// shared `SettingsBodyFlags` so paint / hit / scroll all read one count.
    pub const fn with_source_rows(mut self, source_row_count: usize) -> Self {
        self.source_row_count = source_row_count;
        self
    }
}

/// Round-2 M1/M2 + M1d + M1e + M1f — total content height inside the body.
/// Grows with each milestone as sections light up. The Startup §6, Stealth §7
/// and Updater §8 sections all have conditional rows, so their gating lives in
/// [`SettingsBodyFlags`] (passed by ref) — geometry never reads global state,
/// the shell passes the live values.
pub fn settings_body_content_height(viewport: Size, flags: &SettingsBodyFlags) -> f32 {
    // G3 parity (2026-06-01) — terms summed in the NEW Tauri body order:
    // General(+Paths §2) → Appearance §3 → DisplayMode §4 → Performance §5 →
    // Startup §6 → Stealth §7 → Updater §8 → Backup §9 → Encryption §10 →
    // Plugins §11. The total is order-independent (it is a sum), but the
    // ordering is kept readable to mirror the laid-out chain.
    settings_m2_content_height(viewport, flags.source_row_count)
        // §3 Appearance grid — body-width-driven (4-col card grid), now between
        // §2 Paths and §4 DisplayMode (was painted LAST pre-G3).
        + settings_appearance_content_height(viewport)
        // §4 DisplayMode — promoted out of the General band into its own group.
        + settings_display_mode_content_height()
        + settings_perf_startup_content_height(
            viewport,
            flags.crash_restart_enabled,
            flags.safe_start_after_hibernation,
        )
        + settings_stealth_content_height(flags.stealth_has_retry, flags.stealth_has_error)
        + settings_updater_content_height(flags.updater_kind)
        + settings_backup_content_height(flags.backup_row_count)
        // M7 — §10 Encryption card slots between Backup §9 and Plugins §11
        // (Tauri `<BackupCard/><EncryptionCard/>` adjacency). Fixed-height, so a
        // single constant additive term (no `SettingsBodyFlags` field needed).
        + settings_encryption_content_height()
        + settings_plugins_content_height(flags.plugin_row_count)
}

/// Round-2 M1 — clamp `requested_offset` to `[0, max_scroll]` where
/// `max_scroll = max(0, content_h - body_h)`. Returns 0 when the content
/// already fits, so the scroll Cell can never go negative.
pub fn settings_body_max_scroll(content_total_h: f32, viewport: Size) -> f32 {
    let body = settings_body_rect(viewport);
    (content_total_h - body.height).max(0.0)
}

/// Round-2 M1 + M1d + M1e + M1f — apply a wheel-delta `delta_y` (positive =
/// scroll down) to `current_offset` and clamp. Pure helper so the wheel
/// handler stays allocation-free. The [`SettingsBodyFlags`] feed the dynamic
/// content height so the max-scroll matches whatever conditional rows are
/// currently visible (Startup crash steppers + hibernate slider; Stealth
/// retry/error rows; Updater version/progress/error block).
pub fn settings_clamp_scroll(
    current_offset: f32,
    delta_y: f32,
    viewport: Size,
    flags: &SettingsBodyFlags,
) -> f32 {
    let next = (current_offset + delta_y).max(0.0);
    let content_h = settings_body_content_height(viewport, flags);
    let max = settings_body_max_scroll(content_h, viewport);
    next.min(max)
}

/// Round-2 M2 — Y offset (scroll-space) at which the M2 §2 Paths sections
/// start. G3 parity (2026-06-01): the §4 zone-display-mode picker was promoted
/// out of the General band into its own group between §3 Appearance and §5
/// Performance (Tauri body order General → **Paths** → Appearance → DisplayMode
/// → Performance). So Paths §2 now anchors directly below the M1 toggle band +
/// the language row (the General band's last element) + a section gap — the
/// `+ 2.0` picker-row wedge is gone (now `+ 1.0`: 5 toggles + 1 language row).
fn settings_m2_origin_y_offset() -> f32 {
    SETTINGS_ROW_H_M1 * (SETTINGS_TOP_TOGGLE_COUNT as f32 + 1.0) + SETTINGS_SECTION_GAP
}

/// Round-2 M2 — `桌面源` section label rect (the dim caption above the two
/// source cards).
pub fn settings_sources_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + settings_m2_origin_y_offset(),
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// Round-2 M2 — full row rect for source card `index` (0=primary, 1=public).
pub fn settings_source_row_rect(viewport: Size, scroll_offset_y: f32, index: u8) -> Rect {
    let label = settings_sources_label_rect(viewport, scroll_offset_y);
    Rect {
        x: label.x,
        y: label.bottom() + (SETTINGS_SOURCE_ROW_H + SETTINGS_SOURCE_GAP) * index as f32,
        width: label.width,
        height: SETTINGS_SOURCE_ROW_H,
    }
}

/// M1i fidelity (2026-05-29) — refresh (`↻`) button. In Tauri it is the LAST
/// child of `.desktop-source-list` (`align-self: flex-end`), sitting directly
/// BELOW the cards / empty placeholder at the list's bottom-right — NOT on the
/// section heading row (`SettingsPanel.tsx:354-360`). Click re-runs
/// `all_desktop_dirs` and repopulates `AppState::desktop_sources`
/// (`RefreshDesktopSources`). Its Y follows the live card stack, so the hit
/// rect (`ui::settings_hit`) must pass the same live `source_row_count`.
pub fn settings_sources_refresh_button_rect(
    viewport: Size,
    scroll_offset_y: f32,
    source_row_count: usize,
) -> Rect {
    let last_bottom = settings_sources_cards_bottom(viewport, scroll_offset_y, source_row_count);
    let label = settings_sources_label_rect(viewport, scroll_offset_y);
    Rect {
        x: label.right() - SETTINGS_SOURCE_REFRESH_BTN_W,
        y: last_bottom + SETTINGS_SOURCE_REFRESH_GAP,
        width: SETTINGS_SOURCE_REFRESH_BTN_W,
        height: SETTINGS_SOURCE_REFRESH_BTN_H,
    }
}

/// M1i fidelity — scroll-space bottom Y of the live source-card stack (or the
/// empty placeholder when the list is empty). The refresh button hangs below
/// this, and the 桌面路径 section anchors off the refresh button's bottom.
/// `source_row_count` is the LIVE count (clamped to the cap).
pub fn settings_sources_cards_bottom(
    viewport: Size,
    scroll_offset_y: f32,
    source_row_count: usize,
) -> f32 {
    let label = settings_sources_label_rect(viewport, scroll_offset_y);
    let live = source_row_count.min(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
    if live == 0 {
        // Empty `.desktop-source-empty` placeholder occupies one short line.
        label.bottom() + SETTINGS_SOURCE_EMPTY_H
    } else {
        let last = settings_source_row_rect(viewport, scroll_offset_y, (live - 1) as u8);
        last.bottom()
    }
}

/// Round-2 M2 / M1i fidelity — `桌面路径` section label rect. Anchors off the
/// refresh button bottom (the list's last child), which itself follows the
/// LIVE `source_row_count` — so this section reflows up/down with the live
/// source count (Tauri's flex column).
pub fn settings_desktop_path_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    source_row_count: usize,
) -> Rect {
    let refresh = settings_sources_refresh_button_rect(viewport, scroll_offset_y, source_row_count);
    let body = settings_body_rect(viewport);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: refresh.bottom() + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// Round-2 M2 — `桌面路径` input rect (single-line dark rounded box).
pub fn settings_desktop_path_input_rect(
    viewport: Size,
    scroll_offset_y: f32,
    source_row_count: usize,
) -> Rect {
    let label = settings_desktop_path_label_rect(viewport, scroll_offset_y, source_row_count);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_INPUT_ROW_H,
    }
}

/// Round-2 M2 — `监控值` section label rect.
pub fn settings_watch_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    source_row_count: usize,
) -> Rect {
    let input = settings_desktop_path_input_rect(viewport, scroll_offset_y, source_row_count);
    let body = settings_body_rect(viewport);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: input.bottom() + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// Round-2 M2 — `监控值` textarea rect (multi-line dark rounded box).
pub fn settings_watch_textarea_rect(
    viewport: Size,
    scroll_offset_y: f32,
    source_row_count: usize,
) -> Rect {
    let label = settings_watch_label_rect(viewport, scroll_offset_y, source_row_count);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_TEXTAREA_H,
    }
}

/// M1i fidelity (2026-05-29) — the live height of the inner card+refresh stack
/// of the 桌面源 §2 list, EXCLUDING the heading label and the trailing section
/// gap. Drives the reflow (Tauri's flex column): `live` cards plus the
/// `align-self: flex-end` refresh button below them, or the single empty
/// placeholder + refresh button when the list is empty. `source_row_count` is
/// clamped to [`SETTINGS_SOURCE_ROW_VISIBLE_MAX`].
fn settings_sources_stack_height(source_row_count: usize) -> f32 {
    let live = source_row_count.min(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
    let cards = if live == 0 {
        // Empty `.desktop-source-empty` placeholder.
        SETTINGS_SOURCE_EMPTY_H
    } else {
        SETTINGS_SOURCE_ROW_H * live as f32 + SETTINGS_SOURCE_GAP * (live - 1) as f32
    };
    // Refresh button hangs below the cards / placeholder (list's last child).
    cards + SETTINGS_SOURCE_REFRESH_GAP + SETTINGS_SOURCE_REFRESH_BTN_H
}

/// M1i fidelity — height the 桌面源 §2 source block contributes to the body:
/// heading label + the LIVE card+refresh stack + the trailing section gap.
/// Unlike the old fixed-reserve version, this now REFLOWS with the live count
/// so the scroll clamp matches what is painted (Tauri's flex column). Single
/// source of truth for the source-block height, folded into
/// [`settings_m2_content_height`].
pub fn settings_sources_content_height(source_row_count: usize) -> f32 {
    SETTINGS_SECTION_LABEL_H + settings_sources_stack_height(source_row_count) + SETTINGS_SECTION_GAP
}

/// M1i fidelity — the scroll-space SHIFT (>= 0) every section below the 桌面源
/// block moves UP relative to the fixed [`SETTINGS_SOURCE_ROW_VISIBLE_MAX`]
/// reserve baseline, for the given live source count. The perf-and-below
/// geometry fns root at [`settings_perf_origin_y_offset`], which is pinned at
/// the max-reserve baseline; callers fold this delta into the `scroll_offset_y`
/// they pass to those fns (shifting content UP by `delta` is identical to
/// scrolling DOWN by `delta`). This is the single-base-offset reflow mechanism
/// — no per-section signature churn. The 桌面路径 / 监控值 rows take the live
/// count directly instead (they anchor off the refresh button), so the delta
/// is applied only from Performance §5 downward.
pub fn settings_sources_reserve_delta(source_row_count: usize) -> f32 {
    let max = settings_sources_content_height(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
    let live = settings_sources_content_height(source_row_count);
    (max - live).max(0.0)
}

/// Round-2 M2 / M1i fidelity — total content height of M1 + M2 sections only.
/// M3 helpers extend this further. The source-block contribution is delegated
/// to [`settings_sources_content_height`] (single source of truth) and now
/// reflects the LIVE `source_row_count` so the scroll clamp shrinks/grows with
/// the rendered list.
pub fn settings_m2_content_height(_viewport: Size, source_row_count: usize) -> f32 {
    settings_m2_origin_y_offset()
        + settings_sources_content_height(source_row_count)
        + SETTINGS_SECTION_LABEL_H
        + SETTINGS_INPUT_ROW_H
        + SETTINGS_SECTION_GAP
        + SETTINGS_SECTION_LABEL_H
        + SETTINGS_TEXTAREA_H
        + SETTINGS_SECTION_GAP
}

// ── M1d 2026-05-29 — Performance §5 + Startup management §6 ────────────
//
// Replaces the deleted bespoke 高级 / 未来集成验证 sections (nano-only, not
// in Tauri) with the two genuine Tauri sections from
// `SettingsPanel.tsx:601-698`. Performance has 3 SliderRows (no
// conditionals); Startup has 2 toggles + 2 conditional steppers + 1 toggle
// + 1 conditional slider, so its height is dynamic (gated by
// `crash_restart_enabled` and `safe_start_after_hibernation`).
//
// The number-stepper rects (− value +) and slider track rect below are the
// generalized descendants of the old `settings_advanced_num_*` /
// `settings_advanced_slider_rect` templates.

/// M1d — number of SliderRows in the Performance section (展开/收起/缓存).
pub const SETTINGS_PERF_ROW_COUNT: u8 = 3;

/// M1d — number-stepper mini button (− / +) size.
pub const SETTINGS_NUM_BTN_W: f32 = 24.0;
pub const SETTINGS_NUM_BTN_H: f32 = 24.0;

/// M1d — number-stepper value label width (between − and +).
pub const SETTINGS_NUM_VALUE_W: f32 = 40.0;

/// M1d — slider track geometry shared by Performance + hibernate sliders.
pub const SETTINGS_SLIDER_W: f32 = 200.0;
pub const SETTINGS_SLIDER_TRACK_H: f32 = 4.0;
pub const SETTINGS_SLIDER_THUMB_D: f32 = 14.0;

/// M1d — SliderRow total height. Tauri `.slider-row` is a column: a
/// `label + tabular value` line on top, the `<input type=range>` below
/// (`SettingsPanel.tsx:848-871`). 24 (label line) + 20 (track band) = 44.
pub const SETTINGS_SLIDER_ROW_H: f32 = 44.0;

/// M1d — height of a one-line `.settings-row__desc` caption under a toggle.
pub const SETTINGS_DESC_H: f32 = 18.0;

// ── Performance §5 geometry (3 SliderRows, no conditionals) ────────────

/// M1d / M1i fidelity / G3 parity — scroll-space Y at which the Performance
/// group title starts, PINNED at the fixed [`SETTINGS_SOURCE_ROW_VISIBLE_MAX`]
/// source reserve baseline. The live source-count reflow is applied by callers
/// folding [`settings_sources_reserve_delta`] into `scroll_offset_y`
/// (single-base-offset mechanism) rather than threading the count through every
/// perf-and-below rect fn — so this and all sections rooted on it keep their
/// `(viewport, scroll_offset_y)` signatures untouched.
///
/// G3 parity (2026-06-01): §3 Appearance + §4 DisplayMode now sit between §2
/// Paths and §5 Performance (Tauri body order). Their content heights are added
/// here so Performance and everything below it shift down by exactly the
/// Appearance grid + DisplayMode group. [`settings_appearance_origin_y_offset`]
/// already equals the §2 Paths terms (m2 origin + sources + path + watch); we
/// extend it by the Appearance and DisplayMode section content heights. Takes
/// `viewport` because the Appearance grid height is body-width driven (the same
/// width paint/hit feed `settings_appearance_content_height`), keeping the
/// offset chain exactly consistent with the rendered Appearance section.
fn settings_perf_origin_y_offset(viewport: Size) -> f32 {
    settings_appearance_origin_y_offset()
        + settings_appearance_content_height(viewport)
        + settings_display_mode_content_height()
}

/// M1d — `性能 / Performance` group title rect.
pub fn settings_performance_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + settings_perf_origin_y_offset(viewport),
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M1d — full SliderRow rect for Performance slider `index` (0..3).
pub fn settings_performance_slider_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    index: u8,
) -> Rect {
    let label = settings_performance_label_rect(viewport, scroll_offset_y);
    Rect {
        x: label.x,
        y: label.bottom() + SETTINGS_SLIDER_ROW_H * index as f32,
        width: label.width,
        height: SETTINGS_SLIDER_ROW_H,
    }
}

/// M1d — slider track/hit rect inside a Performance SliderRow (full-width
/// band on the lower line of the row).
pub fn settings_performance_slider_rect(
    viewport: Size,
    scroll_offset_y: f32,
    index: u8,
) -> Rect {
    let row = settings_performance_slider_row_rect(viewport, scroll_offset_y, index);
    Rect {
        x: row.x,
        y: row.bottom() - SETTINGS_SLIDER_THUMB_D - 4.0,
        width: row.width,
        height: SETTINGS_SLIDER_THUMB_D,
    }
}

// ── Startup management §6 geometry (dynamic height) ────────────────────
//
// Row order (visible subset depends on the two gating bools):
//   0  高优先级启动 toggle              (always)
//   0d desc
//   1  崩溃自动重启 toggle              (always) — gates 2/3
//   1d desc
//   2  最大重试次数 stepper             (crash_restart only)
//   3  崩溃窗口（秒）stepper            (crash_restart only)
//   4  休眠安全恢复 toggle              (always) — gates the slider
//   4d desc
//   5  恢复延迟 SliderRow               (hibernation only)

/// M1d — `启动管理 / Startup Management` group title rect. Sits below the
/// Performance section (3 SliderRows + a section gap).
pub fn settings_startup_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let last_perf = settings_performance_slider_row_rect(
        viewport,
        scroll_offset_y,
        SETTINGS_PERF_ROW_COUNT - 1,
    );
    let body = settings_body_rect(viewport);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: last_perf.bottom() + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M1d — `高优先级启动` toggle row rect (row 0, always shown).
pub fn settings_startup_high_priority_row_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let label = settings_startup_label_rect(viewport, scroll_offset_y);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1d — `崩溃自动重启` toggle row rect (row 1, always shown). Sits below the
/// high-priority row + its description caption.
pub fn settings_crash_restart_row_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let prev = settings_startup_high_priority_row_rect(viewport, scroll_offset_y);
    Rect {
        x: prev.x,
        y: prev.bottom() + SETTINGS_DESC_H,
        width: prev.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1d — `最大重试次数` stepper row rect (row 2). Only laid out / painted /
/// hit-tested when `crash_restart_enabled`. Sits below the crash-restart row
/// + its description caption.
pub fn settings_crash_max_retries_row_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let prev = settings_crash_restart_row_rect(viewport, scroll_offset_y);
    Rect {
        x: prev.x,
        y: prev.bottom() + SETTINGS_DESC_H,
        width: prev.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1d — `崩溃窗口（秒）` stepper row rect (row 3). Conditional on
/// `crash_restart_enabled`; sits directly below the retries stepper row.
pub fn settings_crash_window_row_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let prev = settings_crash_max_retries_row_rect(viewport, scroll_offset_y);
    Rect {
        x: prev.x,
        y: prev.bottom(),
        width: prev.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1d — `休眠安全恢复` toggle row rect (row 4, always shown). Its Y depends
/// on whether the two crash steppers are present, so it takes the gating
/// bool as a parameter (geometry stays pure — never reads global state).
///
/// When the steppers are hidden, the row sits below the crash-restart
/// toggle's always-shown description caption (so `+ SETTINGS_DESC_H` clears
/// it). When the steppers are shown, the last stepper (崩溃窗口) has NO
/// description caption, so the row sits directly below it (no extra gap).
pub fn settings_safe_start_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
) -> Rect {
    let (anchor, desc_gap) = if crash_restart_enabled {
        (settings_crash_window_row_rect(viewport, scroll_offset_y), 0.0)
    } else {
        (
            settings_crash_restart_row_rect(viewport, scroll_offset_y),
            SETTINGS_DESC_H,
        )
    };
    Rect {
        x: anchor.x,
        y: anchor.bottom() + desc_gap,
        width: anchor.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1d — `恢复延迟` SliderRow rect (row 5). Conditional on
/// `safe_start_after_hibernation`; positioned below the safe-start toggle +
/// its description caption. Takes `crash_restart_enabled` to chain through
/// the dynamic safe-start anchor.
pub fn settings_hibernate_slider_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
) -> Rect {
    let prev = settings_safe_start_row_rect(viewport, scroll_offset_y, crash_restart_enabled);
    Rect {
        x: prev.x,
        y: prev.bottom() + SETTINGS_DESC_H,
        width: prev.width,
        height: SETTINGS_SLIDER_ROW_H,
    }
}

/// M1d — slider track/hit rect inside the hibernate SliderRow.
pub fn settings_hibernate_slider_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
) -> Rect {
    let row = settings_hibernate_slider_row_rect(viewport, scroll_offset_y, crash_restart_enabled);
    Rect {
        x: row.x,
        y: row.bottom() - SETTINGS_SLIDER_THUMB_D - 4.0,
        width: row.width,
        height: SETTINGS_SLIDER_THUMB_D,
    }
}

/// M1d — right-anchored toggle hit-box inside a Startup-section toggle row.
/// Shared by the renderer (paints the rocker centred in this box) and the
/// hit-tester. Mirrors `SETTINGS_TOP_TOGGLE_HIT_*` so click ergonomics match
/// the General-section toggles.
pub fn settings_startup_toggle_hit_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - SETTINGS_TOP_TOGGLE_HIT_W,
        y: row.y + (row.height - SETTINGS_TOP_TOGGLE_HIT_H) * 0.5,
        width: SETTINGS_TOP_TOGGLE_HIT_W,
        height: SETTINGS_TOP_TOGGLE_HIT_H,
    }
}

/// M1d — number-stepper "+" button rect, right-anchored inside `row`.
pub fn settings_stepper_plus_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - SETTINGS_NUM_BTN_W,
        y: row.y + (row.height - SETTINGS_NUM_BTN_H) * 0.5,
        width: SETTINGS_NUM_BTN_W,
        height: SETTINGS_NUM_BTN_H,
    }
}

/// M1d — number-stepper value label rect (between − and +).
pub fn settings_stepper_value_rect(row: Rect) -> Rect {
    let plus = settings_stepper_plus_rect(row);
    Rect {
        x: plus.x - SETTINGS_NUM_VALUE_W,
        y: plus.y,
        width: SETTINGS_NUM_VALUE_W,
        height: SETTINGS_NUM_BTN_H,
    }
}

/// M1d — number-stepper "−" button rect (left of the value label).
pub fn settings_stepper_minus_rect(row: Rect) -> Rect {
    let value = settings_stepper_value_rect(row);
    Rect {
        x: value.x - SETTINGS_NUM_BTN_W,
        y: value.y,
        width: SETTINGS_NUM_BTN_W,
        height: SETTINGS_NUM_BTN_H,
    }
}

/// M1d — combined height of the Performance + Startup sections, fed into
/// `settings_body_content_height`. Conditional rows make this dynamic, so
/// the two gating bools are parameters (geometry never reads global state).
fn settings_perf_startup_content_height(
    viewport: Size,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> f32 {
    // Performance: title + 3 SliderRows + section gap.
    let perf = SETTINGS_SECTION_LABEL_H
        + SETTINGS_SLIDER_ROW_H * SETTINGS_PERF_ROW_COUNT as f32
        + SETTINGS_SECTION_GAP;
    // Startup: title + the visible-row stack measured off the last laid-out
    // element so the helpers below are the single source of layout truth.
    let title_top = settings_startup_label_rect(viewport, 0.0).y;
    let last_bottom = if safe_start_after_hibernation {
        settings_hibernate_slider_row_rect(viewport, 0.0, crash_restart_enabled).bottom()
    } else {
        // safe-start toggle + its description caption.
        settings_safe_start_row_rect(viewport, 0.0, crash_restart_enabled).bottom()
            + SETTINGS_DESC_H
    };
    let startup = (last_bottom - title_top) + SETTINGS_SECTION_GAP;
    perf + startup
}

// ── M1e 2026-05-29 — Stealth §7 card (`StealthModeCard.tsx`) ────────────
//
// Sits AFTER Startup in the Tauri body order
// (General→Paths→Appearance→Zone→Performance→Startup→Stealth→…). Rows:
//   title                            (always) — 桌面隐形模式
//   status row  [label | pill]       (always)
//   schema-version row [label|value] (always)
//   mirror-health row  [label|value] (always)
//   retry-count row    [label|value] (only when retry_count > 0)
//   last-error block   [label]/[err] (only when last_error.is_some())
//   buttons row  [Refresh][Reapply]  (always)
//   OneDrive warning block           (only when retry_count > 0)
//
// The two conditional flags (`has_retry`, `has_error`) flow as parameters so
// geometry stays pure — the shell passes the live `stealth::status()`
// snapshot. The retry row and OneDrive block are both gated on `has_retry`
// (the backend notes OneDrive typically holds the lock when retries pend).

/// M1e — compact `.settings-row` height for the Stealth label/value rows
/// (shorter than the 44-DIP toggle rows; matches Tauri's `.settings-row`
/// status-line rhythm).
pub const SETTINGS_STEALTH_ROW_H: f32 = 28.0;

/// M1e — Stealth status pill capsule size (reuses the source-card pill tone;
/// generalized colour bucket is `StatusLevel::derive`).
pub const SETTINGS_STEALTH_PILL_W: f32 = 76.0;
pub const SETTINGS_STEALTH_PILL_H: f32 = 22.0;

/// M1e — last-error block: a label line + a wrapped error-code line.
pub const SETTINGS_STEALTH_ERROR_BLOCK_H: f32 = 46.0;

/// M1e — buttons-row height (Refresh + Reapply share the footer button size).
pub const SETTINGS_STEALTH_BTN_ROW_H: f32 = SETTINGS_FOOTER_BTN_H;

/// M1e — OneDrive warning block height (multi-line informational text).
pub const SETTINGS_STEALTH_ONEDRIVE_H: f32 = 52.0;

/// M1e — scroll-space bottom Y of the last laid-out Startup element, the
/// anchor the Stealth title hangs from. Mirrors the branch logic in
/// `settings_perf_startup_content_height` so layout has a single source of
/// truth.
fn settings_startup_section_bottom(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> f32 {
    if safe_start_after_hibernation {
        settings_hibernate_slider_row_rect(viewport, scroll_offset_y, crash_restart_enabled)
            .bottom()
    } else {
        settings_safe_start_row_rect(viewport, scroll_offset_y, crash_restart_enabled).bottom()
            + SETTINGS_DESC_H
    }
}

/// M1e — `桌面隐形模式 / Desktop Stealth Mode` group title rect. Sits below
/// the Startup section + a section gap. Takes the Startup gating bools so its
/// Y follows whatever Startup rows are currently visible.
pub fn settings_stealth_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> Rect {
    let body = settings_body_rect(viewport);
    let bottom = settings_startup_section_bottom(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
    );
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: bottom + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M1e — status row rect (label left + pill right). Row 0, always shown.
pub fn settings_stealth_status_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> Rect {
    let label = settings_stealth_label_rect(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
    );
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_STEALTH_ROW_H,
    }
}

/// M1e — right-anchored status-pill rect inside the status row.
pub fn settings_stealth_pill_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - SETTINGS_STEALTH_PILL_W,
        y: row.y + (row.height - SETTINGS_STEALTH_PILL_H) * 0.5,
        width: SETTINGS_STEALTH_PILL_W,
        height: SETTINGS_STEALTH_PILL_H,
    }
}

/// M1e — schema-version row rect (label + value). Row 1, always shown.
pub fn settings_stealth_schema_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> Rect {
    let prev = settings_stealth_status_row_rect(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
    );
    Rect {
        x: prev.x,
        y: prev.bottom(),
        width: prev.width,
        height: SETTINGS_STEALTH_ROW_H,
    }
}

/// M1e — mirror-health row rect (label + value). Row 2, always shown.
pub fn settings_stealth_mirror_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> Rect {
    let prev = settings_stealth_schema_row_rect(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
    );
    Rect {
        x: prev.x,
        y: prev.bottom(),
        width: prev.width,
        height: SETTINGS_STEALTH_ROW_H,
    }
}

/// M1e — retry-count row rect (label + value). Row 3, ONLY when
/// `retry_count > 0`. Sits directly below the mirror-health row.
pub fn settings_stealth_retry_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> Rect {
    let prev = settings_stealth_mirror_row_rect(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
    );
    Rect {
        x: prev.x,
        y: prev.bottom(),
        width: prev.width,
        height: SETTINGS_STEALTH_ROW_H,
    }
}

/// M1e — last-error block rect (label line + wrapped code line). Row 4, ONLY
/// when `last_error.is_some()`. Its Y depends on whether the retry row is
/// present, so the `has_retry` flag chains through.
pub fn settings_stealth_error_block_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
    has_retry: bool,
) -> Rect {
    let anchor = if has_retry {
        settings_stealth_retry_row_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
        )
    } else {
        settings_stealth_mirror_row_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
        )
    };
    Rect {
        x: anchor.x,
        y: anchor.bottom(),
        width: anchor.width,
        height: SETTINGS_STEALTH_ERROR_BLOCK_H,
    }
}

/// M1e — buttons row rect ([Refresh][Reapply]). Always shown; its Y depends
/// on the two conditional rows above (`has_retry`, `has_error`).
pub fn settings_stealth_buttons_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
    has_retry: bool,
    has_error: bool,
) -> Rect {
    let bottom = if has_error {
        settings_stealth_error_block_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
            has_retry,
        )
        .bottom()
    } else if has_retry {
        settings_stealth_retry_row_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
        )
        .bottom()
    } else {
        settings_stealth_mirror_row_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
        )
        .bottom()
    };
    let body = settings_body_rect(viewport);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: bottom + 6.0,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_STEALTH_BTN_ROW_H,
    }
}

/// M1e — Refresh button rect (left), inside the Stealth buttons row.
pub fn settings_stealth_refresh_button_rect(row: Rect) -> Rect {
    Rect {
        x: row.x,
        y: row.y + (row.height - SETTINGS_FOOTER_BTN_H) * 0.5,
        width: SETTINGS_FOOTER_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// M1e — Reapply button rect (right of Refresh), inside the buttons row.
pub fn settings_stealth_reapply_button_rect(row: Rect) -> Rect {
    let refresh = settings_stealth_refresh_button_rect(row);
    Rect {
        x: refresh.right() + SETTINGS_FOOTER_BTN_GAP,
        y: refresh.y,
        width: SETTINGS_FOOTER_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// M1e — OneDrive warning block rect. ONLY when `retry_count > 0`. Sits below
/// the buttons row. Informational text only (no button — there is no
/// OneDrive-exclusion probe / guide URL in the nano backend, so per §17 this
/// stays text-only rather than painting a dead button).
pub fn settings_stealth_onedrive_block_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
    has_retry: bool,
    has_error: bool,
) -> Rect {
    let buttons = settings_stealth_buttons_row_rect(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
        has_retry,
        has_error,
    );
    Rect {
        x: buttons.x,
        y: buttons.bottom() + 8.0,
        width: buttons.width,
        height: SETTINGS_STEALTH_ONEDRIVE_H,
    }
}

/// M1e — height the Stealth §7 card contributes to
/// `settings_body_content_height`. Conditional rows make it dynamic, so the
/// two flags are parameters (geometry never reads global state). The base
/// rows (title + status + schema + mirror) are always present; retry adds one
/// row, error adds the error block, and retry additionally adds the OneDrive
/// block. A trailing section gap keeps the body bottom padded.
pub fn settings_stealth_content_height(has_retry: bool, has_error: bool) -> f32 {
    let mut h = SETTINGS_SECTION_LABEL_H + SETTINGS_STEALTH_ROW_H * 3.0;
    if has_retry {
        h += SETTINGS_STEALTH_ROW_H;
    }
    if has_error {
        h += SETTINGS_STEALTH_ERROR_BLOCK_H;
    }
    h += 6.0 + SETTINGS_STEALTH_BTN_ROW_H;
    if has_retry {
        h += 8.0 + SETTINGS_STEALTH_ONEDRIVE_H;
    }
    h + SETTINGS_SECTION_GAP
}

// ── M1f 2026-05-29 — Updater §8 card (`UpdaterCard.tsx`) ────────────────
//
// Sits AFTER Stealth in the Tauri body order
// (…→Performance→Startup→Stealth→Updater→Backup→Encryption→Plugins). Rows:
//   title                              (always) — 应用更新
//   status row  [label | pill]         (always)
//   version block [label : version]    (only Available/Ready/Installing/Skipped)
//   progress bar                       (only Downloading)
//   error line                         (only Error)
//   action buttons [Check][Dl/Install][Skip]  (Check always; others state-gated)
//   freq prefs row  [label | chip]     (always)
//   auto-download row [label | toggle] (always)
//
// The version/progress/error blocks are mutually exclusive by status family,
// captured as `UpdaterHeightKind`. Geometry takes that discriminant + the
// Startup/Stealth gating flags (so the title follows whatever Stealth rows are
// visible) — all passed in, never read from global state.

/// M1f — compact label/value/version row height (matches the Stealth row
/// rhythm).
pub const SETTINGS_UPDATER_ROW_H: f32 = 28.0;

/// M1f — status pill capsule size (reuses the Stealth pill footprint; the
/// "有可用更新"/"准备安装" labels are the widest so the pill is a touch wider).
pub const SETTINGS_UPDATER_PILL_W: f32 = 92.0;
pub const SETTINGS_UPDATER_PILL_H: f32 = 22.0;

/// M1f — progress-bar band height (the track sits vertically centred in it).
pub const SETTINGS_UPDATER_PROGRESS_H: f32 = 20.0;
/// M1f — progress-track thickness.
pub const SETTINGS_UPDATER_PROGRESS_TRACK_H: f32 = 6.0;

/// M1f — error line band height (single wrapped line).
pub const SETTINGS_UPDATER_ERROR_H: f32 = 32.0;

/// M1f — action buttons row height (shares the footer button height).
pub const SETTINGS_UPDATER_BTN_ROW_H: f32 = SETTINGS_FOOTER_BTN_H;

/// M1f — wider action button for the bilingual labels (检查更新 / 安装并重启
/// / Install and restart) which overflow the 84-DIP footer button width.
pub const SETTINGS_UPDATER_BTN_W: f32 = 104.0;
/// M1f — gap between adjacent action buttons.
pub const SETTINGS_UPDATER_BTN_GAP: f32 = 8.0;

/// M1f — frequency chip size (cycles Daily/Weekly/Manual). Mirrors the
/// language chip footprint so the prefs rows read as the same control band.
pub const SETTINGS_UPDATER_FREQ_CHIP_W: f32 = 96.0;
pub const SETTINGS_UPDATER_FREQ_CHIP_H: f32 = 28.0;

/// M1f — scroll-space bottom Y of the last laid-out Stealth element, the
/// anchor the Updater title hangs from. Mirrors the branch logic in
/// `settings_stealth_content_height` (buttons row always; OneDrive block only
/// when `has_retry`) so layout has a single source of truth.
fn settings_stealth_section_bottom(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
    has_retry: bool,
    has_error: bool,
) -> f32 {
    if has_retry {
        settings_stealth_onedrive_block_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
            has_retry,
            has_error,
        )
        .bottom()
    } else {
        settings_stealth_buttons_row_rect(
            viewport,
            scroll_offset_y,
            crash_restart_enabled,
            safe_start_after_hibernation,
            has_retry,
            has_error,
        )
        .bottom()
    }
}

/// M1f — `应用更新 / App Updates` group title rect. Sits below the Stealth
/// section + a section gap. Takes all the Startup+Stealth gating flags so its
/// Y follows whatever rows are currently visible above it.
pub fn settings_updater_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
    stealth_has_retry: bool,
    stealth_has_error: bool,
) -> Rect {
    let body = settings_body_rect(viewport);
    let bottom = settings_stealth_section_bottom(
        viewport,
        scroll_offset_y,
        crash_restart_enabled,
        safe_start_after_hibernation,
        stealth_has_retry,
        stealth_has_error,
    );
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: bottom + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M1f — status row rect (label left + pill right). Row 0, always shown. Takes
/// the full flag set to chain off the dynamic title Y.
pub fn settings_updater_status_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let label = settings_updater_label_rect(
        viewport,
        scroll_offset_y,
        flags.crash_restart_enabled,
        flags.safe_start_after_hibernation,
        flags.stealth_has_retry,
        flags.stealth_has_error,
    );
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_UPDATER_ROW_H,
    }
}

/// M1f — right-anchored status-pill rect inside the status row.
pub fn settings_updater_pill_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - SETTINGS_UPDATER_PILL_W,
        y: row.y + (row.height - SETTINGS_UPDATER_PILL_H) * 0.5,
        width: SETTINGS_UPDATER_PILL_W,
        height: SETTINGS_UPDATER_PILL_H,
    }
}

/// M1f — the conditional middle block (version / progress / error) rect. Its
/// height depends on `flags.updater_kind`; `StatusOnly` yields a zero-height
/// rect anchored at the status-row bottom (so the buttons row chains cleanly
/// with no gap). Sits directly below the status row.
pub fn settings_updater_middle_block_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let status = settings_updater_status_row_rect(viewport, scroll_offset_y, flags);
    let height = match flags.updater_kind {
        UpdaterHeightKind::StatusOnly => 0.0,
        UpdaterHeightKind::Versioned => SETTINGS_UPDATER_ROW_H,
        UpdaterHeightKind::Downloading => SETTINGS_UPDATER_PROGRESS_H,
        UpdaterHeightKind::Error => SETTINGS_UPDATER_ERROR_H,
    };
    Rect {
        x: status.x,
        y: status.bottom(),
        width: status.width,
        height,
    }
}

/// M1f — progress-track rect inside the middle block (only meaningful when
/// `flags.updater_kind == Downloading`; the renderer paints the filled portion
/// itself from the fraction). Vertically centred, full row width.
pub fn settings_updater_progress_track_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let block = settings_updater_middle_block_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: block.x,
        y: block.y + (block.height - SETTINGS_UPDATER_PROGRESS_TRACK_H) * 0.5,
        width: block.width,
        height: SETTINGS_UPDATER_PROGRESS_TRACK_H,
    }
}

/// M1f — action buttons row rect ([检查更新][下载/安装并重启][跳过此版本]).
/// Always shown (检查更新 is always visible); sits below the middle block.
pub fn settings_updater_buttons_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let block = settings_updater_middle_block_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: block.x,
        y: block.bottom() + 6.0,
        width: block.width,
        height: SETTINGS_UPDATER_BTN_ROW_H,
    }
}

/// M1f — action button rect for column `index` (0-based, left to right) inside
/// the buttons row. Buttons left-pack; visibility is decided by the caller
/// (`updater_show_*`), so callers must assign a stable column index to the
/// buttons they actually paint. The hit-tester reuses the same index→rect
/// mapping so paint and hit agree.
pub fn settings_updater_button_rect(row: Rect, index: u8) -> Rect {
    let x = row.x + (SETTINGS_UPDATER_BTN_W + SETTINGS_UPDATER_BTN_GAP) * index as f32;
    Rect {
        x,
        y: row.y + (row.height - SETTINGS_FOOTER_BTN_H) * 0.5,
        width: SETTINGS_UPDATER_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// M1f — `检查频率 / Check frequency` prefs row rect (label + cycling chip).
/// Always shown; sits below the action buttons row.
pub fn settings_updater_frequency_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let buttons = settings_updater_buttons_row_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: buttons.x,
        y: buttons.bottom() + 8.0,
        width: buttons.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1f — right-anchored frequency chip rect inside the frequency row.
pub fn settings_updater_frequency_chip_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - SETTINGS_UPDATER_FREQ_CHIP_W,
        y: row.y + (row.height - SETTINGS_UPDATER_FREQ_CHIP_H) * 0.5,
        width: SETTINGS_UPDATER_FREQ_CHIP_W,
        height: SETTINGS_UPDATER_FREQ_CHIP_H,
    }
}

/// M1f — `后台静默下载 / Silent background download` toggle row rect. Always
/// shown; sits directly below the frequency row.
pub fn settings_updater_auto_download_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let freq = settings_updater_frequency_row_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: freq.x,
        y: freq.bottom(),
        width: freq.width,
        height: SETTINGS_ROW_H_M1,
    }
}

/// M1f — right-anchored toggle hit-box inside the auto-download row (mirrors
/// `SETTINGS_TOP_TOGGLE_HIT_*` so click ergonomics match the General toggles).
pub fn settings_updater_auto_download_hit_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - SETTINGS_TOP_TOGGLE_HIT_W,
        y: row.y + (row.height - SETTINGS_TOP_TOGGLE_HIT_H) * 0.5,
        width: SETTINGS_TOP_TOGGLE_HIT_W,
        height: SETTINGS_TOP_TOGGLE_HIT_H,
    }
}

/// M1f — height the Updater §8 card contributes to
/// `settings_body_content_height`. Conditional middle block makes it dynamic,
/// so the status family drives it (pure — no global reads). Always-present
/// rows: title + status + buttons + 2 prefs rows. The middle block adds its
/// kind-specific height. A trailing section gap keeps the body bottom padded.
pub fn settings_updater_content_height(kind: UpdaterHeightKind) -> f32 {
    let middle = match kind {
        UpdaterHeightKind::StatusOnly => 0.0,
        UpdaterHeightKind::Versioned => SETTINGS_UPDATER_ROW_H,
        UpdaterHeightKind::Downloading => SETTINGS_UPDATER_PROGRESS_H,
        UpdaterHeightKind::Error => SETTINGS_UPDATER_ERROR_H,
    };
    SETTINGS_SECTION_LABEL_H
        + SETTINGS_UPDATER_ROW_H
        + middle
        + 6.0
        + SETTINGS_UPDATER_BTN_ROW_H
        + 8.0
        + SETTINGS_ROW_H_M1 * 2.0
        + SETTINGS_SECTION_GAP
}

// ── M1g 2026-05-29 — Backup §9 card (`BackupCard.tsx`) ──────────────────
//
// Sits AFTER Updater in the Tauri body order
// (…→Stealth→Updater→Backup→Encryption→Plugins). Rows:
//   title                              (always) — 设置备份
//   description line                   (always)
//   立即备份 button + Refresh button     (always)
//   info/error line                    (only when settings_backup_status set)
//   backup-list:
//     N entry rows [file·size | 恢复]  (one per visible entry, capped)
//     OR a single backupEmpty row       (when the list is empty)
//
// The list is variable-length: its height grows one `SETTINGS_BACKUP_ROW_H`
// per visible row (capped at `SETTINGS_BACKUP_ROW_VISIBLE_MAX`), or a single
// placeholder row when empty. The capped row count + the status-present flag
// flow through `SettingsBodyFlags::backup_row_count` (built from
// `backup_card::backup_visible_row_count`) so the dynamic height + scroll
// clamp match what's painted. Geometry stays PURE — the count is passed in,
// nothing reads global state. Encryption §10 + Plugins §11 follow in a later
// chunk; this card leaves a trailing section gap for them.

/// M1g — max backup rows the list paints / hit-tests. Reuses the plugins
/// visible-cap rhythm (`SETTINGS_PLUGINS_ROW_VISIBLE_MAX`) so the compact
/// overlay never runs the list off the body. Matches the (now superseded) K1
/// `SETTINGS_BACKUP_ENTRY_VISIBLE_MAX` so the cap is unchanged from the K1
/// shell the runtime replaced.
pub const SETTINGS_BACKUP_ROW_VISIBLE_MAX: usize = 3;

/// M1g — compact backup label/value/description row height (matches the
/// Stealth/Updater 28-DIP status-line rhythm).
pub const SETTINGS_BACKUP_ROW_H: f32 = 28.0;

/// M1g — backup-list entry row height (file·size on the left, 恢复 button on
/// the right).
pub const SETTINGS_BACKUP_ENTRY_ROW_H: f32 = 30.0;

/// M1g — gap between adjacent backup-list entry rows.
pub const SETTINGS_BACKUP_ENTRY_ROW_GAP: f32 = 6.0;

/// M1g — `立即备份 / Create now` button width (wider than a stepper so the
/// bilingual label fits) and the smaller per-row 恢复 / Refresh button width.
pub const SETTINGS_BACKUP_CREATE_BTN_W: f32 = 104.0;
pub const SETTINGS_BACKUP_REFRESH_BTN_W: f32 = 84.0;
pub const SETTINGS_BACKUP_RESTORE_BTN_W: f32 = 64.0;
pub const SETTINGS_BACKUP_BTN_GAP_M1: f32 = 8.0;

/// M1g — `设置备份 / Settings Backup` group title rect. Sits below the Updater
/// section + a section gap. Takes the full flag set so its Y follows whatever
/// Updater rows (status/version/progress/error + prefs) are currently visible.
pub fn settings_backup_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let body = settings_body_rect(viewport);
    // The Updater section's last laid-out element is always the auto-download
    // prefs row (always shown), so anchor off its bottom + a section gap.
    let updater_bottom =
        settings_updater_auto_download_row_rect(viewport, scroll_offset_y, flags).bottom();
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: updater_bottom + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M1g — description line rect (一段说明文字). Row 0, always shown, below the
/// title.
pub fn settings_backup_description_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let label = settings_backup_label_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_BACKUP_ROW_H,
    }
}

/// M1g — actions row rect ([立即备份][刷新]). Always shown; below the
/// description line.
pub fn settings_backup_actions_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let desc = settings_backup_description_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: desc.x,
        y: desc.bottom(),
        width: desc.width,
        height: SETTINGS_BACKUP_BTN_ROW_H,
    }
}

/// M1g — actions row height (shares the footer button height, like the other
/// card button rows).
pub const SETTINGS_BACKUP_BTN_ROW_H: f32 = SETTINGS_FOOTER_BTN_H;

/// M1g — `立即备份 / Create now` button rect (left), inside the actions row.
pub fn settings_backup_create_button_rect(row: Rect) -> Rect {
    Rect {
        x: row.x,
        y: row.y + (row.height - SETTINGS_FOOTER_BTN_H) * 0.5,
        width: SETTINGS_BACKUP_CREATE_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// M1g — `刷新 / Refresh` button rect (right of 立即备份), inside the actions
/// row. Re-lists the backup files (`ListSettingsBackups`).
pub fn settings_backup_refresh_button_rect(row: Rect) -> Rect {
    let create = settings_backup_create_button_rect(row);
    Rect {
        x: create.right() + SETTINGS_BACKUP_BTN_GAP_M1,
        y: create.y,
        width: SETTINGS_BACKUP_REFRESH_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// M1g — info/error line rect. Only painted when `settings_backup_status` is
/// set; below the actions row. Its presence does NOT change the list Y (the
/// list anchors off this rect's reserved slot regardless) so the geometry
/// stays a single linear chain — when no status is set the renderer simply
/// skips painting here and the list still lines up because both branches
/// anchor off `actions_row.bottom()`.
pub fn settings_backup_status_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let actions = settings_backup_actions_row_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: actions.x,
        y: actions.bottom() + 4.0,
        width: actions.width,
        height: SETTINGS_BACKUP_ROW_H,
    }
}

/// M1g — backup-list entry row rect for visible `entry_index` (0-based,
/// newest-first). Sits below the status line. When the list is empty the
/// renderer paints a single `backupEmpty` placeholder at `entry_index = 0`.
pub fn settings_backup_entry_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
    entry_index: usize,
) -> Rect {
    let status = settings_backup_status_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: status.x,
        y: status.bottom()
            + (SETTINGS_BACKUP_ENTRY_ROW_H + SETTINGS_BACKUP_ENTRY_ROW_GAP) * entry_index as f32,
        width: status.width,
        height: SETTINGS_BACKUP_ENTRY_ROW_H,
    }
}

/// M1g — right-anchored `恢复 / Restore` button rect inside an entry row.
pub fn settings_backup_restore_button_rect(row: Rect) -> Rect {
    Rect {
        x: row.right() - SETTINGS_BACKUP_RESTORE_BTN_W,
        y: row.y + (row.height - SETTINGS_FOOTER_BTN_H) * 0.5,
        width: SETTINGS_BACKUP_RESTORE_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// M1g — height the Backup §9 card contributes to
/// `settings_body_content_height`. The variable-length list makes it dynamic,
/// so the (already-capped) `backup_row_count` is the parameter (geometry never
/// reads global state). Always-present: title + description + actions +
/// reserved status line. The list adds either `n` entry rows (+ inter-row
/// gaps) or a single empty-placeholder row. A trailing section gap pads the
/// body bottom (and reserves room for the §10/§11 chunk to come).
pub fn settings_backup_content_height(backup_row_count: usize) -> f32 {
    let base = SETTINGS_SECTION_LABEL_H
        + SETTINGS_BACKUP_ROW_H            // description
        + SETTINGS_BACKUP_BTN_ROW_H        // actions
        + 4.0
        + SETTINGS_BACKUP_ROW_H; // reserved status line
    let rows = backup_row_count.min(SETTINGS_BACKUP_ROW_VISIBLE_MAX);
    let list = if rows == 0 {
        // Empty placeholder occupies one entry-row slot.
        SETTINGS_BACKUP_ENTRY_ROW_H
    } else {
        SETTINGS_BACKUP_ENTRY_ROW_H * rows as f32
            + SETTINGS_BACKUP_ENTRY_ROW_GAP * (rows as f32 - 1.0)
    };
    base + list + SETTINGS_SECTION_GAP
}

// ── M7 2026-06-01 — Encryption §10 inline card (`EncryptionCard.tsx`) ────────
//
// Sits BETWEEN Backup §9 and Plugins §11 in the Tauri body order
// (…→Updater→Backup→**Encryption**→Plugins→footer), matching the Tauri
// `<BackupCard/><EncryptionCard/>` adjacency (`SettingsPanel.tsx:705-706`). The
// card is FIXED-HEIGHT (no variable rows), so unlike Backup/Plugins it adds a
// single constant additive term to `settings_body_content_height` and needs NO
// `SettingsBodyFlags` field. It anchors off the Backup card's last laid-out row
// (the backup list's last visible entry, or the empty placeholder) + a section
// gap; Plugins §11 then re-anchors off this card's status row so the offset
// chain reflows automatically. Layout (top-to-bottom, vertical column):
//   section label                                (always) — 设置加密
//   description line                             (always) — OneDrive sentence
//   current-mode row                             (always) — 当前模式: <mode>
//   3-button mode grid (None / DPAPI / Passphrase) (always)
//   passphrase row (label + masked input box)    (always)
//   hint line                                    (always) — never-stored
//   status banner                                (reserved; painted iff set)
// Both the renderer (paint) and `ui::settings_hit` (hit) call the identical
// helpers below so paint geometry == hit geometry (the project-wide SSoT rule).
// Geometry stays PURE — every helper is a function of (viewport, scroll, flags),
// returning `Copy` `Rect`s; no `AppState` reads (§10).

/// M7 — encryption mode-button row height (each button is a title + sub-label
/// stacked block; matches the footer-button rhythm with room for two lines).
/// #7 §10 item 7 (2026-06-01) — bumped 44→52 to fit Tauri's `padding: 10px 12px`
/// + `gap: 4px` (`EncryptionCard.css:29,36`): 10 (top pad) + 16 (13px title slot)
/// + 4 (gap) + 16 (11px sub slot) + 10 (bottom pad) ≈ 52. The prior 44 packed the
/// two-line content against the chip edges.
pub const SETTINGS_ENCRYPTION_BTN_ROW_H: f32 = 52.0;
/// M7 — encryption passphrase input row height (single-line masked box; shares
/// the §2 path-input rhythm).
pub const SETTINGS_ENCRYPTION_INPUT_ROW_H: f32 = 40.0;
/// M7 — encryption current-mode / hint / status compact row heights (match the
/// other card status-line rhythm).
pub const SETTINGS_ENCRYPTION_ROW_H: f32 = 28.0;
/// M7 — gap between the three mode buttons in the grid.
pub const SETTINGS_ENCRYPTION_BTN_GAP: f32 = 8.0;
/// P13 (#7 fix wave 2026-06-01) — vertical gap between EVERY sibling row of the
/// §10 card (description / current / grid / passphrase-row / hint / status).
/// Tauri `.encryption-card { gap: 10px }` (`EncryptionCard.css:4`). The
/// mode-grid's INTERNAL button gap stays [`SETTINGS_ENCRYPTION_BTN_GAP`] (8px,
/// CSS:23) — this 10px is the inter-row rhythm only.
pub const SETTINGS_ENCRYPTION_ROW_GAP: f32 = 10.0;
/// P4 (#7 fix wave 2026-06-01) — width of the passphrase ROW's left label cell
/// (口令 / Passphrase). Tauri lays the row out `justify-content: space-between`
/// with the label on the left + the input filling the rest; this fixed cell is
/// the native-panel equivalent of the auto-sized `<span>`.
pub const SETTINGS_ENCRYPTION_PASS_LABEL_W: f32 = 64.0;
/// P4 — gap between the passphrase row's label cell and the input box.
pub const SETTINGS_ENCRYPTION_PASS_LABEL_GAP: f32 = 10.0;
/// M7 — number of mode buttons (None / DPAPI / Passphrase).
pub const SETTINGS_ENCRYPTION_MODE_COUNT: u8 = 3;

/// M7 — §10 Encryption card group title rect (设置加密 / Settings Encryption).
/// Anchors off the Backup card's last laid-out row (the backup list's last
/// visible entry, or the single placeholder at index 0) + a section gap — the
/// same anchor Plugins §11 used before this card landed. Takes the full flag
/// set so its Y follows whatever Backup/Updater/Stealth/Startup rows are
/// currently visible.
pub fn settings_encryption_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let body = settings_body_rect(viewport);
    let last_backup_index = flags
        .backup_row_count
        .min(SETTINGS_BACKUP_ROW_VISIBLE_MAX)
        .saturating_sub(1);
    let backup_bottom =
        settings_backup_entry_row_rect(viewport, scroll_offset_y, flags, last_backup_index).bottom();
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: backup_bottom + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M7 — description line rect (the OneDrive sentence). Below the title.
/// P13 — separated from the title by the 10px inter-row gap.
pub fn settings_encryption_desc_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let label = settings_encryption_label_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: label.x,
        y: label.bottom() + SETTINGS_ENCRYPTION_ROW_GAP,
        width: label.width,
        height: SETTINGS_ENCRYPTION_ROW_H,
    }
}

/// M7 — current-mode row rect (当前模式: <mode label>). Below the description.
/// P13 — separated by the 10px inter-row gap.
pub fn settings_encryption_current_mode_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let desc = settings_encryption_desc_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: desc.x,
        y: desc.bottom() + SETTINGS_ENCRYPTION_ROW_GAP,
        width: desc.width,
        height: SETTINGS_ENCRYPTION_ROW_H,
    }
}

/// M7 — the 3-button mode-grid row rect (the band holding all three buttons).
/// Below the current-mode row. Use [`settings_encryption_mode_button_rect`] for
/// individual buttons inside this band.
pub fn settings_encryption_mode_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let current = settings_encryption_current_mode_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: current.x,
        // P13 — separated from the current-mode row by the 10px inter-row gap.
        y: current.bottom() + SETTINGS_ENCRYPTION_ROW_GAP,
        width: current.width,
        height: SETTINGS_ENCRYPTION_BTN_ROW_H,
    }
}

/// M7 — individual mode-button rect inside the grid for `index`
/// (0 = None, 1 = DPAPI, 2 = Passphrase). The three buttons split the row width
/// evenly with two inter-button gaps. PURE — no global state.
pub fn settings_encryption_mode_button_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
    index: u8,
) -> Rect {
    let row = settings_encryption_mode_row_rect(viewport, scroll_offset_y, flags);
    let count = SETTINGS_ENCRYPTION_MODE_COUNT as f32;
    let total_gap = SETTINGS_ENCRYPTION_BTN_GAP * (count - 1.0);
    let btn_w = ((row.width - total_gap) / count).max(0.0);
    let i = (index.min(SETTINGS_ENCRYPTION_MODE_COUNT - 1)) as f32;
    Rect {
        x: row.x + (btn_w + SETTINGS_ENCRYPTION_BTN_GAP) * i,
        y: row.y,
        width: btn_w,
        height: row.height,
    }
}

/// P4 (#7 fix wave 2026-06-01) — the full passphrase ROW band (label cell +
/// input box), below the mode-button grid. Tauri `.encryption-passphrase-row`
/// is a `justify-content: space-between` flex row: a `<span>` label on the left
/// and the `<input>` filling the rest. The label/input sub-rects derive from
/// this band. P13 — separated from the grid by the 10px inter-row gap (was the
/// 8px button gap).
pub fn settings_encryption_passphrase_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let row = settings_encryption_mode_row_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: row.x,
        y: row.bottom() + SETTINGS_ENCRYPTION_ROW_GAP,
        width: row.width,
        height: SETTINGS_ENCRYPTION_INPUT_ROW_H,
    }
}

/// P4 — passphrase ROW left label cell (口令 / Passphrase). The fixed-width
/// left cell of the space-between row; non-interactive.
pub fn settings_encryption_passphrase_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let row = settings_encryption_passphrase_row_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: row.x,
        y: row.y,
        width: SETTINGS_ENCRYPTION_PASS_LABEL_W.min(row.width),
        height: row.height,
    }
}

/// M7 — masked passphrase input box rect. P4 — now ONLY the input sub-rect on
/// the RIGHT of the passphrase row (the left label cell + a gap are reserved by
/// [`settings_encryption_passphrase_label_rect`]); the hit-test for
/// `FocusPassphraseField` targets this sub-rect only.
pub fn settings_encryption_passphrase_input_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let row = settings_encryption_passphrase_row_rect(viewport, scroll_offset_y, flags);
    let label_w = SETTINGS_ENCRYPTION_PASS_LABEL_W.min(row.width);
    let x = row.x + label_w + SETTINGS_ENCRYPTION_PASS_LABEL_GAP;
    Rect {
        x,
        y: row.y,
        width: (row.right() - x).max(0.0),
        height: row.height,
    }
}

/// M7 — hint line rect (the "never stored in plaintext" sentence). Below the
/// passphrase ROW. P13 — separated by the 10px inter-row gap. Spans the full
/// card width (not just the input sub-rect) like the Tauri `.encryption-hint`.
pub fn settings_encryption_hint_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let row = settings_encryption_passphrase_row_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: row.x,
        y: row.bottom() + SETTINGS_ENCRYPTION_ROW_GAP,
        width: row.width,
        height: SETTINGS_ENCRYPTION_ROW_H,
    }
}

/// M7 — status banner rect (error/success). Reserved slot below the hint;
/// painted only when `settings_encryption_status` is `Some`. The presence of a
/// status does NOT change the next section's anchor (Plugins anchors off this
/// rect's reserved slot regardless), keeping the offset chain linear — same
/// pattern as the Backup card's reserved status row.
pub fn settings_encryption_status_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let hint = settings_encryption_hint_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: hint.x,
        // P13 — separated from the hint by the 10px inter-row gap.
        y: hint.bottom() + SETTINGS_ENCRYPTION_ROW_GAP,
        width: hint.width,
        height: SETTINGS_ENCRYPTION_ROW_H,
    }
}

/// M7 — fixed height the §10 Encryption card contributes to
/// `settings_body_content_height`. No variable rows (unlike Backup/Plugins), so
/// this is a constant: label + desc + current-mode + button-row + passphrase
/// input + hint + reserved status + a trailing section gap.
pub fn settings_encryption_content_height() -> f32 {
    SETTINGS_SECTION_LABEL_H
        + SETTINGS_ENCRYPTION_ROW_H            // description
        + SETTINGS_ENCRYPTION_ROW_H            // current-mode row
        + SETTINGS_ENCRYPTION_BTN_ROW_H        // mode-button grid
        + SETTINGS_ENCRYPTION_INPUT_ROW_H      // passphrase row (label + input)
        + SETTINGS_ENCRYPTION_ROW_H            // hint line
        + SETTINGS_ENCRYPTION_ROW_H            // reserved status banner
        // P13 — 6 × 10px inter-row gaps (label→desc→current→grid→passphrase→
        // hint→status). Replaces the single 8px pre-passphrase button gap.
        + SETTINGS_ENCRYPTION_ROW_GAP * 6.0
        + SETTINGS_SECTION_GAP
}

// ── M1h 2026-05-29 — Plugins §11 inline section (`SettingsPanel.tsx:709-781`) ──
//
// Sits LAST in the (currently shipped) Tauri body order
// (…→Updater→Backup→**Plugins**→footer). Encryption §10 is deferred
// (crash-entangled) so Plugins anchors directly after the Backup card for now;
// when Encryption lands it will slot between Backup and Plugins and the offset
// chain reflows automatically (each section anchors off the previous one).
// Layout (top-to-bottom, vertical column):
//   group title                                  (always) — 插件 / Plugins
//   安装插件... button (full width)              (always)
//   plugin-list:
//     N plugin cards [header | author | desc | uninstall]  (one per visible
//                                                            entry, capped)
//     OR a single pluginEmpty placeholder row    (when the list is empty)
//
// Each plugin card is a fixed-height block; the list is variable-length so its
// height grows one card (+ inter-card gap) per visible row (capped at
// [`SETTINGS_PLUGINS_ROW_VISIBLE_MAX`]), or a single placeholder row when
// empty. The capped row count flows through `SettingsBodyFlags::plugin_row_count`
// (built from `plugins_section::plugin_visible_row_count`) so the dynamic height
// + scroll clamp match what's painted. Geometry stays PURE — the count is
// passed in, nothing reads global state.

/// M1h — `安装插件... / Install plugin...` full-width button height (shares the
/// footer button height like the Backup card's action buttons).
pub const SETTINGS_PLUGIN_INSTALL_BTN_H: f32 = SETTINGS_FOOTER_BTN_H;

/// M1h — per-plugin card sub-row heights. Header carries name + v{version} +
/// type badge + enable toggle; the author + description lines sit below it; the
/// actions row hosts the 卸载 / Uninstall button.
pub const SETTINGS_PLUGIN_CARD_HEADER_H: f32 = 24.0;
pub const SETTINGS_PLUGIN_CARD_AUTHOR_H: f32 = 16.0;
pub const SETTINGS_PLUGIN_CARD_DESC_H: f32 = 16.0;
pub const SETTINGS_PLUGIN_CARD_ACTIONS_H: f32 = SETTINGS_FOOTER_BTN_H;
/// M1h — inner vertical padding inside a plugin card (top + bottom each).
pub const SETTINGS_PLUGIN_CARD_PAD_Y: f32 = 6.0;

/// M1h — full plugin-card height (the variable list grows by this per row).
pub const SETTINGS_PLUGIN_CARD_H: f32 = SETTINGS_PLUGIN_CARD_PAD_Y
    + SETTINGS_PLUGIN_CARD_HEADER_H
    + SETTINGS_PLUGIN_CARD_AUTHOR_H
    + SETTINGS_PLUGIN_CARD_DESC_H
    + SETTINGS_PLUGIN_CARD_ACTIONS_H
    + SETTINGS_PLUGIN_CARD_PAD_Y;

/// M1h — empty-state placeholder row height (matches the Backup empty row).
pub const SETTINGS_PLUGIN_EMPTY_ROW_H: f32 = SETTINGS_BACKUP_ENTRY_ROW_H;

/// M1h — gap between adjacent plugin cards.
pub const SETTINGS_PLUGIN_CARD_GAP: f32 = 8.0;

/// M1h — type-badge chip width + the enable-toggle hit-box (right-anchored in
/// the card header) + the 卸载 button width.
pub const SETTINGS_PLUGIN_BADGE_W: f32 = 56.0;
pub const SETTINGS_PLUGIN_TOGGLE_HIT_W: f32 = 60.0;
pub const SETTINGS_PLUGIN_TOGGLE_HIT_H: f32 = 24.0;
pub const SETTINGS_PLUGIN_UNINSTALL_BTN_W: f32 = 72.0;

/// M1h — scroll-space Y at which the Plugins group title starts. M7
/// (2026-06-01): re-anchored off the Encryption §10 card's reserved status row
/// + a section gap (the card now slots between Backup §9 and Plugins §11 to
/// match Tauri's `<BackupCard/><EncryptionCard/>` adjacency). The encryption
/// card is fixed-height, so its status row bottom is a deterministic offset
/// from the Backup card's last row; the whole chain reflows automatically.
/// Takes the full flag set so its Y follows whatever Backup/Updater/Stealth/
/// Startup rows are currently visible.
pub fn settings_plugins_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let body = settings_body_rect(viewport);
    // The §10 Encryption card's last laid-out row is its reserved status slot.
    // Anchoring off its bottom keeps the offset chain linear regardless of
    // whether a status banner is actually painted.
    let encryption_bottom =
        settings_encryption_status_rect(viewport, scroll_offset_y, flags).bottom();
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: encryption_bottom + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M1h — full-width `安装插件...` install button rect. Below the group title.
pub fn settings_plugins_install_button_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let label = settings_plugins_label_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_PLUGIN_INSTALL_BTN_H,
    }
}

/// M1h — plugin-card row rect for visible `card_index` (0-based). Sits below
/// the install button. When the list is empty the renderer paints a single
/// `pluginEmpty` placeholder at `card_index = 0` (its height differs, but the
/// origin is the same slot).
pub fn settings_plugin_card_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
    card_index: usize,
) -> Rect {
    let install = settings_plugins_install_button_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: install.x,
        y: install.bottom()
            + SETTINGS_PLUGIN_CARD_GAP
            + (SETTINGS_PLUGIN_CARD_H + SETTINGS_PLUGIN_CARD_GAP) * card_index as f32,
        width: install.width,
        height: SETTINGS_PLUGIN_CARD_H,
    }
}

/// M1h — empty-state placeholder row rect (when no plugins are installed). Same
/// origin slot as `settings_plugin_card_rect(.., 0)` but the empty-row height.
pub fn settings_plugin_empty_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let install = settings_plugins_install_button_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: install.x,
        y: install.bottom() + SETTINGS_PLUGIN_CARD_GAP,
        width: install.width,
        height: SETTINGS_PLUGIN_EMPTY_ROW_H,
    }
}

/// M1h — right-anchored enable-toggle hit-box inside a plugin card's header
/// sub-row. Maps to `SettingsHit::TogglePlugin(card_index)`.
pub fn settings_plugin_toggle_hit_rect(card: Rect) -> Rect {
    let header_y = card.y + SETTINGS_PLUGIN_CARD_PAD_Y;
    Rect {
        x: card.right() - SETTINGS_PLUGIN_TOGGLE_HIT_W,
        y: header_y + (SETTINGS_PLUGIN_CARD_HEADER_H - SETTINGS_PLUGIN_TOGGLE_HIT_H) * 0.5,
        width: SETTINGS_PLUGIN_TOGGLE_HIT_W,
        height: SETTINGS_PLUGIN_TOGGLE_HIT_H,
    }
}

/// M1h — type-badge chip rect inside a plugin card's header sub-row (left of
/// the toggle). Display-only (no hit), shows theme/widget/organizer.
pub fn settings_plugin_badge_rect(card: Rect) -> Rect {
    let toggle = settings_plugin_toggle_hit_rect(card);
    Rect {
        x: toggle.x - 8.0 - SETTINGS_PLUGIN_BADGE_W,
        y: toggle.y,
        width: SETTINGS_PLUGIN_BADGE_W,
        height: SETTINGS_PLUGIN_TOGGLE_HIT_H,
    }
}

/// M1h — name + v{version} text rect inside a plugin card's header sub-row
/// (left of the type badge). Display-only.
pub fn settings_plugin_name_rect(card: Rect) -> Rect {
    let badge = settings_plugin_badge_rect(card);
    Rect {
        x: card.x + SETTINGS_PLUGIN_CARD_PAD_Y,
        y: card.y + SETTINGS_PLUGIN_CARD_PAD_Y,
        width: (badge.x - card.x - SETTINGS_PLUGIN_CARD_PAD_Y - 8.0).max(0.0),
        height: SETTINGS_PLUGIN_CARD_HEADER_H,
    }
}

/// M1h — author line rect inside a plugin card (below the header sub-row).
pub fn settings_plugin_author_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + SETTINGS_PLUGIN_CARD_PAD_Y,
        y: card.y + SETTINGS_PLUGIN_CARD_PAD_Y + SETTINGS_PLUGIN_CARD_HEADER_H,
        width: card.width - SETTINGS_PLUGIN_CARD_PAD_Y * 2.0,
        height: SETTINGS_PLUGIN_CARD_AUTHOR_H,
    }
}

/// M1h — description line rect inside a plugin card (below the author line).
pub fn settings_plugin_desc_rect(card: Rect) -> Rect {
    let author = settings_plugin_author_rect(card);
    Rect {
        x: author.x,
        y: author.bottom(),
        width: author.width,
        height: SETTINGS_PLUGIN_CARD_DESC_H,
    }
}

/// M1h — `卸载 / Uninstall` button rect inside a plugin card's actions sub-row
/// (right-anchored, below the description line). Maps to
/// `SettingsHit::UninstallPlugin(card_index)`.
pub fn settings_plugin_uninstall_button_rect(card: Rect) -> Rect {
    let desc = settings_plugin_desc_rect(card);
    Rect {
        x: card.right() - SETTINGS_PLUGIN_CARD_PAD_Y - SETTINGS_PLUGIN_UNINSTALL_BTN_W,
        y: desc.bottom() + (SETTINGS_PLUGIN_CARD_ACTIONS_H - SETTINGS_FOOTER_BTN_H) * 0.5,
        width: SETTINGS_PLUGIN_UNINSTALL_BTN_W,
        height: SETTINGS_FOOTER_BTN_H,
    }
}

/// M1h — height the Plugins §11 section contributes to
/// `settings_body_content_height`. Always-present: title + install button. The
/// list adds either `n` plugin cards (+ inter-card gaps, plus the leading gap)
/// or a single empty-placeholder row. A trailing section gap pads the body
/// bottom. The (already-capped) `plugin_row_count` is the parameter — geometry
/// never reads global state.
pub fn settings_plugins_content_height(plugin_row_count: usize) -> f32 {
    let base = SETTINGS_SECTION_LABEL_H + SETTINGS_PLUGIN_INSTALL_BTN_H;
    let rows = plugin_row_count.min(SETTINGS_PLUGINS_ROW_VISIBLE_MAX);
    let list = if rows == 0 {
        SETTINGS_PLUGIN_CARD_GAP + SETTINGS_PLUGIN_EMPTY_ROW_H
    } else {
        SETTINGS_PLUGIN_CARD_GAP
            + SETTINGS_PLUGIN_CARD_H * rows as f32
            + SETTINGS_PLUGIN_CARD_GAP * (rows as f32 - 1.0)
    };
    base + list + SETTINGS_SECTION_GAP
}

// ── M6-UI 2026-05-29 / G3 parity 2026-06-01 — §3 Appearance inline theme grid (`SettingsPanel.tsx:396-536`) ──
//
// G3 parity (2026-06-01): the §3 Appearance section now sits between §2 Paths
// and §4 DisplayMode — matching Tauri's body order General → Paths →
// **Appearance** → DisplayMode → Performance. Previously nano painted it LAST
// (after Plugins §11). The grid geometry (group headings + 17 ThemeCards +
// accent swatch row) is owned by `crate::theme_picker::appearance_layout`,
// which is body-width-driven and fully `Copy` (fixed-cap `[Rect; N]`, no `Vec`
// — §10). These helpers only resolve the section's scroll-space ANCHOR (so
// paint / hit / scroll all agree) and the content-width fed to the layout.
//
// The section flows inside the body D2D scroll-clip (`push_clip(body_rect)`),
// so partial rows at the body edge are masked exactly like every other section.

/// G3 parity — scroll-space Y at which the §3 Appearance group title starts,
/// PINNED at the fixed [`SETTINGS_SOURCE_ROW_VISIBLE_MAX`] source-reserve
/// baseline (same single-base-offset reflow mechanism as
/// [`settings_perf_origin_y_offset`]). It sits one section gap below the §2
/// 监控值 textarea bottom (the last element of the Paths section), computed at
/// the full source reserve. Callers fold [`settings_sources_reserve_delta`]
/// into `scroll_offset_y` so the live source-count reflow shifts Appearance +
/// everything below it.
fn settings_appearance_origin_y_offset() -> f32 {
    settings_m2_origin_y_offset()
        + settings_sources_content_height(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize)
        + SETTINGS_SECTION_LABEL_H
        + SETTINGS_INPUT_ROW_H
        + SETTINGS_SECTION_GAP
        + SETTINGS_SECTION_LABEL_H
        + SETTINGS_TEXTAREA_H
        + SETTINGS_SECTION_GAP
}

/// M6-UI / G3 parity — the §3 Appearance group title rect. Anchors off the §2
/// Paths section bottom (the 监控值 textarea) plus a section gap. The `flags`
/// arg is retained for call-site stability (renderer + hit-tester) but no
/// longer read — the section now roots at the fixed source-reserve baseline
/// like the Performance §5 chain, so its Y is independent of the Backup/Plugins
/// row counts below it.
pub fn settings_appearance_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    _flags: &SettingsBodyFlags,
) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + settings_appearance_origin_y_offset(),
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M6-UI — content width fed to `theme_picker::appearance_layout` (body width
/// minus the two row pads). The 4-col card width derives from this.
pub fn settings_appearance_inner_width(viewport: Size) -> f32 {
    let body = settings_body_rect(viewport);
    (body.width - SETTINGS_ROW_PAD_X * 2.0).max(0.0)
}

/// M6-UI — scroll-space origin (top-left of the grid, below the group title +
/// the picker label) for `theme_picker::appearance_layout`. The renderer + the
/// hit-tester both call this so the inline layout shares one anchor.
pub fn settings_appearance_grid_origin(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> crate::theme_picker::Point {
    let label = settings_appearance_label_rect(viewport, scroll_offset_y, flags);
    crate::theme_picker::Point {
        x: label.x,
        // Group title + a "Choose Theme" picker label line precede the grid.
        y: label.bottom() + SETTINGS_SECTION_LABEL_H,
    }
}

/// M6-UI — the "选择主题 / Choose Theme" picker-label rect (between the
/// Appearance group title and the grid).
pub fn settings_appearance_picker_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let label = settings_appearance_label_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// M6-UI — height the §3 Appearance section contributes to
/// `settings_body_content_height`: the group title + the picker label + the
/// grid/accent layout `total_height` + a trailing section gap. PURE — the grid
/// height delegates to `theme_picker::appearance_content_height` (single source
/// of truth) for the given body width; no global state is read.
pub fn settings_appearance_content_height(viewport: Size) -> f32 {
    SETTINGS_SECTION_LABEL_H
        + SETTINGS_SECTION_LABEL_H
        + crate::theme_picker::appearance_content_height(settings_appearance_inner_width(viewport))
        + SETTINGS_SECTION_GAP
}

#[cfg(test)]
mod m1_tests {
    use super::*;

    fn vp() -> Size {
        Size {
            width: 800.0,
            height: 600.0,
        }
    }

    #[test]
    fn m1_panel_centred_x_top_anchored_y() {
        let p = settings_panel_rect_m1(vp());
        let expected_x = (vp().width - SETTINGS_PANEL_WIDTH_M1) * 0.5;
        assert!((p.x - expected_x).abs() < 0.01);
        assert!((p.y - SETTINGS_PANEL_TOP_MARGIN).abs() < 0.01);
        assert_eq!(p.width, SETTINGS_PANEL_WIDTH_M1);
        // 600 - 16 * 2 = 568 < 700 → clamp to 568.
        assert!(p.height <= SETTINGS_PANEL_HEIGHT_MAX);
    }

    #[test]
    fn m1_header_sticky_at_top_of_panel() {
        let p = settings_panel_rect_m1(vp());
        let h = settings_header_rect(vp());
        assert_eq!(h.x, p.x);
        assert_eq!(h.y, p.y);
        assert_eq!(h.width, p.width);
        assert_eq!(h.height, SETTINGS_HEADER_H_M1);
    }

    #[test]
    fn m1_footer_sticky_at_bottom_of_panel() {
        let p = settings_panel_rect_m1(vp());
        let f = settings_footer_rect(vp());
        assert_eq!(f.x, p.x);
        assert!((f.bottom() - p.bottom()).abs() < 0.01);
        assert_eq!(f.height, SETTINGS_FOOTER_H);
    }

    #[test]
    fn m1_body_sits_between_header_and_footer() {
        let body = settings_body_rect(vp());
        let header = settings_header_rect(vp());
        let footer = settings_footer_rect(vp());
        assert!((body.y - header.bottom()).abs() < 0.01);
        assert!((body.bottom() - footer.y).abs() < 0.01);
    }

    #[test]
    fn m1_close_button_inside_header() {
        let h = settings_header_rect(vp());
        let c = settings_close_button_rect_m1(vp());
        assert!(c.y >= h.y);
        assert!(c.bottom() <= h.bottom());
        assert!(c.right() <= h.right());
        assert_eq!(c.width, SETTINGS_CLOSE_X_SIZE);
    }

    #[test]
    fn m1_footer_buttons_paired_cancel_left_save_right() {
        let cancel = settings_cancel_button_rect(vp());
        let save = settings_save_button_rect(vp());
        assert_eq!(cancel.y, save.y);
        assert_eq!(cancel.width, save.width);
        assert!(cancel.right() < save.x);
        let f = settings_footer_rect(vp());
        assert!(save.right() <= f.right());
    }

    #[test]
    fn m1_top_toggle_rows_stack_vertically_in_order() {
        let v = vp();
        let r0 = settings_top_toggle_row_rect(v, 0.0, 0);
        let r1 = settings_top_toggle_row_rect(v, 0.0, 1);
        let r4 = settings_top_toggle_row_rect(v, 0.0, 4);
        assert!(r0.y < r1.y);
        assert!(r1.y < r4.y);
        assert!((r1.y - r0.y - SETTINGS_ROW_H_M1).abs() < 0.01);
    }

    #[test]
    fn m1_top_toggle_hit_rect_sits_in_row_right_half() {
        let v = vp();
        let row = settings_top_toggle_row_rect(v, 0.0, 0);
        let hit = settings_top_toggle_hit_rect(v, 0.0, 0);
        assert!(hit.x >= row.x + row.width * 0.5);
        assert!(hit.right() <= row.right());
        assert_eq!(hit.width, SETTINGS_TOP_TOGGLE_HIT_W);
        assert_eq!(hit.height, SETTINGS_TOP_TOGGLE_HIT_H);
    }

    #[test]
    fn m1_language_row_sits_below_top_5_toggle_rows() {
        let v = vp();
        let last_toggle = settings_top_toggle_row_rect(v, 0.0, 4);
        let lang = settings_language_row_rect(v, 0.0);
        assert!((lang.y - last_toggle.bottom()).abs() < 0.01);
    }

    #[test]
    fn m1_language_chip_inside_language_row_right_half() {
        let v = vp();
        let row = settings_language_row_rect(v, 0.0);
        let chip = settings_language_chip_rect(v, 0.0);
        assert!(chip.x >= row.x + row.width * 0.5);
        assert!(chip.right() <= row.right());
        assert_eq!(chip.width, SETTINGS_LANGUAGE_CHIP_W);
        assert_eq!(chip.height, SETTINGS_LANGUAGE_CHIP_H);
    }

    #[test]
    fn m1_scroll_offset_shifts_rows_up() {
        let v = vp();
        let row0_at_0 = settings_top_toggle_row_rect(v, 0.0, 0);
        let row0_at_50 = settings_top_toggle_row_rect(v, 50.0, 0);
        assert!((row0_at_50.y + 50.0 - row0_at_0.y).abs() < 0.01);
    }

    /// α4 (Wave I-α) / G3 parity (2026-06-01) — the zone-display-mode picker is
    /// now the §4 DisplayMode group (promoted out of the General band), sitting
    /// below its own group title which itself sits below the §3 Appearance
    /// section. The picker row starts exactly where the §4 group title ends.
    #[test]
    fn alpha4_zone_display_picker_row_sits_below_display_mode_group_title() {
        let v = vp();
        let title = settings_display_mode_label_rect(v, 0.0);
        let picker = settings_zone_display_mode_picker_row_rect(v, 0.0);
        assert!(
            (picker.y - title.bottom()).abs() < 0.01,
            "picker row must start exactly where the §4 group title ends \
             (title.bottom={}, picker.y={})",
            title.bottom(),
            picker.y,
        );
        assert_eq!(picker.height, SETTINGS_ROW_H_M1);
        // §4 DisplayMode sits below §3 Appearance (the appearance accent row),
        // a full section gap clear — the General band no longer contains it.
        let appearance = settings_appearance_label_rect(v, 0.0, &plugin_flags(0));
        assert!(
            title.y > appearance.bottom(),
            "§4 DisplayMode group title (y={}) must sit below §3 Appearance \
             label (bottom={})",
            title.y,
            appearance.bottom(),
        );
        // It is no longer wedged into the General band right under Language.
        let lang = settings_language_row_rect(v, 0.0);
        assert!(
            title.y > lang.bottom() + SETTINGS_SECTION_GAP,
            "§4 DisplayMode must be promoted well below the General band's \
             Language row (title.y={}, lang.bottom={})",
            title.y,
            lang.bottom(),
        );
    }

    #[test]
    fn alpha4_three_radios_pack_left_to_right_inside_picker_row() {
        let v = vp();
        let row = settings_zone_display_mode_picker_row_rect(v, 0.0);
        let r0 = settings_zone_display_mode_radio_rect(v, 0.0, 0);
        let r1 = settings_zone_display_mode_radio_rect(v, 0.0, 1);
        let r2 = settings_zone_display_mode_radio_rect(v, 0.0, 2);
        assert_eq!(r0.y, r1.y);
        assert_eq!(r1.y, r2.y);
        assert!(r0.right() <= r1.x);
        assert!(r1.right() <= r2.x);
        // The whole cluster right-anchors at row.right() and never pokes
        // outside the row. Cluster width (78×3 + 4×2 = 242 DIP) leaves the
        // first radio at ~row.width × 0.36; allow a 0.3 floor so the
        // assertion stays meaningful across SETTINGS_PANEL_WIDTH_M1 tweaks.
        assert!(r0.x >= row.x + row.width * 0.3);
        assert!(r2.right() <= row.right());
        // Per-radio dimensions pinned.
        assert_eq!(r0.width, SETTINGS_RADIO_W);
        assert_eq!(r0.height, SETTINGS_RADIO_H);
    }

    #[test]
    fn alpha4_radio_inner_dot_sits_inside_outer_circle() {
        let v = vp();
        for index in 0..SETTINGS_ZONE_DISPLAY_MODE_COUNT {
            let outer = settings_zone_display_mode_radio_outer_rect(v, 0.0, index);
            let inner = settings_zone_display_mode_radio_inner_rect(v, 0.0, index);
            assert!(inner.x >= outer.x);
            assert!(inner.y >= outer.y);
            assert!(inner.right() <= outer.right());
            assert!(inner.bottom() <= outer.bottom());
            assert_eq!(inner.width, SETTINGS_RADIO_INNER_D);
            assert_eq!(outer.width, SETTINGS_RADIO_OUTER_D);
        }
    }

    #[test]
    fn alpha4_radio_label_sits_right_of_outer_circle() {
        let v = vp();
        for index in 0..SETTINGS_ZONE_DISPLAY_MODE_COUNT {
            let outer = settings_zone_display_mode_radio_outer_rect(v, 0.0, index);
            let label = settings_zone_display_mode_radio_label_rect(v, 0.0, index);
            assert!(label.x >= outer.right());
        }
    }

    /// α4 / G3 parity (2026-06-01) — the relationship INVERTED: the §2 Paths
    /// sources section now sits ABOVE the §4 DisplayMode picker (Tauri body
    /// order General → **Paths** → Appearance → **DisplayMode**). The picker is
    /// no longer wedged between the General band and §2 Paths.
    #[test]
    fn g3_m2_sources_section_sits_above_display_mode_picker() {
        let v = vp();
        let picker = settings_zone_display_mode_picker_row_rect(v, 0.0);
        let sources_label = settings_sources_label_rect(v, 0.0);
        // §2 Paths sources label must sit ABOVE the §4 picker row top.
        assert!(
            sources_label.bottom() <= picker.y,
            "§2 Paths sources label (bottom={}) must sit above the §4 \
             DisplayMode picker row (y={}) post-G3 reorder",
            sources_label.bottom(),
            picker.y,
        );
    }

    #[test]
    fn m1_body_max_scroll_floors_at_zero_when_content_fits() {
        let v = vp();
        let max = settings_body_max_scroll(10.0, v);
        assert_eq!(max, 0.0);
    }

    #[test]
    fn m1_body_max_scroll_returns_overflow_when_content_taller_than_body() {
        let v = vp();
        let body = settings_body_rect(v);
        let max = settings_body_max_scroll(body.height + 120.0, v);
        assert!((max - 120.0).abs() < 0.01);
    }

    #[test]
    fn m1_clamp_scroll_never_goes_negative() {
        let v = vp();
        let f = SettingsBodyFlags::new(true, true, false, false, UpdaterHeightKind::StatusOnly);
        assert_eq!(settings_clamp_scroll(0.0, -100.0, v, &f), 0.0);
        assert_eq!(settings_clamp_scroll(20.0, -100.0, v, &f), 0.0);
    }

    #[test]
    fn m1_clamp_scroll_caps_at_max() {
        let v = vp();
        let f = SettingsBodyFlags::new(true, true, false, false, UpdaterHeightKind::StatusOnly);
        let content = settings_body_content_height(v, &f);
        let max = settings_body_max_scroll(content, v);
        assert_eq!(settings_clamp_scroll(0.0, max + 999.0, v, &f), max);
    }

    #[test]
    fn m1_top_toggle_count_pinned() {
        assert_eq!(SETTINGS_TOP_TOGGLE_COUNT, 5);
    }

    #[test]
    // Intentional const guard: asserts the const shadow-alpha stays at 0.0,
    // so clippy sees a constant value (that is the regression lock).
    #[allow(clippy::assertions_on_constants)]
    fn v5_panel_shadow_alpha_locked_at_zero() {
        // V-5 (TL re-issue 2026-05-21) — the 8-DIP hard-edged drop-shadow
        // ring used to paint at 0.45 (v1) / 0.15 (v2). Both reading as a
        // visible "mask ring" on the wallpaper because `fill_rounded_rect`
        // has no gaussian falloff. The re-issued V-5 contract requires
        // "panel 外只露桌面 wallpaper, 不出现任何 BentoDesk-painted overlay
        // 圈" so the alpha is locked at 0.0 (early-returns out of
        // `fill_rounded_rect` in render.rs at `color.a <= 0.0`).
        // Re-introducing any non-zero alpha resurrects the regression until
        // a gaussian-blur drop-shadow API lands (carry-over task #13).
        assert!(
            SETTINGS_PANEL_SHADOW_ALPHA <= 0.0,
            "panel shadow alpha {} would render as a hard-edged halo / \
             mask ring; keep at 0.0 until a gaussian-blur drop-shadow \
             API lands (carry-over task #13)",
            SETTINGS_PANEL_SHADOW_ALPHA,
        );
        assert!((SETTINGS_PANEL_SHADOW_ALPHA - 0.0).abs() < f32::EPSILON);
    }

    // ── M1h — Plugins §11 inline geometry ──────────────────────────────

    /// Helper: a base flag set with the Plugins section anchored after an empty
    /// Backup list (the shipped layout while Encryption §10 is deferred).
    fn plugin_flags(plugin_rows: usize) -> SettingsBodyFlags {
        SettingsBodyFlags::new(true, true, false, false, UpdaterHeightKind::StatusOnly)
            .with_backup_rows(0)
            .with_plugin_rows(plugin_rows)
    }

    #[test]
    fn m1h_plugins_section_sits_below_encryption_card() {
        // M7 (2026-06-01) — Plugins §11 now anchors off the Encryption §10
        // card's reserved status row (the card slots between Backup §9 and
        // Plugins §11), not directly off the Backup card. The Backup card still
        // sits ABOVE the encryption card, and the encryption status row sits
        // ABOVE the Plugins group title, separated by a section gap.
        let v = vp();
        let f = plugin_flags(2);
        let backup_empty = settings_backup_entry_row_rect(v, 0.0, &f, 0);
        let encryption_status = settings_encryption_status_rect(v, 0.0, &f);
        let plugin_label = settings_plugins_label_rect(v, 0.0, &f);
        // Encryption card sits below the backup card's last row.
        assert!(encryption_status.y >= backup_empty.bottom());
        // Plugins title sits below the encryption card's status row.
        assert!(
            plugin_label.y >= encryption_status.bottom(),
            "plugins title (y={}) must sit below the encryption card's status \
             row (bottom={})",
            plugin_label.y,
            encryption_status.bottom(),
        );
        // A section gap separates them.
        assert!((plugin_label.y - encryption_status.bottom() - SETTINGS_SECTION_GAP).abs() < 0.01);
    }

    #[test]
    fn m1h_install_button_full_width_below_title() {
        let v = vp();
        let f = plugin_flags(0);
        let label = settings_plugins_label_rect(v, 0.0, &f);
        let install = settings_plugins_install_button_rect(v, 0.0, &f);
        // Install button sits directly below the title and spans the same
        // (full body) width.
        assert!((install.y - label.bottom()).abs() < 0.01);
        assert_eq!(install.x, label.x);
        assert_eq!(install.width, label.width);
        assert_eq!(install.height, SETTINGS_PLUGIN_INSTALL_BTN_H);
    }

    #[test]
    fn m1h_plugin_cards_stack_vertically_below_install() {
        let v = vp();
        let f = plugin_flags(3);
        let install = settings_plugins_install_button_rect(v, 0.0, &f);
        let card0 = settings_plugin_card_rect(v, 0.0, &f, 0);
        let card1 = settings_plugin_card_rect(v, 0.0, &f, 1);
        let card2 = settings_plugin_card_rect(v, 0.0, &f, 2);
        // First card sits below the install button (plus the leading gap).
        assert!(card0.y >= install.bottom());
        // Cards stack with a fixed step = card height + inter-card gap.
        assert!(card0.y < card1.y);
        assert!(card1.y < card2.y);
        assert!((card1.y - card0.y - (SETTINGS_PLUGIN_CARD_H + SETTINGS_PLUGIN_CARD_GAP)).abs() < 0.01);
        assert_eq!(card0.height, SETTINGS_PLUGIN_CARD_H);
    }

    #[test]
    fn m1h_card_controls_fit_inside_card_in_order() {
        let v = vp();
        let f = plugin_flags(1);
        let card = settings_plugin_card_rect(v, 0.0, &f, 0);
        let name = settings_plugin_name_rect(card);
        let badge = settings_plugin_badge_rect(card);
        let toggle = settings_plugin_toggle_hit_rect(card);
        let author = settings_plugin_author_rect(card);
        let desc = settings_plugin_desc_rect(card);
        let uninstall = settings_plugin_uninstall_button_rect(card);
        // Header sub-row: name | badge | toggle, packed left→right inside card.
        assert!(name.right() <= badge.x);
        assert!(badge.right() <= toggle.x);
        assert!(toggle.right() <= card.right() + 0.01);
        // Vertical stack: header → author → desc → actions (uninstall), all
        // inside the card.
        assert!(author.y >= name.y);
        assert!(desc.y >= author.bottom() - 0.01);
        assert!(uninstall.y >= desc.bottom() - 0.01);
        assert!(uninstall.bottom() <= card.bottom() + 0.01);
        assert!(uninstall.right() <= card.right() + 0.01);
    }

    #[test]
    fn m1h_plugins_content_height_grows_with_capped_row_count() {
        // 0 (empty placeholder) < few < cap == over-cap (capped).
        let none = settings_plugins_content_height(0);
        let one = settings_plugins_content_height(1);
        let few = settings_plugins_content_height(3);
        let at_cap = settings_plugins_content_height(SETTINGS_PLUGINS_ROW_VISIBLE_MAX);
        let over_cap = settings_plugins_content_height(SETTINGS_PLUGINS_ROW_VISIBLE_MAX + 4);
        assert!(none > 0.0);
        assert!(one > none);
        assert!(few > one);
        // Over-cap clamps to the cap height (visible-row cap honoured).
        assert!((over_cap - at_cap).abs() < f32::EPSILON);
        // The empty-state height is the title + install + gap + one empty row.
        let expected_empty = SETTINGS_SECTION_LABEL_H
            + SETTINGS_PLUGIN_INSTALL_BTN_H
            + SETTINGS_PLUGIN_CARD_GAP
            + SETTINGS_PLUGIN_EMPTY_ROW_H
            + SETTINGS_SECTION_GAP;
        assert!((none - expected_empty).abs() < 0.01);
    }

    #[test]
    fn m1h_plugin_row_count_feeds_body_height_and_scroll() {
        let v = vp();
        // Adding plugin rows must strictly grow the total body content height
        // (so the scroll clamp lets the user reach the new cards).
        let h0 = settings_body_content_height(v, &plugin_flags(0));
        let h2 = settings_body_content_height(v, &plugin_flags(2));
        assert!(h2 > h0);
        // The growth equals the plugins-section delta exactly (no other section
        // depends on plugin_row_count).
        let delta_section =
            settings_plugins_content_height(2) - settings_plugins_content_height(0);
        assert!((h2 - h0 - delta_section).abs() < 0.01);
    }

    // ── M7 — Encryption §10 inline geometry ────────────────────────────────

    #[test]
    fn m7_encryption_section_ordering() {
        // The §10 card label must sit BELOW the Backup card's last row and
        // ABOVE the Plugins group title (anchored between §9 and §11).
        let v = vp();
        let f = plugin_flags(0);
        let backup_last = settings_backup_entry_row_rect(v, 0.0, &f, 0);
        let enc_label = settings_encryption_label_rect(v, 0.0, &f);
        let plugin_label = settings_plugins_label_rect(v, 0.0, &f);
        assert!(
            enc_label.y >= backup_last.bottom() + SETTINGS_SECTION_GAP - 0.01,
            "encryption label (y={}) must sit a section gap below the backup \
             card's last row (bottom={})",
            enc_label.y,
            backup_last.bottom(),
        );
        assert!(
            enc_label.y < plugin_label.y,
            "encryption label (y={}) must sit above the plugins label (y={})",
            enc_label.y,
            plugin_label.y,
        );
    }

    #[test]
    fn m7_encryption_content_height_is_fixed_and_positive() {
        // Fixed-height card (no variable rows): the helper is a constant and
        // must equal the sum of its laid-out rows.
        let h = settings_encryption_content_height();
        assert!(h > 0.0);
        // P13 (#7 fix wave) — 7 rows separated by 6 × 10px inter-row gaps
        // (replacing the old single 8px pre-passphrase button gap).
        let expected = SETTINGS_SECTION_LABEL_H
            + SETTINGS_ENCRYPTION_ROW_H
            + SETTINGS_ENCRYPTION_ROW_H
            + SETTINGS_ENCRYPTION_BTN_ROW_H
            + SETTINGS_ENCRYPTION_INPUT_ROW_H
            + SETTINGS_ENCRYPTION_ROW_H
            + SETTINGS_ENCRYPTION_ROW_H
            + SETTINGS_ENCRYPTION_ROW_GAP * 6.0
            + SETTINGS_SECTION_GAP;
        assert!((h - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn m7_settings_body_content_height_includes_encryption() {
        // The total body height must grow by exactly the encryption card's
        // fixed height vs a hypothetical body without it. We assert the live
        // total minus the sum of all OTHER sections equals the encryption
        // term (i.e. the term is actually included once).
        let v = vp();
        let f = plugin_flags(0);
        let total = settings_body_content_height(v, &f);
        // G3 parity (2026-06-01) — the `others` sum now includes the §4
        // DisplayMode group height (promoted out of the General band into its
        // own section). Without it the body total no longer matches the sum of
        // every non-encryption section.
        let others = settings_m2_content_height(v, f.source_row_count)
            + settings_appearance_content_height(v)
            + settings_display_mode_content_height()
            + settings_perf_startup_content_height(
                v,
                f.crash_restart_enabled,
                f.safe_start_after_hibernation,
            )
            + settings_stealth_content_height(f.stealth_has_retry, f.stealth_has_error)
            + settings_updater_content_height(f.updater_kind)
            + settings_backup_content_height(f.backup_row_count)
            + settings_plugins_content_height(f.plugin_row_count);
        assert!((total - others - settings_encryption_content_height()).abs() < 0.01);
    }

    #[test]
    fn m7_encryption_mode_buttons_are_three_non_overlapping_rects() {
        let v = vp();
        let f = plugin_flags(0);
        let b0 = settings_encryption_mode_button_rect(v, 0.0, &f, 0);
        let b1 = settings_encryption_mode_button_rect(v, 0.0, &f, 1);
        let b2 = settings_encryption_mode_button_rect(v, 0.0, &f, 2);
        // Same row (same y/height), increasing x, no overlap.
        assert_eq!(b0.y, b1.y);
        assert_eq!(b1.y, b2.y);
        assert!(b0.width > 0.0);
        assert!(b1.x >= b0.right() - 0.01);
        assert!(b2.x >= b1.right() - 0.01);
        // All three fit inside the mode-grid row band.
        let row = settings_encryption_mode_row_rect(v, 0.0, &f);
        assert!(b0.x >= row.x - 0.01);
        assert!(b2.right() <= row.right() + 0.01);
    }

    #[test]
    fn m7_encryption_rows_stack_in_order() {
        // label → desc → current-mode → mode-grid → passphrase input → hint →
        // status, each strictly below the previous.
        let v = vp();
        let f = plugin_flags(0);
        let label = settings_encryption_label_rect(v, 0.0, &f);
        let desc = settings_encryption_desc_rect(v, 0.0, &f);
        let current = settings_encryption_current_mode_rect(v, 0.0, &f);
        let mode_row = settings_encryption_mode_row_rect(v, 0.0, &f);
        let input = settings_encryption_passphrase_input_rect(v, 0.0, &f);
        let hint = settings_encryption_hint_rect(v, 0.0, &f);
        let status = settings_encryption_status_rect(v, 0.0, &f);
        assert!(desc.y >= label.bottom() - 0.01);
        assert!(current.y >= desc.bottom() - 0.01);
        assert!(mode_row.y >= current.bottom() - 0.01);
        assert!(input.y >= mode_row.bottom() - 0.01);
        assert!(hint.y >= input.bottom() - 0.01);
        assert!(status.y >= hint.bottom() - 0.01);
    }

    /// P13 (#7 fix wave 2026-06-01) — every sibling row of the §10 card is
    /// separated by EXACTLY the 10px Tauri `gap` (`.encryption-card { gap:10px }`).
    /// Pin each inter-row gap so the rhythm can't silently regress to 0px.
    #[test]
    fn p13_encryption_rows_separated_by_ten_px_gap() {
        let v = vp();
        let f = plugin_flags(0);
        let label = settings_encryption_label_rect(v, 0.0, &f);
        let desc = settings_encryption_desc_rect(v, 0.0, &f);
        let current = settings_encryption_current_mode_rect(v, 0.0, &f);
        let mode_row = settings_encryption_mode_row_rect(v, 0.0, &f);
        let pass_row = settings_encryption_passphrase_row_rect(v, 0.0, &f);
        let hint = settings_encryption_hint_rect(v, 0.0, &f);
        let status = settings_encryption_status_rect(v, 0.0, &f);
        let g = SETTINGS_ENCRYPTION_ROW_GAP;
        assert!((desc.y - label.bottom() - g).abs() < 0.01);
        assert!((current.y - desc.bottom() - g).abs() < 0.01);
        assert!((mode_row.y - current.bottom() - g).abs() < 0.01);
        assert!((pass_row.y - mode_row.bottom() - g).abs() < 0.01);
        assert!((hint.y - pass_row.bottom() - g).abs() < 0.01);
        assert!((status.y - hint.bottom() - g).abs() < 0.01);
    }

    /// P4 (#7 fix wave 2026-06-01) — the passphrase row splits into a LEFT label
    /// cell + a RIGHT input box (Tauri `justify-content: space-between`). The
    /// label sits on the left, the input fills the rest, they don't overlap, and
    /// the input no longer spans the full row width (so a click on the label cell
    /// is NOT a focus hit).
    #[test]
    fn p4_passphrase_row_splits_label_and_input() {
        let v = vp();
        let f = plugin_flags(0);
        let row = settings_encryption_passphrase_row_rect(v, 0.0, &f);
        let label = settings_encryption_passphrase_label_rect(v, 0.0, &f);
        let input = settings_encryption_passphrase_input_rect(v, 0.0, &f);
        // Label is the left cell, input is to its right, no overlap.
        assert!((label.x - row.x).abs() < 0.01, "label hugs the row's left edge");
        assert!(input.x >= label.right() - 0.01, "input sits right of the label");
        assert!(input.x > label.right(), "a gap separates label and input");
        // Input ends at the row's right edge (fills the remaining width).
        assert!((input.right() - row.right()).abs() < 0.01);
        // Input is strictly narrower than the full row (label cell + gap removed).
        assert!(input.width < row.width - SETTINGS_ENCRYPTION_PASS_LABEL_W * 0.5);
        // Same vertical band as the row.
        assert!((label.y - row.y).abs() < 0.01 && (input.y - row.y).abs() < 0.01);
    }
}

#[cfg(test)]
mod m2_tests {
    use super::*;

    fn vp() -> Size {
        Size {
            width: 800.0,
            height: 800.0,
        }
    }

    #[test]
    fn m2_sources_label_sits_below_language_row() {
        let v = vp();
        let lang = settings_language_row_rect(v, 0.0);
        let label = settings_sources_label_rect(v, 0.0);
        assert!(label.y >= lang.bottom() + SETTINGS_SECTION_GAP - 0.01);
    }

    #[test]
    fn m2_source_rows_stack_vertically_below_label() {
        let v = vp();
        let label = settings_sources_label_rect(v, 0.0);
        let r0 = settings_source_row_rect(v, 0.0, 0);
        let r1 = settings_source_row_rect(v, 0.0, 1);
        assert!(r0.y >= label.bottom() - 0.01);
        assert!((r1.y - r0.bottom() - SETTINGS_SOURCE_GAP).abs() < 0.01);
    }

    #[test]
    fn m1i_sources_refresh_button_is_last_child_below_cards() {
        // M1i fidelity — the refresh button is the LAST child of the list,
        // right-anchored BELOW the live card stack (not on the heading row).
        let v = vp();
        let label = settings_sources_label_rect(v, 0.0);
        let refresh = settings_sources_refresh_button_rect(v, 0.0, 4);
        let last_card = settings_source_row_rect(v, 0.0, 3);
        assert!((refresh.right() - label.right()).abs() < 0.01);
        // Sits below the last card (heading-row anchor would put it at label.y).
        assert!(refresh.y >= last_card.bottom() - 0.01);
        assert!(refresh.y > label.bottom());
        assert_eq!(refresh.width, SETTINGS_SOURCE_REFRESH_BTN_W);
    }

    #[test]
    fn m1i_refresh_button_follows_live_card_count() {
        // Fewer live cards → the refresh button rides up by exactly the height
        // of each missing card slot.
        let v = vp();
        let r4 = settings_sources_refresh_button_rect(v, 0.0, 4);
        let r2 = settings_sources_refresh_button_rect(v, 0.0, 2);
        let per_card = SETTINGS_SOURCE_ROW_H + SETTINGS_SOURCE_GAP;
        assert!((r4.y - r2.y - 2.0 * per_card).abs() < 0.01);
    }

    #[test]
    fn m2_desktop_path_input_sits_below_last_source() {
        // Existing invariant must still hold at the full 4-card reserve.
        let v = vp();
        let refresh = settings_sources_refresh_button_rect(
            v,
            0.0,
            SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize,
        );
        let label =
            settings_desktop_path_label_rect(v, 0.0, SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
        let input =
            settings_desktop_path_input_rect(v, 0.0, SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
        assert!(label.y >= refresh.bottom() + SETTINGS_SECTION_GAP - 0.01);
        assert!((input.y - label.bottom()).abs() < 0.01);
        assert_eq!(input.height, SETTINGS_INPUT_ROW_H);
    }

    #[test]
    fn m1i_desktop_path_reflows_with_live_source_count() {
        // M1i fidelity — the 桌面路径 row sits HIGHER with 2 sources than with
        // 4, by exactly 2*(card_height + gap) (Tauri's flex column).
        let v = vp();
        let input2 = settings_desktop_path_input_rect(v, 0.0, 2);
        let input4 = settings_desktop_path_input_rect(v, 0.0, 4);
        let per_card = SETTINGS_SOURCE_ROW_H + SETTINGS_SOURCE_GAP;
        assert!((input4.y - input2.y - 2.0 * per_card).abs() < 0.01);
        assert!(input2.y < input4.y);
    }

    #[test]
    fn m2_watch_textarea_sits_below_path_input() {
        let v = vp();
        let input = settings_desktop_path_input_rect(v, 0.0, 4);
        let label = settings_watch_label_rect(v, 0.0, 4);
        let area = settings_watch_textarea_rect(v, 0.0, 4);
        assert!(label.y >= input.bottom() + SETTINGS_SECTION_GAP - 0.01);
        assert!((area.y - label.bottom()).abs() < 0.01);
        assert_eq!(area.height, SETTINGS_TEXTAREA_H);
    }

    #[test]
    fn m2_content_height_exceeds_body_to_trigger_scroll() {
        let v = vp();
        let body = settings_body_rect(v);
        let content_h = settings_m2_content_height(v, SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
        // M2 should make the body scroll on an 800×800 viewport (since the
        // panel caps at 700 DIP height, body band is ~596 DIP). Five toggles
        // + language + 桌面源(4 cards) + 桌面路径 + 监控值 must exceed body.
        assert!(content_h > body.height);
    }

    #[test]
    fn m2_scroll_offset_shifts_m2_sections_up() {
        let v = vp();
        let r_at_0 = settings_sources_label_rect(v, 0.0);
        let r_at_30 = settings_sources_label_rect(v, 30.0);
        assert!((r_at_30.y + 30.0 - r_at_0.y).abs() < 0.01);
    }

    #[test]
    fn m1i_source_cap_is_four() {
        // The §2 list caps at the 4-slot Windows ceiling (User/Public/
        // OneDrive/Custom). Beyond that the live count is clamped.
        assert_eq!(SETTINGS_SOURCE_ROW_VISIBLE_MAX, 4);
    }

    #[test]
    fn m1i_sources_content_height_reflows_with_count() {
        // M1i fidelity — the source-block height now GROWS with the live count
        // (one card_height + gap per card), and is clamped at the cap.
        let at1 = settings_sources_content_height(1);
        let at2 = settings_sources_content_height(2);
        let at_cap = settings_sources_content_height(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
        let over = settings_sources_content_height(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize + 7);
        let per_card = SETTINGS_SOURCE_ROW_H + SETTINGS_SOURCE_GAP;
        assert!((at2 - at1 - per_card).abs() < 0.01);
        assert!(at2 > at1);
        assert!(at_cap > at2);
        // Clamped past the cap.
        assert!((over - at_cap).abs() < 0.01);
    }

    #[test]
    fn m1i_reserve_delta_shrinks_with_live_count() {
        // The scroll-fold delta is 0 at the full reserve and grows as cards
        // are missing — exactly the blank space the old fixed reserve left.
        let d_full = settings_sources_reserve_delta(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
        let d2 = settings_sources_reserve_delta(2);
        let per_card = SETTINGS_SOURCE_ROW_H + SETTINGS_SOURCE_GAP;
        assert!(d_full.abs() < 0.01);
        assert!((d2 - 2.0 * per_card).abs() < 0.01);
    }

    #[test]
    fn m1i_empty_list_uses_placeholder_height() {
        // Empty list: the block reserves one placeholder line + the refresh
        // button, not zero — so downstream sections do not collide upward.
        let empty = settings_sources_content_height(0);
        let label_plus_gap = SETTINGS_SECTION_LABEL_H + SETTINGS_SECTION_GAP;
        let stack = SETTINGS_SOURCE_EMPTY_H + SETTINGS_SOURCE_REFRESH_GAP + SETTINGS_SOURCE_REFRESH_BTN_H;
        assert!((empty - (label_plus_gap + stack)).abs() < 0.01);
    }
}

#[cfg(test)]
mod m1d_tests {
    //! M1d 2026-05-29 — Performance §5 + Startup management §6 geometry.
    //! Replaces the deleted m3 advanced/overlay tests.
    use super::*;

    fn vp() -> Size {
        Size {
            width: 800.0,
            height: 600.0,
        }
    }

    #[test]
    fn perf_label_sits_below_m2_textarea() {
        // The Performance §5 label roots at the FIXED 4-card reserve baseline
        // (scroll 0, no reflow delta), so it must clear the §2 watch textarea
        // computed at the same full reserve (count = cap). G3 parity
        // (2026-06-01): §3 Appearance + §4 DisplayMode now sit BETWEEN §2 Paths
        // and §5 Performance, so the perf label clears the textarea by even more
        // than pre-G3 (the `>=` still holds — and now with extra slack).
        let v = vp();
        let textarea =
            settings_watch_textarea_rect(v, 0.0, SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
        let label = settings_performance_label_rect(v, 0.0);
        assert!(label.y >= textarea.bottom());
        // The §4 DisplayMode picker (the section directly above Performance)
        // must end at-or-above the perf label — pin the new adjacency.
        let picker = settings_zone_display_mode_picker_row_rect(v, 0.0);
        assert!(
            label.y >= picker.bottom(),
            "Performance §5 label (y={}) must sit below the §4 DisplayMode \
             picker row (bottom={})",
            label.y,
            picker.bottom(),
        );
    }

    #[test]
    fn m1i_perf_reflows_via_reserve_delta() {
        // M1i fidelity — folding the reserve delta into scroll shifts the
        // Performance label (and everything below it) UP by exactly the delta,
        // proving the single-base-offset reflow reaches the lower sections.
        let v = vp();
        let base = settings_performance_label_rect(v, 0.0);
        let delta = settings_sources_reserve_delta(2);
        let reflowed = settings_performance_label_rect(v, delta);
        assert!(delta > 0.0);
        assert!((base.y - reflowed.y - delta).abs() < 0.01);
    }

    #[test]
    fn perf_slider_rows_stack_vertically() {
        let v = vp();
        let r0 = settings_performance_slider_row_rect(v, 0.0, 0);
        let r1 = settings_performance_slider_row_rect(v, 0.0, 1);
        let r2 = settings_performance_slider_row_rect(v, 0.0, 2);
        assert!((r1.y - r0.bottom()).abs() < 0.01);
        assert!((r2.y - r1.bottom()).abs() < 0.01);
        assert_eq!(r0.height, SETTINGS_SLIDER_ROW_H);
    }

    #[test]
    fn perf_slider_track_sits_on_lower_line_full_width() {
        let v = vp();
        for index in 0..SETTINGS_PERF_ROW_COUNT {
            let row = settings_performance_slider_row_rect(v, 0.0, index);
            let track = settings_performance_slider_rect(v, 0.0, index);
            // Track on the lower line (below the label/value line).
            assert!(track.y > row.y + row.height * 0.4);
            assert!(track.bottom() <= row.bottom() + 0.01);
            assert!((track.x - row.x).abs() < 0.01);
            assert!((track.width - row.width).abs() < 0.01);
        }
    }

    #[test]
    fn perf_row_count_pinned() {
        assert_eq!(SETTINGS_PERF_ROW_COUNT, 3);
    }

    #[test]
    fn startup_label_sits_below_performance_section() {
        let v = vp();
        let last_perf =
            settings_performance_slider_row_rect(v, 0.0, SETTINGS_PERF_ROW_COUNT - 1);
        let startup = settings_startup_label_rect(v, 0.0);
        assert!(startup.y >= last_perf.bottom() + SETTINGS_SECTION_GAP - 0.01);
    }

    #[test]
    fn startup_always_rows_stack_with_desc_gaps() {
        let v = vp();
        let label = settings_startup_label_rect(v, 0.0);
        let high = settings_startup_high_priority_row_rect(v, 0.0);
        let crash = settings_crash_restart_row_rect(v, 0.0);
        assert!((high.y - label.bottom()).abs() < 0.01);
        // crash row sits a full row + a desc-line below high priority.
        assert!((crash.y - (high.bottom() + SETTINGS_DESC_H)).abs() < 0.01);
    }

    #[test]
    fn startup_crash_steppers_only_chain_when_enabled() {
        let v = vp();
        let retries = settings_crash_max_retries_row_rect(v, 0.0);
        let window = settings_crash_window_row_rect(v, 0.0);
        // window stepper sits directly below the retries stepper.
        assert!((window.y - retries.bottom()).abs() < 0.01);
        // Steppers' − value + pack right-to-left.
        let plus = settings_stepper_plus_rect(retries);
        let value = settings_stepper_value_rect(retries);
        let minus = settings_stepper_minus_rect(retries);
        assert!(minus.right() <= value.x + 0.01);
        assert!(value.right() <= plus.x + 0.01);
        assert!(plus.right() <= retries.right() + 0.01);
        assert_eq!(plus.width, SETTINGS_NUM_BTN_W);
        assert_eq!(value.width, SETTINGS_NUM_VALUE_W);
    }

    #[test]
    fn safe_start_row_reflows_with_crash_restart_flag() {
        let v = vp();
        let off = settings_safe_start_row_rect(v, 0.0, false);
        let on = settings_safe_start_row_rect(v, 0.0, true);
        // Net effect of showing the two crash steppers is +2 stepper rows: the
        // crash-restart desc-clearing gap (SETTINGS_DESC_H) is present in BOTH
        // branches (OFF adds it directly; ON spends it on the retries-row gap),
        // so it cancels and the delta is exactly two row heights.
        assert!(on.y > off.y);
        assert!((on.y - off.y - SETTINGS_ROW_H_M1 * 2.0).abs() < 0.01);
    }

    #[test]
    fn hibernate_slider_sits_below_safe_start_when_shown() {
        let v = vp();
        let safe = settings_safe_start_row_rect(v, 0.0, true);
        let slider_row = settings_hibernate_slider_row_rect(v, 0.0, true);
        assert!((slider_row.y - (safe.bottom() + SETTINGS_DESC_H)).abs() < 0.01);
        assert_eq!(slider_row.height, SETTINGS_SLIDER_ROW_H);
        let track = settings_hibernate_slider_rect(v, 0.0, true);
        assert!(track.bottom() <= slider_row.bottom() + 0.01);
    }

    #[test]
    fn content_height_grows_with_conditional_rows() {
        let v = vp();
        // Both gates off → shortest. Crash on → +2 stepper rows. Hibernate on
        // → + slider row + desc. All on → tallest.
        let k = UpdaterHeightKind::StatusOnly;
        let none = settings_body_content_height(v, &SettingsBodyFlags::new(false, false, false, false, k));
        let crash = settings_body_content_height(v, &SettingsBodyFlags::new(true, false, false, false, k));
        let hib = settings_body_content_height(v, &SettingsBodyFlags::new(false, true, false, false, k));
        let both = settings_body_content_height(v, &SettingsBodyFlags::new(true, true, false, false, k));
        assert!(crash > none, "crash steppers must add height");
        assert!(hib > none, "hibernate slider must add height");
        assert!(both > crash);
        assert!(both > hib);
        // Crash adds a net 2 stepper rows (the desc-clearing gap cancels
        // between the two branches — see safe_start_row_reflows test).
        assert!((crash - none - SETTINGS_ROW_H_M1 * 2.0).abs() < 0.01);
    }

    #[test]
    fn content_height_exceeds_m2_total() {
        let v = vp();
        // `SettingsBodyFlags::new` defaults source_row_count to 0, so measure
        // the M2 block at the same count for an apples-to-apples comparison.
        let m2 = settings_m2_content_height(v, 0);
        let total = settings_body_content_height(
            v,
            &SettingsBodyFlags::new(true, true, false, false, UpdaterHeightKind::StatusOnly),
        );
        assert!(total > m2);
    }

    #[test]
    fn scroll_offset_shifts_performance_label_up() {
        let v = vp();
        let r_at_0 = settings_performance_label_rect(v, 0.0);
        let r_at_50 = settings_performance_label_rect(v, 50.0);
        assert!((r_at_50.y + 50.0 - r_at_0.y).abs() < 0.01);
    }

    // ── M1e — Stealth §7 geometry ──────────────────────────────────────

    #[test]
    fn m1e_stealth_title_sits_below_startup_section() {
        let v = vp();
        // With both Startup gates on, the Stealth title must clear the
        // hibernate slider row (the lowest Startup element).
        let startup_bottom =
            settings_hibernate_slider_row_rect(v, 0.0, true).bottom();
        let title = settings_stealth_label_rect(v, 0.0, true, true);
        assert!(
            (title.y - (startup_bottom + SETTINGS_SECTION_GAP)).abs() < 0.01,
            "stealth title must start a section gap below the last Startup row \
             (startup_bottom={}, title.y={})",
            startup_bottom,
            title.y,
        );
        assert_eq!(title.height, SETTINGS_SECTION_LABEL_H);
    }

    #[test]
    fn m1e_stealth_base_rows_stack_in_order() {
        let v = vp();
        let title = settings_stealth_label_rect(v, 0.0, true, true);
        let status = settings_stealth_status_row_rect(v, 0.0, true, true);
        let schema = settings_stealth_schema_row_rect(v, 0.0, true, true);
        let mirror = settings_stealth_mirror_row_rect(v, 0.0, true, true);
        assert!((status.y - title.bottom()).abs() < 0.01);
        assert!((schema.y - status.bottom()).abs() < 0.01);
        assert!((mirror.y - schema.bottom()).abs() < 0.01);
        // The status pill right-anchors inside the status row.
        let pill = settings_stealth_pill_rect(status);
        assert!(pill.right() <= status.right() + 0.01);
        assert!(pill.x >= status.x + status.width * 0.5);
        assert_eq!(pill.width, SETTINGS_STEALTH_PILL_W);
    }

    #[test]
    fn m1e_error_block_reflows_when_retry_row_present() {
        let v = vp();
        // Without a retry row, the error block hangs off the mirror row.
        let mirror = settings_stealth_mirror_row_rect(v, 0.0, true, true);
        let err_no_retry =
            settings_stealth_error_block_rect(v, 0.0, true, true, false);
        assert!((err_no_retry.y - mirror.bottom()).abs() < 0.01);
        // With a retry row, the error block sits a full retry row lower.
        let retry = settings_stealth_retry_row_rect(v, 0.0, true, true);
        let err_with_retry =
            settings_stealth_error_block_rect(v, 0.0, true, true, true);
        assert!((err_with_retry.y - retry.bottom()).abs() < 0.01);
        assert!(err_with_retry.y > err_no_retry.y);
    }

    #[test]
    fn m1e_buttons_paired_refresh_left_reapply_right() {
        let v = vp();
        let row = settings_stealth_buttons_row_rect(v, 0.0, true, true, false, false);
        let refresh = settings_stealth_refresh_button_rect(row);
        let reapply = settings_stealth_reapply_button_rect(row);
        assert_eq!(refresh.y, reapply.y);
        assert_eq!(refresh.width, reapply.width);
        assert!(refresh.right() < reapply.x);
        assert!(reapply.right() <= row.right() + 0.01);
    }

    #[test]
    fn m1e_onedrive_block_only_below_buttons() {
        let v = vp();
        let buttons = settings_stealth_buttons_row_rect(v, 0.0, true, true, true, false);
        let onedrive =
            settings_stealth_onedrive_block_rect(v, 0.0, true, true, true, false);
        assert!(onedrive.y > buttons.bottom());
        assert_eq!(onedrive.height, SETTINGS_STEALTH_ONEDRIVE_H);
    }

    #[test]
    fn m1e_stealth_content_height_grows_with_retry_and_error() {
        // Pure additive helper — base < +retry, base < +error, both tallest.
        let base = settings_stealth_content_height(false, false);
        let retry = settings_stealth_content_height(true, false);
        let error = settings_stealth_content_height(false, true);
        let both = settings_stealth_content_height(true, true);
        assert!(retry > base, "retry row + OneDrive block must add height");
        assert!(error > base, "last-error block must add height");
        assert!(both > retry);
        assert!(both > error);
        // The error branch adds exactly the error block height.
        assert!(
            (error - base - SETTINGS_STEALTH_ERROR_BLOCK_H).abs() < 0.01,
            "error-only delta must equal the error block height",
        );
        // The retry branch adds a retry row + the OneDrive block (+ its gap).
        assert!(
            (retry - base
                - SETTINGS_STEALTH_ROW_H
                - 8.0
                - SETTINGS_STEALTH_ONEDRIVE_H)
                .abs()
                < 0.01,
            "retry-only delta must equal retry row + OneDrive block + gap",
        );
    }

    #[test]
    fn m1e_body_content_height_includes_stealth() {
        let v = vp();
        // The full body height with stealth conditionals on must exceed the
        // height with them off (the Stealth card grows).
        let k = UpdaterHeightKind::StatusOnly;
        let off = settings_body_content_height(v, &SettingsBodyFlags::new(true, true, false, false, k));
        let on = settings_body_content_height(v, &SettingsBodyFlags::new(true, true, true, true, k));
        assert!(on > off, "stealth retry+error rows must grow the body");
    }

    #[test]
    fn m1e_clamp_scroll_honours_stealth_flags() {
        let v = vp();
        let k = UpdaterHeightKind::StatusOnly;
        let f_off = SettingsBodyFlags::new(true, true, false, false, k);
        let f_on = SettingsBodyFlags::new(true, true, true, true, k);
        // Taller content (stealth rows on) ⇒ a larger max-scroll clamp.
        let max_off = settings_body_max_scroll(settings_body_content_height(v, &f_off), v);
        let max_on = settings_body_max_scroll(settings_body_content_height(v, &f_on), v);
        let clamped_off = settings_clamp_scroll(0.0, 99999.0, v, &f_off);
        let clamped_on = settings_clamp_scroll(0.0, 99999.0, v, &f_on);
        assert!((clamped_off - max_off).abs() < 0.01);
        assert!((clamped_on - max_on).abs() < 0.01);
        assert!(clamped_on >= clamped_off);
    }

    // ── M1f — Updater §8 geometry ──────────────────────────────────────

    /// All five flag combos used by M1f tests share the both-startup-gates-on
    /// baseline (matches the M1e tests) so the Updater section sits at a stable
    /// Y; only `updater_kind` varies.
    fn flags(kind: UpdaterHeightKind) -> SettingsBodyFlags {
        SettingsBodyFlags::new(true, true, false, false, kind)
    }

    #[test]
    fn m1f_updater_title_sits_below_stealth_section() {
        let v = vp();
        // With no stealth retry, the Stealth section ends at its buttons row.
        let stealth_bottom =
            settings_stealth_buttons_row_rect(v, 0.0, true, true, false, false).bottom();
        let title = settings_updater_label_rect(v, 0.0, true, true, false, false);
        assert!(
            (title.y - (stealth_bottom + SETTINGS_SECTION_GAP)).abs() < 0.01,
            "updater title must start a section gap below the last Stealth row \
             (stealth_bottom={}, title.y={})",
            stealth_bottom,
            title.y,
        );
        assert_eq!(title.height, SETTINGS_SECTION_LABEL_H);
    }

    #[test]
    fn m1f_updater_title_reflows_when_stealth_retry_present() {
        let v = vp();
        // A stealth retry adds the OneDrive block, pushing the updater title
        // lower. Updater kind is irrelevant to the title Y.
        let no_retry = settings_updater_label_rect(v, 0.0, true, true, false, false);
        let with_retry = settings_updater_label_rect(v, 0.0, true, true, true, false);
        assert!(with_retry.y > no_retry.y);
    }

    #[test]
    fn m1f_status_row_and_pill_anchor() {
        let v = vp();
        let f = flags(UpdaterHeightKind::StatusOnly);
        let title = settings_updater_label_rect(v, 0.0, true, true, false, false);
        let status = settings_updater_status_row_rect(v, 0.0, &f);
        assert!((status.y - title.bottom()).abs() < 0.01);
        let pill = settings_updater_pill_rect(status);
        assert!(pill.right() <= status.right() + 0.01);
        assert!(pill.x >= status.x + status.width * 0.5);
        assert_eq!(pill.width, SETTINGS_UPDATER_PILL_W);
    }

    #[test]
    fn m1f_middle_block_height_tracks_status_family() {
        let v = vp();
        let status_only = settings_updater_middle_block_rect(v, 0.0, &flags(UpdaterHeightKind::StatusOnly));
        let versioned = settings_updater_middle_block_rect(v, 0.0, &flags(UpdaterHeightKind::Versioned));
        let downloading = settings_updater_middle_block_rect(v, 0.0, &flags(UpdaterHeightKind::Downloading));
        let error = settings_updater_middle_block_rect(v, 0.0, &flags(UpdaterHeightKind::Error));
        assert_eq!(status_only.height, 0.0);
        assert_eq!(versioned.height, SETTINGS_UPDATER_ROW_H);
        assert_eq!(downloading.height, SETTINGS_UPDATER_PROGRESS_H);
        assert_eq!(error.height, SETTINGS_UPDATER_ERROR_H);
        // The progress track sits inside the downloading block, full width.
        let track = settings_updater_progress_track_rect(v, 0.0, &flags(UpdaterHeightKind::Downloading));
        assert!(track.y >= downloading.y);
        assert!(track.bottom() <= downloading.bottom() + 0.01);
        assert!((track.width - downloading.width).abs() < 0.01);
        assert_eq!(track.height, SETTINGS_UPDATER_PROGRESS_TRACK_H);
    }

    #[test]
    fn m1f_buttons_left_pack_in_column_order() {
        let v = vp();
        let row = settings_updater_buttons_row_rect(v, 0.0, &flags(UpdaterHeightKind::Versioned));
        let b0 = settings_updater_button_rect(row, 0);
        let b1 = settings_updater_button_rect(row, 1);
        let b2 = settings_updater_button_rect(row, 2);
        assert_eq!(b0.y, b1.y);
        assert!(b0.right() <= b1.x + 0.01);
        assert!(b1.right() <= b2.x + 0.01);
        assert!((b0.x - row.x).abs() < 0.01);
        assert_eq!(b0.width, SETTINGS_UPDATER_BTN_W);
    }

    #[test]
    fn m1f_buttons_row_reflows_with_middle_block() {
        let v = vp();
        // The buttons row sits lower when a middle block is present.
        let no_block = settings_updater_buttons_row_rect(v, 0.0, &flags(UpdaterHeightKind::StatusOnly));
        let with_progress = settings_updater_buttons_row_rect(v, 0.0, &flags(UpdaterHeightKind::Downloading));
        assert!(with_progress.y > no_block.y);
        assert!((with_progress.y - no_block.y - SETTINGS_UPDATER_PROGRESS_H).abs() < 0.01);
    }

    #[test]
    fn m1f_prefs_rows_stack_below_buttons() {
        let v = vp();
        let f = flags(UpdaterHeightKind::StatusOnly);
        let buttons = settings_updater_buttons_row_rect(v, 0.0, &f);
        let freq = settings_updater_frequency_row_rect(v, 0.0, &f);
        let auto = settings_updater_auto_download_row_rect(v, 0.0, &f);
        assert!(freq.y >= buttons.bottom());
        assert!((auto.y - freq.bottom()).abs() < 0.01);
        // Chip right-anchors in the frequency row; toggle hit right-anchors in
        // the auto-download row.
        let chip = settings_updater_frequency_chip_rect(freq);
        assert!((chip.right() - freq.right()).abs() < 0.01);
        let hit = settings_updater_auto_download_hit_rect(auto);
        assert!((hit.right() - auto.right()).abs() < 0.01);
        assert_eq!(hit.width, SETTINGS_TOP_TOGGLE_HIT_W);
    }

    #[test]
    fn m1f_content_height_tracks_status_family() {
        let status_only = settings_updater_content_height(UpdaterHeightKind::StatusOnly);
        let versioned = settings_updater_content_height(UpdaterHeightKind::Versioned);
        let downloading = settings_updater_content_height(UpdaterHeightKind::Downloading);
        let error = settings_updater_content_height(UpdaterHeightKind::Error);
        assert!(versioned > status_only);
        assert!(downloading > status_only);
        assert!(error > status_only);
        // Each family adds exactly its middle-block height over StatusOnly.
        assert!((versioned - status_only - SETTINGS_UPDATER_ROW_H).abs() < 0.01);
        assert!((downloading - status_only - SETTINGS_UPDATER_PROGRESS_H).abs() < 0.01);
        assert!((error - status_only - SETTINGS_UPDATER_ERROR_H).abs() < 0.01);
    }

    #[test]
    fn m1f_body_content_height_includes_updater() {
        let v = vp();
        // Body height with the updater downloading (progress block) must exceed
        // the idle (status-only) height — proving the updater feeds the body.
        let idle = settings_body_content_height(v, &flags(UpdaterHeightKind::StatusOnly));
        let dl = settings_body_content_height(v, &flags(UpdaterHeightKind::Downloading));
        assert!(dl > idle, "updater progress block must grow the body");
    }

    #[test]
    fn m1f_flags_round_trip_through_height_fn() {
        let v = vp();
        // The Copy struct's fields drive the same height as the equivalent
        // legacy-style bools would: build two flag sets that differ only in
        // updater_kind and confirm the delta equals the middle-block delta.
        let a = SettingsBodyFlags::new(true, false, true, false, UpdaterHeightKind::StatusOnly);
        let b = SettingsBodyFlags::new(true, false, true, false, UpdaterHeightKind::Error);
        let ha = settings_body_content_height(v, &a);
        let hb = settings_body_content_height(v, &b);
        assert!((hb - ha - SETTINGS_UPDATER_ERROR_H).abs() < 0.01);
        // Round-trip the struct itself (Copy + Eq).
        let c = a;
        assert_eq!(a, c);
        assert_ne!(a, b);
    }

    // ── M1g — Backup §9 geometry ───────────────────────────────────────

    /// Backup flag baseline: both startup gates on (stable Updater Y) + the
    /// updater idle (StatusOnly) so only the backup row count varies.
    fn backup_flags(backup_rows: usize) -> SettingsBodyFlags {
        SettingsBodyFlags::new(true, true, false, false, UpdaterHeightKind::StatusOnly)
            .with_backup_rows(backup_rows)
    }

    #[test]
    fn m1g_backup_title_sits_below_updater_section() {
        let v = vp();
        let f = backup_flags(0);
        let updater_bottom = settings_updater_auto_download_row_rect(v, 0.0, &f).bottom();
        let title = settings_backup_label_rect(v, 0.0, &f);
        // Title clears the Updater section by exactly the section gap.
        assert!((title.y - updater_bottom - SETTINGS_SECTION_GAP).abs() < 0.01);
        assert_eq!(title.height, SETTINGS_SECTION_LABEL_H);
    }

    #[test]
    fn m1g_rows_stack_in_order_title_desc_actions_status_list() {
        let v = vp();
        let f = backup_flags(2);
        let title = settings_backup_label_rect(v, 0.0, &f);
        let desc = settings_backup_description_rect(v, 0.0, &f);
        let actions = settings_backup_actions_row_rect(v, 0.0, &f);
        let status = settings_backup_status_rect(v, 0.0, &f);
        let entry0 = settings_backup_entry_row_rect(v, 0.0, &f, 0);
        assert!((desc.y - title.bottom()).abs() < 0.01);
        assert!((actions.y - desc.bottom()).abs() < 0.01);
        assert!(status.y >= actions.bottom());
        assert!(entry0.y >= status.bottom());
    }

    #[test]
    fn m1g_create_and_refresh_buttons_pack_left_inside_actions_row() {
        let v = vp();
        let row = settings_backup_actions_row_rect(v, 0.0, &backup_flags(0));
        let create = settings_backup_create_button_rect(row);
        let refresh = settings_backup_refresh_button_rect(row);
        assert!((create.x - row.x).abs() < 0.01);
        assert!(create.right() <= refresh.x);
        assert_eq!(create.width, SETTINGS_BACKUP_CREATE_BTN_W);
        assert_eq!(refresh.width, SETTINGS_BACKUP_REFRESH_BTN_W);
        assert!(refresh.right() <= row.right());
    }

    #[test]
    fn m1g_entry_rows_stack_with_gap_and_restore_button_right_anchors() {
        let v = vp();
        let f = backup_flags(3);
        let r0 = settings_backup_entry_row_rect(v, 0.0, &f, 0);
        let r1 = settings_backup_entry_row_rect(v, 0.0, &f, 1);
        let r2 = settings_backup_entry_row_rect(v, 0.0, &f, 2);
        assert!(r0.y < r1.y);
        assert!(r1.y < r2.y);
        // Adjacent rows are one row-height + gap apart.
        assert!(
            (r1.y - r0.y - SETTINGS_BACKUP_ENTRY_ROW_H - SETTINGS_BACKUP_ENTRY_ROW_GAP).abs() < 0.01
        );
        let restore = settings_backup_restore_button_rect(r0);
        assert!(restore.right() <= r0.right());
        assert!(restore.x >= r0.x);
        assert_eq!(restore.width, SETTINGS_BACKUP_RESTORE_BTN_W);
    }

    #[test]
    fn m1g_content_height_grows_with_visible_row_count() {
        // 0 (empty placeholder) / 1 / cap rows — height is monotone up to cap.
        let h0 = settings_backup_content_height(0);
        let h1 = settings_backup_content_height(1);
        let h_cap = settings_backup_content_height(SETTINGS_BACKUP_ROW_VISIBLE_MAX);
        assert!(h1 >= h0, "one entry row ≥ the empty placeholder slot");
        assert!(h_cap > h1, "more rows must grow the section");
        // Over-cap saturates at the cap height (the cap is applied inside).
        let h_over = settings_backup_content_height(SETTINGS_BACKUP_ROW_VISIBLE_MAX + 10);
        assert!((h_over - h_cap).abs() < 0.01);
    }

    #[test]
    fn m1g_body_content_height_includes_backup_rows() {
        let v = vp();
        // Body height with 3 backup rows must exceed the empty-list body — the
        // variable list feeds the body via SettingsBodyFlags::backup_row_count.
        let empty = settings_body_content_height(v, &backup_flags(0));
        let full = settings_body_content_height(v, &backup_flags(SETTINGS_BACKUP_ROW_VISIBLE_MAX));
        assert!(full > empty, "backup list rows must grow the body");
    }

    #[test]
    fn m1g_with_backup_rows_only_changes_backup_field() {
        let base = SettingsBodyFlags::new(true, true, false, false, UpdaterHeightKind::StatusOnly);
        let with = base.with_backup_rows(2);
        assert_eq!(base.backup_row_count, 0);
        assert_eq!(with.backup_row_count, 2);
        assert_eq!(with.crash_restart_enabled, base.crash_restart_enabled);
        assert_eq!(with.updater_kind, base.updater_kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp() -> Size {
        Size {
            width: 800.0,
            height: 600.0,
        }
    }

    #[test]
    fn panel_rect_centres_x_and_top_anchors_y() {
        let v = vp();
        let r = settings_panel_rect(v);
        assert_eq!(r.width, SETTINGS_PANEL_WIDTH);
        assert_eq!(r.height, SETTINGS_PANEL_HEIGHT);
        // (800-360)/2 = 220
        assert!((r.x - 220.0).abs() < 0.01);
        // 600 >= 560 + 16, so y = top margin
        assert!((r.y - SETTINGS_PANEL_TOP_MARGIN).abs() < 0.01);
    }

    #[test]
    fn panel_rect_saturates_to_zero_when_viewport_too_small() {
        let v = Size {
            width: 200.0,
            height: 320.0,
        };
        let r = settings_panel_rect(v);
        assert!(r.x.abs() < 0.01);
        assert!(r.y.abs() < 0.01);
    }

    #[test]
    fn close_button_lives_in_panel_header() {
        let v = vp();
        let p = settings_panel_rect(v);
        let c = settings_close_button_rect(v);
        // Header band — y close to top, x near the right edge of the panel.
        assert!(c.y >= p.y && c.y < p.y + SETTINGS_HEADER_H);
        assert!(c.right() <= p.right());
        assert!(c.x > p.x + p.width * 0.5);
    }

    #[test]
    fn section_rows_stack_vertically_in_order() {
        let v = vp();
        let stealth = settings_stealth_enabled_rect(v);
        let auto = settings_update_auto_download_rect(v);
        let encryption = settings_encryption_mode_rect(v);
        let locale = settings_switch_button_rect(v);
        let zone = settings_zone_display_mode_rect(v);
        let theme = settings_active_theme_rect(v);
        let updater = settings_update_frequency_rect(v);
        let vault = settings_backup_now_rect(v);
        let modals = settings_keybindings_open_rect(v);
        assert!(stealth.y < auto.y);
        assert!(auto.y < encryption.y);
        assert!(encryption.y < locale.y);
        assert!(locale.y < zone.y);
        assert!(zone.y < theme.y);
        assert!(theme.y < updater.y);
        assert!(updater.y < vault.y);
        assert!(vault.y < modals.y);
    }

    #[test]
    fn updater_row_lays_out_inline_actions_left_to_right_of_dropdown() {
        let v = vp();
        let frequency = settings_update_frequency_rect(v);
        let check = settings_update_check_now_rect(v);
        let action = settings_update_action_rect(v);
        let skip = settings_update_skip_rect(v);
        assert_eq!(frequency.y, check.y);
        assert_eq!(check.y, action.y);
        assert_eq!(action.y, skip.y);
        assert!(skip.right() < action.x);
        assert!(action.right() < check.x);
        assert!(check.right() < frequency.x);
    }

    #[test]
    fn theme_row_lays_out_swatch_import_active_left_to_right() {
        let v = vp();
        let swatch = settings_theme_base_rect(v);
        let import = settings_theme_import_rect(v);
        let active = settings_active_theme_rect(v);
        assert_eq!(swatch.y, import.y);
        assert_eq!(import.y, active.y);
        assert!(swatch.right() < import.x);
        assert!(import.right() < active.x);
    }

    #[test]
    fn vault_row_packs_six_chips_left_to_right() {
        let v = vp();
        let chips = [
            settings_backup_now_rect(v),
            settings_backup_list_rect(v),
            settings_backup_restore_rect(v),
            settings_recovery_create_rect(v),
            settings_recovery_diagnostics_rect(v),
            settings_recovery_restore_rect(v),
        ];
        for w in chips.windows(2) {
            assert!(w[0].right() <= w[1].x, "chips overlap: {:?}", w);
            assert_eq!(w[0].y, w[1].y);
            assert_eq!(w[0].height, w[1].height);
        }
    }

    // M1h (2026-05-29) — removed `modal_openers_paired_with_keybindings_left_plugins_right`:
    // the plugins modal-opener button (`settings_plugins_open_rect`) was deleted
    // when the Plugins surface moved inline (§11). The keybindings opener keeps
    // its own coverage elsewhere.

    #[test]
    fn backup_entries_sit_below_vault_row() {
        let v = vp();
        let vault = settings_backup_now_rect(v);
        let entry0 = settings_backup_entry_rect(v, 0);
        let entry1 = settings_backup_entry_rect(v, 1);
        let entry2 = settings_backup_entry_rect(v, 2);
        assert!(entry0.y >= vault.bottom());
        assert!(entry0.right() < entry1.x);
        assert!(entry1.right() < entry2.x);
    }

    #[test]
    fn locale_dropdown_chip_sits_in_locale_row_right_half() {
        let v = vp();
        let p = settings_panel_rect(v);
        let chip = settings_switch_button_rect(v);
        assert_eq!(chip.width, SETTINGS_DROPDOWN_CHIP_W);
        assert_eq!(chip.height, SETTINGS_DROPDOWN_CHIP_H);
        assert!(chip.x >= p.x + p.width * 0.5);
        assert!(chip.right() <= p.right());
    }

    #[test]
    fn toggle_rocker_rect_sits_in_row_right_half() {
        let v = vp();
        let p = settings_panel_rect(v);
        let stealth = settings_stealth_enabled_rect(v);
        let auto = settings_update_auto_download_rect(v);
        for r in [stealth, auto] {
            assert!(r.x >= p.x + p.width * 0.5);
            assert!(r.right() <= p.right());
            assert_eq!(r.width, SETTINGS_SWITCH_BTN_W);
            assert_eq!(r.height, SETTINGS_SWITCH_BTN_H);
        }
    }

    #[test]
    fn panel_shadow_rect_uses_shadow_offsets_and_blur() {
        let panel = Rect {
            x: 20.0,
            y: 10.0,
            width: 360.0,
            height: 580.0,
        };
        let shadow = Shadow {
            offset_x: 2.0,
            offset_y: 5.0,
            blur: 14.0,
            spread: 0.0,
            color: bento_nano_style::Color::from_u8(0, 0, 0, 0x80),
        };
        let shadow_rect = settings_panel_shadow_rect(panel, shadow);
        assert_eq!(shadow_rect.x, 8.0);
        assert_eq!(shadow_rect.y, 1.0);
        assert_eq!(shadow_rect.width, 388.0);
        assert_eq!(shadow_rect.height, 608.0);
    }

    #[test]
    fn keybindings_modal_rows_fit_inside_card() {
        let v = vp();
        let modal = settings_keybindings_modal_rect(v);
        let close = settings_keybindings_close_rect(v);
        let row_0 = settings_keybinding_row_rect(v, 0);
        let row_9 = settings_keybinding_row_rect(v, 9);
        let record = settings_keybinding_record_rect(v, 0);
        let reset = settings_keybinding_reset_rect(v, 0);
        assert!(modal.x >= 0.0);
        assert!(modal.y >= 0.0);
        assert!(close.right() <= modal.right());
        assert!(row_0.y > close.y);
        assert!(row_9.bottom() <= modal.bottom());
        assert!(record.right() < reset.x);
        assert!(reset.right() <= row_0.right());
    }

    // M1h (2026-05-29) — removed `plugins_modal_rows_and_actions_fit_inside_card`:
    // the plugin lifecycle modal geometry it covered was deleted when the
    // Plugins surface moved inline. The inline §11 card geometry is covered by
    // the `m1h_*` tests in the `m1_tests` dark-shell module above.

    #[test]
    fn section_row_count_matches_visible_rows() {
        // 9 rows: stealth, auto, encryption, locale, zone, theme, updater,
        // vault, modal openers. Backup entry strip lives between vault and
        // modals (not counted as a section row).
        assert_eq!(SETTINGS_SECTION_ROW_COUNT, 9);
    }

    #[test]
    fn final_modal_row_fits_inside_panel_chrome() {
        let v = vp();
        let p = settings_panel_rect(v);
        let modals = settings_section_row_rect(v, ROW_INDEX_MODALS);
        assert!(modals.bottom() <= p.bottom());
    }
}
