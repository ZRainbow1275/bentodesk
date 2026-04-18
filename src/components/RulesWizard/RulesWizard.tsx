/**
 * RulesWizard — Outlook-style rules manager.
 *
 * Lists existing rules + a form for creating / editing. Preview panel shows
 * the matching files before the user applies. On "Run now", `run_rule_now`
 * is invoked which records a Time-machine checkpoint for Ctrl+Z undo.
 */
import {
  Component,
  createSignal,
  createEffect,
  For,
  onMount,
  Show,
  batch,
} from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import ZoneIcon from "../Icons/ZoneIcon";
import ConditionBuilder from "./ConditionBuilder";
import ActionBuilder from "./ActionBuilder";
import type { BentoZone } from "../../types/zone";
import type {
  Action,
  ConditionGroup,
  ExecutionReport,
  Rule,
  RunMode,
} from "../../services/rules";
import { t } from "../../i18n";
import "./RulesWizard.css";

interface RulesWizardProps {
  open: boolean;
  onClose: () => void;
}

function emptyRule(): Rule {
  return {
    id: "",
    name: "",
    enabled: true,
    conditions: { kind: "All", value: [] },
    actions: [],
    run_mode: { type: "OnDemand", value: null } as unknown as RunMode,
    last_run: null,
    run_count: 0,
  };
}

const RulesWizard: Component<RulesWizardProps> = (props) => {
  const [rules, setRules] = createSignal<Rule[]>([]);
  const [zones, setZones] = createSignal<BentoZone[]>([]);
  const [editing, setEditing] = createSignal<Rule | null>(null);
  const [preview, setPreview] = createSignal<string[]>([]);
  const [lastReport, setLastReport] = createSignal<ExecutionReport | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  async function refresh() {
    try {
      const [rs, zs] = await Promise.all([
        invoke<Rule[]>("list_rules"),
        invoke<BentoZone[]>("list_zones"),
      ]);
      batch(() => {
        setRules(rs);
        setZones(zs);
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  onMount(() => {
    void refresh();
  });

  createEffect(() => {
    if (props.open) void refresh();
  });

  async function handleSave() {
    const rule = editing();
    if (!rule) return;
    if (!rule.name.trim()) {
      setError("Rule name is required");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      if (rule.id) {
        await invoke("update_rule", { id: rule.id, rule });
      } else {
        await invoke("create_rule", { rule });
      }
      setEditing(null);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleDelete(id: string) {
    try {
      await invoke("delete_rule", { id });
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function handlePreview() {
    const rule = editing();
    if (!rule) return;
    setBusy(true);
    try {
      const hits = await invoke<string[]>("preview_rule_hits", { rule });
      setPreview(hits);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleRunNow(id: string) {
    setBusy(true);
    setError(null);
    try {
      const report = await invoke<ExecutionReport>("run_rule_now", { id });
      setLastReport(report);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Show when={props.open}>
      <div class="rules-wizard__backdrop" onClick={props.onClose}>
        <div class="rules-wizard" onClick={(e) => e.stopPropagation()}>
          <div class="rules-wizard__header">
            <span class="rules-wizard__title">
              <ZoneIcon icon="lightning" size={18} />
              {t("rulesWizardTitle") || "Rules"}
            </span>
            <button class="rules-wizard__close" onClick={props.onClose} aria-label="Close">
              ×
            </button>
          </div>

          <Show when={error()}>
            <div class="rules-wizard__error">{error()}</div>
          </Show>

          <Show when={lastReport()}>
            <div class="rules-wizard__report">
              {t("rulesWizardReport") || "Last run"}:{" "}
              {lastReport()!.matched} {t("rulesWizardMatched") || "matched"},{" "}
              {lastReport()!.actions_taken.join(" · ")}
              <Show when={lastReport()!.errors.length > 0}>
                <div class="rules-wizard__report-errors">
                  {lastReport()!.errors.join("; ")}
                </div>
              </Show>
            </div>
          </Show>

          <div class="rules-wizard__body">
            <Show when={!editing()}>
              <button
                class="rules-wizard__add-btn"
                onClick={() => setEditing(emptyRule())}
              >
                + {t("rulesWizardNewRule") || "New rule"}
              </button>
              <div class="rules-wizard__list">
                <Show
                  when={rules().length > 0}
                  fallback={
                    <div class="rules-wizard__empty">
                      {t("rulesWizardEmpty") || "No rules yet. Create one above."}
                    </div>
                  }
                >
                  <For each={rules()}>
                    {(rule) => (
                      <div class="rules-wizard__row">
                        <div class="rules-wizard__row-info">
                          <span class="rules-wizard__row-name">{rule.name}</span>
                          <span class="rules-wizard__row-meta">
                            {rule.enabled ? "" : "[disabled] "}
                            {rule.actions.length}{" "}
                            {t("rulesWizardActions") || "actions"} · {rule.run_count}{" "}
                            {t("rulesWizardRuns") || "runs"}
                          </span>
                        </div>
                        <div class="rules-wizard__row-actions">
                          <button
                            class="rules-wizard__btn"
                            onClick={() => handleRunNow(rule.id)}
                            disabled={busy()}
                          >
                            {t("rulesWizardRunNow") || "Run now"}
                          </button>
                          <button
                            class="rules-wizard__btn"
                            onClick={() => setEditing(structuredClone(rule))}
                          >
                            {t("rulesWizardEdit") || "Edit"}
                          </button>
                          <button
                            class="rules-wizard__btn rules-wizard__btn--danger"
                            onClick={() => handleDelete(rule.id)}
                          >
                            {"\u{1F5D1}"}
                          </button>
                        </div>
                      </div>
                    )}
                  </For>
                </Show>
              </div>
            </Show>

            <Show when={editing()}>
              {(rule) => (
                <div class="rules-wizard__editor">
                  <label class="rules-wizard__label">
                    {t("rulesWizardName") || "Name"}
                    <input
                      class="rules-wizard__input"
                      value={rule().name}
                      onInput={(e) =>
                        setEditing({ ...rule(), name: e.currentTarget.value })
                      }
                    />
                  </label>

                  <label class="rules-wizard__label rules-wizard__label--checkbox">
                    <input
                      type="checkbox"
                      checked={rule().enabled}
                      onChange={(e) =>
                        setEditing({ ...rule(), enabled: e.currentTarget.checked })
                      }
                    />
                    {t("rulesWizardEnabled") || "Enabled"}
                  </label>

                  <div class="rules-wizard__section">
                    <div class="rules-wizard__section-title">
                      {t("rulesWizardConditions") || "Conditions"}
                    </div>
                    <ConditionBuilder
                      group={rule().conditions}
                      zones={zones()}
                      onChange={(g: ConditionGroup) =>
                        setEditing({ ...rule(), conditions: g })
                      }
                    />
                  </div>

                  <div class="rules-wizard__section">
                    <div class="rules-wizard__section-title">
                      {t("rulesWizardActions") || "Actions"}
                    </div>
                    <ActionBuilder
                      actions={rule().actions}
                      zones={zones()}
                      onChange={(a: Action[]) => setEditing({ ...rule(), actions: a })}
                    />
                  </div>

                  <div class="rules-wizard__section">
                    <div class="rules-wizard__section-title">
                      {t("rulesWizardSchedule") || "Schedule"}
                    </div>
                    <select
                      class="rules-wizard__input"
                      value={rule().run_mode.type}
                      onChange={(e) => {
                        const kind = e.currentTarget.value;
                        const mode: RunMode =
                          kind === "Interval"
                            ? ({ type: "Interval", value: { minutes: 60 } } as RunMode)
                            : kind === "OnFileChange"
                            ? ({ type: "OnFileChange", value: null } as unknown as RunMode)
                            : ({ type: "OnDemand", value: null } as unknown as RunMode);
                        setEditing({ ...rule(), run_mode: mode });
                      }}
                    >
                      <option value="OnDemand">{t("rulesWizardRunOnDemand") || "On demand"}</option>
                      <option value="OnFileChange">
                        {t("rulesWizardRunOnChange") || "On file change"}
                      </option>
                      <option value="Interval">
                        {t("rulesWizardRunInterval") || "Every N minutes"}
                      </option>
                    </select>
                    <Show when={rule().run_mode.type === "Interval"}>
                      <input
                        class="rules-wizard__input"
                        type="number"
                        min="5"
                        value={(rule().run_mode as { value: { minutes: number } }).value.minutes}
                        onInput={(e) => {
                          const minutes = parseInt(e.currentTarget.value, 10) || 60;
                          setEditing({
                            ...rule(),
                            run_mode: {
                              type: "Interval",
                              value: { minutes },
                            } as RunMode,
                          });
                        }}
                      />
                    </Show>
                  </div>

                  <div class="rules-wizard__buttons">
                    <button
                      class="rules-wizard__btn"
                      onClick={handlePreview}
                      disabled={busy()}
                    >
                      {t("rulesWizardPreview") || "Preview hits"}
                    </button>
                    <button
                      class="rules-wizard__btn rules-wizard__btn--primary"
                      onClick={handleSave}
                      disabled={busy()}
                    >
                      {t("rulesWizardSave") || "Save"}
                    </button>
                    <button
                      class="rules-wizard__btn"
                      onClick={() => {
                        setEditing(null);
                        setPreview([]);
                      }}
                    >
                      {t("rulesWizardCancel") || "Cancel"}
                    </button>
                  </div>

                  <Show when={preview().length > 0}>
                    <div class="rules-wizard__preview">
                      <div class="rules-wizard__preview-title">
                        {preview().length} {t("rulesWizardPreviewHits") || "files would match"}
                      </div>
                      <ul>
                        <For each={preview().slice(0, 30)}>
                          {(p) => <li>{p}</li>}
                        </For>
                      </ul>
                    </div>
                  </Show>
                </div>
              )}
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default RulesWizard;
