/**
 * UI state store: expanded zones, active context menu, search state,
 * selected items, settings panel visibility, and focused zone/item tracking.
 */
import { createSignal } from "solid-js";
import { refreshMonitors, invalidateMonitorCache } from "../services/geometry";

// ─── Viewport size (reactive) ────────────────────────────────

interface ViewportSize {
  width: number;
  height: number;
}

const [viewportSize, setViewportSizeSignal] = createSignal<ViewportSize>({
  width: typeof window !== "undefined" ? window.innerWidth : 0,
  height: typeof window !== "undefined" ? window.innerHeight : 0,
});

export function getViewportSize(): ViewportSize {
  return viewportSize();
}

let viewportInstalled = false;
let viewportListener: (() => void) | null = null;

/**
 * Install the `window.resize` listener once. BentoZone reads
 * `viewportSize()` so its anchor-flip memo re-runs when the overlay
 * is resized (DPI change, display hot-plug reflow, etc.).
 */
export function installViewportTracker(): void {
  if (viewportInstalled) return;
  viewportInstalled = true;
  const handler = () => {
    setViewportSizeSignal({
      width: window.innerWidth,
      height: window.innerHeight,
    });
    // Monitor topology can change when the overlay reflows (e.g.
    // primary-monitor swap). Invalidate + refresh lazily.
    invalidateMonitorCache();
    void refreshMonitors();
  };
  window.addEventListener("resize", handler);
  viewportListener = () => window.removeEventListener("resize", handler);
}

export function uninstallViewportTracker(): void {
  if (viewportListener) {
    viewportListener();
    viewportListener = null;
  }
  viewportInstalled = false;
}

/**
 * Bump the viewport signal without changing dimensions. Used when the
 * backend reports `resolution_changed` so all dependents (anchor memos)
 * recompute against the updated monitor cache.
 */
export function bumpViewport(): void {
  setViewportSizeSignal({
    width: window.innerWidth,
    height: window.innerHeight,
  });
}

// ─── Expanded zones ──────────────────────────────────────────

const [expandedZones, setExpandedZones] = createSignal<Set<string>>(
  new Set()
);

export function isZoneExpanded(zoneId: string): boolean {
  return expandedZones().has(zoneId);
}

export function expandZone(zoneId: string): void {
  setExpandedZones((prev) => {
    const next = new Set(prev);
    next.add(zoneId);
    return next;
  });
}

export function collapseZone(zoneId: string): void {
  setExpandedZones((prev) => {
    const next = new Set(prev);
    next.delete(zoneId);
    return next;
  });
}

export function toggleZoneExpanded(zoneId: string): void {
  if (isZoneExpanded(zoneId)) {
    collapseZone(zoneId);
  } else {
    expandZone(zoneId);
  }
}

export function collapseAllZones(): void {
  setExpandedZones(new Set<string>());
}

// ─── Context menu ────────────────────────────────────────────

export type ContextMenuTarget =
  | { type: "zone"; zoneId: string }
  | { type: "item"; zoneId: string; itemId: string }
  | null;

interface ContextMenuState {
  x: number;
  y: number;
  target: ContextMenuTarget;
}

const [contextMenu, setContextMenu] = createSignal<ContextMenuState | null>(
  null
);

export function getContextMenu(): ContextMenuState | null {
  return contextMenu();
}

export function showContextMenu(
  x: number,
  y: number,
  target: ContextMenuTarget
): void {
  setContextMenu({ x, y, target });
}

export function hideContextMenu(): void {
  setContextMenu(null);
}

// ─── Search state ────────────────────────────────────────────

const [searchQuery, setSearchQuerySignal] = createSignal<string>("");
const [searchActiveZone, setSearchActiveZone] = createSignal<string | null>(
  null
);

export function getSearchQuery(): string {
  return searchQuery();
}

export function setSearchQuery(query: string): void {
  setSearchQuerySignal(query);
}

export function getSearchActiveZone(): string | null {
  return searchActiveZone();
}

export function openSearch(zoneId: string): void {
  setSearchActiveZone(zoneId);
  setSearchQuerySignal("");
}

export function closeSearch(): void {
  setSearchActiveZone(null);
  setSearchQuerySignal("");
}

export function isSearchActive(zoneId: string): boolean {
  return searchActiveZone() === zoneId;
}

// ─── Selected item ───────────────────────────────────────────

interface SelectedItem {
  zoneId: string;
  itemId: string;
}

const [selectedItem, setSelectedItemSignal] = createSignal<SelectedItem | null>(
  null
);

export function getSelectedItem(): SelectedItem | null {
  return selectedItem();
}

export function selectItem(zoneId: string, itemId: string): void {
  setSelectedItemSignal({ zoneId, itemId });
}

export function clearSelection(): void {
  setSelectedItemSignal(null);
}

export function isItemSelected(zoneId: string, itemId: string): boolean {
  const sel = selectedItem();
  return sel !== null && sel.zoneId === zoneId && sel.itemId === itemId;
}

// ─── Focused zone (keyboard nav) ────────────────────────────

const [focusedZoneId, setFocusedZoneId] = createSignal<string | null>(null);

export function getFocusedZoneId(): string | null {
  return focusedZoneId();
}

export function setFocusedZone(zoneId: string | null): void {
  setFocusedZoneId(zoneId);
}

// ─── Settings panel ──────────────────────────────────────────

const [settingsPanelOpen, setSettingsPanelOpen] = createSignal(false);

export function isSettingsPanelOpen(): boolean {
  return settingsPanelOpen();
}

export function openSettingsPanel(): void {
  setSettingsPanelOpen(true);
}

export function closeSettingsPanel(): void {
  setSettingsPanelOpen(false);
}

export function toggleSettingsPanel(): void {
  setSettingsPanelOpen((prev) => !prev);
}

// ─── Zone editor dialog ─────────────────────────────────────

const [editingZoneId, setEditingZoneId] = createSignal<string | null>(null);

export function getEditingZoneId(): string | null {
  return editingZoneId();
}

export function openZoneEditor(zoneId: string): void {
  setEditingZoneId(zoneId);
}

export function closeZoneEditor(): void {
  setEditingZoneId(null);
}

// ─── Snapshot picker ────────────────────────────────────────

const [snapshotPickerOpen, setSnapshotPickerOpen] = createSignal(false);

export function isSnapshotPickerOpen(): boolean {
  return snapshotPickerOpen();
}

export function openSnapshotPicker(): void {
  setSnapshotPickerOpen(true);
}

export function closeSnapshotPicker(): void {
  setSnapshotPickerOpen(false);
}

// ─── Timeline (Time-machine, R4-C1) ─────────────────────────

const [timelineOpen, setTimelineOpen] = createSignal(false);

export function isTimelineOpen(): boolean {
  return timelineOpen();
}

export function openTimeline(): void {
  setTimelineOpen(true);
}

export function closeTimeline(): void {
  setTimelineOpen(false);
}

export function toggleTimeline(): void {
  setTimelineOpen((prev) => !prev);
}

// ─── About dialog ───────────────────────────────────────────

const [aboutDialogOpen, setAboutDialogOpen] = createSignal(false);

export function isAboutDialogOpen(): boolean {
  return aboutDialogOpen();
}

export function openAboutDialog(): void {
  setAboutDialogOpen(true);
}

export function closeAboutDialog(): void {
  setAboutDialogOpen(false);
}

// ─── Bulk manager panel (Theme C) ───────────────────────────

const [bulkManagerOpen, setBulkManagerOpen] = createSignal(false);

export function isBulkManagerOpen(): boolean {
  return bulkManagerOpen();
}

export function openBulkManager(): void {
  setBulkManagerOpen(true);
}

export function closeBulkManager(): void {
  setBulkManagerOpen(false);
}

export function toggleBulkManager(): void {
  setBulkManagerOpen((prev) => !prev);
}

// ─── Keybindings panel (Theme C) ───────────────────────────

const [keybindingsPanelOpen, setKeybindingsPanelOpen] = createSignal(false);

export function isKeybindingsPanelOpen(): boolean {
  return keybindingsPanelOpen();
}

export function openKeybindingsPanel(): void {
  setKeybindingsPanelOpen(true);
}

export function closeKeybindingsPanel(): void {
  setKeybindingsPanelOpen(false);
}

// ─── Smart Grouping dialog ──────────────────────────────────

const [smartGroupZoneId, setSmartGroupZoneId] = createSignal<string | null>(
  null
);

export function getSmartGroupZoneId(): string | null {
  return smartGroupZoneId();
}

export function openSmartGroupDialog(zoneId: string): void {
  setSmartGroupZoneId(zoneId);
}

export function closeSmartGroupDialog(): void {
  setSmartGroupZoneId(null);
}

// ─── Confirm dialog (context menu delete confirmation) ──────

const [confirmDialogOpen, setConfirmDialogOpenSignal] = createSignal(false);

export function isConfirmDialogOpen(): boolean {
  return confirmDialogOpen();
}

export function setConfirmDialogOpen(open: boolean): void {
  setConfirmDialogOpenSignal(open);
}

// ─── Aggregate modal state ──────────────────────────────────

/**
 * Returns true when any modal/dialog/overlay is open.
 * Used by the global hotkey handler to avoid collisions.
 */
export function isAnyModalOpen(): boolean {
  return (
    settingsPanelOpen() ||
    editingZoneId() !== null ||
    snapshotPickerOpen() ||
    timelineOpen() ||
    aboutDialogOpen() ||
    smartGroupZoneId() !== null ||
    contextMenu() !== null ||
    confirmDialogOpen() ||
    bulkManagerOpen() ||
    keybindingsPanelOpen()
  );
}
