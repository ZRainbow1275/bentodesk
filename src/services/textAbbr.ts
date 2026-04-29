/**
 * v7 text-fit service — proportional font shrinking, no truncation.
 *
 * Strategy change vs. v6: instead of producing an abbreviated string with a
 * trailing "…", we keep the **complete** name and shrink the font-size until
 * the rendered width fits the container. The minimum size is 8px; if the
 * full name still overflows at 8px, we keep 8px and let the glyphs render
 * tightly (visually dense but information-complete).
 *
 * Algorithm:
 *   1. Measure the full name at the element's default font-size.
 *   2. If nameW <= maxPx → render at default size.
 *   3. Otherwise targetSize = clamp(8, defaultSize * maxPx / nameW, defaultSize).
 *   4. Floor targetSize to integer pixels and re-emit.
 *
 * The hook still owns ResizeObserver + fonts.ready re-measurement so the
 * three trigger paths (mount, container resize, web font load) all converge
 * on a single text() / fontSize() pair.
 *
 * v6 carry-over: classifyChar / segmentName / measureText / getFontCtx are
 * retained and exported because tests and future strategies (e.g. CJK-aware
 * letter-spacing tweaks) may reuse them. The old "candidates"-and-pick-the-
 * richest path and the FIRST_FRAME_FALLBACK_CHARS "…" splice are gone.
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

// ─── Sizing strategy ────────────────────────────────────────

/** Lower bound for the proportional shrink. Below this glyphs are unreadable. */
export const MIN_FONT_SIZE_PX = 8;

export interface FitFontContext {
  /** Family-only CSS `font` shorthand fragment, e.g. `"500 / 1.3 'Inter', sans-serif"`. */
  fontFamilyShorthand: string;
  /** Default (CSS-declared) font-size in pixels — the size we measure at first. */
  defaultFontSizePx: number;
}

export interface FitResult {
  /** The text to render — always equal to the input `name` (never truncated). */
  text: string;
  /** Font-size in CSS pixels. Equal to defaultFontSizePx when the name fits. */
  fontSizePx: number;
}

/**
 * v7 public API: pick a font-size that lets the full `name` render within
 * `maxPx`. Returns both the (untruncated) text and the chosen size so callers
 * can apply it inline.
 *
 * Edge cases:
 *  - Empty name → empty text, default size (caller still has a stable element).
 *  - maxPx <= 0 (pre-layout) → return default size; the parent's overflow
 *    rules + the next ResizeObserver tick will correct any one-frame bleed.
 *    Crucially we no longer emit a "…" placeholder, so the user-visible name
 *    is correct from the moment real measurement runs.
 *  - Full name still overflows at MIN_FONT_SIZE_PX → keep MIN_FONT_SIZE_PX
 *    and let the browser render the glyphs tightly. Information-complete
 *    beats visually-clean per the v7 product decision.
 */
export function fitFontSize(
  name: string,
  maxPx: number,
  ctx: FitFontContext,
): FitResult {
  if (!name) {
    return { text: "", fontSizePx: ctx.defaultFontSizePx };
  }
  if (maxPx <= 0) {
    return { text: name, fontSizePx: ctx.defaultFontSizePx };
  }

  const defaultFont = `${ctx.defaultFontSizePx}px ${ctx.fontFamilyShorthand}`;
  const widthAtDefault = measureText(name, defaultFont);

  if (widthAtDefault <= maxPx) {
    return { text: name, fontSizePx: ctx.defaultFontSizePx };
  }

  // Linear scaling estimate — for fonts that don't kern wildly across sizes
  // this lands within ~1 px of the binary-search optimum without the loop.
  const ratio = maxPx / widthAtDefault;
  const scaled = Math.floor(ctx.defaultFontSizePx * ratio);
  const targetSize = Math.max(
    MIN_FONT_SIZE_PX,
    Math.min(ctx.defaultFontSizePx, scaled),
  );
  return { text: name, fontSizePx: targetSize };
}

/**
 * Read the family-only `font` shorthand and the default font-size from a
 * mounted element. We split size out so that re-measuring after we've
 * applied an inline `font-size: 8px` doesn't poison the "default" reference.
 *
 * The caller is expected to record the default size **once** at mount time
 * (or whenever the element's CSS-declared size genuinely changes — e.g.
 * theme switch, which we do not currently support).
 */
export function readFontContext(el: HTMLElement): FitFontContext {
  if (typeof window === "undefined") {
    return { fontFamilyShorthand: "sans-serif", defaultFontSizePx: 13 };
  }
  const cs = window.getComputedStyle(el);
  const sizePx = parsePx(cs.fontSize) || 13;
  const family = cs.fontFamily || "sans-serif";
  const weight = cs.fontWeight || "normal";
  const style = cs.fontStyle || "normal";
  const variant = cs.fontVariant || "normal";
  const lineHeight = cs.lineHeight && cs.lineHeight !== "normal" ? cs.lineHeight : "normal";
  // Note: we deliberately omit the size from the shorthand so callers can
  // splice in their target size each measurement.
  const fontFamilyShorthand = `${style} ${variant} ${weight} / ${lineHeight} ${family}`;
  return { fontFamilyShorthand, defaultFontSizePx: sizePx };
}

function parsePx(v: string): number {
  const m = /^(\d+(?:\.\d+)?)px$/.exec(v.trim());
  return m ? parseFloat(m[1]) : 0;
}

// ─── Solid hook ─────────────────────────────────────────────

import { createSignal, createMemo, onMount, onCleanup, type Accessor } from "solid-js";

/**
 * Solid composable that turns a long string + an element ref into a reactive
 * font-size that keeps the full text visible. Wires up the trio that
 * StackCapsule / PanelHeader / ZenCapsule / StackTray member rows / ItemCard
 * names all need:
 *
 *   1. A `setRef` callback to attach to the title span.
 *   2. A reactive `text()` accessor — always the full name (v7).
 *   3. A reactive `fontSize()` accessor — pixels, applied inline by callers.
 *   4. A reactive `tooltipDisabled()` accessor — true when default size is
 *      used (i.e. nothing was shrunk and the glyphs match the surrounding
 *      typography). At smaller sizes the tooltip stays enabled so users can
 *      hover for a normal-weight read.
 *
 * The hook owns the ResizeObserver lifecycle and the fonts.ready listener;
 * callers do not manage onCleanup themselves.
 */
export interface UseTextAbbrResult {
  setRef: (el: HTMLElement | undefined) => void;
  text: Accessor<string>;
  fontSize: Accessor<number>;
  tooltipDisabled: Accessor<boolean>;
}

export function useTextAbbr(fullText: () => string): UseTextAbbrResult {
  const [el, setEl] = createSignal<HTMLElement | undefined>();
  const [maxPx, setMaxPx] = createSignal(0);
  const [fontCtx, setFontCtx] = createSignal<FitFontContext>({
    fontFamilyShorthand: "sans-serif",
    defaultFontSizePx: 13,
  });

  onMount(() => {
    const node = el();
    if (!node) return;
    // Capture the CSS-declared default size **before** we ever apply an
    // inline override. Re-reading getComputedStyle later would observe our
    // own inline size and shrink the "default" further on each measurement.
    setFontCtx(readFontContext(node));

    let rafId: number | null = null;
    let lastWidth = -1;
    let disposed = false;
    const commitMeasure = () => {
      rafId = null;
      const w = Math.round(node.clientWidth);
      if (w !== lastWidth && w > 0) {
        lastWidth = w;
        setMaxPx(w);
      }
    };
    const measure = () => {
      if (rafId !== null) return;
      rafId =
        typeof requestAnimationFrame === "function"
          ? requestAnimationFrame(commitMeasure)
          : (setTimeout(commitMeasure, 16) as unknown as number);
    };

    // rAF-poll clientWidth until layout settles. The synchronous mount-time
    // sample frequently reads 0 because the ItemCard sits inside a flex/grid
    // track whose width is distributed in a later layout pass than the one
    // that mounts the child. Polling for up to ~5 frames lets us catch the
    // real width as soon as the parent's flex distribution commits, without
    // blocking the first paint.
    const MAX_BOOTSTRAP_FRAMES = 5;
    let bootstrapAttempts = 0;
    const bootstrapMeasure = () => {
      if (disposed) return;
      const w = Math.round(node.clientWidth);
      if (w > 0) {
        lastWidth = w;
        setMaxPx(w);
        return;
      }
      if (bootstrapAttempts < MAX_BOOTSTRAP_FRAMES) {
        bootstrapAttempts++;
        if (typeof requestAnimationFrame === "function") {
          requestAnimationFrame(bootstrapMeasure);
        } else {
          setTimeout(bootstrapMeasure, 16);
        }
      }
    };
    bootstrapMeasure();

    // When web fonts arrive after first paint, the canvas measurer was using
    // fallback metrics that under/over-estimated glyph width. Re-measure once
    // `document.fonts` signals readiness so the chosen size matches the
    // rendered glyphs. We **do not** refresh the font context here because
    // by this point the element may already carry our inline font-size; the
    // family/weight stay constant across the swap.
    const fontsApi =
      typeof document !== "undefined"
        ? (document as Document & { fonts?: FontFaceSet }).fonts
        : undefined;
    if (fontsApi) {
      fontsApi.ready
        .then(() => {
          if (disposed) return;
          const w = Math.round(node.clientWidth);
          if (w > 0) {
            // Force a recompute even if width is unchanged, since the glyph
            // metrics underlying measureText have shifted.
            lastWidth = -1;
            setMaxPx(w);
          }
        })
        .catch(() => {
          /* ignore — fonts API may reject in some embedded WebViews */
        });
    }

    measure();

    const ro = new ResizeObserver(measure);
    ro.observe(node);
    onCleanup(() => {
      disposed = true;
      ro.disconnect();
      if (rafId !== null) {
        if (typeof cancelAnimationFrame === "function") cancelAnimationFrame(rafId);
        else clearTimeout(rafId as unknown as number);
        rafId = null;
      }
    });
  });

  const fit = createMemo<FitResult>(() => {
    const width = maxPx();
    const name = fullText();
    return fitFontSize(name, width, fontCtx());
  });

  const text = createMemo(() => fit().text);
  const fontSize = createMemo(() => fit().fontSizePx);
  const tooltipDisabled = createMemo(
    () => fit().fontSizePx >= fontCtx().defaultFontSizePx,
  );

  return { setRef: setEl, text, fontSize, tooltipDisabled };
}
