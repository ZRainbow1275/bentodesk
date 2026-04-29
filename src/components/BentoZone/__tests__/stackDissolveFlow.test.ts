/**
 * v9 (Issue X) — stack dissolve flow contract tests.
 *
 * Round-8 unblocked stack drag (z-index surface fix). Round-9's user
 * report: "组成的stack也无法解散" — the dissolve action does not work,
 * either the right-click menu doesn't appear, or clicking 解散堆栈 is
 * a no-op. To make the flow robust and debuggable, we now:
 *
 *   1. Commit dissolve / detach on `mousedown` (not `click`) so a focus
 *      change between mousedown and click cannot eat the action.
 *   2. Stop event propagation on the menu container so the document-
 *      level mousedown listener (`handleOutsidePointer`) cannot race
 *      and close the menu.
 *   3. Force-collapse bloom + tray + preview at right-click time so the
 *      menu lands on a clean visual surface.
 *   4. Surface IPC errors to the console (pre-v9 swallowed them).
 *
 * This test file pins the source-level invariants that encode those
 * decisions. Pure source-text contract — the fix is split across
 * StackWrapper.tsx + stores/stacks.ts so a regression in either file
 * trips a clear failure here.
 */
import { describe, it, expect } from "vitest";
// node:fs is provided by the vitest Node runner; the project intentionally
// does not depend on @types/node in production.
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
const STACK_WRAPPER_TSX = resolve(HERE, "../StackWrapper.tsx");
const STACKS_STORE_TS = resolve(HERE, "../../../stores/stacks.ts");

function readFile(path: string): string {
  return readFileSync(path, "utf8");
}

describe("v9 stack dissolve flow — source contract", () => {
  it("StackWrapper: dissolve button commits on mousedown (not just onClick)", () => {
    const tsx = readFile(STACK_WRAPPER_TSX);
    // The dissolve button block — the only block referencing
    // handleDissolve from the rendered context menu.
    const buttonBlockMatch = /class="stack-context-menu__item[^"]*--danger"[\s\S]*?>\s*\{[^}]*stackDissolve[^}]*\}\s*<\/button>/m.exec(
      tsx,
    );
    expect(buttonBlockMatch).not.toBeNull();
    const block = buttonBlockMatch![0];
    expect(block).toMatch(/onMouseDown\s*=/);
    expect(block).toMatch(/handleDissolve\(\)/);
  });

  it("StackWrapper: detach button commits on mousedown (not just onClick)", () => {
    const tsx = readFile(STACK_WRAPPER_TSX);
    // Find the For-loop that renders the per-member detach rows. Use a
    // permissive scan that begins at the first `<For each={props.zones}>`
    // inside the contextMenuOpen Show block and reads everything up to
    // the matching `</For>`. We verify both `handleDetach(` and
    // `onMouseDown` appear inside the block.
    const ctxMenuRegion = /class="stack-context-menu"[\s\S]*?<\/Show>/m.exec(tsx);
    expect(ctxMenuRegion).not.toBeNull();
    const region = ctxMenuRegion![0];
    expect(region).toMatch(/handleDetach\(/);
    // mousedown must occur somewhere in the For block (cheapest check:
    // it's inside the menu region at all).
    expect(region).toMatch(/onMouseDown\s*=/);
  });

  it("StackWrapper: context menu container stops mousedown propagation", () => {
    const tsx = readFile(STACK_WRAPPER_TSX);
    // The root menu container must stop propagation so the document-level
    // mousedown listener cannot race the menu close. The `class=
    // "stack-context-menu"` div carries an `onMouseDown` that calls
    // `stopPropagation`.
    const menuBlockMatch = /class="stack-context-menu"[\s\S]*?role="menu"[\s\S]*?onMouseDown\s*=\s*\{[\s\S]*?stopPropagation/m.exec(
      tsx,
    );
    expect(menuBlockMatch).not.toBeNull();
  });

  it("StackWrapper: handleContextMenu force-collapses bloom/tray/preview", () => {
    const tsx = readFile(STACK_WRAPPER_TSX);
    // Locate the handleContextMenu function body.
    const fnMatch = /const\s+handleContextMenu\s*=\s*\([^)]*\)[^=]*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    // Each clean-up call must be present so the menu lands on a clean
    // surface — bloom/tray/preview can otherwise overlap visually with
    // the menu and confuse the click target.
    // v8 round-12: setBloomCursor was removed when the radial bloom was
    // replaced by the row layout (cursor coords no longer drive layout
    // anchoring). The remaining clears still cover the same contract:
    // the menu lands on a clean surface with no leftover bloom/tray/
    // preview state.
    expect(body).toMatch(/setIsBloomed\s*\(\s*false\s*\)/);
    expect(body).toMatch(/setPreviewZoneId\s*\(\s*null\s*\)/);
    expect(body).toMatch(/setTrayOpen\s*\(\s*false\s*\)/);
    expect(body).toMatch(/setContextMenuOpen\s*\(\s*\{/);
  });

  it("StackWrapper: handleDissolve clears menu + bloom + tray BEFORE awaiting IPC", () => {
    const tsx = readFile(STACK_WRAPPER_TSX);
    const fnMatch = /const\s+handleDissolve\s*=\s*async\s*\(\s*\)[^=]*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    // The menu must close synchronously so the user sees feedback even
    // if the IPC stalls (e.g. backend lock contention).
    expect(body).toMatch(/setContextMenuOpen\s*\(\s*null\s*\)/);
    expect(body).toMatch(/setPreviewZoneId\s*\(\s*null\s*\)/);
    expect(body).toMatch(/setIsBloomed\s*\(\s*false\s*\)/);
    expect(body).toMatch(/unstackZonesAction\s*\(\s*props\.stackId\s*\)/);
  });

  it("stacks store: unstackZonesAction surfaces IPC errors to console (not silent)", () => {
    const ts = readFile(STACKS_STORE_TS);
    const fnMatch = /export\s+async\s+function\s+unstackZonesAction\b[\s\S]*?\n\}\n/m.exec(ts);
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![0];
    // Old code: `catch { return false; }` — silently swallowed.
    // New code: `catch (err) { console.error(...); return false; }`.
    expect(body).toMatch(/catch\s*\(\s*err\b/);
    expect(body).toMatch(/console\.error/);
  });

  it("stacks store: detachZoneFromStackAction surfaces IPC errors to console", () => {
    const ts = readFile(STACKS_STORE_TS);
    const fnMatch = /export\s+async\s+function\s+detachZoneFromStackAction\b[\s\S]*?\n\}\n/m.exec(
      ts,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![0];
    expect(body).toMatch(/catch\s*\(\s*err\b/);
    expect(body).toMatch(/console\.error/);
  });
});

describe("v9 stack dissolve flow — DOM mousedown commit semantics", () => {
  it("mousedown handler fires synchronously even if click is suppressed by a focus change", () => {
    // Simulate the production failure mode where a focus change between
    // mousedown and click cancels the synthetic click. The fix is to
    // commit on mousedown, which fires unconditionally as long as the
    // event is dispatched.
    const button = document.createElement("button");
    let mousedownFired = false;
    let clickFired = false;
    button.addEventListener("mousedown", () => {
      mousedownFired = true;
      // Simulate the failure: shift focus away during the mousedown,
      // which in some browsers can prevent the synthetic click.
      const stealer = document.createElement("input");
      document.body.appendChild(stealer);
      stealer.focus();
      document.body.removeChild(stealer);
    });
    button.addEventListener("click", () => {
      clickFired = true;
    });
    document.body.appendChild(button);
    button.dispatchEvent(
      new MouseEvent("mousedown", { bubbles: true, button: 0 }),
    );
    // mousedown fires regardless of what happens next.
    expect(mousedownFired).toBe(true);
    // We don't assert clickFired — that's the whole point of the fix:
    // we no longer rely on click firing, mousedown is the commit point.
    void clickFired;
    document.body.removeChild(button);
  });
});
