/**
 * v8 round-13 — bloom open MUST NOT auto-pick a member.
 *
 * Round-12 shipped with a regression where `handlePetalEnter` set
 * `previewZoneId` synchronously on petal mouseenter. Combined with the
 * round-11 bloom entry animation (petals start scaled at the capsule
 * centre and fly out to their row positions), the cursor — sitting on
 * the capsule when the bloom triggers — would graze a petal during
 * the entry frames, instantly committing to that member's
 * FocusedZonePreview before the user had even taken in the choice.
 *
 * The user reported "stack默认直接打开其中的一个zone，这样不好":
 * stack opens, ONE zone is auto-selected. This contradicts the user's
 * mental model — bloom is a "show options" affordance, not a "commit
 * to first option" affordance.
 *
 * Round-13 fix:
 *   - introduce `activePetalId` decoupled from `previewZoneId`
 *   - bloom open: BOTH null (no member implicitly active, no preview
 *     implicitly mounted)
 *   - hover petal: activePetalId flips synchronously (immediate visual
 *     feedback via the breathing pulse), previewZoneId opens after a
 *     150 ms hover-intent debounce
 *   - click petal: both flip synchronously, sticky flag set so the
 *     preview survives hover-off
 *
 * This file pins the source-level invariants for the no-auto-open
 * contract. The bloom render path includes:
 *   - the petal class list MUST key off `activePetalId()`, NOT
 *     `previewZoneId()`
 *   - `handlePetalEnter` MUST schedule the preview via setTimeout, NOT
 *     call `setPreviewZoneId(zoneId)` synchronously
 *   - `handleMouseEnter` (capsule hover) MUST NOT call setPreviewZoneId
 *     OR setActivePetalId at all — entering the wrapper opens the
 *     bloom but commits to no member
 *   - `handleCapsuleClick` (tap-to-bloom) likewise
 *
 * The contract is verified at source-text level rather than via a
 * full Solid render. StackWrapper imports zonesStore + selection +
 * ipc + settings + i18n + the cursor hit-test poller, which would
 * cost ~2 s of bootstrap per test for a fundamentally string-level
 * invariant. The same pattern is used by stackBloomAnimation.test.tsx
 * (round-11) and stackDissolveFlow.test.ts (v9) elsewhere in this
 * directory.
 */
import { describe, it, expect } from "vitest";
// node:fs / url / path are provided by the vitest Node runner; project
// intentionally does not depend on @types/node in production. Pattern
// mirrors stackBloomAnimation.test.tsx + spec-matrix.test.ts.
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
const TSX_PATH = resolve(HERE, "../StackWrapper.tsx");

function readFile(path: string): string {
  return readFileSync(path, "utf8");
}

describe("v8 round-13 — bloom open ≠ preview open (source contract)", () => {
  it("StackWrapper declares an `activePetalId` signal decoupled from previewZoneId", () => {
    const tsx = readFile(TSX_PATH);
    // Both signals must coexist — round-13's whole point is that they
    // are SEPARATE state. If a future refactor merges them back, the
    // auto-open regression returns immediately.
    expect(tsx).toMatch(
      /const\s*\[\s*activePetalId\s*,\s*setActivePetalId\s*\]\s*=\s*createSignal/m,
    );
    expect(tsx).toMatch(
      /const\s*\[\s*previewZoneId\s*,\s*setPreviewZoneId\s*\]\s*=\s*createSignal/m,
    );
  });

  it("StackWrapper declares a `previewSticky` flag for click-committed previews", () => {
    const tsx = readFile(TSX_PATH);
    // Click-commits set sticky=true so the preview survives hover-off
    // of the petal. Without this flag the click path would tear the
    // preview down the moment the cursor moves to a sibling petal.
    expect(tsx).toMatch(
      /const\s*\[\s*previewSticky\s*,\s*setPreviewSticky\s*\]\s*=\s*createSignal/m,
    );
  });

  it("petal class list keys off activePetalId(), NOT previewZoneId()", () => {
    const tsx = readFile(TSX_PATH);
    // Locate the petal button's class= attribute and verify it
    // references activePetalId for the --active modifier. A stray
    // `previewZoneId() === zone.id ? "stack-bloom__petal--active"` is
    // EXACTLY the round-12 regression we are guarding against.
    const classMatch = /class=\{`stack-bloom__petal\s*\$\{([\s\S]*?)stack-bloom__petal--active([\s\S]*?)\}/m.exec(
      tsx,
    );
    expect(classMatch).not.toBeNull();
    const block = classMatch![0];
    expect(block).toMatch(/activePetalId\(\)\s*===\s*zone\.id/m);
    // Strict negative: previewZoneId must NOT drive the active
    // modifier. The class block here is small enough that grepping
    // for `previewZoneId(` inside it is safe.
    expect(block).not.toMatch(/previewZoneId\(\)\s*===\s*zone\.id\s*\?\s*"stack-bloom__petal--active"/m);
  });

  it("handlePetalEnter schedules preview via setTimeout, NOT a synchronous setPreviewZoneId", () => {
    const tsx = readFile(TSX_PATH);
    // The function body must contain a `setTimeout(...)` call AND must
    // NOT contain a bare `setPreviewZoneId(zoneId)` outside that
    // setTimeout (a sticky-swap path is allowed but is gated on
    // previewSticky() being true — see the next test for that).
    const fnMatch = /const\s+handlePetalEnter\s*=\s*\(\s*zoneId[^)]*\)\s*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    // setActivePetalId fires synchronously (immediate visual feedback).
    expect(body).toMatch(/setActivePetalId\(\s*zoneId\s*\)/m);
    // The preview must be scheduled, not committed. We require a
    // setTimeout call referencing setPreviewZoneId in its callback.
    expect(body).toMatch(
      /setTimeout\s*\(\s*\(\s*\)\s*=>\s*\{[\s\S]*?setPreviewZoneId\s*\(\s*zoneId\s*\)[\s\S]*?\}\s*,\s*PREVIEW_HOVER_INTENT_MS\s*\)/m,
    );
  });

  it("handlePetalEnter sticky-swap path: a sticky preview switches synchronously to the new petal", () => {
    const tsx = readFile(TSX_PATH);
    const fnMatch = /const\s+handlePetalEnter\s*=\s*\(\s*zoneId[^)]*\)\s*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    // The sticky-swap branch reads previewSticky() and previewZoneId()
    // and short-circuits with a synchronous setPreviewZoneId. This
    // avoids the 150 ms latency on a panel that's already mounted.
    expect(body).toMatch(/previewSticky\(\)/m);
    expect(body).toMatch(/previewZoneId\(\)\s*!==\s*zoneId/m);
  });

  it("handlePetalLeave cancels the pending preview-open timer + reverts active after grace", () => {
    const tsx = readFile(TSX_PATH);
    const fnMatch = /const\s+handlePetalLeave\s*=\s*\(\s*zoneId[^)]*\)\s*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    // Immediate timer cancel — the deferred preview-open must not fire
    // after the cursor has left the petal.
    expect(body).toMatch(/cancelPreviewOpenTimer\(\)/m);
    // Active revert is gated by the grace timer.
    expect(body).toMatch(
      /setTimeout\s*\(\s*\(\s*\)\s*=>\s*\{[\s\S]*?setActivePetalId\s*\(\s*null\s*\)[\s\S]*?\}\s*,\s*ACTIVE_PETAL_GRACE_MS\s*\)/m,
    );
  });

  it("handlePetalClick commits both signals synchronously and sets the sticky flag", () => {
    const tsx = readFile(TSX_PATH);
    const fnMatch = /const\s+handlePetalClick\s*=\s*\(\s*e\s*:\s*MouseEvent\s*,\s*zoneId[^)]*\)\s*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    // Click is the explicit commit — setActivePetalId + setPreviewZoneId
    // + setPreviewSticky(true) all fire synchronously.
    expect(body).toMatch(/setActivePetalId\(\s*zoneId\s*\)/m);
    expect(body).toMatch(/setPreviewZoneId\(\s*zoneId\s*\)/m);
    expect(body).toMatch(/setPreviewSticky\(\s*true\s*\)/m);
    // Click on a same-id sticky preview toggles closed (matches the
    // pre-round-13 toggle semantics).
    expect(body).toMatch(/previewSticky\(\)\s*&&\s*previewZoneId\(\)\s*===\s*zoneId/m);
  });

  it("handleMouseEnter (capsule hover) does NOT call setPreviewZoneId or setActivePetalId", () => {
    const tsx = readFile(TSX_PATH);
    // The wrapper-level mouseenter handler opens bloom but must not
    // commit to any member zone. If a future edit reintroduces an
    // auto-pick on bloom open here, this test fails immediately.
    const fnMatch = /const\s+handleMouseEnter\s*=\s*\([\s\S]*?\)\s*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    // setIsBloomed(true) IS expected. setPreviewZoneId / setActivePetalId
    // with non-null arguments are NOT.
    expect(body).toMatch(/setIsBloomed\(\s*true\s*\)/m);
    // Strict negative: no setPreviewZoneId(<id>) anywhere in the body.
    // (Allow setPreviewZoneId(null) only — but the round-13 design
    // doesn't need that here either.)
    const previewWriteRegex = /setPreviewZoneId\s*\(\s*[^)\s]/g;
    expect(previewWriteRegex.test(body)).toBe(false);
    const activeWriteRegex = /setActivePetalId\s*\(\s*[^)\s]/g;
    expect(activeWriteRegex.test(body)).toBe(false);
  });

  it("handleCapsuleClick (tap-to-bloom) does NOT call setPreviewZoneId(zoneId) or setActivePetalId(zoneId)", () => {
    const tsx = readFile(TSX_PATH);
    const fnMatch = /const\s+handleCapsuleClick\s*=\s*\([\s\S]*?\)\s*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    // setIsBloomed(true) IS expected on a fresh tap. setPreviewZoneId
    // with a non-null argument is NOT — bloom open never commits.
    expect(body).toMatch(/setIsBloomed\(\s*true\s*\)/m);
    // We allow setPreviewZoneId(null) (the bloom-already-open branch
    // resets state on second tap), so we only forbid non-null writes.
    // Match `setPreviewZoneId(<non-whitespace, non-`null`> ...`).
    const writes = body.match(/setPreviewZoneId\s*\(\s*([^)]*)\)/g) ?? [];
    for (const w of writes) {
      // Strip leading "setPreviewZoneId(" and trailing ")".
      const arg = w.replace(/setPreviewZoneId\s*\(\s*/, "").replace(/\s*\)$/, "");
      expect(arg).toBe("null");
    }
    const activeWrites = body.match(/setActivePetalId\s*\(\s*([^)]*)\)/g) ?? [];
    for (const w of activeWrites) {
      const arg = w.replace(/setActivePetalId\s*\(\s*/, "").replace(/\s*\)$/, "");
      expect(arg).toBe("null");
    }
  });
});

describe("v8 round-13 — pure-reducer model of bloom-open initial state", () => {
  /**
   * Model the round-13 decoupled state shape. Bloom open creates a
   * fresh blank slate: previewZoneId = null AND activePetalId = null.
   * No member is implicitly chosen. Three cases — single member
   * (degenerate), two members (the user's own setup in the bug
   * report), four members (a typical full stack).
   */
  interface BloomState {
    isBloomed: boolean;
    previewZoneId: string | null;
    activePetalId: string | null;
    previewSticky: boolean;
  }

  function blankBloomOpen(memberIds: string[]): BloomState {
    void memberIds; // member count is irrelevant to initial state
    return {
      isBloomed: true,
      previewZoneId: null,
      activePetalId: null,
      previewSticky: false,
    };
  }

  it("single-member stack: bloom open is null/null", () => {
    const s = blankBloomOpen(["m1"]);
    expect(s.isBloomed).toBe(true);
    expect(s.previewZoneId).toBeNull();
    expect(s.activePetalId).toBeNull();
    expect(s.previewSticky).toBe(false);
  });

  it("two-member stack (the user's bug-report shape): bloom open is null/null", () => {
    const s = blankBloomOpen(["网络", "文件"]);
    expect(s.isBloomed).toBe(true);
    expect(s.previewZoneId).toBeNull();
    expect(s.activePetalId).toBeNull();
    expect(s.previewSticky).toBe(false);
  });

  it("four-member stack: bloom open is null/null regardless of member count", () => {
    const s = blankBloomOpen(["m1", "m2", "m3", "m4"]);
    expect(s.isBloomed).toBe(true);
    expect(s.previewZoneId).toBeNull();
    expect(s.activePetalId).toBeNull();
    expect(s.previewSticky).toBe(false);
  });
});
