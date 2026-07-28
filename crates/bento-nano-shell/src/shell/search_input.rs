//! Native shell owner: `search_input`.

use super::*;

pub(super) fn focus_window_for_keyboard(hwnd: HWND) {
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let foreground = GetForegroundWindow();
        let current_thread = GetCurrentThreadId();
        let foreground_thread = if foreground.is_null() || foreground == hwnd {
            0
        } else {
            GetWindowThreadProcessId(foreground, ptr::null_mut())
        };
        let attached = foreground_thread != 0
            && foreground_thread != current_thread
            && AttachThreadInput(current_thread, foreground_thread, 1) != 0;
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        SetActiveWindow(hwnd);
        SetFocus(hwnd);
        if attached {
            AttachThreadInput(current_thread, foreground_thread, 0);
        }
    }
}

pub(super) fn set_main_inline_search_keyboard_focus(hwnd: HWND, active: bool) {
    if hwnd.is_null() {
        return;
    }
    // Main normally stays WS_EX_NOACTIVATE so desktop clicks do not foreground
    // its fullscreen transparent host. Inline Zone search is the one bounded
    // exception: it needs WM_CHAR, so remove the bit only for that session.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let next = if active {
            style & !(WS_EX_NOACTIVATE as isize)
        } else {
            style | WS_EX_NOACTIVATE as isize
        };
        if style != next {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
            SetWindowPos(
                hwnd,
                ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
        if active {
            // A fullscreen desktop host normally lives outside the foreground
            // input queue. `SetForegroundWindow` alone can therefore fail and
            // leave WM_CHAR routed to the previously active application even
            // after WS_EX_NOACTIVATE was removed. Join the two queues only for
            // this bounded hand-off, make Main the active/focused window, then
            // detach immediately. No polling or helper window is required.
            focus_window_for_keyboard(hwnd);
        }
    }
}

pub(super) fn open_inline_zone_search(root: &AppRoot, zone_id: ZoneId, hwnd: HWND) {
    // SAFETY: GetTickCount is total and thread-safe.
    let now_ms = unsafe { GetTickCount() };
    {
        let app = root.app.borrow();
        let previous_target = app.zone_search_target.get();
        let expand_from = app.zones.get(zone_id).and_then(|zone| {
            if zone.is_stack_anchor() || app.zone_pill_body_visible(zone) {
                None
            } else if app.zone_pill_anim_zone.get() == Some(zone_id) {
                Some(sampled_zone_pill_morph(&app))
            } else {
                Some(0.0)
            }
        });
        if previous_target.is_none() {
            // SAFETY: GetForegroundWindow returns either a live HWND or null.
            app.zone_search_previous_foreground
                .set(unsafe { GetForegroundWindow() } as isize);
        }
        if let Some(previous_zone_id) = previous_target.filter(|id| *id != zone_id) {
            app.pill_animator
                .borrow_mut()
                .cancel(previous_zone_id, AnimChannel::InlineSearch);
        }
        app.zone_search_target.set(Some(zone_id));
        app.zone_search_closing.set(false);
        app.zone_search_last_interaction_ms.set(now_ms);
        app.search_bar.borrow_mut().clear();
        app.highlight_overlay.borrow_mut().clear();
        app.reset_zone_content_scroll();
        let mut animator = app.pill_animator.borrow_mut();
        if previous_target == Some(zone_id) {
            animator.start_or_reverse(
                zone_id,
                AnimChannel::InlineSearch,
                now_ms,
                INLINE_SEARCH_IN_DURATION_MS,
                1.0,
                Easing::EaseOutCubic,
            );
        } else {
            animator.start(
                zone_id,
                AnimChannel::InlineSearch,
                now_ms,
                INLINE_SEARCH_IN_DURATION_MS,
                0.0,
                1.0,
                Easing::EaseOutCubic,
            );
        }
        drop(animator);
        if let Some(from_morph) = expand_from {
            begin_zone_pill_segment(&app, zone_id, from_morph, true, now_ms);
        }
    }
    set_main_inline_search_keyboard_focus(hwnd, true);
    arm_hover_frame_timer(hwnd);
    request_redraw(hwnd);
    log_static(format!("search: OpenZoneSearch zone={} inline=true\n", zone_id.0).as_str());
}

pub(super) fn close_inline_zone_search(root: &AppRoot, hwnd: HWND) -> bool {
    // SAFETY: GetTickCount is total and thread-safe.
    let now_ms = unsafe { GetTickCount() };
    let previous = {
        let app = root.app.borrow();
        let Some(zone_id) = app.zone_search_target.get() else {
            return false;
        };
        if app.zone_search_closing.replace(true) {
            return false;
        }
        app.zone_search_last_interaction_ms.set(now_ms);
        app.pill_animator.borrow_mut().start_or_reverse(
            zone_id,
            AnimChannel::InlineSearch,
            now_ms,
            INLINE_SEARCH_OUT_DURATION_MS,
            0.0,
            Easing::EaseOutCubic,
        );
        app.zone_search_previous_foreground.replace(0) as HWND
    };
    set_main_inline_search_keyboard_focus(hwnd, false);
    // Restore the app the user was in before clicking the desktop Zone. The
    // Main host immediately returns to no-activate desktop behavior.
    unsafe {
        if !previous.is_null() && previous != hwnd && IsWindow(previous) != 0 {
            SetForegroundWindow(previous);
        }
    }
    arm_hover_frame_timer(hwnd);
    request_redraw(hwnd);
    log_static("search: CloseZoneSearch inline=true animated=true\n");
    true
}

pub(super) fn touch_inline_zone_search(app: &AppState, now_ms: u32) {
    if app.zone_search_target.get().is_some() && !app.zone_search_closing.get() {
        app.zone_search_last_interaction_ms.set(now_ms);
    }
}

/// Retire the held open animator entry and clear the search model only after
/// the reverse reveal reaches zero. Returns true when paint-visible state
/// changed and the Main HWND needs one final redraw.
pub(super) fn settle_inline_zone_search_animation(app: &AppState, now_ms: u32) -> bool {
    let Some(zone_id) = app.zone_search_target.get() else {
        return false;
    };
    let progress = app.zone_search_animation_progress_at(now_ms);
    let contains = app
        .pill_animator
        .borrow()
        .contains(zone_id, AnimChannel::InlineSearch);
    if app.zone_search_closing.get() {
        if contains || progress > f32::EPSILON {
            return false;
        }
        let collapse_after_search = app.zones.get(zone_id).is_some_and(|zone| {
            !zone.is_stack_anchor()
                && match app.effective_zone_display_mode(zone) {
                    ZoneDisplayMode::Always => false,
                    ZoneDisplayMode::Hover => {
                        app.hover_scheduler.get().expanded_zone() != Some(zone_id)
                    }
                    ZoneDisplayMode::Click => app.selected_zone.get() != Some(zone_id),
                }
        });
        app.zone_search_target.set(None);
        app.zone_search_closing.set(false);
        app.search_bar.borrow_mut().clear();
        app.reset_zone_content_scroll();
        if collapse_after_search {
            begin_zone_pill_segment(app, zone_id, 1.0, false, now_ms);
        }
        log_static("search: inline collapse settled\n");
        return true;
    }
    if contains && progress >= 1.0 - f32::EPSILON {
        app.pill_animator
            .borrow_mut()
            .cancel(zone_id, AnimChannel::InlineSearch);
    }
    false
}

pub(super) fn close_idle_inline_zone_search(root: &AppRoot, hwnd: HWND, now_ms: u32) -> bool {
    let should_close = {
        let app = root.app.borrow();
        app.zone_search_target.get().is_some()
            && !app.zone_search_closing.get()
            && app.search_bar.borrow().query.is_empty()
            && now_ms.wrapping_sub(app.zone_search_last_interaction_ms.get())
                >= search_bar::ZONE_INLINE_IDLE_DISMISS_MS
    };
    should_close && close_inline_zone_search(root, hwnd)
}

pub(super) fn handle_inline_zone_search_char(root: &AppRoot, codepoint: u32, hwnd: HWND) -> bool {
    let Some(character) = char::from_u32(codepoint) else {
        return false;
    };
    if character.is_control() {
        return false;
    }
    let app = root.app.borrow();
    if app.zone_search_target.get().is_none() || app.zone_search_closing.get() {
        return false;
    }
    let changed = app.search_bar.borrow_mut().append_char(character);
    if changed {
        app.reset_zone_content_scroll();
        // SAFETY: GetTickCount is total and thread-safe.
        touch_inline_zone_search(&app, unsafe { GetTickCount() });
    }
    drop(app);
    if changed {
        request_redraw(hwnd);
    }
    changed
}

pub(super) fn handle_inline_zone_search_keydown(
    root: &AppRoot,
    vk: u32,
    hwnd: HWND,
) -> Option<LRESULT> {
    let zone_id = {
        let app = root.app.borrow();
        if app.zone_search_closing.get() {
            return None;
        }
        app.zone_search_target.get()?
    };
    match vk {
        VK_BACKSPACE => {
            let app = root.app.borrow();
            let changed = app.search_bar.borrow_mut().backspace();
            if changed {
                app.reset_zone_content_scroll();
                // SAFETY: GetTickCount is total and thread-safe.
                touch_inline_zone_search(&app, unsafe { GetTickCount() });
                request_redraw(hwnd);
            }
            Some(0)
        }
        VK_ENTER => {
            let first_match = {
                let app = root.app.borrow();
                let query = app.search_bar.borrow().query.clone();
                if query.trim().is_empty() {
                    None
                } else {
                    app.zones.get(zone_id).and_then(|zone| {
                        zone.items
                            .iter()
                            .find(|item| {
                                search_bar::zone_item_matches_query(
                                    item.name.as_ref(),
                                    query.as_str(),
                                )
                            })
                            .map(|item| bento_nano_app::ItemId(item.id.0))
                    })
                }
            };
            if let Some(item_id) = first_match {
                root.dispatcher
                    .push(Command::OpenItemFile(zone_id, item_id));
                close_inline_zone_search(root, hwnd);
            }
            Some(0)
        }
        VK_ESCAPE_KEY => {
            close_inline_zone_search(root, hwnd);
            Some(0)
        }
        _ => None,
    }
}

pub(super) fn handle_search_char(root: &AppRoot, codepoint: u32, hwnd: HWND) -> bool {
    let Some(character) = char::from_u32(codepoint) else {
        return false;
    };
    if character.is_control() {
        return false;
    }
    let query = {
        let app = root.app.borrow();
        let mut search = app.search_bar.borrow_mut();
        if !search.append_char(character) {
            return false;
        }
        search.query.clone()
    };
    root.dispatcher.push(Command::QuerySearch(query));
    request_redraw(hwnd);
    true
}

pub(super) fn handle_search_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> LRESULT {
    match vk {
        VK_UP_KEY => {
            let app = root.app.borrow();
            app.search_bar.borrow_mut().select_prev();
            drop(app);
            let _highlighted = set_highlight_for_search_selection(root);
            if let Some(main) = find_main_hwnd(root) {
                request_redraw(main);
            }
            request_redraw(hwnd);
            0
        }
        VK_DOWN_KEY => {
            let app = root.app.borrow();
            app.search_bar.borrow_mut().select_next();
            drop(app);
            let _highlighted = set_highlight_for_search_selection(root);
            if let Some(main) = find_main_hwnd(root) {
                request_redraw(main);
            }
            request_redraw(hwnd);
            0
        }
        VK_BACKSPACE => {
            let query = {
                let app = root.app.borrow();
                let mut search = app.search_bar.borrow_mut();
                if !search.backspace() {
                    return 0;
                }
                search.query.clone()
            };
            root.dispatcher.push(Command::QuerySearch(query));
            request_redraw(hwnd);
            0
        }
        VK_ENTER => {
            let hit_id = {
                let app = root.app.borrow();
                app.search_bar
                    .borrow()
                    .current_hit()
                    .map(|hit| hit.id.clone())
            };
            if let Some(hit_id) = hit_id {
                root.dispatcher.push(Command::ActivateSearchResult(hit_id));
                request_redraw(hwnd);
            } else {
                let app = root.app.borrow();
                app.search_status
                    .borrow_mut()
                    .replace(SmolStr::new_static(context_menu_text(
                        "尚未选择搜索结果",
                        "No search result selected",
                    )));
                request_redraw(hwnd);
            }
            0
        }
        VK_ESCAPE_KEY => {
            root.dispatcher.push(Command::CloseSearch);
            request_redraw(hwnd);
            0
        }
        _ => 0,
    }
}

pub(super) fn handle_search_lbutton_up(root: &AppRoot, hwnd: HWND, x: f32, y: f32) -> bool {
    let hit = {
        let app = root.app.borrow();
        let visible_count = app.search_bar.borrow().visible_count();
        search_bar::search_hit_test(app.viewport, visible_count, x, y)
    };
    let Some(hit) = hit else {
        return false;
    };
    match hit {
        SearchBarPointerHit::Close => {
            root.dispatcher.push(Command::CloseSearch);
        }
        SearchBarPointerHit::Row(row_index) => {
            let hit_id = {
                let app = root.app.borrow();
                let mut search = app.search_bar.borrow_mut();
                if !search.select_index(row_index) {
                    return true;
                }
                search.current_hit().map(|hit| hit.id.clone())
            };
            if let Some(hit_id) = hit_id {
                root.dispatcher.push(Command::ActivateSearchResult(hit_id));
            }
        }
    }
    request_redraw(hwnd);
    true
}
