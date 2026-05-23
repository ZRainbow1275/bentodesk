//! In-memory inverted index over [`SearchItem`]s.
//!
//! The index is a `HashMap<token, SmallVec<[item_id; 8]>>`. Tokens are
//! lowercase ASCII fragments split on whitespace and path separators
//! (`/`, `\`). Both the title and the path are tokenised; the resulting
//! posting lists union per item.
//!
//! ## Scoring
//!
//! `query()` walks every token in the needle, looks up the per-token
//! posting list, and accumulates a score per matched item:
//!
//! - +`MATCH_BONUS` for any contains-match between needle-token and
//!   indexed-token (substring, case-insensitive).
//! - +`PREFIX_BONUS` extra when the indexed-token *starts with* the
//!   needle-token. The combined effect is that `"foo"` ranking
//!   `"foobar"` (prefix) above `"barfoo"` (mid-string).
//! - +`EXACT_BONUS` extra when the indexed-token equals the
//!   needle-token. Boosts bullseye matches over partial ones.
//!
//! Hits are sorted descending by score, ties broken by `id` lexicographic
//! order so test output is deterministic.

use std::collections::HashMap;

use smallvec::SmallVec;
use smol_str::SmolStr;

use super::types::{SearchHit, SearchItem};

/// Score added when a needle-token appears anywhere inside an
/// indexed-token (substring match, case-insensitive).
const MATCH_BONUS: u32 = 10;

/// Extra score when the indexed-token starts with the needle-token.
const PREFIX_BONUS: u32 = 20;

/// Extra score when the indexed-token equals the needle-token.
const EXACT_BONUS: u32 = 30;

/// Inline posting-list capacity. 8 covers most rare tokens without spilling.
const POSTING_INLINE: usize = 8;

/// Inline indexed-token capacity per item — most items tokenise into
/// fewer than 16 tokens (basename + ~3-deep path + a couple title words).
const ITEM_TOKEN_INLINE: usize = 16;

type PostingList = SmallVec<[SmolStr; POSTING_INLINE]>;
type ItemTokens = SmallVec<[SmolStr; ITEM_TOKEN_INLINE]>;

/// Inverted index for the SearchBar. Internal storage type — the public
/// façade is [`super::Index`], which is what callers depend on.
#[derive(Debug, Clone, Default)]
pub(crate) struct InvertedIndex {
    /// token → list of item ids whose tokenisation contains it.
    by_token: HashMap<SmolStr, PostingList>,
    /// id → original item (so `query` can return the matched_token alongside id).
    items: HashMap<SmolStr, SearchItem>,
    /// id → tokens it produced (so `remove` can prune posting lists cheaply).
    item_tokens: HashMap<SmolStr, ItemTokens>,
}

impl InvertedIndex {
    /// Construct an empty index. Used by tests; production callers go
    /// through [`super::Index::new`] which delegates to `Default`.
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of indexed items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True iff no items have been added.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Drop everything. Used when the desktop scanner restarts a full sweep.
    pub fn clear(&mut self) {
        self.by_token.clear();
        self.items.clear();
        self.item_tokens.clear();
    }

    /// Insert (or replace) an item. Re-indexing an existing id atomically
    /// removes the old posting-list entries before inserting the new ones.
    pub fn add(&mut self, item: SearchItem) {
        if self.items.contains_key(&item.id) {
            self.remove(&item.id.clone());
        }
        let tokens = tokenise(&item);
        for token in &tokens {
            self.by_token
                .entry(token.clone())
                .or_default()
                .push(item.id.clone());
        }
        self.item_tokens.insert(item.id.clone(), tokens);
        self.items.insert(item.id.clone(), item);
    }

    /// Remove an item by id. No-op when the id is unknown.
    pub fn remove(&mut self, id: &SmolStr) {
        let Some(tokens) = self.item_tokens.remove(id) else {
            return;
        };
        for token in &tokens {
            if let Some(list) = self.by_token.get_mut(token) {
                list.retain(|x| x != id);
                if list.is_empty() {
                    self.by_token.remove(token);
                }
            }
        }
        self.items.remove(id);
    }

    /// Query the index. Returns up to `limit` hits ordered by descending
    /// score (ties broken by id ascending).
    ///
    /// An empty needle yields no hits — the SearchBar UI hides the result
    /// list when the input is empty, so there is no caller for "all items".
    pub fn query(&self, needle: &str, limit: usize) -> SmallVec<[SearchHit; 32]> {
        let needle_tokens = tokenise_str(needle);
        if needle_tokens.is_empty() || limit == 0 {
            return SmallVec::new();
        }

        // id → (best_score, best_matched_token)
        let mut acc: HashMap<SmolStr, (u32, SmolStr)> = HashMap::new();

        for needle_token in &needle_tokens {
            for (indexed_token, ids) in &self.by_token {
                let Some(score) = score_pair(needle_token, indexed_token) else {
                    continue;
                };
                for id in ids {
                    let entry = acc.entry(id.clone()).or_insert((0, indexed_token.clone()));
                    if score > entry.0 {
                        entry.0 = score;
                        entry.1 = indexed_token.clone();
                    } else {
                        entry.0 = entry.0.saturating_add(score / 2);
                    }
                }
            }
        }

        let mut hits: SmallVec<[SearchHit; 32]> = acc
            .into_iter()
            .filter_map(|(id, (score, matched_token))| {
                let item = self.items.get(&id)?;
                Some(SearchHit {
                    id,
                    title: item.title.clone(),
                    path: item.path.clone(),
                    kind: item.kind.clone(),
                    score,
                    matched_token,
                })
            })
            .collect();

        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        if hits.len() > limit {
            hits.truncate(limit);
        }
        hits
    }
}

/// Score one needle-token against one indexed-token, both already
/// lowercase. Returns `None` when there is no contains-match.
fn score_pair(needle: &SmolStr, indexed: &SmolStr) -> Option<u32> {
    if !indexed.as_str().contains(needle.as_str()) {
        return None;
    }
    let mut score = MATCH_BONUS;
    if indexed.as_str().starts_with(needle.as_str()) {
        score = score.saturating_add(PREFIX_BONUS);
    }
    if indexed == needle {
        score = score.saturating_add(EXACT_BONUS);
    }
    Some(score)
}

/// Tokenise an item's title + path into a deduplicated list of lowercase
/// SmolStr tokens.
fn tokenise(item: &SearchItem) -> ItemTokens {
    let mut out: ItemTokens = SmallVec::new();
    push_unique(&mut out, tokenise_str(item.title.as_str()));
    push_unique(&mut out, tokenise_str(item.path.as_str()));
    out
}

fn push_unique(dst: &mut ItemTokens, src: SmallVec<[SmolStr; 8]>) {
    for tok in src {
        if !dst.iter().any(|t| t == &tok) {
            dst.push(tok);
        }
    }
}

/// Split `s` on whitespace + path separators (`/` and `\`), lowercase each
/// fragment, and drop empties. Punctuation that is not a separator (e.g.
/// `.`, `-`, `_`) stays inside the token so `report.pdf` survives as a
/// single token.
fn tokenise_str(s: &str) -> SmallVec<[SmolStr; 8]> {
    let mut out: SmallVec<[SmolStr; 8]> = SmallVec::new();
    for raw in s.split(|c: char| c.is_whitespace() || c == '/' || c == '\\') {
        if raw.is_empty() {
            continue;
        }
        let lowered: String = raw.chars().flat_map(|c| c.to_lowercase()).collect();
        if !lowered.is_empty() {
            out.push(SmolStr::from(lowered));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::types::SearchItemKind;

    fn item(id: &str, title: &str, path: &str, kind: SearchItemKind) -> SearchItem {
        SearchItem {
            id: SmolStr::from(id),
            title: SmolStr::from(title),
            path: SmolStr::from(path),
            kind,
        }
    }

    #[test]
    fn add_then_query_returns_the_added_item() {
        let mut idx = InvertedIndex::new();
        idx.add(item(
            "1",
            "Quarterly Report",
            "C:/docs/q1.pdf",
            SearchItemKind::File,
        ));
        let hits = idx.query("quarterly", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id.as_str(), "1");
    }

    #[test]
    fn remove_then_query_returns_no_hit() {
        let mut idx = InvertedIndex::new();
        idx.add(item("1", "alpha", "/x", SearchItemKind::File));
        idx.add(item("2", "beta", "/x", SearchItemKind::File));
        idx.remove(&SmolStr::from("1"));
        let hits = idx.query("alpha", 10);
        assert!(hits.is_empty());
        // Sibling survives.
        let hits = idx.query("beta", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id.as_str(), "2");
    }

    #[test]
    fn case_insensitive_matching() {
        let mut idx = InvertedIndex::new();
        idx.add(item("1", "readme", "/x", SearchItemKind::File));
        let hits = idx.query("README", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id.as_str(), "1");
    }

    #[test]
    fn prefix_bonus_outranks_mid_string_match() {
        let mut idx = InvertedIndex::new();
        // `foobar` → token starts with "foo" → gets PREFIX_BONUS.
        idx.add(item("prefix", "foobar", "/x", SearchItemKind::File));
        // `barfoo` → token contains "foo" mid-string → MATCH_BONUS only.
        idx.add(item("midstr", "barfoo", "/y", SearchItemKind::File));
        let hits = idx.query("foo", 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id.as_str(), "prefix");
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn exact_token_outranks_prefix_match() {
        let mut idx = InvertedIndex::new();
        idx.add(item("exact", "foo", "/x", SearchItemKind::File));
        idx.add(item("prefix", "foobar", "/y", SearchItemKind::File));
        let hits = idx.query("foo", 10);
        assert_eq!(hits[0].id.as_str(), "exact");
    }

    #[test]
    fn re_adding_same_id_replaces_old_tokens() {
        let mut idx = InvertedIndex::new();
        idx.add(item("1", "alpha", "/x", SearchItemKind::File));
        // Re-index with a totally different title.
        idx.add(item("1", "omega", "/y", SearchItemKind::File));
        // Old token must not match.
        assert!(idx.query("alpha", 10).is_empty());
        // New token must match.
        let hits = idx.query("omega", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id.as_str(), "1");
    }

    #[test]
    fn empty_needle_yields_no_hits() {
        let mut idx = InvertedIndex::new();
        idx.add(item("1", "alpha", "/x", SearchItemKind::File));
        assert!(idx.query("", 10).is_empty());
        assert!(idx.query("   ", 10).is_empty());
    }

    #[test]
    fn limit_truncates_results() {
        let mut idx = InvertedIndex::new();
        for i in 0..5 {
            idx.add(item(
                &format!("{i}"),
                "common",
                &format!("/p/{i}"),
                SearchItemKind::File,
            ));
        }
        let hits = idx.query("common", 3);
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn path_separators_split_into_tokens() {
        let mut idx = InvertedIndex::new();
        idx.add(item(
            "1",
            "_",
            "C:/Users/x/Documents/notes.md",
            SearchItemKind::File,
        ));
        // Mid-path component must be findable.
        let hits = idx.query("documents", 10);
        assert_eq!(hits.len(), 1);
        // Backslash separator works too.
        idx.clear();
        idx.add(item(
            "1",
            "_",
            r"C:\Users\x\Documents\notes.md",
            SearchItemKind::File,
        ));
        let hits = idx.query("documents", 10);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn clear_drops_everything() {
        let mut idx = InvertedIndex::new();
        idx.add(item("1", "alpha", "/x", SearchItemKind::File));
        idx.add(item("2", "beta", "/x", SearchItemKind::File));
        assert_eq!(idx.len(), 2);
        idx.clear();
        assert_eq!(idx.len(), 0);
        assert!(idx.is_empty());
        assert!(idx.query("alpha", 10).is_empty());
    }

    #[test]
    fn matched_token_field_reports_a_real_indexed_token() {
        let mut idx = InvertedIndex::new();
        idx.add(item("1", "Quarterly Report", "/x", SearchItemKind::File));
        let hits = idx.query("quart", 10);
        assert_eq!(hits.len(), 1);
        assert!(
            hits[0].matched_token.as_str().contains("quart"),
            "matched_token should reference the indexed token, got {:?}",
            hits[0].matched_token
        );
    }

    #[test]
    fn multiple_needle_tokens_accumulate() {
        let mut idx = InvertedIndex::new();
        idx.add(item("both", "alpha beta", "/x", SearchItemKind::File));
        idx.add(item("one", "alpha", "/y", SearchItemKind::File));
        let hits = idx.query("alpha beta", 10);
        // The item that satisfies both needle tokens must rank above the
        // single-token hit.
        assert_eq!(hits[0].id.as_str(), "both");
    }

    #[test]
    fn smallvec_inline_threshold_holds_for_typical_query() {
        // Sanity: 32 inline slots are enough for normal query sizes.
        let mut idx = InvertedIndex::new();
        for i in 0..16 {
            idx.add(item(
                &format!("{i:02}"),
                "common",
                "/x",
                SearchItemKind::File,
            ));
        }
        let hits = idx.query("common", 32);
        assert_eq!(hits.len(), 16);
        // SmallVec inline threshold is 32 — must not have spilled to heap.
        assert!(!hits.spilled());
    }
}
