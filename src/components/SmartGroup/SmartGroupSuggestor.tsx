/**
 * SmartGroupSuggestor — Modal dialog for smart auto-grouping suggestions.
 *
 * Flow:
 * 1. On open, scans desktop files via ipc.scanDesktop()
 * 2. Sends file paths to ipc.suggestGroups() for backend analysis
 * 3. Displays suggestion cards with an expandable checkbox list of matching
 *    files. Hovering a card pulses the matching desktop icons via the
 *    ghost-layer highlight overlay (R4-C2).
 * 4. "Apply" sends only the currently-checked paths to the backend.
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
import {
  getSmartGroupZoneId,
  closeSmartGroupDialog,
} from "../../stores/ui";
import * as ipc from "../../services/ipc";
import { createZone, loadZones as loadZonesFromStore } from "../../stores/zones";
import { t } from "../../i18n";
import type { SuggestedGroup } from "../../types/zone";
import "./SmartGroupSuggestor.css";

type LoadState = "idle" | "scanning" | "analyzing" | "done" | "error";

const HIGHLIGHT_DURATION_MS = 3_000;

const SmartGroupSuggestor: Component = () => {
  const zoneId = () => getSmartGroupZoneId();

  const [loadState, setLoadState] = createSignal<LoadState>("idle");
  const [suggestions, setSuggestions] = createSignal<SuggestedGroup[]>([]);
  const [errorMsg, setErrorMsg] = createSignal("");
  const [applyingIndex, setApplyingIndex] = createSignal<number | null>(null);
  const [expandedIndex, setExpandedIndex] = createSignal<number | null>(null);
  /**
   * selected[suggestionIndex] = Set of file paths the user currently has
   * checked. Uninitialised indices default to "all matching_files checked".
   */
  const [selectedMap, setSelectedMap] = createSignal<
    Record<number, Set<string>>
  >({});

  const ensureSelection = (index: number, suggestion: SuggestedGroup) => {
    const map = selectedMap();
    if (map[index]) return map[index];
    const next = new Set(suggestion.matching_files);
    setSelectedMap({ ...map, [index]: next });
    return next;
  };

  const getSelected = (
    index: number,
    suggestion: SuggestedGroup
  ): Set<string> => selectedMap()[index] ?? new Set(suggestion.matching_files);

  const toggleSelection = (
    index: number,
    suggestion: SuggestedGroup,
    path: string
  ) => {
    const current = new Set(ensureSelection(index, suggestion));
    if (current.has(path)) {
      current.delete(path);
    } else {
      current.add(path);
    }
    setSelectedMap({ ...selectedMap(), [index]: current });
  };

  const selectAll = (index: number, suggestion: SuggestedGroup) => {
    setSelectedMap({
      ...selectedMap(),
      [index]: new Set(suggestion.matching_files),
    });
  };

  const selectNone = (index: number) => {
    setSelectedMap({ ...selectedMap(), [index]: new Set() });
  };

  const toggleExpanded = (index: number) => {
    setExpandedIndex(expandedIndex() === index ? null : index);
  };

  // Trigger scan + suggest when dialog opens
  createEffect(() => {
    const id = zoneId();
    if (id) {
      setSuggestions([]);
      setErrorMsg("");
      setApplyingIndex(null);
      setExpandedIndex(null);
      setSelectedMap({});
      void runAnalysis();
    }
  });

  async function runAnalysis(): Promise<void> {
    try {
      setLoadState("scanning");
      const files = await ipc.scanDesktop();

      if (files.length === 0) {
        setSuggestions([]);
        setLoadState("done");
        return;
      }

      setLoadState("analyzing");
      const paths = files.map((f) => f.path);
      const groups = await ipc.suggestGroups(paths);

      // Sort by confidence descending
      groups.sort((a, b) => b.confidence - a.confidence);
      setSuggestions(groups);
      setLoadState("done");
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setErrorMsg(message);
      setLoadState("error");
    }
  }

  const handleHoverEnter = (index: number, suggestion: SuggestedGroup) => {
    const selected = getSelected(index, suggestion);
    // Preview every matching file; the backend highlights only those with a
    // known icon position. If the user has trimmed the selection, prefer the
    // trimmed set so the preview tracks their intent.
    const paths =
      selected.size > 0
        ? Array.from(selected)
        : suggestion.matching_files;
    void ipc.highlightDesktopFiles(paths, HIGHLIGHT_DURATION_MS).catch((err) => {
      console.warn("highlightDesktopFiles failed", err);
    });
  };

  const handleHoverLeave = () => {
    void ipc.clearDesktopHighlights().catch((err) => {
      console.warn("clearDesktopHighlights failed", err);
    });
  };

  const handleApply = async (index: number, suggestion: SuggestedGroup) => {
    const id = zoneId();
    if (!id) return;

    const selected = getSelected(index, suggestion);
    if (selected.size === 0) return;

    setApplyingIndex(index);
    try {
      await ipc.applyAutoGroup(id, suggestion.rule, Array.from(selected));
      await ipc.clearDesktopHighlights();
      await loadZonesFromStore();
      closeSmartGroupDialog();
    } catch (err) {
      console.error("Failed to apply auto group:", err);
      setApplyingIndex(null);
    }
  };

  const handleCreateAsNewZone = async (
    index: number,
    suggestion: SuggestedGroup
  ) => {
    const selected = getSelected(index, suggestion);
    if (selected.size === 0) return;

    setApplyingIndex(index);
    try {
      const newZone = await createZone(
        suggestion.name,
        suggestion.icon,
        { x_percent: 30 + Math.random() * 40, y_percent: 20 + Math.random() * 40 },
        { w_percent: 25, h_percent: 45 }
      );
      if (newZone) {
        await ipc.applyAutoGroup(
          newZone.id,
          suggestion.rule,
          Array.from(selected)
        );
        await ipc.clearDesktopHighlights();
        await loadZonesFromStore();
      }
      closeSmartGroupDialog();
    } catch (err) {
      console.error("Failed to create zone from suggestion:", err);
      setApplyingIndex(null);
    }
  };

  // Escape to close
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape" && zoneId()) {
      void ipc.clearDesktopHighlights().catch((err) => {
        console.warn("Failed to clear desktop highlights on Escape:", err);
      });
      closeSmartGroupDialog();
    }
  };

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown);
    void ipc.clearDesktopHighlights().catch((err) => {
      console.warn("Failed to clear desktop highlights on unmount:", err);
    });
  });

  const confidenceLabel = (score: number): string => {
    if (score >= 0.8) return t("smartGroupConfidenceHigh");
    if (score >= 0.5) return t("smartGroupConfidenceMedium");
    return t("smartGroupConfidenceLow");
  };

  const confidenceClass = (score: number): string => {
    if (score >= 0.8) return "smart-group__confidence--high";
    if (score >= 0.5) return "smart-group__confidence--medium";
    return "smart-group__confidence--low";
  };

  const selectedCountLabel = (selected: number, total: number): string => {
    return t("smartGroupPreviewSelectedCount")
      .replace("{{selected}}", String(selected))
      .replace("{{total}}", String(total));
  };

  const fileBasename = (p: string): string => {
    const idx = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
    return idx >= 0 ? p.slice(idx + 1) : p;
  };

  return (
    <Show when={zoneId()}>
      <div
        class="smart-group-overlay"
        onClick={() => closeSmartGroupDialog()}
      >
        <div
          class="smart-group-dialog scale-in"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Header */}
          <div class="smart-group__header">
            <h2 class="smart-group__title">{t("smartGroupTitle")}</h2>
            <button
              class="smart-group__close"
              onClick={() => closeSmartGroupDialog()}
              aria-label={t("smartGroupCloseAriaLabel")}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>

          {/* Body */}
          <div class="smart-group__body">
            <Show when={loadState() === "scanning"}>
              <div class="smart-group__status pulse">
                {t("smartGroupScanning")}
              </div>
            </Show>

            <Show when={loadState() === "analyzing"}>
              <div class="smart-group__status pulse">
                {t("smartGroupAnalyzing")}
              </div>
            </Show>

            <Show when={loadState() === "error"}>
              <div class="smart-group__error">
                {t("smartGroupError")}{errorMsg()}
              </div>
            </Show>

            <Show when={loadState() === "done" && suggestions().length === 0}>
              <div class="smart-group__empty">
                {t("smartGroupEmpty")}
              </div>
            </Show>

            <Show when={loadState() === "done" && suggestions().length > 0}>
              <div class="smart-group__list">
                <For each={suggestions()}>
                  {(suggestion, index) => {
                    const idx = index();
                    return (
                      <div
                        class="smart-group__item"
                        onMouseEnter={() => handleHoverEnter(idx, suggestion)}
                        onMouseLeave={handleHoverLeave}
                      >
                        <div class="smart-group__item-row">
                          <div class="smart-group__item-icon">
                            {suggestion.icon}
                          </div>
                          <div class="smart-group__item-info">
                            <span class="smart-group__item-name">
                              {suggestion.name}
                            </span>
                            <span class="smart-group__item-meta">
                              {suggestion.matching_files.length} {t("smartGroupFiles")}
                              {" \u{2022} "}
                              {suggestion.rule.rule_type === "Extension" &&
                                suggestion.rule.extensions
                                ? suggestion.rule.extensions.join(", ")
                                : suggestion.rule.rule_type}
                            </span>
                          </div>
                          <span
                            class={`smart-group__confidence ${confidenceClass(suggestion.confidence)}`}
                          >
                            {confidenceLabel(suggestion.confidence)}
                            {" "}
                            ({Math.round(suggestion.confidence * 100)}%)
                          </span>
                          <div class="smart-group__item-actions">
                            <button
                              class="smart-group__preview-btn"
                              onClick={() => toggleExpanded(idx)}
                              title={
                                expandedIndex() === idx
                                  ? t("smartGroupPreviewHide")
                                  : t("smartGroupPreviewToggle")
                              }
                            >
                              {expandedIndex() === idx
                                ? t("smartGroupPreviewHide")
                                : t("smartGroupPreviewToggle")}
                            </button>
                            <button
                              class="smart-group__apply-btn"
                              onClick={() =>
                                void handleApply(idx, suggestion)
                              }
                              disabled={
                                applyingIndex() !== null ||
                                getSelected(idx, suggestion).size === 0
                              }
                              title={t("smartGroupApplyToZone")}
                            >
                              {applyingIndex() === idx
                                ? t("smartGroupApplying")
                                : t("smartGroupApply")}
                            </button>
                            <button
                              class="smart-group__new-zone-btn"
                              onClick={() =>
                                void handleCreateAsNewZone(idx, suggestion)
                              }
                              disabled={
                                applyingIndex() !== null ||
                                getSelected(idx, suggestion).size === 0
                              }
                              title={t("smartGroupCreateAsNewZone")}
                            >
                              {t("smartGroupNewZone")}
                            </button>
                          </div>
                        </div>

                        <Show when={expandedIndex() === idx}>
                          <div class="smart-group__preview">
                            <div class="smart-group__preview-toolbar">
                              <span class="smart-group__preview-count">
                                {selectedCountLabel(
                                  getSelected(idx, suggestion).size,
                                  suggestion.matching_files.length
                                )}
                              </span>
                              <div class="smart-group__preview-toolbar-actions">
                                <button
                                  class="smart-group__preview-action"
                                  onClick={() => selectAll(idx, suggestion)}
                                >
                                  {t("smartGroupPreviewAll")}
                                </button>
                                <button
                                  class="smart-group__preview-action"
                                  onClick={() => selectNone(idx)}
                                >
                                  {t("smartGroupPreviewNone")}
                                </button>
                              </div>
                            </div>
                            <Show
                              when={suggestion.matching_files.length > 0}
                              fallback={
                                <div class="smart-group__preview-empty">
                                  {t("smartGroupPreviewEmpty")}
                                </div>
                              }
                            >
                              <ul class="smart-group__preview-list">
                                <For each={suggestion.matching_files}>
                                  {(filePath) => (
                                    <li class="smart-group__preview-item">
                                      <label class="smart-group__preview-label">
                                        <input
                                          type="checkbox"
                                          class="smart-group__preview-checkbox"
                                          checked={getSelected(
                                            idx,
                                            suggestion
                                          ).has(filePath)}
                                          onChange={() =>
                                            toggleSelection(
                                              idx,
                                              suggestion,
                                              filePath
                                            )
                                          }
                                        />
                                        <span
                                          class="smart-group__preview-name"
                                          title={filePath}
                                        >
                                          {fileBasename(filePath)}
                                        </span>
                                      </label>
                                    </li>
                                  )}
                                </For>
                              </ul>
                            </Show>
                          </div>
                        </Show>
                      </div>
                    );
                  }}
                </For>
              </div>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default SmartGroupSuggestor;
