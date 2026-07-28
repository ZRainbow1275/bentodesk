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

    // The locale pointer is process-global. Keep all pointer mutations in one
    // test so Cargo's parallel test runner cannot interleave locale changes.

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
    fn t_initializes_and_switches_locale() {
        init_locale(&ZH_CN);
        assert_eq!(t(zh_ids::APP_NAME), "BentoDesk");
        assert_eq!(t(zh_ids::TOOLBAR_PIN), "钉住");
        assert_eq!(t(zh_ids::SETTINGS_LOCALE), "语言");
        set_locale(&EN_US);
        assert_eq!(t(en_ids::TOOLBAR_SETTINGS), "Settings");
        // Same numeric id, different table → different string.
        assert_eq!(zh_ids::TOOLBAR_SETTINGS, en_ids::TOOLBAR_SETTINGS);
    }

    /// #19-B (2026-05-31) — ZH_CN/EN_US lockstep contract. Because #19-B
    /// defaults non-Chinese-OS users to English, the two tables must stay in
    /// perfect structural lockstep: identical length AND identical empty/non-
    /// empty pattern at every index. A blank slot in one but not the other
    /// would render an empty label for that locale only — this test catches
    /// any future drift (a string added to one table but not the other) at
    /// build time. `entries` is read directly (no process-global install) to
    /// stay ordering-free, mirroring the other `lookup_table_*` tests.
    #[test]
    fn zh_cn_en_us_empty_slots_are_in_lockstep() {
        assert_eq!(
            ZH_CN.len(),
            EN_US.len(),
            "locale tables must enumerate identically"
        );
        for i in 0..ZH_CN.len() {
            assert_eq!(
                ZH_CN.entries[i].is_empty(),
                EN_US.entries[i].is_empty(),
                "ZH_CN/EN_US emptiness mismatch at index {i}: \
                 zh={:?} en={:?}",
                ZH_CN.entries[i],
                EN_US.entries[i]
            );
        }
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

    /// M1a 2026-05-29 — id 141 `SETTING_PORTABLE_MODE` was appended to BOTH
    /// tables (the Tauri General row 5, `SettingsPanel.tsx:294`). A blank slot
    /// would render an empty toggle label in the panel, so pin that both
    /// locales carry a non-empty string at this id. `.get()` reads the table
    /// directly (no process-global locale install) to stay ordering-free,
    /// mirroring the `lookup_table_*` tests above. `zh_ids` and `en_ids` are
    /// the same numeric id by construction (the length test enforces parity).
    #[test]
    fn setting_portable_mode_id_141_present_in_both_locales() {
        assert_eq!(zh_ids::SETTING_PORTABLE_MODE, en_ids::SETTING_PORTABLE_MODE);
        assert!(
            !ZH_CN.get(zh_ids::SETTING_PORTABLE_MODE).is_empty(),
            "zh-CN SETTING_PORTABLE_MODE (id 141) must not be blank"
        );
        assert!(
            !EN_US.get(en_ids::SETTING_PORTABLE_MODE).is_empty(),
            "en-US SETTING_PORTABLE_MODE (id 141) must not be blank"
        );
    }

    #[test]
    fn stack_tray_overlay_strings_are_present_in_both_locales() {
        let ids = [
            (
                zh_ids::BULK_MANAGER_COL_ITEMS,
                en_ids::BULK_MANAGER_COL_ITEMS,
            ),
            (zh_ids::STACK_DISSOLVE, en_ids::STACK_DISSOLVE),
            (zh_ids::STACK_MEMBERS_LABEL, en_ids::STACK_MEMBERS_LABEL),
            (zh_ids::STACK_DETACH_MEMBER, en_ids::STACK_DETACH_MEMBER),
            (zh_ids::STACK_MORE_MEMBERS, en_ids::STACK_MORE_MEMBERS),
            (
                zh_ids::STACK_MORE_STACK_MEMBERS,
                en_ids::STACK_MORE_STACK_MEMBERS,
            ),
            (zh_ids::STACK_REORDER_HINT, en_ids::STACK_REORDER_HINT),
            (zh_ids::FOCUSED_PREVIEW_TITLE, en_ids::FOCUSED_PREVIEW_TITLE),
            (zh_ids::FOCUSED_PREVIEW_EMPTY, en_ids::FOCUSED_PREVIEW_EMPTY),
            (
                zh_ids::STACK_DIMENSION_SEPARATOR,
                en_ids::STACK_DIMENSION_SEPARATOR,
            ),
            (zh_ids::STACK_PREVIEW_ACTIVE, en_ids::STACK_PREVIEW_ACTIVE),
        ];

        for (zh_id, en_id) in ids {
            assert_eq!(zh_id, en_id);
            assert!(
                !ZH_CN.get(zh_id).is_empty(),
                "zh-CN StackTray overlay string {zh_id:?} must not be blank"
            );
            assert!(
                !EN_US.get(en_id).is_empty(),
                "en-US StackTray overlay string {en_id:?} must not be blank"
            );
        }
        assert_eq!(ZH_CN.get(zh_ids::STACK_MEMBERS_LABEL), "成员");
        assert_eq!(EN_US.get(en_ids::STACK_MEMBERS_LABEL), "Members");
        assert_eq!(ZH_CN.get(zh_ids::STACK_PREVIEW_ACTIVE), "预览中");
        assert_eq!(EN_US.get(en_ids::STACK_PREVIEW_ACTIVE), "Preview open");
    }
}
