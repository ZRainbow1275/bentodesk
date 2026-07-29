//! `IDropSource` implementation for OLE drag-and-drop.
//!
//! Provides the two callbacks that OLE invokes during a `DoDragDrop` loop:
//! `QueryContinueDrag` (cancel-on-Esc, drop-on-button-release) and
//! `GiveFeedback` (always use the system default cursors).
//!
//! Lifted verbatim from 1.x — no behavioural change.

use std::sync::atomic::{AtomicUsize, Ordering};

use windows::{
    Win32::{
        Foundation::*,
        System::{Ole::*, SystemServices::MODIFIERKEYS_FLAGS},
    },
    core::*,
};

/// COM object implementing `IDropSource` for BentoDesk drag operations.
///
/// Queried by the OLE subsystem during `DoDragDrop` to determine whether the
/// drag should continue, be cancelled, or result in a drop.
#[implement(IDropSource)]
pub struct BentoDropSource;

/// `MK_LBUTTON` — indicates the left mouse button is currently down.
const MK_LBUTTON: MODIFIERKEYS_FLAGS = MODIFIERKEYS_FLAGS(0x0001);
const VK_LBUTTON: i32 = 0x01;

static QUERY_CONTINUE_DRAG_COUNT: AtomicUsize = AtomicUsize::new(0);
static GIVE_FEEDBACK_COUNT: AtomicUsize = AtomicUsize::new(0);

fn drag_proof_log_enabled() -> bool {
    std::env::var_os("BENTODESK_DRAG_PROOF_LOG").is_some()
}

fn log_drag_proof(msg: &str) {
    if drag_proof_log_enabled() {
        let mut stderr = std::io::stderr();
        let _ = std::io::Write::write_all(&mut stderr, msg.as_bytes());
        let _ = std::io::Write::flush(&mut stderr);
    }
}

fn physical_left_button_down() -> bool {
    // SAFETY: `GetAsyncKeyState` is a read-only User32 query for the current
    // process desktop input state. `VK_LBUTTON` is the documented virtual-key
    // code for the primary mouse button.
    let state =
        unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(VK_LBUTTON) };
    state < 0
}

impl IDropSource_Impl for BentoDropSource_Impl {
    /// Called by OLE to determine whether to continue the drag, cancel, or drop.
    ///
    /// - If Escape is pressed, the drag is cancelled.
    /// - If the left mouse button is released, a drop is performed.
    /// - Otherwise, the drag continues.
    fn QueryContinueDrag(&self, fescapepressed: BOOL, grfkeystate: MODIFIERKEYS_FLAGS) -> HRESULT {
        let logical_left_button_down = grfkeystate.contains(MK_LBUTTON);
        let physical_left_button_down = physical_left_button_down();
        if drag_proof_log_enabled() {
            let count = QUERY_CONTINUE_DRAG_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            if count <= 8 || count.is_multiple_of(50) {
                log_drag_proof(
                    format!(
                        "drag_drop: QueryContinueDrag count={count} escape={} key_state=0x{:x} physical_left_down={}\n",
                        fescapepressed.as_bool(),
                        grfkeystate.0,
                        physical_left_button_down
                    )
                    .as_str(),
                );
            }
        }
        if fescapepressed.as_bool() {
            log_drag_proof("drag_drop: QueryContinueDrag cancel_escape\n");
            DRAGDROP_S_CANCEL
        } else if !logical_left_button_down || !physical_left_button_down {
            // Left button released — perform the drop.
            log_drag_proof("drag_drop: QueryContinueDrag drop_left_released\n");
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    /// Called by OLE to set the drag cursor. We use the system default cursors.
    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        if drag_proof_log_enabled() {
            let count = GIVE_FEEDBACK_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            if count <= 8 || count.is_multiple_of(50) {
                log_drag_proof(format!("drag_drop: GiveFeedback count={count}\n").as_str());
            }
        }
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}
