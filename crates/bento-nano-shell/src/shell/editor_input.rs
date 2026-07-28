//! Native shell owner: `editor_input`.

use super::*;

pub(super) fn handle_zone_editor_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> LRESULT {
    match vk {
        VK_BACKSPACE => {
            let app = root.app.borrow();
            if let Some(session) = app.zone_editor.borrow_mut().as_mut() {
                let _ = session.draft_name.pop();
            }
            request_redraw(hwnd);
            0
        }
        VK_F2_KEY => {
            cycle_zone_editor_icon(root);
            request_redraw(hwnd);
            0
        }
        VK_F3_KEY => {
            cycle_zone_editor_accent(root);
            request_redraw(hwnd);
            0
        }
        VK_F4_KEY => {
            cycle_zone_editor_grid_columns(root);
            request_redraw(hwnd);
            0
        }
        VK_F5_KEY => {
            cycle_zone_editor_capsule(root);
            request_redraw(hwnd);
            0
        }
        VK_ENTER => {
            save_zone_editor(root);
            // SAFETY: hwnd is the live ZoneEditor HWND that received this key.
            unsafe { ShowWindow(hwnd, SW_HIDE) };
            if let Some(main) = find_main_hwnd(root) {
                request_redraw(main);
            }
            0
        }
        VK_ESCAPE_KEY => {
            root.app.borrow().zone_editor.borrow_mut().take();
            // SAFETY: hwnd is the live ZoneEditor HWND that received this key.
            unsafe { ShowWindow(hwnd, SW_HIDE) };
            0
        }
        _ => 0,
    }
}

pub(super) fn handle_zone_editor_lbutton_up(root: &AppRoot, hwnd: HWND, x: f32, y: f32) -> bool {
    let hit = {
        let app = root.app.borrow();
        zone_editor_geometry::zone_editor_hit_test(app.viewport, x, y)
    };
    let Some(hit) = hit else {
        return false;
    };
    log_static(
        format!(
            "zone-editor: lbutton_up x={x:.1} y={y:.1} viewport={:.1}x{:.1} hit={hit:?}\n",
            root.app.borrow().viewport.width,
            root.app.borrow().viewport.height,
        )
        .as_str(),
    );
    match hit {
        ZoneEditorHit::Close => {
            root.app.borrow().zone_editor.borrow_mut().take();
            hide_aux_and_redraw_main(root, hwnd);
            return true;
        }
        ZoneEditorHit::Name => {
            focus_window_for_keyboard(hwnd);
        }
        ZoneEditorHit::Icon => {
            if !queue_zone_editor_icon_picker(root) {
                return false;
            }
        }
        ZoneEditorHit::AccentClear => {
            let app = root.app.borrow();
            if let Some(session) = app.zone_editor.borrow_mut().as_mut() {
                session.draft_accent_color = None;
            }
        }
        ZoneEditorHit::AccentSwatch(index) => {
            let Some(accent) = ACCENT_PALETTE.get(index) else {
                return false;
            };
            let app = root.app.borrow();
            if let Some(session) = app.zone_editor.borrow_mut().as_mut() {
                session.draft_accent_color = Some(SmolStr::new_static(accent));
            }
        }
        ZoneEditorHit::AccentCustom => {
            let initial = {
                let app = root.app.borrow();
                app.zone_editor
                    .borrow()
                    .as_ref()
                    .and_then(|session| session.draft_accent_color.clone())
                    .unwrap_or_else(|| SmolStr::new_static("#3b82f6"))
            };
            if let Some(accent) = choose_native_accent_color(hwnd, initial.as_str()) {
                let app = root.app.borrow();
                if let Some(session) = app.zone_editor.borrow_mut().as_mut() {
                    session.draft_accent_color = Some(accent);
                }
            }
        }
        ZoneEditorHit::GridColumns(columns) => {
            let app = root.app.borrow();
            if let Some(session) = app.zone_editor.borrow_mut().as_mut() {
                session.draft_grid_columns = columns.clamp(GRID_COLUMNS_MIN, GRID_COLUMNS_MAX);
            }
        }
        ZoneEditorHit::CapsuleSize(size) => set_zone_editor_capsule_size(root, size),
        ZoneEditorHit::CapsuleShape(shape) => set_zone_editor_capsule_shape(root, shape),
        ZoneEditorHit::Save => {
            save_zone_editor(root);
            hide_aux_and_redraw_main(root, hwnd);
            return true;
        }
        ZoneEditorHit::Cancel => {
            root.app.borrow().zone_editor.borrow_mut().take();
            hide_aux_and_redraw_main(root, hwnd);
            return true;
        }
    }
    request_redraw(hwnd);
    true
}

pub(super) fn queue_zone_editor_icon_picker(root: &AppRoot) -> bool {
    let zone_id = {
        let app = root.app.borrow();
        app.zone_editor
            .borrow()
            .as_ref()
            .map(|session| session.zone_id)
    };
    let Some(zone_id) = zone_id else {
        return false;
    };
    root.dispatcher.push(Command::OpenIconPicker {
        zone_id: Some(zone_id),
    });
    true
}

pub(super) fn cycle_zone_editor_icon(root: &AppRoot) {
    let app = root.app.borrow();
    let mut editor = app.zone_editor.borrow_mut();
    let Some(session) = editor.as_mut() else {
        return;
    };
    session.draft_icon = next_icon_slug(session.draft_icon.as_str());
}

pub(super) fn cycle_zone_editor_accent(root: &AppRoot) {
    let app = root.app.borrow();
    let mut editor = app.zone_editor.borrow_mut();
    let Some(session) = editor.as_mut() else {
        return;
    };
    session.draft_accent_color = match session.draft_accent_color.as_deref() {
        None => Some(SmolStr::new_static(ACCENT_PALETTE[0])),
        Some(current) => match ACCENT_PALETTE.iter().position(|hex| *hex == current) {
            Some(idx) if idx + 1 < ACCENT_PALETTE.len() => {
                Some(SmolStr::new_static(ACCENT_PALETTE[idx + 1]))
            }
            Some(_) => None,
            None => Some(SmolStr::new_static(ACCENT_PALETTE[0])),
        },
    };
}

pub(super) fn cycle_zone_editor_grid_columns(root: &AppRoot) {
    let app = root.app.borrow();
    let mut editor = app.zone_editor.borrow_mut();
    let Some(session) = editor.as_mut() else {
        return;
    };
    session.draft_grid_columns = if session.draft_grid_columns >= GRID_COLUMNS_MAX {
        GRID_COLUMNS_MIN
    } else {
        session.draft_grid_columns + 1
    };
}

pub(super) fn cycle_zone_editor_capsule(root: &AppRoot) {
    let app = root.app.borrow();
    let mut editor = app.zone_editor.borrow_mut();
    let Some(session) = editor.as_mut() else {
        return;
    };
    let next_idx = ZONE_EDITOR_CAPSULE_PRESETS
        .iter()
        .position(|(size, shape)| {
            session.draft_capsule_size.as_str() == size.wire()
                && session.draft_capsule_shape.as_str() == shape.wire()
        })
        .map_or(0, |idx| (idx + 1) % ZONE_EDITOR_CAPSULE_PRESETS.len());
    let (size, shape) = ZONE_EDITOR_CAPSULE_PRESETS[next_idx];
    session.draft_capsule_size = SmolStr::new_static(size.wire());
    session.draft_capsule_shape = SmolStr::new_static(shape.wire());
}

pub(super) fn set_zone_editor_capsule_size(root: &AppRoot, size: CapsuleSizeChoice) {
    let app = root.app.borrow();
    if let Some(session) = app.zone_editor.borrow_mut().as_mut() {
        session.draft_capsule_size = SmolStr::new_static(size.wire());
    }
}

pub(super) fn set_zone_editor_capsule_shape(root: &AppRoot, shape: CapsuleShapeChoice) {
    let app = root.app.borrow();
    if let Some(session) = app.zone_editor.borrow_mut().as_mut() {
        session.draft_capsule_shape = SmolStr::new_static(shape.wire());
    }
}

pub(super) fn handle_zone_editor_char(root: &AppRoot, codepoint: u32) {
    if matches!(codepoint, VK_BACKSPACE | VK_ENTER | VK_ESCAPE_KEY) {
        return;
    }
    let Some(ch) = char::from_u32(codepoint) else {
        return;
    };
    if ch.is_control() {
        return;
    }
    let app = root.app.borrow();
    if let Some(session) = app.zone_editor.borrow_mut().as_mut() {
        if session.draft_name.chars().count() < NAME_MAX_LEN {
            session.draft_name.push(ch);
        }
    }
}

pub(super) fn save_zone_editor(root: &AppRoot) {
    let session = {
        let app = root.app.borrow();
        app.zone_editor.borrow_mut().take()
    };
    let Some(session) = session else {
        return;
    };
    let trimmed = session.draft_name.trim();
    if !trimmed.is_empty() {
        root.dispatcher
            .push(Command::RenameZone(session.zone_id, SmolStr::new(trimmed)));
    }
    root.dispatcher.push(Command::SetZoneAlias(
        session.zone_id,
        SmolStr::new_static(""),
    ));
    root.dispatcher.push(Command::SetZoneIcon(
        session.zone_id,
        normalize_icon_slug(session.draft_icon.as_str()),
    ));
    root.dispatcher.push(Command::SetZoneAccent(
        session.zone_id,
        session.draft_accent_color,
    ));
    root.dispatcher.push(Command::SetZoneGridColumns(
        session.zone_id,
        session.draft_grid_columns,
    ));
    root.dispatcher.push(Command::SetZoneCapsule(
        session.zone_id,
        session.draft_capsule_size,
        session.draft_capsule_shape,
    ));
}

pub(super) fn handle_item_file_rename_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> LRESULT {
    match vk {
        VK_BACKSPACE => {
            let app = root.app.borrow();
            if let Some(session) = app.item_file_rename.borrow_mut().as_mut() {
                let _ = session.draft_name.pop();
                session.status = None;
            }
            request_redraw(hwnd);
            0
        }
        VK_ENTER => {
            if save_item_file_rename(root) {
                hide_aux_and_redraw_main(root, hwnd);
            } else {
                request_redraw(hwnd);
            }
            0
        }
        VK_ESCAPE_KEY => {
            root.app.borrow().item_file_rename.borrow_mut().take();
            hide_aux_and_redraw_main(root, hwnd);
            0
        }
        _ => 0,
    }
}

pub(super) fn handle_item_file_rename_char(root: &AppRoot, codepoint: u32) {
    if matches!(codepoint, VK_BACKSPACE | VK_ENTER | VK_ESCAPE_KEY) {
        return;
    }
    let Some(ch) = char::from_u32(codepoint) else {
        return;
    };
    if ch.is_control() {
        return;
    }
    let app = root.app.borrow();
    if let Some(session) = app.item_file_rename.borrow_mut().as_mut() {
        if session.draft_name.chars().count() < 128 {
            session.draft_name.push(ch);
            session.status = None;
        }
    }
}

pub(super) fn save_item_file_rename(root: &AppRoot) -> bool {
    let command = {
        let app = root.app.borrow();
        let mut session = app.item_file_rename.borrow_mut();
        let Some(session) = session.as_mut() else {
            return false;
        };
        match normalized_rename_leaf(session.draft_name.as_str()) {
            Ok(name) => Command::RenameItemFile(
                session.zone_id,
                bento_nano_app::ItemId(session.item_id.0),
                SmolStr::new(name),
            ),
            Err(error) => {
                let status = SmolStr::new(
                    if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                        format!("无法重命名：{error}")
                    } else {
                        format!("Rename rejected: {error}")
                    },
                );
                session.status = Some(status.clone());
                app.item_operation_status.borrow_mut().replace(status);
                return false;
            }
        }
    };
    root.app.borrow().item_file_rename.borrow_mut().take();
    root.dispatcher.push(command);
    true
}

pub(super) fn handle_icon_picker_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> LRESULT {
    match vk {
        VK_F2_KEY | VK_RIGHT_KEY => {
            cycle_icon_picker_icon(root);
            request_redraw(hwnd);
            0
        }
        VK_ENTER => {
            save_icon_picker(root);
            hide_aux_and_redraw_main(root, hwnd);
            0
        }
        VK_ESCAPE_KEY => {
            root.app.borrow().icon_picker.borrow_mut().take();
            // SAFETY: hwnd is the live IconPicker HWND that received this key.
            unsafe { ShowWindow(hwnd, SW_HIDE) };
            0
        }
        _ => 0,
    }
}

pub(super) fn cycle_icon_picker_icon(root: &AppRoot) {
    let app = root.app.borrow();
    let mut picker = app.icon_picker.borrow_mut();
    let Some(session) = picker.as_mut() else {
        return;
    };
    session.selected_icon = next_icon_slug(session.selected_icon.as_str());
}

pub(super) fn save_icon_picker(root: &AppRoot) {
    let session = {
        let app = root.app.borrow();
        app.icon_picker.borrow_mut().take()
    };
    let Some(session) = session else {
        return;
    };
    let Some(zone_id) = session.zone_id else {
        let selected_icon = normalize_icon_slug(session.selected_icon.as_str());
        let ids = {
            let app = root.app.borrow();
            app.bulk_manager.borrow().selected().to_vec()
        };
        if ids.is_empty() {
            let app = root.app.borrow();
            app.bulk_manager_status
                .borrow_mut()
                .replace(SmolStr::new_static(
                    "No BulkManager zones selected for icon",
                ));
        } else {
            let updates = ids
                .iter()
                .map(|id| BulkZoneUpdate {
                    id: *id,
                    icon: Some(selected_icon.clone()),
                    ..BulkZoneUpdate::default()
                })
                .collect();
            root.dispatcher.push(Command::BulkUpdateZones(updates));
        }
        return;
    };
    let zone_exists = {
        let app = root.app.borrow();
        app.zones.get(zone_id).is_some()
    };
    if zone_exists {
        root.dispatcher.push(Command::SetZoneIcon(
            zone_id,
            normalize_icon_slug(session.selected_icon.as_str()),
        ));
    } else {
        tracing::warn!(
            target: "bentodesk::picker",
            ?zone_id,
            "IconPicker Enter ignored: target zone is gone"
        );
    }
}

pub(super) fn icon_picker_slug_for_hit(hit: IconPickerHit) -> Option<SmolStr> {
    match hit {
        IconPickerHit::Icon(index) => ALL_ICON_KINDS
            .get(index)
            .map(|kind| SmolStr::new_static(kind.as_str())),
        IconPickerHit::Close => None,
    }
}

pub(super) fn handle_icon_picker_lbutton_up(root: &AppRoot, hwnd: HWND, x: f32, y: f32) -> bool {
    let hit = {
        let app = root.app.borrow();
        picker_geometry::icon_picker_hit_test(app.viewport, x, y, ALL_ICON_KINDS.len())
    };
    let Some(hit) = hit else {
        return false;
    };
    if hit == IconPickerHit::Close {
        root.app.borrow().icon_picker.borrow_mut().take();
        hide_aux_and_redraw_main(root, hwnd);
        return true;
    }
    let Some(selected_icon) = icon_picker_slug_for_hit(hit) else {
        return false;
    };
    {
        let app = root.app.borrow();
        if let Some(session) = app.icon_picker.borrow_mut().as_mut() {
            session.selected_icon = selected_icon;
        }
    }
    save_icon_picker(root);
    hide_aux_and_redraw_main(root, hwnd);
    true
}

pub(super) fn next_icon_slug(current: &str) -> SmolStr {
    let current = IconKind::from_str_opt(current)
        .map(IconKind::as_str)
        .unwrap_or(current);
    let next_idx = ALL_ICON_KINDS
        .iter()
        .position(|kind| kind.as_str() == current)
        .map_or(0, |idx| (idx + 1) % ALL_ICON_KINDS.len());
    SmolStr::new_static(ALL_ICON_KINDS[next_idx].as_str())
}

pub(super) fn normalize_icon_slug(raw: &str) -> SmolStr {
    IconKind::from_str_opt(raw).map_or_else(
        || SmolStr::new_static(DEFAULT_ZONE_ICON),
        |kind| SmolStr::new_static(kind.as_str()),
    )
}

pub(super) fn handle_palette_picker_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> LRESULT {
    match vk {
        VK_F3_KEY | VK_RIGHT_KEY => {
            cycle_palette_picker_accent(root);
            request_redraw(hwnd);
            0
        }
        VK_ENTER => {
            save_palette_picker(root);
            hide_aux_and_redraw_main(root, hwnd);
            0
        }
        VK_ESCAPE_KEY => {
            root.app.borrow().palette_picker.borrow_mut().take();
            // SAFETY: hwnd is the live PalettePicker HWND that received this key.
            unsafe { ShowWindow(hwnd, SW_HIDE) };
            0
        }
        _ => 0,
    }
}

pub(super) fn cycle_palette_picker_accent(root: &AppRoot) {
    let app = root.app.borrow();
    let mut picker = app.palette_picker.borrow_mut();
    let Some(session) = picker.as_mut() else {
        return;
    };
    session.selected_accent = next_palette_accent(session.selected_accent.as_deref());
}

pub(super) fn save_palette_picker(root: &AppRoot) {
    let session = {
        let app = root.app.borrow();
        app.palette_picker.borrow_mut().take()
    };
    let Some(session) = session else {
        return;
    };
    match session.target {
        PaletteTarget::ZoneAccent(zone_id) => {
            let zone_exists = {
                let app = root.app.borrow();
                app.zones.get(zone_id).is_some()
            };
            if zone_exists {
                root.dispatcher
                    .push(Command::SetZoneAccent(zone_id, session.selected_accent));
            } else {
                tracing::warn!(
                    target: "bentodesk::picker",
                    ?zone_id,
                    "PalettePicker Enter ignored: target zone is gone"
                );
            }
        }
        PaletteTarget::ThemeBase => {
            root.dispatcher
                .push(Command::SetThemeBase(session.selected_accent));
        }
        PaletteTarget::BulkManagerSelectedAccent => {
            let ids = {
                let app = root.app.borrow();
                app.bulk_manager.borrow().selected().to_vec()
            };
            if ids.is_empty() {
                let app = root.app.borrow();
                app.bulk_manager_status
                    .borrow_mut()
                    .replace(SmolStr::new_static(
                        "No BulkManager zones selected for color",
                    ));
            } else {
                let accent = session.selected_accent.unwrap_or_default();
                let updates = ids
                    .iter()
                    .map(|id| BulkZoneUpdate {
                        id: *id,
                        accent_color: Some(accent.clone()),
                        ..BulkZoneUpdate::default()
                    })
                    .collect();
                root.dispatcher.push(Command::BulkUpdateZones(updates));
            }
        }
    }
}

pub(super) fn palette_picker_accent_for_hit(hit: PalettePickerHit) -> Option<Option<SmolStr>> {
    match hit {
        PalettePickerHit::Swatch(index) => palette_picker::swatch_table()
            .get(index)
            .map(|swatch| Some(swatch.hex.clone())),
        PalettePickerHit::Clear => Some(None),
    }
}

pub(super) fn handle_palette_picker_lbutton_up(root: &AppRoot, hwnd: HWND, x: f32, y: f32) -> bool {
    let swatch_count = palette_picker::swatch_table().len();
    let hit = {
        let app = root.app.borrow();
        picker_geometry::palette_picker_hit_test(app.viewport, x, y, swatch_count)
    };
    let Some(selected_accent) = hit.and_then(palette_picker_accent_for_hit) else {
        return false;
    };
    {
        let app = root.app.borrow();
        if let Some(session) = app.palette_picker.borrow_mut().as_mut() {
            session.selected_accent = selected_accent;
        }
    }
    save_palette_picker(root);
    hide_aux_and_redraw_main(root, hwnd);
    true
}

pub(super) fn hide_aux_and_redraw_main(root: &AppRoot, hwnd: HWND) {
    unsafe { ShowWindow(hwnd, SW_HIDE) };
    if let Some(main) = find_main_hwnd(root) {
        request_redraw(main);
    }
}
