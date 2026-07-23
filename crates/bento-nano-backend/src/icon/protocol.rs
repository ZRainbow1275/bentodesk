//! T-080 — direct in-process adapter replacing the 1.x
//! `bentodesk://icon/...` Tauri custom protocol.
//!
//! ## ΔE — Tauri custom protocol → in-process call
//!
//! 1.x registered a custom URL scheme through
//! `tauri::Builder::register_uri_scheme_protocol("bentodesk", handler)`,
//! and the WebView2 frontend issued HTTP-like `Request<Vec<u8>>`s that
//! Tauri parsed and dispatched. The nano shell has no WebView — paint
//! is direct Direct2D — so the protocol degrades to a normal in-
//! process function call.
//!
//! Callers (typically `bento-nano-app::dispatcher`) invoke
//! [`lookup_icon`] / [`lookup_custom_icon`] with the cache + config and
//! receive an [`IconResponse`]. The response struct mirrors the
//! 200/404 split that the 1.x handler used so existing logging /
//! metrics code can be ported without re-shaping.
//!
//! `extract_and_cache` and `extract_and_cache_fresh` survive verbatim
//! — they're the population side of the same cache.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::cache::IconCache;
use super::extractor;
use super::{IconConfig, IconError, custom_icons};

const INTERNET_SHORTCUT_ICON_CACHE_REVISION: &str = "internet-shortcut-icon-resource-v1";

/// Status code mirroring the 1.x HTTP response shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IconStatus {
    /// Asset found and returned in `body`.
    Ok,
    /// Asset missing. `body` is a UTF-8 plain-text diagnostic
    /// (matches the 1.x `missing_asset_response`).
    NotFound,
}

/// In-process icon lookup response. Replaces the 1.x
/// `tauri::http::Response<Vec<u8>>`.
///
/// `content_type` is owned `String` rather than `&'static str` so the
/// struct is `Deserialize`-friendly without a `Cow<'a>` lifetime
/// parameter (the master plan §11 ΔB ruling requires every public
/// struct derive serde even when single-process Phase 1 never
/// serialises at runtime).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconResponse {
    pub status: IconStatus,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl IconResponse {
    fn ok_png(body: Vec<u8>) -> Self {
        Self {
            status: IconStatus::Ok,
            content_type: "image/png".to_string(),
            body,
        }
    }

    fn ok_with_mime(mime: &str, body: Vec<u8>) -> Self {
        Self {
            status: IconStatus::Ok,
            content_type: mime.to_string(),
            body,
        }
    }

    fn missing(kind: &str, key: &str) -> Self {
        Self {
            status: IconStatus::NotFound,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: format!("missing bentodesk asset: {kind}:{key}").into_bytes(),
        }
    }
}

/// Look up a previously-cached icon by its hash key (the 16-hex
/// `compute_icon_hash` output). Hot tier hit → zero-copy `Arc` payload
/// cloned into the response; warm tier hit → file read + hot promotion.
/// Miss → 404 response.
pub fn lookup_icon(cache: &IconCache, hash: &str) -> IconResponse {
    if let Some(arc) = cache.get(hash) {
        let body: Vec<u8> = Arc::try_unwrap(arc).unwrap_or_else(|shared| (*shared).clone());
        IconResponse::ok_png(body)
    } else {
        IconResponse::missing("icon", hash)
    }
}

/// Look up a user-uploaded custom icon by UUID. The on-disk file is
/// the source of truth; this call always hits disk (custom icons are
/// not in the LRU cache).
pub fn lookup_custom_icon(config: &IconConfig, uuid: &str) -> IconResponse {
    if let Some((bytes, mime)) = custom_icons::read_bytes(config, uuid) {
        IconResponse::ok_with_mime(mime, bytes)
    } else {
        IconResponse::missing("custom-icon", uuid)
    }
}

/// Extract an icon for a file path and store it in the cache, returning
/// the hash. A `.lnk` is keyed by the shortcut path itself: Explorer may
/// assign a shortcut-specific `IconLocation`, so two shortcuts targeting
/// the same executable must not share a cache identity. If the icon is
/// already cached (hot or warm tier), extraction is skipped.
pub fn extract_and_cache(cache: &IconCache, path: &str) -> Result<String, IconError> {
    extract_and_cache_inner(cache, path, false)
}

/// Same as [`extract_and_cache`] but evicts any existing cache entry
/// first, guaranteeing a fresh extraction. Used by add-item flows that
/// need to overwrite a stale generic icon.
pub fn extract_and_cache_fresh(cache: &IconCache, path: &str) -> Result<String, IconError> {
    extract_and_cache_inner(cache, path, true)
}

fn extract_and_cache_inner(
    cache: &IconCache,
    path: &str,
    force: bool,
) -> Result<String, IconError> {
    let hash = icon_cache_key(path);

    if force {
        cache.remove(&hash);
        // Remove the pre-fix target-keyed entry as well. This is intentionally
        // migration-only: normal lookups never key a shortcut by its target.
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".lnk")
            && let Some(target) = extractor::resolve_lnk_target(path)
        {
            let legacy_target_hash = extractor::compute_icon_hash(&target);
            if legacy_target_hash != hash {
                cache.remove(&legacy_target_hash);
            }
        } else if lower.ends_with(".url") {
            let legacy_url_hash = extractor::compute_icon_hash(path);
            if legacy_url_hash != hash {
                cache.remove(&legacy_url_hash);
            }
        }
    }

    if !cache.contains_any_tier(&hash) {
        let png = extractor::extract_icon_png(path)?;
        cache.put(hash.clone(), png);
    }
    Ok(hash)
}

/// Return the stable cache identity for a concrete item path.
///
/// Shortcut identity intentionally belongs to the shortcut itself rather than
/// its resolved target because Explorer can assign a per-shortcut icon. `.url`
/// keys carry an extractor revision so installations with the former generic
/// URL-file icon perform one bounded startup refresh without invalidating all
/// other cached file icons.
pub fn icon_cache_key(path: &str) -> String {
    if path.to_ascii_lowercase().ends_with(".url") {
        extractor::compute_icon_hash(
            format!("{INTERNET_SHORTCUT_ICON_CACHE_REVISION}\0{path}").as_str(),
        )
    } else {
        extractor::compute_icon_hash(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_response_is_explicit_404() {
        let r = IconResponse::missing("icon", "deadbeef");
        assert_eq!(r.status, IconStatus::NotFound);
        assert!(String::from_utf8_lossy(&r.body).contains("deadbeef"));
    }

    #[test]
    fn lookup_icon_hot_hit_returns_ok_png() {
        let cache = IconCache::new(2);
        cache.put("abc".into(), vec![0x89, 0x50]);
        let r = lookup_icon(&cache, "abc");
        assert_eq!(r.status, IconStatus::Ok);
        assert_eq!(r.content_type, "image/png");
        assert_eq!(r.body, vec![0x89, 0x50]);
    }

    #[test]
    fn lookup_icon_miss_returns_not_found() {
        let cache = IconCache::new(2);
        let r = lookup_icon(&cache, "missing");
        assert_eq!(r.status, IconStatus::NotFound);
    }

    #[test]
    fn icon_response_serde_round_trip() {
        let r = IconResponse::ok_png(vec![1, 2, 3]);
        let s = serde_json::to_string(&r).expect("ser");
        let back: IconResponse = serde_json::from_str(&s).expect("de");
        assert_eq!(back.status, IconStatus::Ok);
        assert_eq!(back.body, vec![1, 2, 3]);
    }

    #[test]
    fn extract_and_cache_skips_when_already_cached() {
        let cache = IconCache::new(4);
        // Pre-populate the cache with the hash that
        // `compute_icon_hash` would produce for our test path. We
        // cannot actually call extract_icon_png in a unit test (it
        // needs a real file + Win32 shell), so we only verify the
        // skip-when-cached branch.
        let path = "C:/test/file.txt";
        let h = extractor::compute_icon_hash(path);
        cache.put(h.clone(), vec![1, 2, 3]);
        let got = extract_and_cache(&cache, path).expect("hit");
        assert_eq!(got, h);
    }

    #[test]
    fn shortcut_cache_identity_is_the_shortcut_not_its_target() {
        let left = icon_cache_key("C:/Desktop/Game - Fox.lnk");
        let right = icon_cache_key("C:/Desktop/Game - Blue.lnk");
        let target = extractor::compute_icon_hash("C:/Games/Game/game.exe");

        assert_ne!(left, right);
        assert_ne!(left, target);
        assert_ne!(right, target);
    }

    #[test]
    fn internet_shortcut_cache_identity_revisions_the_legacy_generic_icon() {
        let path = "C:/Desktop/Game.url";
        let legacy = extractor::compute_icon_hash(path);
        let current = icon_cache_key(path);

        assert_ne!(current, legacy);
        assert_eq!(current, icon_cache_key(path));
        assert_eq!(
            icon_cache_key("C:/Desktop/Game.lnk"),
            extractor::compute_icon_hash("C:/Desktop/Game.lnk")
        );
    }
}
