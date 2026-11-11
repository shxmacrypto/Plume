//! La surface de dessin d'une fenêtre : Direct2D pour les formes, DirectWrite pour le texte.
//! Tout est en DIP ; les filets d'un pixel sont alignés sur les pixels réels pour rester nets à 125 % ou 150 %.

use super::{
    geom::{Color, Rect},
    gfx::{Font, Gfx},
    icons::Icon,
};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    rc::Rc,
};
use windows::{
    Win32::{
        Foundation::{HWND, RECT},
        Graphics::{
            Direct2D::{
                Common::{D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT},
                D2D1_ANTIALIAS_MODE_ALIASED, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_DRAW_TEXT_OPTIONS,
                D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE,
                D2D1_RENDER_TARGET_PROPERTIES, D2D1_ROUNDED_RECT, ID2D1Bitmap, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
            },
            DirectWrite::{
                DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
                DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_NONE, DWRITE_WORD_WRAPPING_WRAP,
                IDWriteInlineObject, IDWriteTextLayout,
            },
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Imaging::IWICBitmapSource,
        },
        UI::WindowsAndMessaging::GetClientRect,
    },
    core::Interface,
};
use windows_numerics::{Matrix3x2, Vector2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Couper le texte au rectangle + polices en couleur (émojis dans les noms de fichiers).
const TEXT_OPTIONS: D2D1_DRAW_TEXT_OPTIONS = D2D1_DRAW_TEXT_OPTIONS(2 | 4);
const IDENTITY: Matrix3x2 = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 };

fn c(col: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: col.r, g: col.g, b: col.b, a: col.a }
}
fn r(q: Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: q.x, top: q.y, right: q.x + q.w, bottom: q.y + q.h }
}

pub struct Canvas {
    pub gfx: Rc<Gfx>,
    hwnd: HWND,
    rt: Option<ID2D1HwndRenderTarget>,
    brush: Option<ID2D1SolidColorBrush>,
    dpi: f32,
    wbuf: Vec<u16>,
    /// Images propres à cette surface (icônes de fichiers…), rangées par clé de l'appelant.
    bitmaps: HashMap<u64, Option<ID2D1Bitmap>>,
    widths: HashMap<(Font, u64), f32>,
}

impl Canvas {
    pub fn new(gfx: Rc<Gfx>, hwnd: HWND, dpi: u32) -> Canvas {
        Canvas { gfx, hwnd, rt: None, brush: None, dpi: dpi as f32, wbuf: Vec::with_capacity(260), bitmaps: HashMap::new(), widths: HashMap::new() }
    }

    fn pixel_size(&self) -> D2D_SIZE_U {
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rc);
        }
        D2D_SIZE_U { width: (rc.right - rc.left).max(1) as u32, height: (rc.bottom - rc.top).max(1) as u32 }
    }

    fn rt(&mut self) -> &ID2D1HwndRenderTarget {
        if self.rt.is_none() {
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                // fenêtre opaque : le texte peut utiliser ClearType
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
                dpiX: self.dpi,
                dpiY: self.dpi,
                ..Default::default()
            };
            let hw = D2D1_HWND_RENDER_TARGET_PROPERTIES { hwnd: self.hwnd, pixelSize: self.pixel_size(), presentOptions: D2D1_PRESENT_OPTIONS_NONE };
            let rt = unsafe { self.gfx.d2d.CreateHwndRenderTarget(&props, &hw) }.expect("surface Direct2D");
            self.brush = unsafe { rt.CreateSolidColorBrush(&c(Color::hex(0)), None) }.ok();
            self.rt = Some(rt);
        }
        self.rt.as_ref().unwrap()
    }

    /// La fenêtre a changé de taille.
    pub fn resize(&mut self) {
        let size = self.pixel_size();
        if let Some(rt) = &self.rt {
            unsafe {
                let _ = rt.Resize(&size);
            }
        }
    }

    pub fn set_dpi(&mut self, dpi: u32) {
        self.dpi = dpi as f32;
        if let Some(rt) = &self.rt {
            unsafe { rt.SetDpi(self.dpi, self.dpi) };
        }
    }

    /// Pixels réels par DIP (1.0 à 100 %, 1.5 à 150 %).
    pub fn scale(&self) -> f32 {
        self.dpi / 96.0
    }

    /// Taille de la fenêtre en DIP.
    pub fn size(&self) -> (f32, f32) {
        let s = self.pixel_size();
        (s.width as f32 / self.scale(), s.height as f32 / self.scale())
    }

    /// Aligne une coordonnée sur un pixel réel.
    pub fn snap(&self, v: f32) -> f32 {
        (v * self.scale()).round() / self.scale()
    }

    pub fn begin(&mut self) {
        unsafe { self.rt().BeginDraw() };
    }

    /// Termine l'image. Si la carte graphique a perdu la surface, elle sera recréée à la prochaine image.
    pub fn end(&mut self) {
        let lost = self.rt.as_ref().is_some_and(|rt| unsafe { rt.EndDraw(None, None) }.is_err());
        if lost {
            self.rt = None;
            self.brush = None;
            self.bitmaps.clear();
        }
    }

    fn brush(&mut self, col: Color) -> ID2D1SolidColorBrush {
        self.rt();
        let b = self.brush.clone().expect("brosse");
        unsafe { b.SetColor(&c(col)) };
        b
    }

    pub fn clear(&mut self, col: Color) {
        unsafe { self.rt().Clear(Some(&c(col))) };
    }

    pub fn fill(&mut self, q: Rect, col: Color) {
        if q.is_empty() || col.a <= 0.0 {
            return;
        }
        let b = self.brush(col);
        unsafe { self.rt().FillRectangle(&r(q), &b) };
    }

    pub fn fill_round(&mut self, q: Rect, radius: f32, col: Color) {
        if q.is_empty() || col.a <= 0.0 {
            return;
        }
        let b = self.brush(col);
        let rad = radius.min(q.w / 2.0).min(q.h / 2.0);
        unsafe { self.rt().FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: r(q), radiusX: rad, radiusY: rad }, &b) };
    }

    /// Contour arrondi tracé à l'intérieur du rectangle (liseré des surfaces de Plume).
    pub fn stroke_round(&mut self, q: Rect, radius: f32, col: Color, width: f32) {
        if q.is_empty() || col.a <= 0.0 {
            return;
        }
        let b = self.brush(col);
        let q = q.inset(width / 2.0);
        let rad = (radius - width / 2.0).max(0.0).min(q.w / 2.0).min(q.h / 2.0);
        unsafe { self.rt().DrawRoundedRectangle(&D2D1_ROUNDED_RECT { rect: r(q), radiusX: rad, radiusY: rad }, &b, width, None) };
    }

    /// Filet horizontal d'un pixel réel.
    pub fn hline(&mut self, x: f32, y: f32, w: f32, col: Color) {
        let px = 1.0 / self.scale();
        let y = self.snap(y);
        self.fill_aliased(Rect { x, y, w, h: px }, col);
    }

    /// Filet vertical d'un pixel réel.
    pub fn vline(&mut self, x: f32, y: f32, h: f32, col: Color) {
        let px = 1.0 / self.scale();
        let x = self.snap(x);
        self.fill_aliased(Rect { x, y, w: px, h }, col);
    }

    fn fill_aliased(&mut self, q: Rect, col: Color) {
        let b = self.brush(col);
        unsafe {
            let rt = self.rt();
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
            rt.FillRectangle(&r(q), &b);
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
    }

    /// Dégradé vertical (en-tête de Plume).
    pub fn gradient_v(&mut self, q: Rect, top: Color, bottom: Color) {
        unsafe {
            let rt = self.rt().clone();
            let stops = [D2D1_GRADIENT_STOP { position: 0.0, color: c(top) }, D2D1_GRADIENT_STOP { position: 1.0, color: c(bottom) }];
            let Ok(coll) = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) else { return };
            let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: Vector2 { X: 0.0, Y: q.y }, endPoint: Vector2 { X: 0.0, Y: q.y + q.h } };
            if let Ok(b) = rt.CreateLinearGradientBrush(&props, None, &coll) {
                rt.FillRectangle(&r(q), &b);
            }
        }
    }

    /// Limite le dessin à un rectangle, jusqu'à `unclip`.
    pub fn clip(&mut self, q: Rect) {
        unsafe { self.rt().PushAxisAlignedClip(&r(q), D2D1_ANTIALIAS_MODE_ALIASED) };
    }

    pub fn unclip(&mut self) {
        unsafe { self.rt().PopAxisAlignedClip() };
    }

    /// Texte sur une ligne, centré en hauteur dans `q`, terminé par « … » s'il dépasse.
    pub fn text(&mut self, s: &str, q: Rect, font: Font, col: Color, align: Align) {
        if s.is_empty() || q.is_empty() || col.a <= 0.0 {
            return;
        }
        let b = self.brush(col);
        let f = self.gfx.format(font);
        self.wbuf.clear();
        self.wbuf.extend(s.encode_utf16());
        unsafe {
            let _ = f.SetTextAlignment(match align {
                Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
            });
            let rt = self.rt.as_ref().unwrap();
            rt.DrawText(&self.wbuf, &f, &r(q), &b, TEXT_OPTIONS, DWRITE_MEASURING_MODE_NATURAL);
        }
    }

    /// La mise en page d'un texte sur plusieurs lignes, coupé aux mots dans la largeur `w`.
    fn layout(&mut self, s: &str, w: f32, font: Font) -> Option<IDWriteTextLayout> {
        let f = self.gfx.format(font);
        self.wbuf.clear();
        self.wbuf.extend(s.encode_utf16());
        unsafe {
            let _ = f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
            let layout = self.gfx.dw.CreateTextLayout(&self.wbuf, &f, w.max(1.0), 100_000.0).ok()?;
            let _ = layout.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
            let _ = layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
            let _ = layout
                .SetTrimming(&DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_NONE, delimiter: 0, delimiterCount: 0 }, None::<&IDWriteInlineObject>);
            Some(layout)
        }
    }

    /// Hauteur d'un texte une fois coupé aux mots dans la largeur `w` (un paragraphe vide : une ligne).
    pub fn text_height(&mut self, s: &str, w: f32, font: Font) -> f32 {
        let line = (font.size() * 1.38).ceil();
        if s.is_empty() {
            return line;
        }
        let mut m = DWRITE_TEXT_METRICS::default();
        match self.layout(s, w, font) {
            Some(layout) if unsafe { layout.GetMetrics(&mut m) }.is_ok() => m.height.ceil().max(line),
            _ => line,
        }
    }

    /// Texte sur plusieurs lignes, coupé aux mots, à partir du coin haut gauche de `q` (dont seule la largeur compte).
    pub fn text_block(&mut self, s: &str, q: Rect, font: Font, col: Color) {
        if s.is_empty() || q.w <= 0.0 || col.a <= 0.0 {
            return;
        }
        let b = self.brush(col);
        if let Some(layout) = self.layout(s, q.w, font) {
            unsafe {
                let rt = self.rt.as_ref().unwrap();
                // 4 : polices en couleur (émojis) ; pas de coupe au rectangle, l'appelant s'en charge (`clip`)
                rt.DrawTextLayout(Vector2 { X: q.x, Y: q.y }, &layout, &b, D2D1_DRAW_TEXT_OPTIONS(4));
            }
        }
    }

    /// Largeur d'un texte, gardée en mémoire (les mêmes libellés reviennent à chaque image).
    pub fn text_width(&mut self, s: &str, font: Font) -> f32 {
        if s.is_empty() {
            return 0.0;
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut h);
        let key = (font, h.finish());
        if let Some(w) = self.widths.get(&key) {
            return *w;
        }
        let f = self.gfx.format(font);
        self.wbuf.clear();
        self.wbuf.extend(s.encode_utf16());
        let w = unsafe {
            let _ = f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
            let mut m = DWRITE_TEXT_METRICS::default();
            match self.gfx.dw.CreateTextLayout(&self.wbuf, &f, 100_000.0, 100.0) {
                Ok(layout) if layout.GetMetrics(&mut m).is_ok() => m.widthIncludingTrailingWhitespace,
                _ => s.chars().count() as f32 * font.size() * 0.55,
            }
        };
        if self.widths.len() > 8000 {
            self.widths.clear();
        }
        self.widths.insert(key, w);
        w
    }

    /// Icône vectorielle au trait (1,9 unité pour une icône de 16), comme dans Plume.
    pub fn icon(&mut self, ic: &Icon, q: Rect, col: Color) {
        self.icon_stroke(ic, q, col, 1.9 * ic.vb / 16.0);
    }

    pub fn icon_stroke(&mut self, ic: &Icon, q: Rect, col: Color, weight: f32) {
        let b = self.brush(col);
        let g = self.gfx.icon(ic);
        let s = q.w / ic.vb;
        unsafe {
            let rt = self.rt.as_ref().unwrap();
            rt.SetTransform(&Matrix3x2 { M11: s, M12: 0.0, M21: 0.0, M22: s, M31: q.x, M32: q.y });
            rt.DrawGeometry(&g, &b, weight, &self.gfx.round);
            rt.SetTransform(&IDENTITY);
        }
    }

    /// Icône pleine (étoile d'un favori, par exemple).
    pub fn icon_fill(&mut self, ic: &Icon, q: Rect, col: Color) {
        let b = self.brush(col);
        let g = self.gfx.icon(ic);
        let s = q.w / ic.vb;
        unsafe {
            let rt = self.rt.as_ref().unwrap();
            rt.SetTransform(&Matrix3x2 { M11: s, M12: 0.0, M21: 0.0, M22: s, M31: q.x, M32: q.y });
            rt.FillGeometry(&g, &b, None);
            rt.SetTransform(&IDENTITY);
        }
    }

    /// Symbole de la police d'icônes de Windows (boutons de fenêtre), centré dans `q`.
    pub fn glyph(&mut self, ch: char, q: Rect, col: Color) {
        let mut buf = [0u8; 4];
        self.text(ch.encode_utf8(&mut buf), q, Font::Glyph, col, Align::Center);
    }

    /// Dessine l'image `key` dans `q`. `make` la fournit la première fois (None : image indisponible, rien n'est dessiné).
    pub fn image(&mut self, key: u64, q: Rect, opacity: f32, make: impl FnOnce() -> Option<IWICBitmapSource>) {
        self.rt();
        if !self.bitmaps.contains_key(&key) {
            let bmp = make().and_then(|src| unsafe { self.rt.as_ref().unwrap().CreateBitmapFromWicBitmap(&src, None) }.ok());
            self.bitmaps.insert(key, bmp);
        }
        if let Some(Some(bmp)) = self.bitmaps.get(&key) {
            unsafe { self.rt.as_ref().unwrap().DrawBitmap(bmp, Some(&r(q)), opacity, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, None) };
        }
    }

    /// Oublie une image (elle sera redemandée au prochain dessin).
    pub fn forget_image(&mut self, key: u64) {
        self.bitmaps.remove(&key);
    }

    /// Accès direct à la cible Direct2D, pour les dessins hors du commun.
    pub fn target(&mut self) -> windows::Win32::Graphics::Direct2D::ID2D1RenderTarget {
        self.rt().cast().expect("cible Direct2D")
    }
}
