//! What happens around a change: a snapshot first, a copy with Plume Backup (or the question when it isn't
//! installed), then the change on another thread. Also the graphics card's try / keep / revert, the stability test,
//! the crash guard, profiles, the sign-in value, snapshots restored, and the monitor window.

use super::{App, Modal, monitor, with_app};
use crate::{
    backend::{Fail, Hive, RUN_KEY, RUN_NAME, Shared, Val},
    backup_link::{self, Outcome},
    engine::{Engine, NO_WAIVER},
    gpu::{Fan, Guard, Knob, Profile, Profiles, StressResult, Telemetry, Tuning, Verdict},
    i18n::{fill, tf},
    snapshot::{self, Change, Why},
};
use plume_ui::Cx;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// How long the "Keep these settings?" question waits before going back.
pub const KEEP_SECS: u64 = 15;
/// How long the stability test lasts.
pub const STRESS_SECS: f64 = 60.0;
/// How long Plume Boost waits for Plume Backup's copy before going on (the copy finishes by itself).
const BACKUP_WAIT: Duration = Duration::from_secs(45);
pub const KEEP_TIMER: usize = 3;

/// A change waiting for its snapshot (and, when Plume Backup is missing, for the user's answer).
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    Apply { ids: Vec<&'static str>, restore: bool },
    Limit { setting: &'static str, ac: bool, val: u32 },
    Attrib { setting: &'static str, val: u32 },
    GpuTry { card: String, tuning: Tuning },
    GpuStock { card: String },
    UndoAll,
    Restore { dir: PathBuf },
}

impl Pending {
    fn why(&self) -> (Why, String) {
        match self {
            Pending::Apply { ids, .. } => (Why::Apply, ids.join(", ")),
            Pending::Limit { setting, .. } | Pending::Attrib { setting, .. } => (Why::Processor, crate::engine::setting_name(setting).to_string()),
            Pending::GpuTry { card, .. } | Pending::GpuStock { card } => (Why::Gpu, card.clone()),
            Pending::UndoAll => (Why::UndoAll, String::new()),
            Pending::Restore { dir } => (Why::Restore, dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
        }
    }
}

/// A graphics card setting being tried: what to go back to.
#[derive(Clone, Debug)]
pub struct Trying {
    pub card: String,
    pub prev: Tuning,
    pub logged: bool,
    /// None while the stability test runs.
    pub deadline: Option<Instant>,
    pub result: Option<StressResult>,
}

#[derive(Clone, Debug, Default)]
pub struct Stress {
    pub done: f64,
    pub total: f64,
    pub tel: Option<Telemetry>,
    pub stop: Arc<AtomicBool>,
}

/// What the snapshot and Plume Backup's copy gave, for the message after a change.
#[derive(Clone, Debug)]
pub struct Protected {
    pub snap: Result<PathBuf, String>,
    pub backup: Option<Outcome>,
}

/// Takes the snapshot, then asks Plume Backup for its copy (on the worker thread).
pub fn protect(be: &Shared, why: Why, detail: &str, backup: bool) -> Protected {
    let snap = snapshot::take(be, why, detail).map_err(|e| e.to_string());
    let backup = (backup && snap.is_ok()).then(|| backup_link::sync(BACKUP_WAIT));
    Protected { snap, backup }
}

pub fn outcome_text(o: &Option<Outcome>) -> String {
    match o {
        None => tf(
            "Snapshot kept on this PC (Plume Backup copies are off in Settings).",
            "Instantané gardé sur ce PC (copies Plume Backup coupées dans Paramètres).",
        )
        .into(),
        Some(Outcome::Copied(_)) => tf("Snapshot saved and copied by Plume Backup.", "Instantané enregistré et copié par Plume Backup.").into(),
        Some(Outcome::Late) => tf("Snapshot saved; Plume Backup is still copying it.", "Instantané enregistré ; Plume Backup le copie encore.").into(),
        Some(Outcome::Missing) => {
            tf("Snapshot kept on this PC only: Plume Backup isn't installed.", "Instantané gardé sur ce PC seulement : Plume Backup n'est pas installé.").into()
        }
        Some(Outcome::Failed(e)) => {
            fill(tf("Snapshot saved, but Plume Backup's copy failed: {}", "Instantané enregistré, mais la copie Plume Backup a échoué : {}"), &[e])
        }
    }
}

impl App {
    // ── the gate: Plume Backup there, or the question ────────────────────────────────────────────────────────────

    /// Runs a change after its snapshot. When the copy with Plume Backup is on but Plume Backup is missing, asks
    /// first (install it, or go on without it for this session).
    pub fn guarded(&mut self, p: Pending, cx: &mut Cx) {
        if self.busy.is_some() {
            return;
        }
        if self.set.backup && !self.backup_skip && backup_link::exe().is_none() {
            self.note("backup missing: asking".to_string());
            self.modal = Some(Modal::BackupMissing { pending: p });
            cx.repaint();
            return;
        }
        self.run_pending(p, cx);
    }

    fn use_backup(&self) -> bool {
        self.set.backup && !self.backup_skip && backup_link::exe().is_some()
    }

    pub fn backup_without(&mut self, cx: &mut Cx) {
        let Some(Modal::BackupMissing { pending }) = self.modal.take() else { return };
        self.backup_skip = true;
        self.note("backup: going on without Plume Backup".to_string());
        self.run_pending(pending, cx);
    }

    /// `PlumeSetup.exe --install backup`, then the waiting change.
    pub fn backup_install(&mut self, cx: &mut Cx) {
        let pending = match self.modal.take() {
            Some(Modal::BackupMissing { pending }) => Some(pending),
            _ => None,
        };
        self.busy = Some("Installing Plume Backup…");
        self.note("backup: installing".to_string());
        cx.repaint();
        let sender = cx.sender();
        std::thread::spawn(move || {
            let r = backup_link::install();
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.busy = None;
                    app.note(format!("backup install: {r:?}"));
                    match r {
                        Ok(()) => {
                            app.say(tf("Plume Backup is installed.", "Plume Backup est installé."), cx);
                            if let Some(p) = pending {
                                app.run_pending(p, cx);
                            }
                        }
                        Err(e) => {
                            app.say(e, cx);
                            if let Some(p) = pending {
                                app.modal = Some(Modal::BackupMissing { pending: p });
                            }
                        }
                    }
                    cx.repaint();
                }
            });
        });
    }

    pub fn run_pending(&mut self, p: Pending, cx: &mut Cx) {
        match p {
            Pending::Apply { ids, restore } => self.start_apply(ids, restore, cx),
            Pending::UndoAll => self.run_undo(None, cx),
            other => self.work(other, cx),
        }
    }

    /// The snapshot (and copy) a worker takes before its change.
    pub fn protect_args(&self, p: &Pending) -> (Shared, Why, String, bool) {
        let (why, detail) = p.why();
        (self.be.clone(), why, detail, self.use_backup())
    }

    pub fn on_protected(&mut self, pr: &Protected) {
        self.note(format!("snapshot {:?} backup {:?}", pr.snap.as_ref().map(|p| p.display().to_string()), pr.backup));
        self.backup_line = outcome_text(&pr.backup);
        self.snaps_list = snapshot::list();
    }

    /// The other changes: processor settings, graphics cards, restoring a snapshot.
    fn work(&mut self, p: Pending, cx: &mut Cx) {
        self.busy = Some(match p {
            Pending::Restore { .. } => "Restoring…",
            _ => "Applying…",
        });
        cx.repaint();
        let (be, why, detail, backup) = self.protect_args(&p);
        let (engine, sender, waiver) = (self.engine.clone(), cx.sender(), self.set.intel_waiver);
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
            }
            let pr = protect(&be, why, &detail, backup);
            let mut e = engine.lock().unwrap_or_else(|e| e.into_inner());
            e.waiver_ok = waiver;
            let out: Result<Done, Fail> = match &p {
                Pending::Limit { setting, ac, val } => e.set_power(setting, *ac, *val).map(|_| Done::Plain),
                // the processor settings Plume Boost manages are shown or hidden together
                Pending::Attrib { val, .. } => snapshot::ATTRIB_SETTINGS.iter().try_for_each(|s| e.set_attrib(s, *val)).map(|_| Done::Plain),
                Pending::GpuTry { card, tuning } => e.set_gpu(card, tuning).map(|(prev, logged)| {
                    arm_guard(&be, card, &prev);
                    Done::Tried { card: card.clone(), prev, logged }
                }),
                Pending::GpuStock { card } => e.reset_gpu(card).map(|prev| {
                    arm_guard(&be, card, &prev);
                    Done::Plain
                }),
                Pending::Restore { dir } => Ok(restore_now(&be, &mut e, dir)),
                Pending::Apply { .. } | Pending::UndoAll => Ok(Done::Plain),
            };
            drop(e);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.busy = None;
                    app.on_protected(&pr);
                    app.note(format!("work {p:?}: {out:?}"));
                    app.refresh();
                    app.reload_gpu();
                    match out {
                        Ok(Done::Tried { card, prev, logged }) => {
                            app.trying = Some(Trying { card, prev, logged, deadline: Some(Instant::now() + Duration::from_secs(KEEP_SECS)), result: None });
                            app.modal = Some(Modal::Keep);
                            cx.timer(KEEP_TIMER, Some(250));
                        }
                        Ok(Done::Restored { changed, fails }) => {
                            let msg = if fails.is_empty() {
                                fill(tf("Snapshot restored: {} settings put back.", "Instantané restauré : {} réglages remis."), &[&changed.to_string()])
                            } else {
                                fill(
                                    tf("{} settings could not be put back: {}", "{} réglages n'ont pas pu être remis : {}"),
                                    &[&fails.len().to_string(), &fails[0]],
                                )
                            };
                            app.say(msg, cx);
                        }
                        Ok(Done::Plain) => {
                            app.say(tf("Changed. Undo it from Changes whenever you like.", "Changé. Annulez-le depuis Changements quand vous voulez."), cx)
                        }
                        Err(Fail::NeedsAdmin) => {
                            app.modal = Some(Modal::Elevate(super::Why::Power));
                        }
                        Err(Fail::Other(w)) if w == NO_WAIVER => app.modal = Some(Modal::Waiver),
                        Err(f) => app.say(f.text(), cx),
                    }
                    cx.repaint();
                }
            });
        });
    }

    // ── graphics cards ───────────────────────────────────────────────────────────────────────────────────────────

    /// Reads the cards, the selected one's settings, the profiles (not while a change runs).
    pub fn reload_gpu(&mut self) {
        self.cards = self.be.gpus();
        if !self.cards.iter().any(|c| c.id == self.set.gpu_card) {
            // the first card that can be tuned, otherwise the first
            self.set.gpu_card = self.cards.iter().find(|c| c.caps.tunable()).or(self.cards.first()).map(|c| c.id.clone()).unwrap_or_default();
        }
        if let Ok(mut c) = self.live_ctl.card.lock() {
            c.clone_from(&self.set.gpu_card);
        }
        let card = self.set.gpu_card.clone();
        let cur = self.be.gpu_get(&card);
        if self.gpu_now != cur || self.draft.is_none() {
            self.draft = cur.clone();
        }
        self.gpu_now = cur;
        self.profiles = Profiles::load();
    }

    pub fn gpu_card(&self) -> Option<&crate::gpu::Card> {
        self.cards.iter().find(|c| c.id == self.set.gpu_card)
    }

    pub fn select_card(&mut self, i: usize, cx: &mut Cx) {
        if let Some(c) = self.cards.get(i) {
            self.set.gpu_card = c.id.clone();
            self.draft = None;
            self.set.save();
            self.reload_gpu();
            cx.repaint();
        }
    }

    pub fn draft_changed(&self) -> bool {
        self.draft.is_some() && self.draft != self.gpu_now
    }

    /// One step of a knob (the − and + buttons).
    pub fn knob_step(&mut self, k: Knob, dir: i8, cx: &mut Cx) {
        let Some(r) = self.gpu_card().and_then(|c| c.caps.range(k)) else { return };
        let step = match k {
            Knob::CoreOffset | Knob::MemOffset | Knob::CoreMax | Knob::MemMax | Knob::Voltage | Knob::VoltOffset => r.step.max(5.0),
            _ => r.step.max(1.0),
        };
        if let Some(d) = &mut self.draft {
            let v = d.get(k).unwrap_or(r.default);
            d.knobs.insert(k, r.clamp(v + step * dir as f64));
        }
        cx.repaint();
    }

    pub fn knob_set(&mut self, k: Knob, v: f64) {
        let Some(r) = self.gpu_card().and_then(|c| c.caps.range(k)) else { return };
        if let Some(d) = &mut self.draft {
            d.knobs.insert(k, r.clamp(v));
        }
    }

    pub fn fan_mode(&mut self, i: usize, cx: &mut Cx) {
        let Some(c) = self.gpu_card().cloned() else { return };
        let Some(d) = &mut self.draft else { return };
        d.fan = match i {
            0 if c.vendor != crate::gpu::Vendor::Amd => Fan::Auto,
            1 => match c.caps.fan_fixed {
                Some(r) => Fan::Fixed(r.clamp(50.0) as u32),
                None => return,
            },
            _ => match &c.caps.fan_curve {
                Some(fc) => {
                    let n = fc.points.max(2);
                    let pts = match &self.gpu_now.as_ref().map(|t| t.fan.clone()) {
                        Some(Fan::Curve(p)) if p.len() == n => p.clone(),
                        _ => (0..n).map(|k| (40 + (k as u32) * 50 / (n as u32 - 1), 30 + (k as u32) * 60 / (n as u32 - 1))).collect(),
                    };
                    Fan::Curve(pts)
                }
                None => return,
            },
        };
        cx.repaint();
    }

    pub fn fan_fixed_step(&mut self, dir: i8, cx: &mut Cx) {
        let Some(r) = self.gpu_card().and_then(|c| c.caps.fan_fixed) else { return };
        if let Some(Tuning { fan: Fan::Fixed(p), .. }) = &mut self.draft {
            *p = r.clamp(*p as f64 + 5.0 * dir as f64) as u32;
        }
        cx.repaint();
    }

    pub fn fan_point(&mut self, i: usize, temp: bool, dir: i8, cx: &mut Cx) {
        let Some(fc) = self.gpu_card().and_then(|c| c.caps.fan_curve.clone()) else { return };
        if let Some(Tuning { fan: Fan::Curve(pts), .. }) = &mut self.draft
            && let Some(p) = pts.get_mut(i)
        {
            if temp {
                p.0 = fc.temp.clamp(p.0 as f64 + 5.0 * dir as f64) as u32;
            } else {
                p.1 = fc.speed.clamp(p.1 as f64 + 5.0 * dir as f64) as u32;
            }
        }
        cx.repaint();
    }

    pub fn gpu_try(&mut self, cx: &mut Cx) {
        let (Some(card), Some(t)) = (self.gpu_card().cloned(), self.draft.clone()) else { return };
        if card.needs_waiver && !self.set.intel_waiver {
            self.modal = Some(Modal::Waiver);
            cx.repaint();
            return;
        }
        let tuning = card.caps.clamp(&t);
        self.note(format!("gpu try {}: {}", card.id, tuning.describe()));
        self.guarded(Pending::GpuTry { card: card.id, tuning }, cx);
    }

    pub fn gpu_stock(&mut self, cx: &mut Cx) {
        let Some(card) = self.gpu_card().cloned() else { return };
        if card.needs_waiver && !self.set.intel_waiver {
            self.modal = Some(Modal::Waiver);
            cx.repaint();
            return;
        }
        self.guarded(Pending::GpuStock { card: card.id }, cx);
    }

    pub fn accept_waiver(&mut self, cx: &mut Cx) {
        self.set.intel_waiver = true;
        self.set.save();
        self.modal = None;
        self.note("intel waiver accepted".to_string());
        self.gpu_try(cx);
    }

    /// "Keep": the settings stay; the crash guard keeps watching for its minutes.
    pub fn keep(&mut self, cx: &mut Cx) {
        if self.trying.take().is_some() {
            self.note("gpu kept".to_string());
            self.modal = None;
            cx.timer(KEEP_TIMER, None);
            self.say(
                tf(
                    "Kept. For the next 10 minutes, a crash or driver reset puts the previous settings back.",
                    "Gardé. Pendant 10 minutes, un plantage ou une réinitialisation du pilote remet les réglages précédents.",
                ),
                cx,
            );
        }
    }

    /// Back to the settings before the try (the countdown ran out, or "Revert").
    pub fn revert(&mut self, why: &str, cx: &mut Cx) {
        let Some(t) = self.trying.take() else { return };
        cx.timer(KEEP_TIMER, None);
        let r = self.engine.lock().map(|mut e| e.revert_gpu(&t.card, &t.prev, t.logged)).unwrap_or(Err(Fail::Missing));
        Guard::clear();
        self.note(format!("gpu reverted ({why}): {r:?}"));
        if !matches!(self.modal, Some(Modal::StressDone { .. })) {
            self.modal = None;
        }
        self.refresh();
        self.draft = None;
        self.reload_gpu();
        match r {
            Ok(()) => self.say(tf("Back to the previous settings.", "Retour aux réglages précédents."), cx),
            Err(f) => self.say(f.text(), cx),
        }
    }

    pub fn keep_tick(&mut self, cx: &mut Cx) {
        let Some(t) = &self.trying else {
            cx.timer(KEEP_TIMER, None);
            return;
        };
        if t.deadline.is_some_and(|d| Instant::now() >= d) {
            self.revert("no answer", cx);
        }
        cx.repaint();
    }

    pub fn keep_secs_left(&self) -> Option<u64> {
        let d = self.trying.as_ref()?.deadline?;
        Some(d.saturating_duration_since(Instant::now()).as_secs_f64().ceil() as u64)
    }

    /// The stability test, from the "Keep these settings?" question (the countdown waits) or from the page.
    pub fn stress_start(&mut self, cx: &mut Cx) {
        if self.stress.is_some() {
            return;
        }
        let Some(card) = self.gpu_card().cloned() else { return };
        if let Some(t) = &mut self.trying {
            t.deadline = None;
        }
        let st = Stress { total: STRESS_SECS, ..Default::default() };
        let stop = st.stop.clone();
        self.stress = Some(st);
        self.modal = Some(Modal::Stress);
        self.note(format!("stress start {}", card.id));
        cx.repaint();
        let (be, sender) = (self.be.clone(), cx.sender());
        std::thread::spawn(move || {
            let s2 = sender;
            let mut last = Instant::now() - Duration::from_secs(1);
            let r = be.gpu_stress(&card.id, STRESS_SECS, &stop, &mut |done, total, tel| {
                if last.elapsed() >= Duration::from_millis(200) {
                    last = Instant::now();
                    s2.send(move |h, cx| {
                        if let Some(app) = with_app(h)
                            && let Some(s) = &mut app.stress
                        {
                            (s.done, s.total, s.tel) = (done, total, tel);
                            cx.repaint();
                        }
                    });
                }
            });
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.on_stress(r, cx);
                }
            });
        });
    }

    fn on_stress(&mut self, r: StressResult, cx: &mut Cx) {
        self.stress = None;
        self.note(format!("stress done: {r:?}"));
        let failed = !r.passed() && !r.stopped;
        let trying = self.trying.is_some();
        if failed && trying {
            // what the test is for: an unstable setting goes back by itself
            self.modal = Some(Modal::StressDone { result: r.clone(), reverted: true });
            self.revert("stability test failed", cx);
            self.modal = Some(Modal::StressDone { result: r, reverted: true });
        } else if trying {
            if let Some(t) = &mut self.trying {
                t.deadline = Some(Instant::now() + Duration::from_secs(KEEP_SECS));
                t.result = Some(r);
            }
            self.modal = Some(Modal::Keep);
            cx.timer(KEEP_TIMER, Some(250));
        } else {
            self.modal = Some(Modal::StressDone { result: r, reverted: false });
        }
        cx.repaint();
    }

    pub fn stress_stop(&mut self) {
        if let Some(s) = &self.stress {
            s.stop.store(true, Ordering::Relaxed);
        }
    }

    /// The crash guard: a driver reset, or a restart without a clean shutdown, soon after a change puts the previous
    /// settings back and pauses "apply at sign-in".
    pub fn check_guard(&mut self, cx: &mut Cx) {
        if self.stress.is_some() || self.trying.is_some() || self.busy.is_some() {
            return;
        }
        let Some(g) = Guard::load() else { return };
        match guard_verdict(&self.be, &g) {
            Verdict::Wait => {}
            Verdict::Stable => {
                self.note("guard: stable".to_string());
                Guard::clear();
            }
            Verdict::Revert(reason) => {
                let r = self.be.gpu_set(&g.card, &g.previous);
                Guard::clear();
                self.set.gpu_signin_blocked = true;
                self.set.gpu_notice = reason.text().to_string();
                self.set.save();
                self.note(format!("guard: reverted {} ({reason:?}): {r:?}", g.card));
                self.draft = None;
                self.reload_gpu();
                self.modal = Some(Modal::GpuReverted { reason: reason.text().to_string() });
                cx.repaint();
            }
        }
    }

    // ── profiles and sign-in ─────────────────────────────────────────────────────────────────────────────────────

    pub fn profile_save(&mut self, cx: &mut Cx) {
        let mut ps = Profiles::load();
        let name = ps.next_name();
        let cards = self.cards.iter().filter(|c| c.caps.tunable()).filter_map(|c| self.be.gpu_get(&c.id).map(|t| (c.id.clone(), t))).collect();
        ps.list.push(Profile { name: name.clone(), cards });
        ps.save();
        self.profiles = ps;
        self.note(format!("profile saved {name}"));
        self.say(fill(tf("Saved as \"{}\".", "Enregistré sous « {} »."), &[&name]), cx);
    }

    /// A profile's values for this card go into the controls; Try applies them.
    pub fn profile_load(&mut self, i: usize, cx: &mut Cx) {
        let card = self.set.gpu_card.clone();
        if let Some(t) = self.profiles.list.get(i).and_then(|p| p.cards.get(&card)).cloned() {
            self.draft = Some(t);
            self.say(tf("Loaded. Press Try to use it.", "Chargé. Appuyez sur Essayer pour l'utiliser."), cx);
        } else {
            self.say(tf("This profile has nothing for this card.", "Ce profil n'a rien pour cette carte."), cx);
        }
    }

    pub fn profile_delete(&mut self, i: usize, cx: &mut Cx) {
        let mut ps = Profiles::load();
        if i >= ps.list.len() {
            return;
        }
        let p = ps.list.remove(i);
        let was_signin = ps.signin.as_deref() == Some(p.name.as_str());
        if was_signin {
            ps.signin = None;
        }
        ps.save();
        self.profiles = ps;
        if was_signin {
            self.set_signin(None, cx);
        }
        cx.repaint();
    }

    /// Chooses the profile applied when Windows starts (None: none). The Run value is logged, so Undo removes it.
    pub fn set_signin(&mut self, name: Option<String>, cx: &mut Cx) {
        let mut ps = Profiles::load();
        ps.signin = name.clone();
        ps.save();
        self.profiles = ps;
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
        let val = name.as_ref().map(|_| Val::Sz(format!("\"{exe}\" --apply-gpu-profile")));
        let r = self.engine.lock().map(|mut e| e.set_reg("gpu_signin", Hive::User, RUN_KEY, RUN_NAME, val)).unwrap_or(Err(Fail::Missing));
        self.note(format!("signin {name:?}: {r:?}"));
        if name.is_some() {
            self.set.gpu_signin_blocked = false;
            self.set.gpu_notice.clear();
            self.set.save();
        }
        self.refresh();
        match r {
            Ok(()) if name.is_some() => {
                self.say(tf("Applied each time you sign in to Windows.", "Appliqué à chaque connexion à Windows."), cx);
            }
            Ok(()) => self.say(tf("No longer applied at sign-in.", "N'est plus appliqué à la connexion."), cx),
            Err(f) => self.say(f.text(), cx),
        }
    }

    // ── snapshots ────────────────────────────────────────────────────────────────────────────────────────────────

    pub fn ask_restore(&mut self, i: usize, cx: &mut Cx) {
        let Some((dir, _)) = self.snaps_list.get(i).cloned() else { return };
        match snapshot::plan(&self.be, &dir) {
            Ok(changes) => {
                let admin = changes.iter().any(Change::needs_admin);
                self.modal = Some(Modal::Restore { dir, lines: changes.iter().map(Change::describe).collect(), admin });
            }
            Err(e) => self.say(e, cx),
        }
        cx.repaint();
    }

    pub fn restore_ok(&mut self, cx: &mut Cx) {
        let Some(Modal::Restore { dir, lines, admin }) = self.modal.clone() else { return };
        if lines.is_empty() {
            self.modal = None;
            self.say(tf("Nothing to change: the PC is already as in this snapshot.", "Rien à changer : le PC est déjà comme dans cet instantané."), cx);
            return;
        }
        if admin && !self.admin {
            self.modal = Some(Modal::Elevate(super::Why::Undo));
            cx.repaint();
            return;
        }
        self.modal = None;
        self.guarded(Pending::Restore { dir }, cx);
    }

    // ── the monitor ──────────────────────────────────────────────────────────────────────────────────────────────

    pub fn toggle_monitor(&mut self, cx: &mut Cx) {
        if let Some(w) = self.monitor.take() {
            w.close();
            self.set.monitor = false;
        } else {
            self.monitor = monitor::open(&self.gfx, self.pal.clone(), self.live.clone());
            self.set.monitor = self.monitor.is_some();
        }
        self.set.save();
        cx.repaint();
    }
}

/// What a worker did, for the window.
#[derive(Debug)]
enum Done {
    Plain,
    Tried { card: String, prev: Tuning, logged: bool },
    Restored { changed: usize, fails: Vec<String> },
}

fn arm_guard(be: &Shared, card: &str, prev: &Tuning) {
    Guard { card: card.into(), changed_at: crate::engine::now(), previous: prev.clone(), boot: be.boot_time() }.save();
}

pub fn guard_verdict(be: &Shared, g: &Guard) -> Verdict {
    g.verdict(crate::engine::now(), be.boot_time(), be.clean_shutdown_time(), be.driver_resets_since(g.changed_at))
}

fn restore_now(be: &Shared, e: &mut Engine, dir: &std::path::Path) -> Done {
    let changes = snapshot::plan(be, dir).unwrap_or_default();
    let fails = snapshot::apply(be, &changes);
    if fails.is_empty() {
        // the PC is as it was then: the undo log of then is the right one
        e.replace_log(snapshot::saved_log(dir));
    }
    Done::Restored { changed: changes.len() - fails.len(), fails: fails.iter().map(|(c, f)| format!("{} ({})", c.describe(), f.text())).collect() }
}

/// For the headless `--apply-gpu-profile` and `--undo-all`: the crash guard's check without a window.
pub fn guard_headless(be: &Shared) -> Option<crate::gpu::Reason> {
    let g = Guard::load()?;
    match guard_verdict(be, &g) {
        Verdict::Wait => None,
        Verdict::Stable => {
            Guard::clear();
            None
        }
        Verdict::Revert(r) => {
            let _ = be.gpu_set(&g.card, &g.previous);
            Guard::clear();
            let mut s = crate::settings::Settings::load();
            s.gpu_signin_blocked = true;
            s.gpu_notice = r.text().to_string();
            s.save();
            Some(r)
        }
    }
}

/// `--apply-gpu-profile` (the Run value at sign-in): the crash guard first, then the chosen profile, through the
/// engine (logged, undoable), with a new guard. Exit code 0 when applied or nothing to do.
pub fn apply_profile_headless(be: Shared) -> i32 {
    if guard_headless(&be).is_some() {
        return 0;
    }
    let set = crate::settings::Settings::load();
    if set.gpu_signin_blocked {
        return 0;
    }
    let ps = Profiles::load();
    let Some(p) = ps.signin.as_ref().and_then(|n| ps.get(n)).cloned() else { return 0 };
    let mut e = Engine::new(be.clone(), Some(crate::settings::data_dir()));
    e.waiver_ok = set.intel_waiver;
    let mut code = 0;
    for (card, t) in &p.cards {
        match e.set_gpu(card, t) {
            Ok((prev, _)) => arm_guard(&be, card, &prev),
            Err(_) => code = 1,
        }
    }
    code
}
