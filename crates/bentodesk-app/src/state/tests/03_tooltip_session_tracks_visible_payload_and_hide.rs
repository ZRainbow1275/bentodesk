#[test]
fn tooltip_session_tracks_visible_payload_and_hide() {
    let app = AppState::new();

    assert!(app.active_tooltip.borrow().is_none());
    assert!(app.show_tooltip_text(SmolStr::new_static("Open settings")));
    assert_eq!(
        app.active_tooltip
            .borrow()
            .as_ref()
            .map(|session| session.text.as_str()),
        Some("Open settings")
    );

    assert!(!app.show_tooltip_text(SmolStr::new_static("Open settings")));
    assert!(app.show_tooltip_text(SmolStr::new_static("Open vault")));
    assert_eq!(
        app.active_tooltip
            .borrow()
            .as_ref()
            .map(|session| session.text.as_str()),
        Some("Open vault")
    );

    assert!(app.hide_tooltip_text());
    assert!(app.active_tooltip.borrow().is_none());
    assert!(!app.hide_tooltip_text());
}

#[test]
fn minibar_sessions_upsert_replace_and_remove() {
    let app = AppState::new();
    let first = MiniBar::new("M0 0L1 1", "Docs", 8);
    let second = MiniBar::new("M0 0L1 1", "Projects", 8);

    app.upsert_minibar(ZoneId(8), first);
    assert_eq!(
        app.active_minibar()
            .as_ref()
            .map(|(_, bar)| bar.label.as_str()),
        Some("Docs")
    );

    app.upsert_minibar(ZoneId(8), second);
    assert_eq!(app.minibars.borrow().len(), 1);
    assert_eq!(
        app.active_minibar()
            .as_ref()
            .map(|(_, bar)| bar.label.as_str()),
        Some("Projects")
    );

    assert!(app.remove_minibar(ZoneId(8)));
    assert!(app.active_minibar().is_none());
    assert!(!app.remove_minibar(ZoneId(8)));
}
