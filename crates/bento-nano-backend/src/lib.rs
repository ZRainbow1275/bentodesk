#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! `bento-nano-backend` — lift-verbatim Win32 backend modules ported from
//! `bentodesk/src-tauri/src/` (BentoDesk 1.x).
//!
//! These modules touch Win32 directly and have minimal coupling to the Tauri
//! runtime. Per the master plan §1 they live in **layer 2.5** between
//! `bento-nano-widget` and `bento-nano-app` so that app-layer code can call
//! `backend::*` as direct function calls — no IPC veneer (single-process
//! invariant, spec §2).
//!
//! ## Porting rules applied
//!
//! - **Spec §9** — every `tokio::*` and `tauri::async_runtime::spawn` is
//!   replaced by `std::thread::spawn` + `crossbeam_channel::{Sender,Receiver}`.
//!   Zero async runtime allowed in this crate or its deps.
//! - **Spec §11** — every `unwrap()` / `expect()` from the 1.x source is
//!   converted to `Result<T, ModuleError>` with hand-rolled error enums
//!   (no `thiserror`, per spec §8.1).
//! - **Spec §11.1** — every `unsafe` block carries a `// SAFETY:` comment.
//! - **Master §11 ΔB ruling** — every public command-input/output struct
//!   carries `#[derive(serde::Serialize, serde::Deserialize)]` even though
//!   the single-process build never serializes at runtime, so v2.x scripting
//!   hooks can re-introduce serialization without breaking compatibility.
//! - **Spec §17** — no `todo!()` / `unimplemented!()` / "TODO later"
//!   placeholders. Every module ships complete or not at all.
//!
//! ## Modules
//!
//! - [`watcher`]     — desktop file-system monitor (T-082).
//! - [`drag_drop`]   — Win32 OLE drag-and-drop COM objects (T-083).
//! - [`ghost_layer`] — desktop overlay window subclass + z-order keeper (T-084).
//! - [`themes`]      — JSON theme loader bridging to `bento_nano_theme` (T-085).
//! - [`icon_positions`] — desktop icon position save/restore via COM
//!   `IFolderView` (T-086).
//! - [`layout`] — BentoZone persistence + screen-resolution monitor +
//!   snapshot manager (T-097).
//! - [`storage`] — atomic JSON write + backup recovery via Win32
//!   `ReplaceFileW` / `MoveFileExW` (T-090).
//! - [`worker_pool`] — fixed-size `std::thread` pool for bounded
//!   concurrency on Win32 shell-cache calls (T-100).
//! - [`stealth`] — hidden-items subsystem split into `hide`/`restore`/`sync`
//!   (T-094a/b/c, master plan §11 R8 — 1.x `hidden_items.rs` 2,797 LOC
//!   re-organised under spec §15 800-LOC ceiling).
//! - [`icon`] — Win32 shell icon subsystem (T-080/T-081 — 1.x
//!   `icon/{cache,extractor,protocol,stats,svg_sanitize,cache_tier,custom_icons}.rs`
//!   ported with hand-rolled LRU + WIC for PNG + state-machine SVG
//!   sanitiser, all replacing `lru`/`image`/`regex`/`uuid`/`chrono` per
//!   spec §8 forbidden-deps list).
//!
//! ## Tauri-bridge replacements
//!
//! Where 1.x called `handle.emit("...", payload)` to push events into the
//! webview, the nano port exposes a `crossbeam_channel::Sender<ModuleEvent>`
//! parameter on the relevant constructor / setup function. The caller
//! (typically `bento-nano-app::dispatcher`) wires the receiver into its own
//! command bus.

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod autostart;
pub mod config_vault;
pub mod desktop_sources;
// Mc-1a — soft-loaded system-DPI query (GetProcAddress) so backend carries no
// static `GetDpiForSystem` import. Self-contained: NO platform dep (spec §8).
pub(crate) mod dpi_compat;
pub mod drag_drop;
pub mod ghost_layer;
pub mod grouping;
pub mod guardrails;
pub mod icon;
pub mod icon_positions;
pub mod layout;
pub mod minibar;
pub mod plugins;
pub mod power;
pub mod recovery_bundle;
pub mod rules;
pub mod search;
pub mod stealth;
pub mod storage;
pub mod system;
pub mod themes;
pub mod time;
pub mod timeline;
pub mod updater;
pub mod watcher;
pub mod worker_pool;
