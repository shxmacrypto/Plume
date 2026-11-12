//! The bars around the canvas: title bar, the current tool's options, the tool rail, the status bar, rulers;
//! and the toast.

use super::{Act, App, CropState, Ct, Hit, OPTS_H, PANEL_W, RAIL_W, RULER, STATUS_H, SelMode, Sg, Sl, Tg, Tool, draw, icons};
use crate::{
    engine::doc::ShapeKind,
    i18n::{fill, tr},
};
use plume_ui::{Align, Canvas, Color, Font, PaintCx, Rect, icons as ui_icons, m, rect};

impl App {
    pub fn paint_caption(&mut self, c: &mut Canvas, w: f32, maximized: bool) {
        let p = self.pal.clone();
        let bar = rect(0.0, 0.0, w, m::TITLE_H);
        self.hits.add(bar, Hit::Bar);
        // logo and name
        self.paint_logo(c, rect(16.0, (m::TITLE_H - 18.0) / 2.0, 18.0, 18.0));
        let ink = if self.active { p.t1 } else { p.t3 };
        c.text("Plume Create", rect(44.0, 0.0, 140.0, m::TITLE_H), Font::BodyBold, ink, Align::Left);
        let mut x = 44.0 + c.text_width("Plume Create", Font::BodyBold) + 10.0;
        if self.editing {
            let name = if self.dirty { format!("{} •", self.name) } else { self.name.clone() };
            c.text(&name, rect(x, 0.0, 260.0, m::TITLE_H), Font::Body, p.t3, Align::Left);
            x += 270.0;
            let _ = x;
        }
        let x0 = w - 3.0 * m::CAPTION_W;
        if self.editing {
            // export, menu, undo, redo
            let ex = rect(x0 - 12.0 - 104.0, 6.0, 104.0, 30.0);
            self.pill_icon(c, ex, &icons::EXPORT, tr("Export"), true, Hit::Act(Act::Export));
            let mut bx = ex.x - 8.0 - 32.0;
            self.icon_btn(c, &ui_icons::MORE, rect(bx, 5.0, 32.0, 32.0), Hit::AppMenu, false);
            bx -= 40.0;
            if self.hist.can_redo() {
                self.icon_btn(c, &icons::REDO, rect(bx, 5.0, 32.0, 32.0), Hit::Act(Act::Redo), false);
            } else {
                c.icon(&icons::REDO, rect(bx + 7.0, 12.0, 18.0, 18.0), p.t4);
            }
            bx -= 34.0;
            if self.hist.can_undo() {
                self.icon_btn(c, &icons::UNDO, rect(bx, 5.0, 32.0, 32.0), Hit::Act(Act::Undo), false);
            } else {
                c.icon(&icons::UNDO, rect(bx + 7.0, 12.0, 18.0, 18.0), p.t4);
            }
            bx -= 40.0;
            self.icon_btn(c, &ui_icons::HOME, rect(bx, 5.0, 32.0, 32.0), Hit::Act(Act::Home), false);
        } else {
            self.icon_btn(c, &ui_icons::MORE, rect(x0 - 12.0 - 32.0, 5.0, 32.0, 32.0), Hit::AppMenu, false);
        }
        let glyphs = [(Hit::Min, '\u{E921}'), (Hit::Max, if maximized { '\u{E923}' } else { '\u{E922}' }), (Hit::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(x0 + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, m::TITLE_H - 8.0);
            let hot = self.hover.as_ref() == Some(&id);
            if hot {
                c.fill(r, if id == Hit::Close { p.close_hover } else { p.hover });
            }
            let ink = if id == Hit::Close && hot {
                Color::hex(0xffffff)
            } else if self.active {
                p.t1
            } else {
                p.t3
            };
            c.glyph(glyph, r, ink);
            self.hits.add(r, id);
        }
    }

    fn paint_logo(&mut self, c: &mut Canvas, r: Rect) {
        if self.logo.is_none() {
            let px = crate::wic::decode_bytes(crate::LOGO, (r.w * c.scale()).ceil() as u32);
            let rt = c.target();
            self.logo = px.and_then(|px| draw::bitmap(&rt, px.w, px.h, &px.to_bgra_premul()));
        }
        if let Some(b) = self.logo.clone() {
            draw::bitmap_at(c, &b, r, 1.0, true, None);
        }
    }

    pub fn paint_rail(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let p = self.pal.clone();
        let r = rect(0.0, m::TITLE_H, RAIL_W, cx.h - m::TITLE_H);
        c.fill(r, p.side);
        c.vline(RAIL_W, r.y, r.h, p.line);
        self.hits.add(r, Hit::Rail);
        let mut y = r.y + 12.0;
        let gap = ((r.h - 12.0 - 70.0) / Tool::ALL.len() as f32).clamp(32.0, 44.0);
        for t in Tool::ALL {
            let b = rect(10.0, y, 40.0, gap - 4.0);
            self.icon_btn(c, t.icon(), b, Hit::Tool(t), self.tool == t);
            y += gap;
        }
        // the two colours
        let (fg, bg) = (self.colour, self.colour2);
        let y = (cx.h - 64.0).max(y + 4.0);
        let back = rect(24.0, y + 16.0, 26.0, 26.0);
        let f2 = bg.f();
        c.fill_round(back, 7.0, Color { r: f2[0], g: f2[1], b: f2[2], a: 1.0 });
        c.stroke_round(back, 7.0, p.t4, 1.0);
        self.hits.add(back, Hit::Act(Act::SwapColours));
        self.swatch(c, rect(10.0, y, 28.0, 28.0), fg, Ct::Brush);
    }

    pub fn paint_options(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let p = self.pal.clone();
        let r = rect(RAIL_W, m::TITLE_H, cx.w - RAIL_W - PANEL_W, OPTS_H);
        c.fill(r, p.bg);
        c.hline(r.x, r.bottom() - 1.0, r.w, p.line);
        self.hits.add(r, Hit::Bar);
        let mut x = r.x + 16.0;
        let y = r.y + 8.0;
        let title = format!("{} ({})", tr(self.tool.label()), self.tool.key());
        c.text(&title, rect(x, r.y, 200.0, r.h), Font::BodyBold, p.t1, Align::Left);
        x += c.text_width(&title, Font::BodyBold).min(200.0) + 20.0;
        let room = r.right() - x - 16.0;
        let small = |c: &mut Canvas, x: f32, label: &str| -> f32 {
            c.text(label, rect(x, r.y, 120.0, r.h), Font::Body, p.t2, Align::Left);
            c.text_width(label, Font::Body) + 8.0
        };
        match self.tool {
            Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Cutout => {
                if self.tool == Tool::Clone {
                    self.segmented(c, rect(x, y, 160.0, 32.0), &[tr("Clone"), tr("Heal")], self.heal as usize, Sg::CloneMode);
                    x += 172.0;
                }
                if self.tool == Tool::Cutout {
                    self.pill_icon(c, rect(x, y, 196.0, 32.0), &icons::WAND, tr("Remove background"), true, Hit::Act(Act::RemoveBg));
                    x += 206.0;
                    self.segmented(c, rect(x, y, 170.0, 32.0), &[tr("Erase"), tr("Restore")], self.cut_restore as usize, Sg::CutMode);
                    x += 182.0;
                }
                let sw = ((room - (x - r.x - 16.0)) / 3.0).clamp(110.0, 170.0);
                self.mini_slider(c, rect(x, r.y + 2.0, sw, r.h - 4.0), tr("Size"), Sl::BrushSize);
                x += sw + 14.0;
                self.mini_slider(c, rect(x, r.y + 2.0, sw, r.h - 4.0), tr("Hardness"), Sl::BrushHard);
                x += sw + 14.0;
                if self.tool != Tool::Cutout {
                    self.mini_slider(c, rect(x, r.y + 2.0, sw, r.h - 4.0), tr("Opacity"), Sl::BrushOpacity);
                } else {
                    self.mini_slider(c, rect(x, r.y + 2.0, sw, r.h - 4.0), tr("Tolerance"), Sl::CutoutTol);
                }
            }
            Tool::Fill => {
                let sw = 180.0;
                self.mini_slider(c, rect(x, r.y + 2.0, sw, r.h - 4.0), tr("Tolerance"), Sl::FillTol);
                x += sw + 16.0;
                c.text(
                    tr("Click a photo layer to fill an area, a shape or a text to recolour it."),
                    rect(x, r.y, r.right() - x - 8.0, r.h),
                    Font::Body,
                    p.t3,
                    Align::Left,
                );
            }
            Tool::Select => {
                let si = match self.sel_mode {
                    SelMode::Rect => 0,
                    SelMode::Ellipse => 1,
                    SelMode::Lasso => 2,
                };
                self.segmented_icons(c, rect(x, y, 120.0, 32.0), &[&icons::MARQUEE, &icons::MARQUEE_OVAL, &icons::LASSO], si, Sg::SelMode);
                x += 132.0;
                if self.selection.is_some() {
                    let acts: [(&str, Act); 7] = [
                        (tr("Deselect"), Act::Deselect),
                        (tr("Invert"), Act::InvertSel),
                        (tr("Fill"), Act::FillSel),
                        (tr("Clear"), Act::ClearSel),
                        (tr("To layer"), Act::SelToLayer),
                        (tr("To mask"), Act::SelToMask),
                        (tr("Crop"), Act::CropToSel),
                    ];
                    for (label, a) in acts {
                        let bw = c.text_width(label, Font::BodyBold) + 26.0;
                        if x + bw > r.right() - 8.0 {
                            break;
                        }
                        self.pill(c, rect(x, y, bw, 32.0), label, false, Hit::Act(a));
                        x += bw + 6.0;
                    }
                } else {
                    c.text(tr("Drag on the canvas to select. Shift: square or circle."), rect(x, r.y, r.right() - x - 8.0, r.h), Font::Body, p.t3, Align::Left);
                }
            }
            Tool::Crop => {
                let crop = self.crop.unwrap_or(CropState { cx: 0.0, cy: 0.0, w: 0.0, h: 0.0, angle: 0.0, ratio: 0 });
                let items = [tr("Free"), tr("Original"), "1:1", "4:5", "9:16", "16:9"];
                let segw = 380.0f32.min(room * 0.5);
                self.segmented(c, rect(x, y, segw, 32.0), &items, crop.ratio, Sg::CropRatio);
                x += segw + 14.0;
                self.mini_slider(c, rect(x, r.y + 2.0, 160.0, r.h - 4.0), tr("Straighten"), Sl::Straighten);
                x += 174.0;
                let ok = rect(r.right() - 16.0 - 96.0, y, 96.0, 32.0);
                self.pill(c, ok, tr("Apply"), true, Hit::Act(Act::CropApply));
                self.pill(c, rect(ok.x - 104.0, y, 96.0, 32.0), tr("Cancel"), false, Hit::Act(Act::CropCancel));
                let _ = x;
            }
            Tool::Shape => {
                let si = match self.new_shape {
                    ShapeKind::Rect => 0,
                    ShapeKind::Ellipse => 1,
                    ShapeKind::Line => 2,
                    ShapeKind::Arrow => 3,
                };
                self.segmented_icons(c, rect(x, y, 160.0, 32.0), &[&icons::SH_RECT, &icons::SH_ELLIPSE, &icons::SH_LINE, &icons::SH_ARROW], si, Sg::NewShape);
                x += 172.0;
                x += small(c, x, tr("Colour"));
                self.swatch(c, rect(x, y + 2.0, 40.0, 28.0), self.colour, Ct::Brush);
                x += 54.0;
                c.text(tr("Drag on the canvas. Shift keeps proportions."), rect(x, r.y, r.right() - x - 8.0, r.h), Font::Body, p.t3, Align::Left);
            }
            Tool::Text => {
                x += small(c, x, tr("Colour"));
                self.swatch(c, rect(x, y + 2.0, 40.0, 28.0), self.colour, Ct::Brush);
                x += 54.0;
                c.text(tr("Click on the canvas to add text, on a text to change it."), rect(x, r.y, r.right() - x - 8.0, r.h), Font::Body, p.t3, Align::Left);
            }
            Tool::Picker => {
                x += small(c, x, tr("Colour"));
                self.swatch(c, rect(x, y + 2.0, 40.0, 28.0), self.colour, Ct::Brush);
                x += 54.0;
                c.text(&self.colour.to_hex(), rect(x, r.y, 90.0, r.h), Font::BodyBold, p.t1, Align::Left);
                x += 96.0;
                c.text(tr("Click on the canvas to take its colour."), rect(x, r.y, r.right() - x - 8.0, r.h), Font::Body, p.t3, Align::Left);
            }
            Tool::Move => {
                let n = self.sel.len();
                let msg = if n == 0 {
                    tr("Click a layer to select it, Shift+click to add. Drag to move; handles scale and turn.").to_string()
                } else {
                    fill(tr("{a} selected · arrows nudge, Shift+arrows by 10"), &[&n.to_string()])
                };
                c.text(&msg, rect(x, r.y, r.right() - x - 8.0, r.h), Font::Body, p.t3, Align::Left);
            }
            Tool::Hand => {
                c.text(tr("Drag to move around. Space does the same with any tool."), rect(x, r.y, r.right() - x - 8.0, r.h), Font::Body, p.t3, Align::Left);
            }
        }
        if self.tool == Tool::Clone {
            let hint = if self.clone_src.is_some() { tr("Paint to copy. Alt+click picks another source.") } else { tr("Alt+click where to copy from.") };
            let hx = r.right() - 16.0 - c.text_width(hint, Font::Body);
            if hx > x + 40.0 {
                c.text(hint, rect(hx, r.y, r.right() - hx, r.h), Font::Body, p.t3, Align::Left);
            }
        }
    }

    /// A compact slider for the options bar: label and value over a thin track.
    pub fn mini_slider(&mut self, c: &mut Canvas, r: Rect, label: &str, sl: Sl) {
        let p = self.pal.clone();
        let (v, lo, hi) = self.slider_get(sl);
        c.text(label, rect(r.x, r.y + 4.0, r.w - 60.0, 18.0), Font::Small, p.t2, Align::Left);
        c.text(&self.slider_text(sl, v), rect(r.right() - 64.0, r.y + 4.0, 64.0, 18.0), Font::Small, p.t1, Align::Right);
        let ty = r.y + 30.0;
        let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
        c.fill_round(rect(r.x, ty - 2.0, r.w, 4.0), 2.0, p.fill2);
        c.fill_round(rect(r.x, ty - 2.0, r.w * t, 4.0), 2.0, p.accent);
        draw::circle(c, r.x + r.w * t, ty, 6.5, Some(Color::hex(0xffffff)), Some((Color::hex(0x000000).fade(0.18), 1.0)));
        self.hits.add(rect(r.x - 6.0, ty - 10.0, r.w + 12.0, 20.0), Hit::Slider(sl));
    }

    pub fn paint_status(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let p = self.pal.clone();
        let r = rect(RAIL_W, cx.h - STATUS_H, cx.w - RAIL_W - PANEL_W, STATUS_H);
        c.fill(r, p.bg);
        c.hline(r.x, r.y, r.w, p.line);
        self.hits.add(r, Hit::Bar);
        let y = r.y + 3.0;
        let size = fill(tr("{a} × {b} px"), &[&self.doc.w.to_string(), &self.doc.h.to_string()]);
        c.text(&size, rect(r.x + 16.0, r.y, 160.0, r.h), Font::Small, p.t3, Align::Left);
        // zoom
        let mut x = r.x + 16.0 + c.text_width(&size, Font::Small) + 20.0;
        self.icon_btn(c, &icons::MINUS, rect(x, y, 28.0, 28.0), Hit::Act(Act::ZoomOut), false);
        x += 30.0;
        let z = format!("{} %", (self.view.zoom * 100.0).round());
        let zr = rect(x, y, 58.0, 28.0);
        if self.hovered(&Hit::Act(Act::Actual)) {
            c.fill_round(zr, 8.0, p.hover);
        }
        c.text(&z, zr, Font::SmallBold, p.t1, Align::Center);
        self.hits.add(zr, Hit::Act(Act::Actual));
        x += 60.0;
        self.icon_btn(c, &ui_icons::PLUS, rect(x, y, 28.0, 28.0), Hit::Act(Act::ZoomIn), false);
        x += 32.0;
        self.icon_btn(c, &icons::FIT, rect(x, y, 28.0, 28.0), Hit::Act(Act::Fit), false);
        x += 40.0;
        self.icon_btn(c, &icons::GRID, rect(x, y, 28.0, 28.0), Hit::Toggle(Tg::Grid), self.set.grid);
        x += 32.0;
        self.icon_btn(c, &icons::RULER, rect(x, y, 28.0, 28.0), Hit::Toggle(Tg::Rulers), self.set.rulers);
        x += 32.0;
        self.icon_btn(c, &icons::MAGNET, rect(x, y, 28.0, 28.0), Hit::Toggle(Tg::Snap), self.set.snap);
        x += 40.0;
        // the pointer's place in the document
        if self.stage().contains(self.mouse) {
            let (dx, dy) = self.to_doc(self.mouse);
            c.text(&format!("{}, {}", dx.floor(), dy.floor()), rect(x, r.y, 100.0, r.h), Font::Small, p.t3, Align::Left);
        }
        self.icon_btn(c, &icons::KEYBOARD, rect(r.right() - 40.0, y, 28.0, 28.0), Hit::Act(Act::Shortcuts), false);
    }

    pub fn paint_rulers(&mut self, c: &mut Canvas) {
        if !self.set.rulers {
            return;
        }
        let p = self.pal.clone();
        let s = self.stage();
        let top = rect(s.x - RULER, s.y - RULER, s.w + RULER, RULER);
        let left = rect(s.x - RULER, s.y, RULER, s.h);
        c.fill(top, p.bg);
        c.fill(left, p.bg);
        c.hline(top.x, top.bottom() - 1.0, top.w, p.line);
        c.vline(left.right() - 1.0, left.y, left.h, p.line);
        // a step between labels of about 80 DIP, in round numbers
        let z = self.view.zoom;
        let raw = 80.0 / z;
        let mag = 10f32.powf(raw.log10().floor());
        let step = [1.0, 2.0, 5.0, 10.0].iter().map(|k| k * mag).find(|v| *v >= raw).unwrap_or(10.0 * mag);
        let first = ((s.x - self.view.ox) / z / step).floor() as i64;
        let last = ((s.right() - self.view.ox) / z / step).ceil() as i64;
        c.clip(top);
        for k in first..=last {
            let v = k as f32 * step;
            let x = self.view.ox + v * z;
            c.vline(x, top.y + 10.0, 10.0, p.t4);
            for j in 1..5 {
                c.vline(x + j as f32 * step * z / 5.0, top.y + 15.0, 5.0, p.t4.fade(0.6));
            }
            c.text(&format!("{}", v as i64), rect(x + 3.0, top.y, 60.0, 12.0), Font::Small, p.t3, Align::Left);
        }
        c.unclip();
        let first = ((s.y - self.view.oy) / z / step).floor() as i64;
        let last = ((s.bottom() - self.view.oy) / z / step).ceil() as i64;
        c.clip(left);
        for k in first..=last {
            let v = k as f32 * step;
            let y = self.view.oy + v * z;
            c.hline(left.x + 10.0, y, 10.0, p.t4);
            for j in 1..5 {
                c.hline(left.x + 15.0, y + j as f32 * step * z / 5.0, 5.0, p.t4.fade(0.6));
            }
            // a short label, written across the narrow band
            let label = format!("{}", v as i64);
            for (i, ch) in label.chars().take(5).enumerate() {
                c.text(&ch.to_string(), rect(left.x + 1.0, y + 2.0 + i as f32 * 11.0, 10.0, 12.0), Font::Small, p.t3, Align::Center);
            }
        }
        c.unclip();
        // the pointer on both rulers
        if s.contains(self.mouse) {
            c.vline(self.mouse.x, top.y, RULER, p.accent);
            c.hline(left.x, self.mouse.y, RULER, p.accent);
        }
    }

    pub fn paint_toast(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let Some((text, at)) = self.toast.clone() else { return };
        if at.elapsed().as_secs_f32() > 4.5 {
            self.toast = None;
            return;
        }
        cx.animate = true;
        let p = self.pal.clone();
        let tw = (c.text_width(&text, Font::Body) + 40.0).min(cx.w - 48.0);
        let bottom = if self.editing { STATUS_H + 24.0 } else { 40.0 };
        let left = if self.editing { RAIL_W } else { 0.0 };
        let right = if self.editing { PANEL_W } else { 0.0 };
        let mid = left + (cx.w - left - right) / 2.0;
        let r = rect(mid - tw / 2.0, cx.h - bottom - 38.0, tw, 38.0);
        c.fill_round(r, 19.0, p.solid2);
        c.stroke_round(r, 19.0, p.ring, 1.0);
        c.text(&text, r.inset_xy(16.0, 0.0), Font::Body, p.t1, Align::Center);
    }
}
