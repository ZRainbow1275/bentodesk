#[test]
fn settings_hit_compact_accent_picker_resolves_after_scroll() {
    let app = app_with_zones(vec![]);
    let source_count = app.desktop_sources.borrow().len();
    let flags = bentodesk_app::settings_panel::SettingsBodyFlags::new(
        app.crash_restart_enabled.get(),
        app.safe_start_after_hibernation.get(),
        false,
        false,
        bentodesk_app::settings_panel::UpdaterHeightKind::StatusOnly,
    )
    .with_source_rows(source_count);
    let body = bentodesk_app::settings_panel::settings_body_rect(app.viewport);
    let reserve_delta =
        bentodesk_app::settings_panel::settings_sources_reserve_delta(source_count);
    let origin_unscrolled = bentodesk_app::settings_panel::settings_appearance_grid_origin(
        app.viewport,
        reserve_delta,
        &flags,
    );
    let layout_unscrolled = bentodesk_app::theme_picker::appearance_layout(
        origin_unscrolled,
        bentodesk_app::settings_panel::settings_appearance_inner_width(app.viewport),
    );
    let content_h =
        bentodesk_app::settings_panel::settings_body_content_height(app.viewport, &flags);
    let max_scroll =
        bentodesk_app::settings_panel::settings_body_max_scroll(content_h, app.viewport);
    let scroll_off = (layout_unscrolled.accent_picker.y - body.y)
        .max(0.0)
        .min(max_scroll);
    app.scroll_offset_y.set(scroll_off);

    let folded_scroll = scroll_off + reserve_delta;
    let origin = bentodesk_app::settings_panel::settings_appearance_grid_origin(
        app.viewport,
        folded_scroll,
        &flags,
    );
    let layout = bentodesk_app::theme_picker::appearance_layout(
        origin,
        bentodesk_app::settings_panel::settings_appearance_inner_width(app.viewport),
    );
    let picker = layout.accent_picker;
    let (px, py) = (
        picker.x + picker.width * 0.5,
        picker.y + picker.height * 0.5,
    );
    assert!(
        py >= body.y && py < body.bottom(),
        "accent picker centre must scroll into the visible body \
         (y={}, body=[{}, {}], source_count={}, reserve_delta={}, \
         max_scroll={}, scroll_off={}, folded_scroll={}, origin_y={})",
        py,
        body.y,
        body.bottom(),
        source_count,
        reserve_delta,
        max_scroll,
        scroll_off,
        folded_scroll,
        origin.y,
    );
    assert_eq!(
        settings_hit(&app, px, py),
        SettingsHit::OpenAccentColorPicker
    );
}
