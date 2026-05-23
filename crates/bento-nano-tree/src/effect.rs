//! # Effect — reactive side-effect primitive (companion to [`Signal`](crate::Signal)).
//!
//! Replaces the planned Phase 1.1.1 Effect from `tree::signal::Signal` doc —
//! landed in T-006 / Wave A.
//!
//! `Effect<F>` owns a callback that runs on the next paint frame whenever any
//! tracked [`Signal`](crate::signal::Signal) is observed dirty. The split
//! mirrors the signal/effect separation that Solid.js, Svelte 5 Runes, and
//! Leptos converge on:
//! - [`Signal`](crate::signal::Signal) owns: `value`, `dirty` flag, `deps`.
//! - `Effect` owns: callback, list of affected `NodeId`s, scheduled-run flag.
//! - Frame loop calls [`EffectArena::flush`] once per frame, which runs every
//!   scheduled effect and marks its affected nodes [`DirtyKind::Layout`](crate::DirtyKind::Layout).
//!
//! ## Why no auto-tracking
//!
//! Solid-style auto-tracking (Effect re-runs because the `f` closure *read* a
//! Signal) requires a thread-local "current observer" pointer set during `f`
//! and a Signal `get` that pushes the current observer into the Signal's
//! subscriber list. That contract assumes Signals are heap-allocated and
//! addressable by ID.
//!
//! `Signal<T>` here is an **owned struct** (no arena, no ID — see
//! `signal.rs`) chosen for spec §10 (no `Box<dyn>`, no heap traffic per
//! widget). Auto-tracking would force either:
//! - an arena-allocated `Signal` (gives up the §10 win), or
//! - `Box<dyn Fn>` callbacks for the subscriber (banned by spec §11),
//! - or a thread-local `RefCell<Vec<*mut Signal>>` (raw pointers + interior
//!   mutability across the whole framework — wrong abstraction).
//!
//! Instead, Effects use **explicit scheduling**: callers invoke
//! [`EffectArena::schedule`] when they observe a Signal turn dirty. The
//! per-frame loop already drains `Signal::is_dirty()` to update the tree;
//! the same loop calls `schedule` on the matching effect. This keeps Effect
//! scope to ~200 LOC and zero new dependencies.
//!
//! Spec refs: §C1 (`macro_rules!` only — no `proc-macro2`/`syn`/`quote`),
//! §10 (`SmallVec<[NodeId; 4]>` for inline tracked-node list; arena reuses
//! freed slots so steady-state has no heap), §11 (no `Box<dyn Fn>` callbacks
//! at runtime — generic `F: Fn()` per Effect, type-erased only inside the
//! arena via a trait object box that allocates **once at create time**, not
//! per frame).

use core::fmt;

use smallvec::SmallVec;

use crate::NodeId;
use crate::dirty::DirtyDigest;

/// Stable handle into [`EffectArena`]. Like [`NodeId`], wraps a slot index
/// plus a generation counter so a freed-and-recycled slot does not silently
/// alias to a stale handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EffectId {
    index: u32,
    generation: u32,
}

impl EffectId {
    /// Sentinel for "no effect". Useful as a default in widget structs that
    /// optionally own an effect.
    pub const INVALID: EffectId = EffectId {
        index: u32::MAX,
        generation: 0,
    };

    pub fn is_invalid(&self) -> bool {
        self.index == u32::MAX
    }
}

/// Arena lookup error. Hand-rolled (spec §11) — no `thiserror`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectError {
    OutOfBounds,
    StaleHandle,
    EmptySlot,
}

/// Sealed type-erased run trait. Lets the arena store heterogeneous closure
/// types in a single `Vec` while keeping `Effect<F>` itself monomorphic.
trait EffectRun {
    fn run(&self);
}

/// Reactive effect. Holds a callback `f` plus the inline list of `NodeId`s it
/// invalidates. The callback runs at most once per frame (idempotent
/// scheduling: re-scheduling an already-scheduled effect is a no-op).
///
/// `F` is generic so the per-effect code is monomorphised — no `Box<dyn Fn>`
/// indirection at the call site. The arena erases the type only when storing
/// the effect; the box allocation happens **once at create time**, not on
/// the hot per-frame run path.
pub struct Effect<F: Fn() + 'static> {
    f: F,
    /// Tree nodes this effect's body affects. When the effect runs, every
    /// id in this list is pushed into the per-frame [`DirtyDigest`] so the
    /// renderer re-layouts only the touched subtree. Inline 4-slot
    /// `SmallVec` matches the typical "effect updates one widget" case
    /// without heap traffic (spec §10).
    tracked_signals: SmallVec<[NodeId; 4]>,
}

impl<F: Fn() + 'static> Effect<F> {
    /// Create a fresh Effect that will run `f` when scheduled. The affected
    /// node list starts empty — call [`Effect::add_tracked_node`] to declare
    /// which tree nodes this effect invalidates.
    pub fn new(f: F) -> Self {
        Self {
            f,
            tracked_signals: SmallVec::new(),
        }
    }

    /// Attach `node` to the affected-list. Idempotent — re-adding is a no-op
    /// so the inline buffer cannot grow duplicates.
    pub fn add_tracked_node(&mut self, node: NodeId) {
        if !self.tracked_signals.contains(&node) {
            self.tracked_signals.push(node);
        }
    }

    /// Invoke the callback unconditionally. Public for tests + advanced
    /// callers; production sites should go through [`EffectArena::flush`]
    /// so the once-per-frame contract holds.
    pub fn run(&self) {
        (self.f)()
    }

    /// Affected node list, in insertion order. Used by [`EffectArena::flush`]
    /// when populating the per-frame dirty digest.
    pub fn tracked_nodes(&self) -> &[NodeId] {
        &self.tracked_signals
    }

    /// Drop the effect explicitly. Equivalent to letting it go out of scope;
    /// kept as an explicit method to mirror Solid.js / Leptos `dispose()`
    /// ergonomics for callers who track effects through `EffectId`.
    pub fn dispose(self) {
        // Drop runs the F destructor. Nothing else to clean up.
    }
}

impl<F: Fn() + 'static> fmt::Debug for Effect<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Effect")
            .field("tracked_nodes", &self.tracked_signals.len())
            .finish_non_exhaustive()
    }
}

impl<F: Fn() + 'static> EffectRun for Effect<F> {
    fn run(&self) {
        Effect::run(self)
    }
}

/// Arena slot. Mirrors the `Tree<T>::Slot` shape (generation-counted,
/// optional payload, free-list reuse) so the lookup discipline matches the
/// rest of the crate.
struct Slot {
    generation: u32,
    payload: Option<EffectEntry>,
}

struct EffectEntry {
    /// Type-erased callable. The `Box` allocation happens once at
    /// [`EffectArena::create`] time and survives until [`EffectArena::dispose`].
    runner: Box<dyn EffectRun>,
    /// Mirrored from [`Effect::tracked_signals`] so the arena can populate
    /// the dirty digest without re-borrowing the boxed runner. Kept inline
    /// (4 slots) to match the per-effect typical fan-out (spec §10).
    tracked_nodes: SmallVec<[NodeId; 4]>,
    /// Pending re-run flag. Set by [`EffectArena::schedule`], cleared by
    /// [`EffectArena::flush`]. Keeps scheduling idempotent — multiple
    /// `schedule` calls between flushes coalesce to one run.
    scheduled: bool,
}

/// Per-tree storage of effects. Lives next to a `Tree<T>` (typically owned
/// by the same `AppState`/`WindowState`) so flush can fold the effect's
/// tracked nodes into the tree's dirty digest in a single call.
///
/// Capacity grows on first use; freed slots are recycled via `free_list` so
/// long-running apps reach a steady state with no further heap activity.
#[derive(Default)]
pub struct EffectArena {
    slots: Vec<Slot>,
    free_list: SmallVec<[u32; 16]>,
}

impl fmt::Debug for EffectArena {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EffectArena")
            .field("slots", &self.slots.len())
            .field("free", &self.free_list.len())
            .finish()
    }
}

impl EffectArena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            slots: Vec::with_capacity(cap),
            free_list: SmallVec::new(),
        }
    }

    /// Number of live effects.
    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.payload.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Insert an [`Effect`] and return its handle. The arena consumes the
    /// effect's callback + tracked-node list; subsequent reads / mutations
    /// go through the returned [`EffectId`].
    pub fn create<F: Fn() + 'static>(&mut self, eff: Effect<F>) -> EffectId {
        let entry = EffectEntry {
            tracked_nodes: eff.tracked_signals.clone(),
            runner: Box::new(eff),
            scheduled: false,
        };
        if let Some(idx) = self.free_list.pop() {
            let slot = &mut self.slots[idx as usize];
            slot.payload = Some(entry);
            return EffectId {
                index: idx,
                generation: slot.generation,
            };
        }
        let idx = self.slots.len() as u32;
        self.slots.push(Slot {
            generation: 0,
            payload: Some(entry),
        });
        EffectId {
            index: idx,
            generation: 0,
        }
    }

    /// Drop an effect and recycle its slot. Returns `EffectError::StaleHandle`
    /// if the id refers to an already-freed slot.
    pub fn dispose(&mut self, id: EffectId) -> Result<(), EffectError> {
        let idx = id.index as usize;
        let slot = self.slots.get_mut(idx).ok_or(EffectError::OutOfBounds)?;
        if slot.generation != id.generation {
            return Err(EffectError::StaleHandle);
        }
        slot.payload.take().ok_or(EffectError::EmptySlot)?;
        slot.generation = slot.generation.wrapping_add(1);
        self.free_list.push(id.index);
        Ok(())
    }

    /// Append `node` to an existing effect's affected list. Useful when the
    /// caller wires up the tracked nodes after `create_effect` (e.g. nodes
    /// that didn't exist yet at create time).
    pub fn add_tracked_node(&mut self, id: EffectId, node: NodeId) -> Result<(), EffectError> {
        let entry = self.entry_mut(id)?;
        if !entry.tracked_nodes.contains(&node) {
            entry.tracked_nodes.push(node);
        }
        Ok(())
    }

    /// Mark an effect for re-run on the next [`EffectArena::flush`]. Multiple
    /// schedules between flushes coalesce — the effect runs at most once per
    /// frame (matches the [`Tree::flush_dirty`](crate::Tree::flush_dirty)
    /// once-per-frame contract in C2).
    pub fn schedule(&mut self, id: EffectId) -> Result<(), EffectError> {
        self.entry_mut(id)?.scheduled = true;
        Ok(())
    }

    /// Returns `true` if the effect has been scheduled and not yet flushed.
    /// Test-only convenience; production code shouldn't branch on this.
    pub fn is_scheduled(&self, id: EffectId) -> Result<bool, EffectError> {
        Ok(self.entry_ref(id)?.scheduled)
    }

    /// Run every scheduled effect, then push each effect's tracked nodes
    /// into `digest` so the renderer re-layouts the touched subtrees.
    /// Returns the number of effects that ran.
    ///
    /// Caller invokes this once per frame, immediately before
    /// [`Tree::flush_dirty`](crate::Tree::flush_dirty), so any nodes the
    /// effect bodies marked end up in the same digest.
    pub fn flush(&mut self, digest: &mut DirtyDigest) -> usize {
        let mut ran = 0usize;
        for slot in &mut self.slots {
            if let Some(entry) = slot.payload.as_mut()
                && entry.scheduled
            {
                entry.runner.run();
                for n in &entry.tracked_nodes {
                    if !digest.layout_invalidated.contains(n) {
                        digest.layout_invalidated.push(*n);
                    }
                }
                entry.scheduled = false;
                ran += 1;
            }
        }
        ran
    }

    fn entry_ref(&self, id: EffectId) -> Result<&EffectEntry, EffectError> {
        let slot = self
            .slots
            .get(id.index as usize)
            .ok_or(EffectError::OutOfBounds)?;
        if slot.generation != id.generation {
            return Err(EffectError::StaleHandle);
        }
        slot.payload.as_ref().ok_or(EffectError::EmptySlot)
    }

    fn entry_mut(&mut self, id: EffectId) -> Result<&mut EffectEntry, EffectError> {
        let slot = self
            .slots
            .get_mut(id.index as usize)
            .ok_or(EffectError::OutOfBounds)?;
        if slot.generation != id.generation {
            return Err(EffectError::StaleHandle);
        }
        slot.payload.as_mut().ok_or(EffectError::EmptySlot)
    }
}

/// Builder context passed to [`create_effect`]. Wraps a mutable borrow of
/// the arena so caller code stays readable. Kept as an alias rather than a
/// wrapper struct so the arena's full API remains accessible.
pub type TreeCtx<'a> = EffectArena;

/// Sugar for `arena.create(Effect::new(f))`. Mirrors the Solid.js
/// `createEffect(f)` ergonomic without dragging in a builder API.
pub fn create_effect<F: Fn() + 'static>(ctx: &mut TreeCtx, f: F) -> EffectId {
    ctx.create(Effect::new(f))
}

/// `effect!` — declarative shorthand for [`create_effect`].
///
/// `effect!(arena, body);` expands to `create_effect(&mut arena, move || body)`.
/// `macro_rules!` only — no `proc-macro2` / `syn` / `quote` (spec §C1, §8.1).
#[macro_export]
macro_rules! effect {
    ($arena:expr, $body:expr) => {
        $crate::effect::create_effect(&mut $arena, move || $body)
    };
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;
    use std::rc::Rc;

    use super::*;

    fn dummy_node(idx: u32) -> NodeId {
        // Build a NodeId via the public surface — the arena doesn't care
        // whether the node actually exists in a Tree, only that the id is
        // stable.
        NodeId {
            index: idx,
            generation: 0,
        }
    }

    #[test]
    fn create_run_dispose_roundtrip() {
        let mut arena = EffectArena::new();
        let counter = Rc::new(Cell::new(0u32));
        let c2 = counter.clone();
        let id = arena.create(Effect::new(move || {
            c2.set(c2.get() + 1);
        }));
        arena.schedule(id).unwrap_or_default();
        let mut digest = DirtyDigest::default();
        let n = arena.flush(&mut digest);
        assert_eq!(n, 1);
        assert_eq!(counter.get(), 1);
        // Second flush with no schedule must be a no-op.
        let n2 = arena.flush(&mut digest);
        assert_eq!(n2, 0);
        assert_eq!(counter.get(), 1);
        assert!(arena.dispose(id).is_ok());
    }

    #[test]
    fn schedule_is_idempotent_within_frame() {
        let mut arena = EffectArena::new();
        let counter = Rc::new(Cell::new(0u32));
        let c2 = counter.clone();
        let id = arena.create(Effect::new(move || {
            c2.set(c2.get() + 1);
        }));
        // Five schedules between flushes coalesce to one run.
        for _ in 0..5 {
            arena.schedule(id).unwrap_or_default();
        }
        let mut digest = DirtyDigest::default();
        arena.flush(&mut digest);
        assert_eq!(counter.get(), 1, "schedule must coalesce per frame");
    }

    #[test]
    fn flush_marks_tracked_nodes_dirty() {
        let mut arena = EffectArena::new();
        let mut eff = Effect::new(|| {});
        let n1 = dummy_node(7);
        let n2 = dummy_node(8);
        eff.add_tracked_node(n1);
        eff.add_tracked_node(n2);
        // Idempotent add — second call must not duplicate.
        eff.add_tracked_node(n1);
        assert_eq!(eff.tracked_nodes().len(), 2);
        let id = arena.create(eff);
        arena.schedule(id).unwrap_or_default();
        let mut digest = DirtyDigest::default();
        arena.flush(&mut digest);
        assert_eq!(digest.layout_invalidated.as_slice(), &[n1, n2]);
    }

    #[test]
    fn dispose_then_stale_handle_errors() {
        let mut arena = EffectArena::new();
        let id = arena.create(Effect::new(|| {}));
        assert!(arena.dispose(id).is_ok());
        assert_eq!(arena.dispose(id), Err(EffectError::StaleHandle));
        assert_eq!(arena.schedule(id), Err(EffectError::StaleHandle));
    }

    #[test]
    fn freelist_recycles_slot_with_bumped_generation() {
        let mut arena = EffectArena::new();
        let id1 = arena.create(Effect::new(|| {}));
        assert!(arena.dispose(id1).is_ok());
        let id2 = arena.create(Effect::new(|| {}));
        assert_eq!(id1.index, id2.index, "slot must be reused");
        assert_ne!(
            id1.generation, id2.generation,
            "generation must bump after dispose"
        );
    }

    #[test]
    fn create_effect_helper_runs_body() {
        let mut arena = EffectArena::new();
        let counter = Rc::new(Cell::new(0u32));
        let c2 = counter.clone();
        let id = create_effect(&mut arena, move || {
            c2.set(42);
        });
        arena.schedule(id).unwrap_or_default();
        let mut digest = DirtyDigest::default();
        arena.flush(&mut digest);
        assert_eq!(counter.get(), 42);
    }

    #[test]
    fn add_tracked_node_via_arena_after_create() {
        let mut arena = EffectArena::new();
        let id = arena.create(Effect::new(|| {}));
        let n = dummy_node(3);
        arena.add_tracked_node(id, n).unwrap_or_default();
        // Idempotent — second add must not duplicate.
        arena.add_tracked_node(id, n).unwrap_or_default();
        arena.schedule(id).unwrap_or_default();
        let mut digest = DirtyDigest::default();
        arena.flush(&mut digest);
        assert_eq!(digest.layout_invalidated.as_slice(), &[n]);
    }
}
