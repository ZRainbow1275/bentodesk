//! Tauri IPC command handlers.
//!
//! Each sub-module exposes `#[tauri::command]` functions that the frontend
//! invokes via `@tauri-apps/api/core::invoke`.

pub mod bulk;
pub mod config_vault;
pub mod context_capsule;
pub mod file_ops;
pub mod grouping;
pub mod icon;
pub mod icon_positions;
pub mod item;
pub mod layout;
pub mod live_folder;
pub mod memory;
pub mod minibar;
pub mod plugins;
pub mod rules;
pub mod settings;
pub mod stealth;
pub mod system;
pub mod timeline;
pub mod updater;
pub mod zone;
