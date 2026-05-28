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

/// Modal opener buttons (keybindings / plugins) — rendered as flat chips in
/// the bottom-action row instead of the pre-K1 title-bar buttons.
pub const SETTINGS_PLUGINS_BTN_W: f32 = 72.0;
pub const SETTINGS_PLUGINS_BTN_H: f32 = SETTINGS_SWITCH_BTN_H;

/// Plugin lifecycle modal geometry (untouched by K1).
pub const SETTINGS_PLUGINS_MODAL_W: f32 = 420.0;
pub const SETTINGS_PLUGINS_MODAL_H: f32 = 260.0;
pub const SETTINGS_PLUGINS_ROW_VISIBLE_MAX: usize = 5;
pub const SETTINGS_PLUGINS_ROW_START_Y: f32 = 54.0;
pub const SETTINGS_PLUGINS_ROW_STEP_Y: f32 = 34.0;
pub const SETTINGS_PLUGINS_ROW_H: f32 = 30.0;
pub const SETTINGS_PLUGINS_INSTALL_BTN_W: f32 = 58.0;
pub const SETTINGS_PLUGINS_REFRESH_BTN_W: f32 = 64.0;
pub const SETTINGS_PLUGINS_TOGGLE_BTN_W: f32 = 54.0;
pub const SETTINGS_PLUGINS_REMOVE_BTN_W: f32 = 64.0;
pub const SETTINGS_PLUGINS_CLOSE_BTN_W: f32 = 32.0;
pub const SETTINGS_PLUGINS_BTN_GAP: f32 = 6.0;

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

/// Plugins-modal opener — sits to the right of the keybindings opener.
pub fn settings_plugins_open_rect(viewport: Size) -> Rect {
    let keys = settings_keybindings_open_rect(viewport);
    Rect {
        x: keys.x + keys.width + SETTINGS_PLUGINS_BTN_GAP,
        y: keys.y,
        width: SETTINGS_PLUGINS_BTN_W,
        height: SETTINGS_PLUGINS_BTN_H,
    }
}

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

pub fn settings_plugins_modal_rect(viewport: Size) -> Rect {
    Rect {
        x: ((viewport.width - SETTINGS_PLUGINS_MODAL_W) * 0.5).max(0.0),
        y: ((viewport.height - SETTINGS_PLUGINS_MODAL_H) * 0.5).max(0.0),
        width: SETTINGS_PLUGINS_MODAL_W,
        height: SETTINGS_PLUGINS_MODAL_H,
    }
}

pub fn settings_plugins_close_rect(viewport: Size) -> Rect {
    let modal = settings_plugins_modal_rect(viewport);
    Rect {
        x: modal.x + modal.width - SETTINGS_PANEL_PADDING - SETTINGS_PLUGINS_CLOSE_BTN_W,
        y: modal.y + 12.0,
        width: SETTINGS_PLUGINS_CLOSE_BTN_W,
        height: SETTINGS_SWITCH_BTN_H,
    }
}

pub fn settings_plugins_refresh_rect(viewport: Size) -> Rect {
    let close = settings_plugins_close_rect(viewport);
    Rect {
        x: close.x - SETTINGS_PLUGINS_BTN_GAP - SETTINGS_PLUGINS_REFRESH_BTN_W,
        y: close.y,
        width: SETTINGS_PLUGINS_REFRESH_BTN_W,
        height: SETTINGS_SWITCH_BTN_H,
    }
}

pub fn settings_plugins_install_rect(viewport: Size) -> Rect {
    let refresh = settings_plugins_refresh_rect(viewport);
    Rect {
        x: refresh.x - SETTINGS_PLUGINS_BTN_GAP - SETTINGS_PLUGINS_INSTALL_BTN_W,
        y: refresh.y,
        width: SETTINGS_PLUGINS_INSTALL_BTN_W,
        height: SETTINGS_SWITCH_BTN_H,
    }
}

pub fn settings_plugin_row_rect(viewport: Size, row_index: usize) -> Rect {
    let modal = settings_plugins_modal_rect(viewport);
    Rect {
        x: modal.x + SETTINGS_PANEL_PADDING,
        y: modal.y + SETTINGS_PLUGINS_ROW_START_Y + SETTINGS_PLUGINS_ROW_STEP_Y * row_index as f32,
        width: modal.width - SETTINGS_PANEL_PADDING * 2.0,
        height: SETTINGS_PLUGINS_ROW_H,
    }
}

pub fn settings_plugin_toggle_rect(viewport: Size, row_index: usize) -> Rect {
    let row = settings_plugin_row_rect(viewport, row_index);
    Rect {
        x: row.right()
            - SETTINGS_PLUGINS_REMOVE_BTN_W
            - SETTINGS_PLUGINS_BTN_GAP
            - SETTINGS_PLUGINS_TOGGLE_BTN_W,
        y: row.y + 3.0,
        width: SETTINGS_PLUGINS_TOGGLE_BTN_W,
        height: SETTINGS_SWITCH_BTN_H,
    }
}

pub fn settings_plugin_uninstall_rect(viewport: Size, row_index: usize) -> Rect {
    let row = settings_plugin_row_rect(viewport, row_index);
    Rect {
        x: row.right() - SETTINGS_PLUGINS_REMOVE_BTN_W,
        y: row.y + 3.0,
        width: SETTINGS_PLUGINS_REMOVE_BTN_W,
        height: SETTINGS_SWITCH_BTN_H,
    }
}

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
/// row. Each radio is a 12-DIP outer circle + 6-DIP inner dot when selected
/// + an inline label. Three radios + 2 inter-radio gaps + leading/trailing
/// padding pack into the right-anchored ~260-DIP cluster, matching the
/// language-chip horizontal anchor for vertical alignment.
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

/// Round-2 M2 — height of one 桌面源 card row (icon + label + meta + state
/// chip + chevron). Slightly taller than a toggle row to host the secondary
/// text line.
pub const SETTINGS_SOURCE_ROW_H: f32 = 56.0;

/// Round-2 M2 — number of source rows painted in M2 (1=primary, 2=public).
pub const SETTINGS_SOURCE_COUNT: u8 = 2;

/// Round-2 M2 — gap between two source cards.
pub const SETTINGS_SOURCE_GAP: f32 = 8.0;

/// Round-2 M2 — width of the right-anchored toggle hit-box inside a
/// source-card row (mirrors `SETTINGS_TOP_TOGGLE_HIT_W` for consistency).
pub const SETTINGS_SOURCE_TOGGLE_HIT_W: f32 = 60.0;
pub const SETTINGS_SOURCE_TOGGLE_HIT_H: f32 = 28.0;

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

/// α4 (Wave I-α, 2026-05-25) — full row rect for the zone-display-mode
/// picker. Sits directly below the language row (M1 toggle band + 1
/// language row) and above the M2 桌面源 section. Honours scroll offset
/// the same way every other body row does.
pub fn settings_zone_display_mode_picker_row_rect(
    viewport: Size,
    scroll_offset_y: f32,
) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + SETTINGS_ROW_H_M1 * (SETTINGS_TOP_TOGGLE_COUNT as f32 + 1.0),
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
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

/// Round-2 M1/M2 + M1d — total content height inside the body. Grows with
/// each milestone as sections light up. M1d's Startup section has
/// conditional rows, so the two gating bools (`crash_restart_enabled`,
/// `safe_start_after_hibernation`) are parameters — geometry never reads
/// global state, the shell passes the live Cells.
pub fn settings_body_content_height(
    viewport: Size,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> f32 {
    settings_m2_content_height(viewport)
        + settings_perf_startup_content_height(
            viewport,
            crash_restart_enabled,
            safe_start_after_hibernation,
        )
}

/// Round-2 M1 — clamp `requested_offset` to `[0, max_scroll]` where
/// `max_scroll = max(0, content_h - body_h)`. Returns 0 when the content
/// already fits, so the scroll Cell can never go negative.
pub fn settings_body_max_scroll(content_total_h: f32, viewport: Size) -> f32 {
    let body = settings_body_rect(viewport);
    (content_total_h - body.height).max(0.0)
}

/// Round-2 M1 + M1d — apply a wheel-delta `delta_y` (positive = scroll down)
/// to `current_offset` and clamp. Pure helper so the wheel handler stays
/// allocation-free. The two gating bools feed the dynamic content height so
/// the max-scroll matches whatever conditional rows are currently visible.
pub fn settings_clamp_scroll(
    current_offset: f32,
    delta_y: f32,
    viewport: Size,
    crash_restart_enabled: bool,
    safe_start_after_hibernation: bool,
) -> f32 {
    let next = (current_offset + delta_y).max(0.0);
    let content_h = settings_body_content_height(
        viewport,
        crash_restart_enabled,
        safe_start_after_hibernation,
    );
    let max = settings_body_max_scroll(content_h, viewport);
    next.min(max)
}

/// Round-2 M2 — Y offset (scroll-space) at which the M2 sections start.
/// Sits below the M1 toggle band + the language row + α4 zone-display
/// picker row + a section gap.
fn settings_m2_origin_y_offset() -> f32 {
    SETTINGS_ROW_H_M1 * (SETTINGS_TOP_TOGGLE_COUNT as f32 + 2.0) + SETTINGS_SECTION_GAP
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

/// Round-2 M2 — right-anchored toggle hit-box on a source card row.
pub fn settings_source_toggle_hit_rect(
    viewport: Size,
    scroll_offset_y: f32,
    index: u8,
) -> Rect {
    let row = settings_source_row_rect(viewport, scroll_offset_y, index);
    Rect {
        x: row.right() - SETTINGS_SOURCE_TOGGLE_HIT_W,
        y: row.y + (row.height - SETTINGS_SOURCE_TOGGLE_HIT_H) * 0.5,
        width: SETTINGS_SOURCE_TOGGLE_HIT_W,
        height: SETTINGS_SOURCE_TOGGLE_HIT_H,
    }
}

/// Round-2 M2 — `桌面路径` section label rect.
pub fn settings_desktop_path_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let last_source = settings_source_row_rect(
        viewport,
        scroll_offset_y,
        SETTINGS_SOURCE_COUNT - 1,
    );
    let body = settings_body_rect(viewport);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: last_source.bottom() + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// Round-2 M2 — `桌面路径` input rect (single-line dark rounded box).
pub fn settings_desktop_path_input_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let label = settings_desktop_path_label_rect(viewport, scroll_offset_y);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_INPUT_ROW_H,
    }
}

/// Round-2 M2 — `监控值` section label rect.
pub fn settings_watch_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let input = settings_desktop_path_input_rect(viewport, scroll_offset_y);
    let body = settings_body_rect(viewport);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: input.bottom() + SETTINGS_SECTION_GAP,
        width: body.width - SETTINGS_ROW_PAD_X * 2.0,
        height: SETTINGS_SECTION_LABEL_H,
    }
}

/// Round-2 M2 — `监控值` textarea rect (multi-line dark rounded box).
pub fn settings_watch_textarea_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let label = settings_watch_label_rect(viewport, scroll_offset_y);
    Rect {
        x: label.x,
        y: label.bottom(),
        width: label.width,
        height: SETTINGS_TEXTAREA_H,
    }
}

/// Round-2 M2 — total content height of M1 + M2 sections only. M3 helpers
/// extend this further.
pub fn settings_m2_content_height(_viewport: Size) -> f32 {
    settings_m2_origin_y_offset()
        + SETTINGS_SECTION_LABEL_H
        + SETTINGS_SOURCE_ROW_H * SETTINGS_SOURCE_COUNT as f32
        + SETTINGS_SOURCE_GAP * (SETTINGS_SOURCE_COUNT - 1) as f32
        + SETTINGS_SECTION_GAP
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

/// M1d — scroll-space Y at which the Performance group title starts. Sits
/// below the full M2 block + a section gap (the M3 advanced/overlay block
/// it replaces used the same origin).
fn settings_perf_origin_y_offset() -> f32 {
    settings_m2_origin_y_offset()
        + SETTINGS_SECTION_LABEL_H
        + SETTINGS_SOURCE_ROW_H * SETTINGS_SOURCE_COUNT as f32
        + SETTINGS_SOURCE_GAP * (SETTINGS_SOURCE_COUNT - 1) as f32
        + SETTINGS_SECTION_GAP
        + SETTINGS_SECTION_LABEL_H
        + SETTINGS_INPUT_ROW_H
        + SETTINGS_SECTION_GAP
        + SETTINGS_SECTION_LABEL_H
        + SETTINGS_TEXTAREA_H
        + SETTINGS_SECTION_GAP
}

/// M1d — `性能 / Performance` group title rect.
pub fn settings_performance_label_rect(viewport: Size, scroll_offset_y: f32) -> Rect {
    let body = settings_body_rect(viewport);
    let origin_y = settings_body_content_origin(viewport, scroll_offset_y);
    Rect {
        x: body.x + SETTINGS_ROW_PAD_X,
        y: origin_y + settings_perf_origin_y_offset(),
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

    /// α4 (Wave I-α, 2026-05-25) — zone-display-mode picker sits between
    /// the language row and the M2 sources section, and its three radios
    /// pack right-to-left in cluster order.
    #[test]
    fn alpha4_zone_display_picker_row_sits_below_language_row() {
        let v = vp();
        let lang = settings_language_row_rect(v, 0.0);
        let picker = settings_zone_display_mode_picker_row_rect(v, 0.0);
        assert!(
            (picker.y - lang.bottom()).abs() < 0.01,
            "picker row must start exactly where the language row ends \
             (lang.bottom={}, picker.y={})",
            lang.bottom(),
            picker.y,
        );
        assert_eq!(picker.height, SETTINGS_ROW_H_M1);
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

    #[test]
    fn alpha4_m2_sources_section_shifts_below_picker_row() {
        let v = vp();
        let picker = settings_zone_display_mode_picker_row_rect(v, 0.0);
        let sources_label = settings_sources_label_rect(v, 0.0);
        // Sources label must clear the picker row by at least the section
        // gap so the picker and the next section never overlap.
        assert!(
            sources_label.y >= picker.bottom(),
            "M2 sources label (y={}) must sit at-or-below picker row \
             bottom (y={}) so α4 picker has dedicated vertical real estate",
            sources_label.y,
            picker.bottom(),
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
        assert_eq!(settings_clamp_scroll(0.0, -100.0, v, true, true), 0.0);
        assert_eq!(settings_clamp_scroll(20.0, -100.0, v, true, true), 0.0);
    }

    #[test]
    fn m1_clamp_scroll_caps_at_max() {
        let v = vp();
        let content = settings_body_content_height(v, true, true);
        let max = settings_body_max_scroll(content, v);
        assert_eq!(settings_clamp_scroll(0.0, max + 999.0, v, true, true), max);
    }

    #[test]
    fn m1_top_toggle_count_pinned() {
        assert_eq!(SETTINGS_TOP_TOGGLE_COUNT, 5);
    }

    #[test]
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
    fn m2_source_toggle_hit_right_anchored_inside_row() {
        let v = vp();
        let row = settings_source_row_rect(v, 0.0, 0);
        let hit = settings_source_toggle_hit_rect(v, 0.0, 0);
        assert!((hit.right() - row.right()).abs() < 0.01);
        assert!(hit.y >= row.y);
        assert!(hit.bottom() <= row.bottom() + 0.01);
    }

    #[test]
    fn m2_desktop_path_input_sits_below_last_source() {
        let v = vp();
        let last_src = settings_source_row_rect(v, 0.0, SETTINGS_SOURCE_COUNT - 1);
        let label = settings_desktop_path_label_rect(v, 0.0);
        let input = settings_desktop_path_input_rect(v, 0.0);
        assert!(label.y >= last_src.bottom() + SETTINGS_SECTION_GAP - 0.01);
        assert!((input.y - label.bottom()).abs() < 0.01);
        assert_eq!(input.height, SETTINGS_INPUT_ROW_H);
    }

    #[test]
    fn m2_watch_textarea_sits_below_path_input() {
        let v = vp();
        let input = settings_desktop_path_input_rect(v, 0.0);
        let label = settings_watch_label_rect(v, 0.0);
        let area = settings_watch_textarea_rect(v, 0.0);
        assert!(label.y >= input.bottom() + SETTINGS_SECTION_GAP - 0.01);
        assert!((area.y - label.bottom()).abs() < 0.01);
        assert_eq!(area.height, SETTINGS_TEXTAREA_H);
    }

    #[test]
    fn m2_content_height_exceeds_body_to_trigger_scroll() {
        let v = vp();
        let body = settings_body_rect(v);
        let content_h = settings_m2_content_height(v);
        // M2 should make the body scroll on an 800×800 viewport (since the
        // panel caps at 700 DIP height, body band is ~596 DIP). Five toggles
        // + language + 桌面源(2 cards) + 桌面路径 + 监控值 must exceed body.
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
    fn m2_source_count_pinned() {
        assert_eq!(SETTINGS_SOURCE_COUNT, 2);
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
        let v = vp();
        let textarea = settings_watch_textarea_rect(v, 0.0);
        let label = settings_performance_label_rect(v, 0.0);
        assert!(label.y >= textarea.bottom());
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
        let none = settings_body_content_height(v, false, false);
        let crash = settings_body_content_height(v, true, false);
        let hib = settings_body_content_height(v, false, true);
        let both = settings_body_content_height(v, true, true);
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
        let m2 = settings_m2_content_height(v);
        let total = settings_body_content_height(v, true, true);
        assert!(total > m2);
    }

    #[test]
    fn scroll_offset_shifts_performance_label_up() {
        let v = vp();
        let r_at_0 = settings_performance_label_rect(v, 0.0);
        let r_at_50 = settings_performance_label_rect(v, 50.0);
        assert!((r_at_50.y + 50.0 - r_at_0.y).abs() < 0.01);
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

    #[test]
    fn modal_openers_paired_with_keybindings_left_plugins_right() {
        let v = vp();
        let keys = settings_keybindings_open_rect(v);
        let plugins = settings_plugins_open_rect(v);
        assert_eq!(keys.y, plugins.y);
        assert!(keys.right() <= plugins.x);
    }

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

    #[test]
    fn plugins_modal_rows_and_actions_fit_inside_card() {
        let v = vp();
        let modal = settings_plugins_modal_rect(v);
        let close = settings_plugins_close_rect(v);
        let refresh = settings_plugins_refresh_rect(v);
        let install = settings_plugins_install_rect(v);
        let row_0 = settings_plugin_row_rect(v, 0);
        let row_4 = settings_plugin_row_rect(v, 4);
        let toggle = settings_plugin_toggle_rect(v, 0);
        let uninstall = settings_plugin_uninstall_rect(v, 0);
        assert!(modal.x >= 0.0);
        assert!(modal.y >= 0.0);
        assert!(close.right() <= modal.right() - SETTINGS_PANEL_PADDING + 0.01);
        assert!(refresh.right() < close.x);
        assert!(install.right() < refresh.x);
        assert!(row_0.y > close.bottom());
        assert!(row_4.bottom() <= modal.bottom() - SETTINGS_PANEL_PADDING + 0.01);
        assert!(toggle.right() < uninstall.x);
        assert!(uninstall.right() <= row_0.right());
    }

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
