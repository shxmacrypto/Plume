//! Icônes des fichiers : celles de Windows. Chaque fichier a un numéro dans la liste d'images du système ;
//! on le trouve d'abord d'après l'extension (instantané, sans lire le disque), puis au besoin d'après le fichier
//! lui-même (exécutables, raccourcis, dossiers personnalisés), sur un fil de travail.

use super::wide;
use crate::ui::{Canvas, Gfx, Rect};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::Path,
    rc::Rc,
};
use windows::{
    Win32::{
        Graphics::Imaging::{GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom},
        Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES},
        UI::{
            Controls::{IImageList, ILD_TRANSPARENT},
            Shell::{
                SHFILEINFOW, SHGFI_DISPLAYNAME, SHGFI_FLAGS, SHGFI_SMALLICON, SHGFI_SYSICONINDEX, SHGFI_TYPENAME, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW,
                SHGetIconOverlayIndexW, SHGetImageList, SHIL_EXTRALARGE, SHIL_JUMBO, SHIL_LARGE, SHIL_SMALL,
            },
            WindowsAndMessaging::DestroyIcon,
        },
    },
    core::{Interface, PCWSTR},
};

/// Ce que le shell dit d'un fichier.
pub struct Info {
    pub icon: i32,
    pub display: String,
    pub type_name: String,
}

fn info(path: PCWSTR, attrs: FILE_FLAGS_AND_ATTRIBUTES, flags: SHGFI_FLAGS) -> Option<Info> {
    let mut fi = SHFILEINFOW::default();
    let r = unsafe { SHGetFileInfoW(path, attrs, Some(&mut fi), size_of::<SHFILEINFOW>() as u32, flags) };
    (r != 0).then(|| Info { icon: fi.iIcon, display: super::from_wide(&fi.szDisplayName), type_name: super::from_wide(&fi.szTypeName) })
}

/// Icône, nom affiché et type d'un fichier ou dossier réel (lit le disque ; à appeler hors du fil de l'interface).
pub fn exact(path: &Path) -> Option<Info> {
    let w = wide(path);
    info(PCWSTR(w.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(0), SHGFI_SYSICONINDEX | SHGFI_SMALLICON | SHGFI_DISPLAYNAME | SHGFI_TYPENAME)
}

/// Icône et type d'après le nom seul (aucun accès au disque).
pub fn generic(name: &str, dir: bool) -> Option<Info> {
    let w: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let attrs = if dir { FILE_ATTRIBUTE_DIRECTORY } else { FILE_ATTRIBUTE_NORMAL };
    info(PCWSTR(w.as_ptr()), attrs, SHGFI_SYSICONINDEX | SHGFI_SMALLICON | SHGFI_TYPENAME | SHGFI_USEFILEATTRIBUTES)
}

/// Ces fichiers portent leur propre icône : celle de l'extension ne suffit pas.
pub fn has_own_icon(ext: &str) -> bool {
    matches!(ext.to_ascii_lowercase().as_str(), "exe" | "lnk" | "ico" | "cur" | "ani" | "scr" | "url" | "appref-ms" | "msc" | "cpl")
}

/// Les listes d'images du système, de la plus petite à la plus grande.
const LISTS: [u32; 4] = [SHIL_SMALL, SHIL_LARGE, SHIL_EXTRALARGE, SHIL_JUMBO];

pub struct Icons {
    gfx: Rc<Gfx>,
    /// (liste, taille en pixels), chargées à la demande.
    lists: RefCell<[Option<(IImageList, i32)>; 4]>,
    by_ext: RefCell<HashMap<String, (i32, Rc<str>)>>,
    folder: Cell<Option<i32>>,
    folder_type: RefCell<Option<Rc<str>>>,
    /// Numéro de la petite flèche des raccourcis dans la liste d'images (0 : inconnu).
    link_overlay: Cell<Option<u32>>,
}

impl Icons {
    pub fn new(gfx: Rc<Gfx>) -> Icons {
        Icons {
            gfx,
            lists: Default::default(),
            by_ext: Default::default(),
            folder: Cell::new(None),
            folder_type: Default::default(),
            link_overlay: Cell::new(None),
        }
    }

    /// Icône et nom de type d'après l'extension, gardés en mémoire.
    pub fn by_ext(&self, ext: &str, dir: bool) -> (i32, Rc<str>) {
        if dir {
            if let (Some(i), Some(t)) = (self.folder.get(), self.folder_type.borrow().clone()) {
                return (i, t);
            }
            let f = generic("dossier", true);
            let r = (f.as_ref().map_or(-1, |f| f.icon), Rc::<str>::from(f.map_or(String::new(), |f| f.type_name)));
            self.folder.set(Some(r.0));
            *self.folder_type.borrow_mut() = Some(r.1.clone());
            return r;
        }
        let key = ext.to_lowercase();
        if let Some(r) = self.by_ext.borrow().get(&key) {
            return r.clone();
        }
        let f = generic(&if key.is_empty() { "fichier".to_string() } else { format!("x.{key}") }, false);
        let r = (f.as_ref().map_or(-1, |f| f.icon), Rc::<str>::from(f.map_or(String::new(), |f| f.type_name)));
        self.by_ext.borrow_mut().insert(key, r.clone());
        r
    }

    /// Liste d'images dont les icônes font au moins `px` pixels (la plus grande sinon) : (rang, liste, taille).
    fn list_for(&self, px: f32) -> Option<(usize, IImageList, i32)> {
        let mut lists = self.lists.borrow_mut();
        let mut best = None;
        for (k, id) in LISTS.iter().enumerate() {
            if lists[k].is_none() {
                let Ok(l) = (unsafe { SHGetImageList::<IImageList>(*id as i32) }) else { continue };
                let (mut cx, mut cy) = (0, 0);
                if unsafe { l.GetIconSize(&mut cx, &mut cy) }.is_err() {
                    continue;
                }
                lists[k] = Some((l, cx));
            }
            let (l, size) = lists[k].clone().unwrap();
            best = Some((k, l, size));
            if size as f32 >= px - 0.5 {
                break;
            }
        }
        best
    }

    fn source(&self, list: &IImageList, index: i32, shortcut: bool) -> Option<IWICBitmapSource> {
        unsafe {
            let mut flags = ILD_TRANSPARENT.0;
            if shortcut {
                let overlay = self.link_overlay.get().unwrap_or_else(|| {
                    // IDO_SHGIOI_LINK : la flèche des raccourcis
                    let i = SHGetIconOverlayIndexW(PCWSTR::null(), 0x0FFF_FFFE);
                    let i = if i > 0 { i as u32 } else { 2 };
                    self.link_overlay.set(Some(i));
                    i
                });
                // INDEXTOOVERLAYMASK
                flags |= overlay << 8;
            }
            let icon = list.GetIcon(index, flags).ok()?;
            let bmp = self.gfx.wic.CreateBitmapFromHICON(icon);
            let _ = DestroyIcon(icon);
            let conv = self.gfx.wic.CreateFormatConverter().ok()?;
            conv.Initialize(&bmp.ok()?, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
            conv.cast().ok()
        }
    }

    /// Dessine l'icône système `index` dans `r` ; `shortcut` ajoute la petite flèche des raccourcis.
    pub fn draw(&self, c: &mut Canvas, index: i32, r: Rect, opacity: f32, shortcut: bool) {
        if index < 0 {
            return;
        }
        let Some((rank, list, _)) = self.list_for(r.w * c.scale()) else { return };
        let key = ((rank as u64) << 40) | ((shortcut as u64) << 32) | index as u64;
        c.image(key, r, opacity, || self.source(&list, index, shortcut));
    }
}
