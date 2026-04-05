/**
 * Tests for the zones Solid.js store.
 *
 * Mocks ../services/ipc to avoid Tauri IPC calls.
 * Focuses on: CRUD operations, produce-based store updates, error handling.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { BentoZone, BentoItem } from "../../types/zone";

// ─── Mock IPC ───────────────────────────────────────────────

vi.mock("../../services/ipc", () => ({
  listZones: vi.fn(),
  createZone: vi.fn(),
  updateZone: vi.fn(),
  deleteZone: vi.fn(),
  reorderZones: vi.fn(),
  addItem: vi.fn(),
  removeItem: vi.fn(),
  moveItem: vi.fn(),
  reorderItems: vi.fn(),
  toggleItemWide: vi.fn(),
  preloadIcons: vi.fn(),
  autoGroupNewFile: vi.fn(),
}));

// Import mocked ipc so we can configure return values
import * as ipc from "../../services/ipc";

// Import store functions — they use the mocked ipc internally
import {
  getZones,
  getZoneById,
  isLoading,
  getError,
  loadZones,
  createZone,
  updateZone,
  deleteZone,
  reorderZones,
  addItem,
  removeItem,
  moveItem,
  reorderItems,
  toggleItemWide,
  zonesStore,
} from "../zones";

// ─── Fixtures ───────────────────────────────────────────────

function makeZone(overrides: Partial<BentoZone> = {}): BentoZone {
  return {
    id: "zone-1",
    name: "Documents",
    icon: "📄",
    position: { x_percent: 10, y_percent: 10 },
    expanded_size: { w_percent: 30, h_percent: 40 },
    items: [],
    accent_color: null,
    sort_order: 0,
    auto_group: null,
    grid_columns: 4,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    capsule_size: "medium",
    capsule_shape: "pill",
    ...overrides,
  };
}

function makeItem(overrides: Partial<BentoItem> = {}): BentoItem {
  return {
    id: "item-1",
    zone_id: "zone-1",
    item_type: "File",
    name: "readme.txt",
    path: "C:\\Users\\test\\Desktop\\readme.txt",
    icon_hash: "abc123",
    grid_position: { col: 0, row: 0, col_span: 1 },
    is_wide: false,
    added_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

// ─── Tests ──────────────────────────────────────────────────

describe("zones store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // ── loadZones ──

  describe("loadZones", () => {
    it("should load zones from IPC and update the store", async () => {
      const zones = [makeZone(), makeZone({ id: "zone-2", name: "Images", sort_order: 1 })];
      vi.mocked(ipc.listZones).mockResolvedValue(zones);

      await loadZones();

      expect(ipc.listZones).toHaveBeenCalledOnce();
      expect(getZones()).toHaveLength(2);
      expect(getZones()[0].name).toBe("Documents");
      expect(getZones()[1].name).toBe("Images");
      expect(isLoading()).toBe(false);
      expect(getError()).toBeNull();
    });

    it("should set loading state during fetch", async () => {
      let resolvePromise: (zones: BentoZone[]) => void;
      vi.mocked(ipc.listZones).mockImplementation(
        () => new Promise<BentoZone[]>((resolve) => { resolvePromise = resolve; })
      );

      const promise = loadZones();
      // Loading should be true while IPC call is in flight
      expect(isLoading()).toBe(true);

      resolvePromise!([]);
      await promise;

      expect(isLoading()).toBe(false);
    });

    it("should set error state when IPC fails", async () => {
      vi.mocked(ipc.listZones).mockRejectedValue(new Error("IPC timeout"));

      await loadZones();

      expect(getError()).toBe("IPC timeout");
      expect(isLoading()).toBe(false);
    });
  });

  // ── createZone ──

  describe("createZone", () => {
    it("should create a zone via IPC and push it to the store", async () => {
      // Pre-populate store
      vi.mocked(ipc.listZones).mockResolvedValue([]);
      await loadZones();

      const newZone = makeZone({ id: "zone-new", name: "New Zone" });
      vi.mocked(ipc.createZone).mockResolvedValue(newZone);

      const result = await createZone(
        "New Zone", "🆕",
        { x_percent: 50, y_percent: 50 },
        { w_percent: 20, h_percent: 30 }
      );

      expect(result).not.toBeNull();
      expect(result!.id).toBe("zone-new");
      expect(getZones()).toHaveLength(1);
      expect(getZones()[0].name).toBe("New Zone");
    });

    it("should return null and set error on IPC failure", async () => {
      vi.mocked(ipc.listZones).mockResolvedValue([]);
      await loadZones();

      vi.mocked(ipc.createZone).mockRejectedValue(new Error("Duplicate name"));

      const result = await createZone(
        "Test", "🔥",
        { x_percent: 0, y_percent: 0 },
        { w_percent: 10, h_percent: 10 }
      );

      expect(result).toBeNull();
      expect(getError()).toBe("Duplicate name");
    });
  });

  // ── updateZone ──

  describe("updateZone", () => {
    it("should update a zone in-place using produce", async () => {
      const zone = makeZone();
      vi.mocked(ipc.listZones).mockResolvedValue([zone]);
      await loadZones();

      const updated = makeZone({ name: "Renamed Docs", icon: "📁" });
      vi.mocked(ipc.updateZone).mockResolvedValue(updated);

      const result = await updateZone("zone-1", { name: "Renamed Docs", icon: "📁" });

      expect(result).not.toBeNull();
      expect(getZoneById("zone-1")?.name).toBe("Renamed Docs");
      expect(getZoneById("zone-1")?.icon).toBe("📁");
    });

    it("should return null on IPC failure", async () => {
      vi.mocked(ipc.listZones).mockResolvedValue([makeZone()]);
      await loadZones();

      vi.mocked(ipc.updateZone).mockRejectedValue(new Error("Not found"));

      const result = await updateZone("zone-1", { name: "X" });
      expect(result).toBeNull();
      expect(getError()).toBe("Not found");
    });
  });

  // ── deleteZone ──

  describe("deleteZone", () => {
    it("should remove the zone from the store after IPC success", async () => {
      vi.mocked(ipc.listZones).mockResolvedValue([
        makeZone(),
        makeZone({ id: "zone-2", name: "Images", sort_order: 1 }),
      ]);
      await loadZones();
      expect(getZones()).toHaveLength(2);

      vi.mocked(ipc.deleteZone).mockResolvedValue(undefined);

      const ok = await deleteZone("zone-1");
      expect(ok).toBe(true);
      expect(getZones()).toHaveLength(1);
      expect(getZones()[0].id).toBe("zone-2");
    });

    it("should return false on IPC failure", async () => {
      vi.mocked(ipc.listZones).mockResolvedValue([makeZone()]);
      await loadZones();

      vi.mocked(ipc.deleteZone).mockRejectedValue(new Error("Permission denied"));

      const ok = await deleteZone("zone-1");
      expect(ok).toBe(false);
      expect(getError()).toBe("Permission denied");
      // Zone should still exist
      expect(getZones()).toHaveLength(1);
    });
  });

  // ── reorderZones ──

  describe("reorderZones", () => {
    it("should reorder zones and update sort_order", async () => {
      vi.mocked(ipc.listZones).mockResolvedValue([
        makeZone({ id: "a", name: "A", sort_order: 0 }),
        makeZone({ id: "b", name: "B", sort_order: 1 }),
        makeZone({ id: "c", name: "C", sort_order: 2 }),
      ]);
      await loadZones();

      vi.mocked(ipc.reorderZones).mockResolvedValue(undefined);

      const ok = await reorderZones(["c", "a", "b"]);
      expect(ok).toBe(true);
      expect(getZones()[0].id).toBe("c");
      expect(getZones()[0].sort_order).toBe(0);
      expect(getZones()[1].id).toBe("a");
      expect(getZones()[1].sort_order).toBe(1);
      expect(getZones()[2].id).toBe("b");
      expect(getZones()[2].sort_order).toBe(2);
    });
  });

  // ── addItem ──

  describe("addItem", () => {
    it("should add an item to the correct zone", async () => {
      vi.mocked(ipc.listZones).mockResolvedValue([makeZone({ items: [] })]);
      await loadZones();

      const item = makeItem();
      vi.mocked(ipc.addItem).mockResolvedValue(item);

      const result = await addItem("zone-1", "C:\\Users\\test\\Desktop\\readme.txt");
      expect(result).not.toBeNull();
      expect(result!.id).toBe("item-1");
      expect(getZoneById("zone-1")?.items).toHaveLength(1);
      expect(getZoneById("zone-1")?.items[0].name).toBe("readme.txt");
    });

    it("should return null on IPC failure", async () => {
      vi.mocked(ipc.listZones).mockResolvedValue([makeZone()]);
      await loadZones();

      vi.mocked(ipc.addItem).mockRejectedValue(new Error("File not found"));

      const result = await addItem("zone-1", "C:\\bad\\path");
      expect(result).toBeNull();
      expect(getError()).toBe("File not found");
    });
  });

  // ── removeItem ──

  describe("removeItem", () => {
    it("should remove an item from the zone", async () => {
      const item = makeItem();
      vi.mocked(ipc.listZones).mockResolvedValue([makeZone({ items: [item] })]);
      await loadZones();
      expect(getZoneById("zone-1")?.items).toHaveLength(1);

      vi.mocked(ipc.removeItem).mockResolvedValue(undefined);

      const ok = await removeItem("zone-1", "item-1");
      expect(ok).toBe(true);
      expect(getZoneById("zone-1")?.items).toHaveLength(0);
    });
  });

  // ── moveItem ──

  describe("moveItem", () => {
    it("should move an item between zones using produce", async () => {
      const item = makeItem({ zone_id: "zone-1" });
      vi.mocked(ipc.listZones).mockResolvedValue([
        makeZone({ id: "zone-1", items: [item] }),
        makeZone({ id: "zone-2", name: "Target", items: [], sort_order: 1 }),
      ]);
      await loadZones();

      vi.mocked(ipc.moveItem).mockResolvedValue(undefined);

      const ok = await moveItem("zone-1", "zone-2", "item-1");
      expect(ok).toBe(true);
      expect(getZoneById("zone-1")?.items).toHaveLength(0);
      expect(getZoneById("zone-2")?.items).toHaveLength(1);
      expect(getZoneById("zone-2")?.items[0].id).toBe("item-1");
    });

    it("should return false on IPC failure", async () => {
      vi.mocked(ipc.listZones).mockResolvedValue([
        makeZone({ id: "zone-1", items: [makeItem()] }),
        makeZone({ id: "zone-2", items: [], sort_order: 1 }),
      ]);
      await loadZones();

      vi.mocked(ipc.moveItem).mockRejectedValue(new Error("Move failed"));

      const ok = await moveItem("zone-1", "zone-2", "item-1");
      expect(ok).toBe(false);
      expect(getError()).toBe("Move failed");
      // Item should still be in original zone
      expect(getZoneById("zone-1")?.items).toHaveLength(1);
    });
  });

  // ── reorderItems ──

  describe("reorderItems", () => {
    it("should reorder items within a zone", async () => {
      const items = [
        makeItem({ id: "i1", name: "first" }),
        makeItem({ id: "i2", name: "second" }),
        makeItem({ id: "i3", name: "third" }),
      ];
      vi.mocked(ipc.listZones).mockResolvedValue([makeZone({ items })]);
      await loadZones();

      vi.mocked(ipc.reorderItems).mockResolvedValue(undefined);

      const ok = await reorderItems("zone-1", ["i3", "i1", "i2"]);
      expect(ok).toBe(true);

      const zone = getZoneById("zone-1")!;
      expect(zone.items[0].id).toBe("i3");
      expect(zone.items[1].id).toBe("i1");
      expect(zone.items[2].id).toBe("i2");
    });
  });

  // ── toggleItemWide ──

  describe("toggleItemWide", () => {
    it("should update the item in the store with the toggled value", async () => {
      const item = makeItem({ is_wide: false });
      vi.mocked(ipc.listZones).mockResolvedValue([makeZone({ items: [item] })]);
      await loadZones();

      const updatedItem = makeItem({ is_wide: true, grid_position: { col: 0, row: 0, col_span: 2 } });
      vi.mocked(ipc.toggleItemWide).mockResolvedValue(updatedItem);

      const result = await toggleItemWide("zone-1", "item-1");
      expect(result).not.toBeNull();
      expect(result!.is_wide).toBe(true);
      expect(getZoneById("zone-1")?.items[0].is_wide).toBe(true);
    });
  });

  // ── zonesStore reactive accessor ──

  describe("zonesStore reactive accessor", () => {
    it("should expose zones, loading, and error properties", async () => {
      vi.mocked(ipc.listZones).mockResolvedValue([makeZone()]);
      await loadZones();

      expect(zonesStore.zones).toHaveLength(1);
      expect(zonesStore.loading).toBe(false);
      expect(zonesStore.error).toBeNull();
    });
  });

  // ── getZoneById ──

  describe("getZoneById", () => {
    it("should return undefined for non-existent zone", async () => {
      vi.mocked(ipc.listZones).mockResolvedValue([makeZone()]);
      await loadZones();

      expect(getZoneById("non-existent")).toBeUndefined();
    });

    it("should return the correct zone", async () => {
      vi.mocked(ipc.listZones).mockResolvedValue([
        makeZone({ id: "zone-1" }),
        makeZone({ id: "zone-2", name: "Other" }),
      ]);
      await loadZones();

      const zone = getZoneById("zone-2");
      expect(zone).toBeDefined();
      expect(zone!.name).toBe("Other");
    });
  });
});
