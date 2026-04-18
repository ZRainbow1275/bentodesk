/**
 * App — Root component for BentoDesk.
 *
 * Responsibilities:
 * - Initialize hit-testing (setIgnoreCursorEvents on mount)
 * - Load zones and settings from backend on mount
 * - Listen for file_changed and resolution_changed events
 * - Render ZoneContainer, ContextMenu, and SettingsPanel
 * - Register global keyboard shortcuts
 */
import { Component, onMount, onCleanup, Show, createEffect } from "solid-js";
import {
  enablePassthrough,
  startPolling,
  stopPolling,
  acquireModalLock,
} from "./services/hitTest";
import {
  startDragDropListener,
  stopDragDropListener,
} from "./services/dropTarget";
import {
  onFileChanged,
  onResolutionChanged,
  onSettingsChanged,
  onTrayNewZone,
  onTraySettings,
  onTrayAbout,
  onTrayAutoOrganize,
  combineCleanups,
  type EventCleanup,
} from "./services/events";
import { registerHotkeys, type HotkeyHandlers } from "./services/hotkeys";
import { preloadIcons } from "./services/ipc";
import {
  loadZones,
  handleFileChanged,
  removeItem,
  createZone,
  zonesStore,
  getOutsideDesktopError,
  clearOutsideDesktopError,
} from "./stores/zones";
import { loadSettings, applySettings } from "./stores/settings";
import {
  expandZone,
  collapseZone,
  collapseAllZones,
  isZoneExpanded,
  toggleZoneExpanded,
  getFocusedZoneId,
  setFocusedZone,
  getSelectedItem,
  clearSelection,
  selectItem,
  openSearch,
  closeSearch,
  getSearchActiveZone,
  hideContextMenu,
  closeSettingsPanel,
  isSettingsPanelOpen,
  openSettingsPanel,
  openSmartGroupDialog,
  installViewportTracker,
  uninstallViewportTracker,
  bumpViewport,
} from "./stores/ui";
import { refreshMonitors, invalidateMonitorCache } from "./services/geometry";
import { openFile } from "./services/ipc";
import { openAboutDialog, isAnyModalOpen } from "./stores/ui";
import { applyCurrentTheme } from "./themes";
import { t } from "./i18n";
import { For } from "solid-js";
import ZoneContainer from "./components/ZoneContainer";
import ContextMenu from "./components/ContextMenu/ContextMenu";
import SettingsPanel from "./components/Settings/SettingsPanel";
import ZoneEditor from "./components/ZoneEditor/ZoneEditor";
import SnapshotPicker from "./components/SnapshotPicker/SnapshotPicker";
import About from "./components/About/About";
import SmartGroupSuggestor from "./components/SmartGroup/SmartGroupSuggestor";
import HighlightOverlay from "./components/SmartGroup/HighlightOverlay";
import TimelinePanel from "./components/Timeline/TimelinePanel";
import DragPreview from "./components/DragPreview";
import DebugOverlay from "./components/DebugOverlay/DebugOverlay";
import BulkManagerPanel from "./components/BulkManager/BulkManagerPanel";
import KeybindingsSection from "./components/Settings/KeybindingsSection";
import { undoCheckpoint, redoCheckpoint } from "./services/ipc";
import {
  openTimeline,
  toggleBulkManager,
  openKeybindingsPanel,
} from "./stores/ui";
import { initKeybindings } from "./stores/keybindings";
import { applyLayout } from "./services/autoLayout";
import { clearMultiSelection } from "./stores/selection";
import { clearAll as clearGlobalHotkeys } from "./services/globalHotkeys";

const App: Component = () => {
  let eventCleanup: EventCleanup | null = null;
  let hotkeyCleanup: (() => void) | null = null;

  onMount(async () => {
    // 0. Suppress WebView2 browser defaults in release builds:
    //    - Block the default context menu globally (custom ContextMenu component handles it)
    //    - Block browser accelerator keys (F5 refresh, F12 devtools, Ctrl+Shift+I, etc.)
    document.addEventListener("contextmenu", (e) => {
      e.preventDefault();
    });
    document.addEventListener("keydown", (e) => {
      // Block F5 (refresh), F12 (devtools), Ctrl+Shift+I (devtools), Ctrl+R / Ctrl+Shift+R (reload)
      if (
        e.key === "F5" ||
        e.key === "F12" ||
        (e.ctrlKey && e.shiftKey && e.key === "I") ||
        (e.ctrlKey && e.shiftKey && e.key === "R") ||
        (e.ctrlKey && e.key === "r")
      ) {
        e.preventDefault();
      }
    });

    // 1. Enable click-through passthrough for the overlay window,
    //    start the cursor-position polling loop for hit detection,
    //    and start the OS-level drag-drop listener for Explorer file drops
    await enablePassthrough();
    startPolling();
    await startDragDropListener();

    // 2. Apply saved theme immediately (from localStorage, before backend responds)
    applyCurrentTheme();

    // 3. Load initial data from backend
    await Promise.all([loadZones(), loadSettings(), refreshMonitors()]);

    // 3a. Track viewport size reactively for anchor-flip geometry
    installViewportTracker();

    // 4. Set up event listeners from backend
    const cleanups = await Promise.all([
      onFileChanged((payload) => {
        handleFileChanged(payload.event_type, payload.path, payload.old_path);
      }),
      onResolutionChanged((_payload) => {
        // Zones use relative coordinates, so positions auto-adjust.
        // Reload zones to get any backend-side bound corrections.
        void loadZones();
        // Re-query monitor topology so anchor-flip uses current bounds.
        invalidateMonitorCache();
        void refreshMonitors();
        bumpViewport();
      }),
      onSettingsChanged((payload) => {
        applySettings(payload);
      }),
      onTrayNewZone(() => {
        // Create a new zone with auto-incrementing name
        const existingNames = new Set(zonesStore.zones.map((z) => z.name));
        let name = t("appNewZonePrefix");
        let counter = 2;
        while (existingNames.has(name)) {
          name = `${t("appNewZonePrefix")} ${counter}`;
          counter++;
        }
        // Stagger position slightly based on zone count to avoid overlap
        const offset = zonesStore.zones.length * 3;
        void createZone(
          name,
          "folder",
          { x_percent: 35 + offset, y_percent: 30 + offset },
          { w_percent: 25, h_percent: 40 }
        );
      }),
      onTraySettings(() => {
        openSettingsPanel();
      }),
      onTrayAbout(() => {
        openAboutDialog();
      }),
      onTrayAutoOrganize(() => {
        // Open smart group dialog for the first zone, or create one if none exist
        const zones = zonesStore.zones;
        if (zones.length > 0) {
          openSmartGroupDialog(zones[0].id);
        } else {
          // Create a default zone first, then open the dialog
          void createZone(
            t("appAutoOrganize"),
            "lightning",
            { x_percent: 30, y_percent: 20 },
            { w_percent: 30, h_percent: 50 }
          ).then((zone) => {
            if (zone) {
              openSmartGroupDialog(zone.id);
            }
          });
        }
      }),
    ]);
    eventCleanup = combineCleanups(...cleanups);

    // 5. Register keyboard shortcuts
    hotkeyCleanup = registerHotkeys(createHotkeyHandlers());

    // 5b. Register OS-level global shortcuts (Tauri plugin-global-shortcut).
    //     These fire regardless of webview focus; conflicts are stored in
    //     `keybindingsState` so the Keybindings panel can show them inline.
    void initKeybindings(getGlobalHotkeyHandler);

    // 6. Preload icons for all visible zone items
    const allPaths = zonesStore.zones.flatMap((z) =>
      z.items.map((i) => i.path)
    );
    if (allPaths.length > 0) {
      void preloadIcons(allPaths);
    }
  });

  // When any modal opens, acquire a modal lock to disable passthrough
  // so that clicks on modal overlays don't pass through to the desktop.
  let releaseModalLock: (() => void) | null = null;
  createEffect(() => {
    if (isAnyModalOpen()) {
      if (!releaseModalLock) {
        releaseModalLock = acquireModalLock();
      }
    } else {
      if (releaseModalLock) {
        releaseModalLock();
        releaseModalLock = null;
      }
    }
  });

  onCleanup(() => {
    releaseModalLock?.();
    stopPolling();
    stopDragDropListener();
    uninstallViewportTracker();
    eventCleanup?.();
    hotkeyCleanup?.();
    void clearGlobalHotkeys();
  });

  return (
    <div
      id="desktop-overlay"
      style={{
        width: "100vw",
        height: "100vh",
        position: "relative",
      }}
    >
      <Show
        when={!zonesStore.loading}
        fallback={null}
      >
        <ZoneContainer />
      </Show>
      <ContextMenu />
      <SettingsPanel />
      <ZoneEditor />
      <SnapshotPicker />
      <TimelinePanel />
      <About />
      <SmartGroupSuggestor />
      <HighlightOverlay />
      <DragPreview />
      <DebugOverlay />
      <BulkManagerPanel />
      <KeybindingsSection handlerFor={getGlobalHotkeyHandler} />
      <Show when={getOutsideDesktopError()}>
        <div class="app-toast app-toast--outside-desktop" role="alert">
          <div class="app-toast__header">
            <div class="app-toast__title">
              {t("addItemErrorOutsideDesktop")}
            </div>
            <button
              class="app-toast__close"
              onClick={clearOutsideDesktopError}
              aria-label={t("settingsCloseAriaLabel")}
            >
              ×
            </button>
          </div>
          <div class="app-toast__path">
            {getOutsideDesktopError()!.path}
          </div>
          <ul class="app-toast__sources">
            <For each={getOutsideDesktopError()!.allowed_sources}>
              {(src) => <li class="app-toast__source">{src}</li>}
            </For>
          </ul>
        </div>
      </Show>
      <Show when={zonesStore.error && !getOutsideDesktopError()}>
        <div class="app-toast app-toast--generic" role="alert">
          {zonesStore.error}
        </div>
      </Show>
    </div>
  );
};

// ─── Hotkey handler factory ──────────────────────────────────

function createHotkeyHandlers(): HotkeyHandlers {
  return {
    onTab: (_e) => {
      // Cycle focus between zones
      const zones = zonesStore.zones;
      if (zones.length === 0) return;

      const currentFocused = getFocusedZoneId();
      let nextIndex = 0;

      if (currentFocused) {
        const currentIndex = zones.findIndex((z) => z.id === currentFocused);
        nextIndex = (currentIndex + 1) % zones.length;
      }

      setFocusedZone(zones[nextIndex].id);
    },

    onEnter: (_e) => {
      // Open focused/selected item
      const sel = getSelectedItem();
      if (sel) {
        const zone = zonesStore.zones.find((z) => z.id === sel.zoneId);
        const item = zone?.items.find((i) => i.id === sel.itemId);
        if (item) {
          void openFile(item.path);
        }
      }
    },

    onSpace: (_e) => {
      // Toggle expand/collapse on focused zone
      const focused = getFocusedZoneId();
      if (focused) {
        toggleZoneExpanded(focused);
      }
    },

    onArrowUp: (_e) => {
      navigateGrid(0, -1);
    },

    onArrowDown: (_e) => {
      navigateGrid(0, 1);
    },

    onArrowLeft: (_e) => {
      navigateGrid(-1, 0);
    },

    onArrowRight: (_e) => {
      navigateGrid(1, 0);
    },

    onDelete: (_e) => {
      const sel = getSelectedItem();
      if (sel) {
        void removeItem(sel.zoneId, sel.itemId).then(() => {
          clearSelection();
        });
      }
    },

    onCtrlF: (_e) => {
      // Open search in focused zone
      const focused = getFocusedZoneId();
      if (focused && isZoneExpanded(focused)) {
        openSearch(focused);
      }
    },

    onEscape: (_e) => {
      // Close in priority: search > context menu > settings > expanded zone
      if (getSearchActiveZone()) {
        closeSearch();
      } else if (isSettingsPanelOpen()) {
        closeSettingsPanel();
      } else {
        hideContextMenu();
        collapseAllZones();
        clearSelection();
        clearMultiSelection();
      }
    },

    onCtrlZ: (_e) => {
      // Undo to previous timeline checkpoint. Surface the timeline panel so
      // the user can see what just happened.
      void undoCheckpoint().then((id) => {
        if (id) {
          openTimeline();
        }
      });
    },

    onCtrlShiftZ: (_e) => {
      void redoCheckpoint().then((id) => {
        if (id) {
          openTimeline();
        }
      });
    },
  };
}

/**
 * Navigate within the focused zone's item grid.
 */
function navigateGrid(dx: number, dy: number): void {
  const focusedZone = getFocusedZoneId();
  if (!focusedZone) return;

  const zone = zonesStore.zones.find((z) => z.id === focusedZone);
  if (!zone || zone.items.length === 0) return;

  // Ensure zone is expanded
  if (!isZoneExpanded(focusedZone)) {
    expandZone(focusedZone);
    return;
  }

  const sel = getSelectedItem();
  const cols = zone.grid_columns || 4;

  if (!sel || sel.zoneId !== focusedZone) {
    // Select first item
    selectItem(focusedZone, zone.items[0].id);
    return;
  }

  const currentIndex = zone.items.findIndex((i) => i.id === sel.itemId);
  if (currentIndex === -1) return;

  const currentRow = Math.floor(currentIndex / cols);
  const currentCol = currentIndex % cols;
  const newRow = currentRow + dy;
  const newCol = currentCol + dx;
  const newIndex = newRow * cols + newCol;

  if (newIndex >= 0 && newIndex < zone.items.length && newCol >= 0 && newCol < cols) {
    selectItem(focusedZone, zone.items[newIndex].id);
  }
}

/**
 * Resolve a handler for a keybindings action id. Used by the Tauri
 * plugin-global-shortcut registrar and the Keybindings panel's record UI
 * so the same callback fires whether the binding originated from defaults,
 * user override, or a runtime rebind.
 */
function getGlobalHotkeyHandler(action: string): (() => void) | undefined {
  switch (action) {
    case "app.toggle":
      // Ask the main window to toggle focus/visibility. A full OS-level
      // minimize is owned by the tray; for now we just bring the overlay
      // to front and expand the first zone if any.
      return () => {
        const first = zonesStore.zones[0];
        if (first) toggleZoneExpanded(first.id);
      };
    case "zone.new":
      return () => {
        const existingNames = new Set(zonesStore.zones.map((z) => z.name));
        let name = t("appNewZonePrefix");
        let counter = 2;
        while (existingNames.has(name)) {
          name = `${t("appNewZonePrefix")} ${counter}`;
          counter++;
        }
        const offset = zonesStore.zones.length * 3;
        void createZone(
          name,
          "folder",
          { x_percent: 35 + offset, y_percent: 30 + offset },
          { w_percent: 25, h_percent: 40 }
        );
      };
    case "zone.duplicate":
      return () => {
        const focused = getFocusedZoneId();
        const zone = focused
          ? zonesStore.zones.find((z) => z.id === focused)
          : zonesStore.zones[0];
        if (!zone) return;
        void createZone(
          `${zone.name} *`,
          zone.icon,
          {
            x_percent: Math.min(zone.position.x_percent + 5, 90),
            y_percent: Math.min(zone.position.y_percent + 5, 90),
          },
          zone.expanded_size
        );
      };
    case "zone.lock-toggle":
      // Locked-zone persistence lands with Theme D's alias/locked fields.
      // Until then this is a no-op the record UI still binds cleanly to.
      return () => {};
    case "zone.hide-all":
      return () => collapseAllZones();
    case "layout.auto-organize":
    case "layout.reflow":
      return () => {
        void applyLayout("grid", zonesStore.zones);
      };
    case "bulk.open-manager":
      return () => toggleBulkManager();
    case "zone.focus.next":
      return () => {
        const zones = zonesStore.zones;
        if (zones.length === 0) return;
        const current = getFocusedZoneId();
        const idx = current
          ? (zones.findIndex((z) => z.id === current) + 1) % zones.length
          : 0;
        setFocusedZone(zones[idx].id);
      };
    case "zone.focus.prev":
      return () => {
        const zones = zonesStore.zones;
        if (zones.length === 0) return;
        const current = getFocusedZoneId();
        const idx = current
          ? (zones.findIndex((z) => z.id === current) - 1 + zones.length) %
            zones.length
          : zones.length - 1;
        setFocusedZone(zones[idx].id);
      };
    default:
      return undefined;
  }
}

// `openKeybindingsPanel` is re-exported from stores/ui so other files (tray
// menu, settings card) can open the panel without importing Theme C internals.
export { openKeybindingsPanel };

export default App;
