use super::*;

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
/// Lifecycle feedback row shown after install/toggle/uninstall attempts.
pub const SETTINGS_PLUGIN_STATUS_H: f32 = 20.0;

/// M1h — type-badge chip width + the enable-toggle hit-box (right-anchored in
/// the card header) + the 卸载 button width.
pub const SETTINGS_PLUGIN_BADGE_W: f32 = 56.0;
pub const SETTINGS_PLUGIN_TOGGLE_HIT_W: f32 = 60.0;
pub const SETTINGS_PLUGIN_TOGGLE_HIT_H: f32 = 24.0;
pub const SETTINGS_PLUGIN_UNINSTALL_BTN_W: f32 = 72.0;

/// M1h — scroll-space Y at which the Plugins group title starts. M7
/// (2026-06-01): re-anchored off the Encryption §10 card's reserved status row
/// + a section gap (the card now slots between Backup §9 and Plugins §11 to
///   match Tauri's `<BackupCard/><EncryptionCard/>` adjacency). The encryption
///   card is fixed-height, so its status row bottom is a deterministic offset
///   from the Backup card's last row; the whole chain reflows automatically.
///
/// Takes the full flag set so its Y follows whatever Backup/Updater/Stealth/
/// Startup rows are currently visible.
pub fn settings_plugins_label_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let body = settings_body_rect(viewport);
    let encryption_bottom = if flags.encryption_status_present {
        settings_encryption_status_rect(viewport, scroll_offset_y, flags).bottom()
    } else {
        settings_encryption_hint_rect(viewport, scroll_offset_y, flags).bottom()
    };
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

/// Optional plugin lifecycle status row below the install button.
pub fn settings_plugin_status_rect(
    viewport: Size,
    scroll_offset_y: f32,
    flags: &SettingsBodyFlags,
) -> Rect {
    let install = settings_plugins_install_button_rect(viewport, scroll_offset_y, flags);
    Rect {
        x: install.x,
        y: install.bottom() + SETTINGS_PLUGIN_CARD_GAP,
        width: install.width,
        height: SETTINGS_PLUGIN_STATUS_H,
    }
}

fn settings_plugin_list_top(install: Rect, flags: &SettingsBodyFlags) -> f32 {
    install.bottom()
        + SETTINGS_PLUGIN_CARD_GAP
        + if flags.plugin_status_present {
            SETTINGS_PLUGIN_STATUS_H + SETTINGS_PLUGIN_CARD_GAP
        } else {
            0.0
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
        y: settings_plugin_list_top(install, flags)
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
        y: settings_plugin_list_top(install, flags),
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

/// Neutral cancel button shown left of the destructive confirm button.
pub fn settings_plugin_uninstall_cancel_button_rect(card: Rect) -> Rect {
    let confirm = settings_plugin_uninstall_button_rect(card);
    Rect {
        x: confirm.x - SETTINGS_PLUGIN_CARD_GAP - SETTINGS_PLUGIN_UNINSTALL_BTN_W,
        y: confirm.y,
        width: SETTINGS_PLUGIN_UNINSTALL_BTN_W,
        height: confirm.height,
    }
}

/// M1h — height the Plugins §11 section contributes to
/// `settings_body_content_height`. Always-present: title + install button. The
/// list adds either `n` plugin cards (+ inter-card gaps, plus the leading gap)
/// or a single empty-placeholder row. A trailing section gap pads the body
/// bottom. The (already-capped) `plugin_row_count` is the parameter — geometry
/// never reads global state.
pub fn settings_plugins_content_height(plugin_row_count: usize) -> f32 {
    settings_plugins_content_height_for_status(plugin_row_count, false)
}

/// Plugin section height including an optional real lifecycle status row.
pub fn settings_plugins_content_height_for_status(
    plugin_row_count: usize,
    status_present: bool,
) -> f32 {
    let base = SETTINGS_SECTION_LABEL_H + SETTINGS_PLUGIN_INSTALL_BTN_H;
    let rows = plugin_row_count.min(SETTINGS_PLUGINS_ROW_VISIBLE_MAX);
    let list = if rows == 0 {
        SETTINGS_PLUGIN_CARD_GAP + SETTINGS_PLUGIN_EMPTY_ROW_H
    } else {
        SETTINGS_PLUGIN_CARD_GAP
            + SETTINGS_PLUGIN_CARD_H * rows as f32
            + SETTINGS_PLUGIN_CARD_GAP * (rows as f32 - 1.0)
    };
    base + if status_present {
        SETTINGS_PLUGIN_STATUS_H + SETTINGS_PLUGIN_CARD_GAP
    } else {
        0.0
    } + list
        + SETTINGS_SECTION_GAP
}
