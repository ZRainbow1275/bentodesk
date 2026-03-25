# Zone Customization API

Zones are the core organizational unit in BentoDesk. Each zone is a "bento box" container that holds desktop files, displayed as a glassmorphism capsule that can be expanded into a panel.

All types are defined in `src/types/zone.ts` and mirror the Rust data model exactly.

## BentoZone Interface

```typescript
interface BentoZone {
  id: string;                        // UUID v4, auto-generated
  name: string;                      // Display name (e.g. "Documents", "Projects")
  icon: string;                      // Emoji character (e.g. "📁", "🎮")
  position: RelativePosition;        // Percentage-based screen position
  expanded_size: RelativeSize;       // Expanded panel dimensions (%)
  items: BentoItem[];                // Files/folders in this zone
  accent_color: string | null;       // Hex color override (e.g. "#3b82f6"), or null for theme default
  sort_order: number;                // Z-order among zones (lower = behind)
  auto_group: AutoGroupRule | null;  // Smart grouping rule, or null for manual-only
  grid_columns: number;              // Number of grid columns in expanded view (default 4)
  created_at: string;                // ISO 8601 creation timestamp
  updated_at: string;                // ISO 8601 last modification timestamp
  capsule_size: CapsuleSize;         // Collapsed capsule size variant (default "medium")
  capsule_shape: CapsuleShape;       // Collapsed capsule shape variant (default "pill")
}
```

### Position Types

```typescript
interface RelativePosition {
  x_percent: number;  // 0.0 - 100.0, horizontal position as % of screen width
  y_percent: number;  // 0.0 - 100.0, vertical position as % of screen height
}

interface RelativeSize {
  w_percent: number;  // Width as % of screen width
  h_percent: number;  // Height as % of screen height
}
```

Positions and sizes are stored as percentages so that zones adapt automatically when the screen resolution changes. A resolution monitor polls every 2 seconds and clamps zones that would overflow the new screen bounds.

## CapsuleShape

The shape of a zone when it is in the collapsed (zen) state.

```typescript
type CapsuleShape = "pill" | "rounded" | "circle" | "minimal";
```

| Value | Description |
|-------|-------------|
| `"pill"` | Elongated pill shape with fully rounded ends. Default shape. |
| `"rounded"` | Rectangle with rounded corners. More compact than pill. |
| `"circle"` | Perfect circle. Best for zones with short names or icon-only display. |
| `"minimal"` | Minimal footprint with very subtle styling. |

## CapsuleSize

The size variant of a zone capsule in the collapsed state.

```typescript
type CapsuleSize = "small" | "medium" | "large";
```

| Value | Description |
|-------|-------------|
| `"small"` | Compact capsule, reduced padding and font size |
| `"medium"` | Default size |
| `"large"` | Larger capsule with more padding, bigger icon and text |

## BentoItem Interface

Each file inside a zone is represented as a `BentoItem`.

```typescript
interface BentoItem {
  id: string;              // UUID v4, auto-generated
  zone_id: string;         // Parent zone ID
  item_type: ItemType;     // "File" | "Folder" | "Shortcut" | "Application"
  name: string;            // Display name (filename without extension for files)
  path: string;            // Absolute filesystem path (hidden location while in zone)
  icon_hash: string;       // Hash for icon cache lookup
  grid_position: GridPosition;  // Position within the zone's grid
  is_wide: boolean;        // If true, spans 2 columns in the grid
  added_at: string;        // ISO 8601 timestamp when added to zone
  original_path?: string | null;   // Desktop path before hiding
  hidden_path?: string | null;     // Path in .bentodesk/ storage
  icon_x?: number | null;         // Desktop icon X position before hiding
  icon_y?: number | null;         // Desktop icon Y position before hiding
}
```

### ItemType

```typescript
type ItemType = "File" | "Folder" | "Shortcut" | "Application";
```

| Value | Description |
|-------|-------------|
| `"File"` | Regular file (document, image, archive, etc.) |
| `"Folder"` | Directory |
| `"Shortcut"` | Windows `.lnk` shortcut file |
| `"Application"` | Executable (`.exe`) or application shortcut |

### GridPosition

```typescript
interface GridPosition {
  col: number;       // Column index (0-based)
  row: number;       // Row index (0-based)
  col_span: number;  // Number of columns spanned (1 for normal, 2 for wide cards)
}
```

The grid layout uses `grid_columns` (from the parent zone) to determine the number of columns. Items are laid out left-to-right, top-to-bottom. Wide items (`is_wide: true`) have `col_span: 2`.

## ZoneUpdate Interface

Used to partially update a zone's properties. All fields are optional.

```typescript
interface ZoneUpdate {
  name?: string;
  icon?: string;
  position?: RelativePosition;
  expanded_size?: RelativeSize;
  accent_color?: string;
  grid_columns?: number;
  auto_group?: AutoGroupRule;
  capsule_size?: CapsuleSize;
  capsule_shape?: CapsuleShape;
}
```

Usage via IPC:

```typescript
import { updateZone } from "../services/ipc";

// Change only the name and icon
await updateZone("zone-uuid-here", {
  name: "My Projects",
  icon: "🚀",
});

// Change capsule appearance
await updateZone("zone-uuid-here", {
  capsule_shape: "circle",
  capsule_size: "large",
  accent_color: "#8b5cf6",
});
```

## AutoGroupRule

Smart grouping rules that automatically assign matching files to a zone.

```typescript
interface AutoGroupRule {
  rule_type: GroupRuleType;          // "Extension" | "ModifiedDate" | "NamePattern"
  pattern: string | null;           // Regex pattern (for NamePattern type)
  extensions: string[] | null;      // File extensions (for Extension type, e.g. [".pdf", ".docx"])
}
```

| Rule Type | Description | Relevant Field |
|-----------|-------------|----------------|
| `"Extension"` | Match files by extension | `extensions` (e.g. `[".pdf", ".docx", ".xlsx"]`) |
| `"ModifiedDate"` | Group by modification date | -- (grouping logic in backend) |
| `"NamePattern"` | Match filenames by regex | `pattern` (e.g. `"^report_.*"`) |

When `auto_group` is set on a zone, the file watcher automatically adds new desktop files that match the rule.

## SuggestedGroup

Returned by the smart grouping suggestion system.

```typescript
interface SuggestedGroup {
  name: string;              // Suggested zone name (e.g. "Documents")
  icon: string;              // Suggested emoji icon
  rule: AutoGroupRule;       // The rule that generates this grouping
  matching_files: string[];  // File paths that match
  confidence: number;        // 0.0 - 1.0 confidence score
}
```

## FileInfo

Metadata for a scanned desktop file.

```typescript
interface FileInfo {
  name: string;
  path: string;
  size: number;           // File size in bytes
  file_type: string;      // "File", "Folder", etc.
  modified_at: string;    // ISO 8601
  created_at: string;     // ISO 8601
  is_directory: boolean;
  extension: string | null;  // File extension including dot (e.g. ".pdf")
}
```

## Data Storage

Zone and item data is persisted in `layout.json` within the Tauri app data directory. The file is saved after every mutation (zone create/update/delete, item add/remove/move).

Hidden files are stored in `Desktop/.bentodesk/{zone_id}/` with a safety manifest tracking all file movements. See [Architecture](./architecture.md) for details on the file hiding mechanism.
