//! COM `IDropSource` implementation for OLE drag-and-drop.
//!
//! Provides drag feedback and handles escape-to-cancel and button-release-to-drop
//! semantics required by the OLE drag-drop protocol.

use windows::{
    core::*,
    Win32::{
        Foundation::*,
        System::{Ole::*, SystemServices::MODIFIERKEYS_FLAGS},
    },
};

/// COM object implementing `IDropSource` for BentoDesk drag operations.
///
/// This object is queried by the OLE subsystem during `DoDragDrop` to determine
/// whether the drag should continue, be cancelled, or result in a drop.
#[implement(IDropSource)]
pub struct BentoDropSource;

/// MK_LBUTTON: indicates the left mouse button is down.
const MK_LBUTTON: MODIFIERKEYS_FLAGS = MODIFIERKEYS_FLAGS(0x0001);

impl IDropSource_Impl for BentoDropSource_Impl {
    /// Called by OLE to determine whether to continue the drag, cancel, or drop.
    ///
    /// - If Escape is pressed, the drag is cancelled.
    /// - If the left mouse button is released, a drop is performed.
    /// - Otherwise, the drag continues.
    fn QueryContinueDrag(&self, fescapepressed: BOOL, grfkeystate: MODIFIERKEYS_FLAGS) -> HRESULT {
        if fescapepressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if !grfkeystate.contains(MK_LBUTTON) {
            // Left button released -- perform the drop
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    /// Called by OLE to set the drag cursor. We use the system default cursors.
    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}
