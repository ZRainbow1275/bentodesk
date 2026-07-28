use super::*;

impl Renderer {
    // Geometric draw helper: the params are independent paint primitives
    // (rect, fill, chrome bundle, M3-A2 scale + M3-A3 hover ramp/press flag).
    // Bundling them into a struct adds indirection at the hot per-item call
    // sites for no real benefit — the conventional render-code shape, so allow it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_item_card(
        &mut self,
        item: &ZoneItem,
        base_rect: bento_nano_style::Rect,
        fill: Color,
        chrome: &item_card::ItemCardChrome,
        hover_t: f32,
        press_held: bool,
        scale: f32,
        label_font_px: f32,
        alpha: f32,
    ) -> Result<(), RenderError> {
        let alpha = alpha.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return Ok(());
        }
        let radius = chrome.card_radius;
        let fade = |color: Color| with_alpha(color, color.a * alpha);
        let text = fade(chrome.text);
        let icon_text = fade(chrome.icon_text);
        let fill = fade(fill);
        let hover_background = fade(chrome.hover_background);
        let hover_border = fade(chrome.hover_border);
        let hover_shadow_inner = fade(chrome.hover_shadow_inner);
        let hover_shadow_outer = fade(chrome.hover_shadow_outer);
        // M3-A2 (2026-05-29) — apply the `item_card::card_scale_for` hover/press
        // multiplier as a Tauri-style centred `transform: scale()`. The card
        // surface AND its inner icon/label inset offsets all inflate/deflate
        // about the card's CENTRE so the glyph + label stay centred (a CSS
        // transform scales the whole subtree, not just the box). `scale == 1.0`
        // (idle / drag-ghost) collapses to the original geometry exactly.
        let mut card_rect = animator::scale_rect_centered(base_rect, scale);
        // FIX 1 (M3-A3) — Tauri `.item-card:hover { transform: translateY(-1px)
        // scale(1.02) }`: the lift rides the same 150ms ease-out ramp as the
        // scale. We offset the scaled rect's `y` by `CARD_HOVER_LIFT_DY *
        // hover_t` (0 at idle → -1px at full hover). Per CSS specificity the
        // `:active` rule respecifies `transform: scale(0.97)` (scale-only), so
        // the inherited lift is DROPPED while the pointer is actively held —
        // `press_held` mirrors that exactly. On release the lift returns.
        if !press_held {
            card_rect.y += item_card::CARD_HOVER_LIFT_DY * hover_t.clamp(0.0, 1.0);
        }
        // FIX 2 (M3-A3) — `:hover { box-shadow: var(--shadow-item-hover) }`: a
        // two-layer drop shadow (0 2 8 / 0 8 24 black) faded in by hover_t.
        // Painted BEHIND the card via the grow-and-fill idiom (one fill per
        // layer, back-to-front: the wider ambient layer first, the tighter
        // contact layer on top), §10 allocation-free — no per-frame heap, no
        // D2D blur effect. Skipped entirely at hover_t ≈ 0 (fill alpha guard).
        let hover_clamped = hover_t.clamp(0.0, 1.0);
        if hover_clamped > 0.0 {
            // Ambient: offset_y 8, blur 24.
            let ambient = bento_nano_style::Rect {
                x: card_rect.x - 24.0,
                y: card_rect.y + 8.0 - 24.0,
                width: card_rect.width + 48.0,
                height: card_rect.height + 48.0,
            };
            self.fill_rounded_rect(
                ambient,
                with_alpha(
                    chrome.hover_shadow_inner,
                    hover_shadow_inner.a * hover_clamped,
                ),
                radius,
            )?;
            // Contact: offset_y 2, blur 8.
            let contact = bento_nano_style::Rect {
                x: card_rect.x - 8.0,
                y: card_rect.y + 2.0 - 8.0,
                width: card_rect.width + 16.0,
                height: card_rect.height + 16.0,
            };
            self.fill_rounded_rect(
                contact,
                with_alpha(
                    chrome.hover_shadow_outer,
                    hover_shadow_outer.a * hover_clamped,
                ),
                radius,
            )?;
        }
        // FIX 2 (M3-A3) — `:hover { background: var(--surface-hover) }`: lerp the
        // base fill toward the hover surface by hover_t (premultiplied-alpha
        // lerp, §10 stack-only). At hover_t 0 this is `fill` exactly (idle /
        // missing / drag bg preserved); at 1.0 it is `--surface-hover`.
        let card_fill = fill.lerp(hover_background, hover_clamped);
        self.fill_rounded_rect(card_rect, card_fill, radius)?;
        // FIX 2 (M3-A3) — `:hover { border-color: var(--border-hover) }`: a 1px
        // stroke whose alpha lerps transparent → `--border-hover` by hover_t.
        // The normal card strokes no border, so this only appears on hover.
        if hover_clamped > 0.0 {
            let border = with_alpha(hover_border, hover_border.a * hover_clamped);
            self.stroke_rounded_rect(card_rect, border, radius, 1.0)?;
        }
        // FIX 3 (M3-A3, DEFERRED) — Tauri `:focus-visible { outline: 2px solid
        // var(--accent-blue); outline-offset: 2px; border-color: transparent }`.
        // nano tracks NO per-item KEYBOARD focus signal distinct from selection
        // (`ZoneItem` has no `selected`/`focused` field; `AppState` only tracks
        // `settings_focused_field` for the Settings text inputs). Building
        // focus-tracking plumbing is out of scope for this parity pass — paint
        // the ring once an item keyboard-focus channel lands.
        // V21-C3 — mirror Tauri's ItemIcon slot: a 36px/28px centred container
        // with the actual bitmap/glyph rendered at 24px/20px inside it.
        let (_icon_container_rect, icon_rect) =
            item_icon_slots_for_card(card_rect, item.is_wide, scale);
        if !self.draw_item_bitmap(item.icon_hash.as_ref(), icon_rect, alpha)? {
            // Wave I2 / R4 — cache misses still use selected-stack line-art
            // icon families, never the old extension-keyed emoji text fallback.
            let kind =
                item_icon::fallback_icon_kind_for_item(item.icon_hash.as_ref(), item.path.as_ref());
            self.draw_icon_glyph(kind.as_str(), icon_rect, icon_text)?;
        }
        // V21-C3/V21-N108/V21-N110 — the label sits on the lower card text
        // rail and follows Tauri's full-text shrink contract (`useTextAbbr`),
        // not DWrite ellipsis trimming.
        let label_text = item_label_visible_name(item.name.as_ref());
        let label_rect = item_label_rect_for_card(card_rect, scale, label_font_px);
        // V21-C3/V21-N129 — weight stays Tauri's 400 contract; size and colour
        // follow the captured 2026-06-02 frame where source tokens conflict.
        // #1 step 13 / V21-N108 — the run is horizontally CENTERED, while the
        // layout box is pinned to the lower rail so DWrite top-near glyph ink
        // matches the WebView reference instead of drifting upward.
        self.draw_item_label_no_wrap(label_text, label_rect, text, label_font_px)?;
        Ok(())
    }

    /// Draw an item icon bitmap if the backend cache has bytes for the item's
    /// icon hash. Returns `false` when fallback text should be used.
    pub(super) fn draw_item_bitmap(
        &mut self,
        icon_hash: &str,
        rect: bento_nano_style::Rect,
        opacity: f32,
    ) -> Result<bool, RenderError> {
        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return Ok(true);
        }
        if icon_hash.is_empty()
            || icon_hash.starts_with("builtin:")
            || self.icon_bitmap_failures.contains(icon_hash)
        {
            return Ok(false);
        }

        if !self.icon_bitmaps.contains_key(icon_hash) {
            let Some(cache) = bento_nano_backend::icon::cache_handle() else {
                return Ok(false);
            };
            let Some(bytes) = cache.get(icon_hash) else {
                // Startup icon repair populates the cache off the UI thread.
                // A miss is therefore pending, not a permanent decode failure.
                return Ok(false);
            };
            let Some(surface) = self.surface.as_ref() else {
                return Ok(false);
            };
            match d2d::bitmap_from_png_bytes(&surface.ctx, bytes.as_ref()) {
                Ok(bitmap) => {
                    let _ = self.icon_bitmaps.insert(icon_hash.to_owned(), bitmap);
                }
                Err(e) => {
                    tracing::warn!(
                        target: "bentodesk::render::icon",
                        %icon_hash,
                        error = %e,
                        "failed to decode cached icon bitmap; using fallback glyph"
                    );
                    let _ = self.icon_bitmap_failures.insert(icon_hash.to_owned());
                    return Ok(false);
                }
            }
        }

        let Some(bitmap) = self.icon_bitmaps.get(icon_hash).cloned() else {
            return Ok(false);
        };
        let d2d_rect = D2D_RECT_F {
            left: rect.x,
            top: rect.y,
            right: rect.x + rect.width,
            bottom: rect.y + rect.height,
        };
        let Some(surface) = self.surface.as_ref() else {
            return Ok(false);
        };
        d2d::draw_bitmap(&surface.ctx, &bitmap, d2d_rect, opacity)?;
        Ok(true)
    }

    pub(super) fn draw_image_file(
        &mut self,
        path: &str,
        rect: bento_nano_style::Rect,
    ) -> Result<(), RenderError> {
        if path.is_empty()
            || rect.width <= 0.0
            || rect.height <= 0.0
            || self.image_file_failures.contains(path)
        {
            return Ok(());
        }

        if !self.image_file_bitmaps.contains_key(path) {
            let bytes = match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::render::image",
                        %path,
                        %error,
                        "failed to read file-backed image widget"
                    );
                    let _ = self.image_file_failures.insert(path.to_owned());
                    return Ok(());
                }
            };
            if bytes.len() > IMAGE_WIDGET_MAX_BYTES {
                tracing::warn!(
                    target: "bentodesk::render::image",
                    %path,
                    bytes = bytes.len(),
                    "file-backed image widget exceeds decode budget"
                );
                let _ = self.image_file_failures.insert(path.to_owned());
                return Ok(());
            }
            let Some(surface) = self.surface.as_ref() else {
                return Ok(());
            };
            match d2d::bitmap_from_image_bytes(&surface.ctx, &bytes) {
                Ok(bitmap) => {
                    let _ = self.image_file_bitmaps.insert(path.to_owned(), bitmap);
                }
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::render::image",
                        %path,
                        error = %error,
                        "failed to decode file-backed image widget"
                    );
                    let _ = self.image_file_failures.insert(path.to_owned());
                    return Ok(());
                }
            }
        }

        let Some(bitmap) = self.image_file_bitmaps.get(path).cloned() else {
            return Ok(());
        };
        let d2d_rect = D2D_RECT_F {
            left: rect.x,
            top: rect.y,
            right: rect.x + rect.width,
            bottom: rect.y + rect.height,
        };
        let Some(surface) = self.surface.as_ref() else {
            return Ok(());
        };
        d2d::draw_bitmap(&surface.ctx, &bitmap, d2d_rect, 1.0)?;
        Ok(())
    }
}
