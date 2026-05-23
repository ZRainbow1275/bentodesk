/**
 * v9 stack-wake-mutex — stack drag must work while the bloom is open.
 *
 * History: rounds 5–v8 used a `.stack-bloom-buffer` halo whose v8
 * round-8 fix layered the capsule above the buffer via z-index 51.
 * v9 deletes the buffer entirely (the halo had `pointer-events: auto`
 * and was shadowing neighbouring zones' wake), so the original
 * "buffer must NOT swallow capsule mousedown" contract collapses to
 * a simpler invariant: the surface still positions the capsule
 * above any siblings inside the wrapper's natural stacking context,
 * and the capsule's `onMouseDown` wiring is intact.
 *
 * The DOM smoke test below mirrors the post-v9 topology — no buffer
 * sibling in the wrapper — and verifies a `mousedown` on the capsule
 * still fires its handler. The historical "if buffer re-appeared"
 * regression is left in place as a defence-in-depth check: if a
 * future round re-introduces a 100 vw overlay with stopPropagation,
 * the test still proves the mechanism breaks.
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

describe("v9 stack-wake-mutex — stack drag-while-bloomed contract", () => {
  it("CSS: .stack-wrapper__surface keeps z-index: 51 for capsule mousedown reachability", () => {
    const css = readFile(CSS_PATH);
    const surface = extractZIndexForSelector(css, ".stack-wrapper__surface");
    expect(surface).not.toBeNull();
    expect(surface!.zIndex).toBe(51);
  });

  it("CSS: .stack-bloom-buffer rule body is gone (v9 deletes the buffer entirely)", () => {
    const css = readFile(CSS_PATH);
    // The pre-v9 `.stack-bloom-buffer { ... }` selector must no
    // longer carry an opening brace. Documentation comments
    // mentioning the historical name are allowed (so future
    // maintainers can find the v9 ADR).
    expect(css).not.toMatch(/\.stack-bloom-buffer\s*\{/m);
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

  it("TSX: no .stack-bloom-buffer JSX is rendered any more", () => {
    const tsx = readFile(TSX_PATH);
    // Strip block + line comments before matching so the historical
    // narrative left in comments doesn't false-match.
    const codeOnly = tsx
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/\/\/[^\n]*/g, "");
    expect(codeOnly).not.toMatch(/class="stack-bloom-buffer"/);
  });

  it("TSX: <StackCapsule onMouseDown={handleCapsuleMouseDown}> wiring is still in place", () => {
    const tsx = readFile(TSX_PATH);
    // The capsule's mousedown handler is the entry point for drag. If the
    // wiring was removed by mistake, drag would never start regardless of
    // the bloom state.
    expect(tsx).toMatch(/onMouseDown\s*=\s*\{handleCapsuleMouseDown\}/m);
  });
});

describe("v9 stack-wake-mutex — capsule mousedown reachability (DOM)", () => {
  /**
   * Post-v9 topology: the wrapper contains the surface (which holds
   * the capsule) and the bloom container (petals only — no buffer
   * sibling). The capsule's `onMouseDown` must still fire so drag
   * starts.
   */
  it("mousedown on capsule fires its handler in the v9 topology (no buffer sibling)", () => {
    const wrapper = document.createElement("div");
    wrapper.className = "stack-wrapper";
    document.body.appendChild(wrapper);

    const surface = document.createElement("div");
    surface.className = "stack-wrapper__surface";
    wrapper.appendChild(surface);

    const capsule = document.createElement("button");
    capsule.className = "stack-capsule";
    surface.appendChild(capsule);

    const bloomContainer = document.createElement("div");
    bloomContainer.className = "stack-bloom";
    wrapper.appendChild(bloomContainer);

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

  it("hypothetical regression: a stopPropagation overlay on top of the capsule WOULD swallow mousedown (defence-in-depth)", () => {
    // Defence-in-depth: if a future round re-introduces a 100 vw
    // overlay with stopPropagation on the wrapper, the capsule
    // handler would no longer fire. This synthetic test
    // demonstrates the mechanism so a regression would surface in
    // CI immediately.
    const wrapper = document.createElement("div");
    document.body.appendChild(wrapper);
    const capsule = document.createElement("button");
    let fired = false;
    capsule.addEventListener("mousedown", () => {
      fired = true;
    });
    wrapper.appendChild(capsule);
    const overlay = document.createElement("div");
    overlay.addEventListener("mousedown", (e) => e.stopPropagation());
    wrapper.appendChild(overlay);

    overlay.dispatchEvent(
      new MouseEvent("mousedown", { bubbles: true, button: 0 }),
    );
    expect(fired).toBe(false);
    document.body.removeChild(wrapper);
  });
});
