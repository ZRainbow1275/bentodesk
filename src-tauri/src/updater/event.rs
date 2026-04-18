//! Event payload DTOs emitted by the updater module.
//!
//! These are kept in their own module so the frontend-facing type surface
//! is easy to audit at a glance — the updater UI subscribes to exactly
//! three named events:
//!
//! * `update:available` — fresh version found by a background check.
//! * `update:progress` — byte-level progress during `download_update`.
//! * `update:ready` — download finished, ready to restart.
//! * `update:error` — any step failed.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProgressPayload {
    pub chunk_len: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorPayload {
    pub kind: String,
    pub message: String,
}
