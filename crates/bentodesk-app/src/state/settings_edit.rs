use super::*;

impl AppState {
    /// Update the hovered expanded PanelHeader button.
    ///
    /// Returns `true` only when paint-visible hover chrome changes, letting the
    /// shell avoid redundant redraws on repeated mouse moves inside the same
    /// 28-DIP button.
    pub fn set_panel_header_button_hover(&self, hover: Option<PanelHeaderButtonHover>) -> bool {
        if self.panel_header_button_hover.get() == hover {
            return false;
        }
        self.panel_header_button_hover.set(hover);
        true
    }

    pub fn is_panel_header_button_hovered(
        &self,
        zone_id: ZoneId,
        button: PanelHeaderButtonKind,
    ) -> bool {
        self.panel_header_button_hover.get() == Some(PanelHeaderButtonHover { zone_id, button })
    }

    /// Update the hovered Settings encryption mode button.
    ///
    /// Returns `true` only when the paint-visible hover fill changes, letting
    /// the shell avoid redundant redraws on repeated mouse moves inside the
    /// same mode button.
    pub fn set_settings_encryption_mode_hover(
        &self,
        hover: Option<SettingsEncryptionMode>,
    ) -> bool {
        if self.settings_encryption_mode_hover.get() == hover {
            return false;
        }
        self.settings_encryption_mode_hover.set(hover);
        true
    }

    pub fn is_settings_encryption_mode_hovered(&self, mode: SettingsEncryptionMode) -> bool {
        self.settings_encryption_mode_hover.get() == Some(mode)
    }

    /// Update the hovered Settings Appearance ThemeCard/accent swatch.
    ///
    /// Returns `true` only when the paint-visible hover chrome changes, letting
    /// the shell avoid redundant redraws on repeated mouse moves in one card.
    pub fn set_settings_appearance_hover(
        &self,
        hover: Option<crate::theme_picker::AppearanceHit>,
    ) -> bool {
        if self.settings_appearance_hover.get() == hover {
            return false;
        }
        self.settings_appearance_hover.set(hover);
        true
    }

    pub fn is_settings_appearance_card_hovered(&self, id: u8) -> bool {
        self.settings_appearance_hover.get() == Some(crate::theme_picker::AppearanceHit::Card(id))
    }

    pub fn is_settings_appearance_accent_hovered(&self, idx: u8) -> bool {
        self.settings_appearance_hover.get()
            == Some(crate::theme_picker::AppearanceHit::Accent(idx))
    }

    pub fn is_settings_appearance_accent_editor_hovered(&self) -> bool {
        self.settings_appearance_hover.get()
            == Some(crate::theme_picker::AppearanceHit::AccentEditor)
    }

    /// Update the hovered Settings header close button.
    ///
    /// Returns `true` only when the visible hover chrome changes, mirroring the
    /// narrow Settings hover channels above.
    pub fn set_settings_close_hover(&self, hover: bool) -> bool {
        if self.settings_close_hover.get() == hover {
            return false;
        }
        self.settings_close_hover.set(hover);
        true
    }

    /// Mark zones as mutated this cycle. `consume_dispatcher` reads + clears.
    pub fn mark_dirty(&self) {
        self.dirty.set(true);
    }

    /// M1a 2026-05-29 — capture the current persisted General-section toggle
    /// values for later Cancel/Escape rollback. Shell wires this on the
    /// `OpenSettings` path so even keyboard-driven launches snapshot before
    /// the user can mutate any toggle.
    pub fn snapshot_settings(&self) -> SettingsSnapshot {
        SettingsSnapshot {
            ghost_layer_enabled: self.setting_desktop_embed.get(),
            launch_at_startup: self.setting_autostart.get(),
            show_in_taskbar: self.setting_show_in_taskbar.get(),
            auto_group_enabled: self.setting_smart_layout.get(),
            portable_mode: self.setting_portable_mode.get(),
            expand_delay_ms: self.expand_delay_ms.get(),
            collapse_delay_ms: self.collapse_delay_ms.get(),
            icon_cache_size: self.icon_cache_size.get(),
            startup_high_priority: self.startup_high_priority.get(),
            crash_restart_enabled: self.crash_restart_enabled.get(),
            crash_max_retries: self.crash_max_retries.get(),
            crash_window_secs: self.crash_window_secs.get(),
            safe_start_after_hibernation: self.safe_start_after_hibernation.get(),
            hibernate_resume_delay_ms: self.hibernate_resume_delay_ms.get(),
            active_theme_id: self.active_theme_id.borrow().clone(),
            zone_display_mode: self.zone_display_mode.get(),
            // W2 — capture the two §2 Paths drafts under the same snapshot so
            // Cancel/Escape replays them back (they're Save-gated like the
            // toggles, not immediate).
            desktop_path_draft: self.desktop_path_draft.borrow().clone(),
            watch_paths_draft: self.watch_paths_draft.borrow().clone(),
        }
    }

    /// M1a 2026-05-29 — restore each General-section toggle Cell from a
    /// snapshot. Used by Cancel/Escape/Close × so cancelled edits never leak
    /// past the in-memory panel. Caller is responsible for clearing
    /// `settings_dirty` and requesting a redraw.
    pub fn restore_settings(&self, snap: &SettingsSnapshot) {
        self.setting_desktop_embed.set(snap.ghost_layer_enabled);
        self.setting_autostart.set(snap.launch_at_startup);
        self.setting_show_in_taskbar.set(snap.show_in_taskbar);
        self.setting_smart_layout.set(snap.auto_group_enabled);
        self.setting_portable_mode.set(snap.portable_mode);
        self.expand_delay_ms.set(snap.expand_delay_ms);
        self.collapse_delay_ms.set(snap.collapse_delay_ms);
        self.icon_cache_size.set(snap.icon_cache_size);
        self.startup_high_priority.set(snap.startup_high_priority);
        self.crash_restart_enabled.set(snap.crash_restart_enabled);
        self.crash_max_retries.set(snap.crash_max_retries);
        self.crash_window_secs.set(snap.crash_window_secs);
        self.safe_start_after_hibernation
            .set(snap.safe_start_after_hibernation);
        self.hibernate_resume_delay_ms
            .set(snap.hibernate_resume_delay_ms);
        // Built-in themes can be restored entirely inside AppState. The shell
        // follows this with its loader-backed restore path so a custom JSON
        // theme is restored with the same guarantee.
        let _ = self.apply_active_theme_by_id(snap.active_theme_id.as_str());
        self.zone_display_mode.set(snap.zone_display_mode);
        // W2 — replay the two §2 Paths drafts so a mid-edit Cancel/Escape never
        // leaks the mutated path/watch values into the rest of the session.
        *self.desktop_path_draft.borrow_mut() = snap.desktop_path_draft.clone();
        *self.watch_paths_draft.borrow_mut() = snap.watch_paths_draft.clone();
    }

    /// V21-N15 — visible value for the inline Appearance accent editor. The
    /// in-flight draft wins; otherwise we show the persisted Tauri accent and
    /// finally the blue default used by the Settings preview.
    pub fn settings_accent_editor_value(&self) -> SmolStr {
        if self.settings_accent_clear_requested.get() {
            return SmolStr::new_static("#3b82f6");
        }
        self.settings_draft_accent_color
            .borrow()
            .clone()
            .or_else(|| self.theme_base_accent.borrow().clone())
            .unwrap_or_else(|| SmolStr::new_static("#3b82f6"))
    }

    /// V21-N15 — focus the inline Appearance accent editor and seed its draft
    /// from the currently displayed value so Backspace/typing edits a real
    /// field instead of a placeholder.
    pub fn focus_settings_accent_color(&self) {
        if self.settings_accent_clear_requested.replace(false) {
            *self.settings_draft_accent_color.borrow_mut() = Some(SmolStr::new_static("#3b82f6"));
        }
        if self.settings_draft_accent_color.borrow().is_none() {
            let seed = self.settings_accent_editor_value();
            *self.settings_draft_accent_color.borrow_mut() = Some(seed);
        }
        self.settings_focused_field
            .set(SettingsTextField::AccentColor);
    }

    /// V21-N16 - visible inline reset for the Appearance accent. The action is
    /// Save-gated: the persisted vault is only changed by `SaveSettings`.
    pub fn request_settings_accent_clear(&self) {
        self.settings_accent_clear_requested.set(true);
        self.settings_draft_accent_color.borrow_mut().take();
        self.settings_focused_field.set(SettingsTextField::None);
        self.settings_dirty.set(true);
    }

    /// V21-N16 — accept an OS colour-dialog result as the in-flight Appearance
    /// accent draft. Persistence remains Save-gated by the shell's
    /// `SaveSettings` path.
    pub fn set_settings_accent_color_from_picker(&self, hex: SmolStr) {
        *self.settings_draft_accent_color.borrow_mut() = Some(hex);
        self.settings_accent_clear_requested.set(false);
        self.settings_focused_field.set(SettingsTextField::None);
        self.settings_dirty.set(true);
    }

    /// V21-N15 — validated accent draft for persistence. Partial or malformed
    /// drafts stay visible/editable but are not flushed to the config vault.
    pub fn settings_valid_accent_draft(&self) -> Option<SmolStr> {
        if self.settings_accent_clear_requested.get() {
            return None;
        }
        let draft = self.settings_draft_accent_color.borrow();
        let raw = draft.as_deref()?;
        if is_valid_accent_hex(raw) {
            Some(SmolStr::new(raw))
        } else {
            None
        }
    }

    /// M7 (2026-06-01) — append a char into the focused NON-passphrase draft
    /// (桌面路径 / 监控值 / accent hex). Returns `true` when the draft changed. Append-only
    /// (type at end); rejects control chars (but `\n` is allowed for the
    /// WatchValues textarea); caps length by SCALAR-VALUE count (CJK-safe) so a
    /// multi-byte path char counts as one. Event-driven (one allocation per
    /// keystroke) — never on the per-frame paint path (§10). The `Passphrase`
    /// field is intentionally NOT handled here: it keeps its own
    /// `passphrase_draft` + commit-on-Enter flow via
    /// `handle_settings_passphrase_char`.
    pub fn settings_focused_push_char(&self, ch: char) -> bool {
        if self.settings_focused_field.get() == SettingsTextField::AccentColor {
            return self.settings_accent_push_char(ch);
        }
        let (draft, cap, allow_newline) = match self.settings_focused_field.get() {
            SettingsTextField::DesktopPath => (
                &self.desktop_path_draft,
                SETTINGS_DESKTOP_PATH_DRAFT_LIMIT,
                false,
            ),
            SettingsTextField::WatchValues => (
                &self.watch_paths_draft,
                SETTINGS_WATCH_VALUES_DRAFT_LIMIT,
                true,
            ),
            SettingsTextField::None
            | SettingsTextField::AccentColor
            | SettingsTextField::Passphrase => {
                return false;
            }
        };
        // Reject control chars — except a literal newline for the multi-line
        // WatchValues textarea (one watch path per line).
        if ch.is_control() && !(allow_newline && ch == '\n') {
            return false;
        }
        let mut current = draft.borrow_mut();
        if current.chars().count() >= cap {
            return false;
        }
        // SmolStr is immutable; rebuild once per keystroke (event-driven, §10).
        let mut next = String::with_capacity(current.len() + ch.len_utf8());
        next.push_str(current.as_str());
        next.push(ch);
        *current = SmolStr::new(next);
        true
    }

    /// M7 — backspace the focused NON-passphrase draft (pops the LAST scalar
    /// value, CJK-safe — never a partial byte). Returns `true` when the draft
    /// changed. Append-only edit model, so the caret is always at the end.
    pub fn settings_focused_backspace(&self) -> bool {
        if self.settings_focused_field.get() == SettingsTextField::AccentColor {
            let mut current = self.settings_draft_accent_color.borrow_mut();
            let Some(raw) = current.as_ref() else {
                return false;
            };
            if raw.is_empty() {
                return false;
            }
            let mut chars = raw.chars();
            chars.next_back();
            *current = Some(SmolStr::new(chars.collect::<String>()));
            return true;
        }
        let draft = match self.settings_focused_field.get() {
            SettingsTextField::DesktopPath => &self.desktop_path_draft,
            SettingsTextField::WatchValues => &self.watch_paths_draft,
            SettingsTextField::None
            | SettingsTextField::AccentColor
            | SettingsTextField::Passphrase => {
                return false;
            }
        };
        let mut current = draft.borrow_mut();
        if current.is_empty() {
            return false;
        }
        // Drop the final scalar value (chars() yields scalars, so collecting
        // all-but-last preserves multi-byte CJK correctly).
        let mut chars = current.chars();
        chars.next_back();
        let next: String = chars.collect();
        *current = SmolStr::new(next);
        true
    }

    /// M7 — caret index for the focused draft = its scalar-value count
    /// (append-only model, so the caret always sits at the end). Returns 0 for
    /// `None`/`Passphrase` (the passphrase field renders its own masked caret).
    pub fn settings_focused_caret(&self) -> usize {
        match self.settings_focused_field.get() {
            SettingsTextField::DesktopPath => self.desktop_path_draft.borrow().chars().count(),
            SettingsTextField::WatchValues => self.watch_paths_draft.borrow().chars().count(),
            SettingsTextField::AccentColor => self.settings_accent_editor_value().chars().count(),
            SettingsTextField::None | SettingsTextField::Passphrase => 0,
        }
    }

    fn settings_accent_push_char(&self, ch: char) -> bool {
        if ch.is_control() {
            return false;
        }
        let mut current = self.settings_draft_accent_color.borrow_mut();
        let raw = current.as_deref().unwrap_or("");
        if raw.chars().count() >= SETTINGS_ACCENT_COLOR_DRAFT_LIMIT {
            return false;
        }
        let mut next = String::with_capacity(raw.len() + ch.len_utf8() + 1);
        next.push_str(raw);
        if raw.is_empty() {
            if ch == '#' {
                next.push('#');
            } else if let Some(hex) = normalize_accent_hex_char(ch) {
                next.push('#');
                next.push(hex);
            } else {
                return false;
            }
        } else if let Some(hex) = normalize_accent_hex_char(ch) {
            next.push(hex);
        } else {
            return false;
        }
        self.settings_accent_clear_requested.set(false);
        *current = Some(SmolStr::new(next));
        true
    }
}
