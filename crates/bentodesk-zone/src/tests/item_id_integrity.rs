use super::*;

#[test]
fn cross_zone_move_remints_zone_scoped_item_id_before_mutating_source() {
    let mut zl = ZoneList::new();
    zl.add(zone(1, 10));
    zl.add(zone(2, 20));
    let source_id = zl
        .add_item(
            ZoneId(1),
            Cow::Borrowed("C:/Desktop/moved.txt"),
            Cow::Borrowed("moved"),
        )
        .expect("source item");
    let resident_id = zl
        .add_item(
            ZoneId(2),
            Cow::Borrowed("C:/Desktop/resident.txt"),
            Cow::Borrowed("resident"),
        )
        .expect("resident item");
    assert_eq!(source_id, resident_id);

    assert!(zl.move_item_to_zone(ZoneId(1), ZoneId(2), source_id, None, None, None));
    let target = zl.get(ZoneId(2)).expect("target zone");
    assert_eq!(target.items.len(), 2);
    assert_ne!(target.items[0].id, target.items[1].id);
    assert_eq!(
        target
            .items
            .iter()
            .find(|item| item.path.as_ref() == "C:/Desktop/resident.txt")
            .map(|item| item.id),
        Some(resident_id)
    );
    assert_eq!(
        target
            .items
            .iter()
            .find(|item| item.path.as_ref() == "C:/Desktop/moved.txt")
            .map(|item| item.id),
        Some(ZoneItemId(2))
    );
}

#[test]
fn duplicate_item_id_repair_preserves_first_and_is_idempotent() {
    let mut zone = zone(1, 10);
    zone.items.push(ZoneItem::new(
        ZoneItemId(1),
        Cow::Borrowed("C:/Desktop/first.txt"),
        Cow::Borrowed("first"),
        0,
        0,
    ));
    zone.items.push(ZoneItem::new(
        ZoneItemId(1),
        Cow::Borrowed("C:/Desktop/duplicate.txt"),
        Cow::Borrowed("duplicate"),
        1,
        0,
    ));
    let mut zl = ZoneList::new();
    zl.add(zone);

    assert_eq!(zl.repair_duplicate_item_ids(), 1);
    let zone = zl.get(ZoneId(1)).expect("zone");
    assert_eq!(zone.items[0].id, ZoneItemId(1));
    assert_eq!(zone.items[1].id, ZoneItemId(2));
    assert_eq!(zl.repair_duplicate_item_ids(), 0);
}

#[test]
fn cross_zone_id_overflow_leaves_source_untouched() {
    let mut source = zone(1, 10);
    source.items.push(ZoneItem::new(
        ZoneItemId(1),
        Cow::Borrowed("C:/Desktop/source.txt"),
        Cow::Borrowed("source"),
        0,
        0,
    ));
    let mut target = zone(2, 20);
    target.items.push(ZoneItem::new(
        ZoneItemId(1),
        Cow::Borrowed("C:/Desktop/collision.txt"),
        Cow::Borrowed("collision"),
        0,
        0,
    ));
    target.items.push(ZoneItem::new(
        ZoneItemId(u64::MAX),
        Cow::Borrowed("C:/Desktop/max.txt"),
        Cow::Borrowed("max"),
        1,
        0,
    ));
    let mut zl = ZoneList::new();
    zl.add(source);
    zl.add(target);

    assert!(!zl.can_move_item_to_zone(ZoneId(1), ZoneId(2), ZoneItemId(1)));
    assert!(!zl.move_item_to_zone(ZoneId(1), ZoneId(2), ZoneItemId(1), None, None, None,));
    assert!(zl.item(ZoneId(1), ZoneItemId(1)).is_some());
}
