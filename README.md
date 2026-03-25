# BentoDesk

**A bento-box style desktop organizer for Windows.** Frosted glass zones float above your wallpaper, letting you visually group and manage desktop files without cluttering your screen.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Windows](https://img.shields.io/badge/Platform-Windows%2010%2F11-0078D6.svg)]()
[![Tauri v2](https://img.shields.io/badge/Tauri-v2-FFC131.svg)](https://v2.tauri.app)

## Screenshots

<!-- TODO: Add screenshots -->
*Screenshots coming soon.*

## Features

### Desktop Organization
- **Bento Zones** -- Create draggable, resizable glassmorphism containers on your desktop
- **Drag & Drop** -- Drag files from Explorer or other zones into any zone
- **Smart Grouping** -- AI-powered file grouping suggestions based on extension, name patterns, and modification dates
- **Auto-Organization** -- Set rules to automatically sort new files into the right zone
- **Layout Snapshots** -- Save and restore your entire zone layout

### Visual Customization
- **10 Built-in Themes** -- Dark, Light, Midnight, Forest, Sunset, Frosted, Solid, Order (Bauhaus), Neo (Neomorphism), Flat
- **Custom Themes** -- Import/export themes as JSON, full CSS variable control
- **Capsule Shapes** -- Pill, Rounded, Circle, or Minimal collapsed zone styles
- **Capsule Sizes** -- Small, Medium, or Large
- **Per-Zone Accent Colors** -- Override theme accent on a per-zone basis

### Desktop Integration
- **Ghost Layer** -- Transparent overlay sits between wallpaper and app windows; invisible to Alt-Tab
- **Click-Through** -- Clicks pass through to desktop icons by default; only captured when hovering zones
- **Native Drag** -- Win32 OLE drag-and-drop for seamless file interaction
- **System Tray** -- Minimal tray icon, full context menu
- **File Watcher** -- Automatically detects new files on the desktop
- **Icon Position Backup** -- Saves and restores desktop icon positions on exit

### Internationalization
- **Chinese (Simplified)** and **English** included
- Reactive locale switching with localStorage persistence
- Type-safe translation keys

---

## Security Notice

> **BentoDesk moves desktop files into a hidden `.bentodesk/` folder on your Desktop directory to organize them into zones.**

This is how it works under the hood:

1. When you add a file to a zone, BentoDesk **moves** (not copies) it from `Desktop/file.txt` to `Desktop/.bentodesk/{zone_id}/file.txt` using a same-drive `fs::rename` (instant, no data copy).
2. The `.bentodesk/` folder is marked with `attrib +h +s` (hidden + system) so it does not appear as a desktop icon.
3. **Files are NEVER deleted.** They are only moved between your Desktop and the `.bentodesk/` subfolder.
4. **On exit, all files are automatically restored** to their original Desktop locations.
5. A **safety manifest** (`manifest.json`) inside `.bentodesk/` tracks every hidden file, providing recovery even if the application crashes.
6. Desktop icon positions are backed up on startup and restored on exit.

### Recovery

If BentoDesk exits unexpectedly (crash, power loss):

- Your files are safely inside `Desktop/.bentodesk/`. You can show hidden files in Explorer (View > Hidden items) and move them back manually.
- The safety manifest at `Desktop/.bentodesk/manifest.json` lists every file's original path.
- On next launch, BentoDesk will detect the state and attempt automatic recovery.

> **DO NOT manually delete the `.bentodesk/` folder while BentoDesk is running.** This will cause data loss for files currently organized in zones.

---

## Installation

### Prerequisites

- **Windows 10** (version 1809+) or **Windows 11**
- **WebView2 Runtime** -- included with Windows 11; Windows 10 users may need to [download it](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)

### Build from Source

Requires:
- [Rust](https://rustup.rs/) (latest stable)
- [Node.js](https://nodejs.org/) (v18+)
- [pnpm](https://pnpm.io/) (v8+)

```bash
# Clone the repository
git clone https://github.com/your-org/bentodesk.git
cd bentodesk

# Install frontend dependencies
pnpm install

# Run in development mode (hot-reload)
pnpm tauri dev

# Build for production
pnpm tauri build
```

The production build generates NSIS and MSI installers in `src-tauri/target/release/bundle/`.

---

## Configuration

### Settings

Access settings via the system tray icon or the gear icon in the UI.

| Setting | Description |
|---------|-------------|
| Desktop Embed Layer | Enable/disable the ghost layer overlay |
| Launch at Startup | Auto-start with Windows |
| Show in Taskbar | Toggle taskbar visibility |
| Smart Auto Group | Enable automatic file grouping |
| Portable Mode | Store data alongside the executable (restart required) |
| Desktop Path | Override the detected desktop path |
| Watch Paths | Additional directories to monitor |
| Theme | Choose from 10 built-in or custom themes |
| Accent Color | Global accent color |
| Expand Delay | Delay before zone expands on hover |
| Collapse Delay | Delay before zone collapses on mouse leave |
| Icon Cache Size | Number of icons to cache in memory |
| Language | Chinese or English |

### Theme Customization

BentoDesk themes control 27 CSS variables covering surfaces, borders, text, accents, shadows, blur, and radii. You can:

1. **Pick a built-in theme** from the theme picker in Settings
2. **Import a custom theme** as JSON via Settings > Developer Options > Import Theme
3. **Export any theme** as JSON for sharing

See the [Theme API Guide](docs/theme-api.md) for full documentation.

### Quick Custom Theme Example

```json
{
  "id": "my-theme",
  "name_key": "themeCustom",
  "preview_colors": ["#1a1a2e", "#e94560", "#eee", "#16213e"],
  "surface_zen": "rgba(26, 26, 46, 0.6)",
  "surface_expanded": "rgba(22, 33, 62, 0.88)",
  "surface_hover": "rgba(233, 69, 96, 0.1)",
  "surface_active": "rgba(233, 69, 96, 0.06)",
  "surface_subtle": "rgba(233, 69, 96, 0.03)",
  "border_zen": "rgba(233, 69, 96, 0.15)",
  "border_expanded": "rgba(233, 69, 96, 0.2)",
  "border_hover": "rgba(233, 69, 96, 0.35)",
  "text_primary": "#eeeeee",
  "text_secondary": "#aaaacc",
  "text_muted": "#666688",
  "accent_blue": "#e94560",
  "accent_purple": "#8b5cf6",
  "accent_green": "#22c55e",
  "accent_orange": "#f97316",
  "accent_pink": "#ec4899",
  "accent_red": "#ef4444",
  "shadow_zen": "0 2px 8px rgba(0,0,0,0.2), 0 8px 32px rgba(0,0,0,0.4)",
  "shadow_expanded": "0 4px 16px rgba(0,0,0,0.25), 0 16px 48px rgba(0,0,0,0.5)",
  "shadow_item_hover": "0 2px 8px rgba(233,69,96,0.08), 0 8px 24px rgba(0,0,0,0.1)",
  "blur_zen": "blur(22px) saturate(180%)",
  "blur_expanded": "blur(28px) saturate(190%)",
  "badge_bg": "rgba(233, 69, 96, 0.15)",
  "radius_capsule": "24px",
  "radius_expanded": "16px",
  "radius_card": "10px",
  "radius_badge": "10px"
}
```

---

## Developer Guide

### Documentation

Full developer documentation is available in the [`docs/`](docs/) folder:

- [Architecture](docs/architecture.md) -- System overview, module map, IPC commands, data flow
- [Theme API](docs/theme-api.md) -- Complete theme development guide with CSS variable reference
- [Zone API](docs/zone-api.md) -- Zone and item data model, types, grid layout
- [i18n Guide](docs/i18n-guide.md) -- Adding languages, translation keys, locale system

### Project Structure

```
bentodesk/
  src/                        # Frontend (Solid.js + TypeScript)
    components/               # UI components
    i18n/                     # Internationalization
      locales/                # Language files (zh-CN.ts, en.ts)
    services/                 # IPC wrappers, hit-test, drag, events
    stores/                   # Reactive state (zones, settings, UI)
    themes/                   # Theme system (types, presets, store)
    types/                    # TypeScript type definitions
  src-tauri/                  # Backend (Rust + Tauri v2)
    src/
      commands/               # IPC command handlers
      config/                 # Settings management
      drag_drop/              # Native Win32 OLE drag-and-drop
      ghost_layer/            # Desktop overlay via Win32 APIs
      grouping/               # Smart file grouping
      hidden_items.rs         # File hiding via .bentodesk/ subfolder
      icon/                   # Icon extraction, caching, URI protocol
      icon_positions/         # Desktop icon position backup/restore
      layout/                 # Zone persistence, snapshots, resolution
      tray/                   # System tray
      watcher/                # Filesystem watcher
  docs/                       # Developer documentation
```

### Development Commands

```bash
# Start development server with hot-reload
pnpm tauri dev

# Type-check frontend
pnpm build    # runs tsc && vite build

# Build production installer
pnpm tauri build
```

### Theme Quick Start

1. Export an existing theme: call `exportThemeAsJSON("dark")` in the browser console
2. Edit the JSON to customize colors
3. Change the `id` to a unique name
4. Import via Settings > Developer Options > Import Theme

See [Theme API](docs/theme-api.md) for the full programmatic API.

---

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Backend | **Rust** + **Tauri v2** |
| Frontend | **Solid.js** + **TypeScript** |
| Rendering | **WebView2** (Chromium-based) |
| Desktop Integration | **Win32 APIs** (COM, DWM, Shell, HiDpi) |
| Build | **Vite** (frontend), **Cargo** (backend) |
| Packaging | **NSIS** / **MSI** installers |

---

## License

[MIT](LICENSE)
