use super::*;

/// Wave C — format a zone item count for the collapsed pill badge. Caps
/// the display at "99+" so the badge geometry doesn't need to grow past
/// `PILL_BADGE_MIN_WIDTH` for typical zones; >999 items is still rendered
/// as "999+" so the result fits the 4-digit budget in
/// `zone_pill_geometry::badge_width_for_count`.
/// Floor retained only for the legacy stack-capsule title shrink path. Ordinary
/// Zone pills use a fixed readable role plus DWrite ellipsis.
pub(super) const PILL_TITLE_MIN_FONT_PX: f32 = 8.0;

/// G5 (2026-06-01) — quantised cache signature for the stack title shrink memo.
/// Folds the label bytes, the available width (rounded to whole DIPs) and the
/// tier base font (×4, rounded), weight, and tracking into one `u64`. A
/// per-frame re-paint of the SAME label at the SAME width/typography hashes
/// identically → cache hit → no DWrite measure, no allocation (§10). Collisions
/// only over-trigger a (correct) re-measure, never a wrong size.
pub(super) fn title_shrink_signature(
    label: &str,
    avail_w: f32,
    base_px: f32,
    weight: u16,
    tracking: f32,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    label.hash(&mut h);
    (avail_w.max(0.0).round() as u32).hash(&mut h);
    ((base_px.max(0.0) * 4.0).round() as u32).hash(&mut h);
    weight.hash(&mut h);
    ((tracking.max(0.0) * 100.0).round() as u32).hash(&mut h);
    h.finish()
}

#[inline]
pub(super) fn text_width_with_tracking(base_width: f32, utf16_units: usize, tracking: f32) -> f32 {
    let tracked_gaps = utf16_units.saturating_sub(1) as f32;
    (base_width + tracking * tracked_gaps).max(0.0)
}

/// G5 (2026-06-01) — pure font-shrink stepper for the pill title (`useTextAbbr`
/// parity), factored out for unit testing without DWrite. Walks the font size
/// down from `base_px` in 1px steps to `PILL_TITLE_MIN_FONT_PX`, calling
/// `measure(size)` (the rendered text width at that size) at each step, and
/// returns the FIRST (largest) size whose measured width fits `avail_w`. If
/// nothing fits down to the floor, returns the floor; callers still draw the
/// complete text, exactly as Tauri v7 `textAbbr.ts` does at `MIN_FONT_SIZE_PX`.
///
/// `measure` is assumed monotonic in size (smaller font ⇒ narrower run), so the
/// first fit found while stepping down is the largest fitting size.
pub(super) fn shrink_font_to_fit(
    base_px: f32,
    avail_w: f32,
    mut measure: impl FnMut(f32) -> f32,
) -> f32 {
    let base = base_px.max(PILL_TITLE_MIN_FONT_PX);
    let mut size = base;
    // Step down in whole-px increments; the loop is bounded by the base→floor
    // span (≤ ~8 iterations for the 11/14/16px tiers), so it is cheap and total.
    while size >= PILL_TITLE_MIN_FONT_PX {
        if measure(size) <= avail_w {
            return size;
        }
        size -= 1.0;
    }
    PILL_TITLE_MIN_FONT_PX
}

pub(super) fn format_small_count(count: usize) -> smol_str::SmolStr {
    // <1000 renders the literal count; >=1000 caps at the 4-char "999+"
    // budget (the <100 vs <1000 split produced identical text, so merged).
    if count < 1000 {
        smol_str::SmolStr::new(count.to_string())
    } else {
        smol_str::SmolStr::new_static("999+")
    }
}

pub(super) fn live_folder_badge_text(path: &str) -> smol_str::SmolStr {
    const MAX_PATH_CHARS: usize = 96;
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return smol_str::SmolStr::new_static("Live folder: <invalid path>");
    }
    let char_count = trimmed.chars().count();
    if char_count <= MAX_PATH_CHARS {
        return smol_str::SmolStr::new(format!("Live: {trimmed}"));
    }
    let head: String = trimmed.chars().take(44).collect();
    let tail: String = trimmed
        .chars()
        .rev()
        .take(44)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    smol_str::SmolStr::new(format!("Live: {head}…{tail}"))
}

pub(super) fn with_alpha(color: Color, alpha: f32) -> Color {
    Color {
        a: alpha.clamp(0.0, 1.0),
        ..color
    }
}

#[inline]
pub(super) fn fade_color(color: Color, opacity: f32) -> Color {
    with_alpha(color, color.a * opacity.clamp(0.0, 1.0))
}

#[inline]
pub(super) fn settings_encryption_mode_button_fill_color(
    is_active: bool,
    is_hovered: bool,
    base_fill: Color,
    hover_fill: Color,
    active_fill: Color,
) -> Color {
    if is_active {
        active_fill
    } else if is_hovered {
        hover_fill
    } else {
        base_fill
    }
}

/// P1 (#7 fix wave 2026-06-01) — pure caret-blink phase. Given a wall-clock
/// `now_ms` (e.g. `GetTickCount`), returns whether the text-field caret should
/// be PAINTED this frame. Windows blinks the caret at ≈530ms (the default
/// `GetCaretBlinkTime`): ON for one 530ms half-period, OFF for the next. Pure
/// (no state, no allocation) so it's unit-testable and §10-safe.
pub fn settings_caret_on(now_ms: u32) -> bool {
    (now_ms / 530) % 2 == 0
}

/// P2 (#7 fix wave 2026-06-01) — the user-visible mode label that MATCHES the
/// mode-button TITLES (Tauri uses one `modeLabel()` for the current-mode value,
/// the buttons, AND the applied banner). Passphrase maps to the FULL token
/// (`ENCRYPTION_MODE_PASSPHRASE_FULL`, id 236 = 自定义口令), NOT the short id 86
/// (密码, `ENCRYPTION_MODE_PASSPHRASE`) the prior current-mode paint used.
/// None/DPAPI reuse the shared button ids — so the current-mode VALUE, the
/// active button TITLE, and the P9 applied banner all read identically (the
/// parity invariant). Replaces the old `localized_encryption_mode` short-label
/// helper, which had no remaining call site after this fix.
pub(super) fn localized_encryption_mode_button_label(
    mode: crate::state::SettingsEncryptionMode,
) -> &'static str {
    use crate::state::SettingsEncryptionMode;
    use bento_nano_style::i18n_zh_cn::ids;
    match mode {
        SettingsEncryptionMode::None => bento_nano_style::t(ids::ENCRYPTION_MODE_NONE),
        SettingsEncryptionMode::Dpapi => bento_nano_style::t(ids::ENCRYPTION_MODE_DPAPI),
        SettingsEncryptionMode::Passphrase => {
            bento_nano_style::t(ids::ENCRYPTION_MODE_PASSPHRASE_FULL)
        }
    }
}

pub(super) fn parse_hex_color(raw: &str) -> Option<Color> {
    let bytes = raw.as_bytes();
    if bytes.len() != 7 || bytes.first().copied() != Some(b'#') {
        return None;
    }
    let r = parse_hex_byte(bytes[1], bytes[2])?;
    let g = parse_hex_byte(bytes[3], bytes[4])?;
    let b = parse_hex_byte(bytes[5], bytes[6])?;
    Some(Color::from_u8(r, g, b, 0xE0))
}

pub(super) fn parse_hex_byte(hi: u8, lo: u8) -> Option<u8> {
    Some((parse_hex_nibble(hi)? << 4) | parse_hex_nibble(lo)?)
}

pub(super) fn parse_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
