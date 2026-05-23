//! Spacing scale — Tailwind-aligned 4 px base.
//!
//! Used for padding, margin, and inter-child gaps in `Toolbar` (`spacing` field
//! today reads `8.0` directly — T-004 migrates that to `theme.spacing.sm`).

/// Six-step scale. `xs=2, sm=4, md=8, lg=12, xl=16, xxl=24` — matches the
/// React tailwind tokens (spacing-1..6) used in the source UI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpacingTokens {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xxl: f32,
}

pub const DEFAULT: SpacingTokens = SpacingTokens {
    xs: 2.0,
    sm: 4.0,
    md: 8.0,
    lg: 12.0,
    xl: 16.0,
    xxl: 24.0,
};
