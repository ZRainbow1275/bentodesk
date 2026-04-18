/**
 * IconPicker — Searchable grid over the full Lucide icon set plus custom uploads.
 *
 * Visible area only loads up to `VISIBLE_CAP` icons at a time (via IntersectionObserver
 * rootMargin buffer), so 1600+ icons never hit the DOM simultaneously.
 */
import {
  Component,
  createEffect,
  createMemo,
  createSignal,
  For,
  onCleanup,
  onMount,
  Show,
} from "solid-js";
import Fuse from "fuse.js";
import iconIndex from "../../generated/icon-index.json";
import LucideDynamic from "../Icons/LucideDynamic";
import { ZONE_ICON_NAMES } from "../Icons/ZoneIcons";
import ZoneIcon from "../Icons/ZoneIcon";
import {
  deleteCustomIcon,
  listCustomIcons,
  uploadCustomIcon,
  type CustomIcon,
} from "../../services/customIcons";
import { t } from "../../i18n";
import "./IconPicker.css";

interface IconEntry {
  name: string;
  tags: string[];
  category: string;
}

const LUCIDE_ENTRIES = iconIndex as IconEntry[];

const CATEGORIES = [
  "all",
  "builtin",
  "custom",
  "work",
  "creative",
  "dev",
  "media",
  "finance",
  "health",
  "files",
  "arrows",
  "system",
  "charts",
  "security",
  "general",
];

const VISIBLE_CAP = 200;

interface IconPickerProps {
  selected?: string;
  onSelect: (icon: string) => void;
  onClose?: () => void;
}

const IconPicker: Component<IconPickerProps> = (props) => {
  const [query, setQuery] = createSignal("");
  const [category, setCategory] = createSignal<string>("all");
  const [customIcons, setCustomIcons] = createSignal<CustomIcon[]>([]);
  const [uploadError, setUploadError] = createSignal<string | null>(null);

  const fuse = createMemo(
    () =>
      new Fuse(LUCIDE_ENTRIES, {
        keys: ["name", "tags"],
        threshold: 0.35,
        ignoreLocation: true,
        minMatchCharLength: 2,
      }),
  );

  const filtered = createMemo(() => {
    const q = query().trim().toLowerCase();
    const cat = category();

    if (cat === "builtin") {
      return ZONE_ICON_NAMES.filter((n) => !q || n.toLowerCase().includes(q)).map((name) => ({
        kind: "builtin" as const,
        name,
      }));
    }

    if (cat === "custom") {
      return customIcons()
        .filter((c) => !q || c.name.toLowerCase().includes(q))
        .map((c) => ({ kind: "custom" as const, icon: c }));
    }

    let pool: IconEntry[] = LUCIDE_ENTRIES;
    if (cat !== "all") {
      pool = LUCIDE_ENTRIES.filter((e) => e.category === cat);
    }

    let results: IconEntry[];
    if (!q) {
      results = pool;
    } else if (cat === "all") {
      results = fuse().search(q).map((r) => r.item);
    } else {
      results = pool.filter(
        (e) =>
          e.name.toLowerCase().includes(q) ||
          e.tags.some((t) => t.toLowerCase().includes(q)),
      );
    }

    return results.slice(0, VISIBLE_CAP).map((e) => ({
      kind: "lucide" as const,
      name: e.name,
    }));
  });

  onMount(() => {
    void listCustomIcons().then(setCustomIcons).catch(() => {});
  });

  async function handleFileUpload(ev: Event) {
    setUploadError(null);
    const input = ev.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    input.value = ""; // allow re-uploading same file
    try {
      const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
      if (!["svg", "png", "ico"].includes(ext)) {
        setUploadError(`Unsupported format: .${ext}`);
        return;
      }
      const bytes = new Uint8Array(await file.arrayBuffer());
      await uploadCustomIcon(ext, Array.from(bytes), file.name);
      setCustomIcons(await listCustomIcons());
      setCategory("custom");
    } catch (err) {
      setUploadError(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleDelete(uuid: string) {
    try {
      await deleteCustomIcon(uuid);
      setCustomIcons(await listCustomIcons());
    } catch (err) {
      console.warn("Failed to delete custom icon:", err);
    }
  }

  return (
    <div class="icon-picker">
      <div class="icon-picker__toolbar">
        <input
          class="icon-picker__search"
          type="search"
          placeholder={t("iconPickerSearch") || "Search icons…"}
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          autofocus
        />
        <label class="icon-picker__upload-btn">
          {t("iconPickerUpload") || "Upload"}
          <input
            type="file"
            accept=".svg,.png,.ico"
            onChange={handleFileUpload}
            style={{ display: "none" }}
          />
        </label>
      </div>

      <div class="icon-picker__tabs">
        <For each={CATEGORIES}>
          {(cat) => (
            <button
              class="icon-picker__tab"
              classList={{ "icon-picker__tab--active": category() === cat }}
              onClick={() => setCategory(cat)}
            >
              {cat}
            </button>
          )}
        </For>
      </div>

      <Show when={uploadError()}>
        <div class="icon-picker__error">{uploadError()}</div>
      </Show>

      <div class="icon-picker__grid">
        <For each={filtered()}>
          {(item) => (
            <IconCell item={item} selected={props.selected} onSelect={props.onSelect} onDelete={handleDelete} />
          )}
        </For>
        <Show when={filtered().length === 0}>
          <div class="icon-picker__empty">{t("iconPickerNoResults") || "No icons found"}</div>
        </Show>
      </div>

      <Show when={category() === "all" && filtered().length >= VISIBLE_CAP}>
        <div class="icon-picker__hint">
          {t("iconPickerRefine") || `Showing first ${VISIBLE_CAP}. Refine your search to see more.`}
        </div>
      </Show>
    </div>
  );
};

type GridItem =
  | { kind: "lucide"; name: string }
  | { kind: "builtin"; name: string }
  | { kind: "custom"; icon: CustomIcon };

interface IconCellProps {
  item: GridItem;
  selected?: string;
  onSelect: (icon: string) => void;
  onDelete: (uuid: string) => void;
}

const IconCell: Component<IconCellProps> = (props) => {
  let ref: HTMLButtonElement | undefined;
  const [visible, setVisible] = createSignal(false);

  onMount(() => {
    if (!ref || typeof IntersectionObserver === "undefined") {
      setVisible(true);
      return;
    }
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            setVisible(true);
            io.disconnect();
          }
        }
      },
      { rootMargin: "200px" },
    );
    io.observe(ref);
    onCleanup(() => io.disconnect());
  });

  const iconKey = () => {
    const it = props.item;
    if (it.kind === "lucide") return `lucide:${it.name}`;
    if (it.kind === "builtin") return it.name;
    return `custom:${it.icon.uuid}`;
  };

  const isSelected = () => props.selected === iconKey();

  const label = () => {
    const it = props.item;
    if (it.kind === "custom") return it.icon.name;
    return it.name;
  };

  return (
    <button
      ref={ref}
      class="icon-picker__cell"
      classList={{ "icon-picker__cell--selected": isSelected() }}
      title={label()}
      onClick={() => props.onSelect(iconKey())}
    >
      <Show when={visible()}>
        {(() => {
          const it = props.item;
          if (it.kind === "lucide") return <LucideDynamic name={it.name} size={24} />;
          if (it.kind === "builtin") return <ZoneIcon icon={it.name} size={24} />;
          return (
            <>
              <img src={it.icon.url} alt={it.icon.name} width={24} height={24} />
              <span
                class="icon-picker__cell-delete"
                onClick={(ev) => {
                  ev.stopPropagation();
                  props.onDelete(it.icon.uuid);
                }}
                title="Delete"
              >
                ×
              </span>
            </>
          );
        })()}
      </Show>
    </button>
  );
};

export default IconPicker;
