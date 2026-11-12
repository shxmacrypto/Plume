//! Drawing beyond plume-ui's basics: bitmaps from pixels (the canvas, thumbnails), turned pictures, lines and
//! outlines (handles, guides, selections), dashed lines, a checkerboard under transparent pixels.

use plume_ui::{Canvas, Color, Rect};
use windows::{
    Win32::Graphics::Direct2D::{
        Common::{
            D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN,
            D2D1_PIXEL_FORMAT,
        },
        D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR, D2D1_BITMAP_PROPERTIES, D2D1_CAP_STYLE_FLAT,
        D2D1_DASH_STYLE_CUSTOM, D2D1_ELLIPSE, D2D1_LINE_JOIN_MITER, D2D1_STROKE_STYLE_PROPERTIES, ID2D1Bitmap, ID2D1RenderTarget, ID2D1StrokeStyle,
    },
    Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
    core::Interface,
};
use windows_numerics::{Matrix3x2, Vector2};

fn col(c: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.r, g: c.g, b: c.b, a: c.a }
}
pub fn rf(q: Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: q.x, top: q.y, right: q.x + q.w, bottom: q.y + q.h }
}
const ID: Matrix3x2 = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 };

/// Identity of the Direct2D surface: bitmaps belong to it and must be made again when it is replaced.
pub fn target_id(rt: &ID2D1RenderTarget) -> usize {
    rt.as_raw() as usize
}

/// A bitmap from BGRA premultiplied pixels.
pub fn bitmap(rt: &ID2D1RenderTarget, w: u32, h: u32, bgra: &[u8]) -> Option<ID2D1Bitmap> {
    if w == 0 || h == 0 {
        return None;
    }
    let props = D2D1_BITMAP_PROPERTIES {
        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
        dpiX: 96.0,
        dpiY: 96.0,
    };
    unsafe { rt.CreateBitmap(D2D_SIZE_U { width: w, height: h }, Some(bgra.as_ptr() as *const _), w * 4, &props).ok() }
}

/// Draws a bitmap into `dest`, turned by `deg` degrees around the point (`px`, `py`).
pub fn bitmap_at(c: &mut Canvas, bmp: &ID2D1Bitmap, dest: Rect, opacity: f32, smooth: bool, turn: Option<(f32, f32, f32)>) {
    let rt = c.target();
    unsafe {
        if let Some((deg, px, py)) = turn {
            let (s, co) = deg.to_radians().sin_cos();
            rt.SetTransform(&Matrix3x2 { M11: co, M12: s, M21: -s, M22: co, M31: px - px * co + py * s, M32: py - px * s - py * co });
        }
        rt.DrawBitmap(
            bmp,
            Some(&rf(dest)),
            opacity,
            if smooth { D2D1_BITMAP_INTERPOLATION_MODE_LINEAR } else { D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR },
            None,
        );
        if turn.is_some() {
            rt.SetTransform(&ID);
        }
    }
}

fn brush(rt: &ID2D1RenderTarget, color: Color) -> Option<windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush> {
    unsafe { rt.CreateSolidColorBrush(&col(color), None).ok() }
}

pub fn line(c: &mut Canvas, x0: f32, y0: f32, x1: f32, y1: f32, color: Color, width: f32) {
    let rt = c.target();
    if let Some(b) = brush(&rt, color) {
        unsafe { rt.DrawLine(Vector2 { X: x0, Y: y0 }, Vector2 { X: x1, Y: y1 }, &b, width, None) };
    }
}

fn dash_style(c: &mut Canvas, phase: f32) -> Option<ID2D1StrokeStyle> {
    let props = D2D1_STROKE_STYLE_PROPERTIES {
        startCap: D2D1_CAP_STYLE_FLAT,
        endCap: D2D1_CAP_STYLE_FLAT,
        dashCap: D2D1_CAP_STYLE_FLAT,
        lineJoin: D2D1_LINE_JOIN_MITER,
        miterLimit: 10.0,
        dashStyle: D2D1_DASH_STYLE_CUSTOM,
        dashOffset: phase,
    };
    unsafe { c.gfx.d2d.CreateStrokeStyle(&props, Some(&[4.0, 4.0])).ok() }
}

/// A closed or open outline through points; `dashes`: black and white "marching ants" moved by `phase`.
pub fn poly(c: &mut Canvas, pts: &[(f32, f32)], closed: bool, color: Color, width: f32, dashes: Option<f32>) {
    if pts.len() < 2 {
        return;
    }
    let rt = c.target();
    unsafe {
        let Ok(geo) = c.gfx.d2d.CreatePathGeometry() else { return };
        let Ok(sink) = geo.Open() else { return };
        sink.BeginFigure(Vector2 { X: pts[0].0, Y: pts[0].1 }, D2D1_FIGURE_BEGIN_FILLED);
        for p in &pts[1..] {
            sink.AddLine(Vector2 { X: p.0, Y: p.1 });
        }
        sink.EndFigure(if closed { D2D1_FIGURE_END_CLOSED } else { D2D1_FIGURE_END_OPEN });
        let _ = sink.Close();
        match dashes {
            Some(phase) => {
                if let Some(b) = brush(&rt, Color::hex(0xffffff)) {
                    rt.DrawGeometry(&geo, &b, width, None);
                }
                let style = dash_style(c, phase);
                if let Some(b) = brush(&rt, Color::hex(0x000000)) {
                    rt.DrawGeometry(&geo, &b, width, style.as_ref());
                }
            }
            None => {
                if let Some(b) = brush(&rt, color) {
                    rt.DrawGeometry(&geo, &b, width, None);
                }
            }
        }
    }
}

/// A filled polygon (crop shade, rotated boxes).
pub fn fill_poly(c: &mut Canvas, pts: &[(f32, f32)], color: Color) {
    if pts.len() < 3 {
        return;
    }
    let rt = c.target();
    unsafe {
        let Ok(geo) = c.gfx.d2d.CreatePathGeometry() else { return };
        let Ok(sink) = geo.Open() else { return };
        sink.BeginFigure(Vector2 { X: pts[0].0, Y: pts[0].1 }, D2D1_FIGURE_BEGIN_FILLED);
        for p in &pts[1..] {
            sink.AddLine(Vector2 { X: p.0, Y: p.1 });
        }
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        let _ = sink.Close();
        if let Some(b) = brush(&rt, color) {
            rt.FillGeometry(&geo, &b, None);
        }
    }
}

pub fn circle(c: &mut Canvas, x: f32, y: f32, r: f32, fill: Option<Color>, stroke: Option<(Color, f32)>) {
    let rt = c.target();
    let e = D2D1_ELLIPSE { point: Vector2 { X: x, Y: y }, radiusX: r, radiusY: r };
    unsafe {
        if let Some(f) = fill
            && let Some(b) = brush(&rt, f)
        {
            rt.FillEllipse(&e, &b);
        }
        if let Some((s, w)) = stroke
            && let Some(b) = brush(&rt, s)
        {
            rt.DrawEllipse(&e, &b, w, None);
        }
    }
}

/// A grey and white checkerboard: where the picture is transparent.
pub fn checker(c: &mut Canvas, r: Rect, size: f32, dark: bool) {
    let (a, b) = if dark { (Color::hex(0x3a3a3c), Color::hex(0x2c2c2e)) } else { (Color::hex(0xffffff), Color::hex(0xe5e5ea)) };
    c.fill(r, a);
    c.clip(r);
    let cols = (r.w / size).ceil() as i32 + 1;
    let rows = (r.h / size).ceil() as i32 + 1;
    // at most a few thousand squares: big cells when the canvas is large
    let step = if cols * rows > 6000 { (((cols * rows) as f32 / 6000.0).sqrt().ceil()) as i32 } else { 1 };
    let s = size * step as f32;
    let mut y = 0;
    while (y as f32) * s < r.h {
        let mut x = y % 2;
        while (x as f32) * s < r.w {
            c.fill(Rect { x: r.x + x as f32 * s, y: r.y + y as f32 * s, w: s, h: s }, b);
            x += 2;
        }
        y += 1;
    }
    c.unclip();
}
