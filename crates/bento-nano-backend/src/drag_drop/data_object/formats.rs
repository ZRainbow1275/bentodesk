//! Shell clipboard-format registration and capability checks.

use super::*;

pub(super) fn preferred_drop_effect_format() -> u16 {
    // SAFETY: `CFSTR_PREFERREDDROPEFFECT` is a process-static, null-terminated
    // Shell clipboard format name. Registering the same name repeatedly is
    // documented to return the existing atom.
    unsafe { RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT) as u16 }
}

pub(super) fn shell_id_list_array_format() -> u16 {
    // SAFETY: `CFSTR_SHELLIDLIST` is a process-static, null-terminated Shell
    // clipboard format name. Registering the same name repeatedly returns the
    // existing atom and does not allocate per call.
    unsafe { RegisterClipboardFormatW(CFSTR_SHELLIDLIST) as u16 }
}

pub(super) fn shell_object_offsets_format() -> u16 {
    // SAFETY: `CFSTR_SHELLIDLISTOFFSET` is a process-static, null-terminated
    // Shell clipboard format name. Registering the same name repeatedly
    // returns the existing atom and does not allocate per call.
    unsafe { RegisterClipboardFormatW(CFSTR_SHELLIDLISTOFFSET) as u16 }
}

pub(super) fn drag_context_format() -> u16 {
    // SAFETY: Clipboard format names are process-static strings and re-register
    // to the same atom across calls.
    unsafe { RegisterClipboardFormatW(w!("DragContext")) as u16 }
}

pub(super) fn drag_source_helper_flags_format() -> u16 {
    // SAFETY: Clipboard format names are process-static strings and re-register
    // to the same atom across calls.
    unsafe { RegisterClipboardFormatW(w!("DragSourceHelperFlags")) as u16 }
}

pub(super) fn drag_image_bits_format() -> u16 {
    // SAFETY: Clipboard format names are process-static strings and re-register
    // to the same atom across calls.
    unsafe { RegisterClipboardFormatW(w!("DragImageBits")) as u16 }
}

pub(super) fn is_showing_text_format() -> u16 {
    // SAFETY: Clipboard format names are process-static strings and re-register
    // to the same atom across calls.
    unsafe { RegisterClipboardFormatW(w!("IsShowingText")) as u16 }
}

pub(super) fn using_default_drag_image_format() -> u16 {
    // SAFETY: Clipboard format names are process-static strings and re-register
    // to the same atom across calls.
    unsafe { RegisterClipboardFormatW(w!("UsingDefaultDragImage")) as u16 }
}

pub(super) fn is_computing_image_format() -> u16 {
    // SAFETY: Clipboard format names are process-static strings and re-register
    // to the same atom across calls.
    unsafe { RegisterClipboardFormatW(w!("IsComputingImage")) as u16 }
}

pub(super) fn disable_drag_text_format() -> u16 {
    // SAFETY: Clipboard format names are process-static strings and re-register
    // to the same atom across calls.
    unsafe { RegisterClipboardFormatW(w!("DisableDragText")) as u16 }
}

pub(super) fn is_showing_layered_format() -> u16 {
    // SAFETY: Clipboard format names are process-static strings and re-register
    // to the same atom across calls.
    unsafe { RegisterClipboardFormatW(w!("IsShowingLayered")) as u16 }
}

pub(super) fn drop_description_format() -> u16 {
    // SAFETY: `CFSTR_DROPDESCRIPTION` is a process-static Shell clipboard
    // format name that maps to the existing atom when re-registered.
    unsafe { RegisterClipboardFormatW(CFSTR_DROPDESCRIPTION) as u16 }
}

pub(super) fn in_shell_drag_loop_format() -> u16 {
    // SAFETY: `CFSTR_INDRAGLOOP` is a process-static Shell clipboard format
    // name that maps to the existing atom when re-registered.
    unsafe { RegisterClipboardFormatW(CFSTR_INDRAGLOOP) as u16 }
}

pub(super) fn supported_format_at(index: usize) -> Option<FORMATETC> {
    let cf_format = match index {
        0 => CF_HDROP.0,
        1 => shell_id_list_array_format(),
        2 => shell_object_offsets_format(),
        3 => preferred_drop_effect_format(),
        4 => in_shell_drag_loop_format(),
        _ => return None,
    };
    Some(FORMATETC {
        cfFormat: cf_format,
        ptd: core::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    })
}

pub(super) fn format_is_supported(fmt: &FORMATETC) -> bool {
    let preferred_drop_effect = preferred_drop_effect_format();
    let shell_id_list_array = shell_id_list_array_format();
    let shell_object_offsets = shell_object_offsets_format();
    let drag_context = drag_context_format();
    let drag_source_helper_flags = drag_source_helper_flags_format();
    let drag_image_bits = drag_image_bits_format();
    let is_showing_text = is_showing_text_format();
    let using_default_drag_image = using_default_drag_image_format();
    let is_computing_image = is_computing_image_format();
    let disable_drag_text = disable_drag_text_format();
    let is_showing_layered = is_showing_layered_format();
    let drop_description = drop_description_format();
    let in_shell_drag_loop = in_shell_drag_loop_format();
    if fmt.dwAspect != DVASPECT_CONTENT.0 {
        return false;
    }
    if fmt.cfFormat == drag_context {
        return fmt.tymed & TYMED_ISTREAM.0 as u32 != 0;
    }
    if fmt.cfFormat == CF_HDROP.0
        || fmt.cfFormat == shell_id_list_array
        || fmt.cfFormat == shell_object_offsets
        || fmt.cfFormat == preferred_drop_effect
        || fmt.cfFormat == drag_source_helper_flags
        || fmt.cfFormat == drag_image_bits
        || fmt.cfFormat == is_showing_text
        || fmt.cfFormat == using_default_drag_image
        || fmt.cfFormat == is_computing_image
        || fmt.cfFormat == disable_drag_text
        || fmt.cfFormat == is_showing_layered
        || fmt.cfFormat == drop_description
        || fmt.cfFormat == in_shell_drag_loop
    {
        return fmt.tymed & TYMED_HGLOBAL.0 as u32 != 0;
    }
    false
}
