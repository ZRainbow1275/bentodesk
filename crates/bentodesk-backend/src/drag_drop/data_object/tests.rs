use super::*;

fn read_preferred_drop_effect(data_object: &IDataObject) -> DROPEFFECT {
    let format = FORMATETC {
        cfFormat: preferred_drop_effect_format(),
        ptd: core::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    // SAFETY: `format` is fully initialized and remains live for the call.
    let mut medium = unsafe { data_object.GetData(&format) }.expect("preferred drop effect");
    // SAFETY: GetData returned TYMED_HGLOBAL for the requested format. The
    // allocation contains exactly one DWORD and remains locked until read.
    let value = unsafe {
        assert_eq!(medium.tymed, TYMED_HGLOBAL.0 as u32);
        let hglobal = medium.u.hGlobal;
        let raw = GlobalLock(hglobal);
        assert!(!raw.is_null());
        let value = raw.cast::<u32>().read();
        let _ = GlobalUnlock(hglobal);
        value
    };
    // SAFETY: this test owns the STGMEDIUM returned by GetData.
    unsafe { ReleaseStgMedium(&mut medium) };
    DROPEFFECT(value)
}

fn set_feedback_drop_effect(data_object: &IDataObject, format_id: u16, effect: DROPEFFECT) {
    let format = FORMATETC {
        cfFormat: format_id,
        ptd: core::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    // SAFETY: build_drop_effect returns an owned movable HGLOBAL. SetData takes
    // ownership because `frelease=true` and releases it exactly once.
    let hglobal = unsafe { BentoDataObject::build_drop_effect(effect) }.expect("feedback HGLOBAL");
    let medium = STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: STGMEDIUM_0 { hGlobal: hglobal },
        pUnkForRelease: std::mem::ManuallyDrop::new(None),
    };
    // SAFETY: `format` and `medium` remain valid for the COM call.
    unsafe { data_object.SetData(&format, &medium, true) }.expect("set feedback effect");
}

#[test]
fn enumerates_file_drop_as_primary_format() {
    let preferred_drop_effect = preferred_drop_effect_format();

    let first = supported_format_at(0).expect("first format");
    let second = supported_format_at(1).expect("second format");
    let third = supported_format_at(2).expect("third format");
    let fourth = supported_format_at(3).expect("fourth format");
    let fifth = supported_format_at(4).expect("fifth format");
    assert_eq!(first.cfFormat, CF_HDROP.0);
    assert_eq!(second.cfFormat, shell_id_list_array_format());
    assert_eq!(third.cfFormat, shell_object_offsets_format());
    assert_eq!(fourth.cfFormat, preferred_drop_effect);
    assert_eq!(fifth.cfFormat, in_shell_drag_loop_format());
    assert!(supported_format_at(5).is_none());

    let preferred = FORMATETC {
        cfFormat: preferred_drop_effect,
        ptd: core::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    assert!(format_is_supported(&preferred));

    let drag_context = FORMATETC {
        cfFormat: drag_context_format(),
        ptd: core::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_ISTREAM.0 as u32,
    };
    assert!(format_is_supported(&drag_context));
}

#[test]
fn builds_shell_id_list_array_cida_with_desktop_parent() {
    let child_one = vec![4, 0, 1, 2, 0, 0];
    let child_two = vec![4, 0, 3, 4, 0, 0];
    let cida = build_cida_bytes(&[child_one.clone(), child_two.clone()]).expect("cida");

    let child_count = u32::from_ne_bytes(cida[0..4].try_into().expect("count bytes"));
    let parent_offset = u32::from_ne_bytes(cida[4..8].try_into().expect("parent offset"));
    let child_one_offset = u32::from_ne_bytes(cida[8..12].try_into().expect("child offset"));
    let child_two_offset = u32::from_ne_bytes(cida[12..16].try_into().expect("child offset"));

    assert_eq!(child_count, 2);
    assert_eq!(parent_offset, 16);
    assert_eq!(
        &cida[parent_offset as usize..child_one_offset as usize],
        [0, 0]
    );
    assert_eq!(
        &cida[child_one_offset as usize..child_two_offset as usize],
        child_one.as_slice()
    );
    assert_eq!(&cida[child_two_offset as usize..], child_two.as_slice());
}

#[test]
fn custom_data_object_reports_explicit_move_or_copy_preference() {
    let default_object: IDataObject =
        BentoDataObject::new(vec![r"C:\BentoDesk\default.txt".to_owned()]).into();
    let move_object: IDataObject = BentoDataObject::with_preferred_effect(
        vec![r"C:\BentoDesk\move.txt".to_owned()],
        DROPEFFECT_MOVE,
    )
    .into();
    let copy_object: IDataObject = BentoDataObject::with_preferred_effect(
        vec![r"C:\BentoDesk\copy.txt".to_owned()],
        DROPEFFECT_COPY,
    )
    .into();

    assert_eq!(read_preferred_drop_effect(&default_object), DROPEFFECT_MOVE);
    assert_eq!(read_preferred_drop_effect(&move_object), DROPEFFECT_MOVE);
    assert_eq!(read_preferred_drop_effect(&copy_object), DROPEFFECT_COPY);
}

#[test]
fn custom_data_object_round_trips_shell_performed_effect_feedback() {
    let data_object: IDataObject =
        BentoDataObject::new(vec![r"C:\BentoDesk\move.txt".to_owned()]).into();
    set_feedback_drop_effect(&data_object, performed_drop_effect_format(), DROPEFFECT(0));
    set_feedback_drop_effect(
        &data_object,
        logical_performed_drop_effect_format(),
        DROPEFFECT_MOVE,
    );

    let (logical, performed) = reported_drop_effects(&data_object);
    assert_eq!(logical, Some(DROPEFFECT_MOVE));
    assert_eq!(performed, Some(DROPEFFECT(0)));
}

#[test]
fn shell_data_object_receives_move_preference() {
    // SAFETY: OLE is initialized for this test thread and balanced below only
    // when this call succeeds.
    let initialized = unsafe { OleInitialize(None) }.is_ok();
    let dir = std::env::temp_dir().join(format!(
        "bentodesk-drag-effect-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("drag test directory");
    let path = dir.join("move.txt");
    std::fs::write(&path, b"move").expect("drag test file");
    let paths = vec![path.to_string_lossy().into_owned()];

    let data_object = try_create_shell_data_object(&paths, DROPEFFECT_MOVE)
        .expect("create Shell data object")
        .expect("Shell data object");
    assert_eq!(read_preferred_drop_effect(&data_object), DROPEFFECT_MOVE);
    drop(data_object);

    let _ = std::fs::remove_dir_all(dir);
    if initialized {
        // SAFETY: balances this test thread's successful OleInitialize.
        unsafe { OleUninitialize() };
    }
}
