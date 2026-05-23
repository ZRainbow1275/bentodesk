//! Border-radius scale — five steps, all `BorderRadius::all`-uniform.
//!
//! Per-corner radii belong to the widget itself when asymmetric (e.g. tab
//! lozenge). The token table only hands out symmetric radii.

use bento_nano_style::BorderRadius;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadiusTokens {
    pub sm: BorderRadius,
    pub md: BorderRadius,
    pub lg: BorderRadius,
    pub xl: BorderRadius,
    /// "full" — large enough to render any practical button/avatar as a
    /// pill; concrete value is 9999px so layout never pre-clamps it.
    pub full: BorderRadius,
}

pub const DEFAULT: RadiusTokens = RadiusTokens {
    sm: BorderRadius::all(4.0),
    md: BorderRadius::all(6.0),
    lg: BorderRadius::all(8.0),
    xl: BorderRadius::all(12.0),
    full: BorderRadius::all(9999.0),
};
