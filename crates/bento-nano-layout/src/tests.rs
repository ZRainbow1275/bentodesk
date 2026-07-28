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
