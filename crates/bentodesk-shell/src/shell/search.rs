//! Native shell owner: `search`.

use super::*;

pub(super) fn add_search_item(
    index: &mut SearchIndex,
    id: &str,
    title: &str,
    path: &str,
    kind: SearchItemKind,
) {
    index.add(SearchItem {
        id: SmolStr::new(id),
        title: SmolStr::new(title),
        path: SmolStr::new(path),
        keywords: SmolStr::default(),
        kind,
    });
}

pub(super) fn add_search_item_with_keywords(
    index: &mut SearchIndex,
    id: &str,
    title: &str,
    path: &str,
    keywords: &str,
    kind: SearchItemKind,
) {
    index.add(SearchItem {
        id: SmolStr::new(id),
        title: SmolStr::new(title),
        path: SmolStr::new(path),
        keywords: SmolStr::new(keywords),
        kind,
    });
}

pub(super) fn add_desktop_file_search_items(
    index: &mut SearchIndex,
    files: &[bentodesk_backend::grouping::scanner::FileInfo],
) {
    for file in files {
        let id = format!("desktop:{}", file.path);
        let kind = if file.is_directory {
            SearchItemKind::Folder
        } else {
            SearchItemKind::File
        };
        add_search_item(index, &id, &file.name, &file.path, kind);
    }
}

pub(super) fn scan_search_desktop_files(
    app: &AppState,
) -> Vec<bentodesk_backend::grouping::scanner::FileInfo> {
    let mut files = Vec::new();
    for dir in configured_desktop_sources_for_app(app) {
        match bentodesk_backend::grouping::scan_desktop_files(&dir) {
            Ok(mut scanned) => files.append(&mut scanned),
            Err(error) => {
                tracing::warn!(
                    target: "bentodesk::search",
                    path = %dir.display(),
                    error = %error,
                    "QuerySearch: Desktop source scan failed"
                );
            }
        }
    }
    files
}

pub(super) fn search_zone_breadcrumb(
    zone_id: u64,
    item_count: usize,
    visible: bool,
    zh: bool,
) -> SmolStr {
    if zh {
        SmolStr::new(format!(
            "区域 {zone_id} · {item_count} 个项目 · {}",
            if visible { "显示" } else { "隐藏" }
        ))
    } else {
        SmolStr::new(format!(
            "Zone {zone_id} · {item_count} {} · {}",
            if item_count == 1 { "item" } else { "items" },
            if visible { "visible" } else { "hidden" }
        ))
    }
}

pub(super) fn search_query_status(query: &str, result_count: usize, zh: bool) -> SmolStr {
    if result_count == 0 {
        localized_message(
            zh,
            format!("未找到“{query}”的匹配结果"),
            format!("No results for \"{query}\""),
        )
    } else if zh {
        SmolStr::new(format!("找到 {result_count} 个实时结果"))
    } else {
        SmolStr::new(format!(
            "{result_count} live {}",
            if result_count == 1 {
                "result"
            } else {
                "results"
            }
        ))
    }
}

pub(super) fn seed_search_index_from_app(app: &AppState) -> SearchIndex {
    use bentodesk_style::i18n_zh_cn::ids;

    let mut index = SearchIndex::new();
    let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);

    for zone in app.zones.iter() {
        let title = zone.display_title();
        let zone_id = format!("zone:{}", zone.id.0);
        let zone_path = search_zone_breadcrumb(zone.id.0, zone.items.len(), zone.visible, zh);
        add_search_item(
            &mut index,
            &zone_id,
            title,
            zone_path.as_str(),
            SearchItemKind::Zone,
        );

        if let Some(live_folder_path) = zone.live_folder_path.as_deref() {
            let folder_id = format!("folder:{}:live", zone.id.0);
            add_search_item(
                &mut index,
                &folder_id,
                live_folder_path,
                live_folder_path,
                SearchItemKind::Folder,
            );
        }

        for item in &zone.items {
            let item_id = format!("item:{}:{}", zone.id.0, item.id.0);
            let item_path = item.path.as_ref();
            let kind = if Path::new(item_path).is_dir() {
                SearchItemKind::Folder
            } else {
                SearchItemKind::File
            };
            add_search_item(&mut index, &item_id, item.name.as_ref(), item_path, kind);
        }
    }

    let desktop_files = scan_search_desktop_files(app);
    add_desktop_file_search_items(&mut index, &desktop_files);

    let settings_group = bentodesk_style::t(ids::SEARCH_GROUP_SETTINGS);
    for (id, title_id, keywords) in [
        (
            "setting:display.locale",
            ids::SEARCH_SETTING_LOCALE,
            "display locale language settings",
        ),
        (
            "setting:updates.check_frequency",
            ids::SEARCH_SETTING_UPDATE_FREQUENCY,
            "update check frequency settings",
        ),
        (
            "setting:updates.auto_download",
            ids::SEARCH_SETTING_AUTO_DOWNLOAD,
            "auto download updates settings",
        ),
        (
            "setting:stealth.enabled",
            ids::SEARCH_SETTING_STEALTH,
            "stealth storage desktop settings",
        ),
        (
            "setting:encryption.mode",
            ids::SEARCH_SETTING_ENCRYPTION,
            "encryption mode settings",
        ),
        (
            "setting:zone_display_mode",
            ids::SEARCH_SETTING_ZONE_DISPLAY,
            "zone display mode settings",
        ),
        (
            "setting:keybindings",
            ids::SEARCH_SETTING_KEYBINDINGS,
            "keybindings keyboard shortcuts settings",
        ),
        (
            "setting:active_theme",
            ids::SEARCH_SETTING_THEME,
            "active theme appearance settings",
        ),
    ] {
        add_search_item_with_keywords(
            &mut index,
            id,
            bentodesk_style::t(title_id),
            settings_group,
            keywords,
            SearchItemKind::Setting,
        );
    }

    let actions_group = bentodesk_style::t(ids::SEARCH_GROUP_ACTIONS);
    for (id, title_id, keywords) in [
        (
            "action:create_zone",
            ids::SEARCH_ACTION_CREATE_ZONE,
            "create new zone",
        ),
        (
            "action:open_settings",
            ids::SEARCH_ACTION_OPEN_SETTINGS,
            "open settings",
        ),
        (
            "action:open_about",
            ids::SEARCH_ACTION_OPEN_ABOUT,
            "open about",
        ),
        (
            "action:open_timeline",
            ids::SEARCH_ACTION_OPEN_TIMELINE,
            "open timeline",
        ),
        (
            "action:open_snapshots",
            ids::SEARCH_ACTION_OPEN_SNAPSHOTS,
            "open snapshots layout",
        ),
        (
            "action:open_suggestor",
            ids::SEARCH_ACTION_OPEN_SUGGESTOR,
            "open smart suggestions grouping",
        ),
        (
            "action:open_bulk_manager",
            ids::SEARCH_ACTION_OPEN_BULK_MANAGER,
            "open bulk manager batch",
        ),
        (
            "action:open_capsule_picker",
            ids::SEARCH_ACTION_OPEN_CAPSULE_PICKER,
            "open context capsules",
        ),
        (
            "action:open_rules",
            ids::SEARCH_ACTION_OPEN_RULES,
            "open rules wizard",
        ),
        (
            "action:list_minibars",
            ids::SEARCH_ACTION_LIST_MINIBARS,
            "list pinned minibars mini bars",
        ),
        (
            "action:toggle_debug_overlay",
            ids::SEARCH_ACTION_TOGGLE_DEBUG,
            "toggle debug overlay information",
        ),
        (
            "action:quit",
            ids::SEARCH_ACTION_QUIT,
            "quit exit BentoDesk",
        ),
    ] {
        add_search_item_with_keywords(
            &mut index,
            id,
            bentodesk_style::t(title_id),
            actions_group,
            keywords,
            SearchItemKind::Action,
        );
    }

    index
}

pub(super) fn search_icon_for_kind(kind: &SearchItemKind) -> SmolStr {
    match kind {
        SearchItemKind::File => SmolStr::new_static("file"),
        SearchItemKind::Folder => SmolStr::new_static("folder"),
        SearchItemKind::Zone => SmolStr::new_static("grid"),
        SearchItemKind::Setting => SmolStr::new_static("settings"),
        SearchItemKind::Action => SmolStr::new_static("code"),
    }
}

pub(super) fn run_search_query(root: &AppRoot, query: &str) -> usize {
    use bentodesk_style::i18n_zh_cn::ids;

    let trimmed = query.trim();
    if trimmed.is_empty() {
        let app = root.app.borrow();
        app.search_bar
            .borrow_mut()
            .set_results(smallvec::SmallVec::new());
        app.highlight_overlay.borrow_mut().clear();
        app.search_status
            .borrow_mut()
            .replace(SmolStr::new_static(bentodesk_style::t(
                ids::SEARCH_IDLE_HINT,
            )));
        drop(app);
        if let Some(main) = find_main_hwnd(root) {
            request_redraw(main);
        }
        return 0;
    }

    let index = {
        let app = root.app.borrow();
        seed_search_index_from_app(&app)
    };
    let backend_hits = index.query(trimmed, search_bar::MAX_VISIBLE_RESULTS);
    let result_count = backend_hits.len();
    let mut rows: smallvec::SmallVec<[search_bar::SearchHit; 8]> = smallvec::SmallVec::new();
    for hit in backend_hits {
        rows.push(search_bar::SearchHit {
            id: hit.id,
            icon: search_icon_for_kind(&hit.kind),
            kind: hit.kind,
            name: hit.title,
            breadcrumb: hit.path,
            score: hit.score,
            matched_token: hit.matched_token,
        });
    }

    let app = root.app.borrow();
    app.search_bar.borrow_mut().set_results(rows);
    drop(app);
    let highlighted = set_highlight_for_search_selection(root);
    if let Some(main) = find_main_hwnd(root) {
        request_redraw(main);
    }
    let app = root.app.borrow();
    let status = search_query_status(
        trimmed,
        result_count,
        bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN),
    );
    app.search_status.borrow_mut().replace(status);
    log_static(
        format!(
            "search: QuerySearch query=\"{}\" results={} highlight_targets={}\n",
            trimmed, result_count, highlighted
        )
        .as_str(),
    );
    result_count
}

pub(super) fn show_search(root: &AppRoot) {
    use bentodesk_style::i18n_zh_cn::ids;

    let _panel = search_bar::build();
    if let Some(main) = find_main_hwnd(root) {
        close_inline_zone_search(root, main);
    }
    {
        let app = root.app.borrow();
        app.search_bar.borrow_mut().clear();
        app.highlight_overlay.borrow_mut().clear();
        app.search_status
            .borrow_mut()
            .replace(SmolStr::new_static(bentodesk_style::t(
                ids::SEARCH_IDLE_HINT,
            )));
    }

    let Some(host) = ensure_aux_window(root, WindowKind::Search) else {
        tracing::warn!(
            target: "bentodesk::search",
            "OpenSearch: ensure_aux_window failed"
        );
        return;
    };

    // SAFETY: canonical show + activate for a focusable aux HWND.
    unsafe {
        ShowWindow(host, SW_SHOW);
        SetForegroundWindow(host);
    }
    request_redraw(host);
    log_static(format!("search: OpenSearch shown hwnd={}\n", host as usize).as_str());
    tracing::info!(
        target: "bentodesk::search",
        "OpenSearch: selected-stack Search HWND shown"
    );
}

pub(super) fn parse_search_zone_id(hit_id: &str) -> Option<ZoneId> {
    let raw = hit_id.strip_prefix("zone:")?;
    raw.parse::<u64>().ok().map(ZoneId)
}

pub(super) fn parse_search_item_id(hit_id: &str) -> Option<(ZoneId, ZoneItemId)> {
    let raw = hit_id.strip_prefix("item:")?;
    let mut parts = raw.split(':');
    let zone_id = parts.next()?.parse::<u64>().ok().map(ZoneId)?;
    let item_id = parts.next()?.parse::<u64>().ok().map(ZoneItemId)?;
    if parts.next().is_some() {
        return None;
    }
    Some((zone_id, item_id))
}

pub(super) fn parse_search_live_folder_zone_id(hit_id: &str) -> Option<ZoneId> {
    let raw = hit_id.strip_prefix("folder:")?;
    let zone_raw = raw.strip_suffix(":live")?;
    zone_raw.parse::<u64>().ok().map(ZoneId)
}

pub(super) fn set_highlight_for_search_hit(
    root: &AppRoot,
    hit: &search_bar::SearchHit,
    timed: bool,
) -> usize {
    let mut targets = smallvec::SmallVec::<[HighlightRect; 8]>::new();
    let mut pulse_paths = smallvec::SmallVec::<[String; 8]>::new();
    {
        let app = root.app.borrow();
        match hit.kind {
            SearchItemKind::Zone => {
                if let Some(zone_id) = parse_search_zone_id(hit.id.as_str()) {
                    if let Some(zone) = app.zones.get(zone_id) {
                        targets.push(highlight_overlay::zone_target_rect(zone));
                    }
                }
            }
            SearchItemKind::File | SearchItemKind::Folder => {
                if let Some((zone_id, item_id)) = parse_search_item_id(hit.id.as_str()) {
                    if let Some(zone) = app.zones.get(zone_id) {
                        if let Some(item) = zone.item(item_id) {
                            targets.push(highlight_overlay::item_target_rect(zone, item));
                        }
                    }
                } else if let Some(zone_id) = parse_search_live_folder_zone_id(hit.id.as_str()) {
                    if let Some(zone) = app.zones.get(zone_id) {
                        targets.push(highlight_overlay::zone_target_rect(zone));
                    }
                } else {
                    let fallback_paths = [hit.breadcrumb.to_string()];
                    targets = highlight_targets_for_paths(&app, &fallback_paths);
                    pulse_paths.push(hit.breadcrumb.to_string());
                }
            }
            SearchItemKind::Setting | SearchItemKind::Action => {}
        }
    }

    let pulses = if pulse_paths.is_empty() {
        smallvec::SmallVec::<[HighlightPulse; 8]>::new()
    } else {
        desktop_pulses_for_paths(root, &pulse_paths)
    };
    let app = root.app.borrow();
    let count = targets.len().saturating_add(pulses.len());
    if count == 0 {
        app.highlight_overlay.borrow_mut().clear();
    } else if timed {
        app.highlight_overlay
            .borrow_mut()
            .set_targets_and_pulses_for(targets, pulses, 3_000);
    } else {
        app.highlight_overlay
            .borrow_mut()
            .set_targets_and_pulses(targets, pulses);
    }
    count
}

pub(super) fn set_highlight_for_search_selection(root: &AppRoot) -> usize {
    let hit = {
        let app = root.app.borrow();
        app.search_bar.borrow().current_hit().cloned()
    };
    match hit.as_ref() {
        Some(selected_hit) => set_highlight_for_search_hit(root, selected_hit, false),
        None => {
            let app = root.app.borrow();
            app.highlight_overlay.borrow_mut().clear();
            0
        }
    }
}

pub(super) fn push_search_action(root: &AppRoot, action_id: &str) -> bool {
    match action_id {
        "action:create_zone" => root
            .dispatcher
            .push(Command::CreateZone(default_zone_spec(root))),
        "action:open_settings" => root.dispatcher.push(Command::OpenSettings),
        "action:open_about" => root.dispatcher.push(Command::OpenAbout),
        "action:open_timeline" => root.dispatcher.push(Command::OpenTimeline),
        "action:open_snapshots" => root.dispatcher.push(Command::OpenSnapshotPicker),
        "action:open_suggestor" => root.dispatcher.push(Command::ShowSuggestor),
        "action:open_bulk_manager" => root.dispatcher.push(Command::OpenBulkManager),
        "action:open_capsule_picker" => root.dispatcher.push(Command::OpenCapsulePicker),
        "action:open_rules" => root.dispatcher.push(Command::OpenRulesWizard),
        "action:list_minibars" => root.dispatcher.push(Command::ListPinnedMinibars),
        "action:toggle_debug_overlay" => root.dispatcher.push(Command::ToggleDebugOverlay),
        "action:quit" => root.dispatcher.push(Command::QuitApp),
        _ => false,
    }
}

pub(super) fn activate_search_hit(root: &AppRoot, hit_id: &str, hwnd: HWND) -> bool {
    let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);
    let hit = {
        let app = root.app.borrow();
        app.search_bar
            .borrow()
            .results
            .iter()
            .find(|hit| hit.id.as_str() == hit_id)
            .cloned()
    };
    let Some(hit) = hit else {
        let app = root.app.borrow();
        app.search_status.borrow_mut().replace(localized_message(
            zh,
            format!("搜索结果已失效：{hit_id}"),
            format!("Search result no longer exists: {hit_id}"),
        ));
        return false;
    };

    log_static(
        format!(
            "search: ActivateSearchResult id={} kind={:?}\n",
            hit.id, hit.kind
        )
        .as_str(),
    );

    match hit.kind {
        SearchItemKind::Zone => {
            let _highlighted = set_highlight_for_search_hit(root, &hit, true);
            let Some(zone_id) = parse_search_zone_id(hit.id.as_str()) else {
                let app = root.app.borrow();
                app.search_status.borrow_mut().replace(localized_message(
                    zh,
                    format!("搜索结果中的区域编号无效：{}", hit.id),
                    format!("Invalid search Zone id: {}", hit.id),
                ));
                return false;
            };
            let app = root.app.borrow();
            if app.zones.get(zone_id).is_none() {
                app.search_status.borrow_mut().replace(localized_message(
                    zh,
                    format!("区域 {} 已不存在", zone_id.0),
                    format!("Zone {} no longer exists", zone_id.0),
                ));
                return false;
            }
            app.selected_zone.set(Some(zone_id));
            app.hovered_zone.set(Some(zone_id));
            app.search_status.borrow_mut().replace(localized_message(
                zh,
                format!("已选择区域 {}", zone_id.0),
                format!("Selected Zone {}", zone_id.0),
            ));
        }
        SearchItemKind::File | SearchItemKind::Folder => {
            let _highlighted = set_highlight_for_search_hit(root, &hit, true);
            let launch_result = shell_execute_path("open", hit.breadcrumb.as_str(), None);
            let app = root.app.borrow();
            let status = match launch_result {
                Ok(()) => localized_message(
                    zh,
                    format!("正在打开：{}", hit.breadcrumb),
                    format!("Opening {}", hit.breadcrumb),
                ),
                Err(code) => localized_message(
                    zh,
                    format!("无法打开 {}：ShellExecuteW failed: {code}", hit.breadcrumb),
                    format!(
                        "Unable to open {}: ShellExecuteW failed: {code}",
                        hit.breadcrumb
                    ),
                ),
            };
            app.search_status.borrow_mut().replace(status);
        }
        SearchItemKind::Setting => {
            let app = root.app.borrow();
            app.highlight_overlay.borrow_mut().clear();
            drop(app);
            root.dispatcher.push(Command::OpenSettings);
            let app = root.app.borrow();
            app.search_status.borrow_mut().replace(localized_message(
                zh,
                format!("正在打开：{}", hit.name),
                format!("Opening {}", hit.name),
            ));
        }
        SearchItemKind::Action => {
            let app = root.app.borrow();
            app.highlight_overlay.borrow_mut().clear();
            drop(app);
            if !push_search_action(root, hit.id.as_str()) {
                let app = root.app.borrow();
                app.search_status.borrow_mut().replace(localized_message(
                    zh,
                    "当前搜索操作不可用",
                    format!("Unsupported search action: {}", hit.id),
                ));
                return false;
            }
            let app = root.app.borrow();
            app.search_status.borrow_mut().replace(localized_message(
                zh,
                format!("正在执行：{}", hit.name),
                format!("Running {}", hit.name),
            ));
        }
    }

    if !hwnd.is_null() {
        // SAFETY: ShowWindow with SW_HIDE on a HWND owned by this process.
        unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
    if let Some(main) = find_main_hwnd(root) {
        request_redraw(main);
    }
    request_redraw(hwnd);
    true
}
