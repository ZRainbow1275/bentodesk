/**
 * rules — Shared TypeScript types matching src-tauri/src/rules/mod.rs.
 */

export type Condition =
  | { type: "ExtensionIn"; value: string[] }
  | { type: "NameMatchesRegex"; value: string }
  | { type: "CreatedBefore"; value: { days_ago: number } }
  | { type: "ModifiedBefore"; value: { days_ago: number } }
  | { type: "SizeGreaterThan"; value: number }
  | { type: "InZone"; value: string }
  | { type: "OnDesktop"; value: null };

export type ConditionNode = Condition | ConditionGroup;

export type ConditionGroup =
  | { kind: "All"; value: ConditionNode[] }
  | { kind: "Any"; value: ConditionNode[] }
  | { kind: "Not"; value: ConditionGroup };

export type Action =
  | { type: "MoveToZone"; value: string }
  | { type: "MoveToFolder"; value: string }
  | { type: "DeleteToRecycleBin"; value: null }
  | { type: "Tag"; value: string[] }
  | { type: "Notify"; value: string };

export type RunMode =
  | { type: "OnDemand"; value: null }
  | { type: "OnFileChange"; value: null }
  | { type: "Interval"; value: { minutes: number } };

export interface Rule {
  id: string;
  name: string;
  enabled: boolean;
  conditions: ConditionGroup;
  actions: Action[];
  run_mode: RunMode;
  last_run: string | null;
  run_count: number;
}

export interface ExecutionReport {
  matched: number;
  actions_taken: string[];
  errors: string[];
  checkpoint_trigger: string;
}
