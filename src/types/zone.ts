// BentoDesk Zone & Item types — mirrors Rust data model exactly

export interface RelativePosition {
  x_percent: number; // 0.0 - 100.0
  y_percent: number; // 0.0 - 100.0
}

export interface RelativeSize {
  w_percent: number; // Width as % of screen
  h_percent: number; // Height as % of screen
}

export interface GridPosition {
  col: number;
  row: number;
  col_span: number; // 1 or 2 (wide cards)
}

export type ItemType = "File" | "Folder" | "Shortcut" | "Application";

export interface BentoItem {
  id: string;          // UUID v4
  zone_id: string;     // Parent zone ID
  item_type: ItemType;
  name: string;        // Display name
  path: string;        // Absolute filesystem path
  icon_hash: string;   // Hash for icon cache lookup
  grid_position: GridPosition;
  is_wide: boolean;    // Spans 2 columns
  added_at: string;    // ISO 8601
  original_path?: string | null;  // Desktop path before hiding
  hidden_path?: string | null;    // Path in hidden_items/ storage
  icon_x?: number | null;         // Desktop icon X position when hidden
  icon_y?: number | null;         // Desktop icon Y position when hidden
  file_missing?: boolean;         // Hidden file was deleted externally
}

export type GroupRuleType = "Extension" | "ModifiedDate" | "NamePattern";

export interface AutoGroupRule {
  rule_type: GroupRuleType;
  pattern: string | null;          // Regex for NamePattern
  extensions: string[] | null;     // For Extension type
}

export type CapsuleSize = "small" | "medium" | "large";
export type CapsuleShape = "pill" | "rounded" | "circle" | "minimal";
export type ZoneDisplayMode = "hover" | "always" | "click";

export interface BentoZone {
  id: string;                        // UUID v4
  name: string;                      // Display name
  icon: string;                      // Emoji character
  position: RelativePosition;        // Percentage-based position
  expanded_size: RelativeSize;       // Expanded dimensions (%)
  items: BentoItem[];                // Items in this zone
  accent_color: string | null;       // Hex color for zone accent
  sort_order: number;                // Z-order among zones
  auto_group: AutoGroupRule | null;  // Smart grouping rule
  grid_columns: number;             // Number of grid columns (default 4)
  created_at: string;               // ISO 8601
  updated_at: string;               // ISO 8601
  capsule_size: CapsuleSize;        // Zen capsule size variant (default "medium")
  capsule_shape: CapsuleShape;      // Zen capsule shape variant (default "pill")
  locked?: boolean;                 // When true, zone cannot be repositioned/resized
  // D2/D3 additive (Theme D v1.2.0) — all optional for zero-migration back-compat.
  /** ID of the stack this zone belongs to; null/undefined = not stacked. */
  stack_id?: string | null;
  /** Z-order within the stack (0 = bottom, N-1 = top). */
  stack_order?: number;
  /** Optional user-defined display alias. Display priority: alias ?? name. */
  alias?: string | null;
  /** Optional per-zone override for reveal behaviour. */
  display_mode?: ZoneDisplayMode | null;
}

export interface ZoneUpdate {
  name?: string;
  icon?: string;
  position?: RelativePosition;
  expanded_size?: RelativeSize;
  accent_color?: string;
  grid_columns?: number;
  auto_group?: AutoGroupRule;
  capsule_size?: CapsuleSize;
  capsule_shape?: CapsuleShape;
  locked?: boolean;
  alias?: string | null;
  display_mode?: ZoneDisplayMode | null;
}

export interface FileInfo {
  name: string;
  path: string;
  size: number;
  file_type: string;
  modified_at: string;
  created_at: string;
  is_directory: boolean;
  extension: string | null;
}

export interface SuggestedGroup {
  name: string;
  icon: string;
  rule: AutoGroupRule;
  matching_files: string[];
  confidence: number; // 0.0-1.0
}

export interface IconHashRepairEntry {
  item_id: string;
  old_icon_hash: string;
  new_icon_hash: string;
}

export interface ItemIconRepairReport {
  repaired_count: number;
  repairs: IconHashRepairEntry[];
}

export interface LayoutNormalizeReport {
  normalized_zone_ids: string[];
}
