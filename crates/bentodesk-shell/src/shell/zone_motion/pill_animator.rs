use super::*;

/// V-8 per-frame tick of the pill hover/press animator. Drops fully-decayed
/// entries and returns `true` only while a sampled visual transition is still
/// in flight. `StatusDotPulse` helpers are dormant until a paint consumer
/// samples them, so they must not keep the main window repainting by themselves.
pub(crate) fn tick_pill_animator(app: &AppState, now_ms: u32) -> bool {
    let mut anim = app.pill_animator.borrow_mut();
    if animation_proof_log_enabled() {
        let occupancy = anim.occupancy();
        for zone in app.zones.iter() {
            if let Some(value) = anim.sample_if_present(zone.id, AnimChannel::PillMorph, now_ms) {
                log_static(
                    format!(
                        "pill_morph_tick: now_ms={now_ms} zone={} value={value:.3} active={} occupancy={occupancy}\n",
                        zone.id.0,
                        anim.is_active_entry(zone.id, AnimChannel::PillMorph, now_ms)
                    )
                    .as_str(),
                );
            }
        }
    }
    anim.tick(now_ms)
}
