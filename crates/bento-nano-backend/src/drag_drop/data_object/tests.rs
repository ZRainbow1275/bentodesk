use super::*;

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
