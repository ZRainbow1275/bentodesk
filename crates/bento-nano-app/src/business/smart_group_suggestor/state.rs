use super::*;

// -----------------------------------------------------------------------------
// SuggestorAction — closed enum of one-shot user intents.
// -----------------------------------------------------------------------------

/// User intent recorded by the panel state machine. Drained once per
/// frame via [`SuggestorState::take_action`]. Translates 1:1 into a
/// [`Command`] (or no-op for `Close`) in the shell consumer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SuggestorAction {
    /// User clicked **Apply** on a suggestion row. Carries the full
    /// suggestion payload so the shell can forward it to the backend
    /// without re-resolving from the suggestion list.
    Apply {
        suggestion_id: SmolStr,
        suggestion: Box<SuggestedGroup>,
    },
    /// User dismissed a single row. Carries the suggestion's stable id
    /// so the shell can prune the matching entry.
    Dismiss { suggestion_id: SmolStr },
    /// User closed the entire panel (close button / Escape / scrim
    /// click). Shell hides the host window — no Command required.
    Close,
}

impl fmt::Display for SuggestorAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Apply { suggestion, .. } => write!(f, "Apply({:?})", suggestion.name),
            Self::Dismiss { suggestion_id } => write!(f, "Dismiss({suggestion_id:?})"),
            Self::Close => f.write_str("Close"),
        }
    }
}

impl SuggestorAction {
    /// Translate the action into a dispatcher [`Command`], or `None` for
    /// the `Close` variant which is shell-local.
    pub fn into_command(self) -> Option<Command> {
        match self {
            Self::Apply { suggestion, .. } => Some(Command::GroupingApply { suggestion }),
            Self::Dismiss { suggestion_id } => Some(Command::SuggestorDismiss { suggestion_id }),
            Self::Close => None,
        }
    }
}

// -----------------------------------------------------------------------------
// SuggestorState — runtime state for the panel.
// -----------------------------------------------------------------------------

/// One suggestion view-row in the panel. Stable `id` is what the hover
/// bridge + dismiss action key off — the backend's `SuggestedGroup`
/// has no native id field, so we derive a `SmolStr` from
/// `name:matching_files.len()` at insertion time.
#[derive(Debug, Clone)]
pub struct SuggestionEntry {
    pub id: SmolStr,
    pub suggestion: SuggestedGroup,
    selected_paths: SmallVec<[SmolStr; 8]>,
    focused_path_index: usize,
}

impl SuggestionEntry {
    /// Build an entry from a backend suggestion, deriving a stable id.
    pub fn from_suggestion(s: SuggestedGroup) -> Self {
        let id = suggestion_id(&s);
        let selected_paths = s
            .matching_files
            .iter()
            .map(SmolStr::new)
            .collect::<SmallVec<[SmolStr; 8]>>();
        Self {
            id,
            suggestion: s,
            selected_paths,
            focused_path_index: 0,
        }
    }

    pub fn total_path_count(&self) -> usize {
        self.suggestion.matching_files.len()
    }

    pub fn selected_path_count(&self) -> usize {
        self.selected_paths.len()
    }

    pub fn focused_path_index(&self) -> usize {
        if self.suggestion.matching_files.is_empty() {
            0
        } else {
            self.focused_path_index
                .min(self.suggestion.matching_files.len() - 1)
        }
    }

    pub fn is_path_selected(&self, path_index: usize) -> bool {
        let Some(path) = self.suggestion.matching_files.get(path_index) else {
            return false;
        };
        self.selected_paths
            .iter()
            .any(|selected| selected.as_str() == path)
    }

    pub fn preview_start_index(&self) -> usize {
        let total = self.total_path_count();
        if total <= MAX_VISIBLE_PREVIEW_FILES {
            return 0;
        }
        let half = MAX_VISIBLE_PREVIEW_FILES / 2;
        self.focused_path_index()
            .saturating_sub(half)
            .min(total.saturating_sub(MAX_VISIBLE_PREVIEW_FILES))
    }

    pub fn preview_file_count(&self) -> usize {
        self.total_path_count()
            .saturating_sub(self.preview_start_index())
            .min(MAX_VISIBLE_PREVIEW_FILES)
    }

    pub fn preview_path_index(&self, preview_offset: usize) -> Option<usize> {
        let path_index = self.preview_start_index().checked_add(preview_offset)?;
        (path_index < self.total_path_count()).then_some(path_index)
    }

    pub fn selected_matching_files(&self) -> Vec<String> {
        self.suggestion
            .matching_files
            .iter()
            .filter(|path| {
                self.selected_paths
                    .iter()
                    .any(|selected| selected.as_str() == path.as_str())
            })
            .cloned()
            .collect()
    }

    fn select_all_paths(&mut self) {
        self.selected_paths = self
            .suggestion
            .matching_files
            .iter()
            .map(SmolStr::new)
            .collect();
    }

    fn select_no_paths(&mut self) {
        self.selected_paths.clear();
    }

    fn focus_prev_path(&mut self) -> bool {
        if self.suggestion.matching_files.is_empty() {
            self.focused_path_index = 0;
            return false;
        }
        self.focused_path_index = self.focused_path_index().saturating_sub(1);
        true
    }

    fn focus_next_path(&mut self) -> bool {
        if self.suggestion.matching_files.is_empty() {
            self.focused_path_index = 0;
            return false;
        }
        self.focused_path_index =
            (self.focused_path_index() + 1).min(self.suggestion.matching_files.len() - 1);
        true
    }

    fn toggle_path(&mut self, path_index: usize) -> bool {
        let Some(path) = self.suggestion.matching_files.get(path_index) else {
            return false;
        };
        if let Some(index) = self
            .selected_paths
            .iter()
            .position(|selected| selected.as_str() == path)
        {
            self.selected_paths.remove(index);
        } else {
            self.selected_paths.push(SmolStr::new(path.as_str()));
        }
        self.focused_path_index = path_index;
        true
    }
}

/// Panel runtime state.
///
/// - `entries` — suggestions currently visible (capped at
///   [`MAX_VISIBLE_SUGGESTIONS`]).
/// - `hovered_id` — id of the row the cursor is over, drives the
///   `HighlightOverlay` preview. `None` when no row is hovered.
/// - `applying_id` — id of the row whose Apply button is currently in
///   flight; the shell sets this before forwarding the command and
///   clears it once the backend ack returns. Disables every Apply
///   button while non-`None`.
/// - `pending_action` — the latest one-shot [`SuggestorAction`] waiting
///   for the shell to drain.
#[derive(Debug, Default)]
pub struct SuggestorState {
    entries: SmallVec<[SuggestionEntry; MAX_VISIBLE_SUGGESTIONS]>,
    selected_index: usize,
    hovered_id: Option<SmolStr>,
    applying_id: Option<SmolStr>,
    pending_action: Option<SuggestorAction>,
}

impl SuggestorState {
    /// New empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the suggestion list — backend has answered the most
    /// recent analyse pass. Truncates to [`MAX_VISIBLE_SUGGESTIONS`]
    /// so the inline `SmallVec` stays inline. Resets transient state
    /// (hover, applying) since old ids are no longer valid.
    pub fn set_suggestions(&mut self, suggestions: Vec<SuggestedGroup>) {
        let mut entries: SmallVec<[SuggestionEntry; MAX_VISIBLE_SUGGESTIONS]> = SmallVec::new();
        for s in suggestions.into_iter().take(MAX_VISIBLE_SUGGESTIONS) {
            entries.push(SuggestionEntry::from_suggestion(s));
        }
        self.entries = entries;
        self.selected_index = 0;
        self.hovered_id = None;
        self.applying_id = None;
    }

    /// Borrow the current suggestion entry list.
    pub fn entries(&self) -> &[SuggestionEntry] {
        &self.entries
    }

    /// Number of rows currently visible in the runtime panel.
    pub fn visible_count(&self) -> usize {
        self.entries.len()
    }

    /// Current keyboard cursor index.
    pub const fn selected_index(&self) -> usize {
        self.selected_index
    }

    /// Current selected row.
    pub fn selected_entry(&self) -> Option<&SuggestionEntry> {
        self.entries.get(self.selected_index)
    }

    /// Number of visible manual checkbox rows for the selected suggestion.
    pub fn selected_preview_file_count(&self) -> usize {
        self.selected_entry()
            .map(SuggestionEntry::preview_file_count)
            .unwrap_or(0)
    }

    /// Select a visible row by index.
    pub fn select_index(&mut self, row_index: usize) -> bool {
        if row_index >= self.entries.len() {
            return false;
        }
        self.selected_index = row_index;
        true
    }

    /// Move the selected cursor up.
    pub fn select_prev(&mut self) {
        if self.entries.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.saturating_sub(1);
        }
    }

    /// Move the selected cursor down.
    pub fn select_next(&mut self) {
        if self.entries.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = (self.selected_index + 1).min(self.entries.len() - 1);
        }
    }

    /// Move the focused manual file left/up within the selected row.
    pub fn focus_prev_path(&mut self) -> bool {
        let Some(entry) = self.entries.get_mut(self.selected_index) else {
            return false;
        };
        entry.focus_prev_path()
    }

    /// Move the focused manual file right/down within the selected row.
    pub fn focus_next_path(&mut self) -> bool {
        let Some(entry) = self.entries.get_mut(self.selected_index) else {
            return false;
        };
        entry.focus_next_path()
    }

    /// Toggle the focused manual checkbox in the selected row.
    pub fn toggle_focused_path(&mut self) -> bool {
        let Some(entry) = self.entries.get_mut(self.selected_index) else {
            return false;
        };
        entry.toggle_path(entry.focused_path_index())
    }

    /// Toggle a visible manual checkbox by preview-row offset.
    pub fn toggle_preview_file(&mut self, preview_offset: usize) -> bool {
        let Some(entry) = self.entries.get_mut(self.selected_index) else {
            return false;
        };
        let Some(path_index) = entry.preview_path_index(preview_offset) else {
            return false;
        };
        entry.toggle_path(path_index)
    }

    /// Select all matching files for the selected row.
    pub fn select_all_for_selected(&mut self) -> bool {
        let Some(entry) = self.entries.get_mut(self.selected_index) else {
            return false;
        };
        entry.select_all_paths();
        true
    }

    /// Select no matching files for the selected row.
    pub fn select_none_for_selected(&mut self) -> bool {
        let Some(entry) = self.entries.get_mut(self.selected_index) else {
            return false;
        };
        entry.select_no_paths();
        true
    }

    /// Currently-hovered suggestion id (drives the highlight overlay).
    pub fn hovered_id(&self) -> Option<&SmolStr> {
        self.hovered_id.as_ref()
    }

    /// The hovered suggestion entry, if any.
    pub fn hovered_entry(&self) -> Option<&SuggestionEntry> {
        let id = self.hovered_id.as_ref()?;
        self.entries.iter().find(|e| &e.id == id)
    }

    /// Suggestion id whose Apply is in flight (for button disable
    /// gating).
    pub fn applying_id(&self) -> Option<&SmolStr> {
        self.applying_id.as_ref()
    }

    /// Mark a row's Apply as in flight — called by the shell after it
    /// drains an `Apply` action and forwards the Command.
    pub fn mark_applying(&mut self, id: SmolStr) {
        self.applying_id = Some(id);
    }

    /// Backend ack returned — clear the in-flight marker.
    pub fn clear_applying(&mut self) {
        self.applying_id = None;
    }

    /// Cursor entered a row — record the hover so the `HighlightOverlay`
    /// can preview the matching items. Idempotent; setting the same id
    /// twice is a no-op.
    pub fn on_row_hover(&mut self, id: SmolStr) {
        self.hovered_id = Some(id);
    }

    /// Cursor left every row — clear the hover.
    pub fn on_row_leave(&mut self) {
        self.hovered_id = None;
    }

    /// User clicked **Apply** on the row identified by `id`. Records an
    /// `Apply` action carrying the suggestion payload. Returns `true`
    /// when the id matched a known entry; `false` (no action recorded)
    /// when the id was stale (suggestions list changed since the click).
    pub fn apply(&mut self, id: &str) -> bool {
        let Some(entry) = self.entries.iter().find(|e| e.id.as_str() == id) else {
            return false;
        };
        let selected_files = entry.selected_matching_files();
        if selected_files.is_empty() {
            return false;
        }
        let mut suggestion = entry.suggestion.clone();
        suggestion.matching_files = selected_files;
        self.pending_action = Some(SuggestorAction::Apply {
            suggestion_id: entry.id.clone(),
            suggestion: Box::new(suggestion),
        });
        true
    }

    /// Apply the currently-selected row.
    pub fn apply_selected(&mut self) -> bool {
        let Some(id) = self.selected_entry().map(|entry| entry.id.clone()) else {
            return false;
        };
        self.apply(id.as_str())
    }

    /// User dismissed a single row. Records a `Dismiss` action. Like
    /// [`apply`], returns `false` for a stale id.
    pub fn dismiss(&mut self, id: &str) -> bool {
        let Some(entry) = self.entries.iter().find(|e| e.id.as_str() == id) else {
            return false;
        };
        self.pending_action = Some(SuggestorAction::Dismiss {
            suggestion_id: entry.id.clone(),
        });
        true
    }

    /// Dismiss the currently-selected row.
    pub fn dismiss_selected(&mut self) -> bool {
        let Some(id) = self.selected_entry().map(|entry| entry.id.clone()) else {
            return false;
        };
        self.dismiss(id.as_str())
    }

    /// Remove a row after the dispatcher has consumed `SuggestorDismiss`.
    pub fn remove_entry(&mut self, id: &str) -> bool {
        let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.id.as_str() == id)
        else {
            return false;
        };
        self.entries.remove(index);
        if self.entries.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= self.entries.len() {
            self.selected_index = self.entries.len() - 1;
        }
        if self.hovered_id.as_deref() == Some(id) {
            self.hovered_id = None;
        }
        if self.applying_id.as_deref() == Some(id) {
            self.applying_id = None;
        }
        true
    }

    /// User closed the panel (close button / Escape / scrim click).
    pub fn close(&mut self) {
        self.pending_action = Some(SuggestorAction::Close);
    }

    /// Drain the latest action. Returns `None` until the user clicks
    /// Apply / Dismiss / Close. One-shot — subsequent calls without
    /// further interaction return `None`.
    pub fn take_action(&mut self) -> Option<SuggestorAction> {
        self.pending_action.take()
    }

    /// Whether an action is pending — diagnostics + UI affordance gating.
    pub fn has_pending_action(&self) -> bool {
        self.pending_action.is_some()
    }
}

// -----------------------------------------------------------------------------
// Builder — returns the chrome Container.
// -----------------------------------------------------------------------------

/// Build the SmartGroupSuggestor panel subtree. Returns the chrome
/// Container today; the row composition (icon + meta + badge + Apply +
/// Dismiss) attaches in the next pass when widget-library ships the
/// final List + Modal primitives. Geometry is pinned per snap.md.
pub fn build() -> WidgetNode {
    let palette = theme::current().palette;
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Px(PANEL_WIDTH_PX),
        height: Length::Auto,
        padding: Edges::all(PANEL_PADDING_PX),
        background: palette.surface,
        radius: BorderRadius::all(PANEL_CORNER_RADIUS_PX),
        ..ContainerNode::default()
    })
}

// -----------------------------------------------------------------------------
// Tests.
// -----------------------------------------------------------------------------
