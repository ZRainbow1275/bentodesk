//! System tray icon and context menu setup.
//!
//! The tray provides quick access to show/hide BentoDesk, create zones,
//! open settings, and exit the application. Left-clicking the tray icon
//! toggles window visibility; right-clicking shows the context menu.
//! The show/hide menu item text dynamically reflects the current window state.

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Emitter, Manager,
};

use crate::error::BentoDeskError;
use crate::ghost_layer::manager as overlay;
use crate::hidden_items;
use crate::icon_positions;
use crate::AppState;

/// Build the system tray icon and context menu.
///
/// Sets up:
/// - Right-click context menu with 隐藏 BentoDesk, New Zone, Settings, About, Exit
/// - Left-click handler that toggles the main window visibility
/// - Dynamic menu text that reflects current window visibility state
pub fn setup_tray(app: &App) -> Result<(), BentoDeskError> {
    // Window starts visible (per tauri.conf.json), so initial text is "Hide"
    let show_item =
        MenuItem::with_id(app, "show_hide", "隐藏 BentoDesk", true, None::<&str>)
            .map_err(|e| {
                BentoDeskError::ConfigError(format!("Failed to create menu item: {e}"))
            })?;
    let new_zone_item = MenuItem::with_id(app, "new_zone", "新建区域", true, None::<&str>)
        .map_err(|e| BentoDeskError::ConfigError(format!("Failed to create menu item: {e}")))?;
    let auto_organize_item =
        MenuItem::with_id(app, "auto_organize", "智能整理桌面", true, None::<&str>)
            .map_err(|e| {
                BentoDeskError::ConfigError(format!("Failed to create menu item: {e}"))
            })?;
    let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)
        .map_err(|e| BentoDeskError::ConfigError(format!("Failed to create menu item: {e}")))?;
    let about_item = MenuItem::with_id(app, "about", "关于", true, None::<&str>)
        .map_err(|e| BentoDeskError::ConfigError(format!("Failed to create menu item: {e}")))?;
    let exit_item = MenuItem::with_id(app, "exit", "退出", true, None::<&str>)
        .map_err(|e| BentoDeskError::ConfigError(format!("Failed to create menu item: {e}")))?;

    let menu = Menu::with_items(
        app,
        &[
            &show_item,
            &new_zone_item,
            &auto_organize_item,
            &settings_item,
            &about_item,
            &exit_item,
        ],
    )
    .map_err(|e| BentoDeskError::ConfigError(format!("Failed to create menu: {e}")))?;

    // Clone the menu item for use inside the event closures so we can
    // update its text dynamically when visibility toggles.
    let show_item_for_menu = show_item.clone();
    let show_item_for_tray = show_item.clone();

    // Load tray icon from the embedded 32x32 PNG (generated from bentodesk.svg).
    // Using include_bytes! embeds the icon at compile time — no file I/O at runtime.
    let tray_icon = {
        let png_bytes = include_bytes!("../../icons/32x32.png");
        let img = image::load_from_memory(png_bytes).expect("Failed to decode tray icon PNG");
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        tauri::image::Image::new_owned(rgba.into_raw(), w, h)
    };

    let _tray = TrayIconBuilder::new()
        .icon(tray_icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("BentoDesk")
        .on_menu_event(move |app_handle, event| {
            match event.id().as_ref() {
                "show_hide" => {
                    toggle_main_window(app_handle, &show_item_for_menu);
                }
                "new_zone" => {
                    tracing::info!("New zone requested from tray");
                    let _ = app_handle.emit("tray_new_zone", ());
                }
                "auto_organize" => {
                    tracing::info!("Auto-organize requested from tray");
                    // Ensure the window is visible so the user can see the dialog
                    if !overlay::is_visible() {
                        overlay::show_window();
                        let _ = show_item_for_menu.set_text("隐藏 BentoDesk");
                    }
                    let _ = app_handle.emit("tray_auto_organize", ());
                }
                "settings" => {
                    tracing::info!("Settings requested from tray");
                    let _ = app_handle.emit("tray_settings", ());
                }
                "about" => {
                    tracing::info!("About requested from tray");
                    let _ = app_handle.emit("tray_about", ());
                }
                "exit" => {
                    tracing::info!("Exit requested from tray — restoring hidden items and icon positions");
                    hidden_items::restore_all_hidden(app_handle);
                    restore_icons_before_exit(app_handle);
                    app_handle.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(move |tray, event| {
            // Left-click toggles window visibility
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle(), &show_item_for_tray);
            }
        })
        .build(app)
        .map_err(|e| BentoDeskError::ConfigError(format!("Failed to build tray: {e}")))?;

    tracing::info!("System tray initialized");
    Ok(())
}

/// Toggle the main window's visibility and update the tray menu item text.
///
/// Uses the shared `AtomicBool` visibility flag from `ghost_layer::manager`
/// instead of querying `window.is_visible()`, which can return stale state
/// when we bypass Tauri's show/hide with direct `ShowWindow` calls.
///
/// Show uses `ShowWindow(SW_SHOWNOACTIVATE)` to prevent the overlay from
/// stealing focus and triggering Windows Focus Assist.
/// Hide uses `ShowWindow(SW_HIDE)` directly.
fn toggle_main_window(
    _app_handle: &tauri::AppHandle,
    show_hide_item: &MenuItem<tauri::Wry>,
) {
    let now_visible = if overlay::is_visible() {
        overlay::hide_window();
        false
    } else {
        overlay::show_window();
        true
    };

    // Update menu text: when visible show "Hide", when hidden show "Show"
    let label = if now_visible {
        "隐藏 BentoDesk"
    } else {
        "显示 BentoDesk"
    };
    let _ = show_hide_item.set_text(label);
}

/// Restore desktop icon positions from the in-memory backup before exiting.
///
/// Falls back to the on-disk backup if the in-memory state is unavailable.
/// Errors are logged but do not prevent the exit.
fn restore_icons_before_exit(app_handle: &tauri::AppHandle) {
    // Try in-memory backup first
    if let Some(state) = app_handle.try_state::<AppState>() {
        if let Ok(backup) = state.icon_backup.lock() {
            if let Some(ref layout) = *backup {
                match icon_positions::restore_layout(layout) {
                    Ok(()) => {
                        tracing::info!("Desktop icon positions restored successfully");
                        return;
                    }
                    Err(e) => {
                        tracing::error!("Failed to restore icon positions from memory: {e}");
                    }
                }
            }
        }
    }

    // Fallback: try loading from disk
    let data_dir = icon_positions::default_data_dir();
    match icon_positions::load_from_file(&data_dir) {
        Ok(Some(layout)) => {
            if let Err(e) = icon_positions::restore_layout(&layout) {
                tracing::error!("Failed to restore icon positions from disk backup: {e}");
            } else {
                tracing::info!("Desktop icon positions restored from disk backup");
            }
        }
        Ok(None) => {
            tracing::warn!("No icon layout backup available for restore");
        }
        Err(e) => {
            tracing::error!("Failed to load icon backup from disk: {e}");
        }
    }
}
