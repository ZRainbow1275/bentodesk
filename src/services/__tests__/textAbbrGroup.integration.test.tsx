/**
 * Integration test for `useTextAbbrGroup` mounted under a real
 * `FontGroupContext.Provider` — covers the path that pure-unit tests for
 * `createFontGroup` cannot: that the composable
 *   1. picks up the surrounding context value (not null), and
 *   2. emits the group's `groupFontSize` from its `fontSize()` accessor
 *      (the value rendered into the inline `style` of every name span).
 *
 * Why this test was added: round 1 unit tests verified the pure `FontGroup`
 * primitive, but a regression where members render at *different* sizes
 * inside a single Provider could only be caught by mounting multiple hooks
 * under one provider and asserting their `fontSize()` values are equal.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render } from "solid-js/web";
import { Component, createSignal, createEffect, type Accessor } from "solid-js";
import {
  FontGroupContext,
  createFontGroup,
  useTextAbbrGroup,
} from "../textAbbrGroup";

beforeEach(() => {
  // Match the canvas measurer stub from textAbbr.test.ts so name-width
  // calculations are deterministic: width = chars * font-size px.
  const fakeCtx = {
    font: "13px sans-serif",
    measureText(this: { font: string }, s: string) {
      const match = /^(\d+(?:\.\d+)?)px/.exec(this.font.trim());
      const size = match ? parseFloat(match[1]) : 13;
      return { width: s.length * size };
    },
  };
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
    fakeCtx as unknown as CanvasRenderingContext2D,
  );
  // jsdom doesn't implement ResizeObserver — stub a no-op so onMount
  // doesn't throw. We don't need real RO callbacks here because we drive
  // measurements via clientWidth + the bootstrap rAF loop.
  if (!(globalThis as Record<string, unknown>).ResizeObserver) {
    (globalThis as Record<string, unknown>).ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
  }
});

afterEach(() => {
  vi.restoreAllMocks();
});

interface ProbeProps {
  name: string;
  width: number;
  onSize: (size: Accessor<number>) => void;
}

/**
 * One title-rendering harness — mimics ItemCard's name span. Reports its
 * reactive `fontSize()` accessor up so the test can assert equality across
 * siblings. We drive `clientWidth` by stubbing the property on the mounted
 * element so the hook's bootstrap rAF immediately picks it up.
 */
const Probe: Component<ProbeProps> = (props) => {
  const abbr = useTextAbbrGroup(() => props.name);
  return (
    <span
      data-testid={`probe-${props.name}`}
      ref={(el) => {
        if (!el) return;
        // Force a non-zero clientWidth and a known font shorthand so the
        // hook's `readFontContext` resolves to {fontFamilyShorthand,
        // defaultFontSizePx: 13} — same as the unit-test stub.
        Object.defineProperty(el, "clientWidth", {
          configurable: true,
          get: () => props.width,
        });
        abbr.setRef(el);
        props.onSize(abbr.fontSize);
      }}
      style={{ "font-size": `${abbr.fontSize()}px` }}
    >
      {abbr.text()}
    </span>
  );
};

describe("useTextAbbrGroup — context propagation", () => {
  it("returns the group's groupFontSize when a Provider is mounted", async () => {
    const host = document.createElement("div");
    document.body.appendChild(host);

    const sizes: Accessor<number>[] = [];

    const fontGroup = createFontGroup(11);
    const dispose = render(() => {
      return (
        <FontGroupContext.Provider value={fontGroup}>
          <Probe name="A" width={1000} onSize={(s) => sizes.push(s)} />
          <Probe name="A very long name that must shrink" width={50} onSize={(s) => sizes.push(s)} />
          <Probe name="medium" width={200} onSize={(s) => sizes.push(s)} />
        </FontGroupContext.Provider>
      );
    }, host);

    // Yield two animation frames so the hook's bootstrap rAF can sample
    // clientWidth and propagate through the createMemo chain.
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r(undefined))));

    // All three probes should report the SAME font-size — the group minimum.
    expect(sizes.length).toBe(3);
    const reported = sizes.map((s) => s());
    expect(reported[0]).toBe(reported[1]);
    expect(reported[1]).toBe(reported[2]);

    // And the same size should be reflected in the DOM inline style.
    const els = host.querySelectorAll("span[data-testid^=probe-]");
    const inlineSizes = Array.from(els).map((el) => (el as HTMLElement).style.fontSize);
    expect(new Set(inlineSizes).size).toBe(1);

    // Defensive: the chosen size must equal the smallest needed size, not
    // simply the default — so the long name actually pulls the column down.
    expect(reported[0]).toBeLessThan(13);

    dispose();
    host.remove();
  });

  it("falls back to per-element sizing when no Provider is mounted", async () => {
    const host = document.createElement("div");
    document.body.appendChild(host);

    const sizes: Accessor<number>[] = [];

    const dispose = render(() => {
      return (
        <>
          <Probe name="short" width={1000} onSize={(s) => sizes.push(s)} />
          <Probe name="VeryLongLongLongName" width={50} onSize={(s) => sizes.push(s)} />
        </>
      );
    }, host);

    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r(undefined))));

    // Without a Provider each name uses its own fitted size, so they differ.
    expect(sizes[0]()).not.toBe(sizes[1]());

    dispose();
    host.remove();
  });

  it("the createEffect chain ties group size into a Solid effect", async () => {
    // Replicates the production path: ItemCard reads `abbr.fontSize()` inside
    // the JSX `style={{ "font-size": ... }}` binding, which is itself a
    // createEffect under the hood. Verify that signal flows propagate.
    const host = document.createElement("div");
    document.body.appendChild(host);

    const captured: number[] = [];
    const fontGroup = createFontGroup(11);
    const [showThird, setShowThird] = createSignal(false);

    const ProbeWithEffect: Component<{ name: string; width: number }> = (p) => {
      const abbr = useTextAbbrGroup(() => p.name);
      createEffect(() => {
        captured.push(abbr.fontSize());
      });
      return (
        <span
          ref={(el) => {
            if (!el) return;
            Object.defineProperty(el, "clientWidth", {
              configurable: true,
              get: () => p.width,
            });
            abbr.setRef(el);
          }}
        >
          {abbr.text()}
        </span>
      );
    };

    const dispose = render(() => {
      return (
        <FontGroupContext.Provider value={fontGroup}>
          <ProbeWithEffect name="A" width={1000} />
          <ProbeWithEffect name="B-medium" width={500} />
          {showThird() && <ProbeWithEffect name="C-extremely-long-pathological" width={40} />}
        </FontGroupContext.Provider>
      );
    }, host);

    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r(undefined))));
    const beforeAdd = captured.slice();
    expect(beforeAdd.length).toBeGreaterThan(0);

    // Adding a third member with a much smaller needed size should
    // ripple through both existing probes (their effects re-fire).
    setShowThird(true);
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r(undefined))));

    // After the small probe registered, the group's emitted size dropped,
    // so additional captures should land at a smaller value than before.
    const afterAdd = captured.slice(beforeAdd.length);
    expect(afterAdd.length).toBeGreaterThan(0);
    const minAfter = Math.min(...afterAdd);
    const maxBefore = Math.max(...beforeAdd);
    expect(minAfter).toBeLessThanOrEqual(maxBefore);

    dispose();
    host.remove();
  });
});
