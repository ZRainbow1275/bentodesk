/**
 * Tests for smartAbbreviate — character classification, segmentation, and
 * progressive candidate generation across CJK / ASCII / mixed inputs.
 *
 * measureText depends on canvas which jsdom doesn't fully implement; we stub
 * getContext to return a controllable width that scales with string length.
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  classifyChar,
  segmentName,
  abbreviationCandidates,
  smartAbbreviate,
} from "../textAbbr";

beforeEach(() => {
  // Ensure each char is "8px" wide so tests can reason about widths.
  const fakeCtx = {
    font: "",
    measureText: (s: string) => ({ width: s.length * 8 }),
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

describe("abbreviationCandidates", () => {
  it("starts with initials for multi-word ASCII", () => {
    const cand = abbreviationCandidates("Visual Studio Code");
    expect(cand[0]).toBe("VSC");
  });

  it("includes CJK head-1 and head-2 for pure CJK", () => {
    const cand = abbreviationCandidates("编译器");
    expect(cand).toContain("编");
    expect(cand).toContain("编译");
  });

  it("returns increasing-length unique candidates ending in full name", () => {
    const cand = abbreviationCandidates("Compiler");
    expect(cand[cand.length - 1]).toBe("Compiler");
    expect(new Set(cand).size).toBe(cand.length);
  });
});

describe("smartAbbreviate", () => {
  it("returns the full name when it fits", () => {
    const result = smartAbbreviate("Code", 100, { font: "12px sans-serif" });
    expect(result).toBe("Code");
  });

  it("returns a 3-char abbreviation for very tight widths (multi-word ASCII)", () => {
    // stub width=8px/char; allow 24px = 3 chars. The algorithm picks the
    // richest 3-char candidate that still fits; both "VSC" and "Vis" are
    // valid — we assert on length rather than identity.
    const result = smartAbbreviate("Visual Studio Code", 24, {
      font: "12px sans-serif",
    });
    expect(result.length).toBe(3);
    // And it must genuinely be an abbreviation, not accidentally fitting.
    expect(result.length).toBeLessThan("Visual Studio Code".length);
  });

  it("returns head-1 CJK when 2 would overflow", () => {
    // width=8px/char; 8px budget → 1 Han char.
    const result = smartAbbreviate("编译器", 8, { font: "12px sans-serif" });
    expect(result).toBe("编");
  });

  it("never returns an empty string for non-empty input", () => {
    const result = smartAbbreviate("Project", 1, {
      font: "12px sans-serif",
    });
    expect(result.length).toBeGreaterThan(0);
  });
});
