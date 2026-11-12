//! Adjustments and looks, pixel by pixel. Tone (levels, exposure, brightness, contrast) acts on each channel the
//! same way, so it is folded into a table; colour (temperature, tint, saturation, vibrance) and the looks follow.

use super::doc::{Adjust, Look};

const LUT: usize = 1024;

#[inline]
fn srgb_to_lin(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}
#[inline]
fn lin_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}
#[inline]
pub fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}
#[inline]
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One channel's tone curve, without the table (tests compare both).
pub fn tone(a: &Adjust, c: f32) -> f32 {
    // levels
    let span = (a.white - a.black).max(0.01);
    let mut c = ((c - a.black) / span).clamp(0.0, 1.0);
    if (a.gamma - 1.0).abs() > 1e-4 {
        c = c.powf(1.0 / a.gamma.max(0.05));
    }
    // exposure: ±2 stops, in linear light
    if a.exposure != 0.0 {
        c = lin_to_srgb((srgb_to_lin(c) * 2f32.powf(a.exposure * 2.0)).min(1.0));
    }
    // brightness: lifts or lowers the middle, black and white stay
    if a.brightness != 0.0 {
        c = c.powf(2f32.powf(-a.brightness));
    }
    // contrast: an S around the middle (or its opposite)
    if a.contrast != 0.0 {
        let p = 2f32.powf(a.contrast * 1.6);
        c = if c < 0.5 { 0.5 * (2.0 * c).powf(p) } else { 1.0 - 0.5 * (2.0 - 2.0 * c).powf(p) };
    }
    c.clamp(0.0, 1.0)
}

/// Everything an adjustment needs, worked out once per layer and drawing.
pub struct Prepared {
    lut: Option<Vec<f32>>,
    adj: Adjust,
    colour: bool,
}

impl Prepared {
    pub fn new(a: &Adjust) -> Prepared {
        let d = Adjust::default();
        let tone_used = a.brightness != 0.0 || a.contrast != 0.0 || a.exposure != 0.0 || a.black != d.black || a.white != d.white || a.gamma != d.gamma;
        let lut = tone_used.then(|| (0..LUT).map(|i| tone(a, i as f32 / (LUT - 1) as f32)).collect());
        let colour = a.saturation != 0.0 || a.vibrance != 0.0 || a.temperature != 0.0 || a.tint != 0.0 || (a.look != Look::None && a.look_amount > 0.0);
        Prepared { lut, adj: *a, colour }
    }
    pub fn pointwise(&self) -> bool {
        self.lut.is_some() || self.colour
    }
    /// Straight colour in, straight colour out.
    #[inline]
    pub fn apply(&self, mut c: [f32; 3]) -> [f32; 3] {
        if let Some(lut) = &self.lut {
            for v in &mut c {
                let x = v.clamp(0.0, 1.0) * (LUT - 1) as f32;
                let i = (x as usize).min(LUT - 2);
                let t = x - i as f32;
                *v = lut[i] + (lut[i + 1] - lut[i]) * t;
            }
        }
        if !self.colour {
            return c;
        }
        let a = &self.adj;
        if a.temperature != 0.0 || a.tint != 0.0 {
            let t = a.temperature * 0.18;
            c[0] *= 1.0 + t;
            c[2] *= 1.0 - t;
            c[1] *= 1.0 - a.tint * 0.14 + t.abs() * 0.02;
            c[0] *= 1.0 + a.tint * 0.05;
            c[2] *= 1.0 + a.tint * 0.05;
        }
        if a.saturation != 0.0 || a.vibrance != 0.0 {
            let l = luma(c);
            let mx = c[0].max(c[1]).max(c[2]);
            let mn = c[0].min(c[1]).min(c[2]);
            let sat = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
            // vibrance: mostly the dull colours
            let k = (1.0 + a.saturation) * (1.0 + a.vibrance * (1.0 - sat) * 1.2);
            for v in &mut c {
                *v = l + (*v - l) * k.max(0.0);
            }
        }
        for v in &mut c {
            *v = v.clamp(0.0, 1.0);
        }
        if a.look != Look::None && a.look_amount > 0.0 {
            let l = look(a.look, c);
            let k = a.look_amount.min(1.0);
            for i in 0..3 {
                c[i] += (l[i] - c[i]) * k;
            }
        }
        c
    }
}

/// Darkening (or, below 0, lightening) towards the edges of the box; `u`, `v` from 0 to 1 across it.
#[inline]
pub fn vignette(amount: f32, u: f32, v: f32) -> f32 {
    let (dx, dy) = ((u - 0.5) * 2.0, (v - 0.5) * 2.0);
    let d = ((dx * dx + dy * dy) / 2.0).sqrt();
    smoothstep(0.3, 1.0, d) * amount.abs()
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let m = |x: f32, y: f32| (x + (y - x) * t).clamp(0.0, 1.0);
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])]
}
fn s_curve(c: f32, k: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    let p = 1.0 + k;
    if c < 0.5 { 0.5 * (2.0 * c).powf(p) } else { 1.0 - 0.5 * (2.0 - 2.0 * c).powf(p) }
}

/// The looks: each a full-strength recipe on a straight colour.
pub fn look(l: Look, c: [f32; 3]) -> [f32; 3] {
    let y = luma(c);
    let grey = [y, y, y];
    let out = match l {
        Look::None => c,
        Look::Vivid => {
            let s = mix3(grey, c, 1.35);
            [s_curve(s[0], 0.25), s_curve(s[1], 0.25), s_curve(s[2], 0.25)]
        }
        Look::Warm => {
            let w = [c[0] * 1.08 + 0.02, c[1] * 1.01 + 0.01, c[2] * 0.86];
            mix3(grey, w, 1.08)
        }
        Look::Cool => {
            let k = [c[0] * 0.9, c[1] * 1.0 + 0.01, c[2] * 1.1 + 0.03];
            mix3(grey, k, 0.95)
        }
        Look::Mono => [y, y, y],
        Look::Noir => {
            let v = s_curve(y, 0.9).powf(1.1);
            [v, v, v]
        }
        Look::Vintage => {
            let s = [y * 1.07 + 0.06, y * 0.95 + 0.04, y * 0.75 + 0.03];
            let m = mix3(c, s, 0.65);
            [m[0] * 0.92 + 0.05, m[1] * 0.92 + 0.04, m[2] * 0.9 + 0.05]
        }
        Look::Faded => {
            let m = mix3(grey, c, 0.7);
            [0.1 + m[0] * 0.82, 0.1 + m[1] * 0.82, 0.12 + m[2] * 0.8]
        }
        Look::Film => {
            // teal in the shadows, amber in the light
            let sh = 1.0 - smoothstep(0.0, 0.55, y);
            let hi = smoothstep(0.45, 1.0, y);
            let m = [c[0] - 0.05 * sh + 0.06 * hi, c[1] + 0.02 * sh + 0.02 * hi, c[2] + 0.06 * sh - 0.06 * hi];
            [s_curve(m[0], 0.15), s_curve(m[1], 0.15), s_curve(m[2], 0.15)]
        }
        Look::Dusk => {
            let hi = smoothstep(0.3, 1.0, y);
            let m = [c[0] + 0.07 * hi, c[1] * 0.94, c[2] + 0.09 * hi + 0.02];
            mix3(grey, m, 1.05)
        }
        Look::Matte => {
            let m = mix3(grey, c, 0.85);
            [0.07 + m[0] * 0.88, 0.08 + m[1] * 0.88, 0.08 + m[2] * 0.86]
        }
        Look::Bright => {
            let m = [c[0].powf(0.8), c[1].powf(0.8), c[2].powf(0.8)];
            mix3([luma(m); 3], m, 1.12)
        }
    };
    [out[0].clamp(0.0, 1.0), out[1].clamp(0.0, 1.0), out[2].clamp(0.0, 1.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_does_nothing() {
        let p = Prepared::new(&Adjust::default());
        assert!(!p.pointwise());
        assert_eq!(p.apply([0.2, 0.5, 0.9]), [0.2, 0.5, 0.9]);
        assert!(Adjust::default().is_identity());
        assert!(Adjust { look: Look::Mono, look_amount: 0.0, ..Adjust::default() }.is_identity());
        assert!(!Adjust { look: Look::Mono, ..Adjust::default() }.is_identity());
    }

    #[test]
    fn tone_moves_the_right_way() {
        let a = |f: fn(&mut Adjust)| {
            let mut a = Adjust::default();
            f(&mut a);
            a
        };
        let bright = a(|a| a.brightness = 0.5);
        assert!(tone(&bright, 0.5) > 0.6 && tone(&bright, 0.0) == 0.0 && tone(&bright, 1.0) == 1.0);
        let dark = a(|a| a.exposure = -0.5);
        assert!(tone(&dark, 0.5) < 0.4);
        let contrast = a(|a| a.contrast = 0.5);
        assert!(tone(&contrast, 0.25) < 0.25 && tone(&contrast, 0.75) > 0.75 && (tone(&contrast, 0.5) - 0.5).abs() < 1e-4);
        let levels = a(|a| {
            a.black = 0.2;
            a.white = 0.8;
        });
        assert_eq!(tone(&levels, 0.1), 0.0);
        assert_eq!(tone(&levels, 0.9), 1.0);
        assert!((tone(&levels, 0.5) - 0.5).abs() < 1e-4);
        // the table agrees with the curve
        let p = Prepared::new(&contrast);
        let got = p.apply([0.3, 0.3, 0.3])[0];
        assert!((got - tone(&contrast, 0.3)).abs() < 0.003);
    }

    #[test]
    fn colour_moves_the_right_way() {
        let p = Prepared::new(&Adjust { saturation: -1.0, ..Adjust::default() });
        let g = p.apply([0.8, 0.2, 0.2]);
        assert!((g[0] - g[1]).abs() < 1e-4 && (g[1] - g[2]).abs() < 1e-4);
        let warm = Prepared::new(&Adjust { temperature: 1.0, ..Adjust::default() }).apply([0.5, 0.5, 0.5]);
        assert!(warm[0] > 0.5 && warm[2] < 0.5);
        let mono = look(Look::Mono, [1.0, 0.0, 0.0]);
        assert!((mono[0] - 0.2126).abs() < 1e-4 && mono[0] == mono[2]);
        // every look stays in range and changes a mid colour, even from a colour pushed past white
        for l in Look::ALL {
            let o = look(l, [1.0, 0.0, 0.05]);
            assert!(o.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{l:?} {o:?}");
            let o = look(l, [0.6, 0.4, 0.3]);
            assert!(o.iter().all(|v| (0.0..=1.0).contains(v)));
            assert_ne!(o, [0.6, 0.4, 0.3], "{l:?}");
        }
        assert!(vignette(1.0, 0.5, 0.5) < 1e-4 && vignette(1.0, 0.0, 0.0) > 0.9);
    }
}
