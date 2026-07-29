//! F1.2 — SearchBar backend (in-memory inverted index + debouncer).
//!
//! The SearchBar UI panel at
//! `bentodesk-app::business::search_bar` was scaffolded against this
//! module's public surface ahead of the body — the panel's tick loop
//! holds its own debounce clock and emits `Command::QuerySearch` (planned
//! for the F1.3 dispatcher wave) carrying the typed needle. This module
//! turns that needle into a ranked [`SearchHit`] list.
//!
//! ## Module surface
//!
//! - [`Index`] — the public façade. `new` / `add` / `remove` / `clear`
//!   mutate the inverted index; `query(needle, limit)` returns the top
//!   matches as a `SmallVec<[SearchHit; 32]>` so typical queries stay
//!   inline.
//! - [`Debouncer`] — wall-clock debouncer (see `debounce` for the
//!   monotonic-clock contract).
//! - [`SearchItem`] / [`SearchHit`] / [`SearchItemKind`] — public types,
//!   all `Debug + Clone + PartialEq + Serialize + Deserialize`.
//!
//! ## Dependency budget (§8)
//!
//! Stdlib `HashMap` + `smol_str` + `smallvec` + `serde` only — no new
//! crate enters the §8 whitelist.
//!
//! ## §10 hot-path discipline
//!
//! Tokens, ids, paths are `SmolStr` (≤22 byte inline). Posting lists and
//! query results are `SmallVec` (8 / 32 inline slots respectively). The
//! `query` path is the SearchBar's hot loop; per-keystroke calls allocate
//! only when posting lists exceed the inline budget.
//!
//! ## §11 / §17 compliance
//!
//! No `unwrap` / `expect` / `panic` outside `#[cfg(test)]`. No `todo!`
//! or `unimplemented!` anywhere — the module is end-to-end functional.

mod debounce;
mod index;
mod types;

pub use debounce::Debouncer;
pub use types::{SearchHit, SearchItem, SearchItemKind};

use smallvec::SmallVec;
use smol_str::SmolStr;

/// Public façade over the inverted index.
///
/// A thin wrapper around `index::InvertedIndex` so the four-method
/// surface specified in the F1.2 task body (`new` / `add` / `remove` /
/// `query` / `clear`) is the only API callers see, leaving room to swap
/// the storage backing later (e.g. a tantivy-style segment tree) without
/// breaking the dispatcher.
#[derive(Debug, Clone, Default)]
pub struct Index {
    inner: index::InvertedIndex,
}

impl Index {
    /// Construct an empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace `item`. Re-indexing an existing id atomically
    /// removes the old posting-list entries before inserting the new ones.
    pub fn add(&mut self, item: SearchItem) {
        self.inner.add(item);
    }

    /// Remove the item identified by `id`. No-op when `id` is unknown.
    pub fn remove(&mut self, id: &SmolStr) {
        self.inner.remove(id);
    }

    /// Drop everything. Used when the desktop scanner restarts a full sweep.
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Number of indexed items.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True iff no items have been added.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Query the index with `needle`, returning at most `limit` hits
    /// ordered by descending score (ties broken by id ascending).
    ///
    /// Returns an empty `SmallVec` when the needle is empty or `limit`
    /// is zero.
    pub fn query(&self, needle: &str, limit: usize) -> SmallVec<[SearchHit; 32]> {
        self.inner.query(needle, limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str, title: &str) -> SearchItem {
        SearchItem {
            id: SmolStr::from(id),
            title: SmolStr::from(title),
            path: SmolStr::from("/desktop"),
            keywords: SmolStr::default(),
            kind: SearchItemKind::File,
        }
    }

    #[test]
    fn façade_add_then_query_round_trips() {
        let mut idx = Index::new();
        idx.add(sample("a", "alpha"));
        let hits = idx.query("alpha", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id.as_str(), "a");
    }

    #[test]
    fn façade_remove_drops_the_hit() {
        let mut idx = Index::new();
        idx.add(sample("a", "alpha"));
        idx.remove(&SmolStr::from("a"));
        assert!(idx.query("alpha", 10).is_empty());
    }

    #[test]
    fn façade_clear_resets_everything() {
        let mut idx = Index::new();
        idx.add(sample("a", "alpha"));
        idx.add(sample("b", "beta"));
        assert_eq!(idx.len(), 2);
        idx.clear();
        assert!(idx.is_empty());
    }

    #[test]
    fn debouncer_re_export_is_constructible() {
        // Surface guard: the SearchBar UI will `use bentodesk_backend::search::Debouncer`,
        // so the re-export must land at this path.
        let mut d = Debouncer::new(120);
        assert!(d.tap(0));
        assert!(!d.tap(50));
        assert!(d.tap(120));
    }

    #[test]
    fn limit_zero_returns_empty() {
        let mut idx = Index::new();
        idx.add(sample("a", "alpha"));
        assert!(idx.query("alpha", 0).is_empty());
    }
}
