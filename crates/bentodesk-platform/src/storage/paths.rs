//! State-root resolution, portable-mode marker, and corrupt-file quarantine.

use super::*;

/// Resolve `%APPDATA%\BentoDesk\zones.bin`.
///
/// If `BENTODESK_STATE_DIR` is set to a non-empty path, that directory is used
/// instead, allowing diagnostics and hand tests to avoid mutating the user's
/// real BentoDesk data.
///
/// Calls `SHGetKnownFolderPath(FOLDERID_RoamingAppData)`. The directory may
/// not exist yet — `read_zones` treats that the same as an absent file
/// (returns an empty list); `write_zones_atomic` creates it on demand.
pub fn appdata_path() -> Result<PathBuf, PlatformError> {
    if let Some(path) = state_dir_override_path() {
        return Ok(path);
    }

    let mut path = state_dir_for_portable_mode(portable_mode_enabled())?;
    path.push("zones.bin");
    Ok(path)
}

/// Return whether the next normal launch should use executable-local storage.
///
/// The marker is intentionally the source of truth rather than a value inside
/// `vault.bin`: the storage root must be selected before that vault can be
/// opened. Runtime-proof overrides keep their own marker under the override
/// directory and still retain absolute precedence in [`appdata_path`].
pub fn portable_mode_enabled() -> bool {
    portable_marker_path()
        .map(|path| path.is_file())
        .unwrap_or(false)
}

/// Enable or disable portable mode for the next process launch.
pub fn set_portable_mode_enabled(enabled: bool) -> Result<(), PlatformError> {
    let marker = portable_marker_path()?;
    if enabled {
        let Some(parent) = marker.parent() else {
            return Err(PlatformError::Storage("portable marker has no parent"));
        };
        fs::create_dir_all(parent).map_err(|error| PlatformError::StorageIo {
            ctx: "create portable marker parent",
            kind: error.kind(),
        })?;
        fs::write(&marker, b"BentoDesk portable mode\n").map_err(|error| PlatformError::StorageIo {
            ctx: "write portable marker",
            kind: error.kind(),
        })
    } else {
        match fs::remove_file(&marker) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(PlatformError::StorageIo {
                ctx: "remove portable marker",
                kind: error.kind(),
            }),
        }
    }
}

/// Resolve the state directory for a requested portable-mode value.
///
/// Isolated proof state always wins. Production portable state lives beside
/// the executable; normal state remains `%APPDATA%\BentoDesk`.
pub fn state_dir_for_portable_mode(portable: bool) -> Result<PathBuf, PlatformError> {
    if let Some(mut path) = state_dir_override_path() {
        let _ = path.pop();
        return Ok(path);
    }
    if portable {
        portable_state_dir()
    } else {
        roaming_state_dir()
    }
}

fn portable_marker_path() -> Result<PathBuf, PlatformError> {
    if let Some(mut path) = state_dir_override_path() {
        let _ = path.pop();
        path.push(PORTABLE_MARKER_FILE);
        return Ok(path);
    }
    let exe = std::env::current_exe().map_err(|error| PlatformError::StorageIo {
        ctx: "resolve executable for portable marker",
        kind: error.kind(),
    })?;
    let Some(parent) = exe.parent() else {
        return Err(PlatformError::Storage("executable has no parent directory"));
    };
    Ok(parent.join(PORTABLE_MARKER_FILE))
}

fn portable_state_dir() -> Result<PathBuf, PlatformError> {
    let exe = std::env::current_exe().map_err(|error| PlatformError::StorageIo {
        ctx: "resolve executable for portable state",
        kind: error.kind(),
    })?;
    let Some(parent) = exe.parent() else {
        return Err(PlatformError::Storage("executable has no parent directory"));
    };
    Ok(parent.join(PORTABLE_DATA_DIR_NAME))
}

fn roaming_state_dir() -> Result<PathBuf, PlatformError> {
    use windows_sys::Win32::Foundation::S_OK;
    use windows_sys::Win32::UI::Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath};

    let mut raw: *mut u16 = core::ptr::null_mut();
    // SAFETY: SHGetKnownFolderPath canonical signature; `raw` written on
    // success and we free it before returning. KF_FLAG_DEFAULT = 0.
    let hr = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_RoamingAppData as *const _,
            0,
            core::ptr::null_mut(),
            &mut raw,
        )
    };
    if hr != S_OK {
        // Phase 2.2 / Ruling 3c — explicitly skip CoTaskMemFree on the
        // error path. The MS contract says SHGetKnownFolderPath may still
        // hand back an allocation alongside a non-S_OK HRESULT; on the
        // documented success-only path `raw` is non-null and we free it
        // below. Releasing a NULL is benign per the spec, but matching
        // §11 enum-error policy means we never call CoTaskMemFree without
        // a real pointer (and if a future failure mode does leak a buffer
        // we'd rather log + investigate than silently free).
        if !raw.is_null() {
            // SAFETY: SHGetKnownFolderPath promises CoTaskMem-allocated.
            unsafe { windows_sys::Win32::System::Com::CoTaskMemFree(raw as *const _) };
        }
        return Err(PlatformError::Hresult {
            ctx: "SHGetKnownFolderPath",
            hr,
        });
    }
    if raw.is_null() {
        return Err(PlatformError::Null {
            ctx: "SHGetKnownFolderPath",
        });
    }

    // Walk the UTF-16 string to its NUL terminator.
    // SAFETY: pointer checked above; bounded by the OS-supplied NUL.
    let len = unsafe {
        let mut p = raw;
        let mut n = 0usize;
        while *p != 0 {
            n += 1;
            p = p.add(1);
        }
        n
    };
    // SAFETY: `raw` valid for `len` u16s by construction above.
    let slice: &[u16] = unsafe { core::slice::from_raw_parts(raw, len) };
    let s = String::from_utf16_lossy(slice);

    // SAFETY: free what SHGetKnownFolderPath allocated. Required by docs.
    unsafe { windows_sys::Win32::System::Com::CoTaskMemFree(raw as *const _) };

    let mut path = PathBuf::from(s);
    path.push("BentoDesk");
    Ok(path)
}

fn state_dir_override_path() -> Option<PathBuf> {
    std::env::var_os(STATE_DIR_ENV)
        .as_deref()
        .and_then(state_dir_override_path_from_value)
}

pub(super) fn state_dir_override_path_from_value(raw: &OsStr) -> Option<PathBuf> {
    let text = raw.to_string_lossy();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut path = PathBuf::from(trimmed);
    path.push("zones.bin");
    Some(path)
}

/// Quarantine a corrupt `zones.bin` by renaming it to
/// `zones.bin.corrupt-{millis}` so the user can recover it manually. Best
/// effort — failures are returned but Phase 2.1 callers ignore them
/// (Ruling A: never block the first frame on storage I/O).
pub fn quarantine_corrupt(path: &Path) -> Result<(), PlatformError> {
    if !path.exists() {
        return Ok(());
    }
    let parent = match path.parent() {
        Some(p) => p,
        None => return Err(PlatformError::Storage("path has no parent")),
    };
    let stem = match path.file_name() {
        Some(s) => s.to_string_lossy().into_owned(),
        None => return Err(PlatformError::Storage("path has no file name")),
    };
    // GetSystemTimeAsFileTime gives a monotonic, lock-step millisecond
    // counter without pulling in chrono (forbidden) or `std::time::Instant`
    // (which is monotonic but lacks a wall-clock cast). Plain `SystemTime`
    // is fine — quarantine names are advisory, not load-bearing.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut new_name = String::with_capacity(stem.len() + 24);
    new_name.push_str(&stem);
    new_name.push_str(".corrupt-");
    let _ = core::fmt::Write::write_fmt(&mut new_name, format_args!("{stamp}"));
    let target = parent.join(new_name);
    match fs::rename(path, &target) {
        Ok(()) => Ok(()),
        Err(e) => Err(PlatformError::StorageIo {
            ctx: "rename to quarantine",
            kind: e.kind(),
        }),
    }
}
