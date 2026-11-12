//! The pages, and what clicks, drags and menus do.

use super::{
    App, Card, CardKind, Dl, Drag, Hit, ImportStage, Menu, MenuAct, Modal, Panel, ROW_H, TOP_H, View,
    draw::{self, ic},
};
use crate::{
    art,
    audio::Cmd,
    i18n::{fill, tr},
    library::{Track, clock},
};
use plume_ui::{Align, Canvas, Color, Cx, Font, Pt, Rect, icons, rect};

fn greeting() -> &'static str {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let h = unsafe { GetLocalTime() }.wHour;
    if (5..12).contains(&h) {
        tr("Good morning")
    } else if (12..18).contains(&h) {
        tr("Good afternoon")
    } else {
        tr("Good evening")
    }
}

impl App {
    /// The songs of the page, in order (what Play plays).
    pub fn page_keys(&mut self) -> Vec<String> {
        match &self.view {
            View::Search => {
                let mut v = self.local_results.clone();
                for k in &self.results {
                    if !v.contains(k) {
                        v.push(k.clone());
                    }
                }
                v
            }
            View::Home | View::Albums | View::Artists | View::Settings => {
                self.lib.recent.iter().filter(|k| self.lib.tracks.contains_key(*k)).take(50).cloned().collect()
            }
            _ => {
                self.refresh_rows();
                self.rows.clone()
            }
        }
    }

    pub fn page_title(&self) -> String {
        match &self.view {
            View::Home => tr("Home").into(),
            View::Search => self.query.clone(),
            View::Songs => tr("Songs").into(),
            View::Albums => tr("Albums").into(),
            View::Artists => tr("Artists").into(),
            View::Liked => tr("Liked songs").into(),
            View::Downloads => tr("Downloads").into(),
            View::Playlist(id) => self.lib.playlist(id).map(|p| p.name.clone()).unwrap_or_default(),
            View::Album(k) => k.split('\u{1}').nth(1).unwrap_or("").to_string(),
            View::Artist(a) => a.clone(),
            View::Settings => tr("Settings").into(),
        }
    }

    pub fn paint_page(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        c.clip(page);
        self.view_h = page.h;
        // a glow of the accent (or of the cover of the page) at the top
        let tint = self.page_tint().unwrap_or(p.accent);
        draw::fade_v(c, rect(page.x, page.y, page.w, 340.0), tint.fade(if p.dark { 0.32 } else { 0.18 }), tint.fade(0.0));
        let x = page.x + 28.0;
        let w = page.w - 56.0;
        let top = TOP_H + 16.0 - self.scroll;
        let h = match self.view.clone() {
            View::Home => self.paint_home(c, x, top, w, page),
            View::Search => self.paint_search(c, x, top, w, page),
            View::Albums => self.paint_albums(c, x, top, w, page),
            View::Artists => self.paint_artists(c, x, top, w, page),
            View::Settings => self.paint_settings(c, x, top, w, page),
            _ => self.paint_list(c, x, top, w, page),
        };
        self.content_h = h + TOP_H + 60.0;
        let max = (self.content_h - self.view_h).max(0.0);
        if self.scroll_to > max {
            self.scroll_to = max;
        }
        // scroll bar
        if self.content_h > self.view_h + 1.0 {
            let track_h = page.h - TOP_H - 8.0;
            let th = (track_h * self.view_h / self.content_h).max(40.0);
            let ty = page.y + TOP_H + (track_h - th) * (self.scroll / max.max(1.0));
            let sb = rect(page.right() - 10.0, ty, 6.0, th);
            let hot = self.hovered(&Hit::Scrollbar) || matches!(self.drag, Some(Drag::Scrollbar(..)));
            c.fill_round(sb, 3.0, p.t1.fade(if hot { 0.4 } else { 0.18 }));
            self.hits.add(rect(page.right() - 14.0, page.y + TOP_H, 14.0, track_h), Hit::Scrollbar);
        }
        c.unclip();
    }

    /// The color of the page's picture (a playlist's cover), for the glow at the top.
    fn page_tint(&mut self) -> Option<Color> {
        let src = match &self.view {
            View::Playlist(id) => {
                self.lib.playlist(id).and_then(|pl| pl.cover.clone().or_else(|| pl.tracks.first().and_then(|k| self.lib.tracks.get(k)).and_then(art::source)))
            }
            View::Album(_) | View::Artist(_) => {
                self.refresh_rows();
                self.rows.first().and_then(|k| self.lib.tracks.get(k)).and_then(art::source)
            }
            View::Liked => return Some(Color::hex(0x6d4aff)),
            _ => return None,
        }?;
        self.ensure_palette(&src);
        self.palette_of(&src).and_then(|p| p.first().copied()).map(art::vivid)
    }

    fn section(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str) -> f32 {
        let p = self.pal.clone();
        draw::text(c, title, rect(x, y, w, 34.0), 22.0, 700, p.t1, Align::Left);
        y + 44.0
    }

    /// Cards in a row (or a grid when `wrap`), each a cover, a title and a line. Returns the bottom.
    #[allow(clippy::too_many_arguments)]
    fn cards_grid(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, page: Rect, cards: Vec<Card>, wrap: bool, round: bool) -> f32 {
        let p = self.pal.clone();
        let cw = 176.0;
        let gap = 18.0;
        let per_row = ((w + gap) / (cw + gap)).floor().max(1.0) as usize;
        let cw = (w - gap * (per_row as f32 - 1.0)) / per_row as f32;
        let count = if wrap { cards.len() } else { cards.len().min(per_row) };
        let ch = cw + 64.0;
        let base = self.cards.len();
        let mut bottom = y;
        for (i, card) in cards.into_iter().take(count).enumerate() {
            let (col, row) = (i % per_row, i / per_row);
            let r = rect(x + col as f32 * (cw + gap), y + row as f32 * (ch + gap), cw, ch);
            bottom = r.bottom();
            if r.bottom() < page.y || r.y > page.bottom() {
                self.cards.push(card);
                continue;
            }
            let idx = base + i;
            let hit = Hit::Card(idx);
            let hot = self.hovered(&hit) || self.hovered(&Hit::CardPlay(idx));
            if hot {
                c.fill_round(r.inset(-8.0), 14.0, p.t1.fade(0.06));
            }
            let cover = rect(r.x, r.y, cw, cw);
            if matches!(card.kind, CardKind::Liked) {
                draw::rounded(c, cover, 10.0, |c| c.gradient_v(cover, Color::hex(0x6d4aff), Color::hex(0xc3b7ff)));
                c.icon_fill(&ic::HEART, rect(cover.x + cw * 0.35, cover.y + cw * 0.35, cw * 0.3, cw * 0.3), Color::hex(0xffffff));
            } else {
                self.cover(c, card.cover.as_deref(), cover, if round { cw / 2.0 } else { 10.0 }, 320);
            }
            if hot && !matches!(card.kind, CardKind::Artist(_)) {
                let pb = rect(cover.right() - 54.0, cover.bottom() - 54.0, 44.0, 44.0);
                c.fill_round(pb, 22.0, p.accent);
                c.icon_fill(&ic::PLAY, rect(pb.x + 13.0, pb.y + 12.0, 20.0, 20.0), p.on_accent);
                self.hits.add(pb, Hit::CardPlay(idx));
            }
            let align = if round { Align::Center } else { Align::Left };
            c.text(&card.title, rect(r.x, cover.bottom() + 10.0, cw, 22.0), Font::BodyBold, p.t1, align);
            c.text(&card.sub, rect(r.x, cover.bottom() + 32.0, cw, 20.0), Font::Small, p.t3, align);
            self.hits.add(r, hit);
            self.cards.push(card);
        }
        bottom + 18.0
    }

    fn playlist_card(&self, id: &str) -> Option<Card> {
        let pl = self.lib.playlist(id)?;
        let cover = pl.cover.clone().or_else(|| pl.tracks.first().and_then(|k| self.lib.tracks.get(k)).and_then(art::source));
        let n = pl.tracks.len();
        Some(Card {
            kind: CardKind::Playlist(pl.id.clone()),
            title: pl.name.clone(),
            sub: if n == 1 { tr("1 song").into() } else { fill(tr("{n} songs"), &[&n.to_string()]) },
            cover,
        })
    }

    fn paint_home(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, page: Rect) -> f32 {
        let p = self.pal.clone();
        self.cards.clear();
        let mut y = y;
        draw::text(c, greeting(), rect(x, y, w, 44.0), 32.0, 800, p.t1, Align::Left);
        y += 60.0;
        // quick tiles: liked songs and the playlists
        let mut tiles: Vec<Card> = Vec::new();
        if !self.lib.liked().is_empty() {
            tiles.push(Card { kind: CardKind::Liked, title: tr("Liked songs").into(), sub: String::new(), cover: None });
        }
        let ids: Vec<String> = self.lib.playlists.iter().map(|pl| pl.id.clone()).collect();
        for id in ids.iter().take(7) {
            if let Some(card) = self.playlist_card(id) {
                tiles.push(card);
            }
        }
        if tiles.is_empty() && self.lib.recent.is_empty() {
            // first launch
            let b = rect(x, y, w.min(720.0), 170.0);
            c.fill_round(b, 18.0, p.card);
            draw::text(c, tr("Start listening"), rect(b.x + 28.0, b.y + 24.0, b.w - 56.0, 34.0), 22.0, 700, p.t1, Align::Left);
            c.text_block(
                tr("Search for a song, an artist or an album, or import a playlist from Spotify, Apple Music, Deezer or SoundCloud."),
                rect(b.x + 28.0, b.y + 64.0, b.w - 56.0, 40.0),
                Font::Body,
                p.t2,
            );
            self.pill(c, rect(b.x + 28.0, b.bottom() - 54.0, 150.0, 38.0), tr("Search"), true, Hit::Nav(View::Search));
            self.pill(c, rect(b.x + 190.0, b.bottom() - 54.0, 200.0, 38.0), tr("Import a playlist"), false, Hit::ImportOpen);
            y += 200.0;
        }
        if !tiles.is_empty() {
            let cols = if w > 900.0 { 4 } else { 2 };
            let gap = 12.0;
            let tw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
            let base = self.cards.len();
            for (i, card) in tiles.iter().enumerate() {
                let r = rect(x + (i % cols) as f32 * (tw + gap), y + (i / cols) as f32 * 68.0, tw, 58.0);
                let hit = Hit::Card(base + i);
                c.fill_round(r, 8.0, if self.hovered(&hit) { p.t1.fade(0.16) } else { p.t1.fade(0.08) });
                let cr = rect(r.x, r.y, 58.0, 58.0);
                if matches!(card.kind, CardKind::Liked) {
                    draw::rounded(c, cr, 8.0, |c| c.gradient_v(cr, Color::hex(0x6d4aff), Color::hex(0xc3b7ff)));
                    c.icon_fill(&ic::HEART, rect(cr.x + 19.0, cr.y + 19.0, 20.0, 20.0), Color::hex(0xffffff));
                } else {
                    self.cover(c, card.cover.as_deref(), cr, 8.0, 128);
                }
                c.text(&card.title, rect(r.x + 72.0, r.y, r.w - 84.0, r.h), Font::BodyBold, p.t1, Align::Left);
                self.hits.add(r, hit);
            }
            self.cards.extend(tiles.iter().cloned());
            y += tiles.len().div_ceil(cols) as f32 * 68.0 + 24.0;
        }
        // recently played
        let recent: Vec<Card> = self
            .lib
            .recent
            .iter()
            .filter_map(|k| self.lib.tracks.get(k))
            .take(12)
            .map(|t| Card { kind: CardKind::Track(t.key.clone()), title: t.title.clone(), sub: t.artist.clone(), cover: art::source(t) })
            .collect();
        if !recent.is_empty() {
            y = self.section(c, x, y, w, tr("Recently played"));
            y = self.cards_grid(c, x, y, w, page, recent, false, false);
        }
        // made for you: radios of the most played songs
        let mut top: Vec<&Track> = self.lib.tracks.values().filter(|t| t.plays > 0 && t.yt.is_some()).collect();
        top.sort_by_key(|t| std::cmp::Reverse(t.plays));
        let mut seen = std::collections::HashSet::new();
        let mixes: Vec<Card> = top
            .into_iter()
            .filter(|t| seen.insert(t.artist.clone()))
            .take(8)
            .map(|t| Card {
                kind: CardKind::Mix(t.key.clone()),
                title: format!("{} · {}", tr("Radio"), t.artist.split(',').next().unwrap_or("")),
                sub: tr("Similar songs").into(),
                cover: art::source(t),
            })
            .collect();
        if !mixes.is_empty() {
            y = self.section(c, x, y, w, tr("Made for you"));
            y = self.cards_grid(c, x, y, w, page, mixes, false, false);
        }
        // recently added albums
        let albums: Vec<Card> = self
            .lib
            .albums()
            .into_iter()
            .take(12)
            .map(|(ar, al, keys)| Card {
                kind: CardKind::Album(format!("{}\u{1}{}", ar.split([',', '&']).next().unwrap_or("").trim().to_lowercase(), al.to_lowercase())),
                title: al,
                sub: ar,
                cover: keys.first().and_then(|k| self.lib.tracks.get(k)).and_then(art::source),
            })
            .collect();
        if !albums.is_empty() {
            y = self.section(c, x, y, w, tr("Recently added"));
            y = self.cards_grid(c, x, y, w, page, albums, false, false);
        }
        y + self.scroll
    }

    fn paint_search(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, page: Rect) -> f32 {
        let p = self.pal.clone();
        self.cards.clear();
        let mut y = y;
        if self.query.trim().is_empty() {
            if !self.lib.searches.is_empty() {
                y = self.section(c, x, y, w, tr("Recent searches"));
                let mut cx_ = x;
                let mut cy = y;
                let list = self.lib.searches.clone();
                for (i, s) in list.iter().enumerate() {
                    let tw = c.text_width(s, Font::Body) + 32.0;
                    if cx_ + tw > x + w {
                        cx_ = x;
                        cy += 44.0;
                    }
                    let r = rect(cx_, cy, tw, 36.0);
                    let hit = Hit::Recent(i);
                    c.fill_round(r, 18.0, if self.hovered(&hit) { p.t1.fade(0.16) } else { p.t1.fade(0.08) });
                    c.text(s, r, Font::Body, p.t1, Align::Center);
                    self.hits.add(r, hit);
                    cx_ += tw + 8.0;
                }
                y = cy + 60.0;
            }
            let b = rect(x, y, w.min(720.0), 130.0);
            c.fill_round(b, 18.0, p.card);
            c.text_block(
                tr("Search for a song, an artist or an album, or import a playlist from Spotify, Apple Music, Deezer or SoundCloud."),
                rect(b.x + 24.0, b.y + 22.0, b.w - 48.0, 40.0),
                Font::Body,
                p.t2,
            );
            self.pill(c, rect(b.x + 24.0, b.bottom() - 52.0, 200.0, 36.0), tr("Import a playlist"), true, Hit::ImportOpen);
            return y + 150.0 + self.scroll;
        }
        // suggestions
        if !self.suggestions.is_empty() {
            let mut cx_ = x;
            let list = self.suggestions.clone();
            for (i, s) in list.iter().enumerate() {
                let tw = c.text_width(s, Font::Small) + 26.0;
                if cx_ + tw > x + w {
                    break;
                }
                let r = rect(cx_, y, tw, 30.0);
                let hit = Hit::Suggestion(i);
                c.fill_round(r, 15.0, if self.hovered(&hit) { p.t1.fade(0.16) } else { p.t1.fade(0.08) });
                c.text(s, r, Font::Small, p.t1, Align::Center);
                self.hits.add(r, hit);
                cx_ += tw + 8.0;
            }
            y += 46.0;
        }
        let keys = self.page_keys();
        let nlocal = self.local_results.len();
        if nlocal > 0 {
            y = self.section(c, x, y, w, tr("In your library"));
            y = self.track_rows(c, x, y, w, page, &keys[..nlocal], 0, false);
            y += 16.0;
        }
        y = self.section(c, x, y, w, tr("On YouTube Music"));
        if self.searching && self.results.is_empty() {
            c.text(tr("Searching…"), rect(x, y, w, 24.0), Font::Body, p.t3, Align::Left);
            y += 40.0;
        } else if let Some(e) = self.search_error.clone() {
            c.text(&e, rect(x, y, w, 24.0), Font::Body, p.t3, Align::Left);
            y += 40.0;
        } else if self.results.is_empty() {
            c.text(tr("No results"), rect(x, y, w, 24.0), Font::Body, p.t3, Align::Left);
            y += 40.0;
        } else {
            y = self.track_rows(c, x, y, w, page, &keys[nlocal..], nlocal, false);
        }
        y + self.scroll
    }

    fn paint_albums(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, page: Rect) -> f32 {
        let p = self.pal.clone();
        self.cards.clear();
        draw::text(c, tr("Albums"), rect(x, y, w, 50.0), 34.0, 800, p.t1, Align::Left);
        let cards: Vec<Card> = self
            .lib
            .albums()
            .into_iter()
            .map(|(ar, al, keys)| Card {
                kind: CardKind::Album(format!("{}\u{1}{}", ar.split([',', '&']).next().unwrap_or("").trim().to_lowercase(), al.to_lowercase())),
                title: al,
                sub: ar,
                cover: keys.first().and_then(|k| self.lib.tracks.get(k)).and_then(art::source),
            })
            .collect();
        if cards.is_empty() {
            c.text(tr("No results"), rect(x, y + 70.0, w, 24.0), Font::Body, p.t3, Align::Left);
            return y + 120.0 + self.scroll;
        }
        self.cards_grid(c, x, y + 70.0, w, page, cards, true, false) + self.scroll
    }

    fn paint_artists(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, page: Rect) -> f32 {
        let p = self.pal.clone();
        self.cards.clear();
        draw::text(c, tr("Artists"), rect(x, y, w, 50.0), 34.0, 800, p.t1, Align::Left);
        let list = self.lib.artists();
        let cards: Vec<Card> = list
            .into_iter()
            .map(|(name, n)| {
                let lower = name.to_lowercase();
                let cover = self.lib.tracks.values().find(|t| t.artist.to_lowercase().starts_with(&lower)).and_then(art::source);
                Card {
                    kind: CardKind::Artist(name.clone()),
                    title: name,
                    sub: if n == 1 { tr("1 song").into() } else { fill(tr("{n} songs"), &[&n.to_string()]) },
                    cover,
                }
            })
            .collect();
        if cards.is_empty() {
            c.text(tr("No results"), rect(x, y + 70.0, w, 24.0), Font::Body, p.t3, Align::Left);
            return y + 120.0 + self.scroll;
        }
        self.cards_grid(c, x, y + 70.0, w, page, cards, true, true) + self.scroll
    }

    /// A page of songs with its header (playlist, album, artist, liked songs, downloads, all songs).
    fn paint_list(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, page: Rect) -> f32 {
        let p = self.pal.clone();
        self.refresh_rows();
        let keys = self.rows.clone();
        let (kind, title, cover): (&str, String, Option<String>) = match &self.view {
            View::Playlist(id) => (
                tr("Playlist"),
                self.page_title(),
                self.lib.playlist(id).and_then(|pl| pl.cover.clone()).or_else(|| keys.first().and_then(|k| self.lib.tracks.get(k)).and_then(art::source)),
            ),
            View::Album(_) => (tr("Album"), self.page_title(), keys.first().and_then(|k| self.lib.tracks.get(k)).and_then(art::source)),
            View::Artist(_) => (tr("Artist"), self.page_title(), keys.first().and_then(|k| self.lib.tracks.get(k)).and_then(art::source)),
            _ => ("", self.page_title(), None),
        };
        let mut y = y;
        let big = 196.0;

        let hx = if matches!(self.view, View::Liked) {
            let cr = rect(x, y, big, big);
            draw::rounded(c, cr, 12.0, |c| c.gradient_v(cr, Color::hex(0x6d4aff), Color::hex(0xc3b7ff)));
            c.icon_fill(&ic::HEART, rect(cr.x + 68.0, cr.y + 68.0, 60.0, 60.0), Color::hex(0xffffff));
            x + big + 28.0
        } else if !kind.is_empty() {
            let round = matches!(self.view, View::Artist(_));
            self.cover(c, cover.as_deref(), rect(x, y, big, big), if round { big / 2.0 } else { 12.0 }, 400);
            x + big + 28.0
        } else {
            x
        };
        let hw = x + w - hx;
        if !kind.is_empty() {
            c.text(kind, rect(hx, y + 54.0, hw, 20.0), Font::SmallBold, p.t2, Align::Left);
        }
        // the biggest size that fits
        let size = [56.0, 46.0, 38.0, 30.0, 24.0].into_iter().find(|s| draw::text_width(c, &title, *s, 800) <= hw).unwrap_or(24.0);
        let ty = if kind.is_empty() && !matches!(self.view, View::Liked) { y } else { y + 74.0 };
        draw::text(c, &title, rect(hx, ty, hw, size + 12.0), size, 800, p.t1, Align::Left);
        let total: f64 = keys.iter().filter_map(|k| self.lib.tracks.get(k)).map(|t| t.dur).sum();
        let n = keys.len();
        let mut sub = if n == 1 { tr("1 song").to_string() } else { fill(tr("{n} songs"), &[&n.to_string()]) };
        if total > 0.0 {
            sub.push_str(&format!(" · {}", clock(total)));
        }
        if let View::Playlist(id) = &self.view
            && let Some(src) = self.lib.playlist(id).and_then(|pl| pl.source.clone())
        {
            let svc = match crate::import::service(&src) {
                Some(crate::import::Service::Spotify) => " · Spotify",
                Some(crate::import::Service::Apple) => " · Apple Music",
                Some(crate::import::Service::Deezer) => " · Deezer",
                Some(crate::import::Service::SoundCloud) => " · SoundCloud",
                Some(crate::import::Service::YouTube) => " · YouTube",
                None => "",
            };
            sub.push_str(svc);
        }
        c.text(&sub, rect(hx, ty + size + 14.0, hw, 20.0), Font::Body, p.t2, Align::Left);
        y += if kind.is_empty() && !matches!(self.view, View::Liked) { size + 50.0 } else { big + 24.0 };
        // play, shuffle, download all, more
        if n > 0 {
            let pb = rect(x, y, 56.0, 56.0);
            let hot = self.hovered(&Hit::PlayAll);
            c.fill_round(if hot { pb.inset(-2.0) } else { pb }, 28.0, p.accent);
            c.icon_fill(&ic::PLAY, rect(pb.x + 17.0, pb.y + 16.0, 24.0, 24.0), p.on_accent);
            self.hits.add(pb, Hit::PlayAll);
            self.icon_btn(c, &ic::SHUFFLE, rect(x + 72.0, y + 10.0, 36.0, 36.0), Hit::ShuffleAll, if self.set.shuffle { p.accent } else { p.t2 });
            let all_local = keys.iter().all(|k| self.lib.tracks.get(k).is_some_and(|t| t.local()));
            self.icon_btn(
                c,
                if all_local { &ic::DONE } else { &ic::DOWNLOAD },
                rect(x + 116.0, y + 10.0, 36.0, 36.0),
                Hit::DownloadAll,
                if all_local { p.accent } else { p.t2 },
            );
            if matches!(self.view, View::Playlist(_)) {
                self.icon_btn(c, &icons::MORE, rect(x + 160.0, y + 10.0, 36.0, 36.0), Hit::HeaderMore, p.t2);
            }
            y += 76.0;
        } else {
            c.text(tr("No results"), rect(x, y, w, 24.0), Font::Body, p.t3, Align::Left);
            y += 40.0;
        }
        // the table
        let numbered = !matches!(self.view, View::Downloads);
        y = self.track_rows(c, x, y, w, page, &keys, 0, numbered);
        y + self.scroll
    }

    /// Rows of songs (only those in view are drawn). `base`: index of the first in the page's songs.
    #[allow(clippy::too_many_arguments)]
    pub fn track_rows(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, page: Rect, keys: &[String], base: usize, numbered: bool) -> f32 {
        let p = self.pal.clone();
        let wide = w > 720.0;
        let playing_key = self.cur.as_ref().map(|(_, k)| k.clone()).or(self.loading.clone());
        // column titles
        let mut yy = y;
        if numbered {
            c.text("#", rect(x + 8.0, yy, 28.0, 30.0), Font::Small, p.t3, Align::Center);
            c.text(tr("Title"), rect(x + 52.0, yy, 200.0, 30.0), Font::Small, p.t3, Align::Left);
            if wide {
                c.text(tr("Album"), rect(x + w * 0.55, yy, 200.0, 30.0), Font::Small, p.t3, Align::Left);
            }
            c.text(tr("Duration"), rect(x + w - 150.0, yy, 90.0, 30.0), Font::Small, p.t3, Align::Right);
            yy += 34.0;
            c.hline(x, yy - 2.0, w, p.line);
        }
        for (j, key) in keys.iter().enumerate() {
            let i = base + j;
            let r = rect(x, yy + j as f32 * ROW_H, w, ROW_H - 2.0);
            if r.bottom() < page.y + TOP_H - 40.0 || r.y > page.bottom() {
                continue;
            }
            let Some(t) = self.lib.tracks.get(key).cloned() else { continue };
            let hit = Hit::Row(i);
            let hot = self.hovered(&hit)
                || self.hovered(&Hit::RowPlay(i))
                || self.hovered(&Hit::RowLike(i))
                || self.hovered(&Hit::RowMore(i))
                || self.hovered(&Hit::RowDownload(i));
            let selected = self.sel == Some(i);
            if selected {
                c.fill_round(r, 8.0, p.t1.fade(0.14));
            } else if hot {
                c.fill_round(r, 8.0, p.t1.fade(0.07));
            }
            self.hits.add(r, hit);
            let is_playing = playing_key.as_deref() == Some(key.as_str());
            // number, or play on hover, or the bars of the song playing
            let nr = rect(r.x + 4.0, r.y, 36.0, r.h);
            if hot {
                c.icon_fill(&ic::PLAY, rect(nr.x + 10.0, nr.y + (nr.h - 16.0) / 2.0, 16.0, 16.0), p.t1);
                self.hits.add(nr, Hit::RowPlay(i));
            } else if is_playing && self.st.playing {
                let t0 = self.t0.elapsed().as_secs_f32();
                for b in 0..3 {
                    let hgt = 5.0 + (((t0 * (5.0 + b as f32 * 1.7)).sin() + 1.0) * 5.0);
                    c.fill_round(rect(nr.x + 10.0 + b as f32 * 6.0, nr.y + nr.h / 2.0 + 8.0 - hgt, 4.0, hgt), 1.0, p.accent);
                }
            } else if numbered {
                c.text(&(j + 1).to_string(), nr, Font::Body, if is_playing { p.accent } else { p.t3 }, Align::Center);
            } else {
                c.icon(&ic::NOTE, rect(nr.x + 10.0, nr.y + (nr.h - 16.0) / 2.0, 16.0, 16.0), p.t3);
            }
            let cr = rect(r.x + 48.0, r.y + 8.0, 42.0, 42.0);
            let src = art::source(&t);
            self.cover(c, src.as_deref(), cr, 5.0, 96);
            let tx = cr.right() + 12.0;
            let title_w = if wide { x + w * 0.55 - tx - 16.0 } else { r.right() - 170.0 - tx };
            c.text(&t.title, rect(tx, r.y + 9.0, title_w, 22.0), Font::BodyBold, if is_playing { p.accent } else { p.t1 }, Align::Left);
            let mut ax = tx;
            if t.explicit {
                let e = rect(tx, r.y + 33.0, 15.0, 15.0);
                c.fill_round(e, 3.0, p.t3);
                c.text("E", e, Font::SmallBold, p.bg, Align::Center);
                ax += 20.0;
            }
            c.text(&t.artist, rect(ax, r.y + 30.0, title_w - (ax - tx), 20.0), Font::Small, p.t2, Align::Left);
            if wide {
                c.text(&t.album, rect(x + w * 0.55, r.y, w * 0.45 - 190.0, r.h), Font::Small, p.t2, Align::Left);
            }
            // download state, like, duration, more
            let dl = self.downloads.get(key).cloned();
            let dr = rect(r.right() - 182.0, r.y + 13.0, 32.0, 32.0);
            match (&dl, t.local()) {
                (_, true) => c.icon(&ic::DONE, dr.inset(8.0), p.accent),
                (Some(Dl::Queued | Dl::Running), _) => {
                    let a = self.t0.elapsed().as_secs_f32() * 4.0;
                    c.icon(&ic::DOWNLOAD, rect(dr.x + 8.0, dr.y + 8.0 + a.sin() * 2.0, 16.0, 16.0), p.accent);
                }
                (Some(Dl::Failed(_)), _) => c.icon(&ic::DOWNLOAD, dr.inset(8.0), p.red()),
                _ if hot => self.icon_btn(c, &ic::DOWNLOAD, dr, Hit::RowDownload(i), p.t2),
                _ => {}
            }
            let lr = rect(r.right() - 146.0, r.y + 13.0, 32.0, 32.0);
            if t.liked {
                self.icon_fill_btn(c, &ic::HEART, lr, Hit::RowLike(i), p.accent);
            } else if hot {
                self.icon_btn(c, &ic::HEART, lr, Hit::RowLike(i), p.t2);
            }
            if t.dur > 0.0 {
                c.text(&clock(t.dur), rect(r.right() - 110.0, r.y, 56.0, r.h), Font::Small, p.t3, Align::Right);
            }
            if hot {
                self.icon_btn(c, &icons::MORE, rect(r.right() - 44.0, r.y + 13.0, 32.0, 32.0), Hit::RowMore(i), p.t2);
            }
        }
        yy + keys.len() as f32 * ROW_H + 10.0
    }

    // ───────────────────────── what clicks do ─────────────────────────

    pub fn act(&mut self, hit: Hit, p: Pt, cx: &mut Cx) {
        match hit {
            Hit::Min => cx.after(|w| w.minimize()),
            Hit::Max => cx.after(|w| w.toggle_maximize()),
            Hit::Close => cx.after(|w| w.close()),
            Hit::Back => self.nav_back(cx),
            Hit::Forward => self.nav_forward(cx),
            Hit::SearchBox => {
                if self.view != View::Search {
                    self.go(View::Search, cx);
                }
                self.edit_for = Some("search");
                self.edit_shown = None;
            }
            Hit::Suggestion(i) => {
                if let Some(s) = self.suggestions.get(i).cloned() {
                    self.query = s;
                    self.win.edit_set_text(&self.query);
                    self.remember_search();
                    self.run_search(cx);
                }
            }
            Hit::Recent(i) => {
                if let Some(s) = self.lib.searches.get(i).cloned() {
                    self.query = s;
                    self.edit_shown = None;
                    self.local_results = self.lib.search(&self.query);
                    self.run_search(cx);
                }
            }
            Hit::Nav(v) => self.go(v, cx),
            Hit::NavPlaylist(id) => self.go(View::Playlist(id), cx),
            Hit::NewPlaylist => {
                self.modal = Some(Modal::NewPlaylist { with: None });
                self.modal_text = fill(tr("My playlist #{n}"), &[&(self.lib.playlists.len() + 1).to_string()]);
                self.edit_for = Some("modal");
                self.edit_shown = None;
            }
            Hit::ImportOpen => {
                self.open_import(None, cx);
                self.edit_shown = None;
            }
            Hit::SidebarToggle => {
                self.set.sidebar_small = !self.set.sidebar_small;
                self.set.save();
            }
            Hit::Row(i) => self.sel = Some(i),
            Hit::RowPlay(i) => self.play_rows(i, cx),
            Hit::RowLike(i) => {
                if let Some(k) = self.page_keys().get(i).cloned() {
                    self.toggle_like(&k, cx);
                }
            }
            Hit::RowDownload(i) => {
                if let Some(k) = self.page_keys().get(i).cloned() {
                    self.download(&k, cx);
                }
            }
            Hit::RowMore(i) => self.row_menu(i, p, cx),
            Hit::Card(i) => self.open_card(i, false, cx),
            Hit::CardPlay(i) => self.open_card(i, true, cx),
            Hit::PlayAll => {
                let keys = self.page_keys();
                let ctx = self.page_title();
                let start = if self.set.shuffle && !keys.is_empty() { (crate::library::now() as usize) % keys.len() } else { 0 };
                self.play_list(keys, start, ctx, cx);
            }
            Hit::ShuffleAll => {
                if !self.set.shuffle {
                    self.set.shuffle = true;
                    self.set.save();
                }
                let keys = self.page_keys();
                let ctx = self.page_title();
                let start = if keys.is_empty() { 0 } else { (crate::library::now() as usize) % keys.len() };
                self.play_list(keys, start, ctx, cx);
            }
            Hit::DownloadAll => {
                let keys = self.page_keys();
                for k in keys {
                    self.download(&k, cx);
                }
            }
            Hit::HeaderMore => {
                if let View::Playlist(id) = self.view.clone() {
                    self.playlist_menu(&id, p, cx);
                }
            }
            Hit::PlayPause => self.toggle_play(cx),
            Hit::Next => self.skip(1, cx),
            Hit::Prev => self.skip(-1, cx),
            Hit::Shuffle => self.toggle_shuffle(cx),
            Hit::Repeat => self.cycle_repeat(cx),
            Hit::Mute => {
                match self.muted_volume.take() {
                    Some(v) => self.set.volume = v,
                    None => {
                        self.muted_volume = Some(self.set.volume);
                        self.set.volume = 0.0;
                    }
                }
                self.engine.send(Cmd::Volume(self.set.volume));
            }
            Hit::LikeNow => {
                if let Some(k) = self.current().map(|t| t.key.clone()) {
                    self.toggle_like(&k, cx);
                }
            }
            Hit::LyricsBtn => {
                if self.now_open {
                    self.now_tab = Panel::Lyrics;
                } else {
                    self.toggle_panel(Panel::Lyrics, cx);
                }
            }
            Hit::QueueBtn => {
                if self.now_open {
                    self.now_tab = Panel::Queue;
                } else {
                    self.toggle_panel(Panel::Queue, cx);
                }
            }
            Hit::NowOpen => {
                self.now_open = true;
                self.menu = None;
            }
            Hit::NowClose => self.now_open = false,
            Hit::NowTab(t) => {
                if self.now_open {
                    self.now_tab = t;
                } else {
                    self.panel = Some(t);
                }
            }
            Hit::NowTitle | Hit::NowArtist => {
                if let Some(a) = self.current().map(|t| t.artist.split(',').next().unwrap_or("").trim().to_string())
                    && !a.is_empty()
                {
                    self.query = a;
                    self.edit_shown = None;
                    self.go(View::Search, cx);
                    self.local_results = self.lib.search(&self.query);
                    self.run_search(cx);
                }
            }
            Hit::LyricLine(i) => {
                if let Some((_, Some(l))) = &self.lyrics
                    && l.synced
                    && let Some((t, _)) = l.lines.get(i)
                {
                    self.engine.send(Cmd::Seek(*t));
                }
            }
            Hit::QueueRow(i) => self.play_at(i, cx),
            Hit::QueueRemove(i) => {
                if i < self.queue.len() && i != self.qpos {
                    self.queue.remove(i);
                    if i < self.qpos {
                        self.qpos -= 1;
                    }
                    self.prepare_next();
                }
            }
            Hit::QueueClear => {
                let cur = self.queue.get(self.qpos).cloned();
                self.queue = cur.into_iter().collect();
                self.qpos = 0;
                self.prepare_next();
            }
            Hit::Timer => {
                let mut items: Vec<(String, MenuAct)> = vec![(tr("Sleep timer").into(), MenuAct::Header)];
                for m in [15, 30, 45, 60, 90] {
                    items.push((fill(tr("Stop in {n} min"), &[&m.to_string()]), MenuAct::Sleep(m)));
                }
                items.push((tr("End of song").into(), MenuAct::SleepEnd));
                if self.sleep_at.is_some() || self.sleep_end_of_song {
                    items.push((tr("Off").into(), MenuAct::SleepOff));
                }
                self.menu = Some(Menu { x: p.x - 100.0, y: p.y - 10.0, items });
            }
            Hit::MenuItem(i) => {
                if let Some(m) = self.menu.take()
                    && let Some((_, a)) = m.items.get(i).cloned()
                {
                    self.menu_act(a, cx);
                }
            }
            Hit::ModalOk => self.modal_ok(cx),
            Hit::ModalCancel => self.close_modal(cx),
            Hit::ModalField => {
                self.edit_for = Some("modal");
                self.edit_shown = None;
            }
            Hit::Toast => self.toast = None,
            Hit::Choice(..) | Hit::Toggle(_) | Hit::Action(_) | Hit::Preset(_) | Hit::FolderRemove(_) => self.settings_act(hit, cx),
            Hit::Seek | Hit::Volume | Hit::Slider(_) | Hit::EqBand(_) | Hit::Scrollbar => {}
        }
        cx.repaint();
    }

    fn open_card(&mut self, i: usize, play: bool, cx: &mut Cx) {
        let Some(card) = self.cards.get(i).cloned() else { return };
        match card.kind {
            CardKind::Playlist(id) => {
                if play {
                    let keys = self.lib.playlist(&id).map(|p| p.tracks.clone()).unwrap_or_default();
                    self.play_list(keys, 0, card.title, cx);
                } else {
                    self.go(View::Playlist(id), cx);
                }
            }
            CardKind::Album(k) => {
                if play {
                    self.go(View::Album(k), cx);
                    let keys = self.page_keys();
                    self.play_list(keys, 0, card.title, cx);
                } else {
                    self.go(View::Album(k), cx);
                }
            }
            CardKind::Artist(a) => self.go(View::Artist(a), cx),
            CardKind::Liked => {
                if play {
                    let keys = self.lib.liked();
                    self.play_list(keys, 0, tr("Liked songs").into(), cx);
                } else {
                    self.go(View::Liked, cx);
                }
            }
            CardKind::Mix(k) => self.start_radio(&k, cx),
            CardKind::Track(k) => {
                // a recently played song: from there on, the recent ones
                let keys: Vec<String> = self.lib.recent.iter().filter(|k| self.lib.tracks.contains_key(*k)).take(50).cloned().collect();
                let start = keys.iter().position(|x| *x == k).unwrap_or(0);
                self.play_list(keys, start, tr("Recently played").into(), cx);
            }
        }
    }

    fn modal_ok(&mut self, cx: &mut Cx) {
        let Some(m) = self.modal.clone() else { return };
        let text = self.win.edit_text().filter(|_| self.edit_shown == Some("modal")).unwrap_or_else(|| self.modal_text.clone());
        self.modal_text = text.clone();
        match m {
            Modal::Import { stage: ImportStage::Done { playlist, .. } } => {
                self.close_modal(cx);
                self.go(View::Playlist(playlist), cx);
            }
            Modal::Import { .. } => {
                if !matches!(&m, Modal::Import { stage: ImportStage::Reading | ImportStage::Matching { .. } }) {
                    self.start_import(cx);
                }
            }
            Modal::NewPlaylist { with } => {
                let name = text.trim();
                if name.is_empty() {
                    return;
                }
                let id = self.lib.new_playlist(name);
                if let Some(k) = with {
                    self.add_to_playlist(&k, &id, cx);
                }
                self.lib.save();
                self.close_modal(cx);
                self.go(View::Playlist(id), cx);
            }
            Modal::Rename(id) => {
                let name = text.trim().to_string();
                if let Some(pl) = self.lib.playlist_mut(&id)
                    && !name.is_empty()
                {
                    pl.name = name;
                }
                self.close_modal(cx);
            }
            Modal::Tools => self.install_tools(false, cx),
        }
    }

    // ───────────────────────── drags ─────────────────────────

    pub fn drag_start(&self, h: &Hit, _p: Pt) -> Option<Drag> {
        match h {
            Hit::Seek if self.cur.is_some() => Some(Drag::Seek(0.0)),
            Hit::Volume => Some(Drag::Volume),
            Hit::Slider(s) => Some(Drag::Slider(s)),
            Hit::EqBand(i) => Some(Drag::EqBand(*i)),
            Hit::Scrollbar => Some(Drag::Scrollbar(0.0, 0.0)),
            _ => None,
        }
    }

    fn rect_of(&self, h: &Hit) -> Option<Rect> {
        self.hits.rect_of(|x| x == h)
    }

    pub fn drag_to(&mut self, d: Drag, p: Pt, cx: &mut Cx) {
        let frac = |r: Rect| ((p.x - r.x) / r.w.max(1.0)).clamp(0.0, 1.0);
        match d {
            Drag::Seek(_) => {
                if let Some(r) = self.rect_of(&Hit::Seek) {
                    self.drag = Some(Drag::Seek(frac(r)));
                }
            }
            Drag::Volume => {
                if let Some(r) = self.rect_of(&Hit::Volume) {
                    self.set.volume = frac(r);
                    self.muted_volume = None;
                    self.engine.send(Cmd::Volume(self.set.volume));
                }
            }
            Drag::Slider(name) => {
                if let Some(r) = self.rect_of(&Hit::Slider(name)) {
                    self.set_slider(name, frac(r));
                }
            }
            Drag::EqBand(i) => {
                if let Some(r) = self.rect_of(&Hit::EqBand(i)) {
                    let v = ((r.bottom() - p.y) / r.h).clamp(0.0, 1.0);
                    let db = (v * 24.0 - 12.0).round();
                    if i == 10 {
                        self.set.preamp = db;
                    } else {
                        self.set.eq[i] = db;
                    }
                    self.set.preset = "Custom".into();
                    self.set.eq_on = true;
                    self.engine.send(Cmd::Eq(self.set.eq_cmd()));
                }
            }
            Drag::Scrollbar(..) => {
                let track = self.view_h - super::TOP_H - 8.0;
                let max = (self.content_h - self.view_h).max(0.0);
                let f = ((p.y - super::TOP_H) / track.max(1.0)).clamp(0.0, 1.0);
                self.scroll_to = f * max;
                self.scroll = self.scroll_to;
            }
        }
        cx.repaint();
    }

    pub fn drag_end(&mut self, d: Drag, _p: Pt, _cx: &mut Cx) {
        match d {
            Drag::Seek(_) => {
                if let Some(Drag::Seek(v)) = Some(d.clone()) {
                    let _ = v;
                }
                // the last position of the drag (kept in self.drag until now)
            }
            Drag::Volume | Drag::Slider(_) | Drag::EqBand(_) => self.set.save(),
            Drag::Scrollbar(..) => {}
        }
        if let Drag::Seek(v) = self.last_seek(d) {
            let dur = self.shown_position().1;
            if dur > 0.0 {
                self.engine.send(Cmd::Seek(v as f64 * dur));
                self.st.pos = v as f64 * dur;
            }
        }
    }

    fn last_seek(&self, d: Drag) -> Drag {
        d
    }

    // ───────────────────────── menus ─────────────────────────

    pub fn row_menu(&mut self, i: usize, p: Pt, cx: &mut Cx) {
        let Some(k) = self.page_keys().get(i).cloned() else { return };
        self.sel = Some(i);
        let pos = match &self.view {
            View::Playlist(id) => Some((id.clone(), i)),
            _ => None,
        };
        self.track_menu(&k, pos, p, cx);
    }

    pub fn track_menu(&mut self, key: &str, in_playlist: Option<(String, usize)>, p: Pt, cx: &mut Cx) {
        let Some(t) = self.lib.tracks.get(key).cloned() else { return };
        let k = key.to_string();
        let mut items: Vec<(String, MenuAct)> = vec![
            (tr("Play next").into(), MenuAct::PlayNext(k.clone())),
            (tr("Add to queue").into(), MenuAct::AddQueue(k.clone())),
            ((if t.liked { "♥ " } else { "♡ " }).to_string() + tr("Like"), MenuAct::Like(k.clone())),
        ];
        if !t.local() {
            items.push((tr("Download").into(), MenuAct::Download(k.clone())));
        }
        if t.yt.is_some() {
            items.push((tr("Song radio").into(), MenuAct::Radio(k.clone())));
        }
        if !t.album.is_empty() {
            items.push((tr("Go to album").into(), MenuAct::GoAlbum(k.clone())));
        }
        if !t.artist.is_empty() {
            items.push((tr("Go to artist").into(), MenuAct::GoArtist(k.clone())));
        }
        if t.local() {
            items.push((tr("Show in folder").into(), MenuAct::ShowFile(k.clone())));
        }
        if t.yt.is_some() || t.url.is_some() {
            items.push((tr("Copy link").into(), MenuAct::CopyLink(k.clone())));
        }
        if let Some((id, i)) = in_playlist {
            items.push((tr("Remove from playlist").into(), MenuAct::RemoveFrom(id, i)));
        }
        items.push((tr("Add to playlist").into(), MenuAct::Header));
        items.push(("+ ".to_string() + tr("New playlist"), MenuAct::NewPlaylistWith(k.clone())));
        for pl in self.lib.playlists.iter().take(8) {
            items.push((format!("   {}", pl.name), MenuAct::AddTo(k.clone(), pl.id.clone())));
        }
        self.menu = Some(Menu { x: p.x, y: p.y, items });
        cx.repaint();
    }

    pub fn playlist_menu(&mut self, id: &str, p: Pt, cx: &mut Cx) {
        let items = vec![
            (tr("Download all").into(), MenuAct::DownloadPlaylist(id.to_string())),
            (tr("Rename").into(), MenuAct::Rename(id.to_string())),
            (tr("Delete playlist").into(), MenuAct::DeletePlaylist(id.to_string())),
        ];
        self.menu = Some(Menu { x: p.x, y: p.y, items });
        cx.repaint();
    }

    fn menu_act(&mut self, a: MenuAct, cx: &mut Cx) {
        match a {
            MenuAct::PlayNext(k) => {
                self.play_next(&k);
                self.say(tr("Added to the queue"), cx);
            }
            MenuAct::AddQueue(k) => {
                self.add_queue(&k);
                self.say(tr("Added to the queue"), cx);
            }
            MenuAct::AddTo(k, id) => self.add_to_playlist(&k, &id, cx),
            MenuAct::NewPlaylistWith(k) => {
                self.modal = Some(Modal::NewPlaylist { with: Some(k) });
                self.modal_text = fill(tr("My playlist #{n}"), &[&(self.lib.playlists.len() + 1).to_string()]);
                self.edit_for = Some("modal");
                self.edit_shown = None;
            }
            MenuAct::Like(k) => self.toggle_like(&k, cx),
            MenuAct::Download(k) => self.download(&k, cx),
            MenuAct::GoAlbum(k) => {
                if let Some(t) = self.lib.tracks.get(&k) {
                    let key = t.album_key();
                    if !t.local() && !t.liked {
                        // an album known only from YouTube Music: search it there
                        self.query = format!("{} {}", t.artist.split(',').next().unwrap_or(""), t.album);
                        self.edit_shown = None;
                        self.go(View::Search, cx);
                        self.local_results = self.lib.search(&self.query);
                        self.run_search(cx);
                    } else {
                        self.go(View::Album(key), cx);
                    }
                }
            }
            MenuAct::GoArtist(k) => {
                if let Some(a) = self.lib.tracks.get(&k).map(|t| t.artist.split(',').next().unwrap_or("").trim().to_string()) {
                    self.query = a.clone();
                    self.edit_shown = None;
                    self.go(View::Search, cx);
                    self.local_results = self.lib.search(&a);
                    self.run_search(cx);
                }
            }
            MenuAct::Radio(k) => self.start_radio(&k, cx),
            MenuAct::RemoveFrom(id, i) => {
                if let Some(pl) = self.lib.playlist_mut(&id)
                    && i < pl.tracks.len()
                {
                    pl.tracks.remove(i);
                }
                self.rows_for = None;
            }
            MenuAct::ShowFile(k) => {
                if let Some(p) = self.lib.tracks.get(&k).and_then(|t| t.path.clone()) {
                    plume_ui::folder::show(&p);
                }
            }
            MenuAct::CopyLink(k) => {
                if let Some(t) = self.lib.tracks.get(&k) {
                    let link = t.yt.as_ref().map(|id| format!("https://music.youtube.com/watch?v={id}")).or(t.url.clone()).unwrap_or_default();
                    if self.win.copy_text(&link) {
                        self.say(tr("Copied"), cx);
                    }
                }
            }
            MenuAct::Rename(id) => {
                self.modal_text = self.lib.playlist(&id).map(|p| p.name.clone()).unwrap_or_default();
                self.modal = Some(Modal::Rename(id));
                self.edit_for = Some("modal");
                self.edit_shown = None;
            }
            MenuAct::DeletePlaylist(id) => {
                self.lib.playlists.retain(|p| p.id != id);
                self.lib.dirty = true;
                if self.view == View::Playlist(id) {
                    self.go(View::Home, cx);
                }
            }
            MenuAct::DownloadPlaylist(id) => {
                let keys = self.lib.playlist(&id).map(|p| p.tracks.clone()).unwrap_or_default();
                for k in keys {
                    self.download(&k, cx);
                }
            }
            MenuAct::Sleep(min) => {
                self.sleep_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(min as u64 * 60));
                self.sleep_end_of_song = false;
                self.say(fill(tr("Stop in {n} min"), &[&min.to_string()]), cx);
            }
            MenuAct::SleepEnd => {
                self.sleep_end_of_song = true;
                self.sleep_at = None;
                self.say(tr("End of song"), cx);
            }
            MenuAct::SleepOff => {
                self.sleep_at = None;
                self.sleep_end_of_song = false;
            }
            MenuAct::Header => {}
        }
        cx.repaint();
    }
}

trait Red {
    fn red(&self) -> Color;
}
impl Red for super::Pal {
    fn red(&self) -> Color {
        Color::hex(0xff453a)
    }
}
