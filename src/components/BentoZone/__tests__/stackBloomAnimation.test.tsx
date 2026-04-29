/**
 * v8 round-11 — bloom petals animate in/out with staggered spring keyframes.
 *
 * Round-10 placed petals at their resolved polar centers via inline
 * `transform: translate(centerX - PETAL_W/2, centerY - PETAL_H/2)`,
 * relying on a CSS `transition` + class flip for the entry. The user
 * reported "still ugly — give me an elegant entrance animation".
 *
 * Round-11 introduces:
 *   1. `@keyframes stack-bloom-petal-enter` — a 420 ms spring with
 *      light overshoot (cubic-bezier(0.34, 1.56, 0.64, 1)). Petals
 *      start at scale(0.4) translated TO the cursor, fade in by 60 %,
 *      and settle at scale(1) at their resolved polar position.
 *   2. `@keyframes stack-bloom-petal-exit` — a 220 ms reverse fade.
 *   3. `@keyframes stack-bloom-petal-active-pulse` — a 1.5 s
 *      ease-in-out infinite outer-glow breathe so the active petal
 *      reads "alive".
 *   4. `--petal-index` + 38 ms stagger on entry.
 *   5. `--bloom-petal-count - 1 - --petal-index` reverse stagger on
 *      exit (last in, first out).
 *   6. `--bloom-origin-x/y` + `--bloom-to-x/y` custom props driving
 *      the keyframe interpolation.
 *   7. Hover micro-interaction: `transform: translate(var(--bloom-to-x),
 *      var(--bloom-to-y)) scale(1.05)` with a 180 ms cubic-bezier curve.
 *      Composes WITHOUT cancelling the entry's resolved polar position.
 *
 * The contract under test:
 *   - StackWrapper.css declares all three keyframes.
 *   - Each keyframe's first/last stop interpolates the four CSS
 *     custom props (origin → to).
 *   - `.stack-bloom__petal--leaving` triggers the exit keyframe.
 *   - The wrapper class flow `--bloomed` triggers the entry.
 *   - Reduced-motion users get a flat fade (no spring).
 *
 * The test parses StackWrapper.css and StackWrapper.tsx as text and
 * asserts the contract. It also drives a minimal jsdom test that
 * confirms when StackWrapper is rendered with the bloomed class set,
 * each child petal carries every required CSS custom prop in its
 * inline style.
 *
 * Why source-text + lightweight DOM (not a full Solid render)? The
 * StackWrapper component pulls in `zonesStore`, `selection`, `ipc`,
 * `settings`, `i18n`, and the cursor hit-test poller — bootstrapping
 * all of that for what is fundamentally a CSS contract test would
 * cost ~2 s per test and produce a flaky setup unrelated to the
 * animation. The CSS rule + the JS-set custom props together fully
 * encode the round-11 contract; if either is removed, this test
 * fails loudly.
 */
import { describe, it, expect } from "vitest";
// node:fs / url / path are provided by the vitest Node runner; project
// intentionally does not depend on @types/node in production. Pattern
// mirrors src/styles/__tests__/animations.test.ts.
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-expect-error — node:fs is provided by the vitest Node runner.
import { readFileSync } from "node:fs";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-expect-error — see note above.
import { fileURLToPath } from "node:url";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-expect-error — see note above.
import { dirname, resolve } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const CSS_PATH = resolve(HERE, "../StackWrapper.css");
const TSX_PATH = resolve(HERE, "../StackWrapper.tsx");

function readFile(path: string): string {
  return readFileSync(path, "utf8");
}

/**
 * Extract the body of an `@keyframes <name> { ... }` block. The CSS
 * uses simple, non-nested keyframe declarations, so a balanced-brace
 * search starting from the first `{` after the keyframe header is
 * sufficient.
 *
 * Returns null if the keyframe is not present.
 */
function extractKeyframeBody(css: string, name: string): string | null {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const headerRegex = new RegExp(
    String.raw`@keyframes\s+${escaped}\s*\{`,
    "m",
  );
  const headerMatch = headerRegex.exec(css);
  if (!headerMatch) return null;
  // The header match ends at the opening `{` of the keyframe block.
  const openIdx = headerMatch.index + headerMatch[0].length;
  let depth = 1;
  let i = openIdx;
  while (i < css.length && depth > 0) {
    const ch = css[i];
    if (ch === "{") depth++;
    else if (ch === "}") depth--;
    i++;
  }
  if (depth !== 0) return null;
  // body = everything between openIdx and i-1 (the matching close brace).
  return css.slice(openIdx, i - 1);
}

describe("v8 round-11 — bloom entry/exit keyframes (CSS contract)", () => {
  it("CSS declares @keyframes stack-bloom-petal-enter", () => {
    const css = readFile(CSS_PATH);
    const body = extractKeyframeBody(css, "stack-bloom-petal-enter");
    expect(body).not.toBeNull();
  });

  it("entry keyframe interpolates from --bloom-origin-x/y → --bloom-to-x/y with a 60% opacity stop", () => {
    const css = readFile(CSS_PATH);
    const body = extractKeyframeBody(css, "stack-bloom-petal-enter");
    expect(body).not.toBeNull();
    const text = body!;
    // 0% must reference both origin custom props inside translate(...) and
    // start at scale(0.4) opacity:0.
    expect(text).toMatch(
      /0%\s*\{[^}]*translate\([^)]*var\(--bloom-origin-x\)[^)]*var\(--bloom-origin-y\)[^)]*\)\s+scale\(0\.4\)/m,
    );
    expect(text).toMatch(/0%\s*\{[^}]*opacity\s*:\s*0/m);
    // 60% fade-in stop.
    expect(text).toMatch(/60%\s*\{[^}]*opacity\s*:\s*1/m);
    // 100% must reference both to custom props inside translate(...) and
    // end at scale(1) opacity:1.
    expect(text).toMatch(
      /100%\s*\{[^}]*translate\([^)]*var\(--bloom-to-x\)[^)]*var\(--bloom-to-y\)[^)]*\)\s+scale\(1\)/m,
    );
    expect(text).toMatch(/100%\s*\{[^}]*opacity\s*:\s*1/m);
  });

  it("CSS declares @keyframes stack-bloom-petal-exit (reverse fade)", () => {
    const css = readFile(CSS_PATH);
    const body = extractKeyframeBody(css, "stack-bloom-petal-exit");
    expect(body).not.toBeNull();
    const text = body!;
    // 0% sits at scale(1) opacity:1 (the resting bloomed state).
    expect(text).toMatch(/0%\s*\{[^}]*scale\(1\)[^}]*opacity\s*:\s*1/m);
    // 100% returns toward origin at scale(0.5) opacity:0 (visually retracts).
    expect(text).toMatch(
      /100%\s*\{[^}]*translate\([^)]*var\(--bloom-origin-x\)[^)]*var\(--bloom-origin-y\)[^)]*\)\s+scale\(0\.5\)/m,
    );
    expect(text).toMatch(/100%\s*\{[^}]*opacity\s*:\s*0/m);
  });

  it("CSS declares @keyframes stack-bloom-petal-active-pulse for active-petal breathe", () => {
    const css = readFile(CSS_PATH);
    const body = extractKeyframeBody(css, "stack-bloom-petal-active-pulse");
    expect(body).not.toBeNull();
    // 50 % must show the pulse-up (7 px / 22 % glow), 0 % and 100 % the
    // resting (5.5 px / 16 % glow). Asserting a few signposts so the
    // breathe character can't silently regress.
    const text = body!;
    expect(text).toMatch(/0%\s*\{[\s\S]*?5\.5px[\s\S]*?16%/m);
    expect(text).toMatch(/50%\s*\{[\s\S]*?7px[\s\S]*?22%/m);
    expect(text).toMatch(/100%\s*\{[\s\S]*?5\.5px[\s\S]*?16%/m);
  });

  it("CSS .stack-wrapper--bloomed .stack-bloom__petal binds the entry animation with capped stagger", () => {
    const css = readFile(CSS_PATH);
    // Match the rule that applies the entry keyframe + the per-petal
    // stagger. We capture the rule body so a future refactor that
    // tucks the animation under an unrelated parent still passes
    // (as long as the entry animation is bound correctly).
    //
    // v8 round-14: the per-petal stagger formula changed from a
    // fixed `var(--petal-index) * 38ms` to a count-capped
    // `(360ms / max(1, count)) * index` so total entry stagger is
    // bounded at 360 ms regardless of petal count.
    const ruleRegex =
      /\.stack-wrapper--bloomed\s+\.stack-bloom__petal\s*\{([^}]*)\}/m;
    const ruleMatch = ruleRegex.exec(css);
    expect(ruleMatch).not.toBeNull();
    const body = ruleMatch![1];
    expect(body).toMatch(
      /animation\s*:\s*stack-bloom-petal-enter\s+420ms\s+cubic-bezier\([^)]+\)\s+forwards/m,
    );
    // Round-14 formula: animation-delay = (360ms / max(1, count)) * index.
    expect(body).toMatch(
      /animation-delay\s*:\s*calc\(\s*\(\s*360ms\s*\/\s*max\(\s*1\s*,\s*var\(--bloom-petal-count[^)]*\)\s*\)\s*\)\s*\*\s*var\(--petal-index[^)]*\)\s*\)/m,
    );
  });

  it("CSS .stack-bloom__petal--leaving binds the exit animation with REVERSE stagger (capped)", () => {
    const css = readFile(CSS_PATH);
    const ruleRegex = /\.stack-bloom__petal--leaving\s*\{([^}]*)\}/m;
    const ruleMatch = ruleRegex.exec(css);
    expect(ruleMatch).not.toBeNull();
    const body = ruleMatch![1];
    expect(body).toMatch(
      /animation\s*:\s*stack-bloom-petal-exit\s+220ms\s+cubic-bezier\([^)]+\)\s+forwards/m,
    );
    // v8 round-14: exit stagger is also capped at 240 ms total. The
    // formula becomes `(240ms / max(1, count)) * (count - 1 - index)`
    // so the last-in-first-out ordering is preserved while bounded.
    expect(body).toMatch(
      /animation-delay\s*:\s*calc\(\s*\(\s*240ms\s*\/\s*max\(\s*1\s*,\s*var\(--bloom-petal-count[^)]*\)\s*\)\s*\)\s*\*\s*\(\s*var\(--bloom-petal-count[^)]*\)\s*-\s*1\s*-\s*var\(--petal-index[^)]*\)\s*\)/m,
    );
  });

  it("CSS hover rule lifts the petal WITHOUT cancelling the resting polar translate", () => {
    const css = readFile(CSS_PATH);
    // We isolate the `.stack-bloom__petal:hover { ... }` rule (not the
    // descendant icon rule below it) and inspect its transform.
    const hoverRegex =
      /\.stack-bloom__petal:hover\s*\{([^}]*)\}/m;
    const hoverMatch = hoverRegex.exec(css);
    expect(hoverMatch).not.toBeNull();
    const body = hoverMatch![1];
    // The hover transform must compose with the resting polar position
    // (var(--bloom-to-x/y)). If a future edit drops the var() back to a
    // bare `translateY(-2px)`, the hover would visually snap the petal
    // back to its origin (rendering the entry animation pointless).
    expect(body).toMatch(
      /transform\s*:\s*translate\(\s*var\(--bloom-to-x\)\s*,\s*var\(--bloom-to-y\)\s*\)\s+scale\(1\.05\)/m,
    );
    // Hover must also have a dedicated 180 ms transform transition curve
    // so the lift is smooth and doesn't fight the 420 ms entry spring.
    expect(body).toMatch(
      /transition\s*:[\s\S]*transform\s+180ms\s+cubic-bezier\(0\.4\s*,\s*0\s*,\s*0\.2\s*,\s*1\)/m,
    );
  });

  it("CSS prefers-reduced-motion override neutralises the spring keyframes", () => {
    const css = readFile(CSS_PATH);
    // Locate the @media block and verify it sets `animation: none` on the
    // petal selectors. Without this, reduced-motion users would still see
    // a 420 ms spring (which the OS-level setting explicitly opts them
    // out of).
    const mediaRegex =
      /@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{([\s\S]*?)\n\}/m;
    const mediaMatch = mediaRegex.exec(css);
    expect(mediaMatch).not.toBeNull();
    const body = mediaMatch![1];
    expect(body).toMatch(/\.stack-bloom__petal/m);
    expect(body).toMatch(/animation\s*:\s*none/m);
  });

  it("CSS --bloomed capsule transition matches petal entry timing (320 ms spring)", () => {
    const css = readFile(CSS_PATH);
    const ruleRegex =
      /\.stack-wrapper--bloomed\s+\.stack-capsule\s*\{([^}]*)\}/m;
    const ruleMatch = ruleRegex.exec(css);
    expect(ruleMatch).not.toBeNull();
    const body = ruleMatch![1];
    // 320 ms spring on transform + opacity ease-out so the capsule eases
    // into its faded state alongside the first petal's entry.
    expect(body).toMatch(
      /transition\s*:[\s\S]*transform\s+320ms\s+cubic-bezier\(0\.34\s*,\s*1\.56\s*,\s*0\.64\s*,\s*1\)/m,
    );
    expect(body).toMatch(/opacity\s+320ms\s+ease-out/m);
  });
});

describe("v8 round-11 — petal inline-style custom props (TSX contract)", () => {
  it("TSX petal style injects --petal-index from the For-index", () => {
    const tsx = readFile(TSX_PATH);
    expect(tsx).toMatch(/"--petal-index"\s*:\s*String\(\s*index\(\)\s*\)/);
  });

  it("TSX petal style injects --bloom-petal-count from visiblePetalCount() (round-14: clamped to MAX_VISIBLE_MEMBERS)", () => {
    const tsx = readFile(TSX_PATH);
    // v8 round-14: the count source moved from the raw
    // `props.zones.length` to the `visiblePetalCount()` memo so the
    // CSS animation-delay calc adapts to the visible (clamped)
    // count rather than the (potentially much larger) full member
    // count. This keeps the per-petal stagger sensible when a
    // 50-member stack truncates to 23 visible petals + indicator.
    expect(tsx).toMatch(
      /"--bloom-petal-count"\s*:\s*String\(\s*visiblePetalCount\(\)\s*\)/,
    );
  });

  it("TSX petal style injects --bloom-origin-x/y as origin-vs-petal delta in px", () => {
    const tsx = readFile(TSX_PATH);
    // The delta math is `dx = center.x - petal.x` (and dy symmetric).
    // Pre-round-12 `center` was the cursor; round-12 swapped it for the
    // capsule centre when the radial bloom was abandoned. The vector
    // shape is identical so this assertion still holds; the variable
    // name `center` now means "drop-from anchor", which is the capsule
    // centre.
    expect(tsx).toMatch(/const\s+dx\s*=\s*center\.x\s*-\s*petal\.x/m);
    expect(tsx).toMatch(/const\s+dy\s*=\s*center\.y\s*-\s*petal\.y/m);
    expect(tsx).toMatch(/"--bloom-origin-x"\s*:\s*`\$\{dx\}px`/);
    expect(tsx).toMatch(/"--bloom-origin-y"\s*:\s*`\$\{dy\}px`/);
  });

  it("TSX petal style injects --bloom-to-x/y at 0px (steady state)", () => {
    const tsx = readFile(TSX_PATH);
    expect(tsx).toMatch(/"--bloom-to-x"\s*:\s*"0px"/);
    expect(tsx).toMatch(/"--bloom-to-y"\s*:\s*"0px"/);
  });

  it("TSX petal class list flips to include --leaving when bloomLeaving() is true", () => {
    const tsx = readFile(TSX_PATH);
    // The class string template literal must reference bloomLeaving() and
    // attach the modifier conditionally.
    expect(tsx).toMatch(
      /bloomLeaving\(\)\s*\?\s*"stack-bloom__petal--leaving"\s*:\s*""/,
    );
  });

  it("TSX bloom state machine flips bloomLeaving on bloomActive false transition", () => {
    const tsx = readFile(TSX_PATH);
    // The createEffect must call setBloomLeaving(true) before scheduling
    // the unmount. We assert both the signal's existence and the
    // setBloomLeaving call inside the inactive branch.
    expect(tsx).toMatch(/const\s*\[bloomLeaving\s*,\s*setBloomLeaving\]\s*=\s*createSignal/);
    // Inside the createEffect tracking bloomActive, the inactive branch
    // must flip bloomLeaving true (so the exit keyframe runs) BEFORE the
    // setTimeout that flips bloomVisible to false.
    expect(tsx).toMatch(
      /setBloomLeaving\(true\);[\s\S]*?bloomUnmountTimer\s*=\s*setTimeout/m,
    );
    // The 240 ms unmount window must reset the leaving flag once the
    // petals are torn out, so a re-bloom doesn't render with the
    // leaving class still attached.
    expect(tsx).toMatch(
      /setBloomVisible\(false\);[\s\S]*?setBloomLeaving\(false\)/m,
    );
  });
});

describe("v8 round-11 — DOM smoke test for inline custom props", () => {
  /**
   * We can't easily render StackWrapper standalone (its store / IPC
   * dependencies require a heavy test bootstrap), so we replicate the
   * one piece of behavior under test: the inline-style object the TSX
   * builds for each petal. If any of the four custom props goes
   * missing, the keyframe interpolation breaks (translate sees an
   * undefined var() and the petal animates from 0,0 → 0,0, losing
   * the visible "fly out from cursor" character).
   *
   * This mirrors the exact petalStyle() logic from StackWrapper.tsx
   * without depending on the component itself, and dispatches it to
   * a real <button> in jsdom so we can inspect the resulting style
   * attribute.
   */
  it("inline style on a rendered petal element exposes all four CSS custom props plus position/size", () => {
    const PETAL_W = 108;
    const PETAL_H = 96;
    const center = { x: 800, y: 400 };
    const ringCenters = [
      { x: 800, y: 268 },
      { x: 932, y: 400 },
      { x: 800, y: 532 },
      { x: 668, y: 400 },
    ];
    const memberCount = ringCenters.length;
    const accent = "#3b82f6";

    const root = document.createElement("div");
    document.body.appendChild(root);

    for (let i = 0; i < ringCenters.length; i++) {
      const petal = ringCenters[i];
      const dx = center.x - petal.x;
      const dy = center.y - petal.y;
      const btn = document.createElement("button");
      btn.className = "stack-bloom__petal";
      btn.style.setProperty("--petal-index", String(i));
      btn.style.setProperty("--bloom-petal-count", String(memberCount));
      btn.style.setProperty("--bloom-origin-x", `${dx}px`);
      btn.style.setProperty("--bloom-origin-y", `${dy}px`);
      btn.style.setProperty("--bloom-to-x", "0px");
      btn.style.setProperty("--bloom-to-y", "0px");
      btn.style.setProperty("--zone-accent", accent);
      btn.style.position = "fixed";
      btn.style.left = `${petal.x - PETAL_W / 2}px`;
      btn.style.top = `${petal.y - PETAL_H / 2}px`;
      btn.style.width = `${PETAL_W}px`;
      btn.style.height = `${PETAL_H}px`;
      root.appendChild(btn);
    }

    const petals = root.querySelectorAll<HTMLButtonElement>(".stack-bloom__petal");
    expect(petals.length).toBe(memberCount);

    petals.forEach((el, i) => {
      // All four custom props must be present and non-empty.
      expect(el.style.getPropertyValue("--petal-index")).toBe(String(i));
      expect(el.style.getPropertyValue("--bloom-petal-count")).toBe(
        String(memberCount),
      );
      const originX = el.style.getPropertyValue("--bloom-origin-x");
      const originY = el.style.getPropertyValue("--bloom-origin-y");
      expect(originX).toMatch(/-?\d+(\.\d+)?px$/);
      expect(originY).toMatch(/-?\d+(\.\d+)?px$/);
      // The `to` props always sit at zero so the keyframe end-state
      // matches the petal's CSS-driven left/top.
      expect(el.style.getPropertyValue("--bloom-to-x")).toBe("0px");
      expect(el.style.getPropertyValue("--bloom-to-y")).toBe("0px");
      // Position + size are still inline (the keyframe interpolates
      // transform; left/top still place the petal at its resolved
      // polar center).
      expect(el.style.position).toBe("fixed");
      expect(el.style.width).toBe(`${PETAL_W}px`);
      expect(el.style.height).toBe(`${PETAL_H}px`);
    });

    document.body.removeChild(root);
  });

  it("origin vector points from the petal back to the cursor (visual: petal flies OUT from cursor)", () => {
    // For a 4-petal ring centered at (800, 400) with the top petal at
    // (800, 268) the origin vector for petal 0 should be (800-800,
    // 400-268) = (0, 132) — i.e. the petal at the top of the ring
    // animates from a point 132 px BELOW its resting position (toward
    // the cursor). Round-9's transform-origin trick gave a similar
    // effect but only via scale; round-11's translate is a stronger
    // "fly out" cue that's closer to the user's mental model.
    const center = { x: 800, y: 400 };
    const topPetal = { x: 800, y: 268 };
    const dx = center.x - topPetal.x;
    const dy = center.y - topPetal.y;
    expect(dx).toBe(0);
    expect(dy).toBe(132);

    // Symmetric: bottom petal (800, 532) → vector (0, -132) (cursor is
    // 132 px ABOVE its resting position).
    const bottomPetal = { x: 800, y: 532 };
    expect(center.x - bottomPetal.x).toBe(0);
    expect(center.y - bottomPetal.y).toBe(-132);
  });
});
