//! T-091 — updater event payloads.
//!
//! 1.x emitted four named string events on the Tauri event bus
//! (`update:available` / `update:progress` / `update:ready` / `update:error`).
//! Per spec §2 single-process invariant the native port collapses these into a
//! single typed enum delivered on a `crossbeam_channel::Sender<UpdateEvent>`.
//!
//! All variants carry serde derives per the master §11 ΔB ruling so v2.x
//! scripting hooks can re-introduce serialization without breaking the wire
//! shape. Build-time only — single-process never serializes at runtime.

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use super::UpdateInfo;

/// Event emitted by [`super::Updater`] as the lifecycle progresses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdateEvent {
    /// A non-skipped version is available.
    Available { info: UpdateInfo },
    /// Streaming download progress. `total_bytes` is `None` when the server
    /// did not send a Content-Length header.
    Progress { progress: UpdateProgress },
    /// Download finished and staged — ready for the next lifecycle step.
    Ready { info: UpdateInfo },
    /// Staged installer was launched and the app can now quit to let the
    /// installer replace files.
    Installing { info: UpdateInfo },
    /// Any step failed. `kind` is one of `"check"` / `"download"` /
    /// `"verify"` / `"install"` so the UI can route the message.
    Error { kind: SmolStr, message: String },
}

/// Streaming download progress.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateProgress {
    pub chunk_len: u64,
    pub total_bytes: Option<u64>,
}
