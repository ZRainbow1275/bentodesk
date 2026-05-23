//! Drop-shadow scale — three steps. Each is a fully-baked `Shadow` so callers
//! can copy directly into a widget without per-component arithmetic.
//!
//! `md` matches the existing `BentoCard::default_chrome` shadow byte-for-byte
//! (offset `(0, 2)`, blur `12`, colour `0x000000_40`).

use bento_nano_style::{Color, Shadow};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowTokens {
    pub sm: Shadow,
    pub md: Shadow,
    pub lg: Shadow,
}

pub const DEFAULT: ShadowTokens = ShadowTokens {
    sm: Shadow {
        offset_x: 0.0,
        offset_y: 1.0,
        blur: 4.0,
        color: Color::from_u8(0x00, 0x00, 0x00, 0x29),
    },
    md: Shadow {
        offset_x: 0.0,
        offset_y: 2.0,
        blur: 12.0,
        color: Color::from_u8(0x00, 0x00, 0x00, 0x40),
    },
    lg: Shadow {
        offset_x: 0.0,
        offset_y: 8.0,
        blur: 24.0,
        color: Color::from_u8(0x00, 0x00, 0x00, 0x66),
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md_matches_bento_card_default_chrome() {
        assert_eq!(DEFAULT.md.offset_x, 0.0);
        assert_eq!(DEFAULT.md.offset_y, 2.0);
        assert_eq!(DEFAULT.md.blur, 12.0);
        assert_eq!(DEFAULT.md.color, Color::from_u8(0x00, 0x00, 0x00, 0x40));
    }
}
