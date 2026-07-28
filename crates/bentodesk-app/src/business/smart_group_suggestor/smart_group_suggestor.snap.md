# SmartGroupSuggestor — Visual Fidelity Snapshot

Per Ruling R3. 1.x source: `bentodesk/src/components/SmartGroup/SmartGroupSuggestor.tsx`
+ `SmartGroupSuggestor.css`. Backend feed:
`bentodesk_backend::grouping::{suggest_groups, SuggestedGroup}`.

> Naming note: 1.x and the brief originally referenced `GroupSuggestion` /
> `IconKind`. Reality is `SuggestedGroup` (T-087, serde-clean) +
> free-form `icon: String` slug. The 2.0 module uses the real names per
> team-lead Option-A ruling 2026-05-03 — wrapping for vocabulary parity
> was YAGNI. Runtime rows pass `suggestion.icon` through the selected-stack
> icon renderer, so legacy unknown/text payloads fall back to a neutral
> built-in glyph instead of being painted as visible text.

## Geometry

- Panel width: **480 px** (1.x `.smart-group-dialog` `width: min(480px, 92vw)`).
- Panel max-height: **80 vh** (capped by hosting Modal; vertical scroll
  inside the row list).
- Border radius: **16 px** (matches all 2.0 modal panels — see
  `timeline_panel.snap.md`).
- Outer panel padding: **24 px** uniform.
- Row gap: **8 px** between suggestion rows.

## Layout

Column, top → bottom:

1. **Header row** — title (`palette.text`, 18 pt Semibold) on the left,
   close `IconButton` on the right. 16 px bottom margin.
2. **Body** — switches between four states keyed off `LoadState`:
   - `Scanning` / `Analyzing` — single centred status line in
     `palette.text_muted`, 14 pt, with a 3-dot pulse animation.
   - `Error` — `palette.danger` text + retry hint.
   - `Empty` — neutral muted "no suggestions" placeholder.
   - `Done` — vertical list of `SuggestionRow`s (see below).
3. **Body scrolls** when the row list overflows the available height.

### SuggestionRow geometry

Row layout (Row direction, 12 px inner gap):

| Slot | Width | Notes |
|------|-------|-------|
| Icon | 28 px square | `suggestion.icon` → selected-stack line-art glyph; unknown legacy payloads use neutral builtin fallback. |
| Info column | flex 1 | Stack: name (`palette.text`, 14 pt Semibold) + meta line (`palette.text_muted`, 12 pt). Meta = `"{n} files · {rule_summary}"`. |
| Confidence badge | auto | Pill, 4 px corner, padding 4 / 8 px. Tone keyed off score (see Tones). |
| Apply button | auto | Primary `ButtonNode`, label = `"Apply"`. Disabled when `applying`. |
| Dismiss button | 24 px square | `IconButton` with `IconKind::X` glyph. |

Row outer padding: 12 px vertical, 16 px horizontal. Row corner radius:
8 px. Row background: `palette.surface_alt` idle, with hover overlay
applied by the row's `IconButton` hover behaviour pattern (mirrors
`item_card.snap.md`).

## Confidence tones

Three buckets per `confidenceTone(score)`:

| Score range | Tone | Background | Text |
|-------------|------|------------|------|
| `>= 0.80` | High | `palette.success` (alpha 0x33) | `palette.success` |
| `>= 0.50` | Medium | `palette.warning` (alpha 0x33) | `palette.warning` |
| `<  0.50` | Low | `palette.text_muted` (alpha 0x33) | `palette.text_muted` |

Thresholds match `bentodesk_backend::grouping::ai_recommender::MERGE_THRESHOLD`
(0.55) — anything that survived the merge floor lands at Medium or higher.

## Hover bridge

`SmartGroupSuggestorState::on_row_hover(idx)` records the hovered row's
`suggestion_id` in `hovered_id`. The shell renders `HighlightOverlay`
above the BentoPanel grid using the matching files referenced by that
id. `on_row_leave()` clears `hovered_id` → overlay paints nothing.

The hover bridge is **pure local state** — never round-trips through
the dispatcher (per team-lead R-2026-05-03 ruling: dispatcher only sees
Apply / Dismiss).

## Commands

| User intent | Action recorded | Drained `Command` |
|-------------|-----------------|--------------------|
| Click **Apply** on row | `SuggestorAction::Apply { suggestion }` | `Command::GroupingApply { suggestion: Box<SuggestedGroup> }` |
| Click row dismiss `X` | `SuggestorAction::Dismiss { suggestion_id }` | `Command::SuggestorDismiss { suggestion_id }` |
| Click panel close `X` | `SuggestorAction::Close` | (no Command — shell hides the WindowKind itself) |

`take_action()` is one-shot per the dialog pattern; the shell drains
once per frame.

## Window class

Hosted in its own modal HWND (`WindowKind::SmartGroup` once the platform
factory grows the variant — falls back to `WindowKind::Settings`-shape
in Wave G). `WS_EX_NOREDIRECTIONBITMAP | WS_POPUPWINDOW | WS_CAPTION |
WS_VISIBLE`.

## Hibernation

§11 R5 eligible — dismiss via close button or Escape triggers
`WM_SHOWWINDOW(false)` and the per-window swap chain release. Hover
state resets across show / hide cycles.

## Smoke verification

`mod tests` proves:

- `confidence_tone` thresholds (`0.85` → High, `0.6` → Medium, `0.3` → Low).
- `SuggestorState::apply` records `Apply { suggestion }` and
  `take_action` returns it once then `None`.
- `SuggestorState::dismiss` records `Dismiss { suggestion_id }`.
- `SuggestorState::on_row_hover` / `on_row_leave` toggle `hovered_id`.
- `SuggestorState::set_suggestions` truncates to `MAX_VISIBLE_SUGGESTIONS`.
- `build()` returns a Column container at `PANEL_WIDTH_PX` width.
