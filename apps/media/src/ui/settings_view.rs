//! Settings: appearance, library folders, photos, "Open with", thumbnails, shortcuts, about.

use super::{App, Hit, SIDEBAR_W, draw};
use crate::{
    i18n::{fill, tr},
    library,
    settings::SPEEDS,
};
use plume_ui::{Align, Canvas, Font, PaintCx, Rect, rect};

const PAD: f32 = 28.0;
const TOP: f32 = 100.0;

impl App {
    /// A row of choices, one of them on.
    fn segmented(&mut self, c: &mut Canvas, x: f32, y: f32, opts: &[(String, Hit, bool)]) {
        let p = self.pal.clone();
        let widths: Vec<f32> = opts.iter().map(|(l, _, _)| c.text_width(l, Font::BodyBold) + 28.0).collect();
        let total: f32 = widths.iter().sum::<f32>() + 8.0;
        c.fill_round(rect(x, y, total, 36.0), 18.0, p.t1.fade(0.06));
        let mut cx = x + 4.0;
        for ((label, hit, on), w) in opts.iter().zip(widths) {
            let r = rect(cx, y + 4.0, w, 28.0);
            if *on {
                c.fill_round(r, 14.0, p.accent);
            } else if self.hovered(hit) {
                c.fill_round(r, 14.0, p.t1.fade(0.08));
            }
            c.text(label, r, Font::BodyBold, if *on { p.on_accent } else { p.t2 }, Align::Center);
            self.hits.add(r, hit.clone());
            cx += w;
        }
    }

    fn section(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, title: &str) {
        let p = self.pal.clone();
        c.fill_round(rect(x, y, w, h), 14.0, p.card);
        c.stroke_round(rect(x, y, w, h), 14.0, p.line, 1.0);
        c.text(title, rect(x + 20.0, y + 14.0, w - 40.0, 22.0), Font::Title, p.t1, Align::Left);
    }

    pub fn paint_settings(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let p = self.pal.clone();
        let x0 = SIDEBAR_W + PAD;
        let w = (cx.w - x0 - PAD - 6.0).clamp(300.0, 760.0);
        draw::text(c, tr("Settings"), rect(x0, 38.0, w, 34.0), 26.0, 700, p.t1, Align::Left);
        self.view_h = cx.h - TOP;
        self.scroll_to = self.scroll_to.clamp(0.0, self.max_scroll());
        let d = self.scroll_to - self.scroll;
        self.scroll = if d.abs() < 0.5 { self.scroll_to } else { self.scroll + d * 0.3 };
        let area = rect(SIDEBAR_W, TOP, cx.w - SIDEBAR_W, self.view_h);
        c.clip(area);
        let mark = self.hits.iter().count();
        let mut y = TOP + 4.0 - self.scroll;
        let start = y;
        let ix = x0 + 20.0;
        let lab = |c: &mut Canvas, t: &str, y: f32| c.text(t, rect(ix, y, w - 40.0, 20.0), Font::Small, p.t3, Align::Left);

        // appearance
        let wide = w > 640.0;
        let ah = if wide { 116.0 } else { 160.0 };
        self.section(c, x0, y, w, ah, tr("Appearance"));
        lab(c, tr("Theme"), y + 46.0);
        let th = self.set.theme.clone();
        let opts: Vec<(String, Hit, bool)> = [("system", tr("System")), ("dark", tr("Dark")), ("light", tr("Light"))]
            .into_iter()
            .map(|(k, l)| (l.to_string(), Hit::Theme(k), th == k))
            .collect();
        self.segmented(c, ix, y + 66.0, &opts);
        let lang = self.set.lang.clone();
        let lx = ix + 300.0;
        if wide {
            c.text(tr("Language"), rect(lx, y + 46.0, 200.0, 20.0), Font::Small, p.t3, Align::Left);
        }
        let opts: Vec<(String, Hit, bool)> =
            [("system", tr("System")), ("en", "English"), ("fr", "Français")].into_iter().map(|(k, l)| (l.to_string(), Hit::Lang(k), lang == k)).collect();
        if wide {
            self.segmented(c, lx, y + 66.0, &opts);
        } else {
            c.text(tr("Language"), rect(ix, y + 106.0, 200.0, 20.0), Font::Small, p.t3, Align::Left);
            self.segmented(c, ix, y + 124.0, &opts);
        }
        y += ah + 16.0;

        // library folders
        let folders = self.set.folders.clone();
        let fh = 136.0 + folders.len().max(1) as f32 * 34.0;
        self.section(c, x0, y, w, fh, tr("Library folders"));
        lab(c, tr("Their subfolders are included."), y + 40.0);
        let mut fy = y + 68.0;
        if folders.is_empty() {
            c.text(tr("No library folder yet"), rect(ix, fy, w - 40.0, 30.0), Font::Body, p.t2, Align::Left);
            fy += 34.0;
        }
        for (i, f) in folders.iter().enumerate() {
            c.icon(&plume_ui::icons::FOLDER, rect(ix, fy + 7.0, 16.0, 16.0), p.t2);
            c.text(&f.display().to_string(), rect(ix + 26.0, fy, w - 160.0, 30.0), Font::Body, p.t1, Align::Left);
            let rr = rect(x0 + w - 110.0, fy + 2.0, 90.0, 26.0);
            let hit = Hit::RemoveFolder(i);
            if self.hovered(&hit) {
                c.fill_round(rr, 13.0, p.t1.fade(0.08));
            }
            c.text(tr("Remove"), rr, Font::SmallBold, p.t2, Align::Center);
            self.hits.add(rr, hit);
            fy += 34.0;
        }
        fy += 10.0;
        let aw = c.text_width(tr("Add a folder"), Font::BodyBold) + 36.0;
        self.pill(c, rect(ix, fy, aw, 34.0), tr("Add a folder"), true, Hit::AddFolder);
        let rw = c.text_width(tr("Rescan"), Font::BodyBold) + 36.0;
        self.pill(c, rect(ix + aw + 10.0, fy, rw, 34.0), tr("Rescan"), false, Hit::Rescan);
        y += fh + 16.0;

        // photos
        self.section(c, x0, y, w, 300.0, tr("Photos"));
        lab(c, tr("Opening zoom"), y + 46.0);
        let z = self.set.zoom.clone();
        let opts: Vec<(String, Hit, bool)> =
            [("fit", tr("Fit")), ("fill", tr("Fill")), ("actual", tr("Actual size"))].into_iter().map(|(k, l)| (l.to_string(), Hit::Zoom(k), z == k)).collect();
        self.segmented(c, ix, y + 66.0, &opts);
        draw::text_wrap(
            c,
            tr("Fit: whole picture, small ones are not enlarged. Fill: whole picture, as large as the window."),
            rect(ix, y + 108.0, w - 40.0, 40.0),
            12.0,
            400,
            p.t3,
            Align::Left,
        );
        lab(c, tr("Slideshow speed"), y + 146.0);
        let sp = self.set.slideshow;
        let opts: Vec<(String, Hit, bool)> = SPEEDS.iter().map(|&s| (format!("{s} s"), Hit::SlideSpeed(s), sp == s)).collect();
        self.segmented(c, ix, y + 166.0, &opts);
        lab(c, tr("Thumbnail size"), y + 216.0);
        let t = self.set.tiles;
        let opts: Vec<(String, Hit, bool)> =
            [(0u8, tr("Small")), (1, tr("Medium")), (2, tr("Large"))].into_iter().map(|(k, l)| (l.to_string(), Hit::Tiles(k), t == k)).collect();
        self.segmented(c, ix, y + 236.0, &opts);
        y += 316.0;

        // open with
        self.section(c, x0, y, w, 120.0, tr("Open with"));
        draw::text_wrap(
            c,
            tr("Offer Plume Media in Windows' \"Open with\" menu for photos and videos."),
            rect(ix, y + 44.0, w - 40.0, 40.0),
            13.0,
            400,
            p.t2,
            Align::Left,
        );
        let label = if self.openwith { tr("Remove from \"Open with\"") } else { tr("Add to \"Open with\"") };
        let ow = c.text_width(label, Font::BodyBold) + 36.0;
        self.pill(c, rect(ix, y + 74.0, ow, 34.0), label, !self.openwith, Hit::OpenWith);
        y += 136.0;

        // thumbnails
        self.section(c, x0, y, w, 96.0, tr("Thumbnails"));
        let size = self.cache_bytes.map(library::bytes).unwrap_or_else(|| "…".into());
        c.text(&fill(tr("{n} on disk"), &[&size]), rect(ix, y + 50.0, w - 200.0, 30.0), Font::Body, p.t2, Align::Left);
        let cw = c.text_width(tr("Clear"), Font::BodyBold) + 36.0;
        self.pill(c, rect(x0 + w - cw - 20.0, y + 48.0, cw, 34.0), tr("Clear"), false, Hit::ClearCache);
        y += 112.0;

        // shortcuts
        let s1 = tr("Photos: ← → previous / next, + − zoom, 0 fit, 1 actual size, R rotate, I info, F full screen, S slideshow, Ctrl+D favorite.");
        let s2 = tr("Videos: Space play / pause, ← → 5 s, ↑ ↓ volume, M mute, F full screen, C subtitles, [ ] speed, Page Up / Down previous / next.");
        let h1 = draw::wrap_height(c, s1, w - 40.0, 13.0, 400);
        let h2 = draw::wrap_height(c, s2, w - 40.0, 13.0, 400);
        let sh = 58.0 + h1 + h2 + 14.0;
        self.section(c, x0, y, w, sh, tr("Keyboard shortcuts"));
        draw::text_wrap(c, s1, rect(ix, y + 46.0, w - 40.0, h1), 13.0, 400, p.t2, Align::Left);
        draw::text_wrap(c, s2, rect(ix, y + 52.0 + h1, w - 40.0, h2), 13.0, 400, p.t2, Align::Left);
        y += sh + 16.0;

        // about
        self.section(c, x0, y, w, 96.0, tr("About"));
        c.text(&format!("Plume Media {}", plume_update::VERSION), rect(ix, y + 44.0, w - 40.0, 20.0), Font::Body, p.t2, Align::Left);
        c.text(tr("Nothing goes online: your photos and videos stay on this PC."), rect(ix, y + 64.0, w - 40.0, 20.0), Font::Small, p.t3, Align::Left);
        y += 112.0;

        c.unclip();
        self.content_h = y - start + 8.0;
        // what the sections drew outside the visible area must not be clickable
        let all: Vec<(Rect, Hit)> = self.hits.iter().cloned().collect();
        self.hits.clear();
        for (i, (r, h)) in all.into_iter().enumerate() {
            let r = if i >= mark { r.intersect(&area) } else { r };
            if !r.is_empty() {
                self.hits.add(r, h);
            }
        }
        self.paint_scrollbar(c, cx.w - 9.0, TOP, self.view_h);
    }
}
