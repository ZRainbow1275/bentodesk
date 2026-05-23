# RulesWizard — Visual Fidelity Snapshot

Per Ruling R3. 1.x source: `bentodesk/src/components/RulesWizard/RulesWizard.tsx`
+ `RulesWizard.css`. Note: 1.x ships a list-+-form layout; nano redesigns
the create-flow as a 5-step wizard while keeping the same data model
(`bento_nano_backend::rules::Rule`).

## Geometry

- Modal panel: **560 px wide × 600 px tall**, `palette.surface` background,
  1 px `palette.border`, 16 px corner radius, scale-in 200 ms ease-out.
- Scrim: full-viewport `palette.scrim` (DARK = 0x00000080,
  LIGHT = 0x00000040), click → cancel.
- Header row: 56 px tall, 24 px horizontal padding. Title (`palette.text`,
  18 pt semibold) on the left, 32 × 32 px close button on the right.

## Step indicator

- 48 px tall, 24 px horizontal padding, 8 px gap between dots.
- Each step is a 24 × 24 px circle; current step uses `palette.accent`
  background + white digit, completed steps use `palette.success` + check
  glyph, pending steps use `palette.surface_alt` + `palette.text_muted`
  digit.
- Connecting line between dots: 2 px tall, `palette.border` for pending,
  `palette.success` for completed.

## Step bodies

Body fills the central region (panel - header - indicator - footer, ~424 px
tall), 24 px padding all sides, vertical scroll when content overflows.

1. **Conditions** — vertical list of condition rows + "+ Add condition"
   button. Each row: dropdown for predicate kind (8 options), inline value
   input (text / number depending on kind), trash icon for delete. Rows
   are wrapped in a top-level All / Any toggle (segmented switch at top).
2. **Action** — single-select row of 5 action cards (MoveToZone,
   MoveToFolder, DeleteToRecycleBin, Tag, Notify), 96 × 96 px each, 8 px
   corner radius, selected card uses `palette.accent` 12% bg + 2 px
   `palette.accent` border. Inline detail input shows below the cards
   based on selection (zone dropdown / folder picker / tag chip input /
   notify text).
3. **Preview** — read-only scrollable list of matched file paths populated
   asynchronously by the shell (`set_preview_hits`). Header row shows
   "N file(s) match"; busy state shows a 13 pt italic
   `palette.text_muted` line "Scanning…".
4. **Name + enable** — single-line Input for rule name (max 64 chars) +
   Toggle for `enabled` + RadioGroup for run mode (OnDemand / OnFileChange
   / Interval). Interval picks a number-stepper for minutes (1..1440).
5. **Review** — read-only summary of every choice made; Back button +
   Save button (primary).

## Footer row

- 64 px tall, 24 px horizontal / 16 px vertical padding.
- Back (left, secondary) + Cancel (left, ghost) + Next/Save (right,
  primary). Back is hidden on step 1; Save replaces Next on step 5.
- 8 px gap between buttons.

## Save / Cancel rules

- Cancel discards the in-progress rule, closes modal — no action emitted
  beyond `RulesWizardAction::Cancel`.
- Save (step 5 only) emits `RulesWizardAction::Save(Rule)` with a fully
  populated `Rule` (id stays empty for create; the shell stamps a UUID).
- Next is disabled when the current step is incomplete (e.g. step 1 with
  zero conditions; step 4 with blank name).

## Keyboard contract

- Escape → cancel.
- Enter on step 5 commits Save (when enabled).
- Arrow keys move focus between condition / action rows.
