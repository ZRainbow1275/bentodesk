//! XML walker — recursive-descent over the SVG subset Lucide emits plus the
//! constructs Lucide could plausibly grow into (`<defs>` / `<use>` /
//! `<linearGradient>` / `<g transform>`).

use smallvec::SmallVec;
use smol_str::SmolStr;
use std::collections::HashMap;

use super::path_d::parse_path_d;
use super::shapes::{emit_circle, emit_rect, retransform_path};
use super::types::{
    Affine, Cmd, DefinedElement, GradientStop, LinearGradient, Parsed, ParsedPath, ViewBox,
};
use super::util::{
    bytes_to_str, parse_color, parse_dimension, parse_f32, parse_offset, parse_transform,
    parse_viewbox, read_num, skip_num_seps,
};
use crate::errors::PlatformError;

type AttrList<'a> = SmallVec<[(&'a [u8], &'a [u8]); 8]>;

pub(super) struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    pub(super) fn new(src: &'a str) -> Self {
        Self {
            src: src.as_bytes(),
            pos: 0,
        }
    }

    pub(super) fn parse_document(&mut self) -> Result<Parsed, PlatformError> {
        let mut out = Parsed {
            paths: SmallVec::new(),
            viewbox: ViewBox::default(),
            defs: HashMap::new(),
            gradients: HashMap::new(),
        };

        loop {
            self.skip_trivia();
            if self.pos >= self.src.len() {
                return Err(PlatformError::Svg("no <svg> root element"));
            }
            if !self.consume_byte(b'<') {
                return Err(PlatformError::Svg("expected '<' at top level"));
            }
            if self.try_skip_special()? {
                continue;
            }
            let tag = self.read_name();
            if tag == b"svg" {
                self.parse_svg(&mut out, Affine::IDENTITY)?;
                return Ok(out);
            }
            return Err(PlatformError::Svg("root element is not <svg>"));
        }
    }

    fn parse_svg(&mut self, out: &mut Parsed, parent_xform: Affine) -> Result<(), PlatformError> {
        let attrs = self.read_attrs()?;
        for (k, v) in &attrs {
            if k == b"viewBox" {
                out.viewbox = parse_viewbox(v)?;
            } else if k == b"width"
                && out.viewbox == ViewBox::default()
                && let Some(w) = parse_dimension(v)
            {
                out.viewbox.width = w;
            } else if k == b"height"
                && out.viewbox == ViewBox::default()
                && let Some(h) = parse_dimension(v)
            {
                out.viewbox.height = h;
            }
        }
        if self.consume_byte(b'/') {
            if !self.consume_byte(b'>') {
                return Err(PlatformError::Svg("malformed self-closing <svg/>"));
            }
            return Ok(());
        }
        if !self.consume_byte(b'>') {
            return Err(PlatformError::Svg("missing '>' on <svg> open tag"));
        }
        self.parse_children(out, parent_xform, b"svg")?;
        Ok(())
    }

    fn parse_children(
        &mut self,
        out: &mut Parsed,
        parent_xform: Affine,
        end_tag: &[u8],
    ) -> Result<(), PlatformError> {
        loop {
            self.skip_trivia();
            if self.pos >= self.src.len() {
                return Err(PlatformError::Svg("unexpected end of input inside element"));
            }
            if !self.consume_byte(b'<') {
                while self.pos < self.src.len() && self.src[self.pos] != b'<' {
                    self.pos += 1;
                }
                continue;
            }
            if self.try_skip_special()? {
                continue;
            }
            if self.consume_byte(b'/') {
                let close = self.read_name();
                self.skip_trivia();
                if !self.consume_byte(b'>') {
                    return Err(PlatformError::Svg("malformed close tag"));
                }
                if close != end_tag {
                    return Err(PlatformError::Svg("mismatched close tag"));
                }
                return Ok(());
            }
            let name = self.read_name().to_vec();
            self.dispatch_element(&name, out, parent_xform)?;
        }
    }

    fn dispatch_element(
        &mut self,
        name: &[u8],
        out: &mut Parsed,
        parent_xform: Affine,
    ) -> Result<(), PlatformError> {
        match name {
            b"path" => self.parse_path(out, parent_xform, false),
            b"circle" => self.parse_circle(out, parent_xform, false),
            b"rect" => self.parse_rect(out, parent_xform, false),
            b"line" => self.parse_line(out, parent_xform, false),
            b"polyline" => self.parse_polyline(out, parent_xform, false, false),
            b"polygon" => self.parse_polyline(out, parent_xform, false, true),
            b"g" => self.parse_group(out, parent_xform),
            b"defs" => self.parse_defs(out),
            b"linearGradient" => self.parse_linear_gradient(out, false),
            b"use" => self.parse_use(out, parent_xform),
            _ => self.skip_element(name),
        }
    }

    fn parse_group(&mut self, out: &mut Parsed, parent_xform: Affine) -> Result<(), PlatformError> {
        let attrs = self.read_attrs()?;
        let mut local = Affine::IDENTITY;
        for (k, v) in &attrs {
            if k == b"transform" {
                local = parse_transform(v)?;
            }
        }
        let combined = parent_xform.compose(local);
        if self.consume_byte(b'/') {
            if !self.consume_byte(b'>') {
                return Err(PlatformError::Svg("malformed self-closing <g/>"));
            }
            return Ok(());
        }
        if !self.consume_byte(b'>') {
            return Err(PlatformError::Svg("missing '>' on <g> open tag"));
        }
        self.parse_children(out, combined, b"g")
    }

    fn parse_defs(&mut self, out: &mut Parsed) -> Result<(), PlatformError> {
        let _attrs = self.read_attrs()?;
        if self.consume_byte(b'/') {
            if !self.consume_byte(b'>') {
                return Err(PlatformError::Svg("malformed self-closing <defs/>"));
            }
            return Ok(());
        }
        if !self.consume_byte(b'>') {
            return Err(PlatformError::Svg("missing '>' on <defs> open tag"));
        }
        loop {
            self.skip_trivia();
            if self.pos >= self.src.len() {
                return Err(PlatformError::Svg("unterminated <defs>"));
            }
            if !self.consume_byte(b'<') {
                while self.pos < self.src.len() && self.src[self.pos] != b'<' {
                    self.pos += 1;
                }
                continue;
            }
            if self.try_skip_special()? {
                continue;
            }
            if self.consume_byte(b'/') {
                let close = self.read_name();
                self.skip_trivia();
                if !self.consume_byte(b'>') {
                    return Err(PlatformError::Svg("malformed close tag in <defs>"));
                }
                if close != b"defs" {
                    return Err(PlatformError::Svg("mismatched close inside <defs>"));
                }
                return Ok(());
            }
            let name = self.read_name().to_vec();
            match name.as_slice() {
                b"path" => self.parse_path(out, Affine::IDENTITY, true)?,
                b"circle" => self.parse_circle(out, Affine::IDENTITY, true)?,
                b"rect" => self.parse_rect(out, Affine::IDENTITY, true)?,
                b"linearGradient" => self.parse_linear_gradient(out, true)?,
                _ => self.skip_element(&name)?,
            }
        }
    }

    fn parse_path(
        &mut self,
        out: &mut Parsed,
        parent_xform: Affine,
        in_defs: bool,
    ) -> Result<(), PlatformError> {
        let attrs = self.read_attrs()?;
        let mut id: Option<SmolStr> = None;
        let mut d: Option<&[u8]> = None;
        let mut local = Affine::IDENTITY;
        for (k, v) in &attrs {
            match *k {
                b"d" => d = Some(v),
                b"id" => id = Some(SmolStr::new(bytes_to_str(v)?)),
                b"transform" => local = parse_transform(v)?,
                _ => {}
            }
        }
        self.consume_self_close_or_open_close()?;
        let Some(d_bytes) = d else {
            return Ok(());
        };
        let xform = parent_xform.compose(local);
        let mut path = ParsedPath::default();
        parse_path_d(d_bytes, &mut path, xform)?;
        place_or_define(out, in_defs, id, path);
        Ok(())
    }

    fn parse_circle(
        &mut self,
        out: &mut Parsed,
        parent_xform: Affine,
        in_defs: bool,
    ) -> Result<(), PlatformError> {
        let attrs = self.read_attrs()?;
        let mut cx = 0.0f32;
        let mut cy = 0.0f32;
        let mut r = 0.0f32;
        let mut id: Option<SmolStr> = None;
        let mut local = Affine::IDENTITY;
        for (k, v) in &attrs {
            match *k {
                b"cx" => cx = parse_f32(v)?,
                b"cy" => cy = parse_f32(v)?,
                b"r" => r = parse_f32(v)?,
                b"id" => id = Some(SmolStr::new(bytes_to_str(v)?)),
                b"transform" => local = parse_transform(v)?,
                _ => {}
            }
        }
        self.consume_self_close_or_open_close()?;
        let xform = parent_xform.compose(local);
        let mut path = ParsedPath::default();
        emit_circle(&mut path, cx, cy, r, xform);
        place_or_define(out, in_defs, id, path);
        Ok(())
    }

    fn parse_rect(
        &mut self,
        out: &mut Parsed,
        parent_xform: Affine,
        in_defs: bool,
    ) -> Result<(), PlatformError> {
        let attrs = self.read_attrs()?;
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        let mut w = 0.0f32;
        let mut h = 0.0f32;
        let mut rx = 0.0f32;
        let mut ry = 0.0f32;
        let mut id: Option<SmolStr> = None;
        let mut local = Affine::IDENTITY;
        for (k, v) in &attrs {
            match *k {
                b"x" => x = parse_f32(v)?,
                b"y" => y = parse_f32(v)?,
                b"width" => w = parse_f32(v)?,
                b"height" => h = parse_f32(v)?,
                b"rx" => rx = parse_f32(v)?,
                b"ry" => ry = parse_f32(v)?,
                b"id" => id = Some(SmolStr::new(bytes_to_str(v)?)),
                b"transform" => local = parse_transform(v)?,
                _ => {}
            }
        }
        self.consume_self_close_or_open_close()?;
        if w <= 0.0 || h <= 0.0 {
            return Ok(());
        }
        if rx == 0.0 && ry > 0.0 {
            rx = ry;
        }
        if ry == 0.0 && rx > 0.0 {
            ry = rx;
        }
        let xform = parent_xform.compose(local);
        let mut path = ParsedPath::default();
        emit_rect(&mut path, x, y, w, h, rx, ry, xform);
        place_or_define(out, in_defs, id, path);
        Ok(())
    }

    fn parse_line(
        &mut self,
        out: &mut Parsed,
        parent_xform: Affine,
        in_defs: bool,
    ) -> Result<(), PlatformError> {
        let attrs = self.read_attrs()?;
        let mut x1 = 0.0f32;
        let mut y1 = 0.0f32;
        let mut x2 = 0.0f32;
        let mut y2 = 0.0f32;
        let mut id: Option<SmolStr> = None;
        let mut local = Affine::IDENTITY;
        for (k, v) in &attrs {
            match *k {
                b"x1" => x1 = parse_f32(v)?,
                b"y1" => y1 = parse_f32(v)?,
                b"x2" => x2 = parse_f32(v)?,
                b"y2" => y2 = parse_f32(v)?,
                b"id" => id = Some(SmolStr::new(bytes_to_str(v)?)),
                b"transform" => local = parse_transform(v)?,
                _ => {}
            }
        }
        self.consume_self_close_or_open_close()?;
        let xform = parent_xform.compose(local);
        let mut path = ParsedPath::default();
        let (ax, ay) = xform.apply(x1, y1);
        let (bx, by) = xform.apply(x2, y2);
        path.commands.push(Cmd::Move(ax, ay));
        path.commands.push(Cmd::Line(bx, by));
        place_or_define(out, in_defs, id, path);
        Ok(())
    }

    fn parse_polyline(
        &mut self,
        out: &mut Parsed,
        parent_xform: Affine,
        in_defs: bool,
        close: bool,
    ) -> Result<(), PlatformError> {
        let attrs = self.read_attrs()?;
        let mut points: Option<&[u8]> = None;
        let mut id: Option<SmolStr> = None;
        let mut local = Affine::IDENTITY;
        for (k, v) in &attrs {
            match *k {
                b"points" => points = Some(v),
                b"id" => id = Some(SmolStr::new(bytes_to_str(v)?)),
                b"transform" => local = parse_transform(v)?,
                _ => {}
            }
        }
        self.consume_self_close_or_open_close()?;
        let Some(pts) = points else {
            return Ok(());
        };
        let xform = parent_xform.compose(local);
        let mut path = ParsedPath::default();
        let mut i = 0;
        let mut first = true;
        while let Ok((x, ni)) = read_num(pts, i) {
            i = ni;
            skip_num_seps(pts, &mut i);
            let (y, ni) = read_num(pts, i)?;
            i = ni;
            let (tx, ty) = xform.apply(x, y);
            if first {
                path.commands.push(Cmd::Move(tx, ty));
                first = false;
            } else {
                path.commands.push(Cmd::Line(tx, ty));
            }
            skip_num_seps(pts, &mut i);
            if i >= pts.len() {
                break;
            }
        }
        if close && !first {
            path.commands.push(Cmd::Close);
        }
        place_or_define(out, in_defs, id, path);
        Ok(())
    }

    fn parse_use(&mut self, out: &mut Parsed, parent_xform: Affine) -> Result<(), PlatformError> {
        let attrs = self.read_attrs()?;
        let mut href: Option<SmolStr> = None;
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        let mut local = Affine::IDENTITY;
        for (k, v) in &attrs {
            match *k {
                b"href" | b"xlink:href" => {
                    let s = bytes_to_str(v)?;
                    let id = s.strip_prefix('#').unwrap_or(s);
                    href = Some(SmolStr::new(id));
                }
                b"x" => x = parse_f32(v)?,
                b"y" => y = parse_f32(v)?,
                b"transform" => local = parse_transform(v)?,
                _ => {}
            }
        }
        self.consume_self_close_or_open_close()?;
        let Some(id) = href else {
            return Ok(());
        };
        let translate = Affine {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: x,
            f: y,
        };
        let xform = parent_xform.compose(local).compose(translate);
        let Some(def) = out.defs.get(&id).cloned() else {
            // <use> referencing an unknown id is silently ignored, matching
            // browser behaviour.
            return Ok(());
        };
        match def {
            DefinedElement::Path(path) => {
                if xform.is_identity() {
                    out.paths.push(path);
                } else {
                    out.paths.push(retransform_path(&path, xform));
                }
            }
        }
        Ok(())
    }

    fn parse_linear_gradient(
        &mut self,
        out: &mut Parsed,
        in_defs: bool,
    ) -> Result<(), PlatformError> {
        let attrs = self.read_attrs()?;
        let mut id: Option<SmolStr> = None;
        let mut transform = Affine::IDENTITY;
        for (k, v) in &attrs {
            match *k {
                b"id" => id = Some(SmolStr::new(bytes_to_str(v)?)),
                b"gradientTransform" => transform = parse_transform(v)?,
                _ => {}
            }
        }
        let mut grad = LinearGradient {
            stops: SmallVec::new(),
            transform,
        };
        if self.consume_byte(b'/') {
            if !self.consume_byte(b'>') {
                return Err(PlatformError::Svg(
                    "malformed self-closing <linearGradient/>",
                ));
            }
        } else {
            if !self.consume_byte(b'>') {
                return Err(PlatformError::Svg("missing '>' on <linearGradient>"));
            }
            loop {
                self.skip_trivia();
                if self.pos >= self.src.len() {
                    return Err(PlatformError::Svg("unterminated <linearGradient>"));
                }
                if !self.consume_byte(b'<') {
                    while self.pos < self.src.len() && self.src[self.pos] != b'<' {
                        self.pos += 1;
                    }
                    continue;
                }
                if self.try_skip_special()? {
                    continue;
                }
                if self.consume_byte(b'/') {
                    let close = self.read_name();
                    self.skip_trivia();
                    if !self.consume_byte(b'>') {
                        return Err(PlatformError::Svg("malformed close tag in linearGradient"));
                    }
                    if close != b"linearGradient" {
                        return Err(PlatformError::Svg("mismatched close inside linearGradient"));
                    }
                    break;
                }
                let name = self.read_name().to_vec();
                if name == b"stop" {
                    let stop_attrs = self.read_attrs()?;
                    let mut offset = 0.0f32;
                    let mut rgba = [0u8, 0u8, 0u8, 0xFFu8];
                    for (k, v) in &stop_attrs {
                        match *k {
                            b"offset" => offset = parse_offset(v)?,
                            b"stop-color" => {
                                let (r, g, b) = parse_color(v)?;
                                rgba[0] = r;
                                rgba[1] = g;
                                rgba[2] = b;
                            }
                            b"stop-opacity" => {
                                let a = parse_f32(v)?.clamp(0.0, 1.0);
                                rgba[3] = (a * 255.0) as u8;
                            }
                            _ => {}
                        }
                    }
                    self.consume_self_close_or_open_close()?;
                    grad.stops.push(GradientStop { offset, rgba });
                } else {
                    self.skip_element(&name)?;
                }
            }
        }
        if let Some(id) = id {
            out.gradients.insert(id, grad);
        }
        let _ = in_defs; // gradients are stored the same way regardless of <defs>
        Ok(())
    }

    fn skip_element(&mut self, name: &[u8]) -> Result<(), PlatformError> {
        let _attrs = self.read_attrs()?;
        if self.consume_byte(b'/') {
            if !self.consume_byte(b'>') {
                return Err(PlatformError::Svg("malformed self-close in skip"));
            }
            return Ok(());
        }
        if !self.consume_byte(b'>') {
            return Err(PlatformError::Svg("missing '>' on element being skipped"));
        }
        let mut depth = 1usize;
        while depth > 0 {
            self.skip_trivia();
            if self.pos >= self.src.len() {
                return Err(PlatformError::Svg("unterminated element while skipping"));
            }
            if !self.consume_byte(b'<') {
                while self.pos < self.src.len() && self.src[self.pos] != b'<' {
                    self.pos += 1;
                }
                continue;
            }
            if self.try_skip_special()? {
                continue;
            }
            if self.consume_byte(b'/') {
                let close = self.read_name();
                self.skip_trivia();
                if !self.consume_byte(b'>') {
                    return Err(PlatformError::Svg("malformed close while skipping"));
                }
                if close == name {
                    depth -= 1;
                }
                continue;
            }
            let inner = self.read_name().to_vec();
            let _ = self.read_attrs()?;
            if self.consume_byte(b'/') {
                if !self.consume_byte(b'>') {
                    return Err(PlatformError::Svg("malformed self-close while skipping"));
                }
            } else {
                if !self.consume_byte(b'>') {
                    return Err(PlatformError::Svg("missing '>' while skipping"));
                }
                if inner == name {
                    depth += 1;
                }
            }
        }
        Ok(())
    }

    fn consume_self_close_or_open_close(&mut self) -> Result<(), PlatformError> {
        if self.consume_byte(b'/') {
            if !self.consume_byte(b'>') {
                return Err(PlatformError::Svg("malformed self-closing element"));
            }
            return Ok(());
        }
        if !self.consume_byte(b'>') {
            return Err(PlatformError::Svg("missing '>' on element"));
        }
        loop {
            self.skip_trivia();
            if self.pos >= self.src.len() {
                return Err(PlatformError::Svg("unterminated leaf element"));
            }
            if !self.consume_byte(b'<') {
                while self.pos < self.src.len() && self.src[self.pos] != b'<' {
                    self.pos += 1;
                }
                continue;
            }
            if self.try_skip_special()? {
                continue;
            }
            if self.consume_byte(b'/') {
                let _ = self.read_name();
                self.skip_trivia();
                if !self.consume_byte(b'>') {
                    return Err(PlatformError::Svg("malformed leaf close"));
                }
                return Ok(());
            }
            let inner = self.read_name().to_vec();
            self.skip_element(&inner)?;
        }
    }

    fn read_attrs(&mut self) -> Result<AttrList<'a>, PlatformError> {
        let mut out: AttrList<'a> = SmallVec::new();
        loop {
            self.skip_trivia();
            if self.pos >= self.src.len() {
                return Err(PlatformError::Svg("unexpected EOF in attribute list"));
            }
            let b = self.src[self.pos];
            if b == b'/' || b == b'>' {
                return Ok(out);
            }
            let name_start = self.pos;
            while self.pos < self.src.len() {
                let c = self.src[self.pos];
                if c == b'=' || c.is_ascii_whitespace() || c == b'/' || c == b'>' {
                    break;
                }
                self.pos += 1;
            }
            let name = &self.src[name_start..self.pos];
            self.skip_trivia();
            if !self.consume_byte(b'=') {
                out.push((name, &[]));
                continue;
            }
            self.skip_trivia();
            if self.pos >= self.src.len() {
                return Err(PlatformError::Svg("EOF after '='"));
            }
            let quote = self.src[self.pos];
            if quote != b'"' && quote != b'\'' {
                return Err(PlatformError::Svg("attribute value not quoted"));
            }
            self.pos += 1;
            let val_start = self.pos;
            while self.pos < self.src.len() && self.src[self.pos] != quote {
                self.pos += 1;
            }
            if self.pos >= self.src.len() {
                return Err(PlatformError::Svg("unterminated attribute value"));
            }
            let value = &self.src[val_start..self.pos];
            self.pos += 1;
            out.push((name, value));
        }
    }

    fn read_name(&mut self) -> &'a [u8] {
        let start = self.pos;
        while self.pos < self.src.len() {
            let b = self.src[self.pos];
            if b.is_ascii_whitespace() || b == b'/' || b == b'>' || b == b'=' {
                break;
            }
            self.pos += 1;
        }
        &self.src[start..self.pos]
    }

    fn consume_byte(&mut self, b: u8) -> bool {
        if self.pos < self.src.len() && self.src[self.pos] == b {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn skip_trivia(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn try_skip_special(&mut self) -> Result<bool, PlatformError> {
        if self.pos < self.src.len() && self.src[self.pos] == b'!' {
            if self.src[self.pos..].starts_with(b"!--") {
                self.pos += 3;
                while self.pos + 2 < self.src.len() && &self.src[self.pos..self.pos + 3] != b"-->" {
                    self.pos += 1;
                }
                if self.pos + 2 >= self.src.len() {
                    return Err(PlatformError::Svg("unterminated comment"));
                }
                self.pos += 3;
                return Ok(true);
            }
            while self.pos < self.src.len() && self.src[self.pos] != b'>' {
                self.pos += 1;
            }
            if self.pos >= self.src.len() {
                return Err(PlatformError::Svg("unterminated <! ... >"));
            }
            self.pos += 1;
            return Ok(true);
        }
        if self.pos < self.src.len() && self.src[self.pos] == b'?' {
            self.pos += 1;
            while self.pos + 1 < self.src.len() && &self.src[self.pos..self.pos + 2] != b"?>" {
                self.pos += 1;
            }
            if self.pos + 1 >= self.src.len() {
                return Err(PlatformError::Svg("unterminated processing instruction"));
            }
            self.pos += 2;
            return Ok(true);
        }
        Ok(false)
    }
}

fn place_or_define(out: &mut Parsed, in_defs: bool, id: Option<SmolStr>, path: ParsedPath) {
    if in_defs {
        if let Some(id) = id {
            out.defs.insert(id, DefinedElement::Path(path));
        }
    } else {
        out.paths.push(path);
    }
}
