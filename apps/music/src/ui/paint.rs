//! The frame of the window: sidebar, top bar, player bar, side panel, menus, dialogs and messages.

use super::{
    App, Hit, ImportStage, Modal, PANEL_W, PLAYER_H, Panel, SIDEBAR_SMALL, SIDEBAR_W, View,
    draw::{self, ic},
};
use crate::{
    art,
    i18n::{fill, tr},
    library::clock,
    yt,
};
use plume_ui::{Align, Canvas, Color, EditStyle, Font, PaintCx, Rect, icons, icons::Icon, m, rect};

impl App {
    pub fn hovered(&self, h: &Hit) -> bool {
        self.hover.as_ref() == Some(h)
    }

    pub fn sidebar_w(&self) -> f32 {
        if self.set.sidebar_small { SIDEBAR_SMALL } else { SIDEBAR_W }
    }

    /// A cover (or its placeholder while it arrives).
    pub fn cover(&mut self, c: &mut Canvas, src: Option<&str>, r: Rect, radius: f32, px: u32) {
        let p = self.pal.clone();
        if let Some(s) = src {
            if let Some(path) = art::cached(s) {
                let key = art::key(s) ^ (px as u64) << 56;
                self.drawn.push(key);
                let wic = self.wic.clone();
                draw::image_fill(c, key, r, radius, 1.0, || art::decode_file(&wic, &path, px));
                return;
            }
            self.request_cover(s);
        }
        c.fill_round(r, radius, if p.dark { Color::hex(0x2a2a30) } else { Color::hex(0xdedee4) });
        let s = (r.w * 0.4).min(48.0);
        c.icon(&ic::NOTE, rect(r.x + (r.w - s) / 2.0, r.y + (r.h - s) / 2.0, s, s), p.t3);
    }

    pub fn icon_btn(&mut self, c: &mut Canvas, icon: &Icon, r: Rect, hit: Hit, color: Color) {
        let hot = self.hovered(&hit);
        if hot {
            c.fill_round(r, r.h / 2.0, self.pal.t1.fade(0.08));
        }
        let s = (r.w * 0.5).round();
        let ink = if hot { self.pal.t1 } else { color };
        c.icon(icon, rect(r.x + (r.w - s) / 2.0, r.y + (r.h - s) / 2.0, s, s), ink);
        self.hits.add(r, hit);
    }

    pub fn icon_fill_btn(&mut self, c: &mut Canvas, icon: &Icon, r: Rect, hit: Hit, color: Color) {
        let s = (r.w * 0.5).round();
        c.icon_fill(icon, rect(r.x + (r.w - s) / 2.0, r.y + (r.h - s) / 2.0, s, s), color);
        self.hits.add(r, hit);
    }

    /// A round button: primary (accent) or quiet.
    pub fn pill(&mut self, c: &mut Canvas, r: Rect, label: &str, primary: bool, hit: Hit) {
        let p = self.pal.clone();
        let hot = self.hovered(&hit);
        let fill = if primary {
            if hot { p.accent.mix(Color::hex(0xffffff), 0.12) } else { p.accent }
        } else if hot {
            p.t1.fade(0.16)
        } else {
            p.t1.fade(0.09)
        };
        c.fill_round(r, r.h / 2.0, fill);
        c.text(label, r, Font::BodyBold, if primary { p.on_accent } else { p.t1 }, Align::Center);
        self.hits.add(r, hit);
    }

    /// A horizontal slider (0..1); returns nothing, the drag does the rest.
    pub fn slider(&mut self, c: &mut Canvas, r: Rect, v: f32, hit: Hit, on: Color) {
        let p = self.pal.clone();
        let hot = self.hovered(&hit) || self.drag.is_some() && self.pressed.as_ref() == Some(&hit);
        let y = r.y + r.h / 2.0;
        let track = rect(r.x, y - 2.0, r.w, 4.0);
        c.fill_round(track, 2.0, p.t1.fade(0.18));
        let w = r.w * v.clamp(0.0, 1.0);
        c.fill_round(rect(r.x, y - 2.0, w, 4.0), 2.0, if hot { on } else { p.t1.fade(0.85) });
        if hot {
            c.fill_round(rect(r.x + w - 6.0, y - 6.0, 12.0, 12.0), 6.0, Color::hex(0xffffff));
        }
        self.hits.add(r, hit);
    }

    pub fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let p = self.pal.clone();
        self.hits.clear();
        c.clear(p.bg);
        let (w, h) = (cx.w, cx.h);
        // smooth scrolling
        let k = if self.set.reduce_motion { 1.0 } else { 0.28 };
        self.scroll += (self.scroll_to - self.scroll) * k;
        if (self.scroll - self.scroll_to).abs() < 0.5 {
            self.scroll = self.scroll_to;
        }
        if self.now_open {
            self.paint_now(c, w, h);
            self.paint_caption(c, w, cx, true);
            self.paint_menu(c, w, h);
            self.paint_toast(c, w, h);
            self.place_edit(None);
            return;
        }
        if self.set.gif_everywhere {
            self.paint_gif(c, rect(0.0, 0.0, w, h), 0.18);
        }
        let sw = self.sidebar_w();
        let pw = if self.panel.is_some() && w > 980.0 { PANEL_W } else { 0.0 };
        let side = rect(0.0, 0.0, sw, h - PLAYER_H);
        let page = rect(sw, 0.0, w - sw - pw, h - PLAYER_H);
        self.paint_page(c, page);
        self.paint_top(c, page);
        if pw > 0.0 {
            let panel = rect(w - pw, 0.0, pw, h - PLAYER_H);
            self.paint_panel(c, panel);
        }
        self.paint_sidebar(c, side);
        self.paint_bar(c, rect(0.0, h - PLAYER_H, w, PLAYER_H));
        self.paint_caption(c, w, cx, false);
        self.paint_menu(c, w, h);
        let field = self.paint_modal(c, w, h);
        self.paint_toast(c, w, h);
        self.place_edit(field);
    }

    /// The single text field of the window: in the search box, or in the open dialog.
    fn place_edit(&mut self, modal_field: Option<Rect>) {
        let p = self.pal.clone();
        let want = match self.edit_for {
            Some("modal") => modal_field.map(|r| (r, self.modal_text.clone(), p.card)),
            Some("search") if !self.now_open && self.modal.is_none() => self.search_rect.map(|r| (r, self.query.clone(), p.t1.fade(0.08).over(p.bg))),
            _ => None,
        };
        match want {
            Some((r, text, bg)) => {
                let inner = rect(r.x + 40.0, r.y + 6.0, r.w - 52.0, r.h - 12.0);
                if self.edit_shown != self.edit_for {
                    let style = EditStyle {
                        fg: p.t1,
                        bg,
                        size: 14.0,
                        select: Some((0, text.encode_utf16().count())),
                        placeholder: String::new(),
                        password: false,
                        multiline: false,
                    };
                    self.win.edit_show(inner, &text, &style);
                    self.win.edit_focus();
                    self.edit_shown = self.edit_for;
                } else {
                    self.win.edit_place(inner);
                }
            }
            None => {
                if self.edit_shown.is_some() {
                    self.win.edit_hide();
                    self.edit_shown = None;
                }
            }
        }
    }

    fn paint_caption(&mut self, c: &mut Canvas, w: f32, cx: &PaintCx, over: bool) {
        let p = self.pal.clone();
        let cap_x = w - 3.0 * m::CAPTION_W;
        let glyphs = [(Hit::Min, '\u{E921}'), (Hit::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Hit::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, 34.0);
            let hot = self.hovered(&id) || self.pressed.as_ref() == Some(&id);
            if hot {
                c.fill(r, if id == Hit::Close { Color::hex(0xc42b1c) } else { p.t1.fade(0.1) });
            }
            let ink = if id == Hit::Close && hot {
                Color::hex(0xffffff)
            } else if over {
                Color::hex(0xffffff).fade(0.85)
            } else {
                p.t2
            };
            c.glyph(glyph, r, ink);
            self.hits.add(r, id);
        }
    }

    fn paint_sidebar(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        c.fill(r, p.side);
        let small = self.set.sidebar_small;
        // the name
        let logo = rect(r.x + 18.0, 18.0, 30.0, 30.0);
        let wic = self.wic.clone();
        c.image(7, logo, 1.0, || art::decode_bytes(&wic, crate::LOGO, 64));
        if !small {
            draw::text(c, "Plume Music", rect(r.x + 58.0, 16.0, r.w - 100.0, 34.0), 18.0, 700, p.t1, Align::Left);
        }
        let tb = if small { rect(r.x + 22.0, 58.0, 32.0, 28.0) } else { rect(r.right() - 44.0, 19.0, 28.0, 28.0) };
        self.icon_btn(c, &icons::SIDEBAR, tb, Hit::SidebarToggle, p.t3);
        let mut y = if small { 96.0 } else { 70.0 };
        let navs: [(&Icon, &str, View); 7] = [
            (&icons::HOME, tr("Home"), View::Home),
            (&icons::SEARCH, tr("Search"), View::Search),
            (&ic::NOTE, tr("Songs"), View::Songs),
            (&ic::ALBUM, tr("Albums"), View::Albums),
            (&ic::MIC, tr("Artists"), View::Artists),
            (&ic::HEART, tr("Liked songs"), View::Liked),
            (&ic::DOWNLOAD, tr("Downloads"), View::Downloads),
        ];
        for (i, (icon, label, v)) in navs.into_iter().enumerate() {
            if i == 2 && !small {
                c.text(tr("Library"), rect(r.x + 22.0, y + 6.0, r.w - 40.0, 22.0), Font::SmallBold, p.t3, Align::Left);
                y += 30.0;
            }
            let row = rect(r.x + 10.0, y, r.w - 20.0, 38.0);
            let active = self.view == v;
            let hit = Hit::Nav(v);
            if active {
                c.fill_round(row, 9.0, p.t1.fade(0.1));
            } else if self.hovered(&hit) {
                c.fill_round(row, 9.0, p.t1.fade(0.05));
            }
            let ink = if active { p.t1 } else { p.t2 };
            let ix = if small { row.x + (row.w - 20.0) / 2.0 } else { row.x + 12.0 };
            if active && icon.name == ic::HEART.name {
                c.icon_fill(icon, rect(ix, row.y + 9.0, 20.0, 20.0), p.accent);
            } else {
                c.icon(icon, rect(ix, row.y + 9.0, 20.0, 20.0), if active { p.accent } else { ink });
            }
            if !small {
                c.text(label, rect(row.x + 44.0, row.y, row.w - 50.0, row.h), if active { Font::BodyBold } else { Font::Body }, ink, Align::Left);
            }
            self.hits.add(row, hit);
            y += 40.0;
        }
        // playlists
        y += 10.0;
        c.hline(r.x + 18.0, y, r.w - 36.0, p.line);
        y += 12.0;
        if !small {
            c.text(tr("Playlists"), rect(r.x + 22.0, y, r.w - 110.0, 26.0), Font::SmallBold, p.t3, Align::Left);
            self.icon_btn(c, &ic::IMPORT, rect(r.right() - 74.0, y - 1.0, 28.0, 28.0), Hit::ImportOpen, p.t2);
            self.icon_btn(c, &icons::PLUS, rect(r.right() - 42.0, y - 1.0, 28.0, 28.0), Hit::NewPlaylist, p.t2);
            y += 32.0;
        } else {
            self.icon_btn(c, &icons::PLUS, rect(r.x + 22.0, y, 32.0, 30.0), Hit::NewPlaylist, p.t2);
            y += 36.0;
        }
        let bottom = r.bottom() - 52.0;
        c.clip(rect(r.x, y, r.w, (bottom - y).max(0.0)));
        let lists: Vec<(String, String, usize, Option<String>)> = self
            .lib
            .playlists
            .iter()
            .map(|pl| {
                let cover = pl.cover.clone().or_else(|| pl.tracks.first().and_then(|k| self.lib.tracks.get(k)).and_then(art::source));
                (pl.id.clone(), pl.name.clone(), pl.tracks.len(), cover)
            })
            .collect();
        for (id, name, n, cover) in lists {
            if y > bottom {
                break;
            }
            let row = rect(r.x + 10.0, y, r.w - 20.0, 52.0);
            let hit = Hit::NavPlaylist(id.clone());
            let active = self.view == View::Playlist(id.clone());
            if active {
                c.fill_round(row, 9.0, p.t1.fade(0.1));
            } else if self.hovered(&hit) {
                c.fill_round(row, 9.0, p.t1.fade(0.05));
            }
            let cx_ = if small { row.x + (row.w - 40.0) / 2.0 } else { row.x + 6.0 };
            self.cover(c, cover.as_deref(), rect(cx_, row.y + 6.0, 40.0, 40.0), 6.0, 96);
            if !small {
                c.text(&name, rect(row.x + 56.0, row.y + 7.0, row.w - 62.0, 20.0), Font::BodyBold, if active { p.accent } else { p.t1 }, Align::Left);
                let sub = if n == 1 { tr("1 song").to_string() } else { fill(tr("{n} songs"), &[&n.to_string()]) };
                c.text(&format!("{} · {}", tr("Playlist"), sub), rect(row.x + 56.0, row.y + 26.0, row.w - 62.0, 18.0), Font::Small, p.t3, Align::Left);
            }
            self.hits.add(row, hit);
            y += 54.0;
        }
        c.unclip();
        // settings, at the bottom
        let row = rect(r.x + 10.0, r.bottom() - 48.0, r.w - 20.0, 38.0);
        let hit = Hit::Nav(View::Settings);
        let active = self.view == View::Settings;
        if active || self.hovered(&hit) {
            c.fill_round(row, 9.0, p.t1.fade(if active { 0.1 } else { 0.05 }));
        }
        let ix = if small { row.x + (row.w - 20.0) / 2.0 } else { row.x + 12.0 };
        c.icon(&icons::GEAR, rect(ix, row.y + 9.0, 20.0, 20.0), if active { p.accent } else { p.t2 });
        if !small {
            c.text(tr("Settings"), rect(row.x + 44.0, row.y, row.w - 50.0, row.h), Font::Body, if active { p.t1 } else { p.t2 }, Align::Left);
        }
        self.hits.add(row, hit);
    }

    /// Back, forward and the search box, over the top of the page.
    fn paint_top(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        let y = 14.0;
        let can_back = !self.back.is_empty();
        let can_fwd = !self.fwd.is_empty();
        let b = rect(page.x + 20.0, y, 34.0, 34.0);
        c.fill_round(b, 17.0, p.bg.fade(0.7));
        self.icon_btn(c, &icons::BACK, b, Hit::Back, if can_back { p.t1 } else { p.t3 });
        let f = rect(page.x + 60.0, y, 34.0, 34.0);
        c.fill_round(f, 17.0, p.bg.fade(0.7));
        self.icon_btn(c, &icons::FORWARD, f, Hit::Forward, if can_fwd { p.t1 } else { p.t3 });
        let sw = (page.w - 300.0).clamp(220.0, 460.0);
        let s = rect(page.x + 108.0, y - 1.0, sw, 36.0);
        let focused = self.edit_for == Some("search");
        c.fill_round(s, 18.0, if focused { p.t1.fade(0.12).over(p.bg) } else { p.t1.fade(0.08).over(p.bg) });
        if focused {
            c.stroke_round(s, 18.0, p.t1.fade(0.5), 1.5);
        }
        c.icon(&icons::SEARCH, rect(s.x + 13.0, s.y + 9.0, 18.0, 18.0), p.t2);
        if !focused {
            let (text, ink) = if self.query.is_empty() { (tr("What do you want to listen to?").to_string(), p.t3) } else { (self.query.clone(), p.t1) };
            c.text(&text, rect(s.x + 40.0, s.y, s.w - 52.0, s.h), Font::Body, ink, Align::Left);
        }
        self.search_rect = Some(s);
        self.hits.add(s, Hit::SearchBox);
        // the import button, on the right of the search
        let ir = rect(s.right() + 10.0, y, 34.0, 34.0);
        if ir.right() < page.right() - 150.0 {
            c.fill_round(ir, 17.0, p.bg.fade(0.7));
            self.icon_btn(c, &ic::IMPORT, ir, Hit::ImportOpen, p.t2);
        }
    }

    fn paint_bar(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        c.fill(r, p.bar);
        c.hline(r.x, r.y, r.w, p.line);
        let t = self.current().cloned();
        // left: the song
        let left_w = (r.w * 0.3).clamp(200.0, 380.0);
        if let Some(t) = &t {
            let cr = rect(r.x + 16.0, r.y + 16.0, 60.0, 60.0);
            let src = art::source(t);
            self.cover(c, src.as_deref(), cr, 8.0, 128);
            self.hits.add(cr, Hit::NowOpen);
            let tx = cr.right() + 14.0;
            let tw = left_w - (tx - r.x) - 44.0;
            c.text(&t.title, rect(tx, r.y + 22.0, tw, 22.0), Font::BodyBold, p.t1, Align::Left);
            let sub = if self.loading.is_some() { tr("Downloading…").to_string() } else { t.artist.clone() };
            c.text(&sub, rect(tx, r.y + 44.0, tw, 20.0), Font::Small, p.t2, Align::Left);
            self.hits.add(rect(tx, r.y + 44.0, tw, 20.0), Hit::NowArtist);
            let heart = rect(r.x + left_w - 40.0, r.y + 30.0, 32.0, 32.0);
            if t.liked {
                self.icon_fill_btn(c, &ic::HEART, heart, Hit::LikeNow, p.accent);
            } else {
                self.icon_btn(c, &ic::HEART, heart, Hit::LikeNow, p.t2);
            }
        }
        // middle: the controls and the progress
        let mid = r.x + r.w / 2.0;
        let cy = r.y + 12.0;
        let on = |b: bool| if b { p.accent } else { p.t2 };
        self.icon_btn(c, &ic::SHUFFLE, rect(mid - 112.0, cy + 2.0, 32.0, 32.0), Hit::Shuffle, on(self.set.shuffle));
        if self.set.shuffle {
            c.fill_round(rect(mid - 97.5, cy + 34.0, 3.0, 3.0), 1.5, p.accent);
        }
        self.icon_fill_btn(c, &ic::PREV, rect(mid - 68.0, cy + 2.0, 32.0, 32.0), Hit::Prev, p.t1);
        let pr = rect(mid - 20.0, cy - 2.0, 40.0, 40.0);
        let hot = self.hovered(&Hit::PlayPause);
        c.fill_round(if hot { pr.inset(-1.5) } else { pr }, 20.0, p.t1);
        let busy = self.loading.is_some() || self.st.buffering;
        let icon = if self.st.playing && !busy { &ic::PAUSE } else { &ic::PLAY };
        if busy {
            // a turning arc while the sound arrives
            let a = self.t0.elapsed().as_secs_f32() * 5.0;
            for i in 0..8 {
                let ang = a + i as f32 * 0.785;
                let (x, y) = (pr.x + 20.0 + ang.cos() * 9.0, pr.y + 20.0 + ang.sin() * 9.0);
                c.fill_round(rect(x - 2.0, y - 2.0, 4.0, 4.0), 2.0, p.bg.fade(0.25 + i as f32 * 0.09));
            }
        } else {
            c.icon_fill(icon, rect(pr.x + 11.0, pr.y + 11.0, 18.0, 18.0), p.bg);
        }
        self.hits.add(pr, Hit::PlayPause);
        self.icon_fill_btn(c, &ic::NEXT, rect(mid + 36.0, cy + 2.0, 32.0, 32.0), Hit::Next, p.t1);
        self.icon_btn(c, &ic::REPEAT, rect(mid + 80.0, cy + 2.0, 32.0, 32.0), Hit::Repeat, on(self.set.repeat > 0));
        if self.set.repeat == 2 {
            c.text("1", rect(mid + 101.0, cy + 2.0, 12.0, 12.0), Font::SmallBold, p.accent, Align::Center);
        }
        if self.set.repeat > 0 {
            c.fill_round(rect(mid + 94.5, cy + 34.0, 3.0, 3.0), 1.5, p.accent);
        }
        let bw = (r.w * 0.38).clamp(260.0, 620.0);
        let by = r.y + 56.0;
        let (pos, dur) = self.shown_position();
        c.text(&clock(pos), rect(mid - bw / 2.0 - 52.0, by, 44.0, 20.0), Font::Small, p.t3, Align::Right);
        c.text(&clock(dur), rect(mid + bw / 2.0 + 8.0, by, 44.0, 20.0), Font::Small, p.t3, Align::Left);
        let v = if dur > 0.0 { (pos / dur) as f32 } else { 0.0 };
        let accent = p.accent;
        self.slider(c, rect(mid - bw / 2.0, by, bw, 20.0), v, Hit::Seek, accent);
        // right: lyrics, queue, volume, full screen
        let mut x = r.right() - 20.0;
        x -= 32.0;
        self.icon_btn(c, &ic::EXPAND, rect(x, r.y + 30.0, 32.0, 32.0), Hit::NowOpen, p.t2);
        x -= 110.0;
        let vol = self.set.volume;
        self.slider(c, rect(x, r.y + 36.0, 100.0, 20.0), vol, Hit::Volume, accent);
        x -= 36.0;
        let vicon = if vol <= 0.001 { &ic::MUTE } else { &ic::VOLUME };
        self.icon_btn(c, vicon, rect(x, r.y + 30.0, 32.0, 32.0), Hit::Mute, p.t2);
        x -= 38.0;
        let qon = self.panel == Some(Panel::Queue);
        self.icon_btn(c, &ic::QUEUE, rect(x, r.y + 30.0, 32.0, 32.0), Hit::QueueBtn, on(qon));
        x -= 38.0;
        let lon = self.panel == Some(Panel::Lyrics);
        self.icon_btn(c, &ic::LYRICS, rect(x, r.y + 30.0, 32.0, 32.0), Hit::LyricsBtn, on(lon));
        x -= 38.0;
        let timer_on = self.sleep_at.is_some() || self.sleep_end_of_song;
        if x > mid + bw / 2.0 + 60.0 {
            self.icon_btn(c, &ic::TIMER, rect(x, r.y + 30.0, 32.0, 32.0), Hit::Timer, on(timer_on));
        }
    }

    /// Position shown: the dragged one while seeking.
    pub fn shown_position(&self) -> (f64, f64) {
        let dur = if self.st.dur > 0.0 { self.st.dur } else { self.current().map(|t| t.dur).unwrap_or(0.0) };
        if let Some(super::Drag::Seek(v)) = &self.drag {
            return (*v as f64 * dur, dur);
        }
        if self.loading.is_some() || self.cur.is_none() {
            return (0.0, dur);
        }
        (self.st.pos.min(dur.max(self.st.pos)), dur)
    }

    fn paint_panel(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        c.fill(r, p.side);
        c.vline(r.x, r.y, r.h, p.line);
        let tabs = [(Panel::Lyrics, tr("Lyrics")), (Panel::Queue, tr("Queue"))];
        let mut x = r.x + 20.0;
        for (tab, label) in tabs {
            let w = c.text_width(label, Font::BodyBold) + 28.0;
            let tr_ = rect(x, 52.0, w, 32.0);
            let active = self.panel == Some(tab);
            c.fill_round(tr_, 16.0, if active { p.t1 } else { p.t1.fade(0.08) });
            c.text(label, tr_, Font::BodyBold, if active { p.bg } else { p.t1 }, Align::Center);
            self.hits.add(tr_, Hit::NowTab(tab));
            x += w + 8.0;
        }
        let body = rect(r.x, 96.0, r.w, r.h - 96.0);
        match self.panel {
            Some(Panel::Lyrics) => self.paint_lyrics(c, body, false),
            Some(Panel::Queue) => self.paint_queue(c, body, false),
            None => {}
        }
    }

    fn paint_menu(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some(menu) = &self.menu else { return };
        let p = self.pal.clone();
        let items: Vec<(String, bool)> = menu.items.iter().map(|(l, a)| (l.clone(), matches!(a, super::MenuAct::Header))).collect();
        let mw = items.iter().map(|(l, _)| c.text_width(l, Font::Body)).fold(160.0f32, f32::max) + 44.0;
        let mh = items.len() as f32 * 36.0 + 12.0;
        let x = menu.x.min(w - mw - 8.0).max(8.0);
        let y = if menu.y + mh > h - 8.0 { (menu.y - mh).max(8.0) } else { menu.y };
        let box_ = rect(x, y, mw, mh);
        c.fill_round(box_.offset(0.0, 6.0).inset(-2.0), 14.0, Color::hex(0).fade(0.25));
        c.fill_round(box_, 12.0, if p.dark { Color::hex(0x2a2a30) } else { Color::hex(0xffffff) });
        c.stroke_round(box_, 12.0, p.line, 1.0);
        for (i, (label, header)) in items.iter().enumerate() {
            let row = rect(x + 6.0, y + 6.0 + i as f32 * 36.0, mw - 12.0, 34.0);
            if *header {
                c.text(label, rect(row.x + 12.0, row.y, row.w - 20.0, row.h), Font::SmallBold, p.t3, Align::Left);
                continue;
            }
            let hit = Hit::MenuItem(i);
            if self.hovered(&hit) {
                c.fill_round(row, 8.0, p.t1.fade(0.08));
            }
            c.text(label, rect(row.x + 12.0, row.y, row.w - 20.0, row.h), Font::Body, p.t1, Align::Left);
            self.hits.add(row, hit);
        }
    }

    /// Dialogs. Returns where the text field goes.
    fn paint_modal(&mut self, c: &mut Canvas, w: f32, h: f32) -> Option<Rect> {
        let modal = self.modal.clone()?;
        let p = self.pal.clone();
        c.fill(rect(0.0, 0.0, w, h), Color::hex(0).fade(0.55));
        let mw = (w - 80.0).min(520.0);
        let (title, body, ok, cancel, field): (String, String, Option<String>, String, bool) = match &modal {
            Modal::Import { stage } => {
                let (b, ok, f) = match stage {
                    // the services and how to bring each one (drawn as a list below)
                    ImportStage::Link => (String::new(), Some(tr("Import").to_string()), true),
                    ImportStage::Reading => (tr("Reading the link…").to_string(), None, false),
                    ImportStage::Matching { done, total, name } => (
                        format!("{name}\n{}", fill(tr("Finding the songs on YouTube Music: {a} of {b}"), &[&done.to_string(), &total.to_string()])),
                        None,
                        false,
                    ),
                    ImportStage::Done { found, missing, capped, .. } => (
                        {
                            let mut t = if missing.is_empty() {
                                fill(tr("Imported: {a} songs"), &[&found.to_string()])
                            } else {
                                fill(tr("Imported: {a} songs, {b} not found"), &[&found.to_string(), &missing.len().to_string()])
                            };
                            for m in missing.iter().take(6) {
                                t.push_str(&format!("\n  · {m}"));
                            }
                            if missing.len() > 6 {
                                t.push_str(&format!("\n  · … +{}", missing.len() - 6));
                            }
                            if *capped {
                                t.push_str("\n\n");
                                t.push_str(tr("Spotify only shows the first 100 songs of a shared link."));
                                t.push(' ');
                                t.push_str(tr("For all of them: in Spotify, open the playlist, press Ctrl+A then Ctrl+C, then click Paste copied songs."));
                            }
                            t
                        },
                        Some(tr("Playlist").to_string()),
                        false,
                    ),
                    ImportStage::Failed(e) => (e.clone(), Some(tr("Import").to_string()), true),
                };
                (tr("Import a playlist").to_string(), b, ok, tr(if matches!(stage, ImportStage::Done { .. }) { "Close" } else { "Cancel" }).to_string(), f)
            }
            Modal::NewPlaylist { .. } => (tr("New playlist").to_string(), String::new(), Some(tr("Create").to_string()), tr("Cancel").to_string(), true),
            Modal::Rename(_) => (tr("Rename").to_string(), String::new(), Some(tr("Save").to_string()), tr("Cancel").to_string(), true),
            Modal::Tools => (
                "yt-dlp".to_string(),
                format!(
                    "{}\n\n{}",
                    tr("Plume Music needs yt-dlp to play songs from YouTube and SoundCloud (a free tool, 20 MB, shared with Plume Web)."),
                    tr("Only download music you have the right to keep.")
                ),
                Some(if self.tools_busy { tr("Installing…") } else { tr("Install yt-dlp") }.to_string()),
                tr("Cancel").to_string(),
                false,
            ),
        };
        let body_h = if body.is_empty() { 0.0 } else { c.text_height(&body, mw - 56.0, Font::Body) + 12.0 };
        let howto = matches!(&modal, Modal::Import { stage: ImportStage::Link });
        let howto_h = if howto { 252.0 } else { 0.0 };
        let mh = 76.0 + body_h + howto_h + if field { 56.0 } else { 0.0 } + 70.0;
        let b = rect((w - mw) / 2.0, (h - mh) / 2.0, mw, mh);
        c.fill_round(b, 18.0, p.card);
        c.stroke_round(b, 18.0, p.line, 1.0);
        draw::text(c, &title, rect(b.x + 28.0, b.y + 22.0, mw - 56.0, 32.0), 20.0, 700, p.t1, Align::Left);
        let mut y = b.y + 66.0;
        if !body.is_empty() {
            c.text_block(&body, rect(b.x + 28.0, y, mw - 56.0, body_h), Font::Body, p.t2);
            y += body_h;
        }
        if howto {
            y = self.paint_howto(c, b.x + 28.0, y, mw - 56.0);
        }
        // a progress bar while matching
        if let Modal::Import { stage: ImportStage::Matching { done, total, .. } } = &modal {
            let bar = rect(b.x + 28.0, y - 4.0, mw - 56.0, 6.0);
            c.fill_round(bar, 3.0, p.t1.fade(0.1));
            c.fill_round(rect(bar.x, bar.y, bar.w * (*done as f32 / (*total).max(1) as f32), 6.0), 3.0, p.accent);
        }
        let mut field_rect = None;
        if field {
            let f = rect(b.x + 28.0, y, mw - 56.0, 42.0);
            c.fill_round(f, 10.0, p.t1.fade(0.06).over(p.card));
            c.stroke_round(f, 10.0, p.accent, 1.5);
            c.icon(&ic::LINK, rect(f.x + 13.0, f.y + 12.0, 18.0, 18.0), p.t3);
            if self.edit_for != Some("modal") {
                c.text(&self.modal_text, rect(f.x + 40.0, f.y, f.w - 52.0, f.h), Font::Body, p.t1, Align::Left);
            }
            self.hits.add(f, Hit::ModalField);
            field_rect = Some(f);
        }
        let by = b.bottom() - 58.0;
        let mut x = b.right() - 28.0;
        if let Some(ok) = ok {
            let bw = c.text_width(&ok, Font::BodyBold) + 40.0;
            x -= bw;
            self.pill(c, rect(x, by, bw, 38.0), &ok, true, Hit::ModalOk);
            x -= 10.0;
        }
        let cw = c.text_width(&cancel, Font::BodyBold) + 36.0;
        x -= cw;
        self.pill(c, rect(x, by, cw, 38.0), &cancel, false, Hit::ModalCancel);
        // songs copied in Spotify (all of a playlist, beyond the 100 of a shared link)
        if let Modal::Import { stage: ImportStage::Link | ImportStage::Failed(_) | ImportStage::Done { capped: true, .. } } = &modal {
            let label = tr("Paste copied songs");
            let pw = c.text_width(label, Font::BodyBold) + 36.0;
            self.pill(c, rect(b.x + 28.0, by, pw, 38.0), label, false, Hit::Action("paste_songs"));
        }
        if matches!(modal, Modal::Tools) {
            let _ = yt::have_ytdlp();
        }
        field_rect
    }

    /// In the import dialog: each service, and how to bring its playlists (Spotify: a link stops at 100 songs, a
    /// copy of the songs has no limit).
    fn paint_howto(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32) -> f32 {
        let p = self.pal.clone();
        let badge = |c: &mut Canvas, x: f32, y: f32, text: &str, col: Color| -> f32 {
            let bw = c.text_width(text, Font::SmallBold) + 18.0;
            let r = rect(x, y, bw, 22.0);
            c.fill_round(r, 11.0, col.fade(0.18));
            c.text(text, r, Font::SmallBold, col, Align::Center);
            bw
        };
        let green = Color::hex(0x1ed760);
        type Badges = Vec<(String, Color)>;
        let rows: [(&str, Color, Badges); 5] = [
            ("Spotify", green, vec![(tr("Link: 100 songs max").to_string(), p.t2), (format!("{} ♾️", tr("Copy & paste: no limit")), green)]),
            ("Apple Music", Color::hex(0xfa2d48), vec![(tr("Link: every song").to_string(), p.t2)]),
            ("Deezer", Color::hex(0xa238ff), vec![(tr("Link: every song").to_string(), p.t2)]),
            ("SoundCloud", Color::hex(0xff5500), vec![(tr("Link: every song").to_string(), p.t2)]),
            ("YouTube Music", Color::hex(0xff0033), vec![(tr("Link: every song").to_string(), p.t2)]),
        ];
        let mut y = y;
        for (name, col, badges) in rows {
            let r = rect(x, y, w, 36.0);
            c.fill_round(rect(r.x, r.y + 13.0, 10.0, 10.0), 5.0, col);
            c.text(name, rect(r.x + 20.0, r.y, 140.0, r.h), Font::BodyBold, p.t1, Align::Left);
            let mut bx = r.x + 160.0;
            for (label, bc) in badges {
                bx += badge(c, bx, r.y + 7.0, &label, bc) + 8.0;
            }
            y += 38.0;
        }
        // the Spotify copy, step by step
        let steps = tr("Spotify, every song: open the playlist in Spotify, press Ctrl+A then Ctrl+C, then click Paste copied songs.");
        let sh = c.text_height(steps, w - 24.0, Font::Small) + 16.0;
        let box_ = rect(x, y + 4.0, w, sh);
        c.fill_round(box_, 10.0, green.fade(0.1));
        c.text_block(steps, rect(box_.x + 12.0, box_.y + 8.0, w - 24.0, sh - 16.0), Font::Small, p.t1);
        y + sh + 14.0
    }

    fn paint_toast(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some((text, at)) = &self.toast else { return };
        let p = self.pal.clone();
        let age = at.elapsed().as_secs_f32();
        let a = if age < 0.2 {
            age / 0.2
        } else if age > 3.6 {
            ((4.0 - age) / 0.4).max(0.0)
        } else {
            1.0
        };
        let tw = (c.text_width(text, Font::Body) + 44.0).min(w - 40.0);
        let r = rect((w - tw) / 2.0, h - PLAYER_H - 66.0 + (1.0 - a) * 10.0, tw, 42.0);
        let bg = if p.dark { Color::hex(0xffffff) } else { Color::hex(0x1c1c20) };
        c.fill_round(r, 21.0, bg.fade(a));
        c.text(text, r.inset_xy(18.0, 0.0), Font::BodyBold, if p.dark { Color::hex(0x111111) } else { Color::hex(0xffffff) }.fade(a), Align::Center);
        self.hits.add(r, Hit::Toast);
    }

    /// The GIF background (when chosen): its next frame when its delay is over.
    pub fn paint_gif(&mut self, c: &mut Canvas, r: Rect, opacity: f32) -> bool {
        if self.gif.is_none() && !self.gif_failed {
            let path = self.set.gif.clone();
            self.gif = path.as_ref().and_then(|p| art::Gif::open(&self.wic, p));
            self.gif_failed = self.gif.is_none();
            self.gif_due = std::time::Instant::now();
        }
        let Some(g) = self.gif.as_mut() else { return false };
        let key_base = 0x6900_0000_0000_0000u64;
        let now = std::time::Instant::now();
        let mut src = None;
        if now >= self.gif_due && !self.set.reduce_motion {
            src = g.advance(&self.wic);
            self.gif_due = now + std::time::Duration::from_millis(g.delay as u64);
            c.forget_image(key_base + self.gif_frame);
            self.gif_frame += 1;
        }
        let (w, h) = (g.w as f32, g.h as f32);
        let key = key_base + self.gif_frame;
        draw::image_cover(c, key, r, w, h, opacity, || src);
        true
    }
}
