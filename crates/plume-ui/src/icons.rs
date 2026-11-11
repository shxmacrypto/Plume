//! Icônes vectorielles de l'interface : les mêmes tracés que Plume (SVG 16 × 16, trait arrondi de 1,9),
//! convertis une fois en géométries Direct2D.

use super::gfx::Gfx;
use windows::Win32::Graphics::Direct2D::{
    Common::{D2D_SIZE_F, D2D1_BEZIER_SEGMENT, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_FILL_MODE_WINDING},
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL, D2D1_QUADRATIC_BEZIER_SEGMENT, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE, ID2D1GeometrySink, ID2D1PathGeometry,
};
use windows_numerics::Vector2;

pub enum Part {
    /// Tracé SVG (`d="…"`).
    P(&'static str),
    /// Cercle : centre x, centre y, rayon.
    C(f32, f32, f32),
    /// Rectangle arrondi : x, y, largeur, hauteur, rayon.
    R(f32, f32, f32, f32, f32),
}
use Part::{C, P, R};

pub struct Icon {
    pub name: &'static str,
    /// Côté de la zone de dessin (viewBox).
    pub vb: f32,
    pub parts: &'static [Part],
}

macro_rules! icons {
    ($($id:ident = $vb:expr, [$($part:expr),+ $(,)?];)+) => {
        $(pub const $id: Icon = Icon { name: stringify!($id), vb: $vb, parts: &[$($part),+] };)+
    };
}

icons! {
    BACK = 16.0, [P("M10 3 5 8l5 5")];
    FORWARD = 16.0, [P("m6 3 5 5-5 5")];
    UP = 16.0, [P("M8 13V3.5M3.5 7.5 8 3l4.5 4.5")];
    RELOAD = 16.0, [P("M13 8a5 5 0 1 1-1.5-3.6M13 2.5v3h-3")];
    SEARCH = 16.0, [C(7.0, 7.0, 4.5), P("m10.5 10.5 3 3")];
    CLOSE = 10.0, [P("M2 2l6 6M8 2 2 8")];
    PLUS = 16.0, [P("M8 3v10M3 8h10")];
    CHEVRON_DOWN = 16.0, [P("m4 6 4 4 4-4")];
    CHEVRON_RIGHT = 16.0, [P("m6 4 4 4-4 4")];
    FOLDER = 16.0, [P("M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z")];
    FILE = 16.0, [P("M4 2h5l3 3v9H4z"), P("M9 2v3h3")];
    STAR = 16.0, [P("m8 2 1.8 3.8 4.2.5-3.1 2.9.8 4.1L8 11.3l-3.7 2 .8-4.1L2 6.3l4.2-.5z")];
    PIN = 16.0, [P("M9.5 2.5 13.5 6.5 11 8l-1 3.5-5.5-5.5L8 5z"), P("M6.8 9.2 3 13")];
    GEAR = 16.0, [C(8.0, 8.0, 2.2), P("M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4")];
    CHECK = 16.0, [P("m3 8.5 3 3 7-7")];
    COPY = 16.0, [R(5.0, 5.0, 8.5, 8.5, 2.0), P("M10.5 5V3.5a1.5 1.5 0 0 0-1.5-1.5H3.5A1.5 1.5 0 0 0 2 3.5V9a1.5 1.5 0 0 0 1.5 1.5H5")];
    TRASH = 16.0, [P("M2.5 4h11M6 4V2.5h4V4M4 4l.7 9.5h6.6L12 4")];
    HOME = 16.0, [P("M2.5 7.5 8 2.5l5.5 5v6H10V10H6v3.5H2.5z")];
    PC = 16.0, [R(2.0, 3.0, 12.0, 8.5, 1.5), P("M6 14h4M8 11.5V14")];
    DRIVE = 16.0, [R(2.0, 9.0, 12.0, 4.5, 1.5), P("M3 9.2 4.6 3.5h6.8L13 9.2"), P("M11.3 11.25h.01")];
    MORE = 16.0, [P("M3.5 8h.01M8 8h.01M12.5 8h.01")];
    SIDEBAR = 16.0, [R(2.0, 2.5, 12.0, 11.0, 2.0), P("M6 2.5v11")];
    VIEW_LIST = 16.0, [P("M2.5 4h11M2.5 8h11M2.5 12h11")];
    SERVER = 16.0, [R(2.5, 2.5, 11.0, 4.6, 1.3), R(2.5, 8.9, 11.0, 4.6, 1.3), P("M5 4.8h.01M5 11.2h.01")];
    MAIL = 16.0, [R(2.0, 3.5, 12.0, 9.0, 1.8), P("M2.6 5.2 8 9.2l5.4-4")];
    INBOX = 16.0, [P("M2.5 9.5 4.3 3.5h7.4l1.8 6v3h-11z"), P("M2.5 9.5h3.2a2.3 2.3 0 0 0 4.6 0h3.2")];
    SEND = 16.0, [P("M13.5 2.5 2.5 7l4.2 1.8L8.5 13z"), P("M13.5 2.5 6.7 8.8")];
    REPLY = 16.0, [P("M6.5 4 2.5 7.8l4 3.8"), P("M2.8 7.8h6.2a4.5 4.5 0 0 1 4.5 4.5")];
    REPLY_ALL = 16.0, [P("M5.5 4 2 7.8l3.5 3.8"), P("M9 4 5.5 7.8 9 11.6"), P("M5.8 7.8h4.2a3.5 3.5 0 0 1 3.5 3.5")];
    MAIL_FORWARD = 16.0, [P("M9.5 4l4 3.8-4 3.8"), P("M13.2 7.8H7a4.5 4.5 0 0 0-4.5 4.5")];
    PENCIL = 16.0, [P("M3 13l.6-3L10.8 2.8a1.4 1.4 0 0 1 2 0l.4.4a1.4 1.4 0 0 1 0 2L6 12.4z"), P("M9.6 4l2.4 2.4")];
    CLIP = 16.0, [P("M12.5 7.5 7.4 12.6a3 3 0 0 1-4.2-4.2l5.6-5.6a2 2 0 0 1 2.8 2.8L6.2 11a1 1 0 0 1-1.4-1.4l4.6-4.6")];
    ARCHIVE = 16.0, [R(2.0, 3.0, 12.0, 3.2, 1.0), P("M3 6.2v6.3h10V6.2M6.5 9h3")];
    JUNK = 16.0, [C(8.0, 8.0, 5.5), P("M4.2 4.2l7.6 7.6")];
    USER = 16.0, [C(8.0, 5.5, 2.6), P("M3 13.5a5 5 0 0 1 10 0")];
    VIEW_GRID = 16.0, [R(2.5, 2.5, 4.5, 4.5, 1.0), R(9.0, 2.5, 4.5, 4.5, 1.0), R(2.5, 9.0, 4.5, 4.5, 1.0), R(9.0, 9.0, 4.5, 4.5, 1.0)];
    SORT_UP = 10.0, [P("m2.5 6.2 2.5-2.6 2.5 2.6")];
    SORT_DOWN = 10.0, [P("m2.5 3.8 2.5 2.6 2.5-2.6")];
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn skip(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }
    /// Nombre SVG : « 1.5 », « -2 », « .5 », collés ou non (« 1.5.5 », « 10-5 »).
    fn num(&mut self) -> Option<f32> {
        self.skip();
        let start = self.i;
        if self.i < self.s.len() && matches!(self.s[self.i], b'-' | b'+') {
            self.i += 1;
        }
        let mut dot = false;
        while self.i < self.s.len() {
            match self.s[self.i] {
                b'0'..=b'9' => self.i += 1,
                b'.' if !dot => {
                    dot = true;
                    self.i += 1;
                }
                _ => break,
            }
        }
        std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok()
    }
    /// Drapeau d'arc : un seul chiffre, parfois collé au suivant.
    fn flag(&mut self) -> Option<bool> {
        self.skip();
        let c = *self.s.get(self.i)?;
        self.i += 1;
        match c {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }
    fn pair(&mut self) -> Option<(f32, f32)> {
        Some((self.num()?, self.num()?))
    }
}

fn v(p: (f32, f32)) -> Vector2 {
    Vector2 { X: p.0, Y: p.1 }
}

/// Dessine un tracé SVG dans `sink`. Renvoie faux si le tracé est mal formé (le reste est ignoré).
fn path(sink: &ID2D1GeometrySink, d: &str) -> bool {
    let mut p = Parser { s: d.as_bytes(), i: 0 };
    let (mut cur, mut start) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
    let mut open = false;
    let mut cmd = b'M';
    // dernier point de contrôle (pour S / s)
    let mut ctrl: Option<(f32, f32)> = None;
    macro_rules! get {
        ($e:expr) => {
            match $e {
                Some(x) => x,
                None => {
                    if open {
                        unsafe { sink.EndFigure(D2D1_FIGURE_END_OPEN) };
                    }
                    return false;
                }
            }
        };
    }
    loop {
        p.skip();
        let Some(&c) = p.s.get(p.i) else { break };
        if c.is_ascii_alphabetic() {
            cmd = c;
            p.i += 1;
        } else if cmd == b'M' {
            cmd = b'L'; // coordonnées qui suivent un M : des lignes
        } else if cmd == b'm' {
            cmd = b'l';
        }
        let rel = cmd.is_ascii_lowercase();
        let abs = |q: (f32, f32), cur: (f32, f32)| if rel { (cur.0 + q.0, cur.1 + q.1) } else { q };
        let up = cmd.to_ascii_uppercase();
        if up != b'M' && up != b'Z' && !open {
            unsafe { sink.BeginFigure(v(cur), D2D1_FIGURE_BEGIN_FILLED) };
            start = cur;
            open = true;
        }
        let mut next_ctrl = None;
        match up {
            b'M' => {
                let q = abs(get!(p.pair()), cur);
                if open {
                    unsafe { sink.EndFigure(D2D1_FIGURE_END_OPEN) };
                }
                unsafe { sink.BeginFigure(v(q), D2D1_FIGURE_BEGIN_FILLED) };
                open = true;
                cur = q;
                start = q;
            }
            b'L' => {
                cur = abs(get!(p.pair()), cur);
                unsafe { sink.AddLine(v(cur)) };
            }
            b'H' => {
                let x = get!(p.num());
                cur.0 = if rel { cur.0 + x } else { x };
                unsafe { sink.AddLine(v(cur)) };
            }
            b'V' => {
                let y = get!(p.num());
                cur.1 = if rel { cur.1 + y } else { y };
                unsafe { sink.AddLine(v(cur)) };
            }
            b'C' | b'S' => {
                let c1 = if up == b'C' { abs(get!(p.pair()), cur) } else { ctrl.map_or(cur, |k| (2.0 * cur.0 - k.0, 2.0 * cur.1 - k.1)) };
                let c2 = abs(get!(p.pair()), cur);
                let end = abs(get!(p.pair()), cur);
                unsafe { sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: v(c1), point2: v(c2), point3: v(end) }) };
                next_ctrl = Some(c2);
                cur = end;
            }
            b'Q' => {
                let c1 = abs(get!(p.pair()), cur);
                let end = abs(get!(p.pair()), cur);
                unsafe { sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT { point1: v(c1), point2: v(end) }) };
                cur = end;
            }
            b'A' => {
                let (rx, ry) = get!(p.pair());
                let rot = get!(p.num());
                let (large, sweep) = (get!(p.flag()), get!(p.flag()));
                let end = abs(get!(p.pair()), cur);
                unsafe {
                    sink.AddArc(&D2D1_ARC_SEGMENT {
                        point: v(end),
                        size: D2D_SIZE_F { width: rx, height: ry },
                        rotationAngle: rot,
                        sweepDirection: if sweep { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE },
                        arcSize: if large { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
                    })
                };
                cur = end;
            }
            b'Z' => {
                if open {
                    unsafe { sink.EndFigure(D2D1_FIGURE_END_CLOSED) };
                    open = false;
                }
                cur = start;
            }
            _ => get!(None::<()>),
        }
        ctrl = next_ctrl;
    }
    if open {
        unsafe { sink.EndFigure(D2D1_FIGURE_END_OPEN) };
    }
    true
}

fn arc(sink: &ID2D1GeometrySink, to: (f32, f32), r: f32) {
    unsafe {
        sink.AddArc(&D2D1_ARC_SEGMENT {
            point: v(to),
            size: D2D_SIZE_F { width: r, height: r },
            rotationAngle: 0.0,
            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
            arcSize: D2D1_ARC_SIZE_SMALL,
        })
    };
}

impl Gfx {
    /// Géométrie d'une icône, construite à la première demande.
    pub fn icon(&self, ic: &Icon) -> ID2D1PathGeometry {
        if let Some(g) = self.icons.borrow().get(ic.name) {
            return g.clone();
        }
        let g = unsafe {
            let g = self.d2d.CreatePathGeometry().expect("géométrie");
            let sink = g.Open().expect("tracé");
            sink.SetFillMode(D2D1_FILL_MODE_WINDING);
            for part in ic.parts {
                match *part {
                    P(d) => {
                        let ok = path(&sink, d);
                        debug_assert!(ok, "tracé SVG invalide : {}", ic.name);
                    }
                    C(cx, cy, r) => {
                        sink.BeginFigure(v((cx - r, cy)), D2D1_FIGURE_BEGIN_FILLED);
                        arc(&sink, (cx + r, cy), r);
                        arc(&sink, (cx - r, cy), r);
                        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    }
                    R(x, y, w, h, r) => {
                        sink.BeginFigure(v((x + r, y)), D2D1_FIGURE_BEGIN_FILLED);
                        sink.AddLine(v((x + w - r, y)));
                        arc(&sink, (x + w, y + r), r);
                        sink.AddLine(v((x + w, y + h - r)));
                        arc(&sink, (x + w - r, y + h), r);
                        sink.AddLine(v((x + r, y + h)));
                        arc(&sink, (x, y + h - r), r);
                        sink.AddLine(v((x, y + r)));
                        arc(&sink, (x + r, y), r);
                        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    }
                }
            }
            let _ = sink.Close();
            g
        };
        self.icons.borrow_mut().insert(ic.name, g.clone());
        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_stuck_together() {
        let mut p = Parser { s: b"1.5.5-2,3 0 11.5", i: 0 };
        assert_eq!(p.num(), Some(1.5));
        assert_eq!(p.num(), Some(0.5));
        assert_eq!(p.num(), Some(-2.0));
        assert_eq!(p.num(), Some(3.0));
        assert_eq!((p.flag(), p.flag(), p.flag()), (Some(false), Some(true), Some(true)));
        assert_eq!(p.num(), Some(0.5));
    }
}
