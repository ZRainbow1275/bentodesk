//! Business UI surfaces — Phase 4 port of the 1.x React `src/components/`
//! cohort.
//!
//! Sub-modules group by visual surface. Master plan §11 ownership lines:
//! - **business-ui-1** — zone / item / icons (T-049..T-056 + T-079) and the
//!   toolbar + tray_menu surfaces shipped during the Wave-E settings
//!   bring-up.
//! - **business-ui-2** — settings family (T-057..T-062) and the timeline /
//!   snapshot picker pair (T-070).
//! - **business-ui-3** — picker / wizard / context-menu / drag-ghost /
//!   shared cohort (T-063..T-079 minus T-070), including capsule_picker /
//!   minibar / popover / tooltip / dialog primitives that live alongside
//!   the icon_picker entry shipped during the Wave-E cross-domain pass.
//!
//! Every component carries a `*.snap.md` text spec next to its source per
//! §11 R3 ruling — corner radius, palette tokens, typography, animation
//! timing, hover state. The spec is the contract; the Rust composition
//! materialises it once the upstream widget primitives ship.
//!
//! Spec compliance:
//! - §11 no panic: zero `unwrap()` / `expect()` / `panic!()` — already
//!   enforced by the workspace clippy lint config.
//! - §10 hot path: the `format_size` / `format_bytes` helpers in
//!   `settings::backup_card` and `settings::updater_card` use `SmolStr`
//!   so short outputs (the common case) stay inline.

pub mod about;
pub mod auto_layout_menu;
pub mod bulk_manager_panel;
pub mod capsule_picker;
pub mod debug_overlay;
pub mod dialog;
pub mod highlight_overlay;
pub mod icon_picker;
pub mod icons;
pub mod item_card;
pub mod item_grid;
pub mod item_icon;
pub mod minibar;
pub mod palette_picker;
pub mod popover;
pub mod rules_wizard;
pub mod search_bar;
pub mod settings;
pub mod smart_group_suggestor;
pub mod stack_tray;
pub mod timeline;
pub mod tooltip;
pub mod tray_menu;
pub mod zen_capsule;
pub mod zone_editor;
