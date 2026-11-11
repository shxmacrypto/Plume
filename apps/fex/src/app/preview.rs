//! Le volet d'aperçu (Alt+P, comme dans l'Explorateur) : à droite de la liste, l'élément qui a le focus. Une image est
//! affichée, un fichier texte montre son début, un dossier son nombre d'éléments ; sinon, son icône et ce qu'on en sait.
//! La lecture se fait sur un autre fil : la liste reste fluide pendant qu'une grande photo se décode.

use super::{App, model::Location};
use crate::ui::{Align, Canvas, Font, Gfx, Rect, Theme, rect};
use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};
use windows::{
    Win32::{
        Foundation::GENERIC_READ,
        Graphics::Imaging::{
            CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone,
            WICBitmapInterpolationModeFant, WICBitmapPaletteTypeMedianCut, WICDecodeMetadataCacheOnDemand,
        },
        System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
    },
    core::{HSTRING, Interface},
};

/// Côté le plus long d'une image d'aperçu, en pixels.
const IMAGE_MAX: u32 = 1400;
/// Ce qui est lu d'un fichier texte.
const TEXT_BYTES: usize = 48 * 1024;
const TEXT_LINES: usize = 160;
const IMAGES: [&str; 12] = ["jpg", "jpeg", "jfif", "png", "gif", "bmp", "ico", "tif", "tiff", "webp", "heic", "dds"];

pub enum Content {
    Loading,
    /// Pixels prêts pour Direct2D (BGRA prémultiplié), largeur, hauteur, numéro d'image du canevas.
    Image(Arc<Vec<u8>>, u32, u32, u64),
    Text(String),
    /// Un dossier et son nombre d'éléments (None : illisible).
    Folder(Option<usize>),
    /// Rien à montrer de plus que l'icône et les renseignements.
    Info,
}

pub struct Preview {
    gfx: Rc<Gfx>,
    pub path: Option<PathBuf>,
    pub content: Content,
    generation: u64,
    /// Image du canevas à oublier au prochain dessin (l'aperçu précédent).
    forget: Option<u64>,
}

impl Preview {
    pub fn new(gfx: Rc<Gfx>) -> Preview {
        Preview { gfx, path: None, content: Content::Info, generation: 0, forget: None }
    }
}

fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// Décode une image, réduite pour l'aperçu. Sur ce fil-ci (COM en multi-fils, sa propre fabrique WIC).
fn decode(path: &Path) -> Option<(Vec<u8>, u32, u32)> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let decoder = wic.CreateDecoderFromFilename(&HSTRING::from(path), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        if w == 0 || h == 0 {
            return None;
        }
        let k = (IMAGE_MAX as f32 / w.max(h) as f32).min(1.0);
        let (tw, th) = (((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1));
        let scaler = wic.CreateBitmapScaler().ok()?;
        scaler.Initialize(&frame, tw, th, WICBitmapInterpolationModeFant).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
        let mut buf = vec![0u8; (tw * th * 4) as usize];
        conv.CopyPixels(std::ptr::null(), tw * 4, &mut buf).ok()?;
        Some((buf, tw, th))
    }
}

/// Le début d'un fichier, s'il ressemble à du texte (UTF-8, ou UTF-16 avec sa marque).
pub fn read_text(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; TEXT_BYTES];
    let n = f.read(&mut buf).ok()?;
    buf.truncate(n);
    let text = if buf.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = buf[2..].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        String::from_utf16_lossy(&units)
    } else {
        // un octet nul au début : un fichier binaire
        if buf.iter().take(8192).any(|&b| b == 0) {
            return None;
        }
        let s = String::from_utf8_lossy(buf.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&buf)).into_owned();
        // trop de caractères de remplacement : ce n'est pas du texte
        if s.chars().take(4000).filter(|&c| c == '\u{FFFD}').count() > 40 {
            return None;
        }
        s
    };
    let mut out = String::new();
    for line in text.lines().take(TEXT_LINES) {
        let line: String = line.chars().take(220).collect();
        out.push_str(&line.replace('\t', "    "));
        out.push('\n');
    }
    Some(out)
}

fn load(path: &Path) -> Content {
    if path.is_dir() {
        return Content::Folder(std::fs::read_dir(path).ok().map(|r| r.take(100_000).count()));
    }
    if IMAGES.contains(&ext_of(path).as_str())
        && let Some((px, w, h)) = decode(path)
    {
        return Content::Image(Arc::new(px), w, h, 0);
    }
    match read_text(path) {
        Some(t) if !t.trim().is_empty() => Content::Text(t),
        _ => Content::Info,
    }
}

impl App {
    /// Le chemin de l'élément qui a le focus (dans un dossier du PC ou une recherche).
    fn preview_target(&self) -> Option<PathBuf> {
        let tab = self.tab();
        if matches!(tab.loc, Location::Remote { .. } | Location::ThisPc) {
            return None;
        }
        let row = tab.focus.filter(|&r| r < tab.rows())?;
        tab.path_of(row)
    }

    /// Suit l'élément qui a le focus ; lance la lecture quand il change.
    fn preview_follow(&mut self) {
        let want = self.preview_target();
        if want == self.preview.path {
            return;
        }
        if let Content::Image(_, _, _, key) = self.preview.content {
            self.preview.forget = Some(key);
        }
        self.preview.generation += 1;
        self.preview.path = want.clone();
        let Some(path) = want else {
            self.preview.content = Content::Info;
            return;
        };
        self.preview.content = Content::Loading;
        let generation = self.preview.generation;
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let content = load(&path);
            sender.send(move |h, cx| {
                if let Some(app) = h.as_any().downcast_mut::<App>()
                    && app.preview.generation == generation
                {
                    app.preview.content = match content {
                        Content::Image(px, w, h, _) => Content::Image(px, w, h, 0x7072_6576_0000_0000 | generation),
                        other => other,
                    };
                    cx.repaint();
                }
            });
        });
    }

    pub(super) fn paint_preview(&mut self, c: &mut Canvas, t: &Theme, pane: Rect) {
        self.preview_follow();
        if let Some(key) = self.preview.forget.take() {
            c.forget_image(key);
        }
        c.fill(pane, t.bg2);
        c.vline(pane.x, pane.y, pane.h, t.sep);
        let s = self.s;
        let inner = pane.inset_xy(18.0, 16.0);
        let Some(path) = self.preview.path.clone() else {
            c.text(s.preview_none, rect(inner.x, inner.y + inner.h / 2.0 - 10.0, inner.w, 20.0), Font::Body, t.t3, Align::Center);
            return;
        };
        let tab = self.tab();
        let row = tab.focus.unwrap_or(0).min(tab.rows().saturating_sub(1));
        let it = tab.item(row);
        let (dir, size, mtime, attrs_icon, shortcut) = (it.is_dir(), it.size, it.mtime, it.icon.get(), it.is_shortcut());
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
        let ext = ext_of(&path);
        let (ext_icon, type_name) = self.icons.by_ext(&ext, dir);
        let icon = if attrs_icon >= 0 { attrs_icon } else { ext_icon };

        // en haut : ce que montre le contenu (image, texte) ; en bas, le nom et les renseignements
        let info_h = 86.0;
        let body = rect(inner.x, inner.y, inner.w, (inner.h - info_h - 12.0).max(0.0));
        match &self.preview.content {
            Content::Image(px, w, h, key) => {
                // jamais plus grand que l'image elle-même (un pixel de l'image = un pixel de l'écran au plus)
                let k = (body.w / *w as f32).min(body.h / *h as f32).min(1.0 / c.scale().max(0.01));
                let (dw, dh) = (*w as f32 * k, *h as f32 * k);
                let r = rect(body.x + (body.w - dw) / 2.0, body.y + (body.h - dh) / 2.0, dw, dh);
                let (px, w, h) = (px.clone(), *w, *h);
                let wic = self.preview.gfx.wic.clone();
                c.image(*key, r, 1.0, move || unsafe {
                    wic.CreateBitmapFromMemory(w, h, &GUID_WICPixelFormat32bppPBGRA, w * 4, &px).ok().and_then(|b| b.cast::<IWICBitmapSource>().ok())
                });
                c.stroke_round(r, 2.0, t.sep, 1.0);
            }
            Content::Text(text) => {
                let card = body;
                c.fill_round(card, 10.0, t.fill);
                c.clip(card.inset(1.0));
                c.text_block(text, card.inset_xy(12.0, 10.0), Font::Small, t.t2);
                c.unclip();
            }
            Content::Loading => c.text(s.preview_loading, body, Font::Body, t.t3, Align::Center),
            Content::Folder(_) | Content::Info => {
                let big = 96.0f32.min(body.w).min(body.h);
                self.icons.draw(c, icon, rect(body.x + (body.w - big) / 2.0, body.y + (body.h - big) / 2.0, big, big), 1.0, shortcut);
            }
        }
        let y = inner.bottom() - info_h;
        c.hline(inner.x, y - 6.0, inner.w, t.sep);
        c.text(&name, rect(inner.x, y, inner.w, 22.0), Font::Title, t.t1, Align::Left);
        let kind = match &self.preview.content {
            Content::Folder(Some(n)) => format!("{} · {}", type_name, s.count(*n as u64, s.item)),
            _ if dir => type_name.to_string(),
            _ => format!("{} · {}", type_name, s.size(size)),
        };
        c.text(&kind, rect(inner.x, y + 26.0, inner.w, 18.0), Font::Small, t.t2, Align::Left);
        c.text(&self.dates.get(mtime), rect(inner.x, y + 46.0, inner.w, 18.0), Font::Small, t.t3, Align::Left);
    }
}
