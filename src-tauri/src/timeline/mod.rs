//! Time-machine timeline for desktop layout history.
//!
//! Provides a ring buffer of auto-captured checkpoints plus an unlimited list
//! of manual-pinned checkpoints. Write operations in zone / item / grouping
//! commands push into this store via [`hook::record_change`], which coalesces
//! rapid bursts via a 500 ms debounce window.
//!
//! Storage: `%APPDATA%/BentoDesk/timeline/checkpoint-{timestamp}.json`.

pub mod checkpoint;
pub mod hook;
pub mod ring_buffer;

#[allow(unused_imports)]
pub use checkpoint::{Checkpoint, CheckpointMeta, CheckpointStore, DeltaSummary};
pub use ring_buffer::TimelineBuffer;
