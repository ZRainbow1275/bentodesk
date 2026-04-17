/**
 * Tests for IPC service wrappers.
 *
 * Mocks @tauri-apps/api/core invoke to verify each wrapper passes
 * the correct command name and argument shape to the Tauri backend.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// ─── Mock Tauri core ────────────────────────────────────────

const mockInvoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Import after mock
import * as ipc from "../ipc";

// ─── Tests ──────────────────────────────────────────────────

describe("ipc service", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(undefined);
  });

  // ── Zone Management ──

  describe("Zone Management", () => {
    it("createZone calls invoke with correct command and args", async () => {
      const fakeZone = { id: "z1" };
      mockInvoke.mockResolvedValue(fakeZone);

      const pos = { x_percent: 10, y_percent: 20 };
      const size = { w_percent: 30, h_percent: 40 };
      const result = await ipc.createZone("Docs", "📄", pos, size);

      expect(mockInvoke).toHaveBeenCalledWith("create_zone", {
        name: "Docs",
        icon: "📄",
        position: pos,
        expandedSize: size,
      });
      expect(result).toBe(fakeZone);
    });

    it("updateZone calls invoke with correct command and args", async () => {
      const updated = { id: "z1", name: "Renamed" };
      mockInvoke.mockResolvedValue(updated);

      const updates = { name: "Renamed" };
      const result = await ipc.updateZone("z1", updates);

      expect(mockInvoke).toHaveBeenCalledWith("update_zone", { id: "z1", updates });
      expect(result).toBe(updated);
    });

    it("deleteZone calls invoke with correct command", async () => {
      await ipc.deleteZone("z1");
      expect(mockInvoke).toHaveBeenCalledWith("delete_zone", { id: "z1" });
    });

    it("listZones calls invoke with correct command", async () => {
      const zones = [{ id: "z1" }, { id: "z2" }];
      mockInvoke.mockResolvedValue(zones);

      const result = await ipc.listZones();
      expect(mockInvoke).toHaveBeenCalledWith("list_zones");
      expect(result).toBe(zones);
    });

    it("reorderZones calls invoke with correct command and args", async () => {
      await ipc.reorderZones(["z2", "z1"]);
      expect(mockInvoke).toHaveBeenCalledWith("reorder_zones", { zoneIds: ["z2", "z1"] });
    });
  });

  // ── Item Management ──

  describe("Item Management", () => {
    it("addItem calls invoke with correct command and args", async () => {
      const item = { id: "i1" };
      mockInvoke.mockResolvedValue(item);

      const result = await ipc.addItem("z1", "C:\\file.txt");
      expect(mockInvoke).toHaveBeenCalledWith("add_item", { zoneId: "z1", path: "C:\\file.txt" });
      expect(result).toBe(item);
    });

    it("removeItem calls invoke with correct command and args", async () => {
      await ipc.removeItem("z1", "i1");
      expect(mockInvoke).toHaveBeenCalledWith("remove_item", { zoneId: "z1", itemId: "i1" });
    });

    it("moveItem calls invoke with correct command and args", async () => {
      await ipc.moveItem("z1", "z2", "i1");
      expect(mockInvoke).toHaveBeenCalledWith("move_item", {
        fromZoneId: "z1",
        toZoneId: "z2",
        itemId: "i1",
      });
    });

    it("reorderItems calls invoke with correct command and args", async () => {
      await ipc.reorderItems("z1", ["i3", "i1", "i2"]);
      expect(mockInvoke).toHaveBeenCalledWith("reorder_items", {
        zoneId: "z1",
        itemIds: ["i3", "i1", "i2"],
      });
    });

    it("toggleItemWide calls invoke with correct command and args", async () => {
      const item = { id: "i1", is_wide: true };
      mockInvoke.mockResolvedValue(item);

      const result = await ipc.toggleItemWide("z1", "i1");
      expect(mockInvoke).toHaveBeenCalledWith("toggle_item_wide", { zoneId: "z1", itemId: "i1" });
      expect(result).toBe(item);
    });
  });

  // ── File Operations ──

  describe("File Operations", () => {
    it("openFile calls invoke with correct command", async () => {
      await ipc.openFile("C:\\readme.txt");
      expect(mockInvoke).toHaveBeenCalledWith("open_file", { path: "C:\\readme.txt" });
    });

    it("revealInExplorer calls invoke with correct command", async () => {
      await ipc.revealInExplorer("C:\\folder");
      expect(mockInvoke).toHaveBeenCalledWith("reveal_in_explorer", { path: "C:\\folder" });
    });

    it("getFileInfo calls invoke with correct command", async () => {
      const info = { name: "file.txt", size: 1024 };
      mockInvoke.mockResolvedValue(info);

      const result = await ipc.getFileInfo("C:\\file.txt");
      expect(mockInvoke).toHaveBeenCalledWith("get_file_info", { path: "C:\\file.txt" });
      expect(result).toBe(info);
    });
  });

  // ── Icon Management ──

  describe("Icon Management", () => {
    it("getIconUrl calls invoke with correct command", async () => {
      mockInvoke.mockResolvedValue("data:image/png;base64,...");

      const result = await ipc.getIconUrl("C:\\app.exe");
      expect(mockInvoke).toHaveBeenCalledWith("get_icon_url", { path: "C:\\app.exe" });
      expect(result).toBe("data:image/png;base64,...");
    });

    it("preloadIcons calls invoke with correct command and args", async () => {
      await ipc.preloadIcons(["C:\\a.exe", "C:\\b.exe"]);
      expect(mockInvoke).toHaveBeenCalledWith("preload_icons", {
        paths: ["C:\\a.exe", "C:\\b.exe"],
      });
    });

    it("clearIconCache calls invoke with correct command", async () => {
      await ipc.clearIconCache();
      expect(mockInvoke).toHaveBeenCalledWith("clear_icon_cache");
    });
  });

  // ── Snapshot Management ──

  describe("Snapshot Management", () => {
    it("saveSnapshot calls invoke with correct command", async () => {
      const snapshot = { id: "s1", name: "My Layout" };
      mockInvoke.mockResolvedValue(snapshot);

      const result = await ipc.saveSnapshot("My Layout");
      expect(mockInvoke).toHaveBeenCalledWith("save_snapshot", { name: "My Layout" });
      expect(result).toBe(snapshot);
    });

    it("loadSnapshot calls invoke with correct command", async () => {
      await ipc.loadSnapshot("s1");
      expect(mockInvoke).toHaveBeenCalledWith("load_snapshot", { id: "s1" });
    });

    it("listSnapshots calls invoke with correct command", async () => {
      mockInvoke.mockResolvedValue([]);
      const result = await ipc.listSnapshots();
      expect(mockInvoke).toHaveBeenCalledWith("list_snapshots");
      expect(result).toEqual([]);
    });

    it("deleteSnapshot calls invoke with correct command", async () => {
      await ipc.deleteSnapshot("s1");
      expect(mockInvoke).toHaveBeenCalledWith("delete_snapshot", { id: "s1" });
    });
  });

  // ── Smart Grouping ──

  describe("Smart Grouping", () => {
    it("scanDesktop calls invoke with correct command", async () => {
      mockInvoke.mockResolvedValue([]);
      const result = await ipc.scanDesktop();
      expect(mockInvoke).toHaveBeenCalledWith("scan_desktop");
      expect(result).toEqual([]);
    });

    it("suggestGroups calls invoke with correct command and args", async () => {
      mockInvoke.mockResolvedValue([]);
      const files = ["C:\\a.txt", "C:\\b.pdf"];
      const result = await ipc.suggestGroups(files);
      expect(mockInvoke).toHaveBeenCalledWith("suggest_groups", { files });
      expect(result).toEqual([]);
    });

    it("applyAutoGroup calls invoke with correct command and args", async () => {
      mockInvoke.mockResolvedValue([]);
      const rule = { rule_type: "Extension" as const, pattern: null, extensions: [".txt"] };
      const result = await ipc.applyAutoGroup("z1", rule);
      expect(mockInvoke).toHaveBeenCalledWith("apply_auto_group", {
        zoneId: "z1",
        rule,
        selectedPaths: null,
      });
      expect(result).toEqual([]);
    });

    it("autoGroupNewFile calls invoke with correct command and args", async () => {
      mockInvoke.mockResolvedValue([]);
      const result = await ipc.autoGroupNewFile("C:\\new-file.txt");
      expect(mockInvoke).toHaveBeenCalledWith("auto_group_new_file", { filePath: "C:\\new-file.txt" });
      expect(result).toEqual([]);
    });
  });

  // ── Settings ──

  describe("Settings", () => {
    it("getSettings calls invoke with correct command", async () => {
      const settings = { theme: "Dark" };
      mockInvoke.mockResolvedValue(settings);

      const result = await ipc.getSettings();
      expect(mockInvoke).toHaveBeenCalledWith("get_settings");
      expect(result).toBe(settings);
    });

    it("updateSettings calls invoke with correct command and args", async () => {
      const updates = { theme: "Light" as const };
      const newSettings = { theme: "Light" };
      mockInvoke.mockResolvedValue(newSettings);

      const result = await ipc.updateSettings(updates);
      expect(mockInvoke).toHaveBeenCalledWith("update_settings", { updates });
      expect(result).toBe(newSettings);
    });
  });

  // ── System ──

  describe("System", () => {
    it("getSystemInfo calls invoke with correct command", async () => {
      const info = { os_version: "Windows 11" };
      mockInvoke.mockResolvedValue(info);

      const result = await ipc.getSystemInfo();
      expect(mockInvoke).toHaveBeenCalledWith("get_system_info");
      expect(result).toBe(info);
    });

    it("startDrag calls invoke with correct command and args", async () => {
      mockInvoke.mockResolvedValue("drag-id");
      const result = await ipc.startDrag(["C:\\a.txt", "C:\\b.txt"]);
      expect(mockInvoke).toHaveBeenCalledWith("start_drag", {
        filePaths: ["C:\\a.txt", "C:\\b.txt"],
      });
      expect(result).toBe("drag-id");
    });

    it("getMemoryUsage calls invoke with correct command", async () => {
      const mem = { working_set_bytes: 1024, peak_working_set_bytes: 2048 };
      mockInvoke.mockResolvedValue(mem);

      const result = await ipc.getMemoryUsage();
      expect(mockInvoke).toHaveBeenCalledWith("get_memory_usage");
      expect(result).toBe(mem);
    });
  });

  // ── JSON Theme Plugin ──

  describe("JSON Theme Plugin", () => {
    it("listThemes calls invoke with correct command", async () => {
      mockInvoke.mockResolvedValue([]);
      const result = await ipc.listThemes();
      expect(mockInvoke).toHaveBeenCalledWith("list_themes");
      expect(result).toEqual([]);
    });

    it("getTheme calls invoke with correct command and args", async () => {
      const theme = { id: "ocean-blue", name: "Ocean Blue" };
      mockInvoke.mockResolvedValue(theme);

      const result = await ipc.getTheme("ocean-blue");
      expect(mockInvoke).toHaveBeenCalledWith("get_theme", { id: "ocean-blue" });
      expect(result).toBe(theme);
    });

    it("getActiveTheme calls invoke with correct command", async () => {
      const theme = { id: "default", name: "Default" };
      mockInvoke.mockResolvedValue(theme);

      const result = await ipc.getActiveTheme();
      expect(mockInvoke).toHaveBeenCalledWith("get_active_theme");
      expect(result).toBe(theme);
    });

    it("setActiveTheme calls invoke with correct command and args", async () => {
      const theme = { id: "ocean-blue", name: "Ocean Blue" };
      mockInvoke.mockResolvedValue(theme);

      const result = await ipc.setActiveTheme("ocean-blue");
      expect(mockInvoke).toHaveBeenCalledWith("set_active_theme", { id: "ocean-blue" });
      expect(result).toBe(theme);
    });
  });

  // ── Error propagation ──

  describe("Error propagation", () => {
    it("should propagate IPC errors to the caller", async () => {
      mockInvoke.mockRejectedValue(new Error("Backend crashed"));

      await expect(ipc.listZones()).rejects.toThrow("Backend crashed");
    });

    it("should propagate non-Error rejections", async () => {
      mockInvoke.mockRejectedValue("string error");

      await expect(ipc.deleteZone("z1")).rejects.toBe("string error");
    });
  });
});
