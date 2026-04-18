/**
 * build-icon-index — Scan `node_modules/lucide-static/icons/*.svg` and produce
 * `src/generated/icon-index.json` — a compact JSON array consumed at runtime by
 * the IconPicker for fuzzy search.
 *
 * Run via `pnpm build:icon-index` (invoked automatically by `pnpm build`).
 */
import { readdirSync, readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { resolve, dirname, basename } from "node:path";
import { fileURLToPath } from "node:url";

interface IconEntry {
  name: string;
  tags: string[];
  category: string;
}

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "..");
const ICON_DIR = resolve(ROOT, "node_modules/lucide-static/icons");
const OUT_FILE = resolve(ROOT, "src/generated/icon-index.json");

const CATEGORY_PATTERNS: [RegExp, string][] = [
  [/^(briefcase|building|office|meeting|target|calendar)/i, "work"],
  [/^(palette|paint|brush|pen|pencil|draw|sparkles)/i, "creative"],
  [/^(code|terminal|git|github|branch|cpu|database|server|bug)/i, "dev"],
  [/^(music|headphones|video|film|camera|image|play|pause)/i, "media"],
  [/^(dollar|euro|yen|banknote|wallet|credit|coins|piggy)/i, "finance"],
  [/^(heart|activity|pulse|stethoscope|pill|thermometer|droplet)/i, "health"],
  [/^(globe|map|compass|plane|train|car|bike)/i, "travel"],
  [/^(home|house|sofa|bed|door|lamp)/i, "home"],
  [/^(cloud|sun|moon|star|snowflake|umbrella|wind|zap)/i, "weather"],
  [/^(arrow|chevron|move|rotate)/i, "arrows"],
  [/^(folder|file|archive|hard-drive|download|upload|save|trash)/i, "files"],
  [/^(user|users|person|baby|smile)/i, "people"],
  [/^(settings|sliders|cog|wrench|hammer|tool)/i, "system"],
  [/^(chart|bar|pie|line|trending|graph|axis)/i, "charts"],
  [/^(book|bookmark|library|newspaper)/i, "reading"],
  [/^(shield|lock|key|eye|fingerprint)/i, "security"],
  [/^(gamepad|dice|puzzle|flag|trophy)/i, "games"],
  [/^(shopping|cart|tag|gift|store|package)/i, "commerce"],
];

function categorize(name: string): string {
  for (const [pattern, cat] of CATEGORY_PATTERNS) {
    if (pattern.test(name)) return cat;
  }
  return "general";
}

function extractTags(name: string, svgContent: string): string[] {
  // name segments
  const segments = name.split("-").filter((s) => s.length > 1);
  const tags = new Set<string>(segments);

  // pull <title> if present
  const titleMatch = svgContent.match(/<title>([^<]+)<\/title>/i);
  if (titleMatch) {
    for (const word of titleMatch[1].toLowerCase().split(/\s+/)) {
      if (word.length > 1) tags.add(word);
    }
  }

  return Array.from(tags);
}

function main() {
  if (!existsSync(ICON_DIR)) {
    console.warn(
      `[build-icon-index] lucide-static not found at ${ICON_DIR}. ` +
        `Writing empty index. Run 'pnpm install' first.`,
    );
    const outDir = dirname(OUT_FILE);
    if (!existsSync(outDir)) mkdirSync(outDir, { recursive: true });
    writeFileSync(OUT_FILE, JSON.stringify([]));
    return;
  }

  const files = readdirSync(ICON_DIR).filter((f) => f.endsWith(".svg"));
  const entries: IconEntry[] = [];
  for (const file of files) {
    const name = basename(file, ".svg");
    // Lightweight tag extraction — we don't read every SVG to keep this fast.
    // Only read when the filename is short (less info to categorize from).
    let tags: string[];
    if (name.length < 5) {
      const svgContent = readFileSync(resolve(ICON_DIR, file), "utf8");
      tags = extractTags(name, svgContent);
    } else {
      tags = extractTags(name, "");
    }
    entries.push({
      name,
      tags,
      category: categorize(name),
    });
  }

  entries.sort((a, b) => a.name.localeCompare(b.name));

  const outDir = dirname(OUT_FILE);
  if (!existsSync(outDir)) mkdirSync(outDir, { recursive: true });
  writeFileSync(OUT_FILE, JSON.stringify(entries));
  console.log(
    `[build-icon-index] Wrote ${entries.length} icons to ${OUT_FILE} (` +
      `${Math.round(JSON.stringify(entries).length / 1024)} KB)`,
  );
}

main();
