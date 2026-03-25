# Technical Architecture

BentoDesk is a Windows desktop organizer built with **Tauri v2** (Rust backend) and **Solid.js** (TypeScript frontend). It renders glassmorphism "zones" that float above the desktop wallpaper, allowing users to organize desktop files into bento-box style containers.

## System Overview

```
+-------------------------------------------------------------+
|  Windows Desktop (Explorer shell / Progman)                  |
|                                                              |
|  +-------------------------------------------------------+  |
|  | Ghost Layer (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE)    |  |
|  | Tauri WebView2 window at HWND_BOTTOM z-order          |  |
|  |                                                       |  |
|  |  +----------+  +----------+  +----------+             |  |
|  |  | Zone A   |  | Zone B   |  | Zone C   |  (capsules) |  |
|  |  | (pill)   |  | (rounded)|  | (circle) |             |  |
|  |  +----------+  +----------+  +----------+             |  |
|  |                                                       |  |
|  +-------------------------------------------------------+  |
|                                                              |
|  [Regular application windows sit above the ghost layer]     |
+-------------------------------------------------------------+
```

## Module Map

### Rust Backend (`src-tauri/src/`)

| Module | File(s) | Purpose |
|--------|---------|---------|
| `lib.rs` | `lib.rs` | Application entry point, Tauri builder, module declarations |
| `commands/` | `zone.rs`, `item.rs`, `file_ops.rs`, `icon.rs`, `layout.rs`, `grouping.rs`, `settings.rs`, `system.rs`, `icon_positions.rs` | IPC command handlers (`#[tauri::command]`) |
| `ghost_layer/` | `manager.rs` | Desktop overlay window management via Win32 APIs |
| `hidden_items` | `hidden_items.rs` | File hiding/restoring via `.bentodesk/` subfolder |
| `layout/` | `persistence.rs`, `snapshot.rs`, `resolution.rs` | Zone layout persistence, snapshots, resolution monitoring |
| `config/` | `settings.rs` | Application settings load/save |
| `icon/` | `cache.rs`, `extractor.rs`, `protocol.rs` | Icon extraction, LRU caching, `bentodesk://` URI protocol |
| `icon_positions/` | `finder.rs`, `reader.rs`, `writer.rs` | Desktop icon position backup/restore via COM |
| `grouping/` | `scanner.rs`, `suggestions.rs`, `rules.rs` | Smart file grouping and auto-organization |
| `watcher/` | `desktop_watcher.rs` | Filesystem watcher for desktop directory changes |
| `tray/` | `menu.rs` | System tray icon and context menu |
| `drag_drop/` | `drag_manager.rs`, `data_object.rs`, `drop_source.rs` | Native Win32 OLE drag-and-drop |
| `error` | `error.rs` | Unified error types (`BentoDeskError`) |

### TypeScript Frontend (`src/`)

| Module | File(s) | Purpose |
|--------|---------|---------|
| `services/ipc.ts` | IPC wrappers | Type-safe `invoke()` calls to all Rust commands |
| `services/hitTest.ts` | Hit-test state machine | Cursor polling, `setIgnoreCursorEvents` toggling |
| `services/drag.ts` | Drag service | Frontend drag coordination |
| `services/dropTarget.ts` | Drop target | File drop handling |
| `services/events.ts` | Event bus | Tauri event listeners |
| `services/hotkeys.ts` | Hotkey service | Keyboard shortcut handling |
| `services/resolution.ts` | Resolution service | Display resolution change handling |
| `stores/zones.ts` | Zone store | Reactive zone state (Solid.js signals) |
| `stores/settings.ts` | Settings store | Reactive app settings |
| `stores/ui.ts` | UI store | Modal state, expanded zones, search |
| `themes/` | `types.ts`, `presets.ts`, `index.ts` | Theme system: types, 10 presets, CSS variable application |
| `i18n/` | `index.ts`, `locales/*.ts` | Internationalization with reactive locale switching |
| `types/` | `zone.ts`, `settings.ts`, `system.ts`, `events.ts` | TypeScript type definitions mirroring Rust models |

## Ghost Layer Overlay

The ghost layer is BentoDesk's core rendering mechanism. It creates a transparent WebView2 window that sits between the desktop wallpaper and normal application windows.

### Window Configuration

- **Extended styles**: `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` -- hides from Alt-Tab, prevents focus stealing
- **Z-order**: Positioned at `HWND_BOTTOM` -- above Progman (desktop shell), below all application windows
- **Decorations**: Explicitly removed via `GWL_STYLE` manipulation (no title bar, no borders)
- **Transparency**: Tauri window configured with `transparent: true`, `decorations: false`, `shadow: false`
- **Taskbar**: `skipTaskbar: true` -- does not appear in the taskbar

### WndProc Subclass

A custom `WndProc` subclass intercepts `WM_WINDOWPOSCHANGING` to prevent Windows from pushing the overlay behind the desktop when the user clicks elsewhere. A bypass flag (`BYPASS_SUBCLASS`) allows BentoDesk's own repositioning calls to pass through.

### Click-Through Mechanism

By default, clicks pass through the overlay to the desktop. The frontend's hit-test state machine uses `setIgnoreCursorEvents` to toggle click capture:

- **PASSTHROUGH** (default) -- clicks reach desktop icons
- **ZONE_HOVER** -- cursor is over a zone, clicks captured by webview
- **DRAGGING** -- zone capsule is being repositioned, always captured
- **MODAL_OPEN** -- a modal/dialog is visible, always captured
- **GRACE_PERIOD** -- cursor just left a zone, short delay before passthrough

## Hit-Test State Machine

```
PASSTHROUGH ----[cursor over zone]----> ZONE_HOVER
ZONE_HOVER  ----[cursor leaves]-------> GRACE_PERIOD
ZONE_HOVER  ----[drag starts]--------> DRAGGING
GRACE_PERIOD ---[timer expires]-------> PASSTHROUGH
GRACE_PERIOD ---[cursor re-enters]---> ZONE_HOVER
DRAGGING    ----[drag ends + in zone]-> ZONE_HOVER
DRAGGING    ----[drag ends + outside]-> GRACE_PERIOD
any         ----[modal opens]---------> MODAL_OPEN
MODAL_OPEN  ----[modal closes]--------> (previous state)
```

## File Hiding Mechanism

BentoDesk organizes desktop files by moving them into a hidden `.bentodesk/` subfolder on the same drive as the Desktop directory.

### How It Works

1. When a file is added to a zone, `fs::rename` moves it from `Desktop/file.txt` to `Desktop/.bentodesk/{zone_id}/file.txt`
2. The `.bentodesk/` folder has `attrib +h +s` (hidden + system) so it does not appear as a desktop icon
3. Same-drive rename is an instant operation (no file copy)
4. A **safety manifest** (`manifest.json`) inside `.bentodesk/` tracks every hidden file with its original path, zone ID, and timestamp

### Safety Invariants

- **Files are NEVER deleted** -- only moved between Desktop and `.bentodesk/`
- If `fs::rename` fails, the file stays visible at its original location (safe default)
- On application exit, ALL hidden files are automatically moved back to their original Desktop paths
- The safety manifest provides recovery even if `layout.json` is lost or corrupted
- Desktop icon positions are backed up on startup and restored on exit via COM APIs

### Exit Lifecycle

1. Move all hidden files from `.bentodesk/` back to their original Desktop paths
2. Restore desktop icon positions (icons must be visible before COM can set positions)

### Legacy Migration

On first run after upgrading from older versions:
- `cleanup_legacy_hidden_dir()` migrates files from the old AppData `hidden_items/` directory
- Files previously hidden via `attrib +h +s` directly are migrated to the subfolder architecture
- `migrate_flat_to_zone_dirs()` moves flat `.bentodesk/` files into zone subdirectories

## IPC Command List

All commands are invoked from the frontend via `@tauri-apps/api/core::invoke`.

### Zone Commands

| Command | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `create_zone` | `name`, `icon`, `position`, `expandedSize` | `BentoZone` | Create a new zone |
| `update_zone` | `id`, `updates: ZoneUpdate` | `BentoZone` | Update zone properties |
| `delete_zone` | `id` | `void` | Delete a zone and restore its files |
| `list_zones` | -- | `BentoZone[]` | List all zones |
| `reorder_zones` | `zoneIds: string[]` | `void` | Reorder zones by sort_order |

### Item Commands

| Command | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `add_item` | `zoneId`, `path` | `BentoItem` | Add a file to a zone (hides it) |
| `remove_item` | `zoneId`, `itemId` | `void` | Remove item from zone (restores file) |
| `move_item` | `fromZoneId`, `toZoneId`, `itemId` | `void` | Move item between zones |
| `reorder_items` | `zoneId`, `itemIds: string[]` | `void` | Reorder items within a zone |
| `toggle_item_wide` | `zoneId`, `itemId` | `BentoItem` | Toggle wide card display |

### File Operations

| Command | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `open_file` | `path` | `void` | Open file with default application |
| `reveal_in_explorer` | `path` | `void` | Show file in Windows Explorer |
| `get_file_info` | `path` | `FileInfo` | Get file metadata |

### Icon Commands

| Command | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `get_icon_url` | `path` | `string` | Get icon URL for a file path |
| `preload_icons` | `paths: string[]` | `void` | Preload icons into cache |
| `clear_icon_cache` | -- | `void` | Clear the icon LRU cache |

### Layout / Snapshot Commands

| Command | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `save_snapshot` | `name` | `DesktopSnapshot` | Save current layout as snapshot |
| `load_snapshot` | `id` | `void` | Load a saved snapshot |
| `list_snapshots` | -- | `DesktopSnapshot[]` | List all snapshots |
| `delete_snapshot` | `id` | `void` | Delete a snapshot |

### Smart Grouping Commands

| Command | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `scan_desktop` | -- | `FileInfo[]` | Scan desktop for files |
| `suggest_groups` | `files: string[]` | `SuggestedGroup[]` | Get grouping suggestions |
| `apply_auto_group` | `zoneId`, `rule: AutoGroupRule` | `BentoItem[]` | Apply auto-group rule to zone |
| `auto_group_new_file` | `filePath` | `[string, BentoItem][]` | Auto-add new file to matching zones |

### Settings Commands

| Command | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `get_settings` | -- | `AppSettings` | Get current settings |
| `update_settings` | `updates: SettingsUpdate` | `AppSettings` | Update settings |

### System Commands

| Command | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `get_system_info` | -- | `SystemInfo` | Get OS and display info |
| `start_drag` | `filePaths: string[]` | `string` | Initiate native OLE drag |
| `get_memory_usage` | -- | `MemoryInfo` | Get app memory usage |

### Icon Position Commands

| Command | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `save_icon_layout` | -- | `void` | Backup desktop icon positions |
| `restore_icon_layout` | -- | `void` | Restore desktop icon positions |

## Application State

The Rust backend maintains shared state via `AppState`, managed by Tauri:

```rust
pub struct AppState {
    pub layout: Mutex<LayoutData>,        // All zones + items
    pub settings: Mutex<AppSettings>,     // User preferences
    pub icon_cache: IconCache,            // LRU icon cache
    pub icon_backup: Mutex<Option<SavedIconLayout>>,  // Desktop icon positions
    pub app_handle: AppHandle,            // Tauri handle for persistence
}
```

State is persisted to JSON files in the Tauri app data directory. Layout and settings are saved after every mutation.

## Custom URI Protocol

BentoDesk registers a `bentodesk://` URI scheme for serving file icons to the WebView:

```
bentodesk://icon/{encoded_path}
```

The icon protocol handler extracts the system icon for the given file path, caches it in the LRU cache, and returns a PNG response.

## Key Dependencies

### Rust

| Crate | Version | Purpose |
|-------|---------|---------|
| `tauri` | 2.x | Application framework with WebView2 |
| `windows` | 0.58 | Win32 API bindings (shell, COM, DWM, HiDpi) |
| `serde` / `serde_json` | 1.x | Serialization for IPC and persistence |
| `tokio` | 1.x | Async runtime |
| `notify` | 7.x | Filesystem watcher |
| `lru` | 0.12 | LRU cache for icons |
| `image` | 0.25 | PNG encoding for icon protocol |
| `chrono` | 0.4 | Timestamps |
| `uuid` | 1.x | UUID v4 generation for zone/item IDs |

### Frontend

| Package | Version | Purpose |
|---------|---------|---------|
| `solid-js` | ^1.9 | Reactive UI framework |
| `@tauri-apps/api` | ^2 | Tauri IPC and window APIs |
| `typescript` | ^5.5 | Type safety |
| `vite` | ^6 | Build tool |
| `vite-plugin-solid` | ^2 | Solid.js Vite integration |
