#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! Library face of `bento-nano-shell`.
//!
//! The shell exists primarily as a binary (`src/main.rs`), but exposing the
//! UI-tree builders via a tiny lib lets integration tests under `tests/`
//! exercise `mount_main_tree` without booting a real Win32 window.

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod hotkey;
pub mod ui;
