/**
 * ZoneEditor — Modal dialog for editing zone properties.
 * Fields: name (text input), icon (emoji grid picker), accent_color (color swatches),
 * grid_columns (slider).
 * Opened from ContextMenu "Edit Zone" action.
 * Escape or overlay click to cancel. Save commits changes via IPC.
 */
import {
  Component,
  Show,
  For,
  createSignal,
  createEffect,
  onMount,
  onCleanup,
} from "solid-js";
import { getEditingZoneId, closeZoneEditor } from "../../stores/ui";
import { getZoneById, updateZone } from "../../stores/zones";
import type { ZoneUpdate, CapsuleShape, CapsuleSize } from "../../types/zone";
import { t } from "../../i18n";
import ZoneIcon from "../Icons/ZoneIcon";
import { ZONE_ICON_NAMES } from "../Icons/ZoneIcons";
import "./ZoneEditor.css";

/** Capsule shape options with SVG preview paths */
const CAPSULE_SHAPES: { value: CapsuleShape; label: string }[] = [
  { value: "pill", label: "zoneEditorCapsuleShapePill" },
  { value: "rounded", label: "zoneEditorCapsuleShapeRounded" },
  { value: "circle", label: "zoneEditorCapsuleShapeCircle" },
  { value: "minimal", label: "zoneEditorCapsuleShapeMinimal" },
];

/** Capsule size options */
const CAPSULE_SIZES: { value: CapsuleSize; label: string }[] = [
  { value: "small", label: "zoneEditorCapsuleSizeSmall" },
  { value: "medium", label: "zoneEditorCapsuleSizeMedium" },
  { value: "large", label: "zoneEditorCapsuleSizeLarge" },
];

/** Predefined accent color palette */
const ACCENT_COLORS = [
  "#3b82f6", // Blue
  "#8b5cf6", // Purple
  "#22c55e", // Green
  "#f97316", // Orange
  "#ec4899", // Pink
  "#ef4444", // Red
  "#eab308", // Yellow
  "#06b6d4", // Cyan
  "#f43f5e", // Rose
  "#a855f7", // Violet
];


const ZoneEditor: Component = () => {
  const editingZoneId = () => getEditingZoneId();

  // Local form state
  const [name, setName] = createSignal("");
  const [icon, setIcon] = createSignal("");
  const [accentColor, setAccentColor] = createSignal<string | null>(null);
  const [gridColumns, setGridColumns] = createSignal(4);
  const [capsuleShape, setCapsuleShape] = createSignal<CapsuleShape>("pill");
  const [capsuleSize, setCapsuleSize] = createSignal<CapsuleSize>("medium");
  const [dirty, setDirty] = createSignal(false);

  // Sync form state when dialog opens
  createEffect(() => {
    const zoneId = editingZoneId();
    if (zoneId) {
      const zone = getZoneById(zoneId);
      if (zone) {
        setName(zone.name);
        setIcon(zone.icon);
        setAccentColor(zone.accent_color);
        setGridColumns(zone.grid_columns || 4);
        setCapsuleShape(zone.capsule_shape || "pill");
        setCapsuleSize(zone.capsule_size || "medium");
        setDirty(false);
      }
    }
  });

  // Escape to close
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape" && editingZoneId()) {
      closeZoneEditor();
    }
  };

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown);
  });

  const markDirty = () => setDirty(true);

  const handleSave = async () => {
    const zoneId = editingZoneId();
    if (!zoneId) return;

    const updates: ZoneUpdate = {
      name: name(),
      icon: icon(),
      accent_color: accentColor() ?? undefined,
      grid_columns: gridColumns(),
      capsule_shape: capsuleShape(),
      capsule_size: capsuleSize(),
    };

    await updateZone(zoneId, updates);
    setDirty(false);
    closeZoneEditor();
  };

  const handleCancel = () => {
    closeZoneEditor();
  };

  return (
    <Show when={editingZoneId()}>
      <div class="zone-editor-overlay" onClick={handleCancel}>
        <div
          class="zone-editor scale-in"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Header */}
          <div class="zone-editor__header">
            <h2 class="zone-editor__title">{t("zoneEditorTitle")}</h2>
            <button
              class="zone-editor__close"
              onClick={handleCancel}
              aria-label={t("zoneEditorCloseAriaLabel")}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>

          {/* Body */}
          <div class="zone-editor__body">
            {/* Zone Name */}
            <div class="zone-editor__field">
              <label class="zone-editor__label">{t("zoneEditorZoneName")}</label>
              <input
                class="zone-editor__input"
                type="text"
                value={name()}
                onInput={(e) => {
                  setName(e.currentTarget.value);
                  markDirty();
                }}
                placeholder={t("zoneEditorZoneNamePlaceholder")}
                maxLength={32}
              />
            </div>

            {/* Icon Picker — SVG icons only (no emoji) */}
            <div class="zone-editor__field">
              <label class="zone-editor__label">{t("zoneEditorIcon")}</label>
              <div class="zone-editor__icon-grid">
                <For each={ZONE_ICON_NAMES}>
                  {(name) => (
                    <button
                      class={`zone-editor__icon-btn ${icon() === name ? "zone-editor__icon-btn--selected" : ""}`}
                      onClick={() => {
                        setIcon(name);
                        markDirty();
                      }}
                      aria-label={`Select icon ${name}`}
                      title={name}
                    >
                      <ZoneIcon icon={name} size={18} />
                    </button>
                  )}
                </For>
              </div>
            </div>

            {/* Accent Color */}
            <div class="zone-editor__field">
              <label class="zone-editor__label">{t("zoneEditorAccentColor")}</label>
              <div class="zone-editor__color-row">
                {/* "None" option */}
                <button
                  class={`zone-editor__color-swatch zone-editor__color-swatch--none ${accentColor() === null ? "zone-editor__color-swatch--selected" : ""}`}
                  onClick={() => {
                    setAccentColor(null);
                    markDirty();
                  }}
                  aria-label="No accent color"
                  title={t("zoneEditorAccentColorNone")}
                />
                <For each={ACCENT_COLORS}>
                  {(color) => (
                    <button
                      class={`zone-editor__color-swatch ${accentColor() === color ? "zone-editor__color-swatch--selected" : ""}`}
                      style={{ background: color }}
                      onClick={() => {
                        setAccentColor(color);
                        markDirty();
                      }}
                      aria-label={`Select color ${color}`}
                    />
                  )}
                </For>
              </div>
            </div>

            {/* Grid Columns */}
            <div class="zone-editor__field">
              <div class="zone-editor__field-top">
                <label class="zone-editor__label">{t("zoneEditorGridColumns")}</label>
                <span class="zone-editor__value">{gridColumns()}</span>
              </div>
              <input
                class="zone-editor__slider"
                type="range"
                min={2}
                max={6}
                step={1}
                value={gridColumns()}
                onInput={(e) => {
                  setGridColumns(parseInt(e.currentTarget.value, 10));
                  markDirty();
                }}
              />
            </div>

            {/* Capsule Shape */}
            <div class="zone-editor__field">
              <label class="zone-editor__label">{t("zoneEditorCapsuleShape")}</label>
              <div class="zone-editor__shape-row">
                <For each={CAPSULE_SHAPES}>
                  {(shape) => (
                    <button
                      class={`zone-editor__shape-btn ${capsuleShape() === shape.value ? "zone-editor__shape-btn--selected" : ""}`}
                      onClick={() => {
                        setCapsuleShape(shape.value);
                        markDirty();
                      }}
                      aria-label={t(shape.label as Parameters<typeof t>[0])}
                      title={t(shape.label as Parameters<typeof t>[0])}
                    >
                      <svg class="zone-editor__shape-preview" viewBox="0 0 48 24" fill="none">
                        <Show when={shape.value === "pill"}>
                          <rect x="1" y="1" width="46" height="22" rx="11" stroke="currentColor" stroke-width="1.5" />
                        </Show>
                        <Show when={shape.value === "rounded"}>
                          <rect x="1" y="1" width="46" height="22" rx="6" stroke="currentColor" stroke-width="1.5" />
                        </Show>
                        <Show when={shape.value === "circle"}>
                          <circle cx="24" cy="12" r="10.5" stroke="currentColor" stroke-width="1.5" />
                        </Show>
                        <Show when={shape.value === "minimal"}>
                          <rect x="1" y="1" width="46" height="22" rx="4" stroke="currentColor" stroke-width="1" stroke-dasharray="3 2" />
                        </Show>
                      </svg>
                      <span class="zone-editor__shape-label">{t(shape.label as Parameters<typeof t>[0])}</span>
                    </button>
                  )}
                </For>
              </div>
            </div>

            {/* Capsule Size */}
            <div class="zone-editor__field">
              <label class="zone-editor__label">{t("zoneEditorCapsuleSize")}</label>
              <div class="zone-editor__size-toggle">
                <For each={CAPSULE_SIZES}>
                  {(size) => (
                    <button
                      class={`zone-editor__size-btn ${capsuleSize() === size.value ? "zone-editor__size-btn--selected" : ""}`}
                      onClick={() => {
                        setCapsuleSize(size.value);
                        markDirty();
                      }}
                      aria-label={t(size.label as Parameters<typeof t>[0])}
                    >
                      {t(size.label as Parameters<typeof t>[0])}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </div>

          {/* Footer */}
          <div class="zone-editor__footer">
            <button
              class="zone-editor__btn zone-editor__btn--secondary"
              onClick={handleCancel}
            >
              {t("zoneEditorBtnCancel")}
            </button>
            <button
              class="zone-editor__btn zone-editor__btn--primary"
              onClick={() => void handleSave()}
              disabled={!dirty() || name().trim().length === 0}
            >
              {t("zoneEditorBtnSave")}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default ZoneEditor;
