/**
 * Tests for the v7 text-fit service.
 *
 * v7 strategy change: instead of producing a truncated string with a
 * trailing "…", the service shrinks the font-size proportionally and keeps
 * the full name. The previous `abbreviationCandidates` / `smartAbbreviate`
 * surface is gone; we test `fitFontSize` plus the retained classification
 * helpers (`classifyChar`, `segmentName`, `measureText`).
 *
 * measureText depends on canvas which jsdom doesn't fully implement; we stub
 * getContext to return a width that scales with both string length and the
 * font-size encoded in the `font` shorthand. That lets us assert size-fit
 * behaviour without depending on real glyph metrics.
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  classifyChar,
  segmentName,
  fitFontSize,
  MIN_FONT_SIZE_PX,
} from "../textAbbr";

beforeEach(() => {
  // Width = string length * font-size-px. Extracts the leading number from
  // a CSS `font` shorthand like "13px sans-serif" or "13px family-shorthand".
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
});

describe("classifyChar", () => {
  it("identifies Han characters", () => {
    expect(classifyChar("中")).toBe("han");
    expect(classifyChar("编")).toBe("han");
  });
  it("identifies kana and hangul", () => {
    expect(classifyChar("あ")).toBe("kana");
    expect(classifyChar("한")).toBe("hangul");
  });
  it("identifies ASCII word chars and separators", () => {
    expect(classifyChar("A")).toBe("word");
    expect(classifyChar("_")).toBe("word");
    expect(classifyChar(" ")).toBe("sep");
    expect(classifyChar("-")).toBe("sep");
  });
});

describe("segmentName", () => {
  it("splits ASCII multi-word into word + sep segments", () => {
    const segs = segmentName("Visual Studio Code");
    expect(segs.filter((s) => s.kind === "ascii").map((s) => s.text)).toEqual([
      "Visual",
      "Studio",
      "Code",
    ]);
  });

  it("splits mixed CJK+ASCII into per-script segments", () => {
    const segs = segmentName("编译器Project");
    expect(segs.map((s) => s.kind)).toEqual(["cjk", "ascii"]);
    expect(segs.map((s) => s.text)).toEqual(["编译器", "Project"]);
  });
});

describe("fitFontSize", () => {
  const ctx = { fontFamilyShorthand: "sans-serif", defaultFontSizePx: 13 };

  it("returns the full name at default size when it fits", () => {
    // "Code" = 4 chars * 13px = 52px, container 100px → fits.
    const result = fitFontSize("Code", 100, ctx);
    expect(result.text).toBe("Code");
    expect(result.fontSizePx).toBe(13);
  });

  it("never truncates the rendered text", () => {
    // 18 chars * 13px = 234px at default; container 24px is far too small.
    // v7 must still emit the complete name and shrink the size, not the text.
    const result = fitFontSize("Visual Studio Code", 24, ctx);
    expect(result.text).toBe("Visual Studio Code");
    expect(result.fontSizePx).toBeLessThan(13);
    expect(result.fontSizePx).toBeGreaterThanOrEqual(MIN_FONT_SIZE_PX);
  });

  it("scales font-size proportionally to fit width", () => {
    // 3 chars * 13px = 39px. Target 26px → ratio 2/3 → ~8.67 → floor → 8 px,
    // clamped to MIN_FONT_SIZE_PX = 8.
    const result = fitFontSize("编译器", 26, ctx);
    expect(result.text).toBe("编译器");
    expect(result.fontSizePx).toBe(8);
  });

  it("keeps font-size at MIN_FONT_SIZE_PX even if full name still overflows", () => {
    // 3 chars * 13px = 39px at default; container 8px is too tight for any
    // size that would honour the full string. v7 contract: render anyway at
    // 8px rather than abbreviate.
    const result = fitFontSize("编译器", 8, ctx);
    expect(result.text).toBe("编译器");
    expect(result.fontSizePx).toBe(MIN_FONT_SIZE_PX);
  });

  it("returns full name and default size when maxPx is 0 (pre-layout)", () => {
    // Pre-layout fallback: no measurement available yet. We must not emit a
    // "…" placeholder — return the complete name and let the next RO tick
    // shrink if needed.
    const result = fitFontSize("Project", 0, ctx);
    expect(result.text).toBe("Project");
    expect(result.fontSizePx).toBe(13);
  });

  it("handles empty input without throwing", () => {
    const result = fitFontSize("", 100, ctx);
    expect(result.text).toBe("");
    expect(result.fontSizePx).toBe(13);
  });

  it("never returns a font-size below MIN_FONT_SIZE_PX", () => {
    // Pathological 1px container — clamp must still kick in.
    const result = fitFontSize("Project", 1, ctx);
    expect(result.fontSizePx).toBe(MIN_FONT_SIZE_PX);
    expect(result.text).toBe("Project");
  });

  it("never exceeds defaultFontSizePx (no upscaling for short names)", () => {
    // Container is huge but we must not enlarge above the CSS-declared size.
    const result = fitFontSize("A", 1000, ctx);
    expect(result.fontSizePx).toBe(13);
  });
});
