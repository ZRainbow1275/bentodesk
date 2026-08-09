use super::*;

pub(super) fn set_preferred_drop_effect(
    data_object: &IDataObject,
    effect: DROPEFFECT,
) -> Result<()> {
    let format = FORMATETC {
        cfFormat: preferred_drop_effect_format(),
        ptd: core::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    // SAFETY: build_drop_effect returns an unlocked movable HGLOBAL containing
    // one DWORD; the STGMEDIUM below transfers or releases that allocation.
    let hglobal = unsafe { BentoDataObject::build_drop_effect(effect)? };
    let mut medium = STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: STGMEDIUM_0 { hGlobal: hglobal },
        pUnkForRelease: std::mem::ManuallyDrop::new(None),
    };
    // SAFETY: `format` and `medium` remain live for the call. The Shell data
    // object takes ownership only when SetData succeeds with `frelease=true`.
    let result = unsafe { data_object.SetData(&format, &medium, true) };
    if result.is_err() {
        // SAFETY: IDataObject does not take ownership when SetData returns an
        // error, so this function still owns and must free the HGLOBAL.
        unsafe { ReleaseStgMedium(&mut medium) };
    }
    result
}

pub(super) unsafe fn drop_effect_from_medium(medium: &STGMEDIUM) -> Option<DROPEFFECT> {
    if medium.tymed != TYMED_HGLOBAL.0 as u32 {
        return None;
    }
    // SAFETY: the active union arm is HGLOBAL because `tymed` was checked.
    let hglobal = unsafe { medium.u.hGlobal };
    // SAFETY: GlobalSize/GlobalLock accept the caller-owned HGLOBAL without
    // taking ownership. A Shell drop-effect payload is exactly one DWORD.
    if unsafe { GlobalSize(hglobal) } < core::mem::size_of::<u32>() {
        return None;
    }
    let raw = unsafe { GlobalLock(hglobal) };
    if raw.is_null() {
        return None;
    }
    // SAFETY: the size check above proves that one u32 can be read.
    let value = unsafe { raw.cast::<u32>().read_unaligned() };
    let _ = unsafe { GlobalUnlock(hglobal) };
    Some(DROPEFFECT(value))
}

fn read_drop_effect_format(data_object: &IDataObject, format_id: u16) -> Option<DROPEFFECT> {
    let format = FORMATETC {
        cfFormat: format_id,
        ptd: core::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    // SAFETY: `format` is fully initialized and live for this COM call.
    let mut medium = unsafe { data_object.GetData(&format) }.ok()?;
    // SAFETY: `medium` is owned by this caller until ReleaseStgMedium below.
    let effect = unsafe { drop_effect_from_medium(&medium) };
    // SAFETY: balances the successful IDataObject::GetData call exactly once.
    unsafe { ReleaseStgMedium(&mut medium) };
    effect
}

pub(crate) fn reported_drop_effects(
    data_object: &IDataObject,
) -> (Option<DROPEFFECT>, Option<DROPEFFECT>) {
    (
        read_drop_effect_format(data_object, logical_performed_drop_effect_format()),
        read_drop_effect_format(data_object, performed_drop_effect_format()),
    )
}
