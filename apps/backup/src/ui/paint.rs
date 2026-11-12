//! Drawing: the title bar, the side bar, and each view, from the state in `App`. Every clickable element is
//! registered in `hits` with its `Id` as it is drawn.

use super::{App, Browse, Field, Id, View, ZONES, Zone, percent};
use crate::{
    engine::{Fail, Kind, Mode, Phase, Plan, Schedule},
    i18n::{self, fill, tr},
    sys::{self, Place},
};
use plume_ui::{
    Align, Canvas, Color, EditStyle, Font, PaintCx, Rect,
    icons::{self, Icon, Part::P},
    m, rect,
};
use windows::{
    Win32::Graphics::Imaging::{
        GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut,
        WICDecodeMetadataCacheOnLoad,
    },
    core::Interface,
};

const LOGO: &[u8] = include_bytes!("../../../../assets/brand/plume-backup-256.png");
const SIDE: f32 = 212.0;

/// A box with an arrow going up into it: backing up.
const I_BACKUP: Icon =
    Icon { name: "bk_backup", vb: 16.0, parts: &[P("M8 10V2.8M5.2 5.6 8 2.8l2.8 2.8"), P("M2.5 9v3.2a1.3 1.3 0 0 0 1.3 1.3h8.4a1.3 1.3 0 0 0 1.3-1.3V9")] };
/// A clock going backwards: restoring.
const I_HISTORY: Icon = Icon { name: "bk_history", vb: 16.0, parts: &[P("M2.8 8a5.2 5.2 0 1 0 1.6-3.8"), P("M2.6 2.6v2.8h2.8"), P("M8 5.2V8l2 1.3")] };
const I_STOP: Icon = Icon { name: "bk_stop", vb: 16.0, parts: &[P("M4.5 4.5h7v7h-7z")] };
const I_MINUS: Icon = Icon { name: "bk_minus", vb: 16.0, parts: &[P("M3.5 8h9")] };

#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    Plain,
    Accent,
    Red,
}

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

pub fn schedule_text(p: &Plan) -> String {
    match p.schedule {
        Schedule::Manual => tr("Only when you ask").into(),
        Schedule::Hourly => tr("Every hour").into(),
        Schedule::Daily => fill(tr("Every day from {}"), &[&format!("{:02}:00", p.hour)]),
        Schedule::Plug => tr("When the drive is plugged in").into(),
    }
}

pub fn mode_text(p: &Plan) -> String {
    match p.mode {
        Mode::Mirror => tr("Mirror").into(),
        Mode::Versions => {
            let mut s =
                if p.keep_versions == 0 { tr("Versions: all kept").to_string() } else { fill(tr("Versions: {} kept"), &[&p.keep_versions.to_string()]) };
            if p.keep_days > 0 {
                s.push_str(&fill(tr(", {} days at most"), &[&p.keep_days.to_string()]));
            }
            s
        }
    }
}

fn days_text(d: u32, zero: &'static str) -> String {
    if d == 0 { tr(zero).into() } else { fill(tr("{} days"), &[&d.to_string()]) }
}

impl App {
    /// Registers a clickable rectangle (only its visible part, inside a scrolled list).
    fn hit(&mut self, r: Rect, id: Id) {
        let r = match self.clip {
            Some(cl) => r.intersect(&cl),
            None => r,
        };
        if !r.is_empty() {
            self.hits.add(r, id);
        }
    }

    fn begin_zone(&mut self, c: &mut Canvas, z: Zone, area: Rect) -> f32 {
        self.areas[z as usize] = area;
        self.clip = Some(area);
        c.clip(area);
        area.y - self.scroll[z as usize]
    }

    /// `end`: where the content stopped (in scrolled coordinates).
    fn end_zone(&mut self, c: &mut Canvas, z: Zone, end: f32) {
        let k = z as usize;
        let area = self.areas[k];
        let content = end + self.scroll[k] - area.y;
        self.scroll_max[k] = (content - area.h + 16.0).max(0.0);
        self.scroll[k] = self.scroll[k].min(self.scroll_max[k]);
        c.unclip();
        self.clip = None;
    }

    fn button(&mut self, c: &mut Canvas, r: Rect, label: &str, style: Style, id: Id) {
        self.button_icon(c, r, None, label, style, id);
    }

    fn button_icon(&mut self, c: &mut Canvas, r: Rect, icon: Option<&Icon>, label: &str, style: Style, id: Id) {
        let t = self.theme.clone();
        let hot = self.hover == Some(id);
        let down = hot && self.pressed == Some(id);
        let base = match style {
            Style::Accent => t.accent,
            Style::Red => t.red,
            Style::Plain => t.fill,
        };
        let fill = match (style, hot, down) {
            (_, _, true) => base.mix(Color::hex(0), 0.15),
            (Style::Plain, true, _) => t.fill2,
            (_, true, _) => base.mix(Color::hex(0xffffff), 0.1),
            _ => base,
        };
        let ink = if style == Style::Plain { t.t1 } else { Color::hex(0xffffff) };
        c.fill_round(r, r.h / 2.0, fill);
        match icon {
            Some(ic) if label.is_empty() => c.icon(ic, r.centered(16.0, 16.0), ink),
            Some(ic) => {
                let tw = c.text_width(label, Font::BodyBold);
                let x = r.x + (r.w - tw - 22.0) / 2.0;
                c.icon(ic, rect(x, r.y + (r.h - 16.0) / 2.0, 16.0, 16.0), ink);
                c.text(label, rect(x + 22.0, r.y, tw + 2.0, r.h), Font::BodyBold, ink, Align::Left);
            }
            None => c.text(label, r, Font::BodyBold, ink, Align::Center),
        }
        self.hit(r, id);
    }

    fn btn_w(c: &mut Canvas, label: &str, icon: bool) -> f32 {
        c.text_width(label, Font::BodyBold) + if icon { 54.0 } else { 32.0 }
    }

    /// The accent for text and small icons on a selection: lighter in dark mode, where the emerald itself is too dim to read.
    fn accent_ink(&self) -> Color {
        let t = &self.theme;
        if t.dark { t.accent.mix(Color::hex(0xffffff), 0.35) } else { t.accent }
    }

    fn card(&self, c: &mut Canvas, r: Rect) {
        c.fill_round(r, 18.0, self.theme.card);
        c.stroke_round(r, 18.0, self.theme.sep, 1.0);
    }

    fn switch(&mut self, c: &mut Canvas, x: f32, y: f32, on: bool) {
        let t = &self.theme;
        let sw = rect(x, y, 44.0, 24.0);
        c.fill_round(sw, 12.0, if on { t.accent } else { t.fill2 });
        let knob = if on { sw.right() - 21.0 } else { sw.x + 3.0 };
        c.fill_round(rect(knob, sw.y + 3.0, 18.0, 18.0), 9.0, Color::hex(0xffffff));
    }

    /// A row with a title, a line of explanation and a switch on the right; the whole row is clickable.
    fn switch_row(&mut self, c: &mut Canvas, at: Rect, title: &str, sub: &str, on: bool, id: Id) -> f32 {
        let (x, y, w) = (at.x, at.y, at.w);
        let t = self.theme.clone();
        let h = if sub.is_empty() { 48.0 } else { 62.0 };
        let r = rect(x, y, w, h);
        if self.hover == Some(id) {
            c.fill_round(r.inset_xy(6.0, 3.0), 10.0, t.hover);
        }
        self.switch(c, r.right() - 22.0 - 44.0, y + (h - 24.0) / 2.0, on);
        let tw = w - 22.0 - 44.0 - 40.0;
        if sub.is_empty() {
            c.text(title, rect(x + 20.0, y, tw, h), Font::Body, t.t1, Align::Left);
        } else {
            c.text(title, rect(x + 20.0, y + 10.0, tw, 22.0), Font::Body, t.t1, Align::Left);
            c.text(sub, rect(x + 20.0, y + 32.0, tw, 18.0), Font::Small, t.t3, Align::Left);
        }
        self.hit(r, id);
        h
    }

    /// Choices side by side, the chosen one raised.
    fn segments(&mut self, c: &mut Canvas, r: Rect, items: &[(String, Id, bool)]) {
        let t = self.theme.clone();
        c.fill_round(r, r.h / 2.0, t.fill);
        let w = r.w / items.len() as f32;
        for (k, (label, id, on)) in items.iter().enumerate() {
            let cell = rect(r.x + k as f32 * w, r.y, w, r.h).inset(3.0);
            if *on {
                c.fill_round(cell, cell.h / 2.0, t.fill2);
                c.stroke_round(cell, cell.h / 2.0, t.ring, 1.0);
            } else if self.hover == Some(*id) {
                c.fill_round(cell, cell.h / 2.0, t.hover);
            }
            c.text(label, cell.inset_xy(6.0, 0.0), if *on { Font::SmallBold } else { Font::Small }, if *on { t.t1 } else { t.t2 }, Align::Center);
            self.hit(cell, *id);
        }
    }

    /// "Title ........ [−] value [+]".
    fn stepper_row(&mut self, c: &mut Canvas, at: Rect, title: &str, value: &str, down: Id, up: Id) -> f32 {
        let (x, y, w) = (at.x, at.y, at.w);
        let t = self.theme.clone();
        let h = 46.0;
        let bx = x + w - 20.0 - 32.0;
        self.button_icon(c, rect(bx, y + 7.0, 32.0, 32.0), Some(&icons::PLUS), "", Style::Plain, up);
        let vw = 110.0;
        c.text(value, rect(bx - vw - 6.0, y, vw, h), Font::BodyBold, t.t1, Align::Center);
        self.button_icon(c, rect(bx - vw - 44.0, y + 7.0, 32.0, 32.0), Some(&I_MINUS), "", Style::Plain, down);
        c.text(title, rect(x + 20.0, y, bx - vw - 60.0 - x - 20.0, h), Font::Body, t.t1, Align::Left);
        h
    }

    /// A text field; the real edit box opens on it when it is clicked.
    fn field(&mut self, c: &mut Canvas, r: Rect, f: Field, placeholder: &str) {
        let t = self.theme.clone();
        let bg = t.fill.over(t.card);
        c.fill_round(r, 10.0, bg);
        let editing = self.field == Some(f);
        c.stroke_round(r, 10.0, if editing { t.accent } else { t.sep }, if editing { 1.5 } else { 1.0 });
        let inner = r.inset_xy(12.0, 7.0);
        if editing {
            if self.edit_pending {
                self.edit_pending = false;
                let st = EditStyle { fg: t.t1, bg, size: 13.0, select: None, placeholder: placeholder.to_string(), password: false, multiline: false };
                self.win.edit_show(inner, &self.field_text(f), &st);
                self.win.edit_focus();
            } else {
                self.win.edit_place(inner);
            }
        } else {
            let text = self.field_text(f);
            if text.is_empty() {
                c.text(placeholder, inner, Font::Body, t.t4, Align::Left);
            } else {
                c.text(&text, inner, Font::Body, t.t1, Align::Left);
            }
            self.hit(r, Id::Field(f));
        }
    }

    // ───────────────────────── frame ─────────────────────────

    pub(super) fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let t = self.theme.clone();
        self.hits.clear();
        self.clip = None;
        self.areas = [Rect::default(); ZONES];
        c.clear(t.bg);
        let x0 = SIDE + 32.0;
        let cw = (cx.w - x0 - 32.0).clamp(320.0, 900.0);
        match self.view {
            View::Plans => self.paint_plans(c, cx, x0, cw),
            View::Edit => self.paint_edit(c, cx, x0, cw),
            View::Restore => self.paint_restore(c, cx, x0, cw),
            View::Settings => self.paint_settings(c, cx, x0, cw),
        }
        self.paint_side(c, cx);
        self.paint_head(c, cx);
        self.paint_toast(c, cx);
        if self.confirm_delete {
            self.paint_confirm(c, cx);
        }
    }

    fn paint_head(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let t = self.theme.clone();
        c.gradient_v(rect(0.0, 0.0, cx.w, m::TITLE_H), t.hdr1, t.hdr2);
        c.hline(0.0, m::TITLE_H - 0.5, cx.w, t.sep);
        let wic = self.gfx.wic.clone();
        c.image(1, rect(14.0, (m::TITLE_H - 18.0) / 2.0, 18.0, 18.0), 1.0, || png(&wic, LOGO));
        c.text("Plume Backup", rect(40.0, 0.0, 200.0, m::TITLE_H), Font::BodyBold, if cx.active { t.t1 } else { t.t3 }, Align::Left);
        let cap_x = cx.w - 3.0 * m::CAPTION_W;
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

    fn paint_side(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let t = self.theme.clone();
        c.fill(rect(0.0, m::TITLE_H, SIDE, cx.h - m::TITLE_H), t.bg2);
        c.vline(SIDE, m::TITLE_H, cx.h - m::TITLE_H, t.sep);
        let items: [(View, &Icon, &str); 3] =
            [(View::Plans, &I_BACKUP, tr("Backups")), (View::Restore, &I_HISTORY, tr("Restore")), (View::Settings, &icons::GEAR, tr("Settings"))];
        let mut y = m::TITLE_H + 16.0;
        for (v, ic, label) in items {
            let r = rect(12.0, y, SIDE - 24.0, 40.0);
            let on = self.view == v || (v == View::Plans && self.view == View::Edit);
            let id = Id::Nav(v);
            if on {
                c.fill_round(r, 10.0, t.accent.fade(0.16));
            } else if self.hover == Some(id) {
                c.fill_round(r, 10.0, t.hover);
            }
            c.icon(ic, rect(r.x + 12.0, r.y + 12.0, 16.0, 16.0), if on { t.accent } else { t.t2 });
            c.text(label, rect(r.x + 40.0, r.y, r.w - 48.0, r.h), if on { Font::BodyBold } else { Font::Body }, t.t1, Align::Left);
            self.hit(r, id);
            y += 44.0;
        }
        // the backup in progress, wherever one is
        if let Some(run) = &self.running {
            let name = self.store.plan(run.id).map(|p| p.name.clone()).unwrap_or_default();
            let pc = percent(&run.prog);
            let r = rect(12.0, cx.h - 84.0, SIDE - 24.0, 68.0);
            c.fill_round(r, 16.0, t.card);
            c.stroke_round(r, 16.0, t.sep, 1.0);
            c.text(&fill(tr("Backing up {}"), &[&name]), rect(r.x + 14.0, r.y + 10.0, r.w - 28.0, 20.0), Font::SmallBold, t.t1, Align::Left);
            c.text(&format!("{pc} %"), rect(r.x + 14.0, r.y + 30.0, r.w - 28.0, 18.0), Font::Small, t.t3, Align::Left);
            let bar = rect(r.x + 14.0, r.y + 52.0, r.w - 28.0, 5.0);
            c.fill_round(bar, 2.5, t.fill);
            c.fill_round(rect(bar.x, bar.y, bar.w * pc as f32 / 100.0, bar.h), 2.5, t.accent);
        }
    }

    fn heading(&mut self, c: &mut Canvas, x: f32, cw: f32, title: &str, sub: &str) -> f32 {
        let t = self.theme.clone();
        let y = m::TITLE_H + 24.0;
        c.text(title, rect(x, y, cw - 220.0, 34.0), Font::Hero, t.t1, Align::Left);
        if !sub.is_empty() {
            c.text(sub, rect(x, y + 38.0, cw, 20.0), Font::Body, t.t3, Align::Left);
        }
        y + 74.0
    }

    // ───────────────────────── plans ─────────────────────────

    fn paint_plans(&mut self, c: &mut Canvas, cx: &PaintCx, x0: f32, cw: f32) {
        let t = self.theme.clone();
        let top = self.heading(c, x0, cw, tr("Backups"), tr("Your folders, copied automatically to a drive, another folder or a PC on your network."));
        let label = tr("New plan");
        let bw = Self::btn_w(c, label, true);
        self.button_icon(c, rect(x0 + cw - bw, m::TITLE_H + 26.0, bw, 34.0), Some(&icons::PLUS), label, Style::Accent, Id::NewPlan);
        let area = rect(x0 - 6.0, top, cw + 12.0, cx.h - top);
        let mut y = self.begin_zone(c, Zone::Plans, area);
        if self.store.plans.is_empty() {
            let r = rect(x0, y + 10.0, cw, 230.0);
            self.card(c, r);
            c.fill_round(r.centered(64.0, 64.0).offset(0.0, -50.0), 20.0, t.accent.fade(0.16));
            c.icon(&I_BACKUP, r.centered(30.0, 30.0).offset(0.0, -50.0), t.accent);
            c.text(tr("No backup plan yet"), rect(r.x, r.y + 116.0, r.w, 26.0), Font::Large, t.t1, Align::Center);
            c.text(tr("Choose the folders to protect and where to copy them."), rect(r.x, r.y + 144.0, r.w, 20.0), Font::Body, t.t3, Align::Center);
            let label = tr("Create a plan");
            let bw = Self::btn_w(c, label, false);
            self.button(c, rect(r.x + (r.w - bw) / 2.0, r.y + 178.0, bw, 34.0), label, Style::Accent, Id::NewPlan);
            y += 250.0;
        }
        for i in 0..self.store.plans.len() {
            let r = rect(x0, y, cw, 136.0);
            if r.bottom() > area.y && r.y < area.bottom() {
                self.paint_plan(c, i, r);
            }
            y += 150.0;
        }
        self.end_zone(c, Zone::Plans, y);
    }

    fn paint_plan(&mut self, c: &mut Canvas, i: usize, r: Rect) {
        let t = self.theme.clone();
        let plan = self.store.plans[i].clone();
        let st = self.store.state(plan.id);
        self.card(c, r);
        let tile = rect(r.x + 20.0, r.y + 20.0, 44.0, 44.0);
        c.fill_round(tile, 13.0, t.accent.fade(0.16));
        let ic = match sys::place(&plan.dest) {
            Place::Removable => &icons::DRIVE,
            Place::Network => &icons::SERVER,
            _ => &icons::FOLDER,
        };
        c.icon(ic, tile.centered(20.0, 20.0), t.accent);
        // buttons, from the right
        let running = self.running.as_ref().filter(|run| run.id == plan.id).map(|run| run.prog.clone());
        let queued = self.queue.contains(&plan.id);
        let mut bx = r.right() - 20.0;
        let by = r.y + 22.0;
        let restore = tr("Restore");
        let w = Self::btn_w(c, restore, false);
        bx -= w;
        self.button(c, rect(bx, by, w, 32.0), restore, Style::Plain, Id::Open(i));
        let edit = tr("Edit");
        let w = Self::btn_w(c, edit, false);
        bx -= w + 8.0;
        self.button(c, rect(bx, by, w, 32.0), edit, Style::Plain, Id::Edit(i));
        if running.is_some() || queued {
            let stop = tr("Stop");
            let w = Self::btn_w(c, stop, true);
            bx -= w + 8.0;
            self.button_icon(c, rect(bx, by, w, 32.0), Some(&I_STOP), stop, Style::Red, Id::Stop(i));
        } else {
            let now = tr("Back up now");
            let w = Self::btn_w(c, now, false);
            bx -= w + 8.0;
            self.button(c, rect(bx, by, w, 32.0), now, Style::Accent, Id::Run(i));
        }
        let tx = r.x + 80.0;
        let tw = bx - tx - 14.0;
        c.text(&plan.name, rect(tx, r.y + 16.0, tw, 24.0), Font::Title, t.t1, Align::Left);
        let names: Vec<String> =
            plan.sources.iter().map(|s| s.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| s.display().to_string())).collect();
        let what = format!("{}  →  {}", names.join(", "), plan.dest.display());
        c.text(&what, rect(tx, r.y + 42.0, tw, 20.0), Font::Body, t.t2, Align::Left);
        let how = format!("{}  ·  {}", schedule_text(&plan), mode_text(&plan));
        c.text(&how, rect(tx, r.y + 64.0, r.right() - 24.0 - tx, 18.0), Font::Small, t.t3, Align::Left);
        let full = r.right() - 24.0 - tx;
        let now = crate::engine::now();
        if let Some(p) = running {
            let pc = percent(&p);
            let text = match p.phase {
                Phase::Scanning => tr("Looking at the files…").to_string(),
                Phase::Cleaning => tr("Tidying up old versions…").to_string(),
                Phase::Copying => fill(
                    tr("{} of {} files · {} of {} · {} %"),
                    &[&i18n::count(p.done), &i18n::count(p.total), &i18n::size(p.bytes_done), &i18n::size(p.bytes_total), &pc.to_string()],
                ),
            };
            c.text(&text, rect(tx, r.y + 90.0, full, 18.0), Font::Small, t.t2, Align::Left);
            let bar = rect(tx, r.y + 114.0, full, 6.0);
            c.fill_round(bar, 3.0, t.fill);
            c.fill_round(rect(bar.x, bar.y, bar.w * pc as f32 / 100.0, bar.h), 3.0, t.accent);
            return;
        }
        let (text, ink) = if queued {
            (tr("Waiting for its turn").to_string(), t.t3)
        } else if plan.paused {
            (tr("Paused: the schedule is off").to_string(), t.t3)
        } else {
            match &st.last {
                None => (tr("Never backed up").to_string(), t.t3),
                Some(l) if l.ok => {
                    let mut s = fill(
                        tr("Last backup {}: {} copied, {} unchanged"),
                        &[&i18n::when(l.time, now), &i18n::count(l.report.copied), &i18n::count(l.report.unchanged)],
                    );
                    if l.report.removed > 0 {
                        s.push_str(&fill(tr(", {} removed from the sources (kept)"), &[&i18n::count(l.report.removed)]));
                    }
                    if !l.report.errors.is_empty() {
                        s.push_str(&fill(tr(", {} skipped"), &[&i18n::count(l.report.errors.len() as u64)]));
                        (s, t.orange)
                    } else {
                        (s, t.green)
                    }
                }
                Some(l) => {
                    let f = crate::settings::fail_from(&l.fail);
                    let ink = if f == Fail::Stopped { t.t3 } else { t.red };
                    (fill(tr("{}: {}"), &[&i18n::when(l.time, now), &App::fail_text(&f)]), ink)
                }
            }
        };
        let mut line = text;
        if self.present.get(&plan.id) == Some(&false) && !queued && plan.schedule != Schedule::Manual {
            line = format!("{}  ·  {line}", tr("Waiting for the destination"));
        }
        c.fill_round(rect(tx, r.y + 103.0, 8.0, 8.0), 4.0, ink);
        c.text(&line, rect(tx + 16.0, r.y + 96.0, full - 16.0, 22.0), Font::Small, if ink == t.t3 { t.t3 } else { t.t1 }, Align::Left);
    }

    // ───────────────────────── the editor ─────────────────────────

    fn section(&self, c: &mut Canvas, x: f32, y: f32, title: &str) -> f32 {
        c.text(title, rect(x + 4.0, y, 600.0, 24.0), Font::Title, self.theme.t1, Align::Left);
        y + 32.0
    }

    fn paint_edit(&mut self, c: &mut Canvas, cx: &PaintCx, x0: f32, cw: f32) {
        let t = self.theme.clone();
        let Some(d) = &self.draft else { return };
        let (is_new, plan) = (d.index.is_none(), d.plan.clone());
        let top = self.heading(c, x0, cw, if is_new { tr("New plan") } else { tr("Edit the plan") }, "") - 30.0;
        let bar_h = 68.0;
        let area = rect(x0 - 6.0, top, cw + 12.0, cx.h - top - bar_h);
        let mut y = self.begin_zone(c, Zone::Form, area);
        let (x, w) = (x0, cw);

        // name
        y = self.section(c, x, y, tr("Name"));
        let card = rect(x, y, w, 64.0);
        self.card(c, card);
        self.field(c, rect(x + 20.0, y + 15.0, w - 40.0, 34.0), Field::Name, tr("Name of the plan"));
        y += 64.0 + 20.0;

        // sources
        y = self.section(c, x, y, tr("Folders to back up"));
        let n = plan.sources.len();
        let h = 12.0 + n.max(1) as f32 * 40.0 + 56.0;
        self.card(c, rect(x, y, w, h));
        let mut ry = y + 12.0;
        if n == 0 {
            c.text(tr("No folder yet."), rect(x + 20.0, ry, w - 40.0, 40.0), Font::Body, t.t3, Align::Left);
        }
        for (k, s) in plan.sources.iter().enumerate() {
            c.icon(&icons::FOLDER, rect(x + 20.0, ry + 12.0, 16.0, 16.0), t.accent);
            c.text(&s.display().to_string(), rect(x + 46.0, ry, w - 46.0 - 64.0, 40.0), Font::Body, t.t1, Align::Left);
            self.button_icon(c, rect(x + w - 20.0 - 32.0, ry + 4.0, 32.0, 32.0), Some(&icons::CLOSE), "", Style::Plain, Id::RemoveSource(k));
            if k + 1 < n {
                c.hline(x + 46.0, ry + 40.0, w - 66.0, t.sep);
            }
            ry += 40.0;
        }
        let label = tr("Add a folder…");
        let bw = Self::btn_w(c, label, true);
        self.button_icon(c, rect(x + 20.0, ry + 10.0, bw, 32.0), Some(&icons::PLUS), label, Style::Plain, Id::AddSource);
        y += h + 20.0;

        // destination
        y = self.section(c, x, y, tr("Save to"));
        let hint = tr(
            "A USB drive, another disk or folder, or a network share such as \\\\PC\\Backups. A USB drive is found again even if Windows gives it another letter.",
        );
        let hint_h = c.text_height(hint, w - 40.0, Font::Small);
        let place = match sys::place(&plan.dest) {
            _ if plan.dest.as_os_str().is_empty() => "",
            Place::Removable => tr("Removable drive"),
            Place::Network => tr("Network share"),
            Place::Fixed => tr("Disk of this PC"),
            Place::Unknown => "",
        };
        let h = 15.0 + 34.0 + 12.0 + hint_h + 16.0;
        self.card(c, rect(x, y, w, h));
        let label = tr("Choose…");
        let bw = Self::btn_w(c, label, false);
        self.button(c, rect(x + w - 20.0 - bw, y + 16.0, bw, 32.0), label, Style::Plain, Id::ChooseDest);
        let mut fw = w - 40.0 - bw - 10.0;
        if !place.is_empty() {
            let pw = c.text_width(place, Font::SmallBold) + 20.0;
            fw -= pw + 8.0;
            let pr = rect(x + 20.0 + fw + 8.0, y + 21.0, pw, 24.0);
            c.fill_round(pr, 12.0, t.accent.fade(0.16));
            c.text(place, pr, Font::SmallBold, t.accent, Align::Center);
        }
        self.field(c, rect(x + 20.0, y + 15.0, fw, 34.0), Field::Dest, r"E:\Backups");
        c.text_block(hint, rect(x + 20.0, y + 15.0 + 34.0 + 10.0, w - 40.0, hint_h), Font::Small, t.t3);
        y += h + 20.0;

        // when
        y = self.section(c, x, y, tr("When"));
        let daily = plan.schedule == Schedule::Daily;
        let h = 16.0 + 38.0 + if daily { 54.0 } else { 16.0 };
        self.card(c, rect(x, y, w, h));
        let items: Vec<(String, Id, bool)> = [
            (tr("Every hour"), Schedule::Hourly),
            (tr("Every day"), Schedule::Daily),
            (tr("When the drive is plugged in"), Schedule::Plug),
            (tr("Only when I ask"), Schedule::Manual),
        ]
        .into_iter()
        .map(|(l, s)| (l.to_string(), Id::Sched(s), plan.schedule == s))
        .collect();
        self.segments(c, rect(x + 20.0, y + 16.0, w - 40.0, 38.0), &items);
        if daily {
            self.stepper_row(c, rect(x, y + 62.0, w, 0.0), tr("From"), &format!("{:02}:00", plan.hour), Id::HourDown, Id::HourUp);
        }
        y += h + 20.0;

        // how
        y = self.section(c, x, y, tr("How"));
        let explain = match plan.mode {
            Mode::Versions => {
                tr("Each backup is a complete dated copy you can go back to. Files that did not change are shared between copies, so they take no extra space.")
            }
            Mode::Mirror => {
                tr("One copy, kept identical to your folders. Files you delete are moved to a dated “deleted” folder at the destination, never simply erased.")
            }
        };
        let eh = c.text_height(explain, w - 40.0, Font::Small);
        let rows = if plan.mode == Mode::Versions { 3.0 } else { 1.0 };
        let h = 16.0 + 38.0 + 10.0 + eh + 8.0 + rows * 46.0 + 8.0;
        self.card(c, rect(x, y, w, h));
        let items = vec![
            (tr("Versions").to_string(), Id::SetMode(Mode::Versions), plan.mode == Mode::Versions),
            (tr("Mirror").to_string(), Id::SetMode(Mode::Mirror), plan.mode == Mode::Mirror),
        ];
        self.segments(c, rect(x + 20.0, y + 16.0, w - 40.0, 38.0), &items);
        c.text_block(explain, rect(x + 20.0, y + 64.0, w - 40.0, eh), Font::Small, t.t3);
        let mut ry = y + 64.0 + eh + 8.0;
        if plan.mode == Mode::Versions {
            let keep = if plan.keep_versions == 0 { tr("All").to_string() } else { plan.keep_versions.to_string() };
            ry += self.stepper_row(c, rect(x, ry, w, 0.0), tr("Versions kept"), &keep, Id::KeepDown, Id::KeepUp);
            ry += self.stepper_row(c, rect(x, ry, w, 0.0), tr("Oldest version kept"), &days_text(plan.keep_days, "No limit"), Id::DaysDown, Id::DaysUp);
        }
        self.stepper_row(c, rect(x, ry, w, 0.0), tr("Removed files kept"), &days_text(plan.deleted_days, "Forever"), Id::DeletedDown, Id::DeletedUp);
        y += h + 20.0;

        // leave out
        y = self.section(c, x, y, tr("Leave out"));
        let h = 15.0 + 34.0 + 8.0 + 62.0 * 2.0 + 6.0;
        self.card(c, rect(x, y, w, h));
        self.field(c, rect(x + 20.0, y + 15.0, w - 40.0, 34.0), Field::Excludes, tr("Patterns, for example *.iso, node_modules, Videos/Raw"));
        let mut ry = y + 15.0 + 34.0 + 8.0;
        ry += self.switch_row(
            c,
            rect(x, ry, w, 0.0),
            tr("Skip temporary files"),
            tr("*.tmp, ~$ files of Office, Thumbs.db, unfinished downloads…"),
            plan.skip_temp,
            Id::SkipTemp,
        );
        self.switch_row(
            c,
            rect(x, ry, w, 0.0),
            tr("Check each copy"),
            tr("Reads every copied file back and compares it with the original. Slower."),
            plan.verify,
            Id::Verify,
        );
        y += h + 20.0;

        // pause
        let h = 62.0;
        self.card(c, rect(x, y, w, h));
        self.switch_row(c, rect(x, y, w, 0.0), tr("Paused"), tr("While paused, the schedule does not start this backup."), plan.paused, Id::Paused);
        y += h + 16.0;
        self.end_zone(c, Zone::Form, y);

        // the bar at the bottom
        let bar = rect(SIDE, cx.h - bar_h, cx.w - SIDE, bar_h);
        c.fill(bar, t.bg);
        c.hline(bar.x, bar.y, bar.w, t.sep);
        let by = bar.y + (bar_h - 34.0) / 2.0;
        let mut bx = x0 + cw;
        for (label, style, id) in [(tr("Save"), Style::Accent, Id::Save), (tr("Cancel"), Style::Plain, Id::Cancel)] {
            let bw = Self::btn_w(c, label, false).max(96.0);
            bx -= bw;
            self.button(c, rect(bx, by, bw, 34.0), label, style, id);
            bx -= 10.0;
        }
        if !is_new {
            let label = tr("Delete the plan");
            let bw = Self::btn_w(c, label, true);
            self.button_icon(c, rect(x0, by, bw, 34.0), Some(&icons::TRASH), label, Style::Plain, Id::Delete);
        }
    }

    // ───────────────────────── restore ─────────────────────────

    fn snap_title(&self, i: usize) -> String {
        let Some(s) = self.snaps.get(i) else { return String::new() };
        let now = crate::engine::now();
        let label = i18n::when(s.time, now);
        // two backups in the same minute: the seconds tell them apart
        if self.snaps.iter().enumerate().any(|(k, o)| k != i && i18n::when(o.time, now) == label) {
            return format!("{label}:{:02}", sys::local(s.time).second);
        }
        label
    }

    fn paint_restore(&mut self, c: &mut Canvas, cx: &PaintCx, x0: f32, cw: f32) {
        let t = self.theme.clone();
        let mut top = self.heading(c, x0, cw, tr("Restore"), tr("Pick a backup by its date, then a file or a folder to put back."));
        if self.store.plans.is_empty() {
            let r = rect(x0, top, cw, 120.0);
            self.card(c, r);
            c.text(tr("No backup plan yet"), rect(r.x, r.y + 30.0, r.w, 26.0), Font::Large, t.t1, Align::Center);
            c.text(tr("Create a plan first: its backups will show here."), rect(r.x, r.y + 62.0, r.w, 20.0), Font::Body, t.t3, Align::Center);
            return;
        }
        // the plans, as pills
        let mut px = x0;
        for i in 0..self.store.plans.len() {
            let name = self.store.plans[i].name.clone();
            let w = c.text_width(&name, Font::BodyBold) + 28.0;
            if px + w > x0 + cw {
                break;
            }
            let r = rect(px, top, w, 32.0);
            let on = i == self.rplan;
            let id = Id::RPlan(i);
            let fill = if on {
                t.accent.fade(0.16)
            } else if self.hover == Some(id) {
                t.fill2
            } else {
                t.fill
            };
            c.fill_round(r, 16.0, fill);
            c.text(&name, r, Font::BodyBold, if on { self.accent_ink() } else { t.t1 }, Align::Center);
            self.hit(r, id);
            px += w + 8.0;
        }
        top += 48.0;
        let h = cx.h - top - 24.0;

        // the dates
        let lw = 252.0;
        let left = rect(x0, top, lw, h);
        self.card(c, left);
        c.text(tr("Backups"), rect(left.x + 20.0, left.y + 14.0, lw - 40.0, 24.0), Font::Title, t.t1, Align::Left);
        let la = rect(left.x + 8.0, left.y + 48.0, lw - 16.0, h - 56.0);
        let msg = match self.browse {
            Browse::Loading if self.snaps.is_empty() => Some(tr("Looking for the backups…")),
            Browse::Missing => Some(tr("The destination is not available: plug in the drive or check the network share.")),
            Browse::Ready if self.snaps.is_empty() => Some(tr("No backup yet.")),
            _ => None,
        };
        if let Some(msg) = msg {
            let mh = c.text_height(msg, la.w - 24.0, Font::Body);
            c.text_block(msg, rect(la.x + 12.0, la.y + 4.0, la.w - 24.0, mh), Font::Body, t.t2);
        } else {
            let mut y = self.begin_zone(c, Zone::Snaps, la);
            for i in 0..self.snaps.len() {
                let r = rect(la.x, y, la.w, 48.0);
                let s = self.snaps[i].clone();
                let id = Id::Snap(i);
                let on = i == self.snap;
                if on {
                    c.fill_round(r, 10.0, t.accent.fade(0.16));
                } else if self.hover == Some(id) {
                    c.fill_round(r, 10.0, t.hover);
                }
                c.text(
                    &self.snap_title(i),
                    rect(r.x + 12.0, r.y + 5.0, r.w - 24.0, 20.0),
                    Font::BodyBold,
                    if on { self.accent_ink() } else { t.t1 },
                    Align::Left,
                );
                let kind = match s.kind {
                    Kind::Version => tr("Version"),
                    Kind::Mirror => tr("Mirror (latest)"),
                    Kind::Deleted => tr("Removed from the sources"),
                };
                let sub = if s.files > 0 { format!("{kind} · {}", i18n::files(s.files)) } else { kind.to_string() };
                c.text(&sub, rect(r.x + 12.0, r.y + 25.0, r.w - 24.0, 18.0), Font::Small, if s.kind == Kind::Deleted { t.orange } else { t.t3 }, Align::Left);
                self.hit(r, id);
                y += 50.0;
            }
            self.end_zone(c, Zone::Snaps, y);
        }

        // the files
        let right = rect(x0 + lw + 14.0, top, cw - lw - 14.0, h);
        self.card(c, right);
        if self.snaps.get(self.snap).is_none() {
            return;
        }
        let rx = right.x + 16.0;
        let rw = right.w - 32.0;
        self.button_icon(c, rect(rx, right.y + 12.0, 32.0, 32.0), Some(&icons::BACK), "", Style::Plain, Id::Up);
        // crumbs: the date, then the folders
        let mut cx_ = rx + 42.0;
        let mut crumbs = vec![self.snap_title(self.snap)];
        crumbs.extend(self.path.iter().cloned());
        let count = crumbs.len();
        for (k, name) in crumbs.into_iter().enumerate() {
            let last = k + 1 == count;
            let w = (c.text_width(&name, Font::BodyBold) + 16.0).min(220.0);
            if cx_ + w > right.right() - 14.0 {
                break;
            }
            let r = rect(cx_, right.y + 12.0, w, 32.0);
            let id = Id::Crumb(k);
            if self.hover == Some(id) && !last {
                c.fill_round(r, 10.0, t.hover);
            }
            c.text(&name, r, Font::BodyBold, if last { t.t1 } else { t.t3 }, Align::Center);
            if !last {
                self.hit(r, id);
                c.icon(&icons::CHEVRON_RIGHT, rect(r.right() + 1.0, r.y + 10.0, 12.0, 12.0), t.t3);
            }
            cx_ += w + 14.0;
        }
        // columns
        let hy = right.y + 56.0;
        let (size_w, date_w) = (90.0, 150.0);
        let name_w = rw - size_w - date_w - 40.0;
        c.text(tr("Name"), rect(rx + 40.0, hy, name_w, 22.0), Font::Small, t.t3, Align::Left);
        c.text(tr("Size"), rect(rx + 40.0 + name_w, hy, size_w, 22.0), Font::Small, t.t3, Align::Right);
        c.text(tr("Modified"), rect(rx + 40.0 + name_w + size_w + 16.0, hy, date_w, 22.0), Font::Small, t.t3, Align::Left);
        c.hline(rx, hy + 24.0, rw, t.sep);
        let bottom_h = 64.0;
        let ra = rect(right.x + 6.0, hy + 26.0, right.w - 12.0, right.bottom() - bottom_h - hy - 26.0);
        let now = crate::engine::now();
        let mut y = self.begin_zone(c, Zone::Rows, ra);
        if self.rows.is_empty() {
            c.text(tr("This folder is empty."), rect(rx + 40.0, y + 8.0, rw, 24.0), Font::Body, t.t3, Align::Left);
        }
        for i in 0..self.rows.len() {
            let r = rect(ra.x, y, ra.w, 40.0);
            if r.bottom() > ra.y && r.y < ra.bottom() {
                let e = self.rows[i].clone();
                let id = Id::Row(i);
                let on = self.sel == Some(i);
                if on {
                    c.fill_round(r, 10.0, t.accent.fade(0.16));
                } else if self.hover == Some(id) {
                    c.fill_round(r, 10.0, t.hover);
                }
                let ink = if e.dir || on { self.accent_ink() } else { t.t2 };
                c.icon(if e.dir { &icons::FOLDER } else { &icons::FILE }, rect(rx + 12.0, r.y + 12.0, 16.0, 16.0), ink);
                c.text(&e.name, rect(rx + 40.0, r.y, name_w - 8.0, 40.0), Font::Body, t.t1, Align::Left);
                if !e.dir {
                    c.text(&i18n::size(e.size), rect(rx + 40.0 + name_w, r.y, size_w, 40.0), Font::Small, t.t2, Align::Right);
                }
                c.text(&i18n::when(e.modified, now), rect(rx + 40.0 + name_w + size_w + 16.0, r.y, date_w, 40.0), Font::Small, t.t2, Align::Left);
                self.hit(r, id);
            }
            y += 40.0;
        }
        self.end_zone(c, Zone::Rows, y);
        // what will be restored, and how
        let by = right.bottom() - bottom_h;
        c.hline(rx, by, rw, t.sep);
        let (l1, l2) = (tr("Restore to its place"), tr("Restore to…"));
        let (w1, w2) = (Self::btn_w(c, l1, false), Self::btn_w(c, l2, false));
        let bx = right.right() - 16.0 - w2;
        self.button(c, rect(bx, by + 15.0, w2, 34.0), l2, Style::Plain, Id::RestoreTo);
        self.button(c, rect(bx - 10.0 - w1, by + 15.0, w1, 34.0), l1, Style::Accent, Id::RestoreHere);
        let what = match (self.restoring, self.restore_rel()) {
            (true, _) => tr("Restoring…").to_string(),
            (_, Some(rel)) => rel.display().to_string(),
            (_, None) => tr("Choose a file or a folder.").to_string(),
        };
        c.text(&what, rect(rx + 4.0, by + 15.0, bx - 10.0 - w1 - rx - 16.0, 34.0), Font::Small, t.t2, Align::Left);
    }

    // ───────────────────────── settings ─────────────────────────

    fn paint_settings(&mut self, c: &mut Canvas, cx: &PaintCx, x0: f32, cw: f32) {
        let t = self.theme.clone();
        let top = self.heading(c, x0, cw, tr("Settings"), "") - 30.0;
        let area = rect(x0 - 6.0, top, cw + 12.0, cx.h - top);
        let mut y = self.begin_zone(c, Zone::Settings, area);
        let (x, w) = (x0, cw);

        y = self.section(c, x, y, tr("In the background"));
        self.card(c, rect(x, y, w, 124.0));
        let (autostart, background) = (self.autostart, self.set.background);
        self.switch_row(
            c,
            rect(x, y, w, 0.0),
            tr("Start with Windows"),
            tr("In the background, so scheduled backups run without opening the window."),
            autostart,
            Id::Autostart,
        );
        c.hline(x + 20.0, y + 62.0, w - 40.0, t.sep);
        self.switch_row(
            c,
            rect(x, y + 62.0, w, 0.0),
            tr("Keep running near the clock"),
            tr("When the window is closed, backups keep their schedule."),
            background,
            Id::Background,
        );
        y += 124.0 + 20.0;

        y = self.section(c, x, y, tr("Notifications"));
        self.card(c, rect(x, y, w, 96.0));
        let (ok, bad) = (self.set.notify_ok, self.set.notify_fail);
        self.switch_row(c, rect(x, y, w, 0.0), tr("When a backup is done"), "", ok, Id::NotifyOk);
        c.hline(x + 20.0, y + 48.0, w - 40.0, t.sep);
        self.switch_row(c, rect(x, y + 48.0, w, 0.0), tr("When a backup fails or skips files"), "", bad, Id::NotifyFail);
        y += 96.0 + 20.0;

        y = self.section(c, x, y, tr("Appearance"));
        self.card(c, rect(x, y, w, 120.0));
        let half = (w - 40.0) / 2.0;
        c.text(tr("Language"), rect(x + 20.0, y + 10.0, half, 40.0), Font::Body, t.t1, Align::Left);
        let lang = ["system", "en", "fr"].iter().position(|l| *l == self.set.lang).unwrap_or(0);
        let items: Vec<(String, Id, bool)> =
            [tr("System"), "English", "Français"].iter().enumerate().map(|(k, l)| (l.to_string(), Id::Lang(k as u8), k == lang)).collect();
        self.segments(c, rect(x + w - 20.0 - 320.0, y + 12.0, 320.0, 36.0), &items);
        c.text(tr("Theme"), rect(x + 20.0, y + 66.0, half, 40.0), Font::Body, t.t1, Align::Left);
        let th = ["system", "dark", "light"].iter().position(|l| *l == self.set.theme).unwrap_or(0);
        let items: Vec<(String, Id, bool)> =
            [tr("System"), tr("Dark"), tr("Light")].iter().enumerate().map(|(k, l)| (l.to_string(), Id::Theme(k as u8), k == th)).collect();
        self.segments(c, rect(x + w - 20.0 - 320.0, y + 68.0, 320.0, 36.0), &items);
        y += 120.0 + 20.0;

        y = self.section(c, x, y, tr("About"));
        let about = tr(
            "Your backups go only where you choose: a drive, a folder, or a share on your network. Plume Backup never sends anything online, and it never changes or deletes anything in the folders it backs up.",
        );
        let ah = c.text_height(about, w - 40.0, Font::Body);
        let h = 18.0 + 24.0 + 8.0 + ah + 18.0;
        self.card(c, rect(x, y, w, h));
        c.text(&format!("Plume Backup {}", plume_update::VERSION), rect(x + 20.0, y + 16.0, w - 40.0, 24.0), Font::Title, t.t1, Align::Left);
        c.text_block(about, rect(x + 20.0, y + 48.0, w - 40.0, ah), Font::Body, t.t2);
        y += h + 16.0;
        self.end_zone(c, Zone::Settings, y);
    }

    // ───────────────────────── over everything ─────────────────────────

    fn paint_toast(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let Some((text, _)) = &self.toast else { return };
        let t = &self.theme;
        let max = (cx.w - SIDE - 60.0).max(200.0);
        let one = c.text_width(text, Font::Body) + 40.0;
        let (tw, th) = if one <= max { (one, 40.0) } else { (max, c.text_height(text, max - 40.0, Font::Body) + 20.0) };
        let r = rect(SIDE + (cx.w - SIDE - tw) / 2.0, cx.h - th - 84.0, tw, th);
        let rad = (th / 2.0).min(20.0);
        c.fill_round(r, rad, t.solid2);
        c.stroke_round(r, rad, t.ring, 1.0);
        if one <= max {
            c.text(text, r.inset_xy(16.0, 0.0), Font::Body, t.t1, Align::Center);
        } else {
            c.text_block(text, rect(r.x + 20.0, r.y + 10.0, tw - 40.0, th - 20.0), Font::Body, t.t1);
        }
    }

    fn paint_confirm(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let t = self.theme.clone();
        let name = self.draft.as_ref().map(|d| d.plan.name.clone()).unwrap_or_default();
        c.fill(rect(0.0, m::TITLE_H, cx.w, cx.h - m::TITLE_H), Color::hex(0).fade(0.45));
        // the question covers the window: nothing behind it is clickable
        self.hits.clear();
        let w = (cx.w - 80.0).min(460.0);
        let sub = tr("The plan goes away. Its backups stay on the destination, where you can still open them.");
        let sh = c.text_height(sub, w - 64.0, Font::Body);
        let h = 28.0 + 26.0 + 12.0 + sh + 28.0 + 36.0 + 24.0;
        let card = rect((cx.w - w) / 2.0, (cx.h - h) / 2.0, w, h);
        c.fill_round(card, 20.0, if t.dark { t.solid2 } else { t.solid });
        c.stroke_round(card, 20.0, t.ring, 1.0);
        c.text(&fill(tr("Delete the plan “{}”?"), &[&name]), rect(card.x + 24.0, card.y + 28.0, w - 48.0, 26.0), Font::Large, t.t1, Align::Center);
        c.text_block(sub, rect(card.x + 32.0, card.y + 66.0, w - 64.0, sh), Font::Body, t.t2);
        let bw = (w - 48.0 - 12.0) / 2.0;
        let by = card.bottom() - 24.0 - 36.0;
        self.button(c, rect(card.x + 24.0, by, bw, 36.0), tr("Cancel"), Style::Plain, Id::DeleteNo);
        self.button(c, rect(card.x + 24.0 + bw + 12.0, by, bw, 36.0), tr("Delete"), Style::Red, Id::DeleteYes);
        let cap_x = cx.w - 3.0 * m::CAPTION_W;
        for (k, id) in [Id::Min, Id::Max, Id::Close].into_iter().enumerate() {
            self.hits.add(rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, m::TITLE_H - 8.0), id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_texts() {
        crate::i18n::set("en");
        let mut p = Plan { schedule: Schedule::Daily, hour: 9, keep_versions: 30, ..Default::default() };
        assert_eq!(schedule_text(&p), "Every day from 09:00");
        assert_eq!(mode_text(&p), "Versions: 30 kept");
        p.keep_days = 60;
        p.keep_versions = 0;
        assert_eq!(mode_text(&p), "Versions: all kept, 60 days at most");
        assert_eq!(days_text(0, "Forever"), "Forever");
    }
}
