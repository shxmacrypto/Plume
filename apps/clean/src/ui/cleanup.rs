//! The Cleanup page: one card per suggestion (what it is, its size, what blocks it), the files of a list to pick
//! one by one, and the bar that starts the cleanup.

use super::{App, Dups, Hit, ROW_H, blocked_text, paint::ic};
use crate::{
    clean::{Blocked, Kind, human},
    i18n::{ago, count, fill, tr},
};
use plume_ui::{Align, Canvas, Color, Font, Rect, icons, icons::Icon, rect};
use std::sync::atomic::Ordering;

const CARD_H: f32 = 96.0;

fn icon_of(k: Kind) -> &'static Icon {
    match k {
        Kind::Temp | Kind::SystemTemp => &ic::CLOCK,
        Kind::RecycleBin => &icons::TRASH,
        Kind::Browser(_) => &ic::GLOBE,
        Kind::Thumbnails => &ic::IMAGE,
        Kind::CrashDumps | Kind::SystemDumps => &ic::BUG,
        Kind::UpdateLeftovers => &ic::BOX,
        Kind::OldDownloads => &ic::DOWN,
        Kind::Large => &ic::BIG,
        Kind::Duplicates => &ic::DUP,
        Kind::EmptyFolders => &ic::EMPTY,
    }
}

impl App {
    pub fn paint_cleanup(&mut self, c: &mut Canvas, body: Rect) {
        if let Some(i) = self.review
            && i < self.sugg.len()
        {
            self.paint_review(c, body, i);
            return;
        }
        let p = self.pal.clone();
        let x0 = body.x + 32.0;
        let w = (body.w - 64.0).min(980.0);
        let total: u64 = self.sugg.iter().filter(|s| s.blocked.is_none() && !s.kind.review() && s.kind != Kind::RecycleBin).map(|s| s.size).sum();
        let sub = if self.measuring && self.sugg.is_empty() {
            tr("Measuring…").to_string()
        } else {
            fill(tr("{size} can be freed safely. Nothing is ticked until you choose."), &[&human(total)])
        };
        let mut y = self.heading(c, x0, body.y + 28.0, w, tr("Free up space"), &sub);
        let label = if self.measuring { tr("Measuring…") } else { tr("Refresh") };
        self.pill_right(c, x0 + w, body.y + 30.0, label, false, Hit::Refresh);
        // the last cleanup
        if let Some(o) = self.result.clone() {
            let r = rect(x0, y, w, 52.0);
            c.fill_round(r, 14.0, p.accent.fade(0.14));
            c.icon(&icons::CHECK, rect(r.x + 18.0, r.y + 18.0, 16.0, 16.0), p.accent);
            let mut s = fill(tr("{size} freed."), &[&human(o.freed)]);
            if self.set.recycle && self.places.sandbox.is_none() {
                s.push(' ');
                s.push_str(tr("Empty the Recycle Bin to get the space back."));
            }
            if o.failed > 0 {
                s.push(' ');
                s.push_str(&fill(tr("{n} in use, left in place."), &[&count(o.failed)]));
            }
            if o.refused > 0 {
                s.push(' ');
                s.push_str(&fill(tr("{n} not touched: Plume Clean never removes them there."), &[&count(o.refused)]));
            }
            c.text(&s, rect(r.x + 46.0, r.y, r.w - 140.0, r.h), Font::Body, p.t1, Align::Left);
            self.pill_right(c, r.right() - 12.0, r.y + 10.0, tr("OK"), false, Hit::DismissResult);
            y += 64.0;
        }
        // administrator rights
        let admin_waiting = self.sugg.iter().any(|s| s.blocked == Some(Blocked::Admin));
        if admin_waiting {
            let r = rect(x0, y, w, 52.0);
            c.fill_round(r, 14.0, p.warn.fade(0.12));
            c.icon(&ic::SHIELD, rect(r.x + 18.0, r.y + 18.0, 16.0, 16.0), p.warn);
            c.text(tr("Some of Windows' own files need administrator rights."), rect(r.x + 46.0, r.y, r.w - 260.0, r.h), Font::Body, p.t1, Align::Left);
            self.pill_right(c, r.right() - 12.0, r.y + 10.0, tr("Restart as administrator"), false, Hit::Elevate);
            y += 64.0;
        }
        // the cards
        let bar_h = 72.0;
        let list = rect(x0 - 8.0, y, w + 16.0, body.bottom() - y - bar_h);
        self.clean_rect = list;
        let content = self.sugg.len() as f32 * (CARD_H + 10.0);
        self.clean_scroll = self.clean_scroll.min((content - list.h + 10.0).max(0.0));
        c.clip(list);
        let n = self.sugg.len();
        for i in 0..n {
            let cy = list.y + i as f32 * (CARD_H + 10.0) - self.clean_scroll;
            if cy > list.bottom() || cy + CARD_H < list.y {
                continue;
            }
            self.paint_card(c, rect(x0, cy, w, CARD_H), i, list);
        }
        c.unclip();
        if self.sugg.is_empty() && self.measuring {
            c.text(tr("Looking at temporary files, caches and downloads…"), list, Font::Body, p.t3, Align::Center);
        }
        // the bar
        let bar = rect(0.0, body.bottom() - bar_h, body.x + body.w, bar_h);
        c.fill(bar, p.bar);
        c.hline(0.0, bar.y, bar.w, p.line);
        let sel = self.selected_total();
        let bin = self.sugg.iter().any(|s| s.selected && s.kind == Kind::RecycleBin && s.blocked.is_none());
        let what = if sel == 0 && !bin { tr("Tick what you want to clean.").to_string() } else { fill(tr("Selected: {size}"), &[&human(sel)]) };
        c.text(&what, rect(x0, bar.y, w - 220.0, bar.h), Font::Title, if sel > 0 || bin { p.t1 } else { p.t3 }, Align::Left);
        let label = if self.cleaning { tr("Cleaning…") } else { tr("Clean selected") };
        let ready = (sel > 0 || bin) && !self.cleaning;
        if ready {
            self.pill_right(c, x0 + w, bar.y + 20.0, label, true, Hit::Clean);
        } else {
            let tw = c.text_width(label, Font::BodyBold) + 32.0;
            let r = rect(x0 + w - tw, bar.y + 20.0, tw, 32.0);
            c.fill_round(r, 16.0, p.t1.fade(0.06));
            c.text(label, r, Font::BodyBold, p.t3, Align::Center);
        }
    }

    fn paint_card(&mut self, c: &mut Canvas, r: Rect, i: usize, clip: Rect) {
        let p = self.pal.clone();
        let s = self.sugg[i].clone();
        let usable = s.blocked.is_none() && s.count > 0;
        c.fill_round(r, 16.0, p.card);
        if s.selected && usable {
            c.stroke_round(r, 16.0, p.accent.fade(0.7), 1.5);
        } else {
            c.stroke_round(r, 16.0, p.line, 1.0);
        }
        let tick = rect(r.x + 18.0, r.y + 20.0, 22.0, 22.0);
        self.check(c, tick, s.selected && usable, usable);
        if usable {
            self.hits.add(rect(r.x, r.y, 56.0, r.h).intersect(&clip), Hit::Tick(i));
        }
        c.fill_round(rect(r.x + 54.0, r.y + 16.0, 30.0, 30.0), 9.0, p.accent.fade(if usable { 0.16 } else { 0.07 }));
        c.icon(icon_of(s.kind), rect(r.x + 61.0, r.y + 23.0, 16.0, 16.0), if usable { p.accent } else { p.t3 });
        let tx = r.x + 98.0;
        let right_w = 220.0;
        c.text(
            tr(s.kind.title()),
            rect(tx, r.y + 14.0, r.w - (tx - r.x) - right_w, 22.0),
            Font::Title,
            if usable || s.blocked.is_some() { p.t1 } else { p.t2 },
            Align::Left,
        );
        // the explanation, on up to two lines
        let ex = rect(tx, r.y + 38.0, r.w - (tx - r.x) - right_w - 10.0, 34.0);
        c.clip(ex.intersect(&clip));
        c.text_block(tr(s.kind.explain()), ex, Font::Small, p.t3);
        c.unclip();
        // size, or why not
        let size = if s.kind == Kind::Duplicates && self.dups != Dups::Done {
            String::new()
        } else if s.count == 0 && s.blocked != Some(Blocked::NeedScan) {
            tr("Nothing to clean").to_string()
        } else if s.blocked == Some(Blocked::NeedScan) {
            String::new()
        } else {
            human(if s.kind.review() && s.items.iter().any(|x| x.selected) { s.selected_size() } else { s.size })
        };
        let big = s.count > 0 && !size.is_empty();
        c.text(
            &size,
            rect(r.right() - right_w, r.y + 12.0, right_w - 20.0, 26.0),
            if big { Font::Large } else { Font::Body },
            if big { p.t1 } else { p.t3 },
            Align::Right,
        );
        let mut tags_y = r.y + 44.0;
        if s.count > 0 && s.kind != Kind::Duplicates || s.kind == Kind::Duplicates && self.dups == Dups::Done {
            let line = if s.kind.review() {
                let picked = s.items.iter().filter(|x| x.selected).count() as u64;
                fill(tr("{a} of {b} ticked"), &[&count(picked), &count(s.count)])
            } else {
                crate::i18n::items(s.count)
            };
            c.text(&line, rect(r.right() - right_w, tags_y - 4.0, right_w - 20.0, 18.0), Font::Small, p.t3, Align::Right);
            tags_y += 18.0;
        }
        // what to do about it
        let ay = r.bottom() - 40.0;
        let right = r.right() - 18.0;
        if let Some(b) = &s.blocked {
            let text = blocked_text(b);
            let tw = c.text_width(&text, Font::SmallBold) + 18.0;
            let color = if *b == Blocked::NeedScan { p.t2 } else { p.warn };
            self.tag(c, right - tw, ay + 6.0, &text, color);
            if *b == Blocked::NeedScan {
                self.pill_right(c, right - tw - 10.0, ay + 1.0, tr("Scan"), false, Hit::GoScan);
            }
        } else if s.kind == Kind::Duplicates && self.dups != Dups::Done {
            if self.dups == Dups::Running {
                let read = self.dup_progress.as_ref().map(|p| p.read.load(Ordering::Relaxed)).unwrap_or(0);
                let l = self.pill_right(c, right, ay, tr("Stop"), false, Hit::CancelDupes);
                c.text(&fill(tr("Comparing files… {size} read"), &[&human(read)]), rect(l - 300.0, ay, 290.0, 32.0), Font::Small, p.t2, Align::Right);
            } else {
                self.pill_right(c, right, ay, tr("Find duplicates"), false, Hit::FindDupes);
            }
        } else if s.kind.review() && s.count > 0 {
            self.pill_right(c, right, ay, tr("Review"), false, Hit::Review(i));
        }
        let _ = tags_y;
    }

    fn paint_review(&mut self, c: &mut Canvas, body: Rect, i: usize) {
        let p = self.pal.clone();
        let s = self.sugg[i].clone();
        let x0 = body.x + 32.0;
        let w = (body.w - 64.0).min(1100.0);
        let picked: Vec<&crate::clean::Item> = s.items.iter().filter(|x| x.selected).collect();
        let sub = fill(tr("{a} of {b} ticked, {size}"), &[&count(picked.len() as u64), &count(s.count), &human(picked.iter().map(|x| x.size).sum())]);
        let y = self.heading(c, x0, body.y + 28.0, w - 200.0, tr(s.kind.title()), &sub);
        self.pill_right(c, x0 + w, body.y + 30.0, tr("Done"), true, Hit::ReviewDone);
        let mut x = x0;
        x += self.pill(c, x, y, tr("Tick all"), false, Hit::ReviewAll(true)) + 8.0;
        x += self.pill(c, x, y, tr("Untick all"), false, Hit::ReviewAll(false)) + 8.0;
        if s.kind == Kind::Duplicates {
            self.pill(c, x, y, tr("Tick all copies but the oldest"), false, Hit::KeepOldest);
        }
        let list = rect(x0 - 8.0, y + 46.0, w + 16.0, body.bottom() - y - 66.0);
        c.fill_round(list, 16.0, p.card);
        let rows = list.inset_xy(6.0, 6.0);
        self.review_rect = rows;
        let rh = ROW_H + 10.0;
        self.review_scroll = self.review_scroll.min((s.items.len() as f32 * rh - rows.h).max(0.0));
        let first = (self.review_scroll / rh).floor() as usize;
        let now = crate::clean::now_filetime();
        c.clip(rows);
        for k in first..s.items.len() {
            let ry = rows.y + k as f32 * rh - self.review_scroll;
            if ry > rows.bottom() {
                break;
            }
            let it = &s.items[k];
            let row = rect(rows.x, ry, rows.w, rh - 2.0);
            let hit = Hit::ReviewRow(k);
            if s.kind == Kind::Duplicates && it.group % 2 == 1 {
                c.fill_round(row, 8.0, p.t1.fade(0.03));
            }
            if self.hovered(&hit) {
                c.fill_round(row, 8.0, p.t1.fade(0.06));
            }
            // the first of each group of duplicates: a thin line above
            if s.kind == Kind::Duplicates && k > 0 && s.items[k - 1].group != it.group {
                c.hline(row.x + 12.0, row.y - 1.0, row.w - 24.0, p.line);
            }
            self.check(c, rect(row.x + 14.0, row.y + 11.0, 20.0, 20.0), it.selected, true);
            c.icon(if it.dir { &icons::FOLDER } else { &icons::FILE }, rect(row.x + 46.0, row.y + 13.0, 16.0, 16.0), p.t2);
            let name = it.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let folder = it.path.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
            let tw = row.w - 74.0 - 240.0;
            c.text(&name, rect(row.x + 72.0, row.y + 3.0, tw, 20.0), Font::Body, p.t1, Align::Left);
            c.text(&folder, rect(row.x + 72.0, row.y + 21.0, tw, 18.0), Font::Small, p.t3, Align::Left);
            c.text(&ago(it.mtime, now), rect(row.right() - 236.0, row.y, 120.0, row.h), Font::Small, p.t3, Align::Right);
            let size = if it.dir && it.size == 0 { tr("empty").to_string() } else { human(it.size) };
            c.text(&size, rect(row.right() - 108.0, row.y, 96.0, row.h), Font::BodyBold, p.t2, Align::Right);
            self.hits.add(row.intersect(&rows), hit);
        }
        c.unclip();
        if s.items.is_empty() {
            c.text(tr("Nothing here."), rows, Font::Body, p.t3, Align::Center);
        }
        let _ = Color::TRANSPARENT;
    }
}
