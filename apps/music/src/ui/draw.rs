//! Drawing beyond plume-ui's basics: pictures with round corners, text in any size (titles, lyrics), radial glows
//! (the moving background), the blurred cover, and Plume Music's own icons.

use plume_ui::{Align, Canvas, Color, Rect, icons::Icon, icons::Part};
use std::{cell::RefCell, collections::HashMap};
use windows::{
    Win32::Graphics::{
        Direct2D::{
            CLSID_D2D1GaussianBlur,
            Common::{D2D_RECT_F, D2D1_BORDER_MODE_HARD, D2D1_COLOR_F, D2D1_COMPOSITE_MODE_SOURCE_OVER, D2D1_GRADIENT_STOP},
            D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS, D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2,
            D2D1_GAUSSIANBLUR_PROP_BORDER_MODE, D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_LAYER_OPTIONS_NONE,
            D2D1_LAYER_PARAMETERS, D2D1_PROPERTY_TYPE_ENUM, D2D1_PROPERTY_TYPE_FLOAT, D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES, D2D1_ROUNDED_RECT, ID2D1Bitmap,
            ID2D1DeviceContext, ID2D1RenderTarget,
        },
        DirectWrite::{
            DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS,
            DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP, IDWriteInlineObject,
            IDWriteTextFormat,
        },
        Imaging::IWICBitmapSource,
    },
    core::{HSTRING, Interface},
};
use windows_numerics::{Matrix3x2, Vector2};

fn col(c: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.r, g: c.g, b: c.b, a: c.a }
}
fn rf(q: Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: q.x, top: q.y, right: q.x + q.w, bottom: q.y + q.h }
}

/// Draws whatever `body` draws, clipped to a rounded rectangle.
pub fn rounded(c: &mut Canvas, q: Rect, radius: f32, body: impl FnOnce(&mut Canvas)) {
    if radius <= 0.5 {
        c.clip(q);
        body(c);
        c.unclip();
        return;
    }
    let rt = c.target();
    let geo = unsafe { c.gfx.d2d.CreateRoundedRectangleGeometry(&D2D1_ROUNDED_RECT { rect: rf(q), radiusX: radius, radiusY: radius }) };
    let Ok(geo) = geo else {
        body(c);
        return;
    };
    unsafe {
        let params = D2D1_LAYER_PARAMETERS {
            contentBounds: rf(q),
            geometricMask: std::mem::ManuallyDrop::new(Some(geo.cast().expect("geometry"))),
            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 },
            opacity: 1.0,
            opacityBrush: std::mem::ManuallyDrop::new(None),
            layerOptions: D2D1_LAYER_OPTIONS_NONE,
        };
        rt.PushLayer(&params, None);
        body(c);
        rt.PopLayer();
        // the layer parameters hold a reference: give it back
        let mut p = params;
        std::mem::ManuallyDrop::drop(&mut p.geometricMask);
    }
}

/// A picture filling `q` (cropped to keep its proportions), with round corners.
pub fn image_fill(c: &mut Canvas, key: u64, q: Rect, radius: f32, opacity: f32, make: impl FnOnce() -> Option<IWICBitmapSource>) {
    // covers are square: drawn straight; other pictures (GIF) are cropped by the clip
    rounded(c, q, radius, |c| c.image(key, q, opacity, make));
}

/// A picture covering `q` while keeping its proportions (w/h: its size).
pub fn image_cover(c: &mut Canvas, key: u64, q: Rect, w: f32, h: f32, opacity: f32, make: impl FnOnce() -> Option<IWICBitmapSource>) {
    let k = (q.w / w.max(1.0)).max(q.h / h.max(1.0));
    let (dw, dh) = (w * k, h * k);
    let r = Rect { x: q.x + (q.w - dw) / 2.0, y: q.y + (q.h - dh) / 2.0, w: dw, h: dh };
    c.clip(q);
    c.image(key, r, opacity, make);
    c.unclip();
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

/// One line of text in any size and weight (400 regular, 600 semibold, 700 bold, 800 heavy), ending in "…".
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

/// Text cut into lines at the words, from the top of `q` (its width counts). Returns its height.
#[allow(clippy::too_many_arguments)]
pub fn text_wrap(c: &mut Canvas, s: &str, q: Rect, size: f32, weight: u32, color: Color, align: Align, draw: bool) -> f32 {
    let Some(f) = format(c, size, weight, align, true) else { return size * 1.3 };
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let Ok(layout) = c.gfx.dw.CreateTextLayout(&w, &f, q.w.max(1.0), 10_000.0) else { return size * 1.3 };
        let mut m = DWRITE_TEXT_METRICS::default();
        let h = if layout.GetMetrics(&mut m).is_ok() { m.height } else { size * 1.3 };
        if draw && color.a > 0.0 {
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

/// A soft round glow: `color` at the center, fading to nothing at radius `r`.
pub fn glow(c: &mut Canvas, cx: f32, cy: f32, r: f32, color: Color) {
    let rt = c.target();
    unsafe {
        let stops = [
            D2D1_GRADIENT_STOP { position: 0.0, color: col(color) },
            D2D1_GRADIENT_STOP { position: 0.55, color: col(color.fade(0.45)) },
            D2D1_GRADIENT_STOP { position: 1.0, color: col(color.fade(0.0)) },
        ];
        let Ok(coll) = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) else { return };
        let props = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
            center: Vector2 { X: cx, Y: cy },
            gradientOriginOffset: Vector2 { X: 0.0, Y: 0.0 },
            radiusX: r,
            radiusY: r,
        };
        if let Ok(b) = rt.CreateRadialGradientBrush(&props, None, &coll) {
            rt.FillEllipse(&D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: r, radiusY: r }, &b);
        }
    }
}

/// A vertical gradient with transparency (headers that fade into the page).
pub fn fade_v(c: &mut Canvas, q: Rect, top: Color, bottom: Color) {
    c.gradient_v(q, top, bottom);
}

/// A cover blurred to fill `q` (Apple Music's background). `bmp`: the cover as a Direct2D bitmap of this target.
pub fn blurred(rt: &ID2D1RenderTarget, bmp: &ID2D1Bitmap, q: Rect, sigma: f32) -> bool {
    unsafe {
        let Ok(dc) = rt.cast::<ID2D1DeviceContext>() else { return false };
        let Ok(fx) = dc.CreateEffect(&CLSID_D2D1GaussianBlur) else { return false };
        fx.SetInput(0, bmp, true);
        let _ = fx.SetValue(D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32, D2D1_PROPERTY_TYPE_FLOAT, &sigma.to_le_bytes());
        let _ = fx.SetValue(D2D1_GAUSSIANBLUR_PROP_BORDER_MODE.0 as u32, D2D1_PROPERTY_TYPE_ENUM, &(D2D1_BORDER_MODE_HARD.0 as u32).to_le_bytes());
        let size = bmp.GetSize();
        // the bitmap scaled to cover the area (the effect works in pixels: the screen's scale is added back)
        let k = (q.w / size.width.max(1.0)).max(q.h / size.height.max(1.0)) * 1.08;
        let ox = q.x + (q.w - size.width * k) / 2.0;
        let oy = q.y + (q.h - size.height * k) / 2.0;
        let (mut dx, mut dy) = (96.0f32, 96.0f32);
        dc.GetDpi(&mut dx, &mut dy);
        let kd = k * dx / 96.0;
        let mut old = Matrix3x2::default();
        dc.GetTransform(&mut old);
        dc.SetTransform(&Matrix3x2 { M11: kd, M12: 0.0, M21: 0.0, M22: kd, M31: ox, M32: oy });
        let Ok(img) = fx.GetOutput() else {
            dc.SetTransform(&old);
            return false;
        };
        dc.DrawImage(&img, None, None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
        dc.SetTransform(&old);
        true
    }
}

/// Plume Music's icons (16 × 16, like Plume's).
pub mod ic {
    use super::*;
    use Part::{C, P, R};

    macro_rules! icons {
        ($($id:ident = [$($part:expr),+ $(,)?];)+) => {
            $(pub const $id: Icon = Icon { name: concat!("music_", stringify!($id)), vb: 16.0, parts: &[$($part),+] };)+
        };
    }

    icons! {
        PLAY = [P("M5 3.2v9.6a.6.6 0 0 0 .9.5l7.6-4.8a.6.6 0 0 0 0-1L5.9 2.7a.6.6 0 0 0-.9.5z")];
        PAUSE = [R(4.0, 3.0, 2.6, 10.0, 0.8), R(9.4, 3.0, 2.6, 10.0, 0.8)];
        NEXT = [P("M3.5 3.5v9l6.5-4.5zM12 3.5v9")];
        PREV = [P("M12.5 3.5v9L6 8zM4 3.5v9")];
        SHUFFLE = [P("M2 4.5h2.5c3.5 0 4 7 7.5 7h2M2 11.5h2.5c1.4 0 2.2-1 2.9-2.3M9.6 6.8C10.2 5.5 11 4.5 12 4.5h2M12.5 2.8 14 4.5l-1.5 1.7M12.5 9.8 14 11.5l-1.5 1.7")];
        REPEAT = [P("M3 7V6a2 2 0 0 1 2-2h8M11 2l2 2-2 2M13 9v1a2 2 0 0 1-2 2H3M5 14l-2-2 2-2")];
        HEART = [P("M8 13.5S2 10 2 6a3 3 0 0 1 6-1 3 3 0 0 1 6 1c0 4-6 7.5-6 7.5z")];
        LYRICS = [P("M3 3.5h10v7H7l-3 2.5v-2.5H3z"), P("M5.5 6h5M5.5 8h3")];
        QUEUE = [P("M2.5 4h8M2.5 8h8M2.5 12h5M12 9.5v4.5M12 14a1.5 1.5 0 1 1-1.5-1.5H12")];
        VOLUME = [P("M2.5 6h2.5L8.5 3v10L5 10H2.5z"), P("M11 5.5a3.5 3.5 0 0 1 0 5M12.8 3.8a6 6 0 0 1 0 8.4")];
        MUTE = [P("M2.5 6h2.5L8.5 3v10L5 10H2.5z"), P("m11 6 3.5 4M14.5 6 11 10")];
        EXPAND = [P("M10 2.5h3.5V6M6 13.5H2.5V10M13.5 2.5 9 7M2.5 13.5 7 9")];
        COLLAPSE = [P("M9 3v4h4M7 13V9H3M9 7l4.5-4.5M7 9l-4.5 4.5")];
        DOWNLOAD = [P("M8 2.5v8M4.5 7 8 10.5 11.5 7M3 13.5h10")];
        DONE = [C(8.0, 8.0, 6.0), P("m5.3 8.2 1.9 1.9 3.6-3.8")];
        NOTE = [P("M6 12V3.5l7-1.5v8.5"), C(4.3, 12.0, 1.7), C(11.3, 10.5, 1.7)];
        IMPORT = [P("M8 2.5v7M5 6.5l3 3 3-3M3 10.5v1.5A1.5 1.5 0 0 0 4.5 13.5h7A1.5 1.5 0 0 0 13 12v-1.5")];
        ALBUM = [C(8.0, 8.0, 5.8), C(8.0, 8.0, 1.6)];
        MIC = [R(5.8, 2.0, 4.4, 7.5, 2.2), P("M3.5 8a4.5 4.5 0 0 0 9 0M8 12.5V14")];
        TIMER = [C(8.0, 9.0, 5.0), P("M8 6.5V9l1.8 1.3M6.5 2h3")];
        LINK = [P("M6.8 9.2a2.5 2.5 0 0 0 3.6 0l2.2-2.2a2.5 2.5 0 0 0-3.6-3.6l-.8.8M9.2 6.8a2.5 2.5 0 0 0-3.6 0L3.4 9a2.5 2.5 0 0 0 3.6 3.6l.8-.8")];
    }
}

#[allow(dead_code)]
const _UNUSED: Option<&D2D_RECT_F> = None;
