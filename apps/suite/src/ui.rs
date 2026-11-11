//! La fenêtre de Plume Suite : une carte par application (installer, ouvrir, retirer) et les mises à jour de toute
//! la suite. Les opérations longues (installeur, téléchargement) tournent sur un autre fil ; la fenêtre reste vivante.

use crate::{
    apps::{self, ALL, App},
    i18n::{Strings, fill},
};
use plume_ui::{Align, Area, Button, Canvas, Color, Cursor, Cx, Event, Font, Gfx, Handler, Hits, PaintCx, Pt, Rect, Theme, Window, m, rect};
use std::{
    any::Any,
    rc::Rc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Graphics::Imaging::{
            GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut,
            WICDecodeMetadataCacheOnLoad,
        },
        UI::WindowsAndMessaging::{IDYES, MB_DEFBUTTON2, MB_ICONQUESTION, MB_YESNO, MessageBoxW},
    },
    core::{HSTRING, Interface},
};

pub const SITE: &str = "https://plumeweb.netlify.app/";
const TOAST_TIMER: usize = 1;
const LOGOS: [&[u8]; 4] = [
    include_bytes!("../../../assets/brand/plume-web-256.png"),
    include_bytes!("../../../assets/brand/plume-fex-256.png"),
    include_bytes!("../../../assets/brand/plume-mail-256.png"),
    include_bytes!("../../../assets/brand/plume-suite-256.png"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    Min,
    Max,
    Close,
    Install(usize),
    Open(usize),
    Remove(usize),
    Update,
    Check,
    Site,
    Notes,
}

/// Où en sont les mises à jour de la suite.
#[derive(Clone, Debug, PartialEq)]
pub enum Upd {
    /// Build de développement (hors du dossier d'installation) : rien n'est vérifié.
    Dev,
    Checking,
    UpToDate,
    Offline,
    Available(String),
    Downloading(String, u32),
    Installing,
    Done,
    Failed(String),
}

pub struct Hub {
    pub win: Window,
    gfx: Rc<Gfx>,
    pub theme: Theme,
    pub s: &'static Strings,
    pub hits: Hits<Id>,
    pub hover: Option<Id>,
    pressed: Option<Id>,
    pub installed: [bool; 3],
    /// Application en cours d'installation (vrai) ou de retrait (faux).
    pub busy: Option<(usize, bool)>,
    pub upd: Upd,
    /// Mettre à jour dès que la vérification aura trouvé une version (demandé par une autre application).
    update_when_found: bool,
    pub toast: Option<(String, Instant)>,
}

/// Décode un PNG intégré au programme en image prête pour Direct2D.
fn png(wic: &IWICImagingFactory, bytes: &'static [u8]) -> Option<IWICBitmapSource> {
    unsafe {
        let stream = wic.CreateStream().ok()?;
        stream.InitializeFromMemory(bytes).ok()?;
        let decoder = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnLoad).ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
        conv.cast::<IWICBitmapSource>().ok()
    }
}

/// La vérification des mises à jour n'a de sens que pour la suite installée (ou un serveur d'essai local).
fn checks_updates() -> bool {
    apps::running_installed() || std::env::var_os("PLUME_UPDATE_FEED").is_some()
}

impl Hub {
    pub fn new(win: Window, gfx: Rc<Gfx>, theme: Theme, s: &'static Strings, updated: bool, update_now: bool) -> Hub {
        let mut h = Hub {
            win,
            gfx,
            theme,
            s,
            hits: Hits::default(),
            hover: None,
            pressed: None,
            installed: [false; 3],
            busy: None,
            upd: if checks_updates() { Upd::Checking } else { Upd::Dev },
            update_when_found: update_now,
            toast: None,
        };
        h.refresh();
        if updated {
            h.upd = Upd::Done;
        } else if h.upd == Upd::Checking {
            h.check();
        }
        h
    }

    pub fn refresh(&mut self) {
        for (i, app) in ALL.iter().enumerate() {
            self.installed[i] = app.installed();
        }
    }

    fn say(&mut self, text: String, cx: &mut Cx) {
        self.toast = Some((text, Instant::now()));
        cx.timer(TOAST_TIMER, Some(500));
        cx.repaint();
    }

    // ───────────────────────── mises à jour ─────────────────────────

    pub fn check(&mut self) {
        if !checks_updates() {
            self.upd = Upd::Dev;
            return;
        }
        self.upd = Upd::Checking;
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let found = plume_update::latest_tag().map(|(tag, _)| tag);
            sender.send(move |h, cx| {
                if let Some(hub) = h.as_any().downcast_mut::<Hub>() {
                    hub.upd = match found {
                        Ok(tag) if plume_update::newer(&tag, plume_update::VERSION) => Upd::Available(tag),
                        Ok(_) => Upd::UpToDate,
                        Err(_) => Upd::Offline,
                    };
                    if hub.update_when_found && matches!(hub.upd, Upd::Available(_)) {
                        hub.update_when_found = false;
                        hub.update();
                    }
                    cx.repaint();
                }
            });
        });
    }

    /// Une autre application de la suite demande la mise à jour (« Mettre à jour » de sa barre).
    pub fn update_requested(&mut self) {
        match self.upd {
            Upd::Available(_) => self.update(),
            Upd::Downloading(..) | Upd::Installing | Upd::Done => {}
            _ => {
                self.update_when_found = true;
                self.check();
            }
        }
    }

    pub fn update(&mut self) {
        let tag = match &self.upd {
            Upd::Available(t) => t.clone(),
            _ => return,
        };
        self.upd = Upd::Downloading(tag.clone(), 0);
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let show = |upd: Upd| {
                sender.send(move |h, cx| {
                    if let Some(hub) = h.as_any().downcast_mut::<Hub>() {
                        hub.upd = upd;
                        cx.repaint();
                    }
                });
            };
            let result = (|| -> Result<(), String> {
                let rel = plume_update::latest()?;
                let asset = rel.asset.ok_or("no installer in this release")?;
                let file = plume_update::download(&asset, &rel.tag, &apps::data_dir().join("updates"), &mut |p| show(Upd::Downloading(rel.tag.clone(), p)))?;
                show(Upd::Installing);
                plume_update::install(&file)
            })();
            match result {
                Ok(()) => sender.send(|h, cx| {
                    if let Some(hub) = h.as_any().downcast_mut::<Hub>() {
                        hub.upd = Upd::Done;
                        hub.refresh();
                        cx.repaint();
                        // la nouvelle Plume Suite prend la place de celle-ci
                        if apps::running_installed() {
                            hub.restart(cx);
                        }
                    }
                }),
                Err(e) => sender.send(move |h, cx| {
                    if let Some(hub) = h.as_any().downcast_mut::<Hub>() {
                        hub.upd = Upd::Failed(e);
                        cx.repaint();
                    }
                }),
            };
        });
    }

    /// Relance la nouvelle version une fois cette fenêtre fermée.
    fn restart(&mut self, cx: &mut Cx) {
        use std::os::windows::process::CommandExt;
        let exe = apps::suite_exe();
        let _ = std::process::Command::new("cmd")
            .raw_arg(format!("/c ping -n 2 127.0.0.1 >nul & start \"\" \"{}\" --updated", exe.display()))
            .current_dir(apps::install_dir())
            .creation_flags(0x0800_0000)
            .spawn();
        cx.after(|w| w.close());
    }

    // ───────────────────────── applications ─────────────────────────

    fn run(&mut self, i: usize, install: bool, cx: &mut Cx) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some((i, install));
        cx.repaint();
        let key = ALL[i].key();
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let r = apps::run_setup(&[if install { "--install" } else { "--uninstall" }, key]);
            sender.send(move |h, cx| {
                if let Some(hub) = h.as_any().downcast_mut::<Hub>() {
                    hub.busy = None;
                    hub.refresh();
                    let s = hub.s;
                    match r {
                        Ok(0) => {}
                        // 1 : l'utilisateur a renoncé (désinstallation de Plume Web)
                        Ok(1) if !install => {}
                        Ok(code) => hub.say(fill(s.failed, &[("e", &code.to_string())]), cx),
                        Err(e) if e == "missing-setup" => hub.say(s.missing_setup.to_string(), cx),
                        Err(e) => hub.say(fill(s.failed, &[("e", &e)]), cx),
                    }
                    cx.repaint();
                }
            });
        });
    }

    fn act(&mut self, id: Id, cx: &mut Cx) {
        match id {
            Id::Min => cx.after(|w| w.minimize()),
            Id::Max => cx.after(|w| w.toggle_maximize()),
            Id::Close => cx.after(|w| w.close()),
            Id::Install(i) => self.run(i, true, cx),
            Id::Open(i) => {
                if !apps::open(ALL[i]) {
                    let s = self.s;
                    self.say(fill(s.failed, &[("e", "open")]), cx);
                }
            }
            Id::Remove(i) => {
                // Plume Web pose ses propres questions (garder ou non les favoris, l'historique…)
                if ALL[i] == App::Web {
                    return self.run(i, false, cx);
                }
                let text = fill(self.s.confirm_remove, &[("app", ALL[i].name())]);
                let title = self.s.title;
                cx.after(move |w| {
                    let yes =
                        unsafe { MessageBoxW(Some(w.hwnd), &HSTRING::from(text), &HSTRING::from(title), MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2) } == IDYES;
                    if yes {
                        w.with(|h, cx| {
                            if let Some(hub) = h.as_any().downcast_mut::<Hub>() {
                                hub.run(i, false, cx);
                            }
                        });
                    }
                });
            }
            Id::Update => match self.upd {
                Upd::Failed(_) => {
                    self.check();
                    self.update_when_found = true;
                }
                _ => self.update(),
            },
            Id::Check => self.check(),
            Id::Site => {
                self.win.open(SITE);
            }
            Id::Notes => {
                self.win.open(&format!("{SITE}notes.html"));
            }
        }
        cx.repaint();
    }

    // ───────────────────────── dessin ─────────────────────────

    fn button(&mut self, c: &mut Canvas, r: Rect, label: &str, primary: bool, id: Option<Id>) {
        let t = &self.theme;
        let hot = id.is_some() && self.hover == id;
        let down = hot && self.pressed == id;
        let fill = match (primary, id.is_some()) {
            (true, true) if down => t.accent.mix(Color::hex(0), 0.18),
            (true, true) if hot => t.accent.mix(Color::hex(0xffffff), 0.1),
            (true, true) => t.accent,
            (_, false) => t.fill,
            (false, true) if hot => t.fill2,
            (false, true) => t.fill,
        };
        c.fill_round(r, r.h / 2.0, fill);
        let ink = match (primary, id.is_some()) {
            (true, true) => Color::hex(0xffffff),
            (_, false) => t.t3,
            (false, true) => t.t1,
        };
        c.text(label, r, Font::BodyBold, ink, Align::Center);
        if let Some(id) = id {
            self.hits.add(r, id);
        }
    }

    fn logo(&mut self, c: &mut Canvas, k: usize, r: Rect) {
        let wic = self.gfx.wic.clone();
        c.image(100 + k as u64, r, 1.0, || png(&wic, LOGOS[k]));
    }

    fn paint_head(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let t = self.theme.clone();
        let w = cx.w;
        c.gradient_v(rect(0.0, 0.0, w, m::TITLE_H), t.hdr1, t.hdr2);
        self.logo(c, 3, rect(14.0, (m::TITLE_H - 18.0) / 2.0, 18.0, 18.0));
        c.text(self.s.title, rect(40.0, 0.0, 200.0, m::TITLE_H), Font::BodyBold, if cx.active { t.t1 } else { t.t3 }, Align::Left);
        let cap_x = w - 3.0 * m::CAPTION_W;
        let glyphs = [(Id::Min, '\u{E921}'), (Id::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Id::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, m::TITLE_H - 8.0);
            let hot = self.hover == Some(id) || self.pressed == Some(id);
            if hot {
                c.fill(r, if id == Id::Close { t.close_hover } else { t.hover });
            }
            let ink = match (id == Id::Close && hot, cx.active) {
                (true, _) => Color::hex(0xffffff),
                (false, true) => t.t1,
                (false, false) => t.t3,
            };
            c.glyph(glyph, r, ink);
            self.hits.add(r, id);
        }
    }

    /// La carte des mises à jour, quand il y a quelque chose à dire. Rend sa hauteur (0 : rien).
    fn paint_update(&mut self, c: &mut Canvas, x: f32, y: f32, cw: f32) -> f32 {
        let t = self.theme.clone();
        let s = self.s;
        let (title, sub, button): (String, String, Option<(&str, Id)>) = match &self.upd {
            Upd::Available(v) => (fill(s.available, &[("v", v.trim_start_matches('v'))]), s.available_sub.into(), Some((s.update_all, Id::Update))),
            Upd::Downloading(v, p) => (fill(s.downloading, &[("v", v.trim_start_matches('v')), ("p", &p.to_string())]), s.available_sub.into(), None),
            Upd::Installing => (s.updating.into(), s.available_sub.into(), None),
            Upd::Done => (fill(s.up_to_date, &[("v", plume_update::VERSION)]), s.updated.into(), None),
            Upd::Failed(e) => (fill(s.update_failed, &[("e", e)]), String::new(), Some((s.retry, Id::Update))),
            _ => return 0.0,
        };
        let card = rect(x, y, cw, 76.0);
        c.fill_round(card, 16.0, t.accent.fade(if t.dark { 0.16 } else { 0.1 }));
        c.stroke_round(card, 16.0, t.accent.fade(0.45), 1.0);
        let text_w = cw - 40.0 - if button.is_some() { 190.0 } else { 0.0 };
        c.text(&title, rect(x + 20.0, y + 14.0, text_w, 22.0), Font::Title, t.t1, Align::Left);
        c.text(&sub, rect(x + 20.0, y + 40.0, text_w, 20.0), Font::Small, t.t2, Align::Left);
        if let Some((label, id)) = button {
            let bw = c.text_width(label, Font::BodyBold) + 40.0;
            self.button(c, rect(card.right() - 20.0 - bw, y + 21.0, bw, 34.0), label, true, Some(id));
        }
        if let Upd::Downloading(_, p) = self.upd {
            let bar = rect(x + 20.0, card.bottom() - 10.0, cw - 40.0, 4.0);
            c.fill_round(bar, 2.0, t.fill);
            c.fill_round(rect(bar.x, bar.y, bar.w * p as f32 / 100.0, bar.h), 2.0, t.accent);
        }
        card.h + 18.0
    }

    fn paint_app(&mut self, c: &mut Canvas, i: usize, card: Rect) {
        let t = self.theme.clone();
        let s = self.s;
        let app = ALL[i];
        let hot = matches!(self.hover, Some(Id::Install(k) | Id::Open(k) | Id::Remove(k)) if k == i);
        c.fill_round(card, 18.0, if hot { t.card.mix(t.t1, 0.03) } else { t.card });
        c.stroke_round(card, 18.0, t.sep, 1.0);
        self.logo(c, i, rect(card.x + 20.0, card.y + (card.h - 60.0) / 2.0, 60.0, 60.0));
        // à droite, les boutons
        let installed = self.installed[i];
        let mut right = card.right() - 20.0;
        let by = card.y + (card.h - 34.0) / 2.0;
        match self.busy {
            Some((k, install)) if k == i => {
                let label = if install { s.installing } else { s.removing };
                let bw = c.text_width(label, Font::BodyBold) + 40.0;
                self.button(c, rect(right - bw, by, bw, 34.0), label, true, None);
                right -= bw;
            }
            // pas encore dans cette version : ni bouton ni installation
            _ if !installed && !app.available() => {}
            _ => {
                let others_busy = self.busy.is_some();
                let (label, id) = if installed { (s.open, Id::Open(i)) } else { (s.install, Id::Install(i)) };
                let bw = (c.text_width(label, Font::BodyBold) + 40.0).max(96.0);
                let enabled = !(others_busy && !installed);
                self.button(c, rect(right - bw, by, bw, 34.0), label, true, enabled.then_some(id));
                right -= bw;
                if installed {
                    let rw = c.text_width(s.remove, Font::BodyBold) + 32.0;
                    self.button(c, rect(right - 10.0 - rw, by, rw, 34.0), s.remove, false, (!others_busy).then_some(Id::Remove(i)));
                    right -= rw + 10.0;
                }
            }
        }
        // au milieu, le nom, l'état et ce que fait l'application
        let x = card.x + 100.0;
        let text_w = (right - 20.0 - x).max(60.0);
        let name_w = c.text_width(app.name(), Font::Large);
        c.text(app.name(), rect(x, card.y + 24.0, text_w, 24.0), Font::Large, t.t1, Align::Left);
        let (state, ink) = match (installed, app.available()) {
            (true, _) => (s.installed, t.green),
            (false, true) => (s.not_installed, t.t3),
            (false, false) => (s.coming_soon, t.accent),
        };
        let pw = c.text_width(state, Font::SmallBold) + 18.0;
        if x + name_w + 12.0 + pw < right - 20.0 {
            let pill = rect(x + name_w + 12.0, card.y + 26.0, pw, 20.0);
            c.fill_round(pill, 10.0, ink.fade(0.16));
            c.text(state, pill, Font::SmallBold, ink, Align::Center);
        }
        c.text(s.desc[i], rect(x, card.y + 54.0, text_w, 20.0), Font::Body, t.t2, Align::Left);
    }

    fn paint_footer(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let t = self.theme.clone();
        let s = self.s;
        let y = cx.h - 44.0;
        c.hline(0.0, y, cx.w, t.sep);
        let v = plume_update::VERSION;
        let status = match &self.upd {
            Upd::Dev => fill(s.dev_build, &[("v", v)]),
            Upd::Checking => s.checking.to_string(),
            Upd::Offline => fill(s.offline, &[("v", v)]),
            Upd::UpToDate => fill(s.up_to_date, &[("v", v)]),
            _ => format!("Plume Suite {v}"),
        };
        let sw = c.text_width(&status, Font::Small);
        c.text(&status, rect(24.0, y, sw + 4.0, 44.0), Font::Small, t.t3, Align::Left);
        if matches!(self.upd, Upd::UpToDate | Upd::Offline) {
            let lw = c.text_width(s.check_now, Font::SmallBold) + 4.0;
            let r = rect(24.0 + sw + 14.0, y + 12.0, lw, 20.0);
            c.text(s.check_now, r, Font::SmallBold, if self.hover == Some(Id::Check) { t.t1 } else { t.accent }, Align::Left);
            self.hits.add(r, Id::Check);
        }
        let mut x = cx.w - 24.0;
        for (label, id) in [(s.notes, Id::Notes), (s.website, Id::Site)] {
            let lw = c.text_width(label, Font::Small) + 4.0;
            x -= lw;
            let r = rect(x, y + 12.0, lw, 20.0);
            c.text(label, r, Font::Small, if self.hover == Some(id) { t.t1 } else { t.t3 }, Align::Left);
            self.hits.add(r, id);
            x -= 18.0;
        }
    }

    fn paint_toast(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let Some((text, _)) = &self.toast else { return };
        let t = &self.theme;
        let tw = (c.text_width(text, Font::Body) + 40.0).min(cx.w - 40.0);
        let r = rect((cx.w - tw) / 2.0, cx.h - 100.0, tw, 38.0);
        c.fill_round(r, 19.0, t.solid2);
        c.stroke_round(r, 19.0, t.ring, 1.0);
        c.text(text, r.inset_xy(16.0, 0.0), Font::Body, t.t1, Align::Center);
    }
}

impl Handler for Hub {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let t = self.theme.clone();
        self.hits.clear();
        c.clear(t.bg);
        let cw = (cx.w - 64.0).clamp(300.0, 780.0);
        let x = (cx.w - cw) / 2.0;
        let mut y = m::TITLE_H + 30.0;
        c.text(self.s.welcome, rect(x, y, cw, 34.0), Font::Hero, t.t1, Align::Left);
        y += 40.0;
        let sub = if self.installed.iter().any(|&b| b) { self.s.sub_some } else { self.s.sub_none };
        c.text(sub, rect(x, y, cw, 20.0), Font::Body, t.t3, Align::Left);
        y += 40.0;
        y += self.paint_update(c, x, y, cw);
        for i in 0..ALL.len() {
            self.paint_app(c, i, rect(x, y, cw, 104.0));
            y += 118.0;
        }
        self.paint_footer(c, cx);
        self.paint_head(c, cx);
        self.paint_toast(c, cx);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::MouseMove { p, .. } => {
                let h = self.hits.at(p).copied();
                if h != self.hover {
                    self.hover = h;
                    cx.repaint();
                }
            }
            Event::MouseLeave => {
                self.hover = None;
                cx.repaint();
            }
            Event::MouseDown { p, button: Button::Left, .. } => {
                self.pressed = self.hits.at(p).copied();
                cx.capture(true);
                cx.repaint();
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                cx.capture(false);
                let up = self.hits.at(p).copied();
                if let (Some(a), Some(b)) = (self.pressed.take(), up)
                    && a == b
                {
                    self.act(a, cx);
                }
                cx.repaint();
            }
            Event::Key { vk: 0x74, .. } => {
                // F5 : vérifier de nouveau
                self.refresh();
                self.check();
                cx.repaint();
            }
            Event::Active(true) => {
                self.refresh();
                cx.repaint();
            }
            Event::Opened(text) => {
                self.win.bring_to_front();
                if text.contains("--update-now") {
                    self.update_requested();
                }
                cx.repaint();
            }
            Event::Timer(TOAST_TIMER) => {
                if self.toast.as_ref().is_none_or(|(_, at)| at.elapsed() > Duration::from_secs(5)) {
                    self.toast = None;
                    cx.timer(TOAST_TIMER, None);
                    cx.repaint();
                }
            }
            Event::Resized | Event::Scale | Event::Active(false) => cx.repaint(),
            _ => {}
        }
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(Id::Max) => Area::MaxButton,
            Some(Id::Min | Id::Close) => Area::Control,
            _ if p.y < m::TITLE_H => Area::Caption,
            _ => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Id::Min | Id::Max | Id::Close) | None => Cursor::Arrow,
            Some(_) => Cursor::Hand,
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
