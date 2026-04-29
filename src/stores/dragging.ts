/**
 * Global drag flag.
 *
 * v6 frontend-zone-fixer (#3): zones must NOT auto-expand on hover or
 * single-click while ANY drag gesture is in flight. The previous
 * `isDragRepositioning` signal was scoped per-component, so a drag
 * starting in zone A could still trip hover-expand on zone B as the
 * cursor swept across it — producing the "ghost panel during drag"
 * the user reported.
 *
 * Producers (call `setIsDragging(true)` on gesture start, `false` in
 * the mouseup `finally`):
 *   - BentoZone.tsx → header drag, group capsule drag, resize handle
 *   - StackWrapper.tsx → stack capsule drag
 *   - ItemCard.tsx → item drag tracking (drag-out / cross-zone drop)
 *
 * Consumers (early-return when `isDragging()` is true):
 *   - BentoZone.handleMouseEnter / handleMouseLeave / handleZoneClick
 *   - StackWrapper.handleMouseEnter / handleCapsuleClick
 */
import { createSignal } from "solid-js";

const [isDragging, setIsDragging] = createSignal(false);

export { isDragging, setIsDragging };
