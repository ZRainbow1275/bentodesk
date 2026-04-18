//! BentoDesk: Zero-interference desktop visual organizer.
//!
//! This is the library root. It declares all backend modules and provides
//! the [`run`] function that initialises the Tauri application.

// Crate-wide allows for pre-existing lints that are scheduled for a separate
// cleanup pass — relaxing them here keeps `cargo clippy -- -D warnings` green
// without forcing drive-by refactors of code owned by other workstreams.
#![allow(dead_code)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::bind_instead_of_map)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
#![allow(clippy::useless_format)]
#![allow(clippy::needless_borrows_for_generic_args)]

// Theme B — mimalloc replaces the MSVC CRT allocator. Lower working set on
// small-string workloads (paths, hashes, JSON keys) and better fragmentation
// behaviour than the system allocator under the churn from `zones.clone()`
// + `LruCache` eviction.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod commands;
mod config;
mod context_capsule;
mod crash_handler;
mod desktop_sources;
mod display;
mod drag_drop;
mod error;
mod ghost_layer;
mod grouping;
mod guardrails;
mod hidden_items;
mod icon;
mod icon_positions;
mod layout;
mod minibar;
mod plugins;
mod power;
mod recovery_bundle;
mod rules;
mod startup;
pub(crate) mod storage;
mod themes;
mod timeline;
mod tray;
mod updater;
mod watcher;

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Listener, Manager};

/// Shared application state managed by Tauri.
pub struct AppState {
    pub layout: Mutex<layout::persistence::LayoutData>,
    pub settings: Mutex<config::settings::AppSettings>,
    pub icon_cache: icon::cache::IconCache,
    pub icon_backup: Mutex<Option<icon_positions::SavedIconLayout>>,
    pub app_handle: AppHandle,
    /// In-memory mirror of the on-disk timeline. Mutated from the command
    /// layer + background debounce thread; the ring buffer holds both auto
    /// captures and user-pinned permanent checkpoints.
    pub timeline: Mutex<timeline::TimelineBuffer>,
    /// Serialises disk writes from `persist_layout` / `persist_settings` so
    /// concurrent Tauri commands cannot interleave file I/O.
    write_lock: Mutex<()>,
}

impl AppState {
    /// Persist the current layout to disk.
    ///
    /// Call this after any mutation to `self.layout` to ensure changes survive
    /// application restarts. Errors are logged but not propagated so that the
    /// in-memory state remains authoritative.
    pub fn persist_layout(&self) {
        let _write = match self.write_lock.lock() {
            Ok(g) => g,
            Err(e) => {
                tracing::error!("Failed to acquire write_lock for layout persistence: {}", e);
                return;
            }
        };
        let layout = match self.layout.lock() {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Failed to acquire layout lock for persistence: {}", e);
                return;
            }
        };
        if let Err(e) = layout.save(&self.app_handle) {
            tracing::error!("Failed to persist layout: {}", e);
        }
    }

    /// Persist the current settings to disk.
    pub fn persist_settings(&self) {
        let _write = match self.write_lock.lock() {
            Ok(g) => g,
            Err(e) => {
                tracing::error!(
                    "Failed to acquire write_lock for settings persistence: {}",
                    e
                );
                return;
            }
        };
        let settings = match self.settings.lock() {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to acquire settings lock for persistence: {}", e);
                return;
            }
        };
        if let Err(e) = settings.save(&self.app_handle) {
            tracing::error!("Failed to persist settings: {}", e);
        }
    }
}

/// Initialise and run the BentoDesk application.
pub fn run() {
    // Initialise structured logging.
    // Honour RUST_LOG if set; otherwise default to "bentodesk=info".
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("bentodesk=info")),
        )
        .init();

    tracing::info!("BentoDesk starting...");

    // --- Set process priority to ABOVE_NORMAL in release builds ---
    // Desktop organizer should be responsive; slightly elevated priority
    // ensures the overlay and file watcher aren't starved by background tasks.
    #[cfg(not(debug_assertions))]
    {
        use windows::Win32::System::Threading::{
            GetCurrentProcess, SetPriorityClass, ABOVE_NORMAL_PRIORITY_CLASS,
        };
        unsafe {
            let process = GetCurrentProcess();
            if let Err(e) = SetPriorityClass(process, ABOVE_NORMAL_PRIORITY_CLASS) {
                tracing::warn!("Failed to set ABOVE_NORMAL priority: {e}");
            } else {
                tracing::info!("Process priority set to ABOVE_NORMAL");
            }
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Load or create default settings and layout
            let settings =
                config::settings::AppSettings::load_or_default(app.handle())?;
            let layout_data =
                layout::persistence::LayoutData::load_or_default(app.handle())?;
            // Theme B — enable the warm (on-disk) tier so LRU eviction from
            // the in-memory tier doesn't cost us another ExtractIconExW on
            // the next request. Warm dir lives next to the app data dir so
            // it's cleared alongside user settings on uninstall.
            let icon_warm_dir = app
                .path()
                .app_data_dir()
                .map(|p| p.join("icon_cache"))
                .unwrap_or_else(|_| std::path::PathBuf::from("icon_cache"));
            let icon_cache = icon::cache::IconCache::with_warm_dir(
                settings.icon_cache_size as usize,
                icon_warm_dir,
            );

            // --- Install crash handler (SEH) ---
            // Must happen after settings are loaded so we know the desktop path.
            // On unhandled exception, this restores hidden files from .bentodesk/
            // back to the desktop, preventing invisible-file data loss.
            crash_handler::install(std::path::PathBuf::from(&settings.desktop_path));

            // Register AppState early so subsequent setup steps can access it.
            // icon_backup starts as None — populated asynchronously below.
            app.manage(AppState {
                layout: Mutex::new(layout_data),
                settings: Mutex::new(settings.clone()),
                icon_cache,
                icon_backup: Mutex::new(None),
                app_handle: app.handle().clone(),
                timeline: Mutex::new(timeline::TimelineBuffer::default()),
                write_lock: Mutex::new(()),
            });

            // --- Timeline bootstrap ---
            // Load any previously persisted checkpoints and prime the
            // write-hook baseline against the current layout so subsequent
            // mutations produce a meaningful delta summary.
            {
                let handle = app.handle();
                let dir = timeline::hook::timeline_dir(handle);
                let store = timeline::checkpoint::CheckpointStore::new(dir);
                if let Some(state) = handle.try_state::<AppState>() {
                    if let Ok(mut buf) = state.timeline.lock() {
                        buf.reload(&store);
                    }
                }
                timeline::hook::init_baseline(handle);
            }

            // Save desktop icon positions on a background thread.
            // COM calls (STA) can take 100ms-1s+ depending on icon count;
            // doing this async avoids blocking window display and passthrough.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    match icon_positions::save_layout() {
                        Ok(layout) => {
                            let data_dir = icon_positions::default_data_dir();
                            if let Err(e) =
                                icon_positions::persist_to_file(&layout, &data_dir)
                            {
                                tracing::warn!(
                                    "Failed to persist icon layout backup to disk: {e}"
                                );
                            }
                            if let Some(state) = handle.try_state::<AppState>() {
                                if let Ok(mut backup) = state.icon_backup.lock() {
                                    *backup = Some(layout);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to save desktop icon positions: {e}");
                            let data_dir = icon_positions::default_data_dir();
                            if let Ok(Some(disk_backup)) =
                                icon_positions::load_from_file(&data_dir)
                            {
                                tracing::info!(
                                    "Loaded previous icon layout backup from disk as fallback"
                                );
                                if let Some(state) = handle.try_state::<AppState>() {
                                    if let Ok(mut backup) = state.icon_backup.lock() {
                                        *backup = Some(disk_backup);
                                    }
                                }
                            }
                        }
                    }
                });
            }

            // --- Ensure .bentodesk/ hidden directory exists ---
            {
                let _hdir = hidden_items::hidden_dir(app.handle());
            }

            // --- Legacy migration ---
            // Migrate from BOTH old architectures:
            // 1. AppData/hidden_items/ directory (old "file move" mode)
            // 2. Files hidden via attrib +h +s (old "reference mode")
            // Both are migrated into the new .bentodesk/ subfolder.
            {
                let migrated = hidden_items::cleanup_legacy_hidden_dir(app.handle());
                if migrated > 0 {
                    tracing::info!(
                        "Legacy migration: {} files migrated to .bentodesk/ subfolder mode",
                        migrated
                    );
                }
            }

            // --- Zone-level isolation migration ---
            // Migrate old flat .bentodesk/ files into zone subdirs based on
            // layout data. Files that are directly in .bentodesk/ (not in a
            // zone subdir) are moved to .bentodesk/{zone_id}/ based on which
            // zone references them.
            {
                let migrated = hidden_items::migrate_flat_to_zone_dirs(app.handle());
                if migrated > 0 {
                    tracing::info!(
                        "Zone isolation migration: {} files moved to zone subdirectories",
                        migrated
                    );
                }
            }

            // --- Verify references ---
            // Check that all referenced files still exist. Mark missing ones.
            {
                let missing = hidden_items::verify_references(app.handle());
                if !missing.is_empty() {
                    let state = app.state::<AppState>();
                    let mut layout = state.layout.lock().expect("layout lock poisoned at startup");
                    let mut marked = 0u32;
                    for zone in &mut layout.zones {
                        for item in &mut zone.items {
                            if let Some(ref orig) = item.original_path {
                                if missing.iter().any(|m| m == orig) {
                                    item.file_missing = true;
                                    marked += 1;
                                }
                            }
                        }
                    }
                    if marked > 0 {
                        layout.last_modified = chrono::Utc::now().to_rfc3339();
                        tracing::warn!(
                            "Startup: marked {} items as file_missing",
                            marked
                        );
                    }
                    drop(layout);
                    state.persist_layout();
                }
            }

            // --- Ensure .bentodesk/ folder stays hidden on startup ---
            // In subfolder mode, files are already inside .bentodesk/.
            // We just need to ensure the folder itself has +h +s attributes.
            {
                hidden_items::reapply_hidden_on_startup(app.handle());
            }

            // --- Stealth attribute guardian ---
            // Walks `.bentodesk/` + every zone subdir and re-asserts
            // HIDDEN | SYSTEM | NOT_CONTENT_INDEXED via Win32 directly.
            // Any failures (typically OneDrive sync holding a lock) enter
            // the retry queue and are re-applied on the next user-triggered
            // `reapply_stealth` IPC or on the next `hidden_dir` touch.
            //
            // Runs off-thread: on a OneDrive desktop the sweep can block on
            // per-file `SetFileAttributesW` waits. Keeping it off the Tauri
            // setup thread ensures the overlay window shows without delay.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    hidden_items::AttrGuard::startup_sweep(&handle);
                });
            }

            // --- Quarantine stale files on desktop root ---
            // Older versions may have left manifest.json / .bak files directly
            // on the desktop. These should only exist inside .bentodesk/.
            // We rename (not delete) with a timestamped prefix so any user
            // data accidentally stored at this path is preserved for recovery.
            {
                let desktop = {
                    let state = app.state::<AppState>();
                    let s = state.settings.lock().expect("settings lock");
                    std::path::PathBuf::from(&s.desktop_path)
                };
                let quarantine_ts =
                    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
                for stale_name in &[
                    "manifest.json",
                    "manifest.json.bak",
                    "manifest.json.tmp",
                ] {
                    let stale_path = desktop.join(stale_name);
                    if stale_path.exists() {
                        let dst = desktop.join(format!(
                            ".bentodesk-quarantine-{}-{}",
                            quarantine_ts, stale_name
                        ));
                        if let Err(e) = std::fs::rename(&stale_path, &dst) {
                            tracing::warn!(
                                "Failed to quarantine stale {}: {e}",
                                stale_path.display()
                            );
                        } else {
                            tracing::info!(
                                "Quarantined stale legacy file {} -> {}",
                                stale_path.display(),
                                dst.display()
                            );
                        }
                    }
                }
            }

            // Set up the always-on-bottom overlay (ghost layer)
            if settings.ghost_layer_enabled {
                if let Err(e) =
                    ghost_layer::manager::GhostLayerManager::attach(app.handle())
                {
                    tracing::warn!(
                        "Ghost layer failed, falling back to normal window: {}",
                        e
                    );
                }
            }

            // Enable passthrough IMMEDIATELY so the desktop stays clickable
            // while the WebView loads. Without this, the overlay window
            // captures all mouse events between ghost layer attach and
            // the frontend's onMount → enablePassthrough() call.
            if let Some(window) = app.get_webview_window("main") {
                if let Err(e) = window.set_ignore_cursor_events(true) {
                    tracing::warn!("Failed to set initial passthrough: {e}");
                }

                // Disable browser keyboard accelerators (F5 refresh, Ctrl+Shift+I devtools, etc.)
                // and default context menus in release builds to prevent exposing
                // WebView2 internals to end users.
                #[cfg(not(debug_assertions))]
                {
                    let _ = window.with_webview(|webview| {
                        // SAFETY: ICoreWebView2Settings is a well-documented COM interface.
                        // The controller lifetime is managed by Tauri; we hold a valid
                        // reference inside the with_webview closure.
                        unsafe {
                            use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings;

                            let core = webview.controller().CoreWebView2().unwrap();
                            let settings: ICoreWebView2Settings = core.Settings().unwrap();

                            // Disable the default WebView2 right-click context menu
                            let _ = settings.SetAreDefaultContextMenusEnabled(false);

                            // Disable DevTools (F12)
                            let _ = settings.SetAreDevToolsEnabled(false);
                        }
                    });
                }
            }

            // Start file system watcher on the Desktop directory
            let handle = app.handle().clone();
            watcher::desktop_watcher::setup_file_watcher(&handle)?;

            // Start resolution change monitor (polls every 2s, clamps zones on change)
            layout::resolution::start_resolution_monitor(app.handle());

            // Build the system tray icon and menu
            tray::menu::setup_tray(app)?;

            // Theme A — tray tooltip bubble: listen for `update:available`
            // and update the tray tooltip so the user notices even when the
            // settings window is hidden. The listener is set up after
            // `setup_tray` so `tray_by_id("main")` resolves.
            {
                #[allow(unused_imports)]
                use tauri::tray::TrayIconEvent;
                let handle = app.handle().clone();
                app.listen_any("update:available", move |event| {
                    let payload = event.payload();
                    let version = serde_json::from_str::<serde_json::Value>(payload)
                        .ok()
                        .and_then(|v| v.get("version").and_then(|s| s.as_str()).map(String::from))
                        .unwrap_or_else(|| "?".to_string());
                    if let Some(tray) = handle.tray_by_id("main") {
                        let _ = tray.set_tooltip(Some(format!(
                            "BentoDesk — update {version} available"
                        )));
                    }
                });

                // Kick off an initial check in the background so the
                // tooltip reflects reality shortly after launch. Honours
                // the user's `check_frequency` preference by short-circuiting
                // when Manual.
                let initial_settings = settings.clone();
                let handle_for_check = app.handle().clone();
                if crate::updater::check_interval_hours(&initial_settings).is_some() {
                    crate::updater::spawn_background_check(handle_for_check);
                }
            }

            // --- Safe mode detection ---
            // If Guardian gave up restarting after a crash loop, it writes a
            // safe_mode.json marker. Detect it, notify the frontend, and
            // remove the marker so subsequent launches are normal.
            if let Ok(app_data) = app.path().app_data_dir() {
                let safe_mode_path = app_data.join("safe_mode.json");
                if safe_mode_path.exists() {
                    tracing::warn!(
                        "Safe mode marker detected at {}",
                        safe_mode_path.display()
                    );
                    // Read the marker content for logging purposes
                    if let Ok(content) = std::fs::read_to_string(&safe_mode_path) {
                        tracing::warn!("Safe mode reason: {}", content);
                    }
                    // Notify frontend
                    if let Err(e) = app.emit("safe_mode_activated", ()) {
                        tracing::error!("Failed to emit safe_mode_activated event: {e}");
                    }
                    // Remove marker so next launch is normal
                    if let Err(e) = std::fs::remove_file(&safe_mode_path) {
                        tracing::error!(
                            "Failed to remove safe mode marker: {e}"
                        );
                    }
                }
            } else {
                tracing::warn!(
                    "app_data_dir unavailable, skipping safe-mode detection"
                );
            }

            // --- Theme E2-d: Rules scheduler ---
            // Spawns a tokio::interval(60s) task that polls rule list and
            // triggers any with RunMode::Interval whose last_run is older
            // than the configured minutes. Each execution records a timeline
            // checkpoint before mutating state so Ctrl+Z restores the prior
            // layout.
            rules::scheduler::spawn(app.handle().clone());

            // --- Theme E2-e: Live Folder rehydration ---
            // Walks the persisted layout and reinstates bindings for every
            // zone with `live_folder_path = Some(..)`. The singleton
            // debouncer is created lazily on the first bind.
            watcher::live_folder::rehydrate_from_layout(app.handle());

            // --- Theme E2-c: Mini Bar forwarder ---
            // Mini Bar webview windows emit `minibar-launch-item` when the
            // user clicks an icon button. Open the file via ShellExecuteW —
            // mirrors `commands::file_ops::open_file` but runs outside the
            // tauri::command return contract. Validation is omitted because
            // mini bar paths always originate from items already in a zone.
            {
                app.listen_any("minibar-launch-item", move |event| {
                    let payload = event.payload();
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
                        return;
                    };
                    let Some(path) = v.get("path").and_then(|p| p.as_str()) else {
                        return;
                    };
                    use std::ffi::OsStr;
                    use std::os::windows::ffi::OsStrExt;
                    let wide_path: Vec<u16> = OsStr::new(path)
                        .encode_wide()
                        .chain(std::iter::once(0))
                        .collect();
                    let wide_open: Vec<u16> = OsStr::new("open")
                        .encode_wide()
                        .chain(std::iter::once(0))
                        .collect();
                    unsafe {
                        windows::Win32::UI::Shell::ShellExecuteW(
                            None,
                            windows::core::PCWSTR(wide_open.as_ptr()),
                            windows::core::PCWSTR(wide_path.as_ptr()),
                            None,
                            None,
                            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
                        );
                    }
                });
            }

            tracing::info!("BentoDesk initialized successfully");
            Ok(())
        })
        .register_uri_scheme_protocol("bentodesk", |ctx, request| {
            let app = ctx.app_handle().clone();
            let state = app.state::<AppState>();
            icon::protocol::handle_icon_request(&app, &state.icon_cache, request)
        })
        .invoke_handler(tauri::generate_handler![
            // Zone commands
            commands::zone::create_zone,
            commands::zone::update_zone,
            commands::zone::delete_zone,
            commands::zone::list_zones,
            commands::zone::reorder_zones,
            // D2/D3: Stack + alias commands
            commands::zone::stack_zones,
            commands::zone::unstack_zones,
            commands::zone::set_zone_alias,
            commands::zone::reorder_stack,
            // Item commands
            commands::item::add_item,
            commands::item::remove_item,
            commands::item::move_item,
            commands::item::reorder_items,
            commands::item::toggle_item_wide,
            // File operations
            commands::file_ops::open_file,
            commands::file_ops::reveal_in_explorer,
            commands::file_ops::get_file_info,
            // Icon commands
            commands::icon::get_icon_url,
            commands::icon::preload_icons,
            commands::icon::clear_icon_cache,
            commands::icon::get_icon_cache_stats,
            // Layout commands
            commands::layout::save_snapshot,
            commands::layout::load_snapshot,
            commands::layout::list_snapshots,
            commands::layout::delete_snapshot,
            // Grouping commands
            commands::grouping::scan_desktop,
            commands::grouping::suggest_groups,
            commands::grouping::apply_auto_group,
            commands::grouping::auto_group_new_file,
            // Rule preview commands (R4-C2)
            ghost_layer::highlight_overlay::highlight_desktop_files,
            ghost_layer::highlight_overlay::clear_desktop_highlights,
            // Settings commands
            commands::settings::get_settings,
            commands::settings::update_settings,
            // System commands
            commands::system::get_system_info,
            commands::system::get_desktop_sources,
            commands::system::start_drag,
            commands::system::get_memory_usage,
            // WebView2 process-group memory (Theme B)
            commands::memory::get_webview2_memory,
            // Display / multi-monitor commands
            display::monitors::list_monitors,
            display::monitors::get_monitor_for_point,
            display::monitors::get_monitor_for_window,
            // Icon position commands
            commands::icon_positions::save_icon_layout,
            commands::icon_positions::restore_icon_layout,
            // Theme commands
            themes::list_themes,
            themes::get_theme,
            themes::get_active_theme,
            themes::set_active_theme,
            // Plugin commands
            commands::plugins::list_plugins,
            commands::plugins::install_plugin,
            commands::plugins::uninstall_plugin,
            commands::plugins::toggle_plugin,
            // Stealth / dotfolder visibility commands
            commands::stealth::get_stealth_status,
            commands::stealth::reapply_stealth,
            commands::stealth::check_onedrive_exclusion_needed,
            // Timeline / time-machine commands (R4-C1)
            commands::timeline::list_checkpoints,
            commands::timeline::get_checkpoint,
            commands::timeline::restore_checkpoint,
            commands::timeline::undo_checkpoint,
            commands::timeline::redo_checkpoint,
            commands::timeline::delete_checkpoint,
            commands::timeline::save_checkpoint_permanent,
            // Bulk operations (Theme C)
            commands::bulk::bulk_update_zones,
            commands::bulk::bulk_delete_zones,
            commands::bulk::apply_layout_algorithm,
            // Updater commands (Theme A)
            commands::updater::check_for_updates,
            commands::updater::download_update,
            commands::updater::install_update_and_restart,
            commands::updater::skip_update_version,
            // Settings vault (backup + encryption, Theme A)
            commands::config_vault::list_settings_backups,
            commands::config_vault::create_settings_backup,
            commands::config_vault::restore_settings_backup,
            commands::config_vault::set_encryption_mode,
            commands::config_vault::verify_passphrase,
            // Theme E — Custom icons (Lucide + user SVG/PNG/ICO)
            commands::icon::upload_custom_icon,
            commands::icon::list_custom_icons,
            commands::icon::delete_custom_icon,
            // Theme E2-a — Context Capsule
            commands::context_capsule::capture_context,
            commands::context_capsule::restore_context,
            commands::context_capsule::list_contexts,
            commands::context_capsule::delete_context,
            // Theme E2-c — Mini Bar
            commands::minibar::pin_zone_as_minibar,
            commands::minibar::unpin_minibar,
            commands::minibar::list_pinned_minibars,
            // Theme E2-e — Live Folder
            commands::live_folder::bind_zone_to_folder,
            commands::live_folder::unbind_zone_folder,
            commands::live_folder::scan_live_folder,
            // Theme E2-b — AI Recommender
            commands::grouping::get_ai_recommendations,
            // Theme E2-d — Rules Engine
            commands::rules::list_rules,
            commands::rules::create_rule,
            commands::rules::update_rule,
            commands::rules::delete_rule,
            commands::rules::preview_rule_hits,
            commands::rules::run_rule_now,
        ])
        .build(tauri::generate_context!())
        .expect("error building BentoDesk")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                // Exit lifecycle order:
                // 0. Flush the recovery bundle so a crash during restore has
                //    the latest state to recover from.
                // 1. Move all hidden files from .bentodesk/ back to their
                //    original Desktop paths.
                // 2. THEN restore icon positions -- the icons must be visible on
                //    the desktop before we can set their positions via COM.
                if let Some(state) = app_handle.try_state::<AppState>() {
                    if let Err(e) = recovery_bundle::refresh_from_state(&state) {
                        tracing::error!("Failed to refresh recovery bundle on exit: {e}");
                    }
                }

                hidden_items::restore_all_hidden(app_handle);

                // Restore icon positions (items are now back on the desktop)
                if let Some(state) = app_handle.try_state::<AppState>() {
                    if let Ok(backup) = state.icon_backup.lock() {
                        if let Some(ref layout) = *backup {
                            if let Err(e) = icon_positions::restore_layout(layout) {
                                tracing::error!(
                                    "Failed to restore icon positions on exit: {e}"
                                );
                            } else {
                                tracing::info!(
                                    "Desktop icon positions restored on exit"
                                );
                            }
                        }
                    }
                }
            }
        });
}
