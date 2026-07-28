//! Native shell owner: `live_folders`.

use super::*;

pub(super) fn live_folder_items_from_path(
    folder: &Path,
    grid_columns: u32,
    preserved_tags: &[(String, smallvec::SmallVec<[Cow<'static, str>; 4]>)],
) -> Result<smallvec::SmallVec<[ZoneItem; 16]>, String> {
    bentodesk_backend::watcher::validate_folder(folder).map_err(|error| error.to_string())?;
    let read_dir = std::fs::read_dir(folder)
        .map_err(|error| format!("live folder scan failed for {}: {error}", folder.display()))?;
    let mut entries = Vec::new();
    for entry_result in read_dir {
        let entry = entry_result.map_err(|error| {
            format!(
                "live folder entry read failed for {}: {error}",
                folder.display()
            )
        })?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        entries.push((name.to_owned(), path));
    }
    entries.sort_by(|left, right| left.0.to_lowercase().cmp(&right.0.to_lowercase()));

    let columns = grid_columns.max(1) as i32;
    let mut items = smallvec::SmallVec::<[ZoneItem; 16]>::new();
    for (index, (_name, path)) in entries.into_iter().enumerate() {
        let path_string = path.to_string_lossy().to_string();
        let icon_hash = load_icon_hash_for_path(&bentodesk_app::ItemPath::new(&path_string))
            .unwrap_or_else(|| "live-folder".to_owned());
        let mut item = ZoneItem::new(
            ZoneItemId((index + 1) as u64),
            Cow::Owned(path_string.clone()),
            Cow::Owned(icon_hash),
            index as i32 % columns,
            index as i32 / columns,
        );
        if let Some((_, tags)) = preserved_tags
            .iter()
            .find(|(existing_path, _)| existing_path == &path_string)
        {
            item.tags = tags.clone();
        }
        items.push(item);
    }
    Ok(items)
}

pub(super) fn live_folder_path_for_zone(root: &AppRoot, zone_id: ZoneId) -> Option<PathBuf> {
    root.app
        .borrow()
        .zones
        .get(zone_id)
        .and_then(|zone| zone.live_folder_path.as_deref())
        .map(PathBuf::from)
}

pub(super) fn set_live_folder_status_for_app(app: &AppState, message: impl Into<SmolStr>) {
    let message = message.into();
    app.rules_wizard_status
        .borrow_mut()
        .replace(message.clone());
    app.item_operation_status
        .borrow_mut()
        .replace(message.clone());
    log_static(format!("live-folder: {message}\n").as_str());
}

pub(super) fn set_live_folder_status(root: &AppRoot, message: impl Into<SmolStr>) {
    let app = root.app.borrow();
    set_live_folder_status_for_app(&app, message);
}

pub(super) fn refresh_live_folder_zone(root: &AppRoot, zone_id: ZoneId) -> Result<bool, String> {
    let (folder, grid_columns, preserved_tags) = {
        let app = root.app.borrow();
        let zone = app
            .zones
            .get(zone_id)
            .ok_or_else(|| format!("live folder zone not found: {}", zone_id.0))?;
        let folder = zone
            .live_folder_path
            .as_deref()
            .ok_or_else(|| format!("zone {} has no live folder binding", zone_id.0))?;
        let preserved_tags = zone
            .items
            .iter()
            .map(|item| (item.path.to_string(), item.tags.clone()))
            .collect::<Vec<_>>();
        (PathBuf::from(folder), zone.grid_columns, preserved_tags)
    };

    let next_items = live_folder_items_from_path(&folder, grid_columns, &preserved_tags)?;
    let item_count = next_items.len();
    let mut app = root.app.borrow_mut();
    {
        let Some(zone) = app.zones.get_mut(zone_id) else {
            return Err(format!("live folder zone not found: {}", zone_id.0));
        };
        if zone.live_folder_path.as_deref() != Some(folder.to_string_lossy().as_ref()) {
            set_live_folder_status_for_app(
                &app,
                localized_current(
                    format!("区域 {} 的文件夹绑定已变化，已跳过刷新", zone_id.0),
                    format!(
                        "Live folder skipped zone {} because its binding changed",
                        zone_id.0
                    ),
                ),
            );
            return Ok(true);
        }
        if zone.items == next_items {
            set_live_folder_status_for_app(
                &app,
                localized_current(
                    format!(
                        "区域 {} 的绑定文件夹已检查：{}，共 {item_count} 个项目，无变化",
                        zone_id.0,
                        folder.display()
                    ),
                    format!(
                        "Live folder checked zone {} from {}; items={item_count}; no changes",
                        zone_id.0,
                        folder.display()
                    ),
                ),
            );
            return Ok(true);
        }
        zone.items = next_items;
    }
    set_live_folder_status_for_app(
        &app,
        localized_current(
            format!(
                "区域 {} 的绑定文件夹已刷新：{}，共 {item_count} 个项目",
                zone_id.0,
                folder.display()
            ),
            format!(
                "Live folder refreshed zone {} from {}; items={item_count}",
                zone_id.0,
                folder.display()
            ),
        ),
    );
    app.mark_dirty();
    Ok(true)
}

pub(super) fn set_live_folder_error(root: &AppRoot, message: impl Into<SmolStr>) {
    set_live_folder_status(root, message);
}

pub(super) fn live_folder_picker_host_exit_code_from_args() -> Option<i32> {
    let mut args = std::env::args();
    let _exe = args.next();
    let flag = args.next()?;
    if flag != LIVE_FOLDER_PICKER_HOST_ARG {
        return None;
    }

    let zone_id = match args.next() {
        Some(raw) => match raw.parse::<u64>() {
            Ok(value) => ZoneId(value),
            Err(error) => {
                log_static(
                    format!("live-folder: picker host invalid zone id {raw}: {error}\n").as_str(),
                );
                return Some(LIVE_FOLDER_PICKER_HOST_ERROR_EXIT);
            }
        },
        None => ZoneId(0),
    };

    let result_path = args.next().map(PathBuf::from);
    Some(live_folder_picker_host_exit_code(
        zone_id,
        result_path.as_deref(),
    ))
}

pub(super) enum LiveFolderPickerHostOutcome {
    Selected(SmolStr),
    Canceled,
    Error(String),
}

pub(super) fn live_folder_picker_host_exit_code(
    zone_id: ZoneId,
    result_path: Option<&Path>,
) -> i32 {
    log_static(
        format!(
            "live-folder: picker host mode pid={} opening zone_id={}\n",
            std::process::id(),
            zone_id.0
        )
        .as_str(),
    );
    // SAFETY: The helper mode runs before the normal D2D/DComp shell initializes
    // and owns the full Shell picker lifetime in this short-lived process.
    let outcome = match unsafe { select_live_folder_from_dialog_on_sta(ptr::null_mut(), zone_id) } {
        Ok(Some(folder)) => LiveFolderPickerHostOutcome::Selected(folder),
        Ok(None) => LiveFolderPickerHostOutcome::Canceled,
        Err(error) => LiveFolderPickerHostOutcome::Error(error),
    };
    if let Some(path) = result_path {
        if let Err(error) = write_live_folder_picker_host_result(path, &outcome) {
            log_static(format!("live-folder: picker host result write failed: {error}\n").as_str());
            return LIVE_FOLDER_PICKER_HOST_ERROR_EXIT;
        }
    }
    match outcome {
        LiveFolderPickerHostOutcome::Selected(folder) => {
            println!("{folder}");
            LIVE_FOLDER_PICKER_HOST_SELECTED_EXIT
        }
        LiveFolderPickerHostOutcome::Canceled => LIVE_FOLDER_PICKER_HOST_CANCEL_EXIT,
        LiveFolderPickerHostOutcome::Error(error) => {
            log_static(format!("live-folder: picker host failed: {error}\n").as_str());
            LIVE_FOLDER_PICKER_HOST_ERROR_EXIT
        }
    }
}

pub(super) fn write_live_folder_picker_host_result(
    path: &Path,
    outcome: &LiveFolderPickerHostOutcome,
) -> Result<(), String> {
    let content = match outcome {
        LiveFolderPickerHostOutcome::Selected(folder) => format!("selected\n{folder}\n"),
        LiveFolderPickerHostOutcome::Canceled => "canceled\n".to_owned(),
        LiveFolderPickerHostOutcome::Error(error) => format!("error\n{error}\n"),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create result dir {} failed: {error}", parent.display()))?;
    }
    std::fs::write(path, content)
        .map_err(|error| format!("write picker result {} failed: {error}", path.display()))
}

pub(super) fn read_live_folder_picker_host_result(path: &Path) -> Result<Option<SmolStr>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("read picker result {} failed: {error}", path.display()))?;
    let mut lines = content.lines();
    match lines.next() {
        Some("selected") => {
            let folder = lines.next().unwrap_or_default().trim();
            if folder.is_empty() {
                Err("native folder picker host returned an empty folder path".to_owned())
            } else {
                Ok(Some(SmolStr::new(folder)))
            }
        }
        Some("canceled") => Ok(None),
        Some("error") => {
            let message = lines.collect::<Vec<_>>().join("\n");
            if message.trim().is_empty() {
                Err("native folder picker host reported an error".to_owned())
            } else {
                Err(message)
            }
        }
        Some(status) => Err(format!(
            "native folder picker host returned unknown status {status}"
        )),
        None => Err("native folder picker host returned an empty result file".to_owned()),
    }
}

pub(super) fn live_folder_picker_result_path(zone_id: ZoneId) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    std::env::temp_dir().join(format!(
        "bentodesk-live-folder-picker-{}-{}-{millis}.txt",
        std::process::id(),
        zone_id.0
    ))
}

pub(super) fn quote_windows_arg(arg: &str) -> String {
    if !arg.is_empty()
        && !arg
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '"'))
    {
        return arg.to_owned();
    }
    let mut quoted = String::with_capacity(arg.len() + 2);
    quoted.push('"');
    for ch in arg.chars() {
        if ch == '"' {
            quoted.push('\\');
        }
        quoted.push(ch);
    }
    quoted.push('"');
    quoted
}

pub(super) fn select_live_folder_from_dialog(
    _owner: HWND,
    zone_id: ZoneId,
) -> Result<Option<SmolStr>, String> {
    let exe = std::env::current_exe()
        .map_err(|error| format!("native folder picker host path unavailable: {error}"))?;
    let result_path = live_folder_picker_result_path(zone_id);
    if result_path.exists() {
        std::fs::remove_file(&result_path).map_err(|error| {
            format!(
                "remove stale picker result {} failed: {error}",
                result_path.display()
            )
        })?;
    }
    log_static(
        format!(
            "live-folder: picker host launching pid={} exe={} zone_id={} result={}\n",
            std::process::id(),
            exe.display(),
            zone_id.0,
            result_path.display()
        )
        .as_str(),
    );
    let parameters = format!(
        "{} {} {}",
        LIVE_FOLDER_PICKER_HOST_ARG,
        zone_id.0,
        quote_windows_arg(&result_path.display().to_string())
    );
    let operation = widen_dynamic("open");
    let exe_w = widen_dynamic(&exe.display().to_string());
    let parameters_w = widen_dynamic(&parameters);
    // SAFETY: ShellExecuteW is called with null-terminated UTF-16 strings that
    // remain alive for the duration of the call. The helper process writes its
    // result to `result_path`; no raw handles are shared with the UI process.
    let launch_result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            exe_w.as_ptr(),
            parameters_w.as_ptr(),
            ptr::null(),
            SW_SHOW,
        )
    };
    if launch_result as isize <= 32 {
        return Err(format!(
            "native folder picker host launch failed: ShellExecuteW={}",
            launch_result as isize
        ));
    }

    let started = SystemTime::now();
    loop {
        if result_path.exists() {
            let result = read_live_folder_picker_host_result(&result_path);
            let _ = std::fs::remove_file(&result_path);
            log_static(
                format!(
                    "live-folder: picker host result received pid={} path={}\n",
                    std::process::id(),
                    result_path.display()
                )
                .as_str(),
            );
            return result;
        }
        let elapsed = SystemTime::now()
            .duration_since(started)
            .unwrap_or_default();
        if elapsed >= LIVE_FOLDER_PICKER_HOST_TIMEOUT {
            return Err(format!(
                "native folder picker host timed out waiting for {}",
                result_path.display()
            ));
        }
        std::thread::sleep(LIVE_FOLDER_PICKER_HOST_POLL);
    }
}

pub(super) unsafe fn select_live_folder_from_dialog_on_sta(
    owner: HWND,
    zone_id: ZoneId,
) -> Result<Option<SmolStr>, String> {
    log_static("live-folder: picker initializing OLE STA\n");
    // SAFETY: The picker runs on a dedicated worker thread. OleInitialize
    // enters the STA/OLE apartment required by Shell common dialogs and is
    // balanced by OleUninitialize after the dialog object has been dropped.
    unsafe { OleInitialize(None) }
        .map_err(|error| format_windows_error("native folder picker OLE init failed", error))?;
    log_static("live-folder: picker OLE STA initialized\n");

    let result = unsafe { select_live_folder_from_file_open_dialog(owner, zone_id) };
    // SAFETY: This balances the successful OleInitialize call made above in the
    // same worker thread.
    unsafe { OleUninitialize() };
    result
}

pub(super) unsafe fn select_live_folder_from_file_open_dialog(
    owner: HWND,
    zone_id: ZoneId,
) -> Result<Option<SmolStr>, String> {
    let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);
    let title_text = localized_message(
        zh,
        format!("为区域 {} 绑定文件夹", zone_id.0),
        format!("Bind live folder to zone {}", zone_id.0),
    );
    let title = widen_dynamic(title_text.as_str());
    let ok_label = widen_dynamic(if zh {
        "选择文件夹"
    } else {
        "Select folder"
    });
    log_static("live-folder: picker showing file-open folder dialog\n");
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }.map_err(
            |error| format_windows_error("native folder picker create dialog failed", error),
        )?;
    let options = unsafe { dialog.GetOptions() }
        .map_err(|error| format_windows_error("native folder picker read options failed", error))?;
    unsafe {
        dialog
            .SetOptions(
                options
                    | FOS_PICKFOLDERS
                    | FOS_FORCEFILESYSTEM
                    | FOS_PATHMUSTEXIST
                    | FOS_NOCHANGEDIR,
            )
            .map_err(|error| {
                format_windows_error("native folder picker set options failed", error)
            })?;
        dialog
            .SetTitle(PCWSTR(title.as_ptr() as _))
            .map_err(|error| {
                format_windows_error("native folder picker set title failed", error)
            })?;
        dialog
            .SetOkButtonLabel(PCWSTR(ok_label.as_ptr() as _))
            .map_err(|error| {
                format_windows_error("native folder picker set OK label failed", error)
            })?;
    }
    let owner = WindowsHwnd(owner as _);
    match unsafe { dialog.Show(owner) } {
        Ok(()) => {}
        Err(error) if is_windows_error_cancelled(&error) => {
            log_static("live-folder: picker file-open dialog canceled\n");
            return Ok(None);
        }
        Err(error) => {
            return Err(format_windows_error(
                "native folder picker show dialog failed",
                error,
            ));
        }
    }
    let item = unsafe { dialog.GetResult() }
        .map_err(|error| format_windows_error("native folder picker result failed", error))?;
    let display_name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }
        .map_err(|error| format_windows_error("native folder picker path failed", error))?;
    let path = unsafe { display_name.to_string() }
        .map_err(|error| format!("native folder picker path UTF-16 decode failed: {error}"))?;
    unsafe { CoTaskMemFree(display_name.as_ptr() as *const _) };
    let folder = path.trim();
    if folder.is_empty() {
        return Err("native folder picker returned an empty folder path".to_owned());
    }
    Ok(Some(SmolStr::new(folder)))
}

pub(super) fn is_windows_error_cancelled(error: &WindowsError) -> bool {
    const HRESULT_FROM_WIN32_ERROR_CANCELLED: i32 = -2147023673;
    error.code().0 == HRESULT_FROM_WIN32_ERROR_CANCELLED
}

pub(super) fn format_windows_error(context: &str, error: WindowsError) -> String {
    format!(
        "{context}: HRESULT 0x{:08X}: {error}",
        error.code().0 as u32
    )
}

pub(super) fn open_live_folder_picker(root: &AppRoot, zone_id: ZoneId) -> bool {
    let owner = find_main_hwnd(root).or_else(|| find_aux_window(root, WindowKind::ContextMenu));
    let Some(owner) = owner else {
        set_live_folder_error(
            root,
            localized_current(
                format!("区域 {} 绑定文件夹失败：窗口不可用", zone_id.0),
                format!(
                    "Bind live folder picker failed for zone {}: owner window unavailable",
                    zone_id.0
                ),
            ),
        );
        return true;
    };
    log_static(format!("live-folder: picker opening zone_id={}\n", zone_id.0).as_str());
    match select_live_folder_from_dialog(owner, zone_id) {
        Ok(Some(folder)) => {
            log_static(
                format!(
                    "live-folder: picker selected zone_id={} folder={}\n",
                    zone_id.0, folder
                )
                .as_str(),
            );
            root.dispatcher
                .push(Command::BindZoneToFolder(zone_id, folder));
            true
        }
        Ok(None) => {
            log_static(format!("live-folder: picker canceled zone_id={}\n", zone_id.0).as_str());
            false
        }
        Err(error) => {
            tracing::warn!(
                target: "bentodesk::live_folder",
                zone_id = zone_id.0,
                error = %error,
                "native live folder picker failed"
            );
            set_live_folder_error(
                root,
                localized_current(
                    format!("区域 {} 绑定文件夹失败：{error}", zone_id.0),
                    format!(
                        "Bind live folder picker failed for zone {}: {error}",
                        zone_id.0
                    ),
                ),
            );
            request_redraw(owner);
            true
        }
    }
}

pub(super) unsafe fn select_theme_file_from_dialog(owner: HWND) -> Result<Option<PathBuf>, String> {
    let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);
    unsafe {
        select_file_from_dialog(
            owner,
            if zh {
                "BentoDesk 主题 JSON (*.json)\0*.json\0所有文件 (*.*)\0*.*\0"
            } else {
                "BentoDesk theme JSON (*.json)\0*.json\0All files (*.*)\0*.*\0"
            },
            if zh {
                "导入 BentoDesk 主题 JSON"
            } else {
                "Import BentoDesk theme JSON"
            },
            "json",
            "theme",
        )
    }
}

pub(super) unsafe fn select_plugin_file_from_dialog(
    owner: HWND,
) -> Result<Option<PathBuf>, String> {
    let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);
    unsafe {
        select_file_from_dialog(
            owner,
            if zh {
                "BentoDesk 插件 (*.bdplugin;*.zip)\0*.bdplugin;*.zip\0所有文件 (*.*)\0*.*\0"
            } else {
                "BentoDesk plugin (*.bdplugin;*.zip)\0*.bdplugin;*.zip\0All files (*.*)\0*.*\0"
            },
            if zh {
                "安装 BentoDesk 插件"
            } else {
                "Install BentoDesk plugin"
            },
            "bdplugin",
            "plugin",
        )
    }
}

pub(super) unsafe fn select_file_from_dialog(
    owner: HWND,
    filter_text: &str,
    title_text: &str,
    default_extension_text: &str,
    failure_label: &str,
) -> Result<Option<PathBuf>, String> {
    const MAX_FILE_PATH_U16: usize = 4096;
    const MAX_FILE_PATH_U32: u32 = 4096;

    let filter = widen_dynamic(filter_text);
    let title = widen_dynamic(title_text);
    let default_extension = widen_dynamic(default_extension_text);
    let mut file_buffer = [0u16; MAX_FILE_PATH_U16];
    // SAFETY: OPENFILENAMEW is a plain C struct. We fill every field required
    // by GetOpenFileNameW before passing its mutable pointer to comdlg32.
    let mut dialog = unsafe { core::mem::zeroed::<OPENFILENAMEW>() };
    dialog.lStructSize = core::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.lpstrFile = file_buffer.as_mut_ptr();
    dialog.nMaxFile = MAX_FILE_PATH_U32;
    dialog.lpstrTitle = title.as_ptr();
    dialog.lpstrDefExt = default_extension.as_ptr();
    dialog.Flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_HIDEREADONLY | OFN_PATHMUSTEXIST;

    // SAFETY: dialog points to a live OPENFILENAMEW; string buffers remain
    // alive for the duration of the modal call; lpstrFile is writable.
    let accepted = unsafe { GetOpenFileNameW(&mut dialog) };
    if accepted == 0 {
        // SAFETY: CommDlgExtendedError is the documented way to distinguish
        // cancellation from a common-dialog failure after GetOpenFileNameW.
        let error = unsafe { CommDlgExtendedError() };
        if error == 0 {
            return Ok(None);
        }
        return Err(format!(
            "native {failure_label} picker failed: 0x{error:08X}"
        ));
    }

    let len = file_buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(file_buffer.len());
    let path = String::from_utf16_lossy(&file_buffer[..len]);
    let path = path.trim();
    if path.is_empty() {
        return Err(format!(
            "native {failure_label} picker returned an empty file path"
        ));
    }
    Ok(Some(PathBuf::from(path)))
}

pub(super) fn bind_zone_to_folder(
    root: &AppRoot,
    zone_id: ZoneId,
    folder: &str,
) -> Result<bool, String> {
    let folder = PathBuf::from(folder.trim());
    if root.app.borrow().zones.get(zone_id).is_none() {
        return Err(format!("live folder zone not found: {}", zone_id.0));
    }
    bentodesk_backend::watcher::bind(zone_id, &folder).map_err(|error| error.to_string())?;
    let changed = {
        let mut app = root.app.borrow_mut();
        let Some(zone) = app.zones.get_mut(zone_id) else {
            let _ = bentodesk_backend::watcher::unbind(zone_id);
            return Err(format!("live folder zone not found: {}", zone_id.0));
        };
        zone.set_live_folder_path(Some(Cow::Owned(folder.to_string_lossy().to_string())))
    };
    if changed {
        root.app.borrow().mark_dirty();
    }
    let refreshed = refresh_live_folder_zone(root, zone_id)?;
    let item_count = root
        .app
        .borrow()
        .zones
        .get(zone_id)
        .map(|zone| zone.items.len())
        .unwrap_or(0);
    root.app
        .borrow()
        .rules_wizard_status
        .borrow_mut()
        .replace(localized_current(
            format!(
                "区域 {} 已绑定文件夹 {}，共 {item_count} 个项目",
                zone_id.0,
                folder.display()
            ),
            format!(
                "Live folder bound zone {} to {}; items={item_count}",
                zone_id.0,
                folder.display()
            ),
        ));
    Ok(changed || refreshed)
}

pub(super) fn unbind_zone_folder(root: &AppRoot, zone_id: ZoneId) -> Result<bool, String> {
    bentodesk_backend::watcher::unbind(zone_id).map_err(|error| error.to_string())?;
    let mut app = root.app.borrow_mut();
    let Some(zone) = app.zones.get_mut(zone_id) else {
        return Err(format!("live folder zone not found: {}", zone_id.0));
    };
    let changed = zone.set_live_folder_path(None) || !zone.items.is_empty();
    if changed {
        zone.items.clear();
        app.mark_dirty();
    }
    app.rules_wizard_status
        .borrow_mut()
        .replace(localized_current(
            format!("区域 {} 已解除文件夹绑定", zone_id.0),
            format!("Live folder unbound from zone {}", zone_id.0),
        ));
    Ok(changed)
}

pub(super) fn rehydrate_live_folder_bindings(root: &AppRoot) -> bool {
    rehydrate_live_folder_bindings_with(
        root,
        |zone_id, folder| {
            bentodesk_backend::watcher::bind(zone_id, folder).map_err(|error| error.to_string())
        },
        refresh_live_folder_zone,
    )
}

pub(super) fn rehydrate_live_folder_bindings_with<Bind, Refresh>(
    root: &AppRoot,
    mut bind: Bind,
    mut refresh: Refresh,
) -> bool
where
    Bind: FnMut(ZoneId, &Path) -> Result<(), String>,
    Refresh: FnMut(&AppRoot, ZoneId) -> Result<bool, String>,
{
    if root.live_folder_rehydrated.replace(true) {
        return false;
    }
    let bindings = {
        let app = root.app.borrow();
        app.zones
            .iter()
            .filter_map(|zone| {
                zone.live_folder_path
                    .as_deref()
                    .map(|folder| (zone.id, PathBuf::from(folder)))
            })
            .collect::<Vec<_>>()
    };

    let mut changed = false;
    for (zone_id, folder) in bindings {
        match bind(zone_id, &folder).and_then(|_| refresh(root, zone_id)) {
            Ok(zone_changed) => changed |= zone_changed,
            Err(error) => {
                tracing::warn!(
                    target: "bentodesk::live_folder",
                    zone_id = zone_id.0,
                    folder = %folder.display(),
                    error = %error,
                    "live folder rehydrate failed"
                );
                set_live_folder_error(
                    root,
                    localized_current(
                        format!("区域 {} 恢复文件夹绑定失败：{error}", zone_id.0),
                        format!(
                            "Live folder rehydrate failed for zone {}: {error}",
                            zone_id.0
                        ),
                    ),
                );
                changed = true;
            }
        }
    }
    changed
}
