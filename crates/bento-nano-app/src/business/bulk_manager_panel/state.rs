use super::*;

// -----------------------------------------------------------------------------
// Table column metadata — drives sort cycling + column header rendering.
// -----------------------------------------------------------------------------

/// One sortable column in the zone table. The sort key cycles through
/// `Name → Items → Accent → Size`; same-key clicks toggle direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortKey {
    #[default]
    Name,
    Items,
    Accent,
    Size,
}

impl SortKey {
    /// Iteration order matches snap.md (left → right across the table).
    pub const ALL: &'static [Self] = &[Self::Name, Self::Items, Self::Accent, Self::Size];

    /// Wire-format token for serde / scripting forward-compat.
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Items => "items",
            Self::Accent => "accent",
            Self::Size => "size",
        }
    }

    /// Static label for the column header cell.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Items => "Items",
            Self::Accent => "Accent",
            Self::Size => "Size",
        }
    }
}

/// Sort direction — ascending or descending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

impl SortDirection {
    /// Flip ascending ⇄ descending.
    pub const fn flipped(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

// -----------------------------------------------------------------------------
// ZoneRow — one entry the panel renders in the table. Mirrors the 1.x
// `BentoZone` slice that the bulk panel actually reads.
// -----------------------------------------------------------------------------

/// One row in the zone table. Pruned shape — only the columns the panel
/// renders + the [`ZoneId`] needed to key selection / dispatch.
///
/// Built by the shell from the live `BentoZone` list before pumping into
/// [`BulkManagerState::set_zones`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneRow {
    /// Stable zone id — selection + bulk-action dispatch key.
    pub id: ZoneId,
    /// Display name (alias if the user set one, else the canonical name).
    pub display_name: SmolStr,
    /// Item count rendered in the Items column.
    pub item_count: u32,
    /// Accent colour hex string (`#rrggbb`); empty when unset.
    pub accent_hex: SmolStr,
    /// Whether the zone is currently rendered on the desktop canvas.
    pub visible: bool,
    /// Whether user layout helpers should leave the zone in place.
    pub locked: bool,
    /// Icon slug currently assigned to the zone.
    pub icon_slug: SmolStr,
    /// Capsule size token currently assigned to the zone.
    pub capsule_size: SmolStr,
    /// Display mode override, or `inherit` when unset.
    pub display_mode: SmolStr,
    /// Width % of the canvas (0..=100).
    pub width_percent: u32,
    /// Height % of the canvas (0..=100).
    pub height_percent: u32,
    /// Position x % of the canvas (0..=100).
    pub position_x_percent: u32,
    /// Position y % of the canvas (0..=100).
    pub position_y_percent: u32,
}

impl ZoneRow {
    /// Area metric used by `SortKey::Size` (`w% × h%`).
    pub fn area_percent(&self) -> u64 {
        u64::from(self.width_percent) * u64::from(self.height_percent)
    }
}

// -----------------------------------------------------------------------------
// BulkManagerAction — closed enum of one-shot user intents.
// -----------------------------------------------------------------------------

/// User intent recorded by the panel state machine. Drained once per
/// frame via [`BulkManagerState::take_action`]. The shell sequences
/// the appropriate per-zone dispatcher Commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BulkManagerAction {
    /// Hide the listed zones via `Command::BulkSetZonesVisible`.
    Hide { ids: Vec<ZoneId> },
    /// Show the listed zones via `Command::BulkSetZonesVisible`.
    Show { ids: Vec<ZoneId> },
    /// Delete the listed zones via `Command::BulkDeleteZones`.
    Delete { ids: Vec<ZoneId> },
    /// Move the listed zones by `delta` via `Command::BulkMoveZones`.
    Move { ids: Vec<ZoneId>, delta: Point },
    /// User dismissed the panel (close button, Escape, or scrim click).
    /// Shell hides the host window — no Command required.
    Close,
}

impl fmt::Display for BulkManagerAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hide { ids } => write!(f, "Hide({})", ids.len()),
            Self::Show { ids } => write!(f, "Show({})", ids.len()),
            Self::Delete { ids } => write!(f, "Delete({})", ids.len()),
            Self::Move { ids, delta } => {
                write!(f, "Move({}, dx={}, dy={})", ids.len(), delta.x, delta.y)
            }
            Self::Close => f.write_str("Close"),
        }
    }
}

// -----------------------------------------------------------------------------
// BulkManagerState — runtime state for the panel.
// -----------------------------------------------------------------------------

/// Panel runtime state.
///
/// - `zones` — full row list as last seeded by the shell.
/// - `search` — current search filter (case-insensitive substring match
///   on `display_name`). Empty string disables the filter.
/// - `sort_key` / `sort_direction` — table sort. Cycle direction on
///   same-key click.
/// - `selected` — set of currently-selected zone ids (inline buffer for
///   the steady-state ≤ 8 batch).
/// - `cursor_index` — keyboard-focused visible row; selection remains keyed
///   by `ZoneId`, not row index.
/// - `pending_action` — latest one-shot [`BulkManagerAction`] the shell
///   has yet to drain.
#[derive(Debug, Default)]
pub struct BulkManagerState {
    zones: Vec<ZoneRow>,
    search: String,
    sort_key: SortKey,
    sort_direction: SortDirection,
    selected: SmallVec<[ZoneId; 8]>,
    cursor_index: usize,
    pending_action: Option<BulkManagerAction>,
    pub(super) text_edit: Option<BulkTextEditState>,
    search_focused: bool,
    delete_confirm_ids: SmallVec<[ZoneId; 8]>,
}

impl BulkManagerState {
    /// New empty state. The shell calls [`set_zones`] before the first
    /// paint with the live zone list.
    ///
    /// [`set_zones`]: BulkManagerState::set_zones
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the zone list — typically called after a refresh. Resets
    /// the selection (old ids may be stale) and drops any pending
    /// action (it referenced the old set).
    pub fn set_zones(&mut self, zones: Vec<ZoneRow>) {
        self.zones = zones;
        self.selected.clear();
        self.cursor_index = 0;
        self.pending_action = None;
        self.text_edit = None;
        self.delete_confirm_ids.clear();
    }

    /// Borrow the current zone row list (pre-search, pre-sort).
    pub fn zones(&self) -> &[ZoneRow] {
        &self.zones
    }

    /// Borrow the current search input.
    pub fn search(&self) -> &str {
        &self.search
    }

    /// Update the search input. Selection survives (the panel does not
    /// touch `selected` when the search filter changes).
    pub fn set_search(&mut self, value: impl Into<String>) {
        self.search = value.into().chars().take(RUNTIME_SEARCH_LIMIT).collect();
        self.clamp_cursor();
    }

    /// Whether WM_CHAR input currently targets the search filter.
    pub fn search_focused(&self) -> bool {
        self.search_focused
    }

    /// Focus the search filter and cancel any active typed metadata edit.
    pub fn focus_search(&mut self) {
        self.cancel_text_edit();
        self.search_focused = true;
    }

    /// Blur the search filter without changing the current filter text.
    pub fn blur_search(&mut self) {
        self.search_focused = false;
    }

    /// Append one user-typed character to the search filter.
    pub fn push_search_char(&mut self, ch: char) -> bool {
        if ch.is_control() || self.search.chars().count() >= RUNTIME_SEARCH_LIMIT {
            return false;
        }
        self.search.push(ch);
        self.clamp_cursor();
        true
    }

    /// Remove the last character from the search filter.
    pub fn backspace_search(&mut self) -> bool {
        let changed = self.search.pop().is_some();
        if changed {
            self.clamp_cursor();
        }
        changed
    }

    /// Clear the search filter.
    pub fn clear_search(&mut self) -> bool {
        if self.search.is_empty() {
            return false;
        }
        self.search.clear();
        self.clamp_cursor();
        true
    }

    /// Borrow the current sort key.
    pub fn sort_key(&self) -> SortKey {
        self.sort_key
    }

    /// Borrow the current sort direction.
    pub fn sort_direction(&self) -> SortDirection {
        self.sort_direction
    }

    /// Click on a column header. Same-key clicks toggle direction;
    /// different-key clicks snap direction back to ascending.
    pub fn set_sort_key(&mut self, key: SortKey) {
        if self.sort_key == key {
            self.sort_direction = self.sort_direction.flipped();
        } else {
            self.sort_key = key;
            self.sort_direction = SortDirection::Ascending;
        }
    }

    /// Borrow the current selection.
    pub fn selected(&self) -> &[ZoneId] {
        &self.selected
    }

    pub fn cursor_index(&self) -> usize {
        self.cursor_index
    }

    pub fn cursor_zone_id(&self) -> Option<ZoneId> {
        let visible = self.visible_rows();
        visible.get(self.cursor_index).map(|row| row.id)
    }

    pub fn set_cursor_index(&mut self, index: usize) {
        let visible_count = self.visible_count();
        self.cursor_index = if visible_count == 0 {
            0
        } else {
            index.min(visible_count - 1)
        };
    }

    pub fn select_next(&mut self) {
        let visible_count = self.visible_count();
        if visible_count == 0 {
            self.cursor_index = 0;
        } else {
            self.cursor_index = (self.cursor_index + 1) % visible_count;
        }
    }

    pub fn select_prev(&mut self) {
        let visible_count = self.visible_count();
        if visible_count == 0 {
            self.cursor_index = 0;
        } else if self.cursor_index == 0 {
            self.cursor_index = visible_count - 1;
        } else {
            self.cursor_index -= 1;
        }
    }

    /// Whether `id` is currently selected.
    pub fn is_selected(&self, id: ZoneId) -> bool {
        self.selected.contains(&id)
    }

    /// Toggle the membership of `id` in the selection set.
    pub fn toggle_selection(&mut self, id: ZoneId) {
        if let Some(idx) = self.selected.iter().position(|s| *s == id) {
            self.selected.remove(idx);
        } else {
            self.selected.push(id);
        }
        self.clear_delete_confirmation();
    }

    pub fn toggle_cursor_selection(&mut self) {
        if let Some(id) = self.cursor_zone_id() {
            self.toggle_selection(id);
        }
    }

    pub fn toggle_visible_row_selection(&mut self, index: usize) {
        let visible = self.visible_rows();
        if let Some(row) = visible.get(index) {
            self.cursor_index = index;
            self.toggle_selection(row.id);
        }
    }

    /// Add every visible row's id to the selection (visible = post-
    /// search filter). Idempotent: duplicates are skipped.
    pub fn select_all(&mut self) {
        let visible_ids: Vec<ZoneId> = self.visible_rows().iter().map(|r| r.id).collect();
        for id in visible_ids {
            if !self.selected.contains(&id) {
                self.selected.push(id);
            }
        }
        self.clear_delete_confirmation();
    }

    /// Remove every visible row's id from the selection. Off-screen
    /// (search-filtered) selections survive.
    pub fn deselect_all(&mut self) {
        let visible_ids: Vec<ZoneId> = self.visible_rows().iter().map(|r| r.id).collect();
        self.selected.retain(|id| !visible_ids.contains(id));
        self.clear_delete_confirmation();
    }

    /// Flip selection membership for every visible row's id.
    pub fn invert_selection(&mut self) {
        let visible_ids: Vec<ZoneId> = self.visible_rows().iter().map(|r| r.id).collect();
        for id in visible_ids {
            if let Some(idx) = self.selected.iter().position(|s| *s == id) {
                self.selected.remove(idx);
            } else {
                self.selected.push(id);
            }
        }
        self.clear_delete_confirmation();
    }

    /// Whether every visible row is currently selected (the header
    /// checkbox renders "deselect all" in that state).
    pub fn all_visible_selected(&self) -> bool {
        let visible = self.visible_rows();
        if visible.is_empty() {
            return false;
        }
        visible.iter().all(|r| self.is_selected(r.id))
    }

    /// Snapshot the visible row set: filter by search, then sort by the
    /// current key + direction. Returns owned `Vec<ZoneRow>` because the
    /// sort step needs an owned copy anyway; callers that only need
    /// length should use [`visible_count`] to skip the clone.
    ///
    /// [`visible_count`]: BulkManagerState::visible_count
    pub fn visible_rows(&self) -> Vec<ZoneRow> {
        let term = self.search.trim().to_lowercase();
        let mut rows: Vec<ZoneRow> = if term.is_empty() {
            self.zones.clone()
        } else {
            self.zones
                .iter()
                .filter(|r| r.display_name.to_lowercase().contains(&term))
                .cloned()
                .collect()
        };
        rows.sort_by(|a, b| {
            let cmp = match self.sort_key {
                SortKey::Name => a.display_name.cmp(&b.display_name),
                SortKey::Items => a.item_count.cmp(&b.item_count),
                SortKey::Accent => a.accent_hex.cmp(&b.accent_hex),
                SortKey::Size => a.area_percent().cmp(&b.area_percent()),
            };
            match self.sort_direction {
                SortDirection::Ascending => cmp,
                SortDirection::Descending => cmp.reverse(),
            }
        });
        rows
    }

    /// Number of rows that pass the search filter. Cheaper than
    /// [`visible_rows`] when callers only need the count.
    ///
    /// [`visible_rows`]: BulkManagerState::visible_rows
    pub fn visible_count(&self) -> usize {
        let term = self.search.trim().to_lowercase();
        if term.is_empty() {
            self.zones.len()
        } else {
            self.zones
                .iter()
                .filter(|r| r.display_name.to_lowercase().contains(&term))
                .count()
        }
    }

    fn clamp_cursor(&mut self) {
        let visible_count = self.visible_count();
        if visible_count == 0 {
            self.cursor_index = 0;
        } else if self.cursor_index >= visible_count {
            self.cursor_index = visible_count - 1;
        }
    }

    /// Whether any bulk action button should render enabled.
    pub fn can_act(&self) -> bool {
        !self.selected.is_empty()
    }

    /// Borrow the selected ids currently awaiting a destructive delete
    /// confirmation.
    pub fn delete_confirmation(&self) -> &[ZoneId] {
        &self.delete_confirm_ids
    }

    /// Clear any pending destructive delete confirmation.
    pub fn clear_delete_confirmation(&mut self) {
        self.delete_confirm_ids.clear();
    }

    fn delete_confirmation_matches_selection(&self) -> bool {
        !self.selected.is_empty()
            && self.selected.len() == self.delete_confirm_ids.len()
            && self
                .selected
                .iter()
                .all(|id| self.delete_confirm_ids.contains(id))
    }

    /// Two-step destructive delete guard. The first call records the current
    /// selected ids and returns `None`; a second call with the same selection
    /// returns the ids to delete and clears the pending confirmation.
    pub fn confirm_delete_or_arm(&mut self) -> Option<Vec<ZoneId>> {
        if !self.can_act() {
            self.clear_delete_confirmation();
            return None;
        }
        if self.delete_confirmation_matches_selection() {
            let ids = self.selected.to_vec();
            self.clear_delete_confirmation();
            Some(ids)
        } else {
            self.delete_confirm_ids.clear();
            self.delete_confirm_ids
                .extend(self.selected.iter().copied());
            None
        }
    }

    /// User clicked the Hide button.
    pub fn click_hide(&mut self) {
        if !self.can_act() {
            return;
        }
        self.clear_delete_confirmation();
        self.pending_action = Some(BulkManagerAction::Hide {
            ids: self.selected.to_vec(),
        });
    }

    /// User clicked the Show button.
    pub fn click_show(&mut self) {
        if !self.can_act() {
            return;
        }
        self.clear_delete_confirmation();
        self.pending_action = Some(BulkManagerAction::Show {
            ids: self.selected.to_vec(),
        });
    }

    /// User clicked the Delete button.
    pub fn click_delete(&mut self) {
        if !self.can_act() {
            return;
        }
        if let Some(ids) = self.confirm_delete_or_arm() {
            self.pending_action = Some(BulkManagerAction::Delete { ids });
        }
    }

    /// User clicked the Move… button. Shell collects the delta from a
    /// secondary input (1.x: separate dialog with x/y fields); the panel
    /// only records intent + the resolved delta.
    pub fn click_move(&mut self, delta: Point) {
        if !self.can_act() {
            return;
        }
        self.clear_delete_confirmation();
        self.pending_action = Some(BulkManagerAction::Move {
            ids: self.selected.to_vec(),
            delta,
        });
    }

    /// User clicked the close button / pressed Escape / clicked the
    /// scrim.
    pub fn click_close(&mut self) {
        self.pending_action = Some(BulkManagerAction::Close);
    }

    /// Drain the latest action — one-shot. Returns `None` until the
    /// user clicks something next.
    pub fn take_action(&mut self) -> Option<BulkManagerAction> {
        self.pending_action.take()
    }

    /// Whether an action is currently pending (diagnostics + UI gating).
    pub fn has_pending_action(&self) -> bool {
        self.pending_action.is_some()
    }
}

// -----------------------------------------------------------------------------
// build() — chrome subtree the shell mounts inside the host HWND.
// -----------------------------------------------------------------------------

/// Build the BulkManagerPanel chrome subtree. Returns the chrome
/// Container today; the header / toolbar / table / footer composition
/// attaches when widget-library ships the final Modal + Grid + List
/// primitives. Geometry is pinned per snap.md.
pub fn build() -> WidgetNode {
    let chrome = BulkManagerChrome::from_palette(theme::current().palette);
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Px(PANEL_WIDTH_PX),
        height: Length::Px(PANEL_HEIGHT_PX),
        padding: Edges::all(PANEL_PADDING_PX),
        background: chrome.panel_background,
        radius: chrome.panel_radius,
        ..ContainerNode::default()
    })
}

// -----------------------------------------------------------------------------
// Tests live in `tests.rs` sibling so this `mod.rs` stays under the §15
// 800-LOC budget; see that file for the full unit + smoke surface.
// -----------------------------------------------------------------------------
