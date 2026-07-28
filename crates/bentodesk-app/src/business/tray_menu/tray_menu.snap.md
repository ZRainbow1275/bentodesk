# TrayMenu — Visual Spec

Source: `bentodesk/src-tauri/src/tray/menu.rs` (195 LOC, OS-native menu).

NOTE: 1.x uses Tauri's OS-native `Menu`/`MenuItem` (`HMENU` via Win32 `TrackPopupMenu`); native's single-process invariant means we render this menu in our own DComp visual tree as a `WindowKind::ContextMenu` HWND popup positioned near the tray icon. The TrayIcon registration (`Shell_NotifyIconW`) lives in `bentodesk-shell`; this snap.md describes the popup surface only.

- **Popup window:** `WindowKind::ContextMenu` HWND, NoActivate + Topmost + NoRedirectionBitmap (per spec §4.1). Auto-sizes to content; default 200 × 6×menuItemHeight px.
- **Chrome:** 8 px corner radius, `palette.surface` bg with backdrop-blur, 1 px `palette.border`, 4 px corner shadow `palette.shadow.popup`.
- **Open animation:** scale 0.95 → 1.0 + opacity 0 → 1, 120 ms `EaseOut`. Anchor: tray-icon position with edge-flip (above tray if no room below).
- **Item:** 32 px tall, padding 8 px 16 px, 13 px Regular `palette.text`. Hover: `palette.hover_overlay` bg, 80 ms ease-out fade-in. Active/pressed: `palette.active_overlay` bg.
- **Items (top → bottom, matching 1.x order):** "显示/隐藏 BentoDesk" (text toggles based on window visibility), divider, "新建区域", "智能整理桌面", divider, "设置", "关于", divider, "退出".
- **Divider:** 1 px tall, `palette.border` color, 4 px vertical margin, full-width minus 8 px padding.
- **Dismiss:** click outside (any other window activation), Escape key, or item click. Click-to-dismiss must NOT propagate the click to the underlying window.
- **i18n:** locale-aware text (Chinese in 1.x; native keeps both via `bentodesk-style::i18n` table).
