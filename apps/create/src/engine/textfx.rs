//! Text effects on the letters' coverage: an outline (the letters grown by a distance map) and a soft shadow.

use super::{
    doc::TextL,
    img::{P4, blur},
};

/// Letters drawn as coverage (0 to 1), `pad` pixels of room on each side for the effects.
pub struct Glyphs {
    pub w: u32,
    pub h: u32,
    pub a: Vec<f32>,
    pub pad: u32,
}

const FAR: f32 = 1e20;

/// Squared distance transform of one line (lower envelope of parabolas).
fn edt_line(f: &[f32], d: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f32::MIN;
    z[1] = f32::MAX;
    let cross = |q: usize, p: usize| ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32);
    for q in 1..n {
        let mut s = cross(q, v[k]);
        // z[0] is the lowest number there is: k never goes below 0
        while s <= z[k] && k > 0 {
            k -= 1;
            s = cross(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f32::MAX;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        let dq = q as f32 - p as f32;
        *out = dq * dq + f[p];
    }
}

/// Distance (pixels) from each pixel to the nearest pixel where `inside` holds.
pub fn distance(w: usize, h: usize, inside: impl Fn(usize) -> bool) -> Vec<f32> {
    let mut g: Vec<f32> = (0..w * h).map(|i| if inside(i) { 0.0 } else { FAR }).collect();
    let n = w.max(h);
    let (mut f, mut d, mut v, mut z) = (vec![0.0; n], vec![0.0; n], vec![0usize; n], vec![0.0f32; n + 1]);
    for x in 0..w {
        for y in 0..h {
            f[y] = g[y * w + x];
        }
        edt_line(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            g[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&g[y * w..y * w + w]);
        edt_line(&f[..w], &mut d[..w], &mut v, &mut z);
        g[y * w..y * w + w].copy_from_slice(&d[..w]);
    }
    for x in &mut g {
        *x = x.sqrt();
    }
    g
}

/// The coloured letters with their outline and shadow, premultiplied, same size as the coverage.
pub fn compose(t: &TextL, g: &Glyphs, scale: f32) -> Vec<P4> {
    let (w, h) = (g.w as usize, g.h as usize);
    let mut out = vec![[0.0f32; 4]; w * h];
    let lay = |out: &mut Vec<P4>, a: &[f32], c: [f32; 4]| {
        for (o, &k) in out.iter_mut().zip(a) {
            if k <= 0.0 {
                continue;
            }
            let s = [c[0] * k, c[1] * k, c[2] * k, c[3] * k];
            let r = 1.0 - s[3];
            *o = [s[0] + o[0] * r, s[1] + o[1] * r, s[2] + o[2] * r, s[3] + o[3] * r];
        }
    };
    // what the shadow and the outline follow: the letters, grown by the outline
    let mut body = g.a.clone();
    if let Some((_, ow)) = t.outline
        && ow > 0.0
    {
        let r = ow * scale;
        let dist = distance(w, h, |i| g.a[i] >= 0.5);
        for (i, b) in body.iter_mut().enumerate() {
            *b = b.max((r - dist[i] + 0.5).clamp(0.0, 1.0));
        }
    }
    if let Some((col, dx, dy, radius)) = t.shadow {
        let (ox, oy) = ((dx * scale).round() as i64, (dy * scale).round() as i64);
        let mut sh = vec![[0.0f32; 1]; w * h];
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let (sx, sy) = (x - ox, y - oy);
                if sx >= 0 && sy >= 0 && sx < w as i64 && sy < h as i64 {
                    sh[(y * w as i64 + x) as usize] = [body[(sy * w as i64 + sx) as usize]];
                }
            }
        }
        blur(&mut sh, w, h, radius * scale);
        let a: Vec<f32> = sh.iter().map(|v| v[0]).collect();
        lay(&mut out, &a, col.p4());
    }
    if let Some((col, ow)) = t.outline
        && ow > 0.0
    {
        lay(&mut out, &body, col.p4());
    }
    lay(&mut out, &g.a, t.color.p4());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::doc::Rgba;

    #[test]
    fn distances() {
        // one inside pixel in the middle of 9 × 9
        let d = distance(9, 9, |i| i == 40);
        assert_eq!(d[40], 0.0);
        assert!((d[41] - 1.0).abs() < 1e-4 && (d[44] - 4.0).abs() < 1e-4);
        assert!((d[0] - (32f32).sqrt()).abs() < 1e-3);
    }

    #[test]
    fn outline_and_shadow() {
        let (w, h) = (30u32, 30u32);
        let mut a = vec![0.0; 900];
        for y in 12..18 {
            for x in 12..18 {
                a[y * 30 + x] = 1.0;
            }
        }
        let g = Glyphs { w, h, a, pad: 8 };
        let t = TextL { color: Rgba::WHITE, outline: Some((Rgba::BLACK, 3.0)), shadow: Some((Rgba([255, 0, 0, 255]), 6.0, 0.0, 0.0)), ..TextL::default() };
        let px = compose(&t, &g, 1.0);
        // the letter: white
        assert!(px[15 * 30 + 15][0] > 0.99);
        // the outline: black, 2 pixels out
        let o = px[15 * 30 + 19];
        assert!(o[3] > 0.99 && o[0] < 0.01);
        // the shadow: red, to the right beyond the outline
        let s = px[15 * 30 + 25];
        assert!(s[0] > 0.9 && s[1] < 0.05, "{s:?}");
        assert_eq!(px[2 * 30 + 2][3], 0.0);
    }
}
