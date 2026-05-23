/**
 * LucideDynamic — Render a Lucide icon by name via on-demand `import('lucide-static/icons/{name}.svg?raw')`.
 *
 * Icons are cached in memory after first load so subsequent uses of the same name
 * are synchronous. The loaded SVG markup is sanitized with DOMPurify before
 * being inserted with `innerHTML` (defense in depth — lucide-static is trusted
 * but user-uploaded SVG flows through the same renderer).
 */
import { Component, createResource, Show } from "solid-js";
import { sanitizeSvg } from "../../services/svgSanitize";
import "./LucideDynamic.css";

const cache = new Map<string, string>();
const inflight = new Map<string, Promise<string>>();

/**
 * Load a Lucide SVG by name. Results are cached indefinitely.
 * Returns the sanitized raw SVG string.
 */
export async function loadLucideSvg(name: string): Promise<string> {
  const cached = cache.get(name);
  if (cached !== undefined) return cached;

  const pending = inflight.get(name);
  if (pending) return pending;

  const promise = (async () => {
    try {
      // Vite dynamic import with ?raw suffix yields string content.
      const mod = await import(
        /* @vite-ignore */ `lucide-static/icons/${name}.svg?raw`
      );
      const raw: string = typeof mod === "string" ? mod : mod.default;
      const safe = sanitizeSvg(raw);
      cache.set(name, safe);
      return safe;
    } catch (err) {
      console.warn(`[LucideDynamic] Failed to load '${name}':`, err);
      cache.set(name, "");
      return "";
    } finally {
      inflight.delete(name);
    }
  })();

  inflight.set(name, promise);
  return promise;
}

/** Peek the cache without triggering a load. Returns `undefined` on miss. */
export function peekLucideSvg(name: string): string | undefined {
  return cache.get(name);
}

interface LucideDynamicProps {
  name: string;
  size?: number;
  class?: string;
}

const LucideDynamic: Component<LucideDynamicProps> = (props) => {
  const [svg] = createResource(
    () => props.name,
    (n) => loadLucideSvg(n),
  );
  const size = () => props.size ?? 20;

  return (
    <span
      class={`lucide-dynamic ${props.class ?? ""}`.trim()}
      style={{ "--lucide-dynamic-size": `${size()}px` }}
    >
      <Show when={svg() && svg()!.length > 0}>
        <span
          class="lucide-dynamic__svg"
          // eslint-disable-next-line solid/no-innerhtml
          innerHTML={svg()}
        />
      </Show>
    </span>
  );
};

export default LucideDynamic;
