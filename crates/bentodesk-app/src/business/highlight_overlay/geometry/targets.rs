use super::*;

/// Full-zone target used by Search zone hits and live-folder hits.
pub fn zone_target_rect(zone: &Zone) -> HighlightRect {
    HighlightRect::new(zone.x as f32, zone.y as f32, zone.w as f32, zone.h as f32)
}

/// Full-zone target from the shared resolved visible rectangle.
pub fn zone_target_rect_for_rect(rect: Rect) -> HighlightRect {
    HighlightRect::from_rect(rect)
}

/// Item target used by Search item hits and Suggestor matching-path previews.
pub fn item_target_rect(zone: &Zone, item: &ZoneItem) -> HighlightRect {
    HighlightRect::from_rect(item_card_rect_for_item(zone, item))
}

/// Item target inside the shared resolved visible panel rectangle.
pub fn item_target_rect_in_panel(zone: &Zone, item: &ZoneItem, panel: Rect) -> HighlightRect {
    HighlightRect::from_rect(item_card_rect_for_item_in_panel(zone, item, panel))
}

/// Renderer paint rect after applying the snap.md inset.
pub fn paint_rect(target: HighlightRect) -> Rect {
    let rect = target.to_rect();
    Rect {
        x: rect.x + TARGET_INSET_PX,
        y: rect.y + TARGET_INSET_PX,
        width: (rect.width - (TARGET_INSET_PX * 2.0)).max(0.0),
        height: (rect.height - (TARGET_INSET_PX * 2.0)).max(0.0),
    }
}

/// Clamp an elapsed pulse value into the repeat-loop phase `0.0..=1.0`.
pub fn pulse_phase(elapsed_ms: u32) -> f32 {
    if PULSE_LOOP_MS == 0 {
        return 0.0;
    }
    (elapsed_ms % PULSE_LOOP_MS) as f32 / PULSE_LOOP_MS as f32
}

/// Expanding halo rect for a desktop-icon pulse.
pub fn pulse_halo_rect(target: &HighlightPulse, phase: f32) -> Rect {
    let clamped = phase.clamp(0.0, 1.0);
    let radius =
        PULSE_HALO_MIN_RADIUS_PX + (PULSE_HALO_RADIUS_PX - PULSE_HALO_MIN_RADIUS_PX) * clamped;
    Rect {
        x: target.x - radius,
        y: target.y - radius,
        width: radius * 2.0,
        height: radius * 2.0,
    }
}

/// Solid center dot rect for a desktop-icon pulse.
pub fn pulse_core_rect(target: &HighlightPulse) -> Rect {
    Rect {
        x: target.x - PULSE_CORE_RADIUS_PX,
        y: target.y - PULSE_CORE_RADIUS_PX,
        width: PULSE_CORE_RADIUS_PX * 2.0,
        height: PULSE_CORE_RADIUS_PX * 2.0,
    }
}

/// Target corner radius from explicit active radius tokens.
pub fn target_radius_from_tokens(radius: RadiusTokens) -> BorderRadius {
    radius.lg
}

/// Target corner radius from the process-default theme.
pub fn target_radius() -> BorderRadius {
    target_radius_from_tokens(radius::DEFAULT)
}
