/**
 * Smoke test for animations.css contract.
 *
 * Goal: keep the #8 spring-expand keyframe definition honest. The user-reported
 * "胶囊伸缩动画不丝滑" symptom in v1.2.2 was a missing overshoot keyframe — the
 * fix relies on the 60% step landing at scale 1.02 and a real cubic-bezier
 * spring curve. We assert the file shape so a careless edit that drops the
 * overshoot or swaps in a linear easing is caught at unit-test time, before
 * QA has to record a video to detect the regression.
 */
import { describe, it, expect } from "vitest";
// Read animations.css via the Node fs API. `node:fs` is available under
// vitest's Node runner; we cast to satisfy the project's strict tsconfig
// which doesn't include @types/node.
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-expect-error — node:fs is provided by the vitest Node runner; the
// project intentionally does not depend on @types/node in production.
import { readFileSync } from "node:fs";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-expect-error — see note above.
import { fileURLToPath } from "node:url";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-expect-error — see note above.
import { dirname, resolve } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const cssText = readFileSync(
  resolve(__dirname, "../animations.css"),
  "utf8",
);

describe("animations.css — #8 spring-expand keyframes", () => {
  it("declares @keyframes spring-expand with a 60% overshoot step", () => {
    // The keyframe body itself contains nested {} steps, so we extract by
    // scanning forward from `@keyframes spring-expand {` to the matching
    // outer `}`. A simple non-greedy /([\s\S]*?)\}/ would terminate at the
    // first inner `}`.
    const start = cssText.indexOf("@keyframes spring-expand");
    expect(start).toBeGreaterThanOrEqual(0);
    const openIdx = cssText.indexOf("{", start);
    expect(openIdx).toBeGreaterThan(start);
    let depth = 1;
    let i = openIdx + 1;
    for (; i < cssText.length && depth > 0; i++) {
      const ch = cssText[i];
      if (ch === "{") depth++;
      else if (ch === "}") depth--;
    }
    const body = cssText.slice(openIdx + 1, i - 1);

    expect(body).toMatch(/0%\s*\{[^}]*transform:\s*scale\(0\.96\)/);
    // The overshoot is the spring signature — must be > 1.
    expect(body).toMatch(/60%\s*\{[^}]*transform:\s*scale\(1\.02\)/);
    expect(body).toMatch(/100%\s*\{[^}]*transform:\s*scale\(1\)/);
  });

  it(".spring-emerge consumer applies the keyframe with a true spring cubic-bezier", () => {
    // The exact magic numbers (0.34, 1.56, 0.64, 1) describe a curve that
    // overshoots on its way to 1 — same curve PIN'd by the v1.2.2 fix spec.
    const block = cssText.match(/\.spring-emerge\s*\{([^}]+)\}/);
    expect(block).not.toBeNull();
    const body = block![1];

    expect(body).toMatch(/animation:\s*spring-expand\b/);
    expect(body).toMatch(/cubic-bezier\(0\.34,\s*1\.56,\s*0\.64,\s*1\)/);
    // forwards keeps the final scale(1) sticking, so the element doesn't
    // snap back to scale(0.96) when the animation fill-mode lapses.
    expect(body).toMatch(/forwards/);
  });

  it("preserves the prefers-reduced-motion override for spring-emerge", () => {
    // Accessibility contract: motion-sensitive users must get a near-instant
    // animation. Dropping this block was the biggest risk of the #8 rewrite.
    const reduced = cssText.match(
      /@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{([\s\S]*?)\}\s*\}/,
    );
    expect(reduced).not.toBeNull();
    expect(reduced![1]).toMatch(/\.spring-emerge\s*\{[^}]*animation-duration:\s*0\.01ms/);
  });

  it("preserves the runtime-effects degradation hooks", () => {
    expect(cssText).toMatch(
      /\[data-runtime-effects="reduced"\][^{]*\.spring-emerge[\s\S]*?animation-duration:\s*0\.12s/,
    );
    expect(cssText).toMatch(
      /\[data-runtime-effects="minimal"\][^{]*\.spring-emerge[\s\S]*?animation:\s*none/,
    );
  });
});
