//! Une fenêtre Plume native : pas de barre de titre Windows (l'application dessine la sienne), mais le vrai cadre
//! du système, donc l'ombre, les coins arrondis de Windows 11, le redimensionnement et l'ancrage aux bords.
//!
//! L'application implémente [`Handler`] : elle dessine la fenêtre entière à partir de son état et reçoit les
//! événements. Ce qui ouvre une boucle modale (menu, glisser-déposer, boîte de dialogue) se lance par
//! [`Cx::after`], une fois l'application relâchée : Windows peut alors redessiner la fenêtre pendant ce temps.

use super::{
    canvas::Canvas,
    edit::{Edit, EditStyle, WM_APP_EDIT_BLUR, WM_APP_EDIT_KEY},
    geom::{Color, Pt, Rect, pt},
    gfx::Gfx,
};
use std::{
    any::Any,
    cell::{Cell, OnceCell, RefCell},
    rc::Rc,
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmFlush, DwmSetWindowAttribute},
            Gdi::{CreateSolidBrush, HDC, InvalidateRect, ScreenToClient, UpdateWindow, ValidateRect},
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::WM_MOUSELEAVE,
            HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
            Input::KeyboardAndMouse::{
                GetFocus, GetKeyState, ReleaseCapture, SetCapture, SetFocus, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent, VK_CONTROL, VK_MENU,
                VK_SHIFT,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{Result, w},
};

/// Message interne : une tâche envoyée par un autre fil (lecture d'un dossier terminée, icône prête…).
const WM_APP_TASK: u32 = WM_APP + 1;
/// Marque des textes échangés entre deux lancements du programme (WM_COPYDATA).
const COPY_TEXT: usize = 0x504C_464D;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
    /// Boutons latéraux de la souris.
    Back,
    Forward,
}

#[derive(Debug)]
pub enum Event {
    MouseMove {
        p: Pt,
        mods: Mods,
    },
    /// `clicks` vaut 2 pour un double-clic.
    MouseDown {
        p: Pt,
        button: Button,
        clicks: u8,
        mods: Mods,
    },
    MouseUp {
        p: Pt,
        button: Button,
        mods: Mods,
    },
    MouseLeave,
    /// Molette, en crans (positif : vers le haut ou vers la droite).
    Wheel {
        p: Pt,
        dx: f32,
        dy: f32,
        mods: Mods,
    },
    /// Touche enfoncée (code de touche virtuelle Windows).
    Key {
        vk: u16,
        mods: Mods,
        repeat: bool,
    },
    Char(char),
    /// La fenêtre devient active ou inactive.
    Active(bool),
    Resized,
    /// L'échelle de l'écran a changé (autre moniteur, réglage Windows).
    Scale,
    /// Windows a changé de thème clair / sombre.
    SystemTheme,
    Timer(usize),
    /// Un lecteur a été branché ou retiré.
    Devices,
    /// Texte envoyé par un autre lancement du programme (sa ligne de commande) : voir [`Window::send_text`].
    Opened(String),
    /// Le texte du champ de saisie a changé (frappe, coller, effacer).
    EditChanged(String),
    /// Touche que le champ de saisie laisse à l'application : Entrée, Échap, Tab, flèches haut et bas, F2, F4.
    EditKey {
        vk: u16,
        mods: Mods,
    },
    /// Le champ de saisie a perdu le focus (clic ailleurs, autre fenêtre).
    EditBlur,
    /// La fenêtre va se fermer.
    Close,
}

/// Ce que le pointeur survole, pour Windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    /// Zone vide de la barre de titre : on y attrape la fenêtre.
    Caption,
    /// Bouton ou onglet de la barre de titre.
    Control,
    /// Le bouton « agrandir » : Windows 11 y montre ses dispositions d'ancrage au survol.
    MaxButton,
    Client,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Arrow,
    Hand,
    SizeWE,
    SizeNS,
}

pub struct PaintCx {
    /// Taille de la fenêtre en DIP.
    pub w: f32,
    pub h: f32,
    pub active: bool,
    pub maximized: bool,
    /// À mettre à vrai pour redessiner à l'image suivante (animation en cours).
    pub animate: bool,
}

pub trait Handler: 'static {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx);
    fn event(&mut self, ev: Event, cx: &mut Cx);
    fn area(&self, p: Pt) -> Area;
    fn cursor(&self, _p: Pt) -> Cursor {
        Cursor::Arrow
    }
    fn as_any(&mut self) -> &mut dyn Any;
}

type After = Box<dyn FnOnce(&Window)>;
type Task = Box<dyn FnOnce(&mut dyn Handler, &mut Cx) + Send>;

/// Ce qu'un événement peut demander à la fenêtre.
pub struct Cx {
    pub window: Window,
    pub scale: f32,
    repaint: bool,
    handled: bool,
    capture: Option<bool>,
    timers: Vec<(usize, Option<u32>)>,
    after: Vec<After>,
}

impl Cx {
    fn new(hwnd: HWND, scale: f32) -> Cx {
        Cx { window: Window { hwnd }, scale, repaint: false, handled: false, capture: None, timers: vec![], after: vec![] }
    }
    /// Redessiner la fenêtre.
    pub fn repaint(&mut self) {
        self.repaint = true;
    }
    /// La touche a été traitée (Windows ne fera rien de plus avec).
    pub fn handled(&mut self) {
        self.handled = true;
    }
    /// Garder (ou rendre) la souris : les déplacements continuent d'arriver hors de la fenêtre.
    pub fn capture(&mut self, on: bool) {
        self.capture = Some(on);
    }
    /// Minuteur répétitif (`None` l'arrête).
    pub fn timer(&mut self, id: usize, ms: Option<u32>) {
        self.timers.push((id, ms));
    }
    /// À faire une fois l'événement traité : tout ce qui bloque (menu, glisser-déposer, dialogue).
    pub fn after(&mut self, f: impl FnOnce(&Window) + 'static) {
        self.after.push(Box::new(f));
    }
    pub fn sender(&self) -> Sender {
        self.window.sender()
    }
}

/// Poignée d'une fenêtre (copiable, valable sur le fil de l'interface).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub hwnd: HWND,
}

/// De quoi envoyer du travail terminé au fil de l'interface depuis un autre fil.
#[derive(Clone, Copy, Debug)]
pub struct Sender(isize);
// SAFETY: un HWND est un simple identifiant ; PostMessage est utilisable depuis n'importe quel fil
unsafe impl Send for Sender {}
unsafe impl Sync for Sender {}

impl Sender {
    /// Exécute `f` sur le fil de l'interface. Faux si la fenêtre n'existe plus.
    pub fn send(&self, f: impl FnOnce(&mut dyn Handler, &mut Cx) + Send + 'static) -> bool {
        let task: Box<Task> = Box::new(Box::new(f));
        let raw = Box::into_raw(task);
        let ok = unsafe { PostMessageW(Some(HWND(self.0 as _)), WM_APP_TASK, WPARAM(0), LPARAM(raw as isize)) }.is_ok();
        if !ok {
            drop(unsafe { Box::from_raw(raw) });
        }
        ok
    }
}

/// Regard sur les messages de la fenêtre, le temps d'un menu du shell (il dessine lui-même certains sous-menus).
pub type Hook = Rc<dyn Fn(u32, WPARAM, LPARAM) -> Option<LRESULT>>;

struct State {
    handler: RefCell<Box<dyn Handler>>,
    canvas: RefCell<Canvas>,
    min: (f32, f32),
    tracking: Cell<bool>,
    /// Comme `tracking`, pour la souris sur le bouton « agrandir » (zone non cliente pour Windows).
    tracking_nc: Cell<bool>,
    active: Cell<bool>,
    /// Première moitié d'une paire de substitution UTF-16 reçue par WM_CHAR.
    high: Cell<u16>,
    /// Alt+touche traité : le WM_SYSCHAR qui suit ne doit pas faire sonner Windows.
    mute_syschar: Cell<bool>,
    /// Le champ de saisie, créé à la première utilisation ; et celui de plusieurs lignes, de même.
    edit: OnceCell<Option<Edit>>,
    edit_multi: OnceCell<Option<Edit>>,
    /// Le champ ouvert en dernier est celui de plusieurs lignes.
    multi: Cell<bool>,
    font: &'static str,
    hook: RefCell<Option<Hook>>,
}

impl State {
    /// Le champ de saisie en service : celui d'une ligne, ou celui de plusieurs.
    fn edit(&self) -> Option<&Edit> {
        if self.multi.get() { &self.edit_multi } else { &self.edit }.get().and_then(|e| e.as_ref())
    }
}

thread_local! {
    static WINDOWS: Cell<usize> = const { Cell::new(0) };
}

fn state<'a>(hwnd: HWND) -> Option<&'a State> {
    let p = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const State;
    // SAFETY: le pointeur est posé par `create` et retiré dans WM_NCDESTROY, toujours sur ce fil
    unsafe { p.as_ref() }
}

fn scale_of(hwnd: HWND) -> f32 {
    unsafe { GetDpiForWindow(hwnd) as f32 / 96.0 }
}

fn mods() -> Mods {
    let down = |vk: u16| unsafe { GetKeyState(vk as i32) } < 0;
    Mods { ctrl: down(VK_CONTROL.0), shift: down(VK_SHIFT.0), alt: down(VK_MENU.0) }
}

/// Position du pointeur (coordonnées client en pixels dans LPARAM) en DIP.
fn client_pt(hwnd: HWND, lp: LPARAM) -> Pt {
    let s = scale_of(hwnd);
    pt((lp.0 & 0xffff) as i16 as f32 / s, ((lp.0 >> 16) & 0xffff) as i16 as f32 / s)
}

/// Position écran (pixels dans LPARAM) en DIP dans la fenêtre.
fn screen_pt(hwnd: HWND, lp: LPARAM) -> Pt {
    let mut p = POINT { x: (lp.0 & 0xffff) as i16 as i32, y: ((lp.0 >> 16) & 0xffff) as i16 as i32 };
    unsafe {
        let _ = ScreenToClient(hwnd, &mut p);
    }
    let s = scale_of(hwnd);
    pt(p.x as f32 / s, p.y as f32 / s)
}

impl Window {
    pub fn sender(&self) -> Sender {
        Sender(self.hwnd.0 as isize)
    }

    /// Donne la main à l'application (hors de tout événement en cours). None si elle est déjà occupée.
    pub fn with<R>(&self, f: impl FnOnce(&mut dyn Handler, &mut Cx) -> R) -> Option<R> {
        let st = state(self.hwnd)?;
        let mut cx = Cx::new(self.hwnd, scale_of(self.hwnd));
        let r = {
            let mut h = st.handler.try_borrow_mut().ok()?;
            f(h.as_mut(), &mut cx)
        };
        finish(self.hwnd, cx);
        Some(r)
    }

    pub fn repaint(&self) {
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    pub fn is_maximized(&self) -> bool {
        unsafe { IsZoomed(self.hwnd) }.as_bool()
    }

    pub fn minimize(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_MINIMIZE);
        }
    }

    pub fn toggle_maximize(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, if self.is_maximized() { SW_RESTORE } else { SW_MAXIMIZE });
        }
    }

    pub fn close(&self) {
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }

    pub fn set_title(&self, title: &str) {
        let t: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let _ = SetWindowTextW(self.hwnd, windows::core::PCWSTR(t.as_ptr()));
        }
    }

    /// Cadre et ombre de Windows en sombre ou en clair.
    pub fn set_dark(&self, dark: bool) {
        let v: i32 = dark as i32;
        unsafe {
            let _ = DwmSetWindowAttribute(self.hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &v as *const i32 as _, 4);
        }
    }

    /// Point de la fenêtre (DIP) en coordonnées écran (pixels), pour placer un menu.
    pub fn to_screen(&self, p: Pt) -> (i32, i32) {
        let s = scale_of(self.hwnd);
        let mut q = POINT { x: (p.x * s).round() as i32, y: (p.y * s).round() as i32 };
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::ClientToScreen(self.hwnd, &mut q);
        }
        (q.x, q.y)
    }

    pub fn scale(&self) -> f32 {
        scale_of(self.hwnd)
    }

    /// Point de l'écran (pixels) en coordonnées de la fenêtre (DIP).
    pub fn from_screen(&self, x: i32, y: i32) -> Pt {
        let mut p = POINT { x, y };
        unsafe {
            let _ = ScreenToClient(self.hwnd, &mut p);
        }
        let s = scale_of(self.hwnd);
        pt(p.x as f32 / s, p.y as f32 / s)
    }

    /// Position à retrouver au prochain lancement.
    pub fn placement(&self) -> Option<Placement> {
        let mut wp = WINDOWPLACEMENT { length: size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
        unsafe { GetWindowPlacement(self.hwnd, &mut wp) }.ok()?;
        let r = wp.rcNormalPosition;
        Some(Placement { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top, maximized: wp.showCmd == SW_SHOWMAXIMIZED.0 as u32 })
    }

    pub fn is_active(&self) -> bool {
        unsafe { GetForegroundWindow() == self.hwnd }
    }

    pub fn is_minimized(&self) -> bool {
        unsafe { IsIconic(self.hwnd) }.as_bool()
    }

    /// Minuteur répétitif de la fenêtre (`None` l'arrête) ; il arrive par `Event::Timer`.
    pub fn timer(&self, id: usize, ms: Option<u32>) {
        unsafe {
            match ms {
                Some(ms) => {
                    SetTimer(Some(self.hwnd), id, ms, None);
                }
                None => {
                    let _ = KillTimer(Some(self.hwnd), id);
                }
            }
        }
    }

    /// Passe la fenêtre au premier plan (restaurée si elle était réduite).
    pub fn bring_to_front(&self) {
        unsafe {
            if IsIconic(self.hwnd).as_bool() {
                let _ = ShowWindow(self.hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(self.hwnd);
        }
    }

    /// Ouvre le champ de saisie sur `r` (zone de texte, en DIP) et lui donne le clavier.
    pub fn edit_show(&self, r: Rect, text: &str, style: &EditStyle) {
        let Some(st) = state(self.hwnd) else { return };
        // un seul champ à la fois : l'autre se ferme
        if st.multi.get() != style.multiline
            && let Some(other) = st.edit()
        {
            other.hide();
        }
        st.multi.set(style.multiline);
        let cell = if style.multiline { &st.edit_multi } else { &st.edit };
        if let Some(e) = cell.get_or_init(|| Edit::create(self.hwnd, st.font, style.multiline)) {
            e.show(r, scale_of(self.hwnd), text, style);
        }
    }

    /// Déplace le champ de saisie ouvert (la fenêtre a changé de taille).
    pub fn edit_place(&self, r: Rect) {
        if let Some(e) = state(self.hwnd).and_then(|st| st.edit()) {
            e.place(r, scale_of(self.hwnd));
        }
    }

    /// Ferme le champ de saisie et rend le clavier à la fenêtre.
    pub fn edit_hide(&self) {
        if let Some(e) = state(self.hwnd).and_then(|st| st.edit()) {
            unsafe {
                if GetFocus() == e.hwnd {
                    let _ = SetFocus(Some(self.hwnd));
                }
            }
            e.hide();
        }
    }

    pub fn edit_visible(&self) -> bool {
        state(self.hwnd).and_then(|st| st.edit()).is_some_and(|e| e.visible())
    }

    /// Texte du champ de saisie ouvert.
    pub fn edit_text(&self) -> Option<String> {
        state(self.hwnd).and_then(|st| st.edit()).filter(|e| e.visible()).map(|e| e.text())
    }

    /// Cache ou montre le texte du champ de saisie ouvert (mot de passe).
    pub fn edit_mask(&self, on: bool) {
        if let Some(e) = state(self.hwnd).and_then(|st| st.edit()) {
            e.mask(on);
        }
    }

    /// Redonne le clavier au champ de saisie ouvert.
    pub fn edit_focus(&self) {
        if let Some(e) = state(self.hwnd).and_then(|st| st.edit()).filter(|e| e.visible()) {
            e.focus();
        }
    }

    /// Remplace le texte du champ de saisie ouvert, sans événement `EditChanged`.
    pub fn edit_set_text(&self, text: &str) {
        if let Some(e) = state(self.hwnd).and_then(|st| st.edit()) {
            e.set_text(text);
        }
    }

    /// Place un texte dans le presse-papiers. Faux si Windows n'a pas voulu.
    pub fn copy_text(&self, text: &str) -> bool {
        use windows::Win32::{
            Foundation::HANDLE,
            System::{
                DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
                Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            },
        };
        let data: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        unsafe {
            if OpenClipboard(Some(self.hwnd)).is_err() {
                return false;
            }
            let _ = EmptyClipboard();
            let mut ok = false;
            if let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2) {
                let p = GlobalLock(mem) as *mut u16;
                if !p.is_null() {
                    std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
                    let _ = GlobalUnlock(mem);
                    // 13 : texte Unicode ; le presse-papiers devient propriétaire de la mémoire
                    ok = SetClipboardData(13, Some(HANDLE(mem.0))).is_ok();
                }
            }
            let _ = CloseClipboard();
            ok
        }
    }

    /// Ouvre un fichier ou une adresse web avec l'application que Windows lui associe.
    pub fn open(&self, target: &str) -> bool {
        use windows::Win32::UI::Shell::ShellExecuteW;
        let wide: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
        unsafe { ShellExecuteW(Some(self.hwnd), w!("open"), windows::core::PCWSTR(wide.as_ptr()), None, None, SW_SHOWNORMAL).0 as usize > 32 }
    }

    /// Marque la fenêtre d'un nom, pour qu'un autre lancement du programme la retrouve ([`find`]).
    pub fn mark(&self, id: &str) {
        let name: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let _ = SetPropW(self.hwnd, windows::core::PCWSTR(name.as_ptr()), Some(windows::Win32::Foundation::HANDLE(1 as _)));
        }
    }

    /// Envoie un texte à cette fenêtre (d'un autre processus) : elle le reçoit par `Event::Opened`.
    /// Lui donne aussi le droit de passer au premier plan. Faux si elle ne répond pas dans les cinq secondes.
    pub fn send_text(&self, text: &str) -> bool {
        use windows::Win32::System::DataExchange::COPYDATASTRUCT;
        let data = COPYDATASTRUCT { dwData: COPY_TEXT, cbData: text.len() as u32, lpData: text.as_ptr() as *mut _ };
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(self.hwnd, Some(&mut pid));
            let _ = AllowSetForegroundWindow(pid);
            let mut result = 0usize;
            let sent = SendMessageTimeoutW(
                self.hwnd,
                WM_COPYDATA,
                WPARAM(0),
                LPARAM(&data as *const COPYDATASTRUCT as isize),
                SMTO_ABORTIFHUNG,
                5000,
                Some(&mut result),
            );
            sent.0 != 0 && result == 1
        }
    }

    /// Installe (ou retire) un regard sur les messages de la fenêtre.
    pub fn set_hook(&self, hook: Option<Hook>) {
        if let Some(st) = state(self.hwnd) {
            *st.hook.borrow_mut() = hook;
        }
    }
}

/// Applique ce que l'événement a demandé. Renvoie vrai si l'événement a été traité.
fn finish(hwnd: HWND, cx: Cx) -> bool {
    unsafe {
        match cx.capture {
            Some(true) => {
                SetCapture(hwnd);
            }
            Some(false) => {
                let _ = ReleaseCapture();
            }
            None => {}
        }
        for (id, ms) in cx.timers {
            match ms {
                Some(ms) => {
                    SetTimer(Some(hwnd), id, ms, None);
                }
                None => {
                    let _ = KillTimer(Some(hwnd), id);
                }
            }
        }
        if cx.repaint {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
    let win = Window { hwnd };
    for f in cx.after {
        f(&win);
    }
    cx.handled
}

fn dispatch(hwnd: HWND, st: &State, ev: Event) -> bool {
    let mut cx = Cx::new(hwnd, scale_of(hwnd));
    match st.handler.try_borrow_mut() {
        Ok(mut h) => h.event(ev, &mut cx),
        Err(_) => return false, // réentrance : l'application traite déjà un événement
    }
    finish(hwnd, cx)
}

fn paint(hwnd: HWND, st: &State) {
    unsafe {
        let _ = ValidateRect(Some(hwnd), None);
    }
    let animate = {
        let (Ok(mut h), Ok(mut c)) = (st.handler.try_borrow_mut(), st.canvas.try_borrow_mut()) else { return };
        let (w, hgt) = c.size();
        let mut cx = PaintCx { w, h: hgt, active: st.active.get(), maximized: unsafe { IsZoomed(hwnd) }.as_bool(), animate: false };
        c.begin();
        h.paint(&mut c, &mut cx);
        c.end();
        cx.animate
    };
    if animate {
        unsafe {
            // attend l'image suivante de l'écran : l'animation suit la fréquence d'affichage sans occuper le processeur
            let _ = DwmFlush();
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

// Un nom de message mal importé deviendrait un motif attrape-tout : le compilateur doit le refuser.
#[deny(unreachable_patterns, unused_variables)]
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(st) = state(hwnd) else { return unsafe { DefWindowProcW(hwnd, msg, wp, lp) } };
    let hook = st.hook.try_borrow().ok().and_then(|h| h.clone());
    if let Some(r) = hook.and_then(|h| h(msg, wp, lp)) {
        return r;
    }
    unsafe {
        match msg {
            // notification du champ de saisie : son texte a changé
            WM_COMMAND => {
                if let Some(e) = st.edit()
                    && lp.0 == e.hwnd.0 as isize
                    && ((wp.0 >> 16) & 0xffff) as u32 == EN_CHANGE
                    && !e.quiet.get()
                {
                    dispatch(hwnd, st, Event::EditChanged(e.text()));
                }
                LRESULT(0)
            }
            WM_CTLCOLOREDIT => match st.edit() {
                Some(e) if lp.0 == e.hwnd.0 as isize => e.colors(HDC(wp.0 as _)),
                _ => DefWindowProcW(hwnd, msg, wp, lp),
            },
            WM_APP_EDIT_KEY => {
                dispatch(hwnd, st, Event::EditKey { vk: wp.0 as u16, mods: mods() });
                LRESULT(0)
            }
            WM_APP_EDIT_BLUR => {
                if let Some(e) = st.edit()
                    && e.visible()
                    && GetFocus() != e.hwnd
                {
                    dispatch(hwnd, st, Event::EditBlur);
                }
                LRESULT(0)
            }
            // La zone cliente couvre aussi l'ancienne barre de titre ; les bords gauche, droit et bas restent à Windows.
            WM_NCCALCSIZE if wp.0 != 0 => {
                let params = &mut *(lp.0 as *mut NCCALCSIZE_PARAMS);
                let top = params.rgrc[0].top;
                let r = DefWindowProcW(hwnd, msg, wp, lp);
                if r.0 != 0 {
                    return r;
                }
                params.rgrc[0].top = top;
                if IsZoomed(hwnd).as_bool() {
                    // agrandie, la fenêtre déborde de l'écran de l'épaisseur du cadre
                    let dpi = GetDpiForWindow(hwnd);
                    params.rgrc[0].top += GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
                }
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let r = DefWindowProcW(hwnd, msg, wp, lp);
                if r.0 != HTCLIENT as isize {
                    return r;
                }
                let p = screen_pt(hwnd, lp);
                let area = st.handler.try_borrow().map_or(Area::Client, |h| h.area(p));
                if !IsZoomed(hwnd).as_bool() && p.y < 5.0 && !matches!(area, Area::Control | Area::MaxButton) {
                    // bord haut : redimensionner
                    let w = st.canvas.try_borrow().map_or(f32::MAX, |c| c.size().0);
                    let corner = match (p.x < 10.0, p.x > w - 10.0) {
                        (true, _) => HTTOPLEFT,
                        (false, true) => HTTOPRIGHT,
                        (false, false) => HTTOP,
                    };
                    return LRESULT(corner as isize);
                }
                LRESULT(match area {
                    Area::Caption => HTCAPTION,
                    Area::MaxButton => HTMAXBUTTON,
                    _ => HTCLIENT,
                } as isize)
            }
            // Sur le bouton « agrandir », la souris arrive par les messages de la zone non cliente : on les traite
            // comme les autres, sans laisser Windows y dessiner son vieux bouton.
            WM_NCMOUSEMOVE if wp.0 == HTMAXBUTTON as usize => {
                if !st.tracking_nc.replace(true) {
                    let mut t =
                        TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE | TME_NONCLIENT, hwndTrack: hwnd, dwHoverTime: 0 };
                    let _ = TrackMouseEvent(&mut t);
                }
                dispatch(hwnd, st, Event::MouseMove { p: screen_pt(hwnd, lp), mods: mods() });
                LRESULT(0)
            }
            WM_NCMOUSELEAVE => {
                if st.tracking_nc.replace(false) {
                    dispatch(hwnd, st, Event::MouseLeave);
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if wp.0 == HTMAXBUTTON as usize => {
                // la suite du geste (le relâchement) arrive ensuite comme pour n'importe quel bouton
                SetCapture(hwnd);
                dispatch(hwnd, st, Event::MouseDown { p: screen_pt(hwnd, lp), button: Button::Left, clicks: 1, mods: mods() });
                LRESULT(0)
            }
            WM_NCLBUTTONUP if wp.0 == HTMAXBUTTON as usize => {
                let _ = ReleaseCapture();
                dispatch(hwnd, st, Event::MouseUp { p: screen_pt(hwnd, lp), button: Button::Left, mods: mods() });
                LRESULT(0)
            }
            // pas de vieille barre de titre redessinée par Windows quand la fenêtre change d'état
            WM_NCACTIVATE => DefWindowProcW(hwnd, msg, wp, LPARAM(-1)),
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                paint(hwnd, st);
                LRESULT(0)
            }
            WM_SIZE => {
                if let Ok(mut c) = st.canvas.try_borrow_mut() {
                    c.resize();
                }
                dispatch(hwnd, st, Event::Resized);
                let _ = InvalidateRect(Some(hwnd), None, false);
                // pendant un redimensionnement, dessiner tout de suite : la fenêtre suit la souris sans bandes vides
                if wp.0 as u32 != SIZE_MINIMIZED {
                    paint(hwnd, st);
                }
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let rc = &*(lp.0 as *const RECT);
                if let Ok(mut c) = st.canvas.try_borrow_mut() {
                    c.set_dpi((wp.0 & 0xffff) as u32);
                }
                let _ = SetWindowPos(hwnd, None, rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top, SWP_NOZORDER | SWP_NOACTIVATE);
                dispatch(hwnd, st, Event::Scale);
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(lp.0 as *mut MINMAXINFO);
                let s = scale_of(hwnd);
                info.ptMinTrackSize = POINT { x: (st.min.0 * s) as i32, y: (st.min.1 * s) as i32 };
                LRESULT(0)
            }
            WM_ACTIVATE => {
                let on = (wp.0 & 0xffff) as u32 != WA_INACTIVE;
                st.active.set(on);
                dispatch(hwnd, st, Event::Active(on));
                let _ = InvalidateRect(Some(hwnd), None, false);
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_MOUSEMOVE => {
                if !st.tracking.replace(true) {
                    let mut t = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    let _ = TrackMouseEvent(&mut t);
                }
                dispatch(hwnd, st, Event::MouseMove { p: client_pt(hwnd, lp), mods: mods() });
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                st.tracking.set(false);
                dispatch(hwnd, st, Event::MouseLeave);
                LRESULT(0)
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN | WM_RBUTTONDBLCLK | WM_MBUTTONDOWN | WM_MBUTTONDBLCLK | WM_XBUTTONDOWN | WM_XBUTTONDBLCLK => {
                let button = match msg {
                    WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => Button::Left,
                    WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => Button::Right,
                    WM_MBUTTONDOWN | WM_MBUTTONDBLCLK => Button::Middle,
                    _ if (wp.0 >> 16) & 0xffff == 1 => Button::Back,
                    _ => Button::Forward,
                };
                let clicks = if matches!(msg, WM_LBUTTONDBLCLK | WM_RBUTTONDBLCLK | WM_MBUTTONDBLCLK | WM_XBUTTONDBLCLK) { 2 } else { 1 };
                // un clic dans la fenêtre reprend le clavier au champ de saisie (qui prévient alors l'application)
                if GetFocus() != hwnd {
                    let _ = SetFocus(Some(hwnd));
                }
                if button == Button::Left {
                    SetCapture(hwnd);
                }
                dispatch(hwnd, st, Event::MouseDown { p: client_pt(hwnd, lp), button, clicks, mods: mods() });
                LRESULT(matches!(msg, WM_XBUTTONDOWN | WM_XBUTTONDBLCLK) as isize)
            }
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
                let button = match msg {
                    WM_LBUTTONUP => Button::Left,
                    WM_RBUTTONUP => Button::Right,
                    WM_MBUTTONUP => Button::Middle,
                    _ if (wp.0 >> 16) & 0xffff == 1 => Button::Back,
                    _ => Button::Forward,
                };
                if button == Button::Left {
                    let _ = ReleaseCapture();
                }
                dispatch(hwnd, st, Event::MouseUp { p: client_pt(hwnd, lp), button, mods: mods() });
                LRESULT((msg == WM_XBUTTONUP) as isize)
            }
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                let d = ((wp.0 >> 16) & 0xffff) as i16 as f32 / WHEEL_DELTA as f32;
                let (dx, dy) = if msg == WM_MOUSEHWHEEL { (d, 0.0) } else { (0.0, d) };
                dispatch(hwnd, st, Event::Wheel { p: screen_pt(hwnd, lp), dx, dy, mods: mods() });
                LRESULT(0)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let handled = dispatch(hwnd, st, Event::Key { vk: wp.0 as u16, mods: mods(), repeat: lp.0 & (1 << 30) != 0 });
                if handled {
                    st.mute_syschar.set(msg == WM_SYSKEYDOWN);
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wp, lp)
                }
            }
            WM_SYSCHAR if st.mute_syschar.replace(false) => LRESULT(0),
            WM_CHAR => {
                let u = wp.0 as u16;
                if (0xD800..0xDC00).contains(&u) {
                    st.high.set(u);
                } else {
                    let hi = st.high.replace(0);
                    let ch = if (0xDC00..0xE000).contains(&u) && hi != 0 {
                        char::from_u32(0x10000 + (((hi as u32) - 0xD800) << 10) + (u as u32 - 0xDC00))
                    } else {
                        char::from_u32(u as u32)
                    };
                    if let Some(ch) = ch.filter(|c| !c.is_control()) {
                        dispatch(hwnd, st, Event::Char(ch));
                    }
                }
                LRESULT(0)
            }
            WM_SETCURSOR if (lp.0 & 0xffff) as u32 == HTCLIENT => {
                let mut p = POINT::default();
                let _ = GetCursorPos(&mut p);
                let _ = ScreenToClient(hwnd, &mut p);
                let s = scale_of(hwnd);
                let cur = st.handler.try_borrow().map_or(Cursor::Arrow, |h| h.cursor(pt(p.x as f32 / s, p.y as f32 / s)));
                let id = match cur {
                    Cursor::Arrow => IDC_ARROW,
                    Cursor::Hand => IDC_HAND,
                    Cursor::SizeWE => IDC_SIZEWE,
                    Cursor::SizeNS => IDC_SIZENS,
                };
                if let Ok(h) = LoadCursorW(None, id) {
                    SetCursor(Some(h));
                }
                LRESULT(1)
            }
            WM_TIMER => {
                dispatch(hwnd, st, Event::Timer(wp.0));
                LRESULT(0)
            }
            WM_SETTINGCHANGE => {
                dispatch(hwnd, st, Event::SystemTheme);
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_COPYDATA => {
                let data = &*(lp.0 as *const windows::Win32::System::DataExchange::COPYDATASTRUCT);
                if data.dwData != COPY_TEXT || data.cbData > 1 << 20 {
                    return LRESULT(0);
                }
                let bytes = if data.lpData.is_null() { &[][..] } else { std::slice::from_raw_parts(data.lpData as *const u8, data.cbData as usize) };
                dispatch(hwnd, st, Event::Opened(String::from_utf8_lossy(bytes).into_owned()));
                LRESULT(1)
            }
            // DBT_DEVICEARRIVAL, DBT_DEVICEREMOVECOMPLETE : clé USB, disque externe, lecteur réseau
            WM_DEVICECHANGE if matches!(wp.0, 0x8000 | 0x8004) => {
                dispatch(hwnd, st, Event::Devices);
                LRESULT(1)
            }
            WM_APP_TASK => {
                let task = Box::from_raw(lp.0 as *mut Task);
                let mut cx = Cx::new(hwnd, scale_of(hwnd));
                match st.handler.try_borrow_mut() {
                    Ok(mut h) => (*task)(h.as_mut(), &mut cx),
                    Err(_) => {
                        // application occupée : la tâche repasse dans la file
                        let _ = PostMessageW(Some(hwnd), WM_APP_TASK, WPARAM(0), LPARAM(Box::into_raw(task) as isize));
                        return LRESULT(0);
                    }
                }
                finish(hwnd, cx);
                LRESULT(0)
            }
            WM_CLOSE => {
                dispatch(hwnd, st, Event::Close);
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_NCDESTROY => {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                // l'état n'est libéré que si personne ne s'en sert (sinon tant pis pour quelques octets)
                if st.handler.try_borrow_mut().is_ok() && st.canvas.try_borrow_mut().is_ok() {
                    drop(Box::from_raw(st as *const State as *mut State));
                }
                if WINDOWS.with(|n| {
                    n.set(n.get().saturating_sub(1));
                    n.get()
                }) == 0
                {
                    PostQuitMessage(0);
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

/// Position d'une fenêtre : rectangle restauré (coordonnées de l'espace de travail, en pixels) et état agrandi.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub maximized: bool,
}

pub struct Options {
    pub title: String,
    /// Taille et taille minimale de la fenêtre, en DIP.
    pub size: (f32, f32),
    pub min: (f32, f32),
    pub dark: bool,
    /// Fond affiché avant la première image.
    pub background: Color,
    /// Faux : la fenêtre s'affiche sans prendre le focus (essais automatiques).
    pub activate: bool,
    /// Position retrouvée de la dernière fois.
    pub placement: Option<Placement>,
}

/// Crée une fenêtre. `make` construit l'application, qui reçoit la poignée de sa fenêtre.
pub fn create(gfx: &Rc<Gfx>, o: Options, make: impl FnOnce(Window) -> Box<dyn Handler>) -> Result<Window> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        thread_local! { static CLASS: Cell<bool> = const { Cell::new(false) }; }
        if !CLASS.with(|c| c.replace(true)) {
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: CS_DBLCLKS,
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                hIcon: LoadIconW(Some(instance.into()), windows::core::PCWSTR(1 as _)).unwrap_or_default(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                hbrBackground: CreateSolidBrush(windows::Win32::Foundation::COLORREF(o.background.colorref())),
                lpszClassName: w!("PlumeWindow"),
                ..Default::default()
            };
            RegisterClassExW(&class);
        }
        let title: Vec<u16> = o.title.encode_utf16().chain(Some(0)).collect();
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("PlumeWindow"),
            windows::core::PCWSTR(title.as_ptr()),
            // WS_CLIPCHILDREN : le dessin de la fenêtre ne passe pas sur le champ de saisie
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let win = Window { hwnd };
        let dpi = GetDpiForWindow(hwnd);
        let st = Box::new(State {
            handler: RefCell::new(make(win)),
            canvas: RefCell::new(Canvas::new(gfx.clone(), hwnd, dpi)),
            min: o.min,
            tracking: Cell::new(false),
            tracking_nc: Cell::new(false),
            active: Cell::new(false),
            high: Cell::new(0),
            mute_syschar: Cell::new(false),
            edit: OnceCell::new(),
            edit_multi: OnceCell::new(),
            multi: Cell::new(false),
            font: gfx.ui_font,
            hook: RefCell::new(None),
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
        WINDOWS.with(|n| n.set(n.get() + 1));
        win.set_dark(o.dark);
        // taille demandée, et cadre recalculé maintenant que la fenêtre a son état
        let s = dpi as f32 / 96.0;
        let _ = SetWindowPos(hwnd, None, 0, 0, (o.size.0 * s) as i32, (o.size.1 * s) as i32, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        let mut show = if o.activate { SW_SHOWNORMAL } else { SW_SHOWNOACTIVATE };
        if let Some(p) = o.placement.filter(|p| p.w >= 300 && p.h >= 200) {
            // Windows ramène de lui-même à l'écran une fenêtre dont le moniteur a disparu
            let wp = WINDOWPLACEMENT {
                length: size_of::<WINDOWPLACEMENT>() as u32,
                showCmd: SW_HIDE.0 as u32,
                rcNormalPosition: RECT { left: p.x, top: p.y, right: p.x + p.w, bottom: p.y + p.h },
                ..Default::default()
            };
            let _ = SetWindowPlacement(hwnd, &wp);
            if p.maximized && o.activate {
                show = SW_SHOWMAXIMIZED;
            }
        }
        let _ = ShowWindow(hwnd, show);
        let _ = UpdateWindow(hwnd);
        Ok(win)
    }
}

/// La fenêtre (de n'importe quel processus) marquée de ce nom par [`Window::mark`].
pub fn find(id: &str) -> Option<Window> {
    struct Search {
        name: Vec<u16>,
        found: Option<HWND>,
    }
    unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> windows::core::BOOL {
        unsafe {
            let search = &mut *(lp.0 as *mut Search);
            if !GetPropW(hwnd, windows::core::PCWSTR(search.name.as_ptr())).is_invalid() {
                search.found = Some(hwnd);
                return false.into();
            }
            true.into()
        }
    }
    let mut search = Search { name: id.encode_utf16().chain(Some(0)).collect(), found: None };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut search as *mut Search as isize));
    }
    search.found.map(|hwnd| Window { hwnd })
}

/// Boucle de messages : rend la main quand la dernière fenêtre est fermée.
pub fn run() {
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
