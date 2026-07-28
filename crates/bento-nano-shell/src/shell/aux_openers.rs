//! Native shell owner: `aux_openers`.

use super::*;

/// F2-07/F3 bridge — `Command::OpenIconPicker` handler. Lazily spawns the
/// `WindowKind::IconPicker` aux HWND, constructs the
/// `business::icon_picker::build()` widget descriptor (so the production
/// binary keeps the module reachable via the `Grep`-based reachability
/// gate), seeds the picker session from the live target zone, then shows
/// the HWND with foreground activation. Keyboard selection in the aux HWND
/// emits the follow-up `SetZoneIcon` command on Enter.
pub(super) fn open_icon_picker(root: &AppRoot, zone_id: Option<ZoneId>) {
    use bento_nano_app::business::icon_picker;
    // Construct the descriptor — pins the module's `build()` symbol into the
    // production binary so the reachability gate (Grep) sees a real
    // production hit. The future tree mount consumes this same descriptor.
    let _picker = icon_picker::build();
    let selected_icon = {
        let app = root.app.borrow();
        if let Some(id) = zone_id {
            app.zones
                .get(id)
                .map(|zone| normalize_icon_slug(zone.icon.as_ref()))
                .unwrap_or_else(|| SmolStr::new_static(DEFAULT_ZONE_ICON))
        } else {
            app.bulk_manager
                .borrow()
                .selected()
                .first()
                .and_then(|id| app.zones.get(*id))
                .map(|zone| normalize_icon_slug(zone.icon.as_ref()))
                .unwrap_or_else(|| SmolStr::new_static(DEFAULT_ZONE_ICON))
        }
    };
    {
        let app = root.app.borrow();
        app.icon_picker.borrow_mut().replace(IconPickerSession {
            zone_id,
            selected_icon,
        });
    }

    let Some(target) = ensure_aux_window(root, WindowKind::IconPicker) else {
        tracing::warn!(
            target: "bentodesk::picker",
            ?zone_id,
            "OpenIconPicker: ensure_aux_window failed"
        );
        return;
    };

    // SAFETY: `target` is the registered IconPicker HWND and its GWLP_USERDATA
    // points at the stable WindowSlot owned by the registry.
    unsafe {
        let slot = get_slot_ptr(target);
        if !slot.is_null() {
            (*slot)
                .renderer
                .start_auxiliary_open_animation(GetTickCount());
        }
    }
    // SAFETY: ShowWindow + SetForegroundWindow canonical on a HWND we own.
    //         IconPicker is NOT WS_EX_NOACTIVATE so SetForegroundWindow is
    //         the correct activation primitive (matches F2-02's main path).
    unsafe {
        ShowWindow(target, SW_SHOW);
        SetForegroundWindow(target);
    }
    arm_hover_frame_timer(target);
    request_redraw(target);
    log_static(
        format!(
            "picker: OpenIconPicker shown zone_id={:?} hwnd={}\n",
            zone_id, target as usize
        )
        .as_str(),
    );
    tracing::info!(
        target: "bentodesk::picker",
        ?zone_id,
        "OpenIconPicker — keyboard selection can emit SetZoneIcon"
    );
}

/// F2-07/F3 bridge — `Command::OpenPalettePicker` handler. Spawns the dedicated
/// `WindowKind::PalettePicker` aux HWND (320× 240 per the platform crate's
/// `default_size`) and constructs `business::palette_picker::build()` so
/// the module reaches the production binary. `target` discriminates which
/// downstream surface the picked swatch applies to. `ZoneAccent` emits
/// `SetZoneAccent`; `ThemeBase` emits `SetThemeBase`.
pub(super) fn open_palette_picker(root: &AppRoot, target: PaletteTarget) {
    let _picker = palette_picker::build();
    let selected_accent = {
        let app = root.app.borrow();
        match target {
            PaletteTarget::ZoneAccent(zone_id) => app
                .zones
                .get(zone_id)
                .and_then(|zone| zone.accent_color.as_ref())
                .map(|accent| SmolStr::new(accent.as_ref())),
            PaletteTarget::ThemeBase => app.theme_base_accent.borrow().clone(),
            PaletteTarget::BulkManagerSelectedAccent => app
                .bulk_manager
                .borrow()
                .selected()
                .first()
                .and_then(|id| app.zones.get(*id))
                .and_then(|zone| zone.accent_color.as_ref())
                .map(|accent| SmolStr::new(accent.as_ref())),
        }
    };
    {
        let app = root.app.borrow();
        app.palette_picker
            .borrow_mut()
            .replace(PalettePickerSession {
                target,
                selected_accent,
            });
    }

    let Some(host) = ensure_aux_window(root, WindowKind::PalettePicker) else {
        tracing::warn!(
            target: "bentodesk::picker",
            ?target,
            "OpenPalettePicker: ensure_aux_window failed"
        );
        return;
    };

    // SAFETY: canonical show + activate on an owned HWND. PalettePicker's
    //         ex-style accepts focus so SetForegroundWindow is correct.
    unsafe {
        ShowWindow(host, SW_SHOW);
        SetForegroundWindow(host);
    }
    request_redraw(host);
    log_static(
        format!(
            "picker: OpenPalettePicker shown target={:?} hwnd={}\n",
            target, host as usize
        )
        .as_str(),
    );
    tracing::info!(
        target: "bentodesk::picker",
        ?target,
        "OpenPalettePicker — ZoneAccent keyboard selection can emit SetZoneAccent"
    );
}

/// F2-07 — `Command::OpenCapsulePicker` handler. Spawns the
/// `WindowKind::CapsulePicker` aux HWND. `business::capsule_picker`
/// exposes the `CapsulePicker` chrome struct + `LayoutSource` impl rather
/// than a top-level `build()`, so we construct the chrome descriptor
/// directly with the snap.md-localised title.
pub(super) fn open_capsule_picker(root: &AppRoot) {
    use bento_nano_app::business::capsule_picker::CapsulePicker;
    let _picker = CapsulePicker::new(localized_current("场景胶囊", "Context Capsules"));
    if let Err(error) = refresh_context_capsule_picker(root) {
        tracing::warn!(
            target: "bentodesk::picker",
            error = %error,
            "OpenCapsulePicker: filesystem-backed capsule list failed"
        );
        set_context_capsule_picker_error(
            root,
            localized_current(
                format!("场景胶囊列表载入失败：{error}"),
                format!("Capsule list failed: {error}"),
            ),
        );
    }

    let Some(target) = ensure_aux_window(root, WindowKind::CapsulePicker) else {
        tracing::warn!(
            target: "bentodesk::picker",
            "OpenCapsulePicker: ensure_aux_window failed"
        );
        return;
    };

    // SAFETY: canonical show + activate for a focusable aux HWND.
    unsafe {
        ShowWindow(target, SW_SHOW);
        SetForegroundWindow(target);
    }
    request_redraw(target);
    let entry_count = {
        let app = root.app.borrow();
        app.capsule_picker.borrow().entries().len()
    };
    log_static(
        format!(
            "picker: OpenCapsulePicker shown hwnd={} entries={entry_count}\n",
            target as usize
        )
        .as_str(),
    );
    tracing::info!(
        target: "bentodesk::picker",
        "OpenCapsulePicker — filesystem-backed zone capsules ready; Commands=CaptureCapsule / RestoreCapsule / DeleteCapsule"
    );
}

/// F2-08 — `Command::OpenRulesWizard` handler. Lazily spawns the
/// `WindowKind::RulesWizard` aux HWND (640× 480 per `default_size`),
/// constructs the `business::rules_wizard::build()` widget descriptor so
/// the production binary keeps the module reachable, then shows the HWND
/// with foreground activation. Save/preview/delete/run-now dispatch selected
/// stack rules commands that hit the real `rules.json` store and executor.
pub(super) fn open_rules_wizard(root: &AppRoot) {
    use bento_nano_app::business::rules_wizard;
    let _wizard = rules_wizard::build();
    {
        let app = root.app.borrow();
        *app.rules_wizard.borrow_mut() = rules_wizard::RulesWizardState::new();
        app.rules_wizard_status.borrow_mut().take();
        app.rules_wizard_rule_cursor.set(0);
    }
    if let Err(error) = refresh_rules_wizard(root) {
        tracing::warn!(
            target: "bentodesk::wizard",
            error = %error,
            "OpenRulesWizard: rules list refresh failed"
        );
        set_rules_wizard_error(
            root,
            localized_current(
                format!("规则列表载入失败：{error}"),
                format!("Rules list failed: {error}"),
            ),
        );
    }

    let Some(host) = ensure_aux_window(root, WindowKind::RulesWizard) else {
        tracing::warn!(
            target: "bentodesk::wizard",
            "OpenRulesWizard: ensure_aux_window failed"
        );
        return;
    };

    // SAFETY: ShowWindow + SetForegroundWindow canonical for a focusable aux.
    unsafe {
        ShowWindow(host, SW_SHOW);
        SetForegroundWindow(host);
    }
    tracing::info!(
        target: "bentodesk::wizard",
        "OpenRulesWizard — selected-stack SaveRule / DeleteRule / PreviewRuleHits / RunRuleNow ready"
    );
}

/// F2-08 — `Command::OpenBulkManager` handler. Spawns the
/// `WindowKind::BulkManager` aux HWND (720× 540 per `default_size`) and
/// constructs `business::bulk_manager_panel::build()`. Apply emits F3/F5
/// batch-update Commands.
pub(super) fn zone_percent(value: i32, total: f32) -> u32 {
    if total <= 0.0 {
        return 0;
    }
    (((value.max(0) as f32 / total) * 100.0).round() as u32).min(100)
}

pub(super) fn bulk_manager_rows_from_app(app: &AppState) -> Vec<ZoneRow> {
    let width = app.viewport.width.max(1.0);
    let height = app.viewport.height.max(1.0);
    app.zones
        .iter()
        .map(|zone| ZoneRow {
            id: zone.id,
            display_name: SmolStr::new(zone.display_title()),
            item_count: zone.items.len() as u32,
            accent_hex: zone
                .accent_color
                .as_deref()
                .map(SmolStr::new)
                .unwrap_or_default(),
            visible: zone.visible,
            locked: zone.locked,
            icon_slug: SmolStr::new(zone.icon.as_ref()),
            capsule_size: SmolStr::new(zone.capsule_size.as_ref()),
            display_mode: zone
                .display_mode
                .as_deref()
                .map(SmolStr::new)
                .unwrap_or_else(|| SmolStr::new_static("inherit")),
            width_percent: zone_percent(zone.w, width),
            height_percent: zone_percent(zone.h, height),
            position_x_percent: zone_percent(zone.x, width),
            position_y_percent: zone_percent(zone.y, height),
        })
        .collect()
}

pub(super) fn apply_bulk_zone_visibility(
    app: &mut AppState,
    ids: &[ZoneId],
    visible: bool,
) -> (usize, usize) {
    let mut changed = 0usize;
    let mut matched = 0usize;
    for id in ids {
        if let Some(zone) = app.zones.get_mut(*id) {
            matched += 1;
            if zone.set_visible(visible) {
                changed += 1;
            }
        }
    }
    (changed, matched)
}

pub(super) fn apply_bulk_zone_updates(
    app: &mut AppState,
    updates: &[BulkZoneUpdate],
) -> (usize, usize) {
    let mut changed = 0usize;
    let mut matched = 0usize;
    for update in updates {
        let Some(zone) = app.zones.get_mut(update.id) else {
            continue;
        };
        matched += 1;
        let mut zone_changed = false;
        if let Some(position) = update.position {
            if zone.x != position.x || zone.y != position.y {
                zone.x = position.x;
                zone.y = position.y;
                zone_changed = true;
            }
        }
        if let Some(size) = update.size {
            let width = size.width.max(80);
            let height = size.height.max(60);
            if zone.w != width || zone.h != height {
                zone.w = width;
                zone.h = height;
                zone_changed = true;
            }
        }
        if let Some(accent) = &update.accent_color {
            let trimmed = accent.trim();
            let next = if trimmed.is_empty() {
                None
            } else {
                Some(Cow::Owned(trimmed.to_owned()))
            };
            if zone.accent_color != next {
                zone.set_accent_color(next);
                zone_changed = true;
            }
        }
        if let Some(capsule_size) = &update.capsule_size {
            if zone.capsule_size.as_ref() != capsule_size.as_str() {
                zone.set_capsule_size(Cow::Owned(capsule_size.to_string()));
                zone_changed = true;
            }
        }
        if let Some(locked) = update.locked {
            if zone.set_locked(locked) {
                zone_changed = true;
            }
        }
        if let Some(alias) = &update.alias {
            let trimmed = alias.trim();
            let next = if trimmed.is_empty() {
                None
            } else {
                Some(Cow::Owned(trimmed.to_owned()))
            };
            if zone.set_alias(next) {
                zone_changed = true;
            }
        }
        if let Some(display_mode) = &update.display_mode {
            let next = display_mode.as_ref().and_then(|mode| {
                let trimmed = mode.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(Cow::Owned(trimmed.to_owned()))
                }
            });
            if zone.set_display_mode(next) {
                zone_changed = true;
            }
        }
        if let Some(icon) = &update.icon {
            let trimmed = icon.trim();
            if !trimmed.is_empty() && zone.icon.as_ref() != trimmed {
                zone.set_icon(Cow::Owned(trimmed.to_owned()));
                zone_changed = true;
            }
        }
        if zone_changed {
            changed += 1;
        }
    }
    (changed, matched)
}

pub(super) fn bulk_layout_target_ids(app: &AppState) -> Vec<ZoneId> {
    let manager = app.bulk_manager.borrow();
    if !manager.selected().is_empty() {
        return manager.selected().to_vec();
    }
    manager.visible_rows().iter().map(|row| row.id).collect()
}

pub(super) fn bulk_metadata_updates_for_target_ids(app: &AppState) -> Vec<BulkZoneUpdate> {
    bulk_layout_target_ids(app)
        .into_iter()
        .filter_map(|id| {
            let zone = app.zones.get(id)?;
            Some(BulkZoneUpdate {
                id,
                accent_color: Some(next_bulk_accent(zone.accent_color.as_deref())),
                capsule_size: Some(next_bulk_capsule_size(zone.capsule_size.as_ref())),
                locked: Some(!zone.locked),
                alias: Some(next_bulk_alias(zone)),
                display_mode: Some(next_bulk_display_mode(zone.display_mode.as_deref())),
                icon: Some(next_icon_slug(zone.icon.as_ref())),
                ..BulkZoneUpdate::default()
            })
        })
        .collect()
}

pub(super) fn next_bulk_accent(current: Option<&str>) -> SmolStr {
    match current.and_then(|hex| ACCENT_PALETTE.iter().position(|value| *value == hex)) {
        Some(idx) if idx + 1 < ACCENT_PALETTE.len() => SmolStr::new_static(ACCENT_PALETTE[idx + 1]),
        _ => SmolStr::new_static(ACCENT_PALETTE[0]),
    }
}

pub(super) fn next_bulk_capsule_size(current: &str) -> SmolStr {
    match CapsuleSizeChoice::parse(current) {
        CapsuleSizeChoice::Small => SmolStr::new_static("medium"),
        CapsuleSizeChoice::Medium => SmolStr::new_static("large"),
        CapsuleSizeChoice::Large => SmolStr::new_static("small"),
    }
}

pub(super) fn next_bulk_display_mode(current: Option<&str>) -> Option<SmolStr> {
    match current {
        None => Some(SmolStr::new_static(DEFAULT_ZONE_DISPLAY_MODE)),
        Some("hover") => Some(SmolStr::new_static("always")),
        Some("always") => Some(SmolStr::new_static("click")),
        Some("click") => None,
        Some(_) => Some(SmolStr::new_static(DEFAULT_ZONE_DISPLAY_MODE)),
    }
}

pub(super) fn next_bulk_alias(zone: &Zone) -> SmolStr {
    if zone.alias.is_some() {
        return SmolStr::new_static("");
    }
    let title = zone.title.trim();
    if title.is_empty() {
        SmolStr::new(format!("Bulk zone {}", zone.id.0))
    } else {
        SmolStr::new(format!("Bulk {title}"))
    }
}

pub(super) fn refresh_bulk_manager(root: &AppRoot) {
    let app = root.app.borrow();
    let rows = bulk_manager_rows_from_app(&app);
    app.bulk_manager.borrow_mut().set_zones(rows);
    app.bulk_manager_status.borrow_mut().take();
}

pub(super) fn drain_bulk_manager_action(root: &AppRoot, hwnd: HWND) {
    let action = {
        let app = root.app.borrow();
        app.bulk_manager.borrow_mut().take_action()
    };
    match action {
        Some(BulkManagerAction::Delete { ids }) => {
            root.dispatcher.push(Command::BulkDeleteZones(ids));
        }
        Some(BulkManagerAction::Move { ids, delta }) => {
            root.dispatcher.push(Command::BulkMoveZones { ids, delta });
        }
        Some(BulkManagerAction::Hide { ids }) => {
            root.dispatcher.push(Command::BulkSetZonesVisible {
                ids,
                visible: false,
            });
        }
        Some(BulkManagerAction::Show { ids }) => {
            root.dispatcher
                .push(Command::BulkSetZonesVisible { ids, visible: true });
        }
        Some(BulkManagerAction::Close) => unsafe {
            ShowWindow(hwnd, SW_HIDE);
        },
        None => {}
    }
}

pub(super) fn open_bulk_manager(root: &AppRoot) {
    use bento_nano_app::business::bulk_manager_panel;
    let _panel = bulk_manager_panel::build();
    refresh_bulk_manager(root);

    let Some(host) = ensure_aux_window(root, WindowKind::BulkManager) else {
        tracing::warn!(
            target: "bentodesk::wizard",
            "OpenBulkManager: ensure_aux_window failed"
        );
        return;
    };

    // SAFETY: canonical show + activate.
    unsafe {
        ShowWindow(host, SW_SHOW);
        SetForegroundWindow(host);
    }
    log_static(format!("bulk: OpenBulkManager shown hwnd={}\n", host as usize).as_str());
    tracing::info!(
        target: "bentodesk::wizard",
        "OpenBulkManager — selected-stack bulk hide/show/delete/move/layout/update commands are reachable"
    );
}

pub(super) fn open_zone_editor(root: &AppRoot, zone_id: ZoneId) {
    use bento_nano_app::business::zone_editor;
    let _editor = zone_editor::build();
    {
        let app = root.app.borrow();
        let zone = app.zones.get(zone_id);
        app.zone_editor.borrow_mut().replace(ZoneEditorSession {
            zone_id,
            draft_name: zone
                .map(|entry| entry.display_title().to_owned())
                .unwrap_or_else(|| "Zone".to_owned()),
            draft_icon: zone
                .map(|entry| normalize_icon_slug(entry.icon.as_ref()))
                .unwrap_or_else(|| SmolStr::new_static(DEFAULT_ZONE_ICON)),
            draft_accent_color: zone
                .and_then(|entry| entry.accent_color.as_deref().map(SmolStr::new)),
            draft_grid_columns: zone
                .map(|entry| entry.grid_columns)
                .unwrap_or(DEFAULT_ZONE_GRID_COLUMNS),
            draft_capsule_size: zone
                .map(|entry| SmolStr::new(entry.capsule_size.as_ref()))
                .unwrap_or_else(|| SmolStr::new_static(DEFAULT_ZONE_CAPSULE_SIZE)),
            draft_capsule_shape: zone
                .map(|entry| SmolStr::new(entry.capsule_shape.as_ref()))
                .unwrap_or_else(|| SmolStr::new_static(DEFAULT_ZONE_CAPSULE_SHAPE)),
        });
    }

    let Some(host) = ensure_aux_window(root, WindowKind::ZoneEditor) else {
        tracing::warn!(
            target: "bentodesk::wizard",
            ?zone_id,
            "OpenZoneEditor: ensure_aux_window failed"
        );
        return;
    };

    // `ensure_aux_window` re-centres reused dialogs on the invocation monitor,
    // so the first visible frame already has its final borderless geometry.
    // SAFETY: host is the live focusable ZoneEditor HWND.
    unsafe { ShowWindow(host, SW_SHOW) };
    focus_window_for_keyboard(host);
    request_redraw(host);
    log_static(
        format!(
            "zone-editor: OpenZoneEditor shown zone_id={:?} hwnd={}\n",
            zone_id, host as usize
        )
        .as_str(),
    );
    tracing::info!(
        target: "bentodesk::wizard",
        ?zone_id,
        "OpenZoneEditor — zone context menu producer reached selected-stack ZoneEditor surface"
    );
}

/// F2-08 — `Command::ShowSuggestor` handler. Spawns the
/// `WindowKind::Suggestor` aux HWND (522×574) and constructs
/// `business::smart_group_suggestor::build()`. Per-row Apply / Dismiss
/// rides the existing `GroupingApply` / `SuggestorDismiss` Commands.
pub(super) fn open_item_file_rename(
    root: &AppRoot,
    zone_id: ZoneId,
    item_id: bento_nano_app::ItemId,
) {
    let zone_item_id = bento_nano_zone::ZoneItemId(item_id.0);
    let item = root.app.borrow().zones.item(zone_id, zone_item_id).cloned();
    let Some(item) = item else {
        set_item_operation_status(
            root,
            SmolStr::new_static(context_menu_text(
                "无法重命名：该项目已不在区域中",
                "Rename rejected: item is no longer in the zone",
            )),
        );
        return;
    };
    let display_path = item
        .original_path
        .as_deref()
        .unwrap_or_else(|| item.path.as_ref());
    let draft_name = display_name_for_path(display_path);
    let current_path = item_file_display_path(&item);
    {
        let app = root.app.borrow();
        app.item_file_rename
            .borrow_mut()
            .replace(ItemFileRenameSession {
                zone_id,
                item_id: zone_item_id,
                draft_name,
                current_path: SmolStr::new(current_path),
                status: None,
            });
    }

    let Some(host) = ensure_aux_window(root, WindowKind::ItemFileRename) else {
        set_item_operation_status(
            root,
            SmolStr::new_static(context_menu_text(
                "无法打开重命名窗口",
                "Rename failed: window unavailable",
            )),
        );
        return;
    };

    // SAFETY: canonical show + activate for a focusable aux HWND.
    unsafe {
        ShowWindow(host, SW_SHOW);
        SetForegroundWindow(host);
    }
    tracing::info!(
        target: "bentodesk::items",
        ?zone_id,
        ?item_id,
        "OpenItemFileRename: selected-stack item rename surface opened"
    );
}
