/**
 * Tests for the global-shortcut wrapper.
 *
 * The plugin module is mocked so registration is purely a JS-side
 * interaction. We verify:
 *  - reserved-accelerator rejection (blacklist)
 *  - accelerator normalization (Win+... → Super+...)
 *  - conflict surfacing on plugin-level failure
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const mockRegister = vi.fn<(acc: string, cb: unknown) => Promise<void>>();
const mockUnregister = vi.fn<(acc: string) => Promise<void>>().mockResolvedValue(undefined);
const mockUnregisterAll = vi.fn<() => Promise<void>>().mockResolvedValue(undefined);
const mockIsRegistered = vi.fn<(acc: string) => Promise<boolean>>().mockResolvedValue(false);

vi.mock("@tauri-apps/plugin-global-shortcut", () => ({
  register: (acc: string, cb: unknown) => mockRegister(acc, cb),
  unregister: (acc: string) => mockUnregister(acc),
  unregisterAll: () => mockUnregisterAll(),
  isRegistered: (acc: string) => mockIsRegistered(acc),
}));

import {
  registerBinding,
  isReservedAccelerator,
  normalizeAccelerator,
  applyBindings,
  clearAll,
} from "../globalHotkeys";

describe("globalHotkeys", () => {
  beforeEach(() => {
    mockRegister.mockReset();
    mockUnregister.mockClear();
    mockUnregisterAll.mockClear();
  });

  it("normalizeAccelerator maps Win → Super and Cmd → Super", () => {
    expect(normalizeAccelerator("Win+L")).toBe("Super+L");
    expect(normalizeAccelerator("Cmd+Space")).toBe("Super+Space");
    expect(normalizeAccelerator("CmdOrCtrl+K")).toBe("Control+K");
  });

  it("isReservedAccelerator blacklists Win+L and Alt+Tab", () => {
    expect(isReservedAccelerator("Win+L")).toBe(true);
    expect(isReservedAccelerator("Super+L")).toBe(true);
    expect(isReservedAccelerator("Alt+Tab")).toBe(true);
    expect(isReservedAccelerator("Control+Shift+N")).toBe(false);
  });

  it("registerBinding rejects reserved accelerators before hitting the plugin", async () => {
    const res = await registerBinding("test.reserved", "Win+L", () => {});
    expect(res).not.toBeNull();
    expect(res?.reason).toBe("reserved");
    expect(mockRegister).not.toHaveBeenCalled();
  });

  it("registerBinding surfaces plugin errors as taken conflicts", async () => {
    mockRegister.mockRejectedValueOnce(new Error("hotkey already registered"));
    const res = await registerBinding("test.taken", "Control+Shift+T", () => {});
    expect(res?.reason).toBe("taken");
  });

  it("applyBindings invokes register for each action with a handler", async () => {
    mockRegister.mockResolvedValue(undefined);
    const conflicts = await applyBindings(
      {
        "a.one": "Control+1",
        "a.two": "Control+2",
      },
      () => () => {}
    );
    expect(conflicts).toEqual([]);
    expect(mockRegister).toHaveBeenCalledTimes(2);
    await clearAll();
  });
});
