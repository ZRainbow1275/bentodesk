import type { RelativePosition } from "../types/zone";

export type GroupDragOrigin = Record<string, RelativePosition>;

export function buildGroupDragPreview(
  origin: GroupDragOrigin,
  delta: RelativePosition,
  maxPercent = 96,
): GroupDragOrigin {
  const next: GroupDragOrigin = {};
  for (const [zoneId, position] of Object.entries(origin)) {
    next[zoneId] = {
      x_percent: Math.max(0, Math.min(maxPercent, position.x_percent + delta.x_percent)),
      y_percent: Math.max(0, Math.min(maxPercent, position.y_percent + delta.y_percent)),
    };
  }
  return next;
}
