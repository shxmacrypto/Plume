//! Drawing beyond plume-ui's basics: wrapped text in any font (annotation text is shown in Arial, which has
//! Helvetica's widths, so it wraps as it will in the file), soft shadows under the pages, free lines, and
//! Plume PDF's own icons.

use plume_ui::{Align, Canvas, Color, Rect, icons::Icon, icons::Part, rect};
use std::{cell::RefCell, collections::HashMap};
use windows::{
    Win32::Graphics::{
        Direct2D::{Common::D2D1_COLOR_F, D2D1_DRAW_TEXT_OPTIONS},
        DirectWrite::{
            DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
            DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TRIMMING,
            DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP, IDWriteInlineObject, IDWriteTextFormat,
        },
    },
    core::HSTRING,
};
use windows_numerics::Vector2;

fn col(c: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.r, g: c.g, b: c.b, a: c.a }
}

/// (family, size × 4, weight, alignment, wrapping)
type FormatKey = (String, u32, u32, u8, bool);

thread_local! {
    static FORMATS: RefCell<HashMap<FormatKey, IDWriteTextFormat>> = RefCell::new(HashMap::new());
}

fn format(c: &Canvas, family: Option<&str>, size: f32, weight: u32, align: Align, wrap: bool) -> Option<IDWriteTextFormat> {
    let fam = family.unwrap_or(c.gfx.ui_font).to_string();
    let key = (fam.clone(), (size * 4.0) as u32, weight, align as u8, wrap);
    if let Some(f) = FORMATS.with(|m| m.borrow().get(&key).cloned()) {
        return Some(f);
    }
    let f = unsafe {
        let f = c
            .gfx
            .dw
            .CreateTextFormat(
                &HSTRING::from(fam),
                None,
                DWRITE_FONT_WEIGHT(weight as i32),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                &HSTRING::from("en-us"),
            )
            .ok()?;
        let _ = f.SetTextAlignment(match align {
            Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
            Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
            Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
        });
        if wrap {
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
            let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
        } else {
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            let sign: Option<IDWriteInlineObject> = c.gfx.dw.CreateEllipsisTrimmingSign(&f).ok();
            let _ = f.SetTrimming(&DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 }, sign.as_ref());
        }
        f
    };
    FORMATS.with(|m| m.borrow_mut().insert(key, f.clone()));
    Some(f)
}

/// Text cut into lines at the words, from the top of `q` (its width counts), in `family` (None: the interface's).
#[allow(clippy::too_many_arguments)]
pub fn text_wrap(c: &mut Canvas, s: &str, q: Rect, family: Option<&str>, size: f32, weight: u32, color: Color, align: Align) {
    if s.is_empty() || color.a <= 0.0 || size < 1.0 {
        return;
    }
    let Some(f) = format(c, family, size, weight, align, true) else { return };
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let Ok(layout) = c.gfx.dw.CreateTextLayout(&w, &f, q.w.max(1.0), q.h.max(1.0)) else { return };
        let rt = c.target();
        if let Ok(b) = rt.CreateSolidColorBrush(&col(color), None) {
            rt.DrawTextLayout(Vector2 { X: q.x, Y: q.y }, &layout, &b, D2D1_DRAW_TEXT_OPTIONS(4 | 2));
        }
    }
}

/// A soft shadow around `q` (pages and cards).
pub fn shadow(c: &mut Canvas, q: Rect, radius: f32, strength: f32) {
    for (k, a) in [(1.0, 0.10), (3.0, 0.07), (6.0, 0.045), (10.0, 0.025)] {
        let r = rect(q.x - k * 0.6, q.y - k * 0.2, q.w + k * 1.2, q.h + k * 1.4);
        c.fill_round(r, radius + k, Color::hexa(0x000000, a * strength));
    }
}

/// A line through points (ink, signature), round ends.
pub fn polyline(c: &mut Canvas, pts: &[(f32, f32)], width: f32, color: Color) {
    if pts.is_empty() {
        return;
    }
    let rt = c.target();
    unsafe {
        let Ok(b) = rt.CreateSolidColorBrush(&col(color), None) else { return };
        let style = c.gfx.round.clone();
        if pts.len() == 1 {
            let (x, y) = pts[0];
            rt.DrawLine(Vector2 { X: x, Y: y }, Vector2 { X: x + 0.01, Y: y }, &b, width, &style);
            return;
        }
        for w in pts.windows(2) {
            rt.DrawLine(Vector2 { X: w[0].0, Y: w[0].1 }, Vector2 { X: w[1].0, Y: w[1].1 }, &b, width, &style);
        }
    }
}

/// A dashed-looking frame (selection of an annotation): a thin outline and corner dots.
pub fn frame(c: &mut Canvas, q: Rect, color: Color) {
    c.stroke_round(q.inset(-3.0), 4.0, color, 1.5);
}

/// Plume PDF's icons (16 × 16, round strokes, like Plume's).
pub mod ic {
    use super::*;
    use Part::{C, P, R};

    macro_rules! icons {
        ($($id:ident = [$($part:expr),+ $(,)?];)+) => {
            $(pub const $id: Icon = Icon { name: concat!("pdf_", stringify!($id)), vb: 16.0, parts: &[$($part),+] };)+
        };
    }

    icons! {
        SAVE = [R(2.5, 2.5, 11.0, 11.0, 1.8), P("M5 2.5v3h5.5v-3M5 13.5v-4h6v4")];
        PRINT = [P("M4.5 6V2.5h7V6"), R(2.0, 6.0, 12.0, 5.5, 1.5), P("M4.5 9.5h7v4h-7z")];
        POINTER = [P("M4 2.5l8 5.6-3.5.8 2 4-1.7.8-2-4L4 12.4z")];
        MARKER = [P("M9.6 2.6l3.8 3.8-5.6 5.6H4v-3.8z"), P("M2.5 14h11")];
        NOTE = [P("M3 2.5h10v7.5l-3.5 3.5H3z"), P("M9.5 13.5V10H13"), P("M5.5 5.5h5M5.5 8h3")];
        PEN = [P("M11 2.6l2.4 2.4-7.9 7.9H3.1v-2.4z"), P("M9.4 4.2l2.4 2.4")];
        TEXT = [P("M3.5 4.5v-2h9v2M8 2.5v11M6 13.5h4")];
        SIGN = [P("M2.5 11c1.6-3.4 3-6.6 4-6.6.9 0 .4 3.6-.6 6s3.2-5.6 4.4-5.6c1 0 .1 4.2 1.3 4.2s2-1.5 2-1.5"), P("M2.5 13.5h11")];
        MINUS = [P("M3.5 8h9")];
        FIT_W = [P("M2.5 3v10M13.5 3v10M5 8h6M6.6 6.2 5 8l1.6 1.8M9.4 6.2 11 8l-1.6 1.8")];
        FIT_P = [P("M5.5 2.5h-3v3M10.5 2.5h3v3M5.5 13.5h-3v-3M10.5 13.5h3v-3")];
        MOON = [P("M13 9.6A5.5 5.5 0 1 1 6.4 3a4.4 4.4 0 0 0 6.6 6.6z")];
        ROT_L = [P("M3 2.8v3.6h3.6"), P("M3.4 6.2A5 5 0 1 1 3.2 9.6")];
        ROT_R = [P("M13 2.8v3.6H9.4"), P("M12.6 6.2A5 5 0 1 0 12.8 9.6")];
        EXTRACT = [P("M9 2.5H4v11h8V5.5z"), P("M9 2.5v3h3"), P("M8 7.2v4.3M6.2 9.8 8 11.5l1.8-1.7")];
        PAGES = [R(2.5, 2.0, 7.5, 9.5, 1.3), P("M6.5 14h5.8a1.2 1.2 0 0 0 1.2-1.2V5.5")];
        OUTLINE = [P("M6 4h7.5M6 8h7.5M8.5 12h5"), C(3.2, 4.0, 0.6), C(3.2, 8.0, 0.6), C(5.7, 12.0, 0.6)];
        UNDO = [P("M5.5 3 2.5 6l3 3"), P("M2.5 6h7a3.8 3.8 0 0 1 0 7.6H7")];
        LOCK = [R(3.5, 7.0, 9.0, 7.0, 1.6), P("M5.5 7V5.2a2.5 2.5 0 0 1 5 0V7")];
        CHEV_UP = [P("m4 10 4-4 4 4")];
        DOC = [P("M4 1.8h5.2L12.5 5v9.2H4z"), P("M9 1.8V5.2h3.5"), P("M6 8.5h4.5M6 11h3")];
    }
}
