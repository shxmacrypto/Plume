//! Pixels. Layer pictures are kept as straight RGBA (8 bits a channel), masks and selections as 8-bit grey.
//! Rendering works on premultiplied floats ([`P4`]).

/// Premultiplied RGBA, 0 to 1.
pub type P4 = [f32; 4];

/// Straight (not premultiplied) RGBA, 8 bits per channel, row after row.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Img {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Img {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Img({}x{})", self.w, self.h)
    }
}

impl Img {
    pub fn new(w: u32, h: u32) -> Img {
        Img { w, h, data: vec![0; w as usize * h as usize * 4] }
    }
    #[cfg(test)]
    pub fn filled(w: u32, h: u32, c: [u8; 4]) -> Img {
        let mut data = Vec::with_capacity(w as usize * h as usize * 4);
        for _ in 0..w as usize * h as usize {
            data.extend_from_slice(&c);
        }
        Img { w, h, data }
    }
    #[inline]
    pub fn idx(&self, x: u32, y: u32) -> usize {
        (y as usize * self.w as usize + x as usize) * 4
    }
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = self.idx(x, y);
        [self.data[i], self.data[i + 1], self.data[i + 2], self.data[i + 3]]
    }
    #[inline]
    pub fn set(&mut self, x: u32, y: u32, c: [u8; 4]) {
        let i = self.idx(x, y);
        self.data[i..i + 4].copy_from_slice(&c);
    }
    /// From Windows' BGRA premultiplied pixels (WIC).
    pub fn from_bgra_premul(w: u32, h: u32, src: &[u8]) -> Img {
        let mut data = vec![0u8; w as usize * h as usize * 4];
        for (d, s) in data.as_chunks_mut::<4>().0.iter_mut().zip(src.as_chunks::<4>().0.iter()) {
            let a = s[3];
            if a == 0 {
                continue;
            }
            let un = |v: u8| ((v as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
            d[0] = un(s[2]);
            d[1] = un(s[1]);
            d[2] = un(s[0]);
            d[3] = a;
        }
        Img { w, h, data }
    }
    /// Windows' BGRA premultiplied pixels (Direct2D, WIC).
    pub fn to_bgra_premul(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.data.len()];
        for (d, s) in out.as_chunks_mut::<4>().0.iter_mut().zip(self.data.as_chunks::<4>().0.iter()) {
            let a = s[3] as u32;
            let pm = |v: u8| ((v as u32 * a + 127) / 255) as u8;
            d[0] = pm(s[2]);
            d[1] = pm(s[1]);
            d[2] = pm(s[0]);
            d[3] = s[3];
        }
        out
    }
    /// The pixel as premultiplied floats.
    #[inline]
    pub fn p4(&self, x: u32, y: u32) -> P4 {
        let i = self.idx(x, y);
        let a = self.data[i + 3] as f32 / 255.0;
        [self.data[i] as f32 / 255.0 * a, self.data[i + 1] as f32 / 255.0 * a, self.data[i + 2] as f32 / 255.0 * a, a]
    }
    /// Bilinear sample at a position in pixels (pixel centres at .5); transparent outside.
    #[inline]
    pub fn sample(&self, x: f32, y: f32) -> P4 {
        let fx = x - 0.5;
        let fy = y - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (w, h) = (self.w as i64, self.h as i64);
        let at = |x: i64, y: i64| -> P4 { if x < 0 || y < 0 || x >= w || y >= h { [0.0; 4] } else { self.p4(x as u32, y as u32) } };
        let (a, b, c, d) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
        let mut o = [0.0; 4];
        for k in 0..4 {
            let top = a[k] + (b[k] - a[k]) * tx;
            let bot = c[k] + (d[k] - c[k]) * tx;
            o[k] = top + (bot - top) * ty;
        }
        o
    }
    /// Half the size (2 × 2 average, weighted by opacity): the levels used when a picture is drawn much smaller.
    pub fn half(&self) -> Img {
        let (nw, nh) = ((self.w / 2).max(1), (self.h / 2).max(1));
        let mut out = Img::new(nw, nh);
        for y in 0..nh {
            for x in 0..nw {
                let mut acc = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = self.p4((x * 2 + dx).min(self.w - 1), (y * 2 + dy).min(self.h - 1));
                    for k in 0..4 {
                        acc[k] += p[k] * 0.25;
                    }
                }
                out.set(x, y, unpremul8(acc));
            }
        }
        out
    }
    /// Resized with an area average (smaller) or bilinear (larger).
    pub fn resized(&self, nw: u32, nh: u32) -> Img {
        let (nw, nh) = (nw.max(1), nh.max(1));
        let mut src = std::borrow::Cow::Borrowed(self);
        while src.w / 2 >= nw && src.h / 2 >= nh {
            src = std::borrow::Cow::Owned(src.half());
        }
        let mut out = Img::new(nw, nh);
        let (kx, ky) = (src.w as f32 / nw as f32, src.h as f32 / nh as f32);
        for y in 0..nh {
            for x in 0..nw {
                let p = src.sample((x as f32 + 0.5) * kx, (y as f32 + 0.5) * ky);
                // edges: the nearest pixel (bilinear would fade them into the transparent outside)
                let p = if p[3] < 0.999 && src.p4(((x as f32 + 0.5) * kx) as u32, ((y as f32 + 0.5) * ky) as u32)[3] >= 0.999 {
                    src.p4(((x as f32 + 0.5) * kx) as u32, ((y as f32 + 0.5) * ky) as u32)
                } else {
                    p
                };
                out.set(x, y, unpremul8(p));
            }
        }
        out
    }
}

/// Premultiplied floats → straight 8-bit RGBA.
#[inline]
pub fn unpremul8(p: P4) -> [u8; 4] {
    let a = p[3].clamp(0.0, 1.0);
    if a <= 0.0 {
        return [0, 0, 0, 0];
    }
    let c = |v: f32| ((v / a).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    [c(p[0]), c(p[1]), c(p[2]), (a * 255.0 + 0.5) as u8]
}

/// Straight 8-bit RGBA → premultiplied floats.
#[inline]
pub fn premul(c: [u8; 4]) -> P4 {
    let a = c[3] as f32 / 255.0;
    [c[0] as f32 / 255.0 * a, c[1] as f32 / 255.0 * a, c[2] as f32 / 255.0 * a, a]
}

/// One channel of 8 bits: masks (white shows, black hides) and selections.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Gray {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Gray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Gray({}x{})", self.w, self.h)
    }
}

impl Gray {
    pub fn new(w: u32, h: u32, v: u8) -> Gray {
        Gray { w, h, data: vec![v; w as usize * h as usize] }
    }
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> u8 {
        self.data[y as usize * self.w as usize + x as usize]
    }
    #[inline]
    pub fn set(&mut self, x: u32, y: u32, v: u8) {
        let w = self.w as usize;
        self.data[y as usize * w + x as usize] = v;
    }
    /// Bilinear sample (0 to 1) at a position in pixels; beyond the edges, the edge's value.
    #[inline]
    pub fn sample_edge(&self, x: f32, y: f32) -> f32 {
        self.sample(x.clamp(0.5, self.w as f32 - 0.5), y.clamp(0.5, self.h as f32 - 0.5), 0.0)
    }
    /// Bilinear sample (0 to 1) at a position in pixels; `outside` beyond the edges.
    #[inline]
    pub fn sample(&self, x: f32, y: f32, outside: f32) -> f32 {
        let fx = x - 0.5;
        let fy = y - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (w, h) = (self.w as i64, self.h as i64);
        let at = |x: i64, y: i64| -> f32 {
            if x < 0 || y < 0 || x >= w || y >= h { outside } else { self.data[y as usize * w as usize + x as usize] as f32 / 255.0 }
        };
        let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * tx;
        let bot = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * tx;
        top + (bot - top) * ty
    }
}

/// A rectangle of whole pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl IRect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> IRect {
        IRect { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
    pub fn intersect(&self, o: &IRect) -> IRect {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        IRect::new(x, y, (self.right().min(o.right()) - x).max(0), (self.bottom().min(o.bottom()) - y).max(0))
    }
    #[cfg(test)]
    pub fn union(&self, o: &IRect) -> IRect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        IRect::new(x, y, self.right().max(o.right()) - x, self.bottom().max(o.bottom()) - y)
    }
    pub fn grow(&self, d: i32) -> IRect {
        IRect::new(self.x - d, self.y - d, self.w + 2 * d, self.h + 2 * d)
    }
    /// The whole pixels covering a rectangle in floats.
    pub fn covering(x0: f32, y0: f32, x1: f32, y1: f32) -> IRect {
        let (a, b) = (x0.floor() as i32, y0.floor() as i32);
        IRect::new(a, b, x1.ceil() as i32 - a, y1.ceil() as i32 - b)
    }
}

/// Premultiplied float pixels over a rectangle of the output.
#[derive(Clone, Debug)]
pub struct Buf {
    pub r: IRect,
    pub px: Vec<P4>,
}

impl Buf {
    pub fn new(r: IRect) -> Buf {
        Buf { r, px: vec![[0.0; 4]; (r.w.max(0) * r.h.max(0)) as usize] }
    }
    #[cfg(test)]
    pub fn at(&self, x: i32, y: i32) -> P4 {
        if x < self.r.x || y < self.r.y || x >= self.r.right() || y >= self.r.bottom() {
            return [0.0; 4];
        }
        self.px[((y - self.r.y) * self.r.w + (x - self.r.x)) as usize]
    }
    /// Straight 8-bit RGBA of the whole buffer.
    pub fn to_img(&self) -> Img {
        let mut img = Img::new(self.r.w as u32, self.r.h as u32);
        for (d, p) in img.data.as_chunks_mut::<4>().0.iter_mut().zip(&self.px) {
            d.copy_from_slice(&unpremul8(*p));
        }
        img
    }
    /// BGRA premultiplied 8 bits (Direct2D).
    pub fn to_bgra(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.px.len() * 4];
        for (d, p) in out.as_chunks_mut::<4>().0.iter_mut().zip(&self.px) {
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            d[0] = q(p[2]);
            d[1] = q(p[1]);
            d[2] = q(p[0]);
            d[3] = q(p[3]);
        }
        out
    }
}

/// Runs `f` on bands of rows in parallel: `f(first_row, rows)` gets a slice of `w` items per row.
pub fn par_rows<T: Send>(data: &mut [T], w: usize, f: impl Fn(usize, &mut [T]) + Sync) {
    if w == 0 || data.is_empty() {
        return;
    }
    let rows = data.len() / w;
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 16);
    if rows < 32 || threads == 1 {
        f(0, data);
        return;
    }
    let per = rows.div_ceil(threads * 2);
    std::thread::scope(|s| {
        for (k, chunk) in data.chunks_mut(per * w).enumerate() {
            let f = &f;
            s.spawn(move || f(k * per, chunk));
        }
    });
}

/// Three box blurs: close to a gaussian of the same spread. Works on any `[f32; N]` pixels, in place, over a `w × h`
/// grid, with `radius` in pixels (fractional radii blend two box sizes).
pub fn blur<const N: usize>(px: &mut [[f32; N]], w: usize, h: usize, radius: f32) {
    if radius < 0.3 || w == 0 || h == 0 {
        return;
    }
    // three passes whose combined spread matches the gaussian's sigma = radius / 2
    let sigma = radius / 2.0;
    let r = ((12.0 * sigma * sigma / 3.0 + 1.0).sqrt() / 2.0).max(0.5);
    for _ in 0..3 {
        box_h(px, w, h, r);
        box_v(px, w, h, r);
    }
}

fn box_line<const N: usize>(line: &[[f32; N]], out: &mut [[f32; N]], r: f32) {
    let n = line.len() as i64;
    let ri = r.floor() as i64;
    let frac = r - ri as f32;
    let norm = 1.0 / (2.0 * r + 1.0);
    let get = |i: i64| -> [f32; N] { if i < 0 || i >= n { [0.0; N] } else { line[i as usize] } };
    let mut acc = [0.0f32; N];
    for i in -ri..=ri {
        let p = get(i);
        for k in 0..N {
            acc[k] += p[k];
        }
    }
    for i in 0..n {
        let (a, b) = (get(i - ri - 1), get(i + ri + 1));
        let mut o = [0.0; N];
        for k in 0..N {
            o[k] = (acc[k] + (a[k] + b[k]) * frac) * norm;
        }
        out[i as usize] = o;
        let (add, sub) = (get(i + ri + 1), get(i - ri));
        for k in 0..N {
            acc[k] += add[k] - sub[k];
        }
    }
}

fn box_h<const N: usize>(px: &mut [[f32; N]], w: usize, _h: usize, r: f32) {
    par_rows(px, w, |_, rows| {
        let mut tmp = vec![[0.0f32; N]; w];
        for row in rows.chunks_exact_mut(w) {
            box_line(row, &mut tmp, r);
            row.copy_from_slice(&tmp);
        }
    });
}

fn box_v<const N: usize>(px: &mut [[f32; N]], w: usize, h: usize, r: f32) {
    // columns through a transposed copy, so the rows code (parallel) does the work
    let mut t = vec![[0.0f32; N]; w * h];
    for y in 0..h {
        for x in 0..w {
            t[x * h + y] = px[y * w + x];
        }
    }
    box_h(&mut t, h, w, r);
    for y in 0..h {
        for x in 0..w {
            px[y * w + x] = t[x * h + y];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplied_round_trip() {
        let mut img = Img::new(2, 1);
        img.set(0, 0, [200, 100, 50, 128]);
        img.set(1, 0, [10, 20, 30, 255]);
        let back = Img::from_bgra_premul(2, 1, &img.to_bgra_premul());
        let a = back.get(0, 0);
        assert!((a[0] as i32 - 200).abs() <= 2 && (a[1] as i32 - 100).abs() <= 2 && a[3] == 128);
        assert_eq!(back.get(1, 0), [10, 20, 30, 255]);
        assert_eq!(unpremul8(premul([40, 80, 120, 255])), [40, 80, 120, 255]);
    }

    #[test]
    fn sampling_and_halving() {
        let img = Img::filled(4, 4, [255, 0, 0, 255]);
        let p = img.sample(2.0, 2.0);
        assert!((p[0] - 1.0).abs() < 1e-5 && (p[3] - 1.0).abs() < 1e-5);
        // half way out of the picture: half transparent
        let edge = img.sample(0.0, 2.0);
        assert!((edge[3] - 0.5).abs() < 1e-5);
        let h = img.half();
        assert_eq!((h.w, h.h), (2, 2));
        assert_eq!(h.get(1, 1), [255, 0, 0, 255]);
        let r = img.resized(3, 7);
        assert_eq!((r.w, r.h), (3, 7));
        assert_eq!(r.get(1, 3), [255, 0, 0, 255]);
    }

    #[test]
    fn blur_keeps_the_total() {
        let (w, h) = (40, 40);
        let mut px = vec![[0.0f32; 1]; w * h];
        px[20 * w + 20] = [1.0];
        blur(&mut px, w, h, 6.0);
        let total: f32 = px.iter().map(|p| p[0]).sum();
        assert!((total - 1.0).abs() < 0.02, "{total}");
        assert!(px[20 * w + 20][0] < 0.2 && px[20 * w + 24][0] > 0.0);
    }

    #[test]
    fn rects() {
        let a = IRect::new(0, 0, 10, 10);
        assert_eq!(a.intersect(&IRect::new(5, 5, 10, 10)), IRect::new(5, 5, 5, 5));
        assert_eq!(a.union(&IRect::new(5, 5, 10, 10)), IRect::new(0, 0, 15, 15));
        assert!(a.intersect(&IRect::new(20, 0, 5, 5)).is_empty());
        assert_eq!(IRect::covering(0.5, 0.5, 2.2, 3.0), IRect::new(0, 0, 3, 3));
    }
}
