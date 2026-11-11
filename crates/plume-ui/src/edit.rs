//! Le champ de saisie : un vrai contrôle EDIT de Windows, sans bordure, posé sur la fenêtre le temps de la saisie
//! (adresse, recherche, renommer). On y gagne tout ce que Windows sait faire : IME, sélection, presse-papiers,
//! annulation, lecteurs d'écran. La fenêtre reçoit `EditChanged`, `EditKey` (Entrée, Échap, Tab, flèches) et `EditBlur`.

use super::geom::{Color, Rect};
use std::cell::Cell;
use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{
            CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DeleteObject, FW_NORMAL, GetDC, GetTextMetricsW, HBRUSH,
            HDC, HFONT, InvalidateRect, OUT_DEFAULT_PRECIS, ReleaseDC, SelectObject, SetBkColor, SetTextColor, TEXTMETRICW,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{GetKeyState, SetFocus, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_F2, VK_F4, VK_RETURN, VK_TAB, VK_UP},
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::{PCWSTR, w},
};

pub(super) const WM_APP_EDIT_KEY: u32 = WM_APP + 2;
pub(super) const WM_APP_EDIT_BLUR: u32 = WM_APP + 3;
const EM_SETSEL: u32 = 0x00B1;
const EM_GETSEL: u32 = 0x00B0;
const EM_REPLACESEL: u32 = 0x00C2;
const EM_SETMARGINS: u32 = 0x00D3;
const EM_SETCUEBANNER: u32 = 0x1501;
const EM_SETPASSWORDCHAR: u32 = 0x00CC;
const ES_AUTOHSCROLL: u32 = 0x0080;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const ES_WANTRETURN: u32 = 0x1000;

pub struct EditStyle {
    pub fg: Color,
    /// Couleur opaque du fond sous le texte (celle du champ dessiné par l'application).
    pub bg: Color,
    /// Taille du texte en DIP.
    pub size: f32,
    /// Sélection à l'ouverture, en caractères UTF-16 : (début, fin). `None` : curseur à la fin.
    pub select: Option<(usize, usize)>,
    /// Texte grisé affiché quand le champ est vide.
    pub placeholder: String,
    /// Mot de passe : les caractères sont remplacés par des points.
    pub password: bool,
    /// Plusieurs lignes (le texte d'un message) : le champ remplit sa zone, Entrée passe à la ligne, et c'est
    /// Ctrl+Entrée que la fenêtre reçoit.
    pub multiline: bool,
}

/// Tout est en `Cell` : Windows rappelle la fenêtre pendant qu'on manipule le contrôle (couleurs, notifications).
pub(super) struct Edit {
    pub hwnd: HWND,
    font: Cell<HFONT>,
    font_px: Cell<i32>,
    brush: Cell<HBRUSH>,
    fg: Cell<Color>,
    bg: Cell<Color>,
    family: Vec<u16>,
    line_h: Cell<i32>,
    multiline: bool,
    /// Le thème (sombre ou clair) donné à sa barre de défilement, pour ne le changer que s'il le faut.
    dark: Cell<Option<bool>>,
    /// Le texte est en train d'être posé par le programme : pas d'événement « modifié ».
    pub quiet: Cell<bool>,
}

fn wz(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

unsafe extern "system" fn edit_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, multiline: usize) -> LRESULT {
    unsafe {
        let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
        let multiline = multiline != 0;
        match msg {
            WM_KEYDOWN => {
                let vk = wp.0 as u16;
                // sur plusieurs lignes, Entrée et les flèches servent au texte : seuls Échap, Tab et Ctrl+Entrée sortent
                let leaves = if multiline {
                    vk == VK_ESCAPE.0 || vk == VK_TAB.0 || (vk == VK_RETURN.0 && ctrl)
                } else {
                    [VK_RETURN.0, VK_ESCAPE.0, VK_TAB.0, VK_UP.0, VK_DOWN.0, VK_F2.0, VK_F4.0].contains(&vk)
                };
                if leaves {
                    if let Ok(parent) = GetParent(h) {
                        SendMessageW(parent, WM_APP_EDIT_KEY, Some(wp), Some(LPARAM(0)));
                    }
                    return LRESULT(0);
                }
                if ctrl && vk == b'A' as u16 {
                    SendMessageW(h, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
                    return LRESULT(0);
                }
            }
            WM_CHAR => match wp.0 {
                // Entrée dans un texte de plusieurs lignes : une nouvelle ligne, que le contrôle ajoute lui-même
                0x0D if multiline => {}
                // Entrée, Échap, Tab, Ctrl+A, Ctrl+Entrée : déjà traités, et pas de « bip »
                0x0D | 0x1B | 0x09 | 0x01 | 0x0A => return LRESULT(0),
                // Ctrl+Retour arrière : efface le mot précédent
                0x7F => {
                    let (mut a, mut b) = (0u32, 0u32);
                    SendMessageW(h, EM_GETSEL, Some(WPARAM(&mut a as *mut u32 as usize)), Some(LPARAM(&mut b as *mut u32 as isize)));
                    if a == b {
                        let mut buf = vec![0u16; GetWindowTextLengthW(h) as usize + 1];
                        let n = GetWindowTextW(h, &mut buf) as usize;
                        let text = &buf[..n.min(a as usize)];
                        let is_sep = |c: u16| c == b' ' as u16 || c == b'\\' as u16 || c == b'/' as u16 || c == b'.' as u16;
                        let mut i = text.len();
                        while i > 0 && is_sep(text[i - 1]) {
                            i -= 1;
                        }
                        while i > 0 && !is_sep(text[i - 1]) {
                            i -= 1;
                        }
                        SendMessageW(h, EM_SETSEL, Some(WPARAM(i)), Some(LPARAM(a as isize)));
                    }
                    SendMessageW(h, EM_REPLACESEL, Some(WPARAM(1)), Some(LPARAM(w!("").as_ptr() as isize)));
                    return LRESULT(0);
                }
                _ => {}
            },
            WM_KILLFOCUS => {
                if let Ok(parent) = GetParent(h) {
                    let _ = PostMessageW(Some(parent), WM_APP_EDIT_BLUR, WPARAM(0), LPARAM(0));
                }
            }
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(h, Some(edit_proc), 1);
            }
            _ => {}
        }
        DefSubclassProc(h, msg, wp, lp)
    }
}

impl Edit {
    pub fn create(parent: HWND, family: &str, multiline: bool) -> Option<Edit> {
        unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            let style = if multiline { WS_VSCROLL | WINDOW_STYLE(ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) } else { WINDOW_STYLE(ES_AUTOHSCROLL) };
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                PCWSTR::null(),
                WS_CHILD | style,
                0,
                0,
                10,
                10,
                Some(parent),
                None,
                Some(instance.into()),
                None,
            )
            .ok()?;
            let _ = SetWindowSubclass(hwnd, Some(edit_proc), 1, multiline as usize);
            SendMessageW(hwnd, EM_SETMARGINS, Some(WPARAM(3)), Some(LPARAM(0)));
            Some(Edit {
                hwnd,
                font: Cell::new(HFONT::default()),
                font_px: Cell::new(0),
                brush: Cell::new(HBRUSH::default()),
                fg: Cell::new(Color::hex(0xffffff)),
                bg: Cell::new(Color::hex(0)),
                family: wz(family),
                line_h: Cell::new(18),
                multiline,
                dark: Cell::new(None),
                quiet: Cell::new(false),
            })
        }
    }

    fn set_font(&self, px: i32) {
        if px == self.font_px.get() && !self.font.get().is_invalid() {
            return;
        }
        unsafe {
            let font = CreateFontW(
                -px,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                PCWSTR(self.family.as_ptr()),
            );
            SendMessageW(self.hwnd, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
            let old = self.font.replace(font);
            if !old.is_invalid() {
                let _ = DeleteObject(old.into());
            }
            self.font_px.set(px);
            // hauteur d'une ligne de texte : le contrôle fait exactement cette hauteur, centré dans le champ
            let dc = GetDC(Some(self.hwnd));
            let prev = SelectObject(dc, font.into());
            let mut tm = TEXTMETRICW::default();
            let _ = GetTextMetricsW(dc, &mut tm);
            SelectObject(dc, prev);
            ReleaseDC(Some(self.hwnd), dc);
            self.line_h.set(tm.tmHeight.max(px));
        }
    }

    /// Place le contrôle : `r` est la zone de texte du champ, en DIP.
    pub fn place(&self, r: Rect, scale: f32) {
        if self.multiline {
            // plusieurs lignes : le contrôle remplit toute sa zone
            let px = |v: f32| (v * scale).round() as i32;
            unsafe {
                let _ = SetWindowPos(self.hwnd, None, px(r.x), px(r.y), px(r.w).max(10), px(r.h).max(10), SWP_NOZORDER | SWP_NOACTIVATE);
            }
            return;
        }
        let (x, w) = ((r.x * scale).round() as i32, (r.w * scale).round() as i32);
        let y = ((r.y + r.h / 2.0) * scale - self.line_h.get() as f32 / 2.0).round() as i32;
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, x, y, w.max(10), self.line_h.get(), SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }

    pub fn show(&self, r: Rect, scale: f32, text: &str, st: &EditStyle) {
        self.set_font((st.size * scale).round() as i32);
        if st.bg != self.bg.get() || self.brush.get().is_invalid() {
            unsafe {
                let old = self.brush.replace(CreateSolidBrush(COLORREF(st.bg.colorref())));
                if !old.is_invalid() {
                    let _ = DeleteObject(old.into());
                }
            }
        }
        self.fg.set(st.fg);
        self.bg.set(st.bg);
        // la barre de défilement d'un texte de plusieurs lignes suit le thème : sombre sur un fond sombre
        let dark = st.bg.r * 0.299 + st.bg.g * 0.587 + st.bg.b * 0.114 < 0.5;
        if self.multiline && self.dark.replace(Some(dark)) != Some(dark) {
            unsafe {
                let _ = windows::Win32::UI::Controls::SetWindowTheme(self.hwnd, if dark { w!("DarkMode_Explorer") } else { w!("Explorer") }, PCWSTR::null());
            }
        }
        self.place(r, scale);
        unsafe {
            self.quiet.set(true);
            // un texte de plusieurs lignes s'écrit avec les fins de ligne de Windows
            let shown = if self.multiline { text.replace("\r\n", "\n").replace('\n', "\r\n") } else { text.to_string() };
            let text = shown.as_str();
            let _ = SetWindowTextW(self.hwnd, PCWSTR(wz(text).as_ptr()));
            let ph = wz(&st.placeholder);
            SendMessageW(self.hwnd, EM_SETCUEBANNER, Some(WPARAM(1)), Some(LPARAM(ph.as_ptr() as isize)));
            SendMessageW(self.hwnd, EM_SETPASSWORDCHAR, Some(WPARAM(if st.password { 0x25CF } else { 0 })), Some(LPARAM(0)));
            self.quiet.set(false);
            let end = text.encode_utf16().count();
            let (a, b) = st.select.unwrap_or((end, end));
            SendMessageW(self.hwnd, EM_SETSEL, Some(WPARAM(a.min(end))), Some(LPARAM(b.min(end) as isize)));
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = InvalidateRect(Some(self.hwnd), None, true);
            let _ = SetFocus(Some(self.hwnd));
        }
    }

    /// Remplace le texte sans prévenir l'application (elle vient de le décider), curseur à la fin.
    pub fn set_text(&self, text: &str) {
        // un texte de plusieurs lignes s'écrit avec les fins de ligne de Windows
        let shown = if self.multiline { text.replace("\r\n", "\n").replace('\n', "\r\n") } else { text.to_string() };
        let text = shown.as_str();
        unsafe {
            self.quiet.set(true);
            let _ = SetWindowTextW(self.hwnd, PCWSTR(wz(text).as_ptr()));
            self.quiet.set(false);
            let end = text.encode_utf16().count();
            SendMessageW(self.hwnd, EM_SETSEL, Some(WPARAM(end)), Some(LPARAM(end as isize)));
        }
    }

    /// Cache (points) ou montre le texte saisi : champ de mot de passe.
    pub fn mask(&self, on: bool) {
        unsafe {
            SendMessageW(self.hwnd, EM_SETPASSWORDCHAR, Some(WPARAM(if on { 0x25CF } else { 0 })), Some(LPARAM(0)));
            let _ = InvalidateRect(Some(self.hwnd), None, true);
        }
    }

    /// Redonne le clavier au champ.
    pub fn focus(&self) {
        unsafe {
            let _ = SetFocus(Some(self.hwnd));
        }
    }

    pub fn hide(&self) {
        unsafe {
            if IsWindowVisible(self.hwnd).as_bool() {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }

    pub fn visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd) }.as_bool()
    }

    pub fn text(&self) -> String {
        unsafe {
            let mut buf = vec![0u16; GetWindowTextLengthW(self.hwnd) as usize + 1];
            let n = GetWindowTextW(self.hwnd, &mut buf) as usize;
            let text = String::from_utf16_lossy(&buf[..n]);
            if self.multiline { text.replace("\r\n", "\n") } else { text }
        }
    }

    /// Couleurs du contrôle, demandées par Windows avant chaque dessin (WM_CTLCOLOREDIT).
    pub fn colors(&self, dc: HDC) -> LRESULT {
        unsafe {
            SetTextColor(dc, COLORREF(self.fg.get().colorref()));
            SetBkColor(dc, COLORREF(self.bg.get().colorref()));
        }
        LRESULT(self.brush.get().0 as isize)
    }
}

impl Drop for Edit {
    fn drop(&mut self) {
        unsafe {
            if !self.font.get().is_invalid() {
                let _ = DeleteObject(self.font.get().into());
            }
            if !self.brush.get().is_invalid() {
                let _ = DeleteObject(self.brush.get().into());
            }
        }
    }
}
