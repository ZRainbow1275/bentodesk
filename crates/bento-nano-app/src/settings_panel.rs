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
//!   the legacy geometry owner so the keybindings surface and compatibility
//!   hit paths continue to share the same rects; no dead-code exemption remains.
//! - **M2** adds 桌面源 list + 桌面路径 input + 监控值 textarea.
//! - **M3** adds the 高级洗脑启动 / 磁吸 / 重叠版本 / 装备状态 mid-section.
//! - **M4** adds 应用更新 + 设置备份 + 设置加密 + 插件 inline.
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
pub const SETTINGS_HEADER_H: f32 = 52.0;

/// Wave K1 / V21-N132 — iOS-style rocker switch track size.
/// Tauri source `.toggle-switch` is 44×24 px.
pub const SETTINGS_TOGGLE_TRACK_W: f32 = 44.0;
pub const SETTINGS_TOGGLE_TRACK_H: f32 = 24.0;
/// Knob diameter — Tauri source `.toggle-switch__thumb` is 20×20 px.
pub const SETTINGS_TOGGLE_KNOB_D: f32 = 20.0;

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

mod appearance;
mod backup;
mod encryption;
mod m1_base;
mod m1_paths;
mod performance_startup;
mod plugins;
mod stealth;
mod updater;

pub use appearance::*;
pub use backup::*;
pub use encryption::*;
pub use m1_base::*;
pub use m1_paths::*;
pub use performance_startup::*;
pub use plugins::*;
pub use stealth::*;
pub use updater::*;

use appearance::settings_appearance_origin_y_offset;
use backup::settings_backup_content_height_for_status;
use encryption::settings_encryption_content_height_for_status;
use m1_base::settings_body_content_origin;
use m1_paths::settings_m2_origin_y_offset;
use performance_startup::settings_perf_startup_content_height;

#[cfg(test)]
mod tests;
