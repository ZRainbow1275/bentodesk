# SvgIcon — visual spec (snap.md)

- **Default size**: 24 px square (matches Lucide convention).
- **Tint**: `palette.text` default; multiplied at the brush boundary via D2D solid brush.
- **Path data**: `&'static str` for compile-time icons (zero alloc), `SmolStr` for runtime.
- **Cache**: `cache_key()` (pointer or FNV-1a hash) feeds platform SVG cache (T-047 LRU 8 MB).
- **No animation**: passive — caller drives transitions via parent's tween.
- **No padding/chrome**: pure icon shape; renderer applies tint as fill.
