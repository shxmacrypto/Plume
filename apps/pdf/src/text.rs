//! The text of each page with the place of every character, read from the page's drawing instructions: what
//! search finds and highlights, and what the highlighter snaps to. Positions are in PDF coordinates.

use crate::edit::{helv_bold_width, helv_width};
use lopdf::{Dictionary, Document, Encoding, Object, ObjectId, content::Content};
use std::collections::HashMap;

/// A page's characters, with a box (x0, y0, x1, y1) for each. Spaces and line breaks added between pieces of
/// text get the box of the character before them, with no width.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageText {
    pub chars: Vec<char>,
    pub boxes: Vec<[f32; 4]>,
}

impl PageText {
    #[cfg(any(test, feature = "test-hook"))]
    pub fn string(&self) -> String {
        self.chars.iter().collect()
    }
}

/// A match: page, first character, number of characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Found {
    pub page: usize,
    pub start: usize,
    pub len: usize,
}

type M = [f32; 6];
const ID: M = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// a × b (PDF's row-vector convention: a is applied first).
fn mul(a: &M, b: &M) -> M {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn apply(m: &M, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn num(o: &Object) -> f32 {
    match o {
        Object::Integer(i) => *i as f32,
        Object::Real(r) => *r,
        _ => 0.0,
    }
}

struct Font<'a> {
    enc: Option<Encoding<'a>>,
    two_bytes: bool,
    widths: HashMap<u32, f32>,
    default: f32,
    helvetica: bool,
    bold: bool,
}

impl Font<'_> {
    fn width(&self, code: u32, ch: Option<char>) -> f32 {
        if let Some(w) = self.widths.get(&code) {
            return *w;
        }
        match (self.helvetica, ch) {
            (true, Some(c)) if self.bold => helv_bold_width(c),
            (true, Some(c)) => helv_width(c),
            _ => self.default,
        }
    }
}

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    doc.dereference(o).map(|x| x.1).unwrap_or(o)
}

fn font<'a>(doc: &'a Document, d: &'a Dictionary) -> Font<'a> {
    let name = |k: &[u8]| d.get(k).and_then(Object::as_name).map(|n| String::from_utf8_lossy(n).to_lowercase()).unwrap_or_default();
    let base = name(b"BaseFont");
    let two_bytes = name(b"Subtype") == "type0";
    let mut widths = HashMap::new();
    let mut default = if base.contains("courier") || base.contains("mono") { 600.0 } else { 520.0 };
    if two_bytes {
        default = 1000.0;
        if let Some(desc) =
            d.get(b"DescendantFonts").ok().and_then(|f| deref(doc, f).as_array().ok()).and_then(|a| a.first()).and_then(|f| deref(doc, f).as_dict().ok())
        {
            if let Ok(dw) = desc.get(b"DW") {
                default = num(deref(doc, dw));
            }
            // /W: [c [w1 w2 …]] or [c_first c_last w]
            if let Some(w) = desc.get(b"W").ok().and_then(|w| deref(doc, w).as_array().ok()) {
                let mut i = 0;
                while i + 1 < w.len() {
                    let first = num(deref(doc, &w[i])) as u32;
                    match deref(doc, &w[i + 1]) {
                        Object::Array(list) => {
                            for (k, v) in list.iter().enumerate() {
                                widths.insert(first + k as u32, num(deref(doc, v)));
                            }
                            i += 2;
                        }
                        last if i + 2 < w.len() => {
                            let v = num(deref(doc, &w[i + 2]));
                            for c in first..=(num(last) as u32).min(first + 65_535) {
                                widths.insert(c, v);
                            }
                            i += 3;
                        }
                        _ => break,
                    }
                }
            }
        }
    } else if let Some(w) = d.get(b"Widths").ok().and_then(|w| deref(doc, w).as_array().ok()) {
        let first = d.get(b"FirstChar").map(|f| num(deref(doc, f)) as u32).unwrap_or(0);
        for (k, v) in w.iter().enumerate() {
            widths.insert(first + k as u32, num(deref(doc, v)));
        }
    }
    let helvetica = widths.is_empty() && (base.contains("helvetica") || base.contains("arial"));
    // standard fonts drawn without a width table: Helvetica's own metrics (Arial shares them)
    let bold = base.contains("bold") || base.contains("black");
    Font { enc: d.get_font_encoding(doc).ok(), two_bytes, widths, default, helvetica, bold }
}

#[derive(Clone)]
struct Gs {
    ctm: M,
    tc: f32,
    tw: f32,
    th: f32,
    tl: f32,
    rise: f32,
    font: Option<Vec<u8>>,
    size: f32,
}

/// The characters drawn on a page, in drawing order, each with its box.
pub fn page_text(doc: &Document, page: ObjectId) -> PageText {
    let mut glyphs: Vec<(char, [f32; 4], f32)> = Vec::new();
    let fonts: HashMap<Vec<u8>, Font> = doc.get_page_fonts(page).unwrap_or_default().into_iter().map(|(n, d)| (n, font(doc, d))).collect();
    let Ok(data) = doc.get_page_content_with_limit(page, 64 << 20) else { return PageText::default() };
    let Ok(content) = Content::decode(&data) else { return PageText::default() };
    let mut gs = Gs { ctm: ID, tc: 0.0, tw: 0.0, th: 1.0, tl: 0.0, rise: 0.0, font: None, size: 12.0 };
    let mut stack: Vec<Gs> = Vec::new();
    let (mut tm, mut tlm) = (ID, ID);
    for op in &content.operations {
        let a = &op.operands;
        let f = |i: usize| a.get(i).map(num).unwrap_or(0.0);
        match op.operator.as_str() {
            "q" => stack.push(gs.clone()),
            "Q" => {
                if let Some(g) = stack.pop() {
                    gs = g;
                }
            }
            "cm" if a.len() == 6 => gs.ctm = mul(&[f(0), f(1), f(2), f(3), f(4), f(5)], &gs.ctm),
            "BT" => {
                tm = ID;
                tlm = ID;
            }
            "Tf" => {
                gs.font = a.first().and_then(|n| n.as_name().ok()).map(<[u8]>::to_vec);
                gs.size = f(1);
            }
            "Tc" => gs.tc = f(0),
            "Tw" => gs.tw = f(0),
            "Tz" => gs.th = f(0) / 100.0,
            "TL" => gs.tl = f(0),
            "Ts" => gs.rise = f(0),
            "Td" | "TD" => {
                if op.operator == "TD" {
                    gs.tl = -f(1);
                }
                tlm = mul(&[1.0, 0.0, 0.0, 1.0, f(0), f(1)], &tlm);
                tm = tlm;
            }
            "Tm" if a.len() == 6 => {
                tlm = [f(0), f(1), f(2), f(3), f(4), f(5)];
                tm = tlm;
            }
            "T*" => {
                tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -gs.tl], &tlm);
                tm = tlm;
            }
            "Tj" | "'" | "\"" | "TJ" => {
                if op.operator != "Tj" && op.operator != "TJ" {
                    if op.operator == "\"" {
                        gs.tw = f(0);
                        gs.tc = f(1);
                    }
                    tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -gs.tl], &tlm);
                    tm = tlm;
                }
                let Some(fo) = gs.font.as_ref().and_then(|n| fonts.get(n)) else { continue };
                let items: Vec<&Object> = match op.operator.as_str() {
                    "TJ" => a.first().and_then(|x| x.as_array().ok()).map(|v| v.iter().collect()).unwrap_or_default(),
                    _ => a.last().into_iter().collect(),
                };
                for it in items {
                    match it {
                        Object::String(bytes, _) => show(bytes, fo, &gs, &mut tm, &mut glyphs),
                        o => {
                            let tx = -num(o) / 1000.0 * gs.size * gs.th;
                            tm = mul(&[1.0, 0.0, 0.0, 1.0, tx, 0.0], &tm);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    arrange(glyphs)
}

fn show(bytes: &[u8], fo: &Font, gs: &Gs, tm: &mut M, out: &mut Vec<(char, [f32; 4], f32)>) {
    let step = if fo.two_bytes { 2 } else { 1 };
    for code in bytes.chunks(step) {
        let c = code.iter().fold(0u32, |v, b| v << 8 | *b as u32);
        let text: Vec<char> = match &fo.enc {
            Some(enc) => enc.bytes_to_string(code).map(|s| s.chars().collect()).unwrap_or_default(),
            None => vec![char::from_u32(c).unwrap_or('?')],
        };
        let w0 = fo.width(c, text.first().copied()) / 1000.0;
        let adv = (w0 * gs.size + gs.tc + if step == 1 && c == 32 { gs.tw } else { 0.0 }) * gs.th;
        let trm = mul(tm, &gs.ctm);
        let (lo, hi) = (gs.rise - 0.22 * gs.size, gs.rise + 0.78 * gs.size);
        let gw = w0 * gs.size * gs.th;
        let n = text.len().max(1) as f32;
        for (k, ch) in text.iter().enumerate() {
            if ch.is_control() {
                continue;
            }
            let (xa, xb) = (gw * k as f32 / n, gw * (k + 1) as f32 / n);
            let pts = [apply(&trm, xa, lo), apply(&trm, xb, lo), apply(&trm, xa, hi), apply(&trm, xb, hi)];
            let x0 = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
            let x1 = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
            let y0 = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
            let y1 = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
            // the font size as drawn (for spacing)
            let em = (trm[2] * trm[2] + trm[3] * trm[3]).sqrt() * gs.size;
            out.push((*ch, [x0, y0, x1, y1], em.abs().max(1.0)));
        }
        *tm = mul(&[1.0, 0.0, 0.0, 1.0, adv, 0.0], tm);
    }
}

/// Spaces between words drawn apart and line breaks between lines.
fn arrange(glyphs: Vec<(char, [f32; 4], f32)>) -> PageText {
    let mut t = PageText::default();
    let mut prev: Option<([f32; 4], f32)> = None;
    for (ch, b, em) in glyphs {
        if let Some((p, pem)) = prev {
            let mid = (b[1] + b[3]) / 2.0;
            let pmid = (p[1] + p[3]) / 2.0;
            let last = t.chars.last().copied().unwrap_or(' ');
            if (mid - pmid).abs() > 0.5 * em.max(pem) {
                if last != '\n' {
                    t.chars.push('\n');
                    t.boxes.push([p[2], p[1], p[2], p[3]]);
                }
            } else if b[0] - p[2] > 0.2 * em && !last.is_whitespace() && ch != ' ' {
                t.chars.push(' ');
                t.boxes.push([p[2], p[1], p[2], p[3]]);
            }
        }
        t.chars.push(ch);
        t.boxes.push(b);
        prev = Some((b, em));
    }
    t
}

fn fold(c: char) -> char {
    match c {
        '\n' | '\t' | '\u{a0}' => ' ',
        '’' | '‘' => '\'',
        c => c.to_lowercase().next().unwrap_or(c),
    }
}

/// Every place `query` appears (ignoring case), page after page.
pub fn find(pages: &[PageText], query: &str) -> Vec<Found> {
    let q: Vec<char> = query.trim().chars().map(fold).collect();
    let mut out = Vec::new();
    if q.is_empty() {
        return out;
    }
    for (page, t) in pages.iter().enumerate() {
        let s: Vec<char> = t.chars.iter().map(|&c| fold(c)).collect();
        let mut i = 0;
        while i + q.len() <= s.len() {
            if s[i..i + q.len()] == q[..] {
                out.push(Found { page, start: i, len: q.len() });
                i += q.len();
            } else {
                i += 1;
            }
        }
    }
    out
}

/// The boxes covering characters `start..start+len`, one per line.
pub fn line_boxes(t: &PageText, start: usize, len: usize) -> Vec<[f32; 4]> {
    let mut out: Vec<[f32; 4]> = Vec::new();
    for i in start..(start + len).min(t.chars.len()) {
        let b = t.boxes[i];
        if b[2] - b[0] <= 0.0 {
            continue;
        }
        match out.last_mut() {
            Some(l) if ((l[1] + l[3]) / 2.0 - (b[1] + b[3]) / 2.0).abs() < 0.5 * (l[3] - l[1]).max(b[3] - b[1]) => {
                *l = [l[0].min(b[0]), l[1].min(b[1]), l[2].max(b[2]), l[3].max(b[3])];
            }
            _ => out.push(b),
        }
    }
    out
}

/// The characters whose middle is inside `area` (x0, y0, x1, y1), as (first, count) — what a highlight drag covers.
pub fn chars_in(t: &PageText, area: [f32; 4]) -> Option<(usize, usize)> {
    let inside: Vec<usize> = (0..t.chars.len())
        .filter(|&i| {
            let b = t.boxes[i];
            let (cx, cy) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
            b[2] > b[0] && cx >= area[0] && cx <= area[2] && cy >= area[1] && cy <= area[3]
        })
        .collect();
    Some((*inside.first()?, inside.last()? - inside.first()? + 1))
}

/// All pages' text.
pub fn all(doc: &Document) -> Vec<PageText> {
    crate::edit::page_ids(doc).into_iter().map(|p| page_text(doc, p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{sample, to_bytes};

    fn texts() -> Vec<PageText> {
        let doc = lopdf::Document::load_mem(&to_bytes(&sample(&["Hello World\nsecond line here", "nothing", "WORLD again, world"]), None).unwrap()).unwrap();
        all(&doc)
    }

    #[test]
    fn text_comes_out_with_its_lines() {
        let t = texts();
        assert_eq!(t[0].string(), "Hello World\nsecond line here");
        assert_eq!(t[0].chars.len(), t[0].boxes.len());
        // "H" sits at x 60 (Td), baseline 760, size 14: its box starts there
        let h = t[0].boxes[0];
        assert!((h[0] - 60.0).abs() < 0.01 && h[1] < 760.0 && h[3] > 760.0 + 9.0, "{h:?}");
        // and is as wide as Helvetica's H (722/1000 × 14)
        assert!((h[2] - h[0] - 10.108).abs() < 0.01);
    }

    #[test]
    fn search_ignores_case_and_finds_every_match() {
        let t = texts();
        let hits = find(&t, "world");
        assert_eq!(hits.iter().map(|h| (h.page, h.start)).collect::<Vec<_>>(), vec![(0, 6), (2, 0), (2, 13)]);
        assert!(find(&t, "  ").is_empty());
        // across the line break, a space in the query matches
        assert_eq!(find(&t, "world second").len(), 1);
        let boxes = line_boxes(&t[0], 6, 12);
        assert_eq!(boxes.len(), 2, "{boxes:?}");
        assert!(boxes[0][1] > boxes[1][1]);
    }

    #[test]
    fn a_drag_selects_the_characters_under_it() {
        let t = texts();
        // the first line's band, from x 55 to 92: "Hello" ends at 91.9, the space after it is centered at 93.8
        let (s, n) = chars_in(&t[0], [55.0, 755.0, 92.0, 775.0]).unwrap();
        assert_eq!(s, 0);
        assert_eq!(t[0].chars[s..s + n].iter().collect::<String>(), "Hello");
        assert!(chars_in(&t[0], [0.0, 0.0, 10.0, 10.0]).is_none());
    }

    #[test]
    fn matrices() {
        let m = mul(&[1.0, 0.0, 0.0, 1.0, 10.0, 0.0], &[2.0, 0.0, 0.0, 2.0, 0.0, 5.0]);
        assert_eq!(apply(&m, 1.0, 1.0), (22.0, 7.0));
    }
}
