//! Business surface — DebugOverlay (FPS / RSS / frame-time HUD).
//!
//! Visual spec: `debug_overlay.snap.md`. The retained-widget `build()`
//! stays available for tree reachability checks, while the selected-stack
//! renderer now paints the fixed-position HUD directly in D2D so the
//! overlay has a real runtime path before the generic fixed-anchor widget
//! family lands.
//!
//! Status: runtime-backed state + renderer body. `Command::ToggleDebugOverlay`
//! flips visibility, the shell can also restore it from persisted
//! `debug_overlay`, and the renderer feeds real FPS/RSS samples.

use bento_nano_layout::Direction;
use bento_nano_style::{BorderRadius, Color, Edges, Length, Rect, Shadow};
use bento_nano_theme::{
    PaletteTokens, RadiusTokens, ShadowTokens, SpacingTokens, radius, shadow, spacing,
};
use bento_nano_widget::{ContainerNode, WidgetNode};
use smallvec::SmallVec;

/// Default overlay geometry per snap.md.
pub const OVERLAY_WIDTH: f32 = 220.0;
pub const OVERLAY_HEIGHT: f32 = 96.0;
pub const EDGE_MARGIN: f32 = 12.0;

/// Rolling window for the FPS readout — last N frame samples averaged.
/// 60 matches the 1.x baseline: at 60 Hz refresh, this is exactly one
/// second of history, which is short enough to react to drops but long
/// enough to suppress per-frame jitter.
pub const SAMPLE_WINDOW_FRAMES: usize = 60;

/// RSS poll cadence — the value barely moves between frames, polling every
/// 500 ms keeps the cost off the render hot path.
pub const RSS_SAMPLE_INTERVAL_MS: u32 = 500;

/// D2D DebugOverlay chrome derived from the active theme palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebugOverlayChrome {
    /// Shadow descriptor behind the fixed diagnostics HUD.
    pub shadow: Shadow,
    /// Panel corner radius.
    pub panel_radius: BorderRadius,
    /// Panel background.
    pub panel: Color,
    /// Title text color.
    pub title: Color,
    /// Metric row text color.
    pub body: Color,
    /// Secondary metric row text color.
    pub muted: Color,
    /// Horizontal text inset inside the diagnostics panel.
    pub text_inset_x: f32,
    /// Title row top inset.
    pub title_top: f32,
    /// Title row height.
    pub title_height: f32,
    /// First metric row top inset.
    pub metric_first_top: f32,
    /// Vertical step between metric rows.
    pub metric_row_gap: f32,
    /// Metric row height.
    pub metric_row_height: f32,
}

impl DebugOverlayChrome {
    /// Build DebugOverlay chrome from explicit active palette tokens.
    pub fn from_palette(palette: PaletteTokens) -> Self {
        Self::from_tokens(palette, radius::DEFAULT, spacing::DEFAULT, shadow::DEFAULT)
    }

    /// Build DebugOverlay chrome from explicit active theme token groups.
    pub fn from_tokens(
        palette: PaletteTokens,
        radius: RadiusTokens,
        spacing: SpacingTokens,
        shadow: ShadowTokens,
    ) -> Self {
        Self {
            shadow: shadow.md,
            panel_radius: radius.xl,
            panel: with_alpha(palette.surface, 0.92),
            title: with_alpha(palette.text, 1.0),
            body: with_alpha(palette.text, 0.92),
            muted: with_alpha(palette.text_muted, 0.92),
            text_inset_x: spacing.xl - spacing.sm,
            title_top: spacing.md,
            title_height: spacing.xl + spacing.sm,
            metric_first_top: spacing.xxl + spacing.md,
            metric_row_gap: spacing.xl + spacing.sm,
            metric_row_height: spacing.xl + spacing.xs,
        }
    }
}

/// DebugOverlay runtime state. The renderer feeds frame timings via
/// `record_frame(us)`; the platform layer feeds RSS via
/// `record_rss_if_due(now_ms, rss_mb)`. The Text rows read directly off
/// the derived getters (`fps()`, `rss_mb()`, `last_frame_us()`).
#[derive(Debug, Default)]
pub struct DebugOverlayState {
    /// Rolling buffer of the last `SAMPLE_WINDOW_FRAMES` frame durations
    /// (microseconds). Inline `SmallVec` so steady-state polling stays
    /// allocation-free.
    /// Buffer is sized 64 (SmallVec inline-array size constraint — power-of-2),
    /// but `SAMPLE_WINDOW_FRAMES` defines the logical cap at 60.
    pub frame_times_us: SmallVec<[u32; 64]>,
    /// Last frame duration recorded — drives the "Frame" row directly.
    pub last_frame_us: u32,
    /// Most recently sampled resident-set size in MB.
    pub last_rss_mb: f32,
    /// Wall-clock ms when RSS was last sampled — gates the next sample.
    pub last_rss_at_ms: u32,
    /// Visibility — toggled by `Command::ToggleDebugOverlay`.
    pub visible: bool,
}

impl DebugOverlayState {
    /// Append a frame timing sample. Drops the oldest entry once the
    /// rolling window is full, so the buffer stays bounded.
    pub fn record_frame(&mut self, us: u32) {
        self.last_frame_us = us;
        if self.frame_times_us.len() == SAMPLE_WINDOW_FRAMES {
            // Cheap O(N) shift over a 60-element inline buffer — once per
            // frame, vastly cheaper than a circular buffer's complexity.
            self.frame_times_us.remove(0);
        }
        self.frame_times_us.push(us);
    }

    /// Sample RSS only if `RSS_SAMPLE_INTERVAL_MS` has elapsed since the
    /// last poll — avoids burning syscall budget every frame.
    pub fn record_rss_if_due(&mut self, now_ms: u32, rss_mb: f32) -> bool {
        if !self.rss_sample_due(now_ms) {
            return false;
        }
        self.last_rss_mb = rss_mb;
        self.last_rss_at_ms = now_ms;
        true
    }

    /// Whether the platform should poll RSS for this frame. Kept separate
    /// from [`Self::record_rss_if_due`] so the renderer avoids calling
    /// `GetProcessMemoryInfo` on every frame when the last sample is still
    /// fresh.
    pub fn rss_sample_due(&self, now_ms: u32) -> bool {
        let elapsed = now_ms.saturating_sub(self.last_rss_at_ms);
        elapsed >= RSS_SAMPLE_INTERVAL_MS || self.last_rss_at_ms == 0
    }

    /// Average FPS over the rolling window, rounded to nearest integer.
    /// Returns 0 when the buffer is empty (cold start, before first frame).
    pub fn fps(&self) -> u32 {
        if self.frame_times_us.is_empty() {
            return 0;
        }
        let sum: u64 = self.frame_times_us.iter().map(|&us| us as u64).sum();
        let avg_us = sum / self.frame_times_us.len() as u64;
        if avg_us == 0 {
            return 0;
        }
        (1_000_000_u64 / avg_us) as u32
    }

    /// Toggle the visible flag — driven by `Command::ToggleDebugOverlay`.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }
}

/// Build the DebugOverlay subtree. Returns the chrome Container today;
/// the 3 monospace Text rows attach when widget-library exposes the
/// fixed-position anchor + Text monospace variants.
pub fn build() -> WidgetNode {
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Px(OVERLAY_WIDTH),
        height: Length::Px(OVERLAY_HEIGHT),
        padding: Edges::all(8.0),
        ..ContainerNode::default()
    })
}

/// Compute the selected-stack filled shadow rect from a panel and token shadow.
pub fn panel_shadow_rect(panel: Rect, shadow: Shadow) -> Rect {
    let spread = shadow.blur.max(0.0);
    Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_layout::LayoutSource;

    #[test]
    fn build_returns_overlay_sized_container() {
        let node = build();
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Px(w) if (w - OVERLAY_WIDTH).abs() < 0.01));
        assert!(matches!(layout.height, Length::Px(h) if (h - OVERLAY_HEIGHT).abs() < 0.01));
    }

    #[test]
    fn fps_zero_with_empty_buffer() {
        let state = DebugOverlayState::default();
        assert_eq!(state.fps(), 0);
    }

    #[test]
    fn fps_steady_60hz_returns_60() {
        let mut state = DebugOverlayState::default();
        // 60 Hz = 16,667 µs per frame.
        for _ in 0..SAMPLE_WINDOW_FRAMES {
            state.record_frame(16_667);
        }
        let fps = state.fps();
        assert!((59..=61).contains(&fps), "expected ~60 fps, got {fps}");
    }

    #[test]
    fn buffer_caps_at_window_size() {
        let mut state = DebugOverlayState::default();
        for i in 0..(SAMPLE_WINDOW_FRAMES * 2) {
            state.record_frame(i as u32);
        }
        assert_eq!(state.frame_times_us.len(), SAMPLE_WINDOW_FRAMES);
        // Oldest entry should be (window_size) — confirms FIFO drop.
        let oldest = state.frame_times_us.first().copied().unwrap_or(0);
        assert_eq!(oldest, SAMPLE_WINDOW_FRAMES as u32);
    }

    #[test]
    fn rss_first_sample_always_records() {
        let mut state = DebugOverlayState::default();
        assert!(state.rss_sample_due(0));
        let recorded = state.record_rss_if_due(0, 42.0);
        assert!(recorded);
        assert!((state.last_rss_mb - 42.0).abs() < 0.01);
    }

    #[test]
    fn rss_skips_within_interval() {
        let mut state = DebugOverlayState::default();
        // Seed with a non-zero `last_rss_at_ms` so the cold-start
        // exception (last_rss_at_ms == 0) doesn't fire.
        state.record_rss_if_due(1, 10.0);
        assert!(!state.rss_sample_due(RSS_SAMPLE_INTERVAL_MS / 2));
        let recorded = state.record_rss_if_due(RSS_SAMPLE_INTERVAL_MS / 2, 99.0);
        assert!(!recorded);
        // Stale value preserved.
        assert!((state.last_rss_mb - 10.0).abs() < 0.01);
    }

    #[test]
    fn rss_records_after_interval() {
        let mut state = DebugOverlayState::default();
        state.record_rss_if_due(1, 10.0);
        assert!(state.rss_sample_due(1 + RSS_SAMPLE_INTERVAL_MS));
        let recorded = state.record_rss_if_due(1 + RSS_SAMPLE_INTERVAL_MS, 99.0);
        assert!(recorded);
        assert!((state.last_rss_mb - 99.0).abs() < 0.01);
    }

    #[test]
    fn toggle_flips_visibility() {
        let mut state = DebugOverlayState::default();
        assert!(!state.visible);
        state.toggle();
        assert!(state.visible);
        state.toggle();
        assert!(!state.visible);
    }

    #[test]
    fn debug_overlay_chrome_accepts_explicit_active_palette() {
        let palette = PaletteTokens {
            bg: Color::from_u8(0x01, 0x02, 0x03, 0xFF),
            surface: Color::from_u8(0x11, 0x12, 0x13, 0xFF),
            surface_alt: Color::from_u8(0x21, 0x22, 0x23, 0xFF),
            border: Color::from_u8(0x31, 0x32, 0x33, 0xFF),
            text: Color::from_u8(0x41, 0x42, 0x43, 0xFF),
            text_muted: Color::from_u8(0x51, 0x52, 0x53, 0xFF),
            accent: Color::from_u8(0x61, 0x62, 0x63, 0xFF),
            accent_hover: Color::from_u8(0x71, 0x72, 0x73, 0xFF),
            danger: Color::from_u8(0x81, 0x82, 0x83, 0xFF),
            success: Color::from_u8(0x91, 0x92, 0x93, 0xFF),
            warning: Color::from_u8(0xA1, 0xA2, 0xA3, 0xFF),
            info: Color::from_u8(0xB1, 0xB2, 0xB3, 0xFF),
            scrim: Color::from_u8(0xC1, 0xC2, 0xC3, 0xFF),
            hover_overlay: Color::from_u8(0xD1, 0xD2, 0xD3, 0xFF),
            active_overlay: Color::from_u8(0xE1, 0xE2, 0xE3, 0xFF),
            selection: Color::from_u8(0xF1, 0xF2, 0xF3, 0xFF),
        };

        let chrome = DebugOverlayChrome::from_palette(palette);

        assert_eq!(chrome.shadow, shadow::DEFAULT.md);
        assert_eq!(chrome.panel_radius, radius::DEFAULT.xl);
        assert_eq!(chrome.panel, with_alpha(palette.surface, 0.92));
        assert_eq!(chrome.title, with_alpha(palette.text, 1.0));
        assert_eq!(chrome.body, with_alpha(palette.text, 0.92));
        assert_eq!(chrome.muted, with_alpha(palette.text_muted, 0.92));
        assert_eq!(
            chrome.text_inset_x,
            spacing::DEFAULT.xl - spacing::DEFAULT.sm
        );
        assert_eq!(chrome.title_top, spacing::DEFAULT.md);
        assert_eq!(
            chrome.metric_first_top,
            spacing::DEFAULT.xxl + spacing::DEFAULT.md
        );
    }

    #[test]
    fn debug_overlay_chrome_accepts_explicit_radius_spacing_shadow_tokens() {
        let palette = PaletteTokens {
            bg: Color::from_u8(0x01, 0x02, 0x03, 0xFF),
            surface: Color::from_u8(0x11, 0x12, 0x13, 0xFF),
            surface_alt: Color::from_u8(0x21, 0x22, 0x23, 0xFF),
            border: Color::from_u8(0x31, 0x32, 0x33, 0xFF),
            text: Color::from_u8(0x41, 0x42, 0x43, 0xFF),
            text_muted: Color::from_u8(0x51, 0x52, 0x53, 0xFF),
            accent: Color::from_u8(0x61, 0x62, 0x63, 0xFF),
            accent_hover: Color::from_u8(0x71, 0x72, 0x73, 0xFF),
            danger: Color::from_u8(0x81, 0x82, 0x83, 0xFF),
            success: Color::from_u8(0x91, 0x92, 0x93, 0xFF),
            warning: Color::from_u8(0xA1, 0xA2, 0xA3, 0xFF),
            info: Color::from_u8(0xB1, 0xB2, 0xB3, 0xFF),
            scrim: Color::from_u8(0xC1, 0xC2, 0xC3, 0xFF),
            hover_overlay: Color::from_u8(0xD1, 0xD2, 0xD3, 0xFF),
            active_overlay: Color::from_u8(0xE1, 0xE2, 0xE3, 0xFF),
            selection: Color::from_u8(0xF1, 0xF2, 0xF3, 0xFF),
        };
        let radius = RadiusTokens {
            sm: BorderRadius::all(3.0),
            md: BorderRadius::all(7.0),
            lg: BorderRadius::all(11.0),
            xl: BorderRadius::all(17.0),
            full: BorderRadius::all(999.0),
        };
        let spacing = SpacingTokens {
            xs: 3.0,
            sm: 5.0,
            md: 9.0,
            lg: 13.0,
            xl: 19.0,
            xxl: 27.0,
        };
        let shadow = ShadowTokens {
            sm: Shadow {
                offset_x: 1.0,
                offset_y: 2.0,
                blur: 3.0,
                spread: 0.0,
                color: Color::from_u8(0x01, 0x01, 0x01, 0x20),
            },
            md: Shadow {
                offset_x: 4.0,
                offset_y: 5.0,
                blur: 6.0,
                spread: 0.0,
                color: Color::from_u8(0x02, 0x02, 0x02, 0x40),
            },
            lg: Shadow {
                offset_x: 7.0,
                offset_y: 8.0,
                blur: 9.0,
                spread: 0.0,
                color: Color::from_u8(0x03, 0x03, 0x03, 0x60),
            },
        };

        let chrome = DebugOverlayChrome::from_tokens(palette, radius, spacing, shadow);

        assert_eq!(chrome.panel_radius, radius.xl);
        assert_eq!(chrome.shadow, shadow.md);
        assert_eq!(chrome.text_inset_x, spacing.xl - spacing.sm);
        assert_eq!(chrome.title_top, spacing.md);
        assert_eq!(chrome.title_height, spacing.xl + spacing.sm);
        assert_eq!(chrome.metric_first_top, spacing.xxl + spacing.md);
        assert_eq!(chrome.metric_row_gap, spacing.xl + spacing.sm);
        assert_eq!(chrome.metric_row_height, spacing.xl + spacing.xs);
    }

    #[test]
    fn panel_shadow_rect_uses_token_shadow_geometry() {
        let panel = Rect {
            x: 100.0,
            y: 40.0,
            width: 220.0,
            height: 96.0,
        };
        let shadow = Shadow {
            offset_x: 3.0,
            offset_y: 7.0,
            blur: 11.0,
            spread: 0.0,
            color: Color::BLACK,
        };

        assert_eq!(
            panel_shadow_rect(panel, shadow),
            Rect {
                x: 92.0,
                y: 36.0,
                width: 242.0,
                height: 118.0,
            }
        );
    }
}
