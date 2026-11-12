//! Text through DirectWrite: the fonts installed on Windows, letters drawn as coverage (Direct2D on a WIC bitmap,
//! grey anti-aliasing), wrapped in the layer's width, with spacing and line height.

use crate::engine::{
    doc::{TextAlign, TextL},
    render::TextRaster,
    textfx::Glyphs,
};
use windows::{
    Win32::{
        Graphics::{
            Direct2D::{
                Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT},
                D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_RENDER_TARGET_PROPERTIES, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
                D2D1CreateFactory, ID2D1Factory,
            },
            DirectWrite::{
                DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
                DWRITE_LINE_SPACING_METHOD_UNIFORM, DWRITE_OVERHANG_METRICS, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
                DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory, IDWriteFactory,
                IDWriteTextLayout, IDWriteTextLayout1,
            },
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Imaging::{CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapCacheOnLoad},
        },
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
    },
    core::{BOOL, Interface, PCWSTR, w},
};
use windows_numerics::Vector2;

pub struct DwText {
    dw: IDWriteFactory,
    d2d: ID2D1Factory,
    wic: IWICImagingFactory,
}

impl DwText {
    pub fn new() -> Option<DwText> {
        unsafe {
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
            Some(DwText { dw, d2d, wic })
        }
    }

    fn layout(&self, t: &TextL, width: f32, scale: f32) -> Option<IDWriteTextLayout> {
        let family: Vec<u16> = t.font.encode_utf16().chain(Some(0)).collect();
        let text: Vec<u16> = t.text.encode_utf16().collect();
        let size = (t.size * scale).max(0.5);
        unsafe {
            let fmt = self
                .dw
                .CreateTextFormat(
                    PCWSTR(family.as_ptr()),
                    None,
                    DWRITE_FONT_WEIGHT(t.weight.clamp(100, 950) as i32),
                    if t.italic { DWRITE_FONT_STYLE_ITALIC } else { DWRITE_FONT_STYLE_NORMAL },
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("en-us"),
                )
                .ok()?;
            let _ = fmt.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
            let _ = fmt.SetTextAlignment(match t.align {
                TextAlign::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                TextAlign::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                TextAlign::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
            });
            let layout = self.dw.CreateTextLayout(&text, &fmt, width.max(1.0), 1.0e6).ok()?;
            let ls = size * t.line.max(0.5);
            let _ = layout.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, ls, ls * 0.8);
            if t.spacing != 0.0
                && let Ok(l1) = layout.cast::<IDWriteTextLayout1>()
            {
                let _ = l1.SetCharacterSpacing(0.0, t.spacing * scale, 0.0, DWRITE_TEXT_RANGE { startPosition: 0, length: text.len() as u32 });
            }
            Some(layout)
        }
    }

    /// The installed font families, sorted.
    pub fn families(&self) -> Vec<String> {
        let mut out = vec![];
        unsafe {
            let mut coll = None;
            if self.dw.GetSystemFontCollection(&mut coll, false).is_err() {
                return out;
            }
            let Some(coll) = coll else { return out };
            for i in 0..coll.GetFontFamilyCount() {
                let Ok(fam) = coll.GetFontFamily(i) else { continue };
                let Ok(names) = fam.GetFamilyNames() else { continue };
                let (mut idx, mut exists) = (0u32, BOOL(0));
                let _ = names.FindLocaleName(w!("en-us"), &mut idx, &mut exists);
                if !exists.as_bool() {
                    idx = 0;
                }
                let Ok(len) = names.GetStringLength(idx) else { continue };
                let mut buf = vec![0u16; len as usize + 1];
                if names.GetString(idx, &mut buf).is_ok() {
                    let name = String::from_utf16_lossy(&buf[..len as usize]);
                    // fonts for symbols only and vertical variants are of no use for titles
                    if !name.starts_with('@') {
                        out.push(name);
                    }
                }
            }
        }
        out.sort_by_key(|a| a.to_lowercase());
        out.dedup();
        out
    }
}

impl TextRaster for DwText {
    fn glyphs(&self, t: &TextL, width: f32, scale: f32, pad: u32) -> Option<Glyphs> {
        if t.text.trim().is_empty() {
            return None;
        }
        let layout = self.layout(t, width, scale)?;
        unsafe {
            let mut m = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut m).ok()?;
            let o = layout.GetOverhangMetrics().unwrap_or(DWRITE_OVERHANG_METRICS::default());
            // room for letters reaching past the box (tall capitals with a tight line height, italics)
            let over = o.left.max(o.top).max(o.right).max(o.bottom).max(0.0).ceil() as u32;
            let pad = pad + over + 2;
            let gw = (width.ceil() as u32 + 2 * pad).clamp(1, 16384);
            let gh = (m.height.ceil().max(1.0) as u32 + 2 * pad).clamp(1, 16384);
            let bmp = self.wic.CreateBitmap(gw, gh, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad).ok()?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            };
            let rt = self.d2d.CreateWicBitmapRenderTarget(&bmp, &props).ok()?;
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = rt.CreateSolidColorBrush(&D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }, None).ok()?;
            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            rt.DrawTextLayout(Vector2 { X: pad as f32, Y: pad as f32 }, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
            rt.EndDraw(None, None).ok()?;
            let mut data = vec![0u8; gw as usize * gh as usize * 4];
            bmp.CopyPixels(std::ptr::null(), gw * 4, &mut data).ok()?;
            let a = data.as_chunks::<4>().0.iter().map(|p| p[3] as f32 / 255.0).collect();
            Some(Glyphs { w: gw, h: gh, a, pad })
        }
    }

    fn measure(&self, t: &TextL, width: f32) -> f32 {
        let Some(layout) = self.layout(t, width, 1.0) else { return t.size * t.line };
        let mut m = DWRITE_TEXT_METRICS::default();
        unsafe {
            if layout.GetMetrics(&mut m).is_err() {
                return t.size * t.line;
            }
        }
        m.height.max(t.size * t.line * 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{
        doc::{Doc, Kind, Layer, Rgba, Xf},
        render::Renderer,
    };

    #[test]
    fn letters_measure_and_draw() {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        let dw = DwText::new().unwrap();
        assert!(dw.families().iter().any(|f| f == "Segoe UI" || f == "Arial"));
        let t = TextL { text: "Hello\nworld".into(), size: 40.0, line: 1.25, color: Rgba::BLACK, ..TextL::default() };
        let h = dw.measure(&t, 400.0);
        assert!((h - 100.0).abs() < 2.0, "{h}");
        // wrapping in a narrow box makes it taller
        let long = TextL { text: "one two three four five six".into(), ..t.clone() };
        assert!(dw.measure(&long, 120.0) > dw.measure(&long, 2000.0) * 2.0);
        // drawn: ink in the box, none far away
        let mut d = Doc::new(400, 200);
        d.layers.push(Layer::new(1, "t", Xf::new(200.0, 100.0, 400.0, h), Kind::Text(TextL { text: "MMMM".into(), size: 80.0, ..t })));
        let img = Renderer::new(Box::new(dw)).image(&d, 1.0);
        let ink = (0..img.h).flat_map(|y| (0..120).map(move |x| (x, y))).filter(|(x, y)| img.get(*x, *y)[3] > 200).count();
        assert!(ink > 500, "{ink}");
        assert_eq!(img.get(390, 190)[3], 0);
    }
}
