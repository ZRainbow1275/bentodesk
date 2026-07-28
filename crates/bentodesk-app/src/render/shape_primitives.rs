use super::*;

impl Renderer {
    /// Borrow the resident D2D context, or return an error when the surface
    /// has been hibernated. All inner draw helpers funnel through this
    /// accessor so the §11 R5 hibernation guard is one-shot, not scattered.
    pub(super) fn ctx(
        &self,
    ) -> Result<&windows::Win32::Graphics::Direct2D::ID2D1DeviceContext, RenderError> {
        match self.surface.as_ref() {
            Some(s) => Ok(&s.ctx),
            None => Err(RenderError::Platform(
                bentodesk_platform::PlatformError::Init(
                    "Renderer: draw call on hibernated surface (T-099)",
                ),
            )),
        }
    }

    pub(super) fn current_logical_transform_matrix(&self) -> Matrix3x2 {
        self.logical_transform_override
            .unwrap_or_else(|| base_scale_matrix(self.base_scale.max(0.01)))
    }

    pub(super) fn set_logical_transform_override(
        &mut self,
        transform: Option<Matrix3x2>,
    ) -> Result<(), RenderError> {
        self.logical_transform_override = transform;
        let current = self.current_logical_transform_matrix();
        let ctx = self.ctx()?;
        // SAFETY: the D2D context is inside a BeginDraw/EndDraw pair. The
        // matrix is stack-owned and copied by D2D for subsequent draw calls.
        unsafe {
            ctx.SetTransform(&current);
        }
        Ok(())
    }

    pub(super) fn svg_fit_matrix_in_current_transform(
        &self,
        rect: bentodesk_style::Rect,
        view_size: f32,
    ) -> Matrix3x2 {
        let scale = (rect.width / view_size).min(rect.height / view_size);
        let glyph_w = view_size * scale;
        let glyph_h = view_size * scale;
        let dx = rect.x + (rect.width - glyph_w) * 0.5;
        let dy = rect.y + (rect.height - glyph_h) * 0.5;
        let logical = self.current_logical_transform_matrix();
        Matrix3x2 {
            M11: scale * logical.M11,
            M12: 0.0,
            M21: 0.0,
            M22: scale * logical.M22,
            M31: dx * logical.M11 + logical.M31,
            M32: dy * logical.M22 + logical.M32,
        }
    }

    /// Push an axis-aligned D2D clip so subsequent paint is masked to `rect`.
    /// Used by the Settings scrollable body (S-02) so partial rows clip cleanly
    /// at the sticky header/footer edges instead of bleeding past them.
    ///
    /// CRITICAL: every `push_clip` MUST be balanced by exactly one `pop_clip`
    /// before the next `Present` — an unbalanced clip corrupts the device
    /// context. Callers using `?` propagation must capture the clipped paint
    /// into a local and run `pop_clip()` before propagating any error. We use
    /// `D2D1_ANTIALIAS_MODE_ALIASED` (hard pixel edge) so the row/header/footer
    /// boundaries stay crisp; the body band is axis-aligned so there is nothing
    /// to antialias.
    pub(super) fn push_clip(&self, rect: bentodesk_style::Rect) -> Result<(), RenderError> {
        let ctx = self.ctx()?;
        // Spec §15.1 — Interface::cast canonical for COM cross-cast.
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        let clip = D2D_RECT_F {
            left: rect.x,
            top: rect.y,
            right: rect.right(),
            bottom: rect.bottom(),
        };
        // SAFETY: rt valid for the call; `clip` lives until the call returns.
        unsafe {
            rt.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
        }
        Ok(())
    }

    /// Pop the most recent `push_clip`. See `push_clip` for the balancing
    /// contract — leaving a clip pushed corrupts the device context.
    pub(super) fn pop_clip(&self) -> Result<(), RenderError> {
        let ctx = self.ctx()?;
        // Spec §15.1 — Interface::cast canonical for COM cross-cast.
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        // SAFETY: rt valid; pairs with the matching PushAxisAlignedClip.
        unsafe {
            rt.PopAxisAlignedClip();
        }
        Ok(())
    }

    /// Frosted-backdrop — build the per-frame `ID2D1BitmapBrush` from the cached
    /// blurred desktop snapshot. Returns `None` (→ flat-tint degrade) when there
    /// is no backdrop or any COM step fails; NEVER panics (spec § "Degrade
    /// ladder"). Called once per Main-overlay frame by `render()` (spec §10).
    ///
    /// Brush transform: the backdrop bitmap is captured at `region.top_left ==
    /// client logical (0,0)` (the Main overlay IS the primary work area), so the
    /// translation is `(0,0)`; the per-axis scale is
    /// `backdrop_brush_scale(downsample, base_scale) = downsample / base_scale`
    /// — see that helper's derivation. ExtendMode CLAMP both axes so the brush
    /// never tiles past the captured region; LINEAR interpolation for a smooth
    /// upscale of the downsampled source.
    pub(super) fn build_backdrop_brush(
        &self,
        ctx: &windows::Win32::Graphics::Direct2D::ID2D1DeviceContext,
    ) -> Option<ID2D1BitmapBrush> {
        let backdrop = self.backdrop.as_ref()?;
        // Spec §15.1 — Interface::cast canonical for COM cross-cast; degrade on
        // failure rather than `?`-propagating a hard error out of the hot path.
        let rt: ID2D1RenderTarget = ctx.cast().ok()?;
        let props = D2D1_BITMAP_BRUSH_PROPERTIES {
            extendModeX: D2D1_EXTEND_MODE_CLAMP,
            extendModeY: D2D1_EXTEND_MODE_CLAMP,
            interpolationMode: D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
        };
        // SAFETY: rt valid for the call; `backdrop.bitmap` (`ID2D1Bitmap1`)
        //         derefs to the `ID2D1Bitmap` the brush wants; `props` lives on
        //         the stack for the call. `None` brush-properties = identity
        //         opacity + identity transform (we set the transform below).
        let brush = unsafe { rt.CreateBitmapBrush(&backdrop.bitmap, Some(&props), None) }.ok()?;
        let s = backdrop_brush_scale(FROSTED_BACKDROP_DOWNSAMPLE, self.base_scale);
        let transform = windows::Foundation::Numerics::Matrix3x2 {
            M11: s,
            M12: 0.0,
            M21: 0.0,
            M22: s,
            M31: 0.0,
            M32: 0.0,
        };
        // SAFETY: brush valid; `SetTransform` lives on the `ID2D1Brush` base
        //         (the bitmap brush derefs to it); `transform` lives for the
        //         call. Maps bitmap-px → pre-world DIP so the frost lands 1:1
        //         on the wallpaper after the world transform applies base_scale.
        unsafe {
            brush.SetTransform(&transform);
        }
        Some(brush)
    }

    /// Frosted-backdrop unified surface fill (spec § "Renderer plumbing"). When
    /// a per-frame backdrop brush exists, paint the blurred desktop CLIPPED to
    /// the rounded shape, then lay a SINGLE `tint` at the Tauri alpha on top —
    /// real frosted glass. With no brush (degrade / `FROSTED_BACKDROP` off) this
    /// is exactly `fill_rounded_rect(rect, tint, radius)`: one clean flat tint,
    /// NEVER the old double translucent layer (so the murk can never return).
    pub(super) fn fill_frosted_rect(
        &self,
        rect: bentodesk_style::Rect,
        tint: Color,
        radius: BorderRadius,
    ) -> Result<(), RenderError> {
        // The baked capture can exist yet still be transparent on a driver that
        // rejects its effect output. A same-colour underlay guarantees the
        // final surface reaches the fallback opacity; a healthy opaque capture
        // simply covers it before the source Tauri tint is applied.
        if let Some(underlay) = frosted_fallback_underlay(tint) {
            self.fill_rounded_rect(rect, underlay, radius)?;
        }
        if let Some(brush) = self.backdrop_brush.as_ref() {
            if rect.width > 0.0 && rect.height > 0.0 {
                let rr = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: rect.x,
                        top: rect.y,
                        right: rect.right(),
                        bottom: rect.bottom(),
                    },
                    radiusX: radius.top_left,
                    radiusY: radius.top_left,
                };
                let ctx = self.ctx()?;
                // Spec §15.1 — Interface::cast canonical for COM cross-cast.
                let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
                // SAFETY: rt valid; `rr` lives for the call; the bitmap brush is
                //         COM-ref-counted and was built for this frame's ctx.
                unsafe {
                    rt.FillRoundedRectangle(&rr, brush);
                }
            }
        }
        self.fill_rounded_rect(rect, tint, radius)
    }

    /// Apply CSS-like group opacity to the complete frosted surface. Fading
    /// only the tint leaves the captured desktop bitmap fully opaque, which
    /// makes stack emerge/bloom transitions look like a hard black slab.
    pub(super) fn fill_frosted_rect_with_group_opacity(
        &self,
        rect: bentodesk_style::Rect,
        tint: Color,
        radius: BorderRadius,
        opacity: f32,
    ) -> Result<(), RenderError> {
        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        if opacity >= 1.0 - f32::EPSILON {
            return self.fill_frosted_rect(rect, tint, radius);
        }

        if let Some(brush) = self.backdrop_brush.as_ref() {
            let rr = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: rect.x,
                    top: rect.y,
                    right: rect.right(),
                    bottom: rect.bottom(),
                },
                radiusX: radius.top_left,
                radiusY: radius.top_left,
            };
            let ctx = self.ctx()?;
            let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
            let backdrop_opacity = frosted_group_backdrop_opacity(tint.a, opacity);
            // SAFETY: brush/rt are valid for this frame. Restore the shared
            // brush to identity opacity before any following surface uses it.
            unsafe {
                brush.SetOpacity(backdrop_opacity);
                rt.FillRoundedRectangle(&rr, brush);
                brush.SetOpacity(1.0);
            }
        }

        self.fill_rounded_rect(rect, fade_color(tint, opacity), radius)
    }

    pub(super) fn fill_rounded_rect(
        &self,
        rect: bentodesk_style::Rect,
        color: Color,
        radius: BorderRadius,
    ) -> Result<(), RenderError> {
        if color.a <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let brush = self.solid_brush(color)?;
        let rr = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: rect.x,
                top: rect.y,
                right: rect.right(),
                bottom: rect.bottom(),
            },
            radiusX: radius.top_left,
            radiusY: radius.top_left,
        };
        let ctx = self.ctx()?;
        // Spec §15.1 — Interface::cast canonical for COM cross-cast.
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        // SAFETY: rt valid; rr lives for the call; brush COM-ref-counted.
        unsafe {
            rt.FillRoundedRectangle(&rr, &brush);
        }
        Ok(())
    }

    /// M6b — paint a multi-layer [`ShadowStack`] under `base` as a simulated
    /// soft fill (the grow-and-fill idiom, no D2D blur effect on the hot path).
    /// Layers draw back-to-front so the inner surface lift sits under the
    /// dominant outer drop.
    ///
    /// #3 step 10 (2026-06-02) — each layer is FEATHERED instead of stamped as
    /// one crisp full-alpha rounded rect. A real CSS `box-shadow: 0 4px 16px ...`
    /// spreads its alpha across a 16–48px Gaussian gradient that is near-zero at
    /// the panel edge; the old single grow-and-fill put the full token alpha
    /// right up to a sharp rectangle boundary, so the expanded zone's 2-layer
    /// shadow read as a hard "extra border" ring ~16px outside the 1px hairline.
    /// We now paint `FEATHER_BANDS` concentric rects per layer, from the full
    /// grow (faint) inward toward the panel (denser): each band carries
    /// `per_band_alpha = 1 - (1 - A)^(1/N)`, so the N bands that overlap nearest
    /// the panel composite back UP to the token alpha `A` (0x33 / 0x66 kept
    /// EXACTLY), while the outer edge — covered by only the first band — fades to
    /// `per_band_alpha`, giving the soft blur falloff. A spread-only ring
    /// (`blur == 0`, e.g. `terminal`'s `0 0 0 1px`) keeps its single crisp fill.
    /// Allocation-free: a fixed stack-`f32` loop, reusing `fill_rounded_rect`
    /// (§10). An empty stack (`flat`/`brutalism`/`editorial`) is a no-op.
    pub(super) fn fill_rounded_rect_vertical_gradient(
        &mut self,
        rect: bentodesk_style::Rect,
        top: Color,
        bottom: Color,
        radius: BorderRadius,
    ) -> Result<(), RenderError> {
        self.fill_rounded_rect_linear_gradient(
            rect,
            top,
            bottom,
            radius,
            vertical_gradient_props(rect),
        )
    }

    pub(super) fn fill_rounded_rect_linear_gradient(
        &mut self,
        rect: bentodesk_style::Rect,
        start: Color,
        end: Color,
        radius: BorderRadius,
        props: D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
    ) -> Result<(), RenderError> {
        if (start.a <= 0.0 && end.a <= 0.0) || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let brush = self.linear_gradient_brush(props, start, end)?;
        let rr = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: rect.x,
                top: rect.y,
                right: rect.right(),
                bottom: rect.bottom(),
            },
            radiusX: radius.top_left,
            radiusY: radius.top_left,
        };
        let ctx = self.ctx()?;
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        unsafe {
            rt.FillRoundedRectangle(&rr, &brush);
        }
        Ok(())
    }

    pub(super) fn linear_gradient_brush(
        &mut self,
        props: D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
        top: Color,
        bottom: Color,
    ) -> Result<ID2D1LinearGradientBrush, RenderError> {
        let needs_rebuild = match self.linear_gradient_brush.as_ref() {
            Some(cached) => cached.top != top || cached.bottom != bottom,
            None => true,
        };
        if needs_rebuild {
            let stops = [d2d_gradient_stop(0.0, top), d2d_gradient_stop(1.0, bottom)];
            let ctx = self.ctx()?;
            let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
            let stop_collection = ok("CreateGradientStopCollection", unsafe {
                rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)
            })?;
            let brush = ok("CreateLinearGradientBrush", unsafe {
                rt.CreateLinearGradientBrush(&props, None, &stop_collection)
            })?;
            self.linear_gradient_brush = Some(CachedLinearGradientBrush { top, bottom, brush });
        }
        let Some(cached) = self.linear_gradient_brush.as_ref() else {
            return Err(RenderError::Platform(
                bentodesk_platform::PlatformError::Init(
                    "Renderer: gradient brush cache missing after rebuild",
                ),
            ));
        };
        let brush = cached.brush.clone();
        unsafe {
            brush.SetStartPoint(props.startPoint);
            brush.SetEndPoint(props.endPoint);
        }
        Ok(brush)
    }

    pub(super) fn draw_shadow_stack(
        &self,
        base: bentodesk_style::Rect,
        stack: bentodesk_style::ShadowStack,
        radius: BorderRadius,
    ) -> Result<(), RenderError> {
        // W13-B (2026-07-13) — a blurred CSS shadow cannot be approximated by
        // repeatedly filling larger opaque geometry. The former twenty-band
        // painter produced the broad black/gray cloud visible in the user's
        // hand-test and multiplied every Zone paint by up to forty fills.
        // Preserve authored zero-blur outline/ring layers (e.g. stack preview)
        // and suppress blur layers until a real native effect is justified.
        for layer in stack.layers() {
            if let Some(rect) = crisp_shadow_rect(base, *layer) {
                self.fill_rounded_rect(rect, layer.color, radius)?;
            }
        }
        Ok(())
    }

    // =========================================================================
    // M6c — the 3 effect render primitives + the post-pass dispatcher.
    //
    // All read `app.active_theme_effect_tauri()` (`Copy`, §10) and no-op for
    // `EffectTauri::None`, so the 14 non-effect themes pay nothing. The blur
    // neon house-style is grow-and-fill (NOT `CLSID_D2D1Shadow`); ordinary
    // box-shadow blur layers are intentionally suppressed by W13-B. GPU draw
    // itself is verified by the §6 visual
    // smoke — no offscreen unit-test harness exists (§3.4); the pure geometry
    // (`scanline_band_count` / `neon_glow_rect` / `chromatic_split_offsets`) is
    // unit-tested instead.
    // =========================================================================

    /// M6c effect dispatcher — the post-pass effect overlay drawn just before
    /// each `EndDraw` (both the aux-window and main-HWND exits) so it covers
    /// every surface, matching Tauri's `<html>`-level `data-theme-effect`
    /// `::after`. Only `Scanlines` is a full-viewport post-pass; `Neon` is
    /// inline in `draw_zones` and `Chromatic` is inline in the title draws, so
    /// this dispatcher handles ONLY the scanline arm (and no-ops otherwise).
    pub(super) fn draw_effect_overlay(&self, app: &AppState) -> Result<(), RenderError> {
        if let bentodesk_style::tokens::EffectTauri::Scanlines(scan) =
            app.active_theme_effect_tauri()
        {
            self.draw_scanline_overlay(scan, app.viewport)?;
        }
        Ok(())
    }

    /// M6c scanline (`terminal`) — full-viewport repeating horizontal bands: a
    /// 1-DIP `#00FF9C`@.06 lit stripe every 3 DIP, over the whole `vp`
    /// (`theme-effects.css:6-21`, Tauri `position:fixed; inset:0`). Drawn as a
    /// post-pass overlay above all content (`z-index:9999`).
    ///
    /// **1:1-INTENT divergence (LOCK, §3.1.4)**: Tauri composites the bands with
    /// `mix-blend-mode: overlay`; D2D's enabled-feature primary blend is
    /// source-over, which `fill_rounded_rect` uses here. At α 0.06 over the
    /// near-black terminal surface the two are visually indistinguishable
    /// (overlay only diverges materially over mid-grey, which the terminal theme
    /// has none of). Deliberate intent-parity, NOT byte-parity — same class as
    /// M6b's font substitution. We do NOT enable a D2D blend-effect feature for a
    /// sub-perceptual delta (§8 over-engineering avoidance).
    ///
    /// §10: a stack-`f32` `while` loop of square (`BorderRadius::ZERO`) fills —
    /// no per-band heap alloc; the band count is `ceil(vh/period)`.
    pub(super) fn draw_scanline_overlay(
        &self,
        scan: bentodesk_style::tokens::ScanlineEffect,
        vp: bentodesk_style::Size,
    ) -> Result<(), RenderError> {
        if scan.color.a <= 0.0
            || vp.width <= 0.0
            || vp.height <= 0.0
            || scan.period_dip <= 0.0
            || scan.band_dip <= 0.0
        {
            return Ok(());
        }
        // `count = ceil(vh / period)` bands at `y = k * period` (the pure helper
        // is the unit-test surface). Indexing `0..count` instead of accumulating
        // a `+= period` float avoids drift on tall viewports.
        let count = scanline_band_count(vp.height, scan.period_dip);
        for k in 0..count {
            let band = bentodesk_style::Rect {
                x: 0.0,
                y: k as f32 * scan.period_dip,
                width: vp.width,
                height: scan.band_dip,
            };
            self.fill_rounded_rect(band, scan.color, BorderRadius::ZERO)?;
        }
        Ok(())
    }

    /// M6c neon (`cyberpunk`) — paint the two-layer `filter: drop-shadow` bloom
    /// behind `base` (`theme-effects.css:23-32`). Reuses the `draw_shadow_stack`
    /// grow-and-fill idiom: each layer grows the rect by its blur (0,0 offset →
    /// symmetric bloom) and fills with the glow colour.
    ///
    /// **ADDITIVE to the M6b `SHADOW_CYBERPUNK` box-shadow** (§1.2 / §3.2.1):
    /// the M6b shadow stack and this `filter` bloom both composite in Tauri with
    /// DIFFERENT blur radii / alphas. Call this AFTER the M6b `draw_shadow_stack`
    /// and BEFORE the surface fill so it layers correctly — do NOT conflate them.
    ///
    /// Draw order (LOCK, §3.2.2): the authored array is `[cyan_inner,
    /// magenta_outer]`; iterating `.rev()` paints the wider magenta (index 1)
    /// FIRST and the tighter brighter cyan (index 0) on TOP, so the bloom reads
    /// cyan-cored with a magenta halo. §10: 2 grown fills, zero alloc; no-op when
    /// a layer's alpha is 0.
    pub(super) fn draw_neon_glow(
        &self,
        base: bentodesk_style::Rect,
        layers: [bentodesk_style::Shadow; 2],
        radius: BorderRadius,
    ) -> Result<(), RenderError> {
        for layer in layers.iter().rev() {
            if layer.color.a <= 0.0 {
                continue;
            }
            let rect = neon_glow_rect(base, layer.blur);
            self.fill_rounded_rect(rect, layer.color, radius)?;
        }
        Ok(())
    }

    /// M6c chromatic (`editorial`) — draw an `h1`/`h2` panel-title glyph run with
    /// the RGB-split aberration (`theme-effects.css:34-40`): a red copy at `+dx`
    /// and a cyan copy at `-dx` BEHIND the primary glyph fill, then the normal
    /// title on top. No-op (a plain `draw_text` fall-through) unless the active
    /// effect is `Chromatic`.
    ///
    /// HEADINGS-ONLY (§1.3 / §3.3): route ONLY panel-title draws through this —
    /// never body text, item labels, or pill labels (Tauri scopes it to `h1,h2`).
    /// §10: when `Chromatic`, 3 `draw_text` calls (the existing `utf16_scratch`
    /// is reused, no new alloc); otherwise a single fall-through draw. The
    /// `effect` is passed by value (`Copy`).
    pub(super) fn draw_text_chromatic_title(
        &mut self,
        text: &str,
        rect: bentodesk_style::Rect,
        color: Color,
        effect: bentodesk_style::tokens::EffectTauri,
    ) -> Result<(), RenderError> {
        if let bentodesk_style::tokens::EffectTauri::Chromatic(c) = effect {
            let (red_x, cyan_x) = chromatic_split_offsets(rect.x, c.dx_dip);
            let red_rect = bentodesk_style::Rect { x: red_x, ..rect };
            let cyan_rect = bentodesk_style::Rect { x: cyan_x, ..rect };
            self.draw_text(text, red_rect, c.red)?;
            self.draw_text(text, cyan_rect, c.cyan)?;
        }
        self.draw_text(text, rect, color)
    }

    /// M1i fidelity (2026-05-29) — stroke a rounded-rect outline (no fill).
    /// Used for the §2 source-card `border: 1px solid var(--border-zen)`. The
    /// stroke is centred on the geometric edge (D2D default), which matches the
    /// CSS `border-box` hairline closely enough at the 1-DIP widths used here.
    pub(super) fn stroke_rounded_rect(
        &self,
        rect: bentodesk_style::Rect,
        color: Color,
        radius: BorderRadius,
        stroke_width: f32,
    ) -> Result<(), RenderError> {
        if color.a <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 || stroke_width <= 0.0 {
            return Ok(());
        }
        let brush = self.solid_brush(color)?;
        let rr = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: rect.x,
                top: rect.y,
                right: rect.right(),
                bottom: rect.bottom(),
            },
            radiusX: radius.top_left,
            radiusY: radius.top_left,
        };
        let ctx = self.ctx()?;
        // Spec §15.1 — Interface::cast canonical for COM cross-cast.
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        // SAFETY: rt valid; rr lives for the call; brush COM-ref-counted; the
        // default stroke style (None) is the canonical solid hairline.
        unsafe {
            rt.DrawRoundedRectangle(&rr, &brush, stroke_width, None);
        }
        Ok(())
    }

    /// Paint the selected-stack expanded panel `border-top` as CSS does: stroke
    /// the full rounded border, then clip that stroke to the top 2-DIP strip.
    /// The old inner filled slab was inset by the full corner radius, so it read
    /// like a second border inside the panel instead of the panel's own top edge.
    pub(super) fn draw_expanded_panel_accent_edge(
        &self,
        rect: bentodesk_style::Rect,
        radius: BorderRadius,
        accent: Color,
    ) -> Result<(), RenderError> {
        if accent.a <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let clip = expanded_panel_accent_clip_rect(rect);
        if clip.width <= 0.0 || clip.height <= 0.0 {
            return Ok(());
        }
        self.push_clip(clip)?;
        let result = self.stroke_rounded_rect(rect, accent, radius, PANEL_ACCENT_EDGE_THICKNESS_PX);
        let pop_result = self.pop_clip();
        result.and(pop_result)
    }

    /// G5 (2026-06-01) — stroke a rounded-rect outline with a DASHED hairline.
    /// Used for the collapsed `minimal`-shape capsule, whose Tauri chrome is
    /// `border: 1px dashed rgba(255,255,255,0.2)` over a transparent body
    /// (`BentoZone.css:92-99 .bento-zone--shape-minimal`). The dash cadence is
    /// the predefined `D2D1_DASH_STYLE_DASH` (2 on / 2 off in stroke-width
    /// units), which reads as a clean CSS-style dashed edge at the 1-DIP width.
    ///
    /// §10: the `ID2D1StrokeStyle` is built ONCE per process and cached in a
    /// `OnceLock` (it is created from the device-INDEPENDENT D2D factory, so it
    /// survives device-loss rebuilds and never re-allocates per frame). §11: no
    /// panic/unwrap — the build is `?`-propagated, the cache uses `get_or_init`
    /// with a fallible inner that falls back to a solid stroke on any error.
    pub(super) fn stroke_rounded_rect_dashed(
        &mut self,
        rect: bentodesk_style::Rect,
        color: Color,
        radius: BorderRadius,
        stroke_width: f32,
    ) -> Result<(), RenderError> {
        use windows::Win32::Graphics::Direct2D::{
            D2D1_CAP_STYLE_FLAT, D2D1_DASH_STYLE_DASH, D2D1_LINE_JOIN_MITER,
            D2D1_STROKE_STYLE_PROPERTIES, ID2D1Factory,
        };
        if color.a <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 || stroke_width <= 0.0 {
            return Ok(());
        }
        // Lazily build + cache the dashed stroke style on the renderer. It is
        // created from the device-INDEPENDENT D2D factory, so the cached handle
        // stays valid across device-loss rebuilds and re-skins; one COM
        // allocation per process, ZERO per frame (§10). Single-threaded UI
        // renderer, so a plain `Option` field is the right cache, not a global
        // (`ID2D1StrokeStyle` is not `Sync`).
        if self.dashed_stroke_style.is_none() {
            let d2d = d2d::factory()?;
            // Cross-cast `ID2D1Factory1` → base `ID2D1Factory` (§15.1 canonical)
            // so `CreateStrokeStyle` resolves to the base overload that takes
            // `D2D1_STROKE_STYLE_PROPERTIES` and returns `ID2D1StrokeStyle`
            // (the `Factory1` overload wants `..._PROPERTIES1`/`...Style1`).
            let factory: ID2D1Factory = ok("Factory1::cast<Factory>", d2d.factory.cast())?;
            let props = D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_FLAT,
                endCap: D2D1_CAP_STYLE_FLAT,
                dashCap: D2D1_CAP_STYLE_FLAT,
                lineJoin: D2D1_LINE_JOIN_MITER,
                miterLimit: 10.0,
                dashStyle: D2D1_DASH_STYLE_DASH,
                dashOffset: 0.0,
            };
            // SAFETY: `props` lives for the call; `dashes: None` selects the
            // predefined DASH cadence; the returned style is COM-ref-counted.
            let style = ok("CreateStrokeStyle", unsafe {
                factory.CreateStrokeStyle(&props, None)
            })?;
            self.dashed_stroke_style = Some(style);
        }
        let dash_style = self.dashed_stroke_style.as_ref();
        let brush = self.solid_brush(color)?;
        let rr = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: rect.x,
                top: rect.y,
                right: rect.right(),
                bottom: rect.bottom(),
            },
            radiusX: radius.top_left,
            radiusY: radius.top_left,
        };
        let ctx = self.ctx()?;
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        // SAFETY: rt valid; rr + dash_style live for the call; brush COM-ref-counted.
        unsafe {
            rt.DrawRoundedRectangle(&rr, &brush, stroke_width, dash_style);
        }
        Ok(())
    }

    /// M6-UI fidelity (2026-05-29) — fill a rectangle rounding ONLY the corners
    /// flagged in `corners` (`[top_left, top_right, bottom_right, bottom_left]`)
    /// to `radius`; flagged-off corners stay square. D2D's `FillRoundedRectangle`
    /// only supports a single uniform radius and there is no rounded-clip
    /// primitive (`PushAxisAlignedClip` is rectangular), so the per-corner
    /// silhouette is materialised as a closed `ID2D1PathGeometry` (one
    /// arc per rounded corner, straight `AddLine` for square ones). This is the
    /// visible-correct approximation for Tauri's `.theme-card__swatches
    /// { border-radius: 8px; overflow: hidden }` masking the 2×2 quadrants:
    /// each corner quadrant rounds only its single OUTER corner so the four
    /// quadrants meet square at the centre cross while the block silhouette is
    /// an 8-DIP rounded square. Path-sink build uses no Rust String/Vec/format!
    /// (§10) — same mechanism as `svg::build` for icon glyphs.
    pub(super) fn fill_partial_rounded_rect(
        &self,
        rect: bentodesk_style::Rect,
        color: Color,
        radius: f32,
        corners: [bool; 4],
    ) -> Result<(), RenderError> {
        use windows::Win32::Graphics::Direct2D::Common::{
            D2D_SIZE_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
        };
        use windows::Win32::Graphics::Direct2D::{
            D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_SMALL, D2D1_SWEEP_DIRECTION_CLOCKWISE,
            ID2D1GeometrySink,
        };
        if color.a <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        // Clamp the radius so it never exceeds half the shortest edge.
        let r = radius.max(0.0).min(rect.width * 0.5).min(rect.height * 0.5);
        if r <= 0.0 || corners == [false; 4] {
            // Nothing to round — fall back to the cheap square fill.
            return self.fill_rounded_rect(rect, color, BorderRadius::ZERO);
        }
        let l = rect.x;
        let t = rect.y;
        let rt_x = rect.right();
        let b = rect.bottom();
        // Per-corner inset (0 when the corner is square so the figure walks
        // straight into the geometric corner).
        let tl = if corners[0] { r } else { 0.0 };
        let tr = if corners[1] { r } else { 0.0 };
        let br = if corners[2] { r } else { 0.0 };
        let bl = if corners[3] { r } else { 0.0 };
        let arc = |to_x: f32, to_y: f32| D2D1_ARC_SEGMENT {
            point: D2D_POINT_2F { x: to_x, y: to_y },
            size: D2D_SIZE_F {
                width: r,
                height: r,
            },
            rotationAngle: 90.0,
            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
            arcSize: D2D1_ARC_SIZE_SMALL,
        };
        // Mc-2b: `d2d::factory()` now returns `Arc<D2dFactory>`; bind it to a
        // local so the `&...factory` borrow outlives this statement (a
        // `&...?.factory` temporary Arc would be dropped at the `;`).
        let d2d_fac = d2d::factory()?;
        let factory = &d2d_fac.factory;
        // SAFETY: factory valid; geometry + sink are freshly created and the
        // sink is closed before this fn returns (mirrors svg::to_d2d_geometry).
        let geom = ok("CreatePathGeometry", unsafe {
            factory.CreatePathGeometry()
        })?;
        let sink: ID2D1GeometrySink = ok("PathGeometry::Open", unsafe { geom.Open() })?;
        // Walk the perimeter clockwise from the top edge, arcing rounded
        // corners and cutting straight to the geometric corner on square ones.
        // SAFETY: sink valid until Close() below; all points live on the stack.
        unsafe {
            sink.BeginFigure(D2D_POINT_2F { x: l + tl, y: t }, D2D1_FIGURE_BEGIN_FILLED);
            // Top edge → top-right corner.
            sink.AddLine(D2D_POINT_2F { x: rt_x - tr, y: t });
            if corners[1] {
                sink.AddArc(&arc(rt_x, t + tr));
            }
            // Right edge → bottom-right corner.
            sink.AddLine(D2D_POINT_2F { x: rt_x, y: b - br });
            if corners[2] {
                sink.AddArc(&arc(rt_x - br, b));
            }
            // Bottom edge → bottom-left corner.
            sink.AddLine(D2D_POINT_2F { x: l + bl, y: b });
            if corners[3] {
                sink.AddArc(&arc(l, b - bl));
            }
            // Left edge → top-left corner.
            sink.AddLine(D2D_POINT_2F { x: l, y: t + tl });
            if corners[0] {
                sink.AddArc(&arc(l + tl, t));
            }
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        }
        // SAFETY: sink valid; Close finalises the geometry before any fill.
        ok("GeometrySink::Close", unsafe { sink.Close() })?;
        let brush = self.solid_brush(color)?;
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; geom + brush outlive the call; no transform change.
        unsafe {
            ctx.FillGeometry(&geom, &brush, None);
        }
        Ok(())
    }
}
