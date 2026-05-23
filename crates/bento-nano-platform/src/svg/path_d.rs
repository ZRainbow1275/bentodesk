//! SVG `<path d="...">` attribute parser.
//!
//! Covers the full SVG 1.1 path command set used by Lucide + plausible custom
//! icons: `M/m L/l H/h V/v C/c S/s Q/q T/t A/a Z/z` plus implicit-repeat
//! coordinates after a command letter.

use super::shapes::emit_arc;
use super::types::{Affine, Cmd, ParsedPath};
use super::util::{read_num, skip_num_seps};
use crate::errors::PlatformError;

pub(super) fn parse_path_d(
    d: &[u8],
    path: &mut ParsedPath,
    xform: Affine,
) -> Result<(), PlatformError> {
    let mut i = 0usize;
    let mut cur_x = 0.0f32;
    let mut cur_y = 0.0f32;
    let mut start_x = 0.0f32;
    let mut start_y = 0.0f32;
    let mut last_cmd = 0u8;
    let mut last_ctrl_x = 0.0f32;
    let mut last_ctrl_y = 0.0f32;
    let mut last_was_cubic = false;
    let mut last_was_quad = false;

    while i < d.len() {
        skip_num_seps(d, &mut i);
        if i >= d.len() {
            break;
        }
        let mut cmd = d[i];
        let is_cmd = cmd.is_ascii_alphabetic();
        if is_cmd {
            i += 1;
        } else {
            cmd = match last_cmd {
                b'M' => b'L',
                b'm' => b'l',
                0 => return Err(PlatformError::Svg("path 'd' must start with M/m")),
                other => other,
            };
        }
        let relative = cmd.is_ascii_lowercase();

        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (x, y, ni) = read_point(d, i, cur_x, cur_y, relative)?;
                i = ni;
                cur_x = x;
                cur_y = y;
                start_x = x;
                start_y = y;
                let (tx, ty) = xform.apply(x, y);
                path.commands.push(Cmd::Move(tx, ty));
                last_was_cubic = false;
                last_was_quad = false;
            }
            b'L' => {
                let (x, y, ni) = read_point(d, i, cur_x, cur_y, relative)?;
                i = ni;
                cur_x = x;
                cur_y = y;
                let (tx, ty) = xform.apply(x, y);
                path.commands.push(Cmd::Line(tx, ty));
                last_was_cubic = false;
                last_was_quad = false;
            }
            b'H' => {
                let (n, ni) = read_num(d, i)?;
                i = ni;
                cur_x = if relative { cur_x + n } else { n };
                let (tx, ty) = xform.apply(cur_x, cur_y);
                path.commands.push(Cmd::Line(tx, ty));
                last_was_cubic = false;
                last_was_quad = false;
            }
            b'V' => {
                let (n, ni) = read_num(d, i)?;
                i = ni;
                cur_y = if relative { cur_y + n } else { n };
                let (tx, ty) = xform.apply(cur_x, cur_y);
                path.commands.push(Cmd::Line(tx, ty));
                last_was_cubic = false;
                last_was_quad = false;
            }
            b'C' => {
                let (x1, y1, ni) = read_point(d, i, cur_x, cur_y, relative)?;
                i = ni;
                let (x2, y2, ni) = read_point(d, i, cur_x, cur_y, relative)?;
                i = ni;
                let (x, y, ni) = read_point(d, i, cur_x, cur_y, relative)?;
                i = ni;
                push_cubic(path, xform, x1, y1, x2, y2, x, y);
                last_ctrl_x = x2;
                last_ctrl_y = y2;
                cur_x = x;
                cur_y = y;
                last_was_cubic = true;
                last_was_quad = false;
            }
            b'S' => {
                let (x1, y1) = if last_was_cubic {
                    (2.0 * cur_x - last_ctrl_x, 2.0 * cur_y - last_ctrl_y)
                } else {
                    (cur_x, cur_y)
                };
                let (x2, y2, ni) = read_point(d, i, cur_x, cur_y, relative)?;
                i = ni;
                let (x, y, ni) = read_point(d, i, cur_x, cur_y, relative)?;
                i = ni;
                push_cubic(path, xform, x1, y1, x2, y2, x, y);
                last_ctrl_x = x2;
                last_ctrl_y = y2;
                cur_x = x;
                cur_y = y;
                last_was_cubic = true;
                last_was_quad = false;
            }
            b'Q' => {
                let (x1, y1, ni) = read_point(d, i, cur_x, cur_y, relative)?;
                i = ni;
                let (x, y, ni) = read_point(d, i, cur_x, cur_y, relative)?;
                i = ni;
                push_quad(path, xform, x1, y1, x, y);
                last_ctrl_x = x1;
                last_ctrl_y = y1;
                cur_x = x;
                cur_y = y;
                last_was_cubic = false;
                last_was_quad = true;
            }
            b'T' => {
                let (x1, y1) = if last_was_quad {
                    (2.0 * cur_x - last_ctrl_x, 2.0 * cur_y - last_ctrl_y)
                } else {
                    (cur_x, cur_y)
                };
                let (x, y, ni) = read_point(d, i, cur_x, cur_y, relative)?;
                i = ni;
                push_quad(path, xform, x1, y1, x, y);
                last_ctrl_x = x1;
                last_ctrl_y = y1;
                cur_x = x;
                cur_y = y;
                last_was_cubic = false;
                last_was_quad = true;
            }
            b'A' => {
                let (rx, ni) = read_num(d, i)?;
                i = ni;
                let (ry, ni) = read_num(d, i)?;
                i = ni;
                let (x_axis_rot_deg, ni) = read_num(d, i)?;
                i = ni;
                let (large_arc, ni) = read_flag(d, i)?;
                i = ni;
                let (sweep, ni) = read_flag(d, i)?;
                i = ni;
                let (x, y, ni) = read_point(d, i, cur_x, cur_y, relative)?;
                i = ni;
                emit_arc(
                    path,
                    xform,
                    cur_x,
                    cur_y,
                    rx,
                    ry,
                    x_axis_rot_deg,
                    large_arc,
                    sweep,
                    x,
                    y,
                );
                cur_x = x;
                cur_y = y;
                last_was_cubic = false;
                last_was_quad = false;
            }
            b'Z' => {
                cur_x = start_x;
                cur_y = start_y;
                path.commands.push(Cmd::Close);
                last_was_cubic = false;
                last_was_quad = false;
            }
            _ => return Err(PlatformError::Svg("unsupported path command")),
        }
        last_cmd = cmd;
    }
    Ok(())
}

fn read_point(
    bytes: &[u8],
    i: usize,
    cur_x: f32,
    cur_y: f32,
    relative: bool,
) -> Result<(f32, f32, usize), PlatformError> {
    let (x, ni) = read_num(bytes, i)?;
    let (y, ni) = read_num(bytes, ni)?;
    let (fx, fy) = if relative {
        (cur_x + x, cur_y + y)
    } else {
        (x, y)
    };
    Ok((fx, fy, ni))
}

fn read_flag(bytes: &[u8], mut i: usize) -> Result<(bool, usize), PlatformError> {
    skip_num_seps(bytes, &mut i);
    if i >= bytes.len() {
        return Err(PlatformError::Svg("expected arc flag"));
    }
    let v = match bytes[i] {
        b'0' => false,
        b'1' => true,
        _ => return Err(PlatformError::Svg("arc flag not '0' or '1'")),
    };
    Ok((v, i + 1))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_cubic(
    path: &mut ParsedPath,
    xform: Affine,
    c1x: f32,
    c1y: f32,
    c2x: f32,
    c2y: f32,
    x: f32,
    y: f32,
) {
    let (tc1x, tc1y) = xform.apply(c1x, c1y);
    let (tc2x, tc2y) = xform.apply(c2x, c2y);
    let (tx, ty) = xform.apply(x, y);
    path.commands.push(Cmd::Cubic {
        c1x: tc1x,
        c1y: tc1y,
        c2x: tc2x,
        c2y: tc2y,
        x: tx,
        y: ty,
    });
}

pub(super) fn push_quad(path: &mut ParsedPath, xform: Affine, c1x: f32, c1y: f32, x: f32, y: f32) {
    let (tc1x, tc1y) = xform.apply(c1x, c1y);
    let (tx, ty) = xform.apply(x, y);
    path.commands.push(Cmd::Quad {
        c1x: tc1x,
        c1y: tc1y,
        x: tx,
        y: ty,
    });
}
