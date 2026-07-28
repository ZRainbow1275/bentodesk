//! Unit + state-machine tests for `zone_pill_geometry` (split out of the
//! production module to honour the §15 800-line budget). `super::*` resolves
//! to the parent module so the private bezier solver + `reached` helper stay
//! reachable from these tests.

use super::*;
use crate::business::zen_capsule::CapsuleSize;
use bentodesk_zone::ZoneId;
use std::borrow::Cow;

fn fixture(x: i32, y: i32) -> Zone {
    Zone::new(ZoneId(1), Cow::Borrowed("Docs"), x, y, 160, 120)
}

/// M2② — fixture with an explicit per-zone capsule appearance so the
/// size/shape wiring in `pill_layout_for_zone` can be exercised.
fn fixture_appearance(size: &'static str, shape: &'static str) -> Zone {
    let mut z = fixture(0, 0);
    z.set_capsule_size(size);
    z.set_capsule_shape(shape);
    z
}

include!("tests/01_pill_layout_uses_tauri_capsule_radius.rs");
include!("tests/02_current_morph_rect_matches_shared_geometry_timeline_.rs");
