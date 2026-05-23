//! Win32 message pump bridge with frame-paced wakeup.
//!
//! Spec §9: zero async runtime. The UI thread is the message pump; the
//! optional crossbeam channel below lets background threads post wake-up
//! requests via `PostThreadMessageW` without dragging tokio in.
//!
//! ## T-007 / Wave A — frame scheduler
//!
//! The pre-T-007 loop used `GetMessageW`, which blocks indefinitely until a
//! message arrives. That's correct for a pure-event UI but starves
//! animations: a hover-fade or zone-resize transition has nothing else to
//! drive its tick when the mouse is idle, so the animation freezes
//! mid-curve until the next user input.
//!
//! T-007 swaps the inner blocking primitive for `MsgWaitForMultipleObjectsEx`
//! with a 17 ms timeout (~60 FPS) on the swap chain's frame-latency
//! waitable object (`IDXGISwapChain2::GetFrameLatencyWaitableObject`,
//! published into this module via [`register_frame_handle`] from
//! `dcomp.rs`). The wait returns when any of:
//! - the waitable handle signals (DXGI ready for the next frame),
//! - a window message arrives, or
//! - the 17 ms tick elapses.
//!
//! After the wait we drain pending messages with `PeekMessageW` and let the
//! existing dispatch path repaint. T-007 only provides the wakeup; the
//! per-frame paint tick belongs to the renderer (it calls
//! `InvalidateRect` from animation code).
//!
//! Cold start (before the first swap chain exists) falls back to `GetMessageW`
//! so the very first window can come up without a registered handle.

use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

use windows_sys::Win32::Foundation::{HANDLE, LPARAM, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx,
    PM_REMOVE, PeekMessageW, PostThreadMessageW, QS_ALLINPUT, TranslateMessage, WM_APP, WM_QUIT,
};

/// Custom window message numbers reserved by the framework. Apps should keep
/// their own custom messages in the `WM_APP + 1024..` range to avoid clashes.
pub const WM_NANO_WAKE: u32 = WM_APP;

/// Frame-pacing timeout — ~16.67 ms rounded up to 17 ms gives 60 FPS even
/// when no window message arrives (idle animation, smooth tween-while-idle).
/// Stored as a constant so animation code can reference the same number when
/// computing per-frame deltas.
pub const FRAME_TIMEOUT_MS: u32 = 17;

/// Process-wide swap chain frame-latency waitable handle, published by
/// `dcomp::WindowComp::create` once the first swap chain exists. Stored as
/// `AtomicPtr<c_void>` because `HANDLE = *mut c_void` and `OnceLock`/`Mutex`
/// would be overkill for a single pointer cell that flips null → non-null
/// exactly once per process lifetime.
///
/// Read by every iteration of [`run`]; null means "fall back to `GetMessageW`"
/// (cold start before the renderer registers).
static FRAME_HANDLE: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

/// Publish the swap chain's frame-latency waitable handle into the message
/// loop. Called once by the renderer (`dcomp::WindowComp::create`) right
/// after `IDXGISwapChain2::SetMaximumFrameLatency(1)` returns success.
///
/// Safe to call from any thread; the store is `Release`-ordered so the next
/// iteration of [`run`] picks it up. Subsequent calls overwrite, but in
/// practice the handle is registered once and lives for the process lifetime
/// (DXGI frame-latency handles are owned by the swap chain and must not be
/// closed by us).
///
/// Passing a null handle deregisters and forces fallback — useful for tests
/// that want to assert the cold-start path.
pub fn register_frame_handle(handle: HANDLE) {
    // `windows-sys` 0.59 defines `HANDLE = *mut c_void` (a type alias, not a
    // newtype), so the store is a direct pointer write — no `as` cast needed.
    FRAME_HANDLE.store(handle, Ordering::Release);
}

/// Run the message pump on the calling thread. Returns when WM_QUIT is posted.
///
/// While the message queue is empty the loop blocks for at most
/// [`FRAME_TIMEOUT_MS`] in `MsgWaitForMultipleObjectsEx`, waking on either
/// the swap chain frame-latency handle (published via
/// [`register_frame_handle`]) or the ~60 FPS tick. Background threads can
/// still post work via [`post_wake`] to break the wait early.
pub fn run() {
    let mut msg = unsafe { core::mem::zeroed::<MSG>() };
    loop {
        // Read once per iteration — `register_frame_handle` may flip the
        // pointer between iterations (cold start → registered).
        let frame_handle = FRAME_HANDLE.load(Ordering::Acquire);

        if frame_handle.is_null() {
            // Cold-start fallback. Equivalent to the pre-T-007 loop —
            // blocks until any message arrives. The first swap chain
            // creation will register a handle and the next iteration
            // upgrades to the frame-paced path.
            //
            // SAFETY: msg is a valid out param; hwnd null = all windows
            //         on this thread; min/max filter 0/0 = all messages.
            let r = unsafe { GetMessageW(&mut msg, ptr::null_mut(), 0, 0) };
            if r == 0 || r == -1 {
                break;
            }
            // SAFETY: msg fully populated by GetMessageW.
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            continue;
        }

        // Frame-paced path. Wait up to FRAME_TIMEOUT_MS for either:
        //   1. the DXGI waitable handle signalling the next frame is due,
        //   2. a window message landing in this thread's queue, or
        //   3. the timeout firing (guarantees animation tick at idle).
        //
        // MWMO_INPUTAVAILABLE plus QS_ALLINPUT mirrors the documented
        // pattern from MSDN's "Using Message Queues" — covers messages
        // already pending at call time as well as those arriving during
        // the wait. Single-handle wait so the array slot is just `&handle`.
        let handle_array = [frame_handle as HANDLE];

        // SAFETY: handle_array.as_ptr() is a valid pointer to an array of
        //         one HANDLE that lives across the call; the handle itself
        //         is owned by DXGI (registered via register_frame_handle)
        //         and remains valid for the swap chain's lifetime, which
        //         outlives the message pump (renderer is dropped after the
        //         pump exits in the shell). MWMO_INPUTAVAILABLE +
        //         QS_ALLINPUT is the canonical MSDN combo for "wait for
        //         object OR any input".
        let _wait_result = unsafe {
            MsgWaitForMultipleObjectsEx(
                1,
                handle_array.as_ptr(),
                FRAME_TIMEOUT_MS,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            )
        };

        // Drain every pending message regardless of which event woke us.
        // PeekMessage with PM_REMOVE pops one at a time; inner loop until
        // queue empty. WM_QUIT propagates out of `pump_pending_messages`
        // as `false`, ending the outer loop.
        if !pump_pending_messages(&mut msg) {
            break;
        }

        debug_trace_tick();
    }
}

/// Drain the thread message queue. Returns `false` when WM_QUIT is observed
/// (caller breaks the outer loop), `true` otherwise.
fn pump_pending_messages(msg: &mut MSG) -> bool {
    loop {
        // SAFETY: msg is a valid out param; hwnd null = all windows on
        //         this thread; min/max filter 0/0 = all messages;
        //         PM_REMOVE pops on success.
        let got = unsafe { PeekMessageW(msg, ptr::null_mut(), 0, 0, PM_REMOVE) };
        if got == 0 {
            return true; // queue empty — back to wait
        }
        if msg.message == WM_QUIT {
            return false;
        }
        // SAFETY: msg fully populated by PeekMessageW.
        unsafe {
            TranslateMessage(msg);
            DispatchMessageW(msg);
        }
    }
}

/// Debug-only single-line trace via `OutputDebugStringA`. Stripped to a
/// no-op in release so the steady-state hot path is one branch on
/// `cfg(debug_assertions)` — the compiler folds it out under `opt-level=z`.
///
/// Trace text is fixed-shape (no `format!` allocation) per spec §10, but
/// includes the absolute tick count via `GetTickCount64` so reading the
/// debug stream you can tell wakeups apart.
#[cfg(debug_assertions)]
fn debug_trace_tick() {
    use core::fmt::Write as _;

    use smallvec::SmallVec;
    use windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringA;
    use windows_sys::Win32::System::SystemInformation::GetTickCount64;

    // 64 bytes ≥ "MsgWait: tick @ 18446744073709551615ms\n\0" (38 chars).
    let mut buf: SmallVec<[u8; 64]> = SmallVec::new();
    struct SvWriter<'a>(&'a mut SmallVec<[u8; 64]>);
    impl core::fmt::Write for SvWriter<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            self.0.extend_from_slice(s.as_bytes());
            Ok(())
        }
    }
    // SAFETY: GetTickCount64 reads a system counter; documented thread-safe.
    let now = unsafe { GetTickCount64() };
    let _ = write!(SvWriter(&mut buf), "MsgWait: tick @ {now}ms\n\0");
    // SAFETY: buffer ends in the explicit NUL byte from the write! call,
    //         satisfying the OutputDebugStringA C-string contract. The
    //         pointer is valid for the duration of the call.
    unsafe { OutputDebugStringA(buf.as_ptr()) };
}

#[cfg(not(debug_assertions))]
#[inline(always)]
fn debug_trace_tick() {}

/// Post a wake-up message to the UI thread. Safe to call from any thread.
///
/// `thread_id` is the value returned by `GetCurrentThreadId()` from the UI
/// thread (cache it once at startup before spawning workers). Returns `true`
/// on success.
pub fn post_wake(thread_id: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
    // SAFETY: PostThreadMessageW is documented thread-safe; thread_id
    //         assumed valid by the caller.
    let r = unsafe { PostThreadMessageW(thread_id, WM_NANO_WAKE, wparam, lparam) };
    r != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    static FRAME_HANDLE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn frame_handle_default_null_after_clear() {
        let _guard = FRAME_HANDLE_TEST_LOCK
            .lock()
            .expect("frame handle test lock");
        // Force a clean slate: previous tests in this binary may have
        // installed a fake handle. The store-null path is documented and
        // makes the cold-start fallback observable.
        register_frame_handle(ptr::null_mut());
        let h = FRAME_HANDLE.load(Ordering::Acquire);
        assert!(h.is_null(), "register_frame_handle(null) must clear");
    }

    #[test]
    fn frame_handle_round_trip_persists_pointer() {
        let _guard = FRAME_HANDLE_TEST_LOCK
            .lock()
            .expect("frame handle test lock");
        // Use a non-null sentinel — we never dereference the handle in
        // tests, only verify the atomic round-trips it intact.
        let fake = 0xDEAD_BEEFusize as HANDLE;
        register_frame_handle(fake);
        let read_back = FRAME_HANDLE.load(Ordering::Acquire);
        assert_eq!(read_back as usize, 0xDEAD_BEEFusize);
        // Reset for any later tests in this binary.
        register_frame_handle(ptr::null_mut());
    }

    #[test]
    fn frame_timeout_is_60_fps_ceiling() {
        // 60 FPS = 16.67 ms; rounding up to 17 keeps wakeups slightly
        // below the next vsync, never missing a frame.
        assert_eq!(FRAME_TIMEOUT_MS, 17);
    }
}
