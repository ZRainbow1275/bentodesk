//! Multi-monitor display querying.
//!
//! Exposes per-monitor work area / full bounds / DPI scale via
//! `EnumDisplayMonitors`, `MonitorFromPoint`, `MonitorFromWindow`,
//! `GetMonitorInfoW` and `GetDpiForMonitor`. Zone geometry (R1 anchor
//! flip) uses this to compute overflow against the specific monitor a
//! capsule lives on, not just the primary monitor's work area.

pub mod monitors;
