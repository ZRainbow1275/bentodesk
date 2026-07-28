# VirtualList — visual spec (snap.md)

- **Window**: fixed `viewport_height`; only rows whose y intersects the window are materialised.
- **Row height**: uniform (caller supplies); variable-height belongs in a v2 widget.
- **Overscan**: 4 rows above + below the visible window for smooth fast-scroll.
- **Scrollbar**: not drawn (BentoDesk visual language); scroll is gesture-driven.
- **Recycling**: caller reuses NodeIds across `visible_range()` calls.
- **No chrome**: transparent — caller paints rows.
