//! Blend modes: how a layer's colour meets what is under it. The usual formulas of the compositing standard,
//! on straight colours, then mixed by both opacities ("source over").

use super::{doc::Blend, img::P4};

#[inline]
fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}
fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut o = c;
    if n < 0.0 && l - n > 1e-6 {
        for v in &mut o {
            *v = l + (*v - l) * l / (l - n);
        }
    }
    if x > 1.0 && x - l > 1e-6 {
        for v in &mut o {
            *v = l + (*v - l) * (1.0 - l) / (x - l);
        }
    }
    o
}
fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}
fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}
fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    if mx - mn <= 1e-6 {
        return [0.0; 3];
    }
    let mut o = [0.0; 3];
    for i in 0..3 {
        o[i] = (c[i] - mn) * s / (mx - mn);
    }
    o
}

#[inline]
fn sep(mode: Blend, b: f32, s: f32) -> f32 {
    match mode {
        Blend::Multiply => b * s,
        Blend::Screen => b + s - b * s,
        Blend::Overlay => sep(Blend::HardLight, s, b),
        Blend::Darken => b.min(s),
        Blend::Lighten => b.max(s),
        Blend::ColorDodge => {
            if b <= 0.0 {
                0.0
            } else if s >= 1.0 {
                1.0
            } else {
                (b / (1.0 - s)).min(1.0)
            }
        }
        Blend::ColorBurn => {
            if b >= 1.0 {
                1.0
            } else if s <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / s).min(1.0)
            }
        }
        Blend::HardLight => {
            if s <= 0.5 {
                b * 2.0 * s
            } else {
                let t = 2.0 * s - 1.0;
                b + t - b * t
            }
        }
        Blend::SoftLight => {
            if s <= 0.5 {
                b - (1.0 - 2.0 * s) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
                b + (2.0 * s - 1.0) * (d - b)
            }
        }
        Blend::Difference => (b - s).abs(),
        Blend::Exclusion => b + s - 2.0 * b * s,
        _ => s,
    }
}

/// The blended colour (straight) of `s` over `b`.
#[inline]
pub fn mix(mode: Blend, b: [f32; 3], s: [f32; 3]) -> [f32; 3] {
    match mode {
        Blend::Normal => s,
        Blend::Hue => set_lum(set_sat(s, sat(b)), lum(b)),
        Blend::Saturation => set_lum(set_sat(b, sat(s)), lum(b)),
        Blend::Color => set_lum(s, lum(b)),
        Blend::Luminosity => set_lum(b, lum(s)),
        m => [sep(m, b[0], s[0]), sep(m, b[1], s[1]), sep(m, b[2], s[2])],
    }
}

/// Lays `src` (premultiplied) over `dst` (premultiplied), with an extra opacity.
#[inline]
pub fn over(mode: Blend, dst: P4, src: P4, opacity: f32) -> P4 {
    let sa = src[3] * opacity;
    if sa <= 0.0 {
        return dst;
    }
    let da = dst[3];
    if mode == Blend::Normal || da <= 0.0 {
        let k = 1.0 - sa;
        return [src[0] * opacity + dst[0] * k, src[1] * opacity + dst[1] * k, src[2] * opacity + dst[2] * k, sa + da * k];
    }
    let sc = [src[0] / src[3], src[1] / src[3], src[2] / src[3]];
    let bc = [dst[0] / da, dst[1] / da, dst[2] / da];
    let m = mix(mode, bc, sc);
    let mut o = [0.0; 4];
    for i in 0..3 {
        o[i] = sa * (1.0 - da) * sc[i] + sa * da * m[i] + (1.0 - sa) * da * bc[i];
    }
    o[3] = sa + da * (1.0 - sa);
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: P4, b: P4) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
    }

    #[test]
    fn normal_and_opacity() {
        let red = [1.0, 0.0, 0.0, 1.0];
        let blue = [0.0, 0.0, 1.0, 1.0];
        assert!(close(over(Blend::Normal, blue, red, 1.0), red));
        assert!(close(over(Blend::Normal, blue, red, 0.5), [0.5, 0.0, 0.5, 1.0]));
        // over nothing: the layer itself
        assert!(close(over(Blend::Multiply, [0.0; 4], [0.25, 0.0, 0.0, 0.5], 1.0), [0.25, 0.0, 0.0, 0.5]));
    }

    #[test]
    fn modes() {
        let grey = [0.5, 0.5, 0.5, 1.0];
        let white = [1.0, 1.0, 1.0, 1.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        assert!(close(over(Blend::Multiply, grey, white, 1.0), grey));
        assert!(close(over(Blend::Multiply, grey, black, 1.0), black));
        assert!(close(over(Blend::Screen, grey, black, 1.0), grey));
        assert!(close(over(Blend::Screen, grey, white, 1.0), white));
        assert!(close(over(Blend::Difference, white, white, 1.0), black));
        assert!(close(over(Blend::Darken, [0.2, 0.8, 0.5, 1.0], [0.6, 0.3, 0.5, 1.0], 1.0), [0.2, 0.3, 0.5, 1.0]));
        assert!(close(over(Blend::Lighten, [0.2, 0.8, 0.5, 1.0], [0.6, 0.3, 0.5, 1.0], 1.0), [0.6, 0.8, 0.5, 1.0]));
        // overlay keeps black and white of the bottom, lightens or darkens the middle
        assert!(close(over(Blend::Overlay, black, white, 1.0), black));
        let o = over(Blend::Overlay, [0.6, 0.6, 0.6, 1.0], white, 1.0);
        assert!(o[0] > 0.6);
        // colour: the hue of the top, the lightness of the bottom
        let c = over(Blend::Color, grey, [1.0, 0.0, 0.0, 1.0], 1.0);
        assert!(c[0] > c[1] && (lum([c[0], c[1], c[2]]) - 0.5).abs() < 0.01);
        let l = over(Blend::Luminosity, [1.0, 0.0, 0.0, 1.0], white, 1.0);
        assert!((lum([l[0], l[1], l[2]]) - 1.0).abs() < 0.02);
        for m in Blend::ALL {
            let o = over(m, [0.3, 0.6, 0.9, 1.0], [0.7, 0.2, 0.4, 0.8], 0.9);
            assert!(o.iter().all(|v| v.is_finite() && *v >= -1e-4 && *v <= 1.0001), "{m:?} {o:?}");
        }
    }
}
