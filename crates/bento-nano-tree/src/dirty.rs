//! Per-frame dirty-tracking digest.
//!
//! Spec §C2 (commitment): `Tree::flush_dirty` runs **once per frame**, batches
//! all marks accumulated since the previous flush, and clears them. The split
//! into `layout_invalidated` vs `repaint_only` lets the renderer skip layout
//! when only paint state changed (e.g. a colour signal).
//!
//! Spec §10: inline buffers — typical frames touch a handful of nodes, so the
//! 32-slot SmallVec keeps everything on the stack at steady state.

use smallvec::SmallVec;

use crate::NodeId;

/// What changed this frame, partitioned by the kind of work the renderer
/// needs to do. Nodes can appear in both vectors when a full
/// layout-and-repaint is required.
#[derive(Debug, Default, Clone)]
pub struct DirtyDigest {
    pub layout_invalidated: SmallVec<[NodeId; 32]>,
    pub repaint_only: SmallVec<[NodeId; 32]>,
}

impl DirtyDigest {
    pub fn is_empty(&self) -> bool {
        self.layout_invalidated.is_empty() && self.repaint_only.is_empty()
    }

    pub fn clear(&mut self) {
        self.layout_invalidated.clear();
        self.repaint_only.clear();
    }
}
