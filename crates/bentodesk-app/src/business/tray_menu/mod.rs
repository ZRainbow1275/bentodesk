//! Business surface — TrayMenu popup (right-click on tray icon).
//!
//! Visual spec: `tray_menu.snap.md`. Composition lands when widget-library
//! ships ContextMenu (T-030) + Popup (T-024) — the menu surface composes
//! from a `WindowKind::ContextMenu` HWND already exposed by the window
//! factory (T-011), with menu items rendered via List (T-026).
//!
//! NOTE: The OS-level tray icon registration (`Shell_NotifyIconW`) lives
//! in `bentodesk-shell`'s wndproc — this module owns ONLY the popup menu
//! surface that draws when the user right-clicks the tray icon. The
//! split mirrors the team-lead's pre-decision boundary
//! (business-ui-1 → team-lead, 2026-05-03): shell owns OS resource
//! lifecycle, business owns the rendered widget tree.
//!
//! Status: active model. The selected-stack shell consumes
//! [`TrayMenuItem::ORDER`] and [`TrayMenuItem::label`] to build a native
//! Win32 `TrackPopupMenu` surface today; [`build`] remains a D2D container
//! seed for any future in-app ContextMenu renderer.

use bentodesk_widget::WidgetNode;

use bentodesk_layout::Direction;
use bentodesk_style::{Edges, Length};
use bentodesk_widget::ContainerNode;

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
    /// Open the new-zone wizard. (1.x emits "tray_new_zone" — native routes
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

    /// `main_visible` parameter only affects [`Self::ShowHideMain`]'s label
    /// (other variants ignore it). Mirrors the 1.x `set_text` toggle on
    /// the show/hide menu item when the main window's visibility flips.
    pub fn label(&self, main_visible: bool) -> &'static str {
        self.label_for_language(
            main_visible,
            bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN),
        )
    }

    /// Pure locale-specific label selection. Keeping this separate from the
    /// process-global locale lookup makes the complete Chinese/English menu
    /// contract testable without racing the global locale pointer.
    pub const fn label_for_language(&self, main_visible: bool, zh: bool) -> &'static str {
        match (self, main_visible, zh) {
            (Self::ShowHideMain, true, true) => "隐藏 BentoDesk",
            (Self::ShowHideMain, false, true) => "显示 BentoDesk",
            (Self::ShowHideMain, true, false) => "Hide BentoDesk",
            (Self::ShowHideMain, false, false) => "Show BentoDesk",
            (Self::NewZone, _, true) => "新建区域",
            (Self::NewZone, _, false) => "New Zone",
            (Self::AutoOrganize, _, true) => "智能整理桌面",
            (Self::AutoOrganize, _, false) => "Organize Desktop",
            (Self::OpenSettings, _, true) => "设置",
            (Self::OpenSettings, _, false) => "Settings",
            (Self::About, _, true) => "关于",
            (Self::About, _, false) => "About",
            (Self::Exit, _, true) => "退出",
            (Self::Exit, _, false) => "Quit",
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
        assert_eq!(
            TrayMenuItem::ShowHideMain.label_for_language(true, true),
            "隐藏 BentoDesk"
        );
        assert_eq!(
            TrayMenuItem::ShowHideMain.label_for_language(false, true),
            "显示 BentoDesk"
        );
        assert_eq!(
            TrayMenuItem::ShowHideMain.label_for_language(true, false),
            "Hide BentoDesk"
        );
        assert_eq!(
            TrayMenuItem::ShowHideMain.label_for_language(false, false),
            "Show BentoDesk"
        );
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
            assert_eq!(
                item.label_for_language(true, true),
                item.label_for_language(false, true)
            );
            assert_eq!(
                item.label_for_language(true, false),
                item.label_for_language(false, false)
            );
        }
    }

    #[test]
    fn all_menu_items_have_complete_chinese_and_english_labels() {
        for item in TrayMenuItem::ORDER {
            assert!(!item.label_for_language(true, true).is_empty());
            assert!(!item.label_for_language(true, false).is_empty());
        }
        assert_eq!(
            TrayMenuItem::AutoOrganize.label_for_language(true, false),
            "Organize Desktop"
        );
        assert_eq!(
            TrayMenuItem::OpenSettings.label_for_language(true, false),
            "Settings"
        );
    }

    #[test]
    fn build_returns_padded_popup_width_container() {
        use bentodesk_layout::LayoutSource;
        use bentodesk_style::Length;
        let node = build();
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Px(w) if (w - POPUP_DEFAULT_WIDTH).abs() < 0.01));
        assert!((layout.padding.left - POPUP_INNER_PADDING).abs() < 0.01);
    }
}
