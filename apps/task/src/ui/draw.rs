//! Drawing beyond plume-ui's basics: text in any size, the live graphs (Direct2D paths), pictures from pixels or
//! PNG bytes, and Plume Task's own icons.

use plume_ui::{Align, Canvas, Color, Rect, icons::Icon, icons::Part};
use std::{cell::RefCell, collections::HashMap, collections::VecDeque};
use windows::{
    Win32::Graphics::{
        Direct2D::{
            Common::{D2D_RECT_F, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN},
            D2D1_DRAW_TEXT_OPTIONS,
        },
        DirectWrite::{
            DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TRIMMING,
            DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, IDWriteInlineObject, IDWriteTextFormat,
        },
        Imaging::{
            GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
            WICDecodeMetadataCacheOnDemand,
        },
    },
    core::{HSTRING, Interface},
};
use windows_numerics::Vector2;

fn col(c: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.r, g: c.g, b: c.b, a: c.a }
}
fn rf(q: Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: q.x, top: q.y, right: q.x + q.w, bottom: q.y + q.h }
}

thread_local! {
    static FORMATS: RefCell<HashMap<(u32, u32, u8), IDWriteTextFormat>> = RefCell::new(HashMap::new());
}

fn format(c: &Canvas, size: f32, weight: u32, align: Align) -> Option<IDWriteTextFormat> {
    let key = ((size * 4.0) as u32, weight, align as u8);
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
        let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
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
    let Some(f) = format(c, size, weight, align) else { return };
    let rt = c.target();
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let Ok(b) = rt.CreateSolidColorBrush(&col(color), None) else { return };
        rt.DrawText(&w, &f, &rf(q), &b, D2D1_DRAW_TEXT_OPTIONS(2 | 4), DWRITE_MEASURING_MODE_NATURAL);
    }
}

/// A line graph of the last values, filled underneath; `max` is the top of the graph. The newest value is on the
/// right; a history shorter than `slots` starts further right.
pub fn graph(c: &mut Canvas, r: Rect, values: &VecDeque<f32>, slots: usize, max: f32, color: Color, fill: bool) {
    if values.len() < 2 || r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    let max = max.max(1e-6);
    let step = r.w / (slots.max(2) - 1) as f32;
    let n = values.len().min(slots);
    let start = values.len() - n;
    let x0 = r.right() - (n - 1) as f32 * step;
    let pts: Vec<Vector2> = (0..n).map(|i| Vector2 { X: x0 + i as f32 * step, Y: r.bottom() - (values[start + i] / max).clamp(0.0, 1.0) * r.h }).collect();
    let rt = c.target();
    unsafe {
        let make = |closed: bool| -> Option<windows::Win32::Graphics::Direct2D::ID2D1PathGeometry> {
            let g = c.gfx.d2d.CreatePathGeometry().ok()?;
            let sink = g.Open().ok()?;
            if closed {
                sink.BeginFigure(Vector2 { X: pts[0].X, Y: r.bottom() }, D2D1_FIGURE_BEGIN_FILLED);
                sink.AddLines(&pts);
                sink.AddLine(Vector2 { X: pts[n - 1].X, Y: r.bottom() });
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            } else {
                sink.BeginFigure(pts[0], D2D1_FIGURE_BEGIN_HOLLOW);
                sink.AddLines(&pts[1..]);
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
            }
            sink.Close().ok()?;
            Some(g)
        };
        if fill
            && let Some(g) = make(true)
            && let Ok(b) = rt.CreateSolidColorBrush(&col(color.fade(0.16)), None)
        {
            rt.FillGeometry(&g, &b, None);
        }
        if let Some(g) = make(false)
            && let Ok(b) = rt.CreateSolidColorBrush(&col(color), None)
        {
            rt.DrawGeometry(&g, &b, 1.6, &c.gfx.round);
        }
    }
}

/// A picture from premultiplied BGRA pixels, kept by the canvas under `key`.
pub fn pixels(c: &mut Canvas, key: u64, q: Rect, px: Option<&(u32, u32, Vec<u8>)>) {
    let wic = c.gfx.wic.clone();
    c.image(key, q, 1.0, || {
        let (w, h, data) = px?;
        let bmp = unsafe { wic.CreateBitmapFromMemory(*w, *h, &GUID_WICPixelFormat32bppPBGRA, w * 4, data) }.ok()?;
        bmp.cast().ok()
    });
}

/// A PNG (the app's logo) as a picture source.
pub fn png(wic: &IWICImagingFactory, bytes: &[u8]) -> Option<IWICBitmapSource> {
    unsafe {
        let stream = wic.CreateStream().ok()?;
        stream.InitializeFromMemory(bytes).ok()?;
        let dec = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let bmp = wic.CreateBitmapFromSource(&conv, windows::Win32::Graphics::Imaging::WICBitmapCacheOnLoad).ok()?;
        bmp.cast().ok()
    }
}

/// Plume Task's icons (16 × 16, round strokes like Plume's).
pub mod ic {
    use super::*;
    use Part::{C, P, R};

    macro_rules! icons {
        ($($id:ident = [$($part:expr),+ $(,)?];)+) => {
            $(pub const $id: Icon = Icon { name: concat!("task_", stringify!($id)), vb: 16.0, parts: &[$($part),+] };)+
        };
    }

    icons! {
        APPS = [R(2.5, 2.5, 4.5, 4.5, 1.2), R(9.0, 2.5, 4.5, 4.5, 1.2), R(2.5, 9.0, 4.5, 4.5, 1.2), R(9.0, 9.0, 4.5, 4.5, 1.2)];
        PULSE = [P("M1.5 8.5h3l1.8-4.5 3 8 1.9-5 1 1.5h2.3")];
        ROCKET = [P("M9.5 2.5c2-1 4-1 4-1s0 2-1 4L8.6 9.4 6.6 7.4z"), P("M6.6 7.4 4 7l-2 2 3 .6M8.6 9.4 9 12l-2 2-.6-3"), P("M4.5 11.5 2.5 13.5")];
        CHIP = [R(4.0, 4.0, 8.0, 8.0, 1.5), R(6.3, 6.3, 3.4, 3.4, 0.6), P("M6 1.8V4M10 1.8V4M6 12v2.2M10 12v2.2M1.8 6H4M1.8 10H4M12 6h2.2M12 10h2.2")];
        MEMORY = [R(1.8, 4.5, 12.4, 6.5, 1.2), P("M4.5 11v2M8 11v2M11.5 11v2M5 7v1.5M8 7v1.5M11 7v1.5")];
        DISK = [R(2.0, 3.0, 12.0, 10.0, 2.0), P("M2 9.5h12"), P("M11 11.25h.01")];
        NET = [P("M5 2.5v10M2.5 5 5 2.5 7.5 5M11 13.5v-10M8.5 11 11 13.5 13.5 11")];
        GPU = [R(1.5, 4.0, 13.0, 8.0, 1.5), C(10.0, 8.0, 2.0), P("M4 7h2.5M4 9h2.5M3.5 12v2")];
        END = [C(8.0, 8.0, 6.0), P("M5.8 5.8l4.4 4.4M10.2 5.8l-4.4 4.4")];
        INFO = [C(8.0, 8.0, 6.0), P("M8 7.2v4M8 4.9h.01")];
        WARN = [P("M8 2.2 14.3 13H1.7z"), P("M8 6.3v3M8 11.2h.01")];
        OK = [C(8.0, 8.0, 6.0), P("m5.3 8.2 1.9 1.9 3.6-3.8")];
        WINDOW = [R(2.0, 3.0, 12.0, 10.0, 1.8), P("M2 6h12")];
    }
}

#[allow(dead_code)]
const _UNUSED: Option<&Icon> = None;
