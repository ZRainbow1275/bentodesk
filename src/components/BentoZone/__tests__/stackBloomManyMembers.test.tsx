/**
 * v8 round-14 — many-member bloom: adaptive sizing + overflow cap +
 * stagger cap + many-members modifier.
 *
 * Round-13 shipped with a fixed 108×96 petal box and a 38 ms-per-petal
 * stagger. Live testing surfaced two concerns at scale (the user
 * feedback "多zone堆叠在一起的性能问题和显示问题"):
 *
 *   - Display: a 16-member stack rendered three rows of full-size
 *     108×96 tiles, dominating the viewport.
 *   - Perf: the entry stagger summed to 608 ms and every petal kept
 *     `will-change: transform, opacity` plus the breathing-pulse
 *     animation alive on every active hover.
 *
 * Round-14 introduces:
 *   - `pickPetalSize(count)` — four buckets shrinking the tile + icon
 *     as count grows (≤ 4 / ≤ 8 / ≤ 16 / > 16).
 *   - `--many-members` modifier on the wrapper when count > 8 — CSS
 *     disables the breathing pulse and simplifies the box-shadow.
 *   - Stagger cap — animation-delay calc rewrites to `(360ms / count)
 *     * index` so total entry stagger never exceeds 360 ms.
 *   - Overflow cap — when count > 24, only 23 real petals render +
 *     a "+N more" indicator in the final slot.
 *
 * Source-text contract tests (matching the round-13
 * stackBloomNoAutoOpen.test.tsx pattern). Mounting the full Solid
 * component would require zonesStore + selection + ipc + settings +
 * i18n + the cursor hit-test poller — orthogonal to the round-14
 * invariants this file pins.
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

import { pickPetalSize } from "../../../services/petalLayout";
import {
  HOVER_INTENT_MS,
  LEAVE_GRACE_MS,
} from "../../../services/hoverIntent";

const HERE = dirname(fileURLToPath(import.meta.url));
const TSX_PATH = resolve(HERE, "../StackWrapper.tsx");
const CSS_PATH = resolve(HERE, "../StackWrapper.css");

function readFile(path: string): string {
  return readFileSync(path, "utf8");
}

describe("v8 round-14 — adaptive petal sizing (pure-fn contract)", () => {
  it("5-member stack picks the 92×84 size bucket", () => {
    expect(pickPetalSize(5)).toEqual({ width: 92, height: 84, iconSize: 32 });
  });

  it("12-member stack picks the 80×72 size bucket", () => {
    expect(pickPetalSize(12)).toEqual({ width: 80, height: 72, iconSize: 28 });
  });

  it("25-member stack picks the compact 72×64 size bucket", () => {
    expect(pickPetalSize(25)).toEqual({ width: 72, height: 64, iconSize: 24 });
  });
});

describe("v8 round-14 — StackWrapper integration with adaptive sizing", () => {
  it("StackWrapper imports pickPetalSize from petalLayout", () => {
    const tsx = readFile(TSX_PATH);
    expect(tsx).toMatch(
      /import\s*\{[^}]*pickPetalSize[^}]*\}\s*from\s*"\.\.\/\.\.\/services\/petalLayout"/m,
    );
  });

  it("StackWrapper computes a petalSize memo from member count", () => {
    const tsx = readFile(TSX_PATH);
    expect(tsx).toMatch(
      /const\s+petalSize\s*=\s*createMemo\(\(\)\s*=>\s*pickPetalSize\(memberCount\(\)\)\)/m,
    );
  });

  it("StackWrapper feeds the bucket-picked size into resolvePetalRow", () => {
    const tsx = readFile(TSX_PATH);
    // Verify the petalRow memo dereferences petalSize() and feeds
    // its width/height into resolvePetalRow — rather than a hard-coded
    // 108×96 literal pair. We pin two invariants: (1) the memo
    // reads petalSize(), (2) the call passes width/height derived
    // from that read (the typical destructured pattern: `const size
    // = petalSize(); ... petalSize: { width: size.width, height:
    // size.height }`).
    const memoMatch = /const\s+petalRow\s*=\s*createMemo\([\s\S]*?const\s+size\s*=\s*petalSize\(\);[\s\S]*?return\s+resolvePetalRow\s*\(\s*\{([\s\S]*?)\}\s*\)/m.exec(
      tsx,
    );
    expect(memoMatch).not.toBeNull();
    const args = memoMatch![1];
    // The args must reference size.width / size.height (the destructured
    // values from `const size = petalSize()`).
    expect(args).toMatch(/size\.width/m);
    expect(args).toMatch(/size\.height/m);
    expect(args).toMatch(/petalCount\s*:\s*visiblePetalCount\(\)/m);
  });

  it("Wrapper class list includes the --many-members modifier when count > 8", () => {
    const tsx = readFile(TSX_PATH);
    // The class-template-literal must reference isManyMembers() and the
    // exact modifier string. A future refactor that drops the modifier
    // re-introduces the round-14 perf regression.
    expect(tsx).toMatch(
      /isManyMembers\(\)\s*\?\s*"stack-wrapper--many-members"/m,
    );
  });
});

describe("v8 round-14 — overflow cap (≤ 23 real petals + indicator)", () => {
  it("StackWrapper declares MAX_VISIBLE_MEMBERS = 24", () => {
    const tsx = readFile(TSX_PATH);
    expect(tsx).toMatch(/const\s+MAX_VISIBLE_MEMBERS\s*=\s*24/m);
  });

  it("visibleZones memo slices to MAX_VISIBLE_MEMBERS - 1 when overflowing", () => {
    const tsx = readFile(TSX_PATH);
    // Confirm the slicing logic reserves one slot for the "+N more"
    // indicator (so 24 members → 23 real petals + indicator).
    expect(tsx).toMatch(
      /props\.zones\.slice\(\s*0\s*,\s*MAX_VISIBLE_MEMBERS\s*-\s*1\s*\)/m,
    );
  });

  it("overflow indicator renders only when isOverflowing() is true", () => {
    const tsx = readFile(TSX_PATH);
    // The indicator must be wrapped in <Show when={isOverflowing()}>.
    expect(tsx).toMatch(
      /<Show\s+when=\{isOverflowing\(\)\}>[\s\S]*?stack-bloom__petal--overflow/m,
    );
  });

  it("overflow indicator displays the count via overflowCount()", () => {
    const tsx = readFile(TSX_PATH);
    expect(tsx).toMatch(/\+\{overflowCount\(\)\}/m);
  });
});

describe("v8 round-14 — stagger cap (total entry ≤ 360 ms / exit ≤ 240 ms)", () => {
  it("CSS animation-delay uses the (360ms / count) cap formula for entry", () => {
    const css = readFile(CSS_PATH);
    // The entry rule lives under `.stack-wrapper--bloomed
    // .stack-bloom__petal`. We accept any whitespace variation in the
    // calc() expression but require the 360ms total cap.
    expect(css).toMatch(
      /\.stack-wrapper--bloomed\s*\.stack-bloom__petal\s*\{[\s\S]*?animation-delay:\s*calc\(\s*\(\s*360ms\s*\/\s*max\(1\s*,\s*var\(--bloom-petal-count[\s\S]*?\)\s*\)\s*\)\s*\*\s*var\(--petal-index/m,
    );
  });

  it("CSS animation-delay uses the (240ms / count) cap formula for exit", () => {
    const css = readFile(CSS_PATH);
    expect(css).toMatch(
      /\.stack-bloom__petal--leaving\s*\{[\s\S]*?animation-delay:\s*calc\(\s*\(\s*240ms\s*\/\s*max\(1\s*,\s*var\(--bloom-petal-count[\s\S]*?\)\s*\)\s*\)\s*\*\s*\(/m,
    );
  });

  it("entry cap holds for a 16-petal stack: per-petal delay ≥ 22.5 ms (360 / 16)", () => {
    // Pure-arithmetic check on the formula. CSS engines compute the
    // calc() at render time but the math is deterministic.
    const totalCapMs = 360;
    const count = 16;
    const perPetalMs = totalCapMs / count;
    expect(perPetalMs).toBeCloseTo(22.5, 1);
    // Total stagger (last petal's delay) is bounded by the cap
    // regardless of count: (cap/count) * (count - 1) → cap * (count - 1) / count.
    const lastPetalDelay = perPetalMs * (count - 1);
    expect(lastPetalDelay).toBeLessThanOrEqual(totalCapMs);
  });

  it("entry cap holds for a 24-petal (overflow-clamped) stack", () => {
    const totalCapMs = 360;
    const count = 24;
    const perPetalMs = totalCapMs / count;
    expect(perPetalMs).toBe(15);
    expect(perPetalMs * (count - 1)).toBeLessThanOrEqual(totalCapMs);
  });
});

describe("v8 round-14 — unified hover-intent constants in StackWrapper", () => {
  it("StackWrapper imports HOVER_INTENT_MS / LEAVE_GRACE_MS / STICKY_GRACE_MS", () => {
    const tsx = readFile(TSX_PATH);
    expect(tsx).toMatch(
      /import\s*\{[\s\S]*?HOVER_INTENT_MS[\s\S]*?LEAVE_GRACE_MS[\s\S]*?STICKY_GRACE_MS[\s\S]*?\}\s*from\s*"\.\.\/\.\.\/services\/hoverIntent"/m,
    );
  });

  it("PREVIEW_HOVER_INTENT_MS is aliased from HOVER_INTENT_MS (no magic number)", () => {
    const tsx = readFile(TSX_PATH);
    expect(tsx).toMatch(
      /const\s+PREVIEW_HOVER_INTENT_MS\s*=\s*HOVER_INTENT_MS/m,
    );
  });

  it("ACTIVE_PETAL_GRACE_MS is aliased from LEAVE_GRACE_MS (no magic number)", () => {
    const tsx = readFile(TSX_PATH);
    expect(tsx).toMatch(
      /const\s+ACTIVE_PETAL_GRACE_MS\s*=\s*LEAVE_GRACE_MS/m,
    );
  });

  it("bloom collapse uses LEAVE_GRACE_MS (or STICKY_GRACE_MS for sticky previews)", () => {
    const tsx = readFile(TSX_PATH);
    // Locate the handleMouseLeave block and verify it references
    // LEAVE_GRACE_MS / STICKY_GRACE_MS rather than a bare 80 literal.
    const fnMatch = /const\s+handleMouseLeave\s*=\s*\([\s\S]*?\)\s*=>\s*\{([\s\S]*?)\n\s*\};/m.exec(
      tsx,
    );
    expect(fnMatch).not.toBeNull();
    const body = fnMatch![1];
    expect(body).toMatch(/LEAVE_GRACE_MS/m);
    expect(body).toMatch(/STICKY_GRACE_MS/m);
    // Strict negative: no bare `80` magic number left in the
    // collapse-grace path. (We accept other 80s elsewhere in the
    // file because comments + non-timer code may legitimately use
    // the number; we only police the function body itself.)
    const tailMatch = /\s*,\s*80\s*\)\s*;/.test(body);
    expect(tailMatch).toBe(false);
  });
});

describe("v8 round-14 — sanity values from the shared module", () => {
  it("HOVER_INTENT_MS resolves to 150", () => {
    expect(HOVER_INTENT_MS).toBe(150);
  });

  it("LEAVE_GRACE_MS resolves to 80", () => {
    expect(LEAVE_GRACE_MS).toBe(80);
  });
});
