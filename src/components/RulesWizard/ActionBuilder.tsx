/**
 * ActionBuilder — Edit the ordered action list of a rule.
 */
import { Component, For, Show } from "solid-js";
import type { Action } from "../../services/rules";
import type { BentoZone } from "../../types/zone";
import { t } from "../../i18n";

type ActionType =
  | "MoveToZone"
  | "MoveToFolder"
  | "DeleteToRecycleBin"
  | "Tag"
  | "Notify";

const ACTION_TYPES: ActionType[] = [
  "MoveToZone",
  "MoveToFolder",
  "DeleteToRecycleBin",
  "Tag",
  "Notify",
];

function defaultAction(kind: ActionType): Action {
  switch (kind) {
    case "MoveToZone":
      return { type: "MoveToZone", value: "" };
    case "MoveToFolder":
      return { type: "MoveToFolder", value: "C:\\Users\\Public\\Desktop\\Archive" };
    case "DeleteToRecycleBin":
      return { type: "DeleteToRecycleBin", value: null } as unknown as Action;
    case "Tag":
      return { type: "Tag", value: ["archive"] };
    case "Notify":
      return { type: "Notify", value: "Rule matched" };
  }
}

interface ActionBuilderProps {
  actions: Action[];
  zones: BentoZone[];
  onChange: (actions: Action[]) => void;
}

const ActionBuilder: Component<ActionBuilderProps> = (props) => {
  const add = () => props.onChange([...props.actions, defaultAction("MoveToZone")]);
  const replace = (i: number, a: Action) => {
    const list = [...props.actions];
    list[i] = a;
    props.onChange(list);
  };
  const remove = (i: number) => props.onChange(props.actions.filter((_, j) => j !== i));

  return (
    <div class="action-builder">
      <For each={props.actions}>
        {(action, idx) => (
          <div class="action-builder__row">
            <select
              class="action-builder__select"
              value={action.type}
              onChange={(e) =>
                replace(idx(), defaultAction(e.currentTarget.value as ActionType))
              }
            >
              <For each={ACTION_TYPES}>{(k) => <option value={k}>{k}</option>}</For>
            </select>

            <Show when={action.type === "MoveToZone"}>
              <select
                class="action-builder__input"
                value={(action as { value: string }).value}
                onChange={(e) =>
                  replace(idx(), { type: "MoveToZone", value: e.currentTarget.value })
                }
              >
                <option value="">(select zone)</option>
                <For each={props.zones}>
                  {(z) => <option value={z.id}>{z.name}</option>}
                </For>
              </select>
            </Show>

            <Show when={action.type === "MoveToFolder"}>
              <input
                class="action-builder__input"
                value={(action as { value: string }).value}
                placeholder="C:\\Users\\You\\Archive"
                onInput={(e) =>
                  replace(idx(), { type: "MoveToFolder", value: e.currentTarget.value })
                }
              />
            </Show>

            <Show when={action.type === "Tag"}>
              <input
                class="action-builder__input"
                placeholder="archive,review"
                value={(action as { value: string[] }).value.join(",")}
                onInput={(e) =>
                  replace(idx(), {
                    type: "Tag",
                    value: e.currentTarget.value
                      .split(",")
                      .map((s) => s.trim())
                      .filter(Boolean),
                  })
                }
              />
            </Show>

            <Show when={action.type === "Notify"}>
              <input
                class="action-builder__input"
                value={(action as { value: string }).value}
                onInput={(e) =>
                  replace(idx(), { type: "Notify", value: e.currentTarget.value })
                }
              />
            </Show>

            <button
              class="action-builder__remove"
              onClick={() => remove(idx())}
              title="Remove"
            >
              ×
            </button>
          </div>
        )}
      </For>
      <button class="action-builder__add" onClick={add}>
        + {t("rulesWizardAddAction") || "Add action"}
      </button>
    </div>
  );
};

export default ActionBuilder;
