<div align="center">
  <img src="crates/bentodesk-app/assets/app-icon.png" width="96" alt="BentoDesk icon">

  # BentoDesk

  **A next-generation bento-box organizer for the Windows desktop—powered by Rust, built for quiet elegance.**

  [English](README.md) · [简体中文](README.zh-CN.md)

  <p><a href="https://github.com/ZRainbow1275/bentodesk/releases/latest"><img src="https://img.shields.io/github/v/release/ZRainbow1275/bentodesk?style=flat-square&amp;label=release" alt="Latest release"></a> <a href="https://github.com/ZRainbow1275/bentodesk/releases"><img src="https://img.shields.io/github/downloads/ZRainbow1275/bentodesk/total?style=flat-square&amp;label=downloads" alt="Total downloads"></a> <a href="https://github.com/ZRainbow1275/bentodesk/stargazers"><img src="https://img.shields.io/github/stars/ZRainbow1275/bentodesk?style=flat-square&amp;label=stars" alt="GitHub stars"></a> <a href="https://github.com/ZRainbow1275/bentodesk/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/ZRainbow1275/bentodesk/ci.yml?branch=main&amp;style=flat-square&amp;label=build" alt="CI"></a> <a href="#requirements"><img src="https://img.shields.io/badge/Windows-10%20%2F%2011-0078D4?style=flat-square&amp;logo=windows11&amp;logoColor=white" alt="Windows 10 and 11"></a> <a href="Cargo.toml"><img src="https://img.shields.io/badge/Rust-2024-000000?style=flat-square&amp;logo=rust&amp;logoColor=white" alt="Rust 2024"></a> <a href="LICENSE"><img src="https://img.shields.io/badge/License-AGPL--3.0-22c55e?style=flat-square" alt="AGPL-3.0 license"></a></p>

  <p>
    <a href="https://github.com/ZRainbow1275/bentodesk/releases/latest"><strong>Download</strong></a> ·
    <a href="docs/media/desktop-tour.mp4">Watch the full tour</a> ·
    <a href="#the-motion-is-the-interface">See how it works</a> ·
    <a href="#building-from-source">Build from source</a> ·
    <a href="#contributing">Contributing</a>
  </p>
</div>

<p align="center">
  <a href="docs/media/desktop-tour.mp4"><img src="docs/media/desktop-tour.webp" width="560" alt="A real BentoDesk desktop tour showing a Zone opening, a Stack blooming, and the desktop returning to rest"></a>
</p>
<p align="center">
  <sub>A Zone opens. A Stack blooms. The desktop goes quiet again. · <a href="docs/media/desktop-tour.mp4">play the 33-second MP4</a></sub>
</p>

## Files in reach. Noise out of sight.

I work from the desktop. The files are useful; the pile is not.

BentoDesk folds ordinary Windows files into small **Zones**. A Zone rests as a
capsule, opens into a grid where it already sits, then gets out of the way
again. Related Zones can become a **Stack** without losing their own layout or
style. Nothing is imported into a proprietary library: the files remain normal
Windows files.

<table width="100%">
  <tr>
    <td width="33%" align="center"><strong>Rest</strong><br><sub>A compact capsule leaves the desktop readable.</sub></td>
    <td width="33%" align="center"><strong>Open</strong><br><sub>Search and work with items in place.</sub></td>
    <td width="33%" align="center"><strong>Fold</strong><br><sub>Leave the Zone and the space returns.</sub></td>
  </tr>
</table>

Version 2.0 is the native Rust rewrite of BentoDesk. It carries forward the
useful ideas from the Tauri-based 1.x releases, removes the web runtime, and
rebuilds motion, typography, file handling, settings, and system integration
around Windows itself.

## The motion is the interface

The capsule and the grid are the same native surface. Geometry, hit testing,
text, icons, and controls follow one animation state, so a reversal continues
from the current frame instead of cutting to a second layer.

<p align="center">
  <a href="docs/media/zone-motion.mp4"><img src="docs/media/zone-motion.webp" width="500" alt="A real BentoDesk Zone expanding, opening search, and showing its native context menu"></a>
</p>
<p align="center">
  <sub>Capsule → grid → search → context menu · <a href="docs/media/zone-motion.mp4">play the MP4</a></sub>
</p>

A Zone can be narrow or wide, use five corner styles, and open on hover, click,
or remain expanded. Rapid movement between Zones does not discard an unfinished
animation.

## The rest of the desk

### 01 · Shape one Zone

Set its name, alias, icon, accent, grid columns, width, and corner style in a
native editor. No browser-backed settings window appears over the desktop.

<p align="center">
  <img src="docs/images/zone-editor.png" width="560" alt="Native BentoDesk Zone editor">
</p>

### 02 · Move the whole layout

Select, show, hide, move, or delete Zones together. Grid, horizontal, vertical,
ring, and natural layouts are clamped to the usable display area.

<p align="center">
  <img src="docs/images/bulk-manager.png" width="720" alt="Native BentoDesk batch Zone manager">
</p>

### 03 · Make it feel at home

Choose light or dark themes, accents, expansion behavior, performance controls,
startup options, backup, encryption, plugins, and updater settings. On first
launch, BentoDesk follows the Windows UI language; English and Simplified
Chinese can then be switched at any time.

<p align="center">
  <img src="docs/images/theme-settings.png" width="420" alt="BentoDesk native theme selector">
</p>

## Small where it matters

<div align="center">
<table>
  <tr>
    <td width="25%" align="center"><strong>2.39 MiB</strong><br><sub>release executable</sub></td>
    <td width="25%" align="center"><strong>17.34 MiB</strong><br><sub>Private Bytes at t60</sub></td>
    <td width="25%" align="center"><strong>235 ms</strong><br><sub>full Zone collapse</sub></td>
    <td width="25%" align="center"><strong>1 process</strong><br><sub>no browser runtime</sub></td>
  </tr>
</table>
</div>

Those numbers come from one isolated five-Zone reference run, not a synthetic
claim. See [Reference measurement](#reference-measurement) for the exact scene
and limits.

## What BentoDesk handles

### Everyday flow

- Move or copy files, folders, and shortcuts with Windows Shell/OLE drag and
  drop; drag an item back out to restore it to the desktop.
- Filter one Zone in place or search across all Zones.
- Combine Zones into a Stack, then bloom its members without flattening their
  individual layout.
- Keep empty desktop space click-through while ordinary application windows
  cover BentoDesk as expected.

### Automation with a review step

- **Smart grouping** turns current desktop files into selectable suggestions;
  nothing moves until a suggestion is applied.
- **Live folders** keep a bound directory and its Zone in sync.
- **Plugins** install from validated local packages and can be enabled,
  disabled, persisted, and removed with confirmation.
- **Rules** handle repeatable local organization without an online service.

### Recovery and file safety

Save and load layout snapshots, inspect structural changes on the timeline, and
restore an earlier arrangement. Settings backups, atomic persistence,
update-package validation, and DPAPI or passphrase encryption provide recovery
paths without a helper browser process.

Drag and drop follows Windows move/copy semantics. Failed or rejected transfers
are not recorded as completed. Snapshot deletion and bulk Zone deletion require
confirmation. Irreplaceable files should still have a backup, just as they
should with any desktop file tool.

## Choosing a desktop organizer

These tools solve different versions of “my desktop is full.” The comparison
uses each product's official description and focuses on workflow rather than a
synthetic benchmark.

| Start with | When it is the better fit |
| --- | --- |
| [Windows folders and desktop icons](https://support.microsoft.com/en-us/windows/experience/personalization/customize-the-desktop-icons-in-windows) | You want no extra software. Windows keeps the familiar icons, shortcuts, and Explorer folders. |
| [Stardock Fences 6](https://www.stardock.com/products/fences/) | You need extensive automation or managed-PC deployment, including sorting rules, tabs, Peek, Folder Portals, and business controls. |
| [Portals](https://portals-app.com/) | You want persistent folder panels, tabs, and precise per-panel styling or display-aware profiles. |
| [Nimi Places](https://mynimi.net/Projects/Nimi-Places/Features/) | Rich previews, labels, sorting rules, and media-oriented containers matter most. |
| **BentoDesk** | You want a local-first organizer whose resting state stays small: native capsule-to-grid motion, Stacks, direct desktop-item handling, layouts, snapshots, and timeline recovery. |

BentoDesk is intentionally focused. It is Windows-only, does not replace
Explorer, and does not provide cloud-account sync. A permanent folder portal,
rich media browser, or managed deployment is better served by a product built
around that job.

## Quick start

1. Download `BentoDesk-2.0.2-windows-x64-portable.zip` from
   [Releases](https://github.com/ZRainbow1275/bentodesk/releases/latest).
2. Check the archive against `SHA256SUMS.txt`, extract it to a writable
   directory, and run `BentoDesk.exe`.
3. Create or manage Zones from the tray menu.
4. Drag files, folders, or shortcuts into a Zone.
5. Choose a theme, expansion mode, and language in Settings.

The portable package needs no Node.js, Tauri, WebView2, or separate browser
runtime. State lives in the current user profile by default; portable mode can
keep it beside the executable.

### Requirements

- Windows 10 1809+ or Windows 11;
- x86-64 processor;
- current Windows updates and graphics drivers are recommended.

## Reference measurement

One isolated run of the public BentoDesk 2.0.2 Windows x64 release at
2560×1368 / 144 DPI, with five Zones, 50 items, and one BentoDesk process:

| Metric | Result |
| --- | ---: |
| Release EXE | 2,506,752 bytes (2.39 MiB) |
| Private Bytes at t10 / t30 / t60 | 17.80 / 17.38 / 17.34 MiB |
| Full Zone expand / collapse | 234 ms / 235 ms |
| Animation tick median / p95 | 16 ms / 16 ms |

## Technology

| Layer | Implementation |
| --- | --- |
| Language and runtime | Rust 2024, single process |
| Windows and input | Win32 / USER32 / DWM |
| Graphics | Direct2D, DirectWrite, DirectComposition, D3D11 |
| Icons and images | Windows Imaging Component, Windows Shell |
| File interaction | Shell/OLE, `ReadDirectoryChangesW` |
| Network and system security | WinHTTP, DPAPI |
| Data | Atomic local persistence, encrypted settings vault |
| Build | MSVC x64, static CRT, size optimization, Fat LTO |

> [!NOTE]
> BentoDesk 2.0 does not ship Tauri, WebView2, Chromium, Node.js, or a
> third-party GUI framework in its runtime. The Tauri 1.x code remains at
> [`v1.3.0`](https://github.com/ZRainbow1275/bentodesk/tree/v1.3.0) and
> [`archive/tauri-1.x`](https://github.com/ZRainbow1275/bentodesk/tree/archive/tauri-1.x).

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

You need Windows 10/11 x64, Rust 1.89 or newer, and Visual Studio 2022 Build
Tools with MSVC and the Windows SDK.

```powershell
git clone https://github.com/ZRainbow1275/bentodesk.git
cd bentodesk

$env:CARGO_BUILD_JOBS = "1"
$env:CARGO_INCREMENTAL = "0"
cargo build --locked --release --target x86_64-pc-windows-msvc -p bentodesk-shell --bin BentoDesk
```

Output:

```text
target\x86_64-pc-windows-msvc\release\BentoDesk.exe
```

Full local checks:

```powershell
cargo fmt --all -- --check
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo doc --locked --workspace --no-deps
cargo deny check
cargo audit
```

## Contributing

Code, tests, translations, themes, plugins, documentation, and focused bug
reports are welcome. Issues and pull requests may be written in English or
Chinese. Start with [CONTRIBUTING.md](CONTRIBUTING.md).

Report security issues through GitHub's private channel described in
[SECURITY.md](SECURITY.md), not in a public issue.

## Thanks

Thanks to GPT 5.6 SOL for the help, to
[Tibo](https://x.com/thsottiaux) for the frequent resets that sped up
BentoDesk's release and polish, and to the
[Linux Do](https://linux.do/) community for its discussion, testing, and
candid feedback.

<p align="center">
  <img src="docs/media/tibo-reset.webp" width="160" alt="Tibo reset meme">
</p>

BentoDesk is maintained by 方寒
([@ZRainbow1275](https://github.com/ZRainbow1275)).

## License

BentoDesk is released under the [GNU AGPL-3.0-or-later](LICENSE).
