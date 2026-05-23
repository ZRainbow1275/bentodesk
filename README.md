# BentoDesk Nano

Self-built minimal-RSS UI framework for BentoDesk 2.0. Single-process, D2D-only, zero WebView, zero retained-mode framework.

## Hard Constraints

- Single-process, single-binary (spec §2)
- ≤ 30 MB Private Bytes main process (spec §1)
- ≤ 6 MB release binary (spec §1)
- Zero async runtime — `GetMessageW` + `crossbeam-channel` only (spec §9)
- D2D + DComp + DWrite via `windows` 0.58; plain Win32 via `windows-sys` 0.59 (spec §3.1.1)
- `WS_EX_NOREDIRECTIONBITMAP` only — no `WS_EX_LAYERED` (spec §4.1)

## Workspace Layout

```
crates/
  bento-nano-platform/  — Win32 + D2D + DComp + DWrite (migrated from D2D spike)
  bento-nano-tree/      — widget tree + arena allocator
  bento-nano-layout/    — flexbox-lite layout engine
  bento-nano-style/     — style structs (no CSS parser)
  bento-nano-widget/    — Container, Text, Image, Button
  bento-nano-app/       — AppState, EventDispatcher, render orchestrator
  bento-nano-shell/     — binary crate; assembles everything
```

## Build

```bash
cargo check --workspace --target x86_64-pc-windows-msvc
cargo build --release
.\target\x86_64-pc-windows-msvc\release\bento-nano-shell.exe
```

## Reference

- `D:/Desktop/CREATOR FOUR/.trellis/spec/guides/architecture-principles.md` — Supreme Law
- `D:/Desktop/CREATOR FOUR/bentodesk-d2d-spike/` — measurement target & reference implementation
