/**
 * v8 round-6 — FocusedZonePreview anchor-rect contract.
 *
 * Round-4 made bloom petals fly polar-coordinate around the cursor, so
 * petals can land 300-500 px from the StackWrapper's natural rect.
 * The legacy preview was `position: absolute; left: calc(100% + 16px)`
 * — i.e. anchored to the wrapper, not the petal — so users saw petals
 * fan out, hovered them, and the preview opened far off-screen
 * (against the original capsule). They reported "petals appear,
 * nothing happens visibly".
 *
 * Round-6 lets the parent (StackWrapper) pass an `anchorRect` prop
 * holding the hovered petal's getBoundingClientRect output. When set,
 * the preview switches to `position: fixed` and computes its left/top
 * from the rect plus the `horizontal`/`vertical` direction props,
 * clamping to the viewport so it never clips off-screen.
 *
 * These tests render FocusedZonePreview directly and inspect the
 * resulting inline style + DOM class list to verify:
 *   1. anchorRect drives `position: fixed` (legacy behavior preserved
 *      when anchorRect is omitted)
 *   2. horizontal: "right" places preview to the right of the petal
 *   3. horizontal: "right" near right viewport edge auto-flips to LEFT
 *      so the preview stays on-screen
 *   4. vertical: "top" near bottom viewport edge auto-flips so the
 *      preview's bottom doesn't clip
 *   5. hit-test registration fires when anchorRect is set, NOT when
 *      it's omitted (legacy tray path is wrapper-registered already)
 *   6. unmount unregisters even if the bloom collapses while the
 *      preview is open
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import type { BentoZone } from "../../../types/zone";

// Stub @tauri-apps/api/window because services/hitTest imports it at
// module-load time. Without this the import resolves to a Vite
// optimize-deps stub that throws.
const mockSetIgnoreCursorEvents = vi.fn().mockResolvedValue(undefined);
const mockOuterPosition = vi.fn().mockResolvedValue({ x: 0, y: 0 });
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    setIgnoreCursorEvents: mockSetIgnoreCursorEvents,
    outerPosition: mockOuterPosition,
  }),
  cursorPosition: vi.fn().mockResolvedValue({ x: 0, y: 0 }),
}));

import FocusedZonePreview from "../FocusedZonePreview";
import * as hitTest from "../../../services/hitTest";

const VIEWPORT = { width: 1920, height: 1080 };

function makeZone(overrides: Partial<BentoZone> = {}): BentoZone {
  return {
    id: "z1",
    name: "Test Zone",
    icon: "📦",
    position: { x_percent: 50, y_percent: 50 },
    expanded_size: { w_percent: 20, h_percent: 38 }, // 384x410 at 1920x1080
    items: [],
    accent_color: null,
    sort_order: 0,
    auto_group: null,
    grid_columns: 4,
    created_at: "2026-04-28T00:00:00Z",
    updated_at: "2026-04-28T00:00:00Z",
    capsule_size: "medium",
    capsule_shape: "pill",
    ...overrides,
  } as BentoZone;
}

beforeEach(() => {
  // jsdom doesn't always size window correctly — pin innerWidth/Height
  // to the same 1920x1080 we use in CSS-pixel math.
  Object.defineProperty(window, "innerWidth", {
    configurable: true,
    value: VIEWPORT.width,
  });
  Object.defineProperty(window, "innerHeight", {
    configurable: true,
    value: VIEWPORT.height,
  });
  // jsdom doesn't implement ResizeObserver — stub a no-op so onMount
  // paths in BentoPanel descendants (textAbbr) don't throw. We don't
  // need real RO callbacks for the anchor decision under test.
  if (!(globalThis as Record<string, unknown>).ResizeObserver) {
    (globalThis as Record<string, unknown>).ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
  }
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("FocusedZonePreview — round-6 anchor-rect", () => {
  it("legacy path: no anchorRect → position: absolute (no inline left)", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const zone = makeZone();

    const dispose = render(
      () => (
        <FocusedZonePreview
          zone={zone}
          horizontal="right"
          vertical="top"
          onClose={() => {}}
        />
      ),
      host,
    );

    const el = host.querySelector(".stack-focused-preview") as HTMLElement;
    expect(el).toBeTruthy();
    // No `--floating` modifier: legacy absolute layout drives via stylesheet.
    expect(el.classList.contains("stack-focused-preview--floating")).toBe(false);
    // Width/height set inline; left/top are NOT (those come from CSS).
    expect(el.style.width).toBe("384px");
    // Browser normalises 410.40000000000003 → 410.4 in the inline style
    // string; assert as a parsed number to be float-precision tolerant.
    expect(parseFloat(el.style.height)).toBeCloseTo(410.4, 1);
    // left/top remain unset inline because legacy mode lets stylesheet take over.
    expect(el.style.left).toBe("");
    expect(el.style.top).toBe("");

    dispose();
    host.remove();
  });

  it("anchorRect set + horizontal=right + petal in left half → preview to the right", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const zone = makeZone();
    // Petal sitting at x=400 (left third), y=300 → 96x96 box.
    const petalRect = {
      left: 400,
      top: 300,
      right: 496,
      bottom: 396,
      width: 96,
      height: 96,
    };

    const dispose = render(
      () => (
        <FocusedZonePreview
          zone={zone}
          horizontal="right"
          vertical="top"
          anchorRect={petalRect}
          onClose={() => {}}
        />
      ),
      host,
    );

    const el = host.querySelector(".stack-focused-preview") as HTMLElement;
    expect(el).toBeTruthy();
    // Floating modifier → CSS overrides + z-index 60.
    expect(el.classList.contains("stack-focused-preview--floating")).toBe(true);
    expect(el.style.position).toBe("fixed");
    // left = petalRect.right + 12 (PREVIEW_GAP_PX) = 508
    expect(el.style.left).toBe("508px");
    // top = petalRect.top = 300
    expect(el.style.top).toBe("300px");
    expect(el.style.zIndex).toBe("60");

    dispose();
    host.remove();
  });

  it("anchorRect set + horizontal=right but petal too close to right edge → flips LEFT to stay on-screen", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    // v8 round-10 (Issue C): floating preview caps at 360 px wide
    // even when zone.expanded_size resolves to ≥360 px (e.g. 384 here).
    // The flip math therefore uses 360, not 384.
    const zone = makeZone();
    const FLOATING_W = 360;
    // Petal at viewport right edge: there's no room for 360px + 12px gap to the right,
    // so the algorithm must flip the preview to the LEFT side of the petal.
    const petalRect = {
      left: 1700,
      top: 300,
      right: 1796,
      bottom: 396,
      width: 96,
      height: 96,
    };

    const dispose = render(
      () => (
        <FocusedZonePreview
          zone={zone}
          horizontal="right"
          vertical="top"
          anchorRect={petalRect}
          onClose={() => {}}
        />
      ),
      host,
    );

    const el = host.querySelector(".stack-focused-preview") as HTMLElement;
    // After the flip + clamp the preview's right edge must not overlap
    // the petal's left edge. The exact left depends on viewport-margin
    // clamping; assert the invariant: preview ends at or before the
    // petal starts.
    const left = parseFloat(el.style.left);
    expect(left).toBeLessThanOrEqual(VIEWPORT.width - FLOATING_W - 16);
    expect(left).toBeGreaterThanOrEqual(16);
    // Preview must sit on the LEFT side of the petal post-flip
    // (preview right edge ≤ petal left edge, +1 px slack for rounding).
    expect(left + FLOATING_W).toBeLessThanOrEqual(petalRect.left + 1);

    dispose();
    host.remove();
  });

  it("anchorRect set + vertical=top + petal near bottom → preview clamps so its bottom stays on-screen", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const zone = makeZone();
    // Petal at bottom of screen — top=1000, bottom=1080. With vertical=top
    // the legacy logic would set top=1000, but 1000 + 410 = 1410 > 1080 →
    // we must reposition so the preview fits.
    const petalRect = {
      left: 400,
      top: 1000,
      right: 496,
      bottom: 1080,
      width: 96,
      height: 80,
    };

    const dispose = render(
      () => (
        <FocusedZonePreview
          zone={zone}
          horizontal="right"
          vertical="top"
          anchorRect={petalRect}
          onClose={() => {}}
        />
      ),
      host,
    );

    const el = host.querySelector(".stack-focused-preview") as HTMLElement;
    const top = parseFloat(el.style.top);
    const height = parseFloat(el.style.height);
    // Preview's bottom must NOT clip — top + height ≤ vh - margin.
    expect(top + height).toBeLessThanOrEqual(VIEWPORT.height - 16 + 1);
    // top must stay within the viewport.
    expect(top).toBeGreaterThanOrEqual(16);

    dispose();
    host.remove();
  });

  it("anchorRect → registerZoneElement called on mount, unregister on cleanup", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const zone = makeZone();
    const petalRect = {
      left: 400,
      top: 300,
      right: 496,
      bottom: 396,
      width: 96,
      height: 96,
    };

    const registerSpy = vi.spyOn(hitTest, "registerZoneElement");
    const unregisterSpy = vi.spyOn(hitTest, "unregisterZoneElement");

    const dispose = render(
      () => (
        <FocusedZonePreview
          zone={zone}
          horizontal="right"
          vertical="top"
          anchorRect={petalRect}
          onClose={() => {}}
        />
      ),
      host,
    );
    expect(registerSpy).toHaveBeenCalledTimes(1);
    expect(registerSpy.mock.calls[0][0]).toBeInstanceOf(HTMLElement);

    dispose();
    host.remove();
    expect(unregisterSpy).toHaveBeenCalled();
  });

  it("legacy path (no anchorRect) → does NOT register with hit-test (wrapper covers it)", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const zone = makeZone();

    const registerSpy = vi.spyOn(hitTest, "registerZoneElement");

    const dispose = render(
      () => (
        <FocusedZonePreview
          zone={zone}
          horizontal="right"
          vertical="top"
          onClose={() => {}}
        />
      ),
      host,
    );
    // No anchorRect → wrapper's own registration covers the preview's
    // hit area (it lives inside the wrapper's rect when absolute), so
    // we must NOT double-register.
    expect(registerSpy).not.toHaveBeenCalled();

    dispose();
    host.remove();
  });

  it("anchorRect with default expanded_size 0/0 → falls back to 360x420", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    // Zone with unset expanded_size — both percents 0 → defaults apply.
    const zone = makeZone({
      expanded_size: { w_percent: 0, h_percent: 0 },
    });
    const petalRect = {
      left: 400,
      top: 300,
      right: 496,
      bottom: 396,
      width: 96,
      height: 96,
    };

    const dispose = render(
      () => (
        <FocusedZonePreview
          zone={zone}
          horizontal="right"
          vertical="top"
          anchorRect={petalRect}
          onClose={() => {}}
        />
      ),
      host,
    );
    const el = host.querySelector(".stack-focused-preview") as HTMLElement;
    expect(el.style.width).toBe("360px");
    expect(el.style.height).toBe("420px");
    // Right of petal: left = 496 + 12 = 508
    expect(el.style.left).toBe("508px");

    dispose();
    host.remove();
  });
});
