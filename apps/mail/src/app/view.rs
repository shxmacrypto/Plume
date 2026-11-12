//! Le dessin de la fenêtre : barre d'outils, comptes et dossiers, liste des messages, message ouvert, barre d'état.
//! Tout est redessiné à chaque image à partir de l'état ; les zones cliquables sont inscrites au passage.

use super::{App, Editing, Id, Lay, ROW_H, TOP, forms::size_text};
use mail_core::{engine::Work, imap::Role};
use plume_ui::{Align, Canvas, Color, Font, PaintCx, Rect, Theme, icons, m, rect};
use windows::Win32::{
    Foundation::{FILETIME, SYSTEMTIME},
    System::{
        SystemInformation::GetLocalTime,
        Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime},
    },
};
use windows::{
    Win32::Graphics::Imaging::{
        GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut,
        WICDecodeMetadataCacheOnLoad,
    },
    core::Interface,
};

const LOGO: &[u8] = include_bytes!("../../../../assets/brand/plume-mail-256.png");

/// Décode le logo intégré au programme en image prête pour Direct2D.
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

/// Un instant (secondes depuis 1970) à l'heure d'ici : (année, mois, jour, heure, minute).
fn local(ts: i64) -> Option<(u16, u16, u16, u16, u16)> {
    let ft = (ts.max(0) as u64 + 11_644_473_600) * 10_000_000;
    let f = FILETIME { dwLowDateTime: ft as u32, dwHighDateTime: (ft >> 32) as u32 };
    let (mut utc, mut here) = (SYSTEMTIME::default(), SYSTEMTIME::default());
    unsafe {
        FileTimeToSystemTime(&f, &mut utc).ok()?;
        if SystemTimeToTzSpecificLocalTime(None, &utc, &mut here).is_err() {
            here = utc;
        }
    }
    Some((here.wYear, here.wMonth, here.wDay, here.wHour, here.wMinute))
}

impl App {
    /// La date d'un message. Dans la liste : l'heure pour aujourd'hui, « Hier », le jour pour cette année, la date
    /// entière avant. `long` : toujours la date et l'heure entières.
    pub fn date_text(&self, ts: i64, long: bool) -> String {
        let Some((y, mo, d, h, mi)) = local(ts).filter(|_| ts > 0) else { return String::new() };
        if long {
            return format!("{y}-{mo:02}-{d:02} {h:02}:{mi:02}");
        }
        let s = self.s;
        let now = unsafe { GetLocalTime() };
        let month = s.months[(mo as usize).clamp(1, 12) - 1];
        if (y, mo, d) == (now.wYear, now.wMonth, now.wDay) {
            format!("{h:02}:{mi:02}")
        } else if (y, mo, d + 1) == (now.wYear, now.wMonth, now.wDay) {
            s.yesterday.to_string()
        } else if y == now.wYear {
            match s.lang {
                super::i18n::Lang::Fr => format!("{d} {month}"),
                super::i18n::Lang::En => format!("{month} {d}"),
            }
        } else {
            format!("{y}-{mo:02}-{d:02}")
        }
    }

    pub fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let t = self.theme.clone();
        self.hits.clear();
        c.clear(t.bg);
        let (w, h) = (cx.w, cx.h);
        let body_h = (h - TOP - m::STATUS_H).max(0.0);
        let nav_w = 236.0f32.min(w * 0.3);
        let list_w = ((w - nav_w) * 0.4).clamp(240.0, 440.0).min((w - nav_w).max(0.0));
        let search = rect((w - 52.0 - 46.0 - 260.0).max(nav_w + 380.0), m::TITLE_H + 6.0, 260.0, m::FIELD_H);
        self.lay = Lay {
            w,
            h,
            nav: rect(0.0, TOP, nav_w, body_h),
            list: rect(nav_w, TOP, list_w, body_h),
            read: rect(nav_w + list_w, TOP, (w - nav_w - list_w).max(0.0), body_h),
            status: rect(0.0, h - m::STATUS_H, w, m::STATUS_H),
            search,
            search_text: rect(search.x + 32.0, search.y, (search.w - 60.0).max(20.0), search.h),
            list_max: 0.0,
        };
        self.paint_reading(c, &t);
        self.paint_list(c, &t);
        self.paint_nav(c, &t);
        self.paint_head(c, cx, &t);
        self.paint_status(c, &t);
        self.paint_hint(c, &t);
        self.paint_menu(c, &t);
        self.paint_sheet(c, &t);
        self.paint_toast(c, &t);
    }

    fn paint_head(&mut self, c: &mut Canvas, cx: &mut PaintCx, t: &Theme) {
        let s = self.s;
        let w = cx.w;
        c.gradient_v(rect(0.0, 0.0, w, TOP), t.hdr1, t.hdr2);
        c.hline(0.0, TOP - 1.0, w, t.edge);
        // ── le nom de l'application, puis les boutons de la fenêtre
        let wic = self.wic.clone();
        c.image(1, rect(14.0, (m::TITLE_H - 18.0) / 2.0, 18.0, 18.0), 1.0, || png(&wic, LOGO));
        c.text("Plume Mail", rect(40.0, 0.0, 200.0, m::TITLE_H), Font::BodyBold, if cx.active { t.t1 } else { t.t3 }, Align::Left);
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
        // ── une version plus récente de la suite : le bouton qui met tout à jour (dans Plume Suite)
        let _ = match self.update.clone() {
            Some(v) => {
                let label = self.s.update_to.replace("{v}", &v);
                let pw = c.text_width(&label, Font::SmallBold) + 26.0;
                let r = rect(cap_x - pw - 10.0, (m::TITLE_H - 26.0) / 2.0, pw, 26.0);
                let fill = if self.hover == Some(Id::Update) { t.accent.mix(Color::hex(0xffffff), 0.12) } else { t.accent };
                c.fill_round(r, 13.0, fill);
                c.text(&label, r, Font::SmallBold, Color::hex(0xffffff), Align::Center);
                self.hits.add(r, Id::Update);
                r.x - 6.0
            }
            None => cap_x,
        };
        // ── la barre d'outils : écrire, puis ce que l'on fait du message ouvert
        let y = m::TITLE_H + 6.0;
        let label_w = c.text_width(s.new_message, Font::BodyBold);
        let new = rect(12.0, y, label_w + 46.0, m::FIELD_H);
        let hot = self.hover == Some(Id::New);
        let fill = match (hot, self.pressed == Some(Id::New)) {
            (true, true) => t.accent.mix(Color::hex(0), 0.18),
            (true, false) => t.accent.mix(Color::hex(0xffffff), 0.1),
            (false, _) => t.accent,
        };
        c.fill_round(new, new.h / 2.0, fill);
        c.icon(&icons::PENCIL, rect(new.x + 12.0, new.y + 8.0, 16.0, 16.0), Color::hex(0xffffff));
        c.text(s.new_message, rect(new.x + 34.0, new.y, label_w + 4.0, new.h), Font::BodyBold, Color::hex(0xffffff), Align::Left);
        self.hits.add(new, Id::New);
        let open = self.reading.as_ref().is_some_and(|r| r.message.is_some());
        let picked = self.selected().map(|m| m.flags);
        let mut x = new.right() + 14.0;
        let groups: [&[(Id, &icons::Icon, bool)]; 3] = [
            &[(Id::Reply, &icons::REPLY, open), (Id::ReplyAll, &icons::REPLY_ALL, open), (Id::Forward, &icons::MAIL_FORWARD, open)],
            &[(Id::Delete, &icons::TRASH, picked.is_some())],
            &[(Id::Unread, &icons::MAIL, picked.is_some()), (Id::Flag, &icons::STAR, picked.is_some())],
        ];
        for (g, group) in groups.into_iter().enumerate() {
            if g > 0 {
                c.vline(x + 5.0, y + 7.0, m::FIELD_H - 14.0, t.sep);
                x += 12.0;
            }
            for &(id, icon, enabled) in group {
                let r = rect(x, y, 36.0, m::FIELD_H);
                if enabled && self.hover == Some(id) {
                    c.fill_round(r, 8.0, t.hover);
                }
                // un message suivi : l'étoile se remplit
                let on = id == Id::Flag && picked.is_some_and(|f| f.flagged);
                let col = if !enabled {
                    t.t4
                } else if on {
                    t.orange
                } else {
                    t.t2
                };
                if on {
                    c.icon_fill(icon, r.centered(16.0, 16.0), col)
                } else {
                    c.icon(icon, r.centered(16.0, 16.0), col)
                }
                if enabled {
                    self.hits.add(r, id);
                }
                x += 38.0;
            }
        }
        // ── à droite : chercher, relever le courrier, le menu
        let sr = self.lay.search;
        let searching = self.edit == Some(Editing::Search);
        c.fill_round(sr, m::FIELD_R, t.fill);
        if searching {
            c.stroke_round(sr, m::FIELD_R, t.accent, 1.5);
        }
        c.icon(&icons::SEARCH, rect(sr.x + 10.0, sr.y + 8.0, 16.0, 16.0), t.t3);
        if !searching {
            let (text, col) = if self.filter.is_empty() { (s.search, t.t3) } else { (self.filter.as_str(), t.t1) };
            c.text(text, self.lay.search_text, Font::Body, col, Align::Left);
        }
        self.hits.add(sr, Id::Search);
        if !self.filter.is_empty() {
            let clear = rect(sr.right() - 28.0, sr.y + 4.0, 24.0, 24.0);
            if self.hover == Some(Id::SearchClear) {
                c.fill_round(clear, 12.0, t.fill2);
            }
            c.icon(&icons::CLOSE, clear.centered(9.0, 9.0), t.t3);
            self.hits.add(clear, Id::SearchClear);
        }
        for (k, (id, icon)) in [(Id::Sync, &icons::RELOAD), (Id::More, &icons::MORE)].into_iter().enumerate() {
            let r = rect(sr.right() + 8.0 + k as f32 * 40.0, y, 36.0, m::FIELD_H);
            if self.hover == Some(id) {
                c.fill_round(r, 8.0, t.hover);
            }
            c.icon(icon, r.centered(16.0, 16.0), t.t2);
            self.hits.add(r, id);
        }
    }

    fn paint_nav(&mut self, c: &mut Canvas, t: &Theme) {
        let nav = self.lay.nav;
        if nav.w <= 0.0 {
            return;
        }
        let s = self.s;
        c.fill(nav, t.bg2);
        c.vline(nav.right() - 1.0, nav.y, nav.h, t.sep);
        c.clip(nav);
        let unread = self.messages.iter().filter(|m| !m.flags.seen).count();
        let mut y = nav.y + 10.0;
        for b in 0..self.boxes.len() {
            // le compte : un clic ouvre sa boîte de réception
            let r = rect(nav.x + 6.0, y, nav.w - 12.0, m::NAV_ROW_H);
            if self.hover == Some(Id::Account(b)) {
                c.fill_round(r, m::ROW_R, t.hover);
            }
            let mb = &self.boxes[b];
            c.icon(&icons::USER, rect(r.x + 8.0, r.y + 7.0, 16.0, 16.0), t.t3);
            c.text(&mb.account.email, rect(r.x + 32.0, r.y, r.w - 52.0, r.h), Font::SmallBold, t.t2, Align::Left);
            if mb.error.is_some() {
                c.fill_round(rect(r.right() - 16.0, r.y + 11.0, 8.0, 8.0), 4.0, t.red);
            }
            self.hits.add(r, Id::Account(b));
            y += m::NAV_ROW_H;
            for f in 0..self.boxes[b].folders.len() {
                let folder = &self.boxes[b].folders[f];
                let r = rect(nav.x + 6.0, y, nav.w - 12.0, m::NAV_ROW_H);
                let current = self.cur.as_ref().is_some_and(|(cb, raw)| *cb == b && *raw == folder.raw);
                if current {
                    c.fill_round(r, m::ROW_R, t.fill);
                } else if !folder.no_select && self.hover == Some(Id::Folder(b, f)) {
                    c.fill_round(r, m::ROW_R, t.hover);
                }
                let icon = match folder.role {
                    Role::Inbox => &icons::INBOX,
                    Role::Sent => &icons::SEND,
                    Role::Drafts => &icons::FILE,
                    Role::Trash => &icons::TRASH,
                    Role::Junk => &icons::JUNK,
                    Role::Archive | Role::All => &icons::ARCHIVE,
                    Role::Other => &icons::FOLDER,
                };
                let x = r.x + 22.0 + folder.depth as f32 * 14.0;
                c.icon(icon, rect(x, r.y + 7.0, 16.0, 16.0), if current { t.accent } else { t.t3 });
                let label = self.folder_label(folder);
                let count = if current && unread > 0 { unread.to_string() } else { String::new() };
                let cw = if count.is_empty() { 0.0 } else { c.text_width(&count, Font::SmallBold) + 14.0 };
                let ink = match (folder.no_select, current) {
                    (true, _) => t.t4,
                    (false, true) => t.t1,
                    (false, false) => t.t2,
                };
                c.text(&label, rect(x + 24.0, r.y, (r.right() - x - 30.0 - cw).max(10.0), r.h), Font::Body, ink, Align::Left);
                if !count.is_empty() {
                    c.text(&count, rect(r.right() - cw - 4.0, r.y, cw, r.h), Font::SmallBold, t.accent, Align::Right);
                }
                if !folder.no_select {
                    self.hits.add(r, Id::Folder(b, f));
                }
                y += m::NAV_ROW_H;
            }
            y += 10.0;
        }
        let r = rect(nav.x + 6.0, y, nav.w - 12.0, m::NAV_ROW_H);
        if self.hover == Some(Id::AddAccount) {
            c.fill_round(r, m::ROW_R, t.hover);
        }
        c.icon(&icons::PLUS, rect(r.x + 8.0, r.y + 7.0, 16.0, 16.0), t.t3);
        c.text(s.add_account, rect(r.x + 32.0, r.y, r.w - 40.0, r.h), Font::Body, t.t3, Align::Left);
        self.hits.add(r, Id::AddAccount);
        c.unclip();
    }

    fn paint_list(&mut self, c: &mut Canvas, t: &Theme) {
        let list = self.lay.list;
        let s = self.s;
        c.fill(list, t.bg);
        c.vline(list.right() - 1.0, list.y, list.h, t.sep);
        self.hits.add(list, Id::List);
        if self.shown.is_empty() {
            let text = if self.boxes.is_empty() {
                s.welcome_text
            } else if self.filter.trim().is_empty() {
                s.no_messages
            } else {
                s.no_match
            };
            let y = list.y + list.h * 0.34;
            if self.boxes.is_empty() {
                c.icon(&icons::MAIL, rect(list.x + (list.w - 20.0) / 2.0, y - 60.0, 20.0, 20.0), t.t2);
                c.text(s.welcome, rect(list.x, y - 30.0, list.w, 24.0), Font::Title, t.t1, Align::Center);
            }
            c.text(text, rect(list.x + 16.0, y, list.w - 32.0, 22.0), Font::Body, t.t3, Align::Center);
            if self.boxes.is_empty() {
                let bw = c.text_width(s.add_account, Font::BodyBold) + 40.0;
                let b = rect(list.x + ((list.w - bw) / 2.0).round(), y + 38.0, bw, 34.0);
                let hot = self.hover == Some(Id::AddAccount);
                let fill = match (hot, self.pressed == Some(Id::AddAccount)) {
                    (true, true) => t.accent.mix(Color::hex(0), 0.18),
                    (true, false) => t.accent.mix(Color::hex(0xffffff), 0.1),
                    (false, _) => t.accent,
                };
                c.fill_round(b, b.h / 2.0, fill);
                c.text(s.add_account, b, Font::BodyBold, Color::hex(0xffffff), Align::Center);
                self.hits.add(b, Id::AddAccount);
            }
            return;
        }
        let total = self.shown.len() as f32 * ROW_H;
        self.lay.list_max = (total - list.h).max(0.0);
        self.list_scroll = self.list_scroll.clamp(0.0, self.lay.list_max);
        let scroll = c.snap(self.list_scroll);
        // dans « Envoyés » et « Brouillons », la personne qui compte est le destinataire
        let outgoing = self.cur.as_ref().is_some_and(|(b, raw)| {
            self.boxes.get(*b).is_some_and(|mb| mb.folders.iter().any(|f| f.raw == *raw && matches!(f.role, Role::Sent | Role::Drafts)))
        });
        c.clip(list);
        let mut toggles: Vec<(Rect, usize)> = vec![];
        let first = (scroll / ROW_H).floor() as usize;
        let last = (((scroll + list.h) / ROW_H).ceil() as usize).min(self.shown.len());
        for row in first..last {
            let msg = &self.messages[self.shown[row]];
            let info = self.rows.get(row).cloned().unwrap_or_default();
            let r = rect(list.x, list.y + row as f32 * ROW_H - scroll, list.w - 1.0, ROW_H);
            let selected = self.sel == Some(msg.uid);
            if selected {
                c.fill_round(r.inset_xy(6.0, 2.0), 10.0, t.selection(self.active));
            } else if self.hover == Some(Id::Row(row)) {
                c.fill_round(r.inset_xy(6.0, 2.0), 10.0, t.hover);
            }
            // une conversation fermée montre le non-lu de tous ses messages
            let closed = info.count > 1 && !self.open_threads.contains(&info.key);
            let unread = if closed { info.unread } else { !msg.flags.seen };
            // les messages plus anciens d'une conversation ouverte, en retrait sous elle
            let indent = if info.child { 18.0 } else { 0.0 };
            if info.child {
                c.vline(r.x + 16.0, r.y, r.h, t.sep);
            }
            if unread {
                c.fill_round(rect(r.x + 10.0 + indent, r.y + 15.0, 8.0, 8.0), 4.0, t.accent);
            }
            let who = if outgoing {
                format!("{} {}", s.to, msg.to)
            } else if msg.from_name.is_empty() {
                msg.from.clone()
            } else {
                msg.from_name.clone()
            };
            let date = self.date_text(msg.date, false);
            let dw = c.text_width(&date, Font::Small) + 4.0;
            let x = r.x + 26.0 + indent;
            // le nombre de messages de la conversation : un clic l'ouvre ou la ferme
            let mut pill_w = 0.0;
            if info.count > 1 && !info.child {
                let label = info.count.to_string();
                pill_w = c.text_width(&label, Font::SmallBold) + 16.0;
                let pill = rect(r.right() - dw - 12.0 - pill_w - 8.0, r.y + 9.0, pill_w, 19.0);
                let hot = self.hover == Some(Id::ThreadToggle(row));
                c.fill_round(pill, 9.5, if hot { t.fill2 } else { t.fill });
                c.text(&label, pill, Font::SmallBold, if closed { t.t2 } else { t.accent }, Align::Center);
                toggles.push((pill, row));
                pill_w += 8.0;
            }
            c.text(
                &who,
                rect(x, r.y + 8.0, (r.w - 26.0 - indent - dw - 22.0 - pill_w).max(10.0), 20.0),
                if unread { Font::BodyBold } else { Font::Body },
                t.t1,
                Align::Left,
            );
            c.text(&date, rect(r.right() - dw - 12.0, r.y + 8.0, dw, 20.0), Font::Small, if unread { t.accent } else { t.t3 }, Align::Right);
            // l'objet, et à sa droite l'étoile et le trombone
            let mut marks = 0.0;
            if msg.flags.flagged {
                c.icon_fill(&icons::STAR, rect(r.right() - 26.0, r.y + 31.0, 13.0, 13.0), t.orange);
                marks += 18.0;
            }
            if msg.attachments {
                c.icon(&icons::CLIP, rect(r.right() - 26.0 - marks, r.y + 31.0, 13.0, 13.0), t.t3);
                marks += 18.0;
            }
            let subject = if msg.subject.is_empty() { s.no_subject } else { msg.subject.as_str() };
            c.text(
                subject,
                rect(x, r.y + 28.0, (r.w - 26.0 - indent - 14.0 - marks).max(10.0), 20.0),
                Font::Body,
                if unread { t.t1 } else { t.t2 },
                Align::Left,
            );
            c.hline(x, r.bottom() - 1.0, r.w - 26.0 - indent, t.sep.fade(0.6));
            self.hits.add(r.intersect(&list), Id::Row(row));
        }
        // au-dessus des lignes : le clic sur le nombre ouvre la conversation sans changer de message
        for (pill, row) in toggles {
            self.hits.add(pill.intersect(&list), Id::ThreadToggle(row));
        }
        c.unclip();
        // la barre de défilement : une simple marque de l'endroit où l'on est
        if self.lay.list_max > 0.0 {
            let thumb_h = (list.h * list.h / total).max(28.0);
            let y = list.y + (list.h - thumb_h) * (self.list_scroll / self.lay.list_max);
            c.fill_round(rect(list.right() - 6.0, y, 4.0, thumb_h), 2.0, t.t4);
        }
    }

    fn paint_reading(&mut self, c: &mut Canvas, t: &Theme) {
        let area = self.lay.read;
        let s = self.s;
        c.fill(area, t.bg);
        if area.w < 80.0 {
            return;
        }
        self.hits.add(area, Id::Reading);
        let links = self.links().len();
        let hover = self.hover;
        // un brouillon se reprend pour être fini et envoyé
        let in_drafts = self
            .reading
            .as_ref()
            .is_some_and(|r| self.boxes.get(r.account).is_some_and(|mb| mb.folders.iter().any(|f| f.raw == r.folder && f.role == Role::Drafts)));
        let Some(r) = self.reading.as_mut() else {
            if !self.boxes.is_empty() {
                c.text(s.pick_message, rect(area.x, area.y + area.h * 0.34, area.w, 22.0), Font::Body, t.t3, Align::Center);
            }
            return;
        };
        let Some(msg) = r.message.as_ref() else {
            c.text(s.loading, rect(area.x, area.y + area.h * 0.34, area.w, 22.0), Font::Body, t.t3, Align::Center);
            return;
        };
        let pad = 28.0;
        let width = (area.w - 2.0 * pad).clamp(60.0, 860.0);
        let x = area.x + pad;
        // la hauteur de chaque paragraphe ne se recalcule que si la largeur change
        if r.laid_for != width {
            r.heights = r.paragraphs.iter().map(|p| c.text_height(p, width, Font::Body)).collect();
            r.laid_for = width;
        }
        let sum = &msg.summary;
        let subject = if sum.subject.is_empty() { s.no_subject } else { sum.subject.as_str() };
        let subject_h = c.text_height(subject, width, Font::Title);
        let note_h = if msg.from_html { c.text_height(s.from_html, width, Font::Small) + 12.0 } else { 0.0 };
        let files_h = if msg.attachments.is_empty() { 0.0 } else { 40.0 };
        let head_h = 22.0 + subject_h + 12.0 + 22.0 + 20.0 + 14.0 + files_h + 44.0 + 12.0;
        let body_h: f32 = r.heights.iter().sum();
        r.max_scroll = (head_h + note_h + body_h + 30.0 - area.h).max(0.0);
        r.scroll = r.scroll.clamp(0.0, r.max_scroll);
        let scroll = c.snap(r.scroll);
        c.clip(area);
        let mut y = area.y + 22.0 - scroll;
        c.text_block(subject, rect(x, y, width, subject_h), Font::Title, t.t1);
        y += subject_h + 12.0;
        let who = if sum.from_name.is_empty() { sum.from.clone() } else { format!("{}  <{}>", sum.from_name, sum.from) };
        let when = {
            let (y4, long) = (sum.date, true);
            // la date entière, à droite de l'expéditeur
            local(y4).filter(|_| y4 > 0 && long).map(|(yy, mo, d, h, mi)| format!("{yy}-{mo:02}-{d:02} {h:02}:{mi:02}")).unwrap_or_default()
        };
        let when_w = c.text_width(&when, Font::Small) + 6.0;
        c.text(&who, rect(x, y, (width - when_w - 8.0).max(10.0), 22.0), Font::BodyBold, t.t1, Align::Left);
        c.text(&when, rect(x + width - when_w, y, when_w, 22.0), Font::Small, t.t3, Align::Right);
        y += 22.0;
        let to = if msg.cc.is_empty() { format!("{} {}", s.to, sum.to) } else { format!("{} {}   {} {}", s.to, sum.to, s.cc, msg.cc) };
        c.text(&to, rect(x, y, width, 20.0), Font::Small, t.t3, Align::Left);
        y += 20.0 + 14.0;
        let mut hits: Vec<(Rect, Id)> = vec![];
        // les pièces jointes : un clic les enregistre
        if !msg.attachments.is_empty() {
            let mut cx = x;
            for (i, a) in msg.attachments.iter().enumerate() {
                let label = format!("{}  ·  {}", a.name, size_text(a.size));
                let w = (c.text_width(&label, Font::Small) + 40.0).min(width);
                if cx + w > x + width && cx > x {
                    break;
                }
                let chip = rect(cx, y, w, 28.0);
                c.fill_round(chip, 14.0, if hover == Some(Id::Attachment(i)) { t.fill2 } else { t.fill });
                c.icon(&icons::CLIP, rect(chip.x + 10.0, chip.y + 7.0, 14.0, 14.0), t.t3);
                c.text(&label, rect(chip.x + 30.0, chip.y, w - 38.0, chip.h), Font::Small, t.t1, Align::Left);
                hits.push((chip, Id::Attachment(i)));
                cx += w + 8.0;
            }
            y += files_h;
        }
        // copier le texte, ouvrir un lien
        let mut bx = x;
        let link_label = format!("{} ({links})", s.links);
        let mut buttons = vec![(Id::CopyText, s.copy_text, true), (Id::Links, link_label.as_str(), links > 0)];
        if in_drafts {
            buttons.insert(0, (Id::EditDraft, s.edit_draft, true));
        }
        for (id, label, enabled) in buttons {
            let w = c.text_width(label, Font::BodyBold) + 32.0;
            let b = rect(bx, y, w, 32.0);
            c.fill_round(b, 16.0, if enabled && hover == Some(id) { t.fill2 } else { t.fill });
            c.text(label, b, Font::BodyBold, if enabled { t.t1 } else { t.t4 }, Align::Center);
            if enabled {
                hits.push((b, id));
            }
            bx += w + 8.0;
        }
        y += 44.0;
        c.hline(x, y, width, t.sep);
        y += 12.0;
        if msg.from_html {
            c.text_block(s.from_html, rect(x, y, width, note_h), Font::Small, t.t3);
            y += note_h;
        }
        // le texte : seuls les paragraphes visibles sont dessinés ; ce qui est cité (« > ») est plus discret
        for (p, height) in r.paragraphs.iter().zip(&r.heights) {
            if y + height >= area.y && y <= area.bottom() {
                c.text_block(p, rect(x, y, width, *height), Font::Body, if p.starts_with('>') { t.t3 } else { t.t1 });
            }
            y += height;
            if y > area.bottom() {
                break;
            }
        }
        c.unclip();
        if r.max_scroll > 0.0 {
            let total = area.h + r.max_scroll;
            let thumb_h = (area.h * area.h / total).max(28.0);
            let ty = area.y + (area.h - thumb_h) * (r.scroll / r.max_scroll);
            c.fill_round(rect(area.right() - 6.0, ty, 4.0, thumb_h), 2.0, t.t4);
        }
        for (r, id) in hits {
            self.hits.add(r.intersect(&area), id);
        }
    }

    fn paint_status(&mut self, c: &mut Canvas, t: &Theme) {
        let st = self.lay.status;
        let s = self.s;
        c.fill(st, t.bg2);
        c.hline(st.x, st.y, st.w, t.sep);
        let unread = self.messages.iter().filter(|m| !m.flags.seen).count();
        let mut left = if self.cur.is_some() { s.count(self.shown.len(), s.message) } else { String::new() };
        if unread > 0 {
            left.push_str(&format!("  ·  {unread} {}", s.unread));
        }
        c.text(&left, rect(st.x + 14.0, st.y, st.w * 0.5, st.h), Font::Small, t.t3, Align::Left);
        // à droite : ce que fait le compte affiché, ou ce qui n'a pas marché
        let b = self.cur.as_ref().map(|c| c.0).or((!self.boxes.is_empty()).then_some(0));
        if let Some(mb) = b.and_then(|b| self.boxes.get(b)) {
            let (text, col) = match (mb.work, &mb.error) {
                (Work::Connecting, _) => (s.connecting.to_string(), t.t3),
                (Work::Syncing, _) => (s.syncing.to_string(), t.t3),
                (Work::Reading, _) => (s.reading.to_string(), t.t3),
                (Work::Sending, _) => (s.sending.to_string(), t.t3),
                (Work::Searching, _) => (s.searching.to_string(), t.t3),
                (Work::Idle, Some(error)) => (error.clone(), t.red),
                (Work::Idle, None) => (String::new(), t.t3),
            };
            c.text(&text, rect(st.x + st.w * 0.4, st.y, st.w * 0.6 - 14.0, st.h), Font::Small, col, Align::Right);
        }
    }

    /// Sous le bouton de la barre d'outils que la souris survole : ce qu'il fait, et son raccourci.
    fn paint_hint(&mut self, c: &mut Canvas, t: &Theme) {
        let Some(id) = self.hover.filter(|_| self.sheet.is_none() && self.menu.is_none()) else { return };
        let (Some(text), Some(button)) = (self.hint_of(id), self.hits.rect_of(|h| *h == id)) else { return };
        let w = c.text_width(&text, Font::Small) + 20.0;
        let x = (button.x + button.w / 2.0 - w / 2.0).clamp(8.0, (self.lay.w - w - 8.0).max(8.0));
        let r = rect(x.round(), button.bottom() + 8.0, w, 26.0);
        c.fill_round(r, 8.0, t.solid2);
        c.stroke_round(r, 8.0, t.ring, 1.0);
        c.text(&text, r, Font::Small, t.t1, Align::Center);
    }

    fn paint_menu(&mut self, c: &mut Canvas, t: &Theme) {
        let Some(menu) = self.menu.as_ref() else { return };
        // un clic ailleurs le ferme
        self.hits.add(rect(0.0, 0.0, self.lay.w, self.lay.h), Id::MenuDim);
        let widest = menu.items.iter().map(|(label, _)| c.text_width(label, Font::Body)).fold(0.0f32, f32::max);
        let w = (widest + 32.0).clamp(200.0, (self.lay.w - 24.0).clamp(200.0, 560.0));
        let h = menu.items.len() as f32 * 30.0 + 12.0;
        let card = rect((menu.at.x - w).max(8.0), menu.at.y.min((self.lay.h - h - 8.0).max(TOP)), w, h);
        c.fill_round(card, 12.0, t.solid);
        c.stroke_round(card, 12.0, t.ring, 1.0);
        for (i, (label, _)) in menu.items.iter().enumerate() {
            let r = rect(card.x + 6.0, card.y + 6.0 + i as f32 * 30.0, w - 12.0, 30.0);
            if self.hover == Some(Id::Menu(i)) {
                c.fill_round(r, 7.0, t.fill);
            }
            c.text(label, r.inset_xy(10.0, 0.0), Font::Body, t.t1, Align::Left);
            self.hits.add(r, Id::Menu(i));
        }
    }

    fn paint_toast(&mut self, c: &mut Canvas, t: &Theme) {
        let Some((text, _)) = self.toast.as_ref() else { return };
        let w = (c.text_width(text, Font::Body) + 36.0).min(self.lay.w - 40.0);
        let r = rect(((self.lay.w - w) / 2.0).round(), self.lay.status.y - 52.0, w, 36.0);
        c.fill_round(r, 18.0, t.solid2);
        c.stroke_round(r, 18.0, t.ring, 1.0);
        c.text(text, r.inset_xy(16.0, 0.0), Font::Body, t.t1, Align::Center);
    }
}
