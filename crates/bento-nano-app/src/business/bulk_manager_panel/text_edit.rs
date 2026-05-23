use super::BulkManagerState;

/// Maximum user-entered draft length for the selected-stack BulkManager
/// text editor. Alias text may be non-ASCII, so the limit is counted in
/// scalar values rather than bytes.
pub const TEXT_EDIT_DRAFT_LIMIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkTextEditField {
    Alias,
    Icon,
    Accent,
    CapsuleSize,
    DisplayMode,
}

impl BulkTextEditField {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Alias => "Alias",
            Self::Icon => "Icon",
            Self::Accent => "Accent",
            Self::CapsuleSize => "Capsule",
            Self::DisplayMode => "Mode",
        }
    }

    pub const fn placeholder(self) -> &'static str {
        match self {
            Self::Alias => "blank clears alias",
            Self::Icon => "folder, star, archive...",
            Self::Accent => "#3b82f6",
            Self::CapsuleSize => "small | medium | large",
            Self::DisplayMode => "hover | always | click | clear",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Alias => Self::Icon,
            Self::Icon => Self::Accent,
            Self::Accent => Self::CapsuleSize,
            Self::CapsuleSize => Self::DisplayMode,
            Self::DisplayMode => Self::Alias,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkTextEditState {
    pub field: BulkTextEditField,
    pub draft: String,
}

impl BulkTextEditState {
    pub fn new(field: BulkTextEditField) -> Self {
        Self {
            field,
            draft: String::new(),
        }
    }
}

impl BulkManagerState {
    pub fn text_edit(&self) -> Option<&BulkTextEditState> {
        self.text_edit.as_ref()
    }

    pub fn start_text_edit(&mut self, field: BulkTextEditField) {
        self.text_edit = Some(BulkTextEditState::new(field));
    }

    pub fn cancel_text_edit(&mut self) {
        self.text_edit = None;
    }

    pub fn cycle_text_edit_field(&mut self) {
        if let Some(edit) = self.text_edit.as_mut() {
            edit.field = edit.field.next();
            edit.draft.clear();
        }
    }

    pub fn push_text_edit_char(&mut self, ch: char) -> bool {
        let Some(edit) = self.text_edit.as_mut() else {
            return false;
        };
        if ch.is_control() || edit.draft.chars().count() >= TEXT_EDIT_DRAFT_LIMIT {
            return false;
        }
        edit.draft.push(ch);
        true
    }

    pub fn backspace_text_edit(&mut self) -> bool {
        let Some(edit) = self.text_edit.as_mut() else {
            return false;
        };
        edit.draft.pop().is_some()
    }
}
