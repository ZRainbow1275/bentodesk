/**
 * Monitor types — mirrors Rust `display::monitors::MonitorInfo`.
 *
 * All rectangles are in **physical pixels** (pre-dividing by `dpi_scale`
 * yields CSS / logical pixels). The overlay window in BentoDesk spans
 * the primary monitor's work area at logical scale; for secondary
 * monitors the frontend must map capsule positions to the appropriate
 * monitor via `get_monitor_for_point`.
 */

export interface MonitorRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface MonitorInfo {
  index: number;
  rect_work: MonitorRect;
  rect_full: MonitorRect;
  dpi_scale: number;
  is_primary: boolean;
  device_name: string;
}
