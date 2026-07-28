use super::*;

impl AppState {
    pub fn active_theme_palette(&self) -> PaletteTokens {
        self.active_theme_tokens.borrow().palette
    }

    /// V21-A — start the Settings dialog scale-in animation from a live shell
    /// timestamp.
    pub fn start_settings_open_animation(&self, now_ms: u32) {
        self.settings_open_started_ms.set(now_ms);
    }

    /// V21-A — normalized Settings open progress at `now_ms`.
    pub fn settings_open_animation_progress_at(&self, now_ms: u32) -> f32 {
        settings_open_animation_progress(self.settings_open_started_ms.get(), now_ms)
    }

    /// V21-A — whether the Settings open animation still needs frame pumping.
    pub fn settings_open_animation_pending_at(&self, now_ms: u32) -> bool {
        self.settings_open.get() && self.settings_open_animation_progress_at(now_ms) < 1.0
    }

    /// M6a/V21-N193 — exact active Tauri-parity palette. Tauri updates theme
    /// surface variables immediately; only Settings ThemeCard chrome animates.
    pub fn active_theme_tauri(&self) -> PaletteTauri {
        *self.active_theme_tauri.borrow()
    }

    /// Builtin ThemeCard id for the current active theme. Custom themes have no
    /// inline card and return `None`. The 17-entry scan occurs only on a theme
    /// producer, never per frame.
    pub fn active_theme_card_id(&self) -> Option<u8> {
        let active = self.active_theme_id.borrow();
        crate::theme_picker::BUILTIN_THEMES
            .iter()
            .find(|preset| preset.theme_id == active.as_str())
            .map(|preset| preset.id)
    }

    /// Selection weight for one Settings ThemeCard at `now_ms`. The previous
    /// card fades `1→0`, the active card fades `0→1`, and all other cards stay
    /// at zero. Settled cards return exactly `0` or `1`.
    pub fn theme_card_selection_progress_at(
        &self,
        card_id: u8,
        is_active: bool,
        now_ms: u32,
    ) -> f32 {
        if !self.theme_transition_active.get() {
            return if is_active { 1.0 } else { 0.0 };
        }
        let progress = theme_transition_progress(self.theme_transition_started_ms.get(), now_ms);
        if progress >= 1.0 {
            self.theme_transition_active.set(false);
            self.theme_transition_from_card.set(None);
            return if is_active { 1.0 } else { 0.0 };
        }
        let eased = theme_transition_ease(progress);
        if is_active {
            eased
        } else if self.theme_transition_from_card.get() == Some(card_id) {
            1.0 - eased
        } else {
            0.0
        }
    }

    /// Start the existing 150ms frame lifecycle for Settings selection chrome.
    /// Global theme palettes have already switched to the target. No Settings
    /// window or no card identity change means there is nothing to animate.
    ///
    /// ponytail: one previous card is enough for normal clicks; add weighted
    /// endpoints only if sub-150ms multi-click reversal is measured in practice.
    pub fn start_theme_transition_from(&self, from_card: Option<u8>, now_ms: u32) -> bool {
        let target_card = self.active_theme_card_id();
        if !self.settings_open.get() || from_card == target_card {
            self.theme_transition_from_card.set(None);
            self.theme_transition_active.set(false);
            return false;
        }
        self.theme_transition_from_card.set(from_card);
        self.theme_transition_started_ms.set(now_ms);
        self.theme_transition_active.set(true);
        true
    }

    /// Whether Settings selection chrome still needs frame pumping at `now_ms`.
    pub fn theme_transition_pending_at(&self, now_ms: u32) -> bool {
        if !self.theme_transition_active.get() {
            return false;
        }
        if !self.settings_open.get()
            || theme_transition_progress(self.theme_transition_started_ms.get(), now_ms) >= 1.0
        {
            self.theme_transition_from_card.set(None);
            self.theme_transition_active.set(false);
            return false;
        }
        true
    }

    /// M6b — the active theme's Tauri-parity radius. `Copy`, bound once per
    /// paint fn (§10). The 17 builtins return their per-theme `RadiusTauri`;
    /// custom JSON themes return the global `RADIUS`.
    pub fn active_theme_radius_tauri(&self) -> RadiusTauri {
        *self.active_theme_radius_tauri.borrow()
    }

    /// M6b — the active theme's Tauri-parity shadow stacks. `Copy`, §10.
    pub fn active_theme_shadow_tauri(&self) -> ShadowTauri {
        *self.active_theme_shadow_tauri.borrow()
    }

    /// M6b — the active theme's Tauri-parity typography (per-theme font family).
    /// `Copy`, §10.
    pub fn active_theme_typography_tauri(&self) -> TypographyTauri {
        *self.active_theme_typography_tauri.borrow()
    }

    /// M6c — the active theme's Tauri-parity effect channel. `Copy`, bound once
    /// per paint fn (§10). Returns `EffectTauri::None` for the 14 non-effect
    /// builtins + custom JSON themes; the 3 effect themes return their authored
    /// scanline/neon/chromatic descriptor.
    pub fn active_theme_effect_tauri(&self) -> EffectTauri {
        *self.active_theme_effect_tauri.borrow()
    }

    pub fn active_theme_radius(&self) -> RadiusTokens {
        self.active_theme_tokens.borrow().radius
    }

    pub fn active_theme_spacing(&self) -> SpacingTokens {
        self.active_theme_tokens.borrow().spacing
    }

    pub fn active_theme_shadow(&self) -> ShadowTokens {
        self.active_theme_tokens.borrow().shadow
    }

    pub fn active_theme_typography(&self) -> TypoTokens {
        self.active_theme_tokens.borrow().typo.clone()
    }

    pub fn apply_active_theme(&self, id: SmolStr, name: SmolStr, tokens: ThemeTokens) -> bool {
        let mut changed = false;
        // M6a — resolve the Tauri-parity palette FIRST, while `id` + `tokens`
        // are still borrowable (both are moved into their RefCells below). The
        // 17 builtins hit a byte-exact const; custom JSON themes derive off the
        // live tokens. This is the single choke-point both boot-restore and
        // live `SetActiveTheme` route through, so one resolve covers both.
        let tauri = crate::theme_bridge::resolve_palette_tauri(id.as_str(), &tokens.palette);
        // M6b — resolve the per-theme Tauri-parity radius/shadow/typography too,
        // while `id` is still borrowable. Builtins hit the per-theme const;
        // custom JSON themes fall back to the global baseline. Same choke-point
        // as the palette so boot-restore + live `SetActiveTheme` stay in sync.
        let radius_tauri =
            bentodesk_style::tokens::radius_tauri_for_theme(id.as_str()).unwrap_or(RADIUS);
        let shadow_tauri =
            bentodesk_style::tokens::shadow_tauri_for_theme(id.as_str()).unwrap_or(SHADOW);
        let typography_tauri =
            bentodesk_style::tokens::typography_tauri_for_theme(id.as_str()).unwrap_or(TYPOGRAPHY);
        // M6c — resolve the per-theme effect (scanlines/neon/chromatic) while
        // `id` is still borrowable. 3 builtins set one; everything else (incl.
        // custom JSON) falls back to `EffectTauri::None`. Family-1 only — the
        // effect does NOT fold into `ThemeTokens` (no Family-2 bridge).
        let effect_tauri = bentodesk_style::tokens::effect_tauri_for_theme(id.as_str())
            .unwrap_or(EffectTauri::None);
        {
            let mut current_id = self.active_theme_id.borrow_mut();
            if *current_id != id {
                *current_id = id;
                changed = true;
            }
        }
        {
            let mut current_name = self.active_theme_name.borrow_mut();
            if *current_name != name {
                *current_name = name;
                changed = true;
            }
        }
        {
            let mut current_tauri = self.active_theme_tauri.borrow_mut();
            if *current_tauri != tauri {
                *current_tauri = tauri;
                changed = true;
            }
        }
        {
            let mut current = self.active_theme_radius_tauri.borrow_mut();
            if *current != radius_tauri {
                *current = radius_tauri;
                changed = true;
            }
        }
        {
            let mut current = self.active_theme_shadow_tauri.borrow_mut();
            if *current != shadow_tauri {
                *current = shadow_tauri;
                changed = true;
            }
        }
        {
            let mut current = self.active_theme_typography_tauri.borrow_mut();
            if *current != typography_tauri {
                *current = typography_tauri;
                changed = true;
            }
        }
        {
            let mut current = self.active_theme_effect_tauri.borrow_mut();
            if *current != effect_tauri {
                *current = effect_tauri;
                changed = true;
            }
        }
        {
            let mut current_tokens = self.active_theme_tokens.borrow_mut();
            if *current_tokens != tokens {
                *current_tokens = tokens;
                changed = true;
            }
        }
        changed
    }

    /// M6a — apply ANY of the 17 builtin themes by id, end-to-end, without
    /// going through the shell's backend loader.
    ///
    /// Sets `active_theme_id` / `active_theme_name`, the renderer `ThemeTokens`
    /// (per-theme radius/shadow/font folded in via
    /// `theme_bridge::theme_tokens_for_theme`) and the byte-exact `PaletteTauri`
    /// together with per-theme Tauri radius/shadow/typography (resolved inside
    /// `apply_active_theme`).
    ///
    /// M6b — closes the former documented partial (the 15 non-registry themes
    /// no longer fall back to the matching-polarity DEFAULT verbatim): the
    /// polarity default is now only the *base* (palette/spacing/line-heights),
    /// onto which `theme_tokens_for_theme` folds the theme's real per-theme
    /// radius (sharp `order`/`flat`/`brutalism`), shadow (Angular `none` flat),
    /// and font family (`terminal`→Consolas, `editorial`→Georgia).
    ///
    /// Returns `Some(changed)` for a known builtin id, `None` for an unknown
    /// id (panic-free, §11 — caller decides whether to route to the custom
    /// JSON loader instead).
    pub fn apply_active_theme_by_id(&self, id: &str) -> Option<bool> {
        // Builtin-only entry point: the id must be one of the 17. The exact
        // `PaletteTauri` is re-resolved inside `apply_active_theme`.
        let tauri = bentodesk_style::tokens::palette_tauri_for_theme(id)?;
        // Renderer ThemeTokens: registry lookup first (dark/light have authored
        // token sets — byte-identical net); the remaining 15 start from the
        // matching-polarity default as the *base* (palette/spacing) and then
        // fold in per-theme radius/shadow/font via the Family-2 bridge.
        let base = THEMES
            .iter()
            .find(|(theme_id, _)| *theme_id == id)
            .map(|(_, tokens)| (*tokens).clone())
            .unwrap_or_else(|| {
                if tauri.is_dark {
                    DARK_DEFAULT.clone()
                } else {
                    LIGHT_DEFAULT.clone()
                }
            });
        let tokens = crate::theme_bridge::theme_tokens_for_theme(id, &base);
        let name = builtin_theme_display_name(id);
        Some(self.apply_active_theme(SmolStr::new(id), name, tokens))
    }

    pub fn set_available_themes(&self, themes: Vec<ThemeOption>) -> bool {
        let mut current = self.available_themes.borrow_mut();
        if *current == themes {
            return false;
        }
        *current = themes;
        true
    }

    pub fn set_settings_plugins(&self, plugins: Vec<SettingsPluginEntry>) -> bool {
        let mut current = self.settings_plugin_entries.borrow_mut();
        if *current == plugins {
            return false;
        }
        *current = plugins;
        true
    }
}
