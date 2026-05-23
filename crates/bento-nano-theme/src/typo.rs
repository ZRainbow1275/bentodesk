//! Typography tokens — font family + size scale + weight + line-height.
//!
//! `font_family` is `SmolStr` so the literal "Microsoft YaHei UI" stays inline
//! (≤22 bytes, no allocation). Sizes are `pt` to match the existing `TextNode::
//! font_size_pt` field. TL Ruling 3 2026-05-21: Win11 CJK render baseline.

use smol_str::SmolStr;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSizes {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xxl: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontWeights {
    pub normal: u16,
    pub medium: u16,
    pub bold: u16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineHeights {
    pub tight: f32,
    pub normal: f32,
    pub loose: f32,
}

/// Typography token bundle. Not `Copy` because `SmolStr` isn't `Copy`; the
/// aggregate `ThemeTokens` is handed out by `&'static` reference (per the §11
/// ruling) and never cloned per-frame.
#[derive(Debug, Clone, PartialEq)]
pub struct TypoTokens {
    pub font_family: SmolStr,
    pub sizes: FontSizes,
    pub weights: FontWeights,
    pub line_heights: LineHeights,
}
