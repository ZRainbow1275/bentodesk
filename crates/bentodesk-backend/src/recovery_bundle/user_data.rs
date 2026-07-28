//! Selected-stack user-data sidecar collection, validation, and restore.

use super::*;

/// Binary user-data sidecar bundled alongside the authoritative `zones.bin`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryUserDataFile {
    /// Path relative to the selected-stack data root.
    pub relative_path: SmolStr,
    /// Explicit codec tag for the raw payload.
    pub codec: SmolStr,
    /// Byte length before base64 encoding.
    pub len_bytes: u64,
    /// FNV-1a checksum of the raw bytes, hex-encoded.
    pub checksum: SmolStr,
    /// Base64 encoded raw bytes.
    pub payload_b64: String,
}

/// Decoded user-data sidecar ready for shell-side restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredUserDataFile {
    /// Safe relative path under the selected-stack data root.
    pub relative_path: SmolStr,
    /// Raw bytes validated against the bundle checksum.
    pub bytes: Vec<u8>,
    /// Stable checksum copied from the bundle metadata.
    pub checksum: SmolStr,
}

/// Result of restoring selected-stack user-data sidecars.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecoveryUserDataRestoreReport {
    /// Number of files written under the selected-stack data root.
    pub restored_files: usize,
    /// Total payload bytes restored.
    pub restored_bytes: u64,
}

fn user_data_relative_wire_path(path: &Path) -> Result<SmolStr, RecoveryBundleError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let Some(text) = part.to_str() else {
                    return Err(RecoveryBundleError::InvalidUserDataPath {
                        path: SmolStr::new(path.display().to_string()),
                    });
                };
                parts.push(text.to_string());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => {
                return Err(RecoveryBundleError::InvalidUserDataPath {
                    path: SmolStr::new(path.display().to_string()),
                });
            }
        }
    }
    let joined = parts.join("/");
    if !safe_user_data_relative_path(&joined) {
        return Err(RecoveryBundleError::InvalidUserDataPath {
            path: SmolStr::new(joined),
        });
    }
    Ok(SmolStr::new(joined))
}

fn push_user_data_file(
    data_root: &Path,
    relative_path: PathBuf,
    files: &mut Vec<RecoveryUserDataFile>,
    total_bytes: &mut u64,
) -> Result<(), RecoveryBundleError> {
    let absolute = data_root.join(&relative_path);
    let metadata = match std::fs::metadata(&absolute) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(RecoveryBundleError::UserDataIo {
                op: "metadata",
                path: absolute,
                source,
            });
        }
    };
    if !metadata.is_file() {
        return Ok(());
    }
    let len = metadata.len();
    if len > USER_DATA_MAX_FILE_BYTES {
        return Err(RecoveryBundleError::UserDataFileTooLarge {
            path: absolute,
            bytes: len,
            max_bytes: USER_DATA_MAX_FILE_BYTES,
        });
    }
    *total_bytes = total_bytes.saturating_add(len);
    if *total_bytes > USER_DATA_MAX_TOTAL_BYTES {
        return Err(RecoveryBundleError::UserDataTotalTooLarge {
            bytes: *total_bytes,
            max_bytes: USER_DATA_MAX_TOTAL_BYTES,
        });
    }
    let bytes = std::fs::read(&absolute).map_err(|source| RecoveryBundleError::UserDataIo {
        op: "read",
        path: absolute.clone(),
        source,
    })?;
    files.push(RecoveryUserDataFile {
        relative_path: user_data_relative_wire_path(&relative_path)?,
        codec: SmolStr::new_static(RECOVERY_BUNDLE_USER_DATA_CODEC),
        len_bytes: bytes.len() as u64,
        checksum: checksum_hex(&bytes),
        payload_b64: base64_encode(&bytes),
    });
    Ok(())
}

fn collect_user_data_dir_files(
    data_root: &Path,
    relative_dir: PathBuf,
    out: &mut Vec<PathBuf>,
) -> Result<(), RecoveryBundleError> {
    let absolute = data_root.join(&relative_dir);
    let read_dir = match std::fs::read_dir(&absolute) {
        Ok(read_dir) => read_dir,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(RecoveryBundleError::UserDataIo {
                op: "read_dir",
                path: absolute,
                source,
            });
        }
    };
    for entry in read_dir {
        let entry = entry.map_err(|source| RecoveryBundleError::UserDataIo {
            op: "read_dir_entry",
            path: absolute.clone(),
            source,
        })?;
        let path = entry.path();
        let file_name = entry.file_name();
        let child_relative = relative_dir.join(file_name);
        let metadata = entry
            .metadata()
            .map_err(|source| RecoveryBundleError::UserDataIo {
                op: "metadata",
                path,
                source,
            })?;
        if metadata.is_dir() {
            collect_user_data_dir_files(data_root, child_relative, out)?;
        } else if metadata.is_file() {
            out.push(child_relative);
        }
    }
    Ok(())
}

/// Collect selected-stack user-data sidecars that are not covered by the
/// primary `zones.bin`, config-vault payload, safety manifest, or icon backup.
pub fn collect_user_data_files(
    data_root: &Path,
) -> Result<Vec<RecoveryUserDataFile>, RecoveryBundleError> {
    let mut relative_paths = Vec::new();
    for file_name in USER_DATA_ROOT_FILES {
        relative_paths.push(PathBuf::from(file_name));
    }
    for dir_name in USER_DATA_DIRS {
        collect_user_data_dir_files(data_root, PathBuf::from(dir_name), &mut relative_paths)?;
    }
    relative_paths.sort();
    relative_paths.dedup();

    let mut files = Vec::new();
    let mut total_bytes = 0u64;
    for relative_path in relative_paths {
        push_user_data_file(data_root, relative_path, &mut files, &mut total_bytes)?;
    }
    validate_user_data_metadata(&files)?;
    Ok(files)
}

fn write_user_data_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), RecoveryBundleError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| RecoveryBundleError::UserDataIo {
            op: "create_dir_all",
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let temp_path = path.with_extension("recovery.tmp");
    std::fs::write(&temp_path, bytes).map_err(|source| RecoveryBundleError::UserDataIo {
        op: "write_temp",
        path: temp_path.clone(),
        source,
    })?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|source| RecoveryBundleError::UserDataIo {
            op: "remove_existing",
            path: path.to_path_buf(),
            source,
        })?;
    }
    std::fs::rename(&temp_path, path).map_err(|source| RecoveryBundleError::UserDataIo {
        op: "rename_temp",
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Restore validated selected-stack user-data sidecars under `data_root`.
pub fn restore_user_data_files(
    data_root: &Path,
    files: &[RecoveredUserDataFile],
) -> Result<RecoveryUserDataRestoreReport, RecoveryBundleError> {
    let mut report = RecoveryUserDataRestoreReport::default();
    for file in files {
        if !safe_user_data_relative_path(file.relative_path.as_str()) {
            return Err(RecoveryBundleError::InvalidUserDataPath {
                path: file.relative_path.clone(),
            });
        }
        let path = data_root.join(file.relative_path.as_str());
        write_user_data_file_atomic(&path, &file.bytes)?;
        report.restored_files = report.restored_files.saturating_add(1);
        report.restored_bytes = report
            .restored_bytes
            .saturating_add(file.bytes.len() as u64);
    }
    Ok(report)
}

fn safe_user_data_relative_path(relative_path: &str) -> bool {
    let path = Path::new(relative_path);
    if path.is_absolute() {
        return false;
    }
    let mut components = 0usize;
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let part_text = part.to_string_lossy();
                if part_text.is_empty() {
                    return false;
                }
                if components == 0 && part_text.eq_ignore_ascii_case(RECOVERY_BUNDLE_DIR) {
                    return false;
                }
                components += 1;
            }
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => return false,
        }
    }
    components > 0
}

pub(super) fn validate_user_data_metadata(
    files: &[RecoveryUserDataFile],
) -> Result<(), RecoveryBundleError> {
    let mut total = 0u64;
    for file in files {
        if !safe_user_data_relative_path(file.relative_path.as_str()) {
            return Err(RecoveryBundleError::InvalidUserDataPath {
                path: file.relative_path.clone(),
            });
        }
        if file.codec.as_str() != RECOVERY_BUNDLE_USER_DATA_CODEC {
            return Err(RecoveryBundleError::UnsupportedUserDataCodec {
                found: file.codec.clone(),
            });
        }
        if file.len_bytes > USER_DATA_MAX_FILE_BYTES {
            return Err(RecoveryBundleError::UserDataFileTooLarge {
                path: PathBuf::from(file.relative_path.as_str()),
                bytes: file.len_bytes,
                max_bytes: USER_DATA_MAX_FILE_BYTES,
            });
        }
        total = total.saturating_add(file.len_bytes);
        if total > USER_DATA_MAX_TOTAL_BYTES {
            return Err(RecoveryBundleError::UserDataTotalTooLarge {
                bytes: total,
                max_bytes: USER_DATA_MAX_TOTAL_BYTES,
            });
        }
    }
    Ok(())
}

fn validate_and_decode_user_data_file(
    file: &RecoveryUserDataFile,
) -> Result<RecoveredUserDataFile, RecoveryBundleError> {
    validate_user_data_metadata(core::slice::from_ref(file))?;
    let decoded = base64_decode(&file.payload_b64)?;
    let actual_len = decoded.len() as u64;
    if actual_len != file.len_bytes {
        return Err(RecoveryBundleError::UserDataLengthMismatch {
            path: file.relative_path.clone(),
            expected: file.len_bytes,
            actual: actual_len,
        });
    }
    let actual_checksum = checksum_hex(&decoded);
    if actual_checksum != file.checksum {
        return Err(RecoveryBundleError::UserDataChecksumMismatch {
            path: file.relative_path.clone(),
            expected: file.checksum.clone(),
            actual: actual_checksum,
        });
    }
    Ok(RecoveredUserDataFile {
        relative_path: file.relative_path.clone(),
        bytes: decoded,
        checksum: file.checksum.clone(),
    })
}

pub(super) fn validate_and_decode_user_data_files(
    files: &[RecoveryUserDataFile],
) -> Result<Vec<RecoveredUserDataFile>, RecoveryBundleError> {
    validate_user_data_metadata(files)?;
    files
        .iter()
        .map(validate_and_decode_user_data_file)
        .collect()
}
