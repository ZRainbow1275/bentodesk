//! mimalloc option tuning, Wave 19 second attempt after Wave 11.1 rollback.
//!
//! ## History
//!
//! Wave 4-11 set `MIMALLOC_*` env vars from Rust `main()`. The Wave 11
//! Postmortem in
//! `.trellis/tasks/04-30-phase-0-d2d-spike/research/extreme-memory-reduction.md`
//! proved this no-op: mimalloc's `_mi_options_init()` runs at DllMain
//! stage, BEFORE Rust `main()` (proven from upstream sources `mimalloc/
//! src/init.c:582-605` + `options.c:81-86,136`). Any `std::env::set_var`
//! issued from `main` is silently dropped. Wave 11.1 deleted this file
//! entirely as a clean retreat.
//!
//! Wave 19 returns the lever via raw `#[link_section = ".CRT$XCU"]`
//! function-pointer placement. The MSVC C runtime walks `.CRT$XCU`
//! during process start-up, calling each function pointer in turn — this
//! is the same mechanism the `ctor` crate macro expands to under the
//! hood. We use the raw form because the `ctor` crate is itself a
//! proc-macro derive crate (pulls `syn` / `quote` / `proc-macro2`),
//! which spec §8.1 line 216 forbids in the dep graph (Wave 19 ruling
//! "Path 2", logged in
//! `.trellis/tasks/04-30-phase-0-d2d-spike/research/extreme-memory-reduction.md`
//! Wave 19 section).
//!
//! ## Wave 18 attribution data driving this wave
//!
//! Wave 18 VMMap forensic showed mimalloc owns ~25-30 MB of MEM_PRIVATE
//! distributed across 5-9 segments (4 MB each, matching mimalloc's
//! default segment geometry). Tuning `arena_reserve` / `purge_delay` /
//! `eager_commit` targets the segment count + the lazy-vs-eager commit
//! decision at arena-grow time. Realistic upside per Wave 18 estimate:
//! 2-5 MB Private Bytes if the writes land before
//! `_mi_options_init()`.
//!
//! ## Why raw `link_section` and not the `ctor` crate (Wave 19 ruling)
//!
//! `ctor` is a proc-macro derive crate. Spec §8.1 line 216 (verbatim):
//! "**所有 proc-macro crate（`syn`、`quote`、`proc-macro2`、…）一律禁止
//! 进入依赖图，包括传递依赖**". The §8.1 sanctioned-exception list
//! (`windows-implement` / `windows-interface`) is documented as
//! "永远只允许追加（不删除）" — a one-way door. Path 2 (this file)
//! achieves identical machine code without consuming the door.
//!
//! ## Failure mode (Wave 11 Postmortem already predicted)
//!
//! `.CRT$XCU` is one of several CRT init slots; the linker sorts them
//! alphabetically. mimalloc's own CRT init (which runs
//! `_mi_options_init()`) lives in `.CRT$XCU` too, and which slot wins
//! depends on link order. After writing, we probe-read
//! `mi_option_arena_reserve`; if the read is mimalloc's default
//! (1 GiB = `1024 * 1024` KiB) instead of our 64 MiB write, ctor fired
//! TOO LATE — `_mi_options_init` had already stamped its defaults and
//! our subsequent `mi_option_set` calls were silently absorbed but
//! ignored at use-time (mimalloc reads options into per-arena state at
//! arena init, not on every alloc). In that case we log
//! `WAVE19_CTOR_TOO_LATE\n` to stderr ONCE via raw `WriteFile`
//! (`std::io::stderr` is unsafe pre-main, especially before the
//! `bento-nano-shell` panic hook is installed) and the wave is
//! treated as Case B (no-op). The code stays as future-instrumentation
//! per the Wave 11 Postmortem precedent.
//!
//! ## Option choice rationale
//!
//! Briefing specified four options: `arena_reserve`, `purge_delay`,
//! `eager_commit`, `segment_cache`. Inspection of the upstream header
//! (`libmimalloc-sys-0.1.47/c_src/mimalloc/v2/include/mimalloc.h:393-441`)
//! showed `mi_option_segment_cache` is **DEPRECATED** in mimalloc v2
//! (renamed to `mi_option_deprecated_segment_cache`, slot index 10) —
//! same status as `mi_option_page_reset` (renamed to
//! `mi_option_deprecated_page_reset`). Per briefing rule "**不要**触
//! deprecated options（多余调用是技术债）", we set only the three live
//! options. The `segment_cache` knob is a no-op on the linked
//! mimalloc binary.
//!
//! ## Option index values
//!
//! `libmimalloc-sys 0.1.47` only re-exports a partial subset of the
//! `mi_option_t` enum constants (search confirms only `show_errors`,
//! `show_stats`, `verbose`, `large_os_pages`, `reserve_huge_os_pages`,
//! `reserve_huge_os_pages_at`, `reserve_os_memory`,
//! `eager_commit_delay`, `use_numa_nodes`, `disallow_os_alloc`,
//! `os_tag`, `max_errors`, `max_warnings`, `max_segment_reclaim` are
//! exposed in `extended.rs`). The three values we need —
//! `eager_commit`, `purge_delay`, `arena_reserve` — are computed by
//! direct enumeration of the upstream header
//! (`mimalloc.h:393-441`, zero-indexed). We define them as local
//! constants to keep the values pinned in the comment above each line
//! and to avoid silent breakage if libmimalloc-sys updates re-order the
//! enum (which it cannot under C ABI rules; this is paranoia).

#![allow(non_upper_case_globals)]

use libmimalloc_sys::{mi_collect, mi_option_get, mi_option_set, mi_option_t};

/// `mi_option_eager_commit` — eager commit segments (default 1, lazy = 0).
/// Header position (zero-indexed): 3.
const MI_OPTION_EAGER_COMMIT: mi_option_t = 3;

/// `mi_option_purge_delay` — memory purging is delayed N ms (default 10).
/// Header position: 15.
const MI_OPTION_PURGE_DELAY: mi_option_t = 15;

/// `mi_option_arena_reserve` — initial arena reservation in KiB
/// (default 1 GiB = 1_048_576 KiB). Header position: 23.
const MI_OPTION_ARENA_RESERVE: mi_option_t = 23;

/// 16 MiB in KiB. mimalloc reads `arena_reserve` as a KiB-quantity per the
/// header comment "internally, this value is in KiB; use
/// `mi_option_get_size`".
const ARENA_RESERVE_KIB: core::ffi::c_long = 16 * 1024;

/// Function pointer registered in the `.CRT$XCU` CRT init array. The
/// MSVC CRT walks this array during process start, before
/// `mainCRTStartup` jumps to Rust `main`. The `#[used]` attribute is
/// required to prevent the linker from GCing the static under
/// `/OPT:REF` (spec `.cargo/config.toml:23`).
#[used]
// Rust 2024 edition gates `link_section` as `unsafe(...)` — see
// rust-lang/rust#82117 (RFC 3325). Wrapping is required because placing
// arbitrary bytes in a named section can violate ABI invariants the
// linker assumes (e.g. mis-typed entries in `.CRT$XCU` would crash at
// process start). Our ABI here is correct: `unsafe extern "C" fn()` is
// exactly what the MSVC CRT walks in `.CRT$XCU`.
#[unsafe(link_section = ".CRT$XCU")]
static WAVE19_INIT_PTR: unsafe extern "C" fn() = wave19_mimalloc_init;

/// Tune mimalloc options before the allocator's first allocation.
///
/// SAFETY: This function is called by the MSVC CRT initializer
///         infrastructure during process start-up, before any Rust
///         code (including `main`) runs.
///         - `mi_option_set` is documented thread-safe (mimalloc takes
///           an internal lock on the option store) and idempotent. At
///           this stage the process is single-threaded so locking is
///           effectively a no-op.
///         - `mi_option_get` is non-mutating; no aliasing concerns.
///         - The `WriteFile` path uses an unowned process pseudo-handle
///           returned by `GetStdHandle(STD_ERROR_HANDLE)`, which is
///           valid for the process lifetime and must NOT be
///           `CloseHandle`'d. Failure to write is silent — by design,
///           per briefing "不要为「stderr write 失败」加复杂错误处理 ——
///           CRT init 阶段，失败就是失败".
unsafe extern "C" fn wave19_mimalloc_init() {
    unsafe {
        // Three live tuning writes. Order matches briefing.
        mi_option_set(MI_OPTION_ARENA_RESERVE, ARENA_RESERVE_KIB); // 16 MiB (default 1 GiB)
        mi_option_set(MI_OPTION_PURGE_DELAY, 0); // immediate purge (default 10 ms)
        mi_option_set(MI_OPTION_EAGER_COMMIT, 0); // lazy commit (default 1)

        // Probe: did the writes stick? `_mi_options_init()` may have
        // already initialised the option store from env defaults, in
        // which case our subsequent writes either land in a stale
        // store or are accepted but never re-read at arena init time.
        // Reading back the value tells us whether mimalloc considers
        // our write authoritative.
        let arena_after = mi_option_get(MI_OPTION_ARENA_RESERVE);
        if arena_after != ARENA_RESERVE_KIB {
            // ctor too late — write silently dropped (or accepted but
            // already-cached default wins at arena init). Log once
            // via raw WriteFile; this stderr path is the only one
            // safe to call from CRT init (`std::io::stderr` initialises
            // its mutex lazily and is not guaranteed safe pre-main).
            log_too_late();
        }
    }
}

/// Emit the single-line WAVE19_CTOR_TOO_LATE marker via
/// `OutputDebugStringA`. This works even when the process has no
/// console attached (the `/SUBSYSTEM:WINDOWS` case — Wave 19 forensic
/// confirmed `GetStdHandle(STD_ERROR_HANDLE)` returns `NULL` at
/// CRT-init stage when no console exists, so the previously attempted
/// `WriteFile`-to-stderr path was silently no-op even on the
/// success-detect branch). `OutputDebugStringA` routes to any
/// attached debugger AND to the system-wide debug stream visible via
/// SysInternals DebugView, which is the documented way to inspect
/// pre-`main` diagnostic output for windowed processes.
fn log_too_late() {
    use windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringA;

    // PCSTR in windows-sys 0.59 is `*const u8`; the literal includes
    // the trailing NUL byte required by the ANSI string contract.
    const MSG: &[u8] = b"WAVE19_CTOR_TOO_LATE\n\0";
    // SAFETY: `OutputDebugStringA` accepts a NUL-terminated ANSI string;
    //         the byte literal above includes the trailing NUL. The
    //         call is documented safe from any execution context
    //         including DllMain and CRT initialisers (it is one of
    //         the few `kernel32` APIs explicitly listed as DllMain-safe).
    unsafe {
        OutputDebugStringA(MSG.as_ptr());
    }
}

/// Force mimalloc to collect abandoned/retained segments after startup-only
/// allocations have been released.
pub fn collect_retained_segments() {
    unsafe {
        mi_collect(true);
    }
}
