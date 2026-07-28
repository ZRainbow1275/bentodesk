use super::*;

impl Renderer {
    pub fn create(hwnd: W_HWND, width: u32, height: u32) -> Result<Self, RenderError> {
        let comp = WindowComp::create(hwnd, width, height)?;
        // `WindowComp::create` always installs a swap chain; the only path
        // that nulls it is T-099 hibernation, which can't run during
        // construction.
        let swap = comp.swap_chain.as_ref().ok_or(RenderError::Platform(
            bento_nano_platform::PlatformError::Init(
                "Renderer::create: swap_chain missing immediately after WindowComp::create",
            ),
        ))?;
        let surface = WindowSurface::create(swap)?;
        // #19-B (2026-05-31) — resolve the UI default against the installed
        // system fonts. Tauri's CSS stack starts with "Segoe UI"; DWrite's
        // system fallback covers CJK glyphs when needed. On a stripped SKU it
        // falls back through Microsoft YaHei UI / Tahoma. ("MS Shell Dlg 2" is
        // a GDI alias DWrite's FindFamilyName cannot resolve — it would always
        // probe-miss — and the resolver's universal tail is already Tahoma, so
        // it is omitted as dead weight.)
        let ui_family: &'static str = dwrite::resolve_default_family(
            dwrite::FontRole::Ui,
            &["Segoe UI", "Microsoft YaHei UI", "Tahoma"],
        );
        let text_format = dwrite::text_format_from_family_name_with_metrics(
            ui_family,
            16.0,
            400,
            1.4,
            dwrite::locale_zh_cn(),
        )?;
        Ok(Self {
            comp,
            surface: Some(surface),
            text_format,
            text_format_family: SmolStr::new_static(ui_family),
            text_format_size_pt: 16.0,
            text_format_weight: 400,
            text_format_line_height: 1.4,
            text_format_cache: SmallVec::new(),
            main_region_installed: false,
            main_region_signature: SmallVec::new(),
            ellipsis_sign: None,
            bloom_petal_ellipsis_sign: None,
            monospace_format: None,
            monospace_ellipsis_sign: None,
            dashed_stroke_style: None,
            linear_gradient_brush: None,
            pill_title_ellipsis_sign: None,
            stack_capsule_title_shrink: None,
            width,
            height,
            utf16_scratch: SmallVec::new(),
            mask_scratch: String::new(),
            // 1.0 = 96 DPI baseline. `render()` overwrites this each frame
            // from `WindowState.dpi` before any draw call observes it.
            base_scale: 1.0,
            logical_transform_override: None,
            auxiliary_open_started_ms: None,
            icon_bitmaps: HashMap::new(),
            icon_bitmap_failures: HashSet::new(),
            image_file_bitmaps: HashMap::new(),
            image_file_failures: HashSet::new(),
            svg_cache: SvgCache::default(),
            debug_overlay_started_at: Instant::now(),
            hwnd,
            device_gen: bento_nano_platform::device_generation(),
            // Frosted-backdrop — no snapshot yet; `backdrop_dirty = true` so the
            // first Main-overlay paint captures the desktop. Brush is per-frame.
            backdrop: None,
            backdrop_dirty: true,
            backdrop_saturation: FROSTED_BACKDROP_SATURATION_DARK,
            backdrop_brush: None,
        })
    }

    /// Frosted-backdrop — mark the cached desktop snapshot stale so the next
    /// Main-overlay `render()` re-captures + re-blurs the primary work area.
    /// The shell calls this on `WM_DISPLAYCHANGE` (resolution / monitor
    /// topology), `WM_SETTINGCHANGE` (wallpaper arrives as SPI_SETDESKWALLPAPER),
    /// and the ToggleMain show transition (the desktop behind the overlay may
    /// have changed while it was hidden). Cheap flag flip — the actual capture
    /// is deferred to the paint hot path (spec §10: no capture off the frame).
    #[inline]
    pub fn mark_backdrop_dirty(&mut self) {
        self.backdrop_dirty = true;
    }

    pub fn start_auxiliary_open_animation(&mut self, now_ms: u32) {
        self.auxiliary_open_started_ms = Some(now_ms);
    }

    pub fn auxiliary_open_animation_pending(&self, now_ms: u32) -> bool {
        self.auxiliary_open_started_ms
            .is_some_and(|started| now_ms.wrapping_sub(started) < AUXILIARY_OPEN_ANIMATION_MS)
    }

    pub fn settle_auxiliary_open_animation(&mut self, now_ms: u32) -> bool {
        let Some(started) = self.auxiliary_open_started_ms else {
            return false;
        };
        if now_ms.wrapping_sub(started) < AUXILIARY_OPEN_ANIMATION_MS {
            return false;
        }
        self.auxiliary_open_started_ms = None;
        true
    }

    /// Re-create the swap chain backbuffer surface after a resize.
    pub fn resize(&mut self, w: u32, h: u32) -> Result<(), RenderError> {
        if let Some(s) = self.surface.as_mut() {
            s.release_target();
        }
        self.comp.resize(w, h)?;
        // When the chain was hibernated, ensure_chain has to be the call site
        // that recreates it — but we still re-bind the surface here so a
        // resize between hibernate-and-show keeps width/height in sync.
        if let Some(swap) = self.comp.swap_chain.as_ref() {
            self.surface = Some(WindowSurface::create(swap)?);
        } else {
            self.surface = None;
        }
        self.width = w;
        self.height = h;
        Ok(())
    }

    /// T-099 — drop the per-window backbuffer (largest per-window allocation,
    /// ~1.2 MB at 480×320×4×2). Surface and swap chain go; visual tree +
    /// DComp target stay so a subsequent `ensure_swap_chain` rebinds without
    /// re-creating the composition. Idempotent: a second call is a no-op.
    pub fn release_swap_chain(&mut self) {
        if let Some(s) = self.surface.as_mut() {
            s.release_target();
        }
        // Drop any cached backdrop with the backbuffer so a hibernated renderer
        // retains neither GPU bitmap nor brush.
        self.backdrop_brush = None;
        self.backdrop = None;
        self.backdrop_dirty = true;
        self.surface = None;
        self.comp.release_chain();
    }

    /// Recreate the backbuffer + D2D surface after `release_swap_chain`.
    /// Idempotent: returns `Ok(())` immediately if already resident.
    pub fn ensure_swap_chain(&mut self, w: u32, h: u32) -> Result<(), RenderError> {
        if self.surface.is_some() && self.comp.swap_chain.is_some() {
            return Ok(());
        }
        self.comp.ensure_chain(w.max(1), h.max(1))?;
        let swap = self.comp.swap_chain.as_ref().ok_or(RenderError::Platform(
            bento_nano_platform::PlatformError::Init(
                "Renderer::ensure_swap_chain: chain still missing after ensure_chain",
            ),
        ))?;
        self.surface = Some(WindowSurface::create(swap)?);
        self.width = w;
        self.height = h;
        Ok(())
    }

    /// Mc-2b — rebuild this window's device-derived COM after a device-lost
    /// event. PRECONDITION: the shell (Impl C chokepoint) has ALREADY called
    /// `platform::recover_device_chain()`, so the process-singleton D3D/D2D/
    /// DComp devices are fresh; this method only rebuilds the per-window objects
    /// that were bound to the dead device. If any step errors it propagates —
    /// the shell's retry cap (Impl C) handles repeated failure.
    pub fn rebuild_after_device_loss(&mut self) -> Result<(), RenderError> {
        // Drop the old D2D context + bitmap target first; both are bound to the
        // dead device and would keep it alive.
        self.surface = None;
        // Rebuild the composition (swap chain + DComp target + root visual) on
        // the recovered device. Replacing `self.comp` drops every old object.
        self.comp = WindowComp::create(self.hwnd, self.width, self.height)?;
        // Mirror `create`: bind a fresh D2D surface to the new backbuffer.
        let swap = self.comp.swap_chain.as_ref().ok_or(RenderError::Platform(
            bento_nano_platform::PlatformError::Init(
                "Renderer::rebuild_after_device_loss: swap_chain missing immediately after WindowComp::create",
            ),
        ))?;
        self.surface = Some(WindowSurface::create(swap)?);
        // Clear device-derived caches: these bitmaps/geometries were created on
        // the now-dead D2D device/factory and must be re-decoded/re-built on the
        // recovered ones. Failure entries also reset so previously-failing icons
        // get one fresh attempt against the new device.
        self.icon_bitmaps.clear();
        self.icon_bitmap_failures.clear();
        self.image_file_bitmaps.clear();
        self.image_file_failures.clear();
        self.svg_cache.clear();
        // G5 — the dashed stroke style was created from the (now-rebuilt) D2D
        // factory; drop it so the next minimal-capsule paint re-creates it
        // against the recovered factory. Cheap one-off rebuild, not per-frame.
        self.dashed_stroke_style = None;
        self.linear_gradient_brush = None;
        // KEEP DWrite-derived state untouched: `text_format`,
        // `text_format_cache`, `ellipsis_sign`, `monospace_format`,
        // `monospace_ellipsis_sign`. DWrite is GPU-INDEPENDENT (design §B / A2),
        // so these survive a device loss and never need rebuilding here.
        self.device_gen = bento_nano_platform::device_generation();
        Ok(())
    }

    /// Whether this renderer currently owns a swap chain. Diagnostics +
    /// the wndproc paint guard read this to decide if a paint should
    /// trigger `ensure_swap_chain` first.
    #[inline]
    pub fn is_resident(&self) -> bool {
        self.surface.is_some() && self.comp.swap_chain.is_some()
    }

    /// Run one frame: layout + draw + present. `win` carries the per-HWND
    /// `LayoutEngine` (cache lives there — Ruling 5 / C3).
    ///
    /// Phase 2.3.1b — `self.width / self.height` are **device pixels** (the
    /// swap chain backbuffer dimensions reported by `WM_SIZE` /
    /// `GetClientRect`). The layout engine + zone collection live in
    /// **logical** units (DIPs), so we divide by `dpi/96` once to obtain the
    /// logical viewport. A single `SetTransform(Scale)` after `BeginDraw`
    /// then projects every logical coordinate onto the right device pixel
    /// without per-call multiplication.
    pub fn render(
        &mut self,
        app: &mut AppState,
        win: &mut WindowState,
        kind: WindowKind,
    ) -> Result<(), RenderError> {
        // Mc-2b — generation self-heal. When another window hit DeviceLost and
        // the shell bumped the generation via `recover_device_chain`, this
        // renderer's device-derived COM is stale; rebuild it on this paint
        // before any draw call touches the dead device. One atomic load per
        // paint entry (§10): `present()` is reached from this single function,
        // so one check here covers both present sites below. The rebuild path
        // is cold (only runs on the first paint after a device loss).
        if renderer_is_stale(self.device_gen, bento_nano_platform::device_generation()) {
            self.rebuild_after_device_loss()?;
        }
        // §10 hot-path: read once, no allocation.
        let frame_started_at = Instant::now();
        let dpi = win.dpi.get();
        let scale = bento_nano_style::dpi::scale_factor(dpi);
        let device_size = bento_nano_style::Size {
            width: self.width as f32,
            height: self.height as f32,
        };
        // Phase 2.3.1b — viewport flipped from device-pixel to logical-DIP.
        // At 96 DPI the conversion is identity (regression-safe); at 192
        // DPI a 960×640 backbuffer becomes a 480×320 logical viewport so
        // the same layout source produces the same logical rects.
        app.viewport = bento_nano_style::dpi::device_size_to_logical(device_size, dpi);
        self.ensure_text_format_for_active_theme(app)?;
        // Phase 2.1 / Ruling A + Q2 — first-paint zone load.
        //
        // Error-class routing:
        //   Ok(list)                  → adopt the list.
        //   Err(Storage(_))           → structural corruption (bad magic /
        //                               version mismatch / truncated). Rename
        //                               the file so the user can recover it,
        //                               start empty.
        //   Err(StorageIo { kind: NotFound, .. })
        //                             → handled inside `read_zones` itself
        //                               (returns Ok(empty)); never reaches
        //                               this arm.
        //   Err(StorageIo { .. })     → access issue (permission denied,
        //                               sharing violation). DON'T rename —
        //                               the file is probably fine, we just
        //                               can't open it now. Start empty.
        //
        // Either branch flips `loaded` so the paint hot path never retries.
        if !win.loaded.get() {
            if !app.zones_path.as_os_str().is_empty() {
                match bento_nano_platform::storage::read_zones(&app.zones_path) {
                    Ok(loaded) => {
                        app.zones = loaded;
                    }
                    Err(bento_nano_platform::PlatformError::Storage(_)) => {
                        let _ = bento_nano_platform::storage::quarantine_corrupt(&app.zones_path);
                    }
                    Err(_) => {
                        // IO / permission / other — leave the file in place.
                    }
                }
            }
            win.loaded.set(true);
        }
        // Phase 2.3.1b — record `base_scale` for the frame so SVG draw paths
        // can compose against it instead of resetting to identity.
        self.base_scale = scale;

        // Frosted-backdrop — clear the per-frame brush FIRST so an unrelated
        // auxiliary frame, a hibernated surface, or a `None` backdrop can never
        // reuse a stale brush (spec §10 / degrade ladder).
        self.backdrop_brush = None;

        // Only Main shares the exact origin and extent of the captured monitor
        // work area. Settings is a movable panel-sized native popup; sampling a
        // work-area bitmap with a zero translation there detached the wallpaper
        // from the window and produced dark/square drag artifacts. Its dense
        // theme surface is the deliberate flat-tint degradation path.
        let uses_frosted_backdrop = kind == WindowKind::Main;
        let backdrop_saturation = frosted_backdrop_saturation_for_palette(app.active_theme_tauri());
        if FROSTED_BACKDROP
            && uses_frosted_backdrop
            && frosted_backdrop_saturation_recapture_needed(
                kind,
                self.backdrop_saturation,
                backdrop_saturation,
            )
        {
            self.backdrop_dirty = true;
        }

        // Frosted-backdrop capture (real acrylic) — Main only, and only when
        // `backdrop_dirty`. This MUST run
        // before the frame's
        // `ctx.BeginDraw()` below: `capture_primary_workarea_blurred` does its
        // OWN BeginDraw/EndDraw internally to bake the blur, and BeginDraw
        // cannot be nested on one device context. The capture is on-demand —
        // steady-state frames reuse the cached bitmap (zero per-frame capture).
        // Degrade-not-panic (spec § "Degrade ladder"): on `Err` we drop the
        // backdrop to `None` (→ flat tint) and STILL clear the dirty flag so we
        // don't re-attempt a failing capture every frame; the next dirty event
        // (display / wallpaper / show) retries.
        if FROSTED_BACKDROP && uses_frosted_backdrop && self.backdrop_dirty {
            let captured = match self.surface.as_ref() {
                Some(surface) => Some(capture_primary_workarea_blurred(
                    &surface.ctx,
                    self.hwnd,
                    FROSTED_BACKDROP_DOWNSAMPLE,
                    FROSTED_BACKDROP_STDDEV,
                    backdrop_saturation,
                )),
                // Hibernated surface — leave dirty set so the next resident
                // paint captures; nothing to do this frame.
                None => None,
            };
            if let Some(result) = captured {
                self.backdrop = match result {
                    Ok(backdrop) => Some(backdrop),
                    Err(error) => {
                        tracing::warn!(
                            target: "bentodesk::render",
                            %error,
                            "frosted backdrop capture unavailable; using flat tint"
                        );
                        None
                    }
                };
                self.backdrop_saturation = backdrop_saturation;
                self.backdrop_dirty = false;
            }
        }

        // Frosted-backdrop — build the per-frame bitmap brush ONCE for Main from
        // the cached `backdrop` (spec §10: one cheap brush build
        // per frame, no capture). Done here, before the long-lived `surface` borrow
        // below, so the `&mut self.backdrop_brush` write does not race the
        // immutable `surface`/`ctx` borrow that the rest of the frame holds.
        // `CreateBitmapBrush` does not need an active `BeginDraw`. A `None`
        // backdrop / build failure leaves `backdrop_brush = None` → flat tint.
        if FROSTED_BACKDROP && uses_frosted_backdrop {
            let brush = match self.surface.as_ref() {
                Some(surface) => self.build_backdrop_brush(&surface.ctx),
                None => None,
            };
            self.backdrop_brush = brush;
        }

        self.logical_transform_override = None;

        // T-099 — paint guard. When the swap chain is hibernated, return
        // `Ok(())`. The wndproc's WM_PAINT arm calls `ensure_swap_chain`
        // before paint when a window becomes visible again, so this only
        // fires for genuine "skip this frame" cases (e.g. paint queued
        // between hibernate and the next show event).
        let Some(surface) = self.surface.as_ref() else {
            return Ok(());
        };
        let ctx = &surface.ctx;

        // SAFETY: surface valid (just unwrapped); D2D draw sequence
        //         BeginDraw → ... → EndDraw, no re-entry between calls.
        unsafe {
            ctx.BeginDraw();
            let clear = D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            ctx.Clear(Some(&clear));
            // Phase 2.3.1b — single SetTransform projects the entire logical
            // coordinate space onto device pixels. Every fill / draw call
            // below this point uses logical units; D2D multiplies by `scale`
            // automatically. SVG paths re-establish the current logical
            // transform because their per-glyph transforms also need the
            // projection.
            let base = base_scale_matrix(scale);
            ctx.SetTransform(&base);
        }

        let auxiliary_open_transform_active = kind == WindowKind::IconPicker
            && self.auxiliary_open_animation_pending(unsafe {
                windows_sys::Win32::System::SystemInformation::GetTickCount()
            });
        if auxiliary_open_transform_active {
            // SAFETY: GetTickCount is total and thread-safe.
            let now_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
            let started = self.auxiliary_open_started_ms.unwrap_or(now_ms);
            let raw = now_ms.wrapping_sub(started) as f32 / AUXILIARY_OPEN_ANIMATION_MS as f32;
            let scale = 0.965 + 0.035 * animator::ease_out_cubic(raw);
            let transform = scale_about_rect_center_matrix(
                self.base_scale,
                picker_geometry::picker_panel(app.viewport),
                scale,
            );
            self.set_logical_transform_override(Some(transform))?;
        }

        let rendered_aux_window = match kind {
            WindowKind::ZoneEditor => {
                self.draw_zone_editor_window(app)?;
                true
            }
            WindowKind::ItemFileRename => {
                self.draw_item_file_rename_window(app)?;
                true
            }
            WindowKind::IconPicker => {
                self.draw_icon_picker_window(app)?;
                true
            }
            WindowKind::PalettePicker => {
                self.draw_palette_picker_window(app)?;
                true
            }
            WindowKind::CapsulePicker => {
                self.draw_capsule_picker_window(app)?;
                true
            }
            WindowKind::RulesWizard => {
                self.draw_rules_wizard_window(app)?;
                true
            }
            WindowKind::BulkManager => {
                self.draw_bulk_manager_window(app)?;
                true
            }
            WindowKind::Timeline => {
                self.draw_timeline_window(app)?;
                true
            }
            WindowKind::SnapshotPicker => {
                self.draw_snapshot_picker_window(app)?;
                true
            }
            WindowKind::Suggestor => {
                self.draw_suggestor_window(app)?;
                true
            }
            WindowKind::Search => {
                self.draw_search_window(app)?;
                true
            }
            WindowKind::MiniBar => {
                self.draw_minibar_window(app)?;
                true
            }
            WindowKind::Tooltip => {
                self.draw_tooltip_window(app)?;
                true
            }
            WindowKind::ContextMenu => {
                self.draw_context_menu_window(app)?;
                true
            }
            WindowKind::About => {
                self.draw_about_panel(app)?;
                true
            }
            WindowKind::Settings => {
                self.draw_settings_window(app)?;
                true
            }
            _ => false,
        };
        if auxiliary_open_transform_active {
            self.set_logical_transform_override(None)?;
        }
        if rendered_aux_window {
            // M6c — scanline post-pass over the aux surface (terminal theme
            // only; no-op otherwise). Tauri's `data-theme-effect` `::after` is
            // a per-document `position:fixed; inset:0` overlay, so each nano
            // HWND paints it over its own client area just before EndDraw.
            self.draw_effect_overlay(app)?;
            let end_ctx = self.ctx()?;
            // SAFETY: surface valid (guarded at the top of render); this
            // closes the auxiliary frame started by BeginDraw above.
            let end = unsafe { end_ctx.EndDraw(None, None) };
            ok("EndDraw", end)?;
            self.comp.present()?;
            return Ok(());
        }

        // Collect (id, rect) pairs into a stack-inlined buffer so the layout
        // result borrow doesn't outlive the dispatch loop (which mutably
        // borrows `self` via `draw_node`).
        let mut ids: SmallVec<[(bento_nano_tree::NodeId, bento_nano_style::Rect); 32]> =
            SmallVec::new();
        {
            let result = win.layout.layout(&app.tree, app.viewport)?;
            for (id, rect) in result.iter() {
                ids.push((*id, *rect));
            }
        }

        for (id, rect) in ids.iter() {
            let node = match app.tree.get(*id) {
                Ok(n) => n,
                Err(_) => continue,
            };
            self.draw_node(node, *rect)?;
        }
        // α5 (S2, 2026-05-24): the prior unconditional `draw_theme_base_accent`
        // call painted a 4-DIP accent strip across the full top edge of the
        // Main HWND on every frame. The Tauri 1.2.4 baseline paints no such
        // strip (grep on bentodesk@6a3b283 returns zero `theme-base` /
        // `base-accent` consumers). On the desktop overlay the strip read as
        // an ugly blue border riding above all foreground apps. The state
        // field + helper stay alive for Settings / the picker pop-up that lets
        // users pick the base accent; only the Main-HWND leak is removed.

        // Phase 2 — zones live outside the widget tree (they're a domain
        // collection, not a tree-mounted card). Render after the tree so
        // they paint on top of the toolbar card; geometry comes straight
        // from `Zone.x/y/w/h` (DIPs).
        self.draw_zones(app)?;
        self.draw_highlight_overlay(app)?;
        if !app.settings_open.get() && !app.about_open.get() {
            self.draw_stack_tray_overlay(app)?;
        }
        // Zone/item menus are transient chrome on the already-resident Main
        // surface. Reusing this renderer avoids a second DComp swap chain and
        // keeps the right-click path inside the strict private-memory budget.
        if app.active_context_menu.borrow().is_some() {
            self.draw_context_menu_window(app)?;
        }

        // Wave K1b — Settings and About each own a dedicated aux HWND (the
        // `WindowKind::Settings` / `WindowKind::About` arms above route to
        // `draw_settings_window` / `draw_about_panel`). Painting the modal a
        // second time on the Main HWND duplicates the panel chrome onto the
        // overlay (two scrims, two cards) which becomes visible after H4
        // raised both surfaces to `WS_EX_TOPMOST`. Skip the legacy Main-side
        // fallback here.
        self.poll_debug_overlay_rss(app);
        self.draw_debug_overlay(app)?;

        // M6c — scanline post-pass over the main desktop surface (terminal
        // theme only; no-op otherwise), AFTER all zones / overlays / debug so
        // the green bands ride on top of everything (`z-index:9999`).
        self.draw_effect_overlay(app)?;

        // SAFETY: surface valid (guarded at the top of this fn); EndDraw
        //         signals the end of this frame's work.
        let end_ctx = self.ctx()?;
        let end = unsafe { end_ctx.EndDraw(None, None) };
        ok("EndDraw", end)?;
        let region_precedes_present = main_region_precedes_present(
            kind,
            app.zone_drag.get().is_some(),
            app.zone_resize.get().is_some(),
        );
        if region_precedes_present {
            // Expand the input/visual clip before submitting the first moved
            // frame. Doing this after Present leaves that frame clipped to the
            // old capsule rect and produces the one-frame blank/flash seen at
            // drag latch.
            self.apply_main_click_through_region(app);
        }
        self.comp.present()?;

        // P0 click-through (CLICKTHROUGH-FIX-VALIDATED.md, 2026-06-02) — clip
        // the Main HWND's window region to the UNION of every painted
        // interactive surface. `WS_EX_TRANSPARENT` is INERT under
        // `WS_EX_NOREDIRECTIONBITMAP` (window.rs:254-256) and `HTTRANSPARENT`
        // alone does NOT reach the bare desktop, so blank pixels of the
        // full-work-area overlay otherwise eat every click. Region clipping
        // keeps DComp / `NoRedirectionBitmap` (spec §4.1) untouched: blank
        // areas fall OUTSIDE the window so clicks land on the desktop
        // natively. Main HWND only — aux dialogs are real windows that own
        // their whole client rect. Stable exact regions apply after present;
        // an active move/resize installs its full-client region before present
        // so the first moved frame cannot be clipped to stale geometry. The
        // Win32 path degrades silently (no panic).
        if kind == WindowKind::Main && !region_precedes_present {
            self.apply_main_click_through_region(app);
        }
        self.record_debug_overlay_frame(app, kind, frame_started_at);
        Ok(())
    }

    /// P0 click-through — set the Main HWND window region to the painted-chrome
    /// union so blank desktop pixels pass clicks through natively.
    ///
    /// The region rects come from [`chrome_region_rects`] (the single source of
    /// truth, mirroring `bento-nano-shell::ui::main_nchittest_kind`), are
    /// expressed in logical DIP, and are converted to PHYSICAL device px here by
    /// multiplying by `base_scale` (= dpi/96; the user runs 150% → ×1.5).
    /// `SetWindowRgn` wants device px, so this conversion MUST happen or the
    /// region misaligns at non-100% DPI.
    ///
    /// GDI lifecycle: each rect becomes a temporary `HRGN` that is OR-combined
    /// into one accumulator; the temporaries are `DeleteObject`-freed after the
    /// combine, and the FINAL accumulator is handed to `SetWindowRgn`, which
    /// TAKES OWNERSHIP (we never `DeleteObject` it; the system frees the prior
    /// region). When NOTHING is painted, an EMPTY 0×0 region is set so the WHOLE
    /// desktop is click-through — the region is NEVER left NULL (NULL = whole
    /// window catches = the original bug).
    ///
    /// Spec §10 hot path: the rect set is a stack-inlined `SmallVec<[_; 16]>`
    /// (no heap unless a process pins >16 zones), N small GDI regions (which
    /// `SetWindowRgn` requires), one `SetWindowRgn`. No `unwrap`/`expect`/`panic`
    /// — every Win32 failure degrades to leaving the previous region (the
    /// ghost-layer passthrough toggle is the belt-and-suspenders fallback).
    pub(super) fn apply_main_click_through_region(&mut self, app: &AppState) {
        // windows-sys 0.59 places ALL of these — including `SetWindowRgn`
        // (which the docs file under user32) — in `Graphics::Gdi`. Verified by
        // compile: `SetWindowRgn` is NOT in `UI::WindowsAndMessaging` here.
        use windows_sys::Win32::Graphics::Gdi::{
            CombineRgn, CreateRectRgn, DeleteObject, RGN_OR, SetWindowRgn,
        };

        // DIP → physical device px. `base_scale` is `dpi/96` (set once per
        // frame at the top of `render`); guard against a degenerate <=0 scale.
        let scale = self.base_scale.max(0.01);
        let mut signature: SmallVec<[DeviceRegionRect; 16]> = SmallVec::new();
        if app.zone_drag.get().is_some() || app.zone_resize.get().is_some() {
            // W13-B — while mouse capture owns an active move/resize, install
            // one stable full-client region. The old code rebuilt SetWindowRgn
            // after every DComp present; the moving visual was clipped by the
            // previous geometry for a frame, producing blank/blue flashes and
            // excessive GDI work. Mouse-up clears the drag state, and the next
            // paint restores the exact chrome-only region.
            if let Some(full_client) = full_client_device_region(app.viewport, scale) {
                signature.push(full_client);
            }
        } else {
            let rects = chrome_region_rects(app);
            for r in rects.iter() {
                // Convert DIP rect → physical px, rounding outward so a painted
                // surface is never under-covered (left/top floor, right/bottom
                // ceil). Clamp non-positive extents away (skip empty rects).
                let left = (r.x * scale).floor() as i32;
                let top = (r.y * scale).floor() as i32;
                let right = (r.right() * scale).ceil() as i32;
                let bottom = (r.bottom() * scale).ceil() as i32;
                if right > left && bottom > top {
                    signature.push((left, top, right, bottom));
                }
            }
        }
        if self.main_region_installed && self.main_region_signature == signature {
            return;
        }

        // Accumulator region. Start EMPTY (0×0) so the "no painted surface"
        // case leaves the whole desktop click-through. `CreateRectRgn` returns
        // a null handle on GDI failure — treat that as "skip region surgery
        // this frame" rather than panic / NULL-region (which would re-arm the
        // whole-window-catches bug).
        // SAFETY: GDI region creation is always callable; null is checked.
        let combined = unsafe { CreateRectRgn(0, 0, 0, 0) };
        if combined.is_null() {
            return;
        }

        let mut built_all_parts = true;
        for &(left, top, right, bottom) in signature.iter() {
            // SAFETY: GDI region creation is always callable; null is checked
            // so a single allocation failure just drops that one rect.
            let part = unsafe { CreateRectRgn(left, top, right, bottom) };
            if part.is_null() {
                built_all_parts = false;
                continue;
            }
            // SAFETY: `combined` and `part` are both live, non-null HRGNs;
            // RGN_OR (an `i32` = `RGN_COMBINE_MODE`) unions `part` into
            // `combined` in place.
            unsafe {
                CombineRgn(combined, combined, part, RGN_OR);
                // `part` was copied into `combined`; free the temporary HRGN.
                DeleteObject(part);
            }
        }

        // Hand the final region to the system. `SetWindowRgn` TAKES OWNERSHIP
        // of `combined` (do NOT DeleteObject it) and frees the window's prior
        // region. bRedraw = FALSE: DComp composites independently, so no
        // invalidate is needed and we avoid a redundant repaint (spec §10).
        // `self.hwnd` is a `windows` 0.58 `HWND(*mut c_void)`; `.0` is the raw
        // pointer that windows-sys 0.59 `SetWindowRgn` expects (same ABI — see
        // `bento-nano-platform::window::to_windows_hwnd`, the inverse bridge).
        // SAFETY: `self.hwnd` is the live Main HWND stashed at `create`;
        // `combined` is a valid HRGN whose ownership transfers to the system.
        // bredraw = 0 (FALSE).
        let applied = unsafe { SetWindowRgn(self.hwnd.0, combined, 0) };
        if applied != 0 {
            self.main_region_signature = signature;
            self.main_region_installed = built_all_parts;
        } else {
            // SAFETY: SetWindowRgn did not take ownership when it failed.
            unsafe {
                DeleteObject(combined);
            }
        }
    }

    pub(super) fn debug_overlay_elapsed_ms(&self) -> u32 {
        u32::try_from(self.debug_overlay_started_at.elapsed().as_millis()).unwrap_or(u32::MAX)
    }

    pub(super) fn ensure_text_format_for_active_theme(
        &mut self,
        app: &AppState,
    ) -> Result<(), RenderError> {
        let typography = app.active_theme_typography();
        let family = typography.font_family;
        let size_pt = typography.sizes.md.max(1.0);
        let weight = dwrite::normalize_font_weight(typography.weights.normal);
        let line_height = dwrite::normalize_line_height(typography.line_heights.normal);
        if self.text_format_family == family
            && (self.text_format_size_pt - size_pt).abs() < f32::EPSILON
            && self.text_format_weight == weight
            && (self.text_format_line_height - line_height).abs() < f32::EPSILON
        {
            return Ok(());
        }
        self.text_format = dwrite::text_format_from_family_name_with_metrics(
            family.as_str(),
            size_pt,
            weight,
            line_height,
            dwrite::locale_zh_cn(),
        )?;
        self.text_format_family = family;
        self.text_format_size_pt = size_pt;
        self.text_format_weight = weight;
        self.text_format_line_height = line_height;
        self.text_format_cache.clear();
        // RC-5 Gap A — the ellipsis sign captures the *previous* format's
        // typography (size/weight/family); drop it so the next no-wrap
        // draw lazily re-creates a sign against the new format. One COM
        // allocation per theme/font swap, none per frame.
        self.ellipsis_sign = None;
        self.pill_title_ellipsis_sign = None;
        self.bloom_petal_ellipsis_sign = None;
        Ok(())
    }

    pub(super) fn poll_debug_overlay_rss(&self, app: &AppState) {
        let now_ms = self.debug_overlay_elapsed_ms();
        let should_poll = {
            let state = app.debug_overlay.borrow();
            state.visible && state.rss_sample_due(now_ms)
        };
        if !should_poll {
            return;
        }
        let memory = get_memory_usage();
        let rss_mb = (memory.working_set_bytes / 1024) as f32 / 1024.0;
        let _recorded = app
            .debug_overlay
            .borrow_mut()
            .record_rss_if_due(now_ms, rss_mb);
    }

    pub(super) fn record_debug_overlay_frame(
        &self,
        app: &AppState,
        kind: WindowKind,
        frame_started_at: Instant,
    ) {
        if kind != WindowKind::Main {
            return;
        }
        let elapsed_us = u32::try_from(frame_started_at.elapsed().as_micros()).unwrap_or(u32::MAX);
        app.debug_overlay.borrow_mut().record_frame(elapsed_us);
    }
}
