//! Drawing helpers: text in any font, size and weight (the text helpers are Plume Convert's, with a font family),
//! raw frames as Direct2D bitmaps (straight, cropped, turned, see-through), and Plume Cut's own icons.

use crate::ffmpeg::Frame;
use plume_ui::{Align, Canvas, Color, Rect, icons::Icon, icons::Part};
use std::{cell::RefCell, collections::HashMap};
use windows::{
    Win32::Graphics::{
        Direct2D::{
            Common::{D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT},
            D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_PROPERTIES, D2D1_DRAW_TEXT_OPTIONS, ID2D1Bitmap,
        },
        DirectWrite::{
            DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS,
            DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP, IDWriteInlineObject,
            IDWriteTextFormat,
        },
        Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
        Imaging::{GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory},
    },
    core::{HSTRING, Interface},
};
use windows_numerics::{Matrix3x2, Vector2};

fn col(c: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.r, g: c.g, b: c.b, a: c.a }
}
pub fn rf(q: Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: q.x, top: q.y, right: q.x + q.w, bottom: q.y + q.h }
}

/// Family, size (quarter points), weight, alignment, wrapped.
type FormatKey = (String, u32, u32, u8, bool);

thread_local! {
    static FORMATS: RefCell<HashMap<FormatKey, IDWriteTextFormat>> = RefCell::new(HashMap::new());
}

fn format(c: &Canvas, family: &str, size: f32, weight: u32, align: Align, wrap: bool) -> Option<IDWriteTextFormat> {
    let family = if family.is_empty() { c.gfx.ui_font } else { family };
    let key = (family.to_string(), (size * 4.0) as u32, weight, align as u8, wrap);
    if let Some(f) = FORMATS.with(|m| m.borrow().get(&key).cloned()) {
        return Some(f);
    }
    let f = unsafe {
        let f = c
            .gfx
            .dw
            .CreateTextFormat(
                &HSTRING::from(family),
                None,
                DWRITE_FONT_WEIGHT(weight as i32),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size.max(1.0),
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
    if FORMATS.with(|m| m.borrow().len()) > 200 {
        FORMATS.with(|m| m.borrow_mut().clear());
    }
    FORMATS.with(|m| m.borrow_mut().insert(key, f.clone()));
    Some(f)
}

/// One line of text in the interface font (400 regular, 600 semibold, 700 bold), ending in "…".
pub fn text(c: &mut Canvas, s: &str, q: Rect, size: f32, weight: u32, color: Color, align: Align) {
    if s.is_empty() || q.w <= 0.0 || color.a <= 0.0 {
        return;
    }
    let Some(f) = format(c, "", size, weight, align, false) else { return };
    let rt = c.target();
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let Ok(b) = rt.CreateSolidColorBrush(&col(color), None) else { return };
        rt.DrawText(&w, &f, &rf(q), &b, D2D1_DRAW_TEXT_OPTIONS(2 | 4), DWRITE_MEASURING_MODE_NATURAL);
    }
}

/// Text cut into lines at the words, from the top of `q`. Returns its height.
pub fn text_wrap(c: &mut Canvas, s: &str, q: Rect, size: f32, weight: u32, color: Color, align: Align) -> f32 {
    let Some(f) = format(c, "", size, weight, align, true) else { return size * 1.3 };
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
    measure(c, s, "", size, weight).0
}

/// Width and height of a text (several lines) in a font family.
pub fn measure(c: &mut Canvas, s: &str, family: &str, size: f32, weight: u32) -> (f32, f32) {
    let Some(f) = format(c, family, size, weight, Align::Center, true) else { return (0.0, 0.0) };
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let Ok(layout) = c.gfx.dw.CreateTextLayout(&w, &f, 100_000.0, 100_000.0) else { return (0.0, 0.0) };
        let mut m = DWRITE_TEXT_METRICS::default();
        if layout.GetMetrics(&mut m).is_ok() { (m.widthIncludingTrailingWhitespace, m.height) } else { (0.0, 0.0) }
    }
}

/// A text of the video (several lines, centred) with its top left at `q`'s, in its family, size and colour.
pub fn video_text(c: &mut Canvas, s: &str, q: Rect, family: &str, size: f32, weight: u32, color: Color) {
    let Some(f) = format(c, family, size, weight, Align::Center, true) else { return };
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let Ok(layout) = c.gfx.dw.CreateTextLayout(&w, &f, q.w.max(1.0), q.h.max(1.0)) else { return };
        let rt = c.target();
        if let Ok(b) = rt.CreateSolidColorBrush(&col(color), None) {
            rt.DrawTextLayout(Vector2 { X: q.x, Y: q.y }, &layout, &b, D2D1_DRAW_TEXT_OPTIONS(4));
        }
    }
}

// ───────────────────────── frames ─────────────────────────

/// A frame as a Direct2D bitmap of this surface.
pub fn bitmap(c: &mut Canvas, f: &Frame) -> Option<ID2D1Bitmap> {
    let rt = c.target();
    let props = D2D1_BITMAP_PROPERTIES {
        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
        dpiX: 96.0,
        dpiY: 96.0,
    };
    unsafe { rt.CreateBitmap(D2D_SIZE_U { width: f.w, height: f.h }, Some(f.data.as_ptr() as _), f.w * 4, &props).ok() }
}

/// A frame as a WIC picture (for `Canvas::image`, which keeps it).
pub fn wic_source(wic: &IWICImagingFactory, f: &Frame) -> Option<IWICBitmapSource> {
    unsafe { wic.CreateBitmapFromMemory(f.w, f.h, &GUID_WICPixelFormat32bppPBGRA, f.w * 4, &f.data).ok().and_then(|b| b.cast().ok()) }
}

/// Draws `bmp` (its part `src`, in pixels) into `dst`, turned by `deg` around `dst`'s centre.
pub fn draw_bitmap(c: &mut Canvas, bmp: &ID2D1Bitmap, dst: Rect, src: Option<Rect>, opacity: f32, deg: f32) {
    let rt = c.target();
    unsafe {
        if deg.abs() > 0.01 {
            let (cx, cy) = (dst.x + dst.w / 2.0, dst.y + dst.h / 2.0);
            rt.SetTransform(&rotation(deg, cx, cy));
        }
        let s = src.map(rf);
        rt.DrawBitmap(bmp, Some(&rf(dst)), opacity.clamp(0.0, 1.0), D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, s.as_ref().map(|r| r as *const _));
        if deg.abs() > 0.01 {
            rt.SetTransform(&Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 });
        }
    }
}

pub fn rotation(deg: f32, cx: f32, cy: f32) -> Matrix3x2 {
    let (s, co) = deg.to_radians().sin_cos();
    Matrix3x2 { M11: co, M12: s, M21: -s, M22: co, M31: cx - co * cx + s * cy, M32: cy - s * cx - co * cy }
}

/// The outline of a rectangle turned by `deg` around its centre.
pub fn outline_turned(c: &mut Canvas, q: Rect, deg: f32, color: Color, width: f32) {
    let rt = c.target();
    unsafe {
        let Ok(b) = rt.CreateSolidColorBrush(&col(color), None) else { return };
        rt.SetTransform(&rotation(deg, q.x + q.w / 2.0, q.y + q.h / 2.0));
        rt.DrawRectangle(&rf(q), &b, width, None);
        rt.SetTransform(&Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 });
    }
}

pub fn line(c: &mut Canvas, x1: f32, y1: f32, x2: f32, y2: f32, color: Color, width: f32) {
    let rt = c.target();
    unsafe {
        if let Ok(b) = rt.CreateSolidColorBrush(&col(color), None) {
            rt.DrawLine(Vector2 { X: x1, Y: y1 }, Vector2 { X: x2, Y: y2 }, &b, width, None);
        }
    }
}

/// A point turned by `deg` around (`cx`, `cy`).
pub fn turn(x: f32, y: f32, cx: f32, cy: f32, deg: f32) -> (f32, f32) {
    let (s, co) = deg.to_radians().sin_cos();
    let (dx, dy) = (x - cx, y - cy);
    (cx + dx * co - dy * s, cy + dx * s + dy * co)
}

/// Plume Cut's icons (16 × 16, round strokes like Plume's).
pub mod ic {
    use super::*;
    use Part::{C, P, R};

    macro_rules! icons {
        ($($id:ident = [$($part:expr),+ $(,)?];)+) => {
            $(pub const $id: Icon = Icon { name: concat!("cut_", stringify!($id)), vb: 16.0, parts: &[$($part),+] };)+
        };
    }

    icons! {
        PLAY = [P("M5 3.2v9.6c0 .6.6.9 1.1.6l7.3-4.8a.7.7 0 0 0 0-1.2L6.1 2.6C5.6 2.3 5 2.6 5 3.2z")];
        PAUSE = [R(4.0, 3.0, 2.6, 10.0, 1.0), R(9.4, 3.0, 2.6, 10.0, 1.0)];
        STEP_BACK = [P("M4 3.5v9M12.5 4 7 8l5.5 4z")];
        STEP_FWD = [P("M12 3.5v9M3.5 4 9 8l-5.5 4z")];
        SCISSORS = [C(4.5, 4.5, 2.0), C(4.5, 11.5, 2.0), P("M6.2 5.7 13.5 12M6.2 10.3 13.5 4")];
        UNDO = [P("M5.5 3 2.5 6l3 3"), P("M2.5 6h7a4 4 0 0 1 0 8H7")];
        REDO = [P("m10.5 3 3 3-3 3"), P("M13.5 6h-7a4 4 0 0 0 0 8H9")];
        RIPPLE = [P("M2.5 4h4v8h-4zM9.5 8h4M11.5 6l-2 2 2 2")];
        MAGNET = [P("M3.5 2.5v5.5a4.5 4.5 0 0 0 9 0V2.5h-3V8a1.5 1.5 0 0 1-3 0V2.5z"), P("M3.5 5h3M9.5 5h3")];
        ZOOM_IN = [C(7.0, 7.0, 4.5), P("m10.5 10.5 3 3M5 7h4M7 5v4")];
        ZOOM_OUT = [C(7.0, 7.0, 4.5), P("m10.5 10.5 3 3M5 7h4")];
        FIT = [P("M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10")];
        TEXT = [P("M3 4V2.5h10V4M8 2.5v11M6 13.5h4")];
        FILM = [R(2.0, 2.5, 12.0, 11.0, 2.0), P("M5 2.5v11M11 2.5v11M2 6h3M2 10h3M11 6h3M11 10h3")];
        NOTE = [P("M6 12V3.5l7-1.5v8.5"), C(4.3, 12.0, 1.7), C(11.3, 10.5, 1.7)];
        IMAGE = [R(2.0, 3.0, 12.0, 10.0, 2.0), C(6.0, 6.5, 1.2), P("m2.5 11.5 3.5-3.5 3 3 2-2 2.5 2.5")];
        SPEAKER = [P("M2.5 6h2.5L8.5 3v10L5 10H2.5z"), P("M11 5.5a3.5 3.5 0 0 1 0 5")];
        MUTE = [P("M2.5 6h2.5L8.5 3v10L5 10H2.5z"), P("m11 6 3.5 4M14.5 6 11 10")];
        EYE = [P("M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z"), C(8.0, 8.0, 2.0)];
        EYE_OFF = [P("M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z"), P("M2.5 13.5l11-11")];
        EXPORT = [P("M8 10V2.5M5 5.5l3-3 3 3"), P("M2.5 9.5v3a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1v-3")];
        IMPORT = [P("M8 2.5V10M5 7l3 3 3-3"), P("M2.5 9.5v3a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1v-3")];
        KEYBOARD = [R(1.5, 4.0, 13.0, 8.5, 1.8), P("M4 7h.01M6.5 7h.01M9 7h.01M11.5 7h.01M5 10h6")];
        TRANSITION = [R(1.5, 4.0, 6.0, 8.0, 1.5), R(8.5, 4.0, 6.0, 8.0, 1.5), P("M7.5 6.5 8.5 8l-1 1.5")];
    }
}
