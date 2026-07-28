//! Command handlers for the `zones` domain.

use super::*;

pub(super) fn dispatch(
    root: &AppRoot,
    _hwnd: HWND,
    command: Command,
    effects: &mut DispatchEffects,
) {
    match command {
        Command::CreateZone(spec) => {
            let mut app = root.app.borrow_mut();
            let id = app.alloc_zone_id();
            let zone = bento_nano_zone::Zone::new(
                id,
                std::borrow::Cow::Owned(spec.name.to_string()),
                spec.origin.x,
                spec.origin.y,
                spec.size.width,
                spec.size.height,
            );
            app.zones.add(zone);
            app.mark_dirty();
            effects.needs_redraw = true;
        }
        Command::DeleteZone(id) => {
            let mut app = root.app.borrow_mut();
            if app.zones.remove(id) {
                app.mark_dirty();
            }
            effects.needs_redraw = true;
        }
        Command::RenameZone(id, name) => {
            let mut app = root.app.borrow_mut();
            if let Some(z) = app.zones.get_mut(id) {
                z.title = std::borrow::Cow::Owned(name.to_string());
                app.mark_dirty();
                effects.needs_redraw = true;
            }
        }
        Command::MoveZone(id, point) => {
            let mut app = root.app.borrow_mut();
            if move_zone_live(&mut app, id, point) {
                // Phase 2.5 — cross-monitor drag clamping. The
                // monitor cache lives on `WindowSlot.state`, which we
                // can't reach without an HWND here. Per-window clamp
                // happens in the slot's WM_MOVE / WM_SIZE; the bus
                // path stays geometry-only. F2 may add a clamp here
                // once the slot routing per command is wired.
                effects.needs_redraw = true;
            }
        }
        Command::ResizeZone(id, size) => {
            let mut app = root.app.borrow_mut();
            if resize_zone_live(&mut app, id, size) {
                effects.needs_redraw = true;
            }
        }
        Command::SetZoneAlias(id, alias) => {
            let mut app = root.app.borrow_mut();
            if let Some(z) = app.zones.get_mut(id) {
                let trimmed = alias.trim();
                let next = if trimmed.is_empty() {
                    None
                } else {
                    Some(std::borrow::Cow::Owned(trimmed.to_owned()))
                };
                if z.set_alias(next) {
                    app.mark_dirty();
                    effects.needs_redraw = true;
                }
            }
        }
        Command::SetZoneIcon(id, icon) => {
            let mut app = root.app.borrow_mut();
            let normalized_icon = normalize_icon_slug(icon.as_str());
            if let Some(z) = app.zones.get_mut(id) {
                z.set_icon(std::borrow::Cow::Owned(normalized_icon.to_string()));
                if let Some(session) = app.zone_editor.borrow_mut().as_mut() {
                    if session.zone_id == id {
                        session.draft_icon = normalized_icon;
                    }
                }
                app.mark_dirty();
                effects.needs_redraw = true;
            }
        }
        Command::SetZoneAccent(id, accent) => {
            let mut app = root.app.borrow_mut();
            if let Some(z) = app.zones.get_mut(id) {
                z.set_accent_color(accent.map(|value| std::borrow::Cow::Owned(value.to_string())));
                app.mark_dirty();
                effects.needs_redraw = true;
            }
        }
        Command::SetThemeBase(accent) => match bento_nano_backend::config_vault::Vault::global() {
            Some(mtx) => match mtx.lock() {
                Ok(mut vault) => {
                    match persist_theme_base_accent_to_vault(&mut vault, accent.as_ref()) {
                        Ok(true) => {
                            let app = root.app.borrow();
                            if apply_theme_base_accent_to_app(&app, accent) {
                                effects.needs_redraw = true;
                            }
                            log_static(
                                format!(
                                    "picker: SetThemeBase persisted accent={}\n",
                                    app.theme_base_accent
                                        .borrow()
                                        .as_deref()
                                        .unwrap_or("default")
                                )
                                .as_str(),
                            );
                        }
                        Ok(false) => {}
                        Err(e) => {
                            tracing::warn!(
                                target: "bentodesk::vault",
                                error = %e,
                                "SetThemeBase flush failed; runtime accent not updated"
                            );
                        }
                    }
                }
                Err(_poisoned) => {
                    tracing::warn!(
                        target: "bentodesk::vault",
                        "SetThemeBase: vault mutex poisoned"
                    );
                }
            },
            None => {
                tracing::warn!(
                    target: "bentodesk::vault",
                    "SetThemeBase: vault not initialised"
                );
            }
        },
        Command::SetActiveTheme(theme_id) => {
            let validation_error = if active_theme_id_is_builtin(theme_id.as_str()) {
                None
            } else {
                load_theme_selection_for_root(root, theme_id.as_str()).err()
            };
            match validation_error {
                None => match bento_nano_backend::config_vault::Vault::global() {
                    Some(mtx) => match mtx.lock() {
                        Ok(mut vault) => {
                            match persist_active_theme_to_vault(&mut vault, &theme_id) {
                                Ok(true) => {
                                    // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
                                    let now_ms = unsafe { GetTickCount() };
                                    let transition_from = {
                                        let app = root.app.borrow();
                                        app.active_theme_card_id()
                                    };
                                    match apply_active_theme_to_app(root, theme_id.clone()) {
                                        Ok(changed) => {
                                            log_static(
                                                format!(
                                                    "theme: SetActiveTheme applied id={theme_id}\n"
                                                )
                                                .as_str(),
                                            );
                                            if changed {
                                                let transition_started = {
                                                    let app = root.app.borrow();
                                                    app.start_theme_transition_from(
                                                        transition_from,
                                                        now_ms,
                                                    )
                                                };
                                                if transition_started {
                                                    log_static(
                                                            format!(
                                                                "theme: transition start id={theme_id} duration_ms={}\n",
                                                                bento_nano_app::state::THEME_TRANSITION_MS
                                                            )
                                                            .as_str(),
                                                        );
                                                    request_theme_surface_redraw(root, true);
                                                } else {
                                                    request_theme_surface_redraw(root, false);
                                                }
                                            }
                                            effects.needs_redraw |= changed;
                                        }
                                        Err(error) => {
                                            set_theme_setting_error(
                                                root,
                                                localized_current(
                                                    format!("应用主题失败：{error}"),
                                                    format!("Theme apply failed: {error}"),
                                                ),
                                            );
                                            effects.needs_redraw = true;
                                        }
                                    }
                                }
                                Ok(false) => {}
                                Err(error) => {
                                    tracing::warn!(
                                        target: "bentodesk::vault",
                                        %theme_id,
                                        error = %error,
                                        "SetActiveTheme flush failed; runtime theme not updated"
                                    );
                                    set_theme_setting_error(
                                        root,
                                        localized_current(
                                            format!("保存主题失败：{error}"),
                                            format!("Theme save failed: {error}"),
                                        ),
                                    );
                                    effects.needs_redraw = true;
                                }
                            }
                        }
                        Err(_poisoned) => {
                            set_theme_setting_error(
                                root,
                                localized_current(
                                    "保存主题失败：设置存储锁不可用",
                                    "Theme save failed: settings storage lock unavailable",
                                ),
                            );
                            effects.needs_redraw = true;
                        }
                    },
                    None => {
                        set_theme_setting_error(
                            root,
                            localized_current(
                                "保存主题失败：设置存储不可用",
                                "Theme save failed: settings storage unavailable",
                            ),
                        );
                        effects.needs_redraw = true;
                    }
                },
                Some(error) => {
                    tracing::warn!(
                        target: "bentodesk::themes",
                        %theme_id,
                        error = %error,
                        "SetActiveTheme rejected"
                    );
                    set_theme_setting_error(
                        root,
                        localized_current(
                            format!("不支持的主题：{theme_id}"),
                            format!("Theme rejected: {theme_id}"),
                        ),
                    );
                    effects.needs_redraw = true;
                }
            }
        }
        Command::ImportTheme(theme_path) => {
            let source_path = PathBuf::from(theme_path.as_str());
            match import_theme_for_root(root, &source_path) {
                Ok(imported) => {
                    let imported_id = imported.id.clone();
                    match load_theme_selection_for_root(root, imported_id.as_str()) {
                        Ok((options, theme)) => {
                            let theme_name = theme.name.clone();
                            match bento_nano_backend::config_vault::Vault::global() {
                                Some(mtx) => match mtx.lock() {
                                    Ok(mut vault) => match persist_active_theme_to_vault(
                                        &mut vault,
                                        &imported_id,
                                    ) {
                                        Ok(true) => {
                                            match apply_active_theme_selection_to_app(
                                                root, options, theme,
                                            ) {
                                                Ok(changed) => {
                                                    set_theme_setting_success(
                                                        root,
                                                        localized_current(
                                                            format!("已导入主题：{theme_name}"),
                                                            format!("Theme imported: {theme_name}"),
                                                        ),
                                                    );
                                                    effects.needs_redraw |= changed;
                                                }
                                                Err(error) => {
                                                    set_theme_setting_error(
                                                        root,
                                                        localized_current(
                                                            format!("应用主题失败：{error}"),
                                                            format!("Theme apply failed: {error}"),
                                                        ),
                                                    );
                                                    effects.needs_redraw = true;
                                                }
                                            }
                                        }
                                        Ok(false) => {}
                                        Err(error) => {
                                            let app = root.app.borrow();
                                            let _changed = app.set_available_themes(options);
                                            drop(app);
                                            set_theme_setting_error(
                                                root,
                                                localized_current(
                                                    format!(
                                                        "主题已导入，但无法保存启用状态：{error}"
                                                    ),
                                                    format!(
                                                        "Theme imported; activation save failed: {error}"
                                                    ),
                                                ),
                                            );
                                            effects.needs_redraw = true;
                                        }
                                    },
                                    Err(_poisoned) => {
                                        let app = root.app.borrow();
                                        let _changed = app.set_available_themes(options);
                                        drop(app);
                                        set_theme_setting_error(
                                            root,
                                            localized_current(
                                                "主题已导入，但设置存储锁不可用",
                                                "Theme imported; activation save failed: settings storage lock unavailable",
                                            ),
                                        );
                                        effects.needs_redraw = true;
                                    }
                                },
                                None => {
                                    let app = root.app.borrow();
                                    let _changed = app.set_available_themes(options);
                                    drop(app);
                                    set_theme_setting_error(
                                        root,
                                        localized_current(
                                            "主题已导入，但设置存储不可用",
                                            "Theme imported; activation save failed: settings storage unavailable",
                                        ),
                                    );
                                    effects.needs_redraw = true;
                                }
                            }
                        }
                        Err(error) => {
                            tracing::warn!(
                                target: "bentodesk::themes",
                                %imported_id,
                                error = %error,
                                "ImportTheme copied but reload rejected imported theme"
                            );
                            set_theme_setting_error(
                                root,
                                localized_current(
                                    format!("重新载入主题失败：{error}"),
                                    format!("Theme reload failed: {error}"),
                                ),
                            );
                            effects.needs_redraw = true;
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::themes",
                        path = %theme_path,
                        error = %error,
                        "ImportTheme rejected"
                    );
                    set_theme_setting_error(
                        root,
                        localized_current(
                            format!("导入主题失败：{error}"),
                            format!("Theme import failed: {error}"),
                        ),
                    );
                    effects.needs_redraw = true;
                }
            }
        }
        Command::ListPlugins => match refresh_settings_plugins_for_root(root) {
            Ok(_changed) => {
                let count = root.app.borrow().settings_plugin_entries.borrow().len();
                root.app.borrow().settings_plugin_status.borrow_mut().take();
                log_static(format!("plugins: ListPlugins count={count}\n").as_str());
                effects.needs_redraw = true;
            }
            Err(error) => {
                tracing::warn!(
                    target: "bentodesk::plugins",
                    error = %error,
                    "ListPlugins failed"
                );
                set_plugin_setting_error(
                    root,
                    localized_plugin_message(
                        bento_nano_style::i18n_zh_cn::ids::PLUGIN_STATUS_LIST_FAILED_PREFIX,
                        error,
                    ),
                );
                effects.needs_redraw = true;
            }
        },
        Command::InstallPlugin(plugin_path) => {
            let source_path = PathBuf::from(plugin_path.as_str());
            let state_dir = state_dir_for_root(root);
            match plugins::install_from_zip(&state_dir, &source_path) {
                Ok(manifest) => match refresh_plugin_dependent_state(root) {
                    Ok(_changed) => {
                        log_static(
                            format!(
                                "plugins: InstallPlugin installed id={} name={}\n",
                                manifest.id, manifest.name
                            )
                            .as_str(),
                        );
                        set_plugin_setting_success(
                            root,
                            localized_plugin_message(
                                bento_nano_style::i18n_zh_cn::ids::PLUGIN_STATUS_INSTALLED_PREFIX,
                                manifest.name,
                            ),
                        );
                        effects.needs_redraw = true;
                    }
                    Err(error) => {
                        set_plugin_setting_error(
                                    root,
                                    localized_plugin_message(
                                        bento_nano_style::i18n_zh_cn::ids::PLUGIN_STATUS_INSTALL_FAILED_PREFIX,
                                        error,
                                    ),
                                );
                        effects.needs_redraw = true;
                    }
                },
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::plugins",
                        path = %plugin_path,
                        error = %error,
                        "InstallPlugin rejected"
                    );
                    set_plugin_setting_error(
                        root,
                        localized_plugin_message(
                            bento_nano_style::i18n_zh_cn::ids::PLUGIN_STATUS_INSTALL_FAILED_PREFIX,
                            error,
                        ),
                    );
                    effects.needs_redraw = true;
                }
            }
        }
        Command::TogglePlugin(plugin_id, enabled) => {
            let state_dir = state_dir_for_root(root);
            match plugins::toggle_enabled(plugin_id.as_str(), enabled, &state_dir) {
                Ok(plugin) => match refresh_plugin_dependent_state(root) {
                    Ok(_changed) => {
                        log_static(
                            format!("plugins: TogglePlugin id={} enabled={enabled}\n", plugin.id)
                                .as_str(),
                        );
                        set_plugin_setting_success(
                            root,
                            SmolStr::new(format!(
                                "{}{}",
                                plugin.name,
                                bento_nano_style::t(if enabled {
                                    bento_nano_style::i18n_zh_cn::ids::PLUGIN_STATUS_ENABLED_SUFFIX
                                } else {
                                    bento_nano_style::i18n_zh_cn::ids::PLUGIN_STATUS_DISABLED_SUFFIX
                                })
                            )),
                        );
                        effects.needs_redraw = true;
                    }
                    Err(error) => {
                        set_plugin_setting_error(
                                    root,
                                    localized_plugin_message(
                                        bento_nano_style::i18n_zh_cn::ids::PLUGIN_STATUS_TOGGLE_FAILED_PREFIX,
                                        error,
                                    ),
                                );
                        effects.needs_redraw = true;
                    }
                },
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::plugins",
                        %plugin_id,
                        enabled,
                        error = %error,
                        "TogglePlugin failed"
                    );
                    set_plugin_setting_error(
                        root,
                        localized_plugin_message(
                            bento_nano_style::i18n_zh_cn::ids::PLUGIN_STATUS_TOGGLE_FAILED_PREFIX,
                            error,
                        ),
                    );
                    effects.needs_redraw = true;
                }
            }
        }
        Command::UninstallPlugin(plugin_id) => {
            let state_dir = state_dir_for_root(root);
            match plugins::uninstall(plugin_id.as_str(), &state_dir) {
                Ok(()) => match refresh_plugin_dependent_state(root) {
                    Ok(_changed) => {
                        log_static(
                            format!("plugins: UninstallPlugin removed id={plugin_id}\n").as_str(),
                        );
                        set_plugin_setting_success(
                            root,
                            localized_plugin_message(
                                bento_nano_style::i18n_zh_cn::ids::PLUGIN_STATUS_REMOVED_PREFIX,
                                plugin_id,
                            ),
                        );
                        effects.needs_redraw = true;
                    }
                    Err(error) => {
                        set_plugin_setting_error(
                                    root,
                                    localized_plugin_message(
                                        bento_nano_style::i18n_zh_cn::ids::PLUGIN_STATUS_UNINSTALL_FAILED_PREFIX,
                                        error,
                                    ),
                                );
                        effects.needs_redraw = true;
                    }
                },
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::plugins",
                        %plugin_id,
                        error = %error,
                        "UninstallPlugin failed"
                    );
                    set_plugin_setting_error(
                                root,
                                localized_plugin_message(
                                    bento_nano_style::i18n_zh_cn::ids::PLUGIN_STATUS_UNINSTALL_FAILED_PREFIX,
                                    error,
                                ),
                            );
                    effects.needs_redraw = true;
                }
            }
        }
        _ => unreachable!("command routed to the wrong zones dispatcher"),
    }
}
