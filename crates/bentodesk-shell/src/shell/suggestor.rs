//! Native shell owner: `suggestor`.

use super::*;

pub(super) fn scan_suggestor_desktop_files(
    root: &AppRoot,
) -> (
    Vec<bentodesk_backend::grouping::scanner::FileInfo>,
    usize,
    usize,
) {
    let desktop_dirs = configured_desktop_sources_for_app(&root.app.borrow());
    let source_count = desktop_dirs.len();
    let mut files = Vec::new();
    let mut error_count = 0usize;
    for dir in &desktop_dirs {
        match bentodesk_backend::grouping::scan_desktop_files(dir) {
            Ok(mut scanned) => files.append(&mut scanned),
            Err(error) => {
                error_count = error_count.saturating_add(1);
                tracing::warn!(
                    target: "bentodesk::suggestor",
                    path = %dir.display(),
                    error = %error,
                    "ShowSuggestor: Desktop scan failed for source"
                );
            }
        }
    }
    (files, source_count, error_count)
}

pub(super) fn seed_suggestor_from_files(
    root: &AppRoot,
    files: &[bentodesk_backend::grouping::scanner::FileInfo],
    source_count: usize,
    error_count: usize,
) -> usize {
    let suggestions = bentodesk_backend::grouping::suggest_groups(files);
    let app = root.app.borrow();
    let dismissed = app.suggestor_dismissed.borrow();
    let filtered = suggestions
        .into_iter()
        .filter(|suggestion| {
            let id = smart_group_suggestor::suggestion_id(suggestion);
            !dismissed.contains(&id)
        })
        .collect::<Vec<_>>();
    let visible_count = filtered.len();
    drop(dismissed);
    app.suggestor.borrow_mut().set_suggestions(filtered);
    let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);
    let status = if source_count == 0 {
        SmolStr::new_static(if zh {
            "尚未配置可用于智能分组的桌面源"
        } else {
            "No Desktop sources resolved for smart grouping"
        })
    } else if files.is_empty() {
        SmolStr::new(if zh {
            format!("已扫描 {source_count} 个桌面源，暂未找到可分组文件")
        } else {
            format!("Scanned {source_count} Desktop source(s); no groupable files found")
        })
    } else if visible_count == 0 {
        SmolStr::new(if zh {
            format!("已扫描 {} 个文件，暂未生成分组建议", files.len())
        } else {
            format!(
                "Scanned {} files; backend returned no visible suggestions",
                files.len()
            )
        })
    } else {
        SmolStr::new(if zh {
            if error_count == 0 {
                format!(
                    "已从 {} 个桌面源的 {} 个文件生成 {} 条建议",
                    source_count,
                    files.len(),
                    visible_count
                )
            } else {
                format!(
                    "已生成 {} 条建议；另有 {} 个桌面源扫描失败",
                    visible_count, error_count
                )
            }
        } else {
            format!(
                "{} suggestion(s) from {} files across {} source(s); scan errors={}",
                visible_count,
                files.len(),
                source_count,
                error_count
            )
        })
    };
    app.suggestor_status.borrow_mut().replace(status);
    visible_count
}

pub(super) fn drain_suggestor_action(root: &AppRoot, hwnd: HWND) {
    let action = {
        let app = root.app.borrow();
        app.suggestor.borrow_mut().take_action()
    };
    let Some(action) = action else {
        return;
    };
    match action {
        smart_group_suggestor::SuggestorAction::Apply {
            suggestion_id,
            suggestion,
        } => {
            {
                let app = root.app.borrow();
                app.suggestor.borrow_mut().mark_applying(suggestion_id);
                app.suggestor_status.borrow_mut().replace(SmolStr::new(
                    if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                        format!("正在应用建议“{}”", suggestion.name)
                    } else {
                        format!("Applying '{}'", suggestion.name)
                    },
                ));
                app.highlight_overlay.borrow_mut().clear();
            }
            if let Some(main) = find_main_hwnd(root) {
                request_redraw(main);
            }
            root.dispatcher.push(Command::GroupingApply { suggestion });
        }
        smart_group_suggestor::SuggestorAction::Dismiss { suggestion_id } => {
            root.dispatcher
                .push(Command::SuggestorDismiss { suggestion_id });
        }
        smart_group_suggestor::SuggestorAction::Close => {
            let app = root.app.borrow();
            app.suggestor.borrow_mut().on_row_leave();
            app.highlight_overlay.borrow_mut().clear();
            drop(app);
            if let Some(main) = find_main_hwnd(root) {
                request_redraw(main);
            }
            if !hwnd.is_null() {
                // SAFETY: hwnd is the focused Suggestor HWND.
                unsafe { ShowWindow(hwnd, SW_HIDE) };
            }
        }
    }
    request_redraw(hwnd);
}

pub(super) fn show_suggestor(root: &AppRoot) {
    let _panel = smart_group_suggestor::build();
    let target_id = {
        let mut app = root.app.borrow_mut();
        ensure_suggestor_target_zone(&mut app)
    };
    let (files, source_count, error_count) = scan_suggestor_desktop_files(root);
    let visible_count = seed_suggestor_from_files(root, &files, source_count, error_count);
    // Preview highlights are opt-in after the user selects a row. Opening the
    // native dialog must not immediately paint a blue selection slab across
    // the desktop behind it.
    root.app.borrow().highlight_overlay.borrow_mut().clear();
    let highlighted = 0;

    let Some(host) = ensure_aux_window(root, WindowKind::Suggestor) else {
        tracing::warn!(
            target: "bentodesk::wizard",
            "ShowSuggestor: ensure_aux_window failed"
        );
        return;
    };

    // SAFETY: canonical show + activate.
    unsafe {
        ShowWindow(host, SW_SHOW);
        SetForegroundWindow(host);
    }
    log_static(
        format!(
            "suggestor: ShowSuggestor target_zone={} files={} suggestions={} sources={} errors={} highlight_targets={}\n",
            target_id.0,
            files.len(),
            visible_count,
            source_count,
            error_count,
            highlighted
        )
        .as_str(),
    );
    tracing::info!(
        target: "bentodesk::wizard",
        files = files.len(),
        suggestions = visible_count,
        "ShowSuggestor — real Desktop suggestions seeded into selected-stack aux HWND"
    );
    tracing::info!(
        target: "bentodesk::wizard",
        "ShowSuggestor — per-row Apply/Dismiss rides existing GroupingApply / SuggestorDismiss"
    );
}
