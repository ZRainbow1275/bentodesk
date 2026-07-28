//! T-081 — warm tier (on-disk) for the icon cache.
//!
//! Direct port of `bentodesk/src-tauri/src/icon/cache_tier.rs`. Unchanged
//! semantics: `<warm_dir>/<hash[0..2]>/<hash>.png` two-char shard, atomic
//! `<file>.tmp` → rename writes, defensive char filter on hash inputs.
//!
//! Tauri-bridge: none. The 1.x version had no `AppHandle` dep here; the
//! port is line-for-line equivalent except for the spec §11 lift away
//! from `.expect("warm-tier write must succeed")`-style panics — every
//! fallible op now returns `std::io::Result` or `Option`, and the public
//! surface logs + degrades on failure rather than panicking.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Warm-tier directory handle. Cheap to construct; clones share the same
/// backing path. Directory creation is lazy — warm reads tolerate a
/// missing directory and return `None`.
#[derive(Debug, Clone)]
pub struct WarmTier {
    root: PathBuf,
}

impl WarmTier {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve the on-disk path for `hash` within the warm tier.
    ///
    /// Returns `None` if `hash` is empty, shorter than 2 chars, or
    /// contains any non-alphanumeric character. The hex-only hash
    /// produced by `extractor::compute_icon_hash` always satisfies this;
    /// the filter exists as defence-in-depth against future callers that
    /// might pass attacker-controlled strings.
    pub fn path_for(&self, hash: &str) -> Option<PathBuf> {
        if hash.is_empty() || hash.len() < 2 {
            return None;
        }
        if !hash.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        let shard = &hash[0..2];
        Some(self.root.join(shard).join(format!("{hash}.png")))
    }

    /// Read `hash` from the warm tier. Returns `None` on any failure
    /// (file missing, permission denied, etc.) — the caller is expected
    /// to fall through to the cold-extract path.
    pub fn read(&self, hash: &str) -> Option<Vec<u8>> {
        let path = self.path_for(hash)?;
        match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                tracing::debug!("warm-tier read failed for {}: {}", path.display(), e);
                None
            }
        }
    }

    /// Write `bytes` to the warm tier under `hash`.
    ///
    /// Uses an atomic `<file>.tmp` → rename so a crash mid-write can't
    /// leave a partial PNG on disk. Errors are returned — the caller
    /// typically logs + increments a counter instead of propagating.
    pub fn write(&self, hash: &str, bytes: &[u8]) -> std::io::Result<()> {
        let target = self
            .path_for(hash)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid hash"))?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = target.with_extension("png.tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_data()?;
        }
        std::fs::rename(&tmp, &target)?;
        Ok(())
    }

    /// True if the warm tier has a copy of `hash`.
    pub fn contains(&self, hash: &str) -> bool {
        self.path_for(hash).map(|p| p.exists()).unwrap_or(false)
    }

    /// Delete `hash` from the warm tier. Silent on not-found.
    pub fn remove(&self, hash: &str) {
        if let Some(p) = self.path_for(hash) {
            let _ = std::fs::remove_file(&p);
        }
    }

    /// Wipe every warm-tier entry. Used by `clear_icon_cache` so a
    /// user-initiated "reset icon cache" action actually reclaims disk.
    pub fn clear(&self) -> std::io::Result<()> {
        if !self.root.exists() {
            return Ok(());
        }
        std::fs::remove_dir_all(&self.root)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!(
            "bento-icon-warm-{}-{}",
            label,
            super::super::unique_icon_id()
        ));
        std::fs::create_dir_all(&d).expect("test dir");
        d
    }

    #[test]
    fn path_rejects_empty_and_too_short_hash() {
        let dir = temp_dir("rej_empty");
        let tier = WarmTier::new(dir.clone());
        assert!(tier.path_for("").is_none());
        assert!(tier.path_for("a").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_rejects_non_alphanumeric() {
        let dir = temp_dir("rej_alnum");
        let tier = WarmTier::new(dir.clone());
        assert!(tier.path_for("../evil").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_shards_by_prefix() {
        let dir = temp_dir("shard");
        let tier = WarmTier::new(dir.clone());
        let p = tier.path_for("abcdef123456").expect("path");
        assert!(p.to_string_lossy().contains("ab"));
        assert!(p.to_string_lossy().ends_with("abcdef123456.png"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_then_read_roundtrip() {
        let dir = temp_dir("rt");
        let tier = WarmTier::new(dir.clone());
        let payload: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 1, 2, 3];
        tier.write("abcdef1234567890", &payload).expect("write");
        assert!(tier.contains("abcdef1234567890"));
        let got = tier.read("abcdef1234567890").expect("read");
        assert_eq!(got, payload);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_missing_returns_none_without_error() {
        let dir = temp_dir("missing");
        let tier = WarmTier::new(dir.clone());
        assert!(tier.read("doesnotexist1234").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_is_silent_on_missing() {
        let dir = temp_dir("rm_silent");
        let tier = WarmTier::new(dir.clone());
        tier.remove("neverwritten0001");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_wipes_directory() {
        let dir = temp_dir("clear");
        let tier = WarmTier::new(dir.clone());
        tier.write("ab12345678901234", b"hello").expect("write");
        tier.clear().expect("clear");
        assert!(!tier.contains("ab12345678901234"));
    }
}
