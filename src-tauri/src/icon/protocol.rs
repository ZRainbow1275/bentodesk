//! Custom protocol handler for `bentodesk://icon/{hash}` and icon caching helper.
//!
//! The WebView2 frontend requests icons via this protocol. The handler looks up
//! the icon hash in the LRU cache and returns PNG bytes. If the hash is not
//! cached (e.g. evicted), a 1x1 transparent placeholder is returned to prevent
//! rendering errors. Use [`extract_and_cache`] to pre-populate the cache before
//! serving requests.

use tauri::http::{Request, Response};

use super::cache::IconCache;
use super::extractor;

/// Handle requests to the `bentodesk://icon/{hash}` custom protocol.
///
/// If the icon is found in the cache, serve it directly with a 1-hour
/// cache-control header. Otherwise, return a transparent 1x1 PNG placeholder
/// so the frontend can display a loading state without a broken image.
pub fn handle_icon_request(
    cache: &IconCache,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let uri = request.uri().to_string();

    // Parse hash from URI: bentodesk://icon/{hash}
    let hash = uri
        .strip_prefix("bentodesk://icon/")
        .or_else(|| uri.strip_prefix("bentodesk://icon\\"))
        .unwrap_or("");

    let decoded_hash = urlencoding::decode(hash)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| hash.to_string());

    if let Some(png_data) = cache.get(&decoded_hash) {
        Response::builder()
            .status(200)
            .header("Content-Type", "image/png")
            .header("Cache-Control", "no-cache")
            .body(png_data)
            .expect("failed to build icon response")
    } else {
        // Icon not found in cache -- return a 1x1 transparent PNG placeholder
        let placeholder = create_transparent_pixel();
        Response::builder()
            .status(200)
            .header("Content-Type", "image/png")
            .body(placeholder)
            .expect("failed to build placeholder response")
    }
}

/// Extract an icon for a file path and store it in the cache, returning the hash.
///
/// For `.lnk` shortcut files, the target is resolved first so the cache key is
/// based on the resolved target path. This ensures that re-adding a shortcut
/// after the icon resolution fix picks up the correct (non-generic) icon instead
/// of serving a stale cached generic icon keyed by the `.lnk` path.
///
/// If the icon is already cached (by hash), extraction is skipped. Returns the
/// hash string suitable for constructing a `bentodesk://icon/{hash}` URL.
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
    // For .lnk files, use the resolved target path as cache key so we don't
    // serve a stale generic icon cached under the .lnk path hash.
    let effective_path = if path.to_lowercase().ends_with(".lnk") {
        extractor::resolve_lnk_target(path).unwrap_or_else(|| path.to_string())
    } else {
        path.to_string()
    };

    let hash = extractor::compute_icon_hash(&effective_path);

    if force {
        cache.remove(&hash);
        // Also evict any stale entry keyed by the raw .lnk path (from older code)
        let lnk_hash = extractor::compute_icon_hash(path);
        if lnk_hash != hash {
            cache.remove(&lnk_hash);
        }
    }

    if !cache.contains(&hash) {
        let png_data = extractor::extract_icon_png(path)?;
        cache.put(hash.clone(), png_data);
    }
    Ok(hash)
}

/// Create a minimal valid 1x1 transparent PNG.
///
/// Used as a fallback when a requested icon hash is not in the cache.
fn create_transparent_pixel() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0]));
    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        1,
        1,
        image::ExtendedColorType::Rgba8,
    )
    .expect("encoding 1x1 PNG should not fail");
    buf
}
