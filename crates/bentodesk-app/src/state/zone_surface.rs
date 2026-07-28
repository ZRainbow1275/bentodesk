use super::*;

impl AppState {
    pub fn set_zone_display_mode(&self, mode: ZoneDisplayMode) -> bool {
        let changed = self.zone_display_mode.get() != mode;
        if !changed {
            return false;
        }
        self.zone_display_mode.set(mode);
        // Structural ownership belongs to the mode under which it was produced.
        // In particular, `selected_zone` is the Click-mode expansion latch. If it
        // survives Click -> Always -> Hover, Hover inherits an expanded panel
        // even with the pointer away and can no longer settle back to a capsule.
        // Clear every mode-owned latch together, then let the new mode's
        // steady-state predicate become authoritative immediately.
        self.selected_zone.set(None);
        let mut scheduler = self.hover_scheduler.get();
        scheduler.reset();
        self.hover_scheduler.set(scheduler);
        self.zone_pill_anim_zone.set(None);
        self.zone_pill_anim_progress.set(1.0);
        self.zone_pill_anim_expanding.set(false);
        self.zone_pill_anim_from_morph.set(0.0);
        true
    }

    /// Current scroll offset for `zone_id`. A different Zone never inherits
    /// the previous Zone's scroll position.
    pub fn zone_content_scroll_offset(&self, zone_id: ZoneId) -> f32 {
        self.zone_content_scroll
            .get()
            .filter(|(current, _)| *current == zone_id)
            .map(|(_, offset)| offset)
            .unwrap_or(0.0)
    }

    /// Set a finite, non-negative expanded-content scroll offset. Returning
    /// `true` means paint/hit geometry must be refreshed.
    pub fn set_zone_content_scroll(&self, zone_id: ZoneId, offset: f32) -> bool {
        let offset = if offset.is_finite() {
            offset.max(0.0)
        } else {
            0.0
        };
        let next = (offset > 0.0).then_some((zone_id, offset));
        let changed = self.zone_content_scroll.get() != next;
        self.zone_content_scroll.set(next);
        changed
    }

    pub fn reset_zone_content_scroll(&self) -> bool {
        self.zone_content_scroll.replace(None).is_some()
    }

    /// Current reveal fraction for the inline Zone search field. A manually
    /// seeded target without an animator entry is treated as settled-open so
    /// tests and restored state never produce an invisible active search.
    pub fn zone_search_animation_progress_at(&self, now_ms: u32) -> f32 {
        let Some(zone_id) = self.zone_search_target.get() else {
            return 0.0;
        };
        let animator = self.pill_animator.borrow();
        if animator.contains(zone_id, AnimChannel::InlineSearch) {
            animator.sample(zone_id, AnimChannel::InlineSearch, now_ms)
        } else if self.zone_search_closing.get() {
            0.0
        } else {
            1.0
        }
    }

    pub fn effective_zone_display_mode(&self, zone: &Zone) -> ZoneDisplayMode {
        zone.display_mode
            .as_deref()
            .and_then(ZoneDisplayMode::parse)
            .unwrap_or_else(|| self.zone_display_mode.get())
    }

    pub fn zone_body_visible_for_mode(&self, zone: &Zone) -> bool {
        // An active inline search is a transient, explicit interaction surface.
        // Keep its Zone expanded independently of Hover/Click/Always until the
        // field's reverse animation settles; otherwise leaving the capsule can
        // hide a still-focused input before its idle timeout.
        if self.zone_search_target.get() == Some(zone.id) {
            return true;
        }
        match self.effective_zone_display_mode(zone) {
            ZoneDisplayMode::Always => true,
            ZoneDisplayMode::Hover => self.hover_scheduler.get().expanded_zone() == Some(zone.id),
            ZoneDisplayMode::Click => self.selected_zone.get() == Some(zone.id),
        }
    }

    /// Shared morph gate. Both directions include raw progress 0.0: the first
    /// expand frame must paint the recorded pill/start shape rather than briefly
    /// falling through to the settled expanded renderer, and the first collapse
    /// frame must retain the complete panel before easing toward the pill.
    pub fn zone_pill_morph_in_flight(&self, zone: &Zone) -> bool {
        if zone.is_stack_anchor() || self.zone_pill_anim_zone.get() != Some(zone.id) {
            return false;
        }
        let progress = self.zone_pill_anim_progress.get();
        progress < 1.0
    }

    /// Z-order (2026-06-02) — whether `zone`'s SETTLED render surface is the
    /// expanded body (panel) rather than the collapsed pill. This is the exact
    /// `pill_body_visible` rule shared by the paint side (`Renderer::draw_zones`)
    /// and the hit sides (`effective_zone_hit_rect` / `effective_zone_chrome_rect`):
    ///
    /// - A stack anchor's body is visible only when it is explicitly SELECTED (a
    ///   focused member) — never on mere hover (hover shows the bloom).
    /// - A normal zone's body follows `zone_body_visible_for_mode`.
    /// - In BOTH cases a RESIZE (armable only on an already-expanded panel) forces
    ///   the body so the resize drag keeps the panel rect.
    /// - A zone drag always uses the collapsed capsule. Tauri collapses an
    ///   expanded panel before moving it and does not drag the large body rect.
    ///
    /// SSoT so paint, hit-rect, and z-layering can never drift.
    pub fn zone_pill_body_visible(&self, zone: &Zone) -> bool {
        let resize_id = self.zone_resize.get().map(|t| t.0);
        let is_dragged = self
            .zone_drag
            .get()
            .is_some_and(|(dragged, _, _)| dragged == zone.id);
        if is_dragged {
            return Some(zone.id) == resize_id;
        }
        if zone.is_stack_anchor() {
            self.selected_zone.get() == Some(zone.id) || Some(zone.id) == resize_id
        } else {
            self.zone_body_visible_for_mode(zone) || Some(zone.id) == resize_id
        }
    }

    /// Z-order (2026-06-02) — whether `zone` belongs to the TOP draw/hit layer.
    /// A zone is on top when its body is visible (settled expanded panel) OR a
    /// pill↔panel morph is in flight for it. The expanded/morphing zones form the
    /// top layer; all collapsed pills are the bottom layer. `draw_zones` paints
    /// the bottom layer first then the top layer (so a panel occludes any pill it
    /// overlaps); the hit/hover resolvers test the top layer first (so a point
    /// inside an expanded panel resolves to the panel, never a pill behind it).
    /// Stack anchors never run the morph, so for them this collapses to the
    /// body-visible rule. SSoT shared by paint and hit so the two can't drift.
    pub fn zone_on_top(&self, zone: &Zone) -> bool {
        if self.zone_pill_body_visible(zone) {
            return true;
        }
        // Morph in flight (pill ↔ panel). Anchors don't morph.
        self.zone_pill_morph_in_flight(zone)
    }

    pub fn show_tooltip_text(&self, text: SmolStr) -> bool {
        let mut active = self.active_tooltip.borrow_mut();
        match active.as_mut() {
            Some(session) if session.text == text => false,
            Some(session) => {
                session.text = text;
                true
            }
            None => {
                *active = Some(TooltipSession { text });
                true
            }
        }
    }

    pub fn hide_tooltip_text(&self) -> bool {
        self.active_tooltip.borrow_mut().take().is_some()
    }

    pub fn upsert_minibar(&self, zone_id: ZoneId, bar: MiniBar) {
        let mut minibars = self.minibars.borrow_mut();
        if let Some((_, current)) = minibars
            .iter_mut()
            .find(|(candidate_id, _)| *candidate_id == zone_id)
        {
            *current = bar;
            return;
        }
        if minibars.len() < MAX_MINIBARS {
            minibars.push((zone_id, bar));
        }
    }

    pub fn remove_minibar(&self, zone_id: ZoneId) -> bool {
        let mut minibars = self.minibars.borrow_mut();
        let before = minibars.len();
        minibars.retain(|(candidate_id, _)| *candidate_id != zone_id);
        minibars.len() != before
    }

    pub fn active_minibar(&self) -> Option<(ZoneId, MiniBar)> {
        self.minibars.borrow().first().cloned()
    }

    /// Allocate a fresh zone id (monotonic, never reuses `ZoneId::INVALID`).
    pub fn alloc_zone_id(&self) -> ZoneId {
        let id = self.next_zone_id.get();
        self.next_zone_id.set(id.wrapping_add(1).max(1));
        ZoneId(id)
    }

    /// Mount `node` as the root widget. Returns the previous root id (if any).
    pub fn mount_root(&mut self, node: WidgetNode) -> NodeId {
        let id = self.tree.create("root", node);
        let _ = self.tree.set_root(id);
        id
    }

    /// Append `child_node` as a child of `parent`. Returns the new id.
    pub fn add_child(
        &mut self,
        parent: NodeId,
        debug_name: impl Into<smol_str::SmolStr>,
        child_node: WidgetNode,
    ) -> Result<NodeId, TreeError> {
        let id = self.tree.create(debug_name, child_node);
        self.tree.append_child(parent, id)?;
        Ok(id)
    }
}
