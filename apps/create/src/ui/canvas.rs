//! The canvas: the document drawn at the zoom shown (only the part in view), the frames and handles of the selected
//! layers, smart guides, the selection, the crop frame; and what each tool does with the mouse there.

use super::{App, CropState, Drag, Field, Hit, SelMode, Tab, Tool, draw};
use crate::{
    engine::{
        doc::{Affine, Id, Kind, Layer, Paint, Rgba, ShapeKind, Xf},
        img::IRect,
        ops,
        paint::{Brush, Stroke, Target},
        select::{SelShape, Selection},
    },
    i18n::{fill, tr},
};
use plume_ui::{Canvas, Color, Cx, Mods, PaintCx, Pt, Rect, rect};
use std::{sync::Arc, time::Instant};

/// Handles: 0 top left, 1 top, 2 top right, 3 right, 4 bottom right, 5 bottom, 6 bottom left, 7 left, 8 turn.
const HANDLE_UV: [(f32, f32); 8] = [(0.0, 0.0), (0.5, 0.0), (1.0, 0.0), (1.0, 0.5), (1.0, 1.0), (0.5, 1.0), (0.0, 1.0), (0.0, 0.5)];

impl App {
    pub fn paint_editor(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let st = self.stage();
        let outer = if self.set.rulers { rect(st.x - super::RULER, st.y - super::RULER, st.w + super::RULER, st.h + super::RULER) } else { st };
        c.fill(outer, self.pal.stage);
        self.paint_canvas(c, st);
        self.paint_rulers(c);
        self.paint_options(c, cx);
        self.paint_rail(c, cx);
        self.paint_status(c, cx);
        self.paint_panel(c, cx);
    }

    /// The frame the handles follow: a single layer's own box, else the bounds of all selected layers.
    pub fn sel_frame(&self) -> Option<Xf> {
        if self.sel.len() == 1 {
            return self.primary().map(|l| l.frame());
        }
        let mut acc: Option<(f32, f32, f32, f32)> = None;
        for id in &self.sel {
            if let Some(b) = self.doc.find(*id).map(|l| l.frame().bounds(0.0)) {
                acc = Some(match acc {
                    None => b,
                    Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
                });
            }
        }
        acc.map(|(x0, y0, x1, y1)| Xf::new((x0 + x1) / 2.0, (y0 + y1) / 2.0, x1 - x0, y1 - y0))
    }

    fn handle_points(&self, f: &Xf) -> ([Pt; 8], Pt) {
        let m = f.to_doc();
        let pts = HANDLE_UV.map(|(u, v)| {
            let (x, y) = m.apply(u * f.w, v * f.h);
            self.to_win(x, y)
        });
        // the turn handle: above the top middle, along the frame's "up"
        let (tx, ty) = m.apply(f.w / 2.0, 0.0);
        let (bx, by) = m.apply(f.w / 2.0, f.h);
        let (dx, dy) = (tx - bx, ty - by);
        let len = (dx * dx + dy * dy).sqrt().max(1e-3);
        let top = self.to_win(tx, ty);
        let rot = Pt { x: top.x + dx / len * 28.0, y: top.y + dy / len * 28.0 };
        (pts, rot)
    }

    pub fn paint_canvas(&mut self, c: &mut Canvas, st: Rect) {
        let p = self.pal.clone();
        self.hits.add(st, Hit::Canvas);
        c.clip(st);
        let z = self.view.zoom;
        let dpi = c.scale();
        let turn = self.crop.filter(|cr| cr.angle != 0.0);
        let doc_r = rect(self.view.ox, self.view.oy, self.doc.w as f32 * z, self.doc.h as f32 * z);
        let turn_at = turn.map(|cr| {
            let cpt = self.to_win(cr.cx, cr.cy);
            (-cr.angle, cpt.x, cpt.y)
        });
        // under the picture: a checkerboard where it is see-through
        if turn_at.is_none() {
            let shadow = doc_r.inset(-1.0);
            c.fill(shadow, Color::hex(0x000000).fade(if p.dark { 0.5 } else { 0.12 }));
            draw::checker(c, doc_r.intersect(&st), 8.0, p.dark);
        }
        // the picture, drawn for this zoom
        let mut s = z * dpi;
        let mut region = if turn.is_some() {
            IRect::new(0, 0, (self.doc.w as f32 * s).ceil() as i32, (self.doc.h as f32 * s).ceil() as i32)
        } else {
            let vx0 = ((st.x - self.view.ox) * dpi).floor() as i32;
            let vy0 = ((st.y - self.view.oy) * dpi).floor() as i32;
            let vis = IRect::new(vx0, vy0, (st.w * dpi).ceil() as i32 + 2, (st.h * dpi).ceil() as i32 + 2);
            vis.intersect(&IRect::new(0, 0, (self.doc.w as f32 * s).ceil() as i32, (self.doc.h as f32 * s).ceil() as i32))
        };
        let area = region.w as f32 * region.h as f32;
        if area > 12.0e6 {
            let k = (12.0e6 / area).sqrt();
            s *= k;
            region = IRect::new(
                (region.x as f32 * k).floor() as i32,
                (region.y as f32 * k).floor() as i32,
                (region.w as f32 * k).ceil() as i32,
                (region.h as f32 * k).ceil() as i32,
            );
        }
        if !region.is_empty() {
            let key = (self.ver, s.to_bits(), region, turn.is_some());
            let fresh = self.frame.as_ref().is_some_and(|f| f.key == key && f.rt == self.rt_id);
            if !fresh {
                let t = Instant::now();
                let buf = self.renderer.render(&self.doc, s, region);
                let rt = c.target();
                if let Some(bmp) = draw::bitmap(&rt, region.w as u32, region.h as u32, &buf.to_bgra()) {
                    self.frame = Some(super::Frame { bmp, key, rt: self.rt_id });
                }
                self.last_render_ms = t.elapsed().as_secs_f32() * 1000.0;
            }
            if let Some(f) = &self.frame {
                let k = z / s;
                let dest = rect(self.view.ox + region.x as f32 * k, self.view.oy + region.y as f32 * k, region.w as f32 * k, region.h as f32 * k);
                if let Some(t) = turn_at {
                    // straightening: the picture turns under a still frame
                    let corners = [(0.0, 0.0), (self.doc.w as f32, 0.0), (self.doc.w as f32, self.doc.h as f32), (0.0, self.doc.h as f32)];
                    let (sn, co) = t.0.to_radians().sin_cos();
                    let pts: Vec<(f32, f32)> = corners
                        .iter()
                        .map(|(x, y)| {
                            let q = self.to_win(*x, *y);
                            let (dx, dy) = (q.x - t.1, q.y - t.2);
                            (t.1 + dx * co - dy * sn, t.2 + dx * sn + dy * co)
                        })
                        .collect();
                    draw::fill_poly(c, &pts, if p.dark { Color::hex(0x3a3a3c) } else { Color::hex(0xffffff) });
                }
                draw::bitmap_at(c, &f.bmp, dest, 1.0, true, turn_at);
            }
        }
        if turn_at.is_none() {
            c.stroke_round(doc_r.inset(-0.5), 0.0, p.line, 1.0);
        }
        // grid
        if self.set.grid && turn.is_none() {
            let g = self.set.grid_size.max(2) as f32;
            if g * z >= 6.0 {
                let col = if p.dark { Color::hex(0xffffff).fade(0.18) } else { Color::hex(0x000000).fade(0.14) };
                let mut x = 0.0;
                while x <= self.doc.w as f32 {
                    c.vline(self.view.ox + x * z, doc_r.y, doc_r.h, col);
                    x += g;
                }
                let mut y = 0.0;
                while y <= self.doc.h as f32 {
                    c.hline(doc_r.x, self.view.oy + y * z, doc_r.w, col);
                    y += g;
                }
            }
        }
        // hovered layer
        if self.tool == Tool::Move && self.drag.is_none() && st.contains(self.mouse) && self.popup.is_none() {
            let (dx, dy) = self.to_doc(self.mouse);
            if let Some(id) = self.doc.pick(dx, dy).map(|id| self.top_of(id, false))
                && !self.sel.contains(&id)
                && let Some(l) = self.doc.find(id)
            {
                let pts: Vec<(f32, f32)> = l.frame().corners().iter().map(|(x, y)| self.to_win(*x, *y)).map(|q| (q.x, q.y)).collect();
                draw::poly(c, &pts, true, p.accent.fade(0.7), 1.0, None);
            }
        }
        // selected layers: their boxes, and the handles
        if self.crop.is_none() {
            for id in self.sel.clone() {
                if let Some(l) = self.doc.find(id) {
                    let pts: Vec<(f32, f32)> = l.frame().corners().iter().map(|(x, y)| self.to_win(*x, *y)).map(|q| (q.x, q.y)).collect();
                    draw::poly(c, &pts, true, p.accent, 1.2, None);
                }
            }
            if self.tool == Tool::Move
                && let Some(f) = self.sel_frame()
                && !self.sel.iter().any(|id| self.doc.find(*id).is_some_and(|l| l.locked))
            {
                let (pts, rot) = self.handle_points(&f);
                if self.sel.len() > 1 {
                    let poly: Vec<(f32, f32)> = [pts[0], pts[2], pts[4], pts[6]].iter().map(|q| (q.x, q.y)).collect();
                    draw::poly(c, &poly, true, p.accent, 1.0, None);
                }
                let lineish = self.sel.len() == 1
                    && self.primary().is_some_and(|l| matches!(&l.kind, Kind::Shape(s) if matches!(s.kind, ShapeKind::Line | ShapeKind::Arrow)));
                draw::line(c, pts[1].x, pts[1].y, rot.x, rot.y, p.accent, 1.0);
                draw::circle(c, rot.x, rot.y, 6.0, Some(Color::hex(0xffffff)), Some((p.accent, 1.5)));
                self.hits.add(rect(rot.x - 10.0, rot.y - 10.0, 20.0, 20.0), Hit::Handle(8));
                for (i, q) in pts.iter().enumerate() {
                    if lineish && i != 3 && i != 7 {
                        continue;
                    }
                    let r = rect(q.x - 5.0, q.y - 5.0, 10.0, 10.0);
                    c.fill_round(r, 2.5, Color::hex(0xffffff));
                    c.stroke_round(r, 2.5, p.accent, 1.5);
                    self.hits.add(r.inset(-4.0), Hit::Handle(i as u8));
                }
            }
        }
        // smart guides
        if !self.guides.xs.is_empty() || !self.guides.ys.is_empty() {
            let gc = Color::hex(0xff2d95);
            for x in self.guides.xs.clone() {
                let q = self.to_win(x, 0.0);
                draw::line(c, q.x, doc_r.y - 2000.0, q.x, doc_r.bottom() + 2000.0, gc, 1.0);
            }
            for y in self.guides.ys.clone() {
                let q = self.to_win(0.0, y);
                draw::line(c, doc_r.x - 2000.0, q.y, doc_r.right() + 2000.0, q.y, gc, 1.0);
            }
        }
        // the selection
        if let Some(s) = &self.selection {
            let pts: Vec<(f32, f32)> = s.shape.outline().iter().map(|(x, y)| self.to_win(*x, *y)).map(|q| (q.x, q.y)).collect();
            draw::poly(c, &pts, true, Color::hex(0xffffff), 1.0, Some(self.ants));
            if s.invert {
                draw::poly(
                    c,
                    &[(doc_r.x, doc_r.y), (doc_r.right(), doc_r.y), (doc_r.right(), doc_r.bottom()), (doc_r.x, doc_r.bottom())],
                    true,
                    Color::hex(0xffffff),
                    1.0,
                    Some(self.ants),
                );
            }
        }
        match &self.drag {
            Some(Drag::Marquee { start, now }) => {
                let shape = self.marquee_shape(*start, *now, false);
                let pts: Vec<(f32, f32)> = shape.outline().iter().map(|(x, y)| self.to_win(*x, *y)).map(|q| (q.x, q.y)).collect();
                draw::poly(c, &pts, true, Color::hex(0xffffff), 1.0, Some(self.ants));
            }
            Some(Drag::Lasso { pts }) => {
                let pts: Vec<(f32, f32)> = pts.iter().map(|(x, y)| self.to_win(*x, *y)).map(|q| (q.x, q.y)).collect();
                draw::poly(c, &pts, false, Color::hex(0xffffff), 1.0, Some(self.ants));
            }
            _ => {}
        }
        // crop frame
        if let Some(cr) = self.crop {
            let ctr = self.to_win(cr.cx, cr.cy);
            let fr = rect(ctr.x - cr.w * z / 2.0, ctr.y - cr.h * z / 2.0, cr.w * z, cr.h * z);
            let shade = Color::hex(0x000000).fade(0.5);
            c.fill(rect(st.x, st.y, st.w, fr.y - st.y), shade);
            c.fill(rect(st.x, fr.bottom(), st.w, st.bottom() - fr.bottom()), shade);
            c.fill(rect(st.x, fr.y, fr.x - st.x, fr.h), shade);
            c.fill(rect(fr.right(), fr.y, st.right() - fr.right(), fr.h), shade);
            c.stroke_round(fr, 0.0, Color::hex(0xffffff), 1.5);
            for k in 1..3 {
                let f = k as f32 / 3.0;
                c.vline(fr.x + fr.w * f, fr.y, fr.h, Color::hex(0xffffff).fade(0.45));
                c.hline(fr.x, fr.y + fr.h * f, fr.w, Color::hex(0xffffff).fade(0.45));
            }
            for (i, (u, v)) in HANDLE_UV.iter().enumerate() {
                let q = Pt { x: fr.x + u * fr.w, y: fr.y + v * fr.h };
                let r = rect(q.x - 6.0, q.y - 6.0, 12.0, 12.0);
                c.fill_round(r, 3.0, Color::hex(0xffffff));
                self.hits.add(r.inset(-5.0), Hit::CropHandle(i as u8));
            }
            self.hits.add(fr.inset(8.0), Hit::CropHandle(9));
            c.text(
                &format!("{} × {}", cr.w.round(), cr.h.round()),
                rect(fr.x, fr.bottom() + 6.0, fr.w, 20.0),
                plume_ui::Font::SmallBold,
                Color::hex(0xffffff),
                plume_ui::Align::Center,
            );
        }
        // the brush's size under the pointer
        if self.tool.brushy() && st.contains(self.mouse) && self.popup.is_none() && self.dialog.is_none() {
            let r = self.set.brush_size * z / 2.0;
            draw::circle(c, self.mouse.x, self.mouse.y, r.max(1.0), None, Some((Color::hex(0x000000).fade(0.6), 2.5)));
            draw::circle(c, self.mouse.x, self.mouse.y, r.max(1.0), None, Some((Color::hex(0xffffff), 1.0)));
            if self.tool == Tool::Clone
                && let Some((sx, sy)) = self.clone_src
            {
                // where the copy comes from, following the brush
                let q = match &self.drag {
                    Some(Drag::Stroke { .. }) => {
                        let off = self.clone_offset.unwrap_or((0.0, 0.0));
                        let (dx, dy) = self.to_doc(self.mouse);
                        self.to_win(dx - off.0, dy - off.1)
                    }
                    _ => self.to_win(sx, sy),
                };
                draw::line(c, q.x - 7.0, q.y, q.x + 7.0, q.y, Color::hex(0xffffff), 1.5);
                draw::line(c, q.x, q.y - 7.0, q.x, q.y + 7.0, Color::hex(0xffffff), 1.5);
                draw::circle(c, q.x, q.y, r.max(1.0), None, Some((Color::hex(0xffffff).fade(0.6), 1.0)));
            }
        }
        c.unclip();
    }

    /// A layer chosen on the canvas: inside a group, the group (Ctrl: the layer itself).
    pub fn top_of(&self, id: Id, deep: bool) -> Id {
        if deep {
            return id;
        }
        let mut cur = id;
        while let Some(g) = self.doc.group_of(cur) {
            cur = g;
        }
        cur
    }

    fn marquee_shape(&self, a: (f32, f32), b: (f32, f32), square: bool) -> SelShape {
        let (mut w, mut h) = (b.0 - a.0, b.1 - a.1);
        if square {
            let s = w.abs().max(h.abs());
            w = s * w.signum();
            h = s * h.signum();
        }
        let (x, y) = (a.0.min(a.0 + w), a.1.min(a.1 + h));
        if self.sel_mode == SelMode::Ellipse { SelShape::Ellipse(x, y, w.abs(), h.abs()) } else { SelShape::Rect(x, y, w.abs(), h.abs()) }
    }

    /// Document → pixels of what a stroke paints on (the picture, or the mask).
    fn target_px(l: &Layer, w: u32, h: u32) -> Affine {
        Affine { a: l.xf.w / w as f32, b: 0.0, c: 0.0, d: l.xf.h / h as f32, e: 0.0, f: 0.0 }.then(&l.xf.to_doc()).invert()
    }

    // ----- the mouse on the canvas -----

    pub fn canvas_down(&mut self, p: Pt, mods: Mods, clicks: u8, cx: &mut Cx) {
        let (x, y) = self.to_doc(p);
        let space = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x20) } < 0;
        if self.tool == Tool::Hand || space {
            self.drag = Some(Drag::Pan { start: p, ox: self.view.ox, oy: self.view.oy });
            return;
        }
        match self.tool {
            Tool::Move => {
                let picked = self.doc.pick(x, y).map(|id| self.top_of(id, mods.ctrl));
                match picked {
                    Some(id) => {
                        if mods.shift {
                            if let Some(i) = self.sel.iter().position(|s| *s == id) {
                                self.sel.remove(i);
                                return;
                            }
                            self.sel.push(id);
                        } else if !self.sel.contains(&id) {
                            self.sel = vec![id];
                        }
                        if clicks >= 2 && self.doc.find(id).is_some_and(|l| matches!(l.kind, Kind::Text(_))) {
                            self.tab = Tab::Layer;
                            self.open_field(Field::LayerText(id), cx);
                            return;
                        }
                        let orig = self.moving_layers();
                        self.drag = Some(Drag::Move { start: (x, y), before: self.doc.clone(), orig, moved: false });
                    }
                    None => {
                        if !mods.shift {
                            self.sel.clear();
                        }
                    }
                }
            }
            Tool::Select => {
                if self.sel_mode == SelMode::Lasso {
                    self.drag = Some(Drag::Lasso { pts: vec![(x, y)] });
                } else {
                    self.drag = Some(Drag::Marquee { start: (x, y), now: (x, y) });
                }
            }
            Tool::Crop => {
                if self.crop.is_none() {
                    self.start_crop();
                }
            }
            Tool::Brush | Tool::Eraser | Tool::Clone | Tool::Cutout => self.start_stroke(x, y, mods, cx),
            Tool::Fill => self.bucket(x, y, cx),
            Tool::Picker => {
                let c = self.renderer.pick(&self.doc, x, y);
                if c[3] > 0 {
                    self.colour = Rgba([c[0], c[1], c[2], 255]);
                    let hex = self.colour.to_hex();
                    self.set.add_colour(hex.clone());
                    self.say(fill(tr("Colour picked: {a}"), &[&hex]), cx);
                }
            }
            Tool::Text => {
                let hit = self.doc.pick(x, y).filter(|id| self.doc.find(*id).is_some_and(|l| matches!(l.kind, Kind::Text(_))));
                let id = match hit {
                    Some(id) => {
                        self.sel = vec![id];
                        id
                    }
                    None => self.add_text_at(x, y, tr("Your text")),
                };
                self.tab = Tab::Layer;
                self.open_field(Field::LayerText(id), cx);
            }
            Tool::Shape => {
                let before = self.doc.clone();
                let id = self.add_shape(self.new_shape, x, y, 1.0, 1.0);
                // one undo step, kept when the drag ends
                self.hist.forget_last();
                self.drag = Some(Drag::NewShape { start: (x, y), id, before });
            }
            Tool::Hand => {}
        }
        cx.repaint();
    }

    /// The layers a move or a transform takes, as they are now (children of a selected group are left to it).
    fn moving_layers(&self) -> Vec<(Id, Layer)> {
        self.sel
            .iter()
            .filter(|id| {
                let mut g = self.doc.group_of(**id);
                while let Some(gid) = g {
                    if self.sel.contains(&gid) {
                        return false;
                    }
                    g = self.doc.group_of(gid);
                }
                true
            })
            .filter_map(|id| self.doc.find(*id).filter(|l| !l.locked).map(|l| (*id, l.clone())))
            .collect()
    }

    pub fn handle_down(&mut self, h: u8, p: Pt) {
        let (x, y) = self.to_doc(p);
        let Some(frame) = self.sel_frame() else { return };
        let orig = self.moving_layers();
        if orig.is_empty() {
            return;
        }
        if h == 8 {
            self.drag = Some(Drag::Rotate { start: (x, y), before: self.doc.clone(), orig, centre: (frame.cx, frame.cy) });
        } else {
            self.drag = Some(Drag::Scale { handle: h, before: self.doc.clone(), orig, frame });
        }
    }

    pub fn start_crop(&mut self) {
        let (w, h) = (self.doc.w as f32, self.doc.h as f32);
        self.crop = Some(CropState { cx: w / 2.0, cy: h / 2.0, w, h, angle: 0.0, ratio: 0 });
        self.selection = None;
    }

    fn start_stroke(&mut self, x: f32, y: f32, mods: Mods, cx: &mut Cx) {
        let size = self.set.brush_size;
        let hard = self.set.brush_hardness;
        let op = self.set.brush_opacity;
        let sel = self.selection.clone();
        if self.tool == Tool::Clone && (mods.alt || self.clone_src.is_none()) {
            if mods.alt {
                self.clone_src = Some((x, y));
                self.say(tr("Source set: now paint where it should go"), cx);
            } else {
                self.say(tr("Alt+click where to copy from first"), cx);
            }
            return;
        }
        let mask = self.tool == Tool::Cutout;
        let id = match self.tool {
            Tool::Brush => self.paint_layer(),
            Tool::Cutout => match self.primary().filter(|l| !l.is_group() && !l.locked) {
                Some(l) => l.id,
                None => {
                    self.say(tr("Select a layer first"), cx);
                    return;
                }
            },
            _ => match self.primary().filter(|l| matches!(l.kind, Kind::Image(_)) && !l.locked) {
                Some(l) => l.id,
                None => {
                    self.say(tr("Select a photo or painting layer first"), cx);
                    return;
                }
            },
        };
        // painting on a mask: give the layer one if it has none
        if mask && self.doc.find(id).is_some_and(|l| l.mask.is_none()) {
            let (mw, mh) = self.doc.find(id).map(super::mask_size).unwrap_or((1, 1));
            self.change("add mask", |d| {
                if let Some(l) = d.find_mut(id) {
                    l.mask = Some(crate::engine::doc::Mask { px: Arc::new(crate::engine::img::Gray::new(mw, mh, 255)), key: 0, on: true });
                }
            });
        }
        let before = self.doc.clone();
        let Some(l) = self.doc.find(id) else { return };
        let (target, w, h) = if mask {
            let m = l.mask.as_ref().unwrap().px.clone();
            let (w, h) = (m.w, m.h);
            (Target::Mask(m), w, h)
        } else {
            let img = l.image().unwrap().clone();
            let (w, h) = (img.w, img.h);
            (Target::Image(img), w, h)
        };
        let to_px = Self::target_px(l, w, h);
        let brush = match self.tool {
            Tool::Brush => Brush::Paint(self.colour),
            Tool::Eraser => Brush::Erase,
            Tool::Cutout => {
                if self.cut_restore {
                    Brush::Show
                } else {
                    Brush::Hide
                }
            }
            _ => {
                let (sx, sy) = self.clone_src.unwrap_or((x, y));
                let (a, b) = (to_px.apply(x, y), to_px.apply(sx, sy));
                self.clone_offset = Some((x - sx, y - sy));
                if self.heal { Brush::Heal(a.0 - b.0, a.1 - b.1) } else { Brush::Clone(a.0 - b.0, a.1 - b.1) }
            }
        };
        let stroke = Stroke::new(brush, size, hard, if mask { 1.0 } else { op }, target, to_px, sel);
        self.drag = Some(Drag::Stroke { id, before, stroke: Box::new(stroke), mask });
        self.stroke_to(x, y);
    }

    /// The stroke goes on, and its pixels go into the document.
    fn stroke_to(&mut self, x: f32, y: f32) {
        let Some(Drag::Stroke { id, stroke, mask, .. }) = &mut self.drag else { return };
        let (id, mask) = (*id, *mask);
        // the picture in progress goes back to the stroke while it paints (one owner: changed in place)
        if let Some(l) = self.doc.find_mut(id) {
            match (&mut l.kind, mask) {
                (_, true) => {
                    if let Some(m) = &mut l.mask
                        && let Target::Mask(cur) = &stroke.out
                        && Arc::ptr_eq(cur, &m.px)
                    {
                        m.px = Arc::new(Default::default());
                    }
                }
                (Kind::Image(im), false) => {
                    if let Target::Image(cur) = &stroke.out
                        && Arc::ptr_eq(cur, &im.px)
                    {
                        im.px = Arc::new(Default::default());
                    }
                }
                _ => {}
            }
        }
        stroke.to(x, y);
        let out = stroke.out.clone();
        let dirty = stroke.dirty;
        if let Some(l) = self.doc.find_mut(id) {
            match (out, &mut l.kind) {
                (Target::Mask(m), _) => {
                    if let Some(mk) = &mut l.mask {
                        mk.px = m;
                    }
                }
                (Target::Image(img), Kind::Image(im)) => {
                    if let Some(d) = dirty {
                        self.renderer.refresh(&img, d);
                    }
                    im.px = img;
                }
                _ => {}
            }
        }
        self.ver += 1;
    }

    fn bucket(&mut self, x: f32, y: f32, cx: &mut Cx) {
        let Some(id) = self.doc.pick(x, y) else { return };
        let Some(l) = self.doc.find(id).cloned() else { return };
        let colour = self.colour;
        match &l.kind {
            Kind::Image(im) => {
                let img = im.px.clone();
                let to_px = Self::target_px(&l, img.w, img.h);
                let (px, py) = to_px.apply(x, y);
                if px < 0.0 || py < 0.0 {
                    return;
                }
                let sel = self.selection.as_ref().map(|s| (s, to_px.invert()));
                let Some(out) = crate::engine::paint::flood_fill(&img, px as u32, py as u32, colour, self.fill_tol, sel) else { return };
                self.change("fill", |d| {
                    if let Some(l) = d.find_mut(id)
                        && let Kind::Image(im) = &mut l.kind
                    {
                        im.px = Arc::new(out);
                    }
                });
            }
            Kind::Shape(_) | Kind::Text(_) => {
                self.change("recolour", |d| {
                    if let Some(l) = d.find_mut(id) {
                        match &mut l.kind {
                            Kind::Shape(s) => {
                                if matches!(s.kind, ShapeKind::Line | ShapeKind::Arrow) {
                                    s.stroke = Some(colour);
                                } else {
                                    s.fill = Some(Paint::Solid(colour));
                                }
                            }
                            Kind::Text(t) => t.color = colour,
                            _ => {}
                        }
                    }
                });
            }
            Kind::Group(_) => {}
        }
        self.sel = vec![id];
        cx.repaint();
    }

    pub fn canvas_move(&mut self, p: Pt, mods: Mods) {
        let (x, y) = self.to_doc(p);
        let z = self.view.zoom;
        let snap_on = self.set.snap && !mods.alt;
        let grid = if self.set.grid { self.set.grid_size as f32 } else { 0.0 };
        let Some(drag) = self.drag.take() else { return };
        let mut keep = Some(drag);
        match keep.as_mut().unwrap() {
            Drag::Pan { start, ox, oy } => {
                self.view.ox = *ox + p.x - start.x;
                self.view.oy = *oy + p.y - start.y;
            }
            Drag::Move { start, orig, moved, .. } => {
                let (mut dx, mut dy) = (x - start.0, y - start.1);
                if !*moved && (dx * z).hypot(dy * z) < 3.0 {
                    self.drag = keep;
                    return;
                }
                *moved = true;
                if mods.shift {
                    if dx.abs() > dy.abs() {
                        dy = 0.0;
                    } else {
                        dx = 0.0;
                    }
                }
                let ids: Vec<Id> = orig.iter().map(|(id, _)| *id).collect();
                self.guides = Default::default();
                if snap_on {
                    let mut b: Option<(f32, f32, f32, f32)> = None;
                    for (_, l) in orig.iter() {
                        let f = l.frame().bounds(0.0);
                        b = Some(match b {
                            None => f,
                            Some(a) => (a.0.min(f.0), a.1.min(f.1), a.2.max(f.2), a.3.max(f.3)),
                        });
                    }
                    if let Some(b) = b {
                        let s = ops::snap(&self.doc, &ids, (b.0 + dx, b.1 + dy, b.2 + dx, b.3 + dy), 6.0 / z, grid);
                        dx += s.dx;
                        dy += s.dy;
                        self.guides = s;
                    }
                }
                for (id, o) in orig.iter() {
                    let mut l = o.clone();
                    l.translate(dx, dy);
                    if let Some(t) = self.doc.find_mut(*id) {
                        *t = l;
                    }
                }
                self.ver += 1;
            }
            Drag::Scale { handle, orig, frame, .. } => {
                self.scale_drag(*handle, orig, frame, x, y, mods);
                self.ver += 1;
            }
            Drag::Rotate { start, orig, centre, .. } => {
                let a0 = (start.1 - centre.1).atan2(start.0 - centre.0);
                let a1 = (y - centre.1).atan2(x - centre.0);
                let mut d = (a1 - a0).to_degrees();
                if orig.len() == 1 && !orig[0].1.is_group() {
                    let mut r = orig[0].1.xf.rot + d;
                    if mods.shift {
                        r = (r / 15.0).round() * 15.0;
                    }
                    if let Some(l) = self.doc.find_mut(orig[0].0) {
                        l.xf.rot = crate::engine::doc::norm_deg(r);
                    }
                } else {
                    if mods.shift {
                        d = (d / 15.0).round() * 15.0;
                    }
                    for (id, o) in orig.iter() {
                        let mut l = o.clone();
                        l.rotate_about(centre.0, centre.1, d);
                        if let Some(t) = self.doc.find_mut(*id) {
                            *t = l;
                        }
                    }
                }
                self.ver += 1;
            }
            Drag::Marquee { now, .. } => *now = (x, y),
            Drag::Lasso { pts } => {
                if pts.last().is_none_or(|l| (l.0 - x).hypot(l.1 - y) * z > 2.0) {
                    pts.push((x, y));
                }
            }
            Drag::NewShape { start, id, .. } => {
                let (sx, sy) = *start;
                let id = *id;
                let kind = self.doc.find(id).and_then(|l| if let Kind::Shape(s) = &l.kind { Some(s.kind) } else { None });
                let xf = match kind {
                    Some(ShapeKind::Line | ShapeKind::Arrow) => {
                        let (mut dx, mut dy) = (x - sx, y - sy);
                        if mods.shift {
                            let a = (dy.atan2(dx) / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
                            let len = dx.hypot(dy);
                            dx = a.cos() * len;
                            dy = a.sin() * len;
                        }
                        let len = dx.hypot(dy).max(1.0);
                        let sw = self.doc.find(id).map_or(8.0, |l| if let Kind::Shape(s) = &l.kind { s.stroke_w } else { 8.0 });
                        let mut xf = Xf::new(sx + dx / 2.0, sy + dy / 2.0, len, (sw * 4.0).max(24.0));
                        xf.rot = dy.atan2(dx).to_degrees();
                        xf
                    }
                    _ => {
                        let (mut w, mut h) = (x - sx, y - sy);
                        if mods.shift {
                            let s = w.abs().max(h.abs());
                            w = s * w.signum();
                            h = s * h.signum();
                        }
                        Xf::new(sx + w / 2.0, sy + h / 2.0, w.abs().max(1.0), h.abs().max(1.0))
                    }
                };
                if let Some(l) = self.doc.find_mut(id) {
                    l.xf = xf;
                }
                self.ver += 1;
            }
            Drag::Stroke { .. } => {
                self.drag = keep;
                self.stroke_to(x, y);
                return;
            }
            Drag::Crop { handle, start, orig } => {
                let (handle, start, orig) = (*handle, *start, *orig);
                self.crop_drag(handle, start, orig, p);
                let _ = start;
            }
            _ => {}
        }
        self.drag = keep;
    }

    fn scale_drag(&mut self, h: u8, orig: &[(Id, Layer)], frame: &Xf, x: f32, y: f32, mods: Mods) {
        let single = orig.len() == 1 && !orig[0].1.is_group();
        // the frame's own axes (a single layer: turned with it)
        let f = if single { orig[0].1.xf } else { *frame };
        let m = f.to_doc();
        let (u, v) = m.invert().apply(x, y);
        let (mut u0, mut v0, mut u1, mut v1) = (0.0, 0.0, f.w, f.h);
        let (left, right, top, bottom) = (matches!(h, 0 | 6 | 7), matches!(h, 2..=4), matches!(h, 0..=2), matches!(h, 4..=6));
        let min = 2.0;
        if left {
            u0 = u.min(f.w - min);
        }
        if right {
            u1 = u.max(min);
        }
        if top {
            v0 = v.min(f.h - min);
        }
        if bottom {
            v1 = v.max(min);
        }
        let corner = matches!(h, 0 | 2 | 4 | 6);
        let kind_keeps = orig.iter().any(|(_, l)| matches!(l.kind, Kind::Image(_) | Kind::Text(_) | Kind::Group(_))) || orig.len() > 1;
        let keep = corner && (kind_keeps != mods.shift);
        let text_side = single && matches!(orig[0].1.kind, Kind::Text(_)) && !corner;
        let (mut kx, mut ky) = ((u1 - u0) / f.w.max(1e-3), (v1 - v0) / f.h.max(1e-3));
        if keep {
            let k = kx.max(ky);
            kx = k;
            ky = k;
            if left {
                u0 = f.w - f.w * k;
            } else {
                u1 = f.w * k;
            }
            if top {
                v0 = f.h - f.h * k;
            } else {
                v1 = f.h * k;
            }
        }
        if single {
            let (id, o) = &orig[0];
            let mut l = o.clone();
            let (ncx, ncy) = m.apply((u0 + u1) / 2.0, (v0 + v1) / 2.0);
            match &mut l.kind {
                Kind::Text(_) if text_side => {
                    if top || bottom {
                        return;
                    }
                    l.xf.w = u1 - u0;
                    l.xf.cx = ncx;
                    l.xf.cy = ncy;
                }
                Kind::Text(_) => {
                    let k = (kx * ky).sqrt();
                    l.scale_about(l.xf.cx, l.xf.cy, k, k);
                    l.xf.cx = ncx;
                    l.xf.cy = ncy;
                }
                _ => {
                    l.xf.w = u1 - u0;
                    l.xf.h = v1 - v0;
                    l.xf.cx = ncx;
                    l.xf.cy = ncy;
                }
            }
            if let Some(t) = self.doc.find_mut(*id) {
                *t = l;
            }
            if text_side {
                self.refit_text(*id);
            }
        } else {
            // the side or corner opposite the handle stays where it is
            let (ax, ay) = m.apply(if left { f.w } else { 0.0 }, if top { f.h } else { 0.0 });
            let (kx, ky) = (if left || right { kx } else { 1.0 }, if top || bottom { ky } else { 1.0 });
            for (id, o) in orig {
                let mut l = o.clone();
                l.scale_about(ax, ay, kx, ky);
                if let Some(t) = self.doc.find_mut(*id) {
                    *t = l;
                }
            }
        }
    }

    fn crop_drag(&mut self, h: u8, start: (f32, f32), orig: CropState, p: Pt) {
        let z = self.view.zoom;
        // in the crop frame's axes (the picture turns, the frame does not)
        let sp = self.to_win(start.0, start.1);
        let _ = sp;
        let (mx, my) = ((p.x - self.drag_origin.x) / z, (p.y - self.drag_origin.y) / z);
        let mut c = orig;
        if h == 9 {
            let (s, co) = orig.angle.to_radians().sin_cos();
            c.cx = orig.cx + mx * co - my * s;
            c.cy = orig.cy + mx * s + my * co;
        } else {
            let (left, right, top, bottom) = (matches!(h, 0 | 6 | 7), matches!(h, 2..=4), matches!(h, 0..=2), matches!(h, 4..=6));
            let (mut x0, mut y0, mut x1, mut y1) = (-orig.w / 2.0, -orig.h / 2.0, orig.w / 2.0, orig.h / 2.0);
            if left {
                x0 = (x0 + mx).min(x1 - 8.0);
            }
            if right {
                x1 = (x1 + mx).max(x0 + 8.0);
            }
            if top {
                y0 = (y0 + my).min(y1 - 8.0);
            }
            if bottom {
                y1 = (y1 + my).max(y0 + 8.0);
            }
            let ratio = match orig.ratio {
                1 => Some(self.doc.w as f32 / self.doc.h as f32),
                2 => Some(1.0),
                3 => Some(0.8),
                4 => Some(9.0 / 16.0),
                5 => Some(16.0 / 9.0),
                _ => None,
            };
            if let Some(r) = ratio {
                let (w, hh) = (x1 - x0, y1 - y0);
                if left || right {
                    let nh = w / r;
                    if top {
                        y0 = y1 - nh;
                    } else if bottom {
                        y1 = y0 + nh;
                    } else {
                        y0 = -nh / 2.0;
                        y1 = nh / 2.0;
                    }
                } else {
                    let nw = hh * r;
                    x0 = -nw / 2.0;
                    x1 = nw / 2.0;
                }
            }
            let (s, co) = orig.angle.to_radians().sin_cos();
            let (fx, fy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            c.cx = orig.cx + fx * co - fy * s;
            c.cy = orig.cy + fx * s + fy * co;
            c.w = x1 - x0;
            c.h = y1 - y0;
        }
        self.crop = Some(c);
    }

    pub fn canvas_up(&mut self, mods: Mods, cx: &mut Cx) {
        self.guides = Default::default();
        let Some(drag) = self.drag.take() else { return };
        match drag {
            Drag::Move { before, moved, .. } => {
                if moved {
                    self.commit(before, "move");
                }
            }
            Drag::Scale { before, .. } => self.commit(before, "scale"),
            Drag::Rotate { before, .. } => self.commit(before, "rotate"),
            Drag::Marquee { start, now } => {
                let shape = self.marquee_shape(start, now, mods.shift);
                let (x0, y0, x1, y1) = shape.bounds();
                self.selection = ((x1 - x0) * self.view.zoom > 2.0 && (y1 - y0) * self.view.zoom > 2.0).then(|| Selection::new(shape, self.doc.w, self.doc.h));
            }
            Drag::Lasso { pts } => {
                self.selection = (pts.len() >= 3).then(|| Selection::new(SelShape::Poly(pts), self.doc.w, self.doc.h));
            }
            Drag::NewShape { id, before, start } => {
                // a click without a drag: a shape of a good size there
                let tiny = self.doc.find(id).is_some_and(|l| l.xf.w * self.view.zoom < 4.0 && l.xf.h * self.view.zoom < 4.0);
                if tiny {
                    let s = self.doc.w.min(self.doc.h) as f32 * 0.25;
                    if let Some(l) = self.doc.find_mut(id) {
                        let line = matches!(&l.kind, Kind::Shape(sh) if matches!(sh.kind, ShapeKind::Line | ShapeKind::Arrow));
                        l.xf =
                            if line { Xf::new(start.0 + s / 2.0, start.1, s, l.xf.h.max(24.0)) } else { Xf::new(start.0 + s / 2.0, start.1 + s / 2.0, s, s) };
                    }
                }
                self.commit(before, "add shape");
                self.tool = Tool::Move;
            }
            Drag::Stroke { id, before, stroke, mask } => {
                let _ = (id, mask, stroke);
                self.commit(before, "paint");
                self.clone_offset = None;
            }
            Drag::Slider { sl, .. } => {
                let _ = sl;
                self.settle();
            }
            Drag::Layer { id, over, moved, .. } => {
                if moved && let Some((parent, index, _)) = over {
                    self.change("order", |d| {
                        ops::move_to(d, id, parent, index);
                    });
                }
            }
            Drag::PickSv(_) | Drag::PickHue(_) | Drag::PickAlpha(_) => {}
            other => {
                // pan, crop: nothing to keep
                drop(other);
            }
        }
        cx.repaint();
    }

    /// Keeps the change made while a slider moved or a colour was picked, as one undo step.
    pub fn settle(&mut self) {
        if let Some(before) = self.field_before.take()
            && (self.doc.layers != before.layers || self.doc.w != before.w || self.doc.h != before.h)
        {
            self.commit(before, "change");
        }
    }
}
