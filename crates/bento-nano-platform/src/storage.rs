//! `bento-nano-platform::storage` — hand-rolled binary codec for the
//! `zones.bin` persistence file (Ruling 1).
//!
//! Wire format (little-endian throughout):
//! ```text
//!   magic       [u8; 4]   = b"BNTZ"
//!   version     u16       = 8
//!   count       u32
//!   ╳ count times:
//!     id        u64
//!     title_len u16        (bytes; UTF-8)
//!     title     [u8; len]  (validated UTF-8)
//!     x         i32
//!     y         i32
//!     w         i32
//!     h         i32
//!     zone_flags u8        (v7; bit0 = hidden, v8 bit1 = locked)
//!     alias_len u16        (v8; 0 = none)
//!     alias     [u8; len]
//!     display_mode_len u16 (v8; 0 = none)
//!     display_mode [u8; len]
//!     live_folder_path_len u16 (v9; 0 = none)
//!     live_folder_path [u8; len]
//!     icon_len  u16       (v5; bytes; UTF-8)
//!     icon      [u8; len]
//!     accent_len u16      (v5; 0 = none)
//!     accent    [u8; len]
//!     grid_columns u32    (v5)
//!     capsule_size_len u16 (v5; bytes; UTF-8)
//!     capsule_size [u8; len]
//!     capsule_shape_len u16 (v5; bytes; UTF-8)
//!     capsule_shape [u8; len]
//!     stack_parent u64     (v2; 0 = none)
//!     stack_count  u16     (v2)
//!     stack_members [u64; stack_count] (v2)
//!     item_count u16       (v3)
//!     ╳ item_count times:
//!       item_id   u64
//!       path_len  u16      (bytes; UTF-8)
//!       path      [u8; len]
//!       name_len  u16      (bytes; UTF-8)
//!       name      [u8; len]
//!       icon_len  u16      (bytes; UTF-8)
//!       icon_hash [u8; len]
//!       grid_x    i32
//!       grid_y    i32
//!       flags     u8       (bit0 = is_wide, bit1 = file_missing)
//!       original_len u16   (v4; 0 = none)
//!       original_path [u8; len]
//!       hidden_len u16     (v4; 0 = none)
//!       hidden_path [u8; len]
//!       tag_count u16      (v6)
//!       ╳ tag_count times:
//!         tag_len u16      (bytes; UTF-8)
//!         tag     [u8; len]
//! ```
//!
//! Spec §8 forbids `serde` / `bincode` / `postcard` / `serde_json` — every
//! field is encoded by hand using `to_le_bytes` / `from_le_bytes`. Spec §11
//! forbids panic-shaped operations: every fallible read returns
//! `PlatformError::Storage*`, never `unwrap`.
//!
//! Atomicity: writes go to `zones.bin.tmp` first, then `MoveFileEx` with
//! `MOVEFILE_REPLACE_EXISTING` swaps the file in. Truncated writes leave
//! the previous good copy untouched.

use std::borrow::Cow;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use bento_nano_zone::{Zone, ZoneId, ZoneItem, ZoneItemId, ZoneList};

use crate::errors::PlatformError;

/// Magic header — guards against random-file misidentification.
const MAGIC: [u8; 4] = *b"BNTZ";
/// Wire format version. v2 adds stack relationships; v3 adds per-zone item
/// lists; v4 adds hidden/original item paths for stealth restore; v5 adds
/// zone appearance fields (icon/accent/grid/capsule) ahead of the v2+ stack
/// payload so old files keep decoding with domain defaults; v6 adds per-item
/// user/rules tags; v7 adds per-zone visibility for BulkManager Hide/Show;
/// v8 adds bulk-update metadata (`locked`, `alias`, `display_mode`); v9 adds
/// live-folder binding metadata.
const VERSION: u16 = 9;
const VERSION_V8: u16 = 8;
const VERSION_V7: u16 = 7;
const VERSION_V6: u16 = 6;
const VERSION_V5: u16 = 5;
const VERSION_V4: u16 = 4;
const VERSION_V3: u16 = 3;
const VERSION_V2: u16 = 2;
const VERSION_V1: u16 = 1;
/// Hard cap on `count`. 100 KB / 64 bytes-per-empty-zone ≈ 1.6 K zones, so
/// 16 K (= ~1 MB) is generous and still rejects pathological garbage.
const MAX_ZONES: u32 = 16_384;
/// Hard cap on `title_len`. 1 KB is an order of magnitude beyond any
/// realistic UI label and still bounds memory if the file is truncated.
const MAX_TITLE_BYTES: u16 = 1024;
/// Hard cap on item string fields (path/name/icon hash). Allows extended
/// Windows paths while rejecting runaway corrupt buffers.
const MAX_ITEM_STRING_BYTES: u16 = 2048;
/// Hard cap on per-zone item count. Keeps corrupt files from forcing large
/// allocations during startup.
const MAX_ITEMS_PER_ZONE: u16 = 4096;
/// Hard cap on tags per item. Tags are user metadata, not an unbounded index.
const MAX_TAGS_PER_ITEM: u16 = 64;
/// Hard cap on a zone's width/height (DIP). Far beyond any real monitor, so a
/// legitimate zone is never clamped, but a corrupt blob (e.g. the legacy
/// `w=170667 h=91200` = logical-viewport ×100 that auto-expands to a full-screen
/// click-eating veil) can never brick the UI. Matches the MAX_ZONES /
/// MAX_TITLE_BYTES sanity-limit idiom.
const MAX_ZONE_DIMENSION: i32 = 8192;
/// Sane fallback width applied when a decoded zone's `w` is out of range.
const DEFAULT_ZONE_W: i32 = 320;
/// Sane fallback height applied when a decoded zone's `h` is out of range.
const DEFAULT_ZONE_H: i32 = 220;
/// Explicit state-dir override for isolated selected-stack runtime proof.
/// The value is a directory; `zones.bin` is appended by [`appdata_path`].
const STATE_DIR_ENV: &str = "BENTODESK_NANO_STATE_DIR";

/// Resolve `%APPDATA%\BentoDesk\zones.bin`.
///
/// If `BENTODESK_NANO_STATE_DIR` is set to a non-empty path, that directory is
/// used instead. This is an explicit diagnostic/runtime-proof override so
/// selected-stack hand tests can avoid mutating the user's real BentoDesk data.
///
/// Calls `SHGetKnownFolderPath(FOLDERID_RoamingAppData)`. The directory may
/// not exist yet — `read_zones` treats that the same as an absent file
/// (returns an empty list); `write_zones_atomic` creates it on demand.
pub fn appdata_path() -> Result<PathBuf, PlatformError> {
    use windows_sys::Win32::Foundation::S_OK;
    use windows_sys::Win32::UI::Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath};

    if let Some(path) = state_dir_override_path() {
        return Ok(path);
    }

    let mut raw: *mut u16 = core::ptr::null_mut();
    // SAFETY: SHGetKnownFolderPath canonical signature; `raw` written on
    // success and we free it before returning. KF_FLAG_DEFAULT = 0.
    let hr = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_RoamingAppData as *const _,
            0,
            core::ptr::null_mut(),
            &mut raw,
        )
    };
    if hr != S_OK {
        // Phase 2.2 / Ruling 3c — explicitly skip CoTaskMemFree on the
        // error path. The MS contract says SHGetKnownFolderPath may still
        // hand back an allocation alongside a non-S_OK HRESULT; on the
        // documented success-only path `raw` is non-null and we free it
        // below. Releasing a NULL is benign per the spec, but matching
        // §11 enum-error policy means we never call CoTaskMemFree without
        // a real pointer (and if a future failure mode does leak a buffer
        // we'd rather log + investigate than silently free).
        if !raw.is_null() {
            // SAFETY: SHGetKnownFolderPath promises CoTaskMem-allocated.
            unsafe { windows_sys::Win32::System::Com::CoTaskMemFree(raw as *const _) };
        }
        return Err(PlatformError::Hresult {
            ctx: "SHGetKnownFolderPath",
            hr,
        });
    }

    // Walk the UTF-16 string to its NUL terminator.
    // SAFETY: pointer non-null on S_OK; bounded by the OS-supplied NUL.
    let len = unsafe {
        let mut p = raw;
        let mut n = 0usize;
        while *p != 0 {
            n += 1;
            p = p.add(1);
        }
        n
    };
    // SAFETY: `raw` valid for `len` u16s by construction above.
    let slice: &[u16] = unsafe { core::slice::from_raw_parts(raw, len) };
    let s = String::from_utf16_lossy(slice);

    // SAFETY: free what SHGetKnownFolderPath allocated. Required by docs.
    unsafe { windows_sys::Win32::System::Com::CoTaskMemFree(raw as *const _) };

    let mut path = PathBuf::from(s);
    path.push("BentoDesk");
    path.push("zones.bin");
    Ok(path)
}

fn state_dir_override_path() -> Option<PathBuf> {
    let raw = std::env::var_os(STATE_DIR_ENV)?;
    state_dir_override_path_from_value(raw.as_os_str())
}

fn state_dir_override_path_from_value(raw: &OsStr) -> Option<PathBuf> {
    let text = raw.to_string_lossy();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut path = PathBuf::from(trimmed);
    path.push("zones.bin");
    Some(path)
}

/// Quarantine a corrupt `zones.bin` by renaming it to
/// `zones.bin.corrupt-{millis}` so the user can recover it manually. Best
/// effort — failures are returned but Phase 2.1 callers ignore them
/// (Ruling A: never block the first frame on storage I/O).
pub fn quarantine_corrupt(path: &Path) -> Result<(), PlatformError> {
    if !path.exists() {
        return Ok(());
    }
    let parent = match path.parent() {
        Some(p) => p,
        None => return Err(PlatformError::Storage("path has no parent")),
    };
    let stem = match path.file_name() {
        Some(s) => s.to_string_lossy().into_owned(),
        None => return Err(PlatformError::Storage("path has no file name")),
    };
    // GetSystemTimeAsFileTime gives a monotonic, lock-step millisecond
    // counter without pulling in chrono (forbidden) or `std::time::Instant`
    // (which is monotonic but lacks a wall-clock cast). Plain `SystemTime`
    // is fine — quarantine names are advisory, not load-bearing.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut new_name = String::with_capacity(stem.len() + 24);
    new_name.push_str(&stem);
    new_name.push_str(".corrupt-");
    let _ = core::fmt::Write::write_fmt(&mut new_name, format_args!("{stamp}"));
    let target = parent.join(new_name);
    match fs::rename(path, &target) {
        Ok(()) => Ok(()),
        Err(e) => Err(PlatformError::StorageIo {
            ctx: "rename to quarantine",
            kind: e.kind(),
        }),
    }
}

/// Read `path` into a `ZoneList`. Returns an empty list when the file is
/// absent (first run). Returns `PlatformError::Storage` on bad magic /
/// version mismatch / truncation.
pub fn read_zones(path: &Path) -> Result<ZoneList, PlatformError> {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ZoneList::new());
        }
        Err(e) => {
            return Err(PlatformError::StorageIo {
                ctx: "open zones.bin",
                kind: e.kind(),
            });
        }
    };

    let mut buf = Vec::new();
    if let Err(e) = file.read_to_end(&mut buf) {
        return Err(PlatformError::StorageIo {
            ctx: "read zones.bin",
            kind: e.kind(),
        });
    }
    let mut zones = decode(&buf)?;

    // Wave G1 (2026-05-20) — one-shot migration. RC-1 stopped writing
    // `display_mode = "always"` to the seed scene, but pre-RC-1 zones.bin
    // files in the wild still carry the stale value. `Hover` is the app-
    // level default and matches the Tauri-faithful "collapsed pill at
    // rest" behaviour. We rewrite the offending zones to `None` and
    // attempt an atomic write-back so subsequent loads see the migrated
    // state. Write-back failure is non-fatal: the migrated in-memory
    // copy is still returned, and the next save cycle will eventually
    // persist it.
    if migrate_stale_display_modes(&mut zones) {
        let _ = write_zones_atomic(path, &zones);
    }
    Ok(zones)
}

/// Wave G1 — strip stale `display_mode = Some("always")` left over from
/// pre-RC-1 seed runs. RC-1 (`seed_benchmark_scene.rs:91`) now writes
/// `None`, but existing zones.bin snapshots still carry the explicit
/// override and pin every zone open. Returns `true` when at least one
/// zone was migrated so callers can persist the cleaned state.
///
/// Pure / allocation-free in the happy (no-op) path: each zone's
/// `display_mode` is inspected via `as_deref` (no clone), and the
/// mutation only fires on the stale-value branch.
pub fn migrate_stale_display_modes(zones: &mut ZoneList) -> bool {
    let mut changed = false;
    for zone in zones.iter_mut() {
        if zone.display_mode.as_deref() == Some("always") {
            zone.set_display_mode(None);
            changed = true;
        }
    }
    changed
}

/// Decode the in-memory buffer. Split out of `read_zones` so unit tests can
/// exercise the parser without touching the filesystem.
pub fn decode(buf: &[u8]) -> Result<ZoneList, PlatformError> {
    let mut cur = Cursor::new(buf);
    let magic = cur.take_array::<4>()?;
    if magic != MAGIC {
        return Err(PlatformError::Storage("magic mismatch"));
    }
    let version = cur.take_u16()?;
    if version != VERSION
        && version != VERSION_V8
        && version != VERSION_V7
        && version != VERSION_V6
        && version != VERSION_V4
        && version != VERSION_V5
        && version != VERSION_V3
        && version != VERSION_V2
        && version != VERSION_V1
    {
        return Err(PlatformError::Storage("version unsupported"));
    }
    let count = cur.take_u32()?;
    if count > MAX_ZONES {
        return Err(PlatformError::Storage("count exceeds MAX_ZONES"));
    }

    let mut zones = ZoneList::new();
    for _ in 0..count {
        let id = cur.take_u64()?;
        let title_len = cur.take_u16()?;
        if title_len > MAX_TITLE_BYTES {
            return Err(PlatformError::Storage("title_len exceeds MAX_TITLE_BYTES"));
        }
        let title_bytes = cur.take_slice(title_len as usize)?;
        let title = match core::str::from_utf8(title_bytes) {
            Ok(s) => s.to_owned(),
            Err(_) => return Err(PlatformError::Storage("title is not valid UTF-8")),
        };
        let x = cur.take_i32()?;
        let y = cur.take_i32()?;
        let mut w = cur.take_i32()?;
        let mut h = cur.take_i32()?;
        // Defensive geometry clamp (every load path: read_zones, recovery,
        // tests). A corrupt/oversized zone (e.g. legacy `w=170667 h=91200`,
        // = logical-viewport ×100) would otherwise auto-expand into a
        // full-screen click-eating veil. Reset BOTH dims to a sane default
        // when either is non-positive or beyond MAX_ZONE_DIMENSION. x/y are
        // left intact — off-screen positions are handled by viewport clamping
        // at migration / render time.
        if w <= 0 || h <= 0 || w > MAX_ZONE_DIMENSION || h > MAX_ZONE_DIMENSION {
            w = DEFAULT_ZONE_W;
            h = DEFAULT_ZONE_H;
        }
        let mut zone = Zone::new(ZoneId(id), Cow::Owned(title), x, y, w, h);
        if version >= VERSION_V7 {
            let zone_flags = cur.take_u8()?;
            zone.visible = zone_flags & 0b0000_0001 == 0;
            zone.locked = version >= VERSION_V8 && zone_flags & 0b0000_0010 != 0;
        }
        if version >= VERSION_V8 {
            zone.set_alias(
                cur.take_optional_utf8_string(MAX_ITEM_STRING_BYTES, "zone alias")?
                    .map(Cow::Owned),
            );
            zone.set_display_mode(
                cur.take_optional_utf8_string(MAX_ITEM_STRING_BYTES, "zone display mode")?
                    .map(Cow::Owned),
            );
        }
        if version >= VERSION {
            zone.set_live_folder_path(
                cur.take_optional_utf8_string(MAX_ITEM_STRING_BYTES, "zone live folder path")?
                    .map(Cow::Owned),
            );
        }
        if version >= VERSION_V5 {
            zone.set_icon(Cow::Owned(
                cur.take_utf8_string(MAX_ITEM_STRING_BYTES, "zone icon")?,
            ));
            zone.set_accent_color(
                cur.take_optional_utf8_string(MAX_ITEM_STRING_BYTES, "zone accent color")?
                    .map(Cow::Owned),
            );
            zone.set_grid_columns(cur.take_u32()?);
            zone.set_capsule_size(Cow::Owned(
                cur.take_utf8_string(MAX_ITEM_STRING_BYTES, "zone capsule size")?,
            ));
            zone.set_capsule_shape(Cow::Owned(
                cur.take_utf8_string(MAX_ITEM_STRING_BYTES, "zone capsule shape")?,
            ));
        }
        if version >= VERSION_V2 {
            let parent = cur.take_u64()?;
            if parent != ZoneId::INVALID.0 {
                zone.stack_parent = Some(ZoneId(parent));
            }
            let member_count = cur.take_u16()?;
            if member_count > 256 {
                return Err(PlatformError::Storage("stack member count exceeds limit"));
            }
            for _ in 0..member_count {
                let member = cur.take_u64()?;
                if member != ZoneId::INVALID.0 {
                    zone.stack_members.push(ZoneId(member));
                }
            }
        }
        if version >= VERSION_V3 {
            let item_count = cur.take_u16()?;
            if item_count > MAX_ITEMS_PER_ZONE {
                return Err(PlatformError::Storage("item count exceeds limit"));
            }
            for _ in 0..item_count {
                let item_id = cur.take_u64()?;
                let path = cur.take_utf8_string(MAX_ITEM_STRING_BYTES, "item path")?;
                let name = cur.take_utf8_string(MAX_ITEM_STRING_BYTES, "item name")?;
                let icon_hash = cur.take_utf8_string(MAX_ITEM_STRING_BYTES, "item icon hash")?;
                let grid_x = cur.take_i32()?;
                let grid_y = cur.take_i32()?;
                let flags = cur.take_u8()?;
                let mut item = ZoneItem::new(
                    ZoneItemId(item_id),
                    Cow::Owned(path),
                    Cow::Owned(icon_hash),
                    grid_x,
                    grid_y,
                );
                item.name = Cow::Owned(name);
                item.is_wide = flags & 0b0000_0001 != 0;
                item.file_missing = flags & 0b0000_0010 != 0;
                if version >= VERSION_V4 {
                    item.original_path = cur
                        .take_optional_utf8_string(MAX_ITEM_STRING_BYTES, "item original path")?
                        .map(Cow::Owned);
                    item.hidden_path = cur
                        .take_optional_utf8_string(MAX_ITEM_STRING_BYTES, "item hidden path")?
                        .map(Cow::Owned);
                }
                if version >= VERSION_V6 {
                    let tag_count = cur.take_u16()?;
                    if tag_count > MAX_TAGS_PER_ITEM {
                        return Err(PlatformError::Storage("tag count exceeds limit"));
                    }
                    for _ in 0..tag_count {
                        item.tags.push(Cow::Owned(
                            cur.take_utf8_string(MAX_ITEM_STRING_BYTES, "item tag")?,
                        ));
                    }
                }
                if item.id != ZoneItemId::INVALID {
                    zone.items.push(item);
                }
            }
        }
        zones.add(zone);
    }
    Ok(zones)
}

/// Encode `zones` into a fresh buffer. Inverse of `decode`.
pub fn encode(zones: &ZoneList) -> Vec<u8> {
    // Header (4+2+4) plus per-zone fixed v5 fields.
    let title_bytes: usize = zones.iter().map(|z| z.title.len()).sum();
    let zone_appearance_bytes: usize = zones
        .iter()
        .map(|z| {
            z.icon.len()
                + z.accent_color.as_deref().map_or(0, str::len)
                + z.capsule_size.len()
                + z.capsule_shape.len()
                + z.alias.as_deref().map_or(0, str::len)
                + z.display_mode.as_deref().map_or(0, str::len)
                + z.live_folder_path.as_deref().map_or(0, str::len)
                + 19
        })
        .sum();
    let member_bytes: usize = zones.iter().map(|z| z.stack_members.len() * 8).sum();
    let item_bytes: usize = zones
        .iter()
        .flat_map(|z| z.items.iter())
        .map(|item| {
            item.path.len()
                + item.name.len()
                + item.icon_hash.len()
                + item.original_path.as_deref().map_or(0, str::len)
                + item.hidden_path.as_deref().map_or(0, str::len)
                + item.tags.iter().map(|tag| tag.len() + 2).sum::<usize>()
                + 33
        })
        .sum();
    let mut out = Vec::with_capacity(
        10 + zones.len() * 43 + title_bytes + zone_appearance_bytes + member_bytes + item_bytes,
    );
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(zones.len() as u32).to_le_bytes());
    for z in zones.iter() {
        out.extend_from_slice(&z.id.0.to_le_bytes());
        // Saturating cast: titles longer than u16::MAX get rejected at
        // decode time anyway via MAX_TITLE_BYTES; clamp here so a runaway
        // user input can't corrupt the on-disk length field.
        let title_len = z.title.len().min(MAX_TITLE_BYTES as usize) as u16;
        out.extend_from_slice(&title_len.to_le_bytes());
        out.extend_from_slice(&z.title.as_bytes()[..title_len as usize]);
        out.extend_from_slice(&z.x.to_le_bytes());
        out.extend_from_slice(&z.y.to_le_bytes());
        out.extend_from_slice(&z.w.to_le_bytes());
        out.extend_from_slice(&z.h.to_le_bytes());
        let mut zone_flags = 0u8;
        if !z.visible {
            zone_flags |= 0b0000_0001;
        }
        if z.locked {
            zone_flags |= 0b0000_0010;
        }
        out.push(zone_flags);
        push_optional_utf8_field(&mut out, z.alias.as_deref(), MAX_ITEM_STRING_BYTES);
        push_optional_utf8_field(&mut out, z.display_mode.as_deref(), MAX_ITEM_STRING_BYTES);
        push_optional_utf8_field(
            &mut out,
            z.live_folder_path.as_deref(),
            MAX_ITEM_STRING_BYTES,
        );
        push_utf8_field(&mut out, z.icon.as_ref(), MAX_ITEM_STRING_BYTES);
        push_optional_utf8_field(&mut out, z.accent_color.as_deref(), MAX_ITEM_STRING_BYTES);
        out.extend_from_slice(&z.grid_columns.to_le_bytes());
        push_utf8_field(&mut out, z.capsule_size.as_ref(), MAX_ITEM_STRING_BYTES);
        push_utf8_field(&mut out, z.capsule_shape.as_ref(), MAX_ITEM_STRING_BYTES);
        out.extend_from_slice(&z.stack_parent.unwrap_or(ZoneId::INVALID).0.to_le_bytes());
        let stack_count = z.stack_members.len().min(u16::MAX as usize) as u16;
        out.extend_from_slice(&stack_count.to_le_bytes());
        for member in z.stack_members.iter().take(stack_count as usize) {
            out.extend_from_slice(&member.0.to_le_bytes());
        }
        let item_count = z.items.len().min(u16::MAX as usize) as u16;
        out.extend_from_slice(&item_count.to_le_bytes());
        for item in z.items.iter().take(item_count as usize) {
            out.extend_from_slice(&item.id.0.to_le_bytes());
            push_utf8_field(&mut out, item.path.as_ref(), MAX_ITEM_STRING_BYTES);
            push_utf8_field(&mut out, item.name.as_ref(), MAX_ITEM_STRING_BYTES);
            push_utf8_field(&mut out, item.icon_hash.as_ref(), MAX_ITEM_STRING_BYTES);
            out.extend_from_slice(&item.x.to_le_bytes());
            out.extend_from_slice(&item.y.to_le_bytes());
            let mut flags = 0u8;
            if item.is_wide {
                flags |= 0b0000_0001;
            }
            if item.file_missing {
                flags |= 0b0000_0010;
            }
            out.push(flags);
            push_optional_utf8_field(
                &mut out,
                item.original_path.as_deref(),
                MAX_ITEM_STRING_BYTES,
            );
            push_optional_utf8_field(&mut out, item.hidden_path.as_deref(), MAX_ITEM_STRING_BYTES);
            let tag_count = item.tags.len().min(MAX_TAGS_PER_ITEM as usize) as u16;
            out.extend_from_slice(&tag_count.to_le_bytes());
            for tag in item.tags.iter().take(tag_count as usize) {
                push_utf8_field(&mut out, tag.as_ref(), MAX_ITEM_STRING_BYTES);
            }
        }
    }
    out
}

fn push_utf8_field(out: &mut Vec<u8>, value: &str, max_len: u16) {
    let len = safe_prefix_len(value, max_len as usize) as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&value.as_bytes()[..len as usize]);
}

fn push_optional_utf8_field(out: &mut Vec<u8>, value: Option<&str>, max_len: u16) {
    match value {
        Some(value) => push_utf8_field(out, value, max_len),
        None => out.extend_from_slice(&0u16.to_le_bytes()),
    }
}

fn safe_prefix_len(value: &str, max: usize) -> usize {
    if value.len() <= max {
        return value.len();
    }
    let mut len = max;
    while len > 0 && !value.is_char_boundary(len) {
        len -= 1;
    }
    len
}

/// Write `zones` to `path` atomically via tmp + `MoveFileEx`. Creates the
/// parent directory if missing. Existing file is replaced as one operation
/// so a crash leaves either the old or new copy, never a half-written one.
pub fn write_zones_atomic(path: &Path, zones: &ZoneList) -> Result<(), PlatformError> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent) {
                return Err(PlatformError::StorageIo {
                    ctx: "create_dir_all parent",
                    kind: e.kind(),
                });
            }
        }
    }

    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp_path = PathBuf::from(tmp);

    {
        let mut f = match OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)
        {
            Ok(f) => f,
            Err(e) => {
                return Err(PlatformError::StorageIo {
                    ctx: "open tmp",
                    kind: e.kind(),
                });
            }
        };
        let buf = encode(zones);
        if let Err(e) = f.write_all(&buf) {
            return Err(PlatformError::StorageIo {
                ctx: "write tmp",
                kind: e.kind(),
            });
        }
        if let Err(e) = f.sync_all() {
            return Err(PlatformError::StorageIo {
                ctx: "sync tmp",
                kind: e.kind(),
            });
        }
    }

    move_file_replace(&tmp_path, path)
}

/// Atomic replace via `MoveFileExW(MOVEFILE_REPLACE_EXISTING |
/// MOVEFILE_WRITE_THROUGH)`. Falls back to `fs::rename` on path encoding
/// failure (extremely rare — only if the path can't survive UTF-16).
fn move_file_replace(from: &Path, to: &Path) -> Result<(), PlatformError> {
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let from_w = to_utf16_z(from);
    let to_w = to_utf16_z(to);

    // SAFETY: both buffers NUL-terminated; flags constants from the crate.
    let ok = unsafe {
        MoveFileExW(
            from_w.as_ptr(),
            to_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        // SAFETY: GetLastError canonical, no aliasing concerns.
        let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        return Err(PlatformError::Win32 {
            ctx: "MoveFileExW(zones.bin)",
            code,
        });
    }
    Ok(())
}

fn to_utf16_z(p: &Path) -> Vec<u16> {
    let mut v: Vec<u16> = p.as_os_str().to_string_lossy().encode_utf16().collect();
    v.push(0);
    v
}

// -----------------------------------------------------------------------------
// Internal cursor — bounds-checked little-endian reads with no panics.
// -----------------------------------------------------------------------------

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take_slice(&mut self, n: usize) -> Result<&'a [u8], PlatformError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(PlatformError::Storage("offset overflow"))?;
        if end > self.buf.len() {
            return Err(PlatformError::Storage("buffer underrun"));
        }
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn take_array<const N: usize>(&mut self) -> Result<[u8; N], PlatformError> {
        let s = self.take_slice(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(s);
        Ok(out)
    }

    fn take_u16(&mut self) -> Result<u16, PlatformError> {
        Ok(u16::from_le_bytes(self.take_array::<2>()?))
    }

    fn take_u8(&mut self) -> Result<u8, PlatformError> {
        let s = self.take_slice(1)?;
        Ok(s[0])
    }

    fn take_u32(&mut self) -> Result<u32, PlatformError> {
        Ok(u32::from_le_bytes(self.take_array::<4>()?))
    }

    fn take_i32(&mut self) -> Result<i32, PlatformError> {
        Ok(i32::from_le_bytes(self.take_array::<4>()?))
    }

    fn take_u64(&mut self) -> Result<u64, PlatformError> {
        Ok(u64::from_le_bytes(self.take_array::<8>()?))
    }

    fn take_utf8_string(
        &mut self,
        max_len: u16,
        label: &'static str,
    ) -> Result<String, PlatformError> {
        let len = self.take_u16()?;
        if len > max_len {
            return Err(PlatformError::Storage("string field exceeds limit"));
        }
        let bytes = self.take_slice(len as usize)?;
        match core::str::from_utf8(bytes) {
            Ok(s) => Ok(s.to_owned()),
            Err(_) => {
                let _ = label;
                Err(PlatformError::Storage("string field is not valid UTF-8"))
            }
        }
    }

    fn take_optional_utf8_string(
        &mut self,
        max_len: u16,
        label: &'static str,
    ) -> Result<Option<String>, PlatformError> {
        let value = self.take_utf8_string(max_len, label)?;
        if value.is_empty() {
            Ok(None)
        } else {
            Ok(Some(value))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_zone::{
        DEFAULT_ZONE_CAPSULE_SHAPE, DEFAULT_ZONE_CAPSULE_SIZE, DEFAULT_ZONE_GRID_COLUMNS,
        DEFAULT_ZONE_ICON,
    };

    fn sample() -> ZoneList {
        let mut zl = ZoneList::new();
        zl.add(Zone::new(
            ZoneId(1),
            Cow::Borrowed("Alpha"),
            10,
            20,
            300,
            200,
        ));
        zl.add(Zone::new(
            ZoneId(2),
            Cow::Owned("Β-zone".to_owned()),
            -50,
            0,
            100,
            50,
        ));
        zl.add(Zone::new(
            ZoneId(0xFFFF_FFFF_FFFF_FFFF),
            Cow::Borrowed(""),
            0,
            0,
            1,
            1,
        ));
        zl
    }

    #[test]
    fn state_dir_override_appends_zones_bin_and_ignores_blank_values() {
        assert!(state_dir_override_path_from_value(std::ffi::OsStr::new("   ")).is_none());

        let path = state_dir_override_path_from_value(std::ffi::OsStr::new(
            r" C:\Temp\bento-nano-isolated-state ",
        ))
        .expect("override path");

        assert_eq!(
            path,
            PathBuf::from(r"C:\Temp\bento-nano-isolated-state").join("zones.bin")
        );
    }

    #[test]
    fn roundtrip_encode_decode_preserves_zones() {
        let zl = sample();
        let buf = encode(&zl);
        let res = decode(&buf);
        assert!(res.is_ok(), "decode must succeed: {:?}", res.as_ref().err());
        let back = match res {
            Ok(v) => v,
            Err(_) => return,
        };
        assert_eq!(back.len(), zl.len());
        for (a, b) in zl.iter().zip(back.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.title.as_ref(), b.title.as_ref());
            assert_eq!((a.x, a.y, a.w, a.h), (b.x, b.y, b.w, b.h));
            assert_eq!(a.visible, b.visible);
            assert_eq!(a.stack_parent, b.stack_parent);
            assert_eq!(a.stack_members, b.stack_members);
        }
    }

    #[test]
    fn roundtrip_preserves_stack_relationships() {
        let mut zl = ZoneList::new();
        zl.add(Zone::new(
            ZoneId(1),
            Cow::Borrowed("Parent"),
            0,
            0,
            100,
            100,
        ));
        zl.add(Zone::new(
            ZoneId(2),
            Cow::Borrowed("Child"),
            10,
            10,
            100,
            100,
        ));
        assert!(zl.stack(ZoneId(1), ZoneId(2)));

        let back = decode(&encode(&zl)).expect("decode");
        let parent = back.get(ZoneId(1)).expect("parent");
        let child = back.get(ZoneId(2)).expect("child");
        assert_eq!(parent.stack_members.as_slice(), &[ZoneId(2)]);
        assert_eq!(child.stack_parent, Some(ZoneId(1)));
    }

    #[test]
    fn roundtrip_preserves_zone_items() {
        let mut zl = ZoneList::new();
        zl.add(Zone::new(ZoneId(1), Cow::Borrowed("Items"), 0, 0, 200, 120));
        let item_id = zl
            .add_item(
                ZoneId(1),
                Cow::Owned("C:/Desktop/App.lnk".to_owned()),
                Cow::Owned("hash-1".to_owned()),
            )
            .expect("item id");
        assert!(zl.move_item(ZoneId(1), item_id, 2, 3));
        assert!(zl.mark_item_missing("C:/Desktop/App.lnk", true));

        let back = decode(&encode(&zl)).expect("decode");
        let zone = back.get(ZoneId(1)).expect("zone");
        assert_eq!(zone.items.len(), 1);
        let item = &zone.items[0];
        assert_eq!(item.id.0, item_id.0);
        assert_eq!(item.name.as_ref(), "App");
        assert_eq!(item.path.as_ref(), "C:/Desktop/App.lnk");
        assert_eq!(item.icon_hash.as_ref(), "hash-1");
        assert_eq!((item.x, item.y), (2, 3));
        assert!(item.file_missing);
        assert_eq!(item.original_path.as_deref(), None);
        assert_eq!(item.hidden_path.as_deref(), None);
        assert!(item.tags.is_empty());
    }

    #[test]
    fn roundtrip_preserves_zone_appearance_fields() {
        let mut zone = Zone::new(ZoneId(1), Cow::Borrowed("Styled"), 0, 0, 240, 160);
        zone.set_icon(Cow::Borrowed("folder_open"));
        zone.set_accent_color(Some(Cow::Borrowed("#3b82f6")));
        zone.set_grid_columns(6);
        zone.set_capsule(Cow::Borrowed("large"), Cow::Borrowed("rounded"));
        let mut zl = ZoneList::new();
        zl.add(zone);

        let back = decode(&encode(&zl)).expect("decode");
        let zone = back.get(ZoneId(1)).expect("zone");
        assert_eq!(zone.icon.as_ref(), "folder_open");
        assert_eq!(zone.accent_color.as_deref(), Some("#3b82f6"));
        assert_eq!(zone.grid_columns, 6);
        assert_eq!(zone.capsule_size.as_ref(), "large");
        assert_eq!(zone.capsule_shape.as_ref(), "rounded");
    }

    #[test]
    fn roundtrip_preserves_zone_visibility() {
        let mut hidden = Zone::new(ZoneId(2), Cow::Borrowed("Hidden"), 8, 8, 120, 80);
        hidden.set_visible(false);
        let mut visible = Zone::new(ZoneId(3), Cow::Borrowed("Visible"), 12, 12, 120, 80);
        visible.set_visible(true);
        let mut zl = ZoneList::new();
        zl.add(hidden);
        zl.add(visible);

        let back = decode(&encode(&zl)).expect("decode");
        assert!(!back.get(ZoneId(2)).expect("hidden zone").visible);
        assert!(back.get(ZoneId(3)).expect("visible zone").visible);
    }

    #[test]
    fn roundtrip_preserves_zone_bulk_metadata() {
        let mut zone = Zone::new(ZoneId(5), Cow::Borrowed("Bulk"), 8, 8, 120, 80);
        zone.set_visible(false);
        zone.set_locked(true);
        zone.set_alias(Some(Cow::Borrowed("Trimmed alias")));
        zone.set_display_mode(Some(Cow::Borrowed("click")));
        zone.set_live_folder_path(Some(Cow::Borrowed("C:/Users/HP/Documents/Live")));
        let mut zl = ZoneList::new();
        zl.add(zone);

        let back = decode(&encode(&zl)).expect("decode");
        let zone = back.get(ZoneId(5)).expect("zone");
        assert!(!zone.visible);
        assert!(zone.locked);
        assert_eq!(zone.alias.as_deref(), Some("Trimmed alias"));
        assert_eq!(zone.display_mode.as_deref(), Some("click"));
        assert_eq!(
            zone.live_folder_path.as_deref(),
            Some("C:/Users/HP/Documents/Live")
        );
    }

    #[test]
    fn roundtrip_preserves_hidden_item_paths() {
        let mut zl = ZoneList::new();
        zl.add(Zone::new(ZoneId(1), Cow::Borrowed("Items"), 0, 0, 200, 120));
        let item_id = zl
            .add_item_with_metadata(
                ZoneId(1),
                Cow::Owned("C:/Users/HP/Desktop/.bentodesk/1/App.lnk".to_owned()),
                Some("C:/Users/HP/Desktop/App.lnk"),
                Cow::Owned("hash-1".to_owned()),
                Some(Cow::Owned("C:/Users/HP/Desktop/App.lnk".to_owned())),
                Some(Cow::Owned(
                    "C:/Users/HP/Desktop/.bentodesk/1/App.lnk".to_owned(),
                )),
            )
            .expect("item id");

        let back = decode(&encode(&zl)).expect("decode");
        let item = back.item(ZoneId(1), item_id).expect("item");
        assert_eq!(item.name.as_ref(), "App");
        assert_eq!(
            item.original_path.as_deref(),
            Some("C:/Users/HP/Desktop/App.lnk")
        );
        assert_eq!(
            item.hidden_path.as_deref(),
            Some("C:/Users/HP/Desktop/.bentodesk/1/App.lnk")
        );
    }

    #[test]
    fn roundtrip_preserves_item_tags() {
        let mut zl = ZoneList::new();
        zl.add(Zone::new(
            ZoneId(1),
            Cow::Borrowed("Tagged"),
            0,
            0,
            200,
            120,
        ));
        let item_id = zl
            .add_item(
                ZoneId(1),
                Cow::Owned("C:/Users/HP/Desktop/Contract.pdf".to_owned()),
                Cow::Owned("hash-1".to_owned()),
            )
            .expect("item id");
        {
            let item = zl
                .get_mut(ZoneId(1))
                .expect("zone")
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .expect("item");
            item.tags.push(Cow::Borrowed("urgent"));
            item.tags.push(Cow::Borrowed("client-a"));
        }

        let back = decode(&encode(&zl)).expect("decode");
        let item = back.item(ZoneId(1), item_id).expect("item");
        let tags: Vec<&str> = item.tags.iter().map(|tag| tag.as_ref()).collect();
        assert_eq!(tags, vec!["urgent", "client-a"]);
    }

    #[test]
    fn decode_v1_zone_defaults_stack_fields() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&VERSION_V1.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&7u64.to_le_bytes());
        buf.extend_from_slice(&4u16.to_le_bytes());
        buf.extend_from_slice(b"Zone");
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&2i32.to_le_bytes());
        buf.extend_from_slice(&3i32.to_le_bytes());
        buf.extend_from_slice(&4i32.to_le_bytes());

        let zones = decode(&buf).expect("v1 decode");
        let zone = zones.get(ZoneId(7)).expect("zone");
        assert!(zone.visible);
        assert!(!zone.locked);
        assert!(zone.alias.is_none());
        assert!(zone.display_mode.is_none());
        assert!(zone.live_folder_path.is_none());
        assert_eq!(zone.stack_parent, None);
        assert!(zone.stack_members.is_empty());
        assert!(zone.items.is_empty());
    }

    #[test]
    fn decode_clamps_corrupt_oversized_zone_geometry() {
        // Reproduces the legacy corruption: a zone persisted with
        // `w=170667 h=91200` (= logical-viewport ×100) auto-expanded into a
        // full-screen click-eating veil. `decode` must reset BOTH dims to the
        // sane default so the load can never brick the UI.
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&VERSION_V1.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&42u64.to_le_bytes());
        buf.extend_from_slice(&4u16.to_le_bytes());
        buf.extend_from_slice(b"Zone");
        buf.extend_from_slice(&11i32.to_le_bytes()); // x preserved
        buf.extend_from_slice(&22i32.to_le_bytes()); // y preserved
        buf.extend_from_slice(&170_667i32.to_le_bytes()); // corrupt w
        buf.extend_from_slice(&91_200i32.to_le_bytes()); // corrupt h

        let zones = decode(&buf).expect("decode corrupt blob");
        let zone = zones.get(ZoneId(42)).expect("zone");
        assert_eq!(zone.x, 11, "x must be preserved");
        assert_eq!(zone.y, 22, "y must be preserved");
        assert_eq!(zone.w, super::DEFAULT_ZONE_W, "corrupt w must reset");
        assert_eq!(zone.h, super::DEFAULT_ZONE_H, "corrupt h must reset");
        assert!(zone.w <= super::MAX_ZONE_DIMENSION);
        assert!(zone.h <= super::MAX_ZONE_DIMENSION);
    }

    #[test]
    fn decode_clamps_nonpositive_zone_geometry() {
        // A zero/negative dimension is equally fatal (degenerate body) — reset
        // BOTH dims to the sane default.
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&VERSION_V1.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&7u64.to_le_bytes());
        buf.extend_from_slice(&4u16.to_le_bytes());
        buf.extend_from_slice(b"Zone");
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes()); // w = 0
        buf.extend_from_slice(&(-5i32).to_le_bytes()); // h < 0

        let zones = decode(&buf).expect("decode degenerate blob");
        let zone = zones.get(ZoneId(7)).expect("zone");
        assert_eq!(zone.w, super::DEFAULT_ZONE_W);
        assert_eq!(zone.h, super::DEFAULT_ZONE_H);
    }

    #[test]
    fn decode_keeps_in_range_geometry_intact() {
        // A legitimately-sized zone must pass through untouched.
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&VERSION_V1.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&5u64.to_le_bytes());
        buf.extend_from_slice(&4u16.to_le_bytes());
        buf.extend_from_slice(b"Zone");
        buf.extend_from_slice(&30i32.to_le_bytes());
        buf.extend_from_slice(&40i32.to_le_bytes());
        buf.extend_from_slice(&512i32.to_le_bytes());
        buf.extend_from_slice(&384i32.to_le_bytes());

        let zones = decode(&buf).expect("decode in-range blob");
        let zone = zones.get(ZoneId(5)).expect("zone");
        assert_eq!(zone.w, 512, "in-range w must be preserved");
        assert_eq!(zone.h, 384, "in-range h must be preserved");
    }

    fn encode_legacy_zone_without_appearance(version: u16) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&version.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&7u64.to_le_bytes());
        buf.extend_from_slice(&4u16.to_le_bytes());
        buf.extend_from_slice(b"Zone");
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&2i32.to_le_bytes());
        buf.extend_from_slice(&3i32.to_le_bytes());
        buf.extend_from_slice(&4i32.to_le_bytes());
        if version >= VERSION_V2 {
            buf.extend_from_slice(&ZoneId::INVALID.0.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
        }
        if version >= VERSION_V3 {
            buf.extend_from_slice(&0u16.to_le_bytes());
        }
        buf
    }

    fn encode_v7_zone_with_visibility(hidden: bool) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&VERSION_V7.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&7u64.to_le_bytes());
        buf.extend_from_slice(&4u16.to_le_bytes());
        buf.extend_from_slice(b"Zone");
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&2i32.to_le_bytes());
        buf.extend_from_slice(&3i32.to_le_bytes());
        buf.extend_from_slice(&4i32.to_le_bytes());
        buf.push(if hidden { 0b0000_0001 } else { 0 });
        push_utf8_field(&mut buf, DEFAULT_ZONE_ICON, MAX_ITEM_STRING_BYTES);
        push_optional_utf8_field(&mut buf, None, MAX_ITEM_STRING_BYTES);
        buf.extend_from_slice(&DEFAULT_ZONE_GRID_COLUMNS.to_le_bytes());
        push_utf8_field(&mut buf, DEFAULT_ZONE_CAPSULE_SIZE, MAX_ITEM_STRING_BYTES);
        push_utf8_field(&mut buf, DEFAULT_ZONE_CAPSULE_SHAPE, MAX_ITEM_STRING_BYTES);
        buf.extend_from_slice(&ZoneId::INVALID.0.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf
    }

    #[test]
    fn decode_v1_to_v4_zone_defaults_appearance_fields() {
        for version in [VERSION_V1, VERSION_V2, VERSION_V3, VERSION_V4] {
            let zones = decode(&encode_legacy_zone_without_appearance(version)).expect("decode");
            let zone = zones.get(ZoneId(7)).expect("zone");
            assert_eq!(zone.icon.as_ref(), DEFAULT_ZONE_ICON, "version={version}");
            assert_eq!(zone.accent_color.as_deref(), None, "version={version}");
            assert_eq!(
                zone.grid_columns, DEFAULT_ZONE_GRID_COLUMNS,
                "version={version}"
            );
            assert_eq!(
                zone.capsule_size.as_ref(),
                DEFAULT_ZONE_CAPSULE_SIZE,
                "version={version}"
            );
            assert_eq!(
                zone.capsule_shape.as_ref(),
                DEFAULT_ZONE_CAPSULE_SHAPE,
                "version={version}"
            );
            assert!(!zone.locked, "version={version}");
            assert!(zone.alias.is_none(), "version={version}");
            assert!(zone.display_mode.is_none(), "version={version}");
            assert!(zone.live_folder_path.is_none(), "version={version}");
        }
    }

    #[test]
    fn decode_v7_zone_defaults_bulk_metadata_but_preserves_visibility() {
        let zones = decode(&encode_v7_zone_with_visibility(true)).expect("v7 decode");
        let zone = zones.get(ZoneId(7)).expect("zone");
        assert!(!zone.visible);
        assert!(!zone.locked);
        assert!(zone.alias.is_none());
        assert!(zone.display_mode.is_none());
        assert!(zone.live_folder_path.is_none());
    }

    fn encode_v8_zone_with_bulk_metadata() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&VERSION_V8.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&8u64.to_le_bytes());
        buf.extend_from_slice(&4u16.to_le_bytes());
        buf.extend_from_slice(b"Zone");
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&2i32.to_le_bytes());
        buf.extend_from_slice(&3i32.to_le_bytes());
        buf.extend_from_slice(&4i32.to_le_bytes());
        buf.push(0b0000_0010);
        push_optional_utf8_field(&mut buf, Some("Alias"), MAX_ITEM_STRING_BYTES);
        push_optional_utf8_field(&mut buf, Some("click"), MAX_ITEM_STRING_BYTES);
        push_utf8_field(&mut buf, DEFAULT_ZONE_ICON, MAX_ITEM_STRING_BYTES);
        push_optional_utf8_field(&mut buf, None, MAX_ITEM_STRING_BYTES);
        buf.extend_from_slice(&DEFAULT_ZONE_GRID_COLUMNS.to_le_bytes());
        push_utf8_field(&mut buf, DEFAULT_ZONE_CAPSULE_SIZE, MAX_ITEM_STRING_BYTES);
        push_utf8_field(&mut buf, DEFAULT_ZONE_CAPSULE_SHAPE, MAX_ITEM_STRING_BYTES);
        buf.extend_from_slice(&ZoneId::INVALID.0.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf
    }

    #[test]
    fn decode_v8_zone_defaults_live_folder_but_preserves_bulk_metadata() {
        let zones = decode(&encode_v8_zone_with_bulk_metadata()).expect("v8 decode");
        let zone = zones.get(ZoneId(8)).expect("zone");
        assert!(zone.visible);
        assert!(zone.locked);
        assert_eq!(zone.alias.as_deref(), Some("Alias"));
        assert_eq!(zone.display_mode.as_deref(), Some("click"));
        assert!(zone.live_folder_path.is_none());
    }

    #[test]
    fn decode_v3_item_defaults_hidden_paths() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&VERSION_V3.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&3u64.to_le_bytes());
        buf.extend_from_slice(&5u16.to_le_bytes());
        buf.extend_from_slice(b"Items");
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&200i32.to_le_bytes());
        buf.extend_from_slice(&120i32.to_le_bytes());
        buf.extend_from_slice(&ZoneId::INVALID.0.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&1u64.to_le_bytes());
        push_utf8_field(&mut buf, "C:/Desktop/a.txt", MAX_ITEM_STRING_BYTES);
        push_utf8_field(&mut buf, "a.txt", MAX_ITEM_STRING_BYTES);
        push_utf8_field(&mut buf, "hash", MAX_ITEM_STRING_BYTES);
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.push(0u8);

        let zones = decode(&buf).expect("v3 decode");
        let item = zones.item(ZoneId(3), ZoneItemId(1)).expect("item");
        assert_eq!(item.original_path.as_deref(), None);
        assert_eq!(item.hidden_path.as_deref(), None);
    }

    #[test]
    fn decode_v2_zone_defaults_item_list() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&VERSION_V2.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&9u64.to_le_bytes());
        buf.extend_from_slice(&5u16.to_le_bytes());
        buf.extend_from_slice(b"Stack");
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&2i32.to_le_bytes());
        buf.extend_from_slice(&3i32.to_le_bytes());
        buf.extend_from_slice(&4i32.to_le_bytes());
        buf.extend_from_slice(&ZoneId::INVALID.0.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());

        let zones = decode(&buf).expect("v2 decode");
        let zone = zones.get(ZoneId(9)).expect("zone");
        assert!(zone.stack_members.is_empty());
        assert!(zone.items.is_empty());
    }

    #[test]
    fn read_zones_returns_empty_when_file_absent() {
        let mut p = std::env::temp_dir();
        p.push("bento-nano-test-missing.bin");
        // Make sure it isn't there.
        let _ = fs::remove_file(&p);
        let res = read_zones(&p);
        assert!(
            res.is_ok(),
            "missing file must yield Ok(empty), got {:?}",
            res.err()
        );
        let zl = match res {
            Ok(v) => v,
            Err(_) => return,
        };
        assert!(zl.is_empty());
    }

    #[test]
    fn decode_corrupt_magic_is_storage_err() {
        let mut buf = encode(&sample());
        buf[0] = b'X';
        let res = decode(&buf);
        assert!(
            matches!(res, Err(PlatformError::Storage("magic mismatch"))),
            "expected Storage(magic mismatch), got {res:?}"
        );
    }

    #[test]
    fn decode_version_mismatch_is_storage_err() {
        let mut buf = encode(&sample());
        buf[4] = 99; // version low byte
        buf[5] = 0;
        let res = decode(&buf);
        assert!(
            matches!(res, Err(PlatformError::Storage("version unsupported"))),
            "expected Storage(version unsupported), got {res:?}"
        );
    }

    #[test]
    fn decode_truncated_buffer_is_storage_err() {
        let buf = encode(&sample());
        // Lop off the last byte — the inner cursor must report underrun.
        let res = decode(&buf[..buf.len() - 1]);
        assert!(
            matches!(res, Err(PlatformError::Storage(_))),
            "expected Storage underrun, got {res:?}"
        );
    }

    #[test]
    fn write_then_read_atomic_roundtrips() {
        let mut p = std::env::temp_dir();
        p.push("bento-nano-test-rt.bin");
        let _ = fs::remove_file(&p);

        let zl = sample();
        let wres = write_zones_atomic(&p, &zl);
        assert!(wres.is_ok(), "write_zones_atomic failed: {:?}", wres.err());

        let rres = read_zones(&p);
        assert!(rres.is_ok(), "read_zones failed: {:?}", rres.as_ref().err());
        let back = match rres {
            Ok(v) => v,
            Err(_) => return,
        };
        assert_eq!(back.len(), zl.len());
        let _ = fs::remove_file(&p);
    }

    /// Wave G1 (2026-05-20) — the migration helper itself: stale
    /// `Some("always")` is rewritten to `None`; other values
    /// (`Some("hover")`, `Some("custom")`, already-`None`) are untouched;
    /// `changed` reports correctly so callers can skip the write-back on
    /// no-op loads.
    #[test]
    fn migrate_stale_display_modes_rewrites_only_always_to_none() {
        let mut zones = ZoneList::new();
        let mut z_always = Zone::new(ZoneId(1), Cow::Borrowed("stale-always"), 0, 0, 100, 100);
        z_always.set_display_mode(Some(Cow::Borrowed("always")));
        let mut z_hover = Zone::new(ZoneId(2), Cow::Borrowed("hover-keep"), 0, 0, 100, 100);
        z_hover.set_display_mode(Some(Cow::Borrowed("hover")));
        let mut z_custom = Zone::new(ZoneId(3), Cow::Borrowed("custom-keep"), 0, 0, 100, 100);
        z_custom.set_display_mode(Some(Cow::Borrowed("custom")));
        let z_none = Zone::new(ZoneId(4), Cow::Borrowed("already-none"), 0, 0, 100, 100);
        zones.add(z_always);
        zones.add(z_hover);
        zones.add(z_custom);
        zones.add(z_none);

        let changed = migrate_stale_display_modes(&mut zones);
        assert!(
            changed,
            "migration must report a change when 'always' present"
        );

        let modes: Vec<Option<String>> = zones
            .iter()
            .map(|z| z.display_mode.as_deref().map(|s| s.to_owned()))
            .collect();
        assert_eq!(
            modes,
            vec![
                None,
                Some("hover".to_owned()),
                Some("custom".to_owned()),
                None,
            ]
        );

        // Idempotent — a second pass reports no change.
        let again = migrate_stale_display_modes(&mut zones);
        assert!(!again, "migration must be no-op when nothing is stale");
    }

    /// Wave G1 — end-to-end: a zones.bin with a stale `display_mode =
    /// "always"` is migrated when loaded through `read_zones`, AND the
    /// cleaned state is persisted back to disk so subsequent loads see
    /// the migrated value without re-running the migration. Mirrors the
    /// user's hand-test scenario where Zone 5 in the seed file refused to
    /// collapse to a pill on hover-leave.
    #[test]
    fn loaded_zones_with_always_mode_are_migrated_to_none() {
        let mut p = std::env::temp_dir();
        p.push("bento-nano-test-display-mode-migration.bin");
        let _ = fs::remove_file(&p);

        // Seed: encode a ZoneList that has a stale "always" zone.
        let mut zones = ZoneList::new();
        let mut zone = Zone::new(ZoneId(5), Cow::Borrowed("Stale Zone 5"), 64, 72, 320, 220);
        zone.set_display_mode(Some(Cow::Borrowed("always")));
        zones.add(zone);

        let wres = write_zones_atomic(&p, &zones);
        assert!(wres.is_ok(), "seed write failed: {:?}", wres.err());

        // Load — expect display_mode to be None after migration.
        let loaded = read_zones(&p).expect("load seeded zones.bin");
        assert_eq!(loaded.len(), 1);
        let migrated = loaded.iter().next().expect("one zone in loaded list");
        assert!(
            migrated.display_mode.is_none(),
            "stale 'always' must be migrated to None, got {:?}",
            migrated.display_mode
        );

        // Second load — the write-back during the first load means the
        // on-disk file no longer has "always", so the second load should
        // also produce None and should NOT trigger another migration.
        let reloaded = read_zones(&p).expect("reload after write-back");
        let z2 = reloaded.iter().next().expect("one zone reloaded");
        assert!(
            z2.display_mode.is_none(),
            "persisted file must be clean after first migration, got {:?}",
            z2.display_mode
        );

        let _ = fs::remove_file(&p);
    }
}
