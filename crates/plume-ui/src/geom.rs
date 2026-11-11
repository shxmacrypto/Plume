//! Géométrie et couleurs. Toutes les mesures sont en DIP (1/96 de pouce) : Direct2D applique l'échelle de l'écran.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

pub const fn pt(x: f32, y: f32) -> Pt {
    Pt { x, y }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub const fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, w, h }
}

impl Rect {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn contains(&self, p: Pt) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
    /// Rétrécit de `d` de chaque côté.
    pub fn inset(&self, d: f32) -> Rect {
        self.inset_xy(d, d)
    }
    pub fn inset_xy(&self, dx: f32, dy: f32) -> Rect {
        rect(self.x + dx, self.y + dy, (self.w - 2.0 * dx).max(0.0), (self.h - 2.0 * dy).max(0.0))
    }
    /// Rectangle `w` × `h` centré dans celui-ci.
    pub fn centered(&self, w: f32, h: f32) -> Rect {
        rect(self.x + (self.w - w) / 2.0, self.y + (self.h - h) / 2.0, w, h)
    }
    /// Retire une bande de largeur `w` à gauche et la renvoie.
    pub fn cut_left(&mut self, w: f32) -> Rect {
        let w = w.min(self.w).max(0.0);
        let r = rect(self.x, self.y, w, self.h);
        self.x += w;
        self.w -= w;
        r
    }
    pub fn cut_right(&mut self, w: f32) -> Rect {
        let w = w.min(self.w).max(0.0);
        self.w -= w;
        rect(self.x + self.w, self.y, w, self.h)
    }
    pub fn cut_top(&mut self, h: f32) -> Rect {
        let h = h.min(self.h).max(0.0);
        let r = rect(self.x, self.y, self.w, h);
        self.y += h;
        self.h -= h;
        r
    }
    pub fn cut_bottom(&mut self, h: f32) -> Rect {
        let h = h.min(self.h).max(0.0);
        self.h -= h;
        rect(self.x, self.y + self.h, self.w, h)
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        rect(x, y, (self.right().min(o.right()) - x).max(0.0), (self.bottom().min(o.bottom()) - y).max(0.0))
    }
    pub fn offset(&self, dx: f32, dy: f32) -> Rect {
        rect(self.x + dx, self.y + dy, self.w, self.h)
    }
}

/// Couleur RVBA, composantes de 0 à 1 (non prémultipliées).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    /// `0xRRGGBB` opaque.
    pub const fn hex(rgb: u32) -> Color {
        Color::hexa(rgb, 1.0)
    }
    /// `0xRRGGBB` avec une opacité.
    pub const fn hexa(rgb: u32, a: f32) -> Color {
        Color { r: ((rgb >> 16) & 0xff) as f32 / 255.0, g: ((rgb >> 8) & 0xff) as f32 / 255.0, b: (rgb & 0xff) as f32 / 255.0, a }
    }
    /// « #0a84ff » (réglage d'accent de Plume).
    pub fn parse(s: &str) -> Option<Color> {
        let h = s.trim().strip_prefix('#')?;
        (h.len() == 6).then(|| u32::from_str_radix(h, 16).ok().map(Color::hex)).flatten()
    }
    /// Même couleur, opacité multipliée par `k`.
    pub fn fade(self, k: f32) -> Color {
        Color { a: self.a * k, ..self }
    }
    /// Mélange vers `o` (`t` de 0 à 1).
    pub fn mix(self, o: Color, t: f32) -> Color {
        let l = |a: f32, b: f32| a + (b - a) * t;
        Color { r: l(self.r, o.r), g: l(self.g, o.g), b: l(self.b, o.b), a: l(self.a, o.a) }
    }
    /// Couleur opaque obtenue en posant celle-ci sur le fond `bg`.
    pub fn over(self, bg: Color) -> Color {
        Color { a: 1.0, ..bg.mix(Color { a: 1.0, ..self }, self.a) }
    }
    /// COLORREF Windows (0x00BBGGRR) pour GDI.
    pub fn colorref(self) -> u32 {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
        c(self.r) | (c(self.g) << 8) | (c(self.b) << 16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_cuts_and_hits() {
        let mut r = rect(0.0, 0.0, 100.0, 50.0);
        let left = r.cut_left(30.0);
        assert_eq!((left, r), (rect(0.0, 0.0, 30.0, 50.0), rect(30.0, 0.0, 70.0, 50.0)));
        let bottom = r.cut_bottom(10.0);
        assert_eq!(bottom, rect(30.0, 40.0, 70.0, 10.0));
        assert!(r.contains(pt(30.0, 0.0)) && !r.contains(pt(100.0, 10.0)) && !r.contains(pt(40.0, 40.0)));
        assert_eq!(rect(0.0, 0.0, 10.0, 10.0).centered(4.0, 2.0), rect(3.0, 4.0, 4.0, 2.0));
        assert!(rect(0.0, 0.0, 5.0, 5.0).intersect(&rect(10.0, 0.0, 5.0, 5.0)).is_empty());
    }

    #[test]
    fn colors() {
        assert_eq!(Color::parse("#0a84ff"), Some(Color::hex(0x0a84ff)));
        assert_eq!(Color::parse("blue"), None);
        assert_eq!(Color::hex(0x0a84ff).colorref(), 0x00ff840a);
        let half = Color::hexa(0xffffff, 0.5).over(Color::hex(0x000000));
        assert!((half.r - 0.5).abs() < 0.01 && half.a == 1.0);
    }
}
