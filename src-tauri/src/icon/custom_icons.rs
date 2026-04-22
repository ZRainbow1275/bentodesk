//! Custom user-uploaded icon storage.
//!
//! Icons are stored under `%APPDATA%/BentoDesk/custom_icons/{uuid}.{ext}`.
//! A `metadata.json` sidecar in the same directory tracks display names and
//! kinds. ICO files are converted to PNG at upload time so the frontend can
//! render them with a single `<img>` tag.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::BentoDeskError;

use super::svg_sanitize::sanitize_svg;

/// Per-icon metadata persisted alongside the binary file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomIcon {
    pub uuid: String,
    pub name: String,
    /// Storage kind after conversion: "svg", "png".
    pub kind: String,
    /// Resolved `bentodesk://custom-icon/{uuid}` URL.
    pub url: String,
    pub created_at: String,
}

/// Container for the metadata file on disk.
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

static LOCK: Mutex<()> = Mutex::new(());

/// Resolve `%APPDATA%/BentoDesk/custom_icons/`, creating it on demand.
pub fn custom_icons_dir(handle: &AppHandle) -> PathBuf {
    let base = crate::storage::state_data_dir(handle);
    let dir = base.join("custom_icons");
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

fn save_index(dir: &Path, idx: &CustomIconIndex) -> Result<(), BentoDeskError> {
    let path = metadata_path(dir);
    let json = serde_json::to_string_pretty(idx)?;
    fs::write(&path, json)?;
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

/// Resolve the on-disk path for a stored custom icon. Returns `None` if missing.
pub fn resolve_file(handle: &AppHandle, uuid: &str) -> Option<PathBuf> {
    let dir = custom_icons_dir(handle);
    let idx = load_index(&dir);
    let meta = idx.icons.iter().find(|m| m.uuid == uuid)?;
    let ext = if meta.kind == "svg" { "svg" } else { "png" };
    Some(dir.join(format!("{uuid}.{ext}")))
}

/// Read the raw bytes + content-type for a stored custom icon.
/// Returns `None` when the UUID is unknown or the file is missing.
pub fn read_bytes(handle: &AppHandle, uuid: &str) -> Option<(Vec<u8>, &'static str)> {
    let dir = custom_icons_dir(handle);
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
/// * SVG → sanitised and stored as-is (extension `.svg`).
/// * PNG → validated (re-decoded) and stored as-is (extension `.png`).
/// * ICO → decoded, the best frame is re-encoded as PNG, stored under `.png`,
///   and the stored kind switches to `"png"`.
pub fn upload(
    handle: &AppHandle,
    kind: &str,
    bytes: Vec<u8>,
    display_name: &str,
) -> Result<String, BentoDeskError> {
    let _guard = LOCK.lock().ok();

    if bytes.is_empty() {
        return Err(BentoDeskError::Generic("Empty upload payload".into()));
    }

    let dir = custom_icons_dir(handle);
    let uuid = uuid::Uuid::new_v4().to_string();
    let kind_norm = kind.to_ascii_lowercase();

    let (stored_kind, file_name) = match kind_norm.as_str() {
        "svg" => {
            let text = String::from_utf8(bytes)
                .map_err(|e| BentoDeskError::Generic(format!("Invalid UTF-8 in SVG: {e}")))?;
            let clean = sanitize_svg(&text).map_err(BentoDeskError::Generic)?;
            let path = dir.join(format!("{uuid}.svg"));
            fs::write(&path, clean.as_bytes())?;
            ("svg", format!("{uuid}.svg"))
        }
        "png" => {
            // Validate by round-tripping through the decoder.
            let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
                .map_err(|e| BentoDeskError::ImageError(e.to_string()))?;
            let mut out = Vec::new();
            img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
                .map_err(|e| BentoDeskError::ImageError(e.to_string()))?;
            let path = dir.join(format!("{uuid}.png"));
            fs::write(&path, &out)?;
            ("png", format!("{uuid}.png"))
        }
        "ico" => {
            let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Ico)
                .map_err(|e| BentoDeskError::ImageError(e.to_string()))?;
            let mut out = Vec::new();
            img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
                .map_err(|e| BentoDeskError::ImageError(e.to_string()))?;
            let path = dir.join(format!("{uuid}.png"));
            fs::write(&path, &out)?;
            ("png", format!("{uuid}.png"))
        }
        other => {
            return Err(BentoDeskError::Generic(format!(
                "Unsupported icon kind: {other}"
            )))
        }
    };

    let mut idx = load_index(&dir);
    idx.icons.push(CustomIconMeta {
        uuid: uuid.clone(),
        name: sanitize_display_name(display_name),
        kind: stored_kind.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    });
    save_index(&dir, &idx)?;
    tracing::info!("Uploaded custom icon {} as {}", display_name, file_name);
    Ok(uuid)
}

/// List all known custom icons with their resolved URLs.
pub fn list(handle: &AppHandle) -> Vec<CustomIcon> {
    let dir = custom_icons_dir(handle);
    let idx = load_index(&dir);
    idx.icons.iter().map(to_custom_icon).collect()
}

/// Delete a custom icon by UUID. No-op when the UUID is unknown.
pub fn delete(handle: &AppHandle, uuid: &str) -> Result<(), BentoDeskError> {
    let _guard = LOCK.lock().ok();
    let dir = custom_icons_dir(handle);
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

fn sanitize_display_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return "custom".to_string();
    }
    // Strip path separators so the name can never leak directory traversal
    // if it is later rendered in UI.
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
}
