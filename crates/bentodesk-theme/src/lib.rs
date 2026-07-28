#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! `bentodesk-theme` — design tokens + observer.
//!
//! Spec §3.2: 100% self-rolled. Spec §10: every token group is `Copy` and
//! every constant is built at compile time — no `String`, no `Box`, no
//! `lazy_static`. Themes are referenced by `&'static ThemeTokens` so the
//! observer never clones the aggregate.
//!
//! Spec §15 layer rule: theme depends on `style` (for `Color`/`Shadow`/
//! `BorderRadius`) and `tree` (for the `Signal` used by `observer`). It is
//! a leaf-tier crate from the perspective of `widget`/`zone`/`app`.

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod observer;
pub mod palette;
pub mod radius;
pub mod shadow;
pub mod spacing;
pub mod theme;
pub mod typo;

pub use observer::{
    Subscriber, SubscriberHandle, ThemeError, ThemeObserver, current, current_id, set_current,
    subscribe, unsubscribe,
};
pub use palette::PaletteTokens;
pub use radius::RadiusTokens;
pub use shadow::ShadowTokens;
pub use spacing::SpacingTokens;
pub use theme::{DARK_DEFAULT, LIGHT_DEFAULT, THEMES, ThemeId};
pub use typo::{FontSizes, FontWeights, LineHeights, TypoTokens};

/// Aggregate of every token group. The renderer reads `&'static ThemeTokens`
/// once per frame and indexes into the sub-groups; the struct itself never
/// participates in hot-path arithmetic.
///
/// Not `Copy` — `TypoTokens` carries a `SmolStr` font family. Per the §11
/// ruling, callers hand around `&'static ThemeTokens` references (see
/// `theme::DARK_DEFAULT` / `LIGHT_DEFAULT`), never clone the aggregate.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeTokens {
    pub palette: PaletteTokens,
    pub spacing: SpacingTokens,
    pub radius: RadiusTokens,
    pub shadow: ShadowTokens,
    pub typo: TypoTokens,
}
