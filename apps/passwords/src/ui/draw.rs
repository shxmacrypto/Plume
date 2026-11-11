//! Drawing beyond plume-ui's basics: text in any size (codes, generated passwords) and Plume Passwords' icons.

use plume_ui::{Align, Canvas, Color, Rect};
use std::{cell::RefCell, collections::HashMap};
use windows::{
    Win32::Graphics::{
        Direct2D::{
            Common::{D2D_RECT_F, D2D1_COLOR_F},
            D2D1_DRAW_TEXT_OPTIONS,
        },
        DirectWrite::{
            DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TRIMMING,
            DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, IDWriteInlineObject, IDWriteTextFormat,
        },
    },
    core::HSTRING,
};

thread_local! {
    static FORMATS: RefCell<HashMap<(u32, u32, u8, bool), IDWriteTextFormat>> = RefCell::new(HashMap::new());
}

/// Monospaced text (passwords, codes): every character readable, 0 and O apart.
const MONO: &str = "Cascadia Mono";

fn format(c: &Canvas, size: f32, weight: u32, align: Align, mono: bool) -> Option<IDWriteTextFormat> {
    let key = ((size * 4.0) as u32, weight, align as u8, mono);
    if let Some(f) = FORMATS.with(|m| m.borrow().get(&key).cloned()) {
        return Some(f);
    }
    let f = unsafe {
        let family = if mono { MONO } else { c.gfx.ui_font };
        let f = c
            .gfx
            .dw
            .CreateTextFormat(
                &HSTRING::from(family),
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
        let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        let sign: Option<IDWriteInlineObject> = c.gfx.dw.CreateEllipsisTrimmingSign(&f).ok();
        let _ = f.SetTrimming(&DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 }, sign.as_ref());
        f
    };
    FORMATS.with(|m| m.borrow_mut().insert(key, f.clone()));
    Some(f)
}

/// One line of text in any size and weight, monospaced or not, ending in "…" if too long.
#[allow(clippy::too_many_arguments)]
pub fn text(c: &mut Canvas, s: &str, q: Rect, size: f32, weight: u32, color: Color, align: Align, mono: bool) {
    if s.is_empty() || q.w <= 0.0 || color.a <= 0.0 {
        return;
    }
    let Some(f) = format(c, size, weight, align, mono) else { return };
    let rt = c.target();
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let Ok(b) = rt.CreateSolidColorBrush(&D2D1_COLOR_F { r: color.r, g: color.g, b: color.b, a: color.a }, None) else { return };
        rt.DrawText(
            &w,
            &f,
            &D2D_RECT_F { left: q.x, top: q.y, right: q.x + q.w, bottom: q.y + q.h },
            &b,
            D2D1_DRAW_TEXT_OPTIONS(2 | 4),
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }
}

/// The color of an entry's round badge, from its title (always the same for the same title).
pub fn badge_color(title: &str) -> Color {
    const COLORS: [u32; 8] = [0x22a75f, 0x0ea5e9, 0x8b5cf6, 0xf97316, 0xe11d48, 0x14b8a6, 0xeab308, 0x6366f1];
    let h = title.to_lowercase().bytes().fold(5381u32, |h, b| h.wrapping_mul(33) ^ b as u32);
    Color::hex(COLORS[(h % COLORS.len() as u32) as usize])
}

pub mod ic {
    use Part::{C, P, R};
    use plume_ui::icons::{Icon, Part};

    macro_rules! icons {
        ($($id:ident = [$($part:expr),+ $(,)?];)+) => {
            $(pub const $id: Icon = Icon { name: concat!("passwords_", stringify!($id)), vb: 16.0, parts: &[$($part),+] };)+
        };
    }

    icons! {
        LOCK = [R(3.0, 7.0, 10.0, 7.0, 1.8), P("M5.3 7V5a2.7 2.7 0 0 1 5.4 0v2")];
        KEY = [C(5.2, 10.8, 2.7), P("M7.2 8.8 13 3M11 5l1.8 1.8M9.6 6.4l1.4 1.4")];
        EYE = [P("M1.8 8S4.2 3.6 8 3.6 14.2 8 14.2 8 11.8 12.4 8 12.4 1.8 8 1.8 8z"), C(8.0, 8.0, 2.1)];
        EYE_OFF = [P("M6.2 3.9A6.4 6.4 0 0 1 8 3.6C11.8 3.6 14.2 8 14.2 8a12 12 0 0 1-1.7 2.2M9.9 9.9A2.1 2.1 0 0 1 6.1 8a2 2 0 0 1 .6-1.4M4.3 5.1A11.5 11.5 0 0 0 1.8 8s2.4 4.4 6.2 4.4a6 6 0 0 0 3.2-.9M2.5 2.5l11 11")];
        CARD = [R(1.8, 3.5, 12.4, 9.0, 1.8), P("M1.8 6.5h12.4M4.3 10h2.6")];
        NOTE = [P("M3.5 2h6l3 3v9h-9z"), P("M9.5 2v3h3M5.6 8.2h4.8M5.6 10.8h3.4")];
        CLOCK = [C(8.0, 8.0, 5.8), P("M8 4.8V8l2.2 1.4")];
        SHIELD = [P("M8 1.8 13 3.6v4.1c0 3.1-2.1 5.4-5 6.5-2.9-1.1-5-3.4-5-6.5V3.6z"), P("m5.8 8 1.6 1.6 2.9-3")];
        DICE = [R(2.3, 2.3, 11.4, 11.4, 2.6), P("M5.4 5.4h.01M10.6 5.4h.01M8 8h.01M5.4 10.6h.01M10.6 10.6h.01")];
        KEYBOARD = [R(1.6, 4.0, 12.8, 8.4, 1.8), P("M4.2 6.6h.01M6.7 6.6h.01M9.3 6.6h.01M11.8 6.6h.01M5 10h6")];
        LINK = [P("M6.8 9.2a2.6 2.6 0 0 0 3.7 0l2.2-2.2a2.6 2.6 0 0 0-3.7-3.7l-.9.9"), P("M9.2 6.8a2.6 2.6 0 0 0-3.7 0L3.3 9a2.6 2.6 0 0 0 3.7 3.7l.9-.9")];
        GRID = [R(2.2, 2.2, 5.0, 5.0, 1.2), R(8.8, 2.2, 5.0, 5.0, 1.2), R(2.2, 8.8, 5.0, 5.0, 1.2), R(8.8, 8.8, 5.0, 5.0, 1.2)];
        FINGER = [P("M4.3 6.5a3.7 3.7 0 0 1 7.4 0v1.6M8 6.6v3.2c0 1.6.6 2.9 1.5 3.9M5.6 8.6c0 2 .5 3.5 1.4 4.8M10.4 10.4c.1 1 .4 1.8.8 2.5M2.8 9.2c0-1 0-2 .2-2.9")];
        IMAGE = [R(2.0, 2.6, 12.0, 10.8, 1.8), C(5.6, 6.2, 1.3), P("m2.4 12 3.8-3.6 2.6 2.4 2-1.8L14 12")];
        IMPORT = [P("M8 2.5v7.5M5 7l3 3 3-3"), P("M2.8 10.5v1.7a1.3 1.3 0 0 0 1.3 1.3h7.8a1.3 1.3 0 0 0 1.3-1.3v-1.7")];
        EXPORT = [P("M8 10V2.5M5 5.5l3-3 3 3"), P("M2.8 10.5v1.7a1.3 1.3 0 0 0 1.3 1.3h7.8a1.3 1.3 0 0 0 1.3-1.3v-1.7")];
        WARN = [P("M8 2.2 14.3 13H1.7z"), P("M8 6.4v3M8 11.2h.01")];
        REPEAT = [P("M3 6.5V6a2.5 2.5 0 0 1 2.5-2.5h7M10.5 1.5l2 2-2 2"), P("M13 9.5v.5a2.5 2.5 0 0 1-2.5 2.5h-7M5.5 14.5l-2-2 2-2")];
    }
}
