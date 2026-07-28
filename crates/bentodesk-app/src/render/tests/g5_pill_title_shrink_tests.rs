use super::{
    PILL_TITLE_MIN_FONT_PX, shrink_font_to_fit, text_width_with_tracking, title_shrink_signature,
};

/// Linear width model: each glyph is `0.6 * font_px` wide, `len` glyphs.
/// Monotone in font size, matching the `shrink_font_to_fit` contract.
fn measure(len: f32) -> impl FnMut(f32) -> f32 {
    move |size: f32| len * size * 0.6
}

#[test]
fn returns_base_when_label_already_fits() {
    // 5 glyphs at 14px → 42 DIPs wide; avail 100 ⇒ no shrink, keep base.
    let got = shrink_font_to_fit(14.0, 100.0, measure(5.0));
    assert!((got - 14.0).abs() < f32::EPSILON);
}

#[test]
fn shrinks_to_largest_fitting_size_no_floor() {
    // 20 glyphs; base 16px → 192 wide; avail 130. The stepper returns the
    // largest whole-px size whose run fits, well above the floor.
    let avail = 130.0_f32;
    let len = 20.0_f32;
    let mut m = measure(len);
    let got = shrink_font_to_fit(16.0, avail, measure(len));
    assert!(got < 16.0, "must have shrunk from base, got {got}");
    assert!(
        got > PILL_TITLE_MIN_FONT_PX,
        "must not bottom out, got {got}"
    );
    // The resolved size genuinely fits and 1px larger would not (the
    // stepper's contract: largest fitting whole-px size).
    assert!(m(got) <= avail, "resolved must fit: {} > {avail}", m(got));
    assert!(m(got + 1.0) > avail, "one px larger must overflow");
}

#[test]
fn bottoms_out_at_floor_when_nothing_fits() {
    // A pathologically long label in a tiny width never fits ⇒ floor (8px),
    // while the draw path still emits the complete text (Tauri v7).
    let got = shrink_font_to_fit(16.0, 4.0, measure(50.0));
    assert!((got - PILL_TITLE_MIN_FONT_PX).abs() < f32::EPSILON);
}

#[test]
fn base_below_floor_is_clamped_up() {
    // A base smaller than the floor never returns below the floor.
    let got = shrink_font_to_fit(4.0, 1000.0, measure(1.0));
    assert!(got >= PILL_TITLE_MIN_FONT_PX);
}

#[test]
fn signature_is_stable_and_discriminates() {
    let a = title_shrink_signature("Documents", 120.0, 14.0, 500, 0.3);
    let b = title_shrink_signature("Documents", 120.0, 14.0, 500, 0.3);
    assert_eq!(a, b, "same inputs must hash identically (cache hit)");
    // Any typography input changing should (almost always) change it.
    assert_ne!(
        a,
        title_shrink_signature("Downloads", 120.0, 14.0, 500, 0.3)
    );
    assert_ne!(a, title_shrink_signature("Documents", 90.0, 14.0, 500, 0.3));
    assert_ne!(
        a,
        title_shrink_signature("Documents", 120.0, 11.0, 500, 0.3)
    );
    assert_ne!(
        a,
        title_shrink_signature("Documents", 120.0, 14.0, 600, 0.3)
    );
    assert_ne!(
        a,
        title_shrink_signature("Documents", 120.0, 14.0, 500, 0.0)
    );
}

#[test]
fn shrink_measurement_includes_letter_spacing_advance() {
    let units = "Compiler".encode_utf16().count();
    let tracking = 0.3;
    let avail = 75.0;
    let mut measure_with_tracking =
        |size: f32| text_width_with_tracking(size * 4.625, units, tracking);

    let got = shrink_font_to_fit(16.0, avail, &mut measure_with_tracking);

    assert_eq!(got, 15.0);
    assert!(measure_with_tracking(got) <= avail);
    assert!(measure_with_tracking(got + 1.0) > avail);
}

#[test]
fn stack_capsule_title_can_shrink_to_tight_grid_column() {
    // V21-C6 — a two-member 220px StackCapsule leaves a tight title column.
    // The stack title must shrink before the floor; this models the full
    // "Benchmark Zone 3" title at the
    // stack token base size (13px).
    let title_len = "Benchmark Zone 3".chars().count() as f32;
    let mut m = |size: f32| title_len * size * 0.52;
    let got = shrink_font_to_fit(13.0, 69.0, &mut m);

    assert!(got < 13.0, "stack title must shrink from base, got {got}");
    assert!(
        got >= PILL_TITLE_MIN_FONT_PX,
        "stack title must respect the shared floor, got {got}"
    );
    assert!(m(got) <= 69.0, "resolved stack title width must fit");
}
