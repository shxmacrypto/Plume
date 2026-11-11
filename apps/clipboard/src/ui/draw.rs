//! Drawing beyond plume-ui's basics: text in any size, and Plume Clipboard's icons.

use plume_ui::{Align, Canvas, Color, Rect, icons::Icon, icons::Part};
use std::{cell::RefCell, collections::HashMap};
use windows::{
    Win32::Graphics::{
        Direct2D::{Common::D2D_RECT_F, Common::D2D1_COLOR_F, D2D1_DRAW_TEXT_OPTIONS},
        DirectWrite::{
            DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TRIMMING,
            DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP, IDWriteInlineObject, IDWriteTextFormat,
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
        }
        let sign: Option<IDWriteInlineObject> = c.gfx.dw.CreateEllipsisTrimmingSign(&f).ok();
        let _ = f.SetTrimming(&DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 }, sign.as_ref());
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

/// Text cut into lines at the words, inside `q` (what does not fit is cut off, the last line ends in "…").
pub fn text_box(c: &mut Canvas, s: &str, q: Rect, size: f32, weight: u32, color: Color) {
    if s.is_empty() || q.w <= 0.0 || q.h <= 0.0 || color.a <= 0.0 {
        return;
    }
    let Some(f) = format(c, size, weight, Align::Left, true) else { return };
    let w: Vec<u16> = s.encode_utf16().collect();
    let rt = c.target();
    unsafe {
        let Ok(layout) = c.gfx.dw.CreateTextLayout(&w, &f, q.w, q.h) else { return };
        if let Ok(b) = rt.CreateSolidColorBrush(&col(color), None) {
            rt.PushAxisAlignedClip(&rf(q), windows::Win32::Graphics::Direct2D::D2D1_ANTIALIAS_MODE_ALIASED);
            rt.DrawTextLayout(Vector2 { X: q.x, Y: q.y }, &layout, &b, D2D1_DRAW_TEXT_OPTIONS(4));
            rt.PopAxisAlignedClip();
        }
    }
}

/// Plume Clipboard's icons (16 × 16, like Plume's).
pub mod ic {
    use super::*;
    use Part::{C, P, R};

    macro_rules! icons {
        ($($id:ident = [$($part:expr),+ $(,)?];)+) => {
            $(pub const $id: Icon = Icon { name: concat!("clip_", stringify!($id)), vb: 16.0, parts: &[$($part),+] };)+
        };
    }

    icons! {
        BOARD = [P("M5.5 3H4.5A1.5 1.5 0 0 0 3 4.5v8A1.5 1.5 0 0 0 4.5 14h7a1.5 1.5 0 0 0 1.5-1.5v-8A1.5 1.5 0 0 0 11.5 3h-1"), R(5.5, 1.8, 5.0, 2.6, 1.0)];
        ALL = [P("M8 2 2.5 5 8 8l5.5-3z"), P("M2.5 8 8 11l5.5-3"), P("M2.5 11 8 14l5.5-3")];
        TEXT = [P("M3 3.5h10M3 6.5h10M3 9.5h10M3 12.5h6")];
        LINK = [P("M6.8 9.2a2.5 2.5 0 0 0 3.6 0l2.2-2.2a2.5 2.5 0 0 0-3.6-3.6l-.8.8M9.2 6.8a2.5 2.5 0 0 0-3.6 0L3.4 9a2.5 2.5 0 0 0 3.6 3.6l.8-.8")];
        IMAGE = [R(2.0, 3.0, 12.0, 10.0, 2.0), C(5.8, 6.4, 1.2), P("M2.5 12l3.5-3.5 2.5 2.5 2-2 3.5 3.5")];
        FILES = [P("M5 2.5h4.5l3 3V12a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 12V4A1.5 1.5 0 0 1 5 2.5z"), P("M9.5 2.5v3h3")];
        SNIPPET = [P("M6 2.5c-1.5 0-2 .7-2 2v1.3c0 .9-.5 1.6-1.5 2.2 1 .6 1.5 1.3 1.5 2.2v1.3c0 1.3.5 2 2 2"), P("M10 2.5c1.5 0 2 .7 2 2v1.3c0 .9.5 1.6 1.5 2.2-1 .6-1.5 1.3-1.5 2.2v1.3c0 1.3-.5 2-2 2")];
        KEYBOARD = [R(1.5, 4.0, 13.0, 8.5, 1.8), P("M4.5 7h.01M7 7h.01M9.5 7h.01M12 7h.01M5 10h6")];
        PAUSE = [R(4.0, 3.0, 2.6, 10.0, 0.8), R(9.4, 3.0, 2.6, 10.0, 0.8)];
        ARROW_UP = [P("M8 13V3.5M4 7.5 8 3.5l4 4")];
        ARROW_DOWN = [P("M8 3v9.5M4 8.5l4 4 4-4")];
        SHIELD = [P("M8 2 3 4v4c0 3 2.2 5.2 5 6 2.8-.8 5-3 5-6V4z")];
    }
}

/// "now", "5 min", "3 h", "yesterday", or the date.
pub fn ago(at: u64, now: u64) -> String {
    use crate::i18n::{fill, tr};
    let d = now.saturating_sub(at);
    if d < 60 {
        tr("now").to_string()
    } else if d < 3600 {
        fill(tr("{n} min ago"), &[&(d / 60).to_string()])
    } else if d < 86_400 {
        fill(tr("{n} h ago"), &[&(d / 3600).to_string()])
    } else if d < 2 * 86_400 {
        tr("yesterday").to_string()
    } else {
        fill(tr("{n} days ago"), &[&(d / 86_400).to_string()])
    }
}
