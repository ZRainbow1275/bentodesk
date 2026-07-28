<div align="center">
  <img src="crates/bentodesk-app/assets/app-icon.png" width="112" alt="BentoDesk icon">

  # BentoDesk

  **Powered by Rust, BentoDesk is a refined, next-generation bento-box organizer for the Windows desktop.**

  [English](README.md) · [简体中文](README.zh-CN.md)

  [![Latest release](https://img.shields.io/github/v/release/ZRainbow1275/bentodesk?style=flat-square&label=release)](https://github.com/ZRainbow1275/bentodesk/releases/latest)
  [![Downloads](https://img.shields.io/github/downloads/ZRainbow1275/bentodesk/total?style=flat-square&label=downloads)](https://github.com/ZRainbow1275/bentodesk/releases)
  [![Stars](https://img.shields.io/github/stars/ZRainbow1275/bentodesk?style=flat-square)](https://github.com/ZRainbow1275/bentodesk/stargazers)
  [![CI](https://img.shields.io/github/actions/workflow/status/ZRainbow1275/bentodesk/ci.yml?branch=main&style=flat-square&label=build)](https://github.com/ZRainbow1275/bentodesk/actions/workflows/ci.yml)
  [![Windows](https://img.shields.io/badge/Windows-10%20%2F%2011-0078D4?style=flat-square&logo=windows11&logoColor=white)](#requirements)
  [![Rust](https://img.shields.io/badge/Rust-2024-000000?style=flat-square&logo=rust&logoColor=white)](#technology)
  [![License](https://img.shields.io/badge/License-AGPL--3.0-22c55e?style=flat-square)](LICENSE)

  [Download](https://github.com/ZRainbow1275/bentodesk/releases/latest) ·
  [Features](#features) ·
  [Usage](#quick-start) ·
  [Build](#building-from-source) ·
  [Contributing](#contributing)
</div>

<!-- MEDIA: docs/media/hero.webp | 16:9 | BentoDesk 2.0 desktop overview and Zone motion. Replace this comment only with current native UI media supplied by the maintainer. -->

## Why BentoDesk

I keep almost everything I am working on within reach on the desktop. That works—until a few active projects turn it into a junk drawer.

BentoDesk began as my way out of that mess. It groups files into Zones that stay compact when idle and open when needed. The files remain ordinary Windows files; BentoDesk only changes how they are arranged and brought back.

Version 2.0 rebuilds the idea in Rust on the native Windows graphics stack. It keeps the features of 1.x while replacing its web runtime and rough edges.

## Four qualities

| | |
| --- | --- |
| **Focused** | A single native process. The release EXE is about 2.40 MiB, and Private Bytes stayed below 18 MiB in the strict five-Zone measurement. |
| **Refined** | Zones, Stacks, themes, text, and motion share one geometry and state model instead of composing desktop UI from web layers. |
| **Direct** | Drag and drop, search, stacking, batch layouts, snapshots, and the timeline operate on real desktop content. |
| **Local-first** | Data remains local by default. Settings can use DPAPI or passphrase encryption, while plugin install, enablement, and removal are checked against validated paths and manifests. |

## How it differs

| Approach | Visible at a glance | Runtime | Data | Organization and extension |
| --- | --- | --- | --- | --- |
| Windows folders | After opening | Explorer | Local | Folders and system search |
| Desktop-fence products | Yes | Product-dependent | Product-dependent | Primarily fences and layouts |
| Electron / WebView organizers | Yes | Browser engine + application layer | Product-dependent | Web technology ecosystem |
| **BentoDesk 2.0** | Persistent capsules, expanded on demand | Rust + Win32 + DirectComposition | Local, optionally encrypted | Zones, Stacks, plugins, rules, snapshots, and timeline |

BentoDesk does not replace Explorer, and it is not intended for every workflow. It focuses on one problem: keeping desktop work visible without letting it fill the screen.

## Features

### Zones: quiet when collapsed, complete when open

Zones support several widths, capsule sizes, and corner styles. The title, icon, item-count badge, and expanded content share one layout. Expansion can be driven by hover, click, or an always-expanded mode.

<!-- MEDIA: docs/media/zone-motion.webp | 16:9 | One Zone showing collapsed, expand, rapid reversal and settled expanded states. -->

### Drag, Stack, and desktop integration

Windows Shell/OLE drag and drop supports moving and copying between Zones, restoring items to the desktop, binding folders, and forming Stacks. Empty desktop space remains click-through, while ordinary application windows can cover BentoDesk normally.

<!-- MEDIA: docs/media/drag-stack.webp | 16:9 | Real file drag into/out of a Zone and two Zones forming a Stack. -->

### Search, editing, and batch management

Search inside a Zone filters that Zone; global search spans all Zones. The editor changes names, aliases, icons, accents, columns, widths, and corners. Batch management supports selection, show/hide, move, delete, and five layout algorithms.

<!-- MEDIA: docs/media/search-bulk.webp | 16:9 | Local Zone search followed by the native bulk manager. -->

### Settings, themes, and two languages

Settings is a native, draggable, scrollable, non-topmost window. It includes light and dark themes, accent colors, performance controls, startup options, backup, encryption, and updater settings. Chinese and English are built in; the first launch follows the Windows UI language, and the choice can be changed in Settings.

<!-- MEDIA: docs/media/settings-themes.webp | 16:9 | Settings Appearance page switching between verified light and dark themes. -->

### Smart grouping, plugins, and live folders

Smart grouping creates reviewable suggestions from real desktop files. Local plugin packages can be installed, enabled, disabled, persisted, and removed with confirmation. Live folders synchronize changes from watched directories into their Zones.

<!-- MEDIA: docs/media/smart-group-plugins.webp | 16:9 | Smart grouping review and plugin management using current native surfaces. -->

### Snapshots, timeline, and recovery

Layouts can be saved and loaded as snapshots, and structural changes can be restored from the timeline. The tray, global hotkeys, and native utility windows provide access to management tools. Recovery and updater paths keep their local validation boundaries and do not launch a browser engine or helper application.

<!-- MEDIA: docs/media/snapshots-timeline.webp | 16:9 | Layout snapshot and timeline recovery flow. -->

## Quick start

1. Download `BentoDesk-2.0.0-windows-x64-portable.zip` from [Releases](https://github.com/ZRainbow1275/bentodesk/releases/latest).
2. Extract it to a normal writable directory and run `BentoDesk.exe`.
3. Create or manage Zones from the tray menu.
4. Drag files, folders, or shortcuts into a Zone.
5. Choose a theme, expansion mode, and language in Settings.

The portable package does not require Node.js, WebView2, or another browser runtime. State is stored in the current user profile by default. Portable mode can keep it beside the executable instead.

### Requirements

- Windows 10 1809+ or Windows 11;
- x86-64 processor;
- current Windows updates and graphics drivers are recommended.

## Current candidate measurements

Measured in one isolated Windows x64 run; hardware will vary:

| Metric | Result |
| --- | ---: |
| Release EXE | 2,518,016 bytes (2.40 MiB) |
| Strict scene | 5 Zones / 50 items / 1 process |
| Private Bytes at t30 | 17.05 MiB |
| Private Bytes at t60 | 16.95 MiB |
| Full Zone expand / collapse | 234 ms / 234 ms |
| Frame interval median / p95 | 16 ms / 16 ms |

## Technology

| Layer | Implementation |
| --- | --- |
| Language and runtime | Rust 2024, single process |
| Windows and input | Win32 / USER32 / DWM |
| Graphics | Direct2D, DirectWrite, DirectComposition, D3D11 |
| Icons and images | Windows Imaging Component, Windows Shell |
| File interaction | Shell/OLE, ReadDirectoryChangesW |
| Network and system security | WinHTTP, DPAPI |
| Data | Atomic local persistence, encrypted settings vault |
| Build | MSVC x64, static CRT, size optimization, Fat LTO |

Primary crates:

```text
bentodesk-shell      Process entry, Win32 message routing, system integration
bentodesk-app        Application state, interaction, render projection
bentodesk-backend    Settings, plugins, rules, recovery, updater
bentodesk-platform   D2D/DWrite/DComp/WIC/Shell/OLE boundaries
bentodesk-zone       Zone domain model
bentodesk-style      Themes, typography, and visual tokens
```

## Building from source

You need:

- Windows 10/11 x64;
- Rust 1.85 or newer;
- Visual Studio 2022 Build Tools with MSVC and the Windows SDK.

```powershell
git clone https://github.com/ZRainbow1275/bentodesk.git
cd bentodesk

$env:CARGO_BUILD_JOBS = "1"
$env:CARGO_INCREMENTAL = "0"
cargo build --release --target x86_64-pc-windows-msvc -p bentodesk-shell --bin BentoDesk
```

Output:

```text
target\x86_64-pc-windows-msvc\release\BentoDesk.exe
```

Full local checks:

```powershell
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
cargo deny check
cargo audit
```

## Contributing

Code, tests, translations, themes, plugins, and documentation are welcome. Issues and pull requests may be written in Chinese or English. Start with [CONTRIBUTING.md](CONTRIBUTING.md).

Security reports should use GitHub's private form described in [SECURITY.md](SECURITY.md), not a public issue.

## Thanks

Thanks to [Tibo](https://x.com/thsottiaux) for the product inspiration, and to the Linux Do community for its discussion and testing.

BentoDesk is maintained by 方寒 ([@ZRainbow1275](https://github.com/ZRainbow1275)).

## License

BentoDesk is released under the [GNU AGPL-3.0-or-later](LICENSE). Use, modification, and distribution must follow the license terms.
