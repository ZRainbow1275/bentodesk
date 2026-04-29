// Dev-only Tauri IPC mock.
//
// Activates only when running in `vite dev` AND the real Tauri runtime
// (__TAURI_INTERNALS__) is absent — i.e. when Playwright / a browser hits
// http://127.0.0.1:5173 directly. Production builds tree-shake this entire
// module away because `import.meta.env.DEV` resolves to the literal `false`.
//
// Provides:
//   1. A canned set of 5 zones — 3 deliberately overlapping so the
//      auto-spread / stack UX can be exercised, 2 spaced apart.
//   2. A handler for every IPC command the boot path or stack interaction
//      touches (list_zones, get_settings, reorder_zones, stack_zones,
//      unstack_zones, bulk_update_zones, normalize_zone_layout, etc.).
//   3. A `window.__bentoTest__` namespace with helpers Playwright can call
//      to reset state, inspect the in-memory layout, and force overlap.

import type { BentoZone } from "./types/zone";
import type { AppSettings } from "./types/settings";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __bentoTest__?: {
      reset: () => void;
      getZones: () => BentoZone[];
      setZones: (zones: BentoZone[]) => void;
      forceOverlap: () => void;
    };
  }
}

if (import.meta.env.DEV && !window.__TAURI_INTERNALS__) {
  const now = new Date().toISOString();

  const makeZone = (
    id: string,
    name: string,
    icon: string,
    x: number,
    y: number,
    sort_order: number,
    extra: Partial<BentoZone> = {}
  ): BentoZone => ({
    id,
    name,
    icon,
    position: { x_percent: x, y_percent: y },
    expanded_size: { w_percent: 22, h_percent: 28 },
    items: [],
    accent_color: null,
    sort_order,
    auto_group: null,
    grid_columns: 4,
    created_at: now,
    updated_at: now,
    capsule_size: "medium",
    capsule_shape: "pill",
    ...extra,
  });

  // 3 overlapping zones top-left (positions almost identical → cluster
  // detection fires), 2 free-standing zones bottom-right.
  const initialZones: BentoZone[] = [
    makeZone("zone-1", "Docs", "📄", 10, 12, 0),
    makeZone("zone-2", "Code", "💻", 11, 13, 1),
    makeZone("zone-3", "Notes", "🗒️", 12, 14, 2),
    makeZone("zone-4", "Media", "🎬", 60, 20, 3),
    makeZone("zone-5", "Tools", "🛠️", 70, 70, 4),
  ];

  let zones: BentoZone[] = initialZones.map((z) => ({ ...z }));
  let stackSeq = 1;

  const settings: AppSettings = {
    schema_version: "1.0",
    version: "1.2.4",
    desktop_path: "D:\\Desktop",
    portable_mode: false,
    launch_at_startup: false,
    show_in_taskbar: false,
    accent_color: "#6366f1",
    theme: { mode: "system", accent: "#6366f1" },
    ghost_layer_enabled: true,
    expand_delay_ms: 120,
    collapse_delay_ms: 200,
    icon_cache_size: 512,
    auto_group_enabled: true,
    watch_paths: [],
    safety_profile: "balanced",
    startup_high_priority: false,
    crash_reports_enabled: false,
    crash_report_endpoint: null,
    crash_report_max_age_days: 14,
    safe_start_threshold: 3,
    safe_start_window_ms: 60_000,
    hibernate_idle_seconds: 0,
    hibernate_enabled: false,
    updates: {
      channel: "stable",
      auto_check: true,
      auto_install: false,
      check_interval_hours: 24,
    },
    encryption: {
      enabled: false,
      kdf: "argon2id",
      kdf_params: { m_kib: 65536, t: 3, p: 1 },
    },
    debug_overlay: false,
    zone_display_mode: "hover",
    // any extra fields the schema grew between snapshots are tolerated by
    // the cast below — this is dev-only and never reaches production.
  } as unknown as AppSettings;

  const handlers: Record<string, (args: any) => any> = {
    list_zones: () => zones,
    get_settings: () => settings,
    update_settings: ({ updates }: { updates: Partial<AppSettings> }) => {
      Object.assign(settings, updates);
      return settings;
    },
    get_system_info: () => ({
      os: "windows",
      version: "10.0.26220",
      arch: "x86_64",
      cpu_count: 16,
      total_memory_bytes: 32 * 1024 * 1024 * 1024,
    }),
    get_desktop_sources: () => [
      {
        monitor_index: 0,
        monitor_name: "Primary",
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        scale_factor: 1.0,
        is_primary: true,
      },
    ],
    get_memory_usage: () => ({
      working_set_bytes: 0,
      peak_working_set_bytes: 0,
      virtual_bytes: 0,
    }),
    get_webview2_memory: () => ({
      host_pid: 0,
      processes: [],
      total_working_set_bytes: 0,
      total_peak_working_set_bytes: 0,
    }),
    get_icon_cache_stats: () => ({
      hot_hits: 0,
      warm_hits: 0,
      misses: 0,
      evictions: 0,
      warm_writes: 0,
      warm_write_failures: 0,
      total_lookups: 0,
      hit_rate: 0,
    }),
    reconcile_all_zone_items: () => ({
      reconciled_count: 0,
      already_managed_count: 0,
      missing_count: 0,
      unknown_count: 0,
      touched_zone_ids: [],
    }),
    repair_item_icon_hashes: () => ({ repaired_count: 0, repairs: [] }),
    normalize_zone_layout: () => ({
      normalized_count: 0,
      adjustments: [],
    }),
    list_themes: () => [],
    get_active_theme: () => null,
    list_plugins: () => [],
    list_snapshots: () => [],
    list_checkpoints: () => [],
    get_stealth_status: () => ({
      applied: true,
      last_error: null,
      retry_count: 0,
      schema_version: "1.0",
      mirror_healthy: true,
    }),
    check_onedrive_exclusion_needed: () => ({
      needed: false,
      desktop_path: "D:\\Desktop",
      exclusion_hint: "",
      guide_url: "",
    }),

    // Zone CRUD ------------------------------------------------------------
    create_zone: ({
      name,
      icon,
      position,
      expandedSize,
    }: {
      name: string;
      icon: string;
      position: { x_percent: number; y_percent: number };
      expandedSize: { w_percent: number; h_percent: number };
    }) => {
      const id = `zone-${zones.length + 1}-${Math.random().toString(36).slice(2, 6)}`;
      const z = makeZone(
        id,
        name,
        icon,
        position.x_percent,
        position.y_percent,
        zones.length
      );
      z.expanded_size = expandedSize;
      zones.push(z);
      return z;
    },
    update_zone: ({ id, updates }: { id: string; updates: Partial<BentoZone> }) => {
      const i = zones.findIndex((z) => z.id === id);
      if (i < 0) throw new Error(`zone ${id} not found`);
      zones[i] = { ...zones[i], ...updates, updated_at: new Date().toISOString() };
      return zones[i];
    },
    delete_zone: ({ id }: { id: string }) => {
      zones = zones.filter((z) => z.id !== id);
    },
    reorder_zones: ({ zoneIds }: { zoneIds: string[] }) => {
      const map = new Map(zones.map((z) => [z.id, z]));
      zones = zoneIds
        .map((id, idx) => {
          const z = map.get(id);
          if (!z) return null;
          return { ...z, sort_order: idx };
        })
        .filter((z): z is BentoZone => z !== null);
    },

    // Stack ---------------------------------------------------------------
    stack_zones: ({ zoneIds }: { zoneIds: string[] }) => {
      const stack_id = `stack-${stackSeq++}`;
      zoneIds.forEach((zid, idx) => {
        const i = zones.findIndex((z) => z.id === zid);
        if (i >= 0) {
          zones[i] = { ...zones[i], stack_id, stack_order: idx };
        }
      });
      return stack_id;
    },
    unstack_zones: ({ stackId }: { stackId: string }) => {
      zones = zones.map((z) =>
        z.stack_id === stackId ? { ...z, stack_id: null, stack_order: 0 } : z
      );
    },
    set_zone_alias: ({ zoneId, alias }: { zoneId: string; alias: string | null }) => {
      const i = zones.findIndex((z) => z.id === zoneId);
      if (i >= 0) zones[i] = { ...zones[i], alias };
    },
    reorder_stack: ({
      stackId,
      zoneId,
      newOrder,
    }: {
      stackId: string;
      zoneId: string;
      newOrder: number;
    }) => {
      const i = zones.findIndex((z) => z.id === zoneId && z.stack_id === stackId);
      if (i >= 0) zones[i] = { ...zones[i], stack_order: newOrder };
    },

    // Bulk ----------------------------------------------------------------
    bulk_update_zones: ({
      updates,
    }: {
      updates: Array<{ id: string } & Partial<BentoZone>>;
    }) => {
      let changed = 0;
      for (const u of updates) {
        const i = zones.findIndex((z) => z.id === u.id);
        if (i >= 0) {
          const { id: _id, ...rest } = u;
          zones[i] = {
            ...zones[i],
            ...(rest as Partial<BentoZone>),
            updated_at: new Date().toISOString(),
          };
          changed++;
        }
      }
      return changed;
    },
    bulk_delete_zones: ({ ids }: { ids: string[] }) => {
      const before = zones.length;
      const set = new Set(ids);
      zones = zones.filter((z) => !set.has(z.id));
      return before - zones.length;
    },
    apply_layout_algorithm: () => 0,

    // Item / file ops are no-ops in browser mode -------------------------
    add_item: () => {
      throw new Error("add_item not supported in browser dev-mock");
    },
    remove_item: () => undefined,
    move_item: () => undefined,
    reorder_items: () => undefined,
    toggle_item_wide: () => {
      throw new Error("toggle_item_wide not supported in browser dev-mock");
    },
    open_file: () => undefined,
    reveal_in_explorer: () => undefined,
    get_file_info: () => {
      throw new Error("get_file_info not supported in browser dev-mock");
    },
    get_icon_url: ({ path }: { path: string }) =>
      `data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32'><rect width='32' height='32' fill='%236366f1'/><text x='16' y='22' font-size='18' text-anchor='middle' fill='white'>?</text></svg>`,
    preload_icons: () => undefined,
    clear_icon_cache: () => undefined,
    scan_desktop: () => [],
    suggest_groups: () => [],
    apply_auto_group: () => [],
    auto_group_new_file: () => [],
    highlight_desktop_files: () => 0,
    clear_desktop_highlights: () => undefined,
    save_snapshot: ({ name }: { name: string }) => ({
      id: `snap-${Date.now()}`,
      name,
      created_at: new Date().toISOString(),
      zones: [],
    }),
    load_snapshot: () => undefined,
    delete_snapshot: () => undefined,
    get_checkpoint: () => {
      throw new Error("get_checkpoint not supported in browser dev-mock");
    },
    restore_checkpoint: () => undefined,
    undo_checkpoint: () => null,
    redo_checkpoint: () => null,
    delete_checkpoint: () => undefined,
    save_checkpoint_permanent: () => {
      throw new Error("save_checkpoint_permanent not supported in browser dev-mock");
    },
    start_drag: () => "noop",
    reapply_stealth: () => ({
      applied: true,
      last_error: null,
      retry_count: 0,
      schema_version: "1.0",
      mirror_healthy: true,
    }),
    get_theme: () => null,
    set_active_theme: () => null,
    install_plugin: () => {
      throw new Error("install_plugin not supported in browser dev-mock");
    },
    uninstall_plugin: () => undefined,
    toggle_plugin: () => {
      throw new Error("toggle_plugin not supported in browser dev-mock");
    },

    // Window controls / passthrough --------------------------------------
    enable_passthrough: () => undefined,
    start_polling: () => undefined,
    start_drag_drop_listener: () => undefined,

    // Tauri builtin plugin commands the SDK fires during startup ----------
    "plugin:event|listen": () => Math.floor(Math.random() * 1e9),
    "plugin:event|unlisten": () => null,
    "plugin:event|emit": () => null,
    "plugin:event|emit_to": () => null,
    "plugin:window|set_ignore_cursor_events": () => null,
    "plugin:window|is_focused": () => true,
    "plugin:window|is_visible": () => true,
    "plugin:window|is_minimized": () => false,
    "plugin:window|is_maximized": () => false,
    "plugin:window|inner_size": () => ({ width: 1280, height: 800 }),
    "plugin:window|outer_size": () => ({ width: 1280, height: 800 }),
    "plugin:window|inner_position": () => ({ x: 0, y: 0 }),
    "plugin:window|outer_position": () => ({ x: 0, y: 0 }),
    "plugin:window|scale_factor": () => 1,
    "plugin:window|theme": () => "light",
    "plugin:window|set_focus": () => null,
    "plugin:window|set_always_on_top": () => null,
    "plugin:window|show": () => null,
    "plugin:window|hide": () => null,
    "plugin:webview|webview_position": () => ({ x: 0, y: 0 }),
    "plugin:webview|webview_size": () => ({ width: 1280, height: 800 }),
    "plugin:webview|set_webview_position": () => null,
    "plugin:webview|set_webview_size": () => null,
    "plugin:globalShortcut|register": () => null,
    "plugin:globalShortcut|unregister": () => null,
    "plugin:globalShortcut|unregister_all": () => null,
    "plugin:globalShortcut|is_registered": () => false,
    "plugin:updater|check": () => ({ available: false }),
    "plugin:dialog|open": () => null,
    "plugin:dialog|save": () => null,
    "plugin:dialog|message": () => null,
    "plugin:dialog|ask": () => false,
    "plugin:dialog|confirm": () => false,
  };

  // Tauri 2's @tauri-apps/api/core invoke() routes through
  // window.__TAURI_INTERNALS__.invoke(cmd, args, options). We provide just
  // enough of that surface to satisfy the SDK without pulling Tauri in.
  const internals = {
    invoke: (cmd: string, args: any = {}) => {
      const handler = handlers[cmd];
      if (!handler) {
        // Soft-fail unknown commands: warn loudly but resolve with undefined so
        // a single missing stub never aborts a startup effect chain (e.g.
        // enablePassthrough → startPolling → startDragDropListener → loadZones).
        // eslint-disable-next-line no-console
        console.warn(`[dev-mock] unhandled IPC command (resolved undefined): ${cmd}`);
        return Promise.resolve(undefined);
      }
      try {
        return Promise.resolve(handler(args));
      } catch (err) {
        return Promise.reject(err);
      }
    },
    transformCallback: (callback?: (...args: any[]) => unknown, _once = false) => {
      const id = Math.floor(Math.random() * 1e9);
      // Echo registration but never fire — backend events don't exist in browser.
      void callback;
      return id;
    },
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    plugins: {},
    convertFileSrc: (path: string, protocol = "asset") => `${protocol}://${encodeURIComponent(path)}`,
  };

  (window as any).__TAURI_INTERNALS__ = internals;

  // Test helpers ---------------------------------------------------------
  window.__bentoTest__ = {
    reset: () => {
      zones = initialZones.map((z) => ({ ...z }));
      stackSeq = 1;
    },
    getZones: () => zones.map((z) => ({ ...z })),
    setZones: (next) => {
      zones = next.map((z) => ({ ...z }));
    },
    forceOverlap: () => {
      // Snap zones 1/2/3 to the same spot so cluster detection fires hard.
      zones = zones.map((z) => {
        if (z.id === "zone-1" || z.id === "zone-2" || z.id === "zone-3") {
          return { ...z, position: { x_percent: 15, y_percent: 15 } };
        }
        return z;
      });
    },
  };

  // eslint-disable-next-line no-console
  console.info("[dev-mock] Tauri IPC mock active — 5 zones, 3 overlapping. Use window.__bentoTest__ to drive it.");
}

export {};
