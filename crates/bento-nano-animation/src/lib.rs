#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! `bento-nano-animation` — animation primitives crate.
//!
//! Per the team-lead's §13 ruling (orphan-rule resolution), the `Lerp` trait
//! and its impls live in `bento-nano-style::lerp` (next to the concrete
//! types). `Easing` and `AnimatedValue` live in `bento-nano-tree::animation`
//! (next to the widget-tree state they tween). This crate is the future
//! home for curves, bezier, and spring (T-042 / T-043 / T-044) — types this
//! crate will own outright, so the orphan rule won't bite a second time.
//!
//! For now the crate exists only as a `prelude` — a single
//! `use bento_nano_animation::prelude::*;` brings in the trait + ergonomic
//! primitives without callers needing to know which leaf crate hosts what.
//!
//! Spec §15 layer rule: `animation → tree` (for `AnimatedValue`/`Easing`) and
//! `animation → style` (for `Lerp`). Acyclic — both are upward edges.

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod prelude {
    //! One-stop import for tween authoring.
    pub use bento_nano_style::Lerp;
    pub use bento_nano_tree::{AnimatedValue, Easing};
}
