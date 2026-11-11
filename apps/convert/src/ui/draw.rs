//! Text in any size and weight (titles), and Plume Convert's own icons. The text helpers are the same as Plume
//! Music's.

use plume_ui::{Align, Canvas, Color, Rect, icons::Icon, icons::Part};
use std::{cell::RefCell, collections::HashMap};
use windows::{
    Win32::Graphics::{
        Direct2D::{Common::D2D_RECT_F, Common::D2D1_COLOR_F, D2D1_DRAW_TEXT_OPTIONS},
        DirectWrite::{
            DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS,
            DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP, IDWriteInlineObject,
            IDWriteTextFormat,
        },
    },
    core::HSTRING,
};
use windows_numerics::Vector2;

fn col(c: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.r, g: c.g, b: c.b, a: c.a }
}
fn rf(q: Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: q.x, top: q.y, right: q.x + q.w, bottom: q.y + q.h }
}

thread_local! {
    static FORMATS: RefCell<HashMap<(u32, u32, u8, bool), IDWriteTextFormat>> = RefCell::new(HashMap::new());
}

fn format(c: &Canvas, size: f32, weight: u32, align: Align, wrap: bool) -> Option<IDWriteTextFormat> {
    let key = ((size * 4.0) as u32, weight, align as u8, wrap);
    if let Some(f) = FORMATS.with(|m| m.borrow().get(&key).cloned()) {
        return Some(f);
    }
    let f = unsafe {
        let f = c
            .gfx
            .dw
            .CreateTextFormat(
                &HSTRING::from(c.gfx.ui_font),
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

/// One line of text in any size and weight (400 regular, 600 semibold, 700 bold), ending in "…".
pub fn text(c: &mut Canvas, s: &str, q: Rect, size: f32, weight: u32, color: Color, align: Align) {
    if s.is_empty() || q.w <= 0.0 || color.a <= 0.0 {
        return;
    }
    let Some(f) = format(c, size, weight, align, false) else { return };
    let rt = c.target();
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let Ok(b) = rt.CreateSolidColorBrush(&col(color), None) else { return };
        rt.DrawText(&w, &f, &rf(q), &b, D2D1_DRAW_TEXT_OPTIONS(2 | 4), DWRITE_MEASURING_MODE_NATURAL);
    }
}

/// Text cut into lines at the words, from the top of `q`. Returns its height.
pub fn text_wrap(c: &mut Canvas, s: &str, q: Rect, size: f32, weight: u32, color: Color, align: Align) -> f32 {
    let Some(f) = format(c, size, weight, align, true) else { return size * 1.3 };
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let Ok(layout) = c.gfx.dw.CreateTextLayout(&w, &f, q.w.max(1.0), 10_000.0) else { return size * 1.3 };
        let mut m = DWRITE_TEXT_METRICS::default();
        let h = if layout.GetMetrics(&mut m).is_ok() { m.height } else { size * 1.3 };
        if color.a > 0.0 {
            let rt = c.target();
            if let Ok(b) = rt.CreateSolidColorBrush(&col(color), None) {
                rt.DrawTextLayout(Vector2 { X: q.x, Y: q.y }, &layout, &b, D2D1_DRAW_TEXT_OPTIONS(4));
            }
        }
        h
    }
}

pub fn text_width(c: &mut Canvas, s: &str, size: f32, weight: u32) -> f32 {
    let Some(f) = format(c, size, weight, Align::Left, true) else { return 0.0 };
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let Ok(layout) = c.gfx.dw.CreateTextLayout(&w, &f, 100_000.0, 1000.0) else { return 0.0 };
        let mut m = DWRITE_TEXT_METRICS::default();
        if layout.GetMetrics(&mut m).is_ok() { m.widthIncludingTrailingWhitespace } else { 0.0 }
    }
}

/// Plume Convert's icons (16 × 16, round strokes like Plume's).
pub mod ic {
    use super::*;
    use Part::{C, P, R};

    macro_rules! icons {
        ($($id:ident = [$($part:expr),+ $(,)?];)+) => {
            $(pub const $id: Icon = Icon { name: concat!("convert_", stringify!($id)), vb: 16.0, parts: &[$($part),+] };)+
        };
    }

    icons! {
        PHONE = [R(4.5, 1.5, 7.0, 13.0, 1.8), P("M7 12h2")];
        CHAT = [P("M3 3.5h10v7H7.5L4.5 13v-2.5H3z"), P("M6 7h.01M8 7h.01M10 7h.01")];
        SHRINK = [P("M2.5 6.5h4v-4M13.5 9.5h-4v4M6.5 6.5 2.5 2.5M9.5 9.5l4 4")];
        NOTE = [P("M6 12V3.5l7-1.5v8.5"), C(4.3, 12.0, 1.7), C(11.3, 10.5, 1.7)];
        GIF = [R(1.5, 3.5, 13.0, 9.0, 2.0), P("M6 6.5H4.8v3H6V8.3M8 6.5v3M10.3 9.5v-3h1.9M10.3 8h1.5")];
        IMAGE = [R(2.0, 3.0, 12.0, 10.0, 2.0), C(6.0, 6.5, 1.2), P("m2.5 11.5 3.5-3.5 3 3 2-2 2.5 2.5")];
        FRAMES = [R(2.0, 2.0, 5.0, 5.0, 1.2), R(9.0, 2.0, 5.0, 5.0, 1.2), R(2.0, 9.0, 5.0, 5.0, 1.2), R(9.0, 9.0, 5.0, 5.0, 1.2)];
        SCISSORS = [C(4.5, 4.5, 2.0), C(4.5, 11.5, 2.0), P("M6.2 5.7 13.5 12M6.2 10.3 13.5 4")];
        MUTE = [P("M2.5 6h2.5L8.5 3v10L5 10H2.5z"), P("m11 6 3.5 4M14.5 6 11 10")];
        ROTATE = [P("M3 8a5 5 0 1 0 1.5-3.6M3 2.5v3h3")];
        SLIDERS = [P("M2.5 4.5h3.2M8.9 4.5h4.6M2.5 11.5h6.6M12.3 11.5h1.2"), C(7.3, 4.5, 1.6), C(10.7, 11.5, 1.6)];
        DROP = [P("M8 2.5v7.5M5 7l3 3 3-3M2.5 10.5v2a1.5 1.5 0 0 0 1.5 1.5h8a1.5 1.5 0 0 0 1.5-1.5v-2")];
        CONVERT = [P("M3 5.5h9.5M10 3l2.5 2.5L10 8M13 10.5H3.5M6 8l-2.5 2.5L6 13")];
        VIDEO = [R(1.5, 3.5, 9.5, 9.0, 2.0), P("m11 7 3.5-2v6L11 9")];
        OPEN = [P("M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z")];
    }
}
