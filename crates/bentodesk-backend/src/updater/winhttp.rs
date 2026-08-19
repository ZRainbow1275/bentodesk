//! Bounded HTTPS transport for updater metadata and artifacts.

use std::path::Path;
use std::time::Duration;

use crossbeam_channel::Sender;

use super::{UpdateEvent, UpdaterError};

#[cfg(windows)]
mod platform {
    use core::ffi::c_void;
    use std::fs::File;
    use std::io::Write;
    use std::ptr;
    use std::time::Instant;

    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Networking::WinHttp::{
        ERROR_WINHTTP_HEADER_NOT_FOUND, INTERNET_DEFAULT_HTTPS_PORT,
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPTION_REDIRECT_POLICY,
        WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP, WINHTTP_QUERY_CONTENT_LENGTH,
        WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_FLAG_NUMBER64, WINHTTP_QUERY_STATUS_CODE,
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
        WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption,
        WinHttpSetTimeouts,
    };

    use super::*;
    use crate::updater::DOWNLOAD_BUFFER_BYTES;
    use crate::updater::artifact::DownloadProgress;

    const RESOLVE_TIMEOUT_MS: i32 = 5_000;
    const CONNECT_TIMEOUT_MS: i32 = 10_000;
    const SEND_TIMEOUT_MS: i32 = 10_000;
    const RECEIVE_TIMEOUT_MS: i32 = 30_000;
    const REQUEST_HEADERS: &str = concat!(
        "Accept: application/vnd.github+json\r\n",
        "X-GitHub-Api-Version: 2022-11-28\r\n",
    );

    struct InternetHandle(*mut c_void);

    impl InternetHandle {
        fn new(handle: *mut c_void, operation: &str) -> Result<Self, UpdaterError> {
            if handle.is_null() {
                return Err(last_error(operation));
            }
            Ok(Self(handle))
        }
    }

    impl Drop for InternetHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: this wrapper exclusively owns a live WinHTTP handle
                // returned by WinHttpOpen/Connect/OpenRequest and closes it once.
                unsafe {
                    let _ = WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    struct Response {
        // Field order closes the request before its parent connection/session.
        request: InternetHandle,
        _connection: InternetHandle,
        _session: InternetHandle,
        content_length: Option<u64>,
        started: Instant,
    }

    impl Response {
        fn open(url: &str, deadline: Duration) -> Result<Self, UpdaterError> {
            let parsed = ParsedHttpsUrl::parse(url)?;
            let started = Instant::now();
            let agent = wide(concat!("BentoDesk/", env!("CARGO_PKG_VERSION")));
            // SAFETY: all UTF-16 inputs are live, NUL-terminated buffers. Null
            // proxy pointers select the system automatic-proxy configuration.
            let session = InternetHandle::new(
                unsafe {
                    WinHttpOpen(
                        agent.as_ptr(),
                        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                        ptr::null(),
                        ptr::null(),
                        0,
                    )
                },
                "WinHttpOpen",
            )?;
            // SAFETY: `session` is live and timeout values are finite milliseconds.
            if unsafe {
                WinHttpSetTimeouts(
                    session.0,
                    RESOLVE_TIMEOUT_MS,
                    CONNECT_TIMEOUT_MS,
                    SEND_TIMEOUT_MS,
                    RECEIVE_TIMEOUT_MS,
                )
            } == 0
            {
                return Err(last_error("WinHttpSetTimeouts"));
            }
            check_deadline(started, deadline)?;

            let host = wide(&parsed.host);
            // SAFETY: `session` is live and `host` is NUL-terminated for this call.
            let connection = InternetHandle::new(
                unsafe { WinHttpConnect(session.0, host.as_ptr(), parsed.port, 0) },
                "WinHttpConnect",
            )?;
            let method = wide("GET");
            let object = wide(&parsed.object);
            // SAFETY: parent connection and all string buffers are live. Null
            // optional strings select HTTP/1.1 defaults and no referrer/types.
            let request = InternetHandle::new(
                unsafe {
                    WinHttpOpenRequest(
                        connection.0,
                        method.as_ptr(),
                        object.as_ptr(),
                        ptr::null(),
                        ptr::null(),
                        ptr::null(),
                        WINHTTP_FLAG_SECURE,
                    )
                },
                "WinHttpOpenRequest",
            )?;
            let redirect_policy = WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP;
            // SAFETY: request is live and the option buffer points to one u32.
            if unsafe {
                WinHttpSetOption(
                    request.0,
                    WINHTTP_OPTION_REDIRECT_POLICY,
                    (&raw const redirect_policy).cast(),
                    u32::try_from(size_of::<u32>()).unwrap_or(4),
                )
            } == 0
            {
                return Err(last_error("WinHttpSetOption(redirect policy)"));
            }
            let headers = wide(REQUEST_HEADERS);
            // SAFETY: request and header buffer are live; no optional body is sent.
            if unsafe {
                WinHttpSendRequest(request.0, headers.as_ptr(), u32::MAX, ptr::null(), 0, 0, 0)
            } == 0
            {
                return Err(last_error("WinHttpSendRequest"));
            }
            check_deadline(started, deadline)?;
            // SAFETY: request is live and WinHTTP owns the received response state.
            if unsafe { WinHttpReceiveResponse(request.0, ptr::null_mut()) } == 0 {
                return Err(last_error("WinHttpReceiveResponse"));
            }
            check_deadline(started, deadline)?;
            let status = query_u32_header(request.0, WINHTTP_QUERY_STATUS_CODE)?;
            if status != 200 {
                return Err(UpdaterError::FetchFailed(format!(
                    "WinHTTP final status must be 200, got {status}"
                )));
            }
            let content_length =
                query_optional_u64_header(request.0, WINHTTP_QUERY_CONTENT_LENGTH)?;
            Ok(Self {
                request,
                _connection: connection,
                _session: session,
                content_length,
                started,
            })
        }

        fn read_raw(&self, buffer: &mut [u8]) -> Result<usize, UpdaterError> {
            let mut read = 0u32;
            let requested = u32::try_from(buffer.len()).map_err(|_| {
                UpdaterError::FetchFailed("WinHTTP read buffer exceeds u32".to_owned())
            })?;
            // SAFETY: request is live and `buffer` is writable for `requested`
            // bytes. WinHTTP initializes `read` with the actual byte count.
            if unsafe {
                WinHttpReadData(
                    self.request.0,
                    buffer.as_mut_ptr().cast(),
                    requested,
                    &mut read,
                )
            } == 0
            {
                return Err(last_error("WinHttpReadData"));
            }
            usize::try_from(read)
                .map_err(|_| UpdaterError::FetchFailed("invalid WinHTTP read count".to_owned()))
        }
    }

    struct ParsedHttpsUrl {
        host: String,
        port: u16,
        object: String,
    }

    impl ParsedHttpsUrl {
        fn parse(url: &str) -> Result<Self, UpdaterError> {
            let Some(rest) = url.strip_prefix("https://") else {
                return Err(UpdaterError::UnsupportedManifestSource(url.to_owned()));
            };
            if rest.is_empty()
                || rest.contains('#')
                || rest.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err(UpdaterError::UnsupportedManifestSource(url.to_owned()));
            }
            let boundary = rest.find(['/', '?']).unwrap_or(rest.len());
            let authority = &rest[..boundary];
            if authority.is_empty()
                || authority.contains('@')
                || authority.contains('[')
                || authority.contains(']')
                || authority.bytes().any(|byte| byte.is_ascii_whitespace())
            {
                return Err(UpdaterError::UnsupportedManifestSource(url.to_owned()));
            }
            let (host, port) = match authority.rsplit_once(':') {
                Some((host, port)) if !host.contains(':') => {
                    let port = port
                        .parse::<u16>()
                        .ok()
                        .filter(|value| *value != 0)
                        .ok_or_else(|| UpdaterError::UnsupportedManifestSource(url.to_owned()))?;
                    (host, port)
                }
                Some(_) => return Err(UpdaterError::UnsupportedManifestSource(url.to_owned())),
                None => (authority, INTERNET_DEFAULT_HTTPS_PORT),
            };
            if !valid_ascii_host(host) {
                return Err(UpdaterError::UnsupportedManifestSource(url.to_owned()));
            }
            let suffix = &rest[boundary..];
            let object = if suffix.is_empty() {
                "/".to_owned()
            } else if suffix.starts_with('?') {
                format!("/{suffix}")
            } else {
                suffix.to_owned()
            };
            if object
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte == b'\\')
            {
                return Err(UpdaterError::UnsupportedManifestSource(url.to_owned()));
            }
            Ok(Self {
                host: host.to_owned(),
                port,
                object,
            })
        }
    }

    fn valid_ascii_host(host: &str) -> bool {
        !host.is_empty()
            && host.len() <= 253
            && host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                    && label
                        .as_bytes()
                        .first()
                        .is_some_and(u8::is_ascii_alphanumeric)
                    && label
                        .as_bytes()
                        .last()
                        .is_some_and(u8::is_ascii_alphanumeric)
            })
    }

    pub(crate) fn fetch_text(
        url: &str,
        max_bytes: usize,
        deadline: Duration,
    ) -> Result<String, UpdaterError> {
        let response = Response::open(url, deadline)?;
        if response
            .content_length
            .is_some_and(|length| length > max_bytes as u64)
        {
            return Err(UpdaterError::FetchFailed(format!(
                "HTTP Content-Length exceeds {max_bytes} bytes"
            )));
        }
        let capacity = response
            .content_length
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or(0);
        let mut bytes = Vec::with_capacity(capacity);
        read_body_with_deadline(
            |buffer| response.read_raw(buffer),
            || response.started.elapsed(),
            deadline,
            |chunk| {
                let count = chunk.len();
                if bytes.len().saturating_add(count) > max_bytes {
                    return Err(UpdaterError::FetchFailed(format!(
                        "HTTP response exceeds {max_bytes} bytes"
                    )));
                }
                bytes.extend_from_slice(chunk);
                Ok(())
            },
        )?;
        if response.content_length != Some(bytes.len() as u64) && response.content_length.is_some()
        {
            return Err(UpdaterError::FetchFailed(
                "HTTP body length does not match Content-Length".to_owned(),
            ));
        }
        String::from_utf8(bytes)
            .map_err(|error| UpdaterError::FetchFailed(format!("HTTP body is not UTF-8: {error}")))
    }

    pub(crate) fn download_to_stage(
        url: &str,
        stage_path: &Path,
        max_bytes: u64,
        deadline: Duration,
        event_tx: &Sender<UpdateEvent>,
    ) -> Result<u64, UpdaterError> {
        let result = download_to_stage_inner(url, stage_path, max_bytes, deadline, event_tx);
        if result.is_err() {
            let _ = std::fs::remove_file(stage_path);
        }
        result
    }

    fn download_to_stage_inner(
        url: &str,
        stage_path: &Path,
        max_bytes: u64,
        deadline: Duration,
        event_tx: &Sender<UpdateEvent>,
    ) -> Result<u64, UpdaterError> {
        let response = Response::open(url, deadline)?;
        if response
            .content_length
            .is_some_and(|length| length == 0 || length > max_bytes)
        {
            return Err(UpdaterError::FetchFailed(format!(
                "HTTP Content-Length must be within 1..={max_bytes} bytes"
            )));
        }
        let mut output = File::create(stage_path).map_err(|error| {
            UpdaterError::FetchFailed(format!("{}: {error}", stage_path.display()))
        })?;
        let mut progress = DownloadProgress::new(event_tx, response.content_length);
        let mut written = 0u64;
        read_body_with_deadline(
            |buffer| response.read_raw(buffer),
            || response.started.elapsed(),
            deadline,
            |chunk| {
                written = written.saturating_add(chunk.len() as u64);
                if written > max_bytes {
                    return Err(UpdaterError::FetchFailed(format!(
                        "HTTP response exceeds {max_bytes} bytes"
                    )));
                }
                output
                    .write_all(chunk)
                    .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
                progress.observe(written)
            },
        )?;
        if response
            .content_length
            .is_some_and(|length| length != written)
        {
            return Err(UpdaterError::FetchFailed(
                "HTTP body length does not match Content-Length".to_owned(),
            ));
        }
        if written == 0 {
            return Err(UpdaterError::FetchFailed(
                "update artifact is empty".to_owned(),
            ));
        }
        output
            .flush()
            .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
        progress.finish(written)?;
        Ok(written)
    }

    fn query_u32_header(handle: *mut c_void, query: u32) -> Result<u32, UpdaterError> {
        let mut value = 0u32;
        let mut bytes = u32::try_from(size_of::<u32>()).unwrap_or(4);
        // SAFETY: request is live and the output buffer/length match one u32.
        if unsafe {
            WinHttpQueryHeaders(
                handle,
                query | WINHTTP_QUERY_FLAG_NUMBER,
                ptr::null(),
                (&raw mut value).cast(),
                &mut bytes,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(last_error("WinHttpQueryHeaders"));
        }
        Ok(value)
    }

    fn query_optional_u64_header(
        handle: *mut c_void,
        query: u32,
    ) -> Result<Option<u64>, UpdaterError> {
        let mut value = 0u64;
        let mut bytes = u32::try_from(size_of::<u64>()).unwrap_or(8);
        // SAFETY: request is live and the output buffer/length match one u64.
        if unsafe {
            WinHttpQueryHeaders(
                handle,
                query | WINHTTP_QUERY_FLAG_NUMBER64,
                ptr::null(),
                (&raw mut value).cast(),
                &mut bytes,
                ptr::null_mut(),
            )
        } != 0
        {
            return Ok(Some(value));
        }
        // SAFETY: GetLastError reads the calling thread's Win32 error slot.
        if unsafe { GetLastError() } == ERROR_WINHTTP_HEADER_NOT_FOUND {
            return Ok(None);
        }
        Err(last_error("WinHttpQueryHeaders(Content-Length)"))
    }

    fn check_deadline(started: Instant, deadline: Duration) -> Result<(), UpdaterError> {
        check_elapsed(started.elapsed(), deadline)
    }

    fn check_elapsed(elapsed: Duration, deadline: Duration) -> Result<(), UpdaterError> {
        if elapsed <= deadline {
            return Ok(());
        }
        Err(UpdaterError::FetchFailed(format!(
            "WinHTTP request exceeded {} second wall-clock deadline",
            deadline.as_secs()
        )))
    }

    fn read_body_with_deadline<Read, Elapsed, Consume>(
        mut read: Read,
        mut elapsed: Elapsed,
        deadline: Duration,
        mut consume: Consume,
    ) -> Result<u64, UpdaterError>
    where
        Read: FnMut(&mut [u8]) -> Result<usize, UpdaterError>,
        Elapsed: FnMut() -> Duration,
        Consume: FnMut(&[u8]) -> Result<(), UpdaterError>,
    {
        let mut total = 0u64;
        let mut buffer = [0u8; DOWNLOAD_BUFFER_BYTES];
        loop {
            check_elapsed(elapsed(), deadline)?;
            let count = read(&mut buffer)?;
            check_elapsed(elapsed(), deadline)?;
            if count == 0 {
                return Ok(total);
            }
            if count > buffer.len() {
                return Err(UpdaterError::FetchFailed(
                    "WinHTTP read count exceeds the requested buffer".to_owned(),
                ));
            }
            total = total.saturating_add(count as u64);
            consume(&buffer[..count])?;
        }
    }

    fn last_error(operation: &str) -> UpdaterError {
        // SAFETY: GetLastError reads the calling thread's Win32 error slot.
        let code = unsafe { GetLastError() };
        UpdaterError::FetchFailed(format!("{operation} failed with Win32 error {code}"))
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(core::iter::once(0)).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn https_url_parser_preserves_official_host_and_object() {
            let parsed = ParsedHttpsUrl::parse(crate::updater::OFFICIAL_LATEST_RELEASE_URL)
                .expect("official updater URL");
            assert_eq!(parsed.host, "api.github.com");
            assert_eq!(parsed.port, INTERNET_DEFAULT_HTTPS_PORT);
            assert_eq!(
                parsed.object,
                "/repos/ZRainbow1275/bentodesk/releases/latest"
            );
        }

        #[test]
        fn https_url_parser_rejects_ambiguous_authorities_and_objects() {
            for invalid in [
                "http://example.com/update.json",
                "https://",
                "https://user@example.com/update.json",
                "https://.example.com/update.json",
                "https://example..com/update.json",
                "https://-example.com/update.json",
                "https://example.com\\update.json",
                "https://example.com/update\0.json",
                "https://[::1]/update.json",
            ] {
                assert!(
                    ParsedHttpsUrl::parse(invalid).is_err(),
                    "must reject {invalid:?}"
                );
            }
        }

        #[test]
        fn total_wall_clock_deadline_rejects_slow_drip_after_bounded_reads() {
            let deadline = Duration::from_secs(5);
            let mut elapsed = Duration::ZERO;
            let mut successful_reads = 0;
            let result = read_body_with_deadline(
                |buffer| {
                    successful_reads += 1;
                    buffer[0] = b'x';
                    Ok(1)
                },
                || {
                    let now = elapsed;
                    elapsed += Duration::from_secs(1);
                    now
                },
                deadline,
                |_chunk| Ok(()),
            );

            assert!(matches!(
                result,
                Err(UpdaterError::FetchFailed(message))
                    if message.contains("wall-clock deadline")
            ));
            assert_eq!(
                successful_reads, 3,
                "the fourth read is blocked by total time"
            );
        }
    }
}

#[cfg(windows)]
pub(super) use platform::{download_to_stage, fetch_text};

#[cfg(not(windows))]
pub(super) fn fetch_text(
    url: &str,
    _max_bytes: usize,
    _deadline: Duration,
) -> Result<String, UpdaterError> {
    Err(UpdaterError::UnsupportedManifestSource(url.to_owned()))
}

#[cfg(not(windows))]
pub(super) fn download_to_stage(
    url: &str,
    _stage_path: &Path,
    _max_bytes: u64,
    _deadline: Duration,
    _event_tx: &Sender<UpdateEvent>,
) -> Result<u64, UpdaterError> {
    Err(UpdaterError::UnsupportedManifestSource(url.to_owned()))
}
