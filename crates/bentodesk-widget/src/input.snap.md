# Input — visual spec (snap.md)

- **Default size**: 240×32 px; 8 px horizontal, 6 px vertical padding.
- **Background**: `palette.surface_alt`; 4 px corner radius.
- **Border**: 1 px `palette.border`; 1 px `palette.accent` when `focused`.
- **Text color**: `palette.text`; placeholder = `palette.text_muted`.
- **Caret**: 1 px `palette.accent`, blinks at 530 ms cycle (caller drives blink).
- **Selection**: `palette.selection` background behind selected codeunits.
- **IME composition**: rendered with 1 px underline on the composition span.
- **Disabled**: 50% alpha; insert/backspace/IME no-ops.
