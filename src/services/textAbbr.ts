/**
 * smartAbbreviate — D3 name truncation service.
 *
 * Problem: `text-overflow: ellipsis` loses too much information when zone
 * names are long. "Compiler Project" → "Compile…" drops the semantic
 * identifier entirely. Users can no longer tell zones apart at a glance.
 *
 * Solution: classify characters by script, tokenize, then progressively
 * emit abbreviations based on script rules:
 *   - pure ASCII single-word: head slice ("Compiler" → "C" → "Co" → "Com")
 *   - multi-word ASCII     : initials ("Visual Studio Code" → "VSC")
 *   - pure CJK             : head 2 characters ("编译器" → "编译")
 *   - mixed                : per-segment abbreviation then concatenate
 *
 * Progressive fill uses off-screen canvas measureText so the caller passes
 * just maxPx + the CSS font context.
 */

// ─── Character classification ───────────────────────────────

type CharClass = "han" | "kana" | "hangul" | "word" | "sep" | "other";

/**
 * Classify a codepoint into a high-level bucket.
 *
 * References:
 *   U+4E00 – U+9FFF  CJK unified
 *   U+3040 – U+30FF  Hiragana + Katakana
 *   U+AC00 – U+D7AF  Hangul syllables
 */
export function classifyChar(ch: string): CharClass {
  if (!ch) return "other";
  const cp = ch.codePointAt(0) ?? 0;
  if (cp >= 0x4e00 && cp <= 0x9fff) return "han";
  if (cp >= 0x3040 && cp <= 0x30ff) return "kana";
  if (cp >= 0xac00 && cp <= 0xd7af) return "hangul";
  // ASCII letters / digits, also \w (underscore, accented via ICU not covered)
  if (/^[\w]$/.test(ch)) return "word";
  if (/^[\s\-_.·・]$/.test(ch)) return "sep";
  return "other";
}

export type SegmentKind = "ascii" | "cjk" | "sep";

export interface Segment {
  kind: SegmentKind;
  text: string;
}

/** Break `name` into contiguous segments of same "script family". */
export function segmentName(name: string): Segment[] {
  const out: Segment[] = [];
  let buf = "";
  let bufKind: SegmentKind | null = null;

  const flush = () => {
    if (buf.length > 0 && bufKind) {
      out.push({ kind: bufKind, text: buf });
    }
    buf = "";
    bufKind = null;
  };

  for (const ch of name) {
    const c = classifyChar(ch);
    let k: SegmentKind;
    if (c === "sep") k = "sep";
    else if (c === "han" || c === "kana" || c === "hangul") k = "cjk";
    else k = "ascii";
    if (bufKind !== null && bufKind !== k) flush();
    bufKind = k;
    buf += ch;
  }
  flush();
  return out;
}

// ─── Text measurement ───────────────────────────────────────

let sharedCtx: CanvasRenderingContext2D | null = null;

function getCtx(font: string): CanvasRenderingContext2D | null {
  if (typeof document === "undefined") return null;
  if (!sharedCtx) {
    const canvas = document.createElement("canvas");
    sharedCtx = canvas.getContext("2d");
  }
  if (sharedCtx) sharedCtx.font = font;
  return sharedCtx;
}

export function measureText(s: string, font: string): number {
  const ctx = getCtx(font);
  if (!ctx) return s.length * 8; // rough heuristic for SSR
  return ctx.measureText(s).width;
}

// ─── Abbreviation strategies ────────────────────────────────

/**
 * Produce an ordered list of candidate strings, from shortest to fullest,
 * all of which are valid abbreviations of `name`. The caller walks the
 * list until one overflows the target width and keeps the previous one.
 */
export function abbreviationCandidates(name: string): string[] {
  const segs = segmentName(name);
  if (segs.length === 0) return [""];
  const asciiSegs = segs.filter((s) => s.kind === "ascii");
  const cjkSegs = segs.filter((s) => s.kind === "cjk");
  const multiAsciiWords = asciiSegs.length > 1;

  const out: string[] = [];

  // Start with initials-only for multi-word ASCII ("Visual Studio Code" → "VSC")
  if (multiAsciiWords && cjkSegs.length === 0) {
    const initials = asciiSegs.map((s) => s.text[0]?.toUpperCase() ?? "").join("");
    if (initials.length > 0) out.push(initials);
  }
  // Head 1, 2 CJK characters for a CJK-heavy segment.
  if (cjkSegs.length > 0 && asciiSegs.length === 0) {
    const first = cjkSegs[0].text;
    if (first.length >= 1) out.push(first.slice(0, 1));
    if (first.length >= 2) out.push(first.slice(0, 2));
  }
  // Mixed: concat first character of each segment, progressively adding.
  if (cjkSegs.length > 0 && asciiSegs.length > 0) {
    const heads = segs.filter((s) => s.kind !== "sep").map((s) => s.text[0] ?? "");
    for (let i = 1; i <= heads.length; i++) {
      const candidate = heads.slice(0, i).join("");
      if (candidate.length > 0) out.push(candidate);
    }
  }

  // Progressive head-slice of the full name: this always fits the longer
  // targets. We start from 1 char and go up to the full length.
  for (let i = 1; i <= name.length; i++) {
    out.push(name.slice(0, i));
  }
  out.push(name);

  // De-duplicate while preserving order (shortest → fullest). Dropping
  // duplicates prevents "1 char" from repeating when strategies collide.
  const seen = new Set<string>();
  return out.filter((s) => {
    if (seen.has(s)) return false;
    seen.add(s);
    return true;
  });
}

/**
 * Public API: choose the richest candidate whose measured width does not
 * exceed `maxPx`. Falls back to the shortest 1-char form if even that
 * overflows (avoids returning empty string).
 */
export function smartAbbreviate(
  name: string,
  maxPx: number,
  fontCtx: { font: string; letterSpacing?: number },
): string {
  if (!name) return "";
  if (maxPx <= 0) return name;
  const font = fontCtx.font || "12px sans-serif";
  // Fast path: entire name fits.
  if (measureText(name, font) <= maxPx) return name;

  const candidates = abbreviationCandidates(name);
  let best = candidates[0] ?? name;
  for (const c of candidates) {
    if (measureText(c, font) <= maxPx) {
      best = c;
    } else {
      break;
    }
  }
  return best;
}

/** Extract a CSS `font` shorthand from a mounted element (for ResizeObserver). */
export function getFontCtx(el: HTMLElement): { font: string } {
  if (typeof window === "undefined") return { font: "12px sans-serif" };
  const cs = window.getComputedStyle(el);
  // `font` shorthand may be empty in some browsers; rebuild from parts.
  const short = cs.font;
  if (short && short.length > 0) return { font: short };
  return {
    font: `${cs.fontStyle} ${cs.fontVariant} ${cs.fontWeight} ${cs.fontSize} / ${cs.lineHeight} ${cs.fontFamily}`,
  };
}
