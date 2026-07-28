#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! `bentodesk-tree` — arena-allocated widget tree.
//!
//! Spec §10: no `Box<dyn>` per node; widgets are stored in an arena keyed by
//! [`NodeId`]. Children link via `SmallVec<[NodeId; 4]>` to avoid heap traffic
//! for the common shallow-tree case.
//!
//! The tree is generic over the per-node payload `T`. The widget crate
//! instantiates `Tree<WidgetNode>` with a tagged enum, keeping monomorphic
//! dispatch (no virtual table).

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod animation;
pub mod dirty;
pub mod effect;
pub mod signal;

pub use animation::{AnimatedValue, Easing, Lerp};
pub use dirty::DirtyDigest;
// `DirtyKind` is defined in this module (it's tied to `Tree`) — re-export it
// alongside the other tree primitives so callers don't need a sub-path.
pub use effect::{Effect, EffectArena, EffectError, EffectId, create_effect};
pub use signal::Signal;

use smallvec::SmallVec;
use smol_str::SmolStr;

/// Stable identifier for an arena slot. Wraps a `u32` (4G nodes is far beyond
/// the budget) plus a generation counter — accessing a freed id is detected
/// rather than silently aliasing a recycled slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId {
    index: u32,
    generation: u32,
}

impl NodeId {
    pub const ROOT_INVALID: NodeId = NodeId {
        index: u32::MAX,
        generation: 0,
    };

    /// Returns true if this id has never been allocated.
    pub fn is_invalid(&self) -> bool {
        self.index == u32::MAX
    }
}

/// Lookup error variants. Hand-rolled (spec §11) — no `thiserror`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeError {
    /// `NodeId.index` out of arena bounds.
    OutOfBounds,
    /// Slot was freed and recycled with a different generation.
    StaleHandle,
    /// Slot index in range but currently empty (freed).
    EmptySlot,
    /// Caller tried to remove a node still owned by a parent.
    StillAttached,
}

#[derive(Debug)]
struct Slot<T> {
    generation: u32,
    payload: Option<NodeData<T>>,
}

#[derive(Debug)]
struct NodeData<T> {
    /// Optional debug label — `SmolStr` keeps short names inline (≤22 bytes).
    pub debug_name: SmolStr,
    pub parent: Option<NodeId>,
    pub children: SmallVec<[NodeId; 4]>,
    pub data: T,
}

/// Kind of invalidation a node accumulates between frames. The renderer cares
/// whether layout has to re-run or whether paint alone is enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyKind {
    /// Geometry changed — layout must re-flow, then repaint.
    Layout,
    /// Only visual attributes changed (colour, opacity, text content).
    Paint,
}

/// Arena-backed retained tree. Iteration order is insertion order; layout
/// passes traverse via `children()` recursion.
#[derive(Debug)]
pub struct Tree<T> {
    slots: Vec<Slot<T>>,
    free_list: SmallVec<[u32; 16]>,
    root: Option<NodeId>,
    pending: dirty::DirtyDigest,
    /// Monotonic frame counter, used by `flush_dirty` to enforce the
    /// once-per-frame contract in debug builds.
    #[cfg(debug_assertions)]
    last_flush_frame: u64,
}

impl<T> Default for Tree<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Tree<T> {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_list: SmallVec::new(),
            root: None,
            pending: dirty::DirtyDigest::default(),
            #[cfg(debug_assertions)]
            last_flush_frame: 0,
        }
    }

    /// Reserve `cap` slots up-front so steady-state operation hits no heap.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            slots: Vec::with_capacity(cap),
            free_list: SmallVec::new(),
            root: None,
            pending: dirty::DirtyDigest::default(),
            #[cfg(debug_assertions)]
            last_flush_frame: 0,
        }
    }

    /// Insert a fresh node and return its handle. The node has no parent yet —
    /// call [`Tree::set_root`] or [`Tree::append_child`] to attach it.
    pub fn create(&mut self, debug_name: impl Into<SmolStr>, data: T) -> NodeId {
        let payload = NodeData {
            debug_name: debug_name.into(),
            parent: None,
            children: SmallVec::new(),
            data,
        };
        if let Some(idx) = self.free_list.pop() {
            let slot = &mut self.slots[idx as usize];
            slot.payload = Some(payload);
            return NodeId {
                index: idx,
                generation: slot.generation,
            };
        }
        let idx = self.slots.len() as u32;
        self.slots.push(Slot {
            generation: 0,
            payload: Some(payload),
        });
        NodeId {
            index: idx,
            generation: 0,
        }
    }

    /// Set `node` as the tree root. Returns the previous root (if any) without
    /// removing it from the arena — caller decides whether to free.
    pub fn set_root(&mut self, node: NodeId) -> Option<NodeId> {
        let prev = self.root;
        self.root = Some(node);
        prev
    }

    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    /// Borrow node data immutably.
    pub fn get(&self, id: NodeId) -> Result<&T, TreeError> {
        self.node_ref(id).map(|n| &n.data)
    }

    /// Borrow node data mutably.
    pub fn get_mut(&mut self, id: NodeId) -> Result<&mut T, TreeError> {
        self.node_mut(id).map(|n| &mut n.data)
    }

    /// Inspect a node's children list.
    pub fn children(&self, id: NodeId) -> Result<&[NodeId], TreeError> {
        self.node_ref(id).map(|n| n.children.as_slice())
    }

    pub fn parent(&self, id: NodeId) -> Result<Option<NodeId>, TreeError> {
        self.node_ref(id).map(|n| n.parent)
    }

    pub fn debug_name(&self, id: NodeId) -> Result<&str, TreeError> {
        self.node_ref(id).map(|n| n.debug_name.as_str())
    }

    /// Append `child` to `parent`'s children. Both nodes must be live; child
    /// must not already have a parent.
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), TreeError> {
        // Validate first.
        let _ = self.node_ref(parent)?;
        {
            let c = self.node_ref(child)?;
            if c.parent.is_some() {
                return Err(TreeError::StillAttached);
            }
        }
        // Mutate parent (children list).
        {
            let p = self.node_mut(parent)?;
            p.children.push(child);
        }
        // Mutate child (parent backref).
        {
            let c = self.node_mut(child)?;
            c.parent = Some(parent);
        }
        Ok(())
    }

    /// Detach `node` from its parent (if any). Node remains in the arena.
    pub fn detach(&mut self, node: NodeId) -> Result<(), TreeError> {
        let parent = match self.node_ref(node)?.parent {
            Some(p) => p,
            None => return Ok(()),
        };
        {
            let p = self.node_mut(parent)?;
            p.children.retain(|c| *c != node);
        }
        let c = self.node_mut(node)?;
        c.parent = None;
        Ok(())
    }

    /// Free `node`'s slot and bump its generation. Caller must ensure no
    /// children dangle (use `detach_recursive` for subtree removal).
    pub fn remove(&mut self, node: NodeId) -> Result<T, TreeError> {
        // Refuse to remove an attached node — caller must detach first to
        // keep parent.children consistent.
        if let Some(_p) = self.node_ref(node)?.parent {
            return Err(TreeError::StillAttached);
        }
        let idx = node.index as usize;
        let slot = self.slots.get_mut(idx).ok_or(TreeError::OutOfBounds)?;
        let payload = slot.payload.take().ok_or(TreeError::EmptySlot)?;
        slot.generation = slot.generation.wrapping_add(1);
        if !payload.children.is_empty() {
            return Err(TreeError::StillAttached);
        }
        if self.root == Some(node) {
            self.root = None;
        }
        self.free_list.push(node.index);
        Ok(payload.data)
    }

    /// Number of live nodes.
    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.payload.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Record that `node` needs work this frame. Idempotent — a node already
    /// marked at the same level is not re-pushed, so accidental double-marks
    /// from re-entrant signal subscribers don't bloat the digest.
    ///
    /// `Layout` implies `Paint`, but we keep them in distinct buckets so the
    /// renderer can branch on the cheaper case without rescanning.
    pub fn mark_dirty(&mut self, node: NodeId, kind: DirtyKind) {
        match kind {
            DirtyKind::Layout => {
                if !self.pending.layout_invalidated.contains(&node) {
                    self.pending.layout_invalidated.push(node);
                }
            }
            DirtyKind::Paint => {
                if !self.pending.repaint_only.contains(&node) {
                    self.pending.repaint_only.push(node);
                }
            }
        }
    }

    /// Drain the pending digest. Spec §C2: caller must invoke this exactly
    /// once per frame, supplying a monotonically increasing `frame_id`.
    /// Debug builds panic if the same `frame_id` is flushed twice — release
    /// builds drop the check to keep the per-frame path branch-free.
    pub fn flush_dirty(&mut self, frame_id: u64) -> dirty::DirtyDigest {
        #[cfg(debug_assertions)]
        {
            debug_assert!(
                frame_id > self.last_flush_frame || frame_id == 0 && self.last_flush_frame == 0,
                "flush_dirty called twice for frame {} (last was {})",
                frame_id,
                self.last_flush_frame,
            );
            self.last_flush_frame = frame_id;
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = frame_id;
        }
        core::mem::take(&mut self.pending)
    }

    /// Inspect the unflushed digest without consuming it. Useful for tests
    /// and the renderer's "should we even schedule a frame?" check.
    pub fn pending_dirty(&self) -> &dirty::DirtyDigest {
        &self.pending
    }

    fn node_ref(&self, id: NodeId) -> Result<&NodeData<T>, TreeError> {
        let slot = self
            .slots
            .get(id.index as usize)
            .ok_or(TreeError::OutOfBounds)?;
        if slot.generation != id.generation {
            return Err(TreeError::StaleHandle);
        }
        slot.payload.as_ref().ok_or(TreeError::EmptySlot)
    }

    fn node_mut(&mut self, id: NodeId) -> Result<&mut NodeData<T>, TreeError> {
        let slot = self
            .slots
            .get_mut(id.index as usize)
            .ok_or(TreeError::OutOfBounds)?;
        if slot.generation != id.generation {
            return Err(TreeError::StaleHandle);
        }
        slot.payload.as_mut().ok_or(TreeError::EmptySlot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_attach_detach_remove_roundtrip() {
        let mut t = Tree::<u32>::new();
        let r = t.create("root", 1);
        let a = t.create("a", 2);
        let b = t.create("b", 3);

        let _ = t.set_root(r);
        assert!(t.append_child(r, a).is_ok());
        assert!(t.append_child(r, b).is_ok());

        assert_eq!(t.children(r).map(|c| c.len()).unwrap_or(0), 2);
        assert_eq!(t.parent(a).ok().flatten(), Some(r));

        // Removing attached node must fail.
        assert_eq!(t.remove(a), Err(TreeError::StillAttached));

        assert!(t.detach(a).is_ok());
        assert_eq!(t.parent(a).ok().flatten(), None);
        assert_eq!(t.remove(a), Ok(2));

        // Stale handle (a) should no longer resolve.
        assert_eq!(t.get(a), Err(TreeError::StaleHandle));
    }

    #[test]
    fn flush_dirty_collects_marks_and_clears() {
        let mut t = Tree::<u32>::new();
        let r = t.create("root", 0);
        let a = t.create("a", 1);
        let b = t.create("b", 2);

        t.mark_dirty(r, DirtyKind::Layout);
        t.mark_dirty(a, DirtyKind::Paint);
        t.mark_dirty(b, DirtyKind::Paint);
        // Idempotent — second mark of `a` must not duplicate.
        t.mark_dirty(a, DirtyKind::Paint);

        let digest = t.flush_dirty(1);
        assert_eq!(digest.layout_invalidated.len(), 1);
        assert_eq!(digest.repaint_only.len(), 2);

        // Subsequent flush on a quiescent tree returns empty.
        let next = t.flush_dirty(2);
        assert!(next.is_empty());
    }

    #[test]
    fn flush_dirty_split_layout_vs_paint() {
        let mut t = Tree::<u32>::new();
        let n = t.create("n", 7);
        t.mark_dirty(n, DirtyKind::Layout);
        let d = t.flush_dirty(1);
        assert_eq!(d.layout_invalidated.as_slice(), &[n]);
        assert!(d.repaint_only.is_empty());
    }

    #[test]
    fn freelist_recycles_slot_with_bumped_generation() {
        let mut t = Tree::<&'static str>::new();
        let n1 = t.create("n1", "v1");
        assert!(t.detach(n1).is_ok());
        let _ = t.remove(n1);
        let n2 = t.create("n2", "v2");
        assert_eq!(n1.index, n2.index, "slot must be reused");
        assert_ne!(n1.generation, n2.generation, "generation must bump");
    }
}
