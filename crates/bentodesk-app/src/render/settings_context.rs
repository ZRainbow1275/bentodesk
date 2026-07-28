use super::*;

#[derive(Clone, Copy)]
pub(super) struct SettingsRenderContext {
    pub(super) viewport: bentodesk_style::Size,
    pub(super) body: Rect,
    pub(super) palette: bentodesk_style::tokens::PaletteTauri,
    pub(super) title_color: Color,
    pub(super) label_color: Color,
    pub(super) accent_on: Color,
    pub(super) track_off: Color,
    pub(super) chip_bg: Color,
    pub(super) chip_border: Color,
    pub(super) toggle_knob_color: Color,
    pub(super) chip_radius: BorderRadius,
    pub(super) btn_radius: BorderRadius,
    pub(super) settings_now_ms: u32,
    pub(super) caret_on: bool,
}
