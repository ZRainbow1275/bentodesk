/**
 * SmartGroupSuggestor — Modal dialog for smart auto-grouping suggestions.
 *
 * Flow:
 * 1. On open, scans desktop files via ipc.scanDesktop()
 * 2. Sends file paths to ipc.suggestGroups() for backend analysis
 * 3. Displays a list of suggested groups with name, icon, confidence, file count
 * 4. "Apply" button on each suggestion calls ipc.applyAutoGroup() to add matching
 *    files to the target zone and set its auto_group rule
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

const SmartGroupSuggestor: Component = () => {
  const zoneId = () => getSmartGroupZoneId();

  const [loadState, setLoadState] = createSignal<LoadState>("idle");
  const [suggestions, setSuggestions] = createSignal<SuggestedGroup[]>([]);
  const [errorMsg, setErrorMsg] = createSignal("");
  const [applyingIndex, setApplyingIndex] = createSignal<number | null>(null);

  // Trigger scan + suggest when dialog opens
  createEffect(() => {
    const id = zoneId();
    if (id) {
      setSuggestions([]);
      setErrorMsg("");
      setApplyingIndex(null);
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

  const handleApply = async (index: number, suggestion: SuggestedGroup) => {
    const id = zoneId();
    if (!id) return;

    setApplyingIndex(index);
    try {
      await ipc.applyAutoGroup(id, suggestion.rule);
      // Reload zones so the frontend store reflects the newly added items
      // (items were hidden from desktop and added to the zone by the backend).
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
    setApplyingIndex(index);
    try {
      // Create a new zone with the suggestion's name and icon at a default position
      const newZone = await createZone(
        suggestion.name,
        suggestion.icon,
        { x_percent: 30 + Math.random() * 40, y_percent: 20 + Math.random() * 40 },
        { w_percent: 25, h_percent: 45 }
      );
      if (newZone) {
        await ipc.applyAutoGroup(newZone.id, suggestion.rule);
        // Reload zones so the frontend store reflects all newly added items
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
      closeSmartGroupDialog();
    }
  };

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown);
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
            {/* Loading states */}
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

            {/* Suggestion list */}
            <Show when={loadState() === "done" && suggestions().length > 0}>
              <div class="smart-group__list">
                <For each={suggestions()}>
                  {(suggestion, index) => (
                    <div class="smart-group__item">
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
                          class="smart-group__apply-btn"
                          onClick={() =>
                            void handleApply(index(), suggestion)
                          }
                          disabled={applyingIndex() !== null}
                          title={t("smartGroupApplyToZone")}
                        >
                          {applyingIndex() === index()
                            ? t("smartGroupApplying")
                            : t("smartGroupApply")}
                        </button>
                        <button
                          class="smart-group__new-zone-btn"
                          onClick={() =>
                            void handleCreateAsNewZone(index(), suggestion)
                          }
                          disabled={applyingIndex() !== null}
                          title={t("smartGroupCreateAsNewZone")}
                        >
                          {t("smartGroupNewZone")}
                        </button>
                      </div>
                    </div>
                  )}
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
