//! # Signal — reactive data primitive.
//!
//! `Signal<T>` only owns data storage + dirty propagation. **Side effects**
//! (IO triggers, logging, kicking off animations) belong to a separate
//! `Effect` primitive — see Phase 1.1.1 `effect.rs` (planned).
//!
//! This split mirrors the signal/effect separation that Solid.js, Svelte 5
//! Runes, and Leptos have all converged on:
//! - `Signal` owns: `value`, `dirty` flag, `deps` list.
//! - `Effect` owns: subscription set, run callback.
//! - The two collaborate via dependency tracking inside `flush_dirty`.
//!
//! **Do not add an `on_change` field to `Signal`.** That would force either
//! `Box<dyn Fn>` (banned by spec §10) or a non-capturing fn pointer (too
//! restrictive for the realistic call sites). Wrong abstraction. If you need
//! callback semantics, build it on top of `Effect`.
//!
//! Spec refs: §C1 (macro_rules! only — no `proc-macro2`/`syn`/`quote`),
//! §10 (`SmallVec<[NodeId; 4]>` for inline deps; no `Vec::new` in hot path),
//! §11 (no `Box<dyn Fn>` callbacks).

use core::cell::Cell;

use smallvec::SmallVec;

use crate::NodeId;

/// Reactive cell. Mutating `set` flips `dirty` only when the new value differs
/// from the current one; subscribers are stored inline (≤ 4 deps allocate
/// nothing on the heap, matching the typical widget-tree fan-out).
#[derive(Debug)]
pub struct Signal<T> {
    value: T,
    dirty: Cell<bool>,
    deps: SmallVec<[NodeId; 4]>,
}

impl<T: PartialEq + Clone> Signal<T> {
    pub fn new(initial: T) -> Self {
        Self {
            value: initial,
            dirty: Cell::new(false),
            deps: SmallVec::new(),
        }
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    /// Set the cell's value. Equal-value writes are dropped silently to keep
    /// the dirty propagation cheap (spec §17 — no slow paths). Returns `true`
    /// when the write actually changed the value.
    pub fn set(&mut self, next: T) -> bool {
        if self.value == next {
            return false;
        }
        self.value = next;
        self.dirty.set(true);
        true
    }

    /// Add `dep` as a subscriber. Idempotent — re-subscribing is a no-op so
    /// the inline buffer can't leak duplicate ids.
    pub fn subscribe(&mut self, dep: NodeId) {
        if !self.deps.contains(&dep) {
            self.deps.push(dep);
        }
    }

    pub fn unsubscribe(&mut self, dep: NodeId) {
        self.deps.retain(|d| *d != dep);
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.get()
    }

    pub fn clear_dirty(&mut self) {
        self.dirty.set(false);
    }

    /// Subscribers in insertion order. `Tree::flush_dirty` consumes this slice
    /// to mark dependent layout nodes for re-evaluation.
    pub fn deps(&self) -> &[NodeId] {
        &self.deps
    }
}

/// `signal!` — declarative shorthand for `Signal::new`.
///
/// Single form: `signal!(name: T = init);` expands to a `let mut` binding of
/// type `Signal<T>`. Callback semantics intentionally live in `Effect`, not
/// here — see the module docs above.
#[macro_export]
macro_rules! signal {
    ($name:ident : $ty:ty = $init:expr) => {
        let mut $name: $crate::signal::Signal<$ty> = $crate::signal::Signal::new($init);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_set_skips_equal_value() {
        let mut s: Signal<u32> = Signal::new(7);
        // First write differs → dirty flips.
        assert!(s.set(8));
        assert!(s.is_dirty());
        s.clear_dirty();
        // Equal write must be a no-op.
        assert!(!s.set(8));
        assert!(!s.is_dirty());
    }

    #[test]
    fn signal_subscribe_is_idempotent() {
        let mut s: Signal<i32> = Signal::new(0);
        let a = NodeId::ROOT_INVALID;
        s.subscribe(a);
        s.subscribe(a);
        assert_eq!(s.deps().len(), 1);
        s.unsubscribe(a);
        assert_eq!(s.deps().len(), 0);
    }

    #[test]
    fn signal_macro_declares_mutable_binding() {
        crate::signal!(count: u32 = 0);
        assert_eq!(*count.get(), 0);
        assert!(count.set(5));
        assert_eq!(*count.get(), 5);
    }
}
