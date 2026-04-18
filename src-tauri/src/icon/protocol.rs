//! Custom protocol handler for `bentodesk://icon/{hash}` and icon caching helper.
//!
//! The WebView2 frontend requests icons via this protocol. The handler looks up
//! the icon hash in the hot-tier cache first; on miss it consults the warm
//! tier (on-disk PNG) and promotes the entry into the hot tier. If neither
//! tier has the bytes, a 1x1 transparent placeholder is returned to prevent
//! rendering errors — callers are expected to have called
//! [`extract_and_cache`] first to populate the cache.

use std::sync::Arc;

use tauri::http::{Request, Response};
use tauri::AppHandle;

use super::cache::IconCache;
use super::custom_icons;
use super::extractor;

/// Handle requests to the `bentodesk://...` custom protocol.
///
/// Routes:
/// * `bentodesk://icon/{hash}` — extracted Windows shell icon (hot/warm cache).
/// * `bentodesk://custom-icon/{uuid}` — user-uploaded icon from disk.
///
/// Returns a placeholder 1x1 transparent PNG when the requested asset is
/// unavailable so `<img src>` tags do not show broken images.
pub fn handle_icon_request(
    handle: &AppHandle,
    cache: &IconCache,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let uri = request.uri().to_string();

    if let Some(rest) = uri
        .strip_prefix("bentodesk://custom-icon/")
        .or_else(|| uri.strip_prefix("bentodesk://custom-icon\\"))
    {
        let uuid = urlencoding::decode(rest)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| rest.to_string());
        // Strip any query string — the frontend may append `?t={timestamp}` to
        // defeat stale browser caching.
        let clean = uuid.split('?').next().unwrap_or(&uuid).to_string();
        if let Some((bytes, mime)) = custom_icons::read_bytes(handle, &clean) {
            return Response::builder()
                .status(200)
                .header("Content-Type", mime)
                .header("Cache-Control", "no-cache")
                .body(bytes)
                .expect("failed to build custom-icon response");
        }
        return placeholder_response();
    }

    let hash = uri
        .strip_prefix("bentodesk://icon/")
        .or_else(|| uri.strip_prefix("bentodesk://icon\\"))
        .unwrap_or("");

    let decoded_hash = urlencoding::decode(hash)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| hash.to_string());

    if let Some(arc) = cache.get(&decoded_hash) {
        let body: Vec<u8> = Arc::try_unwrap(arc).unwrap_or_else(|shared| (*shared).clone());
        Response::builder()
            .status(200)
            .header("Content-Type", "image/png")
            .header("Cache-Control", "no-cache")
            .body(body)
            .expect("failed to build icon response")
    } else {
        placeholder_response()
    }
}

fn placeholder_response() -> Response<Vec<u8>> {
    Response::builder()
        .status(200)
        .header("Content-Type", "image/png")
        .body(create_transparent_pixel())
        .expect("failed to build placeholder response")
}

/// Extract an icon for a file path and store it in the cache, returning the hash.
///
/// For `.lnk` shortcut files, the target is resolved first so the cache key is
/// based on the resolved target path. This ensures that re-adding a shortcut
/// after the icon resolution fix picks up the correct (non-generic) icon instead
/// of serving a stale cached generic icon keyed by the `.lnk` path.
///
/// If the icon is already cached (hot or warm tier), extraction is skipped.
/// Returns the hash string suitable for constructing a `bentodesk://icon/{hash}` URL.
pub fn extract_and_cache(
    cache: &IconCache,
    path: &str,
) -> Result<String, crate::error::BentoDeskError> {
    extract_and_cache_inner(cache, path, false)
}

/// Same as [`extract_and_cache`] but evicts any existing cache entry first,
/// guaranteeing a fresh extraction. Used by `add_item` to ensure stale generic
/// icons are replaced with the correct application icon.
pub fn extract_and_cache_fresh(
    cache: &IconCache,
    path: &str,
) -> Result<String, crate::error::BentoDeskError> {
    extract_and_cache_inner(cache, path, true)
}

fn extract_and_cache_inner(
    cache: &IconCache,
    path: &str,
    force: bool,
) -> Result<String, crate::error::BentoDeskError> {
    let effective_path = if path.to_lowercase().ends_with(".lnk") {
        extractor::resolve_lnk_target(path).unwrap_or_else(|| path.to_string())
    } else {
        path.to_string()
    };

    let hash = extractor::compute_icon_hash(&effective_path);

    if force {
        cache.remove(&hash);
        let lnk_hash = extractor::compute_icon_hash(path);
        if lnk_hash != hash {
            cache.remove(&lnk_hash);
        }
    }

    if !cache.contains_any_tier(&hash) {
        let png_data = extractor::extract_icon_png(path)?;
        cache.put(hash.clone(), png_data);
    }
    Ok(hash)
}

fn create_transparent_pixel() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0]));
    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    image::ImageEncoder::write_image(encoder, img.as_raw(), 1, 1, image::ExtendedColorType::Rgba8)
        .expect("encoding 1x1 PNG should not fail");
    buf
}
