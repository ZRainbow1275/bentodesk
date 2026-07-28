//! T-098 — Mini Bar pinned-zone registry.
//!
//! In 1.x each minibar was a standalone Tauri `WebviewWindow`. The native
//! single-process invariant (spec §2) means windows are HWNDs created by the
//! shell-layer window factory (T-011), not by this module. The minibar
//! backend's only responsibility is to **track which zones are currently
//! pinned-as-minibar** and to enforce the hard cap of 3 concurrent pins.
//!
//! The actual HWND lifecycle is owned by `bentodesk-shell::window::factory`
//! which observes [`MinibarEvent`] and calls `WindowKind::MiniBar` create /
//! destroy in response.
//!
//! ## Hard cap
//!
//! At most 3 concurrent pinned minibars. The 1.x rationale was per-WebView
//! memory cost; the native rationale is per-HWND swap chain cost (R5 / R7 in
//! the master decomposition risk register), but the policy is unchanged.

use std::sync::Mutex;

use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

// ─── Public surface ──────────────────────────────────────────────────

/// Maximum simultaneous pinned minibars. Same as 1.x; rationale shifted
/// from WebView2 cost to swap-chain cost (R5/R7 in master plan §8).
pub const MAX_ACTIVE_MINIBARS: usize = 3;

// ─── Error type (spec §8.1 — hand-rolled, no thiserror) ──────────────

/// Errors surfaced by the minibar registry.
#[derive(Debug)]
pub enum MinibarError {
    /// At most [`MAX_ACTIVE_MINIBARS`] minibars may be pinned at once;
    /// caller asked to pin one beyond the cap.
    CapacityExceeded { cap: usize },
    /// The internal registry mutex was poisoned by a panic in another
    /// thread. The registry recovers via `into_inner`; this variant exists
    /// solely so the caller can log the recovery.
    Poisoned,
    /// Event channel send failed — the receiver was dropped. Not fatal: the
    /// registry mutation succeeded; only the notification was lost.
    EventChannelClosed,
}

impl core::fmt::Display for MinibarError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CapacityExceeded { cap } => {
                write!(
                    f,
                    "minibar cap exceeded: at most {cap} minibars may be pinned at once"
                )
            }
            Self::Poisoned => f.write_str("minibar registry mutex poisoned (recovered)"),
            Self::EventChannelClosed => f.write_str("minibar event channel receiver dropped"),
        }
    }
}

impl core::error::Error for MinibarError {}

// ─── Data model ──────────────────────────────────────────────────────

/// A single pinned minibar's record. `label` is the stable window-factory
/// identifier, equal to `format!("minibar-{zone_id}")` after stripping any
/// non-alphanumeric chars (matches 1.x naming so on-disk references survive).
///
/// Carries serde derives per ΔB ruling so v2.x scripting hooks can receive
/// it without re-deriving.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MinibarEntry {
    pub zone_id: SmolStr,
    pub label: SmolStr,
}

/// Event emitted on registry mutations. The window factory observes this
/// stream and creates / destroys the matching HWND.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MinibarEvent {
    /// A new zone was pinned. Window factory should create a `MiniBar` HWND
    /// for `entry.label` if one does not already exist.
    Pinned { entry: MinibarEntry },
    /// An existing minibar was unpinned. Window factory should close the
    /// HWND identified by `label`.
    Unpinned { label: SmolStr },
}

// ─── Registry ────────────────────────────────────────────────────────

/// In-memory registry of currently pinned minibars.
///
/// The store is `Send + Sync` — multiple dispatcher worker threads may
/// call `pin_zone` / `unpin` / `list` concurrently. Mutation propagates to
/// the window factory through the [`Sender<MinibarEvent>`] passed at
/// construction time.
pub struct MinibarStore {
    entries: Mutex<Vec<MinibarEntry>>,
    event_tx: Sender<MinibarEvent>,
}

impl MinibarStore {
    /// Construct a fresh registry. `event_tx` is the channel the shell
    /// layer drains in its window-factory loop.
    pub fn new(event_tx: Sender<MinibarEvent>) -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            event_tx,
        }
    }

    /// Pin a zone as a minibar. Returns the stable window label so the
    /// caller can reference the new HWND later.
    ///
    /// - Idempotent: pinning an already-pinned zone returns the existing
    ///   label without emitting a duplicate event.
    /// - Capacity-bounded: returns [`MinibarError::CapacityExceeded`] when
    ///   the registry already holds [`MAX_ACTIVE_MINIBARS`] entries.
    pub fn pin_zone(&self, zone_id: &str) -> Result<SmolStr, MinibarError> {
        let label = label_for_zone(zone_id);

        // Mutex recovery per spec §11 — never panic, always recover.
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(existing) = entries.iter().find(|e| e.label == label) {
            return Ok(existing.label.clone());
        }
        if entries.len() >= MAX_ACTIVE_MINIBARS {
            return Err(MinibarError::CapacityExceeded {
                cap: MAX_ACTIVE_MINIBARS,
            });
        }

        let entry = MinibarEntry {
            zone_id: SmolStr::from(zone_id),
            label: label.clone(),
        };
        entries.push(entry.clone());
        // Drop the lock BEFORE emitting so the receiver loop never stalls
        // behind a registry write.
        drop(entries);

        if self.event_tx.send(MinibarEvent::Pinned { entry }).is_err() {
            return Err(MinibarError::EventChannelClosed);
        }
        Ok(label)
    }

    /// Unpin the minibar identified by `label`. Idempotent — unpinning a
    /// missing label is a successful no-op (no event emitted).
    pub fn unpin(&self, label: &str) -> Result<(), MinibarError> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());

        let before = entries.len();
        entries.retain(|e| e.label.as_str() != label);
        let removed = entries.len() < before;
        drop(entries);

        if !removed {
            return Ok(());
        }
        if self
            .event_tx
            .send(MinibarEvent::Unpinned {
                label: SmolStr::from(label),
            })
            .is_err()
        {
            return Err(MinibarError::EventChannelClosed);
        }
        Ok(())
    }

    /// Snapshot of currently pinned minibars. Cloned out under the lock so
    /// callers do not hold the mutex while iterating.
    pub fn list(&self) -> Vec<MinibarEntry> {
        self.entries
            .lock()
            .map(|e| e.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    /// Number of currently pinned minibars.
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .map(|e| e.len())
            .unwrap_or_else(|e| e.into_inner().len())
    }

    /// True iff no minibars are pinned.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────

/// Build the stable window-factory label for a zone id.
///
/// Mirrors 1.x naming: strip non-`[A-Za-z0-9_-]` chars and prefix with
/// `"minibar-"`. The shell layer relies on the prefix to dispatch
/// `WindowKind::MiniBar` lookups.
fn label_for_zone(zone_id: &str) -> SmolStr {
    let mut buf = String::with_capacity(zone_id.len() + 8);
    buf.push_str("minibar-");
    for c in zone_id.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            buf.push(c);
        }
    }
    SmolStr::from(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    #[test]
    fn pin_returns_stable_label() {
        let (tx, _rx) = unbounded::<MinibarEvent>();
        let store = MinibarStore::new(tx);
        let label = store.pin_zone("zone-alpha").expect("pin");
        assert_eq!(label.as_str(), "minibar-zone-alpha");
    }

    #[test]
    fn pin_is_idempotent() {
        let (tx, _rx) = unbounded::<MinibarEvent>();
        let store = MinibarStore::new(tx);
        let a = store.pin_zone("zone-1").expect("first pin");
        let b = store.pin_zone("zone-1").expect("second pin");
        assert_eq!(a, b);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn cap_is_enforced() {
        let (tx, _rx) = unbounded::<MinibarEvent>();
        let store = MinibarStore::new(tx);
        for i in 0..MAX_ACTIVE_MINIBARS {
            store.pin_zone(&format!("zone-{i}")).expect("pin under cap");
        }
        match store.pin_zone("one-too-many") {
            Err(MinibarError::CapacityExceeded { cap }) => assert_eq!(cap, MAX_ACTIVE_MINIBARS),
            other => panic!("expected CapacityExceeded, got {other:?}"),
        }
    }

    #[test]
    fn unpin_removes_entry_and_emits_event() {
        let (tx, rx) = unbounded::<MinibarEvent>();
        let store = MinibarStore::new(tx);
        let label = store.pin_zone("zone-x").expect("pin");
        // Drain the Pinned event so we see Unpinned cleanly.
        let _ = rx.try_recv();
        store.unpin(&label).expect("unpin");
        assert!(store.is_empty());
        match rx.try_recv() {
            Ok(MinibarEvent::Unpinned { label: emitted }) => assert_eq!(emitted, label),
            other => panic!("expected Unpinned event, got {other:?}"),
        }
    }

    #[test]
    fn unpin_unknown_label_is_noop() {
        let (tx, _rx) = unbounded::<MinibarEvent>();
        let store = MinibarStore::new(tx);
        store.unpin("minibar-never-pinned").expect("noop unpin");
        assert!(store.is_empty());
    }

    #[test]
    fn label_strips_invalid_chars() {
        // Spaces, slashes, and CJK get filtered to keep the shell label ASCII.
        assert_eq!(
            label_for_zone("zone alpha/beta").as_str(),
            "minibar-zonealphabeta"
        );
        assert_eq!(label_for_zone("zone-1_v2").as_str(), "minibar-zone-1_v2");
    }

    #[test]
    fn list_returns_snapshot_in_insertion_order() {
        let (tx, _rx) = unbounded::<MinibarEvent>();
        let store = MinibarStore::new(tx);
        store.pin_zone("a").expect("pin a");
        store.pin_zone("b").expect("pin b");
        store.pin_zone("c").expect("pin c");
        let snapshot = store.list();
        assert_eq!(snapshot.len(), 3);
        assert_eq!(snapshot[0].zone_id.as_str(), "a");
        assert_eq!(snapshot[2].zone_id.as_str(), "c");
    }
}
