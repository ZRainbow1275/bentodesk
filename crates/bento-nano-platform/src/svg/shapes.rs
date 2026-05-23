//! Shape lowering — `<circle>` / `<rect>` / SVG arc → cubic bezier(s).

use super::path_d::push_cubic;
use super::types::{Affine, Cmd, ParsedPath};

/// Magic constant for a 4-arc cubic-bezier circle (4 * (sqrt(2) - 1) / 3).
pub(super) const KAPPA: f32 = 0.552_284_8;

pub(super) fn emit_circle(path: &mut ParsedPath, cx: f32, cy: f32, r: f32, xform: Affine) {
    if r <= 0.0 {
        return;
    }
    let k = r * KAPPA;
    let (sx, sy) = xform.apply(cx + r, cy);
    path.commands.push(Cmd::Move(sx, sy));
    push_cubic(path, xform, cx + r, cy + k, cx + k, cy + r, cx, cy + r);
    push_cubic(path, xform, cx - k, cy + r, cx - r, cy + k, cx - r, cy);
    push_cubic(path, xform, cx - r, cy - k, cx - k, cy - r, cx, cy - r);
    push_cubic(path, xform, cx + k, cy - r, cx + r, cy - k, cx + r, cy);
    path.commands.push(Cmd::Close);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_rect(
    path: &mut ParsedPath,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: f32,
    ry: f32,
    xform: Affine,
) {
    let rx = rx.min(w * 0.5);
    let ry = ry.min(h * 0.5);
    if rx == 0.0 || ry == 0.0 {
        let (a, b) = xform.apply(x, y);
        let (c, dpt) = xform.apply(x + w, y);
        let (e, f) = xform.apply(x + w, y + h);
        let (g, hpt) = xform.apply(x, y + h);
        path.commands.push(Cmd::Move(a, b));
        path.commands.push(Cmd::Line(c, dpt));
        path.commands.push(Cmd::Line(e, f));
        path.commands.push(Cmd::Line(g, hpt));
        path.commands.push(Cmd::Close);
        return;
    }
    let kx = rx * KAPPA;
    let ky = ry * KAPPA;
    let (sx, sy) = xform.apply(x + rx, y);
    path.commands.push(Cmd::Move(sx, sy));
    let (lx, ly) = xform.apply(x + w - rx, y);
    path.commands.push(Cmd::Line(lx, ly));
    push_cubic(
        path,
        xform,
        x + w - rx + kx,
        y,
        x + w,
        y + ry - ky,
        x + w,
        y + ry,
    );
    let (lx, ly) = xform.apply(x + w, y + h - ry);
    path.commands.push(Cmd::Line(lx, ly));
    push_cubic(
        path,
        xform,
        x + w,
        y + h - ry + ky,
        x + w - rx + kx,
        y + h,
        x + w - rx,
        y + h,
    );
    let (lx, ly) = xform.apply(x + rx, y + h);
    path.commands.push(Cmd::Line(lx, ly));
    push_cubic(
        path,
        xform,
        x + rx - kx,
        y + h,
        x,
        y + h - ry + ky,
        x,
        y + h - ry,
    );
    let (lx, ly) = xform.apply(x, y + ry);
    path.commands.push(Cmd::Line(lx, ly));
    push_cubic(path, xform, x, y + ry - ky, x + rx - kx, y, x + rx, y);
    path.commands.push(Cmd::Close);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_arc(
    path: &mut ParsedPath,
    xform: Affine,
    x1: f32,
    y1: f32,
    rx_in: f32,
    ry_in: f32,
    x_axis_rot_deg: f32,
    large_arc: bool,
    sweep: bool,
    x2: f32,
    y2: f32,
) {
    // SVG arc → 1-4 cubic bezier segments. Endpoint-to-center conversion
    // follows W3C Implementation Notes for SVG 1.1 §F.6.5.
    if rx_in == 0.0 || ry_in == 0.0 || (x1 == x2 && y1 == y2) {
        let (tx, ty) = xform.apply(x2, y2);
        path.commands.push(Cmd::Line(tx, ty));
        return;
    }
    let mut rx = rx_in.abs();
    let mut ry = ry_in.abs();
    let phi = x_axis_rot_deg.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();

    let dx = (x1 - x2) * 0.5;
    let dy = (y1 - y2) * 0.5;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;

    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }

    let rx_sq = rx * rx;
    let ry_sq = ry * ry;
    let x1p_sq = x1p * x1p;
    let y1p_sq = y1p * y1p;
    let mut radicand =
        (rx_sq * ry_sq - rx_sq * y1p_sq - ry_sq * x1p_sq) / (rx_sq * y1p_sq + ry_sq * x1p_sq);
    if radicand < 0.0 {
        radicand = 0.0;
    }
    let factor = radicand.sqrt() * if large_arc == sweep { -1.0 } else { 1.0 };
    let cxp = factor * (rx * y1p) / ry;
    let cyp = factor * -(ry * x1p) / rx;

    let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) * 0.5;
    let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) * 0.5;

    let theta1 = angle_between(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = angle_between(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    if !sweep && delta > 0.0 {
        delta -= 2.0 * core::f32::consts::PI;
    } else if sweep && delta < 0.0 {
        delta += 2.0 * core::f32::consts::PI;
    }

    let segs = ((delta.abs() / (core::f32::consts::PI / 2.0)).ceil()) as i32;
    let segs = segs.max(1);
    let seg_delta = delta / segs as f32;
    let t = (4.0 / 3.0) * (seg_delta / 4.0).tan();

    let mut theta = theta1;
    for _ in 0..segs {
        let theta_next = theta + seg_delta;
        let (sin_a, cos_a) = theta.sin_cos();
        let (sin_b, cos_b) = theta_next.sin_cos();
        let p1x = cos_phi * (rx * cos_a) - sin_phi * (ry * sin_a) + cx;
        let p1y = sin_phi * (rx * cos_a) + cos_phi * (ry * sin_a) + cy;
        let p4x = cos_phi * (rx * cos_b) - sin_phi * (ry * sin_b) + cx;
        let p4y = sin_phi * (rx * cos_b) + cos_phi * (ry * sin_b) + cy;
        let q1x = p1x + cos_phi * (-rx * sin_a) * t - sin_phi * (ry * cos_a) * t;
        let q1y = p1y + sin_phi * (-rx * sin_a) * t + cos_phi * (ry * cos_a) * t;
        let q2x = p4x - (cos_phi * (-rx * sin_b) * t - sin_phi * (ry * cos_b) * t);
        let q2y = p4y - (sin_phi * (-rx * sin_b) * t + cos_phi * (ry * cos_b) * t);
        push_cubic(path, xform, q1x, q1y, q2x, q2y, p4x, p4y);
        theta = theta_next;
    }
}

fn angle_between(ux: f32, uy: f32, vx: f32, vy: f32) -> f32 {
    let dot = ux * vx + uy * vy;
    let len = ((ux * ux + uy * uy) * (vx * vx + vy * vy)).sqrt();
    let cos_raw = if len == 0.0 { 1.0 } else { dot / len };
    let cos = cos_raw.clamp(-1.0, 1.0);
    let sign = if ux * vy - uy * vx < 0.0 { -1.0 } else { 1.0 };
    sign * cos.acos()
}

/// Apply an extra transform to an already-resolved [`ParsedPath`] (used by
/// `<use href="#id">` instantiation).
pub(super) fn retransform_path(src: &ParsedPath, xform: Affine) -> ParsedPath {
    let mut out = ParsedPath::default();
    out.commands.reserve(src.commands.len());
    for cmd in &src.commands {
        let mapped = match *cmd {
            Cmd::Move(x, y) => {
                let (a, b) = xform.apply(x, y);
                Cmd::Move(a, b)
            }
            Cmd::Line(x, y) => {
                let (a, b) = xform.apply(x, y);
                Cmd::Line(a, b)
            }
            Cmd::Cubic {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                let (a1, b1) = xform.apply(c1x, c1y);
                let (a2, b2) = xform.apply(c2x, c2y);
                let (a, b) = xform.apply(x, y);
                Cmd::Cubic {
                    c1x: a1,
                    c1y: b1,
                    c2x: a2,
                    c2y: b2,
                    x: a,
                    y: b,
                }
            }
            Cmd::Quad { c1x, c1y, x, y } => {
                let (a1, b1) = xform.apply(c1x, c1y);
                let (a, b) = xform.apply(x, y);
                Cmd::Quad {
                    c1x: a1,
                    c1y: b1,
                    x: a,
                    y: b,
                }
            }
            Cmd::Close => Cmd::Close,
        };
        out.commands.push(mapped);
    }
    out
}
