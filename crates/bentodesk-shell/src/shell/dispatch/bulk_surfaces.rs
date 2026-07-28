//! Command handlers for the `bulk_surfaces` domain.

use super::*;

pub(super) fn dispatch(
    root: &AppRoot,
    hwnd: HWND,
    command: Command,
    effects: &mut DispatchEffects,
) {
    match command {
        Command::OpenBulkManager => {
            // F2-08 — open the bulk-action manager aux HWND
            // (`business::bulk_manager_panel`). The selected-stack
            // keyboard path now emits bulk hide/show/delete/move commands.
            open_bulk_manager(root);
        }
        Command::BulkDeleteZones(ids) => {
            let before_snapshot = capture_current_timeline_snapshot(root, "before bulk delete");
            let coalesce_scope = sorted_zone_ids_key(&ids);
            let mut removed = 0usize;
            {
                let mut app = root.app.borrow_mut();
                for id in &ids {
                    if app.zones.remove(*id) {
                        removed += 1;
                    }
                }
                if removed > 0 {
                    app.mark_dirty();
                }
                let status = if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                    format!("已删除 {removed} 个区域")
                } else {
                    format!("Deleted {removed} zones")
                };
                app.bulk_manager_status
                    .borrow_mut()
                    .replace(SmolStr::new(status));
                let rows = bulk_manager_rows_from_app(&app);
                app.bulk_manager.borrow_mut().set_zones(rows);
            }
            if let Some(target) = find_aux_window(root, WindowKind::BulkManager) {
                request_redraw(target);
            }
            if removed > 0 {
                record_coalesced_mutation_timeline_pair(
                    root,
                    before_snapshot,
                    "bulk_pre_apply",
                    "bulk_delete_zones",
                    &coalesce_scope,
                    localized_current("已记录批量删除前后的布局", "Bulk delete checkpointed"),
                );
            }
            log_static(format!("bulk: BulkDeleteZones removed={removed}\n").as_str());
            effects.needs_redraw = true;
        }
        Command::BulkSetZonesVisible { ids, visible } => {
            let before_snapshot = capture_current_timeline_snapshot(root, "before bulk visibility");
            let coalesce_scope = format!("visible={visible}:ids={}", sorted_zone_ids_key(&ids));
            let changed;
            let matched;
            {
                let mut app = root.app.borrow_mut();
                (changed, matched) = apply_bulk_zone_visibility(&mut app, &ids, visible);
                if changed > 0 {
                    app.mark_dirty();
                }
                let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);
                let status = if zh {
                    let action = if visible { "显示" } else { "隐藏" };
                    format!("已{action} {changed} 个区域（匹配 {matched} 个）")
                } else {
                    let action = if visible { "Shown" } else { "Hidden" };
                    format!("{action} {changed} zones ({matched} matched)")
                };
                app.bulk_manager_status
                    .borrow_mut()
                    .replace(SmolStr::new(status));
                let rows = bulk_manager_rows_from_app(&app);
                app.bulk_manager.borrow_mut().set_zones(rows);
            }
            if let Some(target) = find_aux_window(root, WindowKind::BulkManager) {
                request_redraw(target);
            }
            if changed > 0 {
                record_coalesced_mutation_timeline_pair(
                    root,
                    before_snapshot,
                    "bulk_pre_apply",
                    "bulk_set_zones_visible",
                    &coalesce_scope,
                    localized_current(
                        "已记录批量显示状态变更前后的布局",
                        "Bulk visibility checkpointed",
                    ),
                );
            }
            log_static(
                        format!(
                            "bulk: BulkSetZonesVisible visible={visible} changed={changed} matched={matched}\n"
                        )
                        .as_str(),
                    );
            effects.needs_redraw = true;
        }
        Command::BulkApplyLayout { ids, algorithm } => {
            let before_snapshot = capture_current_timeline_snapshot(root, "before bulk layout");
            let coalesce_scope = format!(
                "algorithm={}:ids={}",
                algorithm.wire(),
                sorted_zone_ids_key(&ids)
            );
            let changed;
            let matched;
            {
                let mut app = root.app.borrow_mut();
                (changed, matched) = apply_bulk_layout_algorithm(&mut app, &ids, algorithm);
                if changed > 0 {
                    app.mark_dirty();
                }
                let zh = bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN);
                let layout = bulk_layout_algorithm_text(algorithm, zh);
                let status = if zh {
                    format!("已对 {changed} 个区域应用{layout}布局（匹配 {matched} 个）")
                } else {
                    format!("Applied {layout} layout to {changed} zones ({matched} matched)")
                };
                app.bulk_manager_status
                    .borrow_mut()
                    .replace(SmolStr::new(status));
                let rows = bulk_manager_rows_from_app(&app);
                app.bulk_manager.borrow_mut().set_zones(rows);
            }
            if let Some(target) = find_aux_window(root, WindowKind::BulkManager) {
                request_redraw(target);
            }
            if changed > 0 {
                record_coalesced_mutation_timeline_pair(
                    root,
                    before_snapshot,
                    "bulk_pre_apply",
                    "apply_layout_algorithm",
                    &coalesce_scope,
                    localized_current("已记录批量布局前后的状态", "Bulk layout checkpointed"),
                );
            }
            log_static(
                format!(
                    "bulk: BulkApplyLayout algorithm={} changed={changed} matched={matched}\n",
                    algorithm.wire()
                )
                .as_str(),
            );
            effects.needs_redraw = true;
        }
        Command::BulkUpdateZones(updates) => {
            let before_snapshot = capture_current_timeline_snapshot(root, "before bulk update");
            let update_ids = updates.iter().map(|update| update.id).collect::<Vec<_>>();
            let coalesce_scope = sorted_zone_ids_key(&update_ids);
            let changed;
            let matched;
            {
                let mut app = root.app.borrow_mut();
                (changed, matched) = apply_bulk_zone_updates(&mut app, &updates);
                if changed > 0 {
                    app.mark_dirty();
                }
                let status = if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                    format!("已更新 {changed} 个区域（匹配 {matched} 个）")
                } else {
                    format!("Updated {changed} zones ({matched} matched)")
                };
                app.bulk_manager_status
                    .borrow_mut()
                    .replace(SmolStr::new(status));
                let rows = bulk_manager_rows_from_app(&app);
                app.bulk_manager.borrow_mut().set_zones(rows);
            }
            if let Some(target) = find_aux_window(root, WindowKind::BulkManager) {
                request_redraw(target);
            }
            if changed > 0 {
                record_coalesced_mutation_timeline_pair(
                    root,
                    before_snapshot,
                    "bulk_pre_apply",
                    "bulk_update_zones",
                    &coalesce_scope,
                    localized_current("已记录批量更新前后的状态", "Bulk update checkpointed"),
                );
            }
            log_static(
                format!("bulk: BulkUpdateZones changed={changed} matched={matched}\n").as_str(),
            );
            effects.needs_redraw = true;
        }
        Command::BulkMoveZones { ids, delta } => {
            let before_snapshot = capture_current_timeline_snapshot(root, "before bulk move");
            let coalesce_scope = format!(
                "dx={}:dy={}:ids={}",
                delta.x,
                delta.y,
                sorted_zone_ids_key(&ids)
            );
            let mut moved = 0usize;
            {
                let mut app = root.app.borrow_mut();
                for id in &ids {
                    if let Some(zone) = app.zones.get_mut(*id) {
                        if zone.locked {
                            continue;
                        }
                        zone.x = zone.x.saturating_add(delta.x);
                        zone.y = zone.y.saturating_add(delta.y);
                        moved += 1;
                    }
                }
                if moved > 0 {
                    app.mark_dirty();
                }
                let status = if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                    format!("已将 {moved} 个区域移动 {},{}", delta.x, delta.y)
                } else {
                    format!("Moved {moved} zones by {},{}", delta.x, delta.y)
                };
                app.bulk_manager_status
                    .borrow_mut()
                    .replace(SmolStr::new(status));
                let rows = bulk_manager_rows_from_app(&app);
                app.bulk_manager.borrow_mut().set_zones(rows);
            }
            if let Some(target) = find_aux_window(root, WindowKind::BulkManager) {
                request_redraw(target);
            }
            if moved > 0 {
                record_coalesced_mutation_timeline_pair(
                    root,
                    before_snapshot,
                    "bulk_pre_apply",
                    "bulk_move_zones",
                    &coalesce_scope,
                    localized_current("已记录批量移动前后的布局", "Bulk move checkpointed"),
                );
            }
            log_static(
                format!(
                    "bulk: BulkMoveZones moved={moved} dx={} dy={}\n",
                    delta.x, delta.y
                )
                .as_str(),
            );
            effects.needs_redraw = true;
        }
        Command::OpenZoneEditor(zone_id) => {
            open_zone_editor(root, zone_id);
        }
        Command::ShowSuggestor => {
            // F2-08 — open the smart-group suggestor panel aux HWND
            // (`business::smart_group_suggestor`). Pairs with the
            // existing `SuggestorDismiss` (per-row) and
            // `GroupingApply` (per-row) Commands.
            show_suggestor(root);
        }
        Command::OpenSearch => {
            show_search(root);
            effects.needs_redraw = true;
        }
        Command::QuerySearch(query) => {
            let _result_count = run_search_query(root, query.as_str());
            effects.needs_redraw = true;
        }
        Command::ActivateSearchResult(hit_id) => {
            let search_hwnd = find_aux_window(root, WindowKind::Search).unwrap_or(hwnd);
            if activate_search_hit(root, hit_id.as_str(), search_hwnd) {
                effects.needs_redraw = true;
            }
        }
        Command::CloseSearch => {
            if let Some(main) = find_main_hwnd(root) {
                close_inline_zone_search(root, main);
            }
            if let Some(target) = find_aux_window(root, WindowKind::Search) {
                // SAFETY: ShowWindow with SW_HIDE on a HWND we own.
                unsafe { ShowWindow(target, SW_HIDE) };
            }
            let app = root.app.borrow();
            app.highlight_overlay.borrow_mut().clear();
            drop(app);
            if let Some(target) = find_main_hwnd(root) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::PinZoneAsMinibar(zone_id) => {
            if pin_zone_as_minibar(root, zone_id) {
                persist_minibar_pins_to_vault(root);
                effects.needs_redraw = true;
            }
        }
        Command::UnpinMinibar(zone_id) => {
            if unpin_zone_minibar(root, zone_id) {
                persist_minibar_pins_to_vault(root);
                effects.needs_redraw = true;
            }
        }
        Command::ListPinnedMinibars => {
            show_pinned_minibar_list_status(root);
            effects.needs_redraw = true;
        }
        Command::ShowTooltip { anchor, text } => {
            let main_anchor =
                find_main_hwnd(root).map(|main| bentodesk_app::WindowHandle(main as isize));
            let context_menu_open = root.app.borrow().active_context_menu.borrow().is_some();
            if tooltip_uses_aux_surface(anchor, main_anchor, context_menu_open) {
                show_tooltip(root, anchor, &text);
            } else {
                // Auxiliary panels render their own labels/status. A
                // second DComp HWND here becomes a detached black strip
                // after the original hover target disappears.
                hide_tooltip(root);
            }
        }
        Command::HideTooltip => {
            hide_tooltip(root);
        }
        Command::GroupingApply { suggestion } => {
            let mut app = root.app.borrow_mut();
            let target_id = ensure_suggestor_target_zone(&mut app);
            match bentodesk_backend::grouping::apply_auto_group_to_zone(
                &suggestion,
                target_id,
                &mut app.zones,
            ) {
                Ok(added) => {
                    let target_name = app
                        .zones
                        .get(target_id)
                        .map(|zone| zone.display_title().to_owned())
                        .unwrap_or_else(|| target_id.0.to_string());
                    app.suggestor.borrow_mut().clear_applying();
                    app.suggestor_status.borrow_mut().replace(SmolStr::new(
                        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                            format!("已将 {added} 个文件整理到“{target_name}”")
                        } else {
                            format!("Applied {added} files to '{target_name}'")
                        },
                    ));
                    // The suggestor preview owns its temporary Desktop
                    // highlight. A successful apply closes that surface,
                    // so retaining a timed blue overlay afterwards makes
                    // the operation look like a second floating layer.
                    app.highlight_overlay.borrow_mut().clear();
                    if added > 0 {
                        app.mark_dirty();
                    }
                    log_static(
                                format!(
                                    "suggestor: GroupingApply name=\"{}\" target_zone={} added={} paths={} highlight_cleared=true\n",
                                    suggestion.name,
                                    target_id.0,
                                    added,
                                    suggestion.matching_files.len()
                                )
                                .as_str(),
                            );
                    drop(app);
                    if let Some(target) = find_main_hwnd(root) {
                        request_redraw(target);
                    }
                    if let Some(target) = find_aux_window(root, WindowKind::Suggestor) {
                        // Keep the panel visible while the real backend
                        // operation runs; only a successful apply closes
                        // it so an error remains visible and actionable.
                        unsafe { ShowWindow(target, SW_HIDE) };
                    }
                    effects.needs_redraw = true;
                }
                Err(e) => {
                    app.suggestor.borrow_mut().clear_applying();
                    app.suggestor_status.borrow_mut().replace(SmolStr::new(
                        if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                            format!("应用建议失败：{e}")
                        } else {
                            format!("Apply failed: {e}")
                        },
                    ));
                    tracing::warn!(
                        target: "bentodesk::dispatcher",
                        error = %e,
                        "GroupingApply: apply_auto_group rejected"
                    );
                    drop(app);
                    if let Some(target) = find_aux_window(root, WindowKind::Suggestor) {
                        request_redraw(target);
                    }
                    effects.needs_redraw = true;
                }
            }
        }
        Command::SuggestorDismiss { suggestion_id } => {
            // UI-only state flip — record the dismissal so the panel
            // re-render skips this row. F4 (smart-group panel mount)
            // will read `app.suggestor_dismissed` from its render path.
            let app = root.app.borrow();
            app.suggestor_dismissed
                .borrow_mut()
                .insert(suggestion_id.clone());
            app.suggestor
                .borrow_mut()
                .remove_entry(suggestion_id.as_str());
            app.suggestor_status.borrow_mut().replace(SmolStr::new(
                if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                    "已忽略该分组建议".to_owned()
                } else {
                    format!("Dismissed {suggestion_id}")
                },
            ));
            log_static(format!("suggestor: SuggestorDismiss id={suggestion_id}\n").as_str());
            drop(app);
            let _highlighted = set_highlight_for_suggestor_selection(root);
            if let Some(target) = find_main_hwnd(root) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::ShowContextMenu { anchor, items } => {
            // F2-06 — Win32 TrackPopupMenu spawn via `business::popover`.
            // Items list maps command_id → Command; selection result is
            // pushed back onto the dispatcher.
            // SAFETY: TrackPopupMenu loop is canonical — see Ruling B.
            unsafe { show_context_menu(root, anchor, &items) };
        }
        Command::HideContextMenu => {
            // No persistent context menu HWND exists — TrackPopupMenu is
            // a synchronous modal-loop API, so a separate hide path is a
            // no-op once `show_context_menu` returns. The aux
            // ContextMenu HWND (used as TrackPopupMenu owner) is safe to
            // leave hidden.
            hide_context_menu(root);
        }
        Command::QuitApp => {
            effects.quit_after_drain = true;
        }
        _ => unreachable!("command routed to the wrong bulk_surfaces dispatcher"),
    }
}
