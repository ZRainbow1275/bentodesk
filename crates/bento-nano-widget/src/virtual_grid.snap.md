# VirtualGrid — visual spec (snap.md)

- **Window**: fixed `viewport_width × viewport_height`; only cells in visible row range materialise.
- **Cell size**: uniform; default 8 px gap between cells.
- **Overscan**: 2 rows above + below the window.
- **Scrollbar**: not drawn (BentoDesk visual language).
- **Layout**: routes through `Direction::Grid { columns }`.
- **No chrome**: transparent — caller paints cells.
