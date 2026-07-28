use super::*;

// -----------------------------------------------------------------------------
// WizardStep — five distinct steps. The wizard advances through them in a
// fixed order; the user can skip back via "Back" but can't jump arbitrarily.
// -----------------------------------------------------------------------------

/// Which step the wizard is currently on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WizardStep {
    /// Step 1 — pick the condition tree.
    #[default]
    Conditions,
    /// Step 2 — pick the action to run.
    Action,
    /// Step 3 — preview matches.
    Preview,
    /// Step 4 — name + enable + run mode.
    Name,
    /// Step 5 — review + save.
    Review,
}

impl WizardStep {
    /// One-based step index (1..=5) — surfaced in the indicator dot label.
    pub const fn index(self) -> u32 {
        match self {
            Self::Conditions => 1,
            Self::Action => 2,
            Self::Preview => 3,
            Self::Name => 4,
            Self::Review => 5,
        }
    }

    /// Total number of steps in the wizard — pinned by snap.md.
    pub const TOTAL: u32 = 5;

    /// Move to the next step. Saturates at [`Self::Review`].
    pub const fn next(self) -> Self {
        match self {
            Self::Conditions => Self::Action,
            Self::Action => Self::Preview,
            Self::Preview => Self::Name,
            Self::Name | Self::Review => Self::Review,
        }
    }

    /// Move to the previous step. Saturates at [`Self::Conditions`].
    pub const fn prev(self) -> Self {
        match self {
            Self::Conditions | Self::Action => Self::Conditions,
            Self::Preview => Self::Action,
            Self::Name => Self::Preview,
            Self::Review => Self::Name,
        }
    }

    /// Iteration order for the indicator dot row.
    pub const ALL: &'static [Self] = &[
        Self::Conditions,
        Self::Action,
        Self::Preview,
        Self::Name,
        Self::Review,
    ];
}

// -----------------------------------------------------------------------------
// RulesWizard descriptor — the visual chrome.
// -----------------------------------------------------------------------------

/// Modal-panel chrome for the RulesWizard. The host HWND is sized to the
/// panel; this descriptor describes what paints inside.
#[derive(Debug, Clone)]
pub struct RulesWizard {
    pub background: Color,
    pub border: Color,
    pub title_color: Color,
    pub border_radius: BorderRadius,
    pub padding: Edges,
    pub width: Length,
    pub height: Length,
}

impl RulesWizard {
    pub fn new() -> Self {
        let palette = theme::current().palette;
        Self {
            background: palette.surface,
            border: palette.border,
            title_color: palette.text,
            border_radius: BorderRadius::all(PANEL_CORNER_RADIUS),
            padding: Edges::ZERO,
            width: Length::Px(PANEL_WIDTH),
            height: Length::Px(PANEL_HEIGHT),
        }
    }
}

impl Default for RulesWizard {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutSource for RulesWizard {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            // Column: header → step indicator → body → footer.
            direction: Direction::Column,
            width: self.width,
            height: self.height,
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

// -----------------------------------------------------------------------------
// RulesWizardAction — drained by the shell once per frame.
// -----------------------------------------------------------------------------

/// Action emitted by the wizard. Drained via [`take_action`].
///
/// [`take_action`]: RulesWizardState::take_action
//
// `Rule` is ~150-200 bytes (id SmolStr + name String + ConditionGroup with
// inline Vec + actions Vec + RunMode + last_run + run_count). Boxed inside
// `Save` and `PreviewRequest` to keep `clippy::large_enum_variant` quiet —
// the action is dispatched once per click, so a single heap-alloc on the
// rare Save / PreviewRequest paths is acceptable.
#[derive(Debug, Clone, PartialEq)]
pub enum RulesWizardAction {
    /// User clicked Save on step 5. Carries a fully populated [`Rule`]
    /// (id stays empty for create — the shell stamps a UUID before
    /// calling `rules::upsert`).
    Save(Box<Rule>),
    /// User clicked Cancel / pressed Escape / clicked the scrim.
    Cancel,
    /// Step 3 — user clicked "Refresh" / wizard advanced to Preview. The
    /// shell calls `rules::executor::preview` (which scans the desktop FS,
    /// must run off-thread) and pushes the hits back via
    /// [`set_preview_hits`]. Carries a snapshot of the `Rule` built from
    /// the wizard's current state so the shell can hand a stable struct
    /// to the preview executor.
    ///
    /// [`set_preview_hits`]: RulesWizardState::set_preview_hits
    PreviewRequest(Box<Rule>),
}

impl fmt::Display for RulesWizardAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Save(rule) => write!(f, "Save({})", rule.name),
            Self::Cancel => f.write_str("Cancel"),
            Self::PreviewRequest(rule) => write!(f, "PreviewRequest({})", rule.name),
        }
    }
}

// -----------------------------------------------------------------------------
// RulesWizardState — wizard navigation + per-step drafts.
// -----------------------------------------------------------------------------

/// Wizard state — owns the in-progress rule, the current step, and the
/// per-step draft buffers. Mutated as the user clicks; drained per-frame
/// for the latest action.
#[derive(Debug)]
pub struct RulesWizardState {
    step: WizardStep,
    rule_id: SmolStr,
    name: String,
    enabled: bool,
    combine: CombineMode,
    conditions: Vec<ConditionDraft>,
    condition_cursor: usize,
    action: ActionDraft,
    run_mode: RunModeChoice,
    interval_minutes: u32,
    preview_hits: Vec<String>,
    preview_busy: bool,
    last_error: Option<SmolStr>,
    pending_action: Option<RulesWizardAction>,
}

impl Default for RulesWizardState {
    fn default() -> Self {
        Self::new()
    }
}

impl RulesWizardState {
    /// New state — defaults to step 1 with one empty condition row, the
    /// MoveToZone action, and OnDemand run mode. Matches 1.x `emptyRule()`.
    pub fn new() -> Self {
        Self {
            step: WizardStep::default(),
            rule_id: SmolStr::default(),
            name: String::new(),
            enabled: true,
            combine: CombineMode::default(),
            conditions: vec![ConditionDraft::new()],
            condition_cursor: 0,
            action: ActionDraft::new(),
            run_mode: RunModeChoice::default(),
            interval_minutes: INTERVAL_DEFAULT_MINUTES,
            preview_hits: Vec::new(),
            preview_busy: false,
            last_error: None,
            pending_action: None,
        }
    }

    /// Seed the wizard with an existing rule (Edit mode). Resets navigation
    /// to step 1 so the user can step through the existing values to
    /// confirm the edit.
    pub fn load_rule(&mut self, rule: Rule) {
        self.rule_id = rule.id;
        self.name = rule.name;
        self.enabled = rule.enabled;
        let (combine, draft_rows) = drafts::decompose_conditions(&rule.conditions);
        self.combine = combine;
        self.conditions = if draft_rows.is_empty() {
            vec![ConditionDraft::new()]
        } else {
            draft_rows
        };
        self.condition_cursor = 0;
        self.action = rule
            .actions
            .into_iter()
            .next()
            .map(drafts::action_to_draft)
            .unwrap_or_default();
        self.run_mode = match &rule.run_mode {
            RunMode::OnDemand => RunModeChoice::OnDemand,
            RunMode::OnFileChange => RunModeChoice::OnFileChange,
            RunMode::Interval { .. } => RunModeChoice::Interval,
        };
        self.interval_minutes = match rule.run_mode {
            RunMode::Interval { minutes } => {
                minutes.clamp(INTERVAL_MIN_MINUTES, INTERVAL_MAX_MINUTES)
            }
            _ => INTERVAL_DEFAULT_MINUTES,
        };
        self.step = WizardStep::default();
        self.preview_hits.clear();
        self.preview_busy = false;
        self.last_error = None;
        self.pending_action = None;
    }

    /// Borrow the current step.
    pub fn step(&self) -> WizardStep {
        self.step
    }

    /// Borrow the current name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Update the rule name. Truncates to [`NAME_MAX_LEN`] codepoints.
    pub fn set_name(&mut self, value: impl Into<String>) {
        let v = value.into();
        let truncated: String = v.chars().take(NAME_MAX_LEN).collect();
        self.name = truncated;
    }

    /// Borrow the current enabled flag.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Toggle the enabled flag.
    pub fn set_enabled(&mut self, value: bool) {
        self.enabled = value;
    }

    /// Borrow the current combine mode.
    pub fn combine(&self) -> CombineMode {
        self.combine
    }

    /// Switch the combine mode.
    pub fn set_combine(&mut self, value: CombineMode) {
        self.combine = value;
    }

    /// Borrow the condition drafts.
    pub fn conditions(&self) -> &[ConditionDraft] {
        &self.conditions
    }

    /// Current condition row targeted by typed input and predicate cycling.
    pub fn condition_cursor(&self) -> usize {
        self.condition_cursor
    }

    /// Select the condition row targeted by typed input and predicate cycling.
    pub fn set_condition_cursor(&mut self, idx: usize) {
        self.condition_cursor = if self.conditions.is_empty() {
            0
        } else {
            idx.min(self.conditions.len() - 1)
        };
    }

    /// Move the condition cursor forward, wrapping within existing rows.
    pub fn select_next_condition(&mut self) {
        if self.conditions.is_empty() {
            self.condition_cursor = 0;
        } else {
            self.condition_cursor = (self.condition_cursor + 1) % self.conditions.len();
        }
    }

    /// Update one condition row's kind. No-op for out-of-range indices.
    /// Clears the value if the new kind doesn't need one.
    pub fn set_condition_kind(&mut self, idx: usize, kind: PredicateKind) {
        if let Some(row) = self.conditions.get_mut(idx) {
            row.kind = kind;
            if !kind.needs_value() {
                row.value.clear();
            }
        }
    }

    /// Update one condition row's value. No-op for out-of-range indices.
    pub fn set_condition_value(&mut self, idx: usize, value: impl Into<String>) {
        if let Some(row) = self.conditions.get_mut(idx) {
            row.value = value.into();
        }
    }

    /// Append an empty condition row.
    pub fn add_condition(&mut self) {
        self.conditions.push(ConditionDraft::new());
        self.condition_cursor = self.conditions.len().saturating_sub(1);
    }

    /// Remove a condition row. The wizard always keeps at least one row to
    /// preserve the "empty all-group matches nothing" guard at the
    /// executor level — removing the last row is a no-op.
    pub fn remove_condition(&mut self, idx: usize) {
        if self.conditions.len() <= 1 {
            return;
        }
        if idx < self.conditions.len() {
            self.conditions.remove(idx);
            self.condition_cursor = self
                .condition_cursor
                .min(self.conditions.len().saturating_sub(1));
        }
    }

    /// Borrow the action draft.
    pub fn action(&self) -> &ActionDraft {
        &self.action
    }

    /// Switch the action kind. Clears the inline value when the new kind
    /// doesn't need one (DeleteToRecycleBin).
    pub fn set_action_kind(&mut self, kind: ActionKind) {
        self.action.kind = kind;
        if matches!(kind, ActionKind::DeleteToRecycleBin) {
            self.action.value.clear();
        }
    }

    /// Update the action's inline value.
    pub fn set_action_value(&mut self, value: impl Into<String>) {
        self.action.value = value.into();
    }

    /// Borrow the run mode choice.
    pub fn run_mode(&self) -> RunModeChoice {
        self.run_mode
    }

    /// Switch the run mode.
    pub fn set_run_mode(&mut self, value: RunModeChoice) {
        self.run_mode = value;
    }

    /// Borrow the interval minutes.
    pub fn interval_minutes(&self) -> u32 {
        self.interval_minutes
    }

    /// Update the interval minutes — clamped to
    /// [`INTERVAL_MIN_MINUTES`]..=[`INTERVAL_MAX_MINUTES`].
    pub fn set_interval_minutes(&mut self, value: u32) {
        self.interval_minutes = value.clamp(INTERVAL_MIN_MINUTES, INTERVAL_MAX_MINUTES);
    }

    /// Borrow the preview hits.
    pub fn preview_hits(&self) -> &[String] {
        &self.preview_hits
    }

    /// Replace the preview hits — called by the shell after the off-thread
    /// `rules::executor::preview` returns.
    pub fn set_preview_hits(&mut self, hits: Vec<String>) {
        self.preview_hits = hits;
        self.preview_busy = false;
    }

    /// Whether the preview list is being recomputed off-thread.
    pub fn preview_busy(&self) -> bool {
        self.preview_busy
    }

    /// Mark the preview as busy. Called by the wizard internally before it
    /// emits [`RulesWizardAction::PreviewRequest`].
    fn mark_preview_busy(&mut self) {
        self.preview_busy = true;
    }

    /// Borrow the last error message.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Set the last error — surfaced in the footer banner. Pass `None` to
    /// clear it.
    pub fn set_error(&mut self, msg: Option<SmolStr>) {
        self.last_error = msg;
    }

    /// Whether the user can advance from the current step. Maps to the
    /// "Next" button's `disabled` attribute.
    pub fn can_advance(&self) -> bool {
        match self.step {
            WizardStep::Conditions => self.conditions.iter().any(ConditionDraft::is_valid),
            WizardStep::Action => self.action.is_valid(),
            WizardStep::Preview => true,
            WizardStep::Name => !self.name.trim().is_empty(),
            WizardStep::Review => self.is_complete(),
        }
    }

    /// Whether the entire rule is complete enough to save.
    pub fn is_complete(&self) -> bool {
        !self.name.trim().is_empty()
            && self.action.is_valid()
            && self.conditions.iter().any(ConditionDraft::is_valid)
    }

    /// User clicked "Next". When transitioning into [`WizardStep::Preview`]
    /// also queues a [`RulesWizardAction::PreviewRequest`] so the shell
    /// can run the off-thread preview.
    pub fn click_next(&mut self) {
        if !self.can_advance() {
            return;
        }
        let next = self.step.next();
        if next == WizardStep::Preview {
            self.mark_preview_busy();
            self.preview_hits.clear();
            if let Some(rule) = self.build_rule() {
                self.pending_action = Some(RulesWizardAction::PreviewRequest(Box::new(rule)));
            }
        }
        self.step = next;
    }

    /// User clicked "Back". No-op on step 1.
    pub fn click_back(&mut self) {
        self.step = self.step.prev();
    }

    /// User clicked "Save" on step 5. No-op when [`is_complete`] is false.
    ///
    /// [`is_complete`]: RulesWizardState::is_complete
    pub fn click_save(&mut self) {
        if !self.is_complete() {
            return;
        }
        if let Some(rule) = self.build_rule() {
            self.pending_action = Some(RulesWizardAction::Save(Box::new(rule)));
        }
    }

    /// User clicked Cancel / pressed Escape / clicked the scrim.
    pub fn click_cancel(&mut self) {
        self.pending_action = Some(RulesWizardAction::Cancel);
    }

    /// Drain the latest action — one-shot.
    pub fn take_action(&mut self) -> Option<RulesWizardAction> {
        self.pending_action.take()
    }

    /// Build a `Rule` from the current state. Returns `None` when the
    /// drafts can't be promoted (action not valid; no valid conditions).
    fn build_rule(&self) -> Option<Rule> {
        let nodes: Vec<ConditionNode> = self
            .conditions
            .iter()
            .filter_map(|d| d.to_condition().map(ConditionNode::Leaf))
            .collect();
        if nodes.is_empty() {
            return None;
        }
        let conditions = match self.combine {
            CombineMode::All => ConditionGroup::All(nodes),
            CombineMode::Any => ConditionGroup::Any(nodes),
        };
        let action = self.action.to_action()?;
        let run_mode = match self.run_mode {
            RunModeChoice::OnDemand => RunMode::OnDemand,
            RunModeChoice::OnFileChange => RunMode::OnFileChange,
            RunModeChoice::Interval => RunMode::Interval {
                minutes: self.interval_minutes,
            },
        };
        Some(Rule {
            id: self.rule_id.clone(),
            name: self.name.trim().to_string(),
            enabled: self.enabled,
            conditions,
            actions: vec![action],
            run_mode,
            last_run: None,
            run_count: 0,
        })
    }
}

// -----------------------------------------------------------------------------
// build() — chrome subtree the shell mounts inside the host HWND.
// -----------------------------------------------------------------------------

/// Build the RulesWizard widget subtree. Returns the panel chrome Container
/// today; the header / step indicator / per-step body / footer button row
/// land when widget-library composition primitives ship (Input · Dropdown ·
/// Toggle · RadioGroup · List · Button — already in the widget enum).
pub fn build() -> WidgetNode {
    let chrome = RulesWizard::new();
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: chrome.width,
        height: chrome.height,
        padding: chrome.padding,
        background: chrome.background,
        radius: chrome.border_radius,
        ..ContainerNode::default()
    })
}
