//! Business surface — About modal.
//!
//! Visual spec: `about.snap.md`. The selected-stack renderer now draws the
//! runtime About card directly through Direct2D; this module owns the shared
//! geometry constants, version/build rows, optional D2D container seed, and
//! hit-test contract used by the shell.
//!
//! Status: active runtime model. `Command::OpenAbout` toggles
//! `AppState::about_open`, shows the About HWND when available, and the
//! renderer paints the visible modal from this module's geometry.

use bento_nano_layout::Direction;
use bento_nano_style::{Edges, Length, Rect, Size};
use bento_nano_widget::{ContainerNode, WidgetNode};
use smol_str::SmolStr;

/// Refined borderless About window geometry (DIPs).
pub const WINDOW_WIDTH: f32 = 640.0;
pub const WINDOW_HEIGHT: f32 = 520.0;

/// Padding inside the richer About card.
pub const CONTENT_PADDING: f32 = 32.0;
/// Product identity remains primary; the author avatar is deliberately a
/// compact footer identity rather than the hero image.
pub const APP_ICON_SIZE: f32 = 76.0;
pub const AUTHOR_AVATAR_SIZE: f32 = 42.0;
/// Close button geometry shared by renderer and shell hit-test.
pub const CLOSE_BTN_W: f32 = 32.0;
pub const CLOSE_BTN_H: f32 = 32.0;
pub const PROJECT_BTN_H: f32 = 50.0;
pub const AUTHOR_BTN_H: f32 = 64.0;

pub const AUTHOR: &str = "方寒";
pub const GITHUB_HANDLE: &str = "@ZRainbow1275";
pub const GITHUB_URL: &str = "https://github.com/ZRainbow1275";
pub const PROJECT_URL: &str = "github.com/ZRainbow1275/bentodesk";
pub const PROJECT_URL_FULL: &str = "https://github.com/ZRainbow1275/bentodesk";
/// Cargo metadata and the repository LICENSE are authoritative. The old Tauri
/// translation string still says MIT, but the shipped project is AGPL.
pub const LICENSE_NAME: &str = "AGPL-3.0-or-later";
pub const LICENSE_SUMMARY_ZH: &str = "GNU Affero 通用公共许可证 v3.0 或更高版本";

/// Compiled-in app version, sourced from Cargo metadata. Stable across the
/// session so the About surface and the updater status banner agree on the
/// running version string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Optional build hash injected by the release pipeline through the
/// `BENTO_BUILD_HASH` env var. Falls back to `"dev"` for local builds so
/// the About row never reads "v0.0.1 ()".
pub const BUILD_HASH: &str = match option_env!("BENTO_BUILD_HASH") {
    Some(h) => h,
    None => "dev",
};

/// Format the version + build hash row exactly as the About card renders it.
/// Pulled into the port today so the snap.md "name + version row" text
/// matches at composition time — no re-derivation in the Text widget call.
pub fn format_version() -> SmolStr {
    SmolStr::new(format!("v{VERSION} ({BUILD_HASH})"))
}

/// Build the optional D2D About modal subtree. Runtime painting is currently
/// implemented directly by `Renderer::draw_about_panel`; this seed preserves
/// the shared geometry for any future retained-widget Modal path.
pub fn build() -> WidgetNode {
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Px(WINDOW_WIDTH),
        height: Length::Px(WINDOW_HEIGHT),
        padding: Edges::all(CONTENT_PADDING),
        ..ContainerNode::default()
    })
}

/// Hit-test result for the runtime About overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AboutHit {
    /// Close button hit.
    Close,
    /// Project repository link.
    Project,
    /// Author GitHub profile link.
    Author,
    /// Inside the card but outside the close button.
    Body,
    /// Outside the card; selected-stack parity closes the modal.
    Outside,
}

/// Compute the About panel rect centred in `viewport`.
pub fn panel_rect(viewport: Size) -> Rect {
    Rect {
        x: ((viewport.width - WINDOW_WIDTH) * 0.5).max(0.0),
        y: ((viewport.height - WINDOW_HEIGHT) * 0.5).max(0.0),
        width: WINDOW_WIDTH,
        height: WINDOW_HEIGHT,
    }
}

/// Compute the close button rect in absolute viewport coordinates.
pub fn close_button_rect(viewport: Size) -> Rect {
    let panel = panel_rect(viewport);
    Rect {
        x: panel.x + panel.width - CONTENT_PADDING - CLOSE_BTN_W,
        y: panel.y + CONTENT_PADDING,
        width: CLOSE_BTN_W,
        height: CLOSE_BTN_H,
    }
}

pub fn app_icon_rect(viewport: Size) -> Rect {
    let panel = panel_rect(viewport);
    Rect {
        x: panel.x + CONTENT_PADDING,
        y: panel.y + CONTENT_PADDING,
        width: APP_ICON_SIZE,
        height: APP_ICON_SIZE,
    }
}

pub fn project_button_rect(viewport: Size) -> Rect {
    let panel = panel_rect(viewport);
    Rect {
        x: panel.x + CONTENT_PADDING,
        y: panel.y + 330.0,
        width: panel.width - CONTENT_PADDING * 2.0,
        height: PROJECT_BTN_H,
    }
}

pub fn author_button_rect(viewport: Size) -> Rect {
    let panel = panel_rect(viewport);
    Rect {
        x: panel.x + CONTENT_PADDING,
        y: panel.y + 396.0,
        width: panel.width - CONTENT_PADDING * 2.0,
        height: AUTHOR_BTN_H,
    }
}

pub fn author_avatar_rect(viewport: Size) -> Rect {
    let author = author_button_rect(viewport);
    Rect {
        x: author.x + 11.0,
        y: author.y + (author.height - AUTHOR_AVATAR_SIZE) * 0.5,
        width: AUTHOR_AVATAR_SIZE,
        height: AUTHOR_AVATAR_SIZE,
    }
}

/// Runtime hit-test for About overlay interactions.
pub fn hit_test(viewport: Size, x: f32, y: f32) -> AboutHit {
    fn contains(rect: Rect, x: f32, y: f32) -> bool {
        x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
    }

    if contains(close_button_rect(viewport), x, y) {
        AboutHit::Close
    } else if contains(project_button_rect(viewport), x, y) {
        AboutHit::Project
    } else if contains(author_button_rect(viewport), x, y) {
        AboutHit::Author
    } else if contains(panel_rect(viewport), x, y) {
        AboutHit::Body
    } else {
        AboutHit::Outside
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_layout::LayoutSource;

    #[test]
    fn build_returns_about_window_sized_container() {
        let node = build();
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Px(w) if (w - WINDOW_WIDTH).abs() < 0.01));
        assert!(matches!(layout.height, Length::Px(h) if (h - WINDOW_HEIGHT).abs() < 0.01));
        assert_eq!(layout.direction, Direction::Column);
    }

    #[test]
    fn version_is_non_empty_and_starts_with_v() {
        let v = format_version();
        assert!(v.starts_with('v'), "version must start with 'v', got {v}");
        assert!(v.contains('('), "version must contain build hash in parens");
        assert!(v.ends_with(')'), "version must end with ')'");
    }

    #[test]
    fn build_hash_falls_back_to_dev_when_env_unset() {
        // When the env var isn't set at compile time, the const evaluates
        // to "dev". This pins the fallback so a refactor doesn't silently
        // ship empty parens to the About card.
        if option_env!("BENTO_BUILD_HASH").is_none() {
            assert_eq!(BUILD_HASH, "dev");
        }
    }

    #[test]
    fn padding_matches_snap_md() {
        // The richer profile layout uses 32 px all-around. Pin so a chrome refactor
        // doesn't silently shrink the visual breathing room.
        assert!((CONTENT_PADDING - 32.0).abs() < 0.01);
    }

    #[test]
    fn hit_test_closes_on_button_and_outside_only() {
        let viewport = Size {
            width: 800.0,
            height: 640.0,
        };
        let close = close_button_rect(viewport);
        assert_eq!(
            hit_test(viewport, close.x + 1.0, close.y + 1.0),
            AboutHit::Close
        );
        let project = project_button_rect(viewport);
        assert_eq!(
            hit_test(viewport, project.x + 1.0, project.y + 1.0),
            AboutHit::Project
        );
        let author = author_button_rect(viewport);
        assert_eq!(
            hit_test(viewport, author.x + 1.0, author.y + 1.0),
            AboutHit::Author
        );
        let panel = panel_rect(viewport);
        assert_eq!(
            hit_test(viewport, panel.x + 12.0, panel.y + 12.0),
            AboutHit::Body
        );
        assert_eq!(hit_test(viewport, 1.0, 1.0), AboutHit::Outside);
    }
}
