use super::*;

impl Renderer {
    /// Draw a 1:1 SVG path translated into `rect.origin`. Caller takes
    /// responsibility for sizing — `draw_svg_fit` is the safer entry when
    /// the path's viewbox doesn't match the destination rect.
    pub(super) fn draw_svg(
        &self,
        path_d: &str,
        rect: bento_nano_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        // Mc-2b: bind the `Arc<D2dFactory>` to a local before borrowing `.factory`.
        let d2d_fac = d2d::factory()?;
        let factory = &d2d_fac.factory;
        let geom = svg::build(factory, path_d)?;
        let brush = self.solid_brush(color)?;
        let ctx = self.ctx()?;
        // 1:1 translate — the path is already the right size. Compose against
        // the current logical transform so grouped animations preserve icons.
        let logical = self.current_logical_transform_matrix();
        let m = Matrix3x2 {
            M11: logical.M11,
            M12: 0.0,
            M21: 0.0,
            M22: logical.M22,
            M31: rect.x * logical.M11 + logical.M31,
            M32: rect.y * logical.M22 + logical.M32,
        };
        // SAFETY: ctx valid; brush + geom outlive the call; matrix on stack.
        unsafe {
            ctx.SetTransform(&m);
            ctx.FillGeometry(&geom, &brush, None);
            // Restore the current logical transform so subsequent draw calls
            // stay in the grouped surface animation.
            let base = self.current_logical_transform_matrix();
            ctx.SetTransform(&base);
        }
        Ok(())
    }

    /// Draw an SVG path scaled-to-fit inside `rect`. `view_size` is the
    /// edge length of the source viewbox (typical Lucide / Material glyphs
    /// are 24). Uniform scale preserves the icon's aspect ratio; the glyph
    /// is centred on whichever axis has spare room.
    pub(super) fn draw_svg_fit(
        &self,
        path_d: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        view_size: f32,
    ) -> Result<(), RenderError> {
        if view_size <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        // Mc-2b: bind the `Arc<D2dFactory>` to a local before borrowing `.factory`.
        let d2d_fac = d2d::factory()?;
        let factory = &d2d_fac.factory;
        let geom = svg::build(factory, path_d)?;
        let brush = self.solid_brush(color)?;
        let ctx = self.ctx()?;
        let m = self.svg_fit_matrix_in_current_transform(rect, view_size);
        // SAFETY: ctx valid; brush + geom outlive the call; matrix on stack.
        unsafe {
            ctx.SetTransform(&m);
            ctx.FillGeometry(&geom, &brush, None);
            // Restore the current logical transform so grouped surface
            // animations continue after the per-glyph transform.
            let base = self.current_logical_transform_matrix();
            ctx.SetTransform(&base);
        }
        Ok(())
    }

    /// RC-4 Gap 1 — render a zone-icon name as a real line-art glyph.
    ///
    /// `name` is the wire-format icon string from `Zone.icon` (e.g. "folder",
    /// "settings", "search"). When it resolves to a built-in `IconKind`, the
    /// matching 24×24 source SVG document is drawn via
    /// `draw_svg_document_stroke_fit` (cached geometry). Unknown or legacy text
    /// payloads deliberately render as a neutral built-in glyph instead of
    /// visible emoji/text placeholders.
    pub(super) fn draw_icon_glyph(
        &mut self,
        name: &str,
        rect: bento_nano_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        if !zone_pill_geometry::icon_name_has_visible_glyph(name) {
            return Ok(());
        }
        if let Some(kind) = IconKind::from_str_opt(name) {
            // 24-unit viewbox per `IconKind::source_svg` — every built-in is
            // hand-rolled around 0–24 just like the 1.x Tauri sources.
            // `draw_svg_document_stroke_fit` already h+v-centres the glyph in
            // `rect` (scale-to-fit + 0.5 offset).
            return self.draw_svg_document_stroke_fit(kind.source_svg(), rect, color, 24.0);
        }
        // No-emoji runtime policy (2026-06-18): keep wire compatibility for
        // old layouts that store arbitrary text/emoji icon payloads, but never
        // paint those payloads as UI icons.
        self.draw_svg_document_stroke_fit(IconKind::Document.source_svg(), rect, color, 24.0)
    }

    pub(super) fn draw_svg_document_stroke_fit(
        &mut self,
        svg_document: &'static str,
        rect: bento_nano_style::Rect,
        color: Color,
        view_size: f32,
    ) -> Result<(), RenderError> {
        if view_size <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        // Mc-2b: bind the `Arc<D2dFactory>` to a local before borrowing `.factory`.
        let d2d_fac = d2d::factory()?;
        let factory = &d2d_fac.factory;
        let geom = {
            let cached = self
                .svg_cache
                .get_or_insert(svg_document.as_bytes(), factory)?;
            cached.clone()
        };
        let brush = self.solid_brush(color)?;
        let ctx = self.ctx()?;
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        let m = self.svg_fit_matrix_in_current_transform(rect, view_size);
        // SAFETY: rt valid; geometry and brush are COM references alive for
        // the call; matrix lives on the stack; `None` uses D2D's default
        // round-cap/round-join behavior encoded by the source line art.
        unsafe {
            rt.SetTransform(&m);
            rt.DrawGeometry(&geom, &brush, 1.5, None);
            let base = self.current_logical_transform_matrix();
            rt.SetTransform(&base);
        }
        Ok(())
    }

    pub(super) fn solid_brush(&self, c: Color) -> Result<ID2D1SolidColorBrush, RenderError> {
        Ok(d2d::solid_brush(self.ctx()?, c.r, c.g, c.b, c.a)?)
    }
}
