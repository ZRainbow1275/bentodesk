//! `IDropTarget` implementation for OLE file drop-in.
//!
//! Shell-level `WM_DROPFILES` is kept as a compatibility fallback, but full
//! parity with the Tauri desktop drag-in path requires registering the main
//! HWND as an OLE drop target as well. This module owns:
//!
//! - UI-thread OLE initialisation helpers (`initialize_ole` / `uninitialize_ole`)
//! - HWND registration (`register_drop_target` / `unregister_drop_target`)
//! - `BentoDropTarget`, which accepts `CF_HDROP` payloads and forwards them
//!   into shell-provided callbacks
//!
//! The backend intentionally does **not** know about `AppRoot`, zones, or
//! renderer state. Shell code decides whether a screen point is inside a real
//! zone and what to do with the dropped paths.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::cell::Cell;

use serde::{Deserialize, Serialize};
use windows::{
    Win32::{
        Foundation::{DRAGDROP_E_NOTREGISTERED, DV_E_FORMATETC, HWND, POINTL},
        System::{
            Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, TYMED_HGLOBAL},
            Ole::{
                CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, IDropTarget, IDropTarget_Impl,
                OleInitialize, OleUninitialize, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop,
            },
            SystemServices::MODIFIERKEYS_FLAGS,
        },
        UI::Shell::{DragQueryFileW, HDROP},
    },
    core::{Error as WinError, HRESULT, implement},
};

/// Screen-space point forwarded from OLE drag/drop callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropPoint {
    pub x: i32,
    pub y: i32,
}

impl DropPoint {
    const fn from_pointl(point: &POINTL) -> Self {
        Self {
            x: point.x,
            y: point.y,
        }
    }
}

/// Shell callback: return `true` when the current screen point maps to a real
/// selected-stack drop target.
pub type DropCanAcceptFn = fn(DropPoint) -> bool;
/// Shell callback: commit the dropped paths into the selected-stack runtime.
pub type DropCommitFn = fn(DropPoint, Vec<String>);

/// Errors surfaced by OLE drop-target setup / teardown.
#[derive(Debug)]
pub enum DropTargetError {
    NullHwnd,
    OleInit(WinError),
    Register(WinError),
    Revoke(WinError),
}

impl core::fmt::Display for DropTargetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NullHwnd => f.write_str("drop target: null HWND"),
            Self::OleInit(err) => write!(f, "drop target: OleInitialize failed: {err}"),
            Self::Register(err) => write!(f, "drop target: RegisterDragDrop failed: {err}"),
            Self::Revoke(err) => write!(f, "drop target: RevokeDragDrop failed: {err}"),
        }
    }
}

impl core::error::Error for DropTargetError {}

/// Best-effort OLE initialisation for the shell UI thread.
pub fn initialize_ole() -> Result<(), DropTargetError> {
    // SAFETY: called from the shell's UI thread before RegisterDragDrop.
    unsafe { OleInitialize(None).map_err(DropTargetError::OleInit) }
}

/// Balance a prior [`initialize_ole`] call on the same thread.
pub fn uninitialize_ole() {
    // SAFETY: balanced by the shell after the message loop returns.
    unsafe { OleUninitialize() };
}

/// Register `raw_hwnd` as an OLE drop target.
pub fn register_drop_target(
    raw_hwnd: *mut core::ffi::c_void,
    can_accept: DropCanAcceptFn,
    on_drop: DropCommitFn,
) -> Result<(), DropTargetError> {
    if raw_hwnd.is_null() {
        return Err(DropTargetError::NullHwnd);
    }

    let hwnd = HWND(raw_hwnd);
    let target: IDropTarget = BentoDropTarget::new(can_accept, on_drop).into();

    // SAFETY: caller guarantees UI-thread OLE init; RegisterDragDrop keeps its
    // own COM reference to `target` until RevokeDragDrop.
    unsafe { RegisterDragDrop(hwnd, &target).map_err(DropTargetError::Register) }
}

/// Revoke the OLE drop target registration for `raw_hwnd`.
pub fn unregister_drop_target(raw_hwnd: *mut core::ffi::c_void) -> Result<(), DropTargetError> {
    if raw_hwnd.is_null() {
        return Err(DropTargetError::NullHwnd);
    }

    let hwnd = HWND(raw_hwnd);
    // SAFETY: caller owns the HWND teardown path; `DRAGDROP_E_NOTREGISTERED`
    // is treated as idempotent cleanup so fallback-only runs do not warn.
    unsafe {
        match RevokeDragDrop(hwnd) {
            Ok(()) => Ok(()),
            Err(err) if err.code() == DRAGDROP_E_NOTREGISTERED => Ok(()),
            Err(err) => Err(DropTargetError::Revoke(err)),
        }
    }
}

#[derive(Debug)]
enum DropExtractError {
    MissingDataObject,
    UnsupportedFormat(HRESULT),
    GetData(WinError),
}

impl core::fmt::Display for DropExtractError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingDataObject => f.write_str("drop target payload missing IDataObject"),
            Self::UnsupportedFormat(hr) => {
                write!(f, "drop target payload format unsupported: {hr}")
            }
            Self::GetData(err) => write!(f, "drop target IDataObject::GetData failed: {err}"),
        }
    }
}

#[implement(IDropTarget)]
pub struct BentoDropTarget {
    can_accept: DropCanAcceptFn,
    on_drop: DropCommitFn,
    payload_supported: Cell<bool>,
}

impl BentoDropTarget {
    pub fn new(can_accept: DropCanAcceptFn, on_drop: DropCommitFn) -> Self {
        Self {
            can_accept,
            on_drop,
            payload_supported: Cell::new(false),
        }
    }

    fn effect_for(&self, point: DropPoint) -> DROPEFFECT {
        if self.payload_supported.get() && (self.can_accept)(point) {
            DROPEFFECT_COPY
        } else {
            DROPEFFECT(0)
        }
    }

    fn set_effect(pdweffect: *mut DROPEFFECT, effect: DROPEFFECT) {
        // SAFETY: OLE owns the out-pointer; null is tolerated defensively.
        unsafe {
            if !pdweffect.is_null() {
                *pdweffect = effect;
            }
        }
    }
}

impl IDropTarget_Impl for BentoDropTarget_Impl {
    fn DragEnter(
        &self,
        pdataobj: Option<&IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let payload_supported = match extract_file_paths(pdataobj) {
            Ok(files) => !files.is_empty(),
            Err(DropExtractError::UnsupportedFormat(code)) if code == DV_E_FORMATETC => false,
            Err(err) => {
                tracing::debug!(
                    target: "bentodesk::drag_drop",
                    error = ?err,
                    "OLE DragEnter payload rejected"
                );
                false
            }
        };
        self.payload_supported.set(payload_supported);
        BentoDropTarget::set_effect(pdweffect, self.effect_for(DropPoint::from_pointl(pt)));
        Ok(())
    }

    fn DragOver(
        &self,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        BentoDropTarget::set_effect(pdweffect, self.effect_for(DropPoint::from_pointl(pt)));
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        self.payload_supported.set(false);
        Ok(())
    }

    fn Drop(
        &self,
        pdataobj: Option<&IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let point = DropPoint::from_pointl(pt);
        let files = match extract_file_paths(pdataobj) {
            Ok(files) if !files.is_empty() => files,
            Ok(_) => {
                self.payload_supported.set(false);
                BentoDropTarget::set_effect(pdweffect, DROPEFFECT(0));
                return Ok(());
            }
            Err(err) => {
                tracing::debug!(
                    target: "bentodesk::drag_drop",
                    error = ?err,
                    "OLE Drop payload rejected"
                );
                self.payload_supported.set(false);
                BentoDropTarget::set_effect(pdweffect, DROPEFFECT(0));
                return Ok(());
            }
        };

        self.payload_supported.set(!files.is_empty());
        let effect = self.effect_for(point);
        if effect == DROPEFFECT_COPY {
            (self.on_drop)(point, files);
        }
        self.payload_supported.set(false);
        BentoDropTarget::set_effect(pdweffect, effect);
        Ok(())
    }
}

fn extract_file_paths(pdataobj: Option<&IDataObject>) -> Result<Vec<String>, DropExtractError> {
    let data_object = pdataobj.ok_or(DropExtractError::MissingDataObject)?;
    let format = FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: core::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };

    // SAFETY: `format` is a stack-local FORMATETC describing CF_HDROP via
    // TYMED_HGLOBAL. On success OLE returns ownership through STGMEDIUM, which
    // we release exactly once via ReleaseStgMedium below.
    let mut medium = unsafe {
        data_object
            .GetData(&format as *const FORMATETC)
            .map_err(DropExtractError::GetData)?
    };
    // SAFETY: CF_HDROP + TYMED_HGLOBAL guarantees the STGMEDIUM union holds a
    // valid `hGlobal` handle for DragQueryFileW-compatible DROPFILES data.
    let hdrop = unsafe { HDROP(medium.u.hGlobal.0) };
    let files = collect_hdrop_files(hdrop);
    // SAFETY: `medium` came from IDataObject::GetData above and must be
    // released once after we finish reading the CF_HDROP payload.
    unsafe { ReleaseStgMedium(&mut medium) };
    if files.is_empty() {
        Err(DropExtractError::UnsupportedFormat(DV_E_FORMATETC))
    } else {
        Ok(files)
    }
}

fn collect_hdrop_files(hdrop: HDROP) -> Vec<String> {
    // SAFETY: `hdrop` comes from a CF_HDROP STGMEDIUM returned by OLE.
    let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
    if count == 0 {
        return Vec::new();
    }

    let mut files = Vec::with_capacity(count as usize);
    for idx in 0..count {
        // SAFETY: length query with `None` buffer is the documented pattern.
        let len = unsafe { DragQueryFileW(hdrop, idx, None) };
        if len == 0 {
            continue;
        }
        let mut buf = vec![0u16; len as usize + 1];
        // SAFETY: `buf` is writable and sized for the full NUL-terminated path.
        let written = unsafe { DragQueryFileW(hdrop, idx, Some(buf.as_mut_slice())) };
        if written == 0 {
            continue;
        }
        files.push(String::from_utf16_lossy(&buf[..written as usize]));
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drag_drop::BentoDataObject;

    fn accept_everywhere(_: DropPoint) -> bool {
        true
    }

    fn ignore_drop(_: DropPoint, _: Vec<String>) {}

    #[test]
    fn extract_file_paths_round_trips_from_bento_data_object() {
        let source = vec![
            String::from("C:\\Users\\BentoDeskTest\\Desktop\\alpha.txt"),
            String::from("C:\\Users\\BentoDeskTest\\Desktop\\beta.lnk"),
        ];
        let object: IDataObject = BentoDataObject::new(source.clone()).into();

        let files = extract_file_paths(Some(&object)).expect("extract file paths");

        assert_eq!(files, source);
    }

    #[test]
    fn effect_requires_supported_payload_and_shell_acceptance() {
        let target = BentoDropTarget::new(accept_everywhere, ignore_drop);
        assert_eq!(target.effect_for(DropPoint { x: 10, y: 20 }), DROPEFFECT(0));

        target.payload_supported.set(true);
        assert_eq!(
            target.effect_for(DropPoint { x: 10, y: 20 }),
            DROPEFFECT_COPY
        );
    }
}
