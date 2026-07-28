//! Public types shared across the `search` module.
//!
//! All types derive `Debug, Clone, PartialEq, Serialize, Deserialize` per
//! Wave G cross-cutting rule (dispatcher Command payloads need PartialEq;
//! the ΔB ruling requires serde derives on every public command surface
//! even though the single-process build never serialises at runtime).

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// What kind of thing the SearchBar matched.
///
/// Mirrors the four addressable surfaces the SearchBar can route to in
/// the dispatcher: a desktop file, a desktop folder, a BentoZone, or a
/// settings entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SearchItemKind {
    File,
    Folder,
    Zone,
    Setting,
    Action,
}

/// One indexable record in the SearchBar inverted index.
///
/// `id` is the stable identifier the dispatcher uses to route the
/// "open this hit" command (file path hash for File/Folder, ZoneId for
/// Zone, settings key for Setting). `title` is the display name, `path`
/// is the breadcrumb shown beneath it, and `keywords` contains searchable
/// aliases that are never rendered (for example both Chinese and English
/// command names).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchItem {
    pub id: SmolStr,
    pub title: SmolStr,
    pub path: SmolStr,
    #[serde(default)]
    pub keywords: SmolStr,
    pub kind: SearchItemKind,
}

/// One result row delivered back to the SearchBar UI.
///
/// `score` is opaque (higher = better) — the UI treats it as ordering only.
/// `matched_token` is the indexed token that produced the best score for
/// this hit, surfaced so the UI can highlight the matched span without
/// re-running the matcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub id: SmolStr,
    pub title: SmolStr,
    pub path: SmolStr,
    pub kind: SearchItemKind,
    pub score: u32,
    pub matched_token: SmolStr,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_item_round_trips_through_serde_json() {
        let item = SearchItem {
            id: SmolStr::from("zone-1"),
            title: SmolStr::from("Inbox"),
            path: SmolStr::from("/zones/inbox"),
            keywords: SmolStr::from("收件箱 inbox"),
            kind: SearchItemKind::Zone,
        };
        let json = serde_json::to_string(&item).expect("serialise");
        let back: SearchItem = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(item, back);
    }

    #[test]
    fn legacy_search_item_without_keywords_deserialises() {
        let json = r#"{"id":"zone-1","title":"Inbox","path":"/zones/inbox","kind":"Zone"}"#;
        let item: SearchItem = serde_json::from_str(json).expect("deserialise");
        assert!(item.keywords.is_empty());
    }

    #[test]
    fn search_hit_round_trips_through_serde_json() {
        let hit = SearchHit {
            id: SmolStr::from("file-7"),
            title: SmolStr::from("Readme"),
            path: SmolStr::from("C:/Desktop/readme.md"),
            kind: SearchItemKind::File,
            score: 42,
            matched_token: SmolStr::from("readme"),
        };
        let json = serde_json::to_string(&hit).expect("serialise");
        let back: SearchHit = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(hit, back);
    }

    #[test]
    fn all_item_kinds_round_trip() {
        for kind in [
            SearchItemKind::File,
            SearchItemKind::Folder,
            SearchItemKind::Zone,
            SearchItemKind::Setting,
            SearchItemKind::Action,
        ] {
            let json = serde_json::to_string(&kind).expect("serialise");
            let back: SearchItemKind = serde_json::from_str(&json).expect("deserialise");
            assert_eq!(kind, back);
        }
    }
}
