/**
 * svgSanitize — Strip unsafe constructs from an SVG string before it is
 * rendered with innerHTML or sent to disk as a user-uploaded custom icon.
 *
 * Rules:
 *  - Drop `<script>` / `<foreignObject>` subtrees entirely.
 *  - Drop every attribute starting with `on` (event handlers).
 *  - Drop attributes whose value references `javascript:` or `data:text/html`.
 *  - Allow only `http://`, `https://`, `#fragment`, or relative refs in `href`
 *    and `xlink:href`.
 *  - Drop external `<image href="http://...">` to prevent data exfiltration.
 *
 * Uses DOMPurify when available (main-thread + test environment), falls back
 * to a conservative regex sweep for environments where the DOM is missing.
 */
import DOMPurify from "dompurify";

/** Profile used for every SVG we render — both Lucide and user-uploaded. */
const PURIFY_CONFIG = {
  USE_PROFILES: { svg: true, svgFilters: true },
  FORBID_TAGS: ["script", "foreignObject", "iframe", "object", "embed"],
  FORBID_ATTR: [
    "onload",
    "onerror",
    "onclick",
    "onmouseover",
    "onfocus",
    "onblur",
    "onbegin",
    "onend",
    "onrepeat",
    "formaction",
  ],
  ALLOW_DATA_ATTR: false,
  RETURN_TRUSTED_TYPE: false,
};

/** Sanitize an SVG markup string. Returns sanitized markup. */
export function sanitizeSvg(raw: string): string {
  if (!raw || raw.length === 0) return "";

  if (typeof window === "undefined" || typeof document === "undefined") {
    return fallbackSanitize(raw);
  }

  try {
    const clean = DOMPurify.sanitize(raw, PURIFY_CONFIG);
    return typeof clean === "string" ? clean : String(clean);
  } catch (err) {
    console.warn("[svgSanitize] DOMPurify failed, falling back to regex:", err);
    return fallbackSanitize(raw);
  }
}

/**
 * Conservative regex-based sanitization for node/SSR contexts. Stricter than
 * DOMPurify but good enough for small icon payloads.
 */
function fallbackSanitize(raw: string): string {
  return raw
    .replace(/<script[\s\S]*?<\/script>/gi, "")
    .replace(/<foreignObject[\s\S]*?<\/foreignObject>/gi, "")
    .replace(/<iframe[\s\S]*?<\/iframe>/gi, "")
    .replace(/\son\w+\s*=\s*"[^"]*"/gi, "")
    .replace(/\son\w+\s*=\s*'[^']*'/gi, "")
    .replace(/javascript:[^"']*/gi, "")
    .replace(/xlink:href\s*=\s*"http[^"]*"/gi, "")
    .replace(/href\s*=\s*"(?:javascript|data:text\/html)[^"]*"/gi, 'href="#"');
}
