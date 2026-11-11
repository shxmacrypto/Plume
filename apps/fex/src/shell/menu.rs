//! Menus : les nôtres (menus natifs de Windows, sombres comme la fenêtre) et le menu contextuel du shell,
//! celui de l'Explorateur, avec « Ouvrir avec », « Envoyer vers » et les entrées des autres programmes.

use super::wide;
use crate::ui::Window;
use std::{ffi::CString, path::PathBuf, rc::Rc};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::{
            Com::{CoTaskMemFree, IBindCtx},
            LibraryLoader::{GetProcAddress, LoadLibraryW},
        },
        UI::{
            Shell::{
                CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX, Common::ITEMIDLIST, IContextMenu, IContextMenu2, IContextMenu3, ILFindLastID, IShellFolder,
                SHBindToParent, SHParseDisplayName,
            },
            WindowsAndMessaging::{
                AppendMenuW, CreatePopupMenu, DestroyMenu, GetMenuItemCount, GetMenuStringW, HMENU, InsertMenuW, MF_BYPOSITION, MF_CHECKED, MF_GRAYED,
                MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW, SW_SHOWNORMAL, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_RIGHTBUTTON,
                TrackPopupMenuEx, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR, WM_NULL,
            },
        },
    },
    core::{Interface, PCSTR, PCWSTR, PSTR, w},
};

/// Numéros réservés aux commandes du shell dans un menu contextuel ; les nôtres sont au-dessus.
const SHELL_FIRST: u32 = 1;
const SHELL_LAST: u32 = 0x6FFF;
const CMF_CANRENAME: u32 = 0x10;
const CMF_EXTENDEDVERBS: u32 = 0x100;
const CMIC_MASK_UNICODE: u32 = 0x4000;
const CMIC_MASK_SHIFT_DOWN: u32 = 0x1000_0000;
const CMIC_MASK_PTINVOKE: u32 = 0x2000_0000;
const GCS_VERBA: u32 = 0;

fn wz(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Menus de Windows en sombre ou en clair, comme la fenêtre (fonctions non documentées d'uxtheme, absentes
/// des très vieux Windows 10 : on s'en passe alors).
pub fn set_dark_menus(dark: bool) {
    unsafe {
        let Ok(ux) = LoadLibraryW(w!("uxtheme.dll")) else { return };
        // n° 135 : SetPreferredAppMode (2 = toujours sombre, 3 = toujours clair) ; n° 136 : FlushMenuThemes
        if let Some(set_mode) = GetProcAddress(ux, PCSTR(135 as *const u8)) {
            let set_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(set_mode);
            set_mode(if dark { 2 } else { 3 });
        }
        if let Some(flush) = GetProcAddress(ux, PCSTR(136 as *const u8)) {
            let flush: unsafe extern "system" fn() = std::mem::transmute(flush);
            flush();
        }
    }
}

// ───────────────────────── essais : choisir dans un menu sans l'afficher ─────────────────────────

/// Avec la prise d'essai, un menu ne s'affiche pas : il note ses entrées et « clique » celle qu'on lui a désignée.
#[cfg(feature = "test-hook")]
pub mod test {
    use std::cell::RefCell;
    #[derive(Default)]
    pub struct TestMenu {
        pub on: bool,
        pub choose: Option<String>,
        pub last: Vec<String>,
    }
    thread_local! { pub static TEST: RefCell<TestMenu> = RefCell::new(TestMenu::default()); }
}

/// Texte d'une entrée sans le « & » du raccourci clavier ni ce qui suit la tabulation.
#[cfg(feature = "test-hook")]
fn plain(label: &str) -> String {
    label.split('\t').next().unwrap_or("").replace('&', "")
}

/// Toutes les entrées d'un menu et de ses sous-menus : (numéro, texte « Menu > Sous-menu > Entrée »).
#[cfg(feature = "test-hook")]
unsafe fn entries(h: HMENU, prefix: &str, out: &mut Vec<(u32, String)>, depth: u32) {
    use windows::Win32::UI::WindowsAndMessaging::{GetMenuItemID, GetMenuState, GetSubMenu};
    unsafe {
        for i in 0..GetMenuItemCount(Some(h)).max(0) {
            let mut buf = [0u16; 260];
            let n = GetMenuStringW(h, i as u32, Some(&mut buf), MF_BYPOSITION).max(0) as usize;
            let label = plain(&String::from_utf16_lossy(&buf[..n]));
            let state = GetMenuState(h, i as u32, MF_BYPOSITION);
            let mut text = format!("{prefix}{label}");
            if state & MF_CHECKED.0 != 0 {
                text.push_str(" [x]");
            }
            if state & MF_GRAYED.0 != 0 {
                text.push_str(" [off]");
            }
            let sub = GetSubMenu(h, i);
            if !sub.is_invalid() && depth < 2 {
                if !label.is_empty() {
                    entries(sub, &format!("{prefix}{label} > "), out, depth + 1);
                }
            } else if !label.is_empty() {
                out.push((GetMenuItemID(h, i), text));
            }
        }
    }
}

/// Affiche le menu et renvoie le numéro de l'entrée choisie (0 : aucune).
unsafe fn track(win: &Window, h: HMENU, at: (i32, i32), flags: u32) -> u32 {
    unsafe {
        #[cfg(feature = "test-hook")]
        {
            let on = test::TEST.with(|t| t.borrow().on);
            if on {
                let mut all = vec![];
                entries(h, "", &mut all, 0);
                return test::TEST.with(|t| {
                    let mut t = t.borrow_mut();
                    t.last = all.iter().map(|(_, text)| text.clone()).collect();
                    let wanted = t.choose.take();
                    wanted
                        .and_then(|w| all.iter().find(|(_, text)| text.trim_end_matches(" [x]") == w || *text == w).map(|(id, _)| *id))
                        .filter(|id| *id != u32::MAX)
                        .unwrap_or(0)
                });
            }
        }
        // un menu ne se referme d'un clic à côté que si sa fenêtre est au premier plan
        let _ = SetForegroundWindow(win.hwnd);
        let cmd = TrackPopupMenuEx(h, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0 | flags, at.0, at.1, win.hwnd, None).0 as u32;
        let _ = PostMessageW(Some(win.hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        cmd
    }
}

// ───────────────────────── nos menus ─────────────────────────

pub struct Menu {
    h: HMENU,
}

impl Default for Menu {
    fn default() -> Self {
        Menu::new()
    }
}

impl Menu {
    pub fn new() -> Menu {
        Menu { h: unsafe { CreatePopupMenu() }.unwrap_or_default() }
    }

    pub fn item(&mut self, id: u32, label: &str) -> &mut Self {
        self.add(id, label, false, true)
    }

    /// Entrée cochée ou non.
    pub fn check(&mut self, id: u32, label: &str, checked: bool) -> &mut Self {
        self.add(id, label, checked, true)
    }

    pub fn add(&mut self, id: u32, label: &str, checked: bool, enabled: bool) -> &mut Self {
        let mut flags = MF_STRING;
        if checked {
            flags |= MF_CHECKED;
        }
        if !enabled {
            flags |= MF_GRAYED;
        }
        let text = wz(label);
        unsafe {
            let _ = AppendMenuW(self.h, flags, id as usize, PCWSTR(text.as_ptr()));
        }
        self
    }

    pub fn sep(&mut self) -> &mut Self {
        unsafe {
            let _ = AppendMenuW(self.h, MF_SEPARATOR, 0, PCWSTR::null());
        }
        self
    }

    /// Sous-menu (il appartient désormais à ce menu).
    pub fn sub(&mut self, label: &str, sub: Menu) -> &mut Self {
        let text = wz(label);
        unsafe {
            let _ = AppendMenuW(self.h, MF_POPUP, sub.h.0 as usize, PCWSTR(text.as_ptr()));
        }
        std::mem::forget(sub);
        self
    }

    /// Affiche le menu au point `at` (pixels de l'écran) ; `right` : son bord droit est à ce point.
    /// Bloque jusqu'au choix : à appeler depuis `Cx::after`.
    pub fn show(&self, win: &Window, at: (i32, i32), right: bool) -> Option<u32> {
        let cmd = unsafe { track(win, self.h, at, if right { TPM_RIGHTALIGN.0 } else { 0 }) };
        (cmd != 0).then_some(cmd)
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.h);
        }
    }
}

// ───────────────────────── menu contextuel du shell ─────────────────────────

/// Identifiants shell (PIDL) de quelques chemins, libérés à la fin.
struct Pidls(Vec<*mut ITEMIDLIST>);

impl Drop for Pidls {
    fn drop(&mut self) {
        for p in &self.0 {
            unsafe { CoTaskMemFree(Some(*p as *const _)) };
        }
    }
}

/// Objet d'interface du shell (menu contextuel, données à glisser, cible de dépôt) pour des éléments d'un même dossier.
pub(super) unsafe fn ui_object<T: Interface>(hwnd: HWND, paths: &[PathBuf]) -> Option<T> {
    unsafe {
        let mut abs = Pidls(Vec::with_capacity(paths.len()));
        for p in paths {
            let name = wide(p);
            let mut pidl = std::ptr::null_mut();
            SHParseDisplayName(PCWSTR(name.as_ptr()), None::<&IBindCtx>, &mut pidl, 0, None).ok()?;
            abs.0.push(pidl);
        }
        let folder: IShellFolder = SHBindToParent(*abs.0.first()?, None).ok()?;
        let children: Vec<*const ITEMIDLIST> = abs.0.iter().map(|&p| ILFindLastID(p) as *const ITEMIDLIST).collect();
        folder.GetUIObjectOf(hwnd, &children, None).ok()
    }
}

/// Ce que l'utilisateur a choisi dans le menu contextuel.
#[derive(Debug, PartialEq)]
pub enum Picked {
    Nothing,
    /// Une de nos entrées, placées en tête du menu.
    Own(u32),
    /// « Renommer » : c'est à la liste d'ouvrir son champ de saisie.
    Rename,
    /// Une commande du shell, déjà exécutée (son nom si elle en a un : « delete », « copy », « properties »…).
    Done(String),
}

/// Affiche le menu contextuel de l'Explorateur pour `paths` (tous dans le même dossier), au point `at` de l'écran.
/// `top` : nos entrées, placées en tête (numéros au-dessus de 0x7000). Bloque : à appeler depuis `Cx::after`.
pub fn shell_menu(win: &Window, paths: &[PathBuf], at: (i32, i32), extended: bool, top: &[(u32, String)]) -> Picked {
    unsafe {
        let Some(cm) = ui_object::<IContextMenu>(win.hwnd, paths) else { return Picked::Nothing };
        let Ok(h) = CreatePopupMenu() else { return Picked::Nothing };
        let flags = CMF_CANRENAME | if extended { CMF_EXTENDEDVERBS } else { 0 };
        if cm.QueryContextMenu(h, 0, SHELL_FIRST, SHELL_LAST, flags).is_err() {
            let _ = DestroyMenu(h);
            return Picked::Nothing;
        }
        // nos entrées en tête, sauf celles que le menu de Windows propose déjà (« Copier en tant que chemin d'accès »)
        let existing: Vec<String> = (0..GetMenuItemCount(Some(h)).max(0))
            .map(|i| {
                let mut buf = [0u16; 260];
                let n = GetMenuStringW(h, i as u32, Some(&mut buf), MF_BYPOSITION).max(0) as usize;
                String::from_utf16_lossy(&buf[..n]).replace('&', "").to_lowercase()
            })
            .collect();
        let ours: Vec<&(u32, String)> = top.iter().filter(|(_, label)| !existing.contains(&label.to_lowercase())).collect();
        if !ours.is_empty() {
            let _ = InsertMenuW(h, 0, MF_BYPOSITION | MF_SEPARATOR, 0, PCWSTR::null());
            for (id, label) in ours.iter().rev() {
                let text = wz(label);
                let _ = InsertMenuW(h, 0, MF_BYPOSITION | MF_STRING, *id as usize, PCWSTR(text.as_ptr()));
            }
        }
        // certains sous-menus (« Ouvrir avec », « Envoyer vers ») sont remplis et dessinés par le shell à l'ouverture
        let (cm2, cm3) = (cm.cast::<IContextMenu2>().ok(), cm.cast::<IContextMenu3>().ok());
        win.set_hook(Some(Rc::new(move |msg, wp, lp| match msg {
            WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR => {
                if let Some(c3) = &cm3 {
                    let mut result = LRESULT(0);
                    c3.HandleMenuMsg2(msg, wp, lp, Some(&mut result)).ok().map(|_| result)
                } else if let Some(c2) = &cm2 {
                    c2.HandleMenuMsg(msg, wp, lp).ok().map(|_| LRESULT(if msg == WM_INITMENUPOPUP { 0 } else { 1 }))
                } else {
                    None
                }
            }
            _ => None,
        })));
        let cmd = track(win, h, at, 0);
        win.set_hook(None);
        let picked = if cmd == 0 {
            Picked::Nothing
        } else if !(SHELL_FIRST..=SHELL_LAST).contains(&cmd) {
            Picked::Own(cmd)
        } else {
            let offset = (cmd - SHELL_FIRST) as usize;
            let mut buf = [0u8; 128];
            let verb = if cm.GetCommandString(offset, GCS_VERBA, None, PSTR(buf.as_mut_ptr()), buf.len() as u32).is_ok() {
                String::from_utf8_lossy(&buf[..buf.iter().position(|&b| b == 0).unwrap_or(0)]).to_ascii_lowercase()
            } else {
                String::new()
            };
            if verb == "rename" {
                Picked::Rename
            } else {
                let info = CMINVOKECOMMANDINFOEX {
                    cbSize: size_of::<CMINVOKECOMMANDINFOEX>() as u32,
                    fMask: CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE | if extended { CMIC_MASK_SHIFT_DOWN } else { 0 },
                    hwnd: win.hwnd,
                    // MAKEINTRESOURCE : la commande est désignée par son numéro
                    lpVerb: PCSTR(offset as *const u8),
                    lpVerbW: PCWSTR(offset as *const u16),
                    nShow: SW_SHOWNORMAL.0,
                    ptInvoke: POINT { x: at.0, y: at.1 },
                    ..Default::default()
                };
                let _ = cm.InvokeCommand(&info as *const CMINVOKECOMMANDINFOEX as *const CMINVOKECOMMANDINFO);
                Picked::Done(verb)
            }
        };
        let _ = DestroyMenu(h);
        picked
    }
}

/// Exécute une commande du shell sur des éléments d'un même dossier, comme si elle venait de leur menu contextuel :
/// « copy », « cut », « delete », « properties », « paste » (sur un dossier : coller dedans)…
/// Peut ouvrir des boîtes de dialogue : à appeler depuis `Cx::after`.
pub fn invoke(win: &Window, paths: &[PathBuf], verb: &str, shift: bool) -> bool {
    unsafe {
        let Some(cm) = ui_object::<IContextMenu>(win.hwnd, paths) else { return false };
        let Ok(h) = CreatePopupMenu() else { return false };
        // le menu doit avoir été rempli une fois pour que les commandes soient prêtes
        let _ = cm.QueryContextMenu(h, 0, SHELL_FIRST, SHELL_LAST, if shift { CMF_EXTENDEDVERBS } else { 0 });
        let (ansi, wide_verb) = (CString::new(verb).unwrap_or_default(), wz(verb));
        let info = CMINVOKECOMMANDINFOEX {
            cbSize: size_of::<CMINVOKECOMMANDINFOEX>() as u32,
            fMask: CMIC_MASK_UNICODE | if shift { CMIC_MASK_SHIFT_DOWN } else { 0 },
            hwnd: win.hwnd,
            lpVerb: PCSTR(ansi.as_ptr() as *const u8),
            lpVerbW: PCWSTR(wide_verb.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        let ok = cm.InvokeCommand(&info as *const CMINVOKECOMMANDINFOEX as *const CMINVOKECOMMANDINFO).is_ok();
        let _ = DestroyMenu(h);
        ok
    }
}

/// Les textes du menu contextuel du shell pour `paths`, sans l'afficher (essais).
#[cfg(feature = "test-hook")]
pub fn shell_menu_labels(win: &Window, paths: &[PathBuf]) -> Vec<String> {
    unsafe {
        let Some(cm) = ui_object::<IContextMenu>(win.hwnd, paths) else { return vec![] };
        let Ok(h) = CreatePopupMenu() else { return vec![] };
        let _ = cm.QueryContextMenu(h, 0, SHELL_FIRST, SHELL_LAST, CMF_CANRENAME);
        let mut all = vec![];
        entries(h, "", &mut all, 2);
        let _ = DestroyMenu(h);
        all.into_iter().map(|(_, t)| t).collect()
    }
}
