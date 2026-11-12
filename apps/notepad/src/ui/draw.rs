//! Drawing helpers shared by the main window and the pads: the logo, Plume Notepad's icons, pill buttons, switches
//! and segmented choices.

use plume_ui::{Align, Canvas, Color, Font, Rect, Theme, icons::Icon, icons::Part, rect};
use windows::{
    Win32::{
        Graphics::Imaging::{
            CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapCacheOnLoad, WICBitmapDitherTypeNone,
            WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnLoad,
        },
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IStream},
    },
    core::Interface,
};

pub const LOGO: &[u8] = include_bytes!("../../../../assets/brand/plume-notepad-256.png");
pub const LOGO_KEY: u64 = 1 << 61;

/// A PNG as a bitmap Direct2D can draw.
pub fn drawable(png: &[u8]) -> Option<IWICBitmapSource> {
    unsafe {
        let f: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let stream = f.CreateStream().ok()?;
        stream.InitializeFromMemory(png).ok()?;
        let s: IStream = stream.cast().ok()?;
        let dec = f.CreateDecoderFromStream(&s, std::ptr::null(), WICDecodeMetadataCacheOnLoad).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let conv = f.CreateFormatConverter().ok()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let bmp = f.CreateBitmapFromSource(&conv, WICBitmapCacheOnLoad).ok()?;
        bmp.cast().ok()
    }
}

pub fn logo(c: &mut Canvas, r: Rect) {
    c.image(LOGO_KEY, r, 1.0, || drawable(LOGO));
}

/// The accent as text or a small icon: a touch lighter on dark backgrounds.
pub fn accent_ink(t: &Theme) -> Color {
    if t.dark { t.accent.mix(t.t1, 0.25) } else { t.accent }
}

/// Something chosen: the accent at 16 %.
pub fn chosen(t: &Theme) -> Color {
    t.accent.fade(0.16)
}

/// A pill button: 0 secondary, 1 primary (accent), 2 red. `hot`: under the mouse; `down`: pressed.
#[allow(clippy::too_many_arguments)]
pub fn pill(c: &mut Canvas, t: &Theme, r: Rect, label: &str, kind: u8, hot: bool, down: bool, font: Font) {
    let (fill, ink) = match kind {
        1 => {
            let base = if down {
                t.accent.mix(Color::hex(0x000000), 0.18)
            } else if hot {
                t.accent.mix(Color::hex(0xffffff), 0.1)
            } else {
                t.accent
            };
            (base, crate::model::on_accent(t))
        }
        2 => (
            t.red.fade(if down {
                0.32
            } else if hot {
                0.24
            } else {
                0.16
            }),
            t.red,
        ),
        _ if down => (t.fill2.over(t.bg).mix(t.t1, 0.06), t.t1),
        _ => (if hot { t.fill2 } else { t.fill }, t.t1),
    };
    c.fill_round(r, r.h / 2.0, fill);
    c.text(label, r, font, ink, Align::Center);
}

pub fn pill_w(c: &mut Canvas, label: &str) -> f32 {
    c.text_width(label, Font::BodyBold) + 34.0
}

/// An iOS-style switch at the right of `r`.
pub fn switch(c: &mut Canvas, t: &Theme, r: Rect, on: bool) {
    let sw = rect(r.right() - 46.0, r.y + (r.h - 26.0) / 2.0, 46.0, 26.0);
    c.fill_round(sw, 13.0, if on { t.accent } else { t.fill2 });
    let k = if on { sw.right() - 23.0 } else { sw.x + 3.0 };
    c.fill_round(rect(k, sw.y + 3.0, 20.0, 20.0), 10.0, Color::hex(0xffffff));
}

/// A segmented choice at the right of `r`: a pill track, the chosen segment raised. Returns the segments' rects.
pub fn segments(c: &mut Canvas, t: &Theme, r: Rect, labels: &[&str], cur: usize, hot: Option<usize>) -> Vec<Rect> {
    let widths: Vec<f32> = labels.iter().map(|l| c.text_width(l, Font::SmallBold) + 26.0).collect();
    let total: f32 = widths.iter().sum::<f32>() + 6.0;
    let bg = rect(r.right() - total, r.y + (r.h - 34.0) / 2.0, total, 34.0);
    c.fill_round(bg, 17.0, t.fill);
    let mut x = bg.x + 3.0;
    let mut out = Vec::new();
    for (i, (l, w)) in labels.iter().zip(widths).enumerate() {
        let b = rect(x, bg.y + 3.0, w, 28.0);
        if i == cur {
            c.fill_round(b, 14.0, t.fill2.over(t.bg));
            c.stroke_round(b.inset(0.5), 14.0, t.ring, 1.0);
        } else if hot == Some(i) {
            c.fill_round(b, 14.0, t.hover);
        }
        c.text(l, b, Font::SmallBold, if i == cur { t.t1 } else { t.t2 }, Align::Center);
        out.push(b);
        x += w;
    }
    out
}

/// The pill a toast or a tooltip sits in.
pub fn bubble(c: &mut Canvas, t: &Theme, r: Rect, text: &str, a: f32, font: Font) {
    c.fill_round(r, r.h / 2.0, t.solid2.fade(a));
    c.stroke_round(r.inset(0.5), r.h / 2.0, t.ring.fade(a), 1.0);
    c.text(text, r, font, t.t1.fade(a), Align::Center);
}

/// Plume Notepad's icons (16 × 16, like Plume's).
pub mod ic {
    use super::*;
    use Part::{C, P, R};

    macro_rules! icons {
        ($($id:ident = [$($part:expr),+ $(,)?];)+) => {
            $(pub const $id: Icon = Icon { name: concat!("np_", stringify!($id)), vb: 16.0, parts: &[$($part),+] };)+
        };
    }

    icons! {
        NEW = [P("M4.5 2.5h4.8l2.7 2.7v8.3H4.5z"), P("M9.3 2.5v2.7H12")];
        SAVE = [R(2.5, 2.5, 11.0, 11.0, 2.0), P("M5.2 2.5v3.2h5.3V2.5"), P("M5.2 13.5V9.6h5.6v3.9")];
        UNDO = [P("M3 7h6.5a3.5 3.5 0 0 1 0 7H7"), P("M5.5 4.5 3 7l2.5 2.5")];
        REDO = [P("M13 7H6.5a3.5 3.5 0 0 0 0 7H9"), P("M10.5 4.5 13 7l-2.5 2.5")];
        REPLACE = [P("M3 5.5h9.5l-2.2-2.2"), P("M13 10.5H3.5l2.2 2.2")];
        WRAP = [P("M2.5 3.5h11"), P("M2.5 7.5h9a2.2 2.2 0 0 1 0 4.4H8"), P("M9.6 10.3 8 11.9l1.6 1.6"), P("M2.5 11.9h3")];
        PAD = [P("M3.2 2.5h9.6a.7.7 0 0 1 .7.7v6.3l-4 4H3.2a.7.7 0 0 1-.7-.7V3.2a.7.7 0 0 1 .7-.7z"), P("M13.5 9.5H10a.5.5 0 0 0-.5.5v3.5")];
        PALETTE = [P("M8 2.5a5.5 5.5 0 1 0 0 11c1 0 1.3-.8.9-1.5-.5-.9 0-1.9 1-1.9h1.3A2.3 2.3 0 0 0 13.5 7.8 5.5 5.5 0 0 0 8 2.5z"), C(5.3, 7.2, 0.5), C(7.6, 5.0, 0.5), C(10.4, 5.8, 0.5)];
        ARROW_UP = [P("M8 13V3.5M4 7.5 8 3.5l4 4")];
        ARROW_DOWN = [P("M8 3v9.5M4 8.5l4 4 4-4")];
    }
}
