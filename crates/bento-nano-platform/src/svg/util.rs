//! Number / color / dimension / transform / viewBox parsers shared across the
//! SVG submodules.

use smallvec::SmallVec;

use super::types::{Affine, ViewBox};
use crate::errors::PlatformError;

pub(super) fn bytes_to_str(b: &[u8]) -> Result<&str, PlatformError> {
    std::str::from_utf8(b).map_err(|_| PlatformError::Svg("non-utf8 attribute"))
}

pub(super) fn parse_dimension(v: &[u8]) -> Option<f32> {
    let s = std::str::from_utf8(v).ok()?;
    // Strip a trailing CSS unit (`px`, `pt`, etc) — Lucide always uses bare numbers.
    let trimmed = s.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%');
    trimmed.trim().parse().ok()
}

pub(super) fn parse_f32(v: &[u8]) -> Result<f32, PlatformError> {
    let s = bytes_to_str(v)?.trim();
    s.parse()
        .map_err(|_| PlatformError::Svg("invalid f32 attribute"))
}

pub(super) fn parse_offset(v: &[u8]) -> Result<f32, PlatformError> {
    let s = bytes_to_str(v)?.trim();
    if let Some(stripped) = s.strip_suffix('%') {
        let n: f32 = stripped
            .trim()
            .parse()
            .map_err(|_| PlatformError::Svg("invalid percentage offset"))?;
        Ok((n / 100.0).clamp(0.0, 1.0))
    } else {
        let n: f32 = s
            .parse()
            .map_err(|_| PlatformError::Svg("invalid offset"))?;
        Ok(n.clamp(0.0, 1.0))
    }
}

pub(super) fn parse_viewbox(v: &[u8]) -> Result<ViewBox, PlatformError> {
    let mut i = 0;
    skip_num_seps(v, &mut i);
    let (min_x, ni) = read_num(v, i)?;
    i = ni;
    skip_num_seps(v, &mut i);
    let (min_y, ni) = read_num(v, i)?;
    i = ni;
    skip_num_seps(v, &mut i);
    let (width, ni) = read_num(v, i)?;
    i = ni;
    skip_num_seps(v, &mut i);
    let (height, _) = read_num(v, i)?;
    Ok(ViewBox {
        min_x,
        min_y,
        width,
        height,
    })
}

pub(super) fn parse_color(v: &[u8]) -> Result<(u8, u8, u8), PlatformError> {
    let s = bytes_to_str(v)?.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let bytes = hex.as_bytes();
        match bytes.len() {
            3 => {
                let r = hex4(bytes[0])? * 17;
                let g = hex4(bytes[1])? * 17;
                let b = hex4(bytes[2])? * 17;
                Ok((r, g, b))
            }
            6 => {
                let r = hex4(bytes[0])? * 16 + hex4(bytes[1])?;
                let g = hex4(bytes[2])? * 16 + hex4(bytes[3])?;
                let b = hex4(bytes[4])? * 16 + hex4(bytes[5])?;
                Ok((r, g, b))
            }
            _ => Err(PlatformError::Svg("unsupported hex color length")),
        }
    } else if s.eq_ignore_ascii_case("currentColor") || s == "none" {
        Ok((0, 0, 0))
    } else {
        match s.to_ascii_lowercase().as_str() {
            "black" => Ok((0, 0, 0)),
            "white" => Ok((255, 255, 255)),
            "red" => Ok((255, 0, 0)),
            "green" => Ok((0, 128, 0)),
            "blue" => Ok((0, 0, 255)),
            "transparent" => Ok((0, 0, 0)),
            _ => Err(PlatformError::Svg("unknown color name")),
        }
    }
}

fn hex4(b: u8) -> Result<u8, PlatformError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(PlatformError::Svg("invalid hex digit")),
    }
}

pub(super) fn parse_transform(v: &[u8]) -> Result<Affine, PlatformError> {
    let s = bytes_to_str(v)?;
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut acc = Affine::IDENTITY;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let name_start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        let name = &bytes[name_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'(' {
            return Err(PlatformError::Svg("transform missing '('"));
        }
        i += 1;
        let mut nums: SmallVec<[f32; 6]> = SmallVec::new();
        loop {
            skip_num_seps(bytes, &mut i);
            if i >= bytes.len() || bytes[i] == b')' {
                break;
            }
            let (n, ni) = read_num(bytes, i)?;
            nums.push(n);
            i = ni;
        }
        if i >= bytes.len() || bytes[i] != b')' {
            return Err(PlatformError::Svg("transform missing ')'"));
        }
        i += 1;
        let local = match name {
            b"matrix" if nums.len() == 6 => Affine {
                a: nums[0],
                b: nums[1],
                c: nums[2],
                d: nums[3],
                e: nums[4],
                f: nums[5],
            },
            b"translate" => {
                let tx = nums.first().copied().unwrap_or(0.0);
                let ty = nums.get(1).copied().unwrap_or(0.0);
                Affine {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    e: tx,
                    f: ty,
                }
            }
            b"scale" => {
                let sx = nums.first().copied().unwrap_or(1.0);
                let sy = nums.get(1).copied().unwrap_or(sx);
                Affine {
                    a: sx,
                    b: 0.0,
                    c: 0.0,
                    d: sy,
                    e: 0.0,
                    f: 0.0,
                }
            }
            b"rotate" => {
                let deg = nums.first().copied().unwrap_or(0.0);
                let rad = deg.to_radians();
                let (s, c) = rad.sin_cos();
                if nums.len() == 3 {
                    let cx = nums[1];
                    let cy = nums[2];
                    let t1 = Affine {
                        a: 1.0,
                        b: 0.0,
                        c: 0.0,
                        d: 1.0,
                        e: cx,
                        f: cy,
                    };
                    let r = Affine {
                        a: c,
                        b: s,
                        c: -s,
                        d: c,
                        e: 0.0,
                        f: 0.0,
                    };
                    let t2 = Affine {
                        a: 1.0,
                        b: 0.0,
                        c: 0.0,
                        d: 1.0,
                        e: -cx,
                        f: -cy,
                    };
                    t1.compose(r).compose(t2)
                } else {
                    Affine {
                        a: c,
                        b: s,
                        c: -s,
                        d: c,
                        e: 0.0,
                        f: 0.0,
                    }
                }
            }
            b"skewX" => {
                let deg = nums.first().copied().unwrap_or(0.0);
                let t = deg.to_radians().tan();
                Affine {
                    a: 1.0,
                    b: 0.0,
                    c: t,
                    d: 1.0,
                    e: 0.0,
                    f: 0.0,
                }
            }
            b"skewY" => {
                let deg = nums.first().copied().unwrap_or(0.0);
                let t = deg.to_radians().tan();
                Affine {
                    a: 1.0,
                    b: t,
                    c: 0.0,
                    d: 1.0,
                    e: 0.0,
                    f: 0.0,
                }
            }
            _ => return Err(PlatformError::Svg("unknown transform op")),
        };
        acc = acc.compose(local);
    }
    Ok(acc)
}

pub(super) fn skip_num_seps(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() && matches!(bytes[*i], b' ' | b'\t' | b'\n' | b'\r' | b',') {
        *i += 1;
    }
}

pub(super) fn read_num(bytes: &[u8], mut i: usize) -> Result<(f32, usize), PlatformError> {
    skip_num_seps(bytes, &mut i);
    let start = i;
    if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
        i += 1;
    }
    let mut saw_digit = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        saw_digit = true;
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            saw_digit = true;
            i += 1;
        }
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
            i += 1;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if !saw_digit {
        return Err(PlatformError::Svg("expected number"));
    }
    let s = std::str::from_utf8(&bytes[start..i])
        .map_err(|_| PlatformError::Svg("non-utf8 in number"))?;
    let v: f32 = s
        .parse()
        .map_err(|_| PlatformError::Svg("invalid f32 literal"))?;
    Ok((v, i))
}
