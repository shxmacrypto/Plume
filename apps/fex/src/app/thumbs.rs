//! La vue en icônes, aux tailles de l'Explorateur : très grandes, grandes, moyennes (Ctrl+Maj+1, 2, 3) et petites
//! (Ctrl+Maj+4) ; Ctrl+Maj+6 revient aux détails, Ctrl+molette change de taille. Une grille de cases, avec pour chaque
//! fichier la miniature que Windows sait en faire (photos, vidéos, PDF…), sinon son icône. Les miniatures sont
//! demandées au shell sur un fil à part, à la taille de la vue, gardées en mémoire (les plus récentes, dans une
//! limite de place), et dessinées dès qu'elles arrivent.

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

/// Marge à gauche de la grille.
pub const GPAD: f32 = 10.0;
/// Place des miniatures gardées en mémoire (octets de pixels).
const KEEP_BYTES: usize = 128 << 20;

/// Une taille de la vue en icônes : la case, le carré de la miniature, l'icône quand il n'y en a pas, et la taille
/// demandée au shell (en pixels). `row` : petites icônes, posées à gauche du nom sur une ligne.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    pub w: f32,
    pub h: f32,
    boxs: f32,
    icon: f32,
    ask: i32,
    row: bool,
}

/// Les tailles, de la plus grande à la plus petite (l'ordre de l'Explorateur).
pub const SIZES: [Tile; 4] = [
    Tile { w: 280.0, h: 304.0, boxs: 256.0, icon: 192.0, ask: 512, row: false },
    Tile { w: 136.0, h: 152.0, boxs: 104.0, icon: 72.0, ask: 208, row: false },
    Tile { w: 100.0, h: 102.0, boxs: 56.0, icon: 48.0, ask: 112, row: false },
    Tile { w: 220.0, h: 28.0, boxs: 16.0, icon: 16.0, ask: 0, row: true },
];

/// Pixels BGRA prémultipliés, largeur, hauteur.
pub struct Pix(Vec<u8>, u32, u32);

/// Une miniature : le fichier, et la taille demandée (chaque taille de la vue a la sienne).
type Want = (PathBuf, i32);

#[derive(Default)]
pub struct Thumbs {
    /// None : pas de miniature pour ce fichier (on montre son icône).
    cache: HashMap<Want, Option<Arc<Pix>>>,
    asked: HashSet<Want>,
    order: VecDeque<Want>,
    bytes: usize,
    /// Images du canevas à oublier (miniatures sorties de la mémoire).
    forget: Vec<u64>,
    worker: Option<mpsc::Sender<Want>>,
}

fn key_of(p: &std::path::Path, ask: i32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (p, ask).hash(&mut h);
    0x7468_0000_0000_0000 | (h.finish() & 0x0000_ffff_ffff_ffff)
}

/// La miniature d'un fichier, par le shell (sur le fil des miniatures, COM en appartement).
fn thumbnail(path: &std::path::Path, ask: i32) -> Option<Pix> {
    unsafe {
        let item: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None::<&IBindCtx>).ok()?;
        let hbmp = item.GetImage(SIZE { cx: ask, cy: ask }, SIIGBF_BIGGERSIZEOK | SIIGBF_THUMBNAILONLY).ok()?;
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
    /// La taille de la vue en icônes.
    pub fn tile(&self) -> Tile {
        SIZES[(self.set.icon_size as usize).min(SIZES.len() - 1)]
    }

    /// Colonnes de la grille (1 en vue Détails).
    pub fn grid_cols(&self) -> usize {
        if self.set.thumbs { ((self.lay.list.w - GPAD - 16.0) / self.tile().w).floor().max(1.0) as usize } else { 1 }
    }

    /// Miniatures prêtes (essais).
    #[cfg(feature = "test-hook")]
    pub fn thumbs_ready(&self) -> usize {
        self.thumbs.cache.values().filter(|v| v.is_some()).count()
    }

    /// Hauteur d'une ligne de la liste ou de la grille.
    pub(super) fn line_h(&self) -> f32 {
        if self.set.thumbs { self.tile().h } else { super::ROW_H }
    }

    /// Case de l'élément `row`, en coordonnées de la fenêtre (défilement compris).
    pub fn tile_rect(&self, row: usize) -> Rect {
        let (l, cols, t) = (self.lay.list, self.grid_cols(), self.tile());
        let (line, col) = (row / cols, row % cols);
        rect(l.x + GPAD + col as f32 * t.w, l.y + LIST_TOP + line as f32 * t.h - self.tab().scroll, t.w, t.h)
    }

    /// L'élément sous ce point de la grille.
    pub(super) fn tile_at(&self, x: f32, y_content: f32) -> Option<usize> {
        let (l, cols, t) = (self.lay.list, self.grid_cols(), self.tile());
        let col = ((x - l.x - GPAD) / t.w).floor();
        if col < 0.0 || col as usize >= cols || y_content < 0.0 {
            return None;
        }
        let r = (y_content / t.h) as usize * cols + col as usize;
        (r < self.tab().rows()).then_some(r)
    }

    /// Le rectangle de sélection, en vue en icônes : les cases qu'il touche.
    pub(super) fn lasso_tiles(&mut self) {
        let (l, p) = (self.lay.list, self.mouse);
        let Drag::Lasso { x0, y0, ctrl, base } = &self.drag else { return };
        let (x0, y0, ctrl, base) = (*x0 + l.x, *y0, *ctrl, base.clone());
        let (cols, t) = (self.grid_cols(), self.tile());
        let tab = self.tab_mut();
        let y1 = p.y - l.y + tab.scroll;
        let (xa, xb, ya, yb) = (x0.min(p.x), x0.max(p.x), y0.min(y1) - LIST_TOP, y0.max(y1) - LIST_TOP);
        let mut last = None;
        for r in 0..tab.rows() {
            let (line, col) = (r / cols, r % cols);
            let (tx, ty) = (l.x + GPAD + col as f32 * t.w, line as f32 * t.h);
            let inside = tx < xb && tx + t.w > xa && ty < yb && ty + t.h > ya;
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
    fn ask_thumb(&mut self, want: Want) {
        if !self.thumbs.asked.insert(want.clone()) {
            return;
        }
        if self.thumbs.worker.is_none() {
            let (tx, rx) = mpsc::channel::<Want>();
            let sender = self.win.sender();
            std::thread::spawn(move || {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                }
                while let Ok(want) = rx.recv() {
                    let pix = thumbnail(&want.0, want.1).map(Arc::new);
                    let delivered = sender.send(move |h, cx| {
                        if let Some(app) = h.as_any().downcast_mut::<App>() {
                            app.thumbs.bytes += pix.as_ref().map_or(0, |p| p.0.len());
                            app.thumbs.cache.insert(want.clone(), pix);
                            app.thumbs.order.push_back(want);
                            // les plus anciennes partent quand la place est prise (jamais celle qui arrive)
                            while app.thumbs.bytes > KEEP_BYTES && app.thumbs.order.len() > 1 {
                                if let Some(old) = app.thumbs.order.pop_front() {
                                    if let Some(Some(p)) = app.thumbs.cache.remove(&old) {
                                        app.thumbs.bytes = app.thumbs.bytes.saturating_sub(p.0.len());
                                    }
                                    app.thumbs.asked.remove(&old);
                                    app.thumbs.forget.push(key_of(&old.0, old.1));
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
            let _ = tx.send(want);
        }
    }

    /// Dessine la grille (la vue Détails dessine ses lignes dans `paint_list`).
    pub(super) fn paint_tiles(&mut self, c: &mut Canvas, t: &Theme) {
        for k in std::mem::take(&mut self.thumbs.forget) {
            c.forget_image(k);
        }
        let list = self.lay.list;
        let (cols, size) = (self.grid_cols(), self.tile());
        let tab = self.tab();
        let rows = tab.rows();
        let scroll = tab.scroll;
        let first_line = ((scroll - LIST_TOP) / size.h).floor().max(0.0) as usize;
        let last_line = ((scroll + list.h - LIST_TOP) / size.h).ceil().max(0.0) as usize;
        let (first, last) = ((first_line * cols).min(rows), ((last_line + 1) * cols).min(rows));
        let local = matches!(tab.loc, Location::Path(_) | Location::Search { .. });
        let sel_bg = t.selection(self.active);
        let mut want = vec![];
        c.clip(list);
        for r in first..last {
            let tile = self.tile_rect(r);
            let bg = tile.inset(if size.row { 1.5 } else { 3.0 });
            let radius = if size.row { 6.0 } else { 10.0 };
            let tab = self.tab();
            let it = tab.item(r);
            if it.sel {
                c.fill_round(bg, radius, sel_bg);
            } else if self.hover == Some(Id::Row(r)) && !tab.stale {
                c.fill_round(bg, radius, t.hover);
            }
            if self.keyboard && self.active && tab.focus == Some(r) {
                c.stroke_round(bg, radius, t.t3, 1.0);
            }
            if self.drop_hl == Some(Id::Row(r)) {
                c.fill_round(bg, radius, t.accent.fade(0.22));
                c.stroke_round(bg, radius, t.accent, 1.5);
            }
            // petites icônes : l'icône à gauche du nom, sur une ligne
            let boxr = if size.row {
                rect(tile.x + 8.0, tile.y + (size.h - size.boxs) / 2.0, size.boxs, size.boxs)
            } else {
                rect(tile.x + (size.w - size.boxs) / 2.0, tile.y + 8.0, size.boxs, size.boxs)
            };
            let path = if local && !it.is_dir() && it.drive.is_none() && size.ask > 0 { tab.path_of(r) } else { None };
            let thumb = path.as_ref().and_then(|p| self.thumbs.cache.get(&(p.clone(), size.ask)).cloned());
            match (&path, thumb) {
                (Some(p), Some(Some(pix))) => {
                    let k = (boxr.w / pix.1 as f32).min(boxr.h / pix.2 as f32).min(1.0 / c.scale().max(0.01));
                    let (w, h) = (pix.1 as f32 * k, pix.2 as f32 * k);
                    let r2 = rect(boxr.x + (boxr.w - w) / 2.0, boxr.bottom() - h, w, h);
                    let wic = self.preview_wic();
                    c.image(key_of(p, size.ask), r2, 1.0, move || unsafe {
                        wic.CreateBitmapFromMemory(pix.1, pix.2, &GUID_WICPixelFormat32bppPBGRA, pix.1 * 4, &pix.0)
                            .ok()
                            .and_then(|b| b.cast::<IWICBitmapSource>().ok())
                    });
                }
                (p, cached) => {
                    if let (Some(p), None) = (p, cached) {
                        want.push((p.clone(), size.ask));
                    }
                    let (ext_icon, _) = self.icons.by_ext(it.ext(), it.is_dir());
                    let icon = if it.icon.get() >= 0 { it.icon.get() } else { ext_icon };
                    self.icons.draw(c, icon, boxr.centered(size.icon, size.icon), if tab.stale { 0.5 } else { 1.0 }, it.is_shortcut());
                }
            }
            let name = super::view::display_name(it, self.set.hide_ext).to_string();
            if size.row {
                let x = boxr.right() + 8.0;
                c.text(&name, rect(x, tile.y, (tile.right() - x - 6.0).max(10.0), size.h), Font::Body, t.t1, Align::Left);
            } else {
                let font = if size.w >= 200.0 { Font::Body } else { Font::Small };
                c.text(&name, rect(tile.x + 6.0, boxr.bottom() + 6.0, size.w - 12.0, 20.0), font, t.t1, Align::Center);
            }
            self.hits.add(bg, Id::Row(r));
        }
        c.unclip();
        for p in want {
            self.ask_thumb(p);
        }
    }
}
