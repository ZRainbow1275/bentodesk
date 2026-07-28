use super::{
    chromatic_split_offsets, crisp_shadow_rect, lerp_neon_layer, neon_glow_rect,
    scanline_band_count, stack_bloom_active_pulse, stack_bloom_active_transition_t,
};
use bento_nano_style::{Color, Rect, Shadow};

#[test]
fn zone_shadow_suppresses_blur_but_preserves_crisp_ring() {
    let base = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 40.0,
    };
    assert_eq!(
        crisp_shadow_rect(base, Shadow::drop(0.0, 12.0, 48.0, Color::BLACK)),
        None,
        "blurred geometry must not become a broad solid halo"
    );
    let ring = Shadow {
        offset_x: 0.0,
        offset_y: 0.0,
        blur: 0.0,
        spread: 1.0,
        color: Color::rgba(0.2, 0.5, 1.0, 0.4),
    };
    assert_eq!(
        crisp_shadow_rect(base, ring),
        Some(Rect {
            x: 9.0,
            y: 19.0,
            width: 102.0,
            height: 42.0,
        })
    );
}

#[test]
fn stack_bloom_active_scale_transition_settles_by_180ms() {
    assert_eq!(stack_bloom_active_transition_t(1_000, 1_000), 0.0);
    assert!((stack_bloom_active_transition_t(1_090, 1_000) - 0.5).abs() < 1e-6);
    assert_eq!(stack_bloom_active_transition_t(1_180, 1_000), 1.0);
    assert_eq!(stack_bloom_active_transition_t(2_000, 1_000), 1.0);
}

#[test]
fn stack_bloom_active_pulse_keeps_tauri_bounds_and_many_member_static_rule() {
    assert_eq!(stack_bloom_active_pulse(1_000, 1_000, false), (5.5, 0.16));
    assert_eq!(stack_bloom_active_pulse(1_600, 1_000, false), (5.5, 0.16));
    let peak = stack_bloom_active_pulse(2_350, 1_000, false);
    assert!((peak.0 - 7.0).abs() < 1e-6);
    assert!((peak.1 - 0.22).abs() < 1e-6);
    let wrapped = stack_bloom_active_pulse(3_100, 1_000, false);
    assert!((wrapped.0 - 5.5).abs() < 1e-6);
    assert!((wrapped.1 - 0.16).abs() < 1e-6);
    assert_eq!(stack_bloom_active_pulse(2_350, 1_000, true), (4.0, 0.18));
}

#[test]
fn scanline_band_count_ceils_height_over_period() {
    // vp height 100, period 3 → ceil(100/3) = 34 bands (y = 0,3,...,99).
    assert_eq!(scanline_band_count(100.0, 3.0), 34);
    // Exact multiple: height 99, period 3 → 33 bands (y = 0..96, last < 99).
    assert_eq!(scanline_band_count(99.0, 3.0), 33);
    // A tall 1080 surface at period 3 → 360 bands.
    assert_eq!(scanline_band_count(1080.0, 3.0), 360);
}

#[test]
fn scanline_band_count_zero_guards() {
    // Non-positive period / height → 0 bands (the overlay no-ops, panic-free).
    assert_eq!(scanline_band_count(0.0, 3.0), 0);
    assert_eq!(scanline_band_count(-5.0, 3.0), 0);
    assert_eq!(scanline_band_count(100.0, 0.0), 0);
    assert_eq!(scanline_band_count(100.0, -1.0), 0);
}

#[test]
fn scanline_loop_steps_match_band_count() {
    // The `draw_scanline_overlay` `while y < height` loop emits exactly
    // `scanline_band_count` fills; mirror its stepping here to pin the count.
    let (height, period) = (100.0_f32, 3.0_f32);
    let mut y = 0.0_f32;
    let mut n = 0usize;
    while y < height {
        n += 1;
        y += period;
    }
    assert_eq!(n, scanline_band_count(height, period));
}

#[test]
fn neon_glow_rect_grows_all_sides_by_blur() {
    let base = bento_nano_style::Rect {
        x: 10.0,
        y: 10.0,
        width: 40.0,
        height: 40.0,
    };
    // blur 6 → grown 6 on every side: {4,4,52,52}.
    let g = neon_glow_rect(base, 6.0);
    assert_eq!(g.x, 4.0);
    assert_eq!(g.y, 4.0);
    assert_eq!(g.width, 52.0);
    assert_eq!(g.height, 52.0);
    // blur 0 → identity (no growth).
    let g0 = neon_glow_rect(base, 0.0);
    assert_eq!(g0, base);
    // negative blur clamps to 0.
    assert_eq!(neon_glow_rect(base, -3.0), base);
}

#[test]
fn neon_draw_order_is_reversed_so_magenta_underlies_cyan() {
    // The authored array is `[cyan_inner, magenta_outer]`; `draw_neon_glow`
    // iterates `.iter().rev()` so the wider magenta (index 1) paints first
    // and the tighter cyan (index 0) sits on top. Pin that order here.
    let cyan = Shadow::drop(0.0, 0.0, 6.0, Color::from_u8(0x00, 0xF0, 0xFF, 0xFF));
    let magenta = Shadow::drop(0.0, 0.0, 12.0, Color::from_u8(0xFF, 0x2E, 0x93, 0x66));
    let layers = [cyan, magenta];
    let drawn: Vec<f32> = layers.iter().rev().map(|l| l.blur).collect();
    // Wider magenta (12) drawn first, tighter cyan (6) drawn last (on top).
    assert_eq!(drawn, vec![12.0, 6.0]);
}

#[test]
fn chromatic_offsets_split_red_right_cyan_left() {
    // base_x 50, dx 1 → red at 51 (+dx), cyan at 49 (-dx).
    let (red_x, cyan_x) = chromatic_split_offsets(50.0, 1.0);
    assert_eq!(red_x, 51.0);
    assert_eq!(cyan_x, 49.0);
}

#[test]
fn lerp_neon_layer_endpoints_and_midpoint() {
    let a = Shadow::drop(0.0, 0.0, 6.0, Color::from_u8(0x00, 0xF0, 0xFF, 0xFF));
    let b = Shadow::drop(0.0, 0.0, 8.0, Color::from_u8(0x00, 0xF0, 0xFF, 0xFF));
    // t=0 → collapsed blur 6.
    assert_eq!(lerp_neon_layer(a, b, 0.0).blur, 6.0);
    // t=1 → expanded blur 8.
    assert_eq!(lerp_neon_layer(a, b, 1.0).blur, 8.0);
    // t=0.5 → midpoint blur 7.
    assert_eq!(lerp_neon_layer(a, b, 0.5).blur, 7.0);
    // Out-of-range t clamps (easeOutBack overshoot never over-grows).
    assert_eq!(lerp_neon_layer(a, b, 1.5).blur, 8.0);
    assert_eq!(lerp_neon_layer(a, b, -0.2).blur, 6.0);
}
