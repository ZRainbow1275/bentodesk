//! Tauri IPC command handlers.
//!
//! Each sub-module exposes `#[tauri::command]` functions that the frontend
//! invokes via `@tauri-apps/api/core::invoke`.

pub mod file_ops;
pub mod grouping;
pub mod icon;
pub mod icon_positions;
pub mod item;
pub mod layout;
pub mod plugins;
pub mod settings;
pub mod stealth;
pub mod system;
pub mod timeline;
pub mod zone;
