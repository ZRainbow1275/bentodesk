# BackupCard — Visual Spec

Source: `bentodesk/src/components/Settings/BackupCard.tsx` (150 LOC) + `BackupCard.css`.

- **Card:** plain flex column with `gap: 10px` (no chrome of its own — sits inside the SettingsPanel `.settings-group` rhythm). Composes `.settings-card` + `.settings-card-title` host classes from the panel.
- **Title (`.settings-card-title`):** inherits SettingsPanel section-title styling — see `settings_panel.snap.md`.
- **Description:** `12 px palette.text_secondary` (`#a0a0b0` fallback), `margin: 0`.
- **Actions row:** flex with 8 px gap. "Create Now" button: `padding: 6px 14px`, `font-size: 12px`, `border-radius: 6px`, 1 px `rgba(255,255,255,0.12)` border, `rgba(96,165,250,0.18)` bg, `#60a5fa` text. Disabled state: `opacity: 0.5`, `cursor: not-allowed`.
- **Status banners (mutually exclusive, render under the actions row):** error `#f87171` 12 px text; info `#34d399` 12 px text; both `margin: 0`, no own background.
- **Backup list (`.backup-list`):** unstyled `<ul>`, vertical stack with **6 px gap**.
- **Backup entry (`.backup-entry`):** flex space-between, `padding: 8px 12px`, `rgba(255,255,255,0.04)` bg, `border-radius: 6px`, 13 px text. Left side: monospaced timestamp + 11 px muted size label, 2 px column gap. Right side: Restore button — `padding: 4px 12px`, `border-radius: 6px`, `rgba(255,255,255,0.06)` bg, `inherit` color, 1 px `rgba(255,255,255,0.12)` border, 12 px text. Hover: `rgba(96,165,250,0.2)` bg.
- **Empty state:** `.backup-empty` — centered, 12 px `palette.text_muted`, `padding: 12px`.
- **Timestamp format:** `YYYYMMDDTHHMMSS` → `YYYY-MM-DD HH:MM:SS UTC` (see `format_timestamp`). Size: `<1 KiB → "<n> B"`, otherwise `"<x.x> KB"` (1-decimal KiB) (see `format_size`).
- **Behaviour:** `onMount` calls `listBackups()` and registers `onBackupCreated` listener. `onCreate` → `createBackup()` then refresh. `onRestore(entry)` → `confirm()` modal → `restoreBackup(entry.id)` → `loadSettings()` → refresh. Backend dep: `bentodesk-backend::config_vault::{list_backups, create_backup, restore_backup}` (T-092). Eventual dispatcher hook: emits `Command::SetSetting { key: "backup.last_restored", value: SettingValue::Str(entry_id) }` post-restore so the SettingsPanel host re-derives `dirty`.
