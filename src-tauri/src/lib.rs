//! BentoDesk: Zero-interference desktop visual organizer.
//!
//! This is the library root. It declares all backend modules and provides
//! the [`run`] function that initialises the Tauri application.

mod commands;
mod config;
mod drag_drop;
mod error;
mod ghost_layer;
mod grouping;
mod hidden_items;
mod icon;
mod icon_positions;
mod layout;
mod tray;
mod watcher;

use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// Shared application state managed by Tauri.
pub struct AppState {
    pub layout: Mutex<layout::persistence::LayoutData>,
    pub settings: Mutex<config::settings::AppSettings>,
    pub icon_cache: icon::cache::IconCache,
    pub icon_backup: Mutex<Option<icon_positions::SavedIconLayout>>,
    pub app_handle: AppHandle,
}

impl AppState {
    /// Persist the current layout to disk.
    ///
    /// Call this after any mutation to `self.layout` to ensure changes survive
    /// application restarts. Errors are logged but not propagated so that the
    /// in-memory state remains authoritative.
    pub fn persist_layout(&self) {
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
    // Initialise structured logging
    tracing_subscriber::fmt()
        .with_env_filter("bentodesk=info")
        .init();

    tracing::info!("BentoDesk starting...");

    tauri::Builder::default()
        .setup(|app| {
            // Load or create default settings and layout
            let settings =
                config::settings::AppSettings::load_or_default(app.handle())?;
            let layout_data =
                layout::persistence::LayoutData::load_or_default(app.handle())?;
            let icon_cache =
                icon::cache::IconCache::new(settings.icon_cache_size as usize);

            // Save current desktop icon positions before BentoDesk modifies anything.
            // This runs on a background thread because COM calls must happen on an
            // STA thread, and the setup closure may not be on one.
            let icon_backup = match icon_positions::save_layout() {
                Ok(layout) => {
                    // Persist to disk as a safety net
                    let data_dir = icon_positions::default_data_dir();
                    if let Err(e) = icon_positions::persist_to_file(&layout, &data_dir) {
                        tracing::warn!("Failed to persist icon layout backup to disk: {e}");
                    }
                    Some(layout)
                }
                Err(e) => {
                    tracing::warn!("Failed to save desktop icon positions: {e}");
                    // Try loading from a previous backup on disk
                    let data_dir = icon_positions::default_data_dir();
                    match icon_positions::load_from_file(&data_dir) {
                        Ok(backup) => {
                            if backup.is_some() {
                                tracing::info!(
                                    "Loaded previous icon layout backup from disk as fallback"
                                );
                            }
                            backup
                        }
                        Err(e2) => {
                            tracing::warn!("Failed to load fallback icon backup: {e2}");
                            None
                        }
                    }
                }
            };

            app.manage(AppState {
                layout: Mutex::new(layout_data),
                settings: Mutex::new(settings.clone()),
                icon_cache,
                icon_backup: Mutex::new(icon_backup),
                app_handle: app.handle().clone(),
            });

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

            // Start file system watcher on the Desktop directory
            let handle = app.handle().clone();
            watcher::desktop_watcher::setup_file_watcher(&handle)?;

            // Start resolution change monitor (polls every 2s, clamps zones on change)
            layout::resolution::start_resolution_monitor(app.handle());

            // Build the system tray icon and menu
            tray::menu::setup_tray(app)?;

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
            // Settings commands
            commands::settings::get_settings,
            commands::settings::update_settings,
            // System commands
            commands::system::get_system_info,
            commands::system::start_drag,
            commands::system::get_memory_usage,
            // Icon position commands
            commands::icon_positions::save_icon_layout,
            commands::icon_positions::restore_icon_layout,
        ])
        .build(tauri::generate_context!())
        .expect("error building BentoDesk")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                // Exit lifecycle order:
                // 1. Move all hidden files from .bentodesk/ back to their
                //    original Desktop paths.
                // 2. THEN restore icon positions -- the icons must be visible on
                //    the desktop before we can set their positions via COM.
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
