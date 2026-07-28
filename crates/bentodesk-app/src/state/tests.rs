use std::borrow::Cow;

use bentodesk_style::{BorderRadius, Color};
use bentodesk_theme::{ThemeTokens, palette, radius, shadow, spacing, typo};

use super::*;

include!("tests/01_panel_header_button_hover_tracks_visible_changes_only.rs");
include!("tests/02_m6c_unknown_id_leaves_effect_none.rs");
include!("tests/03_tooltip_session_tracks_visible_payload_and_hide.rs");
