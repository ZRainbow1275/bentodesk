//! `ThemeObserver` — current-theme accessor + subscriber list.
//!
//! T-003 (Phase 1) closes the per-process singleton API on top of the T-001
//! observer struct: `theme::current()` / `theme::set_current()` /
//! `theme::subscribe()` / `theme::unsubscribe()` free functions sit at the
//! crate root and route through a `thread_local!` cell (UI thread is the
//! single writer, so no lock is needed).
//!
//! Spec §10: `current()` returns `&'static ThemeTokens` — zero alloc, zero
//! lock — so per-frame paint code can call it on every widget without budget
//! impact. Subscribers are kept in a `SmallVec<[(SubscriberHandle, NodeId,
//! EffectId); 16]>`; the inline 16 covers BentoDesk's typical "every top-level
//! surface re-paints on theme change" without touching the heap.
//!
//! Spec §11: no `Box<dyn Fn>`, no panic. `set_current` returns
//! [`ThemeError::UnknownThemeId`] when the lookup misses — caller decides
//! whether that's a logic bug or a recoverable miss (settings UI typing).
//!
//! ## Why `SubscriberHandle` carries the EffectId
//!
//! `subscribe` accepts both the `NodeId` (so the theme switch can invalidate
//! the right paint area via `tree.mark_dirty(node, DirtyKind::Paint)`) and the
//! `EffectId` minted by the caller via [`bentodesk_tree::create_effect`].
//! The handle is a stable opaque token the caller stores next to the widget
//! and hands back to [`unsubscribe`] when the widget is dropped. This split
//! preserves §10 — the theme module never owns or boxes the effect callback.
//!
//! ## set_current scheduling contract
//!
//! [`set_current`] walks the subscriber list and calls
//! [`EffectArena::schedule`] on every registered `EffectId`. The next
//! [`EffectArena::flush`] (driven by the frame loop) runs each effect's body,
//! which is responsible for calling `tree.mark_dirty(node, DirtyKind::Paint)`.
//! No silent dirty-marking — the caller's effect closure is the single source
//! of truth for "what does this subscriber do on theme change".

use bentodesk_tree::{EffectArena, EffectId, NodeId};
use core::cell::RefCell;
use smallvec::SmallVec;

use crate::{DARK_DEFAULT, ThemeTokens, theme::ThemeId};

/// Stable opaque token returned by [`subscribe`]. Wraps a monotonic `u32`
/// counter — never recycled, so a stale handle passed to [`unsubscribe`] is
/// silently dropped (no panic per spec §11). Callers store this next to the
/// widget that owns the subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriberHandle(u32);

impl SubscriberHandle {
    /// Sentinel for "not subscribed". Useful as the default for widget structs
    /// that lazily subscribe.
    pub const INVALID: SubscriberHandle = SubscriberHandle(u32::MAX);

    pub fn is_invalid(self) -> bool {
        self.0 == u32::MAX
    }
}

/// Theme-API error. Hand-rolled (spec §8.1 / §11) — no `thiserror`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeError {
    /// `set_current` received a `theme_id` not present in
    /// [`crate::theme::THEMES`].
    UnknownThemeId,
}

impl core::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownThemeId => f.write_str("theme id not found in THEMES registry"),
        }
    }
}

impl core::error::Error for ThemeError {}

/// Process-local theme observer. Holds the active theme handle (`&'static
/// ThemeTokens`, never cloned), an inline subscriber list, and the next
/// handle id to mint.
#[derive(Debug)]
pub struct ThemeObserver {
    id: ThemeId,
    handle: &'static ThemeTokens,
    /// Subscribed widget tree nodes + their effect ids. `SmallVec<[..; 16]>`
    /// covers the typical "every top-level surface re-paints on theme change"
    /// case without touching the heap (spec §10).
    subs: SmallVec<[Subscriber; 16]>,
    /// Monotonic counter for `SubscriberHandle` — never recycled.
    next_handle: u32,
}

/// One row in the observer's subscriber list — pairs a [`SubscriberHandle`]
/// with the (NodeId, EffectId) the caller registered. Exposed for the frame
/// loop / tests; production callers normally only need [`SubscriberHandle`].
#[derive(Debug, Clone, Copy)]
pub struct Subscriber {
    handle: SubscriberHandle,
    node: NodeId,
    effect: EffectId,
}

impl Default for ThemeObserver {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeObserver {
    /// Initialise against `DARK_DEFAULT`. T-024 swaps this with a
    /// settings-driven choice when the persistence layer lands.
    pub fn new() -> Self {
        Self {
            id: ThemeId::new_static("dark"),
            handle: &DARK_DEFAULT,
            subs: SmallVec::new(),
            next_handle: 0,
        }
    }

    pub fn id(&self) -> &ThemeId {
        &self.id
    }

    /// Active theme tokens. Returned by `&'static` reference so callers can
    /// store the pointer cheaply across frames.
    pub fn current(&self) -> &'static ThemeTokens {
        self.handle
    }

    /// Swap to a new theme. `new_id` is matched against the registry; on
    /// match, the handle is updated and the subscriber slice is returned so
    /// the caller (the frame loop) can mark them dirty in one pass. Returns
    /// `None` when `new_id` does not match any registered theme — the caller
    /// decides whether that's a logic error or a recoverable miss.
    pub fn set(&mut self, new_id: ThemeId) -> Option<&[Subscriber]> {
        let next = crate::theme::THEMES
            .iter()
            .find(|(name, _)| *name == new_id.as_str())
            .map(|(_, h)| *h)?;
        if core::ptr::eq(next, self.handle) && self.id == new_id {
            return Some(&[]);
        }
        self.id = new_id;
        self.handle = next;
        Some(self.subs.as_slice())
    }

    /// Subscribe `node` to theme-change notifications, paired with the
    /// `effect` that should fire on change. Returns a unique handle for
    /// later [`Self::unsubscribe`].
    pub fn subscribe(&mut self, node: NodeId, effect: EffectId) -> SubscriberHandle {
        let handle = SubscriberHandle(self.next_handle);
        self.next_handle = self.next_handle.wrapping_add(1);
        self.subs.push(Subscriber {
            handle,
            node,
            effect,
        });
        handle
    }

    /// Unsubscribe by handle. Stale / unknown handles are silently dropped
    /// (no panic, per spec §11).
    pub fn unsubscribe(&mut self, handle: SubscriberHandle) {
        self.subs.retain(|s| s.handle != handle);
    }

    /// Inline view of the current subscriber set. Each entry exposes node +
    /// effect ids the frame loop hands to its `EffectArena`.
    pub fn subscribers(&self) -> &[Subscriber] {
        &self.subs
    }
}

// `Subscriber` is intentionally `pub(crate)` in field shape — callers iterate
// via the public accessors below so the internal layout stays free to change.
impl Subscriber {
    pub fn handle(&self) -> SubscriberHandle {
        self.handle
    }
    pub fn node(&self) -> NodeId {
        self.node
    }
    pub fn effect(&self) -> EffectId {
        self.effect
    }
}

// ---------------------------------------------------------------------------
// Process-local singleton — T-003.
//
// The UI thread is the only writer (spec §9: GetMessageW loop runs on the
// main thread, every paint / theme switch arrives there). `RefCell` inside a
// `thread_local!` is enough — no `Mutex`, no atomic pointer juggling. Read
// path stays cheap because `current()` only borrows immutably and immediately
// drops the borrow before returning the `&'static` handle.
// ---------------------------------------------------------------------------

thread_local! {
    static OBSERVER: RefCell<ThemeObserver> = RefCell::new(ThemeObserver::new());
}

/// Active theme tokens. Zero-alloc, zero-lock, callable on the hot paint path
/// (spec §10). The returned reference is `&'static` because every theme in
/// [`crate::theme::THEMES`] is a `static`.
pub fn current() -> &'static ThemeTokens {
    OBSERVER.with(|o| o.borrow().current())
}

/// Active theme id (e.g. `"dark"` / `"light"`). Cheap clone — `ThemeId` is a
/// `SmolStr` and the baked names are `new_static` so the underlying bytes are
/// `&'static str`.
pub fn current_id() -> ThemeId {
    OBSERVER.with(|o| o.borrow().id().clone())
}

/// Swap to the theme registered as `theme_id`. On hit: updates the singleton
/// handle, then schedules every subscribed [`EffectId`] on `arena` so the
/// next [`EffectArena::flush`] re-runs them (each effect body is responsible
/// for marking its own node dirty, per the design in `effect.rs`).
///
/// On miss: returns [`ThemeError::UnknownThemeId`] — singleton state stays
/// unchanged, no effects scheduled. Spec §11: no panic on user-input miss.
pub fn set_current(arena: &mut EffectArena, theme_id: &str) -> Result<(), ThemeError> {
    OBSERVER.with(|o| {
        let mut obs = o.borrow_mut();
        let id = ThemeId::new(theme_id);
        let subs = obs.set(id).ok_or(ThemeError::UnknownThemeId)?;
        // `schedule` errors on stale handles only. We deliberately swallow
        // those (spec §11: a stale subscriber doesn't poison the switch);
        // the next `unsubscribe` call cleans the handle out for good.
        for s in subs {
            let _ = arena.schedule(s.effect);
        }
        Ok(())
    })
}

/// Subscribe `node` to theme-change re-paints. The caller mints `effect` via
/// [`bentodesk_tree::create_effect`] beforehand, attaching `node` as a
/// tracked node so [`EffectArena::flush`] folds it into the per-frame
/// [`bentodesk_tree::DirtyDigest`].
pub fn subscribe(node: NodeId, effect: EffectId) -> SubscriberHandle {
    OBSERVER.with(|o| o.borrow_mut().subscribe(node, effect))
}

/// Unsubscribe by handle. Stale / unknown handles are silently dropped.
pub fn unsubscribe(handle: SubscriberHandle) {
    OBSERVER.with(|o| o.borrow_mut().unsubscribe(handle))
}

/// Snapshot of the current subscriber count. Test-only helper; production
/// callers shouldn't branch on this.
#[doc(hidden)]
pub fn subscriber_count() -> usize {
    OBSERVER.with(|o| o.borrow().subscribers().len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bentodesk_tree::{DirtyDigest, Effect, create_effect};
    use core::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn observer_defaults_to_dark() {
        let o = ThemeObserver::new();
        assert_eq!(o.id().as_str(), "dark");
        assert!(core::ptr::eq(o.current(), &DARK_DEFAULT));
    }

    #[test]
    fn observer_swaps_to_light_and_returns_subscribers() {
        let mut o = ThemeObserver::new();
        let _h = o.subscribe(NodeId::ROOT_INVALID, EffectId::INVALID);
        let dirty = o.set(ThemeId::new_static("light")).unwrap_or(&[]);
        assert_eq!(dirty.len(), 1);
        assert_eq!(o.id().as_str(), "light");
    }

    #[test]
    fn observer_unknown_id_is_none() {
        let mut o = ThemeObserver::new();
        assert!(o.set(ThemeId::new_static("midnight")).is_none());
        // State unchanged.
        assert_eq!(o.id().as_str(), "dark");
    }

    #[test]
    fn observer_unsubscribe_drops_only_target_handle() {
        let mut o = ThemeObserver::new();
        let h1 = o.subscribe(NodeId::ROOT_INVALID, EffectId::INVALID);
        let h2 = o.subscribe(NodeId::ROOT_INVALID, EffectId::INVALID);
        assert_ne!(h1, h2);
        assert_eq!(o.subscribers().len(), 2);
        o.unsubscribe(h1);
        assert_eq!(o.subscribers().len(), 1);
        assert_eq!(o.subscribers()[0].handle(), h2);
        // Re-unsubscribing a stale handle is a silent no-op.
        o.unsubscribe(h1);
        assert_eq!(o.subscribers().len(), 1);
    }

    // -----------------------------------------------------------------------
    // Singleton API tests. Each test resets the singleton to dark + clears
    // every subscriber so the suite is order-independent.
    // -----------------------------------------------------------------------

    fn reset_singleton() {
        OBSERVER.with(|o| {
            let mut obs = o.borrow_mut();
            *obs = ThemeObserver::new();
        });
    }

    #[test]
    fn singleton_current_returns_dark_by_default() {
        reset_singleton();
        assert!(core::ptr::eq(current(), &DARK_DEFAULT));
        assert_eq!(current_id().as_str(), "dark");
    }

    #[test]
    fn singleton_set_current_swaps_handle() {
        reset_singleton();
        let mut arena = EffectArena::new();
        assert!(set_current(&mut arena, "light").is_ok());
        assert_eq!(current_id().as_str(), "light");
        assert!(core::ptr::eq(current(), &crate::theme::LIGHT_DEFAULT));
    }

    #[test]
    fn singleton_set_current_unknown_id_returns_error() {
        reset_singleton();
        let mut arena = EffectArena::new();
        let result = set_current(&mut arena, "midnight");
        assert_eq!(result, Err(ThemeError::UnknownThemeId));
        // State unchanged on miss.
        assert_eq!(current_id().as_str(), "dark");
    }

    #[test]
    fn singleton_subscribe_then_set_current_schedules_effect() {
        reset_singleton();
        let mut arena = EffectArena::new();
        let counter = Rc::new(Cell::new(0u32));
        let c2 = counter.clone();
        // Caller mints the Effect, then subscribes the (node, effect) pair.
        let mut eff = Effect::new(move || c2.set(c2.get() + 1));
        eff.add_tracked_node(NodeId::ROOT_INVALID);
        let eid = arena.create(eff);
        let _h = subscribe(NodeId::ROOT_INVALID, eid);

        // Switch theme — the effect must be scheduled.
        assert!(set_current(&mut arena, "light").is_ok());
        assert!(arena.is_scheduled(eid).unwrap_or(false));

        // Frame loop drains.
        let mut digest = DirtyDigest::default();
        let ran = arena.flush(&mut digest);
        assert_eq!(ran, 1);
        assert_eq!(counter.get(), 1);
        assert_eq!(
            digest.layout_invalidated.as_slice(),
            &[NodeId::ROOT_INVALID]
        );
    }

    #[test]
    fn singleton_unsubscribe_stops_future_scheduling() {
        reset_singleton();
        let mut arena = EffectArena::new();
        let eid = create_effect(&mut arena, || {});
        let h = subscribe(NodeId::ROOT_INVALID, eid);
        assert_eq!(subscriber_count(), 1);
        unsubscribe(h);
        assert_eq!(subscriber_count(), 0);
        // Switch theme — no effect scheduled because no subscriber left.
        assert!(set_current(&mut arena, "light").is_ok());
        assert!(!arena.is_scheduled(eid).unwrap_or(true));
    }
}
