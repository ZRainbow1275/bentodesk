//! `bentodesk-platform::storage` — hand-rolled binary codec for the
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

use bentodesk_zone::{Zone, ZoneId, ZoneItem, ZoneItemId, ZoneList};

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
/// Explicit BentoDesk 2.0 state-dir override for isolated runtime proof.
/// The value is a directory; `zones.bin` is appended by [`appdata_path`].
const STATE_DIR_ENV: &str = "BENTODESK_STATE_DIR";
/// Marker next to the executable in production. Its presence selects an
/// executable-local data directory on the next launch. Under an isolated
/// `BENTODESK_STATE_DIR` proof run the marker is kept inside that isolated
/// directory so a hand-test can never mutate the installed application.
const PORTABLE_MARKER_FILE: &str = ".bentodesk-portable";
/// Executable-local state directory used while portable mode is enabled.
const PORTABLE_DATA_DIR_NAME: &str = "BentoDeskData";

mod paths;

#[cfg(test)]
use paths::state_dir_override_path_from_value;
pub use paths::{
    appdata_path, portable_mode_enabled, quarantine_corrupt, set_portable_mode_enabled,
    state_dir_for_portable_mode,
};

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
    // W14: old builds could persist a stack anchor as another stack's child
    // while keeping its own members. Repair that hidden tree at the storage
    // boundary so existing user state observes the same flat relation as new
    // in-session stack operations.
    zones.flatten_nested_stacks();
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
mod tests;
