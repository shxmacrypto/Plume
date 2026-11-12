//! The compositor: draws the document at any scale, or only a part of it, layer by layer — the same code for the
//! screen (at the zoom shown) and for export (at the size asked).

use super::{
    adjust::{Prepared, vignette},
    blend,
    doc::{Doc, Kind, Layer, TextL},
    img::{Buf, IRect, Img, P4, blur, par_rows},
    shape,
    textfx::{self, Glyphs},
};
use std::{
    cell::RefCell,
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

/// Draws text as coverage: `width` (pixels) is where lines wrap; the letters are `t.size * scale` pixels.
pub trait TextRaster {
    fn glyphs(&self, t: &TextL, width: f32, scale: f32, pad: u32) -> Option<Glyphs>;
    /// Height (document pixels) of the text wrapped in `width` document pixels.
    fn measure(&self, t: &TextL, width: f32) -> f32;
}

/// A raster in a layer's own box: pixel (i, j) covers box point ((i + .5) / ppu + ou, (j + .5) / ppu + ov).
pub struct Local {
    pub w: u32,
    pub h: u32,
    pub px: Vec<P4>,
    pub ou: f32,
    pub ov: f32,
    pub ppu: f32,
}

impl Local {
    #[inline]
    fn sample(&self, u: f32, v: f32) -> P4 {
        let x = (u - self.ou) * self.ppu - 0.5;
        let y = (v - self.ov) * self.ppu - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (tx, ty) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (w, h) = (self.w as i64, self.h as i64);
        let at = |x: i64, y: i64| if x < 0 || y < 0 || x >= w || y >= h { [0.0; 4] } else { self.px[(y * w + x) as usize] };
        let (a, b, c, d) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
        let mut o = [0.0; 4];
        for k in 0..4 {
            let top = a[k] + (b[k] - a[k]) * tx;
            let bot = c[k] + (d[k] - c[k]) * tx;
            o[k] = top + (bot - top) * ty;
        }
        o
    }
}

/// Text that draws nothing (tests of other parts, and a fallback).
pub struct NoText;
impl TextRaster for NoText {
    fn glyphs(&self, _t: &TextL, _w: f32, _s: f32, _p: u32) -> Option<Glyphs> {
        None
    }
    fn measure(&self, t: &TextL, _w: f32) -> f32 {
        t.size * t.line * t.text.lines().count().max(1) as f32
    }
}

type Mips = (Weak<Img>, Vec<Arc<Img>>);

pub struct Renderer {
    pub text: Box<dyn TextRaster>,
    mips: RefCell<HashMap<usize, Mips>>,
    texts: RefCell<HashMap<u64, Arc<Local>>>,
}

fn text_key(t: &TextL, w: f32, s: f32) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(t).unwrap_or_default().hash(&mut h);
    w.to_bits().hash(&mut h);
    s.to_bits().hash(&mut h);
    h.finish()
}

impl Renderer {
    pub fn new(text: Box<dyn TextRaster>) -> Renderer {
        Renderer { text, mips: RefCell::new(HashMap::new()), texts: RefCell::new(HashMap::new()) }
    }

    /// Smaller copies of a picture (each half the last), made when first needed.
    fn level(&self, img: &Arc<Img>, k: usize) -> Arc<Img> {
        if k == 0 {
            return img.clone();
        }
        let key = Arc::as_ptr(img) as usize;
        let mut mips = self.mips.borrow_mut();
        let fresh = mips.get(&key).is_none_or(|(w, _)| !w.upgrade().is_some_and(|a| Arc::ptr_eq(&a, img)));
        if fresh {
            mips.insert(key, (Arc::downgrade(img), vec![]));
        }
        let levels = &mut mips.get_mut(&key).unwrap().1;
        while levels.len() < k {
            let next = levels.last().map_or_else(|| img.half(), |l| l.half());
            levels.push(Arc::new(next));
        }
        // pictures no longer used are let go
        if mips.len() > 64 {
            mips.retain(|_, (w, _)| w.strong_count() > 0);
        }
        mips.get(&key).unwrap().1[k - 1].clone()
    }

    /// A picture was painted on in place (same allocation): its smaller copies are brought up to date over the
    /// changed area (x0, y0, x1, y1 in its pixels).
    pub fn refresh(&self, img: &Arc<Img>, area: (u32, u32, u32, u32)) {
        let key = Arc::as_ptr(img) as usize;
        let mut mips = self.mips.borrow_mut();
        let Some((_, levels)) = mips.get_mut(&key) else { return };
        let (mut x0, mut y0, mut x1, mut y1) = area;
        let mut prev: Arc<Img> = img.clone();
        for lvl in levels.iter_mut() {
            x0 /= 2;
            y0 /= 2;
            x1 = x1.div_ceil(2) + 1;
            y1 = y1.div_ceil(2) + 1;
            let dst = Arc::make_mut(lvl);
            for y in y0..y1.min(dst.h) {
                for x in x0..x1.min(dst.w) {
                    let mut acc = [0.0f32; 4];
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let p = prev.p4((x * 2 + dx).min(prev.w - 1), (y * 2 + dy).min(prev.h - 1));
                        for k in 0..4 {
                            acc[k] += p[k] * 0.25;
                        }
                    }
                    dst.set(x, y, super::img::unpremul8(acc));
                }
            }
            prev = lvl.clone();
        }
    }

    fn text_local(&self, t: &TextL, w: f32, s: f32) -> Option<Arc<Local>> {
        let key = text_key(t, w, s);
        if let Some(l) = self.texts.borrow().get(&key) {
            return Some(l.clone());
        }
        let pad = (t.reach() * s).ceil() as u32 + 2;
        let g = self.text.glyphs(t, w * s, s, pad)?;
        let px = textfx::compose(t, &g, s);
        let local = Arc::new(Local { w: g.w, h: g.h, px, ou: -(g.pad as f32) / s, ov: -(g.pad as f32) / s, ppu: s });
        let mut texts = self.texts.borrow_mut();
        if texts.len() > 200 {
            texts.clear();
        }
        texts.insert(key, local.clone());
        Some(local)
    }

    /// Height of a text in a box `width` wide (document pixels).
    pub fn measure(&self, t: &TextL, width: f32) -> f32 {
        self.text.measure(t, width)
    }

    /// The document at `scale` (output pixels per document pixel), the part `region` of the output.
    pub fn render(&self, doc: &Doc, scale: f32, region: IRect) -> Buf {
        let mut out = Buf::new(region);
        for l in &doc.layers {
            self.layer(l, scale, &mut out);
        }
        out
    }

    /// The whole document at `scale`, straight RGBA (export).
    pub fn image(&self, doc: &Doc, scale: f32) -> Img {
        let (w, h) = (((doc.w as f32 * scale).round() as i32).max(1), ((doc.h as f32 * scale).round() as i32).max(1));
        self.render(doc, scale, IRect::new(0, 0, w, h)).to_img()
    }

    /// One layer drawn alone (thumbnails, previews of looks).
    pub fn layer_alone(&self, l: &Layer, scale: f32, region: IRect) -> Buf {
        let mut out = Buf::new(region);
        self.layer(l, scale, &mut out);
        out
    }

    fn layer(&self, l: &Layer, s: f32, out: &mut Buf) {
        if !l.visible || l.opacity <= 0.0 {
            return;
        }
        let Some(b) = l.bounds() else { return };
        let blur_px = l.adj.blur * 40.0 * s;
        let margin = if l.adj.needs_neighbours() { (blur_px * 1.6 + 3.0).ceil() as i32 } else { 0 };
        let bb = IRect::covering(b.0 * s, b.1 * s, b.2 * s, b.3 * s).grow(margin);
        let area = bb.intersect(&out.r.grow(margin));
        if area.is_empty() || bb.intersect(&out.r).is_empty() {
            return;
        }
        let mut buf = Buf::new(area);
        match &l.kind {
            Kind::Group(ch) => {
                for c in ch {
                    self.layer(c, s, &mut buf);
                }
            }
            _ => self.content(l, s, &mut buf),
        }
        let (w, h) = (area.w as usize, area.h as usize);
        if l.adj.blur > 0.0 {
            blur(&mut buf.px, w, h, blur_px);
        }
        if l.adj.sharpen > 0.0 {
            let mut soft = buf.px.clone();
            blur(&mut soft, w, h, (1.6 * s).clamp(0.8, 3.0));
            let k = l.adj.sharpen * 1.8;
            for (p, q) in buf.px.iter_mut().zip(&soft) {
                for c in 0..3 {
                    p[c] = (p[c] + (p[c] - q[c]) * k).clamp(0.0, p[3]);
                }
            }
        }
        // tone, colour, vignette, mask, then onto what is below
        let prep = Prepared::new(&l.adj);
        let frame = l.frame();
        let inv = frame.to_doc().invert();
        let mask = l.mask.as_ref().filter(|m| m.on && !l.is_group()).map(|m| m.px.clone());
        let vig = l.adj.vignette;
        let (fw, fh) = (frame.w.max(1e-3), frame.h.max(1e-3));
        let part = area.intersect(&out.r);
        let (ox, ow) = (out.r.x, out.r.w as usize);
        let blend = l.blend;
        let opacity = l.opacity;
        let rows = &mut out.px[((part.y - out.r.y) as usize * ow)..((part.bottom() - out.r.y) as usize * ow)];
        par_rows(rows, ow, |first, chunk| {
            for (ri, row) in chunk.chunks_exact_mut(ow).enumerate() {
                let y = part.y + (first + ri) as i32;
                let src_row = (y - area.y) as usize * w;
                for x in part.x..part.right() {
                    let mut p = buf.px[src_row + (x - area.x) as usize];
                    if p[3] <= 0.0 {
                        continue;
                    }
                    let need_uv = vig != 0.0 || mask.is_some();
                    let (u, v) = if need_uv { inv.apply((x as f32 + 0.5) / s, (y as f32 + 0.5) / s) } else { (0.0, 0.0) };
                    if prep.pointwise() || vig != 0.0 {
                        let a = p[3];
                        let mut c = [p[0] / a, p[1] / a, p[2] / a];
                        if prep.pointwise() {
                            c = prep.apply(c);
                        }
                        if vig != 0.0 {
                            let k = vignette(vig, u / fw, v / fh);
                            for ch in &mut c {
                                *ch = if vig > 0.0 { *ch * (1.0 - k) } else { *ch + (1.0 - *ch) * k };
                            }
                        }
                        p = [c[0] * a, c[1] * a, c[2] * a, a];
                    }
                    if let Some(m) = &mask {
                        let k = m.sample_edge(u / fw * m.w as f32, v / fh * m.h as f32);
                        p = [p[0] * k, p[1] * k, p[2] * k, p[3] * k];
                    }
                    let o = &mut row[(x - ox) as usize];
                    *o = blend::over(blend, *o, p, opacity);
                }
            }
        });
    }

    /// The layer's own pixels into `buf` (normal mode, full opacity).
    fn content(&self, l: &Layer, s: f32, buf: &mut Buf) {
        let inv = l.xf.to_doc().invert();
        let (bw, area) = (buf.r.w as usize, buf.r);
        match &l.kind {
            Kind::Image(im) => {
                let img = &im.px;
                if img.w == 0 || img.h == 0 || l.xf.w <= 0.0 || l.xf.h <= 0.0 {
                    return;
                }
                // a smaller copy when the picture is drawn much smaller than it is
                let ratio = (img.w as f32 / (l.xf.w * s)).min(img.h as f32 / (l.xf.h * s));
                let mut k = 0usize;
                let mut r = ratio;
                while r >= 2.0 && k < 12 {
                    r /= 2.0;
                    k += 1;
                }
                let lvl = self.level(img, k);
                let (kx, ky) = (lvl.w as f32 / l.xf.w, lvl.h as f32 / l.xf.h);
                let (lw, lh) = (l.xf.w, l.xf.h);
                par_rows(&mut buf.px, bw, |first, chunk| {
                    for (ri, row) in chunk.chunks_exact_mut(bw).enumerate() {
                        let y = area.y + (first + ri) as i32;
                        for (xi, o) in row.iter_mut().enumerate() {
                            let x = area.x + xi as i32;
                            let (u, v) = inv.apply((x as f32 + 0.5) / s, (y as f32 + 0.5) / s);
                            if u < -1.0 || v < -1.0 || u > lw + 1.0 || v > lh + 1.0 {
                                continue;
                            }
                            *o = lvl.sample(u * kx, v * ky);
                        }
                    }
                });
            }
            Kind::Shape(sh) => {
                let (w, h) = (l.xf.w, l.xf.h);
                let aa = 1.0 / s;
                let pad = l.pad();
                par_rows(&mut buf.px, bw, |first, chunk| {
                    for (ri, row) in chunk.chunks_exact_mut(bw).enumerate() {
                        let y = area.y + (first + ri) as i32;
                        for (xi, o) in row.iter_mut().enumerate() {
                            let x = area.x + xi as i32;
                            let (u, v) = inv.apply((x as f32 + 0.5) / s, (y as f32 + 0.5) / s);
                            if u < -pad || v < -pad || u > w + pad || v > h + pad {
                                continue;
                            }
                            *o = shape::shade(sh, w, h, u, v, aa);
                        }
                    }
                });
            }
            Kind::Text(t) => {
                let Some(local) = self.text_local(t, l.xf.w, s) else { return };
                par_rows(&mut buf.px, bw, |first, chunk| {
                    for (ri, row) in chunk.chunks_exact_mut(bw).enumerate() {
                        let y = area.y + (first + ri) as i32;
                        for (xi, o) in row.iter_mut().enumerate() {
                            let x = area.x + xi as i32;
                            let (u, v) = inv.apply((x as f32 + 0.5) / s, (y as f32 + 0.5) / s);
                            *o = local.sample(u, v);
                        }
                    }
                });
            }
            Kind::Group(_) => {}
        }
    }

    /// The colour of the document at a point (eyedropper), straight RGBA.
    pub fn pick(&self, doc: &Doc, x: f32, y: f32) -> [u8; 4] {
        let b = self.render(doc, 1.0, IRect::new(x.floor() as i32, y.floor() as i32, 1, 1));
        super::img::unpremul8(b.px[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::doc::{Adjust, Blend, ImageL, Mask, Paint, Rgba, ShapeKind, ShapeL, Xf};
    use crate::engine::img::Gray;

    fn r() -> Renderer {
        Renderer::new(Box::new(NoText))
    }
    fn rect_layer(id: u64, x: f32, y: f32, w: f32, h: f32, c: Rgba) -> Layer {
        let sh = ShapeL { kind: ShapeKind::Rect, fill: Some(Paint::Solid(c)), stroke: None, stroke_w: 0.0, radius: 0.0 };
        Layer::new(id, "r", Xf::new(x + w / 2.0, y + h / 2.0, w, h), Kind::Shape(sh))
    }
    fn image_layer(id: u64, img: Img, x: f32, y: f32, w: f32, h: f32) -> Layer {
        Layer::new(id, "i", Xf::new(x + w / 2.0, y + h / 2.0, w, h), Kind::Image(ImageL { px: Arc::new(img), key: 0 }))
    }

    #[test]
    fn layer_stack_order_and_opacity() {
        let mut d = Doc::new(100, 100);
        d.layers.push(rect_layer(1, 0.0, 0.0, 100.0, 100.0, Rgba::hex(0x0000ff)));
        d.layers.push(rect_layer(2, 25.0, 25.0, 50.0, 50.0, Rgba::hex(0xff0000)));
        let img = r().image(&d, 1.0);
        assert_eq!(img.get(50, 50), [255, 0, 0, 255]);
        assert_eq!(img.get(5, 5), [0, 0, 255, 255]);
        d.layers[1].opacity = 0.5;
        let p = r().image(&d, 1.0).get(50, 50);
        assert!((p[0] as i32 - 128).abs() <= 1 && (p[2] as i32 - 127).abs() <= 1);
        d.layers[1].visible = false;
        assert_eq!(r().image(&d, 1.0).get(50, 50), [0, 0, 255, 255]);
        // half size: half as many pixels, same picture
        d.layers[1].visible = true;
        d.layers[1].opacity = 1.0;
        let half = r().image(&d, 0.5);
        assert_eq!((half.w, half.h), (50, 50));
        assert_eq!(half.get(25, 25), [255, 0, 0, 255]);
    }

    #[test]
    fn blend_mode_and_group() {
        let mut d = Doc::new(10, 10);
        d.layers.push(rect_layer(1, 0.0, 0.0, 10.0, 10.0, Rgba([128, 128, 128, 255])));
        let mut top = rect_layer(2, 0.0, 0.0, 10.0, 10.0, Rgba([128, 128, 128, 255]));
        top.blend = Blend::Multiply;
        d.layers.push(top.clone());
        let p = r().image(&d, 1.0).get(5, 5);
        assert!((p[0] as i32 - 64).abs() <= 1, "{p:?}");
        // in a group, the group's own mode counts: normal inside an isolated group
        d.layers.pop();
        let g = Layer::new(3, "g", Xf::new(0.0, 0.0, 0.0, 0.0), Kind::Group(vec![top]));
        d.layers.push(g);
        assert!((r().image(&d, 1.0).get(5, 5)[0] as i32 - 128).abs() <= 1);
        if let Some(l) = d.find_mut(3) {
            l.blend = Blend::Screen;
        }
        assert!(r().image(&d, 1.0).get(5, 5)[0] > 180);
    }

    #[test]
    fn images_turned_and_masked() {
        let mut img = Img::filled(20, 10, [0, 255, 0, 255]);
        for y in 0..10 {
            for x in 0..10 {
                img.set(x, y, [255, 0, 0, 255]);
            }
        }
        let mut d = Doc::new(40, 40);
        d.layers.push(image_layer(1, img, 10.0, 15.0, 20.0, 10.0));
        let out = r().image(&d, 1.0);
        assert_eq!(out.get(12, 20), [255, 0, 0, 255]);
        assert_eq!(out.get(27, 20), [0, 255, 0, 255]);
        assert_eq!(out.get(5, 5)[3], 0);
        // half a turn: red goes right
        d.layers[0].xf.rot = 180.0;
        let out = r().image(&d, 1.0);
        assert_eq!(out.get(27, 20), [255, 0, 0, 255]);
        // mirrored back
        d.layers[0].xf.flip_x = true;
        d.layers[0].xf.flip_y = true;
        assert_eq!(r().image(&d, 1.0).get(12, 20), [255, 0, 0, 255]);
        // a mask hiding the right half
        let mut m = Gray::new(2, 1, 255);
        m.set(1, 0, 0);
        d.layers[0].mask = Some(Mask { px: Arc::new(m), key: 0, on: true });
        let out = r().image(&d, 1.0);
        assert_eq!(out.get(12, 20)[3], 255);
        assert_eq!(out.get(28, 20)[3], 0);
        // big picture drawn small uses a smaller copy, same colours
        let big = Img::filled(400, 400, [10, 200, 30, 255]);
        let mut d2 = Doc::new(10, 10);
        d2.layers.push(image_layer(1, big, 0.0, 0.0, 10.0, 10.0));
        assert_eq!(r().image(&d2, 1.0).get(5, 5), [10, 200, 30, 255]);
    }

    #[test]
    fn adjustments_apply_to_their_layer_only() {
        let mut d = Doc::new(20, 10);
        d.layers.push(rect_layer(1, 0.0, 0.0, 10.0, 10.0, Rgba([100, 100, 100, 255])));
        let mut l = rect_layer(2, 10.0, 0.0, 10.0, 10.0, Rgba([100, 100, 100, 255]));
        l.adj = Adjust { brightness: 0.6, ..Adjust::default() };
        d.layers.push(l);
        let out = r().image(&d, 1.0);
        assert_eq!(out.get(5, 5), [100, 100, 100, 255]);
        assert!(out.get(15, 5)[0] > 130);
        // blur spreads past the box
        let mut lone = Doc::new(30, 10);
        lone.layers.push(rect_layer(1, 10.0, 0.0, 10.0, 10.0, Rgba::WHITE));
        lone.layers[0].adj = Adjust { blur: 0.1, ..Adjust::default() };
        let out = r().image(&lone, 1.0);
        assert!(out.get(8, 5)[3] > 20 && out.get(8, 5)[3] < 200, "{:?}", out.get(8, 5));
        d.layers[1].adj = Adjust { blur: 0.1, ..Adjust::default() };
        // part of the document draws the same pixels as the whole
        let whole = r().render(&d, 1.0, IRect::new(0, 0, 20, 10));
        let part = r().render(&d, 1.0, IRect::new(8, 2, 5, 5));
        for y in 2..7 {
            for x in 8..13 {
                let (a, b) = (whole.at(x, y), part.at(x, y));
                assert!(a.iter().zip(b).all(|(p, q)| (p - q).abs() < 1e-4), "{x},{y}");
            }
        }
    }
}
