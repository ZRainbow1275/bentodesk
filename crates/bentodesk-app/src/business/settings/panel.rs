//! SettingsPanel — modal host for the 5 settings cards + 6 inline sections.
//!
//! Visual spec: `settings_panel.snap.md` (480 px wide × max-height 80vh,
//! scale-in 200 ms cubic-bezier(0.16, 1, 0.3, 1), `rgba(0,0,0,0.5)` scrim).
//!
//! Runtime status: selected-stack complete. This file keeps the modal geometry
//! constants and legacy widget-tree compatibility hooks in sync with the D2D
//! renderer and shell hit-testing contract; the visible Settings panel is
//! painted directly from live `AppState`.
//!
//! Geometry constants below are exposed `pub` so the existing inline
//! `bentodesk-app::settings_panel` module's hit-tester can reference the
//! 2.0 modal's button rects without relying on the Phase 2.1 inline-overlay
//! constants (which describe a different geometry).
//!
//! ## Snap-drift correction (business-ui-2 ownership pass)
//!
//! The original snap.md and matching constants stated 720 × 620 px, 24 px
//! inner padding, 56 px header, 64 px footer. Cross-checking the 1.x source
//! `bentodesk/src/components/Settings/SettingsPanel.css` showed the actual
//! React baseline is **480 px wide, max-height 80vh (no fixed cap), 52 px
//! header**, and the body uses `padding: var(--spacing-xl) var(--spacing-xl)`
//! with no fixed footer height (footer is `flex-shrink: 0` driven by its
//! contents). Constants below now match the React baseline; if a spec drift
//! is intentional in the future it must update both the snap.md and these
//! constants in lock-step.

use bentodesk_widget::WidgetNode;

use super::default_panel_chrome;

// -----------------------------------------------------------------------------
// Snap.md derived geometry constants — pinned per visual spec for downstream
// hit-testing + animation timing. Values mirror `SettingsPanel.css` 1:1.
// -----------------------------------------------------------------------------

/// Modal width in DIPs — `.settings-panel { width: 480px }`.
pub const PANEL_WIDTH: f32 = 480.0;

/// Maximum modal height as a fraction of viewport — `max-height: 80vh`. The
/// React baseline does NOT impose a fixed pixel cap; height is purely
/// viewport-relative so very tall monitors get a taller modal.
pub const PANEL_MAX_HEIGHT_FRACTION: f32 = 0.80;

/// Header height — title + close-button row (`.settings-panel__header`).
pub const HEADER_HEIGHT: f32 = 52.0;

/// Body horizontal + vertical padding — derived from `var(--spacing-xl)`
/// which the BentoDesk 1.x design tokens resolve to 20 px. Pinned here so
/// the renderer doesn't need to look up the token at every paint.
pub const BODY_PADDING: f32 = 20.0;

/// Footer vertical padding — `var(--spacing-lg)`. Footer height itself is
/// `flex-shrink: 0` driven by the button row's intrinsic 36 px + this
/// padding × 2 = ~64 px observed; not a fixed value.
pub const FOOTER_VERTICAL_PADDING: f32 = 16.0;

/// Open animation duration — 200 ms `cubic-bezier(0.16, 1, 0.3, 1)`. The
/// curve will be supplied by the `bentodesk-animation::Curve` enum once
/// T-042 ships; pinning the duration today so the animation primitive
/// callsite (next pass) doesn't have to re-derive it.
pub const PANEL_OPEN_DURATION_MS: u32 = 200;

/// Section gap inside the body — `var(--spacing-2xl, 28px)` between adjacent
/// `.settings-group` blocks (snap.md "Section title" entry).
pub const SECTION_GAP: f32 = 28.0;

/// Build the SettingsPanel widget subtree used by legacy tree callers. The
/// runtime-visible panel is rendered by the D2D path; this retains the stable
/// geometry contract for hit-testing and mounts.
pub fn build() -> WidgetNode {
    WidgetNode::Container(default_panel_chrome())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bentodesk_layout::LayoutSource;
    use bentodesk_style::Length;

    #[test]
    fn build_returns_a_padded_column_container() {
        let node = build();
        let layout = node.layout();
        // The default panel chrome currently uses 24 px inner padding to
        // mirror the body's `padding: var(--spacing-xl)` rhythm. The exact
        // chrome value migrates to BODY_PADDING once the chrome composes
        // header + body + footer as separate children rather than a single
        // padded shell.
        assert!(layout.padding.left > 0.0);
        assert!(layout.padding.right > 0.0);
        // Width / height are Auto today (driven by the Modal host's
        // sizing); the snap.md PANEL_WIDTH constant locks the value the
        // shell composes against.
        assert!(matches!(layout.width, Length::Auto));
    }

    #[test]
    fn panel_constants_match_snap_spec() {
        // Pin every snap.md value to a const so a snap.md drift can be
        // grep-detected. The Wave H integrator-checker can read this
        // single test file to confirm spec alignment without reading the
        // whole module.
        assert_eq!(PANEL_WIDTH, 480.0);
        assert!((PANEL_MAX_HEIGHT_FRACTION - 0.80).abs() < f32::EPSILON);
        assert_eq!(HEADER_HEIGHT, 52.0);
        assert_eq!(BODY_PADDING, 20.0);
        assert_eq!(FOOTER_VERTICAL_PADDING, 16.0);
        assert_eq!(PANEL_OPEN_DURATION_MS, 200);
        assert_eq!(SECTION_GAP, 28.0);
    }

    #[test]
    fn panel_max_height_resolves_to_80_percent_of_viewport() {
        // `max-height: 80vh` semantics — pin the math so a future change to
        // PANEL_MAX_HEIGHT_FRACTION is caught by this test.
        let viewport_height = 1000.0_f32;
        let resolved = viewport_height * PANEL_MAX_HEIGHT_FRACTION;
        assert!((resolved - 800.0).abs() < 0.01);
    }
}
