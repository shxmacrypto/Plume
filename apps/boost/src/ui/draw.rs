//! The logo (PNG bytes to a picture) and Plume Boost's own icons, 16 × 16 with round strokes like Plume's.

use plume_ui::icons::Icon;
use windows::{
    Win32::Graphics::Imaging::{
        GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapCacheOnLoad, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
        WICDecodeMetadataCacheOnDemand,
    },
    core::Interface,
};

/// A PNG (the app's logo) as a picture source.
pub fn png(wic: &IWICImagingFactory, bytes: &[u8]) -> Option<IWICBitmapSource> {
    unsafe {
        let stream = wic.CreateStream().ok()?;
        stream.InitializeFromMemory(bytes).ok()?;
        let dec = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let bmp = wic.CreateBitmapFromSource(&conv, WICBitmapCacheOnLoad).ok()?;
        bmp.cast().ok()
    }
}

pub mod ic {
    use Part::{C, P, R};
    use plume_ui::icons::{Icon, Part};

    macro_rules! icons {
        ($($id:ident = [$($part:expr),+ $(,)?];)+) => {
            $(pub const $id: Icon = Icon { name: concat!("boost_", stringify!($id)), vb: 16.0, parts: &[$($part),+] };)+
        };
    }

    icons! {
        GAUGE = [P("M2.5 11.5a5.5 5.5 0 1 1 11 0"), P("M8 11.5l2.6-3.4"), C(8.0, 11.5, 0.9)];
        SLIDERS = [P("M3 4.5h10M3 8h10M3 11.5h10"), C(6.0, 4.5, 1.5), C(10.5, 8.0, 1.5), C(5.0, 11.5, 1.5)];
        CHIP = [R(4.0, 4.0, 8.0, 8.0, 1.5), R(6.3, 6.3, 3.4, 3.4, 0.6), P("M6 1.8V4M10 1.8V4M6 12v2.2M10 12v2.2M1.8 6H4M1.8 10H4M12 6h2.2M12 10h2.2")];
        HISTORY = [P("M2.8 8a5.2 5.2 0 1 0 1.6-3.8"), P("M2.5 2.5v2.6h2.6"), P("M8 5.2V8l2 1.4")];
        BOLT = [P("M9 1.8 3.8 9h4l-1 5.2L12.2 7h-4z")];
        SHIELD = [P("M8 1.8 13 3.6v4c0 3.1-2.1 5.4-5 6.6-2.9-1.2-5-3.5-5-6.6v-4z")];
        RESTART = [P("M13 8a5 5 0 1 1-1.5-3.6"), P("M13 2.5v3h-3")];
        SIGNOUT = [P("M6 2.8H3.5v10.4H6"), P("M9.5 5 12.5 8l-3 3M12.5 8H6")];
        GAMEPAD = [P("M5 5h6a3.2 3.2 0 0 1 3.1 3.9l-.5 2.1a1.6 1.6 0 0 1-2.7.7L9.6 10.4H6.4L5.1 11.7a1.6 1.6 0 0 1-2.7-.7l-.5-2.1A3.2 3.2 0 0 1 5 5z"), P("M5.2 7v2.2M4.1 8.1h2.2"), P("M10.8 7.6h.01M11.8 8.8h.01")];
        WARN = [P("M8 2.2 14.3 13H1.7z"), P("M8 6.3v3M8 11.2h.01")];
        OK = [C(8.0, 8.0, 6.0), P("m5.3 8.2 1.9 1.9 3.6-3.8")];
        TICK = [P("m4 8.3 2.6 2.6L12 5.4")];
        CHEVRON = [P("m5.5 6.5 2.5 2.5 2.5-2.5")];
        CHEVRON_UP = [P("m5.5 9.5 2.5-2.5 2.5 2.5")];
        FLASK = [P("M6.2 2h3.6M6.8 2v4.2L3 12.6A1 1 0 0 0 3.9 14h8.2a1 1 0 0 0 .9-1.4L9.2 6.2V2"), P("M4.6 10h6.8")];
        BATTERY = [R(1.8, 4.8, 11.0, 6.4, 1.4), P("M14.4 7v2"), P("M4 7v2M6.2 7v2")];
        GPU = [R(1.5, 4.0, 13.0, 8.0, 1.5), C(10.0, 8.0, 2.0), P("M4 7h2.5M4 9h2.5M3.5 12v2")];
    }

    #[allow(dead_code)]
    const _ALL: [&Icon; 1] = [&TICK];
}

#[allow(dead_code)]
const _UNUSED: Option<&Icon> = None;
