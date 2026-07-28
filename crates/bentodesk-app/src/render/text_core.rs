use super::*;

impl Renderer {
    pub(super) fn draw_text(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        self.draw_text_aligned(text, rect, color, dwrite::TextAlign::DEFAULT)
    }

    /// #1 step 13 (2026-06-02) — single text drawing entry point with explicit
    /// DWrite alignment. Default text still flows through [`draw_text`], while
    /// icon/glyph fallbacks and other centred chips pass a non-default
    /// [`dwrite::TextAlign`]. This keeps the old isolated `draw_text_centered`
    /// path folded into the same layout builder as every other text run.
    pub(super) fn draw_text_aligned(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
        align: dwrite::TextAlign,
    ) -> Result<(), RenderError> {
        let format = self.text_format.clone();
        self.draw_text_with_format(text, rect, color, &format, align)
    }

    /// RC-4 Gap 3 — single-line variant of `draw_text` that disables DWrite
    /// word-wrap and character-trims with an ellipsis when the glyph run
    /// exceeds `rect.width`. Used by BulkManager action buttons whose
    /// 4-letter Latin labels ("Show", "Move", "Close") were wrapping into
    /// "Sho/w", "Mov", "Clos/e" against the wider YaHei UI fallback metrics.
    pub(super) fn draw_text_no_wrap(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let format = self.text_format.clone();
        // RC-5 Gap A — lazy-create the `…` trimming sign on first paint after
        // a format recreate. Without a sign, `SetTrimming(_, None)` silently
        // drops trailing glyphs and users can't tell the label was clipped.
        if self.ellipsis_sign.is_none() {
            self.ellipsis_sign = Some(dwrite::create_ellipsis_sign(&format)?);
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
            self.ellipsis_sign.as_ref(),
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
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_CLIP);
        }
        Ok(())
    }

    pub(super) fn draw_settings_text(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        self.draw_text_with_style(
            text,
            rect,
            color,
            crate::settings_panel::SETTINGS_TEXT_LABEL_SIZE,
            crate::settings_panel::SETTINGS_TEXT_LABEL_WEIGHT,
            crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
        )
    }

    pub(super) fn draw_settings_group_title(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        self.draw_text_tracked(
            text,
            rect,
            color,
            crate::settings_panel::SETTINGS_GROUP_TITLE_SIZE,
            crate::settings_panel::SETTINGS_GROUP_TITLE_WEIGHT,
            crate::settings_panel::SETTINGS_GROUP_TITLE_TRACKING,
        )
    }

    pub(super) fn draw_settings_text_no_wrap(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        self.draw_text_no_wrap_with_style(
            text,
            rect,
            color,
            crate::settings_panel::SETTINGS_TEXT_VALUE_SIZE,
            crate::settings_panel::SETTINGS_TEXT_VALUE_WEIGHT,
            crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )
    }

    pub(super) fn draw_settings_button_text(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
        size: f32,
        weight: u16,
    ) -> Result<(), RenderError> {
        self.draw_text_no_wrap_with_style(
            text,
            rect,
            color,
            size,
            weight,
            1.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
        )
    }

    pub(super) fn draw_settings_row_value(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        self.draw_text_no_wrap_with_style(
            text,
            rect,
            color,
            crate::settings_panel::SETTINGS_TEXT_VALUE_SIZE,
            crate::settings_panel::SETTINGS_TEXT_VALUE_WEIGHT,
            crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
            dwrite::TextAlign {
                h: dwrite::HAlign::Trailing,
                v: dwrite::VAlign::Center,
            },
        )
    }

    /// #7 §10 parity (2026-06-01) — no-wrap variant of [`draw_text_with_style`].
    ///
    /// `draw_text_with_style` routes through `CreateTextLayout`, which leaves
    /// DWrite's default word-wrapping ON and creates a layout object for every
    /// short label. StackTray/Settings fixed chips are many small single-line
    /// runs; building a layout for each one caused a large DirectWrite private
    /// heap jump on first StackTray open. This helper keeps the cached per-style
    /// `IDWriteTextFormat`, temporarily applies NO_WRAP + explicit alignment,
    /// then uses `ID2D1RenderTarget::DrawText` with clipping. The format is reset
    /// to default wrapping/alignment immediately after the draw so shared cached
    /// formats do not leak state into the regular layout path.
    ///
    /// The old styled path used `sign: None`, so overflow was already a silent
    /// trim. `DrawText` + `D2D1_DRAW_TEXT_OPTIONS_CLIP` preserves that "fit in
    /// one line, clipped if necessary" contract without per-label layout COM
    /// allocation.
    ///
    /// §10: reuses `utf16_scratch` (cleared, never freed) and the bounded format
    /// cache; no new dependency or unbounded text cache.
    /// #1 step 12/13 (2026-06-02) — `align` sets the DWrite text/paragraph
    /// alignment for the run. The origin stays the rect's top-left, so a
    /// `Center` horizontal alignment centres the run WITHIN `rect.width` (the
    /// item-card label centring under its icon) and a `Center` vertical
    /// alignment centres WITHIN `rect.height` (the header title / count badge,
    /// exact instead of the old `(band - size*1.4)/2` baseline approximation).
    /// Pass [`dwrite::TextAlign::DEFAULT`] for the legacy Leading/Near top-left.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_text_no_wrap_with_style(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
        size_pt: f32,
        weight: u16,
        line_height: f32,
        align: dwrite::TextAlign,
    ) -> Result<(), RenderError> {
        self.draw_text_no_wrap_with_style_transformed(
            text,
            rect,
            color,
            size_pt,
            weight,
            line_height,
            align,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_text_no_wrap_with_style_transformed(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
        size_pt: f32,
        weight: u16,
        line_height: f32,
        align: dwrite::TextAlign,
        draw_transform: Option<windows::Foundation::Numerics::Matrix3x2>,
    ) -> Result<(), RenderError> {
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let format = self.text_format_for_style(size_pt, weight, line_height)?;
        let brush = self.solid_brush(color)?;
        let layout_rect = D2D_RECT_F {
            left: rect.x,
            top: rect.y,
            right: rect.right(),
            bottom: rect.bottom(),
        };
        let ctx = self.ctx()?;
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        // SAFETY: `format` is a live DWrite COM object. Set* mutates only this
        // cached format's draw properties; we reset them below before returning.
        unsafe {
            ok(
                "StackText.SetTextAlignment",
                format.SetTextAlignment(direct_text_halign(align)),
            )?;
            ok(
                "StackText.SetParagraphAlignment",
                format.SetParagraphAlignment(direct_text_valign(align)),
            )?;
            ok(
                "StackText.SetWordWrapping",
                format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP),
            )?;
        }
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        // SAFETY: rt/format/brush are live COM interfaces. `utf16_scratch` and
        // `layout_rect` live for the call, and DrawText does not retain them.
        unsafe {
            if let Some(transform) = draw_transform.as_ref() {
                rt.SetTransform(transform);
            }
            rt.DrawText(
                &self.utf16_scratch,
                &format,
                &layout_rect,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            if draw_transform.is_some() {
                let base = self.current_logical_transform_matrix();
                rt.SetTransform(&base);
            }
            ok(
                "StackText.ResetTextAlignment",
                format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING),
            )?;
            ok(
                "StackText.ResetParagraphAlignment",
                format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR),
            )?;
            ok(
                "StackText.ResetWordWrapping",
                format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP),
            )?;
        }
        Ok(())
    }

    /// Draw full item labels with no wrapping and no generated ellipsis.
    /// Tauri ItemCard's `useTextAbbrGroup` keeps the complete display name and
    /// shrinks the font size toward 8px instead of substituting `...`.
    pub(super) fn draw_item_label_no_wrap(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
        font_px: f32,
    ) -> Result<(), RenderError> {
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let format = self.text_format_for_style(font_px, 400, 1.4)?;
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
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Near,
            },
        )?;
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
        unsafe {
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_CLIP);
        }
        Ok(())
    }

    /// Draw stack bloom petal names with the native two-line clamp budget.
    ///
    /// The geometry layer supplies the fixed two-line title slot; this draw path
    /// keeps DWrite wrapping enabled and applies character trimming with an
    /// ellipsis sign only when the text exceeds that slot.
    pub(super) fn draw_stack_bloom_petal_name(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let format = self.text_format_for_style(
            stack_tray::BLOOM_PETAL_NAME_FONT_PX,
            stack_tray::BLOOM_PETAL_NAME_FONT_WEIGHT,
            stack_tray::BLOOM_PETAL_NAME_LINE_HEIGHT,
        )?;
        if self.bloom_petal_ellipsis_sign.is_none() {
            self.bloom_petal_ellipsis_sign = Some(dwrite::create_ellipsis_sign(&format)?);
        }
        let trim_sign = self.bloom_petal_ellipsis_sign.clone();
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        let layout = dwrite::create_layout_wrapped_trimmed(
            &self.utf16_scratch,
            &format,
            rect.width.max(1.0),
            rect.height.max(1.0),
            trim_sign.as_ref(),
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Near,
            },
        )?;
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
        unsafe {
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_CLIP);
        }
        Ok(())
    }

    /// G5 (2026-06-01) — measure the laid-out width of `text` at the given style
    /// (no-wrap, single line) via `IDWriteTextLayout::GetMetrics`. Returns the
    /// `widthIncludingTrailingWhitespace` in DIPs. Used by the stack-capsule
    /// title shrink path. Reuses the cached
    /// per-style format from the LRU + the `utf16_scratch` buffer, so a measure
    /// allocates nothing on the heap (§10). A measure layout is built with a
    /// generous `max_w` so the metric reflects the natural (unclamped) run width.
    pub(super) fn measure_label_width(
        &mut self,
        text: &str,
        size_pt: f32,
        weight: u16,
    ) -> Result<f32, RenderError> {
        use windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS;
        let format = self.text_format_for_style(size_pt, weight, 1.0)?;
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        // Large max_w so NO_WRAP measurement returns the intrinsic run width.
        // Alignment is irrelevant to width measurement → DEFAULT.
        let layout = dwrite::create_layout_no_wrap(
            &self.utf16_scratch,
            &format,
            f32::MAX,
            64.0,
            None,
            dwrite::TextAlign::DEFAULT,
        )?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        // SAFETY: layout is a freshly-created COM interface; GetMetrics writes
        // the out-struct and returns HRESULT only on catastrophic error.
        ok("TextLayout::GetMetrics", unsafe {
            layout.GetMetrics(&mut metrics)
        })?;
        Ok(metrics.widthIncludingTrailingWhitespace)
    }
}
