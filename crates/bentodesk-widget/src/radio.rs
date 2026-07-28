//! `Radio` — single-selection group control. Each `Radio` carries a
//! `group_id` (`SmolStr` — inline ≤22 bytes) and a `value_id` (`u32`); only
//! the radio whose `value_id` matches the group's current selection renders
//! the inner dot.
//!
//! Caller owns the group state — pass it through to [`Radio::set_group_value`]
//! when the selection changes (e.g. on the dispatcher's RadioSelected event).
//! This keeps the widget closed-data and avoids a global mutable singleton.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::{BorderRadius, Color, Edges, Length};
use bentodesk_theme as theme;
use bentodesk_tree::{AnimatedValue, Easing};
use smol_str::SmolStr;

pub const SELECT_DURATION_SECS: f32 = 0.140;
pub const DEFAULT_SIZE_PX: f32 = 16.0;

#[derive(Debug, Clone)]
pub struct Radio {
    pub group_id: SmolStr,
    pub value_id: u32,
    pub selected_value: u32,
    pub size: f32,
    pub disabled: bool,
    /// 0.0 = unselected dot hidden, 1.0 = dot at full size + opacity.
    pub dot_anim: AnimatedValue<f32>,
    /// Dispatcher event id pushed when this radio is clicked. Zero = drop.
    pub on_select_event: u32,
    pub ring: Color,
    pub ring_selected: Color,
    pub dot: Color,
    pub radius: BorderRadius,
}

impl Radio {
    pub fn new(group_id: impl Into<SmolStr>, value_id: u32, on_select_event: u32) -> Self {
        let p = theme::current().palette;
        Self {
            group_id: group_id.into(),
            value_id,
            selected_value: 0,
            size: DEFAULT_SIZE_PX,
            disabled: false,
            dot_anim: AnimatedValue::new(0.0),
            on_select_event,
            ring: p.border,
            ring_selected: p.accent,
            dot: p.accent,
            radius: BorderRadius::all(DEFAULT_SIZE_PX * 0.5),
        }
    }

    /// True when this radio's `value_id` matches the active selection.
    pub fn is_selected(&self) -> bool {
        self.selected_value == self.value_id
    }

    /// Update the group's currently selected value. Triggers the dot tween in
    /// the appropriate direction. No-op when `disabled`.
    pub fn set_group_value(&mut self, new_selected: u32) {
        if self.disabled {
            return;
        }
        let was_selected = self.is_selected();
        self.selected_value = new_selected;
        let now_selected = self.is_selected();
        if was_selected != now_selected {
            let target = if now_selected { 1.0 } else { 0.0 };
            self.dot_anim
                .animate_to(target, SELECT_DURATION_SECS, Easing::EaseOut);
        }
    }

    /// Click handler — selects this radio's value within the group, returns
    /// `true` when the selection actually changed.
    pub fn click(&mut self) -> bool {
        if self.disabled || self.is_selected() {
            return false;
        }
        self.set_group_value(self.value_id);
        true
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        self.dot_anim.tick(dt)
    }

    pub fn dot_progress(&self) -> f32 {
        self.dot_anim.current()
    }

    pub fn dot_radius_for_diameter(&self, diameter: f32) -> BorderRadius {
        BorderRadius::all(diameter * 0.5)
    }

    pub fn emit<F: FnMut(u32)>(&self, mut sink: F) -> bool {
        if self.on_select_event == 0 {
            return false;
        }
        sink(self.on_select_event);
        true
    }
}

impl LayoutSource for Radio {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Row,
            width: Length::Px(self.size),
            height: Length::Px(self.size),
            padding: Edges::ZERO,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_click_selects_and_starts_anim() {
        let mut r = Radio::new("theme", 1, 100);
        assert!(!r.is_selected());
        let changed = r.click();
        assert!(changed);
        assert!(r.is_selected());
        let _ = r.tick(SELECT_DURATION_SECS + 0.01);
        assert!((r.dot_progress() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn radio_click_when_already_selected_is_noop() {
        let mut r = Radio::new("theme", 1, 100);
        r.set_group_value(1);
        let changed = r.click();
        assert!(!changed);
    }

    #[test]
    fn radio_set_group_to_other_value_deselects_and_animates_dot_out() {
        let mut r = Radio::new("theme", 1, 100);
        r.set_group_value(1);
        let _ = r.tick(SELECT_DURATION_SECS + 0.01);
        assert!((r.dot_progress() - 1.0).abs() < 1e-3);
        r.set_group_value(2);
        assert!(!r.is_selected());
        let _ = r.tick(SELECT_DURATION_SECS + 0.01);
        assert!((r.dot_progress() - 0.0).abs() < 1e-3);
    }

    #[test]
    fn radio_disabled_click_is_noop() {
        let mut r = Radio::new("theme", 1, 100);
        r.disabled = true;
        let changed = r.click();
        assert!(!changed);
        assert!(!r.is_selected());
    }

    #[test]
    fn dot_radius_tracks_dot_diameter() {
        let r = Radio::new("theme", 1, 100);
        assert_eq!(r.dot_radius_for_diameter(12.0), BorderRadius::all(6.0));
    }
}
