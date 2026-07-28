//! Bounded WinHTTP transport used by updater manifest and artifact downloads.

use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ParsedHttpUrl {
    pub(super) secure: bool,
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) path: String,
}

pub(super) fn parse_http_url(source: &str) -> Result<ParsedHttpUrl, UpdaterError> {
    let (secure, rest) = if let Some(rest) = source.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = source.strip_prefix("http://") {
        (false, rest)
    } else {
        return Err(UpdaterError::UnsupportedManifestSource(source.to_owned()));
    };
    let (host_port, path_tail) = rest.split_once('/').unwrap_or((rest, ""));
    if host_port.is_empty() {
        return Err(UpdaterError::UnsupportedManifestSource(source.to_owned()));
    }
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port_text)) if !host.contains(':') => {
            let port = port_text
                .parse::<u16>()
                .map_err(|_| UpdaterError::UnsupportedManifestSource(source.to_owned()))?;
            (host, port)
        }
        _ => (host_port, if secure { 443 } else { 80 }),
    };
    if host.is_empty() {
        return Err(UpdaterError::UnsupportedManifestSource(source.to_owned()));
    }
    Ok(ParsedHttpUrl {
        secure,
        host: host.to_owned(),
        port,
        path: format!("/{path_tail}"),
    })
}

pub(super) fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(core::iter::once(0)).collect()
}

#[cfg(windows)]
pub(super) struct WinHttpHandle(*mut core::ffi::c_void);

#[cfg(windows)]
impl Drop for WinHttpHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Networking::WinHttp::WinHttpCloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
pub(super) struct WinHttpRequest {
    _session: WinHttpHandle,
    _connect: WinHttpHandle,
    request: WinHttpHandle,
}

#[cfg(windows)]
pub(super) fn winhttp_last_error(context: &str) -> UpdaterError {
    use windows_sys::Win32::Foundation::GetLastError;

    let code = unsafe { GetLastError() };
    UpdaterError::FetchFailed(format!("{context} failed (GetLastError={code})"))
}

#[cfg(windows)]
pub(super) fn open_winhttp_get(source: &str) -> Result<WinHttpRequest, UpdaterError> {
    use windows_sys::Win32::Networking::WinHttp::{
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
        WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2, WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3,
        WINHTTP_OPTION_SECURE_PROTOCOLS, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
        WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
        WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts,
    };

    let parsed = parse_http_url(source)?;
    let agent = wide_null("BentoDesk Updater");
    let session = unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            ptr::null(),
            ptr::null(),
            0,
        )
    };
    if session.is_null() {
        return Err(winhttp_last_error("WinHttpOpen"));
    }
    let session = WinHttpHandle(session);

    // Mc-3 #16: pin TLS 1.2|1.3 on the session handle. On Win8.1 and
    // Win10 < 1709 WinHTTP defaults to TLS 1.0/1.1, which modern update
    // servers (e.g. GitHub releases) reject — the handshake would silently
    // fail. TLS1.2|TLS1.3 is forward+backward safe: OSes lacking 1.3 ignore
    // the unknown bit and negotiate 1.2; older OSes have 1.2 enabled by this
    // option. Best-effort: a non-zero failure only loses the hardening on
    // OSes that already default to 1.2, so we swallow it rather than abort.
    let protocols: u32 = WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3;
    if unsafe {
        WinHttpSetOption(
            session.0,
            WINHTTP_OPTION_SECURE_PROTOCOLS,
            &protocols as *const u32 as *const core::ffi::c_void,
            core::mem::size_of::<u32>() as u32,
        )
    } == 0
    {
        tracing::warn!("updater: WinHttpSetOption(SECURE_PROTOCOLS) failed, using OS default TLS");
    }

    // Mc-3 #16: bound every WinHTTP phase so a black-holed connection
    // (captive portal, dead proxy, firewall drop) can never hang the request
    // thread indefinitely. Milliseconds; resolve must be > 0 (0 = no timeout).
    if unsafe { WinHttpSetTimeouts(session.0, 10_000, 15_000, 15_000, 30_000) } == 0 {
        tracing::warn!("updater: WinHttpSetTimeouts failed, using OS default timeouts");
    }

    let host = wide_null(&parsed.host);
    let connect = unsafe { WinHttpConnect(session.0, host.as_ptr(), parsed.port, 0) };
    if connect.is_null() {
        return Err(winhttp_last_error("WinHttpConnect"));
    }
    let connect = WinHttpHandle(connect);

    let verb = wide_null("GET");
    let path = wide_null(&parsed.path);
    let flags = if parsed.secure {
        WINHTTP_FLAG_SECURE
    } else {
        0
    };
    let request = unsafe {
        WinHttpOpenRequest(
            connect.0,
            verb.as_ptr(),
            path.as_ptr(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            flags,
        )
    };
    if request.is_null() {
        return Err(winhttp_last_error("WinHttpOpenRequest"));
    }
    let request = WinHttpHandle(request);

    if unsafe { WinHttpSendRequest(request.0, ptr::null(), 0, ptr::null(), 0, 0, 0) } == 0 {
        return Err(winhttp_last_error("WinHttpSendRequest"));
    }
    if unsafe { WinHttpReceiveResponse(request.0, ptr::null_mut()) } == 0 {
        return Err(winhttp_last_error("WinHttpReceiveResponse"));
    }

    let mut status_code = 0u32;
    let mut status_len = core::mem::size_of::<u32>() as u32;
    let mut status_index = 0u32;
    if unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(),
            &mut status_code as *mut u32 as *mut _,
            &mut status_len,
            &mut status_index,
        )
    } == 0
    {
        return Err(winhttp_last_error("WinHttpQueryHeaders"));
    }
    if !(200..300).contains(&status_code) {
        return Err(UpdaterError::FetchFailed(format!(
            "{source} returned HTTP {status_code}"
        )));
    }

    Ok(WinHttpRequest {
        _session: session,
        _connect: connect,
        request,
    })
}

#[cfg(windows)]
pub(super) fn winhttp_content_length(request: &WinHttpRequest) -> Option<u64> {
    use windows_sys::Win32::Networking::WinHttp::{
        WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_FLAG_NUMBER, WinHttpQueryHeaders,
    };

    let mut content_length = 0u32;
    let mut content_len = core::mem::size_of::<u32>() as u32;
    let mut content_index = 0u32;
    if unsafe {
        WinHttpQueryHeaders(
            request.request.0,
            WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(),
            &mut content_length as *mut u32 as *mut _,
            &mut content_len,
            &mut content_index,
        )
    } == 0
    {
        return None;
    }
    Some(u64::from(content_length))
}

#[cfg(windows)]
pub(super) fn fetch_manifest_winhttp(source: &str) -> Result<String, UpdaterError> {
    use windows_sys::Win32::Networking::WinHttp::{WinHttpQueryDataAvailable, WinHttpReadData};

    let request = open_winhttp_get(source)?;
    let mut bytes = Vec::<u8>::new();
    loop {
        let mut available = 0u32;
        if unsafe { WinHttpQueryDataAvailable(request.request.0, &mut available) } == 0 {
            return Err(winhttp_last_error("WinHttpQueryDataAvailable"));
        }
        if available == 0 {
            break;
        }
        let next_len = bytes.len().saturating_add(available as usize);
        if next_len > MAX_MANIFEST_BYTES {
            return Err(UpdaterError::FetchFailed(format!(
                "manifest exceeds {MAX_MANIFEST_BYTES} bytes"
            )));
        }
        let start = bytes.len();
        bytes.resize(next_len, 0);
        let mut read = 0u32;
        if unsafe {
            WinHttpReadData(
                request.request.0,
                bytes[start..].as_mut_ptr() as *mut _,
                available,
                &mut read,
            )
        } == 0
        {
            return Err(winhttp_last_error("WinHttpReadData"));
        }
        bytes.truncate(start + read as usize);
        if read == 0 {
            break;
        }
    }
    String::from_utf8(bytes)
        .map_err(|error| UpdaterError::InvalidManifest(format!("manifest is not UTF-8: {error}")))
}

#[cfg(not(windows))]
pub(super) fn fetch_manifest_winhttp(source: &str) -> Result<String, UpdaterError> {
    Err(UpdaterError::UnsupportedManifestSource(source.to_owned()))
}

#[cfg(windows)]
pub(super) fn copy_http_artifact_to_stage_winhttp(
    source: &str,
    stage_path: &Path,
    event_tx: &Sender<UpdateEvent>,
) -> Result<(), UpdaterError> {
    use windows_sys::Win32::Networking::WinHttp::WinHttpReadData;

    let request = open_winhttp_get(source)?;
    let total_bytes = winhttp_content_length(&request);
    let mut output = File::create(stage_path)
        .map_err(|error| UpdaterError::FetchFailed(format!("{}: {error}", stage_path.display())))?;
    let mut buffer = [0u8; DOWNLOAD_BUFFER_BYTES];
    let mut written = 0u64;
    loop {
        let mut read = 0u32;
        if unsafe {
            WinHttpReadData(
                request.request.0,
                buffer.as_mut_ptr() as *mut _,
                buffer.len() as u32,
                &mut read,
            )
        } == 0
        {
            let _ = std::fs::remove_file(stage_path);
            return Err(winhttp_last_error("WinHttpReadData"));
        }
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read as usize])
            .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
        written = written.saturating_add(u64::from(read));
        emit_download_progress(event_tx, written, total_bytes)?;
    }
    output
        .flush()
        .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn copy_http_artifact_to_stage_winhttp(
    source: &str,
    _stage_path: &Path,
    _event_tx: &Sender<UpdateEvent>,
) -> Result<(), UpdaterError> {
    Err(UpdaterError::UnsupportedManifestSource(source.to_owned()))
}
