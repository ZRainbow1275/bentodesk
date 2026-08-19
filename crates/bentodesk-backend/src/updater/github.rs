//! Strict adapter for BentoDesk's immutable GitHub latest Release.

use core::cmp::Ordering;

use serde::Deserialize;
use smol_str::SmolStr;

use super::{MAX_UPDATE_ARTIFACT_BYTES, UpdateInfo, UpdaterError};

const RELEASE_DOWNLOAD_PREFIX: &str =
    "https://github.com/ZRainbow1275/bentodesk/releases/download/";

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    immutable: bool,
    published_at: Option<String>,
    body: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    state: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}

pub(super) fn try_parse_release(
    text: &str,
    current_version: SmolStr,
) -> Result<Option<UpdateInfo>, UpdaterError> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| UpdaterError::InvalidManifest(error.to_string()))?;
    let Some(object) = value.as_object() else {
        return Err(UpdaterError::InvalidManifest(
            "updater manifest root must be a JSON object".to_owned(),
        ));
    };
    if !object.contains_key("tag_name") {
        return Ok(None);
    }

    let release: GithubRelease = serde_json::from_value(value)
        .map_err(|error| UpdaterError::InvalidManifest(error.to_string()))?;
    if release.draft || release.prerelease || !release.immutable {
        return Err(UpdaterError::InvalidManifest(
            "GitHub release must be immutable, non-draft, and non-prerelease".to_owned(),
        ));
    }

    let parts = canonical_tag_parts(&release.tag_name)?;
    let version = format!("{}.{}.{}", parts[0], parts[1], parts[2]);
    let expected_name = format!("BentoDesk-{version}-windows-x64-setup.exe");
    let mut matching = release
        .assets
        .iter()
        .filter(|asset| asset.name == expected_name);
    let asset = matching.next().ok_or_else(|| {
        UpdaterError::InvalidManifest(format!(
            "GitHub release is missing exact setup asset '{expected_name}'"
        ))
    })?;
    if matching.next().is_some() {
        return Err(UpdaterError::InvalidManifest(format!(
            "GitHub release contains duplicate setup asset '{expected_name}'"
        )));
    }
    if asset.state != "uploaded" {
        return Err(UpdaterError::InvalidManifest(
            "GitHub setup asset state must be 'uploaded'".to_owned(),
        ));
    }
    if asset.size == 0 || asset.size > MAX_UPDATE_ARTIFACT_BYTES {
        return Err(UpdaterError::InvalidManifest(format!(
            "GitHub setup asset size must be within 1..={MAX_UPDATE_ARTIFACT_BYTES} bytes"
        )));
    }
    let digest = strict_sha256_digest(asset.digest.as_deref())?;
    let expected_url = format!(
        "{RELEASE_DOWNLOAD_PREFIX}{}/{expected_name}",
        release.tag_name
    );
    if asset.browser_download_url != expected_url {
        return Err(UpdaterError::InvalidManifest(format!(
            "GitHub setup asset URL must equal '{expected_url}'"
        )));
    }

    Ok(Some(UpdateInfo {
        version: SmolStr::new(version),
        current_version,
        date: release.published_at.map(SmolStr::new),
        body: release.body,
        artifact_url: Some(asset.browser_download_url.clone()),
        artifact_sha256: Some(digest),
        signature: None,
    }))
}

fn canonical_tag_parts(tag: &str) -> Result<[&str; 3], UpdaterError> {
    let Some(version) = tag.strip_prefix('v') else {
        return Err(UpdaterError::InvalidManifest(
            "GitHub tag_name must start with lowercase 'v'".to_owned(),
        ));
    };
    canonical_parts(version).ok_or_else(|| {
        UpdaterError::InvalidManifest(
            "GitHub tag_name must be canonical stable vMAJOR.MINOR.PATCH".to_owned(),
        )
    })
}

pub(super) fn canonical_parts(version: &str) -> Option<[&str; 3]> {
    let mut values = version.split('.');
    let parts = [values.next()?, values.next()?, values.next()?];
    if values.next().is_some() || parts.iter().any(|part| !canonical_decimal(part)) {
        return None;
    }
    Some(parts)
}

fn canonical_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

pub(super) fn compare_canonical_versions(candidate: &str, current: &str) -> Option<Ordering> {
    let candidate = candidate.strip_prefix('v').unwrap_or(candidate);
    let current = current.strip_prefix('v').unwrap_or(current);
    let candidate = canonical_parts(candidate)?;
    let current = canonical_parts(current)?;
    for index in 0..candidate.len() {
        let ordering = candidate[index]
            .len()
            .cmp(&current[index].len())
            .then_with(|| candidate[index].cmp(current[index]));
        if ordering != Ordering::Equal {
            return Some(ordering);
        }
    }
    Some(Ordering::Equal)
}

fn strict_sha256_digest(value: Option<&str>) -> Result<String, UpdaterError> {
    let value = value.ok_or_else(|| {
        UpdaterError::InvalidManifest("GitHub setup asset is missing digest".to_owned())
    })?;
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(UpdaterError::InvalidManifest(
            "GitHub setup asset digest must start with 'sha256:'".to_owned(),
        ));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(UpdaterError::InvalidManifest(
            "GitHub setup asset digest must contain exactly 64 hex digits".to_owned(),
        ));
    }
    Ok(hex.to_ascii_lowercase())
}
