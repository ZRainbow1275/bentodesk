//! Canonical list of legitimate Windows Desktop sources.
//!
//! BentoDesk trusts the following locations as "the user's Desktop":
//!
//! 1. The current user's Desktop (`%USERPROFILE%\Desktop`, via `dirs::desktop_dir`)
//! 2. The public Desktop shared with all users (`C:\Users\Public\Desktop`),
//!    resolved through `SHGetKnownFolderPath(FOLDERID_PublicDesktop)`
//! 3. OneDrive-redirected Desktop, when the user has enabled OneDrive's
//!    "Back up my Desktop" feature (`%OneDrive%\Desktop` or `%OneDriveConsumer%\Desktop`)
//! 4. An explicit override coming from `settings.desktop_path`, for advanced users
//!    that maintain a non-standard Desktop location.
//!
//! These sources are canonicalized and de-duplicated so that downstream consumers
//! can trust the resulting list without worrying about overlap (OneDrive frequently
//! redirects `dirs::desktop_dir()` itself).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Strip the Windows `\\?\` extended-length prefix so comparisons are uniform.
fn strip_unc_prefix(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        p.to_path_buf()
    }
}

/// Canonicalize a path for deduplication purposes.
///
/// If canonicalization fails (e.g. the directory does not exist), fall back to
/// the original path. This keeps the function infallible and ensures we never
/// silently drop candidates that simply can't be resolved on the current box.
fn canonicalize_or_raw(p: &Path) -> PathBuf {
    match p.canonicalize() {
        Ok(c) => strip_unc_prefix(&c),
        Err(_) => p.to_path_buf(),
    }
}

/// Normalize a path to a lowercased string key for case-insensitive comparison
/// and hash-based deduplication.
fn normalize_key(p: &Path) -> String {
    p.to_string_lossy()
        .to_lowercase()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_string()
}

/// Resolve the shared public Desktop (`C:\Users\Public\Desktop`) via the
/// Windows Shell known folder API.
///
/// Falls back to `%PUBLIC%\Desktop` if the Shell call fails (which should be
/// vanishingly rare — the folder ID is constant since Vista).
fn public_desktop_dir() -> Option<PathBuf> {
    use windows::Win32::UI::Shell::{
        FOLDERID_PublicDesktop, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
    };

    // SAFETY: SHGetKnownFolderPath is a documented Shell API. We pass a
    // borrowed FOLDERID constant, KF_FLAG_DEFAULT, and no token (use the
    // current process identity). On success the call writes a PWSTR that
    // owns a buffer allocated with CoTaskMemAlloc — release responsibility
    // is the caller's.
    let result = unsafe {
        SHGetKnownFolderPath(&FOLDERID_PublicDesktop, KF_FLAG_DEFAULT, None)
    };

    match result {
        Ok(pwstr) => {
            // SAFETY: PWSTR is a `Copy` newtype around `*mut u16`, which by
            // Rust's rules means it cannot implement Drop. There is therefore
            // NO automatic release — the explicit CoTaskMemFree below is the
            // only release path, and removing it would leak the Shell buffer
            // on every call. We copy the wide string into an owned Rust
            // String first, then immediately release the COM allocation.
            let owned = unsafe { pwstr.to_string().ok() };
            unsafe {
                windows::Win32::System::Com::CoTaskMemFree(Some(pwstr.0 as *const _));
            }
            owned.map(PathBuf::from).filter(|p| p.exists())
        }
        Err(_) => std::env::var_os("PUBLIC")
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

/// Return all active Desktop directories for the current user, in priority
/// order, after canonicalization and case-insensitive deduplication.
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

    // 1. Current user's Desktop (Shell-resolved, honours OneDrive redirection
    //    automatically when OneDrive owns the current-user Desktop).
    if let Some(p) = dirs::desktop_dir() {
        push(p);
    }

    // 2. Public Desktop (shared shortcuts; requires explicit Shell lookup).
    if let Some(p) = public_desktop_dir() {
        push(p);
    }

    // 3. OneDrive-redirected Desktop (only added if it didn't already appear
    //    as dirs::desktop_dir()).
    if let Some(p) = onedrive_desktop_dir() {
        push(p);
    }

    // 4. User-supplied override from settings.desktop_path.
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

/// True iff `path` is *inside* (or equal to) any Desktop source.
///
/// Unlike [`is_under_any_desktop`], this allows nested subdirectories. The
/// recursive allow-list in `commands::file_ops::validate_allowed_path_inner`
/// inlines this logic to avoid re-canonicalizing the input path; this helper
/// remains as the reusable API for future callers (e.g. grouping scanners).
#[allow(dead_code)]
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
    fn custom_desktop_is_included_when_non_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let custom = tmp.path().to_string_lossy().to_string();
        let sources = all_desktop_dirs(Some(&custom));
        let custom_canon = canonicalize_or_raw(tmp.path());
        assert!(
            sources.iter().any(|p| normalize_key(p) == normalize_key(&custom_canon)),
            "custom desktop path should appear in sources: {:?}",
            sources
        );
    }

    #[test]
    fn empty_custom_desktop_is_ignored() {
        let baseline = all_desktop_dirs(None);
        let with_empty = all_desktop_dirs(Some(""));
        let with_whitespace = all_desktop_dirs(Some("   "));
        assert_eq!(baseline.len(), with_empty.len());
        assert_eq!(baseline.len(), with_whitespace.len());
    }

    #[test]
    fn duplicate_custom_matching_user_desktop_is_deduped() {
        // Passing dirs::desktop_dir() as custom should not produce a duplicate.
        if let Some(user_desktop) = dirs::desktop_dir() {
            let as_custom = user_desktop.to_string_lossy().to_string();
            let baseline = all_desktop_dirs(None);
            let doubled = all_desktop_dirs(Some(&as_custom));
            assert_eq!(
                baseline.len(),
                doubled.len(),
                "passing user desktop as custom should be deduped. baseline={:?} doubled={:?}",
                baseline,
                doubled,
            );
        }
    }

    #[test]
    fn is_under_any_desktop_matches_file_under_custom() {
        let tmp = tempfile::tempdir().unwrap();
        let custom = tmp.path().to_string_lossy().to_string();
        let file = tmp.path().join("example.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(is_under_any_desktop(&file, Some(&custom)));
    }

    #[test]
    fn is_under_any_desktop_rejects_unrelated_path() {
        let tmp = tempfile::tempdir().unwrap();
        let custom = tmp.path().to_string_lossy().to_string();
        let other = tempfile::tempdir().unwrap();
        let file = other.path().join("not-a-desktop-file.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(!is_under_any_desktop(&file, Some(&custom)));
    }

    #[test]
    fn is_inside_any_desktop_allows_nested_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        let custom = tmp.path().to_string_lossy().to_string();
        let nested = tmp.path().join(".bentodesk").join("zone-1");
        std::fs::create_dir_all(&nested).unwrap();
        let file = nested.join("deep.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(is_inside_any_desktop(&file, Some(&custom)));
    }

    #[test]
    fn normalize_key_is_case_insensitive_and_slash_agnostic() {
        let a = PathBuf::from(r"C:\Users\Alice\Desktop");
        let b = PathBuf::from("c:/users/alice/desktop/");
        assert_eq!(normalize_key(&a), normalize_key(&b));
    }
}
