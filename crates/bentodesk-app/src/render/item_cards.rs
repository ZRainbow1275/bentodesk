use super::*;
use std::io::Read;

impl Renderer {
    // Geometric draw helper: the params are independent paint primitives
    // (rect, fill, chrome bundle, M3-A2 scale + M3-A3 hover ramp/press flag).
    // Bundling them into a struct adds indirection at the hot per-item call
    // sites for no real benefit — the conventional render-code shape, so allow it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_item_card(
        &mut self,
        item: &ZoneItem,
        base_rect: bentodesk_style::Rect,
        fill: Color,
        chrome: &item_card::ItemCardChrome,
        hover_t: f32,
        press_held: bool,
        scale: f32,
        _label_font_px: f32,
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
            let ambient = bentodesk_style::Rect {
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
            let contact = bentodesk_style::Rect {
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
        // native tracks NO per-item KEYBOARD focus signal distinct from selection
        // (`ZoneItem` has no `selected`/`focused` field; `AppState` only tracks
        // `settings_focused_field` for the Settings text inputs). Building
        // focus-tracking plumbing is out of scope for this parity pass — paint
        // the ring once an item keyboard-focus channel lands.
        let metrics = item_grid::responsive_item_metrics(base_rect.width, item.name.as_ref());
        let slot_side = metrics.icon_slot_side * scale;
        let idle_side = metrics.icon_side * scale;
        let icon_slot = bentodesk_style::Rect {
            x: card_rect.x + ((card_rect.width - slot_side) * 0.5).max(0.0),
            y: card_rect.y + 8.0 * scale,
            width: slot_side,
            height: slot_side,
        };
        let requested_idle_icon = bentodesk_style::Rect {
            x: icon_slot.x + (icon_slot.width - idle_side) * 0.5,
            y: icon_slot.bottom() - idle_side,
            width: idle_side,
            height: idle_side,
        };
        let hover_icon_scale = 1.0 + (item_grid::ITEM_ICON_HOVER_SCALE - 1.0) * hover_clamped;
        let fallback_icon_rect =
            animator::scale_rect_centered(requested_idle_icon, hover_icon_scale);
        if !self.draw_item_bitmap_for_card(
            item.icon_hash.as_ref(),
            requested_idle_icon,
            hover_clamped,
            alpha,
        )? {
            // Wave I2 / R4 — cache misses still use selected-stack line-art
            // icon families, never the old extension-keyed emoji text fallback.
            let kind =
                item_icon::fallback_icon_kind_for_item(item.icon_hash.as_ref(), item.path.as_ref());
            self.draw_icon_glyph(kind.as_str(), fallback_icon_rect, icon_text)?;
        }
        let label_text = item_label_visible_name(item.name.as_ref());
        let label_rect = bentodesk_style::Rect {
            x: card_rect.x + 4.0 * scale,
            y: icon_slot.bottom() + 4.0 * scale,
            width: (card_rect.width - 8.0 * scale).max(0.0),
            height: (card_rect.bottom() - 8.0 * scale - icon_slot.bottom() - 4.0 * scale).max(0.0),
        };
        self.draw_item_label_wrapped(label_text, label_rect, text, metrics.label_font_px * scale)?;
        Ok(())
    }

    /// Draw an item icon bitmap if the backend cache has bytes for the item's
    /// icon hash. Returns `false` when fallback text should be used.
    pub(super) fn draw_item_bitmap(
        &mut self,
        icon_hash: &str,
        rect: bentodesk_style::Rect,
        opacity: f32,
    ) -> Result<bool, RenderError> {
        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return Ok(true);
        }
        let Some(bitmap) = self.cached_item_bitmap(icon_hash)? else {
            return Ok(false);
        };
        self.draw_cached_item_bitmap(&bitmap, rect, opacity)?;
        Ok(true)
    }

    fn draw_item_bitmap_for_card(
        &mut self,
        icon_hash: &str,
        requested_idle: bentodesk_style::Rect,
        hover_t: f32,
        opacity: f32,
    ) -> Result<bool, RenderError> {
        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return Ok(true);
        }
        let Some(bitmap) = self.cached_item_bitmap(icon_hash)? else {
            return Ok(false);
        };
        let decoded_side = bitmap.source_width.min(bitmap.source_height) as f32
            / self.base_scale.max(f32::EPSILON);
        let idle_side = requested_idle
            .width
            .min(requested_idle.height)
            .min(decoded_side / item_grid::ITEM_ICON_HOVER_SCALE);
        let idle = bentodesk_style::Rect {
            x: requested_idle.x + (requested_idle.width - idle_side) * 0.5,
            y: requested_idle.bottom() - idle_side,
            width: idle_side,
            height: idle_side,
        };
        let scale = 1.0 + (item_grid::ITEM_ICON_HOVER_SCALE - 1.0) * hover_t.clamp(0.0, 1.0);
        let rect = animator::scale_rect_centered(idle, scale);
        self.draw_cached_item_bitmap(&bitmap, rect, opacity)?;
        Ok(true)
    }

    fn cached_item_bitmap(
        &mut self,
        icon_hash: &str,
    ) -> Result<Option<CachedIconBitmap>, RenderError> {
        if icon_hash.is_empty()
            || icon_hash.starts_with("builtin:")
            || self.icon_bitmap_failures.contains(icon_hash)
        {
            return Ok(None);
        }

        if !self.icon_bitmaps.contains_key(icon_hash) {
            let Some(cache) = bentodesk_backend::icon::cache_handle() else {
                return Ok(None);
            };
            let Some(bytes) = cache.get(icon_hash) else {
                // Startup icon repair populates the cache off the UI thread.
                // A miss is therefore pending, not a permanent decode failure.
                return Ok(None);
            };
            let Some(surface) = self.surface.as_ref() else {
                return Ok(None);
            };
            let decoded =
                d2d::bitmap_from_png_bytes(&surface.ctx, bytes.as_ref()).and_then(|bitmap| {
                    // SAFETY: `bitmap` is a live decoded D2D bitmap for this call.
                    let source = unsafe { bitmap.GetPixelSize() };
                    let invert_mask = bentodesk_backend::icon::legacy_invert_mask(bytes.as_ref())
                        .map(|(width, height, bits)| {
                            d2d::invert_mask_effect(&surface.ctx, width, height, bits).map(
                                |effect| CachedInvertMask {
                                    effect,
                                    width,
                                    height,
                                },
                            )
                        })
                        .transpose()?;
                    Ok(CachedIconBitmap {
                        bitmap,
                        invert_mask,
                        source_width: source.width,
                        source_height: source.height,
                    })
                });
            match decoded {
                Ok(bitmap) => {
                    if self.icon_bitmaps.len() >= ICON_BITMAP_CACHE_CAPACITY
                        && let Some(oldest) = self.icon_bitmaps.keys().next().cloned()
                    {
                        self.icon_bitmaps.remove(&oldest);
                    }
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
                    return Ok(None);
                }
            }
        }
        Ok(self.icon_bitmaps.get(icon_hash).cloned())
    }

    fn draw_cached_item_bitmap(
        &self,
        bitmap: &CachedIconBitmap,
        rect: bentodesk_style::Rect,
        opacity: f32,
    ) -> Result<(), RenderError> {
        let d2d_rect = D2D_RECT_F {
            left: rect.x,
            top: rect.y,
            right: rect.x + rect.width,
            bottom: rect.y + rect.height,
        };
        let Some(surface) = self.surface.as_ref() else {
            return Ok(());
        };
        d2d::draw_bitmap(&surface.ctx, &bitmap.bitmap, d2d_rect, opacity)?;
        if let Some(mask) = bitmap.invert_mask.as_ref() {
            d2d::draw_invert_mask(
                &surface.ctx,
                &mask.effect,
                mask.width,
                mask.height,
                d2d_rect,
                opacity,
            )?;
        }
        Ok(())
    }

    pub(super) fn draw_image_file(
        &mut self,
        path: &str,
        rect: bentodesk_style::Rect,
    ) -> Result<(), RenderError> {
        if path.is_empty()
            || rect.width <= 0.0
            || rect.height <= 0.0
            || self.image_file_failures.contains(path)
        {
            return Ok(());
        }

        if !self.image_file_bitmaps.contains_key(path) {
            let bytes = match std::fs::File::open(path).and_then(|file| {
                let mut bytes = Vec::new();
                file.take((IMAGE_WIDGET_MAX_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)?;
                Ok(bytes)
            }) {
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
                    if self.image_file_bitmaps.len() >= IMAGE_FILE_BITMAP_CACHE_CAPACITY
                        && let Some(oldest) = self.image_file_bitmaps.keys().next().cloned()
                    {
                        self.image_file_bitmaps.remove(&oldest);
                    }
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
