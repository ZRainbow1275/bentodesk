//! T-083 — OLE drag-and-drop (lift-verbatim from 1.x
//! `src-tauri/src/drag_drop/`).
//!
//! Implements the COM `IDataObject` / `IDropSource` interfaces required by
//! Win32 [`DoDragDrop`](windows::Win32::System::Ole::DoDragDrop) so files
//! dragged out of BentoDesk zones land in Explorer / other targets exactly as
//! they would from the system desktop.
//!
//! Spec §8.1: the `windows-implement` proc-macro is the *only* sanctioned
//! proc-macro besides `serde_derive`; it expands at build-time into the
//! IUnknown vtable boilerplate, with zero runtime symbols.
//!
//! ## Modules
//!
//! - [`data_object`] — `BentoDataObject` (CF_HDROP + HGLOBAL).
//! - [`drop_source`] — `BentoDropSource` (Esc-cancel + button-release-drop).
//! - [`drop_target`] — `BentoDropTarget` (OLE drop-in via `IDropTarget`).
//! - [`drag_manager`] — `start_drag_operation` entry point that wires both
//!   COM objects into a blocking `DoDragDrop` call.

pub mod data_object;
pub mod drag_manager;
pub mod drop_source;
pub mod drop_target;

pub const MAX_DROPPED_FILES: u32 = 1_024;
pub const MAX_DROPPED_PATH_CHARS: u32 = 32_767;
pub const MAX_DROPPED_TOTAL_PATH_CHARS: usize = 1024 * 1024;

pub use data_object::BentoDataObject;
pub use drag_manager::{
    DragDropError, DragOutcome, start_drag_operation, start_drag_operation_from_hwnd,
};
pub use drop_source::BentoDropSource;
pub use drop_target::{
    DropCanAcceptFn, DropCommitFn, DropPoint, DropTargetError, initialize_ole,
    register_drop_target, uninitialize_ole, unregister_drop_target,
};
