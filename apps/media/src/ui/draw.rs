//! Drawing beyond plume-ui's basics: pictures from pixels (kept as Direct2D bitmaps, dropped when not seen for a
//! while), pictures turned and cropped, text in any size (titles, subtitles), and Plume Media's own icons.

use crate::image::Px;
use plume_ui::{Align, Canvas, Color, Rect};
use std::{cell::RefCell, collections::HashMap};
use windows::{
    Win32::Graphics::{
        Direct2D::{
            Common::{D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT},
            D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR, D2D1_BITMAP_PROPERTIES, D2D1_DRAW_TEXT_OPTIONS,
            D2D1_ELLIPSE, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, ID2D1Bitmap, ID2D1DeviceContext, ID2D1RenderTarget,
        },
        DirectWrite::{
            DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
            DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS, DWRITE_TRIMMING,
            DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP, IDWriteInlineObject, IDWriteTextFormat,
        },
        Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
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

/// Identity of the Direct2D surface: bitmaps belong to one surface and must be made again when it is replaced.
pub fn target_id(rt: &ID2D1RenderTarget) -> usize {
    rt.as_raw() as usize
}

pub fn make_bitmap(rt: &ID2D1RenderTarget, px: &Px) -> Option<ID2D1Bitmap> {
    if px.w == 0 || px.h == 0 {
        return None;
    }
    let props = D2D1_BITMAP_PROPERTIES {
        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
        dpiX: 96.0,
        dpiY: 96.0,
    };
    unsafe { rt.CreateBitmap(D2D_SIZE_U { width: px.w, height: px.h }, Some(px.data.as_ptr() as *const _), px.w * 4, &props).ok() }
}

/// An empty bitmap (filled later by `CopyFromMemory`: video frames).
pub fn empty_bitmap(rt: &ID2D1RenderTarget, w: u32, h: u32) -> Option<ID2D1Bitmap> {
    let props = D2D1_BITMAP_PROPERTIES {
        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
        dpiX: 96.0,
        dpiY: 96.0,
    };
    unsafe { rt.CreateBitmap(D2D_SIZE_U { width: w, height: h }, None, 0, &props).ok() }
}

/// Bitmaps by key, the least recently drawn dropped first.
#[derive(Default)]
pub struct Bitmaps {
    map: HashMap<u64, (ID2D1Bitmap, u64)>,
    rt: usize,
    frame: u64,
}

impl Bitmaps {
    /// At the start of each image: everything is dropped if the surface changed.
    pub fn begin(&mut self, rt: &ID2D1RenderTarget) -> bool {
        self.frame += 1;
        let id = target_id(rt);
        if id != self.rt {
            self.map.clear();
            self.rt = id;
            return true;
        }
        false
    }
    pub fn get(&mut self, key: u64) -> Option<ID2D1Bitmap> {
        let f = self.frame;
        self.map.get_mut(&key).map(|e| {
            e.1 = f;
            e.0.clone()
        })
    }
    pub fn has(&self, key: u64) -> bool {
        self.map.contains_key(&key)
    }
    pub fn put(&mut self, rt: &ID2D1RenderTarget, key: u64, px: &Px) -> Option<ID2D1Bitmap> {
        let b = make_bitmap(rt, px)?;
        self.map.insert(key, (b.clone(), self.frame));
        Some(b)
    }
    pub fn remove(&mut self, key: u64) {
        self.map.remove(&key);
    }
    #[cfg(feature = "test-hook")]
    pub fn len(&self) -> usize {
        self.map.len()
    }
    /// Keeps at most `keep` bitmaps, never one drawn in this image.
    pub fn trim(&mut self, keep: usize) -> Vec<u64> {
        if self.map.len() <= keep {
            return vec![];
        }
        let mut v: Vec<(u64, u64)> = self.map.iter().map(|(k, e)| (*k, e.1)).collect();
        v.sort_by_key(|e| e.1);
        let n = self.map.len() - keep;
        let mut gone = vec![];
        for (k, f) in v.into_iter().take(n) {
            if f == self.frame {
                break;
            }
            self.map.remove(&k);
            gone.push(k);
        }
        gone
    }
}

/// How a picture is drawn: cropped part (`src`, in the bitmap's pixels), turned by quarter turns around its center.
pub fn picture(c: &mut Canvas, bmp: &ID2D1Bitmap, dest: Rect, src: Option<Rect>, turns: u32, opacity: f32, smooth: bool) {
    let rt = c.target();
    let cx = dest.x + dest.w / 2.0;
    let cy = dest.y + dest.h / 2.0;
    // the bitmap's own rectangle: turned a quarter, width and height swap
    let r = if turns % 2 == 1 { Rect { x: cx - dest.h / 2.0, y: cy - dest.w / 2.0, w: dest.h, h: dest.w } } else { dest };
    unsafe {
        if !turns.is_multiple_of(4) {
            let a = (turns % 4) as f32 * std::f32::consts::FRAC_PI_2;
            let (s, co) = a.sin_cos();
            rt.SetTransform(&Matrix3x2 { M11: co, M12: s, M21: -s, M22: co, M31: cx - cx * co + cy * s, M32: cy - cx * s - cy * co });
        }
        let srcr = src.map(rf);
        let dc = rt.cast::<ID2D1DeviceContext>().ok();
        match (&dc, smooth) {
            // shrinking: the high quality filter avoids the shimmer of thin lines
            (Some(dc), true) => {
                dc.DrawBitmap(bmp, Some(&rf(r)), opacity, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, srcr.as_ref().map(|s| s as *const _), None)
            }
            _ => rt.DrawBitmap(
                bmp,
                Some(&rf(r)),
                opacity,
                if smooth { D2D1_BITMAP_INTERPOLATION_MODE_LINEAR } else { D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR },
                srcr.as_ref().map(|s| s as *const _),
            ),
        }
        if !turns.is_multiple_of(4) {
            rt.SetTransform(&Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 });
        }
    }
}

/// A picture covering a square (or any) tile: the middle of the picture, cropped to the tile's proportions.
pub fn cover(c: &mut Canvas, bmp: &ID2D1Bitmap, dest: Rect, turns: u32) {
    let s = unsafe { bmp.GetPixelSize() };
    let (bw, bh) = (s.width as f32, s.height as f32);
    // proportions as shown (turned a quarter, the picture is seen sideways)
    let (dw, dh) = if turns % 2 == 1 { (dest.h, dest.w) } else { (dest.w, dest.h) };
    let k = (bw / dw).min(bh / dh);
    let (sw, sh) = (dw * k, dh * k);
    let src = Rect { x: (bw - sw) / 2.0, y: (bh - sh) / 2.0, w: sw, h: sh };
    picture(c, bmp, dest, Some(src), turns, 1.0, true);
}

pub fn circle(c: &mut Canvas, cx: f32, cy: f32, r: f32, color: Color) {
    let rt = c.target();
    unsafe {
        if let Ok(b) = rt.CreateSolidColorBrush(&col(color), None) {
            rt.FillEllipse(&D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: r, radiusY: r }, &b);
        }
    }
}

thread_local! {
    static FORMATS: RefCell<HashMap<(u32, u32, u8, bool), IDWriteTextFormat>> = RefCell::new(HashMap::new());
}

fn format(c: &Canvas, size: f32, weight: u32, align: Align, wrap: bool) -> Option<IDWriteTextFormat> {
    let key = ((size * 4.0) as u32, weight, align as u8, wrap);
    if let Some(f) = FORMATS.with(|m| m.borrow().get(&key).cloned()) {
        return Some(f);
    }
    unsafe {
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
                &HSTRING::new(),
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
            if let Ok(sign) = c.gfx.dw.CreateEllipsisTrimmingSign(&f) {
                let _ = f.SetTrimming(&DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 }, &sign);
            }
        }
        FORMATS.with(|m| m.borrow_mut().insert(key, f.clone()));
        Some(f)
    }
}

/// Text cut at words inside the width of `q`, from its top; gives the height it took.
pub fn text_wrap(c: &mut Canvas, s: &str, q: Rect, size: f32, weight: u32, color: Color, align: Align) -> f32 {
    wrapped(c, s, q, size, weight, align, Some(color))
}

/// The height a text cut at words inside `w` takes, without drawing it.
pub fn wrap_height(c: &mut Canvas, s: &str, w: f32, size: f32, weight: u32) -> f32 {
    wrapped(c, s, Rect { x: 0.0, y: 0.0, w, h: 10.0 }, size, weight, Align::Left, None)
}

fn wrapped(c: &mut Canvas, s: &str, q: Rect, size: f32, weight: u32, align: Align, color: Option<Color>) -> f32 {
    let Some(f) = format(c, size, weight, align, true) else { return 0.0 };
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let Ok(layout) = c.gfx.dw.CreateTextLayout(&w, &f, q.w.max(1.0), 100_000.0) else { return 0.0 };
        let _ = layout.SetTrimming(&DWRITE_TRIMMING::default(), None::<&IDWriteInlineObject>);
        let mut m = DWRITE_TEXT_METRICS::default();
        let _ = layout.GetMetrics(&mut m);
        if let Some(color) = color {
            let rt = c.target();
            if let Ok(b) = rt.CreateSolidColorBrush(&col(color), None) {
                rt.DrawTextLayout(Vector2 { X: q.x, Y: q.y }, &layout, &b, D2D1_DRAW_TEXT_OPTIONS(4));
            }
        }
        m.height
    }
}

/// Width of one line of text in any size.
pub fn text_width(c: &mut Canvas, s: &str, size: f32, weight: u32) -> f32 {
    let Some(f) = format(c, size, weight, Align::Left, false) else { return 0.0 };
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let Ok(layout) = c.gfx.dw.CreateTextLayout(&w, &f, 100_000.0, 100.0) else { return 0.0 };
        let mut m = DWRITE_TEXT_METRICS::default();
        let _ = layout.GetMetrics(&mut m);
        m.widthIncludingTrailingWhitespace
    }
}

/// Plume Media's icons (16 × 16, rounded strokes like plume-ui's).
pub mod ic {
    use plume_ui::icons::{Icon, Part::*};
    macro_rules! icon {
        ($id:ident, $($p:expr),+) => {
            pub const $id: Icon = Icon { name: concat!("media_", stringify!($id)), vb: 16.0, parts: &[$($p),+] };
        };
    }
    icon!(PHOTOS, R(1.5, 2.5, 13.0, 11.0, 2.5), P("M2.2 11.6 5.6 8.2l2.9 2.9 2-2 3.3 3.3"), C(10.6, 6.0, 1.2));
    icon!(VIDEO, R(1.5, 3.5, 9.0, 9.0, 2.2), P("M10.5 7.2 14.5 5v6l-4-2.2"));
    icon!(PLAY, P("M5 3.3v9.4c0 .7.7 1 1.2.7l7.2-4.7c.5-.3.5-1 0-1.4L6.2 2.6C5.7 2.3 5 2.6 5 3.3z"));
    icon!(PAUSE, R(3.8, 2.8, 3.0, 10.4, 1.0), R(9.2, 2.8, 3.0, 10.4, 1.0));
    icon!(VOLUME, P("M2.5 6.2h2.4L8.4 3.2v9.6L4.9 9.8H2.5z"), P("M11 5.6a3.4 3.4 0 0 1 0 4.8"), P("M12.9 3.7a6 6 0 0 1 0 8.6"));
    icon!(MUTED, P("M2.5 6.2h2.4L8.4 3.2v9.6L4.9 9.8H2.5z"), P("m11 6.2 3.6 3.6m0-3.6L11 9.8"));
    icon!(ROT_L, P("M3 8a5 5 0 1 0 1.5-3.6M3 2.5v3h3"));
    icon!(ROT_R, P("M13 8a5 5 0 1 1-1.5-3.6M13 2.5v3h-3"));
    icon!(ZOOM_IN, C(7.0, 7.0, 4.5), P("m10.5 10.5 3 3M5 7h4M7 5v4"));
    icon!(ZOOM_OUT, C(7.0, 7.0, 4.5), P("m10.5 10.5 3 3M5 7h4"));
    icon!(INFO, C(8.0, 8.0, 6.2), P("M8 7.4v3.8M8 4.8v.1"));
    icon!(FULL, P("M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10"));
    icon!(UNFULL, P("M6 2.5V6H2.5M13.5 6H10V2.5M10 13.5V10h3.5M2.5 10H6v3.5"));
    icon!(SLIDES, R(1.5, 3.0, 13.0, 10.0, 2.5), P("M6.6 6.1v3.8L9.9 8z"));
    icon!(CC, R(1.5, 3.0, 13.0, 10.0, 2.5), P("M7.1 6.7a1.7 1.7 0 1 0 0 2.6M11.3 6.7a1.7 1.7 0 1 0 0 2.6"));
    icon!(AUDIO, P("M2.5 11V8.6a5.5 5.5 0 0 1 11 0V11"), R(2.0, 9.4, 3.0, 4.2, 1.2), R(11.0, 9.4, 3.0, 4.2, 1.2));
    icon!(EMPTY, R(2.0, 3.0, 12.0, 10.0, 2.5), P("M2.6 11.2 5.8 8l2.8 2.8 1.8-1.8 3 3"), C(10.6, 6.2, 1.1));
}
