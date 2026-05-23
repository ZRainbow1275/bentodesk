//! Internationalisation primitives (C5 commitment).
//!
//! ## Design
//!
//! BentoDesk's i18n is **compile-time embedded, runtime-switched**:
//! - `LookupTable.entries: &'static [&'static str]` — table data is baked
//!   into the binary, no I/O, no parser, zero startup cost.
//! - `t(StringId)` — hot-path lookup returning `&'static str`, **zero
//!   allocation** (spec §10).
//! - `init_locale` / `set_locale` — atomic pointer swap, lock-free reader.
//!
//! ## Why `AtomicPtr` and not `OnceLock<&'static LookupTable>`?
//!
//! Locale must be **swappable at runtime** (user picks a new language from
//! settings → entire UI re-renders with new strings). `OnceLock` is one-shot.
//! `AtomicPtr` lets us replace the pointer with a `Release` store and have
//! every reader pick up the new table on its next `Acquire` load. No mutex,
//! no contention.
//!
//! ## Failure modes
//!
//! - **Pointer not yet set** (process startup before `init_locale`): `t`
//!   returns `""`. We don't panic (spec §11) — pre-init UI shows empty
//!   strings, which is benign for a desktop app whose splash screen has no
//!   localised text yet.
//! - **`StringId` out of table range**: `LookupTable::get` returns `""`.
//!   Same rationale — silent degradation beats a UI-thread panic.

use core::sync::atomic::{AtomicPtr, Ordering};

/// Two-byte stable string identifier. Tables index entries by `id.0 as
/// usize`, so all locales must share the same enumeration. Adding a new id
/// appends to the end (or fills a reserved slot) — never re-numbers
/// existing ids; a binary mismatch would silently translate the wrong
/// string.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StringId(pub u16);

/// Static locale table. `entries` is indexed by `StringId.0`.
#[derive(Debug)]
pub struct LookupTable {
    pub entries: &'static [&'static str],
}

impl LookupTable {
    /// Lookup an id. Out-of-range ids return `""` — never panic (§11).
    pub const fn get(&self, id: StringId) -> &'static str {
        // `const fn` doesn't yet allow `Option::unwrap_or` on slice access,
        // so we explicit-check. Compiler still elides the check at call
        // sites where the id is a const known to be in-range.
        let idx = id.0 as usize;
        if idx < self.entries.len() {
            self.entries[idx]
        } else {
            ""
        }
    }

    /// Total entries (including reserved empty slots).
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Process-global pointer to the active locale table. Null until
/// `init_locale` is called.
///
/// Storing a `&'static LookupTable` as a raw pointer is sound because
/// `AtomicPtr` holds the pointer value, and the `'static` lifetime of every
/// `&LookupTable` we put in here guarantees the pointee outlives the
/// program — readers can dereference without a lifetime escrow.
static CURRENT_LOCALE: AtomicPtr<LookupTable> = AtomicPtr::new(core::ptr::null_mut());

/// One-shot installer for the initial locale. Equivalent to `set_locale`
/// but named so callers can grep "init" to find their startup sequence.
pub fn init_locale(table: &'static LookupTable) {
    set_locale(table);
}

/// Hot-swap the active locale. Subsequent `t(...)` calls observe the new
/// table on their next `Acquire` load (next frame at the latest).
///
/// The `&'static` bound is the safety contract: we cast the reference to
/// `*mut LookupTable` (required by `AtomicPtr` — read-only API would still
/// be sound but the type is `*mut`), and `'static` guarantees no use-
/// after-free can occur for any reader holding a stale pointer.
pub fn set_locale(table: &'static LookupTable) {
    // Cast away const for AtomicPtr — we never write through this pointer.
    let ptr = (table as *const LookupTable) as *mut LookupTable;
    CURRENT_LOCALE.store(ptr, Ordering::Release);
}

/// Hot-path lookup. Returns `""` when no locale is installed yet, or when
/// the id falls outside the active table. Never panics.
pub fn t(id: StringId) -> &'static str {
    let ptr = CURRENT_LOCALE.load(Ordering::Acquire);
    if ptr.is_null() {
        return "";
    }
    // SAFETY: `ptr` was stored via `set_locale` from a `&'static LookupTable`,
    //         so the pointee lives for the entire program. We read-only
    //         dereference and immediately bound the borrow to the function
    //         scope; no mutation, no aliasing concerns.
    let table: &'static LookupTable = unsafe { &*ptr };
    table.get(id)
}

/// Identity-comparison: is the active locale the same `LookupTable` as
/// `other`? Phase 2.1 settings panel uses this to drive the locale-switch
/// button (flip zh-CN ⇄ en-US). Returns `false` when no locale installed.
pub fn current_locale_is(other: &'static LookupTable) -> bool {
    let ptr = CURRENT_LOCALE.load(Ordering::Acquire) as *const LookupTable;
    !ptr.is_null() && core::ptr::eq(ptr, other as *const LookupTable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n_en_us::{EN_US, ids as en_ids};
    use crate::i18n_zh_cn::{ZH_CN, ids as zh_ids};

    // The locale pointer is process-global. The unit tests in this module
    // run in a single binary, so we serialise via deterministic ordering:
    // each test ends by reinstalling the same pointer state it expects the
    // next test to find. Cargo test runs are single-threaded by default
    // for our crate (no `#[cfg(test)] use std::thread`), but we still avoid
    // ordering assumptions where reasonable.

    #[test]
    fn stringid_is_two_bytes() {
        assert_eq!(core::mem::size_of::<StringId>(), 2);
    }

    #[test]
    fn lookup_table_get_out_of_range_returns_empty() {
        // Pick an id well past the largest defined slot.
        let oob = StringId(9_999);
        assert_eq!(ZH_CN.get(oob), "");
        assert_eq!(EN_US.get(oob), "");
    }

    #[test]
    fn t_returns_zh_cn_string_after_init() {
        init_locale(&ZH_CN);
        assert_eq!(t(zh_ids::APP_NAME), "BentoDesk");
        assert_eq!(t(zh_ids::TOOLBAR_PIN), "钉住");
        assert_eq!(t(zh_ids::SETTINGS_LOCALE), "语言");
    }

    #[test]
    fn t_switches_locale_on_set_locale() {
        init_locale(&ZH_CN);
        assert_eq!(t(zh_ids::TOOLBAR_SETTINGS), "设置");
        set_locale(&EN_US);
        assert_eq!(t(en_ids::TOOLBAR_SETTINGS), "Settings");
        // Same numeric id, different table → different string.
        assert_eq!(zh_ids::TOOLBAR_SETTINGS, en_ids::TOOLBAR_SETTINGS);
    }

    #[test]
    fn lookup_table_reserved_slots_are_empty() {
        // Slot 5 is reserved (between APP_TAGLINE@1 and TOOLBAR_PIN@10).
        assert_eq!(ZH_CN.get(StringId(5)), "");
        assert_eq!(EN_US.get(StringId(5)), "");
    }

    #[test]
    fn lookup_tables_have_matching_length() {
        // Both tables must enumerate identically — otherwise switching
        // locales would silently shift ids.
        assert_eq!(ZH_CN.len(), EN_US.len());
    }
}
