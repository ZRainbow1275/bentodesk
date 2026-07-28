# SearchBar — visual snap

Top-floating search bar opened by global hotkey (Ctrl+K) or
`Command::OpenSearch`. Drives query into `bentodesk-backend::search` and
renders a results list below the input.

- **Window placement:** centred top-third of primary monitor.
- **Window size:** 560 × Auto (results expand downward).
- **Surface:** `theme::current().palette.surface_elevated`,
  `BorderRadius::all(12.0)`, drop shadow (per `BentoCard` style).
- **Input row height:** 48 px, padding `Edges { left: 16, right: 16, top: 12, bottom: 12 }`.
- **Input chrome:** no border, `palette.text` text, `palette.text_dim`
  placeholder ("Search files, zones, settings…").
- **Results list:** max 8 rows visible, each 44 px tall. Rows show icon +
  name + breadcrumb (palette.text_dim). Hover row gets
  `palette.surface_hover`. Selected row (arrow keys) gets
  `palette.accent_translucent`.

## Wire constants

- `WINDOW_WIDTH = 560.0`.
- `INPUT_HEIGHT = 48.0`.
- `RESULT_ROW_HEIGHT = 44.0`.
- `MAX_VISIBLE_RESULTS: usize = 8`.
- `DEBOUNCE_MS: u32 = 120` — 1.x baseline; raises only if the backend
  search call shows hot-path stalls.
- `ESC_DISMISS_MS: u32 = 0` — Esc dismisses immediately, no hold.

## State machine

`SearchBarState` carries `query: SmolStr`, `results: SmallVec<[SearchHit; 8]>`,
`selected: Option<usize>`, `pending_query_at_ms: Option<u32>`. The
`tick(dt_ms)` accumulator fires `Command::QuerySearch(query.clone())` when
the debounce window elapses and the query hasn't changed since.

## Hibernation contract

SearchBar is short-lived (typed → dismissed). When the window hides
(`WM_KILLFOCUS` or Esc), shell calls `release_swap_chain` per T-099.
On next Ctrl+K, shell calls `ensure_swap_chain` before showing.
