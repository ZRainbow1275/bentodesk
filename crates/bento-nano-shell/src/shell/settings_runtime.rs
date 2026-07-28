//! Native shell owner: `settings_runtime`.

use super::*;

pub(super) fn settings_save_failure(root: &AppRoot, message: impl Into<SmolStr>) -> bool {
    let message = message.into();
    tracing::warn!(target: "bentodesk::settings", error = %message, "settings save rejected");
    let app = root.app.borrow();
    app.settings_dirty.set(true);
    app.settings_save_error.borrow_mut().replace(message);
    false
}

pub(super) fn localized_message(
    zh: bool,
    zh_text: impl Into<SmolStr>,
    en_text: impl Into<SmolStr>,
) -> SmolStr {
    if zh { zh_text.into() } else { en_text.into() }
}

pub(super) fn strip_windows_extended_path(path: &str) -> &str {
    path.strip_prefix(r"\\?\").unwrap_or(path)
}

pub(super) fn settings_path_is_within_prefix(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('\\'))
}

pub(super) fn validate_settings_directory(
    raw: &str,
    label_zh: &str,
    label_en: &str,
    zh: bool,
) -> Result<PathBuf, SmolStr> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(localized_message(
            zh,
            format!("{label_zh}不能为空"),
            format!("{label_en} cannot be empty"),
        ));
    }
    let path = Path::new(raw);
    if !path.exists() {
        return Err(localized_message(
            zh,
            format!("{label_zh}不存在：{raw}"),
            format!("{label_en} does not exist: {raw}"),
        ));
    }
    if !path.is_dir() {
        return Err(localized_message(
            zh,
            format!("{label_zh}不是文件夹：{raw}"),
            format!("{label_en} is not a folder: {raw}"),
        ));
    }
    let canonical = std::fs::canonicalize(path).map_err(|error| {
        localized_message(
            zh,
            format!("{label_zh}无法解析：{error}"),
            format!("{label_en} could not be resolved: {error}"),
        )
    })?;
    let canonical_lower = strip_windows_extended_path(canonical.to_string_lossy().as_ref())
        .replace('/', "\\")
        .to_lowercase();
    if let Some(prefix) = SETTINGS_PROTECTED_PREFIXES
        .iter()
        .find(|prefix| settings_path_is_within_prefix(&canonical_lower, prefix))
    {
        return Err(localized_message(
            zh,
            format!("{label_zh}不能位于系统目录 {prefix} 内"),
            format!("{label_en} cannot be inside the system directory {prefix}"),
        ));
    }
    Ok(canonical)
}

pub(super) fn push_unique_settings_source(sources: &mut Vec<PathBuf>, candidate: PathBuf) {
    let folded = candidate
        .to_string_lossy()
        .replace('/', "\\")
        .to_lowercase();
    if sources
        .iter()
        .any(|path| path.to_string_lossy().replace('/', "\\").to_lowercase() == folded)
    {
        return;
    }
    sources.push(candidate);
}

pub(super) fn validate_settings_sources_for_locale(
    snapshot: &SettingsSnapshot,
    zh: bool,
) -> Result<Vec<PathBuf>, SmolStr> {
    let desktop = validate_settings_directory(
        snapshot.desktop_path_draft.as_str(),
        "桌面路径",
        "Desktop path",
        zh,
    )?;
    let desktop_text = desktop.to_string_lossy();
    let mut sources = Vec::new();
    for source in bento_nano_backend::desktop_sources::all_desktop_dirs(Some(&desktop_text)) {
        if source.is_dir() {
            push_unique_settings_source(
                &mut sources,
                std::fs::canonicalize(&source).unwrap_or(source),
            );
        }
    }
    // `all_desktop_dirs` deliberately filters missing paths. Keep the validated
    // custom path even if it was folded out as a duplicate.
    push_unique_settings_source(&mut sources, desktop);

    for (index, raw) in snapshot.watch_paths_draft.lines().enumerate() {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let label_zh = format!("监控路径第 {} 行", index + 1);
        let label_en = format!("Watch path line {}", index + 1);
        let path = validate_settings_directory(raw, label_zh.as_str(), label_en.as_str(), zh)?;
        push_unique_settings_source(&mut sources, path);
    }
    if sources.is_empty() {
        return Err(localized_message(
            zh,
            "没有可用的桌面监控路径",
            "No usable desktop watch path",
        ));
    }
    Ok(sources)
}

pub(super) fn validate_settings_sources(
    snapshot: &SettingsSnapshot,
) -> Result<Vec<PathBuf>, SmolStr> {
    validate_settings_sources_for_locale(
        snapshot,
        bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN),
    )
}

pub(super) fn rebuild_desktop_watcher(root: &AppRoot, sources: &[PathBuf]) -> Result<(), SmolStr> {
    if startup_diag_skip("desktop_watcher") {
        tracing::info!(
            target: "bentodesk::watcher",
            "desktop watcher rebuild skipped by startup diagnostics"
        );
        return Ok(());
    }
    let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
    let replacement =
        bento_nano_backend::watcher::setup_file_watcher(sources, root.desktop_event_tx.clone())
            .map_err(|error| {
                localized_message(
                    zh,
                    format!("无法监控桌面路径：{error}"),
                    format!("Unable to watch desktop paths: {error}"),
                )
            })?;
    let previous = root.desktop_watcher.replace(Some(replacement));
    drop(previous);
    tracing::info!(
        target: "bentodesk::watcher",
        sources = sources.len(),
        "desktop watcher rebuilt from saved Settings paths"
    );
    Ok(())
}

pub(super) fn copy_state_dir_recursive(
    source: &Path,
    target: &Path,
    zh: bool,
) -> Result<(), SmolStr> {
    if source == target || !source.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(target).map_err(|error| {
        localized_message(
            zh,
            format!("无法创建便携数据目录：{error}"),
            format!("Unable to create the portable data directory: {error}"),
        )
    })?;
    let entries = std::fs::read_dir(source).map_err(|error| {
        localized_message(
            zh,
            format!("无法读取当前数据目录：{error}"),
            format!("Unable to read the current data directory: {error}"),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            localized_message(
                zh,
                format!("无法读取数据目录项：{error}"),
                format!("Unable to read a data-directory entry: {error}"),
            )
        })?;
        let file_type = entry.file_type().map_err(|error| {
            localized_message(
                zh,
                format!("无法读取数据目录项类型：{error}"),
                format!("Unable to read a data-directory entry type: {error}"),
            )
        })?;
        let destination = target.join(entry.file_name());
        if file_type.is_symlink() {
            return Err(localized_message(
                zh,
                format!("便携迁移拒绝符号链接：{}", entry.path().display()),
                format!(
                    "Portable migration rejected a symbolic link: {}",
                    entry.path().display()
                ),
            ));
        }
        if file_type.is_dir() {
            copy_state_dir_recursive(entry.path().as_path(), destination.as_path(), zh)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &destination).map_err(|error| {
                localized_message(
                    zh,
                    format!("无法复制便携数据 {}：{error}", entry.path().display()),
                    format!(
                        "Unable to copy portable data {}: {error}",
                        entry.path().display()
                    ),
                )
            })?;
        }
    }
    Ok(())
}

pub(super) fn sync_portable_state(previous: bool, desired: bool) -> Result<(), SmolStr> {
    if previous == desired {
        return Ok(());
    }
    let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
    let source = storage::state_dir_for_portable_mode(previous).map_err(|error| {
        localized_message(
            zh,
            format!("无法定位当前数据目录：{error}"),
            format!("Unable to locate the current data directory: {error}"),
        )
    })?;
    let target = storage::state_dir_for_portable_mode(desired).map_err(|error| {
        localized_message(
            zh,
            format!("无法定位目标数据目录：{error}"),
            format!("Unable to locate the target data directory: {error}"),
        )
    })?;
    copy_state_dir_recursive(source.as_path(), target.as_path(), zh)?;
    storage::set_portable_mode_enabled(desired).map_err(|error| {
        localized_message(
            zh,
            format!("无法切换便携模式：{error}"),
            format!("Unable to switch portable mode: {error}"),
        )
    })?;
    tracing::info!(
        target: "bentodesk::settings",
        previous,
        desired,
        source = %source.display(),
        target_dir = %target.display(),
        "portable mode state synchronized for next launch"
    );
    Ok(())
}

pub(super) fn apply_show_in_taskbar(hwnd: HWND, show: bool) -> Result<(), SmolStr> {
    let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
    if hwnd.is_null() {
        return Err(localized_message(
            zh,
            "主窗口不存在，无法更新任务栏状态",
            "The main window is unavailable; taskbar state was not changed",
        ));
    }
    let _guard = bento_nano_backend::ghost_layer::bypass_subclass_guard();
    // SAFETY: `hwnd` is the live Main HWND. The hide/style/frame/show sequence
    // matches the Tauri baseline and avoids stale taskbar buttons.
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let desired = if show {
            (current | WS_EX_APPWINDOW as isize) & !(WS_EX_TOOLWINDOW as isize)
        } else {
            (current | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize)
        };
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired);
        SetWindowPos(
            hwnd,
            ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let applied = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let correct = if show {
            applied & WS_EX_APPWINDOW as isize != 0 && applied & WS_EX_TOOLWINDOW as isize == 0
        } else {
            applied & WS_EX_TOOLWINDOW as isize != 0 && applied & WS_EX_APPWINDOW as isize == 0
        };
        if !correct {
            return Err(localized_message(
                zh,
                "Windows 未接受任务栏窗口样式",
                "Windows did not accept the requested taskbar window style",
            ));
        }
    }
    Ok(())
}

pub(super) fn apply_desktop_embed(hwnd: HWND, enabled: bool) -> Result<(), SmolStr> {
    let result = if enabled {
        bento_nano_backend::ghost_layer::attach_selected_stack(hwnd)
    } else {
        bento_nano_backend::ghost_layer::detach(hwnd)
    };
    let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
    result.map_err(|error| {
        localized_message(
            zh,
            format!("桌面嵌入切换失败：{error}"),
            format!("Desktop embedding could not be changed: {error}"),
        )
    })
}

pub(super) fn apply_process_priority(high_priority: bool) -> Result<(), SmolStr> {
    let priority = if high_priority {
        ABOVE_NORMAL_PRIORITY_CLASS
    } else {
        NORMAL_PRIORITY_CLASS
    };
    // SAFETY: GetCurrentProcess returns a process pseudo-handle valid for
    // SetPriorityClass. No ownership is transferred.
    if unsafe { SetPriorityClass(GetCurrentProcess(), priority) } == 0 {
        let code = unsafe { GetLastError() };
        return Err(localized_message(
            bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN),
            format!("进程优先级切换失败：Win32 {code}"),
            format!("Unable to change process priority: Win32 {code}"),
        ));
    }
    Ok(())
}

pub(super) fn restart_arg_value(args: &[String], prefix: &str) -> Option<u64> {
    args.iter()
        .find_map(|arg| arg.strip_prefix(prefix)?.parse::<u64>().ok())
}

pub(super) fn restart_registration_command(
    enabled: bool,
    max_retries: u32,
    window_secs: u64,
    now_secs: u64,
    args: &[String],
) -> Option<String> {
    if !enabled || max_retries == 0 {
        return None;
    }
    let mut attempt = restart_arg_value(args, RESTART_ATTEMPT_ARG).unwrap_or(0);
    let mut window_start = restart_arg_value(args, RESTART_WINDOW_START_ARG).unwrap_or(now_secs);
    if now_secs.saturating_sub(window_start) > window_secs {
        attempt = 0;
        window_start = now_secs;
    }
    if attempt >= u64::from(max_retries) {
        return None;
    }
    Some(format!(
        "{RESTART_ATTEMPT_ARG}{} {RESTART_WINDOW_START_ARG}{window_start}",
        attempt + 1
    ))
}

pub(super) fn configure_application_restart(snapshot: &SettingsSnapshot) -> Result<(), SmolStr> {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let args: Vec<String> = std::env::args().collect();
    let command = restart_registration_command(
        snapshot.crash_restart_enabled,
        snapshot.crash_max_retries.max(0) as u32,
        snapshot.crash_window_secs.max(0) as u64,
        now_secs,
        &args,
    );
    let hr = match command {
        Some(command) => {
            let wide = widen_dynamic(command.as_str());
            // SAFETY: `wide` is NUL-terminated and alive for the call.
            unsafe {
                RegisterApplicationRestart(
                    wide.as_ptr(),
                    RESTART_NO_HANG | RESTART_NO_PATCH | RESTART_NO_REBOOT,
                )
            }
        }
        None => {
            // SAFETY: process-scoped unregister has no arguments.
            unsafe { UnregisterApplicationRestart() }
        }
    };
    if hr < 0 {
        return Err(localized_message(
            bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN),
            format!("崩溃自动重启配置失败：HRESULT 0x{:08X}", hr as u32),
            format!(
                "Unable to configure automatic crash restart: HRESULT 0x{:08X}",
                hr as u32
            ),
        ));
    }
    Ok(())
}

pub(super) fn settings_paths_changed(
    previous: &SettingsSnapshot,
    desired: &SettingsSnapshot,
) -> bool {
    previous.desktop_path_draft != desired.desktop_path_draft
        || previous.watch_paths_draft != desired.watch_paths_draft
}

pub(super) fn apply_runtime_settings(
    root: &AppRoot,
    previous: &SettingsSnapshot,
    desired: &SettingsSnapshot,
    desired_sources: &[PathBuf],
) -> Result<(), SmolStr> {
    let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
    if previous.launch_at_startup != desired.launch_at_startup {
        bento_nano_backend::autostart::set_enabled(desired.launch_at_startup).map_err(|error| {
            localized_message(
                zh,
                format!("开机启动配置失败：{error}"),
                format!("Unable to configure launch at startup: {error}"),
            )
        })?;
        if bento_nano_backend::autostart::is_enabled() != desired.launch_at_startup {
            return Err(localized_message(
                zh,
                "开机启动注册表状态校验失败",
                "Launch-at-startup registry verification failed",
            ));
        }
    }

    let main_hwnd = find_main_hwnd(root);
    if previous.show_in_taskbar != desired.show_in_taskbar {
        apply_show_in_taskbar(
            main_hwnd.ok_or_else(|| {
                localized_message(zh, "主窗口尚未创建", "The main window is not available")
            })?,
            desired.show_in_taskbar,
        )?;
    }
    if previous.icon_cache_size != desired.icon_cache_size {
        let Some(cache) = bento_nano_backend::icon::cache_handle() else {
            return Err(localized_message(
                zh,
                "图标缓存尚未初始化",
                "The icon cache is not initialized",
            ));
        };
        cache.resize(desired.icon_cache_size.max(1) as usize);
    }
    if previous.ghost_layer_enabled != desired.ghost_layer_enabled {
        apply_desktop_embed(
            main_hwnd.ok_or_else(|| {
                localized_message(zh, "主窗口尚未创建", "The main window is not available")
            })?,
            desired.ghost_layer_enabled,
        )?;
    }
    if previous.startup_high_priority != desired.startup_high_priority {
        apply_process_priority(desired.startup_high_priority)?;
    }
    if previous.crash_restart_enabled != desired.crash_restart_enabled
        || previous.crash_max_retries != desired.crash_max_retries
        || previous.crash_window_secs != desired.crash_window_secs
    {
        configure_application_restart(desired)?;
    }
    if settings_paths_changed(previous, desired) {
        rebuild_desktop_watcher(root, desired_sources)?;
    }
    sync_portable_state(previous.portable_mode, desired.portable_mode)?;
    Ok(())
}

pub(super) fn snapshot_vault_settings(
    vault: &bento_nano_backend::config_vault::Vault,
) -> Vec<(
    &'static str,
    Option<bento_nano_backend::config_vault::SettingValue>,
)> {
    SETTINGS_TRANSACTION_KEYS
        .iter()
        .map(|key| (*key, vault.get_setting(key)))
        .collect()
}

pub(super) fn restore_vault_settings(
    vault: &mut bento_nano_backend::config_vault::Vault,
    values: &[(
        &'static str,
        Option<bento_nano_backend::config_vault::SettingValue>,
    )],
) {
    for (key, value) in values {
        match value {
            Some(value) => vault.set_setting(key, value.clone()),
            None => {
                vault.remove_setting(key);
            }
        }
    }
}

pub(super) fn localized_current(
    zh_text: impl Into<SmolStr>,
    en_text: impl Into<SmolStr>,
) -> SmolStr {
    localized_message(
        bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN),
        zh_text,
        en_text,
    )
}

pub(super) fn persist_settings_snapshot_to_vault(
    vault: &mut bento_nano_backend::config_vault::Vault,
    snapshot: &SettingsSnapshot,
    accent_draft: Option<&SmolStr>,
    accent_clear_requested: bool,
) {
    vault.set_setting(
        SETTING_GENERAL_GHOST_LAYER_ENABLED,
        bento_nano_backend::config_vault::SettingValue::Bool(snapshot.ghost_layer_enabled),
    );
    vault.set_setting(
        SETTING_GENERAL_LAUNCH_AT_STARTUP,
        bento_nano_backend::config_vault::SettingValue::Bool(snapshot.launch_at_startup),
    );
    vault.set_setting(
        SETTING_GENERAL_SHOW_IN_TASKBAR,
        bento_nano_backend::config_vault::SettingValue::Bool(snapshot.show_in_taskbar),
    );
    vault.set_setting(
        SETTING_GENERAL_AUTO_GROUP_ENABLED,
        bento_nano_backend::config_vault::SettingValue::Bool(snapshot.auto_group_enabled),
    );
    vault.set_setting(
        SETTING_GENERAL_PORTABLE_MODE,
        bento_nano_backend::config_vault::SettingValue::Bool(snapshot.portable_mode),
    );
    vault.set_setting(
        SETTING_PERF_EXPAND_DELAY_MS,
        bento_nano_backend::config_vault::SettingValue::Int(snapshot.expand_delay_ms as i64),
    );
    vault.set_setting(
        SETTING_PERF_COLLAPSE_DELAY_MS,
        bento_nano_backend::config_vault::SettingValue::Int(snapshot.collapse_delay_ms as i64),
    );
    vault.set_setting(
        SETTING_PERF_ICON_CACHE_SIZE,
        bento_nano_backend::config_vault::SettingValue::Int(snapshot.icon_cache_size as i64),
    );
    vault.set_setting(
        SETTING_STARTUP_HIGH_PRIORITY,
        bento_nano_backend::config_vault::SettingValue::Bool(snapshot.startup_high_priority),
    );
    vault.set_setting(
        SETTING_STARTUP_CRASH_RESTART_ENABLED,
        bento_nano_backend::config_vault::SettingValue::Bool(snapshot.crash_restart_enabled),
    );
    vault.set_setting(
        SETTING_STARTUP_CRASH_MAX_RETRIES,
        bento_nano_backend::config_vault::SettingValue::Int(snapshot.crash_max_retries as i64),
    );
    vault.set_setting(
        SETTING_STARTUP_CRASH_WINDOW_SECS,
        bento_nano_backend::config_vault::SettingValue::Int(snapshot.crash_window_secs as i64),
    );
    vault.set_setting(
        SETTING_STARTUP_SAFE_AFTER_HIBERNATION,
        bento_nano_backend::config_vault::SettingValue::Bool(snapshot.safe_start_after_hibernation),
    );
    vault.set_setting(
        SETTING_STARTUP_HIBERNATE_RESUME_DELAY_MS,
        bento_nano_backend::config_vault::SettingValue::Int(
            snapshot.hibernate_resume_delay_ms as i64,
        ),
    );
    persist_settings_accent_to_vault(vault, accent_draft, accent_clear_requested);
    vault.set_setting(
        SETTING_PATHS_DESKTOP_PATH,
        bento_nano_backend::config_vault::SettingValue::Str(snapshot.desktop_path_draft.clone()),
    );
    vault.set_setting(
        SETTING_PATHS_WATCH_PATHS,
        bento_nano_backend::config_vault::SettingValue::Str(snapshot.watch_paths_draft.clone()),
    );
    vault.set_setting(
        SETTING_ACTIVE_THEME,
        bento_nano_backend::config_vault::SettingValue::Str(snapshot.active_theme_id.clone()),
    );
    vault.set_setting(
        SETTING_ZONE_DISPLAY_MODE,
        bento_nano_backend::config_vault::SettingValue::Str(SmolStr::new_static(
            snapshot.zone_display_mode.as_wire(),
        )),
    );
}
