//! Drag initiation and coordination.
//!
//! Receives a list of file paths from the frontend, constructs the OLE data
//! objects, and calls `DoDragDrop`. The result (moved / copied / cancelled)
//! is returned to the frontend so it can update the UI.

use windows::Win32::{
    Foundation::*,
    System::{Com::*, Ole::*},
};

use crate::error::BentoDeskError;

/// Initiate an OLE drag-and-drop operation for the given file paths.
///
/// This function **blocks the calling thread** until the user completes or
/// cancels the drag. It initialises COM (STA), creates the COM data objects,
/// calls `DoDragDrop`, and cleans up COM on return.
///
/// Returns `"dropped"` or `"cancelled"` as a string for easy IPC transport.
pub fn start_drag_operation(file_paths: &[String]) -> Result<String, BentoDeskError> {
    if file_paths.is_empty() {
        return Err(BentoDeskError::DragError("No files to drag".into()));
    }

    tracing::info!("Initiating OLE drag for {} file(s)", file_paths.len());

    unsafe {
        // OLE drag-drop requires a single-threaded apartment.
        // SAFETY: CoInitializeEx is safe to call from any thread. We initialise
        // as STA which is required for DoDragDrop. If COM was already initialized
        // on this thread (S_FALSE), that is acceptable.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let data_object: IDataObject =
            super::data_object::BentoDataObject::new(file_paths.to_vec()).into();
        let drop_source: IDropSource = super::drop_source::BentoDropSource.into();

        let mut effect = DROPEFFECT(0);

        // SAFETY: DoDragDrop is called with valid COM objects. It blocks until
        // the drag operation completes. The effect output tells us what happened.
        let hr = DoDragDrop(
            &data_object,
            &drop_source,
            DROPEFFECT_COPY | DROPEFFECT_MOVE,
            &mut effect,
        );

        CoUninitialize();

        // HRESULT is a struct wrapping i32, so we compare using equality rather
        // than pattern matching.
        if hr == DRAGDROP_S_DROP {
            tracing::info!("Drag completed: effect = {:?}", effect);
            Ok("dropped".to_string())
        } else if hr == DRAGDROP_S_CANCEL {
            tracing::info!("Drag cancelled by user");
            Ok("cancelled".to_string())
        } else {
            tracing::warn!("DoDragDrop returned unexpected HRESULT: {:?}", hr);
            Err(BentoDeskError::DragError(format!(
                "DoDragDrop returned: {:?}",
                hr
            )))
        }
    }
}
