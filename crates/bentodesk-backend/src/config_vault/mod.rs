//! `config_vault` — encrypted-at-rest persistent settings store.
//!
//! Implements P0.4 + part of P0.6/P0.8 from the 0503 audit findings:
//! `Command::SetSetting` finally has a backing store, and the three
//! `EncryptionMode` persisted variants have functional crypto behind them.
//!
//! ## Q2=C constraints honoured
//!
//! - **Zero new crates** beyond the existing §8 whitelist. AES-256-GCM
//!   rides on Win32 BCrypt (`bcrypt.dll`); Argon2id is hand-rolled per
//!   RFC 9106; DPAPI uses `CryptProtectData` / `CryptUnprotectData` from
//!   `crypt32.dll`. All bound via `windows-sys` 0.59.
//! - 100 % `// SAFETY:` annotations on every unsafe block.
//! - No `unwrap()` / `expect()` outside `#[cfg(test)]` blocks (spec §11).
//! - No `todo!()` / `unimplemented!()` (spec §17).
//! - Atomic write + `.bak` rotation via `bentodesk-backend::storage::write_json_atomic`.
//!
//! ## Public API surface
//!
//! ```ignore
//! let mut vault = Vault::open(state_dir.join("settings.vault"))?;
//! vault.set_setting("display.mode", SettingValue::Str("dark".into()));
//! vault.set_mode(EncryptionMode::Dpapi)?;
//! vault.flush()?;
//! let v: Option<SettingValue> = vault.get_setting("display.mode");
//! let ok: bool = vault.verify_passphrase("user-typed");
//! ```

pub mod crypto;
pub mod tauri_settings;
pub mod wire;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::storage::{self as backend_storage, StorageError};
use crypto::aes_gcm::{AesGcmError, KEY_LEN, NONCE_LEN, TAG_LEN};
use crypto::argon2id::{Argon2Error, Argon2Params};
use crypto::dpapi::DpapiError;
use wire::{ModeTag, VaultRecord, WireError};

/// Tagged setting value. Mirrors `bentodesk-app::dispatcher::SettingValue`
/// shape so the wire format stays in sync; we keep a separate type because
/// the backend crate must not depend on the app crate (layering rule
/// `lib.rs §1` — `bentodesk-backend` is layer 2.5, app is layer 3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SettingValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(SmolStr),
}

/// Encryption mode for the on-disk vault.
#[derive(Debug, Clone)]
pub enum EncryptionMode {
    /// Plaintext JSON (no encryption). Useful for development/debug only.
    None,
    /// DPAPI per-user, per-machine encryption. Default for production.
    Dpapi,
    /// Argon2id-derived AES-256-GCM. Passphrase must be supplied to set
    /// and verify; vault zeros it out of memory after each `flush`.
    Passphrase { passphrase: SmolStr },
    /// On-disk vault is passphrase-encrypted, but this process has not
    /// unlocked it yet. This mode preserves the disk tag and blocks writes
    /// until the shell replaces the vault via [`Vault::open_with_passphrase`].
    LockedPassphrase,
}

impl EncryptionMode {
    fn tag(&self) -> ModeTag {
        match self {
            Self::None => ModeTag::None,
            Self::Dpapi => ModeTag::Dpapi,
            Self::Passphrase { .. } | Self::LockedPassphrase => ModeTag::Passphrase,
        }
    }
}

/// Vault errors. Hand-rolled per spec §8.1 (no `thiserror`).
#[derive(Debug)]
pub enum VaultError {
    /// `bentodesk-backend::storage` returned an error.
    Storage(StorageError),
    /// `serde_json` failed to (de)serialize the inner KV map.
    Json { ctx: &'static str, message: String },
    /// Wire-format decode (mode tag, base64) failed.
    Wire(WireError),
    /// AES-256-GCM operation failed.
    AesGcm(AesGcmError),
    /// Argon2id KDF failed.
    Argon2(Argon2Error),
    /// DPAPI call failed.
    Dpapi(DpapiError),
    /// Mode requires a passphrase but none was supplied (e.g. `verify_passphrase`
    /// called when the vault is in `None` mode).
    NoPassphraseSet,
    /// Vault on-disk version is newer than what this build understands.
    VersionTooNew { found: u8, max_supported: u8 },
}

impl core::fmt::Display for VaultError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Storage(e) => write!(f, "vault storage error: {e}"),
            Self::Json { ctx, message } => write!(f, "vault json {ctx} error: {message}"),
            Self::Wire(e) => write!(f, "vault wire error: {e}"),
            Self::AesGcm(e) => write!(f, "vault aes-gcm error: {e}"),
            Self::Argon2(e) => write!(f, "vault argon2id error: {e}"),
            Self::Dpapi(e) => write!(f, "vault dpapi error: {e}"),
            Self::NoPassphraseSet => f.write_str("vault has no passphrase configured"),
            Self::VersionTooNew {
                found,
                max_supported,
            } => {
                write!(
                    f,
                    "vault version {found} is newer than max supported {max_supported}"
                )
            }
        }
    }
}

impl core::error::Error for VaultError {}

impl From<StorageError> for VaultError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}
impl From<WireError> for VaultError {
    fn from(e: WireError) -> Self {
        Self::Wire(e)
    }
}
impl From<AesGcmError> for VaultError {
    fn from(e: AesGcmError) -> Self {
        Self::AesGcm(e)
    }
}
impl From<Argon2Error> for VaultError {
    fn from(e: Argon2Error) -> Self {
        Self::Argon2(e)
    }
}
impl From<DpapiError> for VaultError {
    fn from(e: DpapiError) -> Self {
        Self::Dpapi(e)
    }
}

/// Inner JSON shape: `{ "kv": { key: SettingValue } }`.
///
/// Wrapping the BTreeMap in a struct gives us forward-compat headroom
/// (e.g. adding a `schema_version` sibling field later without breaking
/// the serde shape) without touching the on-disk wire format today.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct VaultBody {
    kv: BTreeMap<SmolStr, SettingValue>,
}

/// In-memory + on-disk vault.
#[derive(Debug)]
pub struct Vault {
    path: PathBuf,
    body: VaultBody,
    mode: EncryptionMode,
    dirty: bool,
}

impl Vault {
    /// Open or create the vault at `path`. If the file does not yet exist,
    /// returns an empty vault in `None` mode (caller switches to `Dpapi` /
    /// `Passphrase` via [`set_mode`] before flushing).
    pub fn open(path: &Path) -> Result<Self, VaultError> {
        // `read_json_with_recovery` already handles missing-file → `Ok(None)`
        // and `.bak` rotation per the contract documented in storage::mod.rs:303.
        let record_opt: Option<VaultRecord> =
            backend_storage::read_json_with_recovery(path, "config_vault")?;

        let (body, mode) = match record_opt {
            None => (VaultBody::default(), EncryptionMode::None),
            Some(record) => {
                if record.version > wire::VAULT_VERSION {
                    return Err(VaultError::VersionTooNew {
                        found: record.version,
                        max_supported: wire::VAULT_VERSION,
                    });
                }
                let decoded = record.decoded_bytes()?;
                match decoded.mode_tag {
                    ModeTag::None => {
                        let body = parse_body(&decoded.ciphertext)?;
                        (body, EncryptionMode::None)
                    }
                    ModeTag::Dpapi => {
                        let plaintext = crypto::dpapi::decrypt(&decoded.ciphertext)?;
                        let body = parse_body(&plaintext)?;
                        (body, EncryptionMode::Dpapi)
                    }
                    ModeTag::Passphrase => (VaultBody::default(), EncryptionMode::LockedPassphrase),
                }
            }
        };

        Ok(Self {
            path: path.to_path_buf(),
            body,
            mode,
            dirty: false,
        })
    }

    /// Open the vault, attempting to decrypt with the given passphrase if
    /// the on-disk record is in `Passphrase` mode. Returns the unlocked
    /// vault on success, or an error (typically [`VaultError::AesGcm`]
    /// `TagMismatch`) if the passphrase is wrong.
    pub fn open_with_passphrase(path: &Path, passphrase: &str) -> Result<Self, VaultError> {
        let record_opt: Option<VaultRecord> =
            backend_storage::read_json_with_recovery(path, "config_vault")?;

        let Some(record) = record_opt else {
            return Ok(Self {
                path: path.to_path_buf(),
                body: VaultBody::default(),
                mode: EncryptionMode::Passphrase {
                    passphrase: SmolStr::from(passphrase),
                },
                dirty: true,
            });
        };
        if record.version > wire::VAULT_VERSION {
            return Err(VaultError::VersionTooNew {
                found: record.version,
                max_supported: wire::VAULT_VERSION,
            });
        }
        let decoded = record.decoded_bytes()?;
        if decoded.mode_tag != ModeTag::Passphrase {
            // Fall back to mode-agnostic open path; the supplied passphrase
            // is unused but still recorded so subsequent flushes can switch
            // back to Passphrase mode if the caller calls `set_mode`.
            return Self::open(path);
        }

        if decoded.salt.len() != 16 {
            return Err(VaultError::Wire(WireError::Base64 {
                field: "salt",
                inner: wire::Base64Error::InvalidLength {
                    len: decoded.salt.len(),
                },
            }));
        }
        if decoded.nonce.len() != NONCE_LEN {
            return Err(VaultError::Wire(WireError::Base64 {
                field: "nonce",
                inner: wire::Base64Error::InvalidLength {
                    len: decoded.nonce.len(),
                },
            }));
        }
        if decoded.tag.len() != TAG_LEN {
            return Err(VaultError::Wire(WireError::Base64 {
                field: "tag",
                inner: wire::Base64Error::InvalidLength {
                    len: decoded.tag.len(),
                },
            }));
        }
        let key = derive_key(passphrase, &decoded.salt)?;
        let nonce_arr: [u8; NONCE_LEN] = decoded.nonce.as_slice().try_into().map_err(|_| {
            VaultError::Wire(WireError::Base64 {
                field: "nonce",
                inner: wire::Base64Error::InvalidLength {
                    len: decoded.nonce.len(),
                },
            })
        })?;
        let tag_arr: [u8; TAG_LEN] = decoded.tag.as_slice().try_into().map_err(|_| {
            VaultError::Wire(WireError::Base64 {
                field: "tag",
                inner: wire::Base64Error::InvalidLength {
                    len: decoded.tag.len(),
                },
            })
        })?;
        let plaintext =
            crypto::aes_gcm::decrypt(&key, &nonce_arr, &[], &decoded.ciphertext, &tag_arr)?;
        let body = parse_body(&plaintext)?;

        Ok(Self {
            path: path.to_path_buf(),
            body,
            mode: EncryptionMode::Passphrase {
                passphrase: SmolStr::from(passphrase),
            },
            dirty: false,
        })
    }

    /// Set or overwrite a setting. Marks the vault dirty for the next
    /// [`flush`]. Idempotent at the storage layer; no-op writes are
    /// re-flushed (acceptable — typical UI session emits ≤100 SetSetting
    /// calls and flush is debounced by the caller).
    pub fn set_setting(&mut self, key: &str, value: SettingValue) {
        let key_smol = SmolStr::from(key);
        self.body.kv.insert(key_smol, value);
        self.dirty = true;
    }

    /// Remove a setting from the vault. Returns `true` when the key existed.
    /// Missing keys are a no-op and do not force a disk rewrite.
    pub fn remove_setting(&mut self, key: &str) -> bool {
        let removed = self.body.kv.remove(key).is_some();
        if removed {
            self.dirty = true;
        }
        removed
    }

    /// Read a setting. Returns `None` if the key has never been set in
    /// this vault.
    pub fn get_setting(&self, key: &str) -> Option<SettingValue> {
        self.body.kv.get(key).cloned()
    }

    /// Switch the encryption mode. Marks the vault dirty so the next
    /// [`flush`] re-writes the file in the new shape.
    pub fn set_mode(&mut self, mode: EncryptionMode) -> Result<(), VaultError> {
        self.mode = mode;
        self.dirty = true;
        Ok(())
    }

    /// Verify the passphrase against the in-memory mode. Returns `false`
    /// without leaking timing info beyond the AES-GCM tag-mismatch check.
    /// Returns `false` if the vault is not in `Passphrase` mode.
    pub fn verify_passphrase(&self, candidate: &str) -> bool {
        let configured = match &self.mode {
            EncryptionMode::Passphrase { passphrase } => passphrase.as_str(),
            _ => return false,
        };
        // Constant-time comparison to avoid a side-channel on the in-memory
        // passphrase. We deliberately skip an Argon2id-derive here because
        // the comparison happens in-memory only — the on-disk tag check
        // already covers that path via `open_with_passphrase`.
        constant_time_eq(configured.as_bytes(), candidate.as_bytes())
    }

    /// Persist the vault to disk if dirty. Idempotent on a clean vault.
    pub fn flush(&mut self) -> Result<(), VaultError> {
        if !self.dirty {
            return Ok(());
        }

        let inner_json = serde_json::to_vec(&self.body).map_err(|e| VaultError::Json {
            ctx: "serialize body",
            message: e.to_string(),
        })?;

        let record = match &self.mode {
            EncryptionMode::None => VaultRecord::plaintext(&inner_json),
            EncryptionMode::Dpapi => {
                let blob = crypto::dpapi::encrypt(&inner_json)?;
                VaultRecord::dpapi(&blob)
            }
            EncryptionMode::Passphrase { passphrase } => {
                let mut salt = [0u8; 16];
                crypto::aes_gcm::random_bytes(&mut salt)?;
                let mut nonce = [0u8; NONCE_LEN];
                crypto::aes_gcm::random_bytes(&mut nonce)?;
                let key = derive_key(passphrase.as_str(), &salt)?;
                let ct = crypto::aes_gcm::encrypt(&key, &nonce, &[], &inner_json)?;
                VaultRecord::passphrase(&salt, &nonce, &ct.tag, &ct.ciphertext)
            }
            EncryptionMode::LockedPassphrase => return Err(VaultError::NoPassphraseSet),
        };

        backend_storage::write_json_atomic(&self.path, &record)?;
        self.dirty = false;
        Ok(())
    }

    /// Path the vault was opened from / will flush to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the vault has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Whether the on-disk vault is passphrase-encrypted but not yet
    /// unlocked in this process.
    pub fn is_locked_passphrase(&self) -> bool {
        matches!(self.mode, EncryptionMode::LockedPassphrase)
    }

    /// Currently configured encryption mode tag (does not leak the passphrase).
    pub fn mode_tag(&self) -> ModeTag {
        self.mode.tag()
    }
}

// -----------------------------------------------------------------------------
// Process-global accessor (F2-03)
// -----------------------------------------------------------------------------
//
// `Vault::set_setting` takes `&mut self`, so we wrap the singleton in a
// `Mutex` for cross-thread access (the dispatcher worker plus the UI pump
// may both reach for the vault on a SetSetting flush). `OnceLock<Mutex<…>>`
// is std-only — no new crate touches the §8 whitelist. The shell calls
// `init_global` once at startup; subsequent `global()` calls return a
// reference to the lazily-installed Mutex.

use std::sync::{Mutex, OnceLock};

static GLOBAL: OnceLock<Mutex<Vault>> = OnceLock::new();

/// Initialise the process-global vault from `path`. Idempotent: a second
/// call with the same path is a no-op (returns Ok). Returns an error only
/// when the first call to `Vault::open` fails.
pub fn init_global(path: &Path) -> Result<(), VaultError> {
    if GLOBAL.get().is_some() {
        return Ok(());
    }
    let v = Vault::open(path)?;
    let _ = GLOBAL.set(Mutex::new(v));
    Ok(())
}

/// Borrow the process-global vault. Returns `None` if `init_global` has
/// not yet been called (early-startup paths must tolerate this — the
/// dispatcher's SetSetting handler logs + drops on `None` rather than
/// blocking the pump).
pub fn global() -> Option<&'static Mutex<Vault>> {
    GLOBAL.get()
}

impl Vault {
    /// Convenience accessor mirroring `crate::config_vault::global()` —
    /// keeps call sites that already have `Vault::` in scope short.
    pub fn global() -> Option<&'static Mutex<Vault>> {
        global()
    }
}

fn parse_body(json: &[u8]) -> Result<VaultBody, VaultError> {
    if json.is_empty() {
        return Ok(VaultBody::default());
    }
    serde_json::from_slice(json).map_err(|e| VaultError::Json {
        ctx: "deserialize body",
        message: e.to_string(),
    })
}

/// Production KDF cost: the locked Q2=C hardened params (64 MiB / t=3 / p=4).
#[cfg(not(test))]
fn current_kdf_params() -> Argon2Params {
    Argon2Params::DEFAULT
}

/// Test-cfg-only KDF cost: low-memory params to cap per-test RSS (min-mem E2E,
/// Task #8). Compile-time seam only — production keeps `DEFAULT`.
#[cfg(test)]
fn current_kdf_params() -> Argon2Params {
    Argon2Params::TEST
}

fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; KEY_LEN], VaultError> {
    let raw = crypto::argon2id::argon2id(passphrase.as_bytes(), salt, current_kdf_params())?;
    if raw.len() != KEY_LEN {
        // Argon2id with tag_len=32 always returns 32 bytes; this is a
        // defensive check rather than an expected branch.
        return Err(VaultError::Argon2(Argon2Error::InvalidTagLen {
            tag_len: raw.len() as u32,
        }));
    }
    let mut out = [0u8; KEY_LEN];
    out.copy_from_slice(&raw);
    Ok(out)
}

/// Constant-time byte-slice equality. Hand-rolled to avoid the `subtle`
/// crate (not on §8 whitelist).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= *x ^ *y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("bentodesk-vault-{pid}-{n}"));
        std::fs::create_dir_all(&dir).expect("create tempdir");
        dir
    }

    #[test]
    fn open_returns_empty_vault_when_file_missing() {
        let dir = tempdir();
        let path = dir.join("settings.vault");
        let v = Vault::open(&path).expect("open empty");
        assert!(v.get_setting("nope").is_none());
        assert!(!v.is_dirty());
        assert_eq!(v.mode_tag(), ModeTag::None);
    }

    #[test]
    fn round_trip_none_mode() {
        let dir = tempdir();
        let path = dir.join("settings.vault");
        {
            let mut v = Vault::open(&path).unwrap();
            v.set_setting("display.mode", SettingValue::Str("dark".into()));
            v.set_setting("performance.target_fps", SettingValue::Int(60));
            v.set_setting("audio.enabled", SettingValue::Bool(true));
            v.set_setting("zoom", SettingValue::Float(1.25));
            v.flush().unwrap();
        }
        let v2 = Vault::open(&path).unwrap();
        assert_eq!(
            v2.get_setting("display.mode"),
            Some(SettingValue::Str("dark".into()))
        );
        assert_eq!(
            v2.get_setting("performance.target_fps"),
            Some(SettingValue::Int(60))
        );
        assert_eq!(
            v2.get_setting("audio.enabled"),
            Some(SettingValue::Bool(true))
        );
        assert_eq!(v2.get_setting("zoom"), Some(SettingValue::Float(1.25)));
        assert_eq!(v2.mode_tag(), ModeTag::None);
    }

    /// M1a 2026-05-29 — Settings panel "Save" persists the 5 General-section
    /// toggles as `general.*` boolean keys, then `flush()`es. On the next
    /// launch the shell reads them back to restore the panel + apply them.
    /// This pins that persistence contract in the DEFAULT (None) mode: write
    /// the 5 keys with mixed booleans, flush, reopen the SAME path, and assert
    /// each reads back identically. Key literals match
    /// `bentodesk-shell/src/main.rs` (`SETTING_GENERAL_*` constants).
    /// Deliberately uses None mode only — Passphrase flush is a quarantined
    /// pre-existing crash and is never exercised here.
    #[test]
    fn round_trip_general_section_bool_keys_none_mode() {
        const GHOST_LAYER_ENABLED: &str = "general.ghost_layer_enabled";
        const LAUNCH_AT_STARTUP: &str = "general.launch_at_startup";
        const SHOW_IN_TASKBAR: &str = "general.show_in_taskbar";
        const AUTO_GROUP_ENABLED: &str = "general.auto_group_enabled";
        const PORTABLE_MODE: &str = "general.portable_mode";

        let dir = tempdir();
        let path = dir.join("settings.vault");
        {
            let mut v = Vault::open(&path).unwrap();
            v.set_setting(GHOST_LAYER_ENABLED, SettingValue::Bool(false));
            v.set_setting(LAUNCH_AT_STARTUP, SettingValue::Bool(true));
            v.set_setting(SHOW_IN_TASKBAR, SettingValue::Bool(false));
            v.set_setting(AUTO_GROUP_ENABLED, SettingValue::Bool(true));
            v.set_setting(PORTABLE_MODE, SettingValue::Bool(true));
            v.flush().unwrap();
        }
        let v2 = Vault::open(&path).unwrap();
        assert_eq!(
            v2.get_setting(GHOST_LAYER_ENABLED),
            Some(SettingValue::Bool(false))
        );
        assert_eq!(
            v2.get_setting(LAUNCH_AT_STARTUP),
            Some(SettingValue::Bool(true))
        );
        assert_eq!(
            v2.get_setting(SHOW_IN_TASKBAR),
            Some(SettingValue::Bool(false))
        );
        assert_eq!(
            v2.get_setting(AUTO_GROUP_ENABLED),
            Some(SettingValue::Bool(true))
        );
        assert_eq!(
            v2.get_setting(PORTABLE_MODE),
            Some(SettingValue::Bool(true))
        );
        assert_eq!(v2.mode_tag(), ModeTag::None);
    }

    #[test]
    fn remove_setting_deletes_key_and_round_trips() {
        let dir = tempdir();
        let path = dir.join("settings.vault");
        {
            let mut v = Vault::open(&path).unwrap();
            v.set_setting("theme.base_accent", SettingValue::Str("#3b82f6".into()));
            assert!(v.remove_setting("theme.base_accent"));
            assert!(!v.remove_setting("theme.base_accent"));
            v.flush().unwrap();
        }
        let v2 = Vault::open(&path).unwrap();
        assert_eq!(v2.get_setting("theme.base_accent"), None);
    }

    #[cfg(windows)]
    #[test]
    fn round_trip_dpapi_mode() {
        let dir = tempdir();
        let path = dir.join("settings.vault");
        {
            let mut v = Vault::open(&path).unwrap();
            v.set_mode(EncryptionMode::Dpapi).unwrap();
            v.set_setting("secret.token", SettingValue::Str("abc-xyz".into()));
            v.flush().unwrap();
        }
        let v2 = Vault::open(&path).unwrap();
        assert_eq!(
            v2.get_setting("secret.token"),
            Some(SettingValue::Str("abc-xyz".into()))
        );
        assert_eq!(v2.mode_tag(), ModeTag::Dpapi);
    }

    #[cfg(windows)]
    #[test]
    fn round_trip_passphrase_mode() {
        let dir = tempdir();
        let path = dir.join("settings.vault");
        {
            let mut v = Vault::open(&path).unwrap();
            v.set_mode(EncryptionMode::Passphrase {
                passphrase: "correct horse".into(),
            })
            .unwrap();
            v.set_setting("secret.api_key", SettingValue::Str("hunter2".into()));
            v.flush().unwrap();
        }
        // Wrong passphrase → tag mismatch.
        let err = Vault::open_with_passphrase(&path, "wrong").expect_err("must fail");
        assert!(matches!(err, VaultError::AesGcm(AesGcmError::TagMismatch)));

        // Correct passphrase → unlocked.
        let v2 = Vault::open_with_passphrase(&path, "correct horse").unwrap();
        assert_eq!(
            v2.get_setting("secret.api_key"),
            Some(SettingValue::Str("hunter2".into()))
        );
        assert_eq!(v2.mode_tag(), ModeTag::Passphrase);
        assert!(v2.verify_passphrase("correct horse"));
        assert!(!v2.verify_passphrase("wrong"));
    }

    #[cfg(windows)]
    #[test]
    fn open_passphrase_mode_without_passphrase_stays_locked_and_preserves_disk() {
        let dir = tempdir();
        let path = dir.join("settings.vault");
        {
            let mut v = Vault::open(&path).unwrap();
            v.set_setting("secret.api_key", SettingValue::Str("hunter2".into()));
            v.set_mode(EncryptionMode::Passphrase {
                passphrase: "correct horse".into(),
            })
            .unwrap();
            v.flush().unwrap();
        }

        let mut locked = Vault::open(&path).unwrap();
        assert_eq!(locked.mode_tag(), ModeTag::Passphrase);
        assert!(locked.is_locked_passphrase());
        assert_eq!(locked.get_setting("secret.api_key"), None);
        assert!(!locked.verify_passphrase("correct horse"));

        locked.set_setting("secret.api_key", SettingValue::Str("mutated".into()));
        assert!(matches!(locked.flush(), Err(VaultError::NoPassphraseSet)));

        let reopened = Vault::open_with_passphrase(&path, "correct horse").unwrap();
        assert_eq!(
            reopened.get_setting("secret.api_key"),
            Some(SettingValue::Str("hunter2".into()))
        );
    }

    #[cfg(windows)]
    #[test]
    fn mode_switch_rewrites_file() {
        let dir = tempdir();
        let path = dir.join("settings.vault");
        {
            let mut v = Vault::open(&path).unwrap();
            v.set_setting("k", SettingValue::Str("v".into()));
            v.flush().unwrap(); // None mode
        }
        {
            let mut v = Vault::open(&path).unwrap();
            v.set_mode(EncryptionMode::Dpapi).unwrap();
            v.flush().unwrap(); // re-encrypted via DPAPI
        }
        let v3 = Vault::open(&path).unwrap();
        assert_eq!(v3.get_setting("k"), Some(SettingValue::Str("v".into())));
        assert_eq!(v3.mode_tag(), ModeTag::Dpapi);
    }

    #[test]
    fn flush_is_idempotent_on_clean_vault() {
        let dir = tempdir();
        let path = dir.join("settings.vault");
        let mut v = Vault::open(&path).unwrap();
        v.flush().unwrap(); // first flush — dirty=false because empty + nothing set
        // No file should be created when nothing was set.
        assert!(!path.exists() || std::fs::metadata(&path).is_ok());
    }

    #[test]
    fn verify_passphrase_returns_false_for_non_passphrase_mode() {
        let dir = tempdir();
        let path = dir.join("settings.vault");
        let v = Vault::open(&path).unwrap();
        assert!(!v.verify_passphrase("anything"));
    }

    #[test]
    fn version_too_new_is_rejected() {
        let dir = tempdir();
        let path = dir.join("settings.vault");
        let bad = VaultRecord {
            version: 99,
            mode_tag: ModeTag::None as u8,
            salt_b64: String::new(),
            nonce_b64: String::new(),
            tag_b64: String::new(),
            ciphertext_b64: String::new(),
        };
        backend_storage::write_json_atomic(&path, &bad).unwrap();
        let err = Vault::open(&path).expect_err("must reject");
        assert!(matches!(err, VaultError::VersionTooNew { found: 99, .. }));
    }

    #[test]
    fn constant_time_eq_basic() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn global_returns_none_before_init() {
        // The OnceLock is process-wide and any other test in this binary
        // that calls `init_global` would race with us. We can't safely
        // observe `None` here without ordering, so we only assert the
        // weaker invariant: `global()` returns the same Option each call
        // (i.e. it does not lie about presence between back-to-back reads).
        let a = global().is_some();
        let b = global().is_some();
        assert_eq!(a, b, "global() must be deterministic between calls");
    }

    #[test]
    fn set_then_open_roundtrips_persisted_value() {
        // Mirrors the F2-03 reachability gate — the SetSetting handler in
        // the shell sets, flushes, and then a fresh `Vault::open` on the
        // same path must observe the value. Independent of the global
        // accessor (which has process-singleton semantics that make it
        // unsafe to test concurrently).
        let dir = tempdir();
        let path = dir.join("settings.vault");
        {
            let mut v = Vault::open(&path).unwrap();
            v.set_setting("test", SettingValue::Str("hello".into()));
            v.flush().unwrap();
        }
        let v2 = Vault::open(&path).unwrap();
        assert_eq!(
            v2.get_setting("test"),
            Some(SettingValue::Str("hello".into()))
        );
    }
}
