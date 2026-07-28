//! `Modal` — full-window overlay surface with click-outside dismiss.
//!
//! Spec §3.2: like Popup, this is the placement / dismiss-routing layer; the
//! HWND lives in the platform layer (`WindowKind::Settings` for the settings
//! modal, sized to the parent monitor work area). The widget owns the scrim
//! colour, dismiss policy, and modal state machine.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::tokens as style_tokens;
use bentodesk_style::{BorderRadius, Color, Edges, Length, Rect, Shadow, Size};
use bentodesk_theme as theme;
use bentodesk_tree::{AnimatedValue, Easing};

pub const FADE_DURATION_SECS: f32 = 0.180;

/// Dismiss-on-click policy. `OutsideOnly` is the BentoDesk default — clicking
/// the scrim closes; clicking inside the body never closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModalDismiss {
    #[default]
    OutsideOnly,
    /// Clicks anywhere close — used for transient confirmations.
    Anywhere,
    /// Modal is sticky — only programmatic close works.
    None,
}

#[derive(Debug, Clone, Copy)]
pub struct Modal {
    /// Inner body size (logical pixels). The window is full-screen; the body
    /// is centred inside.
    pub body_size: Size,
    pub visible: bool,
    pub dismiss: ModalDismiss,
    /// 0 = scrim invisible, 1 = scrim fully tinted. Renderer multiplies the
    /// scrim alpha by this for the fade-in.
    pub fade_anim: AnimatedValue<f32>,
    pub scrim: Color,
    pub body_background: Color,
    pub body_border: Color,
    pub body_radius: BorderRadius,
    pub body_padding: Edges,
    pub body_shadow: Shadow,
    /// Dispatcher event id pushed when the modal closes (any cause: outside
    /// click, ESC, programmatic). Zero = drop.
    pub on_dismiss_event: u32,
}

impl Modal {
    pub fn new(body_size: Size, on_dismiss_event: u32) -> Self {
        let p = theme::current().palette;
        Self {
            body_size,
            visible: false,
            dismiss: ModalDismiss::OutsideOnly,
            fade_anim: AnimatedValue::new(0.0),
            scrim: p.scrim,
            body_background: p.surface,
            body_border: p.border,
            body_radius: BorderRadius::all(8.0),
            body_padding: Edges::all(16.0),
            // Wave B migration: `bentodesk_style::tokens::SHADOW.ink_modal`
            // (offset (0,8), blur 32, color 0x00000099). Byte-identical to the
            // pre-migration literal — `tokens::tests::shadow_ink_modal_*` guards parity.
            body_shadow: style_tokens::SHADOW.ink_modal,
            on_dismiss_event,
        }
    }

    /// Show the modal — starts fade-in.
    pub fn open(&mut self) {
        if self.visible {
            return;
        }
        self.visible = true;
        self.fade_anim
            .animate_to(1.0, FADE_DURATION_SECS, Easing::EaseOut);
    }

    /// Close the modal — starts fade-out. Call [`Self::tick`] until the fade
    /// completes; visibility flips to `false` then.
    pub fn close(&mut self) {
        if !self.visible {
            return;
        }
        self.fade_anim
            .animate_to(0.0, FADE_DURATION_SECS, Easing::EaseOut);
    }

    /// Compute body rect centred in `screen`. Returns `None` when the body is
    /// too large to fit; caller decides whether to clamp or shrink.
    pub fn body_rect(&self, screen: Size) -> Option<Rect> {
        if self.body_size.width > screen.width || self.body_size.height > screen.height {
            return None;
        }
        Some(Rect {
            x: (screen.width - self.body_size.width) * 0.5,
            y: (screen.height - self.body_size.height) * 0.5,
            width: self.body_size.width,
            height: self.body_size.height,
        })
    }

    /// Inspect a pointer-down at `point` against `body` to decide dismissal.
    /// Returns `true` when the click should close the modal.
    pub fn should_dismiss_on_click(&self, point: (f32, f32), body: Rect) -> bool {
        match self.dismiss {
            ModalDismiss::None => false,
            ModalDismiss::Anywhere => true,
            ModalDismiss::OutsideOnly => {
                let (x, y) = point;
                let inside = x >= body.x && x <= body.right() && y >= body.y && y <= body.bottom();
                !inside
            }
        }
    }

    /// Tick the fade animation. When the fade completes at 0.0 the modal
    /// flips `visible` to false. Returns `true` while in flight.
    pub fn tick(&mut self, dt: f32) -> bool {
        let active = self.fade_anim.tick(dt);
        if !active && (self.fade_anim.current() - 0.0).abs() < 1e-3 && self.visible {
            self.visible = false;
        }
        active
    }

    pub fn fade_progress(&self) -> f32 {
        self.fade_anim.current()
    }

    pub fn emit_dismiss<F: FnMut(u32)>(&self, mut sink: F) -> bool {
        if self.on_dismiss_event == 0 {
            return false;
        }
        sink(self.on_dismiss_event);
        true
    }
}

impl LayoutSource for Modal {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Column,
            width: Length::Px(self.body_size.width),
            height: Length::Px(self.body_size.height),
            padding: self.body_padding,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_open_starts_invisible_then_fades_in() {
        let mut m = Modal::new(
            Size {
                width: 400.0,
                height: 300.0,
            },
            0,
        );
        assert!(!m.visible);
        m.open();
        assert!(m.visible);
        let _ = m.tick(FADE_DURATION_SECS + 0.01);
        assert!((m.fade_progress() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn modal_close_starts_fade_then_invisible() {
        let mut m = Modal::new(
            Size {
                width: 400.0,
                height: 300.0,
            },
            0,
        );
        m.open();
        let _ = m.tick(FADE_DURATION_SECS + 0.01);
        m.close();
        let _ = m.tick(FADE_DURATION_SECS + 0.01);
        assert!(!m.visible);
        assert!((m.fade_progress() - 0.0).abs() < 1e-3);
    }

    #[test]
    fn modal_body_rect_centred_in_screen() {
        let m = Modal::new(
            Size {
                width: 400.0,
                height: 300.0,
            },
            0,
        );
        let r = m
            .body_rect(Size {
                width: 1000.0,
                height: 800.0,
            })
            .unwrap_or(Rect::ZERO);
        assert!((r.x - 300.0).abs() < 1e-3);
        assert!((r.y - 250.0).abs() < 1e-3);
    }

    #[test]
    fn modal_oversize_body_returns_none() {
        let m = Modal::new(
            Size {
                width: 1500.0,
                height: 300.0,
            },
            0,
        );
        assert!(
            m.body_rect(Size {
                width: 1000.0,
                height: 800.0
            })
            .is_none()
        );
    }

    #[test]
    fn modal_outside_only_dismisses_on_scrim_click() {
        let mut m = Modal::new(
            Size {
                width: 400.0,
                height: 300.0,
            },
            1,
        );
        m.open();
        let body = Rect {
            x: 300.0,
            y: 250.0,
            width: 400.0,
            height: 300.0,
        };
        assert!(m.should_dismiss_on_click((10.0, 10.0), body));
        assert!(!m.should_dismiss_on_click((400.0, 350.0), body));
    }

    #[test]
    fn modal_dismiss_none_never_dismisses() {
        let mut m = Modal::new(
            Size {
                width: 400.0,
                height: 300.0,
            },
            1,
        );
        m.dismiss = ModalDismiss::None;
        let body = Rect {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 300.0,
        };
        assert!(!m.should_dismiss_on_click((10.0, 10.0), body));
    }
}
