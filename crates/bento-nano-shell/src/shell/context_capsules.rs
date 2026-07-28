//! Native shell owner: `context_capsules`.

use super::*;

#[derive(Debug)]
pub(super) enum ContextCapsuleError {
    MissingZonesParent,
    EmptyCapsuleId,
    CapsuleNotFound(SmolStr),
    InvalidEnvelope(SmolStr),
    Codec(PlatformError),
    Base64(bento_nano_backend::config_vault::wire::Base64Error),
    Json(serde_json::Error),
    Io {
        op: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
}

impl core::fmt::Display for ContextCapsuleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingZonesParent => f.write_str("zones path has no parent directory"),
            Self::EmptyCapsuleId => f.write_str("context capsule id is empty"),
            Self::CapsuleNotFound(capsule_id) => {
                write!(f, "context capsule not found: {capsule_id}")
            }
            Self::InvalidEnvelope(message) => {
                write!(f, "context capsule envelope invalid: {message}")
            }
            Self::Codec(source) => write!(f, "context capsule payload invalid: {source}"),
            Self::Base64(source) => write!(f, "context capsule base64 invalid: {source}"),
            Self::Json(source) => write!(f, "context capsule json invalid: {source}"),
            Self::Io { op, path, source } => {
                write!(f, "{op} failed at {}: {source}", path.display())
            }
        }
    }
}

impl core::error::Error for ContextCapsuleError {}

impl From<PlatformError> for ContextCapsuleError {
    fn from(value: PlatformError) -> Self {
        Self::Codec(value)
    }
}

impl From<bento_nano_backend::config_vault::wire::Base64Error> for ContextCapsuleError {
    fn from(value: bento_nano_backend::config_vault::wire::Base64Error) -> Self {
        Self::Base64(value)
    }
}

impl From<serde_json::Error> for ContextCapsuleError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Debug)]
pub(super) struct ContextCapsuleCandidate {
    path: PathBuf,
    entry: CapsuleEntry,
    modified: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ContextCapsuleWindow {
    pub(super) title: String,
    pub(super) class_name: String,
    pub(super) process_name: String,
    pub(super) rect: (i32, i32, i32, i32),
    pub(super) is_maximized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ContextCapsuleEnvelope {
    pub(super) schema: u32,
    pub(super) name: String,
    pub(super) icon: String,
    pub(super) captured_at: String,
    pub(super) zones_codec: String,
    pub(super) zones_bin_b64: String,
    pub(super) windows: Vec<ContextCapsuleWindow>,
}

#[derive(Debug, Clone)]
pub(super) struct LiveContextWindow {
    pub(super) hwnd: HWND,
    pub(super) title: String,
    pub(super) class_name: String,
    pub(super) process_name: String,
    pub(super) rect: (i32, i32, i32, i32),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ContextWindowRestoreReport {
    restored: usize,
    pending: usize,
    errors: usize,
}

pub(super) const CONTEXT_CAPSULE_SCHEMA: u32 = 2;
pub(super) const CONTEXT_CAPSULE_ZONES_CODEC: &str = "bento-nano-zones-bin-v1";
pub(super) const CONTEXT_CAPSULE_FILE_PREFIX: &str = "capsule-";
pub(super) const CONTEXT_CAPSULE_FILE_SUFFIX: &str = ".bin";
pub(super) const CONTEXT_WINDOW_MIN_SIZE: i32 = 50;

pub(super) fn context_capsule_dir_for_zones_path(
    zones_path: &Path,
) -> Result<PathBuf, ContextCapsuleError> {
    let Some(parent) = zones_path.parent() else {
        return Err(ContextCapsuleError::MissingZonesParent);
    };
    Ok(parent.join("capsules"))
}

pub(super) fn context_capsule_file_name(capsule_id: &str) -> String {
    format!("{CONTEXT_CAPSULE_FILE_PREFIX}{capsule_id}{CONTEXT_CAPSULE_FILE_SUFFIX}")
}

pub(super) fn context_capsule_id_from_file_name(file_name: &str) -> Option<&str> {
    file_name
        .strip_prefix(CONTEXT_CAPSULE_FILE_PREFIX)
        .and_then(|value| value.strip_suffix(CONTEXT_CAPSULE_FILE_SUFFIX))
        .filter(|value| !value.is_empty())
}

pub(super) fn sanitize_context_capsule_name(name: &str) -> SmolStr {
    let trimmed = name.trim();
    let source = if trimmed.is_empty() {
        "Context Capsule"
    } else {
        trimmed
    };
    let mut output = String::with_capacity(source.len().min(80));
    let mut last_dash = false;
    for (char_index, ch) in source.chars().enumerate() {
        if char_index >= 48 {
            break;
        }
        let invalid =
            ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*');
        if invalid || ch.is_whitespace() {
            if !last_dash {
                output.push('-');
                last_dash = true;
            }
        } else {
            output.push(ch);
            last_dash = false;
        }
    }
    let sanitized = output.trim_matches('-');
    if sanitized.is_empty() {
        SmolStr::new_static("Context-Capsule")
    } else {
        SmolStr::new(sanitized)
    }
}

pub(super) fn display_name_from_context_capsule_id(capsule_id: &str) -> SmolStr {
    let name_segment = capsule_id.splitn(4, '-').nth(3).unwrap_or(capsule_id);
    let mut output = String::with_capacity(name_segment.len());
    let mut last_space = false;
    for ch in name_segment.chars() {
        if matches!(ch, '-' | '_') {
            if !last_space {
                output.push(' ');
                last_space = true;
            }
        } else {
            output.push(ch);
            last_space = false;
        }
    }
    let display = output.trim();
    if display.is_empty() {
        SmolStr::new_static(context_menu_text("场景胶囊", "Context Capsule"))
    } else {
        SmolStr::new(display)
    }
}

pub(super) fn default_context_capsule_name() -> SmolStr {
    SmolStr::new(format!(
        "{} {}",
        context_menu_text("场景胶囊", "Context Capsule"),
        bento_nano_backend::time::now_compact_rfc3339()
    ))
}

pub(super) fn new_context_capsule_id(name: &str) -> SmolStr {
    let stamp = bento_nano_backend::time::now_compact_rfc3339();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.subsec_nanos());
    let safe_name = sanitize_context_capsule_name(name);
    SmolStr::new(format!(
        "{stamp}-{:x}-{nanos:08x}-{safe_name}",
        std::process::id()
    ))
}

pub(super) fn context_capsule_payload_is_json(payload: &[u8]) -> bool {
    payload
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        == Some(b'{')
}

pub(super) fn encode_context_capsule_envelope(
    zones: &ZoneList,
    name: &str,
    windows: Vec<ContextCapsuleWindow>,
) -> Result<Vec<u8>, ContextCapsuleError> {
    let envelope = ContextCapsuleEnvelope {
        schema: CONTEXT_CAPSULE_SCHEMA,
        name: name.trim().to_owned(),
        icon: "briefcase".to_owned(),
        captured_at: bento_nano_backend::time::now_rfc3339(),
        zones_codec: CONTEXT_CAPSULE_ZONES_CODEC.to_owned(),
        zones_bin_b64: bento_nano_backend::config_vault::wire::base64_encode(&storage::encode(
            zones,
        )),
        windows,
    };
    serde_json::to_vec_pretty(&envelope).map_err(ContextCapsuleError::Json)
}

pub(super) fn decode_context_capsule_envelope(
    payload: &[u8],
) -> Result<ContextCapsuleEnvelope, ContextCapsuleError> {
    let envelope: ContextCapsuleEnvelope = serde_json::from_slice(payload)?;
    if envelope.schema != CONTEXT_CAPSULE_SCHEMA {
        return Err(ContextCapsuleError::InvalidEnvelope(SmolStr::new(format!(
            "unsupported schema {}",
            envelope.schema
        ))));
    }
    if envelope.zones_codec != CONTEXT_CAPSULE_ZONES_CODEC {
        return Err(ContextCapsuleError::InvalidEnvelope(SmolStr::new(format!(
            "unsupported zones codec {}",
            envelope.zones_codec
        ))));
    }
    Ok(envelope)
}

pub(super) fn envelope_entry_from_payload(
    capsule_id: SmolStr,
    fallback_modified: SystemTime,
    payload: &[u8],
) -> CapsuleEntry {
    if context_capsule_payload_is_json(payload) {
        if let Ok(envelope) = decode_context_capsule_envelope(payload) {
            return CapsuleEntry::new(
                capsule_id,
                SmolStr::new(envelope.name),
                SmolStr::new(envelope.icon),
                envelope.captured_at,
            );
        }
    }
    CapsuleEntry::new(
        capsule_id.clone(),
        display_name_from_context_capsule_id(capsule_id.as_str()),
        "archive",
        bento_nano_backend::time::system_time_to_rfc3339(fallback_modified),
    )
}

pub(super) fn decode_context_capsule_zones(
    payload: &[u8],
) -> Result<ZoneList, ContextCapsuleError> {
    if !context_capsule_payload_is_json(payload) {
        return Ok(storage::decode(payload)?);
    }
    let envelope = decode_context_capsule_envelope(payload)?;
    let zones_payload =
        bento_nano_backend::config_vault::wire::base64_decode(&envelope.zones_bin_b64)?;
    let zones = storage::decode(&zones_payload)?;
    let report = restore_captured_context_windows(&envelope.windows);
    log_static(
        format!(
            "capsule: restore window report name={} total={} restored={} pending={} errors={}\n",
            envelope.name,
            envelope.windows.len(),
            report.restored,
            report.pending,
            report.errors
        )
        .as_str(),
    );
    tracing::info!(
        target: "bentodesk::context_capsule",
        capsule_name = %envelope.name,
        windows_total = envelope.windows.len(),
        windows_restored = report.restored,
        windows_pending = report.pending,
        windows_errors = report.errors,
        "restored selected-stack context capsule envelope"
    );
    Ok(zones)
}

pub(super) fn class_is_context_capsule_excluded(class_name: &str) -> bool {
    const BLACKLIST: &[&str] = &[
        "Progman",
        "WorkerW",
        "Shell_TrayWnd",
        "Shell_SecondaryTrayWnd",
        "Button",
        "NotifyIconOverflowWindow",
        "Windows.UI.Core.CoreWindow",
        "ApplicationFrameWindow",
        "Bento",
    ];
    BLACKLIST.iter().any(|needle| class_name.contains(needle))
}

pub(super) fn read_window_text(hwnd: HWND) -> Option<String> {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return None;
    }
    let mut buffer = vec![0u16; len as usize + 1];
    let copied = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
    if copied <= 0 {
        return None;
    }
    let title = String::from_utf16_lossy(&buffer[..copied as usize]);
    let title = title.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_owned())
    }
}

pub(super) fn read_window_class(hwnd: HWND) -> Option<String> {
    let mut buffer = [0u16; 256];
    let copied = unsafe { GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
    if copied <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..copied as usize]))
}

pub(super) fn lookup_context_process_name(pid: u32) -> String {
    if pid == 0 {
        return String::new();
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return String::new();
    }
    let mut buffer = [0u16; 260];
    let copied = unsafe {
        GetModuleFileNameExW(
            handle,
            ptr::null_mut(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    };
    unsafe {
        CloseHandle(handle);
    }
    if copied == 0 {
        return String::new();
    }
    let full = String::from_utf16_lossy(&buffer[..copied as usize]);
    full.rsplit(['\\', '/']).next().unwrap_or(&full).to_owned()
}

pub(super) fn enumerate_live_context_windows() -> Vec<LiveContextWindow> {
    let mut windows = Vec::<LiveContextWindow>::new();
    let lparam = &mut windows as *mut Vec<LiveContextWindow> as LPARAM;
    unsafe {
        EnumWindows(Some(enum_context_window_proc), lparam);
    }
    windows
}

pub(super) unsafe extern "system" fn enum_context_window_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out = unsafe { &mut *(lparam as *mut Vec<LiveContextWindow>) };
    if unsafe { IsWindowVisible(hwnd) } == 0 || unsafe { IsIconic(hwnd) } != 0 {
        return 1;
    }
    let Some(title) = read_window_text(hwnd) else {
        return 1;
    };
    let Some(class_name) = read_window_class(hwnd) else {
        return 1;
    };
    if class_is_context_capsule_excluded(&class_name) {
        return 1;
    }
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return 1;
    }
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    out.push(LiveContextWindow {
        hwnd,
        title,
        class_name,
        process_name: lookup_context_process_name(pid),
        rect: (rect.left, rect.top, rect.right, rect.bottom),
    });
    1
}

pub(super) fn capture_live_context_windows() -> Vec<ContextCapsuleWindow> {
    if cfg!(test) {
        return Vec::new();
    }
    enumerate_live_context_windows()
        .into_iter()
        .map(|window| ContextCapsuleWindow {
            title: window.title,
            class_name: window.class_name,
            process_name: window.process_name,
            rect: window.rect,
            is_maximized: unsafe { IsZoomed(window.hwnd) } != 0,
        })
        .collect()
}

pub(super) fn context_title_distance(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    let left: Vec<char> = a.chars().collect();
    let right: Vec<char> = b.chars().collect();
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];
    for (left_index, left_ch) in left.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_ch) in right.iter().enumerate() {
            let cost = usize::from(left_ch != right_ch);
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

pub(super) fn context_titles_similar(left: &str, right: &str) -> bool {
    let left = left.trim().to_lowercase();
    let right = right.trim().to_lowercase();
    if left.is_empty() || right.is_empty() {
        return false;
    }
    left == right
        || left.contains(&right)
        || right.contains(&left)
        || context_title_distance(&left, &right) <= 3
}

pub(super) fn match_context_window(
    captured: &ContextCapsuleWindow,
    live: &[LiveContextWindow],
) -> Option<HWND> {
    let mut best: Option<(HWND, i32, i64)> = None;
    for window in live {
        let mut score = 0i32;
        if !captured.class_name.is_empty() && captured.class_name == window.class_name {
            score += 2;
        }
        if !captured.process_name.is_empty()
            && captured
                .process_name
                .eq_ignore_ascii_case(window.process_name.as_str())
        {
            score += 2;
        }
        if context_titles_similar(&captured.title, &window.title) {
            score += 1;
        }
        if score < 2 {
            continue;
        }
        let distance = (i64::from(window.rect.0) - i64::from(captured.rect.0)).abs()
            + (i64::from(window.rect.1) - i64::from(captured.rect.1)).abs();
        match best {
            None => best = Some((window.hwnd, score, distance)),
            Some((_, best_score, best_distance))
                if score > best_score || (score == best_score && distance < best_distance) =>
            {
                best = Some((window.hwnd, score, distance));
            }
            _ => {}
        }
    }
    best.map(|(hwnd, _, _)| hwnd)
}

pub(super) fn restore_captured_context_windows(
    windows: &[ContextCapsuleWindow],
) -> ContextWindowRestoreReport {
    if cfg!(test) {
        return ContextWindowRestoreReport::default();
    }
    let live = enumerate_live_context_windows();
    let mut report = ContextWindowRestoreReport::default();
    for window in windows {
        let Some(hwnd) = match_context_window(window, &live) else {
            report.pending = report.pending.saturating_add(1);
            continue;
        };
        let (left, top, right, bottom) = window.rect;
        let width = (right - left).max(CONTEXT_WINDOW_MIN_SIZE);
        let height = (bottom - top).max(CONTEXT_WINDOW_MIN_SIZE);
        if window.is_maximized {
            unsafe {
                ShowWindow(hwnd, SW_MAXIMIZE);
            }
            report.restored = report.restored.saturating_add(1);
            continue;
        }
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
        }
        let ok = unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOP,
                left,
                top,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        };
        if ok == 0 {
            report.errors = report.errors.saturating_add(1);
        } else {
            report.restored = report.restored.saturating_add(1);
        }
    }
    report
}

pub(super) fn context_capsule_capture_name(root: &AppRoot) -> SmolStr {
    let app = root.app.borrow();
    let picker = app.capsule_picker.borrow();
    let name = picker.new_name().trim();
    if name.is_empty() {
        default_context_capsule_name()
    } else {
        SmolStr::new(name)
    }
}

pub(super) fn selected_context_capsule_id(root: &AppRoot) -> Option<SmolStr> {
    let app = root.app.borrow();
    app.capsule_picker
        .borrow()
        .selected_entry()
        .map(|entry| entry.id.clone())
}

pub(super) fn set_context_capsule_picker_error(root: &AppRoot, message: SmolStr) {
    let app = root.app.borrow();
    let mut picker = app.capsule_picker.borrow_mut();
    picker.set_busy(false);
    picker.set_error(Some(message));
}

pub(super) fn clear_context_capsule_picker_error(root: &AppRoot) {
    let app = root.app.borrow();
    let mut picker = app.capsule_picker.borrow_mut();
    picker.set_busy(false);
    picker.set_error(None);
}

pub(super) fn collect_context_capsule_candidates(
    zones_path: &Path,
) -> Result<Vec<ContextCapsuleCandidate>, ContextCapsuleError> {
    let capsule_dir = context_capsule_dir_for_zones_path(zones_path)?;
    if !capsule_dir.exists() {
        return Ok(Vec::new());
    }
    let mut candidates = Vec::new();
    let read_dir = std::fs::read_dir(&capsule_dir).map_err(|source| ContextCapsuleError::Io {
        op: "read capsule dir",
        path: capsule_dir.clone(),
        source,
    })?;
    for item in read_dir {
        let dir_entry = item.map_err(|source| ContextCapsuleError::Io {
            op: "read capsule dir entry",
            path: capsule_dir.clone(),
            source,
        })?;
        let path = dir_entry.path();
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(capsule_id) = context_capsule_id_from_file_name(file_name) else {
            continue;
        };
        let capsule_id = SmolStr::new(capsule_id);
        let metadata = dir_entry
            .metadata()
            .map_err(|source| ContextCapsuleError::Io {
                op: "read capsule metadata",
                path: path.clone(),
                source,
            })?;
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
        let entry = match std::fs::read(&path) {
            Ok(payload) => envelope_entry_from_payload(capsule_id.clone(), modified, &payload),
            Err(_) => CapsuleEntry::new(
                capsule_id.clone(),
                display_name_from_context_capsule_id(capsule_id.as_str()),
                "archive",
                bento_nano_backend::time::system_time_to_rfc3339(modified),
            ),
        };
        candidates.push(ContextCapsuleCandidate {
            path,
            entry,
            modified,
        });
    }
    candidates.sort_by(|left, right| right.modified.cmp(&left.modified));
    Ok(candidates)
}

pub(super) fn list_context_capsules_for_path(
    zones_path: &Path,
) -> Result<smallvec::SmallVec<[CapsuleEntry; 8]>, ContextCapsuleError> {
    let mut entries = smallvec::SmallVec::new();
    for candidate in collect_context_capsule_candidates(zones_path)?
        .into_iter()
        .take(32)
    {
        entries.push(candidate.entry);
    }
    Ok(entries)
}

pub(super) fn find_context_capsule_file_by_id(
    zones_path: &Path,
    capsule_id: &str,
) -> Result<PathBuf, ContextCapsuleError> {
    if capsule_id.trim().is_empty() {
        return Err(ContextCapsuleError::EmptyCapsuleId);
    }
    let candidates = collect_context_capsule_candidates(zones_path)?;
    candidates
        .into_iter()
        .find(|candidate| candidate.entry.id.as_str() == capsule_id)
        .map(|candidate| candidate.path)
        .ok_or_else(|| ContextCapsuleError::CapsuleNotFound(SmolStr::new(capsule_id)))
}

pub(super) fn capture_context_capsule_for_path(
    zones_path: &Path,
    zones: &ZoneList,
    name: &str,
) -> Result<CapsuleEntry, ContextCapsuleError> {
    let capsule_dir = context_capsule_dir_for_zones_path(zones_path)?;
    std::fs::create_dir_all(&capsule_dir).map_err(|source| ContextCapsuleError::Io {
        op: "create capsule dir",
        path: capsule_dir.clone(),
        source,
    })?;
    let capsule_id = new_context_capsule_id(name);
    let capsule_path = capsule_dir.join(context_capsule_file_name(capsule_id.as_str()));
    let temp_path = capsule_dir.join(format!("{capsule_id}.tmp"));
    let payload = encode_context_capsule_envelope(zones, name, capture_live_context_windows())?;
    std::fs::write(&temp_path, payload).map_err(|source| ContextCapsuleError::Io {
        op: "write capsule temp file",
        path: temp_path.clone(),
        source,
    })?;
    if let Err(source) = std::fs::rename(&temp_path, &capsule_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(ContextCapsuleError::Io {
            op: "promote capsule file",
            path: capsule_path,
            source,
        });
    }
    Ok(CapsuleEntry::new(
        capsule_id.clone(),
        SmolStr::new(name.trim()),
        "briefcase",
        bento_nano_backend::time::now_rfc3339(),
    ))
}

pub(super) fn restore_context_capsule_for_path(
    zones_path: &Path,
    capsule_id: &str,
) -> Result<ZoneList, ContextCapsuleError> {
    let capsule_path = find_context_capsule_file_by_id(zones_path, capsule_id)?;
    let payload = std::fs::read(&capsule_path).map_err(|source| ContextCapsuleError::Io {
        op: "read capsule file",
        path: capsule_path.clone(),
        source,
    })?;
    decode_context_capsule_zones(&payload)
}

pub(super) fn delete_context_capsule_for_path(
    zones_path: &Path,
    capsule_id: &str,
) -> Result<(), ContextCapsuleError> {
    let capsule_path = find_context_capsule_file_by_id(zones_path, capsule_id)?;
    std::fs::remove_file(&capsule_path).map_err(|source| ContextCapsuleError::Io {
        op: "delete capsule file",
        path: capsule_path,
        source,
    })
}

pub(super) fn refresh_context_capsule_picker(root: &AppRoot) -> Result<(), ContextCapsuleError> {
    let zones_path = root.app.borrow().zones_path.clone();
    let entries = list_context_capsules_for_path(&zones_path)?;
    let app = root.app.borrow();
    let mut picker = app.capsule_picker.borrow_mut();
    picker.set_entries(entries);
    picker.set_busy(false);
    picker.set_error(None);
    Ok(())
}

pub(super) fn capture_context_capsule(
    root: &AppRoot,
    name: &str,
) -> Result<CapsuleEntry, ContextCapsuleError> {
    let (zones_path, zones) = {
        let app = root.app.borrow();
        (app.zones_path.clone(), app.zones.clone())
    };
    let entry = capture_context_capsule_for_path(&zones_path, &zones, name)?;
    refresh_context_capsule_picker(root)?;
    Ok(entry)
}

pub(super) fn restore_context_capsule(
    root: &AppRoot,
    capsule_id: &str,
) -> Result<usize, ContextCapsuleError> {
    let zones_path = root.app.borrow().zones_path.clone();
    let zones = restore_context_capsule_for_path(&zones_path, capsule_id)?;
    let restored_count = zones.len();
    {
        let mut app = root.app.borrow_mut();
        app.zones = zones;
        app.mark_dirty();
    }
    refresh_context_capsule_picker(root)?;
    Ok(restored_count)
}

pub(super) fn delete_context_capsule(
    root: &AppRoot,
    capsule_id: &str,
) -> Result<(), ContextCapsuleError> {
    let zones_path = root.app.borrow().zones_path.clone();
    delete_context_capsule_for_path(&zones_path, capsule_id)?;
    refresh_context_capsule_picker(root)
}
