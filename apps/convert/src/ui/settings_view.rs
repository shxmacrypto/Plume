//! The settings page.

use super::{App, Hit, Tools, draw};
use crate::{
    i18n::{fill, tr},
    tools,
};
use plume_ui::{Align, Canvas, Rect, rect};

impl App {
    fn group(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str) -> f32 {
        let p = self.pal.clone();
        draw::text(c, title, rect(x, y, w, 30.0), 17.0, 600, p.t1, Align::Left);
        y + 40.0
    }

    /// A row: label (and a line under it) on the left; returns the row's rect for the control on the right.
    fn row(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, label: &str, sub: &str) -> Rect {
        let p = self.pal.clone();
        let h = if sub.is_empty() { 50.0 } else { 62.0 };
        let r = rect(x, y, w, h);
        let lw = w * 0.5;
        if sub.is_empty() {
            draw::text(c, label, rect(x, y, lw, h), 13.0, 400, p.t1, Align::Left);
        } else {
            draw::text(c, label, rect(x, y + 9.0, lw, 22.0), 13.0, 400, p.t1, Align::Left);
            draw::text(c, sub, rect(x, y + 32.0, w * 0.62, 20.0), 12.0, 400, p.t3, Align::Left);
        }
        c.hline(x, y + h, w, p.line);
        r
    }

    /// A segmented control on the right of a row.
    fn row_chips(&mut self, c: &mut Canvas, r: Rect, key: &'static str) {
        let w = self.segmented_width(c, key);
        self.segmented(c, r.right() - w, r.y + (r.h - 34.0) / 2.0, None, key);
    }

    pub fn paint_settings(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        c.clip(page);
        let w = (page.w - 96.0).min(760.0);
        let x = page.x + (page.w - w) / 2.0;
        let top = page.y + 20.0 - self.left_scroll;
        let mut y = top;
        draw::text(c, tr("Settings"), rect(x, y, w, 44.0), 24.0, 700, p.t1, Align::Left);
        y += 64.0;

        y = self.group(c, x, y, w, tr("Appearance"));
        let r = self.row(c, x, y, w, tr("Theme"), "");
        self.row_chips(c, r, "theme");
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Language"), "");
        self.row_chips(c, r, "lang");
        y += r.h + 28.0;

        y = self.group(c, x, y, w, tr("Converting"));
        let dest = match &self.set.out_dir {
            Some(d) => d.display().to_string(),
            None => tr("Next to the original").to_string(),
        };
        let r = self.row(c, x, y, w, tr("Save results"), &dest);
        let l = tr("Choose a folder…");
        let bw = draw::text_width(c, l, 13.0, 600) + 32.0;
        let mut bx = r.right() - bw;
        self.pill(c, rect(bx, r.y + 13.0, bw, 36.0), l, false, Hit::OutFolder, true);
        if self.set.out_dir.is_some() {
            let l2 = tr("Next to the original");
            let bw2 = draw::text_width(c, l2, 13.0, 600) + 32.0;
            bx -= bw2 + 8.0;
            self.pill(c, rect(bx, r.y + 13.0, bw2, 36.0), l2, false, Hit::OutReset, true);
        }
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Keep metadata"), tr("Title, date, place and chapters of videos and sounds."));
        let on = self.set.keep_meta;
        self.switch(c, r, on, "keep_meta");
        y += r.h + 1.0;
        let sub = if !self.set.hw {
            tr("Off: the processor does everything.").to_string()
        } else if self.hw_checking {
            tr("Looking for the graphics card's encoder…").to_string()
        } else {
            match &self.hw_label {
                Some(l) if !l.is_empty() => fill(tr("Found: {name}. Falls back to the processor if it fails."), &[l]),
                Some(_) => tr("None found: the processor does it").to_string(),
                None => tr("Faster H.264, H.265 and AV1 with NVIDIA, Intel or AMD.").to_string(),
            }
        };
        let r = self.row(c, x, y, w, tr("Graphics card encoding"), &sub);
        let on = self.set.hw;
        self.switch(c, r, on, "hw");
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Explorer menu"), tr("Adds \"Convert with Plume\" when you right-click a file."));
        let on = self.menu_on;
        self.switch(c, r, on, "context_menu");
        y += r.h + 28.0;

        y = self.group(c, x, y, w, "FFmpeg");
        let (sub, show_btn) = match &self.tools {
            Tools::Ready => (fill(tr("Installed in {path}"), &[&tools::tools_dir().display().to_string()]), false),
            Tools::Missing => (tr("Downloaded the first time a video or a sound is converted (about 100 MB).").to_string(), true),
            Tools::Installing(f) => (fill(tr("Downloading FFmpeg… {n} %"), &[&((f * 100.0) as u32).to_string()]), false),
            Tools::Failed(e) => (crate::i18n::colon(tr("FFmpeg could not be downloaded"), e), true),
        };
        let r = self.row(c, x, y, w, tr("Video and sound engine"), &sub);
        if show_btn {
            let l = tr("Download now");
            let bw = draw::text_width(c, l, 13.0, 600) + 32.0;
            self.pill(c, rect(r.right() - bw, r.y + 13.0, bw, 36.0), l, true, Hit::Action("install"), true);
        }
        y += r.h + 18.0;
        let about = tr("Plume Convert works on this PC: your files are never uploaded. The only download is FFmpeg, from gyan.dev.");
        let h = draw::text_wrap(c, about, rect(x, y, w, 100.0), 12.0, 400, p.t3, Align::Left);
        y += h + 10.0;
        draw::text(c, &format!("Plume Convert {}", plume_update::VERSION), rect(x, y, w, 20.0), 12.0, 400, p.t3, Align::Left);
        y += 40.0;
        self.left_h = (y - top) - page.h;
        if self.left_h <= 0.0 {
            self.left_scroll = 0.0;
        }
        c.unclip();
    }
}
