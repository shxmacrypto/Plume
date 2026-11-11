//! La vue « Vignettes » (Ctrl+Maj+2 ; Ctrl+Maj+6 revient aux détails) : une grille de cases, avec pour chaque
//! fichier la miniature que Windows sait en faire (photos, vidéos, PDF…), sinon son icône. Les miniatures sont
//! demandées au shell sur un fil à part, gardées en mémoire (les plus récentes), et dessinées dès qu'elles arrivent.

use super::{App, Drag, Id, LIST_TOP, model::Location};
use crate::ui::{Align, Canvas, Font, Rect, Theme, rect};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{Arc, mpsc},
};
use windows::{
    Win32::{
        Foundation::SIZE,
        Graphics::{
            Gdi::{BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDIBits, GetObjectW, HGDIOBJ},
            Imaging::{GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource},
        },
        System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, IBindCtx},
        UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_THUMBNAILONLY},
    },
    core::{HSTRING, Interface},
};

/// Une case de la grille, en DIP.
pub const TILE_W: f32 = 136.0;
pub const TILE_H: f32 = 152.0;
/// Le carré où va la miniature.
const BOX: f32 = 104.0;
/// Marge à gauche de la grille.
pub const GPAD: f32 = 10.0;
/// Taille demandée au shell, en pixels.
const ASK: i32 = 208;
/// Miniatures gardées en mémoire.
const KEEP: usize = 600;

/// Pixels BGRA prémultipliés, largeur, hauteur.
pub struct Pix(Vec<u8>, u32, u32);

#[derive(Default)]
pub struct Thumbs {
    /// None : pas de miniature pour ce fichier (on montre son icône).
    cache: HashMap<PathBuf, Option<Arc<Pix>>>,
    asked: HashSet<PathBuf>,
    order: VecDeque<PathBuf>,
    /// Images du canevas à oublier (miniatures sorties de la mémoire).
    forget: Vec<u64>,
    worker: Option<mpsc::Sender<PathBuf>>,
}

fn key_of(p: &std::path::Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    p.hash(&mut h);
    0x7468_0000_0000_0000 | (h.finish() & 0x0000_ffff_ffff_ffff)
}

/// La miniature d'un fichier, par le shell (sur le fil des miniatures, COM en appartement).
fn thumbnail(path: &std::path::Path) -> Option<Pix> {
    unsafe {
        let item: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None::<&IBindCtx>).ok()?;
        let hbmp = item.GetImage(SIZE { cx: ASK, cy: ASK }, SIIGBF_BIGGERSIZEOK | SIIGBF_THUMBNAILONLY).ok()?;
        let mut bm = BITMAP::default();
        let got = GetObjectW(HGDIOBJ(hbmp.0), size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
        let (w, h) = (bm.bmWidth.max(0) as u32, bm.bmHeight.max(0) as u32);
        let mut px = vec![0u8; (w * h * 4) as usize];
        let ok = got != 0 && w > 0 && h > 0 && {
            let dc = CreateCompatibleDC(None);
            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32), // de haut en bas
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let lines = GetDIBits(dc, hbmp, 0, h, Some(px.as_mut_ptr() as _), &mut bi, DIB_RGB_COLORS);
            let _ = DeleteDC(dc);
            lines as u32 == h
        };
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        if !ok {
            return None;
        }
        // une photo arrive souvent sans transparence (alpha à zéro partout) : elle est opaque
        if px.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
            px.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p[3] = 255);
        } else {
            px.as_chunks_mut::<4>().0.iter_mut().for_each(|p| {
                let a = p[3] as u32;
                for c in &mut p[..3] {
                    *c = ((*c as u32 * a + 127) / 255) as u8;
                }
            });
        }
        Some(Pix(px, w, h))
    }
}

impl App {
    /// Colonnes de la grille (1 en vue Détails).
    pub fn grid_cols(&self) -> usize {
        if self.set.thumbs { ((self.lay.list.w - GPAD - 16.0) / TILE_W).floor().max(1.0) as usize } else { 1 }
    }

    /// Miniatures prêtes (essais).
    #[cfg(feature = "test-hook")]
    pub fn thumbs_ready(&self) -> usize {
        self.thumbs.cache.values().filter(|v| v.is_some()).count()
    }

    /// Hauteur d'une ligne de la liste ou de la grille.
    pub(super) fn line_h(&self) -> f32 {
        if self.set.thumbs { TILE_H } else { super::ROW_H }
    }

    /// Case de l'élément `row`, en coordonnées de la fenêtre (défilement compris).
    pub fn tile_rect(&self, row: usize) -> Rect {
        let (l, cols) = (self.lay.list, self.grid_cols());
        let (line, col) = (row / cols, row % cols);
        rect(l.x + GPAD + col as f32 * TILE_W, l.y + LIST_TOP + line as f32 * TILE_H - self.tab().scroll, TILE_W, TILE_H)
    }

    /// L'élément sous ce point de la grille.
    pub(super) fn tile_at(&self, x: f32, y_content: f32) -> Option<usize> {
        let (l, cols) = (self.lay.list, self.grid_cols());
        let col = ((x - l.x - GPAD) / TILE_W).floor();
        if col < 0.0 || col as usize >= cols || y_content < 0.0 {
            return None;
        }
        let r = (y_content / TILE_H) as usize * cols + col as usize;
        (r < self.tab().rows()).then_some(r)
    }

    /// Le rectangle de sélection, en vue Vignettes : les cases qu'il touche.
    pub(super) fn lasso_tiles(&mut self) {
        let (l, p) = (self.lay.list, self.mouse);
        let Drag::Lasso { x0, y0, ctrl, base } = &self.drag else { return };
        let (x0, y0, ctrl, base) = (*x0 + l.x, *y0, *ctrl, base.clone());
        let cols = self.grid_cols();
        let tab = self.tab_mut();
        let y1 = p.y - l.y + tab.scroll;
        let (xa, xb, ya, yb) = (x0.min(p.x), x0.max(p.x), y0.min(y1) - LIST_TOP, y0.max(y1) - LIST_TOP);
        let mut last = None;
        for r in 0..tab.rows() {
            let (line, col) = (r / cols, r % cols);
            let (tx, ty) = (l.x + GPAD + col as f32 * TILE_W, line as f32 * TILE_H);
            let inside = tx < xb && tx + TILE_W > xa && ty < yb && ty + TILE_H > ya;
            let i = tab.idx(r);
            let was = base.get(i).copied().unwrap_or(false);
            tab.items[i].sel = if ctrl { was ^ inside } else { inside };
            if inside {
                last = Some(r);
            }
        }
        tab.recount();
        if last.is_some() {
            tab.focus = last;
        }
    }

    /// Demande une miniature au fil des miniatures (créé au premier besoin).
    fn ask_thumb(&mut self, path: PathBuf) {
        if !self.thumbs.asked.insert(path.clone()) {
            return;
        }
        if self.thumbs.worker.is_none() {
            let (tx, rx) = mpsc::channel::<PathBuf>();
            let sender = self.win.sender();
            std::thread::spawn(move || {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                }
                while let Ok(path) = rx.recv() {
                    let pix = thumbnail(&path).map(Arc::new);
                    let delivered = sender.send(move |h, cx| {
                        if let Some(app) = h.as_any().downcast_mut::<App>() {
                            app.thumbs.cache.insert(path.clone(), pix);
                            app.thumbs.order.push_back(path);
                            while app.thumbs.order.len() > KEEP {
                                if let Some(old) = app.thumbs.order.pop_front() {
                                    app.thumbs.cache.remove(&old);
                                    app.thumbs.asked.remove(&old);
                                    app.thumbs.forget.push(key_of(&old));
                                }
                            }
                            cx.repaint();
                        }
                    });
                    if !delivered {
                        break;
                    }
                }
            });
            self.thumbs.worker = Some(tx);
        }
        if let Some(tx) = &self.thumbs.worker {
            let _ = tx.send(path);
        }
    }

    /// Dessine la grille (la vue Détails dessine ses lignes dans `paint_list`).
    pub(super) fn paint_tiles(&mut self, c: &mut Canvas, t: &Theme) {
        for k in std::mem::take(&mut self.thumbs.forget) {
            c.forget_image(k);
        }
        let list = self.lay.list;
        let cols = self.grid_cols();
        let tab = self.tab();
        let rows = tab.rows();
        let scroll = tab.scroll;
        let first_line = ((scroll - LIST_TOP) / TILE_H).floor().max(0.0) as usize;
        let last_line = ((scroll + list.h - LIST_TOP) / TILE_H).ceil().max(0.0) as usize;
        let (first, last) = ((first_line * cols).min(rows), ((last_line + 1) * cols).min(rows));
        let local = matches!(tab.loc, Location::Path(_) | Location::Search { .. });
        let sel_bg = t.selection(self.active);
        let mut want = vec![];
        c.clip(list);
        for r in first..last {
            let tile = self.tile_rect(r);
            let bg = tile.inset(3.0);
            let tab = self.tab();
            let it = tab.item(r);
            if it.sel {
                c.fill_round(bg, 10.0, sel_bg);
            } else if self.hover == Some(Id::Row(r)) && !tab.stale {
                c.fill_round(bg, 10.0, t.hover);
            }
            if self.keyboard && self.active && tab.focus == Some(r) {
                c.stroke_round(bg, 10.0, t.t3, 1.0);
            }
            if self.drop_hl == Some(Id::Row(r)) {
                c.fill_round(bg, 10.0, t.accent.fade(0.22));
                c.stroke_round(bg, 10.0, t.accent, 1.5);
            }
            let boxr = rect(tile.x + (TILE_W - BOX) / 2.0, tile.y + 8.0, BOX, BOX);
            let path = if local && !it.is_dir() && it.drive.is_none() { tab.path_of(r) } else { None };
            let thumb = path.as_ref().and_then(|p| self.thumbs.cache.get(p).cloned());
            match (&path, thumb) {
                (Some(p), Some(Some(pix))) => {
                    let k = (boxr.w / pix.1 as f32).min(boxr.h / pix.2 as f32).min(1.0 / c.scale().max(0.01));
                    let (w, h) = (pix.1 as f32 * k, pix.2 as f32 * k);
                    let r2 = rect(boxr.x + (boxr.w - w) / 2.0, boxr.bottom() - h, w, h);
                    let wic = self.preview_wic();
                    c.image(key_of(p), r2, 1.0, move || unsafe {
                        wic.CreateBitmapFromMemory(pix.1, pix.2, &GUID_WICPixelFormat32bppPBGRA, pix.1 * 4, &pix.0)
                            .ok()
                            .and_then(|b| b.cast::<IWICBitmapSource>().ok())
                    });
                }
                (p, cached) => {
                    if let (Some(p), None) = (p, cached) {
                        want.push(p.clone());
                    }
                    let (ext_icon, _) = self.icons.by_ext(it.ext(), it.is_dir());
                    let icon = if it.icon.get() >= 0 { it.icon.get() } else { ext_icon };
                    self.icons.draw(c, icon, boxr.centered(72.0, 72.0), if tab.stale { 0.5 } else { 1.0 }, it.is_shortcut());
                }
            }
            let name = super::view::display_name(it, self.set.hide_ext).to_string();
            c.text(&name, rect(tile.x + 6.0, boxr.bottom() + 6.0, TILE_W - 12.0, 20.0), Font::Small, t.t1, Align::Center);
            self.hits.add(bg, Id::Row(r));
        }
        c.unclip();
        for p in want {
            self.ask_thumb(p);
        }
    }
}
