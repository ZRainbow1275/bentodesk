//! Drag initiation and coordination.
//!
//! Receives a list of file paths from the caller, constructs the OLE data
//! objects, and calls `DoDragDrop`. The result (moved / copied / cancelled)
//! is returned as a typed [`DragOutcome`] (1.x returned a `String` for IPC
//! transport — nano callers get a typed enum and can serialize it via the
//! `Serialize` derive if v2.x scripting needs it).
//!
//! **This function blocks the calling thread** until the user completes or
//! cancels the drag — the OLE message loop spins inside `DoDragDrop`. Spec
//! §9 says background work uses `std::thread + crossbeam_channel`, so the
//! caller should typically invoke this on a worker thread, not the UI
//! message-pump thread.

use serde::{Deserialize, Serialize};

use windows::Win32::{
    Foundation::*,
    System::{Com::IDataObject, Ole::*},
};
use windows::core::{HRESULT, Interface};

/// Errors surfaced by [`start_drag_operation`].
#[derive(Debug)]
pub enum DragDropError {
    /// Caller passed an empty `file_paths` slice.
    NoFiles,
    /// The dedicated STA drag worker could not be created.
    WorkerSpawn,
    /// The dedicated STA drag worker panicked before returning an OLE result.
    WorkerPanicked,
    /// `DoDragDrop` returned an HRESULT we did not classify as success or
    /// cancellation.
    Unexpected(HRESULT),
}

impl core::fmt::Display for DragDropError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoFiles => f.write_str("no files to drag"),
            Self::WorkerSpawn => f.write_str("failed to spawn OLE drag worker"),
            Self::WorkerPanicked => f.write_str("OLE drag worker panicked"),
            Self::Unexpected(hr) => write!(f, "DoDragDrop returned: {hr:?}"),
        }
    }
}

impl core::error::Error for DragDropError {}

/// Outcome of a `DoDragDrop` call.
///
/// 1.x returned `"dropped"` / `"cancelled"` strings; nano returns a typed
/// enum. The string-style variants are still exposed via [`DragOutcome::as_str`]
/// for byte-compatible IPC re-introduction (master plan §11 ΔB).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DragOutcome {
    Dropped,
    Cancelled,
}

impl DragOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dropped => "dropped",
            Self::Cancelled => "cancelled",
        }
    }
}

fn drag_proof_log_enabled() -> bool {
    std::env::var_os("BENTODESK_NANO_DRAG_PROOF_LOG").is_some()
}

fn log_drag_proof(msg: &str) {
    if drag_proof_log_enabled() {
        let mut stderr = std::io::stderr();
        let _ = std::io::Write::write_all(&mut stderr, msg.as_bytes());
        let _ = std::io::Write::flush(&mut stderr);
    }
}

/// Initiate an OLE drag-and-drop operation for the given file paths.
///
/// Initialises COM (STA), creates the COM data objects, calls `DoDragDrop`,
/// and uninitialises COM on return. Returns:
///
/// - `Ok(DragOutcome::Dropped)` if the user dropped the payload onto a
///   target that accepted it.
/// - `Ok(DragOutcome::Cancelled)` if the user pressed Esc / the OS cancelled.
/// - `Err(DragDropError::NoFiles)` if `file_paths` is empty.
/// - `Err(DragDropError::Unexpected(hr))` for any other HRESULT.
pub fn start_drag_operation(file_paths: &[String]) -> Result<DragOutcome, DragDropError> {
    start_drag_operation_inner(file_paths, None)
}

/// Initiate an OLE drag-and-drop operation for the given file paths using the
/// selected-stack source HWND.
///
/// The source HWND lets the dedicated STA worker attach to the selected-stack
/// input thread and keep diagnostics tied to the real BentoDesk window while
/// `DoDragDrop` still uses BentoDesk's own `IDropSource` semantics.
pub fn start_drag_operation_from_hwnd(
    file_paths: &[String],
    source_hwnd: isize,
) -> Result<DragOutcome, DragDropError> {
    if source_hwnd == 0 {
        return start_drag_operation_inner(file_paths, None);
    }
    if file_paths.is_empty() {
        return Err(DragDropError::NoFiles);
    }

    let hwnd = HWND(source_hwnd as *mut core::ffi::c_void);
    // If the caller is already running on the source HWND thread, run the OLE
    // drag loop inline. Calling `DoDragDrop` from a joined worker while the
    // source window thread is still inside `WM_MOUSEMOVE` can leave OLE with a
    // data object probe (`EnumFormatEtc`) but no real source-window drag loop.
    // Native Win32 drag sources normally call `DoDragDrop` from the same
    // thread that received the mouse gesture; keep that contract for the
    // selected-stack shell path and reserve the worker for non-window-thread
    // callers.
    unsafe {
        let current_thread = windows_sys::Win32::System::Threading::GetCurrentThreadId();
        let source_thread = windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
            hwnd.0 as windows_sys::Win32::Foundation::HWND,
            core::ptr::null_mut(),
        );
        if should_run_drag_inline_for_source_thread(source_thread, current_thread) {
            log_drag_proof(
                format!(
                    "drag_drop: source_thread={} current_thread={} inline_source_thread=true\n",
                    source_thread, current_thread
                )
                .as_str(),
            );
            return start_drag_operation_inner(file_paths, Some(hwnd));
        }
        log_drag_proof(
            format!(
                "drag_drop: source_thread={} current_thread={} inline_source_thread=false\n",
                source_thread, current_thread
            )
            .as_str(),
        );
    }

    let paths = file_paths.to_vec();
    let source_hwnd_bits = source_hwnd;
    let worker = std::thread::Builder::new()
        .name("bentodesk-ole-drag-out".to_owned())
        .spawn(move || {
            let worker_hwnd = HWND(source_hwnd_bits as *mut core::ffi::c_void);
            start_drag_operation_inner(paths.as_slice(), Some(worker_hwnd))
        })
        .map_err(|_| DragDropError::WorkerSpawn)?;
    worker.join().map_err(|_| DragDropError::WorkerPanicked)?
}

fn should_run_drag_inline_for_source_thread(source_thread: u32, current_thread: u32) -> bool {
    source_thread != 0 && source_thread == current_thread
}

fn classify_drag_result(
    hr: HRESULT,
    effect: DROPEFFECT,
    source_window_drag: bool,
) -> Result<DragOutcome, DragDropError> {
    if hr == DRAGDROP_S_DROP || (source_window_drag && hr == S_OK && effect != DROPEFFECT(0)) {
        Ok(DragOutcome::Dropped)
    } else if hr == DRAGDROP_S_CANCEL {
        Ok(DragOutcome::Cancelled)
    } else {
        Err(DragDropError::Unexpected(hr))
    }
}

fn start_drag_operation_inner(
    file_paths: &[String],
    source_hwnd: Option<HWND>,
) -> Result<DragOutcome, DragDropError> {
    if file_paths.is_empty() {
        return Err(DragDropError::NoFiles);
    }

    tracing::info!(
        "drag_drop: initiating OLE drag for {} file(s)",
        file_paths.len()
    );
    log_drag_proof(
        format!(
            "drag_drop: initiating OLE drag for {} file(s)\n",
            file_paths.len()
        )
        .as_str(),
    );

    // SAFETY: The whole block is `unsafe` because every Win32 call below is
    // FFI. The invariants are:
    //   1. CoInitializeEx is called before any other COM API on this thread.
    //   2. CoUninitialize matches the CoInitializeEx (one-to-one on the
    //      thread regardless of the return value of the init call).
    //   3. The IDataObject / IDropSource pointers passed to DoDragDrop are
    //      valid for the duration of the call (they live in this stack frame).
    unsafe {
        // `DoDragDrop` requires OLE initialisation, not just plain COM STA.
        // This worker owns its OLE apartment for the duration of the drag.
        if let Err(err) = OleInitialize(None) {
            log_drag_proof(
                format!("drag_drop: OleInitialize failed hr={:?}\n", err.code()).as_str(),
            );
            return Err(DragDropError::Unexpected(err.code()));
        }
        log_drag_proof("drag_drop: OleInitialize ok\n");

        // Ensure the dedicated STA worker has a message queue before entering
        // OLE's modal drag loop.
        let mut msg: windows_sys::Win32::UI::WindowsAndMessaging::MSG = core::mem::zeroed();
        let _ = windows_sys::Win32::UI::WindowsAndMessaging::PeekMessageW(
            &mut msg,
            core::ptr::null_mut(),
            0,
            0,
            windows_sys::Win32::UI::WindowsAndMessaging::PM_NOREMOVE,
        );
        log_drag_proof("drag_drop: worker message queue ready\n");
        let left_state = windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x01);
        log_drag_proof(format!("drag_drop: VK_LBUTTON state=0x{left_state:x}\n").as_str());

        let data_object: IDataObject = super::data_object::create_drag_data_object(file_paths)
            .map_err(|err| DragDropError::Unexpected(err.code()))?;

        let source_window_drag = source_hwnd.is_some();
        let mut attached_source_thread = None;
        let drag_result = if let Some(hwnd) = source_hwnd {
            let current_thread = windows_sys::Win32::System::Threading::GetCurrentThreadId();
            let source_thread =
                windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                    hwnd.0 as windows_sys::Win32::Foundation::HWND,
                    core::ptr::null_mut(),
                );
            log_drag_proof(
                format!(
                    "drag_drop: source_thread={} current_thread={} attach_pending\n",
                    source_thread, current_thread
                )
                .as_str(),
            );
            let attached_input = source_thread != 0
                && source_thread != current_thread
                && windows_sys::Win32::System::Threading::AttachThreadInput(
                    current_thread,
                    source_thread,
                    1,
                ) != 0;
            if attached_input {
                attached_source_thread = Some((current_thread, source_thread));
            }
            log_drag_proof(
                format!(
                    "drag_drop: source_thread={} current_thread={} attached_input={}\n",
                    source_thread, current_thread, attached_input
                )
                .as_str(),
            );
            log_drag_proof(format!("drag_drop: SHDoDragDrop enter hwnd={hwnd:?}\n").as_str());
            // The typed `windows` wrapper maps every successful HRESULT to
            // `Ok(DROPEFFECT)`, which loses the distinction between `S_OK` and
            // `DRAGDROP_S_DROP`. Preserve the raw Shell result so a completed
            // desktop drop can safely remove its non-Ctrl source item.
            let mut raw_effect = 0u32;
            let raw_hr = windows_sys::Win32::UI::Shell::SHDoDragDrop(
                hwnd.0 as windows_sys::Win32::Foundation::HWND,
                data_object.as_raw(),
                core::ptr::null_mut(),
                windows_sys::Win32::System::Ole::DROPEFFECT_COPY
                    | windows_sys::Win32::System::Ole::DROPEFFECT_MOVE,
                &mut raw_effect,
            );
            let hr = HRESULT(raw_hr);
            let effect = DROPEFFECT(raw_effect);
            log_drag_proof(
                format!("drag_drop: SHDoDragDrop returned hr={hr:?} effect={effect:?}\n").as_str(),
            );
            Ok((hr, effect))
        } else {
            let drop_source: IDropSource = super::drop_source::BentoDropSource.into();
            let mut effect = DROPEFFECT(0);

            // SAFETY: data_object + drop_source are alive for the duration of
            // this call (drop runs only on return); &mut effect is a valid
            // out-pointer in this stack frame.
            log_drag_proof("drag_drop: DoDragDrop enter\n");
            let hr = DoDragDrop(
                &data_object,
                &drop_source,
                DROPEFFECT_COPY | DROPEFFECT_MOVE,
                &mut effect,
            );
            log_drag_proof(
                format!("drag_drop: DoDragDrop returned hr={hr:?} effect={effect:?}\n").as_str(),
            );
            Ok((hr, effect))
        };

        if let Some((current_thread, source_thread)) = attached_source_thread {
            let _ = windows_sys::Win32::System::Threading::AttachThreadInput(
                current_thread,
                source_thread,
                0,
            );
        }

        OleUninitialize();

        let (hr, effect) = match drag_result {
            Ok(value) => value,
            Err(hr) => return Err(DragDropError::Unexpected(hr)),
        };

        match classify_drag_result(hr, effect, source_window_drag) {
            Ok(DragOutcome::Dropped) => {
                tracing::info!("drag_drop: completed (effect = {:?})", effect);
                log_drag_proof(format!("drag_drop: completed (effect = {effect:?})\n").as_str());
                Ok(DragOutcome::Dropped)
            }
            Ok(DragOutcome::Cancelled) => {
                tracing::info!("drag_drop: cancelled by user");
                log_drag_proof("drag_drop: cancelled by user\n");
                Ok(DragOutcome::Cancelled)
            }
            Err(err) => {
                tracing::warn!("drag_drop: unexpected HRESULT {hr:?}");
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_files_errors_out_without_calling_co_init() {
        let result = start_drag_operation(&[]);
        assert!(matches!(result, Err(DragDropError::NoFiles)));
    }

    #[test]
    fn drag_outcome_serde_round_trip() {
        for variant in [DragOutcome::Dropped, DragOutcome::Cancelled] {
            let json = serde_json::to_string(&variant).expect("serialize");
            let parsed: DragOutcome = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(variant, parsed);
        }
    }

    #[test]
    fn drag_outcome_as_str_matches_1x_strings() {
        assert_eq!(DragOutcome::Dropped.as_str(), "dropped");
        assert_eq!(DragOutcome::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn source_thread_drag_runs_inline_only_for_same_live_thread() {
        assert!(should_run_drag_inline_for_source_thread(42, 42));
        assert!(!should_run_drag_inline_for_source_thread(0, 42));
        assert!(!should_run_drag_inline_for_source_thread(7, 42));
    }

    #[test]
    fn raw_shell_drop_hresult_is_classified_even_when_effect_is_zero() {
        assert!(matches!(
            classify_drag_result(DRAGDROP_S_DROP, DROPEFFECT(0), true),
            Ok(DragOutcome::Dropped)
        ));
    }

    #[test]
    fn shell_cancel_never_becomes_a_drop() {
        assert!(matches!(
            classify_drag_result(DRAGDROP_S_CANCEL, DROPEFFECT_COPY, true),
            Ok(DragOutcome::Cancelled)
        ));
    }

    #[test]
    fn source_shell_s_ok_requires_an_accepted_effect() {
        assert!(matches!(
            classify_drag_result(S_OK, DROPEFFECT_COPY, true),
            Ok(DragOutcome::Dropped)
        ));
        assert!(matches!(
            classify_drag_result(S_OK, DROPEFFECT(0), true),
            Err(DragDropError::Unexpected(hr)) if hr == S_OK
        ));
    }
}
