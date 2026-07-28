//! Business surface — Timeline family (TimelinePanel + SnapshotPicker).
//!
//! Visual specs:
//! - `timeline_panel.snap.md` (R4-C1 time-machine slider, ~820 px modal)
//! - `snapshot_picker.snap.md` (440 px snapshot list dialog)
//!
//! The shell mounts these states in native `Timeline` / `SnapshotPicker`
//! HWNDs and routes real save, load, restore, delete and pin operations through
//! `bento-nano-backend::timeline`. The `build()` entry points provide the
//! widget-tree geometry descriptor consumed when each native surface opens.

use bento_nano_backend::timeline::{Checkpoint, CheckpointMeta};
use bento_nano_layout::Direction;
use bento_nano_style::{Edges, Length};
use bento_nano_widget::ContainerNode;
use smol_str::SmolStr;

pub mod panel;
pub mod snapshot_picker;

pub use panel::build as build_timeline_panel;
pub use snapshot_picker::build as build_snapshot_picker;

/// Default chrome shared by TimelinePanel and
/// SnapshotPicker (24 px / 0 px outer padding respectively are applied per
/// surface; this default is a vertical column with no padding so the
/// caller's `padding` override always wins).
pub(crate) fn default_modal_chrome(padding: Edges) -> ContainerNode {
    ContainerNode {
        direction: Direction::Column,
        width: Length::Auto,
        height: Length::Auto,
        padding,
        ..ContainerNode::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct TimelinePanelState {
    entries: Vec<CheckpointMeta>,
    cursor: usize,
    active: Option<Checkpoint>,
    status: Option<SmolStr>,
    error: Option<SmolStr>,
    restore_confirm_id: Option<SmolStr>,
    delete_confirm_id: Option<SmolStr>,
}

impl TimelinePanelState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[CheckpointMeta] {
        &self.entries
    }

    pub fn cursor_index(&self) -> usize {
        self.cursor
    }

    pub fn active(&self) -> Option<&Checkpoint> {
        self.active.as_ref()
    }

    pub fn status(&self) -> Option<&SmolStr> {
        self.status.as_ref()
    }

    pub fn error(&self) -> Option<&SmolStr> {
        self.error.as_ref()
    }

    pub fn restore_confirmation(&self) -> Option<&SmolStr> {
        self.restore_confirm_id.as_ref()
    }

    pub fn delete_confirmation(&self) -> Option<&SmolStr> {
        self.delete_confirm_id.as_ref()
    }

    pub fn selected_id(&self) -> Option<SmolStr> {
        self.entries.get(self.cursor).map(|entry| entry.id.clone())
    }

    pub fn set_entries(&mut self, entries: Vec<CheckpointMeta>) {
        self.entries = entries;
        if self.entries.is_empty() {
            self.cursor = 0;
            self.active = None;
            self.clear_confirmations();
        } else if self.cursor >= self.entries.len() {
            self.cursor = self.entries.len() - 1;
            self.clear_confirmations();
        }
    }

    pub fn set_active(&mut self, active: Option<Checkpoint>) {
        self.active = active;
    }

    pub fn set_status(&mut self, status: impl Into<SmolStr>) {
        self.status = Some(status.into());
        self.error = None;
    }

    pub fn set_error(&mut self, error: impl Into<SmolStr>) {
        self.error = Some(error.into());
    }

    pub fn clear_status(&mut self) {
        self.status = None;
        self.error = None;
    }

    pub fn clear_confirmations(&mut self) {
        self.restore_confirm_id = None;
        self.delete_confirm_id = None;
    }

    pub fn confirm_restore_or_arm(&mut self, checkpoint_id: SmolStr) -> bool {
        if self
            .restore_confirm_id
            .as_ref()
            .is_some_and(|id| id.as_str() == checkpoint_id.as_str())
        {
            self.clear_confirmations();
            true
        } else {
            self.restore_confirm_id = Some(checkpoint_id);
            self.delete_confirm_id = None;
            false
        }
    }

    pub fn confirm_delete_or_arm(&mut self, checkpoint_id: SmolStr) -> bool {
        if self
            .delete_confirm_id
            .as_ref()
            .is_some_and(|id| id.as_str() == checkpoint_id.as_str())
        {
            self.clear_confirmations();
            true
        } else {
            self.delete_confirm_id = Some(checkpoint_id);
            self.restore_confirm_id = None;
            false
        }
    }

    pub fn select_prev(&mut self) {
        if !self.entries.is_empty() {
            self.cursor = self.cursor.saturating_sub(1);
            self.clear_confirmations();
        }
    }

    pub fn select_next(&mut self) {
        if !self.entries.is_empty() {
            self.cursor = (self.cursor + 1).min(self.entries.len() - 1);
            self.clear_confirmations();
        }
    }

    pub fn select_index(&mut self, index: usize) -> bool {
        if index < self.entries.len() {
            self.cursor = index;
            self.clear_confirmations();
            true
        } else {
            false
        }
    }
}
