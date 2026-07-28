use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDragCandidate {
    pub zone_id: ZoneId,
    pub item_id: ZoneItemId,
    pub path: SmolStr,
    pub start_x: i32,
    pub start_y: i32,
    pub last_x: i32,
    pub last_y: i32,
    pub is_internal_dragging: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneEditorSession {
    pub zone_id: ZoneId,
    pub draft_name: String,
    pub draft_icon: SmolStr,
    pub draft_accent_color: Option<SmolStr>,
    pub draft_grid_columns: u32,
    pub draft_capsule_size: SmolStr,
    pub draft_capsule_shape: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemFileRenameSession {
    pub zone_id: ZoneId,
    pub item_id: ZoneItemId,
    pub draft_name: String,
    pub current_path: SmolStr,
    pub status: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconPickerSession {
    pub zone_id: Option<ZoneId>,
    pub selected_icon: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PalettePickerSession {
    pub target: PaletteTarget,
    pub selected_accent: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooltipSession {
    pub text: SmolStr,
}

/// Expanded PanelHeader action button kind shared by shell hit-testing and D2D paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelHeaderButtonKind {
    /// Magnifier button that opens Search.
    Search,
    /// Close button that collapses the expanded panel.
    Close,
}

/// Currently hovered expanded PanelHeader button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelHeaderButtonHover {
    pub zone_id: ZoneId,
    pub button: PanelHeaderButtonKind,
}

impl PanelHeaderButtonHover {
    pub const fn new(zone_id: ZoneId, button: PanelHeaderButtonKind) -> Self {
        Self { zone_id, button }
    }
}
