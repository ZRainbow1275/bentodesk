use super::*;
use bentodesk_zone::{
    DEFAULT_ZONE_CAPSULE_SHAPE, DEFAULT_ZONE_CAPSULE_SIZE, DEFAULT_ZONE_GRID_COLUMNS,
    DEFAULT_ZONE_ICON,
};

fn sample() -> ZoneList {
    let mut zl = ZoneList::new();
    zl.add(Zone::new(
        ZoneId(1),
        Cow::Borrowed("Alpha"),
        10,
        20,
        300,
        200,
    ));
    zl.add(Zone::new(
        ZoneId(2),
        Cow::Owned("Β-zone".to_owned()),
        -50,
        0,
        100,
        50,
    ));
    zl.add(Zone::new(
        ZoneId(0xFFFF_FFFF_FFFF_FFFF),
        Cow::Borrowed(""),
        0,
        0,
        1,
        1,
    ));
    zl
}

#[test]
fn state_dir_override_appends_zones_bin_and_ignores_blank_values() {
    assert!(state_dir_override_path_from_value(std::ffi::OsStr::new("   ")).is_none());

    let path = state_dir_override_path_from_value(std::ffi::OsStr::new(
        r" C:\Temp\bentodesk-isolated-state ",
    ))
    .expect("override path");

    assert_eq!(
        path,
        PathBuf::from(r"C:\Temp\bentodesk-isolated-state").join("zones.bin")
    );
}

#[test]
fn roundtrip_encode_decode_preserves_zones() {
    let zl = sample();
    let buf = encode(&zl);
    let res = decode(&buf);
    assert!(res.is_ok(), "decode must succeed: {:?}", res.as_ref().err());
    let back = match res {
        Ok(v) => v,
        Err(_) => return,
    };
    assert_eq!(back.len(), zl.len());
    for (a, b) in zl.iter().zip(back.iter()) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.title.as_ref(), b.title.as_ref());
        assert_eq!((a.x, a.y, a.w, a.h), (b.x, b.y, b.w, b.h));
        assert_eq!(a.visible, b.visible);
        assert_eq!(a.stack_parent, b.stack_parent);
        assert_eq!(a.stack_members, b.stack_members);
    }
}

#[test]
fn roundtrip_preserves_stack_relationships() {
    let mut zl = ZoneList::new();
    zl.add(Zone::new(
        ZoneId(1),
        Cow::Borrowed("Parent"),
        0,
        0,
        100,
        100,
    ));
    zl.add(Zone::new(
        ZoneId(2),
        Cow::Borrowed("Child"),
        10,
        10,
        100,
        100,
    ));
    assert!(zl.stack(ZoneId(1), ZoneId(2)));

    let back = decode(&encode(&zl)).expect("decode");
    let parent = back.get(ZoneId(1)).expect("parent");
    let child = back.get(ZoneId(2)).expect("child");
    assert_eq!(parent.stack_members.as_slice(), &[ZoneId(2)]);
    assert_eq!(child.stack_parent, Some(ZoneId(1)));
}

#[test]
fn decode_flattens_legacy_nested_stack_relationships() {
    let mut zl = ZoneList::new();
    zl.add(Zone::new(
        ZoneId(1),
        Cow::Borrowed("Legacy source"),
        0,
        0,
        100,
        100,
    ));
    zl.add(Zone::new(
        ZoneId(2),
        Cow::Borrowed("Legacy member"),
        10,
        10,
        100,
        100,
    ));
    zl.add(Zone::new(
        ZoneId(4),
        Cow::Borrowed("Target"),
        20,
        20,
        100,
        100,
    ));
    assert!(zl.stack(ZoneId(1), ZoneId(2)));
    zl.get_mut(ZoneId(1)).expect("source").stack_parent = Some(ZoneId(4));
    zl.get_mut(ZoneId(4))
        .expect("target")
        .stack_members
        .push(ZoneId(1));

    let back = decode(&encode(&zl)).expect("decode");

    assert_eq!(
        back.stack_member_ids(ZoneId(4)).map(|ids| ids.into_vec()),
        Some(vec![ZoneId(4), ZoneId(1), ZoneId(2)])
    );
    assert_eq!(
        back.get(ZoneId(2)).and_then(|zone| zone.stack_parent),
        Some(ZoneId(4))
    );
    assert!(back.get(ZoneId(1)).is_some_and(|zone| {
        zone.stack_parent == Some(ZoneId(4)) && zone.stack_members.is_empty()
    }));
}

#[test]
fn roundtrip_preserves_zone_items() {
    let mut zl = ZoneList::new();
    zl.add(Zone::new(ZoneId(1), Cow::Borrowed("Items"), 0, 0, 200, 120));
    let item_id = zl
        .add_item(
            ZoneId(1),
            Cow::Owned("C:/Desktop/App.lnk".to_owned()),
            Cow::Owned("hash-1".to_owned()),
        )
        .expect("item id");
    assert!(zl.move_item(ZoneId(1), item_id, 2, 3));
    assert!(zl.mark_item_missing("C:/Desktop/App.lnk", true));

    let back = decode(&encode(&zl)).expect("decode");
    let zone = back.get(ZoneId(1)).expect("zone");
    assert_eq!(zone.items.len(), 1);
    let item = &zone.items[0];
    assert_eq!(item.id.0, item_id.0);
    assert_eq!(item.name.as_ref(), "App");
    assert_eq!(item.path.as_ref(), "C:/Desktop/App.lnk");
    assert_eq!(item.icon_hash.as_ref(), "hash-1");
    assert_eq!((item.x, item.y), (2, 3));
    assert!(item.file_missing);
    assert_eq!(item.original_path.as_deref(), None);
    assert_eq!(item.hidden_path.as_deref(), None);
    assert!(item.tags.is_empty());
}

#[test]
fn roundtrip_preserves_zone_appearance_fields() {
    let mut zone = Zone::new(ZoneId(1), Cow::Borrowed("Styled"), 0, 0, 240, 160);
    zone.set_icon(Cow::Borrowed("folder_open"));
    zone.set_accent_color(Some(Cow::Borrowed("#3b82f6")));
    zone.set_grid_columns(6);
    zone.set_capsule(Cow::Borrowed("large"), Cow::Borrowed("rounded"));
    let mut zl = ZoneList::new();
    zl.add(zone);

    let back = decode(&encode(&zl)).expect("decode");
    let zone = back.get(ZoneId(1)).expect("zone");
    assert_eq!(zone.icon.as_ref(), "folder_open");
    assert_eq!(zone.accent_color.as_deref(), Some("#3b82f6"));
    assert_eq!(zone.grid_columns, 6);
    assert_eq!(zone.capsule_size.as_ref(), "large");
    assert_eq!(zone.capsule_shape.as_ref(), "rounded");
}

#[test]
fn roundtrip_preserves_zone_visibility() {
    let mut hidden = Zone::new(ZoneId(2), Cow::Borrowed("Hidden"), 8, 8, 120, 80);
    hidden.set_visible(false);
    let mut visible = Zone::new(ZoneId(3), Cow::Borrowed("Visible"), 12, 12, 120, 80);
    visible.set_visible(true);
    let mut zl = ZoneList::new();
    zl.add(hidden);
    zl.add(visible);

    let back = decode(&encode(&zl)).expect("decode");
    assert!(!back.get(ZoneId(2)).expect("hidden zone").visible);
    assert!(back.get(ZoneId(3)).expect("visible zone").visible);
}

#[test]
fn roundtrip_preserves_zone_bulk_metadata() {
    let mut zone = Zone::new(ZoneId(5), Cow::Borrowed("Bulk"), 8, 8, 120, 80);
    zone.set_visible(false);
    zone.set_locked(true);
    zone.set_alias(Some(Cow::Borrowed("Trimmed alias")));
    zone.set_display_mode(Some(Cow::Borrowed("click")));
    zone.set_live_folder_path(Some(Cow::Borrowed("C:/Users/BentoDeskTest/Documents/Live")));
    let mut zl = ZoneList::new();
    zl.add(zone);

    let back = decode(&encode(&zl)).expect("decode");
    let zone = back.get(ZoneId(5)).expect("zone");
    assert!(!zone.visible);
    assert!(zone.locked);
    assert_eq!(zone.alias.as_deref(), Some("Trimmed alias"));
    assert_eq!(zone.display_mode.as_deref(), Some("click"));
    assert_eq!(
        zone.live_folder_path.as_deref(),
        Some("C:/Users/BentoDeskTest/Documents/Live")
    );
}

#[test]
fn roundtrip_preserves_hidden_item_paths() {
    let mut zl = ZoneList::new();
    zl.add(Zone::new(ZoneId(1), Cow::Borrowed("Items"), 0, 0, 200, 120));
    let item_id = zl
        .add_item_with_metadata(
            ZoneId(1),
            Cow::Owned("C:/Users/BentoDeskTest/Desktop/.bentodesk/1/App.lnk".to_owned()),
            Some("C:/Users/BentoDeskTest/Desktop/App.lnk"),
            Cow::Owned("hash-1".to_owned()),
            Some(Cow::Owned(
                "C:/Users/BentoDeskTest/Desktop/App.lnk".to_owned(),
            )),
            Some(Cow::Owned(
                "C:/Users/BentoDeskTest/Desktop/.bentodesk/1/App.lnk".to_owned(),
            )),
        )
        .expect("item id");

    let back = decode(&encode(&zl)).expect("decode");
    let item = back.item(ZoneId(1), item_id).expect("item");
    assert_eq!(item.name.as_ref(), "App");
    assert_eq!(
        item.original_path.as_deref(),
        Some("C:/Users/BentoDeskTest/Desktop/App.lnk")
    );
    assert_eq!(
        item.hidden_path.as_deref(),
        Some("C:/Users/BentoDeskTest/Desktop/.bentodesk/1/App.lnk")
    );
}

#[test]
fn roundtrip_preserves_item_tags() {
    let mut zl = ZoneList::new();
    zl.add(Zone::new(
        ZoneId(1),
        Cow::Borrowed("Tagged"),
        0,
        0,
        200,
        120,
    ));
    let item_id = zl
        .add_item(
            ZoneId(1),
            Cow::Owned("C:/Users/BentoDeskTest/Desktop/Contract.pdf".to_owned()),
            Cow::Owned("hash-1".to_owned()),
        )
        .expect("item id");
    {
        let item = zl
            .get_mut(ZoneId(1))
            .expect("zone")
            .items
            .iter_mut()
            .find(|item| item.id == item_id)
            .expect("item");
        item.tags.push(Cow::Borrowed("urgent"));
        item.tags.push(Cow::Borrowed("client-a"));
    }

    let back = decode(&encode(&zl)).expect("decode");
    let item = back.item(ZoneId(1), item_id).expect("item");
    let tags: Vec<&str> = item.tags.iter().map(|tag| tag.as_ref()).collect();
    assert_eq!(tags, vec!["urgent", "client-a"]);
}

#[test]
fn decode_v1_zone_defaults_stack_fields() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&MAGIC);
    buf.extend_from_slice(&VERSION_V1.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&7u64.to_le_bytes());
    buf.extend_from_slice(&4u16.to_le_bytes());
    buf.extend_from_slice(b"Zone");
    buf.extend_from_slice(&1i32.to_le_bytes());
    buf.extend_from_slice(&2i32.to_le_bytes());
    buf.extend_from_slice(&3i32.to_le_bytes());
    buf.extend_from_slice(&4i32.to_le_bytes());

    let zones = decode(&buf).expect("v1 decode");
    let zone = zones.get(ZoneId(7)).expect("zone");
    assert!(zone.visible);
    assert!(!zone.locked);
    assert!(zone.alias.is_none());
    assert!(zone.display_mode.is_none());
    assert!(zone.live_folder_path.is_none());
    assert_eq!(zone.stack_parent, None);
    assert!(zone.stack_members.is_empty());
    assert!(zone.items.is_empty());
}

#[test]
fn decode_clamps_corrupt_oversized_zone_geometry() {
    // Reproduces the legacy corruption: a zone persisted with
    // `w=170667 h=91200` (= logical-viewport ×100) auto-expanded into a
    // full-screen click-eating veil. `decode` must reset BOTH dims to the
    // sane default so the load can never brick the UI.
    let mut buf = Vec::new();
    buf.extend_from_slice(&MAGIC);
    buf.extend_from_slice(&VERSION_V1.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&42u64.to_le_bytes());
    buf.extend_from_slice(&4u16.to_le_bytes());
    buf.extend_from_slice(b"Zone");
    buf.extend_from_slice(&11i32.to_le_bytes()); // x preserved
    buf.extend_from_slice(&22i32.to_le_bytes()); // y preserved
    buf.extend_from_slice(&170_667i32.to_le_bytes()); // corrupt w
    buf.extend_from_slice(&91_200i32.to_le_bytes()); // corrupt h

    let zones = decode(&buf).expect("decode corrupt blob");
    let zone = zones.get(ZoneId(42)).expect("zone");
    assert_eq!(zone.x, 11, "x must be preserved");
    assert_eq!(zone.y, 22, "y must be preserved");
    assert_eq!(zone.w, super::DEFAULT_ZONE_W, "corrupt w must reset");
    assert_eq!(zone.h, super::DEFAULT_ZONE_H, "corrupt h must reset");
    assert!(zone.w <= super::MAX_ZONE_DIMENSION);
    assert!(zone.h <= super::MAX_ZONE_DIMENSION);
}

#[test]
fn decode_clamps_nonpositive_zone_geometry() {
    // A zero/negative dimension is equally fatal (degenerate body) — reset
    // BOTH dims to the sane default.
    let mut buf = Vec::new();
    buf.extend_from_slice(&MAGIC);
    buf.extend_from_slice(&VERSION_V1.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&7u64.to_le_bytes());
    buf.extend_from_slice(&4u16.to_le_bytes());
    buf.extend_from_slice(b"Zone");
    buf.extend_from_slice(&0i32.to_le_bytes());
    buf.extend_from_slice(&0i32.to_le_bytes());
    buf.extend_from_slice(&0i32.to_le_bytes()); // w = 0
    buf.extend_from_slice(&(-5i32).to_le_bytes()); // h < 0

    let zones = decode(&buf).expect("decode degenerate blob");
    let zone = zones.get(ZoneId(7)).expect("zone");
    assert_eq!(zone.w, super::DEFAULT_ZONE_W);
    assert_eq!(zone.h, super::DEFAULT_ZONE_H);
}

#[test]
fn decode_keeps_in_range_geometry_intact() {
    // A legitimately-sized zone must pass through untouched.
    let mut buf = Vec::new();
    buf.extend_from_slice(&MAGIC);
    buf.extend_from_slice(&VERSION_V1.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&5u64.to_le_bytes());
    buf.extend_from_slice(&4u16.to_le_bytes());
    buf.extend_from_slice(b"Zone");
    buf.extend_from_slice(&30i32.to_le_bytes());
    buf.extend_from_slice(&40i32.to_le_bytes());
    buf.extend_from_slice(&512i32.to_le_bytes());
    buf.extend_from_slice(&384i32.to_le_bytes());

    let zones = decode(&buf).expect("decode in-range blob");
    let zone = zones.get(ZoneId(5)).expect("zone");
    assert_eq!(zone.w, 512, "in-range w must be preserved");
    assert_eq!(zone.h, 384, "in-range h must be preserved");
}

fn encode_legacy_zone_without_appearance(version: u16) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&MAGIC);
    buf.extend_from_slice(&version.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&7u64.to_le_bytes());
    buf.extend_from_slice(&4u16.to_le_bytes());
    buf.extend_from_slice(b"Zone");
    buf.extend_from_slice(&1i32.to_le_bytes());
    buf.extend_from_slice(&2i32.to_le_bytes());
    buf.extend_from_slice(&3i32.to_le_bytes());
    buf.extend_from_slice(&4i32.to_le_bytes());
    if version >= VERSION_V2 {
        buf.extend_from_slice(&ZoneId::INVALID.0.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
    }
    if version >= VERSION_V3 {
        buf.extend_from_slice(&0u16.to_le_bytes());
    }
    buf
}

fn encode_v7_zone_with_visibility(hidden: bool) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&MAGIC);
    buf.extend_from_slice(&VERSION_V7.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&7u64.to_le_bytes());
    buf.extend_from_slice(&4u16.to_le_bytes());
    buf.extend_from_slice(b"Zone");
    buf.extend_from_slice(&1i32.to_le_bytes());
    buf.extend_from_slice(&2i32.to_le_bytes());
    buf.extend_from_slice(&3i32.to_le_bytes());
    buf.extend_from_slice(&4i32.to_le_bytes());
    buf.push(if hidden { 0b0000_0001 } else { 0 });
    push_utf8_field(&mut buf, DEFAULT_ZONE_ICON, MAX_ITEM_STRING_BYTES);
    push_optional_utf8_field(&mut buf, None, MAX_ITEM_STRING_BYTES);
    buf.extend_from_slice(&DEFAULT_ZONE_GRID_COLUMNS.to_le_bytes());
    push_utf8_field(&mut buf, DEFAULT_ZONE_CAPSULE_SIZE, MAX_ITEM_STRING_BYTES);
    push_utf8_field(&mut buf, DEFAULT_ZONE_CAPSULE_SHAPE, MAX_ITEM_STRING_BYTES);
    buf.extend_from_slice(&ZoneId::INVALID.0.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf
}

#[test]
fn decode_v1_to_v4_zone_defaults_appearance_fields() {
    for version in [VERSION_V1, VERSION_V2, VERSION_V3, VERSION_V4] {
        let zones = decode(&encode_legacy_zone_without_appearance(version)).expect("decode");
        let zone = zones.get(ZoneId(7)).expect("zone");
        assert_eq!(zone.icon.as_ref(), DEFAULT_ZONE_ICON, "version={version}");
        assert_eq!(zone.accent_color.as_deref(), None, "version={version}");
        assert_eq!(
            zone.grid_columns, DEFAULT_ZONE_GRID_COLUMNS,
            "version={version}"
        );
        assert_eq!(
            zone.capsule_size.as_ref(),
            DEFAULT_ZONE_CAPSULE_SIZE,
            "version={version}"
        );
        assert_eq!(
            zone.capsule_shape.as_ref(),
            DEFAULT_ZONE_CAPSULE_SHAPE,
            "version={version}"
        );
        assert!(!zone.locked, "version={version}");
        assert!(zone.alias.is_none(), "version={version}");
        assert!(zone.display_mode.is_none(), "version={version}");
        assert!(zone.live_folder_path.is_none(), "version={version}");
    }
}

#[test]
fn decode_v7_zone_defaults_bulk_metadata_but_preserves_visibility() {
    let zones = decode(&encode_v7_zone_with_visibility(true)).expect("v7 decode");
    let zone = zones.get(ZoneId(7)).expect("zone");
    assert!(!zone.visible);
    assert!(!zone.locked);
    assert!(zone.alias.is_none());
    assert!(zone.display_mode.is_none());
    assert!(zone.live_folder_path.is_none());
}

fn encode_v8_zone_with_bulk_metadata() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&MAGIC);
    buf.extend_from_slice(&VERSION_V8.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&8u64.to_le_bytes());
    buf.extend_from_slice(&4u16.to_le_bytes());
    buf.extend_from_slice(b"Zone");
    buf.extend_from_slice(&1i32.to_le_bytes());
    buf.extend_from_slice(&2i32.to_le_bytes());
    buf.extend_from_slice(&3i32.to_le_bytes());
    buf.extend_from_slice(&4i32.to_le_bytes());
    buf.push(0b0000_0010);
    push_optional_utf8_field(&mut buf, Some("Alias"), MAX_ITEM_STRING_BYTES);
    push_optional_utf8_field(&mut buf, Some("click"), MAX_ITEM_STRING_BYTES);
    push_utf8_field(&mut buf, DEFAULT_ZONE_ICON, MAX_ITEM_STRING_BYTES);
    push_optional_utf8_field(&mut buf, None, MAX_ITEM_STRING_BYTES);
    buf.extend_from_slice(&DEFAULT_ZONE_GRID_COLUMNS.to_le_bytes());
    push_utf8_field(&mut buf, DEFAULT_ZONE_CAPSULE_SIZE, MAX_ITEM_STRING_BYTES);
    push_utf8_field(&mut buf, DEFAULT_ZONE_CAPSULE_SHAPE, MAX_ITEM_STRING_BYTES);
    buf.extend_from_slice(&ZoneId::INVALID.0.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf
}

#[test]
fn decode_v8_zone_defaults_live_folder_but_preserves_bulk_metadata() {
    let zones = decode(&encode_v8_zone_with_bulk_metadata()).expect("v8 decode");
    let zone = zones.get(ZoneId(8)).expect("zone");
    assert!(zone.visible);
    assert!(zone.locked);
    assert_eq!(zone.alias.as_deref(), Some("Alias"));
    assert_eq!(zone.display_mode.as_deref(), Some("click"));
    assert!(zone.live_folder_path.is_none());
}

#[test]
fn decode_v3_item_defaults_hidden_paths() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&MAGIC);
    buf.extend_from_slice(&VERSION_V3.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&3u64.to_le_bytes());
    buf.extend_from_slice(&5u16.to_le_bytes());
    buf.extend_from_slice(b"Items");
    buf.extend_from_slice(&0i32.to_le_bytes());
    buf.extend_from_slice(&0i32.to_le_bytes());
    buf.extend_from_slice(&200i32.to_le_bytes());
    buf.extend_from_slice(&120i32.to_le_bytes());
    buf.extend_from_slice(&ZoneId::INVALID.0.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&1u64.to_le_bytes());
    push_utf8_field(&mut buf, "C:/Desktop/a.txt", MAX_ITEM_STRING_BYTES);
    push_utf8_field(&mut buf, "a.txt", MAX_ITEM_STRING_BYTES);
    push_utf8_field(&mut buf, "hash", MAX_ITEM_STRING_BYTES);
    buf.extend_from_slice(&0i32.to_le_bytes());
    buf.extend_from_slice(&0i32.to_le_bytes());
    buf.push(0u8);

    let zones = decode(&buf).expect("v3 decode");
    let item = zones.item(ZoneId(3), ZoneItemId(1)).expect("item");
    assert_eq!(item.original_path.as_deref(), None);
    assert_eq!(item.hidden_path.as_deref(), None);
}

#[test]
fn decode_v2_zone_defaults_item_list() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&MAGIC);
    buf.extend_from_slice(&VERSION_V2.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&9u64.to_le_bytes());
    buf.extend_from_slice(&5u16.to_le_bytes());
    buf.extend_from_slice(b"Stack");
    buf.extend_from_slice(&1i32.to_le_bytes());
    buf.extend_from_slice(&2i32.to_le_bytes());
    buf.extend_from_slice(&3i32.to_le_bytes());
    buf.extend_from_slice(&4i32.to_le_bytes());
    buf.extend_from_slice(&ZoneId::INVALID.0.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());

    let zones = decode(&buf).expect("v2 decode");
    let zone = zones.get(ZoneId(9)).expect("zone");
    assert!(zone.stack_members.is_empty());
    assert!(zone.items.is_empty());
}

#[test]
fn read_zones_returns_empty_when_file_absent() {
    let mut p = std::env::temp_dir();
    p.push("bentodesk-test-missing.bin");
    // Make sure it isn't there.
    let _ = fs::remove_file(&p);
    let res = read_zones(&p);
    assert!(
        res.is_ok(),
        "missing file must yield Ok(empty), got {:?}",
        res.err()
    );
    let zl = match res {
        Ok(v) => v,
        Err(_) => return,
    };
    assert!(zl.is_empty());
}

#[test]
fn decode_corrupt_magic_is_storage_err() {
    let mut buf = encode(&sample());
    buf[0] = b'X';
    let res = decode(&buf);
    assert!(
        matches!(res, Err(PlatformError::Storage("magic mismatch"))),
        "expected Storage(magic mismatch), got {res:?}"
    );
}

#[test]
fn decode_version_mismatch_is_storage_err() {
    let mut buf = encode(&sample());
    buf[4] = 99; // version low byte
    buf[5] = 0;
    let res = decode(&buf);
    assert!(
        matches!(res, Err(PlatformError::Storage("version unsupported"))),
        "expected Storage(version unsupported), got {res:?}"
    );
}

#[test]
fn decode_truncated_buffer_is_storage_err() {
    let buf = encode(&sample());
    // Lop off the last byte — the inner cursor must report underrun.
    let res = decode(&buf[..buf.len() - 1]);
    assert!(
        matches!(res, Err(PlatformError::Storage(_))),
        "expected Storage underrun, got {res:?}"
    );
}

#[test]
fn write_then_read_atomic_roundtrips() {
    let mut p = std::env::temp_dir();
    p.push("bentodesk-test-rt.bin");
    let _ = fs::remove_file(&p);

    let zl = sample();
    let wres = write_zones_atomic(&p, &zl);
    assert!(wres.is_ok(), "write_zones_atomic failed: {:?}", wres.err());

    let rres = read_zones(&p);
    assert!(rres.is_ok(), "read_zones failed: {:?}", rres.as_ref().err());
    let back = match rres {
        Ok(v) => v,
        Err(_) => return,
    };
    assert_eq!(back.len(), zl.len());
    let _ = fs::remove_file(&p);
}

/// Wave G1 (2026-05-20) — the migration helper itself: stale
/// `Some("always")` is rewritten to `None`; other values
/// (`Some("hover")`, `Some("custom")`, already-`None`) are untouched;
/// `changed` reports correctly so callers can skip the write-back on
/// no-op loads.
#[test]
fn migrate_stale_display_modes_rewrites_only_always_to_none() {
    let mut zones = ZoneList::new();
    let mut z_always = Zone::new(ZoneId(1), Cow::Borrowed("stale-always"), 0, 0, 100, 100);
    z_always.set_display_mode(Some(Cow::Borrowed("always")));
    let mut z_hover = Zone::new(ZoneId(2), Cow::Borrowed("hover-keep"), 0, 0, 100, 100);
    z_hover.set_display_mode(Some(Cow::Borrowed("hover")));
    let mut z_custom = Zone::new(ZoneId(3), Cow::Borrowed("custom-keep"), 0, 0, 100, 100);
    z_custom.set_display_mode(Some(Cow::Borrowed("custom")));
    let z_none = Zone::new(ZoneId(4), Cow::Borrowed("already-none"), 0, 0, 100, 100);
    zones.add(z_always);
    zones.add(z_hover);
    zones.add(z_custom);
    zones.add(z_none);

    let changed = migrate_stale_display_modes(&mut zones);
    assert!(
        changed,
        "migration must report a change when 'always' present"
    );

    let modes: Vec<Option<String>> = zones
        .iter()
        .map(|z| z.display_mode.as_deref().map(|s| s.to_owned()))
        .collect();
    assert_eq!(
        modes,
        vec![
            None,
            Some("hover".to_owned()),
            Some("custom".to_owned()),
            None,
        ]
    );

    // Idempotent — a second pass reports no change.
    let again = migrate_stale_display_modes(&mut zones);
    assert!(!again, "migration must be no-op when nothing is stale");
}

/// Wave G1 — end-to-end: a zones.bin with a stale `display_mode =
/// "always"` is migrated when loaded through `read_zones`, AND the
/// cleaned state is persisted back to disk so subsequent loads see
/// the migrated value without re-running the migration. Mirrors the
/// user's hand-test scenario where Zone 5 in the seed file refused to
/// collapse to a pill on hover-leave.
#[test]
fn loaded_zones_with_always_mode_are_migrated_to_none() {
    let mut p = std::env::temp_dir();
    p.push("bentodesk-test-display-mode-migration.bin");
    let _ = fs::remove_file(&p);

    // Seed: encode a ZoneList that has a stale "always" zone.
    let mut zones = ZoneList::new();
    let mut zone = Zone::new(ZoneId(5), Cow::Borrowed("Stale Zone 5"), 64, 72, 320, 220);
    zone.set_display_mode(Some(Cow::Borrowed("always")));
    zones.add(zone);

    let wres = write_zones_atomic(&p, &zones);
    assert!(wres.is_ok(), "seed write failed: {:?}", wres.err());

    // Load — expect display_mode to be None after migration.
    let loaded = read_zones(&p).expect("load seeded zones.bin");
    assert_eq!(loaded.len(), 1);
    let migrated = loaded.iter().next().expect("one zone in loaded list");
    assert!(
        migrated.display_mode.is_none(),
        "stale 'always' must be migrated to None, got {:?}",
        migrated.display_mode
    );

    // Second load — the write-back during the first load means the
    // on-disk file no longer has "always", so the second load should
    // also produce None and should NOT trigger another migration.
    let reloaded = read_zones(&p).expect("reload after write-back");
    let z2 = reloaded.iter().next().expect("one zone reloaded");
    assert!(
        z2.display_mode.is_none(),
        "persisted file must be clean after first migration, got {:?}",
        z2.display_mode
    );

    let _ = fs::remove_file(&p);
}
