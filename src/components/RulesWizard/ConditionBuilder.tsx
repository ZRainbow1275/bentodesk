/**
 * ConditionBuilder — MVP flat condition list editor.
 *
 * Renders the condition group as a list of leaves with Add / Remove buttons.
 * Deeper nesting (group within group) is left to v1.3 — this MVP covers
 * ~95% of realistic rules.
 */
import { Component, For, Show } from "solid-js";
import type { Condition, ConditionGroup, ConditionNode } from "../../services/rules";
import type { BentoZone } from "../../types/zone";
import { t } from "../../i18n";

type ConditionType =
  | "ExtensionIn"
  | "NameMatchesRegex"
  | "CreatedBefore"
  | "ModifiedBefore"
  | "SizeGreaterThan"
  | "InZone"
  | "OnDesktop";

const CONDITION_TYPES: ConditionType[] = [
  "ExtensionIn",
  "NameMatchesRegex",
  "CreatedBefore",
  "ModifiedBefore",
  "SizeGreaterThan",
  "InZone",
  "OnDesktop",
];

function defaultCondition(kind: ConditionType): Condition {
  switch (kind) {
    case "ExtensionIn":
      return { type: "ExtensionIn", value: ["tmp"] };
    case "NameMatchesRegex":
      return { type: "NameMatchesRegex", value: ".*" };
    case "CreatedBefore":
      return { type: "CreatedBefore", value: { days_ago: 7 } };
    case "ModifiedBefore":
      return { type: "ModifiedBefore", value: { days_ago: 30 } };
    case "SizeGreaterThan":
      return { type: "SizeGreaterThan", value: 1_048_576 };
    case "InZone":
      return { type: "InZone", value: "" };
    case "OnDesktop":
      return { type: "OnDesktop", value: null } as unknown as Condition;
  }
}

interface ConditionBuilderProps {
  group: ConditionGroup;
  zones: BentoZone[];
  onChange: (g: ConditionGroup) => void;
}

const ConditionBuilder: Component<ConditionBuilderProps> = (props) => {
  const nodes = (): ConditionNode[] => {
    const g = props.group;
    if (g.kind === "All" || g.kind === "Any") return g.value;
    return [];
  };

  const currentKind = (): "All" | "Any" => {
    const k = props.group.kind;
    return k === "Any" ? "Any" : "All";
  };

  const emit = (kind: "All" | "Any", value: ConditionNode[]) => {
    props.onChange(
      kind === "Any" ? { kind: "Any", value } : { kind: "All", value }
    );
  };

  const setKind = (kind: "All" | "Any") => emit(kind, nodes());

  const addLeaf = () => {
    const next: ConditionNode = defaultCondition("ExtensionIn") as Condition;
    emit(currentKind(), [...nodes(), next]);
  };

  const replaceAt = (idx: number, cond: Condition) => {
    const list = [...nodes()];
    list[idx] = cond as ConditionNode;
    emit(currentKind(), list);
  };

  const removeAt = (idx: number) => {
    emit(currentKind(), nodes().filter((_, i) => i !== idx));
  };

  return (
    <div class="condition-builder">
      <div class="condition-builder__mode">
        <label>
          <input
            type="radio"
            name="condition-mode"
            checked={props.group.kind === "All"}
            onChange={() => setKind("All")}
          />
          {t("rulesWizardAll") || "Match ALL"}
        </label>
        <label>
          <input
            type="radio"
            name="condition-mode"
            checked={props.group.kind === "Any"}
            onChange={() => setKind("Any")}
          />
          {t("rulesWizardAny") || "Match ANY"}
        </label>
      </div>
      <For each={nodes()}>
        {(node, idx) => (
          <Show when={isLeaf(node)}>
            <ConditionRow
              cond={node as Condition}
              zones={props.zones}
              onChange={(c) => replaceAt(idx(), c)}
              onRemove={() => removeAt(idx())}
            />
          </Show>
        )}
      </For>
      <button class="condition-builder__add" onClick={addLeaf}>
        + {t("rulesWizardAddCondition") || "Add condition"}
      </button>
    </div>
  );
};

function isLeaf(node: ConditionNode): boolean {
  return (
    typeof node === "object" && node !== null && "type" in node && !("kind" in node)
  );
}

interface ConditionRowProps {
  cond: Condition;
  zones: BentoZone[];
  onChange: (c: Condition) => void;
  onRemove: () => void;
}

const ConditionRow: Component<ConditionRowProps> = (props) => {
  const current = () => props.cond.type as ConditionType;

  return (
    <div class="condition-builder__row">
      <select
        class="condition-builder__select"
        value={current()}
        onChange={(e) => props.onChange(defaultCondition(e.currentTarget.value as ConditionType))}
      >
        <For each={CONDITION_TYPES}>{(kind) => <option value={kind}>{kind}</option>}</For>
      </select>

      <Show when={props.cond.type === "ExtensionIn"}>
        <input
          class="condition-builder__input"
          placeholder="tmp,log,bak"
          value={(props.cond as { value: string[] }).value.join(",")}
          onInput={(e) =>
            props.onChange({
              type: "ExtensionIn",
              value: e.currentTarget.value
                .split(",")
                .map((s) => s.trim().replace(/^\./, ""))
                .filter(Boolean),
            })
          }
        />
      </Show>

      <Show when={props.cond.type === "NameMatchesRegex"}>
        <input
          class="condition-builder__input"
          placeholder="^report"
          value={(props.cond as { value: string }).value}
          onInput={(e) =>
            props.onChange({ type: "NameMatchesRegex", value: e.currentTarget.value })
          }
        />
      </Show>

      <Show when={props.cond.type === "CreatedBefore" || props.cond.type === "ModifiedBefore"}>
        <input
          class="condition-builder__input"
          type="number"
          min="1"
          value={(props.cond as { value: { days_ago: number } }).value.days_ago}
          onInput={(e) =>
            props.onChange({
              type: props.cond.type as "CreatedBefore" | "ModifiedBefore",
              value: { days_ago: parseInt(e.currentTarget.value, 10) || 1 },
            })
          }
        />
        <span class="condition-builder__hint">{t("rulesWizardDays") || "days"}</span>
      </Show>

      <Show when={props.cond.type === "SizeGreaterThan"}>
        <input
          class="condition-builder__input"
          type="number"
          min="0"
          value={(props.cond as { value: number }).value}
          onInput={(e) =>
            props.onChange({
              type: "SizeGreaterThan",
              value: parseInt(e.currentTarget.value, 10) || 0,
            })
          }
        />
        <span class="condition-builder__hint">bytes</span>
      </Show>

      <Show when={props.cond.type === "InZone"}>
        <select
          class="condition-builder__input"
          value={(props.cond as { value: string }).value}
          onChange={(e) => props.onChange({ type: "InZone", value: e.currentTarget.value })}
        >
          <option value="">(select zone)</option>
          <For each={props.zones}>
            {(z) => <option value={z.id}>{z.name}</option>}
          </For>
        </select>
      </Show>

      <button class="condition-builder__remove" onClick={props.onRemove} title="Remove">
        ×
      </button>
    </div>
  );
};

export default ConditionBuilder;
