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

mod commands;
mod config;
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
mod plugins;
mod power;
mod recovery_bundle;
mod startup;
pub(crate) mod storage;
mod themes;
mod timeline;
mod tray;
mod watcher;

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

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
        .setup(|app| {
            // Load or create default settings and layout
            let settings =
                config::settings::AppSettings::load_or_default(app.handle())?;
            let layout_data =
                layout::persistence::LayoutData::load_or_default(app.handle())?;
            let icon_cache =
                icon::cache::IconCache::new(settings.icon_cache_size as usize);

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

            // --- Clean up stale files on desktop root ---
            // Older versions may have left manifest.json / .bak files directly
            // on the desktop. These should only exist inside .bentodesk/.
            {
                let desktop = {
                    let state = app.state::<AppState>();
                    let s = state.settings.lock().expect("settings lock");
                    std::path::PathBuf::from(&s.desktop_path)
                };
                for stale_name in &[
                    "manifest.json",
                    "manifest.json.bak",
                    "manifest.json.tmp",
                ] {
                    let stale_path = desktop.join(stale_name);
                    if stale_path.exists() {
                        if let Err(e) = std::fs::remove_file(&stale_path) {
                            tracing::warn!(
                                "Failed to remove stale {}: {e}",
                                stale_path.display()
                            );
                        } else {
                            tracing::info!(
                                "Removed stale legacy file: {}",
                                stale_path.display()
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

            // --- Safe mode detection ---
            // If Guardian gave up restarting after a crash loop, it writes a
            // safe_mode.json marker. Detect it, notify the frontend, and
            // remove the marker so subsequent launches are normal.
            {
                let app_data = app
                    .path()
                    .app_data_dir()
                    .expect("app_data_dir must be available");
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
            }

            tracing::info!("BentoDesk initialized successfully");
            Ok(())
        })
        .register_uri_scheme_protocol("bentodesk", |ctx, request| {
            let state = ctx.app_handle().state::<AppState>();
            icon::protocol::handle_icon_request(&state.icon_cache, request)
        })
        .invoke_handler(tauri::generate_handler![
            // Zone commands
            commands::zone::create_zone,
            commands::zone::update_zone,
            commands::zone::delete_zone,
            commands::zone::list_zones,
            commands::zone::reorder_zones,
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
