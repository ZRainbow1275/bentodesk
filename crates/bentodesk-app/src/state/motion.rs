use super::*;

#[inline]
pub fn settings_open_animation_progress(started_ms: u32, now_ms: u32) -> f32 {
    if SETTINGS_OPEN_ANIMATION_MS == 0 {
        return 1.0;
    }
    (now_ms.wrapping_sub(started_ms) as f32 / SETTINGS_OPEN_ANIMATION_MS as f32).clamp(0.0, 1.0)
}

#[inline]
pub fn settings_open_animation_ease(t: f32) -> f32 {
    css_ease_out(t.clamp(0.0, 1.0))
}

#[inline]
pub fn settings_open_animation_scale(eased: f32) -> f32 {
    SETTINGS_OPEN_SCALE_FROM + (1.0 - SETTINGS_OPEN_SCALE_FROM) * eased.clamp(0.0, 1.0)
}

#[inline]
pub fn theme_transition_progress(started_ms: u32, now_ms: u32) -> f32 {
    if THEME_TRANSITION_MS == 0 {
        return 1.0;
    }
    (now_ms.wrapping_sub(started_ms) as f32 / THEME_TRANSITION_MS as f32).clamp(0.0, 1.0)
}

#[inline]
pub fn theme_transition_ease(t: f32) -> f32 {
    css_ease_out(t.clamp(0.0, 1.0))
}

#[inline]
fn css_ease_out(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let mut lo = 0.0;
    let mut hi = 1.0;
    for _ in 0..12 {
        let mid = (lo + hi) * 0.5;
        if cubic_bezier_axis(0.0, 0.58, mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    cubic_bezier_axis(0.0, 1.0, (lo + hi) * 0.5)
}

#[inline]
fn cubic_bezier_axis(c1: f32, c2: f32, t: f32) -> f32 {
    let inv = 1.0 - t;
    3.0 * inv * inv * t * c1 + 3.0 * inv * t * t * c2 + t * t * t
}
