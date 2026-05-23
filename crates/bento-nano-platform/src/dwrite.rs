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

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_LINE_SPACING_METHOD_UNIFORM,
    DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWriteCreateFactory, IDWriteFactory, IDWriteInlineObject, IDWriteTextFormat,
    IDWriteTextLayout,
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
