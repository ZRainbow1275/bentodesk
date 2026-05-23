//! Data Protection API (DPAPI) wrapper — `CryptProtectData` / `CryptUnprotectData`.
//!
//! DPAPI is the per-user, per-machine system-key encryption Windows ships
//! out of the box. Q2=C ruling makes this the default `EncryptionMode` —
//! zero new crates and no passphrase prompt at startup. The trade-off is
//! per-machine binding (an exported `settings.vault` cannot be read on
//! another machine), which is the right default for desktop-organizer state.

#[cfg(windows)]
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CryptProtectData, CryptUnprotectData,
};

/// `CRYPTPROTECT_UI_FORBIDDEN = 0x1` — never display a UI prompt; fail
/// instead. nano runs background flushes that must not block on user input.
#[cfg(windows)]
const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

/// Description string passed as `szDataDescr`. Visible if a user runs
/// `CryptUnprotectData` from another tool, so identify the artefact.
#[cfg(windows)]
const DPAPI_DESCRIPTION: windows_sys::core::PCWSTR = windows_sys::core::w!("BentoDesk-Nano-Vault");

/// Errors surfaced by the DPAPI helpers. Hand-rolled per spec §8.1.
#[derive(Debug)]
pub enum DpapiError {
    /// `CryptProtectData` returned `FALSE`. Win32 last-error captured.
    Protect { last_error: u32 },
    /// `CryptUnprotectData` returned `FALSE`. Last-error captured.
    Unprotect { last_error: u32 },
}

impl core::fmt::Display for DpapiError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Protect { last_error } => {
                write!(
                    f,
                    "CryptProtectData failed (GetLastError = {last_error:#x})"
                )
            }
            Self::Unprotect { last_error } => {
                write!(
                    f,
                    "CryptUnprotectData failed (GetLastError = {last_error:#x})"
                )
            }
        }
    }
}

impl core::error::Error for DpapiError {}

/// Encrypt `plaintext` via DPAPI. Returns the opaque DPAPI blob — its
/// internal layout is undocumented and version-dependent; treat as a
/// black box and persist verbatim.
#[cfg(windows)]
pub fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>, DpapiError> {
    use windows_sys::Win32::Foundation::GetLastError;

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: plaintext.len() as u32,
        pbData: plaintext.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: core::ptr::null_mut(),
    };

    // SAFETY: `input` describes the caller's plaintext slice (read-only;
    // CryptProtectData does not mutate it despite the *mut signature).
    // `output.pbData` is set by DPAPI to a LocalAlloc'd buffer that we
    // copy out + free immediately afterwards.
    let ok = unsafe {
        CryptProtectData(
            &input,
            DPAPI_DESCRIPTION,
            core::ptr::null(),
            core::ptr::null(),
            core::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    let _ = &mut input; // silence unused-mut lint
    if ok == 0 {
        // SAFETY: GetLastError has no preconditions.
        let err = unsafe { GetLastError() };
        return Err(DpapiError::Protect { last_error: err });
    }

    // Copy DPAPI's output into a Rust Vec, then free the LocalAlloc.
    let len = output.cbData as usize;
    let mut blob = Vec::with_capacity(len);
    if !output.pbData.is_null() && len > 0 {
        // SAFETY: DPAPI guarantees `pbData` points to `cbData` valid bytes;
        // we read once and copy out.
        unsafe {
            blob.extend_from_slice(core::slice::from_raw_parts(output.pbData, len));
        }
    }
    free_dpapi_blob(&mut output);
    Ok(blob)
}

/// Decrypt a DPAPI blob produced by [`encrypt`].
#[cfg(windows)]
pub fn decrypt(ciphertext: &[u8]) -> Result<Vec<u8>, DpapiError> {
    use windows_sys::Win32::Foundation::GetLastError;

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: ciphertext.len() as u32,
        pbData: ciphertext.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: core::ptr::null_mut(),
    };
    let mut descr_out: windows_sys::core::PWSTR = core::ptr::null_mut();

    // SAFETY: `input` describes the caller's read-only ciphertext slice.
    // `descr_out` and `output.pbData` are populated by DPAPI with
    // LocalAlloc'd buffers we free below.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            &mut descr_out,
            core::ptr::null(),
            core::ptr::null(),
            core::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    let _ = &mut input;

    // Free the DPAPI-allocated description string regardless of success.
    if !descr_out.is_null() {
        // SAFETY: descr_out is a PWSTR owned by DPAPI's LocalAlloc.
        unsafe {
            local_free(descr_out as *mut core::ffi::c_void);
        }
    }

    if ok == 0 {
        // SAFETY: GetLastError has no preconditions.
        let err = unsafe { GetLastError() };
        return Err(DpapiError::Unprotect { last_error: err });
    }

    let len = output.cbData as usize;
    let mut plaintext = Vec::with_capacity(len);
    if !output.pbData.is_null() && len > 0 {
        // SAFETY: DPAPI guarantees `pbData` points to `cbData` valid bytes.
        unsafe {
            plaintext.extend_from_slice(core::slice::from_raw_parts(output.pbData, len));
        }
    }
    free_dpapi_blob(&mut output);
    Ok(plaintext)
}

#[cfg(windows)]
fn free_dpapi_blob(blob: &mut CRYPT_INTEGER_BLOB) {
    if !blob.pbData.is_null() {
        // SAFETY: DPAPI's CryptProtectData / CryptUnprotectData document that
        // the caller frees `pbData` via LocalFree.
        unsafe {
            local_free(blob.pbData as *mut core::ffi::c_void);
        }
        blob.pbData = core::ptr::null_mut();
        blob.cbData = 0;
    }
}

#[cfg(windows)]
unsafe fn local_free(p: *mut core::ffi::c_void) {
    use windows_sys::Win32::Foundation::LocalFree;
    // SAFETY: caller guarantees `p` was allocated by LocalAlloc (DPAPI does
    // this for both pbData and the description PWSTR). LocalFree(NULL) is
    // documented as a no-op.
    unsafe {
        LocalFree(p as *mut _);
    }
}

#[cfg(not(windows))]
pub fn encrypt(_plaintext: &[u8]) -> Result<Vec<u8>, DpapiError> {
    Err(DpapiError::Protect {
        last_error: 0xC000_0001,
    })
}

#[cfg(not(windows))]
pub fn decrypt(_ciphertext: &[u8]) -> Result<Vec<u8>, DpapiError> {
    Err(DpapiError::Unprotect {
        last_error: 0xC000_0001,
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn round_trip_short_plaintext() {
        let plaintext = b"some user setting";
        let blob = encrypt(plaintext).expect("dpapi encrypt");
        // The DPAPI blob is opaque but always strictly larger than the input
        // (header + IV + tag).
        assert!(blob.len() > plaintext.len(), "got {} bytes", blob.len());
        let decoded = decrypt(&blob).expect("dpapi decrypt");
        assert_eq!(decoded, plaintext);
    }

    #[test]
    fn round_trip_empty_plaintext() {
        let blob = encrypt(&[]).expect("dpapi encrypt empty");
        let decoded = decrypt(&blob).expect("dpapi decrypt empty");
        assert!(decoded.is_empty());
    }

    #[test]
    fn corrupt_blob_is_rejected() {
        let mut blob = encrypt(b"data").expect("encrypt");
        // Corrupt mid-blob (after the header) — DPAPI integrity check fails.
        let mid = blob.len() / 2;
        blob[mid] ^= 0xff;
        let err = decrypt(&blob).expect_err("must fail");
        assert!(matches!(err, DpapiError::Unprotect { .. }));
    }
}
