//! Offline-safe metadata reads for Windows Shell shortcuts.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IPersistFile, STGM,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{Interface, PCWSTR};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkIconLocation {
    pub path: String,
    pub index: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkMetadata {
    pub target: Option<String>,
    pub icon: Option<LinkIconLocation>,
    pub blocked_reference: bool,
}

fn utf16_string(buffer: &[u16]) -> String {
    let len = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len])
}

/// Read stored target and icon paths without resolving or opening either one.
/// The local `.lnk` file is the only path touched before both references pass
/// BentoDesk's shared automatic-access guard.
pub(crate) fn read_link_metadata(lnk_path: &str) -> Option<LinkMetadata> {
    if crate::path_may_access_network(Path::new(lnk_path)) {
        return None;
    }

    // SAFETY: only a successful initialization owned by this thread is
    // balanced below. An existing apartment remains owned by its caller.
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let result = (|| -> Option<LinkMetadata> {
        // SAFETY: ShellLink is a registered in-process COM class.
        let link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.ok()?;
        let persist: IPersistFile = Interface::cast(&link).ok()?;
        let wide_path: Vec<u16> = OsStr::new(lnk_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: the local shortcut path is NUL-terminated and loaded read-only.
        unsafe { persist.Load(PCWSTR(wide_path.as_ptr()), STGM(0)) }.ok()?;

        let mut target_buffer = [0u16; 32_768];
        // GetPath does not call IShellLink::Resolve; it only returns stored data.
        let target = unsafe { link.GetPath(&mut target_buffer, std::ptr::null_mut(), 0) }
            .ok()
            .map(|()| utf16_string(&target_buffer))
            .filter(|value| !value.is_empty());

        let mut icon_buffer = [0u16; 32_768];
        let mut icon_index = 0;
        let icon_path = unsafe { link.GetIconLocation(&mut icon_buffer, &mut icon_index) }
            .ok()
            .map(|()| utf16_string(&icon_buffer))
            .filter(|value| !value.is_empty());

        let target_blocked = target
            .as_deref()
            .is_some_and(|value| crate::path_may_access_network(Path::new(value)));
        let icon_blocked = icon_path
            .as_deref()
            .is_some_and(|value| crate::path_may_access_network(Path::new(value)));
        let safe_target = target
            .filter(|_| !target_blocked)
            .filter(|value| Path::new(value).exists());
        let safe_icon = icon_path
            .filter(|_| !icon_blocked)
            .map(|path| LinkIconLocation {
                path,
                index: icon_index,
            });

        Some(LinkMetadata {
            target: safe_target,
            icon: safe_icon,
            blocked_reference: target_blocked || icon_blocked,
        })
    })();

    if initialized {
        // SAFETY: balances this thread's successful CoInitializeEx.
        unsafe { CoUninitialize() };
    }
    result
}

pub fn resolve_lnk_target(lnk_path: &str) -> Option<String> {
    read_link_metadata(lnk_path)?.target
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::BOOL;

    #[test]
    fn local_link_rejects_embedded_unc_before_icon_extraction() {
        // SAFETY: balance only the apartment initialized by this test thread.
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        let path = std::env::temp_dir().join(format!(
            "bentodesk-network-reference-{}.lnk",
            std::process::id()
        ));
        let wide_link: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let target: Vec<u16> = OsStr::new(r"\\server\share\app.exe")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let icon: Vec<u16> = OsStr::new(r"\\server\share\icon.dll")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let result = (|| -> windows::core::Result<()> {
            // SAFETY: ShellLink is a registered in-process COM class.
            let link: IShellLinkW =
                unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }?;
            unsafe {
                link.SetPath(PCWSTR(target.as_ptr()))?;
                link.SetIconLocation(PCWSTR(icon.as_ptr()), 0)?;
            }
            let persist: IPersistFile = Interface::cast(&link)?;
            // SAFETY: the destination is a live, NUL-terminated local path.
            unsafe { persist.Save(PCWSTR(wide_link.as_ptr()), BOOL::from(true)) }
        })();
        if initialized {
            // SAFETY: balances this thread's successful CoInitializeEx.
            unsafe { CoUninitialize() };
        }
        result.expect("write local shortcut fixture");

        let metadata = read_link_metadata(&path.to_string_lossy()).expect("shortcut metadata");
        assert!(metadata.blocked_reference);
        assert!(metadata.target.is_none());
        assert!(metadata.icon.is_none());
        assert!(matches!(
            crate::icon::extractor::extract_icon_png(&path.to_string_lossy()),
            Err(crate::icon::IconError::Io { message, .. })
                if message.contains("shortcut references")
        ));

        let _ = std::fs::remove_file(path);
    }
}
