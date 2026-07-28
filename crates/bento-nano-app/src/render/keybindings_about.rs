use super::*;

impl Renderer {
    // M1h (2026-05-29) — `draw_plugins_modal` was deleted. The plugins surface
    // moved from a gated, light-`active_theme_palette()` in-panel MODAL to an
    // always-inline §11 section of the dark scrollable Settings body, painted by
    // the M1h block inside `draw_settings_panel`'s body-paint closure (dark
    // dialog tokens, full-width Install button, plugin-card list with type
    // badge + toggle + author + description + Uninstall). Reachability is
    // unchanged: Install → `InstallPlugin` (file picker), per-card toggle →
    // `TogglePlugin(idx)`, per-card uninstall → `UninstallPlugin(idx)`.

    /// Draw the selected-stack keybindings recorder/reset modal. This is the
    /// native D2D replacement for the Tauri KeybindingsSection portal: rows
    /// come from the shared settings action catalog, current chords are read
    /// from the real config vault, and capture/reset results are rendered
    /// visibly per action.
    pub(super) fn draw_keybindings_modal(&mut self, app: &AppState) -> Result<(), RenderError> {
        use crate::business::settings::keybindings_section;
        use crate::settings_panel::{
            settings_keybinding_record_rect, settings_keybinding_reset_rect,
            settings_keybinding_row_rect, settings_keybindings_close_rect,
            settings_keybindings_modal_rect, settings_panel_shadow_rect,
        };
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
        let palette = app.active_theme_palette();
        let radius_tokens = app.active_theme_radius();
        let spacing_tokens = app.active_theme_spacing();
        let shadow_tokens = app.active_theme_shadow();
        let modal_scrim = with_alpha(palette.scrim, 0.45);
        let modal_bg = with_alpha(palette.surface, 0.98);
        let title_color = with_alpha(palette.text, 0.96);
        let label_color = with_alpha(palette.text, 0.94);
        let muted_text = with_alpha(palette.text_muted, 0.95);
        let btn_bg = with_alpha(palette.accent, 0.80);
        let btn_disabled_bg = with_alpha(palette.surface_alt, 0.78);
        let chip_bg = with_alpha(palette.surface_alt, 0.96);
        let success_text = with_alpha(palette.success, 0.95);
        let error_text = with_alpha(palette.danger, 0.95);
        let modal_radius = radius_tokens.xl;
        let control_radius = radius_tokens.md;
        let panel_shadow = shadow_tokens.lg;
        let title_pad_x = spacing_tokens.xl;
        let title_pad_y = spacing_tokens.lg;
        let control_pad_x = spacing_tokens.md;
        let control_pad_y = spacing_tokens.xs + 1.0;
        let close_pad_x = (spacing_tokens.lg - spacing_tokens.xs).max(0.0);
        let control_text_rect = |rect: Rect| Rect {
            x: rect.x + control_pad_x,
            y: rect.y + control_pad_y,
            width: (rect.width - control_pad_x * 2.0).max(0.0),
            height: (rect.height - control_pad_y * 2.0).max(0.0),
        };

        let viewport = app.viewport;
        let scrim_rect = bento_nano_style::Rect {
            x: 0.0,
            y: 0.0,
            width: viewport.width,
            height: viewport.height,
        };
        self.fill_rounded_rect(scrim_rect, modal_scrim, BorderRadius::ZERO)?;

        let modal = settings_keybindings_modal_rect(viewport);
        let modal_shadow_rect = settings_panel_shadow_rect(modal, panel_shadow);
        self.fill_rounded_rect(modal_shadow_rect, panel_shadow.color, modal_radius)?;
        self.fill_rounded_rect(modal, modal_bg, modal_radius)?;

        let title_rect = bento_nano_style::Rect {
            x: modal.x + title_pad_x,
            y: modal.y + title_pad_y,
            width: modal.width - title_pad_x * 2.0,
            height: 24.0,
        };
        // M6c — keybindings modal title (`h2` panel header).
        self.draw_text_chromatic_title(
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::KEYBINDINGS_TITLE),
            title_rect,
            title_color,
            app.active_theme_effect_tauri(),
        )?;
        let close_rect = settings_keybindings_close_rect(viewport);
        self.fill_rounded_rect(close_rect, btn_bg, control_radius)?;
        self.draw_text(
            "×",
            bento_nano_style::Rect {
                x: close_rect.x + close_pad_x,
                y: close_rect.y + spacing_tokens.xs,
                width: (close_rect.width - close_pad_x * 2.0).max(0.0),
                height: (close_rect.height - spacing_tokens.sm).max(0.0),
            },
            title_color,
        )?;

        let recording = app.settings_keybinding_recording.borrow().clone();
        let feedback = app.settings_keybinding_feedback.borrow().clone();
        for (row_index, row) in keybindings_section::keybinding_rows().iter().enumerate() {
            let row_rect = settings_keybinding_row_rect(viewport, row_index);
            let record_rect = settings_keybinding_record_rect(viewport, row_index);
            let reset_rect = settings_keybinding_reset_rect(viewport, row_index);
            let recording_this = recording.as_deref() == Some(row.action);
            let recording_other = recording.is_some() && !recording_this;

            let label_rect = bento_nano_style::Rect {
                x: row_rect.x,
                y: row_rect.y + spacing_tokens.xs,
                width: 138.0,
                height: 16.0,
            };
            self.draw_text(row.localized_label(zh), label_rect, label_color)?;

            let chip_rect = bento_nano_style::Rect {
                x: row_rect.x + 146.0,
                y: row_rect.y + spacing_tokens.xs,
                width: 116.0,
                height: 22.0,
            };
            self.fill_rounded_rect(chip_rect, chip_bg, control_radius)?;
            let chord = if recording_this {
                smol_str::SmolStr::new(bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::KEYBINDINGS_RECORDING,
                ))
            } else {
                keybindings_section::current_chord_for_action(row.action).unwrap_or_else(|| {
                    smol_str::SmolStr::new(bento_nano_style::t(
                        bento_nano_style::i18n_zh_cn::ids::KEYBINDINGS_UNSUPPORTED,
                    ))
                })
            };
            self.draw_text(
                chord.as_str(),
                control_text_rect(chip_rect),
                if recording_this {
                    success_text
                } else {
                    muted_text
                },
            )?;

            self.fill_rounded_rect(
                record_rect,
                if recording_other {
                    btn_disabled_bg
                } else {
                    btn_bg
                },
                control_radius,
            )?;
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::KEYBINDINGS_RECORD),
                control_text_rect(record_rect),
                if recording_other {
                    muted_text
                } else {
                    title_color
                },
            )?;
            self.fill_rounded_rect(reset_rect, btn_bg, control_radius)?;
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::KEYBINDINGS_RESET),
                control_text_rect(reset_rect),
                title_color,
            )?;

            if let Some(active_feedback) =
                feedback.as_ref().filter(|msg| msg.action() == row.action)
            {
                let feedback_rect = bento_nano_style::Rect {
                    x: row_rect.x,
                    y: row_rect.y + 18.0,
                    width: row_rect.width - 132.0,
                    height: 10.0,
                };
                self.draw_text(
                    active_feedback.message(),
                    feedback_rect,
                    if active_feedback.is_error() {
                        error_text
                    } else {
                        success_text
                    },
                )?;
            }
        }

        Ok(())
    }

    /// Decode one compiled-in About image once per renderer/device generation
    /// and draw it without a filesystem or network dependency.
    pub(super) fn draw_embedded_about_image(
        &mut self,
        cache_key: &str,
        bytes: &[u8],
        role: &'static str,
        rect: bento_nano_style::Rect,
    ) -> Result<(), RenderError> {
        if rect.width <= 0.0 || rect.height <= 0.0 || self.image_file_failures.contains(cache_key) {
            return Ok(());
        }

        if !self.image_file_bitmaps.contains_key(cache_key) {
            let Some(surface) = self.surface.as_ref() else {
                return Ok(());
            };
            match d2d::bitmap_from_image_bytes(&surface.ctx, bytes) {
                Ok(bitmap) => {
                    let _ = self.image_file_bitmaps.insert(cache_key.to_owned(), bitmap);
                }
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::render::about",
                        image_role = role,
                        error = %error,
                        "failed to decode compiled-in About image"
                    );
                    let _ = self.image_file_failures.insert(cache_key.to_owned());
                    return Ok(());
                }
            }
        }

        let Some(bitmap) = self.image_file_bitmaps.get(cache_key).cloned() else {
            return Ok(());
        };
        let destination = D2D_RECT_F {
            left: rect.x,
            top: rect.y,
            right: rect.right(),
            bottom: rect.bottom(),
        };
        let Some(surface) = self.surface.as_ref() else {
            return Ok(());
        };
        d2d::draw_bitmap(&surface.ctx, &bitmap, destination, 1.0)?;
        Ok(())
    }

    pub(super) fn draw_about_app_icon(
        &mut self,
        rect: bento_nano_style::Rect,
    ) -> Result<(), RenderError> {
        self.draw_embedded_about_image(
            "embedded:about-app-icon",
            include_bytes!("../../assets/app-icon.png"),
            "app-icon",
            rect,
        )
    }

    pub(super) fn draw_about_avatar(
        &mut self,
        rect: bento_nano_style::Rect,
    ) -> Result<(), RenderError> {
        self.draw_embedded_about_image(
            "embedded:about-author-avatar",
            include_bytes!("../../assets/about-avatar.png"),
            "author-avatar",
            rect,
        )
    }

    /// Draw the selected-stack About window as a complete native product
    /// surface: identity, author, version, stack, design principles and a real
    /// GitHub action. The opaque fallback intentionally avoids the old fuzzy
    /// shadow/transparent halo on hosts where acrylic is unavailable.
    pub(super) fn draw_about_panel(&mut self, app: &AppState) -> Result<(), RenderError> {
        use crate::business::about;

        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
        let palette = app.active_theme_palette();
        let radius = app.active_theme_radius();
        let viewport = app.viewport;
        let panel = about::panel_rect(viewport);
        let panel_bg = with_alpha(palette.surface, 1.0);
        let card_bg = with_alpha(palette.surface_alt, 0.96);
        let button_bg = with_alpha(palette.surface_alt, 0.98);
        let border = with_alpha(palette.border, 0.78);
        let accent_border = with_alpha(palette.accent, 0.58);
        let title = with_alpha(palette.text, 1.0);
        let body = with_alpha(palette.text, 0.94);
        let muted = with_alpha(palette.text_muted, 0.94);
        let accent = with_alpha(palette.accent, 1.0);

        self.fill_rounded_rect(panel, panel_bg, radius.xl)?;
        self.stroke_rounded_rect(panel, border, radius.xl, 1.0)?;

        let app_icon_frame = about::app_icon_rect(viewport);
        self.fill_rounded_rect(app_icon_frame, card_bg, radius.lg)?;
        self.stroke_rounded_rect(app_icon_frame, accent_border, radius.lg, 1.0)?;
        self.draw_about_app_icon(bento_nano_style::Rect {
            x: app_icon_frame.x + 6.0,
            y: app_icon_frame.y + 6.0,
            width: app_icon_frame.width - 12.0,
            height: app_icon_frame.height - 12.0,
        })?;

        let identity_x = app_icon_frame.right() + 18.0;
        let identity_w = (panel.right() - identity_x - 76.0).max(0.0);
        self.draw_text_no_wrap_with_style(
            "BentoDesk",
            bento_nano_style::Rect {
                x: identity_x,
                y: panel.y + 30.0,
                width: identity_w,
                height: 34.0,
            },
            title,
            26.0,
            700,
            1.2,
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
        )?;
        self.draw_text_no_wrap_with_style(
            if zh {
                "轻量、原生、专注的 Windows 桌面整理器"
            } else {
                "A lightweight, native Windows desktop organizer"
            },
            bento_nano_style::Rect {
                x: identity_x,
                y: panel.y + 68.0,
                width: identity_w,
                height: 22.0,
            },
            body,
            14.0,
            500,
            1.35,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            about::format_version().as_str(),
            bento_nano_style::Rect {
                x: identity_x,
                y: panel.y + 98.0,
                width: 132.0,
                height: 22.0,
            },
            accent,
            11.0,
            600,
            1.3,
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
        )?;

        let content_x = panel.x + about::CONTENT_PADDING;
        let content_w = panel.width - about::CONTENT_PADDING * 2.0;
        self.fill_rounded_rect(
            bento_nano_style::Rect {
                x: content_x,
                y: panel.y + 132.0,
                width: content_w,
                height: 1.0,
            },
            with_alpha(palette.border, 0.45),
            BorderRadius::ZERO,
        )?;
        self.draw_text_no_wrap_with_style(
            if zh {
                "为专注而整理"
            } else {
                "Organize for focus"
            },
            bento_nano_style::Rect {
                x: content_x,
                y: panel.y + 151.0,
                width: content_w,
                height: 26.0,
            },
            title,
            18.0,
            650,
            1.3,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_with_style(
            if zh {
                "用原生技术整理桌面空间，让文件、快捷方式和工作流保持清爽、可控；无需 WebView 运行时。"
            } else {
                "Organize files, shortcuts, and workflows with native Windows surfaces—without a WebView runtime."
            },
            bento_nano_style::Rect {
                x: content_x,
                y: panel.y + 183.0,
                width: content_w,
                height: 38.0,
            },
            body,
            12.5,
            400,
            1.55,
        )?;

        let card_gap = 12.0;
        let card_w = (content_w - card_gap) * 0.5;
        let feature_cards = if zh {
            [
                ("原生运行时", "Rust · Win32 · Direct2D", "code"),
                ("开源许可证", about::LICENSE_NAME, "copy"),
            ]
        } else {
            [
                ("Native runtime", "Rust · Win32 · Direct2D", "code"),
                ("Open-source license", about::LICENSE_NAME, "copy"),
            ]
        };
        for (index, (heading, detail, icon)) in feature_cards.into_iter().enumerate() {
            let card = bento_nano_style::Rect {
                x: content_x + index as f32 * (card_w + card_gap),
                y: panel.y + 232.0,
                width: card_w,
                height: 82.0,
            };
            self.fill_rounded_rect(card, card_bg, radius.md)?;
            self.stroke_rounded_rect(card, border, radius.md, 1.0)?;
            self.draw_icon_glyph(
                icon,
                bento_nano_style::Rect {
                    x: card.x + 15.0,
                    y: card.y + 16.0,
                    width: 17.0,
                    height: 17.0,
                },
                accent,
            )?;
            self.draw_text_no_wrap_with_style(
                heading,
                bento_nano_style::Rect {
                    x: card.x + 42.0,
                    y: card.y + 14.0,
                    width: card.width - 57.0,
                    height: 20.0,
                },
                title,
                13.0,
                600,
                1.3,
                dwrite::TextAlign::DEFAULT,
            )?;
            self.draw_text_no_wrap_with_style(
                detail,
                bento_nano_style::Rect {
                    x: card.x + 16.0,
                    y: card.y + 46.0,
                    width: card.width - 32.0,
                    height: 18.0,
                },
                muted,
                11.5,
                450,
                1.3,
                dwrite::TextAlign::DEFAULT,
            )?;
        }

        let project = about::project_button_rect(viewport);
        self.fill_rounded_rect(project, button_bg, radius.md)?;
        self.stroke_rounded_rect(project, accent_border, radius.md, 1.0)?;
        self.draw_icon_glyph(
            "external_link",
            bento_nano_style::Rect {
                x: project.x + 16.0,
                y: project.y + 16.0,
                width: 18.0,
                height: 18.0,
            },
            accent,
        )?;
        self.draw_text_no_wrap_with_style(
            if zh { "项目源代码" } else { "Source code" },
            bento_nano_style::Rect {
                x: project.x + 46.0,
                y: project.y + 6.0,
                width: 116.0,
                height: 18.0,
            },
            title,
            12.5,
            600,
            1.3,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            about::PROJECT_URL,
            bento_nano_style::Rect {
                x: project.x + 46.0,
                y: project.y + 25.0,
                width: project.width - 94.0,
                height: 15.0,
            },
            muted,
            10.0,
            400,
            1.25,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_icon_glyph(
            "arrow_right",
            centered_square_rect(
                bento_nano_style::Rect {
                    x: project.right() - 42.0,
                    y: project.y,
                    width: 42.0,
                    height: project.height,
                },
                14.0,
            ),
            muted,
        )?;

        let author = about::author_button_rect(viewport);
        self.fill_rounded_rect(author, card_bg, radius.md)?;
        self.stroke_rounded_rect(author, border, radius.md, 1.0)?;
        let avatar = about::author_avatar_rect(viewport);
        self.fill_rounded_rect(avatar, with_alpha(palette.surface, 1.0), radius.md)?;
        self.stroke_rounded_rect(avatar, border, radius.md, 1.0)?;
        self.draw_about_avatar(bento_nano_style::Rect {
            x: avatar.x + 2.0,
            y: avatar.y + 2.0,
            width: avatar.width - 4.0,
            height: avatar.height - 4.0,
        })?;
        let author_label = if zh {
            format!("作者 · {}", about::AUTHOR)
        } else {
            format!("Author · {}", about::AUTHOR_EN)
        };
        self.draw_text_no_wrap_with_style(
            author_label.as_str(),
            bento_nano_style::Rect {
                x: avatar.right() + 13.0,
                y: author.y + 10.0,
                width: 180.0,
                height: 20.0,
            },
            title,
            12.5,
            600,
            1.3,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            format!("GitHub {}", about::GITHUB_HANDLE).as_str(),
            bento_nano_style::Rect {
                x: avatar.right() + 13.0,
                y: author.y + 33.0,
                width: author.width - avatar.width - 72.0,
                height: 18.0,
            },
            muted,
            10.5,
            400,
            1.3,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_icon_glyph(
            "external_link",
            centered_square_rect(
                bento_nano_style::Rect {
                    x: author.right() - 42.0,
                    y: author.y,
                    width: 42.0,
                    height: author.height,
                },
                14.0,
            ),
            muted,
        )?;

        let license_summary = if zh {
            about::LICENSE_SUMMARY_ZH
        } else {
            about::LICENSE_SUMMARY_EN
        };
        self.draw_text_no_wrap_with_style(
            format!("{license_summary} · {}", about::LICENSE_NAME).as_str(),
            bento_nano_style::Rect {
                x: content_x,
                y: panel.y + 475.0,
                width: content_w,
                height: 18.0,
            },
            muted,
            9.75,
            400,
            1.25,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
        )?;

        let close = about::close_button_rect(viewport);
        self.fill_rounded_rect(close, card_bg, radius.md)?;
        self.stroke_rounded_rect(close, border, radius.md, 1.0)?;
        self.draw_icon_glyph("x", centered_square_rect(close, 14.0), title)?;
        Ok(())
    }
}
