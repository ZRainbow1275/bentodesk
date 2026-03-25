/**
 * Resolution & DPI validation service.
 *
 * Provides utility functions to clamp zone positions and sizes
 * to valid viewport bounds, accounting for current screen dimensions.
 */
import type { RelativePosition, RelativeSize } from "../types/zone";

/** Minimum zone size as percentage of screen */
const MIN_SIZE_PERCENT = 10;

/** Maximum zone size as percentage of screen */
const MAX_SIZE_PERCENT = 80;

/** Maximum position (prevents zone from going off-screen) */
const MAX_POSITION_PERCENT = 95;

/**
 * Clamp a relative position to valid viewport bounds.
 * Ensures the zone's top-left corner stays within the visible area.
 */
export function clampPosition(pos: RelativePosition): RelativePosition {
  return {
    x_percent: Math.max(0, Math.min(MAX_POSITION_PERCENT, pos.x_percent)),
    y_percent: Math.max(0, Math.min(MAX_POSITION_PERCENT, pos.y_percent)),
  };
}

/**
 * Clamp a relative size to valid bounds.
 * Prevents zones from being too small to interact with or too large.
 */
export function clampSize(size: RelativeSize): RelativeSize {
  return {
    w_percent: Math.max(MIN_SIZE_PERCENT, Math.min(MAX_SIZE_PERCENT, size.w_percent)),
    h_percent: Math.max(MIN_SIZE_PERCENT, Math.min(MAX_SIZE_PERCENT, size.h_percent)),
  };
}

/**
 * Validate and clamp position + size together.
 * Ensures the zone doesn't overflow the right/bottom edges.
 */
export function validateZoneBounds(
  pos: RelativePosition,
  size: RelativeSize
): { position: RelativePosition; size: RelativeSize } {
  const clampedSize = clampSize(size);
  const maxX = 100 - clampedSize.w_percent;
  const maxY = 100 - clampedSize.h_percent;

  return {
    position: {
      x_percent: Math.max(0, Math.min(maxX, pos.x_percent)),
      y_percent: Math.max(0, Math.min(maxY, pos.y_percent)),
    },
    size: clampedSize,
  };
}

/**
 * Convert a pixel position to relative percentage position.
 */
export function pixelToPercent(
  pixelX: number,
  pixelY: number
): RelativePosition {
  return {
    x_percent: (pixelX / window.innerWidth) * 100,
    y_percent: (pixelY / window.innerHeight) * 100,
  };
}

/**
 * Convert a percentage position to pixel position.
 */
export function percentToPixel(
  pos: RelativePosition
): { x: number; y: number } {
  return {
    x: (pos.x_percent / 100) * window.innerWidth,
    y: (pos.y_percent / 100) * window.innerHeight,
  };
}
