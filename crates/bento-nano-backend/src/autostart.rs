//! Mc-3 #12 — REAL Windows autostart via `HKCU\...\CurrentVersion\Run`.
//!
//! Before this module the Settings "run at startup" toggle was a functional
//! lie: it only flipped an in-memory `Cell` + marked settings dirty. It never
//! touched the registry, so enabling it did nothing — the app did not start
//! with Windows, and the displayed state reflected the persisted settings
//! mirror rather than reality.
//!
//! This module owns the real registration. The standard per-user autostart
//! location is the `Run` key under `HKEY_CURRENT_USER`:
//!
//! ```text
//! HKCU\Software\Microsoft\Windows\CurrentVersion\Run
//!   BentoDesk = "<quoted current_exe path>"
//! ```
//!
//! Per-user (`HKCU`) requires no elevation. The registry is the single source
//! of truth: [`is_enabled`] reads it back so the toggle can never lie again.
//!
//! ## Porting / spec notes
//!
//! - **Spec §8** — zero new crates. Reuses the already-enabled
//!   `Win32_System_Registry` + `Win32_Foundation` `windows-sys` features. The
//!   read side mirrors [`crate::system`]'s `RegGetValueW` idiom; this module
//!   adds the write/delete side (`RegCreateKeyExW` / `RegSetValueExW` /
//!   `RegOpenKeyExW` / `RegDeleteValueW`).
//! - **Spec §11** — no `unwrap`/`expect`/`panic` outside `cfg(test)`. Every
//!   `current_exe()` failure → [`AutostartError::NoExePath`]; every non-success
//!   `LSTATUS` → [`AutostartError::Registry`] (except the tolerated
//!   `ERROR_FILE_NOT_FOUND` when deleting an already-absent value).
//! - **Spec §11.1** — every `unsafe` block carries a `// SAFETY:` comment.

/// Registry value name written under the `Run` key. Identifies our entry.
const VALUE_NAME: &str = "BentoDesk";
/// Pre-2.0 native value retained as a read/delete compatibility alias.
const LEGACY_VALUE_NAME: &str = "BentoDeskNano";

/// The per-user autostart subkey, relative to `HKEY_CURRENT_USER`.
const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Errors surfaced by the autostart helpers. Hand-rolled per spec §8.1 / §11.
#[derive(Debug)]
pub enum AutostartError {
    /// `std::env::current_exe()` failed, so there is no path to register.
    NoExePath,
    /// A `Reg*` call returned a non-success `LSTATUS` (Win32 error code).
    Registry(i32),
}

impl core::fmt::Display for AutostartError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoExePath => write!(f, "could not resolve the current executable path"),
            Self::Registry(code) => {
                write!(f, "registry operation failed (LSTATUS = {code})")
            }
        }
    }
}

impl core::error::Error for AutostartError {}

// ─── Public API ──────────────────────────────────────────────────────────

/// Enable or disable launch-at-startup for the current user.
///
/// When `enabled`, registers the current executable (quoted, so a path with
/// spaces survives the shell parse Windows performs on the `Run` value) under
/// the `Run` key. When disabled, removes the value (tolerating an already-
/// absent entry as success).
#[cfg(windows)]
pub fn set_enabled(enabled: bool) -> Result<(), AutostartError> {
    if enabled {
        let exe = std::env::current_exe().map_err(|_| AutostartError::NoExePath)?;
        // Quote the path so embedded spaces are preserved when Windows parses
        // the Run value as a command line.
        let quoted = format!("\"{}\"", exe.display());
        run_key_set(VALUE_NAME, &quoted)?;
        let _ = run_key_delete(LEGACY_VALUE_NAME);
        Ok(())
    } else {
        let current = run_key_delete(VALUE_NAME);
        let legacy = run_key_delete(LEGACY_VALUE_NAME);
        current.and(legacy)
    }
}

/// Returns `true` when our autostart value exists under the `Run` key. This is
/// the real source of truth — it reads the registry rather than any persisted
/// settings mirror.
#[cfg(windows)]
pub fn is_enabled() -> bool {
    run_key_exists(VALUE_NAME) || run_key_exists(LEGACY_VALUE_NAME)
}

#[cfg(not(windows))]
pub fn set_enabled(_enabled: bool) -> Result<(), AutostartError> {
    Err(AutostartError::NoExePath)
}

#[cfg(not(windows))]
pub fn is_enabled() -> bool {
    false
}

// ─── Private Win32 helpers ─────────────────────────────────────────────────

/// Encode a Rust `&str` as a NUL-terminated UTF-16 buffer for the `*W` APIs.
#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(core::iter::once(0)).collect()
}

/// Create-or-open `HKCU\...\Run` and write `name = value` as a `REG_SZ`.
///
/// `RegCreateKeyExW` opens the key if it exists or creates it otherwise (the
/// `Run` key always exists on a healthy install, but creating it is harmless
/// and matches the documented idempotent pattern). The value data is the
/// UTF-16 string plus its trailing NUL; `cbData` is the byte count of that
/// buffer (u16 count × 2), as `RegSetValueExW` documents for `REG_SZ`.
#[cfg(windows)]
fn run_key_set(name: &str, value: &str) -> Result<(), AutostartError> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
        RegCloseKey, RegCreateKeyExW, RegSetValueExW,
    };

    let subkey = wide(RUN_SUBKEY);
    let value_name = wide(name);
    let data: Vec<u16> = value.encode_utf16().chain(core::iter::once(0)).collect();
    let cb_data = (data.len() * 2) as u32;

    let mut hkey: HKEY = core::ptr::null_mut();

    // SAFETY: `subkey` is a valid NUL-terminated wide string. We pass null for
    // the optional class / security-attributes / disposition out-params. On
    // success `hkey` receives an owned key handle that we close below; on
    // failure it is left null and `RegCloseKey(null)` is not called.
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            core::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            core::ptr::null(),
            &mut hkey,
            core::ptr::null_mut(),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(AutostartError::Registry(status as i32));
    }

    // SAFETY: `hkey` is a valid open key with KEY_SET_VALUE access. `value_name`
    // is a valid NUL-terminated wide string. `data` is a valid UTF-16 buffer of
    // `cb_data` bytes (including the trailing NUL), matching the REG_SZ contract.
    let set_status = unsafe {
        RegSetValueExW(
            hkey,
            value_name.as_ptr(),
            0,
            REG_SZ,
            data.as_ptr().cast(),
            cb_data,
        )
    };

    // SAFETY: `hkey` was returned by RegCreateKeyExW and is not used afterwards.
    unsafe {
        RegCloseKey(hkey);
    }

    if set_status != ERROR_SUCCESS {
        return Err(AutostartError::Registry(set_status as i32));
    }
    Ok(())
}

/// Delete `name` from `HKCU\...\Run`. An already-absent value
/// (`ERROR_FILE_NOT_FOUND`) is treated as success — the desired end state
/// (no autostart entry) already holds.
#[cfg(windows)]
fn run_key_delete(name: &str) -> Result<(), AutostartError> {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, RegCloseKey, RegDeleteValueW, RegOpenKeyExW,
    };

    let subkey = wide(RUN_SUBKEY);
    let value_name = wide(name);

    let mut hkey: HKEY = core::ptr::null_mut();

    // SAFETY: `subkey` is a valid NUL-terminated wide string. On success `hkey`
    // receives an owned handle closed below. A missing Run key (unlikely) is
    // surfaced as a Registry error, not silently swallowed.
    let open_status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        )
    };
    if open_status == ERROR_FILE_NOT_FOUND {
        // The whole Run key is absent → the value cannot exist → already gone.
        return Ok(());
    }
    if open_status != ERROR_SUCCESS {
        return Err(AutostartError::Registry(open_status as i32));
    }

    // SAFETY: `hkey` is a valid open key with KEY_SET_VALUE access and
    // `value_name` is a valid NUL-terminated wide string.
    let del_status = unsafe { RegDeleteValueW(hkey, value_name.as_ptr()) };

    // SAFETY: `hkey` was returned by RegOpenKeyExW and is not used afterwards.
    unsafe {
        RegCloseKey(hkey);
    }

    if del_status == ERROR_SUCCESS || del_status == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(AutostartError::Registry(del_status as i32))
    }
}

/// Returns `true` when `name` exists under `HKCU\...\Run` as a `REG_SZ`.
///
/// Mirrors the [`crate::system`] `RegGetValueW` call shape but against
/// `HKEY_CURRENT_USER` + the `Run` subkey. Passing null for the type / data /
/// size out-params performs an existence-only probe.
#[cfg(windows)]
fn run_key_exists(name: &str) -> bool {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};

    let subkey = wide(RUN_SUBKEY);
    let value_name = wide(name);

    // SAFETY: `subkey` and `value_name` are valid NUL-terminated wide strings.
    // Passing null for `pdwtype`, `pvdata`, and `pcbdata` performs a presence
    // check without copying the value out (documented as legal). The function
    // returns ERROR_SUCCESS only when a REG_SZ value with that name exists.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value_name.as_ptr(),
            RRF_RT_REG_SZ,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    };

    status == ERROR_SUCCESS
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// A deliberately unique value name so the round-trip NEVER touches the
    /// real `"BentoDeskNano"` autostart entry and cannot leave a stray
    /// registration behind even if an assert trips mid-test.
    const TEST_VALUE_NAME: &str = "BentoDeskNano__unit_test_marker__";
    /// Separate marker for the absent-delete test. The Rust test harness runs
    /// tests concurrently; sharing `TEST_VALUE_NAME` let this test delete the
    /// round-trip test's value between its write and read.
    const TEST_ABSENT_VALUE_NAME: &str = "BentoDeskNano__unit_test_absent_marker__";

    /// Round-trips the private helpers with a dummy value. This exercises the
    /// real write/read/delete path against `HKCU\...\Run` WITHOUT registering
    /// the test binary (we never call the public `set_enabled(true)`, which
    /// would write `current_exe()` — i.e. the test runner — under the real
    /// value name).
    #[test]
    fn round_trip_under_unique_value_name() {
        // Start clean in case a previous aborted run left the marker behind.
        let _ = run_key_delete(TEST_VALUE_NAME);

        run_key_set(TEST_VALUE_NAME, "test").expect("set test marker");
        assert!(
            run_key_exists(TEST_VALUE_NAME),
            "marker must exist after set"
        );

        run_key_delete(TEST_VALUE_NAME).expect("delete test marker");
        assert!(
            !run_key_exists(TEST_VALUE_NAME),
            "marker must be gone after delete"
        );
    }

    /// Deleting an already-absent value is success, not an error.
    #[test]
    fn delete_absent_is_ok() {
        // Ensure absent first (no-op if it never existed).
        let _ = run_key_delete(TEST_ABSENT_VALUE_NAME);
        run_key_delete(TEST_ABSENT_VALUE_NAME).expect("deleting an absent value is success");
    }
}
