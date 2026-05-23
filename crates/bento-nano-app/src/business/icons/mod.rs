//! Business surface — `ZoneIcon` / `LucideDynamic` family (T-079).
//!
//! Visual spec: see `icons.snap.md`. The `IconRef` enum is the locked
//! wire-format dispatcher; the `IconKind` enum lifts the 30 hand-rolled
//! 1.x SVGs into compile-checked variants. Both round-trip through serde
//! so 1.x layout JSON keeps loading verbatim.
//!
//! Status: selected-stack reachable. The 30 source Tauri SVG documents are
//! embedded as static literals and are rendered by the D2D IconPicker path.

use bento_nano_layout::Direction;
use bento_nano_style::Length;
use bento_nano_widget::{ContainerNode, WidgetNode};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Default render size (logical px) for any icon when the caller doesn't
/// supply one. Mirrors 1.x `props.size ?? 20`.
pub const ICON_DEFAULT_SIZE_PX: f32 = 20.0;

/// Built-in zone icon — the 30 hand-rolled 1.x line-art SVGs. Variant order
/// matches the 1.x `ZONE_ICONS` registry insertion order so listing
/// surfaces (icon picker grid) render identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IconKind {
    #[default]
    Folder,
    Document,
    Image,
    Music,
    Video,
    Code,
    Download,
    Archive,
    Star,
    Bookmark,
    Tag,
    Globe,
    Lightning,
    Briefcase,
    Gamepad,
    Palette,
    #[serde(alias = "arrow-right")]
    ArrowRight,
    Trash,
    Search,
    Copy,
    #[serde(alias = "external-link")]
    ExternalLink,
    #[serde(alias = "folder-open")]
    FolderOpen,
    Camera,
    Columns,
    X,
    Edit,
    Grid,
    Square,
    Pin,
    Settings,
}

impl IconKind {
    /// Return the lower-snake-case name as it appears in 1.x `zones[i].icon`.
    /// This is what `serde_json::to_string` emits (without quotes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Document => "document",
            Self::Image => "image",
            Self::Music => "music",
            Self::Video => "video",
            Self::Code => "code",
            Self::Download => "download",
            Self::Archive => "archive",
            Self::Star => "star",
            Self::Bookmark => "bookmark",
            Self::Tag => "tag",
            Self::Globe => "globe",
            Self::Lightning => "lightning",
            Self::Briefcase => "briefcase",
            Self::Gamepad => "gamepad",
            Self::Palette => "palette",
            Self::ArrowRight => "arrow_right",
            Self::Trash => "trash",
            Self::Search => "search",
            Self::Copy => "copy",
            Self::ExternalLink => "external_link",
            Self::FolderOpen => "folder_open",
            Self::Camera => "camera",
            Self::Columns => "columns",
            Self::X => "x",
            Self::Edit => "edit",
            Self::Grid => "grid",
            Self::Square => "square",
            Self::Pin => "pin",
            Self::Settings => "settings",
        }
    }

    /// Inverse of `as_str` — useful when adopting layout fields that aren't
    /// yet typed. Returns `None` for unknown names so the caller can fall
    /// back to `IconRef::Text`.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        const TABLE: &[(&str, IconKind)] = &[
            ("folder", IconKind::Folder),
            ("document", IconKind::Document),
            ("image", IconKind::Image),
            ("music", IconKind::Music),
            ("video", IconKind::Video),
            ("code", IconKind::Code),
            ("download", IconKind::Download),
            ("archive", IconKind::Archive),
            ("star", IconKind::Star),
            ("bookmark", IconKind::Bookmark),
            ("tag", IconKind::Tag),
            ("globe", IconKind::Globe),
            ("lightning", IconKind::Lightning),
            ("briefcase", IconKind::Briefcase),
            ("gamepad", IconKind::Gamepad),
            ("palette", IconKind::Palette),
            ("arrow_right", IconKind::ArrowRight),
            ("arrow-right", IconKind::ArrowRight),
            ("trash", IconKind::Trash),
            ("search", IconKind::Search),
            ("copy", IconKind::Copy),
            ("external_link", IconKind::ExternalLink),
            ("external-link", IconKind::ExternalLink),
            ("folder_open", IconKind::FolderOpen),
            ("folder-open", IconKind::FolderOpen),
            ("camera", IconKind::Camera),
            ("columns", IconKind::Columns),
            ("x", IconKind::X),
            ("edit", IconKind::Edit),
            ("grid", IconKind::Grid),
            ("square", IconKind::Square),
            ("pin", IconKind::Pin),
            ("settings", IconKind::Settings),
        ];
        TABLE.iter().find(|(k, _)| *k == s).map(|(_, v)| *v)
    }

    /// Returns true when `wire` names this icon in either the current
    /// selected-stack snake_case form or the source Tauri hyphenated form.
    pub fn matches_wire(self, wire: &str) -> bool {
        Self::from_str_opt(wire) == Some(self)
    }

    /// Full 24x24 SVG document from the Tauri `ZoneIcons.tsx` source baseline.
    pub const fn source_svg(self) -> &'static str {
        match self {
            Self::Folder => {
                r#"<svg viewBox="0 0 24 24"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>"#
            }
            Self::Document => {
                r#"<svg viewBox="0 0 24 24"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/></svg>"#
            }
            Self::Image => {
                r#"<svg viewBox="0 0 24 24"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>"#
            }
            Self::Music => {
                r#"<svg viewBox="0 0 24 24"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>"#
            }
            Self::Video => {
                r#"<svg viewBox="0 0 24 24"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2" ry="2"/></svg>"#
            }
            Self::Code => {
                r#"<svg viewBox="0 0 24 24"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>"#
            }
            Self::Download => {
                r#"<svg viewBox="0 0 24 24"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>"#
            }
            Self::Archive => {
                r#"<svg viewBox="0 0 24 24"><polyline points="21 8 21 21 3 21 3 8"/><rect x="1" y="3" width="22" height="5"/><line x1="10" y1="12" x2="14" y2="12"/></svg>"#
            }
            Self::Star => {
                r#"<svg viewBox="0 0 24 24"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>"#
            }
            Self::Bookmark => {
                r#"<svg viewBox="0 0 24 24"><path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z"/></svg>"#
            }
            Self::Tag => {
                r#"<svg viewBox="0 0 24 24"><path d="M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z"/><line x1="7" y1="7" x2="7.01" y2="7"/></svg>"#
            }
            Self::Globe => {
                r#"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg>"#
            }
            Self::Lightning => {
                r#"<svg viewBox="0 0 24 24"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg>"#
            }
            Self::Briefcase => {
                r#"<svg viewBox="0 0 24 24"><rect x="2" y="7" width="20" height="14" rx="2" ry="2"/><path d="M16 21V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v16"/></svg>"#
            }
            Self::Gamepad => {
                r#"<svg viewBox="0 0 24 24"><line x1="6" y1="12" x2="10" y2="12"/><line x1="8" y1="10" x2="8" y2="14"/><line x1="15" y1="13" x2="15.01" y2="13"/><line x1="18" y1="11" x2="18.01" y2="11"/><rect x="2" y="6" width="20" height="12" rx="2"/></svg>"#
            }
            Self::Palette => {
                r#"<svg viewBox="0 0 24 24"><circle cx="13.5" cy="6.5" r="0.5"/><circle cx="17.5" cy="10.5" r="0.5"/><circle cx="8.5" cy="7.5" r="0.5"/><circle cx="6.5" cy="12" r="0.5"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h1.996c3.051 0 5.555-2.503 5.555-5.554C21.965 6.012 17.461 2 12 2z"/></svg>"#
            }
            Self::ArrowRight => {
                r#"<svg viewBox="0 0 24 24"><line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/></svg>"#
            }
            Self::Trash => {
                r#"<svg viewBox="0 0 24 24"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/></svg>"#
            }
            Self::Search => {
                r#"<svg viewBox="0 0 24 24"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>"#
            }
            Self::Copy => {
                r#"<svg viewBox="0 0 24 24"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>"#
            }
            Self::ExternalLink => {
                r#"<svg viewBox="0 0 24 24"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/></svg>"#
            }
            Self::FolderOpen => {
                r#"<svg viewBox="0 0 24 24"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2v1"/><path d="M2 10l2.6 8.4a1 1 0 0 0 1 .6h12.8a1 1 0 0 0 1-.6L22 10H2z"/></svg>"#
            }
            Self::Camera => {
                r#"<svg viewBox="0 0 24 24"><path d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z"/><circle cx="12" cy="13" r="4"/></svg>"#
            }
            Self::Columns => {
                r#"<svg viewBox="0 0 24 24"><rect x="3" y="3" width="7" height="18" rx="1"/><rect x="14" y="3" width="7" height="18" rx="1"/></svg>"#
            }
            Self::X => {
                r#"<svg viewBox="0 0 24 24"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>"#
            }
            Self::Edit => {
                r#"<svg viewBox="0 0 24 24"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>"#
            }
            Self::Grid => {
                r#"<svg viewBox="0 0 24 24"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/></svg>"#
            }
            Self::Square => {
                r#"<svg viewBox="0 0 24 24"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/></svg>"#
            }
            Self::Pin => {
                r#"<svg viewBox="0 0 24 24"><line x1="12" y1="17" x2="12" y2="22"/><path d="M9 10.76V8a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2.76l3 6.24v3H6v-3z"/></svg>"#
            }
            Self::Settings => {
                r#"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>"#
            }
        }
    }
}

/// All 30 built-in icon kinds in registry order — used by the icon picker
/// to render the gallery. Mirrors 1.x `ZONE_ICON_NAMES`.
pub const ALL_ICON_KINDS: [IconKind; 30] = [
    IconKind::Folder,
    IconKind::Document,
    IconKind::Image,
    IconKind::Music,
    IconKind::Video,
    IconKind::Code,
    IconKind::Download,
    IconKind::Archive,
    IconKind::Star,
    IconKind::Bookmark,
    IconKind::Tag,
    IconKind::Globe,
    IconKind::Lightning,
    IconKind::Briefcase,
    IconKind::Gamepad,
    IconKind::Palette,
    IconKind::ArrowRight,
    IconKind::Trash,
    IconKind::Search,
    IconKind::Copy,
    IconKind::ExternalLink,
    IconKind::FolderOpen,
    IconKind::Camera,
    IconKind::Columns,
    IconKind::X,
    IconKind::Edit,
    IconKind::Grid,
    IconKind::Square,
    IconKind::Pin,
    IconKind::Settings,
];

/// Three-namespace icon dispatcher. Mirrors the four branches of 1.x
/// `ZoneIcon`'s `parsed` memo. `Text` covers both "bare unknown name"
/// and "explicit emoji" — both are rendered as a single text run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconRef {
    Builtin(IconKind),
    Lucide(SmolStr),
    Custom(SmolStr),
    Text(SmolStr),
}

impl IconRef {
    /// Parse a 1.x icon string. Single pass; never allocates beyond the
    /// stored payload.
    pub fn parse(raw: &str) -> Self {
        if let Some(rest) = raw.strip_prefix("lucide:") {
            return Self::Lucide(SmolStr::new(rest));
        }
        if let Some(rest) = raw.strip_prefix("custom:") {
            return Self::Custom(SmolStr::new(rest));
        }
        if let Some(kind) = IconKind::from_str_opt(raw) {
            return Self::Builtin(kind);
        }
        Self::Text(SmolStr::new(raw))
    }
}

/// Build the icon container at the default size.
pub fn build() -> WidgetNode {
    build_with(ICON_DEFAULT_SIZE_PX)
}

/// Build the icon container at a given square size (logical px).
pub fn build_with(size_px: f32) -> WidgetNode {
    WidgetNode::Container(ContainerNode {
        direction: Direction::Row,
        width: Length::Px(size_px),
        height: Length::Px(size_px),
        ..ContainerNode::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_layout::LayoutSource;

    #[test]
    fn default_size_matches_snap_md() {
        assert!((ICON_DEFAULT_SIZE_PX - 20.0).abs() < 0.01);
    }

    #[test]
    fn all_kinds_round_trip_through_str() {
        for kind in ALL_ICON_KINDS {
            let s = kind.as_str();
            assert_eq!(IconKind::from_str_opt(s), Some(kind));
        }
    }

    #[test]
    fn all_kinds_count() {
        assert_eq!(ALL_ICON_KINDS.len(), 30);
    }

    #[test]
    fn parse_lucide_namespace() {
        assert_eq!(
            IconRef::parse("lucide:home"),
            IconRef::Lucide(SmolStr::new("home"))
        );
        assert_eq!(IconRef::parse("lucide:"), IconRef::Lucide(SmolStr::new("")));
    }

    #[test]
    fn parse_custom_namespace() {
        assert_eq!(
            IconRef::parse("custom:abcd-1234"),
            IconRef::Custom(SmolStr::new("abcd-1234"))
        );
    }

    #[test]
    fn parse_builtin_name() {
        assert_eq!(IconRef::parse("folder"), IconRef::Builtin(IconKind::Folder));
        assert_eq!(
            IconRef::parse("external_link"),
            IconRef::Builtin(IconKind::ExternalLink)
        );
        assert_eq!(
            IconRef::parse("external-link"),
            IconRef::Builtin(IconKind::ExternalLink)
        );
        assert_eq!(
            IconRef::parse("folder-open"),
            IconRef::Builtin(IconKind::FolderOpen)
        );
    }

    #[test]
    fn parse_unknown_falls_through_to_text() {
        assert_eq!(IconRef::parse(""), IconRef::Text(SmolStr::new("")));
        assert_eq!(
            IconRef::parse("\u{1F4A1}"),
            IconRef::Text(SmolStr::new("\u{1F4A1}"))
        );
        assert_eq!(
            IconRef::parse("not_a_real_icon"),
            IconRef::Text(SmolStr::new("not_a_real_icon"))
        );
    }

    #[test]
    fn build_default_is_20_square() {
        let node = build();
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Px(w) if (w - 20.0).abs() < 0.01));
        assert!(matches!(layout.height, Length::Px(h) if (h - 20.0).abs() < 0.01));
    }

    #[test]
    fn icon_kind_serde_uses_snake_case() {
        // Lock the wire-format: 1.x JSON had `"icon": "folder_open"`, etc.
        assert_eq!(
            serde_json::to_string(&IconKind::FolderOpen).unwrap_or_default(),
            "\"folder_open\""
        );
        assert_eq!(
            serde_json::to_string(&IconKind::ExternalLink).unwrap_or_default(),
            "\"external_link\""
        );
        assert_eq!(
            serde_json::to_string(&IconKind::ArrowRight).unwrap_or_default(),
            "\"arrow_right\""
        );
        let back: IconKind = serde_json::from_str("\"folder\"").unwrap_or_default();
        assert_eq!(back, IconKind::Folder);
        let hyphen_back: IconKind = serde_json::from_str("\"folder-open\"").unwrap_or_default();
        assert_eq!(hyphen_back, IconKind::FolderOpen);
    }

    #[test]
    fn source_svg_documents_parse_through_selected_stack_parser() {
        for kind in ALL_ICON_KINDS {
            let parsed = bento_nano_platform::svg::Parsed::from_bytes(kind.source_svg().as_bytes());
            assert!(
                parsed.is_ok(),
                "{} svg failed to parse: {:?}",
                kind.as_str(),
                parsed.err()
            );
        }
    }
}
