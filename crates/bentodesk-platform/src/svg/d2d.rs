//! `Parsed` → `ID2D1PathGeometry` materialization.
//!
//! One geometry per icon (multi-path SVGs collapse into a single geometry —
//! the D2D builder accepts multiple `BeginFigure` / `EndFigure` pairs in one
//! sink). Per spec §10 paint reuses the cached geometry; this builder runs
//! once per icon at first paint.

use windows::Win32::Graphics::Direct2D::Common::{
    D2D_POINT_2F, D2D1_BEZIER_SEGMENT, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
    D2D1_FIGURE_END_OPEN,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_QUADRATIC_BEZIER_SEGMENT, ID2D1Factory1, ID2D1GeometrySink, ID2D1PathGeometry,
};
use windows::core::Interface;

use super::types::{Cmd, Parsed, ParsedPath};
use crate::errors::{PlatformError, ok};

impl Parsed {
    /// Materialise an `ID2D1PathGeometry` containing every parsed path.
    pub fn to_d2d_geometry(
        &self,
        factory: &ID2D1Factory1,
    ) -> Result<ID2D1PathGeometry, PlatformError> {
        // SAFETY: factory valid. ID2D1Factory1::CreatePathGeometry returns
        // ID2D1PathGeometry1; cast to base ID2D1PathGeometry per spec §15.1
        // (canonical Interface::cast over QueryInterface).
        let geom1 = ok("CreatePathGeometry", unsafe {
            factory.CreatePathGeometry()
        })?;
        let geom: ID2D1PathGeometry = ok("PathGeometry1::cast<PathGeometry>", geom1.cast())?;
        // SAFETY: geom valid; Open returns a sink that must be closed before drop.
        let sink: ID2D1GeometrySink = ok("PathGeometry::Open", unsafe { geom.Open() })?;
        let res = fill_sink(self, &sink);
        // SAFETY: sink valid; Close finalises geometry — call regardless of fill_sink result.
        let close_res = unsafe { sink.Close() };
        res?;
        ok("GeometrySink::Close", close_res)?;
        Ok(geom)
    }
}

fn fill_sink(parsed: &Parsed, sink: &ID2D1GeometrySink) -> Result<(), PlatformError> {
    for path in &parsed.paths {
        emit_path(sink, path)?;
    }
    Ok(())
}

fn emit_path(sink: &ID2D1GeometrySink, path: &ParsedPath) -> Result<(), PlatformError> {
    let mut figure_open = false;
    let mut start = D2D_POINT_2F { x: 0.0, y: 0.0 };
    let mut cur = D2D_POINT_2F { x: 0.0, y: 0.0 };
    for cmd in &path.commands {
        match *cmd {
            Cmd::Move(x, y) => {
                if figure_open {
                    // SAFETY: sink valid for the duration of fill_sink.
                    unsafe { sink.EndFigure(D2D1_FIGURE_END_OPEN) };
                }
                cur = D2D_POINT_2F { x, y };
                start = cur;
                // SAFETY: sink valid.
                unsafe { sink.BeginFigure(cur, D2D1_FIGURE_BEGIN_FILLED) };
                figure_open = true;
            }
            Cmd::Line(x, y) => {
                if !figure_open {
                    cur = D2D_POINT_2F { x, y };
                    start = cur;
                    // SAFETY: sink valid.
                    unsafe { sink.BeginFigure(cur, D2D1_FIGURE_BEGIN_FILLED) };
                    figure_open = true;
                } else {
                    cur = D2D_POINT_2F { x, y };
                    // SAFETY: sink valid.
                    unsafe { sink.AddLine(cur) };
                }
            }
            Cmd::Cubic {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                if !figure_open {
                    // SAFETY: sink valid.
                    unsafe { sink.BeginFigure(cur, D2D1_FIGURE_BEGIN_FILLED) };
                    figure_open = true;
                    start = cur;
                }
                let bezier = D2D1_BEZIER_SEGMENT {
                    point1: D2D_POINT_2F { x: c1x, y: c1y },
                    point2: D2D_POINT_2F { x: c2x, y: c2y },
                    point3: D2D_POINT_2F { x, y },
                };
                // SAFETY: sink valid; bezier lives across the call.
                unsafe { sink.AddBezier(&bezier) };
                cur = D2D_POINT_2F { x, y };
            }
            Cmd::Quad { c1x, c1y, x, y } => {
                if !figure_open {
                    // SAFETY: sink valid.
                    unsafe { sink.BeginFigure(cur, D2D1_FIGURE_BEGIN_FILLED) };
                    figure_open = true;
                    start = cur;
                }
                let q = D2D1_QUADRATIC_BEZIER_SEGMENT {
                    point1: D2D_POINT_2F { x: c1x, y: c1y },
                    point2: D2D_POINT_2F { x, y },
                };
                // SAFETY: sink valid; q lives across the call.
                unsafe { sink.AddQuadraticBezier(&q) };
                cur = D2D_POINT_2F { x, y };
            }
            Cmd::Close => {
                if figure_open {
                    // SAFETY: sink valid.
                    unsafe { sink.EndFigure(D2D1_FIGURE_END_CLOSED) };
                    figure_open = false;
                    cur = start;
                }
            }
        }
    }
    if figure_open {
        // SAFETY: sink valid.
        unsafe { sink.EndFigure(D2D1_FIGURE_END_OPEN) };
    }
    Ok(())
}
