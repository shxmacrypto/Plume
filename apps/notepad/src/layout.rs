//! Where characters go on screen: their widths, tab stops, and how a line is cut into rows when the text wraps.
//! Widths come from DirectWrite (each character measured once per font and size); tests use fixed widths.

use std::{cell::RefCell, collections::HashMap};
use windows::{
    Win32::Graphics::DirectWrite::{
        DWRITE_FONT_FEATURE, DWRITE_FONT_FEATURE_TAG_CONTEXTUAL_ALTERNATES, DWRITE_FONT_FEATURE_TAG_KERNING, DWRITE_FONT_FEATURE_TAG_REQUIRED_LIGATURES,
        DWRITE_FONT_FEATURE_TAG_STANDARD_LIGATURES, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_METRICS,
        DWRITE_WORD_WRAPPING_NO_WRAP, IDWriteFactory, IDWriteTextFormat, IDWriteTypography,
    },
    core::HSTRING,
};

/// Spaces per tab stop.
pub const TAB: f32 = 4.0;

/// What is drawn for a character: control characters get their visible picture (␀, ␛…).
pub fn shown(c: char) -> char {
    match c {
        '\u{0}'..='\u{8}' | '\u{b}'..='\u{1f}' | '\n' => char::from_u32(0x2400 + c as u32).unwrap_or('?'),
        '\u{7f}' => '\u{2421}',
        _ => c,
    }
}

/// Characters drawn two cells wide in a fixed-width font (CJK, emoji, full-width forms).
pub fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF | 0x20000..=0x3FFFD)
}

/// The font and size of a text area, and the width of every character met so far.
pub struct Measure {
    pub family: String,
    pub size: f32,
    /// Height of a row.
    pub line_h: f32,
    /// Where the text of a row starts below the top of the row.
    pub text_y: f32,
    pub tab_w: f32,
    ascii: [f32; 128],
    others: RefCell<HashMap<char, f32>>,
    pub dw: Option<IDWriteFactory>,
    pub format: Option<IDWriteTextFormat>,
    pub typography: Option<IDWriteTypography>,
}

impl Measure {
    /// Fixed widths, for tests: every character `w` wide (wide ones twice).
    pub fn fixed(w: f32) -> Measure {
        Measure {
            family: String::new(),
            size: w / 0.6,
            line_h: (w / 0.6 * 1.45).ceil(),
            text_y: 0.0,
            tab_w: w * TAB,
            ascii: [w; 128],
            others: RefCell::new(HashMap::new()),
            dw: None,
            format: None,
            typography: None,
        }
    }

    pub fn new(dw: &IDWriteFactory, family: &str, size: f32) -> Measure {
        let mut m = Measure::fixed(size * 0.6);
        m.family = family.to_string();
        m.size = size;
        unsafe {
            let Ok(f) = dw.CreateTextFormat(
                &HSTRING::from(family),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                &HSTRING::from("en-us"),
            ) else {
                return m;
            };
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            m.dw = Some(dw.clone());
            m.format = Some(f);
            // no kerning or ligatures: each character keeps its own width, so the caret falls where it should
            if let Ok(t) = dw.CreateTypography() {
                for tag in [
                    DWRITE_FONT_FEATURE_TAG_KERNING,
                    DWRITE_FONT_FEATURE_TAG_STANDARD_LIGATURES,
                    DWRITE_FONT_FEATURE_TAG_REQUIRED_LIGATURES,
                    DWRITE_FONT_FEATURE_TAG_CONTEXTUAL_ALTERNATES,
                ] {
                    let _ = t.AddFontFeature(DWRITE_FONT_FEATURE { nameTag: tag, parameter: 0 });
                }
                m.typography = Some(t);
            }
        }
        // the row: a little air above and below the font's own height
        let natural = m.measure_str("Ag").1;
        m.line_h = (natural * 1.18).max(size * 1.3).ceil();
        m.text_y = ((m.line_h - natural) / 2.0).floor();
        for i in 32..127u8 {
            m.ascii[i as usize] = m.measure_str(&(i as char).to_string()).0;
        }
        let space = m.ascii[32];
        for i in (0..32).chain(127..128) {
            m.ascii[i] = m.measure_str(&shown(char::from(i as u8)).to_string()).0.max(space);
        }
        m.tab_w = space * TAB;
        m
    }

    /// (width, height) of a short text in this font.
    fn measure_str(&self, s: &str) -> (f32, f32) {
        let (Some(dw), Some(f)) = (&self.dw, &self.format) else { return (self.size * 0.6 * s.chars().count() as f32, self.line_h) };
        let w: Vec<u16> = s.encode_utf16().collect();
        unsafe {
            let mut m = DWRITE_TEXT_METRICS::default();
            match dw.CreateTextLayout(&w, f, 10_000.0, 1_000.0) {
                Ok(l) => {
                    if let Some(t) = &self.typography {
                        let _ = l.SetTypography(t, windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_RANGE { startPosition: 0, length: w.len() as u32 });
                    }
                    if l.GetMetrics(&mut m).is_ok() {
                        return (m.widthIncludingTrailingWhitespace, m.height);
                    }
                    (self.size * 0.6, self.line_h)
                }
                Err(_) => (self.size * 0.6, self.line_h),
            }
        }
    }

    /// The width of a character (not a tab).
    pub fn adv(&self, c: char) -> f32 {
        if (c as u32) < 128 {
            return self.ascii[c as usize];
        }
        if let Some(w) = self.others.borrow().get(&c) {
            return *w;
        }
        let w = if self.dw.is_some() {
            self.measure_str(&shown(c).to_string()).0
        } else if is_wide(c) {
            self.ascii[b'M' as usize] * 2.0
        } else if matches!(c as u32, 0x300..=0x36F) {
            0.0
        } else {
            self.ascii[b'M' as usize]
        };
        self.others.borrow_mut().insert(c, w);
        w
    }

    /// The width of a character at `x` from the start of its row (tabs go to the next stop).
    pub fn w_at(&self, c: char, x: f32) -> f32 {
        if c == '\t' { ((x / self.tab_w).floor() + 1.0) * self.tab_w - x } else { self.adv(c) }
    }

    /// The width of a piece of a row (starting at the row's start).
    pub fn width(&self, s: &str) -> f32 {
        let mut x = 0.0;
        for c in s.chars() {
            x += self.w_at(c, x);
        }
        x
    }

    /// The byte in `s` nearest to `x` (from the start of the row).
    pub fn col_at(&self, s: &str, x: f32) -> usize {
        let mut cur = 0.0;
        for (i, c) in s.char_indices() {
            let w = self.w_at(c, cur);
            if x < cur + w / 2.0 {
                return i;
            }
            cur += w;
        }
        s.len()
    }

    /// Where the rows of a line start (byte offsets, the first is 0), cut at the last space that fits in `width`.
    pub fn wrap(&self, line: &str, width: f32) -> Vec<usize> {
        let mut rows = vec![0];
        if width <= 0.0 || line.len() < 2 {
            return rows;
        }
        // short ASCII lines that surely fit: no work
        if line.is_ascii() && !line.contains('\t') && line.len() as f32 * self.ascii[b'W' as usize].max(self.ascii[b'm' as usize]) <= width {
            return rows;
        }
        let mut start = 0;
        let mut x = 0.0;
        let mut brk: Option<usize> = None;
        for (i, c) in line.char_indices() {
            let w = self.w_at(c, x);
            // spaces may hang past the edge; anything else goes to the next row
            if x + w > width && i > start && c != ' ' && c != '\t' {
                let at = match brk {
                    Some(b) if b > start && b <= i => b,
                    _ => i,
                };
                rows.push(at);
                start = at;
                brk = None;
                x = self.width(&line[at..i]);
            }
            x += self.w_at(c, x);
            if c == ' ' || c == '\t' || c == '-' || is_wide(c) {
                brk = Some(i + c.len_utf8());
            }
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_and_tabs() {
        let m = Measure::fixed(10.0);
        assert_eq!(m.width("abc"), 30.0);
        assert_eq!(m.width("\tx"), 50.0);
        assert_eq!(m.width("ab\tx"), 50.0);
        assert_eq!(m.width("abcd\tx"), 90.0);
        assert_eq!(m.width("中a"), 30.0);
        assert_eq!(m.col_at("abcd", 14.0), 1);
        assert_eq!(m.col_at("abcd", 16.0), 2);
        assert_eq!(m.col_at("abcd", 999.0), 4);
        assert_eq!(m.col_at("é", 6.0), 2);
    }

    #[test]
    fn wrap_at_spaces() {
        let m = Measure::fixed(10.0);
        // 10 characters per row
        assert_eq!(m.wrap("short", 100.0), vec![0]);
        assert_eq!(m.wrap("hello world again", 100.0), vec![0, 6, 12]);
        // no space: cut anywhere
        assert_eq!(m.wrap("abcdefghijklmnopqrstuvwxyz", 100.0), vec![0, 10, 20]);
        // spaces hang at the end of a row
        assert_eq!(m.wrap("abcdefghij     klm", 100.0), vec![0, 15]);
        assert_eq!(m.wrap("x", 0.0), vec![0]);
        // every row fits
        let line = "the quick brown fox jumps over the lazy dog ".repeat(5);
        let rows = m.wrap(&line, 120.0);
        for w in rows.windows(2) {
            assert!(m.width(line[w[0]..w[1]].trim_end()) <= 120.0);
        }
    }
}
