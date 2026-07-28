use super::*;

impl Renderer {
    /// Draw a collapsed-pill title at the stable readable typography role.
    /// DWrite performs single-line character trimming with an inline ellipsis;
    /// capsule size changes available width, never the perceived text scale.
    pub(super) fn draw_pill_title_ellipsis(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
        font_px: f32,
        tracking_px: f32,
    ) -> Result<(), RenderError> {
        use windows::Win32::Graphics::DirectWrite::{DWRITE_TEXT_RANGE, IDWriteTextLayout1};

        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let format = self.text_format_for_style(font_px, 500, 1.0)?;
        if self.pill_title_ellipsis_sign.is_none() {
            self.pill_title_ellipsis_sign = Some(dwrite::create_ellipsis_sign(&format)?);
        }
        self.utf16_scratch.clear();
        self.utf16_scratch.extend(text.encode_utf16());
        let layout = dwrite::create_layout_no_wrap(
            &self.utf16_scratch,
            &format,
            rect.width.max(1.0),
            rect.height.max(1.0),
            self.pill_title_ellipsis_sign.as_ref(),
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
        )?;
        if tracking_px.abs() > f32::EPSILON {
            let layout1: IDWriteTextLayout1 = ok("TextLayout::cast<TextLayout1>", layout.cast())?;
            let range = DWRITE_TEXT_RANGE {
                startPosition: 0,
                length: self.utf16_scratch.len() as u32,
            };
            // SAFETY: layout1 is private to this draw and the range covers the
            // exact UTF-16 source run retained in `utf16_scratch`.
            unsafe {
                let _ = layout1.SetCharacterSpacing(0.0, tracking_px, 0.0, range);
            }
        }
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: layout and brush remain alive for the immediate D2D call.
        unsafe {
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_CLIP);
        }
        Ok(())
    }

    /// V21-C6 (2026-06-22) — Tauri `StackCapsule` also delegates its title to
    /// `useTextAbbr`: the grid column owns the width, and the label shrinks
    /// before it visually truncates. native previously drew stack titles at a
    /// fixed 13px/600, producing `"Benchmark..."` in the 220px two-member
    /// capsule. This shares the ordinary capsule shrink path while preserving
    /// StackCapsule's typography token (13px / 600 / centered line box).
    pub(super) fn draw_stack_capsule_title_shrink_to_fit_transformed(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
        draw_transform: Option<windows::Foundation::Numerics::Matrix3x2>,
    ) -> Result<(), RenderError> {
        self.draw_title_shrink_to_fit(
            text,
            rect,
            color,
            zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_PX,
            zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_WEIGHT,
            1.2,
            0.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
            draw_transform,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_title_shrink_to_fit(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
        base_px: f32,
        weight: u16,
        line_height: f32,
        tracking: f32,
        align: dwrite::TextAlign,
        draw_transform: Option<windows::Foundation::Numerics::Matrix3x2>,
    ) -> Result<(), RenderError> {
        use windows::Win32::Graphics::DirectWrite::{DWRITE_TEXT_RANGE, IDWriteTextLayout1};
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let avail_w = rect.width;
        let sig = title_shrink_signature(text, avail_w, base_px, weight, tracking);
        // --- Resolve the fit font size (cache → measure-and-shrink) ---------
        let resolved_px = match self.stack_capsule_title_shrink {
            Some((cached_sig, px)) if cached_sig == sig => px,
            _ => {
                // Miss: step the font down until it fits (or hit the floor) via
                // the shared pure `shrink_font_to_fit` stepper. The `measure`
                // closure threads any DWrite error out through `measure_err` so
                // the loop stays panic-free (§11); a measure failure short-
                // circuits the stepper to the floor and is surfaced below.
                let mut measure_err: Option<RenderError> = None;
                let utf16_units = text.encode_utf16().count();
                let resolved = shrink_font_to_fit(base_px, avail_w, |size| {
                    if measure_err.is_some() {
                        // Already failed — report "fits" so the stepper stops
                        // immediately at the current size; the error wins below.
                        return 0.0;
                    }
                    match self.measure_label_width(text, size, weight) {
                        Ok(w) => text_width_with_tracking(w, utf16_units, tracking),
                        Err(e) => {
                            measure_err = Some(e);
                            0.0
                        }
                    }
                });
                if let Some(e) = measure_err {
                    return Err(e);
                }
                self.stack_capsule_title_shrink = Some((sig, resolved));
                resolved
            }
        };
        let format = self.text_format_for_style(resolved_px, weight, line_height)?;
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        let layout = dwrite::create_layout_no_wrap(
            &self.utf16_scratch,
            &format,
            rect.width.max(1.0),
            rect.height.max(1.0),
            None,
            align,
        )?;
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        if tracking.abs() > f32::EPSILON {
            // Letter-spacing via IDWriteTextLayout1 (§15.1 canonical cast).
            let layout1: IDWriteTextLayout1 = ok("TextLayout::cast<TextLayout1>", layout.cast())?;
            let range = DWRITE_TEXT_RANGE {
                startPosition: 0,
                length: self.utf16_scratch.len() as u32,
            };
            // SAFETY: layout1 is freshly created; SetCharacterSpacing only mutates
            // per-instance spacing over the canonical full range.
            unsafe {
                let _ = layout1.SetCharacterSpacing(0.0, tracking, 0.0, range);
                if let Some(transform) = draw_transform.as_ref() {
                    ctx.SetTransform(transform);
                }
                ctx.DrawTextLayout(origin, &layout1, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
                if draw_transform.is_some() {
                    let base = self.current_logical_transform_matrix();
                    ctx.SetTransform(&base);
                }
            }
        } else {
            // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
            unsafe {
                if let Some(transform) = draw_transform.as_ref() {
                    ctx.SetTransform(transform);
                }
                ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
                if draw_transform.is_some() {
                    let base = self.current_logical_transform_matrix();
                    ctx.SetTransform(&base);
                }
            }
        }
        Ok(())
    }

    pub(super) fn draw_text_with_style(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
        size_pt: f32,
        weight: u16,
        line_height: f32,
    ) -> Result<(), RenderError> {
        let format = self.text_format_for_style(size_pt, weight, line_height)?;
        self.draw_text_with_format(text, rect, color, &format, dwrite::TextAlign::DEFAULT)
    }

    pub(super) fn draw_text_with_format(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
        format: &IDWriteTextFormat,
        align: dwrite::TextAlign,
    ) -> Result<(), RenderError> {
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        // Reuse the UTF-16 scratch buffer (spec §10 hot-path).
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        let layout = dwrite::create_layout(
            &self.utf16_scratch,
            format,
            rect.width.max(1.0),
            rect.height.max(1.0),
            align,
        )?;
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
        unsafe {
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        }
        Ok(())
    }

    pub(super) fn text_format_for_style(
        &mut self,
        size_pt: f32,
        weight: u16,
        line_height: f32,
    ) -> Result<IDWriteTextFormat, RenderError> {
        let size_pt = size_pt.max(1.0);
        let weight = dwrite::normalize_font_weight(weight);
        let line_height = dwrite::normalize_line_height(line_height);
        if (self.text_format_size_pt - size_pt).abs() < f32::EPSILON
            && self.text_format_weight == weight
            && (self.text_format_line_height - line_height).abs() < f32::EPSILON
        {
            return Ok(self.text_format.clone());
        }
        if let Some(index) = self.text_format_cache.iter().position(|cached| {
            cached.family == self.text_format_family
                && (cached.size_pt - size_pt).abs() < f32::EPSILON
                && cached.weight == weight
                && (cached.line_height - line_height).abs() < f32::EPSILON
        }) {
            let format = self.text_format_cache[index].format.clone();
            if index + 1 < self.text_format_cache.len() {
                let entry = self.text_format_cache.remove(index);
                self.text_format_cache.push(entry);
            }
            return Ok(format);
        }
        let family = self.text_format_family.clone();
        let format = dwrite::text_format_from_family_name_with_metrics(
            family.as_str(),
            size_pt,
            weight,
            line_height,
            dwrite::locale_zh_cn(),
        )?;
        let entry = CachedTextFormat {
            family,
            size_pt,
            weight,
            line_height,
            format: format.clone(),
        };
        if self.text_format_cache.len() >= TEXT_FORMAT_CACHE_CAPACITY {
            self.text_format_cache.remove(0);
        }
        self.text_format_cache.push(entry);
        Ok(format)
    }

    /// M1i fidelity (2026-05-29) — lazily create/cache the monospace text
    /// format for the §2 source-card path line. Tauri's `.desktop-source-card
    /// __path` uses `font-family: ui-monospace, Consolas, monospace`; Consolas
    /// is the Win10/11 fixed-pitch system font (no bundled `.ttf`, spec §5).
    /// `size_pt` is the path font size in DIP (11). Cached against the size so
    /// a theme swap (which only touches the proportional body font) never
    /// invalidates it. One COM allocation per recreate, zero per frame.
    pub(super) fn ensure_monospace_format(
        &mut self,
        size_pt: f32,
    ) -> Result<IDWriteTextFormat, RenderError> {
        let size_pt = size_pt.max(1.0);
        if let Some(cached) = self.monospace_format.as_ref() {
            if (cached.size_pt - size_pt).abs() < f32::EPSILON {
                return Ok(cached.format.clone());
            }
        }
        // #19-B (2026-05-31) — resolve a MONOSPACE family that DWrite confirms
        // is installed BEFORE creating the format, so a stripped SKU lacking
        // Consolas never falls through `text_format_from_family_name`'s
        // proportional fallback into a wrong-metric body face. Normal Windows
        // has Consolas → identical to before (Q2 pixel-1:1).
        let family = SmolStr::new_static(dwrite::resolve_default_family(
            dwrite::FontRole::Monospace,
            &[
                "Consolas",
                "Cascadia Mono",
                "Cascadia Code",
                "Lucida Console",
                "Courier New",
            ],
        ));
        let format = dwrite::text_format_from_family_name_with_metrics(
            family.as_str(),
            size_pt,
            400,
            1.2,
            dwrite::locale_zh_cn(),
        )?;
        self.monospace_format = Some(CachedTextFormat {
            family,
            size_pt,
            weight: 400,
            line_height: 1.2,
            format: format.clone(),
        });
        // A new monospace format invalidates the monospace `…` sign.
        self.monospace_ellipsis_sign = None;
        Ok(format)
    }

    /// M1i fidelity — draw the §2 source-card path line in the monospace format
    /// with DWrite character-trimming (`…`) when it overflows `rect.width`.
    /// Mirrors Tauri's `overflow: hidden; text-overflow: ellipsis; white-space:
    /// nowrap` on `.desktop-source-card__path`.
    pub(super) fn draw_text_monospace_ellipsis(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
        size_pt: f32,
    ) -> Result<(), RenderError> {
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let format = self.ensure_monospace_format(size_pt)?;
        if self.monospace_ellipsis_sign.is_none() {
            self.monospace_ellipsis_sign = Some(dwrite::create_ellipsis_sign(&format)?);
        }
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        let layout = dwrite::create_layout_no_wrap(
            &self.utf16_scratch,
            &format,
            rect.width.max(1.0),
            rect.height.max(1.0),
            self.monospace_ellipsis_sign.as_ref(),
            dwrite::TextAlign::DEFAULT,
        )?;
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
        unsafe {
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        }
        Ok(())
    }

    /// M6-UI fidelity (2026-05-29) — draw an UPPERCASE, letter-tracked label.
    /// Mirrors Tauri `.theme-group__title { text-transform: uppercase;
    /// letter-spacing: 1px }`. The `text` is upper-cased the same way the
    /// watched badge path does (`to_uppercase()` — a no-op for the CJK zh
    /// headings 圆角玻璃/实心/方角现代/个性, an EN-glyph caps fold otherwise),
    /// and the 1-DIP per-glyph tracking is applied via DWrite
    /// `IDWriteTextLayout1::SetCharacterSpacing` (trailing advance) over the
    /// whole run — the true typographic equivalent of CSS letter-spacing, for
    /// both locales. The `to_uppercase()` allocation matches the already-shipped
    /// badge pattern (§10: the headings paint once per visible frame, not on the
    /// per-item hot path).
    pub(super) fn draw_text_tracked(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
        size_pt: f32,
        weight: u16,
        tracking: f32,
    ) -> Result<(), RenderError> {
        use windows::Win32::Graphics::DirectWrite::{DWRITE_TEXT_RANGE, IDWriteTextLayout1};
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let upper = text.to_uppercase();
        let format = self.text_format_for_style(size_pt, weight, 1.0)?;
        self.utf16_scratch.clear();
        for u in upper.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        let layout = dwrite::create_layout(
            &self.utf16_scratch,
            &format,
            rect.width.max(1.0),
            rect.height.max(1.0),
            dwrite::TextAlign::DEFAULT,
        )?;
        // SetCharacterSpacing lives on IDWriteTextLayout1 — cross-cast per
        // spec §15.1 (canonical Interface::cast). Apply `tracking` as the
        // trailing advance over the entire glyph run; leading + min-advance 0.
        let layout1: IDWriteTextLayout1 = ok("TextLayout::cast<TextLayout1>", layout.cast())?;
        let range = DWRITE_TEXT_RANGE {
            startPosition: 0,
            length: self.utf16_scratch.len() as u32,
        };
        // SAFETY: layout1 is a freshly-created COM interface; SetCharacterSpacing
        // only mutates per-instance spacing state over the canonical full range.
        unsafe {
            let _ = layout1.SetCharacterSpacing(0.0, tracking, 0.0, range);
        }
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
        unsafe {
            ctx.DrawTextLayout(origin, &layout1, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        }
        Ok(())
    }
}
