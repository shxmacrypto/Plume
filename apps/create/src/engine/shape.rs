//! Shapes drawn from their outline's distance: crisp at any zoom, turned or not, with smooth edges.

use super::{
    doc::{Paint, ShapeKind, ShapeL},
    img::P4,
};

#[inline]
fn cov(d: f32, aa: f32) -> f32 {
    (0.5 - d / aa).clamp(0.0, 1.0)
}

fn round_box(px: f32, py: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let r = r.min(hw).min(hh).max(0.0);
    let qx = px.abs() - hw + r;
    let qy = py.abs() - hh + r;
    let out = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    out + qx.max(qy).min(0.0) - r
}

fn ellipse(px: f32, py: f32, a: f32, b: f32) -> f32 {
    let (a, b) = (a.max(0.01), b.max(0.01));
    let k0 = ((px / a).powi(2) + (py / b).powi(2)).sqrt();
    let k1 = ((px / (a * a)).powi(2) + (py / (b * b)).powi(2)).sqrt();
    if k1 < 1e-9 { -a.min(b) } else { k0 * (k0 - 1.0) / k1 }
}

fn segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((px - ax) * dx + (py - ay) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qy) = (ax + dx * t - px, ay + dy * t - py);
    (qx * qx + qy * qy).sqrt()
}

/// Signed distance to a convex polygon given clockwise or not (exact inside, close outside).
fn convex(px: f32, py: f32, pts: &[(f32, f32)]) -> f32 {
    let n = pts.len();
    // which way round the points go
    let mut area = 0.0;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        area += a.0 * b.1 - b.0 * a.1;
    }
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let mut d = f32::MIN;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let len = (ex * ex + ey * ey).sqrt().max(1e-6);
        // outward normal
        let (nx, ny) = (ey / len * sign, -ex / len * sign);
        d = d.max((px - a.0) * nx + (py - a.1) * ny);
    }
    d
}

/// Size of an arrow's head for a line `w` long.
pub fn arrow_head(sh: &ShapeL, w: f32) -> f32 {
    (sh.stroke_w * 3.2).max(14.0).min(w * 0.6)
}

/// Coverage (fill, stroke) of the point (u, v) of a `w` × `h` box; `aa`: box units per output pixel.
pub fn coverage(sh: &ShapeL, w: f32, h: f32, u: f32, v: f32, aa: f32) -> (f32, f32) {
    let (px, py) = (u - w / 2.0, v - h / 2.0);
    let sw = if sh.stroke.is_some() { sh.stroke_w.max(0.0) } else { 0.0 };
    match sh.kind {
        ShapeKind::Rect | ShapeKind::Ellipse => {
            let d = if sh.kind == ShapeKind::Rect { round_box(px, py, w / 2.0, h / 2.0, sh.radius) } else { ellipse(px, py, w / 2.0, h / 2.0) };
            let fill = if sh.fill.is_some() { cov(d, aa) } else { 0.0 };
            let stroke = if sw > 0.0 { (cov(d, aa) - cov(d + sw, aa)).max(0.0) } else { 0.0 };
            (fill, stroke)
        }
        ShapeKind::Line | ShapeKind::Arrow => {
            let t = sh.stroke_w.max(1.0);
            let y = h / 2.0;
            let mut d;
            if sh.kind == ShapeKind::Arrow {
                let head = arrow_head(sh, w);
                d = segment(u, v, t / 2.0, y, w - head * 0.7, y) - t / 2.0;
                let tri = [(w, y), (w - head, y - head * 0.62), (w - head, y + head * 0.62)];
                d = d.min(convex(u, v, &tri));
            } else {
                d = segment(u, v, t / 2.0, y, w - t / 2.0, y) - t / 2.0;
            }
            (cov(d, aa), 0.0)
        }
    }
}

/// The colour of the fill at (u, v) (premultiplied).
#[inline]
pub fn paint_at(p: &Paint, w: f32, h: f32, u: f32, v: f32) -> P4 {
    match p {
        Paint::Solid(c) => c.p4(),
        Paint::Linear(a, b, angle) => {
            let (s, c) = angle.to_radians().sin_cos();
            let (du, dv) = (u / w.max(1e-3) - 0.5, v / h.max(1e-3) - 0.5);
            let t = (0.5 + (du * c + dv * s) / (c.abs() + s.abs()).max(1e-3)).clamp(0.0, 1.0);
            let (pa, pb) = (a.p4(), b.p4());
            [pa[0] + (pb[0] - pa[0]) * t, pa[1] + (pb[1] - pa[1]) * t, pa[2] + (pb[2] - pa[2]) * t, pa[3] + (pb[3] - pa[3]) * t]
        }
    }
}

/// The shape's colour at (u, v): stroke over fill (lines and arrows: one colour).
#[inline]
pub fn shade(sh: &ShapeL, w: f32, h: f32, u: f32, v: f32, aa: f32) -> P4 {
    let (f, s) = coverage(sh, w, h, u, v, aa);
    if f <= 0.0 && s <= 0.0 {
        return [0.0; 4];
    }
    match sh.kind {
        ShapeKind::Line | ShapeKind::Arrow => {
            let c = sh.stroke.map(|c| c.p4()).or(sh.fill.map(|p| paint_at(&p, w, h, u, v))).unwrap_or([0.0; 4]);
            [c[0] * f, c[1] * f, c[2] * f, c[3] * f]
        }
        _ => {
            let mut o = [0.0; 4];
            if f > 0.0
                && let Some(p) = &sh.fill
            {
                let c = paint_at(p, w, h, u, v);
                o = [c[0] * f, c[1] * f, c[2] * f, c[3] * f];
            }
            if s > 0.0
                && let Some(sc) = sh.stroke
            {
                let c = sc.p4();
                let k = 1.0 - c[3] * s;
                o = [c[0] * s + o[0] * k, c[1] * s + o[1] * k, c[2] * s + o[2] * k, c[3] * s + o[3] * k];
            }
            o
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::doc::Rgba;

    #[test]
    fn rectangles_ellipses_lines() {
        let rect = ShapeL { kind: ShapeKind::Rect, fill: Some(Paint::Solid(Rgba::WHITE)), stroke: None, stroke_w: 0.0, radius: 0.0 };
        assert_eq!(coverage(&rect, 100.0, 50.0, 50.0, 25.0, 1.0).0, 1.0);
        assert_eq!(coverage(&rect, 100.0, 50.0, 102.0, 25.0, 1.0).0, 0.0);
        assert!((coverage(&rect, 100.0, 50.0, 100.0, 25.0, 1.0).0 - 0.5).abs() < 0.01);
        // a round corner leaves the very corner empty
        let round = ShapeL { radius: 20.0, ..rect.clone() };
        assert_eq!(coverage(&round, 100.0, 50.0, 1.0, 1.0, 1.0).0, 0.0);
        let ell = ShapeL { kind: ShapeKind::Ellipse, ..rect.clone() };
        assert_eq!(coverage(&ell, 100.0, 50.0, 50.0, 25.0, 1.0).0, 1.0);
        assert_eq!(coverage(&ell, 100.0, 50.0, 3.0, 3.0, 1.0).0, 0.0);
        assert_eq!(coverage(&ell, 100.0, 50.0, 2.0, 25.0, 1.0).0, 1.0);
        // a stroke lies inside the edge
        let ring = ShapeL { fill: None, stroke: Some(Rgba::BLACK), stroke_w: 6.0, ..rect.clone() };
        assert_eq!(coverage(&ring, 100.0, 50.0, 3.0, 25.0, 1.0), (0.0, 1.0));
        assert_eq!(coverage(&ring, 100.0, 50.0, 50.0, 25.0, 1.0).1, 0.0);
        let line = ShapeL { kind: ShapeKind::Line, stroke_w: 4.0, ..ring.clone() };
        assert_eq!(coverage(&line, 100.0, 20.0, 50.0, 10.0, 1.0).0, 1.0);
        assert_eq!(coverage(&line, 100.0, 20.0, 50.0, 16.0, 1.0).0, 0.0);
        let arrow = ShapeL { kind: ShapeKind::Arrow, ..line.clone() };
        let head = arrow_head(&arrow, 100.0);
        // the head is wider than the line near its base
        assert_eq!(coverage(&arrow, 100.0, 40.0, 100.0 - head * 0.8, 20.0 + head * 0.35, 1.0).0, 1.0);
        assert_eq!(coverage(&line, 100.0, 40.0, 100.0 - head * 0.8, 20.0 + head * 0.35, 1.0).0, 0.0);
    }

    #[test]
    fn gradient_ends() {
        let p = Paint::Linear(Rgba::BLACK, Rgba::WHITE, 0.0);
        assert!(paint_at(&p, 100.0, 10.0, 0.0, 5.0)[0] < 0.01);
        assert!(paint_at(&p, 100.0, 10.0, 100.0, 5.0)[0] > 0.99);
        let down = Paint::Linear(Rgba::BLACK, Rgba::WHITE, 90.0);
        assert!(paint_at(&down, 100.0, 10.0, 50.0, 10.0)[0] > 0.99);
    }
}
