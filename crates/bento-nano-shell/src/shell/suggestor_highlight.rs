//! Native shell owner: `suggestor_highlight`.

use super::*;

pub(super) fn suggestor_manual_selection_status(app: &AppState) -> SmolStr {
    let suggestor = app.suggestor.borrow();
    let Some(entry) = suggestor.selected_entry() else {
        return SmolStr::new_static(context_menu_text(
            "尚未选择分组建议",
            "No suggestion selected for manual file selection",
        ));
    };
    let total = entry.total_path_count();
    if total == 0 {
        return SmolStr::new_static(context_menu_text(
            "当前建议没有匹配文件",
            "Selected suggestion has no matching files",
        ));
    }
    let focused = entry.focused_path_index();
    let file_name = entry
        .suggestion
        .matching_files
        .get(focused)
        .map(|path| smart_group_suggestor::path_basename(path))
        .unwrap_or("-");
    let marker = if entry.is_path_selected(focused) {
        "✓"
    } else {
        "○"
    };
    SmolStr::new(
        if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
            format!(
                "本次整理已选 {}/{}；当前文件 {}/{} {} {}",
                entry.selected_path_count(),
                total,
                focused.saturating_add(1),
                total,
                marker,
                file_name
            )
        } else {
            format!(
                "Manual apply: {}/{} checked; file {}/{} {} {}",
                entry.selected_path_count(),
                total,
                focused.saturating_add(1),
                total,
                marker,
                file_name
            )
        },
    )
}

pub(super) fn set_suggestor_manual_selection_status(app: &AppState) {
    let status = suggestor_manual_selection_status(app);
    app.suggestor_status.borrow_mut().replace(status);
}

pub(super) fn comparable_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for character in path.chars() {
        let normalized = if character == '\\' { '/' } else { character };
        for lowered in normalized.to_lowercase() {
            out.push(lowered);
        }
    }
    while out.ends_with('/') && out.len() > 1 {
        out.pop();
    }
    out
}

pub(super) fn item_path_matches(item: &ZoneItem, target_path: &str) -> bool {
    let target = comparable_path(target_path);
    let mut matches = comparable_path(item.path.as_ref()) == target;
    if !matches {
        matches = item
            .original_path
            .as_deref()
            .map(comparable_path)
            .is_some_and(|path| path == target);
    }
    if !matches {
        matches = item
            .hidden_path
            .as_deref()
            .map(comparable_path)
            .is_some_and(|path| path == target);
    }
    matches
}

pub(super) fn path_matches_visible_zone_item(app: &AppState, target_path: &str) -> bool {
    app.zones.iter().any(|zone| {
        zone.is_visible()
            && !zone.is_stacked_child()
            && zone
                .items
                .iter()
                .any(|item| item_path_matches(item, target_path))
    })
}

pub(super) fn icon_name_matches_path(icon_name: &str, target_path: &str) -> bool {
    let path = Path::new(target_path);
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if icon_name.eq_ignore_ascii_case(file_name) {
        return true;
    }
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| icon_name.eq_ignore_ascii_case(stem))
}

pub(super) fn load_desktop_icon_layout_for_highlight(
    root: &AppRoot,
) -> Option<bento_nano_backend::icon_positions::SavedIconLayout> {
    let state_dir = state_dir_for_root(root);
    match bento_nano_backend::icon_positions::load_from_file(&state_dir) {
        Ok(Some(layout)) => return Some(layout),
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                target: "bentodesk::highlight",
                error = %error,
                path = %state_dir.display(),
                "highlight: icon-position backup unavailable"
            );
        }
    }
    match bento_nano_backend::icon_positions::save_layout() {
        Ok(layout) => Some(layout),
        Err(error) => {
            tracing::warn!(
                target: "bentodesk::highlight",
                error = %error,
                "highlight: live desktop icon read unavailable"
            );
            None
        }
    }
}

pub(super) fn desktop_pulses_for_paths(
    root: &AppRoot,
    paths: &[String],
) -> smallvec::SmallVec<[HighlightPulse; 8]> {
    let unmatched_paths = {
        let app = root.app.borrow();
        paths
            .iter()
            .filter(|path| !path_matches_visible_zone_item(&app, path))
            .cloned()
            .collect::<smallvec::SmallVec<[String; 8]>>()
    };
    if unmatched_paths.is_empty() {
        return smallvec::SmallVec::new();
    }

    let Some(layout) = load_desktop_icon_layout_for_highlight(root) else {
        return smallvec::SmallVec::new();
    };
    let mut pulses = smallvec::SmallVec::<[HighlightPulse; 8]>::new();
    for icon in layout.icons {
        if unmatched_paths
            .iter()
            .any(|path| icon_name_matches_path(&icon.name, path))
        {
            pulses.push(HighlightPulse::new(icon.name, icon.x as f32, icon.y as f32));
        }
    }
    pulses
}

pub(super) fn highlight_targets_for_paths(
    app: &AppState,
    paths: &[String],
) -> smallvec::SmallVec<[HighlightRect; 8]> {
    let mut targets = smallvec::SmallVec::<[HighlightRect; 8]>::new();
    for zone in app.zones.iter() {
        if !zone.is_visible() || zone.is_stacked_child() {
            continue;
        }
        for item in &zone.items {
            if paths
                .iter()
                .any(|target_path| item_path_matches(item, target_path))
            {
                targets.push(highlight_overlay::item_target_rect(zone, item));
            }
        }
    }
    targets
}

pub(super) fn set_highlight_for_paths_with_duration(
    root: &AppRoot,
    paths: &[String],
    duration_ms: Option<u32>,
) -> usize {
    let targets = {
        let app = root.app.borrow();
        highlight_targets_for_paths(&app, paths)
    };
    let pulses = desktop_pulses_for_paths(root, paths);
    let count = targets.len().saturating_add(pulses.len());
    let app = root.app.borrow();
    if count == 0 {
        app.highlight_overlay.borrow_mut().clear();
    } else if let Some(duration) = duration_ms {
        app.highlight_overlay
            .borrow_mut()
            .set_targets_and_pulses_for(targets, pulses, duration);
    } else {
        app.highlight_overlay
            .borrow_mut()
            .set_targets_and_pulses(targets, pulses);
    }
    count
}

pub(super) fn set_highlight_for_paths(root: &AppRoot, paths: &[String]) -> usize {
    set_highlight_for_paths_with_duration(root, paths, None)
}

pub(super) fn set_highlight_for_suggestor_selection(root: &AppRoot) -> usize {
    let paths = {
        let app = root.app.borrow();
        app.suggestor
            .borrow()
            .selected_entry()
            .map(|entry| entry.selected_matching_files())
            .unwrap_or_default()
    };
    set_highlight_for_paths(root, &paths)
}

pub(super) fn handle_suggestor_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> LRESULT {
    match vk {
        VK_UP_KEY => {
            let app = root.app.borrow();
            app.suggestor.borrow_mut().select_prev();
            set_suggestor_manual_selection_status(&app);
            drop(app);
            let _highlighted = set_highlight_for_suggestor_selection(root);
            if let Some(main) = find_main_hwnd(root) {
                request_redraw(main);
            }
            request_redraw(hwnd);
            0
        }
        VK_DOWN_KEY => {
            let app = root.app.borrow();
            app.suggestor.borrow_mut().select_next();
            set_suggestor_manual_selection_status(&app);
            drop(app);
            let _highlighted = set_highlight_for_suggestor_selection(root);
            if let Some(main) = find_main_hwnd(root) {
                request_redraw(main);
            }
            request_redraw(hwnd);
            0
        }
        VK_LEFT_KEY => {
            let app = root.app.borrow();
            let _ = app.suggestor.borrow_mut().focus_prev_path();
            set_suggestor_manual_selection_status(&app);
            request_redraw(hwnd);
            0
        }
        VK_RIGHT_KEY => {
            let app = root.app.borrow();
            let _ = app.suggestor.borrow_mut().focus_next_path();
            set_suggestor_manual_selection_status(&app);
            request_redraw(hwnd);
            0
        }
        VK_SPACE_KEY => {
            let app = root.app.borrow();
            if app.suggestor.borrow_mut().toggle_focused_path() {
                set_suggestor_manual_selection_status(&app);
                drop(app);
                let _highlighted = set_highlight_for_suggestor_selection(root);
                if let Some(main) = find_main_hwnd(root) {
                    request_redraw(main);
                }
            } else {
                app.suggestor_status
                    .borrow_mut()
                    .replace(SmolStr::new_static(context_menu_text(
                        "当前没有可切换的文件",
                        "No file checkbox available to toggle",
                    )));
            }
            request_redraw(hwnd);
            0
        }
        VK_A_KEY => {
            let app = root.app.borrow();
            if app.suggestor.borrow_mut().select_all_for_selected() {
                set_suggestor_manual_selection_status(&app);
                drop(app);
                let _highlighted = set_highlight_for_suggestor_selection(root);
                if let Some(main) = find_main_hwnd(root) {
                    request_redraw(main);
                }
            }
            request_redraw(hwnd);
            0
        }
        VK_N_KEY => {
            let app = root.app.borrow();
            if app.suggestor.borrow_mut().select_none_for_selected() {
                set_suggestor_manual_selection_status(&app);
                app.highlight_overlay.borrow_mut().clear();
                drop(app);
                if let Some(main) = find_main_hwnd(root) {
                    request_redraw(main);
                }
            }
            request_redraw(hwnd);
            0
        }
        VK_ENTER => {
            let app = root.app.borrow();
            if app.suggestor.borrow_mut().apply_selected() {
                drop(app);
                drain_suggestor_action(root, hwnd);
            } else {
                app.suggestor_status
                    .borrow_mut()
                    .replace(SmolStr::new_static(context_menu_text(
                        "尚未选择要整理的文件",
                        "No checked files selected to apply; press Space or A",
                    )));
                request_redraw(hwnd);
            }
            0
        }
        VK_DELETE_KEY | VK_D_KEY => {
            let app = root.app.borrow();
            if app.suggestor.borrow_mut().dismiss_selected() {
                drop(app);
                drain_suggestor_action(root, hwnd);
            } else {
                app.suggestor_status
                    .borrow_mut()
                    .replace(SmolStr::new_static(context_menu_text(
                        "尚未选择要忽略的建议",
                        "No suggestion selected to dismiss",
                    )));
                request_redraw(hwnd);
            }
            0
        }
        VK_ESCAPE_KEY => {
            let app = root.app.borrow();
            app.suggestor.borrow_mut().close();
            app.highlight_overlay.borrow_mut().clear();
            drop(app);
            drain_suggestor_action(root, hwnd);
            0
        }
        _ => 0,
    }
}

pub(super) fn handle_suggestor_lbutton_up(root: &AppRoot, hwnd: HWND, x: f32, y: f32) -> bool {
    let hit = {
        let app = root.app.borrow();
        let visible_count = app.suggestor.borrow().visible_count();
        let visible_preview_count = app.suggestor.borrow().selected_preview_file_count();
        smart_group_suggestor::suggestor_hit_test(
            app.viewport,
            visible_count,
            visible_preview_count,
            x,
            y,
        )
    };
    let Some(hit) = hit else {
        return false;
    };
    match hit {
        SuggestorPointerHit::Apply(row_index) => {
            let app = root.app.borrow();
            if app.suggestor.borrow_mut().select_index(row_index) {
                let _ = app.suggestor.borrow_mut().apply_selected();
                drop(app);
                drain_suggestor_action(root, hwnd);
            }
        }
        SuggestorPointerHit::Dismiss(row_index) => {
            let app = root.app.borrow();
            if app.suggestor.borrow_mut().select_index(row_index) {
                let _ = app.suggestor.borrow_mut().dismiss_selected();
                drop(app);
                drain_suggestor_action(root, hwnd);
            }
        }
        SuggestorPointerHit::Row(row_index) => {
            let app = root.app.borrow();
            let mut suggestor = app.suggestor.borrow_mut();
            if suggestor.select_index(row_index) {
                if let Some(entry) = suggestor.selected_entry() {
                    let id = entry.id.clone();
                    suggestor.on_row_hover(id);
                }
                drop(suggestor);
                set_suggestor_manual_selection_status(&app);
                drop(app);
                let _highlighted = set_highlight_for_suggestor_selection(root);
                if let Some(main) = find_main_hwnd(root) {
                    request_redraw(main);
                }
            }
            request_redraw(hwnd);
        }
        SuggestorPointerHit::SelectAllFiles => {
            let app = root.app.borrow();
            if app.suggestor.borrow_mut().select_all_for_selected() {
                set_suggestor_manual_selection_status(&app);
                drop(app);
                let _highlighted = set_highlight_for_suggestor_selection(root);
                if let Some(main) = find_main_hwnd(root) {
                    request_redraw(main);
                }
            }
            request_redraw(hwnd);
        }
        SuggestorPointerHit::SelectNoFiles => {
            let app = root.app.borrow();
            if app.suggestor.borrow_mut().select_none_for_selected() {
                set_suggestor_manual_selection_status(&app);
                app.highlight_overlay.borrow_mut().clear();
                drop(app);
                if let Some(main) = find_main_hwnd(root) {
                    request_redraw(main);
                }
            }
            request_redraw(hwnd);
        }
        SuggestorPointerHit::TogglePreviewFile(preview_offset) => {
            let app = root.app.borrow();
            if app
                .suggestor
                .borrow_mut()
                .toggle_preview_file(preview_offset)
            {
                set_suggestor_manual_selection_status(&app);
                drop(app);
                let _highlighted = set_highlight_for_suggestor_selection(root);
                if let Some(main) = find_main_hwnd(root) {
                    request_redraw(main);
                }
            } else {
                app.suggestor_status
                    .borrow_mut()
                    .replace(SmolStr::new_static(context_menu_text(
                        "该文件已不在当前预览中",
                        "Preview checkbox is no longer available",
                    )));
            }
            request_redraw(hwnd);
        }
        SuggestorPointerHit::Close => {
            let app = root.app.borrow();
            app.suggestor.borrow_mut().close();
            app.highlight_overlay.borrow_mut().clear();
            drop(app);
            drain_suggestor_action(root, hwnd);
        }
    }
    true
}
