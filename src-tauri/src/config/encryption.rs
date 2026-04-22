//! Settings encryption layer (Theme A — A3).
//!
//! Three modes are supported:
//!
//! * [`EncryptionMode::None`] — store plaintext. Default, backwards compatible.
//! * [`EncryptionMode::Dpapi`] — wrap the serialized settings bytes with
//!   Windows DPAPI (`CryptProtectData` scoped to the current user). No
//!   passphrase required; protection is transparent but can only be unwrapped
//!   from the same Windows account on the same machine.
//! * [`EncryptionMode::Passphrase`] — derive a 32-byte key via Argon2id from a
//!   user-supplied passphrase and encrypt with AES-256-GCM. Usable across
//!   machines as long as the passphrase is remembered.
//!
//! The encryption layer operates on byte blobs so it can wrap whatever the
//! caller serializes (typically a `serde_json::Value`). It never touches
//! `AppSettings` directly, keeping Theme A changes orthogonal to the existing
//! settings shape.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::error::BentoDeskError;

/// Persisted encryption mode choice. Written under `settings.encryption.mode`
/// so a fresh downgrade still reads a known-default shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptionMode {
    #[default]
    None,
    Dpapi,
    Passphrase,
}

/// Envelope written to disk when encryption is enabled. The plaintext
/// [`crate::config::settings::AppSettings`] is JSON-serialized then wrapped
/// inside one of these envelopes, keyed by `mode`. On load, the app peeks at
/// the shape (plain JSON vs. this envelope) to decide which branch to take.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum EncryptedBlob {
    /// DPAPI-protected ciphertext. `data` is base64-encoded `CryptProtectData`
    /// output — decipherable only by the same Windows user.
    Dpapi { data: String },
    /// AES-256-GCM ciphertext. Nonce and Argon2 salt are stored inline so the
    /// file is self-describing for the lifetime of the user's passphrase.
    Passphrase {
        data: String,
        nonce: String,
        salt: String,
    },
}

const GCM_NONCE_LEN: usize = 12;
const ARGON2_SALT_LEN: usize = 16;
const ARGON2_OUTPUT_LEN: usize = 32;

/// Key material wrapper that zeroises on drop so the derived AES key cannot
/// be left behind on the stack or in Drop-elided heap allocations.
#[derive(Zeroize)]
#[zeroize(drop)]
struct DerivedKey([u8; ARGON2_OUTPUT_LEN]);

fn derive_key(passphrase: &str, salt: &[u8]) -> Result<DerivedKey, BentoDeskError> {
    let mut out = [0u8; ARGON2_OUTPUT_LEN];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| BentoDeskError::ConfigError(format!("Argon2 key derivation failed: {e}")))?;
    Ok(DerivedKey(out))
}

/// Encrypt `plaintext` with AES-256-GCM using a key derived from `passphrase`.
pub fn encrypt_with_passphrase(
    plaintext: &[u8],
    passphrase: &str,
) -> Result<EncryptedBlob, BentoDeskError> {
    if passphrase.is_empty() {
        return Err(BentoDeskError::ConfigError(
            "Passphrase cannot be empty".to_string(),
        ));
    }

    let mut salt = [0u8; ARGON2_SALT_LEN];
    let mut nonce_bytes = [0u8; GCM_NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut nonce_bytes);

    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key.0)
        .map_err(|e| BentoDeskError::ConfigError(format!("AES-GCM key init failed: {e}")))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: b"bentodesk.settings.v1",
            },
        )
        .map_err(|e| BentoDeskError::ConfigError(format!("AES-GCM encrypt failed: {e}")))?;

    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    Ok(EncryptedBlob::Passphrase {
        data: B64.encode(ciphertext),
        nonce: B64.encode(nonce_bytes),
        salt: B64.encode(salt),
    })
}

/// Decrypt an AES-GCM envelope produced by [`encrypt_with_passphrase`].
pub fn decrypt_with_passphrase(
    blob: &EncryptedBlob,
    passphrase: &str,
) -> Result<Vec<u8>, BentoDeskError> {
    let EncryptedBlob::Passphrase { data, nonce, salt } = blob else {
        return Err(BentoDeskError::ConfigError(
            "Passphrase decrypt called on non-passphrase envelope".to_string(),
        ));
    };

    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let ciphertext = B64
        .decode(data)
        .map_err(|e| BentoDeskError::ConfigError(format!("base64 data decode: {e}")))?;
    let nonce_bytes = B64
        .decode(nonce)
        .map_err(|e| BentoDeskError::ConfigError(format!("base64 nonce decode: {e}")))?;
    let salt_bytes = B64
        .decode(salt)
        .map_err(|e| BentoDeskError::ConfigError(format!("base64 salt decode: {e}")))?;

    if nonce_bytes.len() != GCM_NONCE_LEN {
        return Err(BentoDeskError::ConfigError(format!(
            "Invalid GCM nonce length: expected {GCM_NONCE_LEN}, got {}",
            nonce_bytes.len()
        )));
    }

    let key = derive_key(passphrase, &salt_bytes)?;
    let cipher = Aes256Gcm::new_from_slice(&key.0)
        .map_err(|e| BentoDeskError::ConfigError(format!("AES-GCM key init failed: {e}")))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: &ciphertext,
                aad: b"bentodesk.settings.v1",
            },
        )
        .map_err(|_| {
            BentoDeskError::ConfigError("AES-GCM decrypt failed (wrong passphrase?)".to_string())
        })
}

/// Encrypt `plaintext` via Windows DPAPI bound to the current user.
#[cfg(windows)]
pub fn encrypt_with_dpapi(plaintext: &[u8]) -> Result<EncryptedBlob, BentoDeskError> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};

    let input = CRYPT_INTEGER_BLOB {
        cbData: plaintext.len() as u32,
        pbData: plaintext.as_ptr() as *mut u8,
    };
    let description: Vec<u16> = "BentoDesk Settings"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut output = CRYPT_INTEGER_BLOB::default();

    // SAFETY: `input` points at a valid slice for the duration of the call,
    // `output` is a zero-initialized blob Windows will populate, and we free
    // the returned memory via LocalFree below. The PCWSTR is null-terminated.
    let ok = unsafe {
        CryptProtectData(
            &input as *const _,
            PCWSTR(description.as_ptr()),
            None,
            None,
            None,
            0,
            &mut output as *mut _,
        )
    };

    ok.map_err(|e| BentoDeskError::ConfigError(format!("CryptProtectData failed: {e}")))?;

    // SAFETY: CryptProtectData sets pbData / cbData on success. We copy the
    // bytes out before freeing the Windows-allocated buffer.
    let ciphertext =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };

    // SAFETY: Windows requires us to free the blob via LocalFree on success.
    unsafe {
        let _ = LocalFree(windows::Win32::Foundation::HLOCAL(output.pbData as *mut _));
    }

    Ok(EncryptedBlob::Dpapi {
        data: B64.encode(ciphertext),
    })
}

#[cfg(windows)]
pub fn decrypt_with_dpapi(blob: &EncryptedBlob) -> Result<Vec<u8>, BentoDeskError> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

    let EncryptedBlob::Dpapi { data } = blob else {
        return Err(BentoDeskError::ConfigError(
            "DPAPI decrypt called on non-DPAPI envelope".to_string(),
        ));
    };

    let mut ciphertext = B64
        .decode(data)
        .map_err(|e| BentoDeskError::ConfigError(format!("base64 DPAPI decode: {e}")))?;

    let input = CRYPT_INTEGER_BLOB {
        cbData: ciphertext.len() as u32,
        pbData: ciphertext.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();

    // SAFETY: Same contract as `encrypt_with_dpapi`. All pointers are valid
    // for the call; the returned buffer is freed with LocalFree.
    let ok = unsafe {
        CryptUnprotectData(
            &input as *const _,
            None,
            None,
            None,
            None,
            0,
            &mut output as *mut _,
        )
    };
    ok.map_err(|e| {
        BentoDeskError::ConfigError(format!(
            "CryptUnprotectData failed (different user or machine?): {e}"
        ))
    })?;

    // SAFETY: output is populated by CryptUnprotectData; copy-out-then-free.
    let plaintext =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };

    // SAFETY: Free the Windows-allocated buffer.
    unsafe {
        let _ = LocalFree(windows::Win32::Foundation::HLOCAL(output.pbData as *mut _));
    }

    let _ = input;
    Ok(plaintext)
}

// Non-windows stubs so the module still compiles in CI mirrors. In practice
// BentoDesk only ships on Windows, but keeping these avoids a cfg() cascade
// every time encryption is referenced.
#[cfg(not(windows))]
pub fn encrypt_with_dpapi(_plaintext: &[u8]) -> Result<EncryptedBlob, BentoDeskError> {
    Err(BentoDeskError::ConfigError(
        "DPAPI only available on Windows".to_string(),
    ))
}

#[cfg(not(windows))]
pub fn decrypt_with_dpapi(_blob: &EncryptedBlob) -> Result<Vec<u8>, BentoDeskError> {
    Err(BentoDeskError::ConfigError(
        "DPAPI only available on Windows".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passphrase_roundtrip_succeeds() {
        let plaintext = b"hello secrets";
        let envelope = encrypt_with_passphrase(plaintext, "correct horse battery staple").unwrap();
        let recovered = decrypt_with_passphrase(&envelope, "correct horse battery staple").unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn passphrase_wrong_key_fails() {
        let envelope = encrypt_with_passphrase(b"payload", "secret1").unwrap();
        let err = decrypt_with_passphrase(&envelope, "secret2").unwrap_err();
        assert!(err.to_string().contains("decrypt failed"));
    }

    #[test]
    fn empty_passphrase_rejected() {
        let err = encrypt_with_passphrase(b"x", "").unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn passphrase_produces_unique_nonces() {
        let a = encrypt_with_passphrase(b"same input", "same key").unwrap();
        let b = encrypt_with_passphrase(b"same input", "same key").unwrap();
        match (a, b) {
            (
                EncryptedBlob::Passphrase { nonce: na, .. },
                EncryptedBlob::Passphrase { nonce: nb, .. },
            ) => assert_ne!(na, nb, "Nonces must be unique per encrypt"),
            _ => panic!("Expected passphrase envelopes"),
        }
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_roundtrip_succeeds_on_windows() {
        let plaintext = b"dpapi payload";
        let envelope = encrypt_with_dpapi(plaintext).unwrap();
        let recovered = decrypt_with_dpapi(&envelope).unwrap();
        assert_eq!(recovered, plaintext);
    }
}
