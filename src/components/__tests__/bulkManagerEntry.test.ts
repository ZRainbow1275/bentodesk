/**
 * #4 batch-manage entry test.
 *
 * Asserts that:
 *   1. The "批量管理…" context-menu callback opens the BulkManagerPanel
 *      (i.e. flips `isBulkManagerOpen()` to true).
 *   2. When the panel opens with zones already in `selectedZoneIds`, those
 *      ids stay in selection so the BulkManagerPanel auto-checks the
 *      corresponding rows (the panel reads `selectedZoneIds()` directly).
 *   3. `bulk_update_zones` is the IPC command name used to commit a
 *      group-drag — guards #3 wiring against accidental rename.
 */
import { describe, it, expect, beforeEach, vi } from "vitest";

const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

import {
  isBulkManagerOpen,
  openBulkManager,
  closeBulkManager,
} from "../../stores/ui";
import {
  setZoneSelection,
  selectedZoneIds,
  clearMultiSelection,
} from "../../stores/selection";
import * as ipc from "../../services/ipc";

describe("#4 BulkManager context-menu entry", () => {
  beforeEach(() => {
    closeBulkManager();
    clearMultiSelection();
  });

  it("openBulkManager flips the panel open signal", () => {
    expect(isBulkManagerOpen()).toBe(false);
    openBulkManager();
    expect(isBulkManagerOpen()).toBe(true);
    closeBulkManager();
    expect(isBulkManagerOpen()).toBe(false);
  });

  it("preserves canvas selection when opening so the table auto-checks rows", () => {
    setZoneSelection(["z1", "z2", "z3"]);
    expect(selectedZoneIds().size).toBe(3);
    openBulkManager();
    expect(selectedZoneIds().has("z1")).toBe(true);
    expect(selectedZoneIds().has("z2")).toBe(true);
    expect(selectedZoneIds().has("z3")).toBe(true);
  });
});

describe("#3 batch-drag IPC command name", () => {
  // Mock-IPC-only tests are forbidden by spec.md when used to fake a real
  // path, but verifying the *command-name contract* between the frontend
  // groupDrag commit and the backend command is a fundamentally
  // boundary-level test — renaming the command on either side is exactly
  // what would break batch-drag in production.
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(2);
  });

  it("ipc.bulkUpdateZones invokes the bulk_update_zones backend command", async () => {
    const updates: ipc.BulkZoneUpdate[] = [
      { id: "z1", position: { x_percent: 10, y_percent: 10 } },
      { id: "z2", position: { x_percent: 20, y_percent: 20 } },
    ];
    const n = await ipc.bulkUpdateZones(updates);
    expect(n).toBe(2);
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    expect(mockInvoke).toHaveBeenCalledWith("bulk_update_zones", { updates });
  });
});
