//! The document: a canvas size and a stack of layers (bottom first). Pictures and masks are shared (`Arc`), so a
//! copy of the document — one per undo step — costs little until a picture is painted on.

use super::img::{Gray, Img};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub type Id = u64;

/// A colour, straight RGBA 8 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub const WHITE: Rgba = Rgba([255, 255, 255, 255]);
    pub const BLACK: Rgba = Rgba([0, 0, 0, 255]);
    pub const fn hex(v: u32) -> Rgba {
        Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
    }
    pub fn with_alpha(self, a: u8) -> Rgba {
        Rgba([self.0[0], self.0[1], self.0[2], a])
    }
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0[0], self.0[1], self.0[2])
    }
    pub fn parse(s: &str) -> Option<Rgba> {
        let h = s.trim().trim_start_matches('#');
        let v = u32::from_str_radix(h, 16).ok()?;
        match h.len() {
            6 => Some(Rgba::hex(v)),
            3 => {
                let (r, g, b) = ((v >> 8) & 15, (v >> 4) & 15, v & 15);
                Some(Rgba([(r * 17) as u8, (g * 17) as u8, (b * 17) as u8, 255]))
            }
            _ => None,
        }
    }
    /// Premultiplied floats.
    pub fn p4(self) -> [f32; 4] {
        super::img::premul(self.0)
    }
    /// Straight floats.
    pub fn f(self) -> [f32; 4] {
        [self.0[0] as f32 / 255.0, self.0[1] as f32 / 255.0, self.0[2] as f32 / 255.0, self.0[3] as f32 / 255.0]
    }
}

/// Where a layer sits: its box (`w` × `h` document pixels) centred on (`cx`, `cy`), turned by `rot` degrees
/// (clockwise), mirrored before turning.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Xf {
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
    pub rot: f32,
    #[serde(default)]
    pub flip_x: bool,
    #[serde(default)]
    pub flip_y: bool,
}

/// 2 × 3 affine matrix: x' = a·x + c·y + e, y' = b·x + d·y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Affine {
    #[cfg(test)]
    pub const ID: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };
    #[inline]
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }
    pub fn invert(&self) -> Affine {
        let det = self.a * self.d - self.b * self.c;
        let k = if det.abs() < 1e-12 { 0.0 } else { 1.0 / det };
        let (a, b, c, d) = (self.d * k, -self.b * k, -self.c * k, self.a * k);
        Affine { a, b, c, d, e: -(a * self.e + c * self.f), f: -(b * self.e + d * self.f) }
    }
    /// `self` then `o`.
    pub fn then(&self, o: &Affine) -> Affine {
        Affine {
            a: o.a * self.a + o.c * self.b,
            b: o.b * self.a + o.d * self.b,
            c: o.a * self.c + o.c * self.d,
            d: o.b * self.c + o.d * self.d,
            e: o.a * self.e + o.c * self.f + o.e,
            f: o.b * self.e + o.d * self.f + o.f,
        }
    }
    #[cfg(test)]
    pub fn scale(s: f32) -> Affine {
        Affine { a: s, b: 0.0, c: 0.0, d: s, e: 0.0, f: 0.0 }
    }
}

impl Xf {
    pub fn new(cx: f32, cy: f32, w: f32, h: f32) -> Xf {
        Xf { cx, cy, w, h, rot: 0.0, flip_x: false, flip_y: false }
    }
    /// Local box coordinates (0..w, 0..h) → document.
    pub fn to_doc(self) -> Affine {
        let (s, c) = self.rot.to_radians().sin_cos();
        let fx = if self.flip_x { -1.0 } else { 1.0 };
        let fy = if self.flip_y { -1.0 } else { 1.0 };
        // centre the box, mirror, turn, move
        let (a, b, cc, d) = (c * fx, s * fx, -s * fy, c * fy);
        let (ox, oy) = (-self.w / 2.0, -self.h / 2.0);
        Affine { a, b, c: cc, d, e: a * ox + cc * oy + self.cx, f: b * ox + d * oy + self.cy }
    }
    /// The four corners in the document (top left, top right, bottom right, bottom left of the box).
    pub fn corners(&self) -> [(f32, f32); 4] {
        let m = self.to_doc();
        [m.apply(0.0, 0.0), m.apply(self.w, 0.0), m.apply(self.w, self.h), m.apply(0.0, self.h)]
    }
    /// Axis-aligned bounds in the document, `pad` local units around the box.
    pub fn bounds(&self, pad: f32) -> (f32, f32, f32, f32) {
        let m = self.to_doc();
        let pts = [m.apply(-pad, -pad), m.apply(self.w + pad, -pad), m.apply(self.w + pad, self.h + pad), m.apply(-pad, self.h + pad)];
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (x, y) in pts {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        (x0, y0, x1, y1)
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let (u, v) = self.to_doc().invert().apply(x, y);
        u >= 0.0 && v >= 0.0 && u <= self.w && v <= self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default, Hash)]
pub enum Blend {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Blend {
    pub const ALL: [Blend; 16] = [
        Blend::Normal,
        Blend::Multiply,
        Blend::Screen,
        Blend::Overlay,
        Blend::Darken,
        Blend::Lighten,
        Blend::ColorDodge,
        Blend::ColorBurn,
        Blend::HardLight,
        Blend::SoftLight,
        Blend::Difference,
        Blend::Exclusion,
        Blend::Hue,
        Blend::Saturation,
        Blend::Color,
        Blend::Luminosity,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Blend::Normal => "Normal",
            Blend::Multiply => "Multiply",
            Blend::Screen => "Screen",
            Blend::Overlay => "Overlay",
            Blend::Darken => "Darken",
            Blend::Lighten => "Lighten",
            Blend::ColorDodge => "Color dodge",
            Blend::ColorBurn => "Color burn",
            Blend::HardLight => "Hard light",
            Blend::SoftLight => "Soft light",
            Blend::Difference => "Difference",
            Blend::Exclusion => "Exclusion",
            Blend::Hue => "Hue",
            Blend::Saturation => "Saturation",
            Blend::Color => "Color",
            Blend::Luminosity => "Luminosity",
        }
    }
}

/// The one-click looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default, Hash)]
pub enum Look {
    #[default]
    None,
    Vivid,
    Warm,
    Cool,
    Mono,
    Noir,
    Vintage,
    Faded,
    Film,
    Dusk,
    Matte,
    Bright,
}

impl Look {
    pub const ALL: [Look; 11] =
        [Look::Vivid, Look::Warm, Look::Cool, Look::Mono, Look::Noir, Look::Vintage, Look::Faded, Look::Film, Look::Dusk, Look::Matte, Look::Bright];
    pub fn label(self) -> &'static str {
        match self {
            Look::None => "Original",
            Look::Vivid => "Vivid",
            Look::Warm => "Warm",
            Look::Cool => "Cool",
            Look::Mono => "Mono",
            Look::Noir => "Noir",
            Look::Vintage => "Vintage",
            Look::Faded => "Faded",
            Look::Film => "Film",
            Look::Dusk => "Dusk",
            Look::Matte => "Matte",
            Look::Bright => "Bright",
        }
    }
}

/// Adjustments of a layer, all applied when drawing: the layer itself never changes. Most run from -1 to 1, 0 being
/// "no change"; `blur` and `sharpen` from 0 to 1; levels: `black` < `white` in 0..1 and a `gamma` around 1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Adjust {
    pub brightness: f32,
    pub contrast: f32,
    pub exposure: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub temperature: f32,
    pub tint: f32,
    pub black: f32,
    pub white: f32,
    pub gamma: f32,
    pub sharpen: f32,
    pub blur: f32,
    pub vignette: f32,
    pub look: Look,
    pub look_amount: f32,
}

impl Default for Adjust {
    fn default() -> Adjust {
        Adjust {
            brightness: 0.0,
            contrast: 0.0,
            exposure: 0.0,
            saturation: 0.0,
            vibrance: 0.0,
            temperature: 0.0,
            tint: 0.0,
            black: 0.0,
            white: 1.0,
            gamma: 1.0,
            sharpen: 0.0,
            blur: 0.0,
            vignette: 0.0,
            look: Look::None,
            look_amount: 1.0,
        }
    }
}

impl Adjust {
    #[cfg(any(test, feature = "test-hook"))]
    pub fn is_identity(&self) -> bool {
        let tone = Adjust { look: Look::None, look_amount: 1.0, ..*self } == Adjust::default();
        tone && (self.look == Look::None || self.look_amount <= 0.0)
    }
    /// Only tone and colour (no blur, sharpen or vignette): each pixel on its own.
    pub fn needs_neighbours(&self) -> bool {
        self.blur > 0.0 || self.sharpen > 0.0
    }
}

/// A layer mask: white shows the layer, black hides it. Stretched over the layer's box.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mask {
    #[serde(skip)]
    pub px: Arc<Gray>,
    /// Index of the picture in a project file.
    #[serde(default)]
    pub key: u32,
    #[serde(default = "yes")]
    pub on: bool,
}

fn yes() -> bool {
    true
}

impl PartialEq for Mask {
    fn eq(&self, o: &Mask) -> bool {
        Arc::ptr_eq(&self.px, &o.px) && self.on == o.on
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageL {
    #[serde(skip)]
    pub px: Arc<Img>,
    #[serde(default)]
    pub key: u32,
}

impl PartialEq for ImageL {
    fn eq(&self, o: &ImageL) -> bool {
        Arc::ptr_eq(&self.px, &o.px)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default, Hash)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextL {
    pub text: String,
    pub font: String,
    /// Size of the letters in document pixels.
    pub size: f32,
    /// 100 (thin) to 900 (black).
    pub weight: u16,
    pub italic: bool,
    pub color: Rgba,
    pub align: TextAlign,
    /// Extra space between letters, in document pixels.
    pub spacing: f32,
    /// Line height, as a multiple of the size.
    pub line: f32,
    pub outline: Option<(Rgba, f32)>,
    /// Colour, offset x and y, blur radius (document pixels).
    pub shadow: Option<(Rgba, f32, f32, f32)>,
}

impl Default for TextL {
    fn default() -> TextL {
        TextL {
            text: String::new(),
            font: "Segoe UI".into(),
            size: 64.0,
            weight: 700,
            italic: false,
            color: Rgba::WHITE,
            align: TextAlign::Left,
            spacing: 0.0,
            line: 1.2,
            outline: None,
            shadow: None,
        }
    }
}

impl TextL {
    /// How far outline and shadow reach beyond the letters' box (document pixels).
    pub fn reach(&self) -> f32 {
        let o = self.outline.map_or(0.0, |o| o.1);
        let s = self.shadow.map_or(0.0, |s| s.1.abs().max(s.2.abs()) + s.3 * 1.5);
        o.max(s) + 2.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default, Hash)]
pub enum ShapeKind {
    #[default]
    Rect,
    Ellipse,
    Line,
    Arrow,
}

/// How a shape is filled.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Paint {
    Solid(Rgba),
    /// From one colour to another along an angle (degrees, 0 = left to right).
    Linear(Rgba, Rgba, f32),
}

impl Paint {
    pub fn first(&self) -> Rgba {
        match self {
            Paint::Solid(c) | Paint::Linear(c, _, _) => *c,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShapeL {
    pub kind: ShapeKind,
    pub fill: Option<Paint>,
    pub stroke: Option<Rgba>,
    pub stroke_w: f32,
    /// Corner radius (rectangles).
    pub radius: f32,
}

impl Default for ShapeL {
    fn default() -> ShapeL {
        ShapeL { kind: ShapeKind::Rect, fill: Some(Paint::Solid(Rgba::hex(0xdb2b9a))), stroke: None, stroke_w: 8.0, radius: 0.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Image(ImageL),
    Text(TextL),
    Shape(ShapeL),
    Group(Vec<Layer>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: Id,
    pub name: String,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub blend: Blend,
    pub xf: Xf,
    #[serde(default)]
    pub adj: Adjust,
    #[serde(default)]
    pub mask: Option<Mask>,
    pub kind: Kind,
    /// Groups: shown open in the layers list.
    #[serde(default)]
    pub open: bool,
}

fn one() -> f32 {
    1.0
}

impl Layer {
    pub fn new(id: Id, name: impl Into<String>, xf: Xf, kind: Kind) -> Layer {
        Layer {
            id,
            name: name.into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: Blend::Normal,
            xf,
            adj: Adjust::default(),
            mask: None,
            kind,
            open: true,
        }
    }
    pub fn image(&self) -> Option<&Arc<Img>> {
        match &self.kind {
            Kind::Image(i) => Some(&i.px),
            _ => None,
        }
    }
    pub fn is_group(&self) -> bool {
        matches!(self.kind, Kind::Group(_))
    }
    pub fn children(&self) -> &[Layer] {
        match &self.kind {
            Kind::Group(c) => c,
            _ => &[],
        }
    }
    /// How far the layer's pixels reach beyond its box, in document pixels.
    pub fn pad(&self) -> f32 {
        match &self.kind {
            Kind::Text(t) => t.reach(),
            Kind::Shape(s) => match s.kind {
                ShapeKind::Line | ShapeKind::Arrow => s.stroke_w * 2.0 + 2.0,
                _ => 1.0,
            },
            _ => 1.0,
        }
    }
    /// Bounds in the document: x0, y0, x1, y1 (groups: their children's).
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        match &self.kind {
            Kind::Group(c) => {
                let mut acc: Option<(f32, f32, f32, f32)> = None;
                for l in c {
                    if let Some(b) = l.bounds() {
                        acc = Some(match acc {
                            None => b,
                            Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
                        });
                    }
                }
                acc
            }
            _ => Some(self.xf.bounds(self.pad())),
        }
    }
    /// The box of a group (its children's bounds) or the layer's own.
    pub fn frame(&self) -> Xf {
        match &self.kind {
            Kind::Group(_) => match self.bounds() {
                Some((x0, y0, x1, y1)) => Xf::new((x0 + x1) / 2.0, (y0 + y1) / 2.0, x1 - x0, y1 - y0),
                None => self.xf,
            },
            _ => self.xf,
        }
    }
    /// Moves the layer (groups: all their children).
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.xf.cx += dx;
        self.xf.cy += dy;
        if let Kind::Group(c) = &mut self.kind {
            for l in c {
                l.translate(dx, dy);
            }
        }
    }
    /// Turns the layer around a point of the document (groups: all their children).
    pub fn rotate_about(&mut self, px: f32, py: f32, deg: f32) {
        let (s, c) = deg.to_radians().sin_cos();
        let (dx, dy) = (self.xf.cx - px, self.xf.cy - py);
        self.xf.cx = px + dx * c - dy * s;
        self.xf.cy = py + dx * s + dy * c;
        self.xf.rot = norm_deg(self.xf.rot + deg);
        if let Kind::Group(ch) = &mut self.kind {
            for l in ch {
                l.rotate_about(px, py, deg);
            }
        }
    }
    /// Scales the layer from a point of the document (groups: all their children). Text grows its letters.
    pub fn scale_about(&mut self, px: f32, py: f32, kx: f32, ky: f32) {
        self.xf.cx = px + (self.xf.cx - px) * kx;
        self.xf.cy = py + (self.xf.cy - py) * ky;
        match &mut self.kind {
            Kind::Group(ch) => {
                for l in ch {
                    l.scale_about(px, py, kx, ky);
                }
            }
            Kind::Text(t) => {
                let k = (kx * ky).abs().sqrt();
                t.size *= k;
                t.spacing *= k;
                if let Some(o) = &mut t.outline {
                    o.1 *= k;
                }
                if let Some(s) = &mut t.shadow {
                    s.1 *= k;
                    s.2 *= k;
                    s.3 *= k;
                }
                self.xf.w *= kx.abs();
                self.xf.h *= ky.abs();
            }
            Kind::Shape(s) => {
                let k = (kx * ky).abs().sqrt();
                s.stroke_w *= k;
                s.radius *= k;
                self.xf.w *= kx.abs();
                self.xf.h *= ky.abs();
            }
            Kind::Image(_) => {
                self.xf.w *= kx.abs();
                self.xf.h *= ky.abs();
            }
        }
    }
    /// The layer and all the layers inside it.
    pub fn walk<'a>(&'a self, out: &mut Vec<&'a Layer>) {
        out.push(self);
        for c in self.children() {
            c.walk(out);
        }
    }
}

pub fn norm_deg(d: f32) -> f32 {
    let mut d = d % 360.0;
    if d > 180.0 {
        d -= 360.0;
    }
    if d <= -180.0 {
        d += 360.0;
    }
    d
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Doc {
    pub w: u32,
    pub h: u32,
    /// Bottom layer first.
    pub layers: Vec<Layer>,
    pub next_id: Id,
}

impl Doc {
    pub fn new(w: u32, h: u32) -> Doc {
        Doc { w, h, layers: vec![], next_id: 1 }
    }
    pub fn id(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    /// A layer anywhere in the stack.
    pub fn find(&self, id: Id) -> Option<&Layer> {
        fn go(v: &[Layer], id: Id) -> Option<&Layer> {
            for l in v {
                if l.id == id {
                    return Some(l);
                }
                if let Some(f) = go(l.children(), id) {
                    return Some(f);
                }
            }
            None
        }
        go(&self.layers, id)
    }
    pub fn find_mut(&mut self, id: Id) -> Option<&mut Layer> {
        fn go(v: &mut [Layer], id: Id) -> Option<&mut Layer> {
            for l in v {
                if l.id == id {
                    return Some(l);
                }
                if let Kind::Group(c) = &mut l.kind
                    && let Some(f) = go(c, id)
                {
                    return Some(f);
                }
            }
            None
        }
        go(&mut self.layers, id)
    }
    /// The list holding a layer (the stack, or a group's children) and its place in it.
    pub fn parent_of(&self, id: Id) -> Option<(Option<Id>, usize)> {
        fn go(v: &[Layer], parent: Option<Id>, id: Id) -> Option<(Option<Id>, usize)> {
            for (i, l) in v.iter().enumerate() {
                if l.id == id {
                    return Some((parent, i));
                }
                if let Some(f) = go(l.children(), Some(l.id), id) {
                    return Some(f);
                }
            }
            None
        }
        go(&self.layers, None, id)
    }
    pub fn list_mut(&mut self, parent: Option<Id>) -> Option<&mut Vec<Layer>> {
        match parent {
            None => Some(&mut self.layers),
            Some(p) => match &mut self.find_mut(p)?.kind {
                Kind::Group(c) => Some(c),
                _ => None,
            },
        }
    }
    /// Takes a layer out of the stack.
    pub fn remove(&mut self, id: Id) -> Option<Layer> {
        let (parent, i) = self.parent_of(id)?;
        let list = self.list_mut(parent)?;
        Some(list.remove(i))
    }
    /// All layers, depth first, bottom first (groups before their children).
    pub fn all(&self) -> Vec<&Layer> {
        let mut out = vec![];
        for l in &self.layers {
            l.walk(&mut out);
        }
        out
    }
    /// The top-most visible, unlocked layer under a point (inside groups: the child).
    pub fn pick(&self, x: f32, y: f32) -> Option<Id> {
        fn go(v: &[Layer], x: f32, y: f32) -> Option<Id> {
            for l in v.iter().rev() {
                if !l.visible {
                    continue;
                }
                if let Kind::Group(c) = &l.kind {
                    if let Some(id) = go(c, x, y) {
                        return Some(id);
                    }
                    continue;
                }
                if l.xf.contains(x, y) && !l.locked {
                    return Some(l.id);
                }
            }
            None
        }
        go(&self.layers, x, y)
    }
    /// The group a layer is in, if any.
    pub fn group_of(&self, id: Id) -> Option<Id> {
        self.parent_of(id).and_then(|(p, _)| p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_round_trip() {
        let mut xf = Xf::new(100.0, 50.0, 40.0, 20.0);
        xf.rot = 30.0;
        xf.flip_x = true;
        let m = xf.to_doc();
        let (x, y) = m.apply(10.0, 5.0);
        let (u, v) = m.invert().apply(x, y);
        assert!((u - 10.0).abs() < 1e-3 && (v - 5.0).abs() < 1e-3);
        // the centre of the box is the layer's centre
        let (cx, cy) = m.apply(20.0, 10.0);
        assert!((cx - 100.0).abs() < 1e-3 && (cy - 50.0).abs() < 1e-3);
        assert!(xf.contains(100.0, 50.0) && !xf.contains(200.0, 50.0));
        // mirrored: the left of the box is on the right
        let (lx, _) = Xf { rot: 0.0, ..xf }.to_doc().apply(0.0, 10.0);
        assert!((lx - 120.0).abs() < 1e-3);
        let comp = Affine::scale(2.0).then(&Affine { e: 5.0, ..Affine::ID });
        assert_eq!(comp.apply(1.0, 1.0), (7.0, 2.0));
    }

    #[test]
    fn stack_edits() {
        let mut d = Doc::new(100, 100);
        let a = d.id();
        let b = d.id();
        let g = d.id();
        let la = Layer::new(a, "a", Xf::new(10.0, 10.0, 10.0, 10.0), Kind::Shape(ShapeL::default()));
        let lb = Layer::new(b, "b", Xf::new(50.0, 50.0, 10.0, 10.0), Kind::Shape(ShapeL::default()));
        d.layers.push(Layer::new(g, "g", Xf::new(0.0, 0.0, 0.0, 0.0), Kind::Group(vec![la, lb])));
        assert_eq!(d.parent_of(b), Some((Some(g), 1)));
        assert_eq!(d.pick(50.0, 50.0), Some(b));
        assert_eq!(d.find(g).unwrap().bounds().map(|b| (b.0.round(), b.3.round())), Some((4.0, 56.0)));
        d.find_mut(g).unwrap().translate(5.0, 0.0);
        assert_eq!(d.find(a).unwrap().xf.cx, 15.0);
        assert!(d.remove(a).is_some());
        assert_eq!(d.find(g).unwrap().children().len(), 1);
        assert_eq!(norm_deg(370.0), 10.0);
        assert_eq!(norm_deg(-190.0), 170.0);
    }

    #[test]
    fn colours() {
        assert_eq!(Rgba::parse("#ff8000"), Some(Rgba([255, 128, 0, 255])));
        assert_eq!(Rgba::parse("f80"), Some(Rgba([255, 136, 0, 255])));
        assert_eq!(Rgba::hex(0x123456).to_hex(), "#123456");
        assert_eq!(Rgba::parse("nope"), None);
    }
}
