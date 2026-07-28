use super::*;
use crate::settings_panel::*;

impl Renderer {
    pub(super) fn draw_settings_plugins(
        &mut self,
        app: &AppState,
        context: SettingsRenderContext,
        scroll: f32,
        backup_flags: SettingsBodyFlags,
    ) -> Result<SettingsBodyFlags, RenderError> {
        let SettingsRenderContext {
            viewport,
            body,
            palette,
            title_color,
            label_color,
            accent_on,
            track_off,
            chip_bg,
            chip_border,
            chip_radius,
            btn_radius,
            ..
        } = context;
        let row_visible =
            |row: Rect, body: Rect| -> bool { row.bottom() > body.y && row.y < body.bottom() };
        let controls = palette.control_palette();
        // ── M1h — Plugins §11 section (`SettingsPanel.tsx:709-781`) ──────
        //
        // Sits after the Encryption §10 card in the Tauri body order
        // (…→Backup→**Encryption**→Plugins→footer). M7 (2026-06-01) re-anchored
        // `settings_plugins_label_rect` off the encryption card's status row, so
        // this paint follows the encryption block automatically. Reads the live
        // `app.settings_plugin_entries` snapshot (populated on Settings open +
        // after every install/toggle/uninstall by the shell). The list is
        // variable-length, capped at SETTINGS_PLUGINS_ROW_VISIBLE_MAX; the
        // capped count threads through the same `SettingsBodyFlags` the
        // hit-tester + scroll-clamp use (via `with_plugin_rows`) so paint and
        // hit geometry agree. PURE view-model helpers (badge id, visible cap,
        // empty predicate, header text) come from
        // `business::settings::plugins_section`. Dark dialog tokens only — the
        // old modal's light `active_theme_palette()` was dropped.
        use crate::business::settings::plugins_section as plg;
        use crate::settings_panel::{
            SETTINGS_PLUGINS_ROW_VISIBLE_MAX, settings_plugin_author_rect,
            settings_plugin_badge_rect, settings_plugin_card_rect, settings_plugin_desc_rect,
            settings_plugin_empty_row_rect, settings_plugin_name_rect, settings_plugin_status_rect,
            settings_plugin_toggle_hit_rect, settings_plugin_uninstall_button_rect,
            settings_plugin_uninstall_cancel_button_rect, settings_plugins_install_button_rect,
            settings_plugins_label_rect,
        };
        // Snapshot the entries out of the RefCell BEFORE the fallible paint
        // calls so no borrow spans them (mirrors the Backup/Stealth pattern).
        let plugin_entries = app.settings_plugin_entries.borrow().clone();
        let plugin_status_snapshot = app.settings_plugin_status.borrow().clone();
        let plugin_uninstall_confirm = app.settings_plugin_uninstall_confirm.get();
        let plugin_visible = plg::plugin_visible_row_count(&plugin_entries);
        let plugin_flags = backup_flags
            .with_plugin_rows(plugin_visible)
            .with_plugin_status(plugin_status_snapshot.is_some());
        // Group title — 插件 / Plugins (reuses SETTINGS_PLUGINS id 36).
        let plugin_label = settings_plugins_label_rect(viewport, scroll, &plugin_flags);
        if row_visible(plugin_label, body) {
            self.draw_settings_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_PLUGINS),
                plugin_label,
                label_color,
            )?;
        }
        // Full-width 安装插件... button (neutral chip) → InstallPlugin.
        let plugin_install = settings_plugins_install_button_rect(viewport, scroll, &plugin_flags);
        if row_visible(plugin_install, body) {
            self.fill_rounded_rect(plugin_install, controls.fill, btn_radius)?;
            self.stroke_rounded_rect(plugin_install, controls.border, btn_radius, 1.0)?;
            self.draw_settings_button_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::PLUGIN_INSTALL),
                plugin_install,
                label_color,
                13.0,
                500,
            )?;
        }
        if let Some(status) = plugin_status_snapshot.as_ref() {
            let status_row = settings_plugin_status_rect(viewport, scroll, &plugin_flags);
            if row_visible(status_row, body) {
                let (text, color) = match status {
                    crate::state::SettingsBackupStatus::Error(message) => {
                        (message.as_str(), with_alpha(palette.accent_red, 0.95))
                    }
                    crate::state::SettingsBackupStatus::Success(message) => {
                        (message.as_str(), with_alpha(palette.accent_green, 0.95))
                    }
                };
                self.draw_settings_text_no_wrap(text, status_row, color)?;
            }
        }
        // plugin-list — N plugin cards or one pluginEmpty placeholder.
        if plg::plugin_list_is_empty(&plugin_entries) {
            let empty_row = settings_plugin_empty_row_rect(viewport, scroll, &plugin_flags);
            if row_visible(empty_row, body) {
                self.draw_text_no_wrap_with_style(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::PLUGIN_EMPTY),
                    empty_row,
                    palette.text_muted,
                    12.0,
                    400,
                    1.0,
                    dwrite::TextAlign {
                        h: dwrite::HAlign::Center,
                        v: dwrite::VAlign::Center,
                    },
                )?;
            }
        } else {
            for (card_index, plugin) in plugin_entries
                .iter()
                .take(SETTINGS_PLUGINS_ROW_VISIBLE_MAX)
                .enumerate()
            {
                let card = settings_plugin_card_rect(viewport, scroll, &plugin_flags, card_index);
                if !row_visible(card, body) {
                    continue;
                }
                // Card surface — raised chip behind the whole card.
                self.fill_rounded_rect(card, chip_bg, chip_radius)?;
                self.stroke_rounded_rect(card, chip_border, chip_radius, 1.0)?;
                // Header — name · v{version} (left), type badge + enable toggle
                // (right). The header text is formatted once per visible card.
                let name_rect = settings_plugin_name_rect(card);
                self.draw_settings_text_no_wrap(
                    plg::format_plugin_header(plugin).as_str(),
                    name_rect,
                    title_color,
                )?;
                // Type badge — accent-tinted chip (theme=purple, widget=blue,
                // organizer=green; `SettingsPanel.css:612-625`).
                let badge_rect = settings_plugin_badge_rect(card);
                let badge_accent = match plugin.plugin_type.as_str() {
                    "widget" => palette.accent_blue,
                    "organizer" => palette.accent_green,
                    _ => palette.accent_purple,
                };
                self.fill_rounded_rect(
                    badge_rect,
                    with_alpha(badge_accent, 0.20),
                    bento_nano_style::BorderRadius::all(badge_rect.height * 0.5),
                )?;
                self.draw_settings_button_text(
                    bento_nano_style::t(plg::plugin_type_label_id(plugin.plugin_type.as_str())),
                    badge_rect,
                    with_alpha(badge_accent, 1.0),
                    11.0,
                    600,
                )?;
                // Enable toggle — accent when on, neutral track when off →
                // TogglePlugin(card_index).
                let toggle_rect = settings_plugin_toggle_hit_rect(card);
                let toggle_radius = bento_nano_style::BorderRadius::all(toggle_rect.height * 0.5);
                self.fill_rounded_rect(
                    toggle_rect,
                    if plugin.enabled { accent_on } else { track_off },
                    toggle_radius,
                )?;
                self.draw_settings_button_text(
                    if plugin.enabled {
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BTN_ON)
                    } else {
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BTN_OFF)
                    },
                    toggle_rect,
                    if plugin.enabled {
                        controls.on_accent
                    } else {
                        title_color
                    },
                    11.0,
                    600,
                )?;
                // Author line (muted).
                let author_rect = settings_plugin_author_rect(card);
                self.draw_settings_text_no_wrap(
                    plugin.author.as_str(),
                    author_rect,
                    with_alpha(palette.text_muted, 0.95),
                )?;
                // Description line (muted).
                let desc_rect = settings_plugin_desc_rect(card);
                self.draw_settings_text_no_wrap(
                    plugin.description.as_str(),
                    desc_rect,
                    with_alpha(palette.text_muted, 0.95),
                )?;
                // Actions — the first destructive click arms an inline
                // confirmation; no native dialog or intermediate window.
                let uninstall_btn = settings_plugin_uninstall_button_rect(card);
                if plugin_uninstall_confirm == Some(card_index) {
                    let cancel_btn = settings_plugin_uninstall_cancel_button_rect(card);
                    let prompt_rect = bento_nano_style::Rect {
                        x: desc_rect.x,
                        y: uninstall_btn.y,
                        width: (cancel_btn.x - desc_rect.x - 8.0).max(0.0),
                        height: uninstall_btn.height,
                    };
                    self.draw_text_no_wrap_with_style(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::PLUGIN_CONFIRM_UNINSTALL,
                        ),
                        prompt_rect,
                        with_alpha(palette.accent_red, 0.95),
                        11.0,
                        500,
                        1.0,
                        dwrite::TextAlign {
                            h: dwrite::HAlign::Leading,
                            v: dwrite::VAlign::Center,
                        },
                    )?;
                    self.fill_rounded_rect(cancel_btn, chip_bg, btn_radius)?;
                    self.stroke_rounded_rect(cancel_btn, chip_border, btn_radius, 1.0)?;
                    self.draw_settings_button_text(
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTING_CANCEL),
                        cancel_btn,
                        label_color,
                        12.0,
                        500,
                    )?;
                    self.fill_rounded_rect(
                        uninstall_btn,
                        with_alpha(palette.accent_red, 0.90),
                        btn_radius,
                    )?;
                    self.draw_settings_button_text(
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BTN_CONFIRM),
                        uninstall_btn,
                        palette.readable_text_on(palette.accent_red),
                        12.0,
                        600,
                    )?;
                } else {
                    self.fill_rounded_rect(
                        uninstall_btn,
                        with_alpha(palette.accent_red, 0.85),
                        btn_radius,
                    )?;
                    self.draw_settings_button_text(
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::PLUGIN_UNINSTALL),
                        uninstall_btn,
                        palette.readable_text_on(palette.accent_red),
                        12.0,
                        500,
                    )?;
                }
            }
        }
        Ok(plugin_flags)
    }
}
