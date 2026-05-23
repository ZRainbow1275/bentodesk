//! Standalone widget helpers (Wave K1).
//!
//! Reusable geometry + draw primitives that are too small to deserve their
//! own business module but are too large to inline at every call site.
//! Each helper is `Copy` and allocation-free per spec §10.

pub mod toggle_switch;

pub use toggle_switch::{ToggleSwitch, toggle_switch_layout};
