//! Business surface — TrayMenu popup (right-click on tray icon).
//!
//! Visual spec: `tray_menu.snap.md`. Composition lands when widget-library
//! ships ContextMenu (T-030) + Popup (T-024) — the menu surface composes
//! from a `WindowKind::ContextMenu` HWND already exposed by the window
//! factory (T-011), with menu items rendered via List (T-026).
//!
//! NOTE: The OS-level tray icon registration (`Shell_NotifyIconW`) lives
//! in `bento-nano-shell`'s wndproc — this module owns ONLY the popup menu
//! surface that draws when the user right-clicks the tray icon. The
//! split mirrors the team-lead's pre-decision boundary
//! (business-ui-1 → team-lead, 2026-05-03): shell owns OS resource
//! lifecycle, business owns the rendered widget tree.
//!
//! Status: active model. The selected-stack shell consumes
//! [`TrayMenuItem::ORDER`] and [`TrayMenuItem::label`] to build a native
//! Win32 `TrackPopupMenu` surface today; [`build`] remains a D2D container
//! seed for any future in-app ContextMenu renderer.

use bento_nano_widget::WidgetNode;

use bento_nano_layout::Direction;
use bento_nano_style::{Edges, Length};
use bento_nano_widget::ContainerNode;

/// Build the optional D2D TrayMenu popup subtree. The runtime tray menu is
/// currently a native Win32 popup sourced from [`TrayMenuItem::ORDER`]; this
/// keeps the future in-app ContextMenu path sharing the same business model.
pub fn build() -> WidgetNode {
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        // Default popup width — snap.md mandate. Auto-grows with content
        // when the items list ships.
        width: Length::Px(POPUP_DEFAULT_WIDTH),
        height: Length::Auto,
        padding: Edges::all(POPUP_INNER_PADDING),
        ..ContainerNode::default()
    })
}

/// Default popup width per snap.md (auto-grows when items render wider).
pub const POPUP_DEFAULT_WIDTH: f32 = 200.0;

/// Inner padding around the items list per snap.md.
pub const POPUP_INNER_PADDING: f32 = 4.0;

/// Per-item row height per snap.md.
pub const ITEM_HEIGHT: f32 = 32.0;

/// Open animation duration (ms) per snap.md — 120 ms EaseOut on scale +
/// opacity. Pinned today so the next-pass animation primitive doesn't
/// re-derive the value.
pub const POPUP_OPEN_DURATION_MS: u32 = 120;

/// Closed enum of every TrayMenu item. The shell maps each variant to a
/// concrete dispatcher [`crate::dispatcher::Command`] when the user clicks.
/// Order matches the 1.x menu order from `src-tauri/src/tray/menu.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrayMenuItem {
    /// Toggle the main window's visibility. Label flips between
    /// "显示 BentoDesk" (when hidden) and "隐藏 BentoDesk" (when visible).
    ShowHideMain,
    /// Open the new-zone wizard. (1.x emits "tray_new_zone" — nano routes
    /// to `Command::CreateZone` after the wizard collects a spec.)
    NewZone,
    /// Trigger desktop auto-organize. (1.x emits "tray_auto_organize".)
    AutoOrganize,
    /// Open the SettingsPanel. (Routes to `Command::OpenSettings`.)
    OpenSettings,
    /// Open the About modal.
    About,
    /// Quit the application. (Routes to `Command::QuitApp`.)
    Exit,
}

impl TrayMenuItem {
    /// Display order — matches the 1.x menu top-to-bottom.
    pub const ORDER: &'static [TrayMenuItem] = &[
        Self::ShowHideMain,
        // Divider after ShowHideMain.
        Self::NewZone,
        Self::AutoOrganize,
        // Divider after AutoOrganize.
        Self::OpenSettings,
        Self::About,
        // Divider after About.
        Self::Exit,
    ];

    /// `true` when a divider should render BEFORE this item. Matches the
    /// 1.x `Menu::with_items` separator placement (after ShowHideMain,
    /// AutoOrganize, About).
    pub const fn needs_divider_before(&self) -> bool {
        matches!(self, Self::NewZone | Self::OpenSettings | Self::Exit)
    }

    /// Localised label for the menu item. Today returns Chinese to match
    /// the 1.x verbatim; the locale-switching handle lands when the
    /// `bento-nano-style::i18n` table grows the menu keys.
    ///
    /// `main_visible` parameter only affects [`Self::ShowHideMain`]'s label
    /// (other variants ignore it). Mirrors the 1.x `set_text` toggle on
    /// the show/hide menu item when the main window's visibility flips.
    pub const fn label(&self, main_visible: bool) -> &'static str {
        match self {
            Self::ShowHideMain => {
                if main_visible {
                    "隐藏 BentoDesk"
                } else {
                    "显示 BentoDesk"
                }
            }
            Self::NewZone => "新建区域",
            Self::AutoOrganize => "智能整理桌面",
            Self::OpenSettings => "设置",
            Self::About => "关于",
            Self::Exit => "退出",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_matches_one_x_menu_layout() {
        // Six items in the 1.x menu order; pinning so a refactor can't
        // silently re-order them.
        assert_eq!(TrayMenuItem::ORDER.len(), 6);
        assert_eq!(TrayMenuItem::ORDER[0], TrayMenuItem::ShowHideMain);
        assert_eq!(TrayMenuItem::ORDER[5], TrayMenuItem::Exit);
    }

    #[test]
    fn dividers_split_into_three_logical_groups() {
        // Per snap.md: divider before NewZone, before OpenSettings, before
        // Exit. Three dividers across six items → 4 groups (R "Show/Hide",
        // "Zone ops", "App actions", "Exit").
        let with_dividers: usize = TrayMenuItem::ORDER
            .iter()
            .filter(|i| i.needs_divider_before())
            .count();
        assert_eq!(with_dividers, 3);
    }

    #[test]
    fn show_hide_label_flips_with_main_visibility() {
        assert_eq!(TrayMenuItem::ShowHideMain.label(true), "隐藏 BentoDesk");
        assert_eq!(TrayMenuItem::ShowHideMain.label(false), "显示 BentoDesk");
    }

    #[test]
    fn other_labels_are_visibility_independent() {
        // The `main_visible` arg should NOT affect non-show/hide labels.
        for item in [
            TrayMenuItem::NewZone,
            TrayMenuItem::AutoOrganize,
            TrayMenuItem::OpenSettings,
            TrayMenuItem::About,
            TrayMenuItem::Exit,
        ] {
            assert_eq!(item.label(true), item.label(false));
        }
    }

    #[test]
    fn build_returns_padded_popup_width_container() {
        use bento_nano_layout::LayoutSource;
        use bento_nano_style::Length;
        let node = build();
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Px(w) if (w - POPUP_DEFAULT_WIDTH).abs() < 0.01));
        assert!((layout.padding.left - POPUP_INNER_PADDING).abs() < 0.01);
    }
}
