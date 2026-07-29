//! Phase 2.1 Ruling A — persistence integration tests.
//!
//! These tests exercise the full save → load → save loop against a
//! tempfile-style path. Real shell loads happen inside `Renderer::render`
//! at first paint — we can't boot a D2D pipeline in CI, so the tests
//! drive `storage::*` directly using the same `AppState.zones_path` that
//! the renderer reads. End-to-end coverage of the wndproc still lives in
//! the binary — this layer ensures the contract `mark_dirty → drain →
//! write_zones_atomic → read_zones` round-trips faithfully.

#![forbid(unsafe_op_in_unsafe_fn)]

use std::borrow::Cow;
use std::path::PathBuf;

use bentodesk_app::AppState;
use bentodesk_platform::storage;
use bentodesk_zone::{Zone, ZoneId};

fn unique_tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("bentodesk-persist-{name}-{pid}-{nanos}.bin"));
    p
}

#[test]
fn dirty_flag_save_then_reload_preserves_zones() {
    let path = unique_tmp("rt");
    let _ = std::fs::remove_file(&path);

    // Stage 1 — fresh AppState mutates zones, marks dirty, writes.
    let mut app = AppState::new();
    app.zones_path = path.clone();
    app.zones.add(Zone::new(
        ZoneId(7),
        Cow::Borrowed("PersistMe"),
        100,
        200,
        300,
        150,
    ));
    app.mark_dirty();
    assert!(app.dirty.get(), "mark_dirty must flip the flag");

    // Mimic `consume_dispatcher`'s tail save block.
    let res = storage::write_zones_atomic(&app.zones_path, &app.zones);
    assert!(res.is_ok(), "write_zones_atomic failed: {:?}", res.err());
    app.dirty.set(false);
    assert!(!app.dirty.get(), "save block must clear the dirty flag");

    // Stage 2 — fresh process, fresh AppState, first-paint load.
    let mut app2 = AppState::new();
    app2.zones_path = path.clone();
    let read_res = storage::read_zones(&app2.zones_path);
    assert!(
        read_res.is_ok(),
        "read must succeed: {:?}",
        read_res.as_ref().err()
    );
    let loaded = match read_res {
        Ok(v) => v,
        Err(_) => {
            let _ = std::fs::remove_file(&path);
            return;
        }
    };
    app2.zones = loaded;
    assert_eq!(app2.zones.len(), 1);
    let z = match app2.zones.get(ZoneId(7)) {
        Some(z) => z,
        None => {
            let _ = std::fs::remove_file(&path);
            return;
        }
    };
    assert_eq!(z.title.as_ref(), "PersistMe");
    assert_eq!((z.x, z.y, z.w, z.h), (100, 200, 300, 150));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn missing_file_yields_empty_list_not_error() {
    let path = unique_tmp("missing");
    let _ = std::fs::remove_file(&path);
    let res = storage::read_zones(&path);
    assert!(res.is_ok(), "absent file must return Ok(empty)");
    let zl = res.unwrap_or_default();
    assert!(zl.is_empty());
}

#[test]
fn corrupt_file_quarantines_and_first_paint_recovers_to_empty() {
    let path = unique_tmp("corrupt");
    // Plant a deliberately-bad file (wrong magic).
    let plant = std::fs::write(&path, b"NOPE\x00\x00\x00\x00");
    assert!(plant.is_ok(), "plant corrupt failed: {:?}", plant.err());

    // First-paint flow: read fails → quarantine → list stays empty.
    let read = storage::read_zones(&path);
    assert!(read.is_err(), "corrupt file must be rejected");
    let q = storage::quarantine_corrupt(&path);
    assert!(q.is_ok(), "quarantine_corrupt failed: {:?}", q.err());

    // Original path no longer holds the corrupt file.
    assert!(
        !path.exists(),
        "quarantine must rename the original out of the way"
    );

    // Reading again on the now-missing path yields the empty-list path.
    let after = storage::read_zones(&path);
    assert!(after.is_ok());
    assert!(after.unwrap_or_default().is_empty());

    // Cleanup the quarantined sibling — whatever it got renamed to.
    if let Some(parent) = path.parent()
        && let Ok(entries) = std::fs::read_dir(parent)
    {
        for e in entries.flatten() {
            if let Some(name) = e.file_name().to_str()
                && name.contains(".corrupt-")
                && name.contains("bentodesk-persist-corrupt")
            {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}
