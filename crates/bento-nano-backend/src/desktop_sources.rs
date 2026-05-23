//! T-093 — canonical list of legitimate Windows Desktop sources.
//!
//! Lift-port of `bentodesk/src-tauri/src/desktop_sources.rs`. The 1.x source
//! reached for the `dirs` crate (forbidden — not on §8 whitelist) for both
//! the current-user Desktop and one helper. The nano port resolves both via
//! `SHGetKnownFolderPath` directly, which is what `dirs` itself does on
//! Windows; we just skip the cross-platform indirection.
//!
//! BentoDesk trusts the following locations as "the user's Desktop":
//!
//! 1. The current user's Desktop (`SHGetKnownFolderPath(FOLDERID_Desktop)`),
//!    which honours OneDrive redirection automatically when OneDrive owns
//!    the shell-resolved Desktop.
//! 2. The public Desktop shared with all users
//!    (`SHGetKnownFolderPath(FOLDERID_PublicDesktop)`), typically
//!    `C:\Users\Public\Desktop`.
//! 3. OneDrive-redirected Desktop (`%OneDrive%\Desktop` or
//!    `%OneDriveConsumer%\Desktop`) when "Back up my Desktop" is on but the
//!    shell did not redirect FOLDERID_Desktop itself.
//! 4. An explicit override coming from `settings.desktop_path`, for advanced
//!    users that maintain a non-standard Desktop location.
//!
//! Sources are canonicalised and de-duplicated so that downstream consumers
//! can trust the resulting list without worrying about overlap.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

// ─── Error type (spec §8.1 — hand-rolled, no thiserror) ──────────────

/// Errors surfaced by the desktop-source resolver.
#[derive(Debug)]
pub enum DesktopSourcesError {
    /// `SHGetKnownFolderPath` returned a non-`S_OK` HRESULT for the named
    /// folder identifier.
    Hresult { ctx: &'static str, hr: i32 },
}

impl core::fmt::Display for DesktopSourcesError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Hresult { ctx, hr } => {
                write!(f, "{ctx}: SHGetKnownFolderPath failed (hr={hr:#x})")
            }
        }
    }
}

impl core::error::Error for DesktopSourcesError {}

// ─── Path helpers ────────────────────────────────────────────────────

/// Strip the Windows `\\?\` extended-length prefix so comparisons are uniform.
fn strip_unc_prefix(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        p.to_path_buf()
    }
}

/// Canonicalise a path for deduplication. If canonicalisation fails (e.g. the
/// directory does not exist), fall back to the original path so we never
/// silently drop candidates that simply can't be resolved on the current box.
fn canonicalize_or_raw(p: &Path) -> PathBuf {
    match p.canonicalize() {
        Ok(c) => strip_unc_prefix(&c),
        Err(_) => p.to_path_buf(),
    }
}

/// Normalise a path to a lowercased string key for case-insensitive comparison
/// and hash-based deduplication.
fn normalize_key(p: &Path) -> String {
    p.to_string_lossy()
        .to_lowercase()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_string()
}

// ─── SHGetKnownFolderPath wrapper ────────────────────────────────────

/// Resolve a `FOLDERID_*` GUID to its filesystem path via the Shell.
///
/// Returns `Ok(Some(path))` when the folder exists on disk, `Ok(None)` when
/// the call succeeded but the resolved path does not exist (uninstalled
/// OneDrive, deleted folder, etc.), and `Err` only on hard HRESULT failures.
fn known_folder_path(
    folder_id: &windows_sys::core::GUID,
    ctx: &'static str,
) -> Result<Option<PathBuf>, DesktopSourcesError> {
    use windows_sys::Win32::Foundation::S_OK;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;

    let mut raw: *mut u16 = core::ptr::null_mut();
    // SAFETY: SHGetKnownFolderPath canonical signature; `raw` is written on
    // S_OK and freed below. KF_FLAG_DEFAULT = 0; passing null token uses the
    // current process identity per MSDN.
    let hr =
        unsafe { SHGetKnownFolderPath(folder_id as *const _, 0, core::ptr::null_mut(), &mut raw) };
    if hr != S_OK {
        if !raw.is_null() {
            // SAFETY: the docs allow a non-null buffer alongside a non-S_OK
            // HRESULT in some failure modes; release if present.
            unsafe { CoTaskMemFree(raw as *const _) };
        }
        return Err(DesktopSourcesError::Hresult { ctx, hr });
    }

    // SAFETY: `raw` is non-null on S_OK; walk to NUL to determine length.
    let len = unsafe {
        let mut p = raw;
        let mut n = 0usize;
        while *p != 0 {
            n += 1;
            p = p.add(1);
        }
        n
    };
    // SAFETY: raw valid for `len` u16 elements per the loop above.
    let slice: &[u16] = unsafe { core::slice::from_raw_parts(raw, len) };
    let s = String::from_utf16_lossy(slice);
    // SAFETY: free what SHGetKnownFolderPath allocated. Required by docs.
    unsafe { CoTaskMemFree(raw as *const _) };

    let path = PathBuf::from(s);
    if path.exists() {
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

/// Resolve the current user's Desktop folder via the Shell. Honours OneDrive
/// redirection automatically when the shell folder ID points there.
///
/// Public so sibling modules (e.g. `system::get_system_info`, classifiers)
/// can reuse the `SHGetKnownFolderPath(FOLDERID_Desktop)` boilerplate.
pub fn user_desktop_dir() -> Option<PathBuf> {
    use windows_sys::Win32::UI::Shell::FOLDERID_Desktop;
    known_folder_path(&FOLDERID_Desktop, "user_desktop_dir")
        .ok()
        .flatten()
}

/// Resolve the shared public Desktop (`C:\Users\Public\Desktop`).
///
/// Falls back to `%PUBLIC%\Desktop` if the Shell call fails (vanishingly
/// rare — the folder ID is constant since Vista).
fn public_desktop_dir() -> Option<PathBuf> {
    use windows_sys::Win32::UI::Shell::FOLDERID_PublicDesktop;
    match known_folder_path(&FOLDERID_PublicDesktop, "public_desktop_dir") {
        Ok(Some(p)) => Some(p),
        _ => std::env::var_os("PUBLIC")
            .map(|p| PathBuf::from(p).join("Desktop"))
            .filter(|p| p.exists()),
    }
}

/// Detect a OneDrive-redirected Desktop if OneDrive "Back up Desktop" is on.
///
/// Prefers the business-tenant variable `OneDrive`, falling back to the
/// consumer variable `OneDriveConsumer`. Returns `None` unless the candidate
/// directory actually exists on disk.
fn onedrive_desktop_dir() -> Option<PathBuf> {
    for var in &["OneDrive", "OneDriveConsumer"] {
        if let Some(root) = std::env::var_os(var) {
            let candidate = PathBuf::from(root).join("Desktop");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

// ─── Public surface ──────────────────────────────────────────────────

/// Return all active Desktop directories for the current user, in priority
/// order, after canonicalisation and case-insensitive deduplication.
///
/// The `custom` argument is typically `Some(settings.desktop_path.as_str())`.
/// An empty string is treated the same as `None`.
pub fn all_desktop_dirs(custom: Option<&str>) -> Vec<PathBuf> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<PathBuf> = Vec::new();

    let mut push = |candidate: PathBuf| {
        let canonical = canonicalize_or_raw(&candidate);
        let key = normalize_key(&canonical);
        if !key.is_empty() && seen.insert(key) {
            out.push(canonical);
        }
    };

    if let Some(p) = user_desktop_dir() {
        push(p);
    }
    if let Some(p) = public_desktop_dir() {
        push(p);
    }
    if let Some(p) = onedrive_desktop_dir() {
        push(p);
    }
    if let Some(custom) = custom {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            push(PathBuf::from(trimmed));
        }
    }

    out
}

/// True iff the parent directory of `path` matches (case-insensitively) any
/// of the legitimate Desktop sources. Intended for drag-and-drop validation.
pub fn is_under_any_desktop(path: &Path, custom: Option<&str>) -> bool {
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => path.to_path_buf(),
    };
    let parent_canon = canonicalize_or_raw(&parent);
    let parent_key = normalize_key(&parent_canon);
    if parent_key.is_empty() {
        return false;
    }

    for source in all_desktop_dirs(custom) {
        let source_key = normalize_key(&source);
        if parent_key == source_key {
            return true;
        }
    }
    false
}

/// True iff `path` is *inside* (or equal to) any Desktop source. Allows
/// nested subdirectories. Used by grouping scanners that follow `.bentodesk/`
/// hidden subdirectories under a managed Desktop.
pub fn is_inside_any_desktop(path: &Path, custom: Option<&str>) -> bool {
    let canon = canonicalize_or_raw(path);
    let canon_key = normalize_key(&canon);
    if canon_key.is_empty() {
        return false;
    }

    for source in all_desktop_dirs(custom) {
        let source_key = normalize_key(&source);
        if source_key.is_empty() {
            continue;
        }
        if canon_key == source_key || canon_key.starts_with(&format!("{source_key}\\")) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn empty_custom_desktop_is_ignored() {
        let baseline = all_desktop_dirs(None);
        let with_empty = all_desktop_dirs(Some(""));
        let with_whitespace = all_desktop_dirs(Some("   "));
        assert_eq!(baseline.len(), with_empty.len());
        assert_eq!(baseline.len(), with_whitespace.len());
    }

    #[test]
    fn normalize_key_is_case_insensitive_and_slash_agnostic() {
        let a = PathBuf::from(r"C:\Users\Alice\Desktop");
        let b = PathBuf::from("c:/users/alice/desktop/");
        assert_eq!(normalize_key(&a), normalize_key(&b));
    }

    #[test]
    fn nonexistent_path_is_not_under_any_desktop() {
        let p = PathBuf::from(r"Z:\does-not-exist\file.txt");
        assert!(!is_under_any_desktop(&p, None));
    }

    #[test]
    fn empty_path_yields_empty_normalize_key() {
        let p = PathBuf::from("");
        assert_eq!(normalize_key(&p), "");
    }

    #[test]
    fn strip_unc_prefix_removes_extended_length() {
        let raw = PathBuf::from(r"\\?\C:\Users\Alice\Desktop");
        let stripped = strip_unc_prefix(&raw);
        assert_eq!(stripped.to_string_lossy(), "C:\\Users\\Alice\\Desktop");
    }

    #[test]
    fn strip_unc_prefix_passes_through_normal_path() {
        let raw = PathBuf::from(r"C:\Users\Alice\Desktop");
        let stripped = strip_unc_prefix(&raw);
        assert_eq!(stripped, raw);
    }
}
