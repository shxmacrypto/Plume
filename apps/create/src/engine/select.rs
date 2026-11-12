//! Selections: a rectangle, an ellipse or a free outline (lasso), kept as its shape and as coverage over the
//! document (smooth edges), possibly inverted.

use super::img::Gray;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub enum SelShape {
    Rect(f32, f32, f32, f32),
    Ellipse(f32, f32, f32, f32),
    Poly(Vec<(f32, f32)>),
}

#[derive(Clone, Debug)]
pub struct Selection {
    pub shape: SelShape,
    pub invert: bool,
    /// Coverage over the whole document, one value per document pixel.
    pub mask: Arc<Gray>,
}

/// Fills a polygon on a grid of `w` × `h` pixels: four rows of samples per pixel, exact span ends across.
pub fn fill_poly(pts: &[(f32, f32)], w: u32, h: u32) -> Gray {
    let mut acc = vec![0f32; w as usize * h as usize];
    if pts.len() >= 3 {
        const SUB: usize = 4;
        let mut xs: Vec<f32> = Vec::new();
        let (mut y0, mut y1) = (f32::MAX, f32::MIN);
        for p in pts {
            y0 = y0.min(p.1);
            y1 = y1.max(p.1);
        }
        let first = (y0.floor().max(0.0)) as u32;
        let last = (y1.ceil().max(0.0) as u32).min(h);
        for py in first..last {
            for sub in 0..SUB {
                let y = py as f32 + (sub as f32 + 0.5) / SUB as f32;
                xs.clear();
                for i in 0..pts.len() {
                    let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                    if (a.1 <= y && b.1 > y) || (b.1 <= y && a.1 > y) {
                        xs.push(a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0));
                    }
                }
                xs.sort_by(|a, b| a.total_cmp(b));
                for pair in xs.as_chunks::<2>().0 {
                    let (l, r) = (pair[0].max(0.0), pair[1].min(w as f32));
                    if r <= l {
                        continue;
                    }
                    let row = py as usize * w as usize;
                    let (li, ri) = (l.floor() as usize, (r.ceil() as usize).min(w as usize));
                    for x in li..ri {
                        let cover = ((x + 1) as f32).min(r) - (x as f32).max(l);
                        acc[row + x] += cover.max(0.0) / SUB as f32;
                    }
                }
            }
        }
    }
    Gray { w, h, data: acc.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect() }
}

fn ellipse_pts(x: f32, y: f32, w: f32, h: f32) -> Vec<(f32, f32)> {
    let n = ((w + h) * 0.6).clamp(32.0, 720.0) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            (x + w / 2.0 + t.cos() * w / 2.0, y + h / 2.0 + t.sin() * h / 2.0)
        })
        .collect()
}

impl SelShape {
    /// The outline as points (drawing the "marching ants", rasterizing).
    pub fn outline(&self) -> Vec<(f32, f32)> {
        match self {
            SelShape::Rect(x, y, w, h) => vec![(*x, *y), (x + w, *y), (x + w, y + h), (*x, y + h)],
            SelShape::Ellipse(x, y, w, h) => ellipse_pts(*x, *y, *w, *h),
            SelShape::Poly(p) => p.clone(),
        }
    }
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let pts = self.outline();
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (x, y) in pts {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        (x0, y0, x1, y1)
    }
}

impl Selection {
    pub fn new(shape: SelShape, doc_w: u32, doc_h: u32) -> Selection {
        let mask = Arc::new(fill_poly(&shape.outline(), doc_w, doc_h));
        Selection { shape, invert: false, mask }
    }
    /// Coverage (0 to 1) at a point of the document.
    #[inline]
    pub fn at(&self, x: f32, y: f32) -> f32 {
        let v = self.mask.sample(x, y, 0.0);
        if self.invert { 1.0 - v } else { v }
    }
    /// Its bounds in the document, clamped to it.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let (w, h) = (self.mask.w as f32, self.mask.h as f32);
        if self.invert {
            return (0.0, 0.0, w, h);
        }
        let (x0, y0, x1, y1) = self.shape.bounds();
        (x0.max(0.0), y0.max(0.0), x1.min(w), y1.min(h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_cover_their_inside() {
        let s = Selection::new(SelShape::Rect(10.0, 10.0, 20.0, 10.0), 50, 50);
        assert_eq!(s.mask.get(15, 15), 255);
        assert_eq!(s.mask.get(5, 15), 0);
        assert_eq!(s.mask.get(30, 15), 0);
        assert!(s.at(15.5, 15.5) > 0.99 && s.at(40.0, 40.0) == 0.0);
        let e = Selection::new(SelShape::Ellipse(0.0, 0.0, 40.0, 40.0), 50, 50);
        assert_eq!(e.mask.get(20, 20), 255);
        assert_eq!(e.mask.get(1, 1), 0);
        let mut inv = Selection::new(SelShape::Poly(vec![(0.0, 0.0), (20.0, 0.0), (0.0, 20.0)]), 30, 30);
        assert_eq!(inv.mask.get(3, 3), 255);
        assert_eq!(inv.mask.get(18, 18), 0);
        // a pixel cut by the diagonal: about half
        let half = inv.mask.get(10, 9) as i32;
        assert!((half - 128).abs() < 40, "{half}");
        inv.invert = true;
        assert!(inv.at(18.5, 18.5) > 0.99);
        assert_eq!(inv.bounds(), (0.0, 0.0, 30.0, 30.0));
    }
}
