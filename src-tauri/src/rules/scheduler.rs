//! Periodic rule scheduler — ticks every 60s and fires due rules.

use std::time::Duration;

use chrono::Utc;
use tauri::AppHandle;
use tokio::time::interval;

use super::executor;

/// Start the background scheduler. Idempotent — spawns exactly once.
pub fn spawn(handle: AppHandle) {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(60));
        loop {
            ticker.tick().await;
            run_due_rules(&handle);
        }
    });
}

/// Inspect every rule, running the ones whose interval has elapsed.
fn run_due_rules(handle: &AppHandle) {
    let rules = super::load_all(handle);
    let now = Utc::now();
    for rule in rules.iter().filter(|r| r.enabled) {
        if !executor::should_run_now(rule, now) {
            continue;
        }
        match executor::execute(handle, rule) {
            Ok(report) => {
                if report.errors.is_empty() {
                    tracing::info!(
                        "rule '{}' matched {} files, actions: {:?}",
                        rule.name,
                        report.matched,
                        report.actions_taken
                    );
                } else {
                    tracing::warn!(
                        "rule '{}' matched {} files but produced errors: {:?}",
                        rule.name,
                        report.matched,
                        report.errors
                    );
                }
            }
            Err(e) => tracing::warn!("rule '{}' execution failed: {e}", rule.name),
        }
    }
}
