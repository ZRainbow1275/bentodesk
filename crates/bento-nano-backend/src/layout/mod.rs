//! T-097 — layout persistence + screen resolution + snapshot manager.
//!
//! Per master plan §11 Q5 ruling: this crate (`bento-nano-backend::layout`)
//! owns the **persistence state** for the BentoDesk layout — full 24-field
//! `BentoZone`, `LayoutData`, `DesktopSnapshot`, `Resolution`. The sibling
//! `bento-nano-zone` crate keeps the minimal *render state* and depends on
//! this module only for de-/serialisation shapes (it does not pull
//! layout-engine code).
//!
//! ## Modules
//!
//! - [`persistence`] — `BentoZone`, `BentoItem`, `LayoutData`, `ZoneUpdate`,
//!   plus the `RelativePosition` / `RelativeSize` / `GridPosition` /
//!   `ItemType` / `AutoGroupRule` value types.
//! - [`resolution`] — `Resolution`, `get_current_resolution`, `get_dpi_scale`,
//!   percentage <-> pixel conversion, zone-clamp on display change, and the
//!   `start_resolution_monitor` background poller (channel-based, no Tauri).
//! - [`snapshot`] — `DesktopSnapshot` + `SnapshotManager` for save/load/list.
//!
//! ## What changed vs 1.x
//!
//! - `tauri::AppHandle` dependencies dropped — every entry takes the data
//!   directory `&Path` and an event `Sender<…>` instead.
//! - `chrono` dropped — every `Utc::now().to_rfc3339()` routes through
//!   [`crate::time::now_rfc3339`] (Q1 ruling).
//! - `windows` 0.58 typed `GetSystemMetrics` / `GetDpiForSystem` calls
//!   replaced by `windows-sys` 0.59 raw bindings.
//! - `crate::storage` dependency dropped from the load/save path — `LayoutData`
//!   reads/writes via plain `std::fs::{read, write}`. Atomic-write + `.bak`
//!   recovery is the dispatcher's job (it composes `crate::storage` once T-090
//!   stabilises). The hot-path types stay decoupled from the persistence
//!   strategy.
//! - Hand-rolled error enum [`LayoutError`] (spec §8.1).

pub mod persistence;
pub mod resolution;
pub mod snapshot;

pub use persistence::{
    AutoGroupRule, BentoItem, BentoZone, GridPosition, GroupRuleType, ItemType, LayoutData,
    LayoutError, RelativePosition, RelativeSize, ZoneUpdate,
};
pub use resolution::{
    Resolution, ResolutionChangedPayload, clamp_zone, clamp_zones_to_screen,
    get_current_resolution, get_dpi_scale, pixels_to_relative_x, pixels_to_relative_y,
    relative_x_to_pixels, relative_y_to_pixels, shutdown_resolution_monitor,
    start_resolution_monitor,
};
pub use snapshot::{DesktopSnapshot, SnapshotManager};
