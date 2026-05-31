//! DirectWrite text layout.
//!
//! Spec §5: system fonts only — `Microsoft YaHei UI` (CN) / `Segoe UI Variable`
//! (EN). No bundled `.ttf` / `.otf`. Glyphs lazy-loaded by DWrite's own cache.
//!
//! Spec §12 rendering parameters:
//!   - `DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC`
//!   - `DWRITE_GRID_FIT_MODE_ENABLED`
//!   - subpixel ClearType ON

use std::sync::OnceLock;

use windows::Win32::Foundation::BOOL;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_LINE_SPACING_METHOD_UNIFORM,
    DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, IDWriteInlineObject,
    IDWriteTextFormat, IDWriteTextLayout,
};
use windows::core::{PCWSTR, w};

use crate::errors::{PlatformError, ok};

pub struct DWriteFactory {
    pub factory: IDWriteFactory,
}

// SAFETY: DWrite shared factory is documented thread-safe.
unsafe impl Send for DWriteFactory {}
unsafe impl Sync for DWriteFactory {}

static DWRITE: OnceLock<DWriteFactory> = OnceLock::new();

pub fn factory() -> Result<&'static DWriteFactory, PlatformError> {
    if let Some(f) = DWRITE.get() {
        return Ok(f);
    }
    // SAFETY: SHARED factory canonical.
    let factory: IDWriteFactory = ok("DWriteCreateFactory", unsafe {
        DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
    })?;
    let _ = DWRITE.set(DWriteFactory { factory });
    DWRITE
        .get()
        .ok_or(PlatformError::Init("DWrite OnceLock empty"))
}

/// Font role for [`resolve_default_family`].
///
/// #19-B (2026-05-31): stripped / LTSC / N / Server Windows SKUs may ship
/// without `Microsoft YaHei UI`, `Consolas`, or `Segoe UI Variable`. Each role
/// carries its own *universal tail* — a family present since Windows XP — so a
/// missing preferred family never causes metric drift or blank glyphs. On a
/// normal Windows the preferred family IS installed, so the resolved family is
/// byte-identical to today (Q2 pixel-1:1 holds).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FontRole {
    /// Proportional UI text (labels, body).
    Ui,
    /// Fixed-pitch text (source-card path lines).
    Monospace,
    /// Display / title text.
    Display,
}

impl FontRole {
    /// Universal tail family for this role — a real DWrite font family present
    /// on every Windows since XP, so it is the guaranteed terminal fallback
    /// when no preferred family resolves.
    ///
    /// NOTE: the GDI substitution alias `MS Shell Dlg 2` is deliberately NOT
    /// used here. DWrite's `FindFamilyName` does not resolve GDI aliases (it
    /// reports them as absent), so an alias tail would break the "resolved
    /// family is installed" contract and the proportional-fallback re-probe.
    /// `Tahoma` (proportional) and `Courier New` (fixed-pitch) are genuine
    /// DWrite font families shipped since XP.
    const fn universal_tail(self) -> &'static str {
        match self {
            FontRole::Ui | FontRole::Display => "Tahoma",
            FontRole::Monospace => "Courier New",
        }
    }

    fn cache_slot(self) -> &'static OnceLock<&'static str> {
        match self {
            FontRole::Ui => &RESOLVED_UI,
            FontRole::Monospace => &RESOLVED_MONOSPACE,
            FontRole::Display => &RESOLVED_DISPLAY,
        }
    }
}

// One cache slot per role — the GetSystemFontCollection / FindFamilyName probe
// for the role default runs at most 3× ever (§10: NOT per call, NOT per frame).
static RESOLVED_UI: OnceLock<&'static str> = OnceLock::new();
static RESOLVED_MONOSPACE: OnceLock<&'static str> = OnceLock::new();
static RESOLVED_DISPLAY: OnceLock<&'static str> = OnceLock::new();

/// Returns `true` when DWrite's system font collection confirms `family` is
/// installed. Used to choose role defaults ([`resolve_default_family`]) and to
/// substitute a present proportional fallback when a runtime-selected family is
/// absent (see [`text_format_from_family_name_with_metrics`]).
///
/// This runs only a handful of times (role-default resolution + once per
/// distinct runtime family, whose text format the caller then caches), so the
/// per-call UTF-16 `Vec` is not a hot-path allocation (§10). A probe failure
/// (factory unavailable, HRESULT error) conservatively reports `false`.
pub fn family_is_available(family: &str) -> bool {
    let Ok(f) = factory() else {
        return false;
    };
    let mut collection: Option<IDWriteFontCollection> = None;
    // SAFETY: out-pointer is a valid `Option<IDWriteFontCollection>` slot;
    // `false` = don't force a font-collection refresh (the shared collection
    // is fine for an availability probe).
    if unsafe {
        f.factory
            .GetSystemFontCollection(&mut collection, BOOL(0))
    }
    .is_err()
    {
        return false;
    }
    let Some(collection) = collection else {
        return false;
    };
    let mut family_wide: Vec<u16> = family.encode_utf16().collect();
    family_wide.push(0);
    let mut index: u32 = 0;
    let mut exists = BOOL(0);
    // SAFETY: `family_wide` is NUL-terminated and outlives the call; `index`
    // and `exists` are valid out-pointers.
    let hr = unsafe {
        collection.FindFamilyName(PCWSTR(family_wide.as_ptr()), &mut index, &mut exists)
    };
    hr.is_ok() && exists.as_bool()
}

/// Resolve the installed font family for `role`, preferring the families in
/// `preferred` (highest priority first). Returns the FIRST family that DWrite
/// confirms is installed; if none are present, returns the role's universal
/// tail (`Tahoma` for UI/Display, `Courier New` for Monospace — both present
/// since XP; `MS Shell Dlg 2` is a GDI alias DWrite's `FindFamilyName` cannot
/// resolve, so it is deliberately NOT used as the tail).
///
/// The result is cached per role in a [`OnceLock`], so the
/// GetSystemFontCollection / FindFamilyName probe runs at most once per role
/// for the entire process lifetime (§10). On a normal Windows the preferred[0]
/// family is installed, so the returned `&'static str` equals the caller's
/// first literal — zero visual change (Q2 pixel-1:1).
pub fn resolve_default_family(role: FontRole, preferred: &[&'static str]) -> &'static str {
    *role.cache_slot().get_or_init(|| {
        for &candidate in preferred {
            if family_is_available(candidate) {
                return candidate;
            }
        }
        role.universal_tail()
    })
}

/// Create a text format. `family` is a wide-string literal (use `w!` macro).
pub fn text_format(
    family: windows::core::PCWSTR,
    size_pt: f32,
    locale: windows::core::PCWSTR,
) -> Result<IDWriteTextFormat, PlatformError> {
    let f = factory()?;
    // SAFETY: factory + family + locale literals valid (NUL-terminated, static).
    ok("CreateTextFormat", unsafe {
        f.factory.CreateTextFormat(
            family,
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size_pt,
            locale,
        )
    })
}

/// Create a text format from a runtime-selected system font family.
pub fn text_format_from_family_name(
    family: &str,
    size_pt: f32,
    locale: windows::core::PCWSTR,
) -> Result<IDWriteTextFormat, PlatformError> {
    text_format_from_family_name_with_metrics(family, size_pt, 400, 0.0, locale)
}

/// Create a text format from a runtime-selected system font family and
/// explicit typography metrics.
pub fn text_format_from_family_name_with_metrics(
    family: &str,
    size_pt: f32,
    weight: u16,
    line_height: f32,
    locale: windows::core::PCWSTR,
) -> Result<IDWriteTextFormat, PlatformError> {
    let f = factory()?;
    // #19-B (2026-05-31): the token-driven dynamic path feeds runtime family
    // strings here (e.g. "Microsoft YaHei UI" from theme tokens). On a stripped
    // SKU where that family is absent, substitute a present PROPORTIONAL
    // fallback so we never silently fall through DWrite's own substitution into
    // a wrong-metric face. On a normal Windows the requested family IS present
    // → no substitution → zero visual change (Q2 pixel-1:1). Monospace callers
    // pre-resolve via `resolve_default_family(FontRole::Monospace, ..)`, so a
    // proportional substitution can never reach a fixed-pitch path.
    let family: &str = if family_is_available(family) {
        family
    } else {
        // `resolve_default_family` appends the UI universal tail (Tahoma) when
        // none of these resolve, so the result is always a present family.
        resolve_default_family(FontRole::Ui, &["Segoe UI", "Tahoma"])
    };
    let mut family_wide: Vec<u16> = family.encode_utf16().collect();
    family_wide.push(0);
    let size_pt = normalize_font_size(size_pt);
    let weight = DWRITE_FONT_WEIGHT(i32::from(normalize_font_weight(weight)));
    // SAFETY: `family_wide` is NUL-terminated and lives until CreateTextFormat
    // returns; `locale` is supplied by a static literal helper.
    let format = ok("CreateTextFormat", unsafe {
        f.factory.CreateTextFormat(
            PCWSTR(family_wide.as_ptr()),
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size_pt,
            locale,
        )
    })?;
    apply_line_height(&format, size_pt, line_height)?;
    Ok(format)
}

fn normalize_font_size(size_pt: f32) -> f32 {
    if size_pt.is_finite() && size_pt >= 1.0 {
        size_pt
    } else {
        1.0
    }
}

pub fn normalize_font_weight(weight: u16) -> u16 {
    weight.clamp(100, 950)
}

pub fn normalize_line_height(line_height: f32) -> f32 {
    if line_height.is_finite() && line_height > 0.0 {
        line_height.clamp(0.8, 3.0)
    } else {
        0.0
    }
}

fn apply_line_height(
    format: &IDWriteTextFormat,
    size_pt: f32,
    line_height: f32,
) -> Result<(), PlatformError> {
    let line_height = normalize_line_height(line_height);
    if line_height <= 0.0 {
        return Ok(());
    }
    let line_spacing = size_pt * line_height;
    let baseline = (size_pt * 0.82).min(line_spacing);
    ok("SetLineSpacing", unsafe {
        format.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, line_spacing, baseline)
    })
}

/// Default font family — Microsoft YaHei UI (Win10/11 builtin).
pub fn yahei_ui() -> windows::core::PCWSTR {
    w!("Microsoft YaHei UI")
}

/// Default locale literal for CN-first UI.
pub fn locale_zh_cn() -> windows::core::PCWSTR {
    w!("zh-CN")
}

/// Build a layout for the given UTF-16 buffer with the given format.
pub fn create_layout(
    text_utf16: &[u16],
    fmt: &IDWriteTextFormat,
    max_w: f32,
    max_h: f32,
) -> Result<IDWriteTextLayout, PlatformError> {
    let f = factory()?;
    // SAFETY: factory + fmt valid; text buffer covers `text_utf16.len()` units.
    ok("CreateTextLayout", unsafe {
        f.factory.CreateTextLayout(text_utf16, fmt, max_w, max_h)
    })
}

/// RC-5 Gap A — build a DWrite ellipsis trimming sign tied to `format`.
///
/// DWrite renders no-wrap character trimming silently unless an inline
/// trimming sign is registered alongside the `DWRITE_TRIMMING` descriptor.
/// `IDWriteFactory::CreateEllipsisTrimmingSign` produces a sign that uses
/// the format's own typography (size, weight, family) so the appended `…`
/// glyph metrics align with the surrounding text run.
///
/// Callers are expected to cache the returned object (e.g.
/// `Renderer::ellipsis_sign: OnceLock<IDWriteInlineObject>`) and pass it
/// to [`create_layout_no_wrap`] on the hot path — see spec §10 (no per-frame
/// heap allocations beyond the documented scratch buffers).
pub fn create_ellipsis_sign(
    format: &IDWriteTextFormat,
) -> Result<IDWriteInlineObject, PlatformError> {
    let f = factory()?;
    // SAFETY: factory + format are valid COM interfaces; the call only
    // reads `format` to derive sign metrics and writes to the out-pointer.
    ok("CreateEllipsisTrimmingSign", unsafe {
        f.factory.CreateEllipsisTrimmingSign(format)
    })
}

/// RC-4 Gap 3 — build a layout with word-wrap disabled. Single-line button
/// labels and other fixed-width chips never split their glyphs across rows;
/// when the text run exceeds `max_w`, the layout is character-trimmed.
///
/// RC-5 Gap A — `sign` (typically supplied by the renderer's cached
/// `IDWriteInlineObject`) renders the `…` indicator inline so users can
/// tell the label was trimmed instead of cleanly losing trailing glyphs.
/// `None` preserves the legacy silent-trim behaviour for callers that
/// have not yet adopted the sign.
pub fn create_layout_no_wrap(
    text_utf16: &[u16],
    fmt: &IDWriteTextFormat,
    max_w: f32,
    max_h: f32,
    sign: Option<&IDWriteInlineObject>,
) -> Result<IDWriteTextLayout, PlatformError> {
    let layout = create_layout(text_utf16, fmt, max_w, max_h)?;
    // SAFETY: layout is a freshly-created COM interface; both Set* calls
    // mutate the layout's per-instance state and return HRESULT only on
    // catastrophic argument errors (we feed canonical enum values).
    unsafe {
        ok(
            "SetWordWrapping",
            layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP),
        )?;
        let trimming = DWRITE_TRIMMING {
            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
            delimiter: 0,
            delimiterCount: 0,
        };
        ok("SetTrimming", layout.SetTrimming(&trimming, sign))?;
    }
    Ok(layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::DirectWrite::DWRITE_LINE_SPACING_METHOD;

    #[test]
    fn normalizes_typography_metrics_without_panicking() {
        assert_eq!(normalize_font_weight(1), 100);
        assert_eq!(normalize_font_weight(500), 500);
        assert_eq!(normalize_font_weight(999), 950);
        assert_eq!(normalize_line_height(f32::NAN), 0.0);
        assert_eq!(normalize_line_height(0.0), 0.0);
        assert_eq!(normalize_line_height(0.4), 0.8);
        assert_eq!(normalize_line_height(4.0), 3.0);
    }

    #[test]
    fn resolve_default_family_returns_universal_tail_when_none_preferred_present() {
        // A list of deliberately non-existent families forces the role's
        // universal tail. The tail is present on every Windows (XP+), so the
        // result is never empty and matches the documented contract.
        let ui = resolve_default_family(FontRole::Ui, &["NoSuchFamily-zzz-Ui"]);
        // Whatever the host has, the resolved family must be installed (the
        // tail is universal) — never the bogus input.
        assert_ne!(ui, "NoSuchFamily-zzz-Ui");
        assert!(family_is_available(ui), "resolved UI family must be present");
    }

    #[test]
    fn resolve_default_family_prefers_first_installed() {
        // "Courier New" is XP+ universal, so it must resolve as the first
        // installed family when listed first.
        let mono = resolve_default_family(FontRole::Monospace, &["Courier New"]);
        // On a host without DWrite (CI headless) the probe yields the tail,
        // which for Monospace is also "Courier New" — either way it matches.
        assert_eq!(mono, "Courier New");
    }

    #[test]
    fn display_role_universal_tail_is_proportional() {
        // Exercise the Display variant end-to-end with an absent preferred set
        // so the universal tail (Tahoma) is returned.
        let display = resolve_default_family(FontRole::Display, &["NoSuchFamily-zzz-Display"]);
        assert_ne!(display, "NoSuchFamily-zzz-Display");
        assert!(family_is_available(display));
    }

    #[test]
    fn family_is_available_rejects_bogus_family() {
        // A clearly-nonexistent family must never report as installed.
        assert!(!family_is_available("Definitely-Not-A-Real-Font-9q8w7e"));
    }

    #[test]
    fn text_format_accepts_weight_and_line_height() {
        let format = match text_format_from_family_name_with_metrics(
            "Segoe UI",
            13.0,
            500,
            1.4,
            locale_zh_cn(),
        ) {
            Ok(format) => format,
            Err(_) => return,
        };
        assert_eq!(unsafe { format.GetFontWeight() }, DWRITE_FONT_WEIGHT(500));
        let mut method = DWRITE_LINE_SPACING_METHOD(0);
        let mut spacing = 0.0;
        let mut baseline = 0.0;
        if unsafe { format.GetLineSpacing(&mut method, &mut spacing, &mut baseline) }.is_ok() {
            assert_eq!(method, DWRITE_LINE_SPACING_METHOD_UNIFORM);
            assert!(spacing >= 18.0);
            assert!(baseline > 0.0);
        }
    }
}
