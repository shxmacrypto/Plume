//! Painting on pixels: brush, eraser, clone and heal on a picture layer; hiding and showing on a mask; filling.
//! A stroke keeps the pixels as they were when it began and how much each one is covered so far: its opacity is
//! never exceeded however often the brush passes, and the eraser or clone always work from the picture as it was.

use super::{
    doc::{Affine, Rgba},
    img::{Gray, Img, P4, blur, premul, unpremul8},
    select::Selection,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Brush {
    Paint(Rgba),
    Erase,
    /// Copies the picture from `dx`, `dy` (picture pixels) away.
    Clone(f32, f32),
    /// Like clone, keeping the light and colour of the place painted (the texture comes from the source).
    Heal(f32, f32),
    /// On a mask: hide (black) or show (white).
    Hide,
    Show,
}

/// What a stroke paints on.
#[derive(Clone)]
pub enum Target {
    Image(Arc<Img>),
    Mask(Arc<Gray>),
}

impl Target {
    pub fn size(&self) -> (u32, u32) {
        match self {
            Target::Image(i) => (i.w, i.h),
            Target::Mask(m) => (m.w, m.h),
        }
    }
}

pub struct Stroke {
    pub brush: Brush,
    /// Diameter in document pixels.
    pub size: f32,
    pub hardness: f32,
    pub opacity: f32,
    base: Target,
    /// The result so far.
    pub out: Target,
    cov: Vec<u8>,
    /// Document → target pixels, and back.
    to_px: Affine,
    to_doc: Affine,
    /// Target pixels per document pixel (average).
    ppd: f32,
    last: Option<(f32, f32)>,
    sel: Option<Selection>,
    /// Heal: the base, softened.
    soft: Option<Vec<[f32; 3]>>,
    /// Changed area in target pixels: x0, y0, x1, y1.
    pub dirty: Option<(u32, u32, u32, u32)>,
}

impl Stroke {
    /// `to_px`: document → target pixels (a layer's inverse transform, scaled to its pixels).
    pub fn new(brush: Brush, size: f32, hardness: f32, opacity: f32, base: Target, to_px: Affine, sel: Option<Selection>) -> Stroke {
        let (w, h) = base.size();
        let ppd = ((to_px.a * to_px.d - to_px.b * to_px.c).abs()).sqrt().max(1e-4);
        let soft = match (&brush, &base) {
            (Brush::Heal(..), Target::Image(img)) => {
                let mut px: Vec<[f32; 3]> =
                    img.data.as_chunks::<4>().0.iter().map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]).collect();
                blur(&mut px, w as usize, h as usize, (size * ppd * 0.5).max(2.0));
                Some(px)
            }
            _ => None,
        };
        Stroke {
            brush,
            size,
            hardness: hardness.clamp(0.0, 1.0),
            opacity: opacity.clamp(0.0, 1.0),
            out: base.clone(),
            base,
            cov: vec![0; w as usize * h as usize],
            to_doc: to_px.invert(),
            to_px,
            ppd,
            last: None,
            sel,
            soft,
            dirty: None,
        }
    }

    /// The brush goes to a point of the document (from where it was: dabs in between).
    pub fn to(&mut self, x: f32, y: f32) {
        let r = self.size / 2.0;
        let step = (r * 0.2).max(0.5);
        match self.last {
            None => self.dab(x, y),
            Some((lx, ly)) => {
                let d = ((x - lx).powi(2) + (y - ly).powi(2)).sqrt();
                if d < step {
                    return;
                }
                let n = (d / step).floor() as usize;
                for i in 1..=n {
                    let t = i as f32 * step / d;
                    self.dab(lx + (x - lx) * t, ly + (y - ly) * t);
                }
                let t = n as f32 * step / d;
                self.last = Some((lx + (x - lx) * t, ly + (y - ly) * t));
                return;
            }
        }
        self.last = Some((x, y));
    }

    fn dab(&mut self, x: f32, y: f32) {
        let (w, h) = self.base.size();
        let (cx, cy) = self.to_px.apply(x, y);
        let r = (self.size / 2.0 * self.ppd).max(0.5);
        let x0 = (cx - r - 1.0).floor().max(0.0) as u32;
        let y0 = (cy - r - 1.0).floor().max(0.0) as u32;
        let x1 = ((cx + r + 1.0).ceil().max(0.0) as u32).min(w);
        let y1 = ((cy + r + 1.0).ceil().max(0.0) as u32).min(h);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let hard = self.hardness.min(0.98);
        for py in y0..y1 {
            for px in x0..x1 {
                let d = ((px as f32 + 0.5 - cx).powi(2) + (py as f32 + 0.5 - cy).powi(2)).sqrt() / r;
                if d >= 1.0 {
                    continue;
                }
                let t = ((d - hard) / (1.0 - hard)).clamp(0.0, 1.0);
                let a = 1.0 - t * t * (3.0 - 2.0 * t);
                let i = py as usize * w as usize + px as usize;
                let v = (a * 255.0 + 0.5) as u8;
                if v > self.cov[i] {
                    self.cov[i] = v;
                }
            }
        }
        self.dirty = Some(match self.dirty {
            None => (x0, y0, x1, y1),
            Some(d) => (d.0.min(x0), d.1.min(y0), d.2.max(x1), d.3.max(y1)),
        });
        self.redo(x0, y0, x1, y1);
    }

    /// Works out the pixels of an area again from the base and the coverage.
    fn redo(&mut self, x0: u32, y0: u32, x1: u32, y1: u32) {
        let (w, _) = self.base.size();
        let op = self.opacity;
        match (&self.base, &mut self.out) {
            (Target::Image(base), Target::Image(out)) => {
                let out = Arc::make_mut(out);
                for py in y0..y1 {
                    for px in x0..x1 {
                        let i = py as usize * w as usize + px as usize;
                        let mut k = self.cov[i] as f32 / 255.0 * op;
                        if k <= 0.0 {
                            continue;
                        }
                        if let Some(s) = &self.sel {
                            let (dx, dy) = self.to_doc.apply(px as f32 + 0.5, py as f32 + 0.5);
                            k *= s.at(dx, dy);
                            if k <= 0.0 {
                                out.set(px, py, base.get(px, py));
                                continue;
                            }
                        }
                        let b = base.p4(px, py);
                        let res: P4 = match self.brush {
                            Brush::Paint(c) => {
                                let c = c.p4();
                                let k = k * 1.0;
                                [b[0] + (c[0] - b[0]) * k, b[1] + (c[1] - b[1]) * k, b[2] + (c[2] - b[2]) * k, b[3] + (c[3] - b[3]) * k]
                            }
                            Brush::Erase => [b[0] * (1.0 - k), b[1] * (1.0 - k), b[2] * (1.0 - k), b[3] * (1.0 - k)],
                            Brush::Clone(dx, dy) => {
                                let s = base.sample(px as f32 + 0.5 - dx, py as f32 + 0.5 - dy);
                                [b[0] + (s[0] - b[0]) * k, b[1] + (s[1] - b[1]) * k, b[2] + (s[2] - b[2]) * k, b[3] + (s[3] - b[3]) * k]
                            }
                            Brush::Heal(dx, dy) => {
                                let (sx, sy) = (px as f32 + 0.5 - dx, py as f32 + 0.5 - dy);
                                let s = base.sample(sx, sy);
                                let soft = self.soft.as_ref().unwrap();
                                let at = |x: f32, y: f32| -> [f32; 3] {
                                    let (xi, yi) = ((x.max(0.0) as u32).min(base.w - 1), (y.max(0.0) as u32).min(base.h - 1));
                                    soft[yi as usize * w as usize + xi as usize]
                                };
                                let (ls, ld) = (at(sx, sy), at(px as f32, py as f32));
                                let sa = s[3].max(1e-4);
                                let mut c = [0.0; 4];
                                for ch in 0..3 {
                                    let v = (s[ch] / sa + ld[ch] - ls[ch]).clamp(0.0, 1.0);
                                    let bv = if b[3] > 0.0 { b[ch] / b[3] } else { 0.0 };
                                    c[ch] = bv + (v - bv) * k;
                                }
                                c[3] = b[3] + (s[3] - b[3]) * k;
                                [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
                            }
                            Brush::Hide | Brush::Show => b,
                        };
                        out.set(px, py, unpremul8(res));
                    }
                }
            }
            (Target::Mask(base), Target::Mask(out)) => {
                let out = Arc::make_mut(out);
                let goal = if self.brush == Brush::Show { 255.0 } else { 0.0 };
                for py in y0..y1 {
                    for px in x0..x1 {
                        let i = py as usize * w as usize + px as usize;
                        let mut k = self.cov[i] as f32 / 255.0 * op;
                        if let Some(s) = &self.sel {
                            let (dx, dy) = self.to_doc.apply(px as f32 + 0.5, py as f32 + 0.5);
                            k *= s.at(dx, dy);
                        }
                        let b = base.get(px, py) as f32;
                        out.set(px, py, (b + (goal - b) * k).round().clamp(0.0, 255.0) as u8);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Fills the area around a pixel whose colour is close to its own (`tolerance` 0 to 1), within a selection
/// (`sel`, with `to_doc`: picture pixels → document). Returns the new picture, or None if nothing changed.
pub fn flood_fill(img: &Img, x: u32, y: u32, colour: Rgba, tolerance: f32, sel: Option<(&Selection, Affine)>) -> Option<Img> {
    if x >= img.w || y >= img.h {
        return None;
    }
    let seed = img.get(x, y);
    let tol = (tolerance.clamp(0.0, 1.0) * 255.0) as i32;
    let close = |c: [u8; 4]| (0..4).all(|k| (c[k] as i32 - seed[k] as i32).abs() <= tol);
    let (w, h) = (img.w as usize, img.h as usize);
    let mut seen = vec![false; w * h];
    let mut stack = vec![(x as usize, y as usize)];
    let mut out = img.clone();
    let paint = premul(colour.0);
    let mut changed = false;
    while let Some((sx, sy)) = stack.pop() {
        // a whole run of the row, then the rows above and below
        let mut l = sx;
        while l > 0 && !seen[sy * w + l - 1] && close(img.get(l as u32 - 1, sy as u32)) {
            l -= 1;
        }
        let mut r = sx;
        while r + 1 < w && !seen[sy * w + r + 1] && close(img.get(r as u32 + 1, sy as u32)) {
            r += 1;
        }
        for px in l..=r {
            let i = sy * w + px;
            if seen[i] || !close(img.get(px as u32, sy as u32)) {
                continue;
            }
            seen[i] = true;
            let k = match &sel {
                Some((s, to_doc)) => {
                    let (dx, dy) = to_doc.apply(px as f32 + 0.5, sy as f32 + 0.5);
                    s.at(dx, dy)
                }
                None => 1.0,
            };
            if k > 0.0 {
                let b = img.p4(px as u32, sy as u32);
                let res = [b[0] + (paint[0] - b[0]) * k, b[1] + (paint[1] - b[1]) * k, b[2] + (paint[2] - b[2]) * k, b[3] + (paint[3] - b[3]) * k];
                out.set(px as u32, sy as u32, unpremul8(res));
                changed = true;
            }
            for ny in [sy.wrapping_sub(1), sy + 1] {
                if ny < h && !seen[ny * w + px] && close(img.get(px as u32, ny as u32)) {
                    stack.push((px, ny));
                }
            }
        }
    }
    changed.then_some(out)
}

/// Clears (`fill` None) or fills the selected part of a picture. `to_doc`: picture pixels → document.
pub fn fill_selection(img: &Img, sel: &Selection, to_doc: Affine, fill: Option<Rgba>) -> Img {
    let mut out = img.clone();
    for y in 0..img.h {
        for x in 0..img.w {
            let (dx, dy) = to_doc.apply(x as f32 + 0.5, y as f32 + 0.5);
            let k = sel.at(dx, dy);
            if k <= 0.0 {
                continue;
            }
            let b = img.p4(x, y);
            let res = match fill {
                None => [b[0] * (1.0 - k), b[1] * (1.0 - k), b[2] * (1.0 - k), b[3] * (1.0 - k)],
                Some(c) => {
                    let c = c.p4();
                    [b[0] + (c[0] - b[0]) * k, b[1] + (c[1] - b[1]) * k, b[2] + (c[2] - b[2]) * k, b[3] + (c[3] - b[3]) * k]
                }
            };
            out.set(x, y, unpremul8(res));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::select::SelShape;

    #[test]
    fn brush_never_goes_past_its_opacity() {
        let base = Arc::new(Img::filled(40, 40, [0, 0, 0, 255]));
        let mut s = Stroke::new(Brush::Paint(Rgba::WHITE), 10.0, 1.0, 0.5, Target::Image(base.clone()), Affine::ID, None);
        for _ in 0..5 {
            s.to(10.0, 20.0);
            s.to(30.0, 20.0);
            s.to(10.0, 20.0);
        }
        let Target::Image(out) = &s.out else { panic!() };
        let p = out.get(20, 20);
        assert!((p[0] as i32 - 128).abs() <= 2, "{p:?}");
        assert_eq!(out.get(20, 5), [0, 0, 0, 255]);
        // the base is untouched (undo keeps it)
        assert_eq!(base.get(20, 20), [0, 0, 0, 255]);
    }

    #[test]
    fn eraser_clone_heal_and_masks() {
        let mut img = Img::filled(40, 20, [200, 50, 50, 255]);
        for y in 0..20 {
            for x in 20..40 {
                img.set(x, y, [50, 50, 200, 255]);
            }
        }
        let img = Arc::new(img);
        let mut e = Stroke::new(Brush::Erase, 6.0, 1.0, 1.0, Target::Image(img.clone()), Affine::ID, None);
        e.to(10.0, 10.0);
        let Target::Image(out) = &e.out else { panic!() };
        assert_eq!(out.get(10, 10)[3], 0);
        let mut c = Stroke::new(Brush::Clone(20.0, 0.0), 6.0, 1.0, 1.0, Target::Image(img.clone()), Affine::ID, None);
        c.to(30.0, 10.0);
        let Target::Image(out) = &c.out else { panic!() };
        assert_eq!(out.get(30, 10), [200, 50, 50, 255]);
        let mut hl = Stroke::new(Brush::Heal(20.0, 0.0), 6.0, 1.0, 1.0, Target::Image(img.clone()), Affine::ID, None);
        hl.to(30.0, 10.0);
        let Target::Image(out) = &hl.out else { panic!() };
        // heal keeps the colour of the place: still blue
        let p = out.get(30, 10);
        assert!(p[2] > p[0], "{p:?}");
        let m = Arc::new(Gray::new(10, 10, 255));
        let mut hide = Stroke::new(Brush::Hide, 4.0, 1.0, 1.0, Target::Mask(m), Affine::ID, None);
        hide.to(5.0, 5.0);
        let Target::Mask(out) = &hide.out else { panic!() };
        assert_eq!(out.get(5, 5), 0);
        assert_eq!(out.get(0, 0), 255);
    }

    #[test]
    fn selections_limit_painting_and_filling() {
        let img = Img::filled(20, 20, [0, 0, 0, 255]);
        let sel = Selection::new(SelShape::Rect(0.0, 0.0, 10.0, 20.0), 20, 20);
        let mut s = Stroke::new(Brush::Paint(Rgba::WHITE), 20.0, 1.0, 1.0, Target::Image(Arc::new(img.clone())), Affine::ID, Some(sel.clone()));
        s.to(10.0, 10.0);
        let Target::Image(out) = &s.out else { panic!() };
        assert_eq!(out.get(5, 10), [255, 255, 255, 255]);
        assert_eq!(out.get(15, 10), [0, 0, 0, 255]);
        let filled = fill_selection(&img, &sel, Affine::ID, Some(Rgba::hex(0xff0000)));
        assert_eq!(filled.get(2, 2), [255, 0, 0, 255]);
        assert_eq!(filled.get(12, 2), [0, 0, 0, 255]);
        let cleared = fill_selection(&img, &sel, Affine::ID, None);
        assert_eq!(cleared.get(2, 2)[3], 0);
    }

    #[test]
    fn flood_fill_stops_at_other_colours() {
        let mut img = Img::filled(10, 10, [255, 255, 255, 255]);
        for y in 0..10 {
            img.set(5, y, [0, 0, 0, 255]);
        }
        let out = flood_fill(&img, 1, 1, Rgba::hex(0x00ff00), 0.1, None).unwrap();
        assert_eq!(out.get(0, 9), [0, 255, 0, 255]);
        assert_eq!(out.get(5, 5), [0, 0, 0, 255]);
        assert_eq!(out.get(8, 5), [255, 255, 255, 255]);
    }
}
