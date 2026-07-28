//! Manifest parsing, artifact staging, verification, and installer launch.

use super::*;

// ─── Pure helpers (preserved verbatim from 1.x) ──────────────────────

#[derive(Debug, Deserialize)]
pub(super) struct RawManifest {
    version: Option<String>,
    current_version: Option<String>,
    date: Option<String>,
    pub_date: Option<String>,
    body: Option<String>,
    notes: Option<String>,
    artifact_url: Option<String>,
    download_url: Option<String>,
    url: Option<String>,
    sha256: Option<String>,
    artifact_sha256: Option<String>,
    signature: Option<String>,
    platforms: Option<BTreeMap<String, RawPlatformManifest>>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawPlatformManifest {
    url: Option<String>,
    sha256: Option<String>,
    artifact_sha256: Option<String>,
    signature: Option<String>,
}

pub(super) fn parse_update_manifest(
    text: &str,
    fallback_current_version: SmolStr,
) -> Result<UpdateInfo, UpdaterError> {
    let raw: RawManifest = serde_json::from_str(text)
        .map_err(|error| UpdaterError::InvalidManifest(error.to_string()))?;
    let version = raw
        .version
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UpdaterError::InvalidManifest("missing non-empty version".to_owned()))?;
    let current_version = raw
        .current_version
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(SmolStr::new)
        .unwrap_or(fallback_current_version);
    let date = raw
        .date
        .or(raw.pub_date)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(SmolStr::new);
    let body = raw
        .body
        .or(raw.notes)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let platform = raw
        .platforms
        .as_ref()
        .and_then(|platforms| platforms.get(TAURI_WINDOWS_X64_PLATFORM));
    let platform_artifact_url = platform
        .and_then(|value| value.url.as_ref())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let platform_artifact_sha256 = platform
        .and_then(|value| value.artifact_sha256.as_ref().or(value.sha256.as_ref()))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let platform_signature = platform
        .and_then(|value| value.signature.as_ref())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let artifact_url = platform_artifact_url.or_else(|| {
        raw.artifact_url
            .or(raw.download_url)
            .or(raw.url)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    });
    let artifact_sha256 = platform_artifact_sha256.or_else(|| {
        raw.artifact_sha256
            .or(raw.sha256)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    });
    let signature = platform_signature.or_else(|| {
        raw.signature
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    });
    Ok(UpdateInfo {
        version: SmolStr::new(version),
        current_version,
        date,
        body,
        artifact_url,
        artifact_sha256,
        signature,
    })
}

pub(super) fn staged_artifact_path(version: &str, source: &str) -> Result<PathBuf, UpdaterError> {
    let mut dir = std::env::temp_dir();
    dir.push("bentodesk-nano-update");
    std::fs::create_dir_all(&dir)
        .map_err(|error| UpdaterError::FetchFailed(format!("{}: {error}", dir.display())))?;
    let mut file_name = String::from("bentodesk-nano-");
    file_name.push_str(&sanitize_path_component(version));
    file_name.push_str(".update");
    if source.ends_with(".exe") {
        file_name.push_str(".exe");
    } else if source.ends_with(".msi") {
        file_name.push_str(".msi");
    }
    dir.push(file_name);
    Ok(dir)
}

pub(super) fn sanitize_path_component(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    if out.is_empty() {
        out.push_str("unknown");
    }
    out
}

pub(super) fn artifact_source_path(source: &str) -> Result<PathBuf, UpdaterError> {
    let source = source.trim();
    if source.contains("://") && !source.starts_with("file://") {
        return Err(UpdaterError::UnsupportedManifestSource(source.to_owned()));
    }
    if let Some(rest) = source.strip_prefix("file://") {
        Ok(PathBuf::from(rest))
    } else {
        Ok(PathBuf::from(source))
    }
}

pub(super) fn copy_artifact_to_stage(
    source: &str,
    stage_path: &PathBuf,
    event_tx: &Sender<UpdateEvent>,
) -> Result<(), UpdaterError> {
    let source = source.trim();
    if source.starts_with("http://") || source.starts_with("https://") {
        return copy_http_artifact_to_stage_winhttp(source, stage_path, event_tx);
    }
    let source_path = artifact_source_path(source)?;
    let mut input = File::open(&source_path).map_err(|error| {
        UpdaterError::FetchFailed(format!("{}: {error}", source_path.display()))
    })?;
    let total_bytes = input.metadata().ok().map(|metadata| metadata.len());
    let mut output = File::create(stage_path)
        .map_err(|error| UpdaterError::FetchFailed(format!("{}: {error}", stage_path.display())))?;
    let mut buffer = [0u8; DOWNLOAD_BUFFER_BYTES];
    let mut written = 0u64;
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
        written += count as u64;
        emit_download_progress(event_tx, written, total_bytes)?;
    }
    output
        .flush()
        .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
    Ok(())
}

pub(super) fn emit_download_progress(
    event_tx: &Sender<UpdateEvent>,
    written: u64,
    total_bytes: Option<u64>,
) -> Result<(), UpdaterError> {
    event_tx
        .send(UpdateEvent::Progress {
            progress: UpdateProgress {
                chunk_len: written,
                total_bytes,
            },
        })
        .map_err(|_| UpdaterError::EventChannelClosed)
}

pub(super) fn validate_manifest_integrity_policy(info: &UpdateInfo) -> Result<(), UpdaterError> {
    if info.artifact_sha256.is_some() || info.signature.is_some() {
        return Ok(());
    }
    tracing::warn!(
        target: "bentodesk::updater",
        version = %info.version,
        "updater manifest has no sha256/artifact_sha256 integrity field; allowing legacy/internal unsigned artifact"
    );
    Ok(())
}

pub(super) fn verify_staged_artifact(
    info: &UpdateInfo,
    stage_path: &Path,
    minisign_public_key: &str,
) -> Result<(), UpdaterError> {
    if let Some(expected) = info.artifact_sha256.as_deref() {
        let expected = normalize_sha256_hex(expected)?;
        let actual = sha256_file(stage_path)?;
        let actual_hex = hex_encode(&actual);
        if actual_hex != expected {
            return Err(UpdaterError::VerificationFailed(format!(
                "sha256 mismatch for {}: expected {expected}, got {actual_hex}",
                stage_path.display()
            )));
        }
    }
    if let Some(signature) = info.signature.as_deref() {
        verify_minisign_signature(minisign_public_key, signature, stage_path)?;
    }
    Ok(())
}

pub(super) fn verify_minisign_signature(
    public_key_text: &str,
    signature_text: &str,
    stage_path: &Path,
) -> Result<(), UpdaterError> {
    let public_key = PublicKey::decode(public_key_text.trim()).map_err(|error| {
        UpdaterError::VerificationFailed(format!(
            "embedded minisign public key is invalid: {error}"
        ))
    })?;
    let decoded_signature = decode_tauri_minisign_signature(signature_text)?;
    let signature = Signature::decode(decoded_signature.as_str()).map_err(|error| {
        UpdaterError::VerificationFailed(format!("minisign signature is invalid: {error}"))
    })?;
    let mut verifier = public_key.verify_stream(&signature).map_err(|error| {
        UpdaterError::VerificationFailed(format!(
            "minisign stream verification setup failed: {error}"
        ))
    })?;
    let mut file = File::open(stage_path)
        .map_err(|error| UpdaterError::FetchFailed(format!("{}: {error}", stage_path.display())))?;
    let mut buffer = [0u8; DOWNLOAD_BUFFER_BYTES];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
        if count == 0 {
            break;
        }
        verifier.update(&buffer[..count]);
    }
    verifier.finalize().map_err(|error| {
        UpdaterError::VerificationFailed(format!(
            "minisign signature mismatch for {}: {error}",
            stage_path.display()
        ))
    })
}

pub(super) fn decode_tauri_minisign_signature(value: &str) -> Result<String, UpdaterError> {
    let trimmed = value.trim();
    if looks_like_minisign_signature(trimmed) {
        return Ok(trimmed.to_owned());
    }

    let decoded = decode_base64_signature(trimmed)?;
    let decoded_text = String::from_utf8(decoded).map_err(|error| {
        UpdaterError::VerificationFailed(format!(
            "minisign signature base64 payload is not UTF-8: {error}"
        ))
    })?;
    let decoded_trimmed = decoded_text.trim();
    if !looks_like_minisign_signature(decoded_trimmed) {
        return Err(UpdaterError::VerificationFailed(
            "minisign signature base64 payload is not a minisign signature".to_owned(),
        ));
    }
    Ok(decoded_trimmed.to_owned())
}

pub(super) fn looks_like_minisign_signature(value: &str) -> bool {
    value.contains("untrusted comment:") && value.contains("\ntrusted comment:")
}

pub(super) fn decode_base64_signature(value: &str) -> Result<Vec<u8>, UpdaterError> {
    let mut decoded = Vec::with_capacity(value.len().saturating_mul(3) / 4);
    let mut quartet = [0u8; 4];
    let mut quartet_len = 0usize;
    let mut saw_padding = false;

    for (index, byte) in value.bytes().enumerate() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        let sextet = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                saw_padding = true;
                64
            }
            _ => {
                return Err(UpdaterError::VerificationFailed(format!(
                    "minisign signature base64 decode failed at byte {index}: invalid character"
                )));
            }
        };
        if saw_padding && sextet != 64 {
            return Err(UpdaterError::VerificationFailed(
                "minisign signature base64 decode failed: non-padding character after padding"
                    .to_owned(),
            ));
        }

        quartet[quartet_len] = sextet;
        quartet_len += 1;
        if quartet_len == 4 {
            decode_base64_quartet(&quartet, &mut decoded)?;
            quartet_len = 0;
        }
    }

    if quartet_len != 0 {
        return Err(UpdaterError::VerificationFailed(
            "minisign signature base64 decode failed: incomplete quartet".to_owned(),
        ));
    }

    Ok(decoded)
}

pub(super) fn decode_base64_quartet(
    quartet: &[u8; 4],
    decoded: &mut Vec<u8>,
) -> Result<(), UpdaterError> {
    if quartet[0] == 64 || quartet[1] == 64 {
        return Err(UpdaterError::VerificationFailed(
            "minisign signature base64 decode failed: padding in leading positions".to_owned(),
        ));
    }
    if quartet[2] == 64 {
        if quartet[3] != 64 {
            return Err(UpdaterError::VerificationFailed(
                "minisign signature base64 decode failed: invalid padding sequence".to_owned(),
            ));
        }
        decoded.push((quartet[0] << 2) | (quartet[1] >> 4));
        return Ok(());
    }

    decoded.push((quartet[0] << 2) | (quartet[1] >> 4));
    decoded.push(((quartet[1] & 0x0F) << 4) | (quartet[2] >> 2));
    if quartet[3] != 64 {
        decoded.push(((quartet[2] & 0x03) << 6) | quartet[3]);
    }
    Ok(())
}

pub(super) fn normalize_sha256_hex(value: &str) -> Result<String, UpdaterError> {
    let trimmed = value.trim();
    let without_prefix = trimmed
        .strip_prefix("sha256:")
        .or_else(|| trimmed.strip_prefix("SHA256:"))
        .unwrap_or(trimmed);
    let mut out = String::with_capacity(SHA256_HEX_CHARS);
    for ch in without_prefix.chars() {
        if ch.is_ascii_hexdigit() {
            out.push(ch.to_ascii_lowercase());
        } else if matches!(ch, ' ' | '\t' | '\r' | '\n' | ':' | '-') {
            continue;
        } else {
            return Err(UpdaterError::VerificationFailed(format!(
                "sha256 contains non-hex character '{ch}'"
            )));
        }
    }
    if out.len() != SHA256_HEX_CHARS {
        return Err(UpdaterError::VerificationFailed(format!(
            "sha256 must contain {SHA256_HEX_CHARS} hex characters, got {}",
            out.len()
        )));
    }
    Ok(out)
}

pub(super) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(windows)]
pub(super) struct BCryptSha256Alg(BCRYPT_ALG_HANDLE);

#[cfg(windows)]
impl BCryptSha256Alg {
    fn open() -> Result<Self, UpdaterError> {
        let mut handle: BCRYPT_ALG_HANDLE = ptr::null_mut();
        // SAFETY: `phalgorithm` points to a stack out-parameter, the algorithm
        // identifier is the static null-terminated UTF-16 SHA256 identifier
        // exposed by windows-sys, and `pszimplementation = NULL` selects the
        // default CNG provider.
        let status = unsafe {
            BCryptOpenAlgorithmProvider(&mut handle, BCRYPT_SHA256_ALGORITHM, ptr::null(), 0)
        };
        if status != 0 {
            return Err(UpdaterError::VerificationFailed(format!(
                "BCryptOpenAlgorithmProvider(SHA256) returned NTSTATUS {status:#x}"
            )));
        }
        Ok(Self(handle))
    }
}

#[cfg(windows)]
impl Drop for BCryptSha256Alg {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: handle is owned by this RAII wrapper after a successful
            // BCryptOpenAlgorithmProvider call and is closed exactly once here.
            unsafe {
                BCryptCloseAlgorithmProvider(self.0, 0);
            }
        }
    }
}

#[cfg(windows)]
pub(super) struct BCryptSha256Hash(BCRYPT_HASH_HANDLE);

#[cfg(windows)]
impl BCryptSha256Hash {
    fn create(alg: &BCryptSha256Alg) -> Result<Self, UpdaterError> {
        let mut handle: BCRYPT_HASH_HANDLE = ptr::null_mut();
        // SAFETY: `alg.0` is a valid SHA-256 algorithm handle. The hash handle
        // out-parameter is a stack pointer. Passing NULL hash-object/secret is
        // the documented CNG path for provider-managed hash object storage and
        // an unkeyed SHA-256 hash.
        let status =
            unsafe { BCryptCreateHash(alg.0, &mut handle, ptr::null_mut(), 0, ptr::null(), 0, 0) };
        if status != 0 {
            return Err(UpdaterError::VerificationFailed(format!(
                "BCryptCreateHash(SHA256) returned NTSTATUS {status:#x}"
            )));
        }
        Ok(Self(handle))
    }

    fn update(&self, bytes: &[u8]) -> Result<(), UpdaterError> {
        // SAFETY: `self.0` is a live hash handle owned by this wrapper and
        // `bytes` is a valid immutable buffer for the duration of the call.
        let status = unsafe { BCryptHashData(self.0, bytes.as_ptr(), bytes.len() as u32, 0) };
        if status != 0 {
            return Err(UpdaterError::VerificationFailed(format!(
                "BCryptHashData(SHA256) returned NTSTATUS {status:#x}"
            )));
        }
        Ok(())
    }

    fn finish(&self) -> Result<[u8; SHA256_DIGEST_BYTES], UpdaterError> {
        let mut digest = [0u8; SHA256_DIGEST_BYTES];
        // SAFETY: `self.0` is a live hash handle and `digest` is a valid
        // mutable output buffer of the exact SHA-256 digest size.
        let status =
            unsafe { BCryptFinishHash(self.0, digest.as_mut_ptr(), digest.len() as u32, 0) };
        if status != 0 {
            return Err(UpdaterError::VerificationFailed(format!(
                "BCryptFinishHash(SHA256) returned NTSTATUS {status:#x}"
            )));
        }
        Ok(digest)
    }
}

#[cfg(windows)]
impl Drop for BCryptSha256Hash {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: handle is owned by this RAII wrapper after a successful
            // BCryptCreateHash call and is destroyed exactly once here.
            unsafe {
                BCryptDestroyHash(self.0);
            }
        }
    }
}

#[cfg(windows)]
pub(super) fn sha256_file(path: &Path) -> Result<[u8; SHA256_DIGEST_BYTES], UpdaterError> {
    let alg = BCryptSha256Alg::open()?;
    let hash = BCryptSha256Hash::create(&alg)?;
    let mut file = File::open(path)
        .map_err(|error| UpdaterError::FetchFailed(format!("{}: {error}", path.display())))?;
    let mut buffer = [0u8; DOWNLOAD_BUFFER_BYTES];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| UpdaterError::FetchFailed(error.to_string()))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count])?;
    }
    hash.finish()
}

#[cfg(not(windows))]
pub(super) fn sha256_file(_path: &Path) -> Result<[u8; SHA256_DIGEST_BYTES], UpdaterError> {
    Err(UpdaterError::VerificationFailed(
        "SHA-256 artifact verification requires the selected Windows CNG backend".to_owned(),
    ))
}

pub(super) fn validate_staged_installer(path: &Path) -> Result<(), UpdaterError> {
    if !path.is_file() {
        return Err(UpdaterError::FetchFailed(format!(
            "staged updater artifact is missing: {}",
            path.display()
        )));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("exe") {
        return Err(UpdaterError::InvalidManifest(format!(
            "staged updater artifact is not an NSIS .exe: {}",
            path.display()
        )));
    }
    Ok(())
}

pub(super) fn launch_nsis_installer(path: &Path) -> Result<(), UpdaterError> {
    ProcessCommand::new(path)
        .arg("/S")
        .spawn()
        .map(|_| ())
        .map_err(|error| UpdaterError::InstallFailed(format!("{}: {error}", path.display())))
}

pub(super) fn version_is_newer(candidate: &str, current: &str) -> bool {
    let candidate = candidate.trim().trim_start_matches('v');
    let current = current.trim().trim_start_matches('v');
    if candidate == current {
        return false;
    }
    match (parse_version_parts(candidate), parse_version_parts(current)) {
        (Some(candidate_parts), Some(current_parts)) => candidate_parts > current_parts,
        _ => candidate > current,
    }
}

pub(super) fn parse_version_parts(value: &str) -> Option<[u64; 4]> {
    let mut parts = [0u64; 4];
    let mut seen = 0usize;
    for raw in value.split(['.', '-']) {
        if seen >= parts.len() {
            return None;
        }
        let token = raw.trim();
        if token.is_empty() {
            return None;
        }
        let digits = token
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if digits.is_empty() || digits.len() != token.len() {
            return None;
        }
        parts[seen] = digits.parse::<u64>().ok()?;
        seen += 1;
    }
    if seen == 0 {
        return None;
    }
    Some(parts)
}
