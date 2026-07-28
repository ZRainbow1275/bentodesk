# FileIcon — visual spec (snap.md)

- **Default size**: 32×32 px (Win32 `SHIL_LARGE`). Renderer also caches 64 px (`SHIL_EXTRALARGE`) for HiDPI.
- **Background**: transparent default; caller may set for card-style placeholders.
- **Corner radius**: 4 px on the bitmap clip (rounded-rect mask).
- **Pending state**: `cache_hash == 0` → renderer shows placeholder (generic file glyph).
- **Resolution**: caller invokes `IExtractIconW` via the platform layer; updates `cache_hash` on completion.
- **Path storage**: `Arc<PathBuf>` shared with the cache key — no duplication.
