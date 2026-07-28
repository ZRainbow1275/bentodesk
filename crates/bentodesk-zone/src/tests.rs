use super::*;

fn zone(id: u64, x: i32) -> Zone {
    Zone::new(ZoneId(id), Cow::Borrowed("z"), x, 0, 100, 100)
}

#[test]
fn zone_list_add_and_iter_preserve_order() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));
    let xs: Vec<i32> = zl.iter().map(|z| z.x).collect();
    assert_eq!(xs, vec![10, 20, 30]);
    assert_eq!(zl.len(), 3);
}

#[test]
fn zone_list_remove_returns_true_on_hit() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    assert!(zl.remove(ZoneId(1)));
    assert_eq!(zl.len(), 1);
    assert!(zl.get(ZoneId(1)).is_none());
    assert!(zl.get(ZoneId(2)).is_some());
    assert!(!zl.remove(ZoneId(99)), "missing id must report false");
}

#[test]
fn auto_arrange_items_sorts_one_zone_and_rebuilds_its_grid() {
    let mut z = zone(7, 0);
    z.grid_columns = 2;
    let zulu = z
        .add_item(Cow::Borrowed("C:/Desktop/Zulu.txt"), Cow::Borrowed("z"))
        .expect("zulu");
    let alpha = z
        .add_item(Cow::Borrowed("C:/Desktop/alpha.txt"), Cow::Borrowed("a"))
        .expect("alpha");
    let beta = z
        .add_item(Cow::Borrowed("C:/Desktop/Beta.txt"), Cow::Borrowed("b"))
        .expect("beta");

    assert!(z.auto_arrange_items());
    assert_eq!(
        z.items.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![alpha, beta, zulu]
    );
    assert_eq!(
        z.items
            .iter()
            .map(|item| (item.x, item.y))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0), (0, 1)]
    );
    assert!(
        !z.auto_arrange_items(),
        "already-arranged items are a no-op"
    );
}

#[test]
fn zone_list_get_mut_allows_geometry_edit() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    let z = match zl.get_mut(ZoneId(1)) {
        Some(z) => z,
        None => return,
    };
    z.x = 999;
    z.w = 200;
    let z2 = match zl.get(ZoneId(1)) {
        Some(z) => z,
        None => return,
    };
    assert_eq!(z2.x, 999);
    assert_eq!(z2.w, 200);
}

#[test]
fn zone_id_invalid_is_sentinel_zero() {
    assert_eq!(ZoneId::INVALID, ZoneId(0));
    let zl = ZoneList::new();
    assert!(zl.is_empty());
}

#[test]
fn zone_visibility_defaults_visible_and_reports_changes() {
    let mut z = zone(7, 0);
    assert!(z.is_visible());
    assert!(z.set_visible(false));
    assert!(!z.is_visible());
    assert!(!z.set_visible(false));
    assert!(z.set_visible(true));
    assert!(z.is_visible());
}

#[test]
fn zone_bulk_metadata_defaults_and_reports_changes() {
    let mut z = zone(8, 0);
    assert!(!z.locked);
    assert!(z.alias.is_none());
    assert_eq!(z.display_title(), z.title.as_ref());
    assert!(z.display_mode.is_none());

    assert!(z.set_locked(true));
    assert!(!z.set_locked(true));
    assert!(z.locked);
    assert!(z.set_locked(false));
    assert!(!z.locked);

    assert!(z.set_alias(Some(Cow::Borrowed("Alias"))));
    assert!(!z.set_alias(Some(Cow::Borrowed("Alias"))));
    assert_eq!(z.alias.as_deref(), Some("Alias"));
    assert_eq!(z.display_title(), "Alias");
    assert!(z.set_alias(None));
    assert!(z.alias.is_none());
    assert_eq!(z.display_title(), z.title.as_ref());

    assert!(z.set_display_mode(Some(Cow::Borrowed("hover"))));
    assert!(!z.set_display_mode(Some(Cow::Borrowed("hover"))));
    assert_eq!(z.display_mode.as_deref(), Some("hover"));
    assert!(z.set_display_mode(None));
    assert!(z.display_mode.is_none());
}

fn ordered_ids(zl: &ZoneList) -> Vec<u64> {
    zl.iter().map(|z| z.id.0).collect()
}

#[test]
fn move_to_index_to_smaller_index_shifts_intervening_zones() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));
    zl.add(zone(4, 40));

    assert!(zl.move_to_index(ZoneId(4), 1));
    assert_eq!(
        ordered_ids(&zl),
        vec![1, 4, 2, 3],
        "moving id=4 to idx=1 must insert before former idx-1 zone"
    );
}

#[test]
fn move_to_index_to_larger_index_shifts_intervening_zones() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));
    zl.add(zone(4, 40));

    assert!(zl.move_to_index(ZoneId(1), 2));
    assert_eq!(
        ordered_ids(&zl),
        vec![2, 3, 1, 4],
        "moving id=1 to idx=2 must land it after the original idx-2 zone slot"
    );
}

#[test]
fn move_to_index_missing_id_returns_false_and_preserves_order() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));

    assert!(!zl.move_to_index(ZoneId(999), 0));
    assert_eq!(ordered_ids(&zl), vec![1, 2]);
}

#[test]
fn move_to_index_clamps_oversized_idx_to_last_slot() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));

    assert!(zl.move_to_index(ZoneId(1), 99));
    assert_eq!(
        ordered_ids(&zl),
        vec![2, 3, 1],
        "idx > len-1 must clamp the moved zone to the tail"
    );
}

#[test]
fn move_to_index_same_position_is_noop() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));

    assert!(zl.move_to_index(ZoneId(2), 1));
    assert_eq!(ordered_ids(&zl), vec![1, 2, 3]);
}

#[test]
fn move_to_index_on_single_element_list_is_idempotent() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));

    assert!(zl.move_to_index(ZoneId(1), 0));
    assert!(zl.move_to_index(ZoneId(1), 99));
    assert_eq!(ordered_ids(&zl), vec![1]);
}

#[test]
fn stack_folds_child_under_parent_and_unstack_releases_it() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));

    assert!(zl.stack(ZoneId(1), ZoneId(2)));
    assert!(matches!(zl.get(ZoneId(1)), Some(parent) if parent.is_stack_anchor()));
    assert!(matches!(zl.get(ZoneId(2)), Some(child) if child.is_stacked_child()));

    assert!(zl.unstack(ZoneId(2)));
    assert!(matches!(zl.get(ZoneId(1)), Some(parent) if !parent.is_stack_anchor()));
    assert!(matches!(zl.get(ZoneId(2)), Some(child) if !child.is_stacked_child()));
}

#[test]
fn stack_anchor_merge_flattens_the_whole_source_stack() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));
    zl.add(zone(4, 40));

    assert!(zl.stack(ZoneId(1), ZoneId(2)));
    assert!(zl.stack(ZoneId(1), ZoneId(3)));
    assert!(zl.stack(ZoneId(4), ZoneId(1)));

    assert_eq!(
        zl.stack_member_ids(ZoneId(4)).map(|ids| ids.into_vec()),
        Some(vec![ZoneId(4), ZoneId(1), ZoneId(2), ZoneId(3)])
    );
    for id in [ZoneId(1), ZoneId(2), ZoneId(3)] {
        assert_eq!(
            zl.get(id).and_then(|zone| zone.stack_parent),
            Some(ZoneId(4))
        );
    }
    assert!(matches!(zl.get(ZoneId(1)), Some(zone) if zone.stack_members.is_empty()));
    assert!(
        zl.iter()
            .all(|zone| !(zone.is_stacked_child() && zone.is_stack_anchor()))
    );
}

#[test]
fn stacking_onto_a_member_keeps_its_existing_anchor() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));

    assert!(zl.stack(ZoneId(1), ZoneId(2)));
    assert!(zl.stack(ZoneId(2), ZoneId(3)));
    assert_eq!(zl.stack_anchor_for(ZoneId(3)), Some(ZoneId(1)));
    assert_eq!(
        zl.stack_member_ids(ZoneId(1)).map(|ids| ids.into_vec()),
        Some(vec![ZoneId(1), ZoneId(2), ZoneId(3)])
    );
}

#[test]
fn stacking_members_that_already_share_an_anchor_is_a_noop() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));

    assert!(zl.stack(ZoneId(1), ZoneId(2)));
    assert!(zl.stack(ZoneId(1), ZoneId(3)));
    assert!(!zl.stack(ZoneId(2), ZoneId(3)));
    assert_eq!(
        zl.stack_member_ids(ZoneId(1)).map(|ids| ids.into_vec()),
        Some(vec![ZoneId(1), ZoneId(2), ZoneId(3)])
    );
}

#[test]
fn flatten_nested_stacks_repairs_legacy_hidden_tree_without_reordering() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));
    zl.add(zone(4, 40));
    zl.add(zone(5, 50));
    assert!(zl.stack(ZoneId(1), ZoneId(2)));
    assert!(zl.stack(ZoneId(1), ZoneId(3)));
    assert!(zl.stack(ZoneId(4), ZoneId(5)));

    // Legacy state: source anchor 1 was appended below anchor 4 but its
    // own children were left pointing at 1, creating a hidden tree.
    zl.get_mut(ZoneId(1)).unwrap().stack_parent = Some(ZoneId(4));
    zl.get_mut(ZoneId(4))
        .unwrap()
        .stack_members
        .insert(0, ZoneId(1));

    assert!(zl.flatten_nested_stacks());
    assert_eq!(
        zl.stack_member_ids(ZoneId(4)).map(|ids| ids.into_vec()),
        Some(vec![ZoneId(4), ZoneId(1), ZoneId(2), ZoneId(3), ZoneId(5)])
    );
    for id in [ZoneId(1), ZoneId(2), ZoneId(3), ZoneId(5)] {
        assert_eq!(
            zl.get(id).and_then(|zone| zone.stack_parent),
            Some(ZoneId(4))
        );
    }
    assert!(
        zl.iter()
            .all(|zone| !(zone.is_stacked_child() && zone.is_stack_anchor()))
    );
    assert!(!zl.flatten_nested_stacks());
}

#[test]
fn flatten_nested_stacks_promotes_a_cycle_root_instead_of_hiding_every_zone() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.get_mut(ZoneId(1)).unwrap().stack_parent = Some(ZoneId(2));
    zl.get_mut(ZoneId(1)).unwrap().stack_members.push(ZoneId(2));
    zl.get_mut(ZoneId(2)).unwrap().stack_parent = Some(ZoneId(1));
    zl.get_mut(ZoneId(2)).unwrap().stack_members.push(ZoneId(1));

    assert!(zl.flatten_nested_stacks());
    assert!(zl.iter().any(|zone| zone.stack_parent.is_none()));
    assert!(
        zl.iter()
            .all(|zone| !(zone.is_stacked_child() && zone.is_stack_anchor()))
    );
}

#[test]
fn move_group_to_preserves_every_stack_member_offset() {
    let mut zl = ZoneList::new();
    zl.add(Zone::new(ZoneId(1), "anchor", 100, 80, 120, 90));
    zl.add(Zone::new(ZoneId(2), "child-a", 140, 150, 120, 90));
    zl.add(Zone::new(ZoneId(3), "child-b", 70, 210, 120, 90));
    assert!(zl.stack(ZoneId(1), ZoneId(2)));
    assert!(zl.stack(ZoneId(1), ZoneId(3)));

    assert!(zl.move_group_to(ZoneId(1), 300, 330));
    assert_eq!(
        zl.get(ZoneId(1)).map(|zone| (zone.x, zone.y)),
        Some((300, 330))
    );
    assert_eq!(
        zl.get(ZoneId(2)).map(|zone| (zone.x, zone.y)),
        Some((340, 400))
    );
    assert_eq!(
        zl.get(ZoneId(3)).map(|zone| (zone.x, zone.y)),
        Some((270, 460))
    );
    assert!(!zl.move_group_to(ZoneId(1), 300, 330));
}

#[test]
fn move_group_to_moves_a_free_zone_without_touching_others() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));

    assert!(zl.move_group_to(ZoneId(1), 200, 240));
    assert_eq!(
        zl.get(ZoneId(1)).map(|zone| (zone.x, zone.y)),
        Some((200, 240))
    );
    assert_eq!(
        zl.get(ZoneId(2)).map(|zone| (zone.x, zone.y)),
        Some((20, 0))
    );
}

#[test]
fn unstack_anchor_releases_all_members() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));

    assert!(zl.stack(ZoneId(1), ZoneId(2)));
    assert!(zl.stack(ZoneId(1), ZoneId(3)));
    assert!(zl.unstack(ZoneId(1)));

    assert!(matches!(zl.get(ZoneId(1)), Some(parent) if parent.stack_members.is_empty()));
    assert!(matches!(zl.get(ZoneId(2)), Some(child) if child.stack_parent.is_none()));
    assert!(matches!(zl.get(ZoneId(3)), Some(child) if child.stack_parent.is_none()));
}

#[test]
fn dissolve_stack_scattered_releases_members_into_row() {
    let mut zl = ZoneList::new();
    zl.add(Zone::new(ZoneId(1), "anchor", 100, 80, 120, 90));
    zl.add(Zone::new(ZoneId(2), "child-a", 100, 80, 120, 90));
    zl.add(Zone::new(ZoneId(3), "child-b", 100, 80, 120, 90));

    assert!(zl.stack(ZoneId(1), ZoneId(2)));
    assert!(zl.stack(ZoneId(1), ZoneId(3)));
    assert!(zl.dissolve_stack_scattered(ZoneId(1), 640, 480));

    assert!(
        matches!(zl.get(ZoneId(1)), Some(zone) if zone.stack_parent.is_none() && zone.stack_members.is_empty())
    );
    assert!(matches!(zl.get(ZoneId(2)), Some(zone) if zone.stack_parent.is_none()));
    assert!(matches!(zl.get(ZoneId(3)), Some(zone) if zone.stack_parent.is_none()));
    assert_eq!(
        zl.get(ZoneId(1)).map(|zone| (zone.x, zone.y)),
        Some((100, 80))
    );
    assert_eq!(
        zl.get(ZoneId(2)).map(|zone| (zone.x, zone.y)),
        Some((100 + 120 + STACK_SCATTER_GAP_DIP, 80))
    );
    assert_eq!(
        zl.get(ZoneId(3)).map(|zone| (zone.x, zone.y)),
        Some((100 + (120 + STACK_SCATTER_GAP_DIP) * 2, 80))
    );
}

#[test]
fn dissolve_stack_scattered_wraps_and_clamps_near_right_edge() {
    let mut zl = ZoneList::new();
    zl.add(Zone::new(ZoneId(1), "anchor", 250, 40, 120, 80));
    zl.add(Zone::new(ZoneId(2), "child-a", 250, 40, 120, 80));
    zl.add(Zone::new(ZoneId(3), "child-b", 250, 40, 120, 80));

    assert!(zl.stack(ZoneId(1), ZoneId(2)));
    assert!(zl.stack(ZoneId(1), ZoneId(3)));
    assert!(zl.dissolve_stack_scattered(ZoneId(1), 320, 220));

    assert_eq!(
        zl.get(ZoneId(1)).map(|zone| (zone.x, zone.y)),
        Some((200, 40))
    );
    assert_eq!(
        zl.get(ZoneId(2)).map(|zone| (zone.x, zone.y)),
        Some((200, 40 + 80 + STACK_SCATTER_GAP_DIP))
    );
    assert_eq!(
        zl.get(ZoneId(3)).map(|zone| (zone.x, zone.y)),
        Some((200, 220 - 80))
    );
    for zone in zl.iter() {
        assert!(zone.x >= 0 && zone.y >= 0);
        assert!(zone.x + zone.w <= 320);
        assert!(zone.y + zone.h <= 220);
    }
}

#[test]
fn detach_from_stack_keeps_remaining_members_stacked() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));

    assert!(zl.stack(ZoneId(1), ZoneId(2)));
    assert!(zl.stack(ZoneId(1), ZoneId(3)));

    let outcome = zl
        .detach_from_stack(ZoneId(2))
        .expect("member should detach");

    assert_eq!(outcome.detached_member, ZoneId(2));
    assert_eq!(outcome.new_anchor, Some(ZoneId(1)));
    assert_eq!(outcome.remaining_count, 2);
    assert_eq!(zl.stack_anchor_for(ZoneId(3)), Some(ZoneId(1)));
    assert!(matches!(zl.get(ZoneId(2)), Some(child) if child.stack_parent.is_none()));
    assert_eq!(
        zl.stack_member_ids(ZoneId(1)).map(|ids| ids.into_vec()),
        Some(vec![ZoneId(1), ZoneId(3)])
    );
}

#[test]
fn detach_stack_anchor_promotes_remaining_member() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));

    assert!(zl.stack(ZoneId(1), ZoneId(2)));
    assert!(zl.stack(ZoneId(1), ZoneId(3)));

    let outcome = zl
        .detach_from_stack(ZoneId(1))
        .expect("anchor should detach");

    assert_eq!(outcome.detached_member, ZoneId(1));
    assert_eq!(outcome.new_anchor, Some(ZoneId(2)));
    assert_eq!(outcome.remaining_count, 2);
    assert!(
        matches!(zl.get(ZoneId(1)), Some(zone) if !zone.is_stacked_child() && !zone.is_stack_anchor())
    );
    assert_eq!(zl.stack_anchor_for(ZoneId(3)), Some(ZoneId(2)));
}

#[test]
fn detach_from_two_member_stack_dissolves_remainder() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));

    assert!(zl.stack(ZoneId(1), ZoneId(2)));

    let outcome = zl
        .detach_from_stack(ZoneId(2))
        .expect("member should detach");

    assert_eq!(outcome.new_anchor, None);
    assert_eq!(outcome.remaining_count, 1);
    assert!(matches!(zl.get(ZoneId(1)), Some(zone) if !zone.is_stack_anchor()));
    assert!(matches!(zl.get(ZoneId(2)), Some(zone) if !zone.is_stacked_child()));
}

#[test]
fn reorder_stack_member_changes_child_order_under_stable_anchor() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));
    zl.add(zone(4, 40));

    assert!(zl.stack(ZoneId(1), ZoneId(2)));
    assert!(zl.stack(ZoneId(1), ZoneId(3)));
    assert!(zl.stack(ZoneId(1), ZoneId(4)));

    assert!(zl.reorder_stack_member(ZoneId(1), ZoneId(4), 1));

    assert_eq!(
        zl.stack_member_ids(ZoneId(1)).map(|ids| ids.into_vec()),
        Some(vec![ZoneId(1), ZoneId(4), ZoneId(2), ZoneId(3)])
    );
}

#[test]
fn reorder_stack_member_rejects_anchor_and_foreign_member() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    zl.add(zone(3, 30));

    assert!(zl.stack(ZoneId(1), ZoneId(2)));

    assert!(!zl.reorder_stack_member(ZoneId(1), ZoneId(1), 1));
    assert!(!zl.reorder_stack_member(ZoneId(1), ZoneId(3), 1));
    assert_eq!(
        zl.stack_member_ids(ZoneId(1)).map(|ids| ids.into_vec()),
        Some(vec![ZoneId(1), ZoneId(2)])
    );
}

#[test]
fn display_name_strips_shortcut_suffix_only() {
    assert_eq!(display_name_for_path("C:/Desktop/App.lnk"), "App");
    assert_eq!(display_name_for_path("C:/Desktop/Site.URL"), "Site");
    assert_eq!(display_name_for_path("C:/Desktop/report.pdf"), "report.pdf");
}

#[test]
fn zone_defaults_include_appearance_fields() {
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("z"), 10, 20, 100, 80);
    assert_eq!(zone.icon.as_ref(), DEFAULT_ZONE_ICON);
    assert_eq!(zone.accent_color.as_deref(), None);
    assert_eq!(zone.grid_columns, DEFAULT_ZONE_GRID_COLUMNS);
    assert_eq!(zone.capsule_size.as_ref(), DEFAULT_ZONE_CAPSULE_SIZE);
    assert_eq!(zone.capsule_shape.as_ref(), DEFAULT_ZONE_CAPSULE_SHAPE);
}

#[test]
fn zone_items_add_move_remove_and_missing_state() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));

    let maybe_item_id = zl.add_item(
        ZoneId(1),
        Cow::Owned("C:/Desktop/report.pdf".to_owned()),
        Cow::Owned("abc".to_owned()),
    );
    assert!(maybe_item_id.is_some());
    let item_id = match maybe_item_id {
        Some(item_id) => item_id,
        None => return,
    };
    assert!(matches!(
        zl.get(ZoneId(1)),
        Some(zone)
            if zone.items.len() == 1
                && zone.items[0].name.as_ref() == "report.pdf"
                && zone.items[0].icon_hash.as_ref() == "abc"
    ));

    assert!(zl.move_item(ZoneId(1), item_id, 2, 3));
    assert!(matches!(
        zl.get(ZoneId(1)),
        Some(zone) if zone.items.first().map(|item| (item.x, item.y)) == Some((2, 3))
    ));

    assert!(zl.toggle_item_wide(ZoneId(1), item_id));
    assert!(matches!(
        zl.get(ZoneId(1)),
        Some(zone) if zone.items.first().map(|item| item.is_wide) == Some(true)
    ));

    zl.add(zone(2, 20));
    assert!(zl.move_item_to_zone(ZoneId(1), ZoneId(2), item_id, None, None));
    assert!(matches!(zl.get(ZoneId(1)), Some(zone) if zone.items.is_empty()));
    assert!(matches!(zl.get(ZoneId(2)), Some(zone) if zone.items.len() == 1));

    assert!(zl.mark_item_missing("C:/Desktop/report.pdf", true));
    assert!(matches!(
        zl.get(ZoneId(2)),
        Some(zone) if zone.items.first().map(|item| item.file_missing) == Some(true)
    ));

    assert!(zl.update_item_file_metadata(
        ZoneId(2),
        item_id,
        Cow::Owned("C:/Desktop/renamed.pdf".to_owned()),
        None,
        Some(Cow::Owned("C:/Original/renamed.pdf".to_owned())),
        Some(Cow::Owned("C:/Desktop/renamed.pdf".to_owned())),
    ));
    assert!(matches!(
        zl.item(ZoneId(2), item_id),
        Some(item)
            if item.name.as_ref() == "renamed.pdf"
                && item.path.as_ref() == "C:/Desktop/renamed.pdf"
                && item.original_path.as_deref() == Some("C:/Original/renamed.pdf")
                && item.hidden_path.as_deref() == Some("C:/Desktop/renamed.pdf")
                && !item.file_missing
    ));

    assert!(zl.remove_item(ZoneId(2), item_id));
    assert!(matches!(zl.get(ZoneId(2)), Some(zone) if zone.items.is_empty()));
}

#[test]
fn add_item_and_cross_zone_move_use_target_grid_columns() {
    let mut left = zone(1, 10);
    left.set_grid_columns(2);
    let mut right = zone(2, 20);
    right.set_grid_columns(3);
    let mut zl = ZoneList::new();
    zl.add(left);
    zl.add(right);

    let first = zl
        .add_item(
            ZoneId(1),
            Cow::Owned("C:/Desktop/one.txt".to_owned()),
            Cow::Owned("h1".to_owned()),
        )
        .expect("first");
    let second = zl
        .add_item(
            ZoneId(1),
            Cow::Owned("C:/Desktop/two.txt".to_owned()),
            Cow::Owned("h2".to_owned()),
        )
        .expect("second");
    let third = zl
        .add_item(
            ZoneId(1),
            Cow::Owned("C:/Desktop/three.txt".to_owned()),
            Cow::Owned("h3".to_owned()),
        )
        .expect("third");

    assert_eq!(
        zl.item(ZoneId(1), first).map(|item| (item.x, item.y)),
        Some((0, 0))
    );
    assert_eq!(
        zl.item(ZoneId(1), second).map(|item| (item.x, item.y)),
        Some((1, 0))
    );
    assert_eq!(
        zl.item(ZoneId(1), third).map(|item| (item.x, item.y)),
        Some((0, 1))
    );

    assert!(zl.move_item_to_zone(ZoneId(1), ZoneId(2), third, None, None));
    assert_eq!(
        zl.item(ZoneId(2), third).map(|item| (item.x, item.y)),
        Some((0, 0))
    );
}
