/**
 * Runtime-effects degradation — CSS consumption layer.
 *
 * `services/runtimeHealth.ts` writes `data-runtime-effects` onto <html>; the
 * tail of `styles/animations.css` consumes that attribute to compress or
 * disable transitions. Audit (2026-04-25) flagged that the CSS side previously
 * had **zero** subscriptions, so the JS signal was a dead end.
 *
 * Verification strategy under jsdom (which does NOT cascade stylesheet rules
 * into `getComputedStyle`):
 *   1. Toggling `documentElement.dataset.runtimeEffects` is observable.
 *   2. The compound selector `[data-runtime-effects="minimal"] .foo` actually
 *      matches a descendant element when the dataset is set on <html>. This
 *      is what the production CSS rule needs to work — if descendant-side
 *      selector matching ever regressed, this test would catch it before the
 *      animation pipeline silently no-ops.
 */
import { describe, it, expect, afterEach } from "vitest";

afterEach(() => {
  delete document.documentElement.dataset.runtimeEffects;
  document
    .querySelectorAll("[data-test-probe-target]")
    .forEach((el) => el.remove());
});

describe("runtime-effects dataset signal", () => {
  it("writes the attribute on documentElement", () => {
    document.documentElement.dataset.runtimeEffects = "minimal";
    expect(document.documentElement.getAttribute("data-runtime-effects")).toBe(
      "minimal",
    );
  });

  it("matches the [data-runtime-effects=...] attribute selector", () => {
    document.documentElement.dataset.runtimeEffects = "reduced";
    const hit = document.querySelector('[data-runtime-effects="reduced"]');
    expect(hit).toBe(document.documentElement);
  });

  it("supports switching between modes", () => {
    document.documentElement.dataset.runtimeEffects = "full";
    expect(
      document.querySelector('[data-runtime-effects="minimal"]'),
    ).toBeNull();
    document.documentElement.dataset.runtimeEffects = "minimal";
    expect(
      document.querySelector('[data-runtime-effects="minimal"]'),
    ).toBe(document.documentElement);
  });
});

describe("compound selector reaches descendants (production rule shape)", () => {
  function makeProbeTarget(className: string): HTMLDivElement {
    const target = document.createElement("div");
    target.className = className;
    target.setAttribute("data-test-probe-target", "true");
    document.body.appendChild(target);
    return target;
  }

  it("matches '.spring-expand' descendant under data-runtime-effects=minimal", () => {
    document.documentElement.dataset.runtimeEffects = "minimal";
    const target = makeProbeTarget("spring-expand");

    const hit = document.querySelector(
      '[data-runtime-effects="minimal"] .spring-expand',
    );
    expect(hit).toBe(target);
  });

  it("matches '.content-reveal' descendant under data-runtime-effects=reduced", () => {
    document.documentElement.dataset.runtimeEffects = "reduced";
    const target = makeProbeTarget("content-reveal");

    const hit = document.querySelector(
      '[data-runtime-effects="reduced"] .content-reveal',
    );
    expect(hit).toBe(target);
  });

  it("matches '.item-lift' descendant under data-runtime-effects=minimal", () => {
    document.documentElement.dataset.runtimeEffects = "minimal";
    const target = makeProbeTarget("item-lift");

    const hit = document.querySelector(
      '[data-runtime-effects="minimal"] .item-lift',
    );
    expect(hit).toBe(target);
  });

  it("does NOT match the selector when dataset is full", () => {
    document.documentElement.dataset.runtimeEffects = "full";
    makeProbeTarget("spring-expand");

    const hit = document.querySelector(
      '[data-runtime-effects="minimal"] .spring-expand',
    );
    expect(hit).toBeNull();
  });

  it("matches '.scale-in' / '.pulse' / '.item-enter' (animation classes) under minimal", () => {
    document.documentElement.dataset.runtimeEffects = "minimal";
    const a = makeProbeTarget("scale-in");
    const b = makeProbeTarget("pulse");
    const c = makeProbeTarget("item-enter");

    expect(
      document.querySelector('[data-runtime-effects="minimal"] .scale-in'),
    ).toBe(a);
    expect(
      document.querySelector('[data-runtime-effects="minimal"] .pulse'),
    ).toBe(b);
    expect(
      document.querySelector('[data-runtime-effects="minimal"] .item-enter'),
    ).toBe(c);
  });
});
