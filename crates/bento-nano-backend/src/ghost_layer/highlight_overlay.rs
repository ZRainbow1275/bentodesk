//! Desktop file highlight overlay — C2 Rule Preview.
//!
//! Pushes pulsing-circle preview events to the renderer so the user can see
//! which desktop files would be affected by a smart-group suggestion.
//!
//! ## What this module owns (vs T-086)
//!
//! - The [`HighlightTarget`] / [`HighlightPayload`] schema (forward-compat
//!   with v2.x scripting via serde derives).
//! - [`emit_highlight`] / [`emit_clear`] — push these on the renderer's
//!   command channel.
//!
//! ## What this module does NOT own
//!
//! - Resolving file paths → desktop icon positions. That requires the icon
//!   layout backup which lives in `T-086 icon_positions` (not yet ported).
//!   Callers walk their own layout and build the [`HighlightTarget`] vec
//!   themselves; once T-086 lands the helper migrates there.
//! - The actual D2D drawing — the renderer (`bento-nano-app::render`)
//!   subscribes to the channel and handles the pulse animation.

use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};

/// Default highlight duration (milliseconds) if the caller does not specify.
pub const DEFAULT_HIGHLIGHT_DURATION_MS: u64 = 3_000;

/// A single highlight target resolved from a file path.
///
/// Coordinates are in **desktop logical** units; the renderer converts to
/// per-monitor pixel space at paint time using the monitor it's drawing on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HighlightTarget {
    /// Display name of the desktop icon (e.g. `"report.pdf"`).
    pub name: String,
    /// Desktop logical x coordinate of the icon center.
    pub x: i32,
    /// Desktop logical y coordinate of the icon center.
    pub y: i32,
    /// Index of the monitor containing this point. `None` if the point falls
    /// off-screen (icon was on a now-disconnected monitor); the renderer
    /// should fall back to the primary monitor in that case.
    pub monitor_index: Option<u32>,
}

/// Highlight command pushed onto the renderer's channel.
///
/// `Highlight { targets, duration_ms }` flashes the supplied targets;
/// `Clear` removes any in-flight highlights immediately.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HighlightPayload {
    Highlight {
        targets: Vec<HighlightTarget>,
        duration_ms: u64,
    },
    Clear,
}

/// Push a `Highlight` payload onto the renderer's channel.
///
/// Logs at `warn!` if the channel is closed (matches 1.x behaviour where the
/// failure path also `tracing::warn!`'d).
pub fn emit_highlight(
    tx: &Sender<HighlightPayload>,
    targets: Vec<HighlightTarget>,
    duration_ms: u64,
) {
    if tx
        .send(HighlightPayload::Highlight {
            targets,
            duration_ms,
        })
        .is_err()
    {
        tracing::warn!("ghost_layer: highlight channel closed");
    }
}

/// Push a `Clear` payload onto the renderer's channel.
pub fn emit_clear(tx: &Sender<HighlightPayload>) {
    if tx.send(HighlightPayload::Clear).is_err() {
        tracing::warn!("ghost_layer: highlight channel closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_target_serde_round_trip() {
        let original = HighlightTarget {
            name: "report.pdf".into(),
            x: 100,
            y: 200,
            monitor_index: Some(1),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: HighlightTarget = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, parsed);
    }

    #[test]
    fn highlight_payload_serde_round_trip_for_clear() {
        let json = serde_json::to_string(&HighlightPayload::Clear).expect("serialize");
        let parsed: HighlightPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, HighlightPayload::Clear);
    }

    #[test]
    fn emit_highlight_sends_one_payload() {
        let (tx, rx) = crossbeam_channel::unbounded();
        emit_highlight(
            &tx,
            vec![HighlightTarget {
                name: "x".into(),
                x: 1,
                y: 2,
                monitor_index: None,
            }],
            500,
        );
        let payload = rx.try_recv().expect("payload sent");
        assert!(matches!(
            payload,
            HighlightPayload::Highlight {
                targets,
                duration_ms: 500,
            } if targets.len() == 1
        ));
    }
}
