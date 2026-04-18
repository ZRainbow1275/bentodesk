//! Warm tier (on-disk) for the icon cache.
//!
//! Layout: `<warm_dir>/<hash[0..2]>/<hash>.png`. The two-char prefix
//! sharding keeps any single directory under ~4k entries even for
//! very large desktop inventories.
//!
//! Writes are fire-and-forget from the hot-tier put path; reads are
//! synchronous because they happen on the icon protocol handler's
//! thread when the hot tier misses. File I/O is cheap relative to
//! `ExtractIconExW`/`SHGetFileInfoW` (~100-500us vs tens of ms) so
//! the warm-tier read stays off the critical path in practice.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Warm-tier directory handle. Cheap to construct; clones share the
/// same backing path. Directory creation is lazy — warm reads tolerate
/// a missing directory and return `None`.
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
    /// Returns `None` if `hash` is empty or contains any character that
    /// would escape the sharded directory (defence-in-depth — the
    /// hasher in `extractor::compute_icon_hash` only produces hex so
    /// this should never trigger in practice).
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
    use tempfile::TempDir;

    #[test]
    fn path_rejects_empty_and_too_short_hash() {
        let dir = TempDir::new().unwrap();
        let tier = WarmTier::new(dir.path().to_path_buf());
        assert!(tier.path_for("").is_none());
        assert!(tier.path_for("a").is_none());
    }

    #[test]
    fn path_rejects_non_alphanumeric() {
        let dir = TempDir::new().unwrap();
        let tier = WarmTier::new(dir.path().to_path_buf());
        assert!(tier.path_for("../evil").is_none());
    }

    #[test]
    fn path_shards_by_prefix() {
        let dir = TempDir::new().unwrap();
        let tier = WarmTier::new(dir.path().to_path_buf());
        let p = tier.path_for("abcdef123456").unwrap();
        assert!(p.to_string_lossy().contains("ab"));
        assert!(p.to_string_lossy().ends_with("abcdef123456.png"));
    }

    #[test]
    fn write_then_read_roundtrip() {
        let dir = TempDir::new().unwrap();
        let tier = WarmTier::new(dir.path().to_path_buf());
        let payload: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 1, 2, 3];
        tier.write("abcdef1234567890", &payload).unwrap();
        assert!(tier.contains("abcdef1234567890"));
        let got = tier.read("abcdef1234567890").unwrap();
        assert_eq!(got, payload);
    }

    #[test]
    fn read_missing_returns_none_without_error() {
        let dir = TempDir::new().unwrap();
        let tier = WarmTier::new(dir.path().to_path_buf());
        assert!(tier.read("doesnotexist1234").is_none());
    }

    #[test]
    fn remove_is_silent_on_missing() {
        let dir = TempDir::new().unwrap();
        let tier = WarmTier::new(dir.path().to_path_buf());
        tier.remove("neverwritten0001"); // should not panic
    }

    #[test]
    fn clear_wipes_directory() {
        let dir = TempDir::new().unwrap();
        let tier = WarmTier::new(dir.path().to_path_buf());
        tier.write("ab12345678901234", b"hello").unwrap();
        tier.clear().unwrap();
        assert!(!tier.contains("ab12345678901234"));
    }
}
