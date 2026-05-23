//! AES-256-GCM via Win32 BCrypt (`bcrypt.dll`).
//!
//! Q2=C ruling: zero new crates. Cryptography rides on the windows-sys
//! 0.59 binding metadata only — no `aes-gcm` / `ring` / `rust-crypto`.
//!
//! Algorithm: AES-256-GCM with a 12-byte nonce (96-bit, the GCM
//! recommended size per NIST SP 800-38D §5.2.1.1) and a 16-byte tag.
//! Key length: 32 bytes (AES-256). Caller supplies key + nonce.
//!
//! Tag mismatches surface as [`AesGcmError::TagMismatch`] (BCrypt returns
//! `STATUS_AUTH_TAG_MISMATCH = 0xC000A002`).

#[cfg(windows)]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_AES_ALGORITHM, BCRYPT_ALG_HANDLE, BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO,
    BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION, BCRYPT_CHAIN_MODE_GCM, BCRYPT_CHAINING_MODE,
    BCRYPT_KEY_HANDLE, BCryptCloseAlgorithmProvider, BCryptDecrypt, BCryptDestroyKey,
    BCryptEncrypt, BCryptGenerateSymmetricKey, BCryptOpenAlgorithmProvider, BCryptSetProperty,
};

/// AES-256-GCM key length in bytes.
pub const KEY_LEN: usize = 32;
/// AES-GCM nonce length in bytes (NIST SP 800-38D recommended).
pub const NONCE_LEN: usize = 12;
/// AES-GCM authentication tag length in bytes.
pub const TAG_LEN: usize = 16;

/// `STATUS_AUTH_TAG_MISMATCH` — surfaced by `BCryptDecrypt` when the
/// supplied tag does not authenticate the ciphertext+aad.
#[cfg(windows)]
const STATUS_AUTH_TAG_MISMATCH: i32 = 0xC000_A002u32 as i32;

/// Errors surfaced by the AES-GCM helpers. Hand-rolled per spec §8.1.
#[derive(Debug)]
pub enum AesGcmError {
    /// Caller passed a key/nonce/tag of the wrong length.
    LengthMismatch {
        what: &'static str,
        expected: usize,
        got: usize,
    },
    /// `BCryptDecrypt` returned `STATUS_AUTH_TAG_MISMATCH` — the ciphertext
    /// has been tampered with, the tag is wrong, or the wrong key was used.
    TagMismatch,
    /// Any other BCrypt NTSTATUS != 0.
    BCryptStatus { ctx: &'static str, ntstatus: i32 },
}

impl core::fmt::Display for AesGcmError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::LengthMismatch {
                what,
                expected,
                got,
            } => {
                write!(f, "{what} length mismatch: expected {expected}, got {got}")
            }
            Self::TagMismatch => f.write_str("AES-GCM tag mismatch (ciphertext or key tampered)"),
            Self::BCryptStatus { ctx, ntstatus } => {
                write!(f, "{ctx}: BCrypt NTSTATUS {ntstatus:#x}")
            }
        }
    }
}

impl core::error::Error for AesGcmError {}

/// Encrypted output: ciphertext + 16-byte auth tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AesGcmCiphertext {
    pub ciphertext: Vec<u8>,
    pub tag: [u8; TAG_LEN],
}

#[cfg(windows)]
struct AesGcmAlg(BCRYPT_ALG_HANDLE);

#[cfg(windows)]
impl AesGcmAlg {
    fn open() -> Result<Self, AesGcmError> {
        let mut handle: BCRYPT_ALG_HANDLE = core::ptr::null_mut();
        // SAFETY: `phalgorithm` is a valid out-pointer to a stack `BCRYPT_ALG_HANDLE`.
        // `BCRYPT_AES_ALGORITHM` is a static null-terminated UTF-16 string from the
        // windows-sys binding. `pszimplementation = NULL` selects the default provider.
        let status = unsafe {
            BCryptOpenAlgorithmProvider(&mut handle, BCRYPT_AES_ALGORITHM, core::ptr::null(), 0)
        };
        if status != 0 {
            return Err(AesGcmError::BCryptStatus {
                ctx: "BCryptOpenAlgorithmProvider(AES)",
                ntstatus: status,
            });
        }

        // Set chaining mode to GCM. The string `BCRYPT_CHAIN_MODE_GCM` is a
        // static null-terminated UTF-16 from windows-sys; we pass its byte
        // length including the null terminator (bytes = chars * 2 since UTF-16,
        // and we count the trailing 0u16 too).
        let gcm_str = BCRYPT_CHAIN_MODE_GCM;
        let mut gcm_byte_len: u32 = 0;
        // SAFETY: gcm_str is a static null-terminated UTF-16 string; we
        // compute its byte length by walking until the null terminator.
        unsafe {
            let mut p = gcm_str;
            while *p != 0 {
                gcm_byte_len += 2;
                p = p.add(1);
            }
            gcm_byte_len += 2; // include null terminator
        }
        // SAFETY: `handle` is a valid open algorithm handle from the call
        // immediately above. `BCRYPT_CHAINING_MODE` and `BCRYPT_CHAIN_MODE_GCM`
        // are static UTF-16 strings owned by the windows-sys binding.
        let status = unsafe {
            BCryptSetProperty(
                handle as *mut _,
                BCRYPT_CHAINING_MODE,
                gcm_str as *const u8,
                gcm_byte_len,
                0,
            )
        };
        if status != 0 {
            // SAFETY: `handle` is a valid handle from BCryptOpenAlgorithmProvider.
            unsafe {
                BCryptCloseAlgorithmProvider(handle, 0);
            }
            return Err(AesGcmError::BCryptStatus {
                ctx: "BCryptSetProperty(ChainingMode=GCM)",
                ntstatus: status,
            });
        }

        Ok(Self(handle))
    }
}

#[cfg(windows)]
impl Drop for AesGcmAlg {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: handle is a valid algorithm handle owned by this struct;
            // dropping after a successful open is the BCrypt-required disposal.
            unsafe {
                BCryptCloseAlgorithmProvider(self.0, 0);
            }
        }
    }
}

#[cfg(windows)]
struct AesGcmKey(BCRYPT_KEY_HANDLE);

#[cfg(windows)]
impl AesGcmKey {
    fn import(alg: &AesGcmAlg, key: &[u8; KEY_LEN]) -> Result<Self, AesGcmError> {
        let mut handle: BCRYPT_KEY_HANDLE = core::ptr::null_mut();
        // SAFETY: `alg.0` is a valid algorithm handle. `key` is a 32-byte
        // contiguous slice; `pbsecret`/`cbsecret` describe its address+length.
        // `pbkeyobject = NULL` + `cbkeyobject = 0` lets BCrypt allocate the
        // key-object buffer internally (modern OS only — Win 7+).
        let status = unsafe {
            BCryptGenerateSymmetricKey(
                alg.0,
                &mut handle,
                core::ptr::null_mut(),
                0,
                key.as_ptr(),
                key.len() as u32,
                0,
            )
        };
        if status != 0 {
            return Err(AesGcmError::BCryptStatus {
                ctx: "BCryptGenerateSymmetricKey",
                ntstatus: status,
            });
        }
        Ok(Self(handle))
    }
}

#[cfg(windows)]
impl Drop for AesGcmKey {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: handle owned by this struct; release via documented BCrypt API.
            unsafe {
                BCryptDestroyKey(self.0);
            }
        }
    }
}

/// AES-256-GCM encryption. Returns ciphertext (same length as plaintext)
/// and a 16-byte auth tag.
#[cfg(windows)]
pub fn encrypt(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<AesGcmCiphertext, AesGcmError> {
    let alg = AesGcmAlg::open()?;
    let key_handle = AesGcmKey::import(&alg, key)?;

    let mut tag = [0u8; TAG_LEN];
    let mut ciphertext = vec![0u8; plaintext.len()];

    // BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO carries the nonce + AAD + tag
    // pointers. We must keep the local `nonce`/`aad`/`tag` slices alive for
    // the BCryptEncrypt call (lifetime of `mode_info` is bounded by this fn).
    let mut nonce_local = *nonce;
    // `mode_info` is held by-value for the duration of the BCryptEncrypt
    // call; the API mutates `pbTag` (writes the auth tag) through the
    // raw pointer, but the struct fields themselves are untouched.
    let mode_info = BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
        cbSize: core::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32,
        dwInfoVersion: BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION,
        pbNonce: nonce_local.as_mut_ptr(),
        cbNonce: nonce_local.len() as u32,
        pbAuthData: if aad.is_empty() {
            core::ptr::null_mut()
        } else {
            aad.as_ptr() as *mut u8
        },
        cbAuthData: aad.len() as u32,
        pbTag: tag.as_mut_ptr(),
        cbTag: tag.len() as u32,
        pbMacContext: core::ptr::null_mut(),
        cbMacContext: 0,
        cbAAD: 0,
        cbData: 0,
        dwFlags: 0,
    };

    let mut bytes_written: u32 = 0;
    // SAFETY:
    // - `key_handle.0` is a valid AES key handle.
    // - `plaintext` and `ciphertext` are non-overlapping; ciphertext is
    //   pre-allocated to plaintext.len() (no growth required for GCM).
    // - `mode_info` lives for the duration of the call; its pbNonce/pbTag/
    //   pbAuthData pointers reference local stack allocations (`nonce_local`,
    //   `tag`) and the caller's `aad` slice (no aliasing with input/output).
    // - `pbiv = NULL`, `cbiv = 0` is required for GCM (nonce supplied via mode_info).
    let status = unsafe {
        BCryptEncrypt(
            key_handle.0,
            plaintext.as_ptr(),
            plaintext.len() as u32,
            &mode_info as *const _ as *const core::ffi::c_void,
            core::ptr::null_mut(),
            0,
            ciphertext.as_mut_ptr(),
            ciphertext.len() as u32,
            &mut bytes_written,
            0,
        )
    };
    // Keep `nonce_local` alive past the unsafe block (its address was
    // captured into `mode_info.pbNonce`).
    let _ = &nonce_local;
    if status != 0 {
        return Err(AesGcmError::BCryptStatus {
            ctx: "BCryptEncrypt(AES-GCM)",
            ntstatus: status,
        });
    }
    debug_assert_eq!(bytes_written as usize, ciphertext.len());

    Ok(AesGcmCiphertext { ciphertext, tag })
}

/// AES-256-GCM decryption. Verifies the tag against the ciphertext+aad
/// before returning the plaintext.
#[cfg(windows)]
pub fn decrypt(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    aad: &[u8],
    ciphertext: &[u8],
    tag: &[u8; TAG_LEN],
) -> Result<Vec<u8>, AesGcmError> {
    let alg = AesGcmAlg::open()?;
    let key_handle = AesGcmKey::import(&alg, key)?;

    let mut plaintext = vec![0u8; ciphertext.len()];
    let mut nonce_local = *nonce;
    let mut tag_local = *tag;

    let mode_info = BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
        cbSize: core::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32,
        dwInfoVersion: BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION,
        pbNonce: nonce_local.as_mut_ptr(),
        cbNonce: nonce_local.len() as u32,
        pbAuthData: if aad.is_empty() {
            core::ptr::null_mut()
        } else {
            aad.as_ptr() as *mut u8
        },
        cbAuthData: aad.len() as u32,
        pbTag: tag_local.as_mut_ptr(),
        cbTag: tag_local.len() as u32,
        pbMacContext: core::ptr::null_mut(),
        cbMacContext: 0,
        cbAAD: 0,
        cbData: 0,
        dwFlags: 0,
    };

    let mut bytes_written: u32 = 0;
    // SAFETY: same justification as `encrypt`. BCryptDecrypt verifies the
    // tag before populating the output buffer; on tag mismatch it returns
    // STATUS_AUTH_TAG_MISMATCH and the output is left zeroed.
    let status = unsafe {
        BCryptDecrypt(
            key_handle.0,
            ciphertext.as_ptr(),
            ciphertext.len() as u32,
            &mode_info as *const _ as *const core::ffi::c_void,
            core::ptr::null_mut(),
            0,
            plaintext.as_mut_ptr(),
            plaintext.len() as u32,
            &mut bytes_written,
            0,
        )
    };
    // Keep nonce_local + tag_local alive past the unsafe block (their
    // addresses were captured into mode_info.pbNonce / pbTag).
    let _ = &nonce_local;
    let _ = &tag_local;
    if status == STATUS_AUTH_TAG_MISMATCH {
        return Err(AesGcmError::TagMismatch);
    }
    if status != 0 {
        return Err(AesGcmError::BCryptStatus {
            ctx: "BCryptDecrypt(AES-GCM)",
            ntstatus: status,
        });
    }
    Ok(plaintext)
}

// Non-Windows fallbacks — the nano backend is Windows-only by spec but the
// cross-cfg shape keeps signatures honest for `cargo check --target` on dev
// boxes. These return `BCryptStatus { ntstatus: 0xC0000001 (STATUS_UNSUCCESSFUL) }`.
#[cfg(not(windows))]
pub fn encrypt(
    _key: &[u8; KEY_LEN],
    _nonce: &[u8; NONCE_LEN],
    _aad: &[u8],
    _plaintext: &[u8],
) -> Result<AesGcmCiphertext, AesGcmError> {
    Err(AesGcmError::BCryptStatus {
        ctx: "encrypt(non-windows)",
        ntstatus: 0xC000_0001u32 as i32,
    })
}

#[cfg(not(windows))]
pub fn decrypt(
    _key: &[u8; KEY_LEN],
    _nonce: &[u8; NONCE_LEN],
    _aad: &[u8],
    _ciphertext: &[u8],
    _tag: &[u8; TAG_LEN],
) -> Result<Vec<u8>, AesGcmError> {
    Err(AesGcmError::BCryptStatus {
        ctx: "decrypt(non-windows)",
        ntstatus: 0xC000_0001u32 as i32,
    })
}

/// Generate cryptographically-secure random bytes via Win32 `BCryptGenRandom`.
#[cfg(windows)]
pub fn random_bytes(out: &mut [u8]) -> Result<(), AesGcmError> {
    use windows_sys::Win32::Security::Cryptography::{
        BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
    };
    // SAFETY: `out` is a contiguous mutable slice; `cbbuffer` is its byte
    // length. Passing `BCRYPT_USE_SYSTEM_PREFERRED_RNG` lets the algorithm
    // handle be NULL — BCrypt routes to the system default RNG.
    let status = unsafe {
        BCryptGenRandom(
            core::ptr::null_mut(),
            out.as_mut_ptr(),
            out.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status != 0 {
        return Err(AesGcmError::BCryptStatus {
            ctx: "BCryptGenRandom",
            ntstatus: status,
        });
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn random_bytes(_out: &mut [u8]) -> Result<(), AesGcmError> {
    Err(AesGcmError::BCryptStatus {
        ctx: "random_bytes(non-windows)",
        ntstatus: 0xC000_0001u32 as i32,
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn round_trip_empty_aad() {
        let key = [0x42u8; KEY_LEN];
        let nonce = [0x11u8; NONCE_LEN];
        let plaintext = b"hello, vault" as &[u8];

        let ct = encrypt(&key, &nonce, &[], plaintext).expect("encrypt");
        let pt = decrypt(&key, &nonce, &[], &ct.ciphertext, &ct.tag).expect("decrypt");
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn round_trip_with_aad() {
        let key = [0x99u8; KEY_LEN];
        let nonce = [0x77u8; NONCE_LEN];
        let aad = b"vault-record-v1";
        let plaintext = b"the quick brown fox jumps over the lazy dog" as &[u8];

        let ct = encrypt(&key, &nonce, aad, plaintext).expect("encrypt");
        let pt = decrypt(&key, &nonce, aad, &ct.ciphertext, &ct.tag).expect("decrypt");
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn round_trip_empty_plaintext() {
        let key = [0u8; KEY_LEN];
        let nonce = [1u8; NONCE_LEN];
        let ct = encrypt(&key, &nonce, &[], &[]).expect("encrypt");
        assert_eq!(ct.ciphertext.len(), 0);
        let pt = decrypt(&key, &nonce, &[], &ct.ciphertext, &ct.tag).expect("decrypt");
        assert!(pt.is_empty());
    }

    #[test]
    fn tag_mismatch_is_detected() {
        let key = [0xaau8; KEY_LEN];
        let nonce = [0xbbu8; NONCE_LEN];
        let plaintext = b"secret data";
        let mut ct = encrypt(&key, &nonce, &[], plaintext).expect("encrypt");

        // Flip a tag bit.
        ct.tag[0] ^= 0x01;

        let err = decrypt(&key, &nonce, &[], &ct.ciphertext, &ct.tag).expect_err("must fail");
        assert!(matches!(err, AesGcmError::TagMismatch));
    }

    #[test]
    fn wrong_key_is_detected_via_tag_mismatch() {
        let key1 = [0x11u8; KEY_LEN];
        let key2 = [0x22u8; KEY_LEN];
        let nonce = [0x33u8; NONCE_LEN];
        let plaintext = b"secret";
        let ct = encrypt(&key1, &nonce, &[], plaintext).expect("encrypt");
        let err = decrypt(&key2, &nonce, &[], &ct.ciphertext, &ct.tag).expect_err("must fail");
        assert!(matches!(err, AesGcmError::TagMismatch));
    }

    #[test]
    fn ciphertext_tamper_is_detected() {
        let key = [0xcdu8; KEY_LEN];
        let nonce = [0xefu8; NONCE_LEN];
        let plaintext = b"important payload";
        let mut ct = encrypt(&key, &nonce, &[], plaintext).expect("encrypt");
        ct.ciphertext[0] ^= 0x01;
        let err = decrypt(&key, &nonce, &[], &ct.ciphertext, &ct.tag).expect_err("must fail");
        assert!(matches!(err, AesGcmError::TagMismatch));
    }

    #[test]
    fn random_bytes_fills_buffer() {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        random_bytes(&mut a).expect("rand a");
        random_bytes(&mut b).expect("rand b");
        // Two independent draws of 32 bytes are essentially guaranteed to
        // differ; a collision implies a defective RNG (probability ≈ 2^-256).
        assert_ne!(a, b);
        // Sanity: not all zero.
        assert!(a.iter().any(|&x| x != 0));
    }
}
