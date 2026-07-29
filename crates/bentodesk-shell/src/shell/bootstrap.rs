//! Native shell owner: `bootstrap`.

use super::*;

pub(super) fn run() {
    // Mc-1b(a) — DELIBERATE behavioural change from the previous "minimal /
    // silent" hook. With `windows_subsystem="windows"` (no console) and
    // `panic="abort"` (release), stderr is NULL → a panic vanished entirely.
    // The hook now ALSO routes the message to the debug stream
    // (`OutputDebugStringA`) and raises a user-visible `MessageBoxW`. It must
    // stay panic-free itself (a panic-in-a-panic-hook double-faults): we build
    // the strings with checked formatting, allocate a `Vec`/`SmallVec` (an
    // alloc is acceptable here since we are aborting anyway), and use no
    // unwrap/expect/index.
    std::panic::set_hook(Box::new(|info| {
        use core::fmt::Write as _;
        let mut buf: smallvec::SmallVec<[u8; 256]> = smallvec::SmallVec::new();
        struct SvWriter<'a>(&'a mut smallvec::SmallVec<[u8; 256]>);
        impl core::fmt::Write for SvWriter<'_> {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                self.0.extend_from_slice(s.as_bytes());
                Ok(())
            }
        }
        let (file, line) = match info.location() {
            Some(l) => (l.file(), l.line()),
            None => ("<unknown>", 0),
        };
        // Build the human-readable message once: location + (if available)
        // the panic payload string.
        let mut msg = format!("BentoDesk panic at {file}:{line}");
        if let Some(s) = info.payload().downcast_ref::<&str>() {
            let _ = write!(&mut msg, ": {s}");
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            let _ = write!(&mut msg, ": {s}");
        }

        // Keep the existing stderr line (no-op under windows_subsystem, but
        // visible when launched from a console / under a wrapper).
        let _ = writeln!(SvWriter(&mut buf), "{msg}");
        let _ = std::io::Write::write_all(&mut std::io::stderr(), &buf);

        // Debug-stream channel (visible in DebugView / a debugger).
        // `OutputDebugStringA` wants a NUL-terminated ANSI/UTF-8 buffer.
        let mut dbg_bytes: Vec<u8> = msg.clone().into_bytes();
        dbg_bytes.push(0);
        // SAFETY: `dbg_bytes` is NUL-terminated; OutputDebugStringA only reads.
        unsafe {
            OutputDebugStringA(dbg_bytes.as_ptr());
        }

        // User-visible box (the only channel a normal user can see).
        show_fatal_box("BentoDesk — 致命错误 / Fatal Error", &msg);
    }));

    // Mc-1b — single-instance guard. Placed before locale init / window
    // creation. Without this, autorun + a manual double-click
    // produce two processes racing zones.bin / vault.bin / the tray / hotkeys
    // / the watcher.
    //
    // Session-local name (no `Global\` prefix) → per-session, which is correct
    // for a per-user desktop overlay (one instance per interactive session).
    {
        let mut mutex_name: Vec<u16> = "BentoDesk.SingleInstance.7E2A1C90".encode_utf16().collect();
        mutex_name.push(0);
        // SAFETY: name buffer is NUL-terminated; bInitialOwner = FALSE (0).
        let mutex = unsafe { CreateMutexW(ptr::null(), 0, mutex_name.as_ptr()) };
        // SAFETY: GetLastError reads the calling thread's last-error code set
        //         by CreateMutexW above.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // An instance is already running — best-effort wake it, then exit.
            // SAFETY: CLASS_NAME is a NUL-terminated wide class string; a null
            //         window title matches any window of that class. All calls
            //         tolerate a null/!found HWND.
            unsafe {
                let existing = FindWindowW(CLASS_NAME.as_ptr(), ptr::null());
                if !existing.is_null() {
                    PostMessageW(existing, WM_WAKE_INSTANCE, 0, 0);
                    SetForegroundWindow(existing);
                }
            }
            std::process::exit(0);
        }
        // Keep the HANDLE alive for the whole process: stash it (do NOT
        // CloseHandle — closing releases the named mutex). `CreateMutexW`
        // returns a HANDLE (≈ *mut c_void); store as isize.
        let _ = MUTEX_HANDLE.set(mutex as isize);
    }

    // Phase 1.3 — install the locale BEFORE any widget asks for a localised
    // string. Subsequent `set_locale` calls hot-swap on the next frame.
    // #19-B (2026-05-31) — default to the OS UI language (zh-CN on a Chinese
    // OS, English otherwise) instead of unconditional zh-CN.
    let default_locale = detected_default_locale();
    bentodesk_style::init_locale(default_locale);
    if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
        log_static("startup: locale=zh-CN\n");
    } else {
        log_static("startup: locale=en-US\n");
    }
    log_static(if bentodesk_platform::dcomp::acrylic_feature_enabled() {
        "startup: acrylic_feature=on\n"
    } else {
        "startup: acrylic_feature=off\n"
    });

    // Phase 2.3.1a / Mc-1a — per-monitor DPI awareness via the soft-loaded
    // cascade (PER_MONITOR_AWARE_V2 → PER_MONITOR_AWARE → shcore → SetProcessDPIAware).
    // GetProcAddress-resolved so the EXE carries no static DPI import and loads
    // on Win10 <1607/1703, Win8.1/8/7. Best-effort at process-init.
    bentodesk_platform::dpi::set_process_dpi_awareness();

    let ole_drop_ready = if startup_diag_skip("ole") {
        false
    } else {
        match bentodesk_backend::drag_drop::initialize_ole() {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(
                    target: "bentodesk::drag_drop",
                    error = %e,
                    "OLE init failed; keeping WM_DROPFILES compatibility path only"
                );
                false
            }
        }
    };

    let (desktop_event_tx, desktop_event_rx) = crossbeam_channel::unbounded();
    let (live_folder_event_tx, live_folder_event_rx) = crossbeam_channel::unbounded();
    let (ghost_event_tx, ghost_event_rx) = crossbeam_channel::unbounded();
    let (power_event_tx, power_event_rx) = crossbeam_channel::unbounded();
    let (updater_event_tx, updater_event_rx) = crossbeam_channel::unbounded();
    let (rules_scheduler_event_tx, rules_scheduler_event_rx) = crossbeam_channel::unbounded();
    bentodesk_backend::ghost_layer::set_event_sender(ghost_event_tx);
    let desktop_sources = bentodesk_backend::desktop_sources::all_desktop_dirs(None);
    let desktop_watcher = if startup_diag_skip("desktop_watcher") {
        None
    } else {
        match bentodesk_backend::watcher::setup_file_watcher(
            &desktop_sources,
            desktop_event_tx.clone(),
        ) {
            Ok(watcher) => Some(watcher),
            Err(e) => {
                tracing::warn!(
                    target: "bentodesk::watcher",
                    sources = desktop_sources.len(),
                    error = %e,
                    "desktop watcher startup failed"
                );
                None
            }
        }
    };
    if !startup_diag_skip("live_folder")
        && let Err(e) = bentodesk_backend::watcher::ensure_initialised(live_folder_event_tx)
    {
        tracing::warn!(
            target: "bentodesk::live_folder",
            error = %e,
            "live folder watcher startup failed"
        );
    }

    // T-010 — install the process AppRoot before creating any window. Seed the
    // editable Desktop path from the real resolved user Desktop rather than
    // the old machine-specific `D:\Desktop` placeholder.
    let app_state = AppState::new();
    if let Some(primary_desktop) = desktop_sources.first() {
        *app_state.desktop_path_draft.borrow_mut() =
            SmolStr::new(primary_desktop.to_string_lossy());
    }
    install_app_root(Box::new(AppRoot {
        app: RefCell::new(app_state),
        registry: RefCell::new(WindowRegistry::new()),
        dispatcher: EventDispatcher::new(),
        hovered: RefCell::new(None),
        // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
        last_tick_ms: Cell::new(unsafe { GetTickCount() }),
        frame_id: Cell::new(0),
        recovery_state: Cell::new(bentodesk_platform::RecoveryState::Healthy),
        last_recovery_at: Cell::new(None),
        minibar_roster: RefCell::new(MiniBarRoster::new()),
        minibars: RefCell::new(smallvec::SmallVec::new()),
        zone_context_menu: RefCell::new(None),
        item_context_menu: RefCell::new(None),
        pending_item_drag_out: RefCell::new(None),
        pending_stack_drop_bloom: Cell::new(None),
        item_drag_out_active: Cell::new(false),
        tray_context_menu: RefCell::new(None),
        tray_context_menu_consumed: Cell::new(false),
        hotkey_bindings: RefCell::new(default_hotkey_bindings()),
        global_hotkeys: RefCell::new(smallvec::SmallVec::new()),
        tray_registered: Cell::new(false),
        tray_retry_attempts: Cell::new(0),
        tray_uid_only: Cell::new(false),
        desktop_watcher: RefCell::new(desktop_watcher),
        desktop_event_tx,
        desktop_events: desktop_event_rx,
        live_folder_events: live_folder_event_rx,
        live_folder_rehydrated: Cell::new(false),
        ghost_events: ghost_event_rx,
        power_event_tx,
        power_events: power_event_rx,
        updater: Updater::new(updater_event_tx),
        updater_events: updater_event_rx,
        rules_scheduler_events: rules_scheduler_event_rx,
        timeline_buffer: RefCell::new(TimelineBuffer::default()),
    }));

    // F2-03 — initialise the process-global config vault. Resolved as a
    // sibling of the existing zones.bin under `%APPDATA%\BentoDesk\` to
    // share the storage helper's directory-creation path. Failure here is
    // best-effort — the dispatcher's SetSetting handler tolerates a
    // missing global by logging instead of blocking the pump.
    if let Ok(zones_path) = storage::appdata_path()
        && let Some(dir) = zones_path.parent()
    {
        if let Some(root) = app_root() {
            root.app.borrow_mut().zones_path = zones_path.clone();
        }
        let vault_path = dir.join("vault.bin");
        if startup_diag_skip("vault") {
            tracing::info!(
                target: "bentodesk::startup_diag",
                "BENTODESK_DIAG_SKIP=vault; config vault startup skipped"
            );
        } else if let Err(e) = bentodesk_backend::config_vault::init_global(&vault_path) {
            tracing::warn!(
                target: "bentodesk::vault",
                error = %e,
                "Vault init_global failed — SetSetting will log-and-drop"
            );
        } else if let Some(root) = app_root() {
            migrate_legacy_tauri_settings_to_vault(dir);
            apply_persisted_settings_from_vault(root);
            maybe_start_background_update_check(root);
        }

        let icon_config = bentodesk_backend::icon::IconConfig {
            app_data_dir: smol_str::SmolStr::new(dir.to_string_lossy().as_ref()),
        };
        if !startup_diag_skip("icon") {
            let icon_cache = bentodesk_backend::icon::init(&icon_config);
            if let Some(root) = app_root() {
                icon_cache.resize(root.app.borrow().icon_cache_size.get().max(1) as usize);
            }
        }
        if let Some(root) = app_root() {
            // The marker chooses the storage root before vault.bin can be
            // opened, so it is the runtime truth for the toggle.
            root.app
                .borrow()
                .setting_portable_mode
                .set(storage::portable_mode_enabled());
            let startup_settings = root.app.borrow().snapshot_settings();
            if let Err(error) = apply_process_priority(startup_settings.startup_high_priority) {
                tracing::warn!(
                    target: "bentodesk::settings",
                    %error,
                    "startup process-priority restore failed"
                );
            }
            if let Err(error) = configure_application_restart(&startup_settings) {
                tracing::warn!(
                    target: "bentodesk::settings",
                    %error,
                    "startup crash-restart restore failed"
                );
            }
            match validate_settings_sources(&startup_settings) {
                Ok(sources) => {
                    if let Err(error) = rebuild_desktop_watcher(root, &sources) {
                        tracing::warn!(
                            target: "bentodesk::watcher",
                            %error,
                            "startup Settings-path watcher restore failed; initial watcher retained"
                        );
                    }
                }
                Err(error) => tracing::warn!(
                    target: "bentodesk::watcher",
                    %error,
                    "startup Settings paths invalid; initial watcher retained"
                ),
            }
        }
        if !startup_diag_skip("recovery")
            && let Some(root) = app_root()
        {
            run_startup_recovery_bundle_heal(root, &zones_path);
        }
        if let Some(root) = app_root() {
            run_startup_layout_load_or_migrate(root, &zones_path);
        }
        if !startup_diag_skip("rules") {
            bentodesk_backend::rules::scheduler::spawn(
                dir.to_path_buf(),
                rules_scheduler_event_tx,
                Duration::from_secs(60),
            );
        }
    }

    // Wave C (05-20 visual parity) — Main HWND is a borderless fullscreen
    // transparent overlay sitting at the primary monitor work area.
    // `bentodesk_platform::main_window_rect` returns the live primary work
    // area in device pixels from `MonitorFromPoint(0,0) → GetMonitorInfoW`;
    // on a headless test harness it falls back to `default_size(Main)`.
    //
    // The platform `create_window` helper applies a 96→system-DPI scale to
    // `WindowDesc.width/height`; since `main_window_rect` already returns
    // device pixels, we leave `desc` at its default size during creation
    // and snap to the work area via `SetWindowPos` afterwards. This keeps
    // the geometry math identical between cold start and the
    // `WM_DPICHANGED` / `WM_DISPLAYCHANGE` paths below.
    let (main_x, main_y, main_w, main_h) = bentodesk_platform::main_window_rect();
    let desc = WindowDesc::for_kind(CLASS_NAME, WIN_TITLE, Some(wnd_proc), WindowKind::Main);

    let hwnd = match create_window(&desc, ptr::null_mut()) {
        Ok(h) => h,
        Err(e) => {
            let _ = std::io::Write::write_all(
                &mut std::io::stderr(),
                format!("BentoDesk fatal: {e}\n").as_bytes(),
            );
            // Mc-1b(b) — make the fatal start-up failure visible (stderr is
            // NULL under windows_subsystem="windows").
            show_fatal_box(
                "BentoDesk — 致命错误 / Fatal Error",
                &format!("窗口创建失败 / Window creation failed: {e}"),
            );
            std::process::exit(1);
        }
    };
    // Snap Main to the primary-monitor work area. SWP_NOZORDER keeps the
    // window above the desktop without forcing topmost; SWP_NOACTIVATE
    // prevents focus theft on startup.
    // SAFETY: hwnd just created and validated non-null above.
    unsafe {
        SetWindowPos(
            hwnd,
            ptr::null_mut(),
            main_x,
            main_y,
            main_w.max(1),
            main_h.max(1),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    let acrylic_runtime = match bentodesk_platform::dcomp::device() {
        Ok(_) => bentodesk_platform::dcomp::acrylic_runtime_available(),
        Err(_) => None,
    };
    log_static(match acrylic_runtime {
        Some(true) => "startup: acrylic_runtime=available\n",
        Some(false) => "startup: acrylic_runtime=unavailable\n",
        None => "startup: acrylic_runtime=unknown\n",
    });

    if let Some(root) = app_root() {
        restore_persisted_minibars_from_vault(root);
    }

    // Enables Explorer -> BentoDesk selected-stack producers for AddItem.
    // RegisterDragDrop is the primary OLE parity path; WM_DROPFILES remains a
    // compatibility fallback that reaches the same item model and persistence.
    unsafe { DragAcceptFiles(hwnd, 1) };
    if ole_drop_ready
        && let Err(e) = bentodesk_backend::drag_drop::register_drop_target(
            hwnd,
            ole_drop_can_accept,
            ole_drop_commit,
        )
    {
        tracing::warn!(
            target: "bentodesk::drag_drop",
            error = %e,
            "RegisterDragDrop failed; WM_DROPFILES remains the active fallback"
        );
    }

    if let Some(root) = app_root() {
        let show_in_taskbar = root.app.borrow().setting_show_in_taskbar.get();
        if let Err(error) = apply_show_in_taskbar(hwnd, show_in_taskbar) {
            tracing::warn!(
                target: "bentodesk::settings",
                %error,
                "startup taskbar-visibility restore failed"
            );
        }
    }

    let desktop_embed_enabled = app_root()
        .map(|root| root.app.borrow().setting_desktop_embed.get())
        .unwrap_or(true);
    if startup_diag_skip("ghost") {
        tracing::info!(
            target: "bentodesk::startup_diag",
            "BENTODESK_DIAG_SKIP=ghost; ghost layer startup skipped"
        );
    } else if !desktop_embed_enabled {
        tracing::info!(
            target: "bentodesk::ghost_layer",
            "ghost layer startup skipped by saved Settings"
        );
    } else if let Err(e) = bentodesk_backend::ghost_layer::attach_selected_stack(hwnd) {
        tracing::warn!(
            target: "bentodesk::ghost_layer",
            error = %e,
            "ghost layer attach failed; main window remains normal HWND"
        );
    } else {
        // The Main HWND region is the selected-stack click-through boundary:
        // blank desktop pixels fall outside the window, and painted chrome
        // remains interactive. Do not arm the old always-on 50 ms ghost polling
        // timer at startup; it repeatedly scans global cursor state even when
        // the app is idle and blows the strict resident memory budget. Pointer
        // movement inside the region still updates passthrough state through
        // the normal WM_MOUSEMOVE path.
        // V-10 live-audit (2026-05-21) — log the Main HWND ex_style at
        // t+0/100/500/2000ms after attach so a live hand-test can correlate
        // user-reported click-through breakage to whether the
        // WS_EX_TRANSPARENT bit is actually present. Audit-only; the
        // observed values feed `_v10_live_audit.md`.
        unsafe {
            log_main_ex_style_audit(hwnd, "post_attach_t+0ms");
        }
        let hwnd_raw = hwnd as usize;
        std::thread::spawn(move || {
            let hwnd = hwnd_raw as HWND;
            std::thread::sleep(std::time::Duration::from_millis(100));
            // SAFETY: `hwnd` was stored from the live main HWND above; the OS
            // keeps it valid for the lifetime of the process.
            unsafe { log_main_ex_style_audit(hwnd, "audit_t+100ms") };
            std::thread::sleep(std::time::Duration::from_millis(400));
            unsafe { log_main_ex_style_audit(hwnd, "audit_t+500ms") };
            std::thread::sleep(std::time::Duration::from_millis(1500));
            unsafe { log_main_ex_style_audit(hwnd, "audit_t+2000ms") };
        });
    }

    message_loop::run();
    if ole_drop_ready {
        bentodesk_backend::drag_drop::uninitialize_ole();
    }
}
