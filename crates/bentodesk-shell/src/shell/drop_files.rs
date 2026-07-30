//! Native shell owner: `drop_files`.

use super::*;

pub(super) fn queue_add_items(
    root: &AppRoot,
    zone_id: ZoneId,
    files: Vec<String>,
    source: &'static str,
) {
    if files.is_empty() {
        return;
    }
    let file_count = files.len();
    for file in files {
        root.dispatcher.push(Command::AddItem(
            zone_id,
            bentodesk_app::ItemPath::new(file),
        ));
    }
    log_static(
        format!(
            "items: queued dropped files source={} zone_id={} file_count={}\n",
            source, zone_id.0, file_count
        )
        .as_str(),
    );
    tracing::info!(
        target: "bentodesk::items",
        ?zone_id,
        file_count,
        source,
        "queued dropped files into zone"
    );
}

pub(super) fn zone_for_client_device_point(
    root: &AppRoot,
    slot: &WindowSlot,
    device_x: i32,
    device_y: i32,
) -> Option<ZoneId> {
    let app = root.app.borrow();
    let dpi = slot.state.dpi.get();
    let x = bentodesk_style::dpi::device_to_logical_f32(device_x as f32, dpi);
    let y = bentodesk_style::dpi::device_to_logical_f32(device_y as f32, dpi);
    ui::hit_test_zone(&app, x, y)
}

pub(super) fn zone_for_screen_point(
    root: &AppRoot,
    hwnd: HWND,
    point: bentodesk_backend::drag_drop::DropPoint,
) -> Option<ZoneId> {
    let p = unsafe { get_slot_ptr(hwnd) };
    if p.is_null() {
        return None;
    }
    let slot = unsafe { &*p };
    let mut client = POINT {
        x: point.x,
        y: point.y,
    };
    // SAFETY: hwnd is owned by the shell; ScreenToClient mutates `client`
    // in-place and is tolerant of normal top-level windows.
    unsafe { ScreenToClient(hwnd, &mut client) };
    zone_for_client_device_point(root, slot, client.x, client.y)
}

pub(super) fn ole_drop_can_accept(point: bentodesk_backend::drag_drop::DropPoint) -> bool {
    let Some(root) = app_root() else {
        return false;
    };
    if root.item_drag_out_active.get() {
        tracing::debug!(
            target: "bentodesk::drag_drop",
            ?point,
            "OLE drop target rejected while BentoDesk is the drag source"
        );
        return false;
    }
    let Some(hwnd) = find_main_hwnd(root) else {
        return false;
    };
    zone_for_screen_point(root, hwnd, point).is_some()
}

pub(super) fn ole_drop_commit(point: bentodesk_backend::drag_drop::DropPoint, files: Vec<String>) {
    let Some(root) = app_root() else {
        return;
    };
    if root.item_drag_out_active.get() {
        tracing::warn!(
            target: "bentodesk::items",
            file_count = files.len(),
            "OLE drop ignored because BentoDesk is currently dragging an item out"
        );
        log_static(
            format!(
                "items: ignored self OLE drop during drag-out file_count={}\n",
                files.len()
            )
            .as_str(),
        );
        return;
    }
    let Some(hwnd) = find_main_hwnd(root) else {
        return;
    };
    let Some(zone_id) = zone_for_screen_point(root, hwnd, point) else {
        tracing::warn!(
            target: "bentodesk::items",
            file_count = files.len(),
            "OLE drop ignored: screen point is not inside a zone"
        );
        return;
    };
    queue_add_items(root, zone_id, files, "ole_drop_target");
    consume_dispatcher(root, hwnd);
    request_redraw(hwnd);
}

pub(super) struct DropFilesPayload {
    files: Vec<String>,
    client_point: Option<POINT>,
    raw_payload: bool,
}

pub(super) fn handle_drop_files(root: &AppRoot, slot: &WindowSlot, hdrop: HDROP) -> bool {
    let payload = collect_drop_files(hdrop);
    let raw_payload = payload.raw_payload;
    let files = payload.files;
    log_static(format!("wm_dropfiles: files={}\n", files.len()).as_str());
    if files.is_empty() {
        tracing::warn!(
            target: "bentodesk::items",
            "WM_DROPFILES contained no readable filesystem paths"
        );
        log_static("wm_dropfiles: no readable paths\n");
        return raw_payload;
    }

    let mut point = payload.client_point.unwrap_or(POINT { x: 0, y: 0 });
    let has_point = if payload.client_point.is_some() {
        true
    } else {
        (unsafe { DragQueryPoint(hdrop, &mut point) }) != 0
    };
    log_static(
        format!(
            "wm_dropfiles: has_point={} client=({}, {})\n",
            has_point, point.x, point.y
        )
        .as_str(),
    );
    let zone_id = {
        if !has_point {
            None
        } else {
            zone_for_client_device_point(root, slot, point.x, point.y)
        }
    };

    let Some(zone_id) = zone_id else {
        tracing::warn!(
            target: "bentodesk::items",
            file_count = files.len(),
            "WM_DROPFILES ignored: drop point is not inside a zone"
        );
        log_static(
            format!(
                "wm_dropfiles: ignored outside zone files={} client=({}, {})\n",
                files.len(),
                point.x,
                point.y
            )
            .as_str(),
        );
        return raw_payload;
    };

    queue_add_items(root, zone_id, files, "wm_dropfiles");
    raw_payload
}

pub(super) fn finish_drop_files_handle(hdrop: HDROP, raw_payload: bool) {
    if raw_payload {
        log_static("wm_dropfiles: skipped DragFinish for raw DROPFILES payload\n");
        return;
    }
    let flags = unsafe { GlobalFlags(hdrop.cast()) };
    let size = unsafe { GlobalSize(hdrop.cast()) };
    if flags == GMEM_INVALID_HANDLE_FLAG || size == 0 {
        log_static("wm_dropfiles: skipped DragFinish for non-HGLOBAL payload\n");
        return;
    }
    unsafe { DragFinish(hdrop) };
}

pub(super) fn collect_drop_files(hdrop: HDROP) -> DropFilesPayload {
    if let Some(raw_payload) = collect_raw_dropfiles_payload(hdrop) {
        log_static(
            format!(
                "wm_dropfiles: raw DROPFILES fallback files={}\n",
                raw_payload.files.len()
            )
            .as_str(),
        );
        return raw_payload;
    }

    let count = unsafe { DragQueryFileW(hdrop, u32::MAX, core::ptr::null_mut(), 0) };
    if count == 0 || count > bentodesk_backend::drag_drop::MAX_DROPPED_FILES {
        return DropFilesPayload {
            files: Vec::new(),
            client_point: None,
            raw_payload: false,
        };
    }
    let mut files = Vec::with_capacity(count as usize);
    let mut total_path_chars = 0usize;
    for idx in 0..count {
        let len = unsafe { DragQueryFileW(hdrop, idx, core::ptr::null_mut(), 0) };
        if len == 0 {
            continue;
        }
        total_path_chars = total_path_chars.saturating_add(len as usize);
        if len > bentodesk_backend::drag_drop::MAX_DROPPED_PATH_CHARS
            || total_path_chars > bentodesk_backend::drag_drop::MAX_DROPPED_TOTAL_PATH_CHARS
        {
            return DropFilesPayload {
                files: Vec::new(),
                client_point: None,
                raw_payload: false,
            };
        }
        let mut buf = vec![0u16; len as usize + 1];
        let written = unsafe { DragQueryFileW(hdrop, idx, buf.as_mut_ptr(), buf.len() as u32) };
        if written == 0 {
            continue;
        }
        files.push(String::from_utf16_lossy(&buf[..written as usize]));
    }
    DropFilesPayload {
        files,
        client_point: None,
        raw_payload: false,
    }
}

pub(super) fn collect_raw_dropfiles_payload(hdrop: HDROP) -> Option<DropFilesPayload> {
    let available = readable_region_size(hdrop.cast_const())?;
    if available < DROPFILES_HEADER_LEN {
        return None;
    }
    let readable_len = available.min(MAX_RAW_DROPFILES_BYTES);
    let payload = unsafe { std::slice::from_raw_parts(hdrop.cast::<u8>(), readable_len) };
    let pfiles = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
    if !(DROPFILES_HEADER_LEN..readable_len).contains(&pfiles) {
        return None;
    }
    let point = POINT {
        x: i32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]),
        y: i32::from_le_bytes([payload[8], payload[9], payload[10], payload[11]]),
    };
    let f_wide = u32::from_le_bytes([payload[16], payload[17], payload[18], payload[19]]) != 0;
    let files = if f_wide {
        collect_raw_dropfiles_wide(&payload[pfiles..])
    } else {
        collect_raw_dropfiles_ansi(&payload[pfiles..])
    };
    if files.is_empty() {
        return None;
    }
    Some(DropFilesPayload {
        files,
        client_point: Some(point),
        raw_payload: true,
    })
}

pub(super) fn collect_raw_dropfiles_wide(data: &[u8]) -> Vec<String> {
    let mut files = Vec::new();
    let mut current = Vec::new();
    for chunk in data.chunks_exact(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        if unit == 0 {
            if current.is_empty() {
                break;
            }
            files.push(String::from_utf16_lossy(&current));
            current.clear();
            continue;
        }
        if current.len() >= MAX_RAW_DROPFILES_PATH_CHARS {
            return Vec::new();
        }
        current.push(unit);
    }
    files
}

pub(super) fn collect_raw_dropfiles_ansi(data: &[u8]) -> Vec<String> {
    let mut files = Vec::new();
    let mut current = Vec::new();
    for byte in data.iter().copied() {
        if byte == 0 {
            if current.is_empty() {
                break;
            }
            files.push(String::from_utf8_lossy(&current).into_owned());
            current.clear();
            continue;
        }
        if current.len() >= MAX_RAW_DROPFILES_PATH_CHARS {
            return Vec::new();
        }
        current.push(byte);
    }
    files
}

pub(super) fn readable_region_size(address: *const core::ffi::c_void) -> Option<usize> {
    if address.is_null() {
        return None;
    }
    let mut info = std::mem::MaybeUninit::<MEMORY_BASIC_INFORMATION>::uninit();
    let written = unsafe {
        VirtualQuery(
            address,
            info.as_mut_ptr(),
            std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
        )
    };
    if written == 0 {
        return None;
    }
    let info = unsafe { info.assume_init() };
    if info.State != MEM_COMMIT || !is_readable_memory_protect(info.Protect) {
        return None;
    }
    Some(info.RegionSize)
}

pub(super) fn is_readable_memory_protect(protect: u32) -> bool {
    if protect & (PAGE_GUARD | PAGE_NOACCESS) != 0 {
        return false;
    }
    matches!(
        protect & 0xFF,
        PAGE_READONLY
            | PAGE_READWRITE
            | PAGE_WRITECOPY
            | PAGE_EXECUTE_READ
            | PAGE_EXECUTE_READWRITE
            | PAGE_EXECUTE_WRITECOPY
    )
}
