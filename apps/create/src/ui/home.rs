//! The start page: open a photo to edit, or start a design from a size or a template; the files opened lately.

use super::{Act, App, Dialog, Hit, draw, icons};
use crate::{
    engine::{
        doc::TextL,
        img::IRect,
        templates::{PRESETS, TEMPLATES, template},
    },
    i18n::tr,
};
use plume_ui::{Align, Canvas, Font, PaintCx, Rect, icons as ui_icons, m, rect};

impl App {
    pub fn paint_home(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let p = self.pal.clone();
        let w = (cx.w - 96.0).clamp(600.0, 1120.0);
        let x = (cx.w - w) / 2.0;
        let mut y = m::TITLE_H + 28.0;
        c.text(tr("What will you create?"), rect(x, y, w, 34.0), Font::Hero, p.t1, Align::Left);
        y += 38.0;
        c.text(
            tr("Edit a photo, or start a design from a size or a template. Everything stays on this PC."),
            rect(x, y, w, 22.0),
            Font::Body,
            p.t3,
            Align::Left,
        );
        y += 40.0;
        // two cards side by side
        let left_w = (w * 0.3).max(260.0);
        let card_h = 300.0;
        let photo = rect(x, y, left_w, card_h);
        self.card(c, photo);
        let ix = photo.x + 24.0;
        c.fill_round(rect(ix, photo.y + 24.0, 48.0, 48.0), 14.0, p.accent.fade(0.16));
        c.icon(&icons::IMAGE, rect(ix + 12.0, photo.y + 36.0, 24.0, 24.0), p.ink);
        c.text(tr("Edit a photo"), rect(ix, photo.y + 88.0, photo.w - 48.0, 24.0), Font::Large, p.t1, Align::Left);
        let blurb = tr("Crop, straighten, adjust the light and colours, add a look, text or shapes, remove the background.");
        c.text_block(blurb, rect(ix, photo.y + 118.0, photo.w - 48.0, 0.0), Font::Body, p.t3);
        self.pill_icon(
            c,
            rect(ix, photo.bottom() - 24.0 - 36.0 - 44.0, photo.w - 48.0, 36.0),
            &ui_icons::FOLDER,
            tr("Open a photo…"),
            true,
            Hit::Act(Act::Open),
        );
        c.text(
            tr("PNG, JPEG, WebP, HEIC, GIF, BMP · or a .plumecreate project"),
            rect(ix, photo.bottom() - 24.0 - 30.0, photo.w - 48.0, 30.0),
            Font::Small,
            p.t3,
            Align::Center,
        );
        // sizes
        let design = rect(photo.right() + 14.0, y, w - left_w - 14.0, card_h);
        self.card(c, design);
        c.text(tr("Start a design"), rect(design.x + 24.0, design.y + 20.0, design.w - 48.0, 24.0), Font::Large, p.t1, Align::Left);
        let cols = 4;
        let gx = design.x + 24.0;
        let gw = design.w - 48.0;
        let tw = (gw - (cols - 1) as f32 * 10.0) / cols as f32;
        let th = 104.0;
        for i in 0..PRESETS.len() + 1 {
            let r = rect(gx + (i % cols) as f32 * (tw + 10.0), design.y + 60.0 + (i / cols) as f32 * (th + 10.0), tw, th);
            let hit = if i < PRESETS.len() { Hit::Preset(i) } else { Hit::Act(Act::StartCustom) };
            let hot = self.hovered(&hit);
            c.fill_round(r, 12.0, if hot { p.fill2 } else { p.fill });
            let (name, size, pw, ph) = match PRESETS.get(i) {
                Some(pr) => (tr(pr.name).to_string(), format!("{} × {}", pr.w, pr.h), pr.w as f32, pr.h as f32),
                None => (tr("Custom size").to_string(), tr("Any width and height").to_string(), 4.0, 3.0),
            };
            // the shape of the canvas
            let k = (38.0 / pw).min(38.0 / ph);
            let sh = rect(r.x + 14.0, r.y + 12.0 + (38.0 - ph * k) / 2.0, (pw * k).max(4.0), (ph * k).max(4.0));
            c.stroke_round(sh, 3.0, if hot { p.ink } else { p.t3 }, 1.5);
            if i == PRESETS.len() {
                c.icon(&ui_icons::PLUS, sh.centered(14.0, 14.0), p.t2);
            }
            c.text(&name, rect(r.x + 14.0, r.y + 56.0, r.w - 20.0, 20.0), Font::BodyBold, p.t1, Align::Left);
            c.text(&size, rect(r.x + 14.0, r.y + 76.0, r.w - 20.0, 18.0), Font::Small, p.t3, Align::Left);
            self.hits.add(r, hit);
        }
        y += card_h + 28.0;
        // templates
        c.text(tr("Templates"), rect(x, y, w, 24.0), Font::Large, p.t1, Align::Left);
        y += 34.0;
        self.ensure_template_previews(c);
        let n = TEMPLATES.len();
        let gap = 14.0;
        // all the templates in one row: their height follows the room there is
        let ratios: f32 = TEMPLATES.iter().map(|t| (t.w as f32 / t.h as f32).max(0.56)).sum();
        let tile_h = 168.0f32.min(cx.h - y - 120.0).min((w - gap * (n - 1) as f32) / ratios).max(80.0);
        let mut tx = x;
        for (i, t) in TEMPLATES.iter().enumerate() {
            let ar = t.w as f32 / t.h as f32;
            let tw = tile_h * ar.max(0.56);
            let r = rect(tx, y, tw, tile_h);
            if r.right() > x + w {
                break;
            }
            let hit = Hit::Template(i);
            let hot = self.hovered(&hit);
            c.fill_round(r, 12.0, p.fill);
            if let Some(Some(b)) = self.tpl_bitmaps.get(i).cloned() {
                let size = unsafe { b.GetSize() };
                let k = (r.w / size.width).min(r.h / size.height);
                let (iw, ih) = (size.width * k, size.height * k);
                c.clip(r);
                draw::bitmap_at(c, &b, rect(r.x + (r.w - iw) / 2.0, r.y + (r.h - ih) / 2.0, iw, ih), 1.0, true, None);
                c.unclip();
            }
            c.stroke_round(r, 12.0, if hot { p.accent } else { p.line }, if hot { 2.0 } else { 1.0 });
            c.text(tr(t.name), rect(r.x, r.bottom() + 6.0, r.w, 20.0), Font::Body, if hot { p.ink } else { p.t2 }, Align::Center);
            self.hits.add(rect(r.x, r.y, r.w, r.h + 26.0), hit);
            tx += tw + gap;
        }
        y += tile_h + 44.0;
        // lately opened
        let recent: Vec<_> = self.set.recent.iter().filter(|p| p.exists()).cloned().collect();
        if !recent.is_empty() && y + 70.0 < cx.h {
            c.text(tr("Recent"), rect(x, y, w, 24.0), Font::Large, p.t1, Align::Left);
            y += 32.0;
            let mut rx = x;
            for (i, path) in recent.iter().enumerate() {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let bw = (c.text_width(&name, Font::Body) + 44.0).min(260.0);
                if rx + bw > x + w {
                    break;
                }
                let r = rect(rx, y, bw, 34.0);
                let hit = Hit::Recent(i);
                c.fill_round(r, 17.0, if self.hovered(&hit) { p.fill2 } else { p.fill });
                let ic = if crate::wic::ext(path) == crate::engine::project::EXT { &icons::LAYERS } else { &icons::IMAGE };
                c.icon(ic, rect(r.x + 12.0, r.y + 9.0, 16.0, 16.0), p.t2);
                c.text(&name, rect(r.x + 34.0, r.y, r.w - 44.0, r.h), Font::Body, p.t1, Align::Left);
                self.hits.add(r, hit);
                rx += bw + 8.0;
            }
        }
    }

    pub fn card(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        c.fill_round(r, 18.0, p.card);
        c.stroke_round(r, 18.0, p.line, 1.0);
    }

    fn ensure_template_previews(&mut self, c: &mut Canvas) {
        if self.tpl_bitmaps.len() == TEMPLATES.len() {
            return;
        }
        let rt = c.target();
        let r = &self.renderer;
        let measure = |t: &TextL, w: f32| r.measure(t, w);
        let fr = crate::i18n::french();
        let h = 180.0 * c.scale();
        self.tpl_bitmaps = TEMPLATES
            .iter()
            .map(|t| {
                let d = template(t.key, fr, &measure)?;
                let s = h / d.h as f32;
                let region = IRect::new(0, 0, (d.w as f32 * s).ceil() as i32, (d.h as f32 * s).ceil() as i32);
                let b = self.renderer.render(&d, s, region);
                draw::bitmap(&rt, region.w as u32, region.h as u32, &b.to_bgra())
            })
            .collect();
    }

    /// A click on the start page.
    pub fn home_click(&mut self, hit: &Hit, cx: &mut plume_ui::Cx) -> bool {
        match hit {
            Hit::Preset(i) => {
                let pr = PRESETS[*i];
                self.new_design(pr.w, pr.h, tr(pr.name));
            }
            Hit::Act(Act::StartCustom) if self.dialog.is_none() => {
                self.dialog = Some(Dialog::Custom { w: self.set.custom_w, h: self.set.custom_h });
            }
            Hit::Template(i) => {
                let key = TEMPLATES[*i].key;
                self.start_template(key);
            }
            Hit::Recent(i) => {
                let recent: Vec<_> = self.set.recent.iter().filter(|p| p.exists()).cloned().collect();
                if let Some(p) = recent.get(*i).cloned() {
                    self.open_path(&p, cx);
                }
            }
            _ => return false,
        }
        cx.repaint();
        true
    }
}
