use super::settings_caret_on;

/// P1 (#7 fix wave 2026-06-01) — the caret is ON for the first ~530ms
/// half-period and OFF for the next, toggling at the Windows blink cadence.
/// Pure function of `now_ms` (no state) so it's directly unit-testable.
#[test]
fn caret_blinks_on_530ms_half_period() {
    // First half-period (0..530) → ON.
    assert!(settings_caret_on(0));
    assert!(settings_caret_on(265));
    assert!(settings_caret_on(529));
    // Second half-period (530..1060) → OFF.
    assert!(!settings_caret_on(530));
    assert!(!settings_caret_on(800));
    assert!(!settings_caret_on(1059));
    // Third half-period (1060..1590) → ON again (period wraps).
    assert!(settings_caret_on(1060));
    assert!(settings_caret_on(1500));
    // The phase alternates every 530ms with no gaps.
    for k in 0..16u32 {
        let mid = k * 530 + 100;
        assert_eq!(settings_caret_on(mid), k % 2 == 0, "half-period {k} phase");
    }
}
