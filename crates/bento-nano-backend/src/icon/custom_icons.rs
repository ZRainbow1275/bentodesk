//! T-081 — custom user-uploaded icon storage.
//!
//! Direct port of `bentodesk/src-tauri/src/icon/custom_icons.rs` with
//! these mechanical changes:
//!
//! - `tauri::AppHandle` → [`super::IconConfig`].
//! - `crate::storage::state_data_dir(handle)` → `config.app_data_path()`.
//! - `image::load_from_memory_with_format(.., Png)` → WIC validate-decode
//!   via [`super::wic`] (round-trips through `IWICBitmapDecoder`); the
//!   re-encoded bytes are written to disk so any decoder quirks are
//!   normalised at upload time.
//! - `image::load_from_memory_with_format(.., Ico)` → WIC ICO decoder,
//!   then re-encode the largest frame as PNG.
//! - `uuid::Uuid::new_v4().to_string()` → [`super::unique_icon_id()`]
//!   (32-hex-char identifier, see `mod.rs`).
//! - `chrono::Utc::now().to_rfc3339()` → `crate::time::now_rfc3339()`.
//! - All public structs derive `serde::Serialize/Deserialize` per
//!   master plan §11 ΔB ruling.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::svg_sanitize::sanitize_svg;
use super::{IconConfig, IconError, now_iso8601, unique_icon_id, wic};

// ─── Public types ────────────────────────────────────────────────────

/// Per-icon metadata exposed to callers (with resolved `bentodesk://`
/// URL). Identical shape to the 1.x `CustomIcon` struct so the v2.x
/// scripting surface stays compatible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomIcon {
    pub uuid: String,
    pub name: String,
    /// Storage kind after conversion: `"svg"` or `"png"`.
    pub kind: String,
    /// Resolved `bentodesk://custom-icon/{uuid}` URL.
    pub url: String,
    pub created_at: String,
}

/// On-disk metadata index. Persisted as `metadata.json` alongside the
/// icon files. The shape is unchanged from 1.x.
#[derive(Debug, Default, Serialize, Deserialize)]
struct CustomIconIndex {
    icons: Vec<CustomIconMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CustomIconMeta {
    uuid: String,
    name: String,
    kind: String,
    created_at: String,
}

// ─── Internal helpers ────────────────────────────────────────────────

/// Process-wide write lock so concurrent `upload`/`delete` calls
/// don't race on `metadata.json`. The lock is held only while the
/// index is read-modify-written; icon-byte writes happen outside.
static LOCK: Mutex<()> = Mutex::new(());

/// Resolve the custom-icons directory, creating it on demand.
pub fn custom_icons_dir(config: &IconConfig) -> PathBuf {
    let dir = config.custom_icons_dir();
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    dir
}

fn metadata_path(dir: &Path) -> PathBuf {
    dir.join("metadata.json")
}

fn load_index(dir: &Path) -> CustomIconIndex {
    let path = metadata_path(dir);
    if !path.exists() {
        return CustomIconIndex::default();
    }
    match fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => CustomIconIndex::default(),
    }
}

fn save_index(dir: &Path, idx: &CustomIconIndex) -> Result<(), IconError> {
    let path = metadata_path(dir);
    let json = serde_json::to_string_pretty(idx).map_err(|e| IconError::Json {
        path: path.clone(),
        message: e.to_string(),
    })?;
    fs::write(&path, json).map_err(|e| IconError::Io {
        path: path.clone(),
        message: e.to_string(),
    })?;
    Ok(())
}

fn to_custom_icon(meta: &CustomIconMeta) -> CustomIcon {
    CustomIcon {
        uuid: meta.uuid.clone(),
        name: meta.name.clone(),
        kind: meta.kind.clone(),
        url: format!("bentodesk://custom-icon/{}", meta.uuid),
        created_at: meta.created_at.clone(),
    }
}

// ─── Public API ──────────────────────────────────────────────────────

/// Resolve the on-disk path for a stored custom icon. `None` if the
/// UUID is unknown.
pub fn resolve_file(config: &IconConfig, uuid: &str) -> Option<PathBuf> {
    let dir = custom_icons_dir(config);
    let idx = load_index(&dir);
    let meta = idx.icons.iter().find(|m| m.uuid == uuid)?;
    let ext = if meta.kind == "svg" { "svg" } else { "png" };
    Some(dir.join(format!("{uuid}.{ext}")))
}

/// Read the raw bytes + content-type for a stored custom icon. `None`
/// when the UUID is unknown or the file is missing.
pub fn read_bytes(config: &IconConfig, uuid: &str) -> Option<(Vec<u8>, &'static str)> {
    let dir = custom_icons_dir(config);
    let idx = load_index(&dir);
    let meta = idx.icons.iter().find(|m| m.uuid == uuid)?;
    let (ext, mime) = if meta.kind == "svg" {
        ("svg", "image/svg+xml")
    } else {
        ("png", "image/png")
    };
    let path = dir.join(format!("{uuid}.{ext}"));
    let bytes = fs::read(&path).ok()?;
    Some((bytes, mime))
}

/// Upload a user-provided icon. Returns the generated UUID.
///
/// * SVG → sanitised via [`super::svg_sanitize::sanitize_svg`] then
///   stored as `.svg`.
/// * PNG → re-decoded + re-encoded via WIC as a normalisation pass,
///   stored as `.png`.
/// * ICO → decoded via WIC, the largest frame is re-encoded as PNG,
///   stored as `.png` and the recorded `kind` switches to `"png"`.
pub fn upload(
    config: &IconConfig,
    kind: &str,
    bytes: Vec<u8>,
    display_name: &str,
) -> Result<String, IconError> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());

    if bytes.is_empty() {
        return Err(IconError::EmptyUpload);
    }

    let dir = custom_icons_dir(config);
    let uuid = unique_icon_id();
    let kind_norm = kind.to_ascii_lowercase();

    let (stored_kind, file_name) = match kind_norm.as_str() {
        "svg" => {
            let text = String::from_utf8(bytes).map_err(|e| IconError::InvalidPng {
                message: format!("svg payload not valid UTF-8: {e}"),
            })?;
            let clean = sanitize_svg(&text).map_err(|reason| IconError::SvgRejected { reason })?;
            let path = dir.join(format!("{uuid}.svg"));
            fs::write(&path, clean.as_bytes()).map_err(|e| IconError::Io {
                path: path.clone(),
                message: e.to_string(),
            })?;
            ("svg", format!("{uuid}.svg"))
        }
        "png" => {
            let png = validate_and_normalise_png(&bytes)?;
            let path = dir.join(format!("{uuid}.png"));
            fs::write(&path, &png).map_err(|e| IconError::Io {
                path: path.clone(),
                message: e.to_string(),
            })?;
            ("png", format!("{uuid}.png"))
        }
        "ico" => {
            let png = decode_ico_to_png(&bytes)?;
            let path = dir.join(format!("{uuid}.png"));
            fs::write(&path, &png).map_err(|e| IconError::Io {
                path: path.clone(),
                message: e.to_string(),
            })?;
            ("png", format!("{uuid}.png"))
        }
        other => {
            return Err(IconError::UnsupportedKind {
                kind: other.to_string(),
            });
        }
    };

    let mut idx = load_index(&dir);
    idx.icons.push(CustomIconMeta {
        uuid: uuid.clone(),
        name: sanitize_display_name(display_name),
        kind: stored_kind.to_string(),
        created_at: now_iso8601(),
    });
    save_index(&dir, &idx)?;
    tracing::info!("Uploaded custom icon {} as {}", display_name, file_name);
    Ok(uuid)
}

/// List all known custom icons with their resolved URLs.
pub fn list(config: &IconConfig) -> Vec<CustomIcon> {
    let dir = custom_icons_dir(config);
    let idx = load_index(&dir);
    idx.icons.iter().map(to_custom_icon).collect()
}

/// Delete a custom icon by UUID. No-op when the UUID is unknown.
pub fn delete(config: &IconConfig, uuid: &str) -> Result<(), IconError> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = custom_icons_dir(config);
    let mut idx = load_index(&dir);
    let before = idx.icons.len();
    idx.icons.retain(|i| i.uuid != uuid);
    if idx.icons.len() == before {
        return Ok(());
    }
    for ext in &["svg", "png"] {
        let path = dir.join(format!("{uuid}.{ext}"));
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
    }
    save_index(&dir, &idx)?;
    Ok(())
}

// ─── PNG / ICO codec helpers (WIC, replaces `image` crate) ───────────

/// Decode a PNG via WIC and re-encode it. The 1.x version used
/// `image::DynamicImage::write_to(.., Png)` to normalise; we do the
/// same via WIC: decode → grab first frame as RGBA8 → re-encode.
fn validate_and_normalise_png(bytes: &[u8]) -> Result<Vec<u8>, IconError> {
    let (rgba, w, h) = wic_decode_to_rgba(bytes, ContainerFormat::Png)
        .map_err(|message| IconError::InvalidPng { message })?;
    wic::encode_png(&rgba, w, h)
}

/// Decode an ICO via WIC and return the largest frame re-encoded as
/// PNG. Replaces `image::load_from_memory_with_format(.., Ico)`.
fn decode_ico_to_png(bytes: &[u8]) -> Result<Vec<u8>, IconError> {
    let (rgba, w, h) = wic_decode_largest_ico_to_rgba(bytes)
        .map_err(|message| IconError::InvalidIco { message })?;
    wic::encode_png(&rgba, w, h)
}

/// Container-format selector for [`wic_decode_to_rgba`].
enum ContainerFormat {
    Png,
}

/// Internal: WIC decode → 32bppRGBA8 RGBA pixels + dimensions. The
/// caller picks the container format (currently only PNG; ICO uses a
/// specialised "pick largest frame" variant below).
fn wic_decode_to_rgba(bytes: &[u8], fmt: ContainerFormat) -> Result<(Vec<u8>, u32, u32), String> {
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppRGBA,
        IWICBitmapSource, IWICFormatConverter, IWICImagingFactory, IWICStream,
        WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut, WICDecodeMetadataCacheOnLoad,
    };
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
    use windows::core::Interface;

    let _ = fmt; // only PNG today; ICO has its own helper below
    let _ = GUID_ContainerFormatPng;

    // SAFETY: factory + stream + decoder creation; pure COM calls.
    let factory: IWICImagingFactory =
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| format!("CoCreateInstance: {e}"))?;
    let stream: IWICStream =
        unsafe { factory.CreateStream() }.map_err(|e| format!("CreateStream: {e}"))?;
    let buf = bytes.to_vec();
    // SAFETY: InitializeFromMemory borrows our buffer; we keep it alive.
    unsafe { stream.InitializeFromMemory(&buf) }
        .map_err(|e| format!("InitializeFromMemory: {e}"))?;

    // SAFETY: CreateDecoderFromStream auto-detects PNG container.
    let decoder = unsafe {
        factory.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnLoad)
    }
    .map_err(|e| format!("CreateDecoderFromStream: {e}"))?;

    // SAFETY: PNGs always have exactly one frame.
    let frame = unsafe { decoder.GetFrame(0) }.map_err(|e| format!("GetFrame: {e}"))?;
    let frame_src: IWICBitmapSource = frame
        .cast()
        .map_err(|e| format!("frame.cast IBitmapSource: {e}"))?;

    let (mut w, mut h) = (0u32, 0u32);
    // SAFETY: GetSize writes two output u32s.
    unsafe { frame_src.GetSize(&mut w, &mut h) }.map_err(|e| format!("GetSize: {e}"))?;
    if w == 0 || h == 0 {
        return Err("zero-sized PNG frame".into());
    }

    // Convert to 32bppRGBA via IWICFormatConverter (handles paletted /
    // greyscale / 16bit input cleanly).
    let converter: IWICFormatConverter = unsafe { factory.CreateFormatConverter() }
        .map_err(|e| format!("CreateFormatConverter: {e}"))?;
    // SAFETY: Converter init with target format = RGBA8.
    unsafe {
        converter
            .Initialize(
                &frame_src,
                &GUID_WICPixelFormat32bppRGBA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeMedianCut,
            )
            .map_err(|e| format!("Initialize converter: {e}"))?;
    }

    let stride = w * 4;
    let total = (stride as usize).saturating_mul(h as usize);
    let mut pixels = vec![0u8; total];
    // SAFETY: CopyPixels reads `h` rows of `stride` bytes into our buffer.
    unsafe { converter.CopyPixels(std::ptr::null(), stride, &mut pixels) }
        .map_err(|e| format!("CopyPixels: {e}"))?;

    Ok((pixels, w, h))
}

/// Internal: decode an ICO container, pick the largest frame by
/// pixel count, return RGBA8.
fn wic_decode_largest_ico_to_rgba(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat32bppRGBA, IWICBitmapSource,
        IWICFormatConverter, IWICImagingFactory, IWICStream, WICBitmapDitherTypeNone,
        WICBitmapPaletteTypeMedianCut, WICDecodeMetadataCacheOnLoad,
    };
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
    use windows::core::Interface;

    // SAFETY: factory + stream creation.
    let factory: IWICImagingFactory =
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| format!("CoCreateInstance: {e}"))?;
    let stream: IWICStream =
        unsafe { factory.CreateStream() }.map_err(|e| format!("CreateStream: {e}"))?;
    let buf = bytes.to_vec();
    // SAFETY: InitializeFromMemory borrows our buffer.
    unsafe { stream.InitializeFromMemory(&buf) }
        .map_err(|e| format!("InitializeFromMemory: {e}"))?;

    // SAFETY: CreateDecoderFromStream auto-detects ICO container.
    let decoder = unsafe {
        factory.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnLoad)
    }
    .map_err(|e| format!("CreateDecoderFromStream: {e}"))?;

    // SAFETY: GetFrameCount tells us how many PNG/BMP frames the ICO has.
    let count = unsafe { decoder.GetFrameCount() }.map_err(|e| format!("GetFrameCount: {e}"))?;
    if count == 0 {
        return Err("ICO has zero frames".into());
    }

    let mut best_idx = 0u32;
    let mut best_pixels = 0u64;
    for i in 0..count {
        // SAFETY: GetFrame and GetSize on each frame index.
        let frame = unsafe { decoder.GetFrame(i) }.map_err(|e| format!("GetFrame({i}): {e}"))?;
        let src: IWICBitmapSource = frame
            .cast()
            .map_err(|e| format!("cast IBitmapSource: {e}"))?;
        let (mut w, mut h) = (0u32, 0u32);
        unsafe { src.GetSize(&mut w, &mut h) }.map_err(|e| format!("GetSize({i}): {e}"))?;
        let area = (w as u64) * (h as u64);
        if area > best_pixels {
            best_pixels = area;
            best_idx = i;
        }
    }

    // SAFETY: GetFrame on the chosen index.
    let chosen_frame =
        unsafe { decoder.GetFrame(best_idx) }.map_err(|e| format!("GetFrame(best): {e}"))?;
    let chosen_src: IWICBitmapSource = chosen_frame
        .cast()
        .map_err(|e| format!("cast best IBitmapSource: {e}"))?;
    let (mut w, mut h) = (0u32, 0u32);
    // SAFETY: GetSize on the chosen frame.
    unsafe { chosen_src.GetSize(&mut w, &mut h) }.map_err(|e| format!("GetSize(chosen): {e}"))?;
    if w == 0 || h == 0 {
        return Err("chosen ICO frame is zero-sized".into());
    }

    let converter: IWICFormatConverter = unsafe { factory.CreateFormatConverter() }
        .map_err(|e| format!("CreateFormatConverter: {e}"))?;
    // SAFETY: Initialise converter against the chosen frame.
    unsafe {
        converter
            .Initialize(
                &chosen_src,
                &GUID_WICPixelFormat32bppRGBA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeMedianCut,
            )
            .map_err(|e| format!("Initialize converter: {e}"))?;
    }

    let stride = w * 4;
    let total = (stride as usize).saturating_mul(h as usize);
    let mut pixels = vec![0u8; total];
    // SAFETY: CopyPixels into our buffer.
    unsafe { converter.CopyPixels(std::ptr::null(), stride, &mut pixels) }
        .map_err(|e| format!("CopyPixels: {e}"))?;

    Ok((pixels, w, h))
}

/// Strip path separators + truncate to 80 chars. Defence against UI
/// renderers leaking directory traversal from a maliciously-named
/// upload.
fn sanitize_display_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return "custom".to_string();
    }
    trimmed
        .chars()
        .filter(|c| !matches!(*c, '/' | '\\' | '\n' | '\r' | '\t'))
        .take(80)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_stripped_and_truncated() {
        assert_eq!(sanitize_display_name(""), "custom");
        assert_eq!(
            sanitize_display_name("../../../etc/passwd"),
            "......etcpasswd"
        );
        assert_eq!(sanitize_display_name(&"x".repeat(200)).len(), 80);
    }

    fn temp_config(label: &str) -> (IconConfig, PathBuf) {
        let mut d = std::env::temp_dir();
        d.push(format!(
            "bento-icon-custom-{}-{}",
            label,
            super::super::unique_icon_id()
        ));
        std::fs::create_dir_all(&d).expect("test dir");
        let cfg = IconConfig {
            app_data_dir: smol_str::SmolStr::new(d.to_string_lossy()),
        };
        (cfg, d)
    }

    #[test]
    fn upload_rejects_empty_payload() {
        let (cfg, dir) = temp_config("empty");
        let r = upload(&cfg, "svg", Vec::new(), "x");
        assert!(matches!(r, Err(IconError::EmptyUpload)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upload_rejects_unknown_kind() {
        let (cfg, dir) = temp_config("kind");
        let r = upload(&cfg, "tiff", b"x".to_vec(), "x");
        assert!(matches!(r, Err(IconError::UnsupportedKind { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upload_svg_round_trips() {
        let (cfg, dir) = temp_config("svg");
        let svg = b"<svg><circle r='5'/></svg>".to_vec();
        let uuid = upload(&cfg, "svg", svg, "circle").expect("svg");
        let listed = list(&cfg);
        assert!(listed.iter().any(|c| c.uuid == uuid));
        let path = resolve_file(&cfg, &uuid).expect("path");
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_unknown_is_noop() {
        let (cfg, dir) = temp_config("delete_unknown");
        let r = delete(&cfg, "doesnotexist");
        assert!(r.is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_removes_file_and_index_entry() {
        let (cfg, dir) = temp_config("delete");
        let svg = b"<svg><circle r='5'/></svg>".to_vec();
        let uuid = upload(&cfg, "svg", svg, "circle").expect("svg");
        delete(&cfg, &uuid).expect("delete");
        assert!(list(&cfg).iter().all(|c| c.uuid != uuid));
        assert!(resolve_file(&cfg, &uuid).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
