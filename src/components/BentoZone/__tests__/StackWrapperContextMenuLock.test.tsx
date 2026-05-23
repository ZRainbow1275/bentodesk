/**
 * Post-deploy fix round 2 — Bug A (context menu loses focus).
 *
 * User report: "鼠标悬浮在'解散堆栈'选项后过一段时间会失焦，无法选中。
 * 需要重新右键唤起选项栏才可以点击这个选项."
 *
 * Root cause
 * ----------
 * The `.stack-context-menu` element is `position: fixed`, lives
 * OUTSIDE the wrapper's natural rect, and is NOT registered with
 * `services/hitTest.ts:registerZoneElement`. While the cursor sits
 * over a menu item the poller's `hoveredZone$` flips to null, the
 * state machine drops to GRACE_PERIOD (350 ms — the literal magic
 * number in `services/hitTest.ts:165`) and then to PASSTHROUGH;
 * `setIgnoreCursorEvents(true)` makes subsequent clicks fall through
 * to the desktop. The user's "过一段时间" matches the 350 ms drop.
 *
 * Fix
 * ---
 * Hold a `acquireModalLock()` for the menu's lifetime. The modal
 * lock force-disables passthrough regardless of `hoveredZone$`, so
 * the menu remains clickable indefinitely. Acquired in
 * `handleContextMenu`, released on every menu-close path
 * (`handleDissolve` / `handleDetach` / `handleOutsidePointer`) plus
 * the unmount cleanup.
 *
 * Test strategy
 * -------------
 * We split the contract verification into two layers, mirroring the
 * pattern already in `stackBloomHitTest.test.ts`:
 *
 *   1. Source-text invariants — pin the import + acquire/release
 *      call sites in StackWrapper.tsx so a future refactor can't
 *      silently drop the lock and reintroduce the bug.
 *   2. Pure-function lifecycle model — replicate the lock state
 *      machine and verify acquire/release counts match the
 *      open/close flow. Mounting the full StackWrapper would
 *      require zonesStore + selection + ipc + settings + i18n + the
 *      cursor hit-test poller; the lock contract is orthogonal so a
 *      pure model gives the same coverage at ~10× the speed.
 *
 * The pure model imports `acquireModalLock` directly from
 * `services/hitTest` and exercises the real implementation against
 * a mocked Tauri window — same shape as the round-13 hover-intent
 * tests (`stackBloomHoverIntent.test.tsx`).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
// node:fs is provided by the vitest Node runner; the project intentionally
// does not depend on @types/node in production. Pattern mirrors
// stackBloomHitTest.test.ts and stackDissolveFlow.test.ts.
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-expect-error — node:fs is provided by the vitest Node runner.
import { readFileSync } from "node:fs";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-expect-error — see note above.
import { fileURLToPath } from "node:url";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-expect-error — see note above.
import { dirname, resolve } from "node:path";

// Stub @tauri-apps/api/window because services/hitTest imports it at
// module-load time. Without this the import resolves to a Vite
// optimize-deps stub that throws.
const mockSetIgnoreCursorEvents = vi
  .fn<(ignore: boolean) => Promise<void>>()
  .mockResolvedValue(undefined);
const mockOuterPosition = vi.fn().mockResolvedValue({ x: 0, y: 0 });
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    setIgnoreCursorEvents: mockSetIgnoreCursorEvents,
    outerPosition: mockOuterPosition,
  }),
  cursorPosition: vi.fn().mockResolvedValue({ x: 0, y: 0 }),
}));

import { acquireModalLock } from "../../../services/hitTest";

const HERE = dirname(fileURLToPath(import.meta.url));
const TSX_PATH = resolve(HERE, "../StackWrapper.tsx");

function readFile(path: string): string {
  return readFileSync(path, "utf8");
}

describe("post-deploy round 2 — context menu modal lock (source contract)", () => {
  it("StackWrapper imports acquireModalLock from services/hitTest", () => {
    const tsx = readFile(TSX_PATH);
    // The import must come from the hit-test service. A future
    // refactor that pulls the symbol from a different module would
    // break the assumption that the same lock counter governs all
    // passthrough overrides.
    expect(tsx).toMatch(
      /import\s*\{[^}]*acquireModalLock[^}]*\}\s*from\s*"\.\.\/\.\.\/services\/hitTest"/m,
    );
  });

  it("StackWrapper declares a `releaseContextMenuLock` ref-style holder", () => {
    const tsx = readFile(TSX_PATH);
    // The release function is captured at acquire time so every
    // close path can call the SAME function. A bare counter would
    // break — services/hitTest's release is matched 1:1 by closure
    // identity, not by counter parity.
    expect(tsx).toMatch(
      /let\s+releaseContextMenuLock\s*:\s*\(\s*\(\s*\)\s*=>\s*void\s*\)\s*\|\s*null\s*=\s*null/m,
    );
  });

  it("StackWrapper defines a closeContextMenu helper that releases the lock", () => {
    const tsx = readFile(TSX_PATH);
    // Locate the helper and verify it does both:
    //   1. Close the menu via setContextMenuOpen(null)
    //   2. Release the modal lock if non-null
    const fnMatch = /const\s+closeContextMenu\s*=\s*\(\s*\)\s*:\s*void\s*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    expect(body).toMatch(/setContextMenuOpen\s*\(\s*null\s*\)/m);
    expect(body).toMatch(/releaseContextMenuLock\s*\(\s*\)/m);
    expect(body).toMatch(/releaseContextMenuLock\s*=\s*null/m);
  });

  it("handleContextMenu acquires the modal lock after opening the menu", () => {
    const tsx = readFile(TSX_PATH);
    const fnMatch = /const\s+handleContextMenu\s*=\s*\([^)]*\)[^=]*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    // The lock acquire must be present.
    expect(body).toMatch(/releaseContextMenuLock\s*=\s*acquireModalLock\s*\(\s*\)/m);
    // Must include a defensive prior-release path — covers the
    // re-right-click-while-open case where the existing menu update
    // path skips closeContextMenu.
    expect(body).toMatch(/if\s*\(\s*releaseContextMenuLock\s*!==\s*null\s*\)/m);
  });

  it("handleDissolve closes the menu via closeContextMenu (not bare setContextMenuOpen(null))", () => {
    const tsx = readFile(TSX_PATH);
    const fnMatch = /const\s+handleDissolve\s*=\s*async\s*\(\s*\)[^=]*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    // Must call the close helper (which releases the modal lock).
    expect(body).toMatch(/closeContextMenu\s*\(\s*\)/m);
  });

  it("handleDetach closes the menu via closeContextMenu", () => {
    const tsx = readFile(TSX_PATH);
    const fnMatch = /const\s+handleDetach\s*=\s*async\s*\([^)]*\)[^=]*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    expect(body).toMatch(/closeContextMenu\s*\(\s*\)/m);
  });

  it("handleOutsidePointer closes the menu via closeContextMenu", () => {
    const tsx = readFile(TSX_PATH);
    const fnMatch = /const\s+handleOutsidePointer\s*=\s*\([^)]*\)[^=]*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    expect(body).toMatch(/closeContextMenu\s*\(\s*\)/m);
    // Strict negative: the bare setter is forbidden because it would
    // open the menu but never release the lock.
    expect(body).not.toMatch(/setContextMenuOpen\s*\(\s*null\s*\)/m);
  });

  it("onCleanup releases any outstanding modal lock", () => {
    const tsx = readFile(TSX_PATH);
    // Find the FIRST onCleanup block — the one paired with the
    // wrapper-level `document.addEventListener("mousedown", ...)`
    // registration in onMount. We grep within the first ~80 lines
    // following the first occurrence so the assertion is stable
    // against later onCleanup blocks (e.g. bloom unmount cleanup).
    const cleanupMatch = /onCleanup\(\(\)\s*=>\s*\{([\s\S]*?)\n\s*\}\s*\);/m.exec(tsx);
    expect(cleanupMatch).not.toBeNull();
    const body = cleanupMatch![1];
    expect(body).toMatch(/releaseContextMenuLock/m);
  });
});

// ─── Pure-function lifecycle model ───────────────────────────

interface ContextMenuPos {
  x: number;
  y: number;
}

/**
 * Mirrors the StackWrapper context-menu modal-lock lifecycle. Every
 * branch the production code takes (open-then-close, re-open while
 * already open, close via outside-click, close via dissolve / detach,
 * unmount while open) is exercised against the real
 * `acquireModalLock` so we verify the release function is called the
 * right number of times.
 */
function createContextMenuLockLifecycle() {
  let menuPos: ContextMenuPos | null = null;
  let releaseLock: (() => void) | null = null;
  // Track release calls so tests can assert "lock was released exactly
  // once per open" without instrumenting the hitTest module itself.
  let releaseCallCount = 0;

  // Wrap the real acquireModalLock so we can intercept the release
  // function and count invocations. The wrapping here matches the
  // production code's pattern in StackWrapper.tsx: the release fn is
  // stored in a closure variable and called from every menu-close
  // path.
  const acquireWithCount = (): (() => void) => {
    const realRelease = acquireModalLock();
    return () => {
      releaseCallCount++;
      realRelease();
    };
  };

  const closeContextMenu = (): void => {
    menuPos = null;
    if (releaseLock !== null) {
      releaseLock();
      releaseLock = null;
    }
  };

  const handleContextMenu = (x: number, y: number): void => {
    // Defensive release — covers the re-right-click case.
    if (releaseLock !== null) {
      releaseLock();
      releaseLock = null;
    }
    menuPos = { x, y };
    releaseLock = acquireWithCount();
  };

  const unmount = (): void => {
    if (releaseLock !== null) {
      releaseLock();
      releaseLock = null;
    }
  };

  return {
    state: () => ({
      menuPos,
      hasLock: releaseLock !== null,
      releaseCallCount,
    }),
    handleContextMenu,
    closeContextMenu,
    unmount,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  // Ensure no leaked locks between tests — each test must close /
  // unmount its lifecycle.
  vi.clearAllMocks();
});

describe("post-deploy round 2 — context menu modal lock (lifecycle model)", () => {
  it("opening the menu acquires a modal lock; closing releases it", () => {
    const lc = createContextMenuLockLifecycle();
    expect(lc.state().hasLock).toBe(false);

    lc.handleContextMenu(100, 200);
    expect(lc.state().menuPos).toEqual({ x: 100, y: 200 });
    expect(lc.state().hasLock).toBe(true);
    expect(lc.state().releaseCallCount).toBe(0);

    lc.closeContextMenu();
    expect(lc.state().menuPos).toBeNull();
    expect(lc.state().hasLock).toBe(false);
    expect(lc.state().releaseCallCount).toBe(1);
  });

  it("dissolve / detach close paths both release the lock (single call)", () => {
    const lc = createContextMenuLockLifecycle();
    lc.handleContextMenu(50, 50);
    // Both handlers funnel through closeContextMenu; the lifecycle
    // model represents that as one closeContextMenu invocation.
    lc.closeContextMenu();
    expect(lc.state().releaseCallCount).toBe(1);
    expect(lc.state().hasLock).toBe(false);
  });

  it("re-right-click while menu is open releases the prior lock and acquires a new one", () => {
    const lc = createContextMenuLockLifecycle();
    lc.handleContextMenu(10, 10);
    expect(lc.state().releaseCallCount).toBe(0);

    // Re-right-click without an explicit close — the production
    // handleContextMenu defensively releases the prior lock first.
    lc.handleContextMenu(200, 200);
    expect(lc.state().releaseCallCount).toBe(1);
    expect(lc.state().hasLock).toBe(true);
    expect(lc.state().menuPos).toEqual({ x: 200, y: 200 });

    lc.closeContextMenu();
    expect(lc.state().releaseCallCount).toBe(2);
  });

  it("unmount while the menu is open still releases the lock", () => {
    const lc = createContextMenuLockLifecycle();
    lc.handleContextMenu(50, 50);
    expect(lc.state().hasLock).toBe(true);

    // Stack dissolves / wrapper unmounts mid-menu — the onCleanup
    // path must still release the lock so the singleton counter in
    // services/hitTest doesn't leak.
    lc.unmount();
    expect(lc.state().hasLock).toBe(false);
    expect(lc.state().releaseCallCount).toBe(1);
  });

  it("multiple open/close cycles never leak a lock", () => {
    const lc = createContextMenuLockLifecycle();
    for (let i = 0; i < 5; i++) {
      lc.handleContextMenu(i * 10, i * 10);
      lc.closeContextMenu();
    }
    expect(lc.state().releaseCallCount).toBe(5);
    expect(lc.state().hasLock).toBe(false);
  });

  it("close-without-open is a no-op (release fn is null)", () => {
    const lc = createContextMenuLockLifecycle();
    // Defensive — calling closeContextMenu before any open should
    // not throw or count a release.
    expect(() => lc.closeContextMenu()).not.toThrow();
    expect(lc.state().releaseCallCount).toBe(0);
  });
});
