//! Ressources graphiques du processus : fabriques Direct2D / DirectWrite / WIC, polices, formats de texte.
//! Elles ne dépendent d'aucune fenêtre (les brosses et images, elles, appartiennent à un `Canvas`).

use std::{cell::RefCell, collections::HashMap};
use windows::{
    Win32::{
        Graphics::{
            Direct2D::{
                D2D1_CAP_STYLE_ROUND, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_LINE_JOIN_ROUND, D2D1_STROKE_STYLE_PROPERTIES, D2D1CreateFactory, ID2D1Factory,
                ID2D1PathGeometry, ID2D1StrokeStyle,
            },
            DirectWrite::{
                DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory,
                IDWriteFontCollection, IDWriteTextFormat,
            },
            Imaging::{CLSID_WICImagingFactory, IWICImagingFactory},
        },
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
    },
    core::{BOOL, PCWSTR, Result, w},
};

/// Les styles de texte de l'interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Font {
    /// Texte courant, 13 px.
    Body,
    /// Texte courant en demi-gras (onglet actif, titres de lignes).
    BodyBold,
    /// Texte secondaire, 12 px (en-têtes de colonnes, barre d'état).
    Small,
    SmallBold,
    /// Titres, 15 px demi-gras.
    Title,
    /// Grand titre d'une page, 17 px demi-gras.
    Large,
    /// Titre principal d'une fenêtre, 24 px gras.
    Hero,
    /// Symboles de fenêtre (Segoe Fluent Icons), 10 px.
    Glyph,
}

impl Font {
    /// (police de symboles ?, taille en DIP, graisse)
    fn spec(self) -> (bool, f32, i32) {
        match self {
            Font::Body => (false, 13.0, 400),
            Font::BodyBold => (false, 13.0, 600),
            Font::Small => (false, 12.0, 400),
            Font::SmallBold => (false, 12.0, 600),
            Font::Title => (false, 15.0, 600),
            Font::Large => (false, 17.0, 600),
            Font::Hero => (false, 24.0, 700),
            Font::Glyph => (true, 10.0, 400),
        }
    }
    pub fn size(self) -> f32 {
        self.spec().1
    }
}

pub struct Gfx {
    pub d2d: ID2D1Factory,
    pub dw: IDWriteFactory,
    pub wic: IWICImagingFactory,
    /// Traits à bouts et angles arrondis, comme les icônes de Plume.
    pub round: ID2D1StrokeStyle,
    /// « Segoe UI Variable Text » (Windows 11), sinon « Segoe UI ».
    pub ui_font: &'static str,
    /// « Segoe Fluent Icons » (Windows 11), sinon « Segoe MDL2 Assets ».
    pub glyph_font: &'static str,
    formats: RefCell<HashMap<Font, IDWriteTextFormat>>,
    pub(super) icons: RefCell<HashMap<&'static str, ID2D1PathGeometry>>,
}

fn has_font(fonts: &IDWriteFontCollection, family: PCWSTR) -> bool {
    let (mut index, mut exists) = (0u32, BOOL(0));
    unsafe { fonts.FindFamilyName(family, &mut index, &mut exists).is_ok() && exists.as_bool() }
}

impl Gfx {
    /// À appeler sur le fil de l'interface, COM initialisé.
    pub fn new() -> Result<Gfx> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let round = d2d.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    dashCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    miterLimit: 10.0,
                    ..Default::default()
                },
                None,
            )?;
            let mut fonts = None;
            dw.GetSystemFontCollection(&mut fonts, false)?;
            let fonts = fonts.expect("polices du système");
            let ui_font = if has_font(&fonts, w!("Segoe UI Variable Text")) { "Segoe UI Variable Text" } else { "Segoe UI" };
            let glyph_font = if has_font(&fonts, w!("Segoe Fluent Icons")) { "Segoe Fluent Icons" } else { "Segoe MDL2 Assets" };
            Ok(Gfx { d2d, dw, wic, round, ui_font, glyph_font, formats: Default::default(), icons: Default::default() })
        }
    }

    /// Format DirectWrite d'un style : une ligne, centrée en hauteur, coupée par « … » si elle dépasse.
    pub fn format(&self, font: Font) -> IDWriteTextFormat {
        if let Some(f) = self.formats.borrow().get(&font) {
            return f.clone();
        }
        let (glyph, size, weight) = font.spec();
        let family: Vec<u16> = (if glyph { self.glyph_font } else { self.ui_font }).encode_utf16().chain(Some(0)).collect();
        let f = unsafe {
            let f = self
                .dw
                .CreateTextFormat(PCWSTR(family.as_ptr()), None, DWRITE_FONT_WEIGHT(weight), DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!(""))
                .expect("format de texte");
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            if !glyph && let Ok(sign) = self.dw.CreateEllipsisTrimmingSign(&f) {
                let _ = f.SetTrimming(&DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 }, &sign);
            }
            f
        };
        self.formats.borrow_mut().insert(font, f.clone());
        f
    }
}
