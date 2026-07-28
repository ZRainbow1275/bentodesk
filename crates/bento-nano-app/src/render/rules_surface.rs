use super::*;

impl Renderer {
    pub(super) fn draw_rules_wizard_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
        let chrome = rules_wizard::RulesWizardChrome::from_tokens(
            app.active_theme_palette(),
            app.active_theme_radius(),
            app.active_theme_shadow(),
        );
        let viewport = app.viewport;
        let panel = rules_wizard::rules_wizard_panel_rect(viewport);
        let shadow_rect = rules_wizard::rules_wizard_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        // M6c — rules wizard panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "自动整理规则"
            } else {
                "Automation Rules"
            },
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: panel.width - 36.0,
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        self.draw_text(
            if zh {
                "按步骤设置条件与操作；完成后可预览、保存或运行规则。"
            } else {
                "Configure conditions and actions step by step, then preview, save, or run."
            },
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 50.0,
                width: panel.width - 36.0,
                height: 24.0,
            },
            chrome.muted_color,
        )?;

        let wizard = app.rules_wizard.borrow();
        let rules = app.rules_wizard_rules.borrow();
        let cursor = app.rules_wizard_rule_cursor.get();
        let rule_window_start =
            rules_wizard::rules_wizard_visible_rule_window_start(cursor, rules.len());
        let rule_window_summary = localized_visible_range(
            rule_window_start,
            rules.len(),
            rules_wizard::RUNTIME_VISIBLE_RULE_LIMIT,
            zh,
        );
        let status = app.rules_wizard_status.borrow().clone();
        let step = wizard.step();
        let step_line = smol_str::SmolStr::new(if zh {
            format!(
                "步骤 {}/{}　· {}　· {}　· {}",
                step.index(),
                WizardStep::TOTAL,
                wizard_step_label(step, true),
                if wizard.is_complete() {
                    "已完成"
                } else {
                    "编辑中"
                },
                if wizard.enabled() {
                    "已启用"
                } else {
                    "已停用"
                }
            )
        } else {
            format!(
                "Step {}/{} · {} · {} · {}",
                step.index(),
                WizardStep::TOTAL,
                wizard_step_label(step, false),
                if wizard.is_complete() {
                    "Complete"
                } else {
                    "Editing"
                },
                if wizard.enabled() {
                    "Enabled"
                } else {
                    "Disabled"
                }
            )
        });
        self.draw_text(
            step_line.as_str(),
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 82.0,
                width: panel.width - 36.0,
                height: 24.0,
            },
            chrome.body_color,
        )?;

        let base_status_text = if let Some(error) = wizard.last_error() {
            smol_str::SmolStr::new(if zh {
                format!("错误：{error}")
            } else {
                format!("Error: {error}")
            })
        } else if let Some(status) = status {
            status
        } else {
            smol_str::SmolStr::new(if zh {
                format!("已载入 {} 条规则", rules.len())
            } else {
                format!("Loaded {} saved rules", rules.len())
            })
        };
        let status_text = if let Some(summary) = rule_window_summary {
            smol_str::SmolStr::new(format!("{base_status_text} — {summary}"))
        } else {
            base_status_text
        };
        self.draw_text(
            status_text.as_str(),
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 108.0,
                width: panel.width - 36.0,
                height: 24.0,
            },
            if wizard.last_error().is_some() {
                chrome.error_color
            } else {
                chrome.muted_color
            },
        )?;

        let has_saved_rules = !rules.is_empty();
        let has_conditions = !wizard.conditions().is_empty();
        let action_palette = app.active_theme_tauri();
        for spec in rules_wizard::RULES_WIZARD_ACTION_BUTTONS {
            let rect = rules_wizard::rules_wizard_button_rect(viewport, *spec);
            let enabled = match spec.hit {
                rules_wizard::RulesWizardPointerHit::Edit
                | rules_wizard::RulesWizardPointerHit::Run
                | rules_wizard::RulesWizardPointerHit::Delete => has_saved_rules,
                rules_wizard::RulesWizardPointerHit::RemoveCondition
                | rules_wizard::RulesWizardPointerHit::NextCondition => has_conditions,
                _ => true,
            };
            let emphasis = if !enabled {
                AuxiliaryActionEmphasis::Disabled
            } else {
                match spec.hit {
                    rules_wizard::RulesWizardPointerHit::NextSave => {
                        AuxiliaryActionEmphasis::Primary
                    }
                    rules_wizard::RulesWizardPointerHit::Delete => AuxiliaryActionEmphasis::Danger,
                    _ => AuxiliaryActionEmphasis::Secondary,
                }
            };
            let action = auxiliary_action_chrome(action_palette, emphasis);
            self.fill_rounded_rect(rect, action.fill, chrome.button_radius)?;
            self.stroke_rounded_rect(rect, action.border, chrome.button_radius, 1.0)?;
            self.draw_text_no_wrap(
                rules_action_label(spec.hit, step, zh),
                bento_nano_style::Rect {
                    x: rect.x + 6.0,
                    y: rect.y + 5.0,
                    width: rect.width - 12.0,
                    height: 16.0,
                },
                action.text,
            )?;
        }

        let form_x = panel.x + 18.0;
        let list_x = panel.x + panel.width * 0.54;
        let top = panel.y + rules_wizard::RUNTIME_FORM_TOP_PX;
        let form_w = (panel.width * 0.50).max(260.0);
        let list_w = panel.width - (list_x - panel.x) - 18.0;
        let condition_index = wizard.condition_cursor();
        let condition_count = wizard.conditions().len();
        let condition_window_start = rules_wizard::rules_wizard_visible_condition_window_start(
            condition_index,
            condition_count,
        );
        let condition_window_summary = localized_visible_range(
            condition_window_start,
            condition_count,
            rules_wizard::RUNTIME_VISIBLE_CONDITION_LIMIT,
            zh,
        );
        let action = wizard.action();
        let action_text = smol_str::SmolStr::new(if zh {
            format!(
                "执行：{}　· {}",
                action_label(action.kind, true),
                if action.value.trim().is_empty() {
                    "请填写目标"
                } else {
                    action.value.as_str()
                }
            )
        } else {
            format!(
                "Action: {} · {}",
                action_label(action.kind, false),
                if action.value.trim().is_empty() {
                    "Enter a target"
                } else {
                    action.value.as_str()
                }
            )
        });
        let name_text = smol_str::SmolStr::new(if zh {
            format!(
                "名称：{}",
                if wizard.name().trim().is_empty() {
                    "请填写规则名称"
                } else {
                    wizard.name()
                }
            )
        } else {
            format!(
                "Name: {}",
                if wizard.name().trim().is_empty() {
                    "Enter a rule name"
                } else {
                    wizard.name()
                }
            )
        });
        let run_text = smol_str::SmolStr::new(if zh {
            format!(
                "运行方式：{}　· 每 {} 分钟",
                run_mode_label(wizard.run_mode(), true),
                wizard.interval_minutes()
            )
        } else {
            format!(
                "Run: {} · every {} min",
                run_mode_label(wizard.run_mode(), false),
                wizard.interval_minutes()
            )
        });
        let preview_text = smol_str::SmolStr::new(if zh {
            if wizard.preview_busy() {
                "预览：正在计算…".to_owned()
            } else {
                format!("预览：命中 {} 项", wizard.preview_hits().len())
            }
        } else if wizard.preview_busy() {
            "Preview: calculating…".to_owned()
        } else {
            format!("Preview: {} matches", wizard.preview_hits().len())
        });

        let conditions_heading = if let Some(summary) = condition_window_summary {
            smol_str::SmolStr::new(format!(
                "{} [{}] — {summary}",
                if zh { "条件" } else { "Conditions" },
                combine_label(wizard.combine(), zh)
            ))
        } else {
            smol_str::SmolStr::new(format!(
                "{} [{}]",
                if zh { "条件" } else { "Conditions" },
                combine_label(wizard.combine(), zh)
            ))
        };
        self.draw_text(
            conditions_heading.as_str(),
            bento_nano_style::Rect {
                x: form_x,
                y: top,
                width: form_w,
                height: 24.0,
            },
            chrome.title_color,
        )?;
        if condition_count == 0 {
            self.draw_text(
                if zh {
                    "尚未添加条件"
                } else {
                    "No conditions"
                },
                bento_nano_style::Rect {
                    x: form_x,
                    y: top + 32.0,
                    width: form_w,
                    height: 22.0,
                },
                chrome.muted_color,
            )?;
        } else {
            for (display_index, row_index) in (condition_window_start
                ..condition_count
                    .min(condition_window_start + rules_wizard::RUNTIME_VISIBLE_CONDITION_LIMIT))
                .enumerate()
            {
                let Some(row) = wizard.conditions().get(row_index) else {
                    continue;
                };
                let rect = rules_wizard::rules_wizard_condition_row_rect(viewport, display_index);
                let selected = row_index == condition_index.min(condition_count.saturating_sub(1));
                self.fill_rounded_rect(
                    rect,
                    if selected {
                        chrome.selected_background
                    } else {
                        chrome.row_background
                    },
                    chrome.row_radius,
                )?;
                let text = smol_str::SmolStr::new(format!(
                    "{} {}. {} · {}",
                    if selected { "›" } else { " " },
                    row_index + 1,
                    predicate_label(row.kind, zh),
                    if row.value.trim().is_empty() {
                        if zh {
                            "请填写条件值"
                        } else {
                            "Enter a value"
                        }
                    } else {
                        row.value.as_str()
                    }
                ));
                self.draw_text(
                    text.as_str(),
                    bento_nano_style::Rect {
                        x: rect.x + 10.0,
                        y: rect.y + 4.0,
                        width: rect.width - 20.0,
                        height: 16.0,
                    },
                    chrome.body_color,
                )?;
            }
        }

        let detail_top = top
            + 44.0
            + rules_wizard::RUNTIME_VISIBLE_CONDITION_LIMIT as f32
                * rules_wizard::RUNTIME_CONDITION_ROW_STRIDE_PX;
        for (idx, line) in [
            action_text.as_str(),
            preview_text.as_str(),
            name_text.as_str(),
            run_text.as_str(),
        ]
        .iter()
        .enumerate()
        {
            self.draw_text(
                line,
                bento_nano_style::Rect {
                    x: form_x,
                    y: detail_top + idx as f32 * 24.0,
                    width: form_w,
                    height: 20.0,
                },
                chrome.body_color,
            )?;
        }

        self.draw_text(
            if zh { "已保存规则" } else { "Saved rules" },
            bento_nano_style::Rect {
                x: list_x,
                y: top,
                width: list_w,
                height: 24.0,
            },
            chrome.title_color,
        )?;
        if rules.is_empty() {
            self.draw_text(
                if zh {
                    "暂无已保存规则。完成左侧步骤后选择“下一步/保存”。"
                } else {
                    "No rules saved yet. Complete the steps and select Next/Save."
                },
                bento_nano_style::Rect {
                    x: list_x,
                    y: top + 32.0,
                    width: list_w,
                    height: 42.0,
                },
                chrome.muted_color,
            )?;
        } else {
            for (display_index, rule) in rules
                .iter()
                .skip(rule_window_start)
                .take(rules_wizard::RUNTIME_VISIBLE_RULE_LIMIT)
                .enumerate()
            {
                let index = rule_window_start + display_index;
                let row = rules_wizard::rules_wizard_rule_row_rect(viewport, display_index);
                let selected = index == cursor.min(rules.len().saturating_sub(1));
                self.fill_rounded_rect(
                    row,
                    if selected {
                        chrome.selected_background
                    } else {
                        chrome.row_background
                    },
                    chrome.row_radius,
                )?;
                let text = smol_str::SmolStr::new(if zh {
                    format!(
                        "{} {}　· {}",
                        if selected { "›" } else { " " },
                        rule.name,
                        if rule.enabled {
                            "已启用"
                        } else {
                            "已停用"
                        }
                    )
                } else {
                    format!(
                        "{} {} · {}",
                        if selected { "›" } else { " " },
                        rule.name,
                        if rule.enabled { "Enabled" } else { "Disabled" }
                    )
                });
                self.draw_text(
                    text.as_str(),
                    bento_nano_style::Rect {
                        x: row.x + 10.0,
                        y: row.y + 6.0,
                        width: row.width - 20.0,
                        height: 18.0,
                    },
                    chrome.body_color,
                )?;
            }
        }

        for (index, hit) in wizard.preview_hits().iter().take(4).enumerate() {
            let line = rules_preview_hit_label(hit, index, zh);
            self.draw_text(
                line.as_str(),
                bento_nano_style::Rect {
                    x: form_x,
                    y: detail_top + 104.0 + index as f32 * 22.0,
                    width: form_w,
                    height: 18.0,
                },
                chrome.muted_color,
            )?;
        }
        Ok(())
    }
}
