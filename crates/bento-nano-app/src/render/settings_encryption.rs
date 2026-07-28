use super::*;
use crate::settings_panel::*;

impl Renderer {
    pub(super) fn draw_settings_encryption(
        &mut self,
        app: &AppState,
        context: SettingsRenderContext,
        scroll: f32,
        backup_flags: SettingsBodyFlags,
    ) -> Result<(), RenderError> {
        let SettingsRenderContext {
            viewport,
            body,
            palette,
            title_color,
            label_color,
            accent_on,
            btn_radius,
            caret_on,
            ..
        } = context;
        let row_visible =
            |row: Rect, body: Rect| -> bool { row.bottom() > body.y && row.y < body.bottom() };
        let controls = palette.control_palette();
        let input_box_radius = BorderRadius::all(10.0);
        // ── M7 — Encryption §10 card (`EncryptionCard.tsx`) ─────────────
        //
        // Slots BETWEEN Backup §9 and Plugins §11, matching the Tauri
        // `<BackupCard/><EncryptionCard/>` adjacency. Fixed-height card (no
        // variable rows) painted on top of the already-wired passphrase backend.
        // Controls (top→bottom): section label / OneDrive description / current-
        // mode row / 3-button mode grid (active button accent-highlighted) /
        // passphrase row (LEFT label cell + RIGHT masked input box — P4) / hint
        // line / status banner (error red / success green). The mode-button
        // geometry + the passphrase label/input rects come from the
        // `settings_encryption_*_rect` helpers (paint==hit SSoT).
        //
        // #7 fix wave 2026-06-01 — Tauri `EncryptionCard.tsx`/`.css` 1:1 parity:
        //   P1  caret BLINKS at ~530ms (`settings_now_ms` threaded in; the prior
        //       "no per-frame clock" claim was false — the shell pump keeps
        //       redrawing while a field is focused), still allocation-free (§10);
        //   P2  current-mode VALUE uses the SAME label source as the mode-button
        //       TITLES (`encryption_mode_button_title_id` → Passphrase = id 236);
        //   P3  literal ':' after the current-mode label;
        //   P4  passphrase LABEL painted left of the input;
        //   P5  inactive buttons ALWAYS stroke rgba(255,255,255,0.08) + fill
        //       rgba(255,255,255,0.04); active fill rgba(96,165,250,0.18) + #60a5fa;
        //   P6  unfocused input ALWAYS strokes rgba(255,255,255,0.12) + fills
        //       rgba(255,255,255,0.06);
        //   P7  active button TITLE stays text_primary (NOT recolored blue);
        //   P8  current-mode VALUE is bold (weight 700, `<strong>`);
        //   P11 description = text_secondary (not text_muted);
        //   P16 placeholder = text_primary @ 0.45 alpha.
        // The mask string is built once per paint into the reusable `mask_scratch`
        // buffer + the caret glyph is appended only when `caret_on` (no per-frame
        // heap alloc — §10). NEVER paints the literal passphrase.
        use crate::settings_panel::{
            SETTINGS_ENCRYPTION_MODE_COUNT, settings_encryption_current_mode_rect,
            settings_encryption_desc_rect, settings_encryption_hint_rect,
            settings_encryption_label_rect, settings_encryption_mode_button_rect,
            settings_encryption_passphrase_input_rect, settings_encryption_passphrase_label_rect,
            settings_encryption_status_rect,
        };
        use crate::state::SettingsTextField;
        // Live encryption state, read once (Copy / cheap clones) so no RefCell
        // borrow spans the fallible paint calls below (mirrors the Backup/Stealth
        // snapshot pattern).
        let enc_mode = app.encryption_mode.get();
        let enc_status_snapshot = app.settings_encryption_status.borrow().clone();
        let enc_passphrase_focused = app.passphrase_entry_active.get()
            && matches!(
                app.settings_focused_field.get(),
                SettingsTextField::Passphrase
            );
        // Masked passphrase: number of dots = scalar count of the draft. Built
        // into a reusable scratch String (cleared, never freed) so the paint
        // path stays allocation-light (§10). NEVER the literal passphrase.
        let enc_pass_len = app.passphrase_draft.borrow().chars().count().min(128);
        // The Tauri card authored white overlays for its dark default.
        // Nano derives equivalent neutral chrome from the active palette
        // so light and personality themes retain the same hierarchy.
        let enc_active_border = accent_on;
        let enc_active_fill = with_alpha(enc_active_border, 0.18);
        let enc_hover_fill = with_alpha(enc_active_border, 0.12);
        let enc_btn_base_fill = palette.neutral_overlay(0.04);
        let enc_btn_base_border = palette.neutral_overlay(0.08);
        let enc_input_fill = controls.fill;
        let enc_input_border = controls.border;
        // P11 — `.encryption-card-description` is text_secondary (#a0a0b0), 12px;
        // the 11px `.encryption-mode-sub` / `.encryption-hint` stay text_muted.
        let enc_desc_color = palette.text_secondary;
        // #7 §10 item 8 (2026-06-01) — Tauri renders `var(--color-text-muted)`
        // at FULL opacity (EncryptionCard.css:60,83); pass `text_muted` directly.
        // The prior `with_alpha(.., 0.95)` faded the mode-sub + hint ~5% extra.
        let enc_muted = palette.text_muted;

        // Section label — 设置加密 / Settings Encryption.
        let enc_label = settings_encryption_label_rect(viewport, scroll, &backup_flags);
        if row_visible(enc_label, body) {
            self.draw_text_no_wrap_with_style(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_CARD_TITLE),
                enc_label,
                title_color,
                15.0,
                600,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Leading,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }
        // Description line (OneDrive sentence) — P11: 12px text_secondary.
        let enc_desc = settings_encryption_desc_rect(viewport, scroll, &backup_flags);
        if row_visible(enc_desc, body) {
            self.draw_text_with_style(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_CARD_DESC),
                enc_desc,
                enc_desc_color,
                12.0,
                400,
                1.0,
            )?;
        }
        // Current-mode row — 当前模式: <mode label>. Two draws (label + value).
        // P3 — literal ':' after the label (Tauri JSX `{...}:`); built into the
        // reusable `mask_scratch` (cleared before the passphrase mask reuses it)
        // so the colon append stays allocation-free (§10). P8 — the VALUE is bold
        // (weight 700, Tauri `<strong>`). P2 — the value uses the button-title
        // label source so it equals the active button TITLE (e.g. 自定义口令).
        let enc_current = settings_encryption_current_mode_rect(viewport, scroll, &backup_flags);
        if row_visible(enc_current, body) {
            let current_label =
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_CURRENT_MODE);
            let label_w = (if current_label.is_ascii() {
                96.0_f32
            } else {
                72.0_f32
            })
            .min(enc_current.width);
            let label_part = bento_nano_style::Rect {
                x: enc_current.x,
                y: enc_current.y,
                width: label_w,
                height: enc_current.height,
            };
            // P3 — append ':' to the localized label without a per-frame heap
            // alloc by composing into the reusable scratch buffer.
            self.mask_scratch.clear();
            self.mask_scratch.push_str(current_label);
            self.mask_scratch.push(':');
            let label_buf = core::mem::take(&mut self.mask_scratch);
            // #7 §10 item 3 (2026-06-01) — `.encryption-current` is 13px/400
            // (EncryptionCard.css:14); the colon-suffixed label half previously
            // inherited the default 16px no-wrap format. The VALUE half below
            // already paints at 13/700.
            let label_result = self.draw_text_no_wrap_with_style(
                label_buf.as_str(),
                label_part,
                label_color,
                13.0,
                400,
                1.0,
                dwrite::TextAlign::DEFAULT,
            );
            self.mask_scratch = label_buf;
            label_result?;
            let value_part = bento_nano_style::Rect {
                x: label_part.right() + 6.0,
                y: enc_current.y,
                width: (enc_current.right() - label_part.right() - 6.0).max(0.0),
                height: enc_current.height,
            };
            // P8 — bold value (weight 700, 13px). Uses the button-title source
            // (P2) so it matches the active mode button's title exactly.
            self.draw_text_with_style(
                localized_encryption_mode_button_label(enc_mode),
                value_part,
                title_color,
                13.0,
                700,
                1.0,
            )?;
        }
        // 3-button mode grid — None / DPAPI / Passphrase. Active button gets the
        // accent fill + border; inactive buttons get the neutral chip fill. Each
        // button paints a bold title + an 11px muted sub-label.
        for index in 0..SETTINGS_ENCRYPTION_MODE_COUNT {
            let btn = settings_encryption_mode_button_rect(viewport, scroll, &backup_flags, index);
            if !row_visible(btn, body) {
                continue;
            }
            let this_mode = match index {
                0 => crate::state::SettingsEncryptionMode::None,
                1 => crate::state::SettingsEncryptionMode::Dpapi,
                _ => crate::state::SettingsEncryptionMode::Passphrase,
            };
            let is_active = this_mode == enc_mode;
            let is_hovered = app.is_settings_encryption_mode_hovered(this_mode);
            // V21-N7 (2026-06-26) — Tauri `.encryption-mode-btn:hover:not(:disabled)`
            // paints `rgba(96,165,250,0.12)`. Active remains stronger: the
            // selected button keeps the 0.18 fill even under the pointer.
            // P5 — ALWAYS fill (base rgba(255,255,255,0.04) / active 96,165,250,0.18)
            // and ALWAYS stroke a 1px border (base rgba(255,255,255,0.08) / active
            // #60a5fa). #7 §10 item 6 (2026-06-01) — Tauri `.encryption-mode-btn
            // .active` only changes the border COLOR (#60a5fa); the WIDTH stays the
            // base 1px (EncryptionCard.css:32,44-46). The prior 1.5px active stroke
            // read ~50% heavier than the inactive chips — the visible delta this fixes.
            self.fill_rounded_rect(
                btn,
                settings_encryption_mode_button_fill_color(
                    is_active,
                    is_hovered,
                    enc_btn_base_fill,
                    enc_hover_fill,
                    enc_active_fill,
                ),
                btn_radius,
            )?;
            if is_active {
                self.stroke_rounded_rect(btn, enc_active_border, btn_radius, 1.0)?;
            } else {
                self.stroke_rounded_rect(btn, enc_btn_base_border, btn_radius, 1.0)?;
            }
            // Title (top line) + sub-label (bottom line) stacked inside the btn.
            let (title_id, sub_id) = match index {
                0 => (
                    bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_NONE,
                    bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_NONE_SUB,
                ),
                1 => (
                    bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_DPAPI,
                    bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_DPAPI_SUB,
                ),
                _ => (
                    bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_PASSPHRASE_FULL,
                    bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_PASSPHRASE_SUB,
                ),
            };
            // #7 §10 item 7 (2026-06-01) — Tauri `.encryption-mode-btn` is
            // `padding: 10px 12px` with `gap: 4px` (EncryptionCard.css:29,36).
            // Title sits 12px from the left / 10px from the top; the sub-label
            // follows 4px below the title. The prior btn.x+6 / btn.y+4 packed the
            // text too tight to the chip edges. `SETTINGS_ENCRYPTION_BTN_ROW_H`
            // was bumped 44→52 to fit (10 + 13 + 4 + 11 + 10 ≈ 48 with rounding).
            let title_rect = bento_nano_style::Rect {
                x: btn.x + 12.0,
                y: btn.y + 10.0,
                width: (btn.width - 24.0).max(0.0),
                height: 16.0,
            };
            // P7 — the title is ALWAYS text_primary (Tauri `.encryption-mode-title`
            // has `color: inherit`, no active recolor). Activation is conveyed by
            // the fill + border only. The prior accent-blue active title was the
            // visible delta this fixes. #7 §10 item 1 — `.encryption-mode-title`
            // is `font-weight: 600; font-size: 13px` (EncryptionCard.css:53-56);
            // no explicit line-height on the title (1.0).
            self.draw_text_no_wrap_with_style(
                bento_nano_style::t(title_id),
                title_rect,
                title_color,
                13.0,
                600,
                1.0,
                dwrite::TextAlign::DEFAULT,
            )?;
            let sub_rect = bento_nano_style::Rect {
                x: btn.x + 12.0,
                y: title_rect.bottom() + 4.0,
                width: (btn.width - 24.0).max(0.0),
                height: 16.0,
            };
            // #7 §10 item 2 — `.encryption-mode-sub` is `font-size: 11px;
            // line-height: 1.3` at text_muted (EncryptionCard.css:58-62).
            self.draw_text_no_wrap_with_style(
                bento_nano_style::t(sub_id),
                sub_rect,
                enc_muted,
                11.0,
                400,
                1.3,
                dwrite::TextAlign::DEFAULT,
            )?;
        }
        // P4 — passphrase ROW left label cell (口令 / Passphrase). Tauri puts a
        // `<span>` to the LEFT of the input (`justify-content: space-between`);
        // the token (id 238) existed but was never painted. 13px title color.
        let enc_pass_label =
            settings_encryption_passphrase_label_rect(viewport, scroll, &backup_flags);
        if row_visible(enc_pass_label, body) {
            let label_text_rect = bento_nano_style::Rect {
                x: enc_pass_label.x,
                y: enc_pass_label.y + (enc_pass_label.height - 16.0) * 0.5,
                width: enc_pass_label.width,
                height: 16.0,
            };
            // #7 §10 item 4 (2026-06-01) — `.encryption-passphrase-row` is
            // `font-size: 13px` (EncryptionCard.css:64-70); the `<span>` label
            // inherits it. Previously drawn at the default 16px no-wrap format.
            self.draw_text_no_wrap_with_style(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_PASSPHRASE_LABEL),
                label_text_rect,
                title_color,
                13.0,
                400,
                1.0,
                dwrite::TextAlign::DEFAULT,
            )?;
        }
        // Masked passphrase input box (RIGHT sub-rect of the row — P4). Paints
        // '•' × draft-char-count (or the placeholder when empty + not focused),
        // plus a BLINKING caret bar (P1) at the text end when focused. Never the
        // literal draft. P6 — ALWAYS stroke a 1px base border + fill; focus
        // re-strokes the accent on top.
        let enc_input = settings_encryption_passphrase_input_rect(viewport, scroll, &backup_flags);
        if row_visible(enc_input, body) {
            self.fill_rounded_rect(enc_input, enc_input_fill, input_box_radius)?;
            // P6 — base 1px border always; P1/focus — accent re-stroke on top.
            self.stroke_rounded_rect(enc_input, enc_input_border, input_box_radius, 1.0)?;
            if enc_passphrase_focused {
                self.stroke_rounded_rect(enc_input, enc_active_border, input_box_radius, 1.0)?;
            }
            // #7 §10 item 5 (2026-06-01) — Tauri input `padding: 6px 10px`
            // (EncryptionCard.css:78); the L/R inset is 10px (was 12px here).
            let text_rect = bento_nano_style::Rect {
                x: enc_input.x + 10.0,
                y: enc_input.y + (enc_input.height - 16.0) * 0.5,
                width: (enc_input.width - 20.0).max(0.0),
                height: 16.0,
            };
            if enc_pass_len == 0 && !enc_passphrase_focused {
                // P16 — placeholder at ~45% of the primary text color (Tauri
                // ::placeholder default), distinct from the live-text color.
                // #7 §10 item 5 — input text is `font-size: 12px`
                // (EncryptionCard.css:79); placeholder shares it.
                self.draw_text_no_wrap_with_style(
                    bento_nano_style::t(
                        bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_PASSPHRASE_PLACEHOLDER,
                    ),
                    text_rect,
                    with_alpha(palette.text_primary, 0.45),
                    12.0,
                    400,
                    1.0,
                    dwrite::TextAlign::DEFAULT,
                )?;
            } else {
                // Build the mask once into the reusable scratch buffer (cleared,
                // not freed → allocation-light per §10). U+2022 BULLET.
                self.mask_scratch.clear();
                for _ in 0..enc_pass_len {
                    self.mask_scratch.push('\u{2022}');
                }
                // P1 — append the caret glyph ONLY on the ON half of the blink
                // (gated by `caret_on`); on the OFF half it's omitted so the caret
                // visibly blinks at the Windows ~530ms cadence.
                if enc_passphrase_focused && caret_on {
                    self.mask_scratch.push('\u{2502}'); // U+2502 BOX DRAWINGS LIGHT VERTICAL
                }
                // Clone-free: pass a &str slice of the scratch buffer. The draw
                // call copies into its own utf16 scratch, so the borrow is short.
                // #7 §10 item 5 — masked text is the input's 12px/400 (CSS:79).
                let masked = core::mem::take(&mut self.mask_scratch);
                let draw_result = self.draw_text_no_wrap_with_style(
                    masked.as_str(),
                    text_rect,
                    title_color,
                    12.0,
                    400,
                    1.0,
                    dwrite::TextAlign::DEFAULT,
                );
                self.mask_scratch = masked;
                draw_result?;
            }
        }
        // Hint line — never-stored sentence, 11px muted.
        let enc_hint = settings_encryption_hint_rect(viewport, scroll, &backup_flags);
        if row_visible(enc_hint, body) {
            self.draw_text_with_style(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_PASSPHRASE_HINT),
                enc_hint,
                enc_muted,
                11.0,
                400,
                1.0,
            )?;
        }
        // Status banner — painted only when a status is set. Error → red,
        // Success → green (Tauri `#f87171` / `#34d399`).
        if let Some(status) = enc_status_snapshot.as_ref() {
            let enc_status_row = settings_encryption_status_rect(viewport, scroll, &backup_flags);
            if row_visible(enc_status_row, body) {
                let (text, color) = match status {
                    crate::state::SettingsBackupStatus::Error(msg) => {
                        (msg.as_str(), with_alpha(palette.accent_red, 0.95))
                    }
                    crate::state::SettingsBackupStatus::Success(msg) => {
                        (msg.as_str(), with_alpha(palette.accent_green, 0.95))
                    }
                };
                self.draw_settings_text(text, enc_status_row, color)?;
            }
        }
        Ok(())
    }
}
