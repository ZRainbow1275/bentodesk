#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! `bento-nano-layout` — flexbox-lite layout engine.
//!
//! Spec §3.2: 100% self-rolled; no taffy / stretch / yoga.
//! Spec §10: layout buffers use `SmallVec<[Rect; N]>`; no per-frame heap.
//!
//! ## Phase 1 surface (closed)
//!
//!   - Single-pass main-axis layout with row OR column direction.
//!   - Fixed sizes (`Length::Px`) and fractional sizes (`Length::Fraction`).
//!   - Flex grow on `Length::Auto` children, sharing leftover main-axis space.
//!   - Cross-axis stretch (default) or fixed cross size.
//!   - Padding on the parent only.
//!
//! ## Phase 2 additions (T-037..T-041)
//!
//!   - `LayoutDesc::gap` — inter-child spacing along the main axis.
//!   - `LayoutDesc::align` / `justify` — cross-axis + main-axis alignment.
//!   - `LayoutDesc::margin` — per-side outer spacing on each child (collapsed
//!     against siblings on the main axis, summed on the cross axis).
//!   - `Direction::Grid { columns }` — fixed-column grid placement, row-major.
//!   - Recursion depth limit (defensive) — `MAX_DEPTH` rejects pathological
//!     trees rather than blowing the stack.

#![forbid(unsafe_op_in_unsafe_fn)]

use bento_nano_style::{Edges, Length, Rect, Size};
use bento_nano_tree::{NodeId, Tree, TreeError};
use smallvec::SmallVec;

/// Layout error variants. Hand-rolled (spec §11) — no `thiserror`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutError {
    /// Tree access failed (node id stale / out of bounds / etc).
    Tree(TreeError),
    /// Caller passed a viewport with non-finite or negative dimensions.
    InvalidViewport,
    /// Recursion exceeded [`MAX_DEPTH`]. Defensive — pathological trees
    /// (cycles or deeply nested config) hit this before blowing the stack.
    DepthLimit,
}

impl From<TreeError> for LayoutError {
    fn from(e: TreeError) -> Self {
        LayoutError::Tree(e)
    }
}

/// Recursion ceiling. 96 covers BentoDesk's deepest realistic nesting (Settings
/// modal → Tab → Card → Form group → Row → Column → Input ≈ 12) with margin
/// for plugin / business surfaces; defensive against accidental cycles.
pub const MAX_DEPTH: u32 = 96;

/// Main axis direction. `Row` lays children left-to-right, `Column` top-to-bottom.
/// `Grid { columns }` places children row-major into a fixed-column grid; row
/// height is the per-child requested height (or the leftover share for `Auto`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    Row,
    #[default]
    Column,
    /// Row-major grid with `columns` cells per row. Cell width = parent main
    /// axis / columns. Cell height = child's requested height (or `Auto`
    /// uses the cell width as a fallback square).
    Grid {
        columns: u32,
    },
}

/// Cross-axis alignment of children. Default is `Stretch` so existing Phase 1
/// callers keep their full-cross-axis layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Stretch,
    Start,
    Center,
    End,
}

/// Main-axis distribution of children (CSS flexbox `justify-content` subset).
/// Default is `Start` so existing Phase 1 callers keep packed-from-origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    /// Equal gap **between** children. Outer edges flush.
    SpaceBetween,
    /// Equal gap around each child (half-gap on outer edges).
    SpaceAround,
}

/// Per-node layout description. Widgets supply this via the [`LayoutSource`]
/// trait so the engine never owns widget data directly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutDesc {
    pub direction: Direction,
    pub width: Length,
    pub height: Length,
    pub padding: Edges,
    /// Inter-child gap along the main axis. Counted (n-1) times for n
    /// children. Cross-axis gap matches in `Direction::Grid` between rows.
    pub gap: f32,
    /// Cross-axis alignment for non-stretching children.
    pub align: Align,
    /// Main-axis distribution.
    pub justify: Justify,
    /// Outer margin applied to **this** node when laid out by its parent.
    /// Summed against parent padding on the cross axis; reduces the main-axis
    /// allocation by left+right (Row) or top+bottom (Column).
    pub margin: Edges,
}

impl Default for LayoutDesc {
    fn default() -> Self {
        Self {
            direction: Direction::Column,
            width: Length::Auto,
            height: Length::Auto,
            padding: Edges::ZERO,
            gap: 0.0,
            align: Align::Stretch,
            justify: Justify::Start,
            margin: Edges::ZERO,
        }
    }
}

/// Bridge from the tree's payload to a [`LayoutDesc`]. Implementing this trait
/// lets the layout engine stay decoupled from widget enums.
pub trait LayoutSource {
    fn layout(&self) -> LayoutDesc;
}

/// Computed rectangle per node — output of one layout pass. Address by NodeId
/// via [`LayoutResult::get`].
#[derive(Debug, Default)]
pub struct LayoutResult {
    /// SmallVec inline-cap covers steady-state UI (~32 nodes per pass).
    rects: SmallVec<[(NodeId, Rect); 32]>,
}

impl LayoutResult {
    pub fn get(&self, id: NodeId) -> Option<Rect> {
        self.rects
            .iter()
            .find_map(|(nid, r)| if *nid == id { Some(*r) } else { None })
    }

    pub fn iter(&self) -> impl Iterator<Item = &(NodeId, Rect)> {
        self.rects.iter()
    }

    pub fn len(&self) -> usize {
        self.rects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    pub fn clear(&mut self) {
        self.rects.clear();
    }
}

/// Cache key — hash-equivalent identity for "the tree state we last laid
/// out". Tree mutations (create / append / detach / remove) all change
/// `tree.len()`; viewport resizes change `viewport`; signal-driven
/// repaints flow through `Tree::mark_dirty` and bump the dirty epoch via
/// the renderer's frame loop.
///
/// `viewport` uses raw `f32` bits comparison to dodge `PartialEq`'s NaN
/// quirks — invalid viewports never reach the cache (rejected earlier).
#[derive(Debug, Clone, Copy)]
struct CacheKey {
    viewport_w_bits: u32,
    viewport_h_bits: u32,
    tree_len: usize,
    /// Caller-supplied generation tag — the renderer bumps this whenever
    /// `Tree::flush_dirty` returns a non-empty digest, so a layout-only
    /// signal change still invalidates.
    epoch: u64,
}

impl CacheKey {
    fn matches(&self, other: &CacheKey) -> bool {
        self.viewport_w_bits == other.viewport_w_bits
            && self.viewport_h_bits == other.viewport_h_bits
            && self.tree_len == other.tree_len
            && self.epoch == other.epoch
    }
}

/// Layout engine. Caches the most recent pass result keyed on (viewport,
/// tree.len, epoch); calls with the same key short-circuit and reuse the
/// cached `LayoutResult` (C3 commitment).
#[derive(Debug, Default)]
pub struct LayoutEngine {
    result: LayoutResult,
    last_key: Option<CacheKey>,
    /// Counter of cache hits since construction — exposed for tests and
    /// renderer instrumentation. Not part of the spec budget.
    hit_count: u64,
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow the most recent layout result without re-running the pass.
    /// Returns `None` when no `layout()` call has happened yet (the result
    /// is empty either way, but `None` lets hit-testing skip the iter).
    pub fn last_result(&self) -> Option<&LayoutResult> {
        if self.result.is_empty() {
            None
        } else {
            Some(&self.result)
        }
    }

    /// Total cache-hit count since construction. Used by tests to verify
    /// the C3 short-circuit fires; renderer debug overlay can surface it.
    pub fn cache_hits(&self) -> u64 {
        self.hit_count
    }

    /// Force the next `layout()` call to re-run, even if the cache key
    /// matches. Use after structural mutations the engine can't observe
    /// (e.g. swapping a node's `LayoutSource`-relevant fields without
    /// changing the tree topology).
    pub fn invalidate(&mut self) {
        self.last_key = None;
    }

    /// Run a single layout pass over `tree.root()` inside `viewport`.
    /// Returns a borrow of the cached result; valid until the next call.
    pub fn layout<T: LayoutSource>(
        &mut self,
        tree: &Tree<T>,
        viewport: Size,
    ) -> Result<&LayoutResult, LayoutError> {
        self.layout_with_epoch(tree, viewport, 0)
    }

    /// Layout with an explicit cache epoch. The renderer increments the
    /// epoch each time `Tree::flush_dirty` reports work, so a frame whose
    /// digest was empty hits the cache even if the renderer calls
    /// `layout()` defensively.
    pub fn layout_with_epoch<T: LayoutSource>(
        &mut self,
        tree: &Tree<T>,
        viewport: Size,
        epoch: u64,
    ) -> Result<&LayoutResult, LayoutError> {
        if !viewport.width.is_finite()
            || !viewport.height.is_finite()
            || viewport.width < 0.0
            || viewport.height < 0.0
        {
            return Err(LayoutError::InvalidViewport);
        }
        let key = CacheKey {
            viewport_w_bits: viewport.width.to_bits(),
            viewport_h_bits: viewport.height.to_bits(),
            tree_len: tree.len(),
            epoch,
        };
        if let Some(prev) = self.last_key {
            if prev.matches(&key) && !self.result.is_empty() {
                self.hit_count += 1;
                return Ok(&self.result);
            }
        }

        self.result.clear();
        let root = match tree.root() {
            Some(r) => r,
            None => {
                self.last_key = Some(key);
                return Ok(&self.result);
            }
        };
        let root_rect = Rect {
            x: 0.0,
            y: 0.0,
            width: viewport.width,
            height: viewport.height,
        };
        layout_subtree(tree, root, root_rect, &mut self.result, 0)?;
        self.last_key = Some(key);
        Ok(&self.result)
    }
}

/// Recursive flex-lite pass. Fills `result` for every visited node.
fn layout_subtree<T: LayoutSource>(
    tree: &Tree<T>,
    node: NodeId,
    parent_rect: Rect,
    out: &mut LayoutResult,
    depth: u32,
) -> Result<(), LayoutError> {
    if depth >= MAX_DEPTH {
        return Err(LayoutError::DepthLimit);
    }
    let desc = tree.get(node)?.layout();
    let self_rect = resolve_self_rect(parent_rect, desc.width, desc.height);
    out.rects.push((node, self_rect));

    let children = tree.children(node)?;
    if children.is_empty() {
        return Ok(());
    }

    // Inner content rect after padding.
    let pad = desc.padding;
    let content = Rect {
        x: self_rect.x + pad.left,
        y: self_rect.y + pad.top,
        width: (self_rect.width - pad.horizontal()).max(0.0),
        height: (self_rect.height - pad.vertical()).max(0.0),
    };

    match desc.direction {
        Direction::Grid { columns } => {
            layout_grid(tree, children, content, columns, desc.gap, out, depth)?;
        }
        Direction::Row | Direction::Column => {
            layout_flex(tree, children, content, &desc, out, depth)?;
        }
    }
    Ok(())
}

/// Flex (Row/Column) pass — handles gap, align, justify, margin alongside the
/// Phase 1 auto-grow distribution.
fn layout_flex<T: LayoutSource>(
    tree: &Tree<T>,
    children: &[NodeId],
    content: Rect,
    parent: &LayoutDesc,
    out: &mut LayoutResult,
    depth: u32,
) -> Result<(), LayoutError> {
    let row = matches!(parent.direction, Direction::Row);
    let main_total = if row { content.width } else { content.height };

    // Gap allocation — counted (n-1) times for n children.
    let gap_total = if children.len() < 2 {
        0.0
    } else {
        parent.gap * (children.len() - 1) as f32
    };

    // Pass 1 — measure fixed/fraction children, count auto children. Margin on
    // the main axis subtracts from the child's main allocation; on the cross
    // axis it shrinks the available cross size for that child only.
    let mut consumed = 0.0_f32;
    let mut auto_count: u32 = 0;
    let mut child_main: SmallVec<[f32; 16]> = SmallVec::new();
    for &c in children {
        let cdesc = tree.get(c)?.layout();
        let cmargin_main = if row {
            cdesc.margin.horizontal()
        } else {
            cdesc.margin.vertical()
        };
        let main = if row {
            length_main(cdesc.width, content.width)
        } else {
            length_main(cdesc.height, content.height)
        };
        match main {
            Some(v) => {
                consumed += v + cmargin_main;
                child_main.push(v);
            }
            None => {
                auto_count += 1;
                consumed += cmargin_main; // margin still counted
                child_main.push(f32::NAN);
            }
        }
    }
    consumed += gap_total;
    let leftover = (main_total - consumed).max(0.0);
    let auto_share = if auto_count == 0 {
        0.0
    } else {
        leftover / auto_count as f32
    };

    // Pass 2 — compute justify offset + per-child gap (SpaceBetween/SpaceAround).
    // Used main = consumed when there are auto children (they ate `leftover`);
    // otherwise leftover bleeds into the justify calculation.
    let used = if auto_count > 0 { main_total } else { consumed };
    let free = (main_total - used).max(0.0);
    let (origin_offset, between_extra) = justify_offsets(parent.justify, free, children.len());

    let mut cursor = if row { content.x } else { content.y } + origin_offset;
    let cross_full = if row { content.height } else { content.width };

    for (i, &c) in children.iter().enumerate() {
        let cdesc = tree.get(c)?.layout();
        let main = if child_main[i].is_nan() {
            auto_share
        } else {
            child_main[i]
        };
        // Margin on the leading edge of the main axis pushes the cursor.
        let lead_main_margin = if row {
            cdesc.margin.left
        } else {
            cdesc.margin.top
        };
        let trail_main_margin = if row {
            cdesc.margin.right
        } else {
            cdesc.margin.bottom
        };
        cursor += lead_main_margin;

        // Cross-axis sizing — Stretch fills cross_full minus margins; explicit
        // length resolves; Auto with non-stretch align falls back to cross_full.
        let cross_margin = if row {
            cdesc.margin.vertical()
        } else {
            cdesc.margin.horizontal()
        };
        let cross_avail = (cross_full - cross_margin).max(0.0);
        let cross_explicit = if row {
            length_main(cdesc.height, cross_full)
        } else {
            length_main(cdesc.width, cross_full)
        };
        let cross_size = match (parent.align, cross_explicit) {
            (_, Some(v)) => v.min(cross_avail),
            (Align::Stretch, None) => cross_avail,
            (Align::Start | Align::Center | Align::End, None) => cross_avail,
        };
        let cross_origin = if row { content.y } else { content.x }
            + if row {
                cdesc.margin.top
            } else {
                cdesc.margin.left
            };
        let cross_extra = (cross_avail - cross_size).max(0.0);
        let cross_pos = match parent.align {
            Align::Start | Align::Stretch => cross_origin,
            Align::Center => cross_origin + cross_extra * 0.5,
            Align::End => cross_origin + cross_extra,
        };

        let child_rect = if row {
            Rect {
                x: cursor,
                y: cross_pos,
                width: main,
                height: cross_size,
            }
        } else {
            Rect {
                x: cross_pos,
                y: cursor,
                width: cross_size,
                height: main,
            }
        };
        layout_subtree(tree, c, child_rect, out, depth + 1)?;
        cursor += main + trail_main_margin;
        // Inter-child gap + justify-extra (only between, not after the last).
        if i + 1 < children.len() {
            cursor += parent.gap + between_extra;
        }
    }
    Ok(())
}

/// Compute the leading offset and per-gap extra for a given `Justify`. `free`
/// is the leftover main-axis space after children + gap + margin; it is zero
/// when at least one child is `Auto` (auto children eat all the slack).
fn justify_offsets(j: Justify, free: f32, n: usize) -> (f32, f32) {
    if n == 0 || free <= 0.0 {
        return (0.0, 0.0);
    }
    match j {
        Justify::Start => (0.0, 0.0),
        Justify::Center => (free * 0.5, 0.0),
        Justify::End => (free, 0.0),
        Justify::SpaceBetween => {
            if n < 2 {
                (free * 0.5, 0.0)
            } else {
                (0.0, free / (n - 1) as f32)
            }
        }
        Justify::SpaceAround => {
            let slot = free / n as f32;
            (slot * 0.5, slot)
        }
    }
}

/// Grid pass — row-major into `columns`-wide rows. Cell width = main / columns
/// minus inter-col gap. Cell height honours per-child requested height; `Auto`
/// uses the cell width as a square fallback. Inter-row gap matches `gap`.
fn layout_grid<T: LayoutSource>(
    tree: &Tree<T>,
    children: &[NodeId],
    content: Rect,
    columns: u32,
    gap: f32,
    out: &mut LayoutResult,
    depth: u32,
) -> Result<(), LayoutError> {
    if columns == 0 || children.is_empty() {
        return Ok(());
    }
    let cols = columns as f32;
    let total_h_gap = if columns < 2 {
        0.0
    } else {
        gap * (columns - 1) as f32
    };
    let cell_w = ((content.width - total_h_gap) / cols).max(0.0);

    for (i, &c) in children.iter().enumerate() {
        let col = (i as u32) % columns;
        let row = (i as u32) / columns;
        let cdesc = tree.get(c)?.layout();
        let cell_h = match cdesc.height {
            Length::Px(v) => v,
            Length::Fraction(f) => content.height * f,
            Length::Auto => cell_w, // square fallback
        };
        let x = content.x + col as f32 * (cell_w + gap);
        let y = content.y + row as f32 * (cell_h + gap);
        let child_rect = Rect {
            x,
            y,
            width: cell_w,
            height: cell_h,
        };
        layout_subtree(tree, c, child_rect, out, depth + 1)?;
    }
    Ok(())
}

/// Resolve `self_rect` for a node whose parent already gave it a rectangle.
/// Fixed `Length::Px` overrides the parent allocation; `Length::Fraction`
/// scales the parent rect; `Length::Auto` accepts the parent allocation.
fn resolve_self_rect(parent: Rect, w: Length, h: Length) -> Rect {
    let width = match w {
        Length::Px(v) => v,
        Length::Fraction(f) => parent.width * f,
        Length::Auto => parent.width,
    };
    let height = match h {
        Length::Px(v) => v,
        Length::Fraction(f) => parent.height * f,
        Length::Auto => parent.height,
    };
    Rect {
        x: parent.x,
        y: parent.y,
        width,
        height,
    }
}

/// Resolve a child's main-axis length. `Auto` returns `None` so the parent can
/// distribute leftover space across all auto children equally.
fn length_main(len: Length, parent_main: f32) -> Option<f32> {
    match len {
        Length::Px(v) => Some(v),
        Length::Fraction(f) => Some(parent_main * f),
        Length::Auto => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct LayoutOnly(LayoutDesc);

    impl LayoutSource for LayoutOnly {
        fn layout(&self) -> LayoutDesc {
            self.0
        }
    }

    fn ld(direction: Direction, w: Length, h: Length) -> LayoutDesc {
        LayoutDesc {
            direction,
            width: w,
            height: h,
            ..LayoutDesc::default()
        }
    }

    #[test]
    fn column_distributes_auto_children_equally() {
        let mut t = Tree::<LayoutOnly>::new();
        let root = t.create(
            "root",
            LayoutOnly(ld(Direction::Column, Length::Px(100.0), Length::Px(300.0))),
        );
        let a = t.create(
            "a",
            LayoutOnly(ld(Direction::Column, Length::Auto, Length::Auto)),
        );
        let b = t.create(
            "b",
            LayoutOnly(ld(Direction::Column, Length::Auto, Length::Auto)),
        );
        let _ = t.set_root(root);
        assert!(t.append_child(root, a).is_ok());
        assert!(t.append_child(root, b).is_ok());

        let mut engine = LayoutEngine::new();
        let viewport = Size {
            width: 100.0,
            height: 300.0,
        };
        let res = engine.layout(&t, viewport);
        assert!(res.is_ok(), "layout call must succeed; got {:?}", res.err());
        let res = match res {
            Ok(r) => r,
            Err(_) => return,
        };

        let ra = res.get(a).unwrap_or(Rect::ZERO);
        let rb = res.get(b).unwrap_or(Rect::ZERO);
        assert!((ra.height - 150.0).abs() < 1e-3);
        assert!((rb.height - 150.0).abs() < 1e-3);
        assert!((rb.y - 150.0).abs() < 1e-3);
    }

    #[test]
    fn layout_cache_short_circuits_on_repeat_call() {
        // C3 commitment: identical (viewport, tree.len, epoch) returns the
        // cached rects without re-running the recursion.
        let mut t = Tree::<LayoutOnly>::new();
        let root = t.create(
            "root",
            LayoutOnly(ld(Direction::Column, Length::Px(40.0), Length::Px(60.0))),
        );
        let _ = t.set_root(root);

        let mut engine = LayoutEngine::new();
        let viewport = Size {
            width: 40.0,
            height: 60.0,
        };
        assert_eq!(engine.cache_hits(), 0);
        let _ = engine.layout(&t, viewport);
        assert_eq!(engine.cache_hits(), 0, "first call must compute");
        let _ = engine.layout(&t, viewport);
        assert_eq!(engine.cache_hits(), 1, "second identical call must hit");
        let _ = engine.layout(&t, viewport);
        assert_eq!(engine.cache_hits(), 2);
    }

    #[test]
    fn layout_cache_invalidates_on_viewport_change() {
        let mut t = Tree::<LayoutOnly>::new();
        let root = t.create(
            "root",
            LayoutOnly(ld(Direction::Column, Length::Px(40.0), Length::Px(60.0))),
        );
        let _ = t.set_root(root);

        let mut engine = LayoutEngine::new();
        let _ = engine.layout(
            &t,
            Size {
                width: 40.0,
                height: 60.0,
            },
        );
        let _ = engine.layout(
            &t,
            Size {
                width: 80.0,
                height: 60.0,
            },
        );
        assert_eq!(engine.cache_hits(), 0, "viewport change must miss cache");
    }

    #[test]
    fn layout_cache_invalidates_on_tree_growth() {
        let mut t = Tree::<LayoutOnly>::new();
        let root = t.create(
            "root",
            LayoutOnly(ld(Direction::Column, Length::Auto, Length::Auto)),
        );
        let _ = t.set_root(root);

        let mut engine = LayoutEngine::new();
        let viewport = Size {
            width: 40.0,
            height: 60.0,
        };
        let _ = engine.layout(&t, viewport);
        // Add a child — tree.len() bumps from 1 to 2.
        let c = t.create(
            "c",
            LayoutOnly(ld(Direction::Column, Length::Auto, Length::Auto)),
        );
        let _ = t.append_child(root, c);
        let _ = engine.layout(&t, viewport);
        assert_eq!(engine.cache_hits(), 0, "tree growth must miss cache");
    }

    #[test]
    fn layout_cache_invalidate_drops_key() {
        let mut t = Tree::<LayoutOnly>::new();
        let root = t.create(
            "root",
            LayoutOnly(ld(Direction::Column, Length::Px(40.0), Length::Px(60.0))),
        );
        let _ = t.set_root(root);

        let mut engine = LayoutEngine::new();
        let viewport = Size {
            width: 40.0,
            height: 60.0,
        };
        let _ = engine.layout(&t, viewport);
        let _ = engine.layout(&t, viewport);
        assert_eq!(engine.cache_hits(), 1);
        engine.invalidate();
        let _ = engine.layout(&t, viewport);
        assert_eq!(engine.cache_hits(), 1, "invalidate must force recompute");
    }

    // -----------------------------------------------------------------------
    // Phase 2 (T-037..T-041) regression tests.
    // -----------------------------------------------------------------------

    /// Run a layout pass and look up the rect for `target`. Returns
    /// `Rect::ZERO` if either layout or lookup fails — tests then assert
    /// against the expected non-zero rect, surfacing the failure as a normal
    /// assertion rather than tripping clippy's denied panic / unwrap forms.
    fn layout_and_get(
        engine: &mut LayoutEngine,
        t: &Tree<LayoutOnly>,
        viewport: Size,
        target: NodeId,
    ) -> Rect {
        match engine.layout(t, viewport) {
            Ok(res) => res.get(target).unwrap_or(Rect::ZERO),
            Err(_) => Rect::ZERO,
        }
    }

    #[test]
    fn row_with_gap_inserts_inter_child_spacing() {
        let mut t = Tree::<LayoutOnly>::new();
        let mut root_desc = ld(Direction::Row, Length::Px(100.0), Length::Px(20.0));
        root_desc.gap = 10.0;
        let root = t.create("root", LayoutOnly(root_desc));
        let a = t.create(
            "a",
            LayoutOnly(ld(Direction::Row, Length::Px(30.0), Length::Px(20.0))),
        );
        let b = t.create(
            "b",
            LayoutOnly(ld(Direction::Row, Length::Px(30.0), Length::Px(20.0))),
        );
        let _ = t.set_root(root);
        let _ = t.append_child(root, a);
        let _ = t.append_child(root, b);

        let mut engine = LayoutEngine::new();
        let viewport = Size {
            width: 100.0,
            height: 20.0,
        };
        let ra = layout_and_get(&mut engine, &t, viewport, a);
        let rb = layout_and_get(&mut engine, &t, viewport, b);
        assert!((ra.x - 0.0).abs() < 1e-3);
        assert!((rb.x - 40.0).abs() < 1e-3);
    }

    #[test]
    fn row_with_align_center_centers_short_child() {
        let mut t = Tree::<LayoutOnly>::new();
        let mut root_desc = ld(Direction::Row, Length::Px(40.0), Length::Px(100.0));
        root_desc.align = Align::Center;
        let root = t.create("root", LayoutOnly(root_desc));
        let c = t.create(
            "c",
            LayoutOnly(ld(Direction::Row, Length::Px(40.0), Length::Px(20.0))),
        );
        let _ = t.set_root(root);
        let _ = t.append_child(root, c);

        let mut engine = LayoutEngine::new();
        let viewport = Size {
            width: 40.0,
            height: 100.0,
        };
        let rc = layout_and_get(&mut engine, &t, viewport, c);
        assert!((rc.y - 40.0).abs() < 1e-3, "got y={}", rc.y);
        assert!((rc.height - 20.0).abs() < 1e-3);
    }

    #[test]
    fn row_with_justify_space_between_distributes_extra() {
        let mut t = Tree::<LayoutOnly>::new();
        let mut root_desc = ld(Direction::Row, Length::Px(100.0), Length::Px(20.0));
        root_desc.justify = Justify::SpaceBetween;
        let root = t.create("root", LayoutOnly(root_desc));
        let a = t.create(
            "a",
            LayoutOnly(ld(Direction::Row, Length::Px(20.0), Length::Px(20.0))),
        );
        let b = t.create(
            "b",
            LayoutOnly(ld(Direction::Row, Length::Px(20.0), Length::Px(20.0))),
        );
        let c = t.create(
            "c",
            LayoutOnly(ld(Direction::Row, Length::Px(20.0), Length::Px(20.0))),
        );
        let _ = t.set_root(root);
        let _ = t.append_child(root, a);
        let _ = t.append_child(root, b);
        let _ = t.append_child(root, c);

        let mut engine = LayoutEngine::new();
        let viewport = Size {
            width: 100.0,
            height: 20.0,
        };
        let ra = layout_and_get(&mut engine, &t, viewport, a);
        let rb = layout_and_get(&mut engine, &t, viewport, b);
        let rc = layout_and_get(&mut engine, &t, viewport, c);
        assert!((ra.x - 0.0).abs() < 1e-3);
        assert!((rb.x - 40.0).abs() < 1e-3);
        assert!((rc.x - 80.0).abs() < 1e-3);
    }

    #[test]
    fn child_margin_shrinks_main_allocation() {
        let mut t = Tree::<LayoutOnly>::new();
        let root = t.create(
            "root",
            LayoutOnly(ld(Direction::Row, Length::Px(100.0), Length::Px(20.0))),
        );
        let mut child_desc = ld(Direction::Row, Length::Auto, Length::Auto);
        child_desc.margin = Edges {
            top: 0.0,
            right: 10.0,
            bottom: 0.0,
            left: 20.0,
        };
        let c = t.create("c", LayoutOnly(child_desc));
        let _ = t.set_root(root);
        let _ = t.append_child(root, c);

        let mut engine = LayoutEngine::new();
        let viewport = Size {
            width: 100.0,
            height: 20.0,
        };
        let rc = layout_and_get(&mut engine, &t, viewport, c);
        assert!((rc.x - 20.0).abs() < 1e-3, "got x={}", rc.x);
        assert!((rc.width - 70.0).abs() < 1e-3, "got w={}", rc.width);
    }

    #[test]
    fn grid_places_children_row_major() {
        let mut t = Tree::<LayoutOnly>::new();
        let mut root_desc = ld(
            Direction::Grid { columns: 2 },
            Length::Px(200.0),
            Length::Px(200.0),
        );
        root_desc.gap = 10.0;
        let root = t.create("root", LayoutOnly(root_desc));
        let ids: Vec<_> = (0..4)
            .map(|_| {
                t.create(
                    "c",
                    LayoutOnly(ld(Direction::Row, Length::Px(80.0), Length::Px(40.0))),
                )
            })
            .collect();
        let _ = t.set_root(root);
        for &c in &ids {
            let _ = t.append_child(root, c);
        }

        let mut engine = LayoutEngine::new();
        let viewport = Size {
            width: 200.0,
            height: 200.0,
        };
        let r0 = layout_and_get(&mut engine, &t, viewport, ids[0]);
        let r1 = layout_and_get(&mut engine, &t, viewport, ids[1]);
        let r2 = layout_and_get(&mut engine, &t, viewport, ids[2]);
        let r3 = layout_and_get(&mut engine, &t, viewport, ids[3]);
        assert!((r0.x - 0.0).abs() < 1e-3 && (r0.y - 0.0).abs() < 1e-3);
        assert!((r1.x - 105.0).abs() < 1e-3 && (r1.y - 0.0).abs() < 1e-3);
        assert!((r2.x - 0.0).abs() < 1e-3 && (r2.y - 50.0).abs() < 1e-3);
        assert!((r3.x - 105.0).abs() < 1e-3 && (r3.y - 50.0).abs() < 1e-3);
    }

    #[test]
    fn recursion_depth_limit_rejects_deep_chain() {
        // Build a degenerate chain of MAX_DEPTH + 4 nodes — layout must reject
        // before blowing the stack.
        let mut t = Tree::<LayoutOnly>::new();
        let root = t.create(
            "root",
            LayoutOnly(ld(Direction::Column, Length::Px(10.0), Length::Px(10.0))),
        );
        let _ = t.set_root(root);
        let mut prev = root;
        for _ in 0..(MAX_DEPTH as usize + 4) {
            let n = t.create(
                "n",
                LayoutOnly(ld(Direction::Column, Length::Px(10.0), Length::Px(10.0))),
            );
            let _ = t.append_child(prev, n);
            prev = n;
        }

        let mut engine = LayoutEngine::new();
        let err = engine.layout(
            &t,
            Size {
                width: 10.0,
                height: 10.0,
            },
        );
        assert!(matches!(err, Err(LayoutError::DepthLimit)));
    }

    #[test]
    fn benchmark_500_node_tree_completes() {
        // Defensive bench (T-041) — fan-out tree of 500 nodes. Asserts the
        // layout pass returns Ok with the expected rect count.
        let mut t = Tree::<LayoutOnly>::new();
        let root = t.create(
            "root",
            LayoutOnly(ld(Direction::Column, Length::Px(500.0), Length::Px(500.0))),
        );
        let _ = t.set_root(root);
        for _ in 0..500 {
            let c = t.create(
                "c",
                LayoutOnly(ld(Direction::Column, Length::Px(1.0), Length::Px(1.0))),
            );
            let _ = t.append_child(root, c);
        }
        let mut engine = LayoutEngine::new();
        let viewport = Size {
            width: 500.0,
            height: 500.0,
        };
        let len = match engine.layout(&t, viewport) {
            Ok(r) => r.len(),
            Err(_) => 0,
        };
        assert_eq!(len, 501, "expected 500 children + 1 root rects");
    }
}
