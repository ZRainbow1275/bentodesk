//! T-080 / T-081 — icon subsystem ported from
//! `bentodesk/src-tauri/src/icon/`.
//!
//! ## Layout
//!
//! - [`stats`] — atomic hit/miss counters (T-080).
//! - [`cache_tier`] — warm (on-disk) tier with sharded layout (T-081).
//! - [`cache`] — two-tier `IconCache` (hot LRU + warm tier) (T-080).
//! - [`extractor`] — Win32 shell icon extraction (`SHGetFileInfoW`,
//!   `ExtractIconExW`, `IShellLinkW`) → PNG via WIC (T-080).
//! - [`custom_icons`] — user-uploaded SVG/PNG/ICO storage (T-081).
//! - [`protocol`] — direct in-process lookup adapter replacing the 1.x
//!   `bentodesk://icon/...` Tauri custom protocol (T-080).
//! - [`svg_sanitize`] — defensive SVG sanitiser, hand-rolled state
//!   machine (no `regex`) (T-080).
//! - `mod.rs` — public re-exports + `IconError` + hand-rolled LRU + WIC
//!   PNG codec wrappers + uniqueness helper (T-080/081).
//!
//! ## Tauri-bridge replacements
//!
//! - `tauri::AppHandle` → explicit [`IconConfig`] struct (carries
//!   `app_data_dir` only — the 1.x code reached through `AppHandle` to
//!   look this up).
//! - `tauri::AssetResolver` / custom-protocol `Request<Vec<u8>>`
//!   handler → direct `Response`-shaped struct returned from
//!   [`protocol::lookup_icon`] / [`protocol::lookup_custom_icon`]. The
//!   webview is gone in native (single-process Direct2D paint); the
//!   "protocol" survives only as an in-process API the dispatcher can
//!   call (cite ΔE).
//! - `handle.emit("icon:cleared", ())` → caller-supplied
//!   `crossbeam_channel::Sender<IconEvent>` parameter.
//!
//! ## Spec compliance
//!
//! - **§8 forbidden crates** removed:
//!   - `lru` → hand-rolled doubly-linked-list-free LRU using
//!     `VecDeque<String>` for ordering + `HashMap<String, Arc<Vec<u8>>>`
//!     for O(1) access. ~80 LOC. See `lru::HotLru`.
//!   - `image` → WIC (`windows::Win32::Graphics::Imaging`) for PNG
//!     decode/encode. See `wic::{encode_png, decode_png_alpha_check}`.
//!   - `regex` → state-machine sanitiser in `svg_sanitize`.
//!   - `uuid` → `unique_icon_id()` (SystemTime-nanos + atomic counter
//!     + FNV-1a mix → 32-hex chars; matches uuid v4 string length).
//!   - `chrono` → `crate::time::now_rfc3339()` (Wave 5d helper).
//!   - `tempfile` → `std::env::temp_dir()` + `unique_icon_id()` for tests.
//! - **§11** error type: hand-rolled [`IconError`] enum implementing
//!   `Display + core::error::Error`, no `thiserror`.
//! - **§11.1** every `unsafe` block carries a `// SAFETY:` comment.
//! - **§11 ΔB ruling** every public icon struct + the [`IconEvent`] enum
//!   derives `serde::Serialize/Deserialize` for v2.x scripting shape.
//! - **§17** every code path complete; no `todo!()` / `unimplemented!()`.

#![allow(clippy::module_name_repetitions)]

pub mod cache;
pub mod cache_tier;
pub mod custom_icons;
pub mod extractor;
pub mod protocol;
pub mod stats;
pub mod svg_sanitize;

use core::sync::atomic::{AtomicU64, Ordering};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::time::now_rfc3339;

// ─── Public configuration ────────────────────────────────────────────

/// Cheap-to-clone configuration handed to every icon entry point.
///
/// In 1.x these values lived on `AppState.app_data_dir`; the native port
/// hoists them so callers (typically `bentodesk-app::dispatcher`)
/// decide once where the path comes from.
///
/// `app_data_dir` is stored as `SmolStr` (not `PathBuf`) so the struct
/// is `serde::Serialize/Deserialize` without the `serde[std]` feature
/// — the workspace `serde` is `default-features = false` and `PathBuf`
/// requires `std` for serde impls. See [`Self::app_data_path`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconConfig {
    /// `%APPDATA%/BentoDesk/` (or platform equivalent). Used as the
    /// base for `custom_icons/` and `icon_cache/` warm tier.
    pub app_data_dir: SmolStr,
}

impl IconConfig {
    /// Convenience: `app_data_dir` as `&Path` for `std::fs` interop.
    pub fn app_data_path(&self) -> &std::path::Path {
        std::path::Path::new(self.app_data_dir.as_str())
    }

    /// Resolved warm-tier directory: `<app_data_dir>/icon_cache/`.
    pub fn warm_dir(&self) -> PathBuf {
        self.app_data_path().join("icon_cache")
    }

    /// Resolved custom-icons directory: `<app_data_dir>/custom_icons/`.
    pub fn custom_icons_dir(&self) -> PathBuf {
        self.app_data_path().join("custom_icons")
    }
}

// ─── Public events ───────────────────────────────────────────────────

/// Side-channel events emitted from the icon subsystem. Dispatcher
/// receives these via a `crossbeam_channel::Sender<IconEvent>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IconEvent {
    /// `clear_icon_cache` finished. Hot + warm tiers are empty.
    Cleared,
    /// Custom icon uploaded. `uuid` is the storage key; `name` is the
    /// human-facing display name (sanitised).
    CustomUploaded { uuid: String, name: String },
    /// Custom icon deleted by uuid.
    CustomDeleted { uuid: String },
    /// Cache stats snapshot pushed (e.g. on debug-overlay refresh).
    StatsRefreshed(stats::IconCacheStatsSnapshot),
}

// ─── Public error enum (hand-rolled per spec §11) ────────────────────

/// All errors surfaced from the icon subsystem. Hand-rolled with
/// `Display` + `core::error::Error` impls (no `thiserror` per spec §8).
#[derive(Debug)]
pub enum IconError {
    /// SHGetFileInfoW / ExtractIconExW returned no icon for `path`.
    /// `win32_error` is the `GetLastError()` value at the time of failure.
    Extract { path: String, win32_error: u32 },
    /// All extraction strategies for `path` produced an all-transparent
    /// PNG, indicating an invisible / bogus icon.
    AllTransparent { path: String },
    /// COM init / call failed. `ctx` describes the call site.
    Com { ctx: &'static str, message: String },
    /// WIC encode/decode failed.
    Wic { ctx: &'static str, message: String },
    /// std::io error wrapping (manifest read/write, custom-icon file I/O).
    Io { path: PathBuf, message: String },
    /// JSON parse / serialise error in `metadata.json`.
    Json { path: PathBuf, message: String },
    /// Custom-icon upload payload was empty.
    EmptyUpload,
    /// Unsupported custom-icon kind (only `svg`, `png`, `ico` accepted).
    UnsupportedKind { kind: String },
    /// SVG sanitiser rejected the input.
    SvgRejected { reason: String },
    /// PNG validation re-decode failed for an upload.
    InvalidPng { message: String },
    /// ICO decode failed for an upload (no usable frame).
    InvalidIco { message: String },
}

impl core::fmt::Display for IconError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Extract { path, win32_error } => {
                write!(
                    f,
                    "icon extract failed for {path:?} (win32 error 0x{win32_error:08x})"
                )
            }
            Self::AllTransparent { path } => {
                write!(f, "icon for {path:?} was all-transparent (rejected)")
            }
            Self::Com { ctx, message } => write!(f, "COM error in {ctx}: {message}"),
            Self::Wic { ctx, message } => write!(f, "WIC error in {ctx}: {message}"),
            Self::Io { path, message } => write!(f, "I/O error on {}: {message}", path.display()),
            Self::Json { path, message } => {
                write!(f, "JSON error on {}: {message}", path.display())
            }
            Self::EmptyUpload => f.write_str("custom icon upload payload was empty"),
            Self::UnsupportedKind { kind } => write!(f, "unsupported custom icon kind: {kind}"),
            Self::SvgRejected { reason } => write!(f, "SVG rejected by sanitiser: {reason}"),
            Self::InvalidPng { message } => write!(f, "invalid PNG upload: {message}"),
            Self::InvalidIco { message } => write!(f, "invalid ICO upload: {message}"),
        }
    }
}

impl core::error::Error for IconError {}

// ─── Hand-rolled LRU (replaces `lru` crate per spec §8) ──────────────

/// Doubly-linked-list-free LRU map. O(n) on access (move-to-front
/// requires a `VecDeque` rotation), but the cache capacity here is
/// small (default 256 hot entries; 32 MB total bytes ceiling), and the
/// access path competes with file I/O + `ExtractIconExW` round-trips
/// that dwarf any constant-factor savings from a cuter data structure.
///
/// Access pattern matches the 1.x `lru::LruCache`:
/// - `put(k, v)` → insert (or replace), bump to most-recently-used end
/// - `get(k)` → look up + bump to MRU end
/// - `peek(k)` → look up without bump (for byte-budget bookkeeping)
/// - `pop_lru()` → evict + return least-recently-used entry
/// - `pop(k)` → remove + return arbitrary key
/// - `iter()` → all (k, v) pairs in arbitrary order
/// - `resize(new_cap)` → cap hot tier; evictions emitted in LRU order
pub(crate) struct HotLru {
    capacity: usize,
    map: HashMap<String, Arc<Vec<u8>>>,
    /// Front = least-recently-used; back = most-recently-used. Rotation
    /// on access keeps this invariant.
    order: VecDeque<String>,
}

impl HotLru {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            capacity: cap,
            map: HashMap::with_capacity(cap),
            order: VecDeque::with_capacity(cap),
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    /// Look up + bump to MRU end. Clones the `Arc` (cheap). `None` on miss.
    pub fn get(&mut self, key: &str) -> Option<Arc<Vec<u8>>> {
        if !self.map.contains_key(key) {
            return None;
        }
        // Move key to MRU end.
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            if let Some(k) = self.order.remove(pos) {
                self.order.push_back(k);
            }
        }
        self.map.get(key).cloned()
    }

    /// Look up without bump. Used by `IconCache::put` to peek the
    /// previous byte size when replacing an existing entry.
    pub fn peek(&self, key: &str) -> Option<&Arc<Vec<u8>>> {
        self.map.get(key)
    }

    /// Insert or replace. New / replaced entries become MRU.
    pub fn put(&mut self, key: String, value: Arc<Vec<u8>>) {
        if self.map.contains_key(&key) {
            // Refresh ordering for the existing key.
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, value);
    }

    /// Evict the least-recently-used entry. Returns `(key, value)` or
    /// `None` if empty. Used by `IconCache::put_hot_only` when the
    /// total-bytes budget is exceeded.
    pub fn pop_lru(&mut self) -> Option<(String, Arc<Vec<u8>>)> {
        let key = self.order.pop_front()?;
        let value = self.map.remove(&key)?;
        Some((key, value))
    }

    /// Remove an arbitrary key. Returns the value if present.
    pub fn pop(&mut self, key: &str) -> Option<Arc<Vec<u8>>> {
        let value = self.map.remove(key)?;
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
        }
        Some(value)
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    /// Iterate over all (key, value) in arbitrary order. Used by
    /// `IconCache::resize` to recompute the byte total after capping.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Arc<Vec<u8>>)> {
        self.map.iter()
    }

    /// Resize the capacity. Excess entries are evicted in LRU order.
    /// Caller is expected to update its own byte-total accounting via
    /// the returned eviction list.
    pub fn resize(&mut self, new_capacity: usize) -> Vec<Arc<Vec<u8>>> {
        let cap = new_capacity.max(1);
        self.capacity = cap;
        let mut evicted = Vec::new();
        while self.len() > cap {
            if let Some((_k, v)) = self.pop_lru() {
                evicted.push(v);
            } else {
                break;
            }
        }
        evicted
    }
}

// ─── WIC PNG codec wrappers ──────────────────────────────────────────

/// Win32-only PNG decode/encode + transparency check. Replaces the
/// 1.x `image` crate dependency. Each entry point creates a transient
/// `IWICImagingFactory` (cheap; ~µs) so callers don't have to manage
/// COM lifetime themselves.
pub(crate) mod wic {
    use super::IconError;

    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppRGBA,
        IWICBitmapDecoder, IWICBitmapEncoder, IWICBitmapFrameDecode, IWICBitmapFrameEncode,
        IWICImagingFactory, IWICStream, WICBitmapEncoderNoCache,
        WICBitmapPaletteTypeFixedHalftone256, WICDecodeMetadataCacheOnLoad,
    };
    use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, STREAM_SEEK_SET};
    use windows::core::GUID;

    /// Encode raw RGBA8 pixels (`width * height * 4` bytes) as a PNG
    /// byte vector via WIC. Returns `IconError::Wic` on any failure.
    pub fn encode_png(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>, IconError> {
        let expected_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|p| p.checked_mul(4))
            .ok_or_else(|| IconError::Wic {
                ctx: "encode_png/size",
                message: "pixel count overflow".to_string(),
            })?;
        if pixels.len() != expected_len {
            return Err(IconError::Wic {
                ctx: "encode_png/size",
                message: format!(
                    "pixel buffer is {} bytes, expected {}",
                    pixels.len(),
                    expected_len
                ),
            });
        }

        // SAFETY: CoCreateInstance is safe to call. The CLSID + IID pair
        // (`CLSID_WICImagingFactory` + `IWICImagingFactory`) is well-defined.
        // Failure surfaces as a typed `windows::core::Error`.
        let factory: IWICImagingFactory =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
                .map_err(|e| IconError::Wic {
                    ctx: "encode_png/CoCreateInstance",
                    message: e.to_string(),
                })?;

        // SAFETY: a null HGLOBAL asks OLE to allocate a growable backing
        // store. `fDeleteOnRelease=true` transfers that memory's lifetime to
        // the returned stream. IWICStream::InitializeFromMemory is fixed-size
        // and cannot be used as an encoder destination.
        let stream = unsafe { CreateStreamOnHGlobal(HGLOBAL::default(), true) }.map_err(|e| {
            IconError::Wic {
                ctx: "encode_png/CreateStreamOnHGlobal",
                message: e.to_string(),
            }
        })?;

        // SAFETY: Encoder creation; container format = PNG; vendor null.
        let encoder: IWICBitmapEncoder = unsafe {
            factory.CreateEncoder(&GUID_ContainerFormatPng as *const GUID, std::ptr::null())
        }
        .map_err(|e| IconError::Wic {
            ctx: "encode_png/CreateEncoder",
            message: e.to_string(),
        })?;

        // SAFETY: Initialise encoder against the in-memory stream.
        unsafe { encoder.Initialize(&stream, WICBitmapEncoderNoCache) }.map_err(|e| {
            IconError::Wic {
                ctx: "encode_png/Initialize",
                message: e.to_string(),
            }
        })?;

        // SAFETY: CreateNewFrame allocates a new IWICBitmapFrameEncode
        // associated with the encoder.
        let mut frame: Option<IWICBitmapFrameEncode> = None;
        unsafe { encoder.CreateNewFrame(&mut frame, std::ptr::null_mut()) }.map_err(|e| {
            IconError::Wic {
                ctx: "encode_png/CreateNewFrame",
                message: e.to_string(),
            }
        })?;
        let frame = frame.ok_or_else(|| IconError::Wic {
            ctx: "encode_png/frame_null",
            message: "CreateNewFrame returned NULL".to_string(),
        })?;

        // SAFETY: Frame initialise with default props.
        unsafe { frame.Initialize(None) }.map_err(|e| IconError::Wic {
            ctx: "encode_png/frame.Initialize",
            message: e.to_string(),
        })?;

        // SAFETY: SetSize takes plain u32 dimensions.
        unsafe { frame.SetSize(width, height) }.map_err(|e| IconError::Wic {
            ctx: "encode_png/SetSize",
            message: e.to_string(),
        })?;

        // SAFETY: Pixel format negotiation. We request 32bppRGBA; WIC
        // may return a different format if PNG container can't carry
        // it, but for the formats we use this is always honoured.
        let mut pf = GUID_WICPixelFormat32bppRGBA;
        unsafe { frame.SetPixelFormat(&mut pf) }.map_err(|e| IconError::Wic {
            ctx: "encode_png/SetPixelFormat",
            message: e.to_string(),
        })?;

        // SAFETY: WritePixels writes `height` rows of `width*4` bytes
        // from our caller-owned pixel buffer.
        let stride = width * 4;
        unsafe { frame.WritePixels(height, stride, pixels) }.map_err(|e| IconError::Wic {
            ctx: "encode_png/WritePixels",
            message: e.to_string(),
        })?;

        // SAFETY: Commit + flush both the frame and the encoder.
        unsafe { frame.Commit() }.map_err(|e| IconError::Wic {
            ctx: "encode_png/frame.Commit",
            message: e.to_string(),
        })?;
        unsafe { encoder.Commit() }.map_err(|e| IconError::Wic {
            ctx: "encode_png/encoder.Commit",
            message: e.to_string(),
        })?;

        // Read the encoded PNG bytes back out of the growable IStream.
        // SAFETY: Seek the IStream pointer to offset 0.
        unsafe {
            stream
                .Seek(0, STREAM_SEEK_SET, None)
                .map_err(|e| IconError::Wic {
                    ctx: "encode_png/Seek",
                    message: e.to_string(),
                })?;
        }

        let mut out: Vec<u8> = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let mut read: u32 = 0;
            // SAFETY: Read up to `buf.len()` bytes; returns S_OK or S_FALSE.
            let hr = unsafe {
                stream.Read(
                    buf.as_mut_ptr().cast(),
                    buf.len() as u32,
                    Some(&mut read as *mut u32),
                )
            };
            if hr.is_err() {
                return Err(IconError::Wic {
                    ctx: "encode_png/Read",
                    message: format!("hresult 0x{:08x}", hr.0),
                });
            }
            if read == 0 {
                break;
            }
            out.extend_from_slice(&buf[..read as usize]);
        }

        let _ = WICBitmapPaletteTypeFixedHalftone256; // silence unused
        Ok(out)
    }

    /// Decode `png_bytes` as PNG and return `true` if every pixel has
    /// alpha == 0 (an "invisible" icon — the 1.x extractor used this
    /// to detect bogus extractions). On decode failure we return
    /// `false` (do not reject valid icons due to a parser hiccup).
    pub fn decode_png_alpha_check(png_bytes: &[u8]) -> bool {
        // SAFETY: Factory creation (see encode_png).
        let factory: IWICImagingFactory =
            match unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
            {
                Ok(f) => f,
                Err(_) => return false,
            };

        // SAFETY: CreateStream + InitializeFromMemory with the input bytes.
        let stream: IWICStream = match unsafe { factory.CreateStream() } {
            Ok(s) => s,
            Err(_) => return false,
        };
        let init_buf = png_bytes.to_vec();
        // SAFETY: Initialise the stream over our `init_buf`. The buffer
        // outlives the WIC calls below (it lives to the end of this fn).
        if unsafe { stream.InitializeFromMemory(&init_buf) }.is_err() {
            return false;
        }

        // SAFETY: CreateDecoderFromStream uses the GUID for PNG; vendor null.
        let decoder: IWICBitmapDecoder = match unsafe {
            factory.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnLoad)
        } {
            Ok(d) => d,
            Err(_) => return false,
        };

        // SAFETY: GetFrame(0) — PNGs always have exactly one frame.
        let frame: IWICBitmapFrameDecode = match unsafe { decoder.GetFrame(0) } {
            Ok(f) => f,
            Err(_) => return false,
        };

        let (mut w, mut h) = (0u32, 0u32);
        // SAFETY: GetSize writes the two output u32s.
        if unsafe { frame.GetSize(&mut w, &mut h) }.is_err() {
            return false;
        }
        if w == 0 || h == 0 {
            return false;
        }

        let stride = w * 4;
        let total = (stride as usize).saturating_mul(h as usize);
        let mut buf = vec![0u8; total];

        // SAFETY: CopyPixels into our buffer. Pixel format may differ
        // from RGBA8 but for a transparency check we only inspect the
        // alpha byte at offset 3 of each 4-byte chunk; PNG container
        // always reports either RGBA8 or pre-converted, so this is safe.
        if unsafe { frame.CopyPixels(std::ptr::null(), stride, &mut buf) }.is_err() {
            return false;
        }

        buf.chunks_exact(4).all(|p| p[3] == 0)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use windows::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
        };

        #[test]
        fn hglobal_png_stream_grows_and_round_trips() {
            // SAFETY: WIC is COM-based. Balance only a successful apartment
            // initialisation; an existing apartment remains owned by its host.
            let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
            let outcome = (|| -> Result<(Vec<u8>, bool), IconError> {
                let png = encode_png(&[0xE2, 0x44, 0x44, 0xFF], 1, 1)?;
                let transparent = decode_png_alpha_check(&png);
                Ok((png, transparent))
            })();
            if initialized {
                // SAFETY: balances the successful CoInitializeEx above on
                // this test thread after every WIC object has been dropped.
                unsafe { CoUninitialize() };
            }

            let (png, transparent) = outcome.expect("encode one opaque pixel");
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
            assert!(!transparent);
        }
    }
}

// ─── Uniqueness helper (replaces `uuid` per spec §8) ─────────────────

/// 32-hex-char identifier used for custom-icon storage filenames and
/// test scratch dir names. Not cryptographically random — combines
/// SystemTime nanos with an atomic counter through an FNV-1a mix.
/// Collision-resistant within a single process and well under any
/// realistic upload-rate; matches the `uuid::new_v4()` string length.
pub(crate) fn unique_icon_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let bumped = COUNTER.fetch_add(1, Ordering::Relaxed);

    // Two independent FNV-1a passes (different seeds) → 64 bits each
    // → 128 bits → 32 hex chars.
    let mut h1 = 0xcbf2_9ce4_8422_2325u64;
    let mut h2 = 0x9e37_79b9_7f4a_7c15u64;
    for byte in nanos
        .to_le_bytes()
        .iter()
        .chain(bumped.to_le_bytes().iter())
    {
        h1 ^= *byte as u64;
        h1 = h1.wrapping_mul(0x100_0000_01b3);
        h2 = h2.wrapping_add(*byte as u64);
        h2 ^= h2 >> 17;
        h2 = h2.wrapping_mul(0xed5a_d4bb);
    }
    format!("{h1:016x}{h2:016x}")
}

/// Wall-clock timestamp helper delegating to the shared `crate::time`
/// helper. Wrapped here so the icon submodules don't all have to thread
/// the import.
pub(crate) fn now_iso8601() -> String {
    now_rfc3339()
}

// ─── Shared subsystem state (singleton) ──────────────────────────────

/// Lazy-initialised singleton holding the `IconCache`. Constructed on
/// first call to [`init`] and shared across the dispatcher's
/// invocations.
struct IconState {
    cache: Arc<cache::IconCache>,
}

static STATE: std::sync::OnceLock<Mutex<Option<IconState>>> = std::sync::OnceLock::new();

fn state_cell() -> &'static Mutex<Option<IconState>> {
    STATE.get_or_init(|| Mutex::new(None))
}

/// Initialise the icon subsystem with the given config. Must be called
/// once at startup (after COM init) before any `extract_*` /
/// `lookup_*` call. Subsequent calls overwrite the previous config —
/// useful for tests; production callers should call exactly once.
///
/// Returns the shared [`IconCache`] handle so the caller can hand it
/// to dispatcher callbacks that need direct access (e.g. preloader).
pub fn init(config: &IconConfig) -> Arc<cache::IconCache> {
    let cache = Arc::new(cache::IconCache::with_warm_dir(256, config.warm_dir()));
    let cell = state_cell();
    let mut guard = cell.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(IconState {
        cache: Arc::clone(&cache),
    });
    cache
}

/// Access the shared cache. Returns `None` if [`init`] has not been
/// called yet (test harness / boot ordering).
pub fn cache_handle() -> Option<Arc<cache::IconCache>> {
    let cell = state_cell();
    let guard = cell.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().map(|s| Arc::clone(&s.cache))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_icon_id_is_32_hex() {
        let s = unique_icon_id();
        assert_eq!(s.len(), 32);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn unique_icon_id_no_collision_under_burst() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..2048 {
            assert!(seen.insert(unique_icon_id()));
        }
    }

    #[test]
    fn icon_config_warm_and_custom_dirs() {
        let cfg = IconConfig {
            app_data_dir: SmolStr::new("C:/AppData"),
        };
        assert!(cfg.warm_dir().ends_with("icon_cache"));
        assert!(cfg.custom_icons_dir().ends_with("custom_icons"));
    }

    #[test]
    fn hot_lru_basic_put_get_evict() {
        let mut lru = HotLru::new(2);
        lru.put("a".into(), Arc::new(vec![1]));
        lru.put("b".into(), Arc::new(vec![2]));
        assert!(lru.get("a").is_some());
        // After get("a"), b is LRU; inserting c evicts b.
        lru.put("c".into(), Arc::new(vec![3]));
        // We exceed capacity by one — caller must pop_lru() to drain.
        assert_eq!(lru.len(), 3);
        let evicted = lru.pop_lru();
        assert_eq!(evicted.map(|(k, _)| k), Some("b".to_string()));
    }

    #[test]
    fn hot_lru_replace_refreshes_position() {
        let mut lru = HotLru::new(3);
        lru.put("a".into(), Arc::new(vec![1]));
        lru.put("b".into(), Arc::new(vec![2]));
        // Re-put "a" → moves to MRU.
        lru.put("a".into(), Arc::new(vec![10]));
        // Now LRU is "b".
        assert_eq!(lru.pop_lru().map(|(k, _)| k), Some("b".to_string()));
    }

    #[test]
    fn hot_lru_pop_removes_arbitrary_key() {
        let mut lru = HotLru::new(3);
        lru.put("a".into(), Arc::new(vec![1]));
        lru.put("b".into(), Arc::new(vec![2]));
        let v = lru.pop("a");
        assert!(v.is_some());
        assert!(!lru.contains("a"));
        assert!(lru.contains("b"));
    }

    #[test]
    fn icon_error_display_extract() {
        let e = IconError::Extract {
            path: "C:/x.exe".into(),
            win32_error: 5,
        };
        let msg = format!("{e}");
        assert!(msg.contains("x.exe"));
        assert!(msg.contains("0x00000005"));
    }

    #[test]
    fn icon_event_serde_round_trip() {
        let e = IconEvent::CustomUploaded {
            uuid: "abc".into(),
            name: "cat".into(),
        };
        let s = serde_json::to_string(&e).expect("ser");
        let back: IconEvent = serde_json::from_str(&s).expect("de");
        match back {
            IconEvent::CustomUploaded { uuid, name } => {
                assert_eq!(uuid, "abc");
                assert_eq!(name, "cat");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn now_iso8601_emits_z_terminated() {
        let s = now_iso8601();
        assert!(s.ends_with('Z'));
    }
}
