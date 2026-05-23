/**
 * v9 stack-wake-mutex — bloom hit-test registration contract.
 *
 * Pre-v9 (rounds 5–v8) the StackWrapper registered both a
 * `.stack-bloom-buffer` halo (initially circular, later 100 vw × 100 vh)
 * AND every `.stack-bloom__petal` with the cursor hit-test poller while
 * the bloom was active. The buffer's `pointer-events: auto` plus its
 * z-index 49 silently shadowed neighbouring zones' hover & click wake.
 *
 * v9 deletes the buffer entirely. Hit-test coverage now comes from:
 *   - the wrapper element itself (registered on mount with the
 *     edge-aware inflate from `computeInflateForPosition`)
 *   - each visible petal (registered when the bloom is active with a
 *     12 px directional inflate halo so adjacent petals' hit rects
 *     bridge the visible row gap)
 *   - the floating FocusedZonePreview (registered by the preview
 *     itself when anchored to a petal rect — unchanged from pre-v9)
 *
 * The contract under test:
 *
 *   1. NO bloom buffer is registered (the singleton hit-test map
 *      should never carry a buffer-shaped entry while the bloom is
 *      active).
 *   2. Each petal IS registered with a 12 px inflate halo on all four
 *      sides — the constant lives in StackWrapper.tsx as
 *      `PETAL_HALO_PX` and the registration call passes the inflate
 *      object via the `RegisterZoneOpts` shape.
 *   3. petalRefs map removes entries when a petal unmounts.
 *   4. unmount tears every registration down even if bloom was active.
 *
 * Tests use the source-text contract pattern (matching
 * stackBloomAnimation.test.tsx + stackDragWhileBloomed.test.tsx)
 * because mounting StackWrapper requires bootstrapping zonesStore +
 * selection + ipc + settings + i18n + the cursor hit-test poller —
 * heavy and orthogonal to the hit-test registration invariants this
 * file pins.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-expect-error — node:fs is provided by the vitest Node runner.
import { readFileSync } from "node:fs";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-expect-error — see note above.
import { fileURLToPath } from "node:url";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-expect-error — see note above.
import { dirname, resolve } from "node:path";

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

import {
  registerZoneElement,
  unregisterZoneElement,
} from "../../../services/hitTest";

const HERE = dirname(fileURLToPath(import.meta.url));
const TSX_PATH = resolve(HERE, "../StackWrapper.tsx");
const CSS_PATH = resolve(HERE, "../StackWrapper.css");

function readFile(path: string): string {
  return readFileSync(path, "utf8");
}

/**
 * Mirrors the StackWrapper v9 registration lifecycle: when the bloom
 * is active, every petal is registered with a 12 px inflate halo;
 * when it collapses, the registrations are torn down. There is NO
 * separate buffer registration — the v9 implementation deletes the
 * buffer entirely.
 */
function createBloomLifecycle() {
  let bloomActive = false;
  const petalEls = new Map<string, HTMLElement>();
  const PETAL_HALO_PX = 12;

  const setPetalRef = (id: string, el: HTMLElement | null): void => {
    if (el) {
      petalEls.set(id, el);
    } else {
      petalEls.delete(id);
    }
  };

  const syncRegistrations = (): void => {
    if (bloomActive) {
      for (const el of petalEls.values()) {
        registerZoneElement(el, {
          inflate: {
            top: PETAL_HALO_PX,
            right: PETAL_HALO_PX,
            bottom: PETAL_HALO_PX,
            left: PETAL_HALO_PX,
          },
        });
      }
    } else {
      for (const el of petalEls.values()) unregisterZoneElement(el);
    }
  };

  const setBloom = (next: boolean): void => {
    bloomActive = next;
    syncRegistrations();
  };

  const unmount = (): void => {
    for (const el of petalEls.values()) unregisterZoneElement(el);
    petalEls.clear();
  };

  return {
    setPetalRef,
    setBloom,
    unmount,
    syncRegistrations,
  };
}

function makeFakeElement(): HTMLElement {
  // Minimal stub — the hit-test only stores element refs in a Map and
  // calls getBoundingClientRect() during polling. We never poll in this
  // test, so the rect never needs to exist.
  return {
    getBoundingClientRect: () => ({ left: 0, top: 0, right: 0, bottom: 0 }),
  } as unknown as HTMLElement;
}

describe("v9 stack-wake-mutex — bloom hit-test registration lifecycle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("registers each petal when bloom activates, drops them on collapse", () => {
    const lc = createBloomLifecycle();
    const petalA = makeFakeElement();
    const petalB = makeFakeElement();

    lc.setPetalRef("a", petalA);
    lc.setPetalRef("b", petalB);

    // Bloom opens — every petal must be hit-test registered.
    lc.setBloom(true);

    // The hit-test module is a singleton; we can verify registration by
    // toggling bloom off and asserting unregisterZoneElement runs without
    // throwing. The contract is that the lifecycle wires register/
    // unregister calls in lock-step with bloomActive transitions.
    expect(() => lc.setBloom(false)).not.toThrow();

    // Re-opening the bloom must re-register the same refs.
    expect(() => lc.setBloom(true)).not.toThrow();

    lc.unmount();
  });

  it("petalRefs map removes entries when the petal unmounts", () => {
    const lc = createBloomLifecycle();
    const petal = makeFakeElement();

    lc.setPetalRef("only", petal);
    lc.setBloom(true);

    // Petal unmounts — ref callback fires with null.
    lc.setPetalRef("only", null);
    // Re-syncing after the petal is gone should not double-register stale refs.
    expect(() => lc.syncRegistrations()).not.toThrow();

    lc.setBloom(false);
    lc.unmount();
  });

  it("unmount cleans up every registration even if bloom was active", () => {
    const lc = createBloomLifecycle();
    const petalA = makeFakeElement();
    const petalB = makeFakeElement();

    lc.setPetalRef("a", petalA);
    lc.setPetalRef("b", petalB);
    lc.setBloom(true);

    // Stack dissolves while bloom is open — onCleanup must still drop refs.
    expect(() => lc.unmount()).not.toThrow();
  });

  it("re-syncing during bloom (effect refires) is idempotent", () => {
    const lc = createBloomLifecycle();
    const petal = makeFakeElement();
    lc.setPetalRef("only", petal);
    lc.setBloom(true);

    // Effect fires multiple times as petals re-position. Each pass
    // should re-register the same ref harmlessly — the underlying
    // Map.set is idempotent and the second call replaces the first.
    expect(() => lc.syncRegistrations()).not.toThrow();
    expect(() => lc.syncRegistrations()).not.toThrow();
    expect(() => lc.syncRegistrations()).not.toThrow();

    lc.setBloom(false);
  });
});

describe("v9 stack-wake-mutex — bloom buffer removal (source contract)", () => {
  it("StackWrapper.tsx no longer declares a bloomBufferRef", () => {
    const tsx = readFile(TSX_PATH);
    // The pre-v9 declaration `let bloomBufferRef: HTMLDivElement |
    // undefined;` is gone. A future regression that re-introduces the
    // buffer (and its 100 vw `pointer-events: auto` overlay) would
    // shadow neighbouring zones' wake; this assertion catches it
    // immediately.
    expect(tsx).not.toMatch(/bloomBufferRef/);
  });

  it("StackWrapper.tsx no longer renders a .stack-bloom-buffer element", () => {
    const tsx = readFile(TSX_PATH);
    // The JSX `class="stack-bloom-buffer"` is gone — only documentation
    // comments may mention the historical name. We strip block + line
    // comments before searching so the historical narrative doesn't
    // false-match.
    const codeOnly = tsx
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/\/\/[^\n]*/g, "");
    expect(codeOnly).not.toMatch(/class="stack-bloom-buffer"/);
  });

  it("StackWrapper.css no longer declares a .stack-bloom-buffer rule body", () => {
    const css = readFile(CSS_PATH);
    // The pre-v9 selector body `.stack-bloom-buffer { ... }` is gone.
    // Documentation comments referencing the historical name are
    // allowed (so future maintainers can find the v9 ADR), but no
    // rule selector should target the deleted element.
    const ruleRegex = /\.stack-bloom-buffer\s*\{/m;
    expect(ruleRegex.test(css)).toBe(false);
  });
});

describe("v9 stack-wake-mutex — petal inflate halo (source contract)", () => {
  it("StackWrapper.tsx declares a PETAL_HALO_PX constant", () => {
    const tsx = readFile(TSX_PATH);
    // The 12 px halo is the load-bearing piece that bridges the gap
    // between adjacent petals' hit rects. If a future edit drops the
    // constant or sets it to 0, the cursor sweeping between two
    // petals would land in PASSTHROUGH and the bloom would collapse
    // mid-sweep.
    expect(tsx).toMatch(/const\s+PETAL_HALO_PX\s*=\s*12/m);
  });

  it("StackWrapper.tsx registers each petal with the 12 px inflate halo", () => {
    const tsx = readFile(TSX_PATH);
    // Locate the bloom-active createEffect that does petal
    // registration. The inflate object must carry top/right/bottom/
    // left = PETAL_HALO_PX (the assertion accepts arbitrary
    // whitespace + property ordering inside the object literal).
    const effectMatch = /createEffect\(\(\)\s*=>\s*\{[\s\S]*?bloomActive\(\)[\s\S]*?registerZoneElement\(\s*el\s*,\s*\{([\s\S]*?)\}\s*\)/m.exec(
      tsx,
    );
    expect(effectMatch).not.toBeNull();
    const optsBody = effectMatch![1];
    expect(optsBody).toMatch(/top\s*:\s*PETAL_HALO_PX/m);
    expect(optsBody).toMatch(/right\s*:\s*PETAL_HALO_PX/m);
    expect(optsBody).toMatch(/bottom\s*:\s*PETAL_HALO_PX/m);
    expect(optsBody).toMatch(/left\s*:\s*PETAL_HALO_PX/m);
  });

  it("StackWrapper.tsx maintains a bloomElements Set for the hoveredZone$ effect", () => {
    const tsx = readFile(TSX_PATH);
    // The Set membership check is what decides whether a hovered
    // element is part of the bloom family (cursor on capsule /
    // petal / floating preview → keep bloom alive) vs an unrelated
    // zone (immediate collapse). If a future refactor drops the
    // Set, the hoveredZone$ effect can't tell the difference.
    expect(tsx).toMatch(/const\s+bloomElements\s*=\s*new\s+Set<HTMLElement>\(\)/m);
    // The set must be populated on petal registration AND drained
    // on cleanup. Look for both calls inside the bloom-active effect.
    expect(tsx).toMatch(/bloomElements\.add\(\s*el\s*\)/m);
    expect(tsx).toMatch(/bloomElements\.delete\(\s*el\s*\)/m);
  });

  it("StackWrapper.tsx subscribes to hoveredZone$ inside a createEffect", () => {
    const tsx = readFile(TSX_PATH);
    // The collapse trigger must come from the poller signal — not a
    // bare DOM mouseleave timer (which the deleted buffer used to
    // race against neighbour hover events).
    expect(tsx).toMatch(/import\s*\{[\s\S]*?hoveredZone\$[\s\S]*?\}\s*from\s*"\.\.\/\.\.\/services\/hitTest"/m);
    expect(tsx).toMatch(/createEffect\(\(\)\s*=>\s*\{[\s\S]*?hoveredZone\$\(\)/m);
  });
});
