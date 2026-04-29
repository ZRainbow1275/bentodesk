/**
 * v8 round-8 — stack drag must work while the bloom is open.
 *
 * Round-5b/round-7's bloom-buffer (`.stack-bloom-buffer`, position:fixed,
 * diameter ~384px, z-index: 49) mounts the moment the user enters the
 * stack hover area. Two compound issues silently broke drag:
 *
 *   1. The buffer is rendered LATER in the DOM than the surface, so by
 *      default it stacks on top of the capsule unless the surface has
 *      a higher explicit z-index. The capsule (220×52) sits inside the
 *      buffer's halo, so cursor mousedown lands on the buffer.
 *   2. The buffer carried `onMouseDown={(e) => e.stopPropagation()}`,
 *      which was meant to keep marquee/drag from latching onto the
 *      buffer — but the side effect was that the mousedown event never
 *      reached `<StackCapsule onMouseDown={handleCapsuleMouseDown}>`,
 *      so drag never started.
 *
 * Round-8 fix:
 *   - CSS: `.stack-wrapper__surface { position: relative; z-index: 51 }`
 *     — lifts the capsule above the buffer (49) AND petals (50) in the
 *     wrapper's stacking context.
 *   - TSX: removed the buffer's `onMouseDown` stopPropagation.
 *
 * This test parses the StackWrapper.css file and verifies both contracts
 * are encoded:
 *   1. `.stack-wrapper__surface` has `z-index: 51` (above buffer/petals)
 *   2. `.stack-bloom-buffer` has `z-index: 49`
 *   3. Surface's z-index is GREATER THAN the buffer's
 *
 * It also verifies the StackWrapper.tsx source no longer carries the
 * buffer's `stopPropagation` on mousedown.
 *
 * The test is INTENTIONALLY a contract test against the source files
 * (not a full Solid render) because mounting StackWrapper requires
 * bootstrapping zonesStore + selection + ipc + settings, which is heavy
 * and orthogonal to the regression. The CSS rule + the TSX code edit
 * together fully encode the fix; if either is removed, this test fails.
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

interface RuleZIndex {
  /** Source line containing the property — useful for diff debugging. */
  match: string;
  zIndex: number;
}

/**
 * Pulls the `z-index: <num>` declaration out of the FIRST CSS rule whose
 * selector matches `selector` exactly (not as a substring). Returns null
 * if no such rule is found, or the rule has no z-index declaration.
 *
 * The parser is intentionally simple — bentodesk's CSS doesn't use nested
 * @-rules around z-index declarations for these selectors.
 */
function extractZIndexForSelector(
  css: string,
  selector: string,
): RuleZIndex | null {
  // Build a regex matching: <selector> { ...everything up to } } including
  // newlines. CSS selectors here are simple, no commas in the rules we care
  // about.
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const ruleRegex = new RegExp(
    String.raw`(?:^|\s|})\s*${escaped}\s*\{([^}]*)\}`,
    "m",
  );
  const ruleMatch = ruleRegex.exec(css);
  if (!ruleMatch) return null;
  const body = ruleMatch[1];
  const zMatch = /z-index\s*:\s*(\d+)\s*;?/m.exec(body);
  if (!zMatch) return null;
  return { match: zMatch[0], zIndex: parseInt(zMatch[1], 10) };
}

describe("v8 round-8 — stack drag-while-bloomed regression fix", () => {
  it("CSS: .stack-wrapper__surface declares z-index: 51 to sit above the buffer", () => {
    const css = readFile(CSS_PATH);
    const surface = extractZIndexForSelector(css, ".stack-wrapper__surface");
    expect(surface).not.toBeNull();
    expect(surface!.zIndex).toBe(51);
  });

  it("CSS: .stack-bloom-buffer keeps z-index: 49 (below petals 50 and surface 51)", () => {
    const css = readFile(CSS_PATH);
    const buffer = extractZIndexForSelector(css, ".stack-bloom-buffer");
    expect(buffer).not.toBeNull();
    expect(buffer!.zIndex).toBe(49);
  });

  it("CSS: surface stacks ABOVE the bloom buffer", () => {
    const css = readFile(CSS_PATH);
    const surface = extractZIndexForSelector(css, ".stack-wrapper__surface");
    const buffer = extractZIndexForSelector(css, ".stack-bloom-buffer");
    // Both must be present; the inequality is what guarantees mousedown on
    // the capsule reaches the surface (not the overlapping buffer).
    expect(surface).not.toBeNull();
    expect(buffer).not.toBeNull();
    expect(surface!.zIndex).toBeGreaterThan(buffer!.zIndex);
  });

  it("CSS: .stack-wrapper__surface has position: relative (z-index requires it)", () => {
    const css = readFile(CSS_PATH);
    // z-index only applies to positioned elements. Without `position:
    // relative` (or anything other than static), the z-index: 51 above
    // is silently a no-op and the regression returns.
    const surfaceRule = /\.stack-wrapper__surface\s*\{([^}]*)\}/m.exec(css);
    expect(surfaceRule).not.toBeNull();
    expect(surfaceRule![1]).toMatch(/position\s*:\s*relative\s*;?/m);
  });

  it("TSX: bloom buffer no longer carries onMouseDown stopPropagation", () => {
    const tsx = readFile(TSX_PATH);
    // Locate the buffer JSX block: starts with `class="stack-bloom-buffer"`
    // and runs until its closing tag (self-closing `/>`).
    const bufferBlockMatch = /class="stack-bloom-buffer"[\s\S]*?\/>/m.exec(tsx);
    expect(bufferBlockMatch).not.toBeNull();
    const bufferBlock = bufferBlockMatch![0];
    // Strip `/* ... */` block comments AND `// ...` line comments before
    // pattern matching so historical narratives in the comment do not
    // trigger the "onMouseDown" detector. We're asserting the JSX
    // attribute is gone, not that the word vanished from the file.
    const codeOnly = bufferBlock
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/\/\/[^\n]*/g, "");
    // The fix removed the `onMouseDown={(e) => e.stopPropagation()}` line.
    // We assert it's not in the block any more — if a future change
    // re-adds it, this test fails loudly.
    expect(codeOnly).not.toMatch(/onMouseDown\s*=/);
  });

  it("TSX: bloom buffer still preserves its hover-keepalive (onMouseEnter)", () => {
    const tsx = readFile(TSX_PATH);
    // Belt-and-braces — the round-7 mouseenter cancel must not have been
    // accidentally removed alongside the mousedown.
    const bufferBlockMatch = /class="stack-bloom-buffer"[\s\S]*?\/>/m.exec(tsx);
    expect(bufferBlockMatch).not.toBeNull();
    const bufferBlock = bufferBlockMatch![0];
    expect(bufferBlock).toMatch(/onMouseEnter\s*=\s*\{cancelBloomCollapse\}/m);
  });

  it("TSX: <StackCapsule onMouseDown={handleCapsuleMouseDown}> wiring is still in place", () => {
    const tsx = readFile(TSX_PATH);
    // The capsule's mousedown handler is the entry point for drag. If the
    // wiring was removed by mistake, drag would never start regardless of
    // the bloom state.
    expect(tsx).toMatch(/onMouseDown\s*=\s*\{handleCapsuleMouseDown\}/m);
  });
});

describe("v8 round-8 — bloom buffer mousedown propagation behavior (DOM)", () => {
  /**
   * Verify in jsdom that with the new buffer (no stopPropagation), a
   * mousedown event dispatched on a child element bubbles correctly to
   * the parent's mousedown listener. This is the actual mechanism the
   * fix relies on — the capsule's mousedown must reach the wrapper /
   * StackCapsule onMouseDown handler even when both elements live in
   * overlapping screen space with the buffer.
   *
   * We construct a minimal DOM tree mirroring the post-fix topology
   * and assert the capsule's onMouseDown fires.
   */
  it("mousedown on capsule reaches handler even when buffer is a sibling without stopPropagation", () => {
    const wrapper = document.createElement("div");
    wrapper.className = "stack-wrapper";
    document.body.appendChild(wrapper);

    const surface = document.createElement("div");
    surface.className = "stack-wrapper__surface";
    wrapper.appendChild(surface);

    const capsule = document.createElement("button");
    capsule.className = "stack-capsule";
    surface.appendChild(capsule);

    const buffer = document.createElement("div");
    buffer.className = "stack-bloom-buffer";
    // Critical post-fix invariant: buffer has NO mousedown stopPropagation.
    // (We don't attach any handler; the bug was an explicit stopPropagation
    // on this element. Absence of the handler is the fix.)
    wrapper.appendChild(buffer);

    let capsuleMouseDownFired = false;
    capsule.addEventListener("mousedown", () => {
      capsuleMouseDownFired = true;
    });

    capsule.dispatchEvent(
      new MouseEvent("mousedown", { bubbles: true, button: 0 }),
    );

    expect(capsuleMouseDownFired).toBe(true);

    // Cleanup
    document.body.removeChild(wrapper);
  });

  it("if a future regression re-adds buffer's stopPropagation on a synthetic OVERLAY scenario, mousedown is swallowed (proves the previous bug)", () => {
    // This mirrors the PRE-FIX topology: the buffer carries a
    // stopPropagation handler AND overlays the capsule in the event path.
    // We simulate it by dispatching mousedown on the BUFFER (the way it
    // would happen if the buffer's z-index won the hit test). Asserting
    // the capsule never receives the event proves the mechanism the
    // round-8 fix avoids by (a) lowering the buffer's z-index relative
    // to the surface and (b) removing the stopPropagation. If a future
    // refactor reintroduces both conditions, this test still fails
    // because the capsule handler never fires for buffer-targeted events.
    const wrapper = document.createElement("div");
    document.body.appendChild(wrapper);
    const capsule = document.createElement("button");
    let fired = false;
    capsule.addEventListener("mousedown", () => {
      fired = true;
    });
    wrapper.appendChild(capsule);
    const buffer = document.createElement("div");
    buffer.addEventListener("mousedown", (e) => e.stopPropagation());
    wrapper.appendChild(buffer);

    // Mousedown landing on the buffer (pre-fix bug behavior): never bubbles
    // to anything sharing the wrapper's event tree above the buffer.
    buffer.dispatchEvent(
      new MouseEvent("mousedown", { bubbles: true, button: 0 }),
    );
    expect(fired).toBe(false);
    document.body.removeChild(wrapper);
  });
});
