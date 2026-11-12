//! The right panel: the selected layer's properties, its adjustments, the looks; below, the layers.

use super::{App, Ct, Drag, Field, Hit, PANEL_W, STATUS_H, Sg, Sl, Tab, Tg, draw, icons};
use crate::{
    engine::{
        doc::{Blend, Id, Kind, Layer, Look, Paint, Rgba, ShapeKind, TextAlign},
        img::IRect,
        ops::AlignTo,
    },
    i18n::{fill, tr},
};
use plume_ui::{Align, Canvas, Font, PaintCx, Rect, icons as ui_icons, m, rect};

pub const ADJ: [(&str, u8); 13] = [
    ("Exposure", 0),
    ("Brightness", 1),
    ("Contrast", 2),
    ("Saturation", 3),
    ("Vibrance", 4),
    ("Temperature", 5),
    ("Tint", 6),
    ("Black point", 7),
    ("White point", 8),
    ("Midtones", 9),
    ("Sharpen", 10),
    ("Blur", 11),
    ("Vignette", 12),
];

const WEIGHTS: [u16; 5] = [300, 400, 600, 700, 900];
const ROW: f32 = 40.0;

impl App {
    // ----- values behind the controls -----

    /// (value as shown, lowest, highest).
    pub fn slider_get(&self, sl: Sl) -> (f32, f32, f32) {
        let l = self.primary();
        let text = l.and_then(|l| if let Kind::Text(t) = &l.kind { Some(t) } else { None });
        let shape = l.and_then(|l| if let Kind::Shape(s) = &l.kind { Some(s) } else { None });
        let adj = l.map(|l| l.adj).unwrap_or_default();
        match sl {
            Sl::BrushSize => (self.set.brush_size, 1.0, 500.0),
            Sl::BrushHard => (self.set.brush_hardness * 100.0, 0.0, 100.0),
            Sl::BrushOpacity => (self.set.brush_opacity * 100.0, 1.0, 100.0),
            Sl::FillTol => (self.fill_tol * 100.0, 0.0, 100.0),
            Sl::CutoutTol => (self.cut_tol * 100.0, 0.0, 100.0),
            Sl::Opacity => (l.map_or(100.0, |l| l.opacity * 100.0), 0.0, 100.0),
            Sl::Rotation => (l.map_or(0.0, |l| l.xf.rot), -180.0, 180.0),
            Sl::Adj(k) => {
                let v = match k {
                    0 => adj.exposure * 100.0,
                    1 => adj.brightness * 100.0,
                    2 => adj.contrast * 100.0,
                    3 => adj.saturation * 100.0,
                    4 => adj.vibrance * 100.0,
                    5 => adj.temperature * 100.0,
                    6 => adj.tint * 100.0,
                    7 => adj.black * 200.0,
                    8 => (adj.white - 0.5) * 200.0,
                    9 => adj.gamma,
                    10 => adj.sharpen * 100.0,
                    11 => adj.blur * 100.0,
                    _ => adj.vignette * 100.0,
                };
                match k {
                    7 | 8 | 10 | 11 => (v, 0.0, 100.0),
                    9 => (v, 0.2, 3.0),
                    _ => (v, -100.0, 100.0),
                }
            }
            Sl::LookAmount => (adj.look_amount * 100.0, 0.0, 100.0),
            Sl::TextSize => (text.map_or(64.0, |t| t.size), 4.0, 800.0),
            Sl::TextSpacing => (text.map_or(0.0, |t| t.spacing), -20.0, 100.0),
            Sl::TextLine => (text.map_or(1.2, |t| t.line), 0.6, 3.0),
            Sl::OutlineW => (text.and_then(|t| t.outline).map_or(0.0, |o| o.1), 0.0, 40.0),
            Sl::ShadowBlur => (text.and_then(|t| t.shadow).map_or(0.0, |s| s.3), 0.0, 60.0),
            Sl::ShadowY => (text.and_then(|t| t.shadow).map_or(0.0, |s| s.2), -60.0, 60.0),
            Sl::StrokeW => (shape.map_or(0.0, |s| s.stroke_w), 0.0, 100.0),
            Sl::Radius => (shape.map_or(0.0, |s| s.radius), 0.0, 500.0),
            Sl::GradAngle => (
                shape.and_then(|s| s.fill).map_or(0.0, |f| match f {
                    Paint::Linear(_, _, a) => a,
                    _ => 0.0,
                }),
                0.0,
                360.0,
            ),
            Sl::Straighten => (self.crop.map_or(0.0, |c| c.angle), -45.0, 45.0),
            Sl::ExportQuality => (
                match &self.dialog {
                    Some(super::Dialog::Export { quality, .. }) => *quality as f32,
                    _ => 90.0,
                },
                1.0,
                100.0,
            ),
        }
    }

    pub fn slider_text(&self, sl: Sl, v: f32) -> String {
        match sl {
            Sl::BrushSize | Sl::TextSize | Sl::OutlineW | Sl::ShadowBlur | Sl::ShadowY | Sl::StrokeW | Sl::Radius | Sl::TextSpacing => {
                format!("{} px", v.round())
            }
            Sl::Rotation | Sl::GradAngle | Sl::Straighten => format!("{:.1}°", v),
            Sl::TextLine => format!("{v:.2}"),
            Sl::Adj(9) => format!("{v:.2}"),
            Sl::Adj(_) => {
                let r = v.round();
                if r > 0.0 && !matches!(sl, Sl::Adj(7 | 8 | 10 | 11)) { format!("+{r}") } else { format!("{r}") }
            }
            _ => format!("{} %", v.round()),
        }
    }

    /// Does the slider change the document (an undo step)?
    pub fn slider_on_doc(sl: Sl) -> bool {
        !matches!(sl, Sl::BrushSize | Sl::BrushHard | Sl::BrushOpacity | Sl::FillTol | Sl::CutoutTol | Sl::Straighten | Sl::ExportQuality)
    }

    pub fn slider_set(&mut self, sl: Sl, v: f32) {
        let (_, lo, hi) = self.slider_get(sl);
        let v = v.clamp(lo, hi);
        let ids = self.sel.clone();
        let mut refit = vec![];
        match sl {
            Sl::BrushSize => self.set.brush_size = v.round().max(1.0),
            Sl::BrushHard => self.set.brush_hardness = v / 100.0,
            Sl::BrushOpacity => self.set.brush_opacity = v / 100.0,
            Sl::FillTol => self.fill_tol = v / 100.0,
            Sl::CutoutTol => self.cut_tol = v / 100.0,
            Sl::Straighten => {
                if let Some(c) = &mut self.crop {
                    c.angle = v;
                    // the frame shrinks so no empty corner comes in
                    let (w, h) = (self.doc.w as f32, self.doc.h as f32);
                    let (s, co) = v.to_radians().sin_cos();
                    let (bw, bh) = (w * co.abs() + h * s.abs(), w * s.abs() + h * co.abs());
                    let k = (w / bw).min(h / bh);
                    c.cx = w / 2.0;
                    c.cy = h / 2.0;
                    c.w = w * k;
                    c.h = h * k;
                }
            }
            Sl::ExportQuality => {
                if let Some(super::Dialog::Export { quality, .. }) = &mut self.dialog {
                    *quality = v.round() as u32;
                }
            }
            _ => {
                for id in ids {
                    let Some(l) = self.doc.find_mut(id) else { continue };
                    match sl {
                        Sl::Opacity => l.opacity = v / 100.0,
                        Sl::Rotation => {
                            if l.is_group() {
                                let f = l.frame();
                                let d = v - l.xf.rot;
                                l.rotate_about(f.cx, f.cy, d);
                                l.xf.rot = v;
                            } else {
                                l.xf.rot = v;
                            }
                        }
                        Sl::Adj(k) => {
                            let a = &mut l.adj;
                            match k {
                                0 => a.exposure = v / 100.0,
                                1 => a.brightness = v / 100.0,
                                2 => a.contrast = v / 100.0,
                                3 => a.saturation = v / 100.0,
                                4 => a.vibrance = v / 100.0,
                                5 => a.temperature = v / 100.0,
                                6 => a.tint = v / 100.0,
                                7 => a.black = (v / 200.0).min(a.white - 0.02),
                                8 => a.white = (0.5 + v / 200.0).max(a.black + 0.02),
                                9 => a.gamma = v,
                                10 => a.sharpen = v / 100.0,
                                11 => a.blur = v / 100.0,
                                _ => a.vignette = v / 100.0,
                            }
                        }
                        Sl::LookAmount => l.adj.look_amount = v / 100.0,
                        _ => {}
                    }
                    if let Kind::Text(t) = &mut l.kind {
                        match sl {
                            Sl::TextSize => t.size = v,
                            Sl::TextSpacing => t.spacing = v,
                            Sl::TextLine => t.line = v,
                            Sl::OutlineW => {
                                let c = t.outline.map_or(Rgba::BLACK, |o| o.0);
                                t.outline = (v > 0.0).then_some((c, v));
                            }
                            Sl::ShadowBlur => {
                                if let Some(s) = &mut t.shadow {
                                    s.3 = v;
                                }
                            }
                            Sl::ShadowY => {
                                if let Some(s) = &mut t.shadow {
                                    s.2 = v;
                                }
                            }
                            _ => {}
                        }
                        if matches!(sl, Sl::TextSize | Sl::TextSpacing | Sl::TextLine) {
                            refit.push(l.id);
                        }
                    }
                    if let Kind::Shape(s) = &mut l.kind {
                        match sl {
                            Sl::StrokeW => s.stroke_w = v,
                            Sl::Radius => s.radius = v,
                            Sl::GradAngle => {
                                if let Some(Paint::Linear(_, _, a)) = &mut s.fill {
                                    *a = v;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                self.touch();
            }
        }
        for id in refit {
            self.refit_text(id);
        }
    }

    pub fn colour_get(&self, ct: Ct) -> Rgba {
        let l = self.primary();
        match (ct, l.map(|l| &l.kind)) {
            (Ct::Brush, _) => self.colour,
            (Ct::Text, Some(Kind::Text(t))) => t.color,
            (Ct::Outline, Some(Kind::Text(t))) => t.outline.map_or(Rgba::BLACK, |o| o.0),
            (Ct::Shadow, Some(Kind::Text(t))) => t.shadow.map_or(Rgba([0, 0, 0, 140]), |s| s.0),
            (Ct::Fill, Some(Kind::Shape(s))) => s.fill.map_or(self.colour, |f| f.first()),
            (Ct::Fill2, Some(Kind::Shape(s))) => match s.fill {
                Some(Paint::Linear(_, b, _)) => b,
                _ => Rgba::WHITE,
            },
            (Ct::Stroke, Some(Kind::Shape(s))) => s.stroke.unwrap_or(Rgba::BLACK),
            _ => self.colour,
        }
    }

    pub fn colour_set(&mut self, ct: Ct, c: Rgba) {
        if ct == Ct::Brush {
            self.colour = c;
            return;
        }
        let ids = self.sel.clone();
        for id in ids {
            let Some(l) = self.doc.find_mut(id) else { continue };
            match (&mut l.kind, ct) {
                (Kind::Text(t), Ct::Text) => t.color = c,
                (Kind::Text(t), Ct::Outline) => t.outline = Some((c, t.outline.map_or(4.0, |o| o.1))),
                (Kind::Text(t), Ct::Shadow) => {
                    if let Some(s) = &mut t.shadow {
                        s.0 = c;
                    }
                }
                (Kind::Shape(s), Ct::Fill) => {
                    s.fill = Some(match s.fill {
                        Some(Paint::Linear(_, b, a)) => Paint::Linear(c, b, a),
                        _ => Paint::Solid(c),
                    })
                }
                (Kind::Shape(s), Ct::Fill2) => {
                    if let Some(Paint::Linear(a, _, ang)) = s.fill {
                        s.fill = Some(Paint::Linear(a, c, ang));
                    }
                }
                (Kind::Shape(s), Ct::Stroke) => s.stroke = Some(c),
                _ => {}
            }
        }
        self.touch();
    }

    pub fn toggle(&mut self, tg: Tg) {
        match tg {
            Tg::Grid => self.set.grid = !self.set.grid,
            Tg::Rulers => {
                self.set.rulers = !self.set.rulers;
            }
            Tg::Snap => self.set.snap = !self.set.snap,
            _ => {
                let ids = self.sel.clone();
                let mut refit = vec![];
                self.change("toggle", |d| {
                    for id in ids {
                        let Some(l) = d.find_mut(id) else { continue };
                        if tg == Tg::MaskOn {
                            if let Some(m) = &mut l.mask {
                                m.on = !m.on;
                            }
                            continue;
                        }
                        match &mut l.kind {
                            Kind::Text(t) => {
                                match tg {
                                    Tg::Italic => t.italic = !t.italic,
                                    Tg::Outline => t.outline = if t.outline.is_some() { None } else { Some((Rgba::BLACK, (t.size / 16.0).max(1.0).round())) },
                                    Tg::Shadow => {
                                        t.shadow = if t.shadow.is_some() {
                                            None
                                        } else {
                                            Some((Rgba([0, 0, 0, 150]), 0.0, (t.size / 14.0).round().max(2.0), (t.size / 8.0).round().max(2.0)))
                                        }
                                    }
                                    _ => {}
                                }
                                refit.push(l.id);
                            }
                            Kind::Shape(s) => match tg {
                                Tg::FillOn => s.fill = if s.fill.is_some() { None } else { Some(Paint::Solid(Rgba::hex(0xdb2b9a))) },
                                Tg::StrokeOn => {
                                    s.stroke = if s.stroke.is_some() { None } else { Some(Rgba::BLACK) };
                                    if s.stroke_w <= 0.0 {
                                        s.stroke_w = 6.0;
                                    }
                                }
                                Tg::Gradient => {
                                    s.fill = match s.fill {
                                        Some(Paint::Linear(a, _, _)) => Some(Paint::Solid(a)),
                                        Some(Paint::Solid(a)) => Some(Paint::Linear(a, Rgba::hex(0xff8a3d), 90.0)),
                                        None => Some(Paint::Linear(Rgba::hex(0xdb2b9a), Rgba::hex(0xff8a3d), 90.0)),
                                    }
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                });
                for id in refit {
                    self.refit_text(id);
                }
            }
        }
        if matches!(tg, Tg::Grid | Tg::Rulers | Tg::Snap) {
            self.set.save();
        }
    }

    pub fn segment(&mut self, sg: Sg, i: usize, cx: &mut plume_ui::Cx) {
        match sg {
            Sg::SelMode => self.sel_mode = [super::SelMode::Rect, super::SelMode::Ellipse, super::SelMode::Lasso][i.min(2)],
            Sg::CloneMode => self.heal = i == 1,
            Sg::CutMode => self.cut_restore = i == 1,
            Sg::NewShape => self.new_shape = [ShapeKind::Rect, ShapeKind::Ellipse, ShapeKind::Line, ShapeKind::Arrow][i.min(3)],
            Sg::Tab => {
                self.tab = [Tab::Layer, Tab::Adjust, Tab::Looks][i.min(2)];
                self.panel_scroll = 0.0;
            }
            Sg::Theme => self.act(super::Act::Theme(["system", "dark", "light"][i.min(2)]), cx),
            Sg::Lang => self.act(super::Act::Lang(["system", "en", "fr"][i.min(2)]), cx),
            Sg::Format => {
                if let Some(super::Dialog::Export { format, .. }) = &mut self.dialog {
                    *format = [crate::wic::Format::Png, crate::wic::Format::Jpeg, crate::wic::Format::Webp][i.min(2)];
                }
            }
            Sg::ExportSize => {
                let k = [0.5, 1.0, 2.0][i.min(2)];
                let (dw, dh) = (self.doc.w as f32, self.doc.h as f32);
                if let Some(super::Dialog::Export { w, h, .. }) = &mut self.dialog {
                    *w = (dw * k).round() as u32;
                    *h = (dh * k).round() as u32;
                }
            }
            Sg::CropRatio => {
                let (dw, dh) = (self.doc.w as f32, self.doc.h as f32);
                if let Some(c) = &mut self.crop {
                    c.ratio = i;
                    let r = match i {
                        1 => Some(dw / dh),
                        2 => Some(1.0),
                        3 => Some(4.0 / 5.0),
                        4 => Some(9.0 / 16.0),
                        5 => Some(16.0 / 9.0),
                        _ => None,
                    };
                    if let Some(r) = r {
                        // the largest frame of that shape in the current one
                        let (w, h) = if c.w / c.h > r { (c.h * r, c.h) } else { (c.w, c.w / r) };
                        c.w = w;
                        c.h = h;
                    }
                }
            }
            Sg::ShapeKind | Sg::Weight | Sg::Align => {
                let ids = self.sel.clone();
                let mut refit = vec![];
                self.change("style", |d| {
                    for id in ids {
                        let Some(l) = d.find_mut(id) else { continue };
                        match (&mut l.kind, sg) {
                            (Kind::Shape(s), Sg::ShapeKind) => {
                                let k = [ShapeKind::Rect, ShapeKind::Ellipse, ShapeKind::Line, ShapeKind::Arrow][i.min(3)];
                                if matches!(k, ShapeKind::Line | ShapeKind::Arrow) && s.stroke.is_none() {
                                    s.stroke = s.fill.map(|f| f.first());
                                }
                                if matches!(k, ShapeKind::Rect | ShapeKind::Ellipse) && s.fill.is_none() && s.stroke.is_none() {
                                    s.fill = Some(Paint::Solid(Rgba::hex(0xdb2b9a)));
                                }
                                s.kind = k;
                            }
                            (Kind::Text(t), Sg::Weight) => {
                                t.weight = WEIGHTS[i.min(4)];
                                refit.push(l.id);
                            }
                            (Kind::Text(t), Sg::Align) => t.align = [TextAlign::Left, TextAlign::Center, TextAlign::Right][i.min(2)],
                            _ => {}
                        }
                    }
                });
                for id in refit {
                    self.refit_text(id);
                }
            }
        }
    }

    // ----- painting -----

    pub fn paint_panel(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let p = self.pal.clone();
        let x0 = cx.w - PANEL_W;
        let top = m::TITLE_H;
        let full = rect(x0, top, PANEL_W, cx.h - top);
        c.fill(full, p.side);
        c.vline(x0, top, full.h, p.line);
        self.hits.add(full, Hit::PanelArea);
        let pad = 16.0;
        let w = PANEL_W - 2.0 * pad;
        let tabs = rect(x0 + pad, top + 12.0, w, 32.0);
        let ti = match self.tab {
            Tab::Layer => 0,
            Tab::Adjust => 1,
            Tab::Looks => 2,
        };
        // properties: the upper part, scrolling
        let layers_h = ((cx.h - top) * 0.42).clamp(200.0, 420.0);
        let props = rect(x0, tabs.bottom() + 8.0, PANEL_W, cx.h - STATUS_H.min(0.0) - layers_h - tabs.bottom() - 8.0);
        self.panel_h = props.h;
        c.clip(props);
        let y0 = props.y + 8.0 - self.panel_scroll;
        let used = match self.tab {
            Tab::Layer => self.paint_props(c, x0 + pad, y0, w),
            Tab::Adjust => self.paint_adjust(c, x0 + pad, y0, w),
            Tab::Looks => self.paint_looks(c, x0 + pad, y0, w),
        };
        c.unclip();
        // what scrolled out of view is not clickable: the bands above and below cover it
        self.hits.add(rect(x0, top, PANEL_W, props.y - top), Hit::PanelArea);
        self.hits.add(rect(x0, props.bottom(), PANEL_W, cx.h - props.bottom()), Hit::PanelArea);
        self.segmented(c, tabs, &[tr("Layer"), tr("Adjust"), tr("Looks")], ti, Sg::Tab);
        let content = used - y0 + 16.0;
        let max = (content - props.h).max(0.0);
        if self.panel_scroll > max {
            self.panel_scroll = max;
        }
        if max > 0.0 {
            let bar_h = (props.h * props.h / content).max(24.0);
            let by = props.y + (props.h - bar_h) * (self.panel_scroll / max);
            c.fill_round(rect(cx.w - 6.0, by, 3.0, bar_h), 1.5, p.t4);
        }
        // the layers
        let ly = props.bottom();
        c.hline(x0, ly, PANEL_W, p.line);
        self.paint_layers(c, rect(x0, ly, PANEL_W, cx.h - ly));
    }

    fn paint_props(&mut self, c: &mut Canvas, x: f32, mut y: f32, w: f32) -> f32 {
        let p = self.pal.clone();
        if self.sel.len() > 1 {
            y += self.section(c, x, y, w, &fill(tr("{a} layers"), &[&self.sel.len().to_string()]));
            y = self.paint_align(c, x, y, w);
            let half = (w - 8.0) / 2.0;
            self.pill_icon(c, rect(x, y, half, 34.0), &icons::GROUP, tr("Group"), false, Hit::Act(super::Act::Group));
            self.pill_icon(c, rect(x + half + 8.0, y, half, 34.0), &icons::FLIP_H, tr("Flip"), false, Hit::Act(super::Act::FlipH));
            return y + 46.0;
        }
        let Some(l) = self.primary().cloned() else {
            y += self.section(c, x, y, w, tr("Canvas"));
            c.text(&fill(tr("{a} × {b} pixels"), &[&self.doc.w.to_string(), &self.doc.h.to_string()]), rect(x, y, w, 20.0), Font::Body, p.t2, Align::Left);
            y += 30.0;
            self.pill(c, rect(x, y, w, 34.0), tr("Change the canvas size…"), false, Hit::Act(super::Act::CanvasSize));
            y += 50.0;
            c.text_block(tr("Click a layer on the canvas or in the list below to change it."), rect(x, y, w, 0.0), Font::Body, p.t3);
            return y + 40.0;
        };
        // name
        let editing_name = self.field == Some(Field::LayerName(l.id));
        let nr = rect(x, y, w, 34.0);
        self.field_box(c, nr, if editing_name { "" } else { &l.name }, Field::LayerName(l.id));
        y += 46.0;
        match &l.kind {
            Kind::Text(t) => {
                y += 6.0;
                y += self.section(c, x, y, w, tr("Text"));
                let tb = rect(x, y, w, 76.0);
                let editing = self.field == Some(Field::LayerText(l.id));
                c.fill_round(tb, 10.0, p.fill.over(p.side));
                if editing {
                    c.stroke_round(tb, 10.0, p.accent, 1.5);
                } else {
                    if self.hovered(&Hit::Field(Field::LayerText(l.id))) {
                        c.stroke_round(tb, 10.0, p.t4, 1.0);
                    }
                    c.clip(tb.inset(8.0));
                    c.text_block(&t.text, tb.inset(10.0), Font::Body, p.t1);
                    c.unclip();
                }
                self.hits.add(tb, Hit::Field(Field::LayerText(l.id)));
                y += 86.0;
                // font
                let fr = rect(x, y, w, 34.0);
                let hot = self.hovered(&Hit::FontMenu);
                c.fill_round(fr, 10.0, if hot { p.fill2 } else { p.fill });
                c.text(&t.font, fr.inset_xy(12.0, 0.0), Font::Body, p.t1, Align::Left);
                c.icon(&ui_icons::CHEVRON_DOWN, rect(fr.right() - 26.0, fr.y + 9.0, 16.0, 16.0), p.t3);
                self.hits.add(fr, Hit::FontMenu);
                y += 44.0;
                y += self.slider(c, x, y, w, tr("Size"), Sl::TextSize);
                let wi = WEIGHTS.iter().position(|v| *v >= t.weight).unwrap_or(4);
                self.segmented(c, rect(x, y, w, 32.0), &[tr("Thin"), tr("Regular"), tr("Semi"), tr("Bold"), tr("Black")], wi, Sg::Weight);
                y += 42.0;
                let ai = match t.align {
                    TextAlign::Left => 0,
                    TextAlign::Center => 1,
                    TextAlign::Right => 2,
                };
                self.segmented_icons(c, rect(x, y, w - 92.0, 32.0), &[&icons::TA_LEFT, &icons::TA_CENTER, &icons::TA_RIGHT], ai, Sg::Align);
                self.icon_btn(c, &icons::ITALIC, rect(x + w - 84.0, y, 36.0, 32.0), Hit::Toggle(Tg::Italic), t.italic);
                self.swatch(c, rect(x + w - 40.0, y, 40.0, 32.0), t.color, Ct::Text);
                y += 44.0;
                y += self.slider(c, x, y, w, tr("Letter spacing"), Sl::TextSpacing);
                y += self.slider(c, x, y, w, tr("Line height"), Sl::TextLine);
                y += self.switch(c, rect(x, y, w - 48.0, 30.0), tr("Outline"), t.outline.is_some(), Tg::Outline);
                if let Some((oc, _)) = t.outline {
                    self.swatch(c, rect(x + w - 40.0, y - 32.0, 40.0, 28.0), oc, Ct::Outline);
                    y += self.slider(c, x, y, w, tr("Outline width"), Sl::OutlineW);
                }
                y += self.switch(c, rect(x, y, w - 48.0, 30.0), tr("Shadow"), t.shadow.is_some(), Tg::Shadow);
                if let Some((sc, ..)) = t.shadow {
                    self.swatch(c, rect(x + w - 40.0, y - 32.0, 40.0, 28.0), sc, Ct::Shadow);
                    y += self.slider(c, x, y, w, tr("Shadow softness"), Sl::ShadowBlur);
                    y += self.slider(c, x, y, w, tr("Shadow distance"), Sl::ShadowY);
                }
            }
            Kind::Shape(s) => {
                y += 6.0;
                y += self.section(c, x, y, w, tr("Shape"));
                let ki = match s.kind {
                    ShapeKind::Rect => 0,
                    ShapeKind::Ellipse => 1,
                    ShapeKind::Line => 2,
                    ShapeKind::Arrow => 3,
                };
                self.segmented_icons(c, rect(x, y, w, 32.0), &[&icons::SH_RECT, &icons::SH_ELLIPSE, &icons::SH_LINE, &icons::SH_ARROW], ki, Sg::ShapeKind);
                y += 44.0;
                let lineish = matches!(s.kind, ShapeKind::Line | ShapeKind::Arrow);
                if !lineish {
                    y += self.switch(c, rect(x, y, w - 48.0, 30.0), tr("Fill"), s.fill.is_some(), Tg::FillOn);
                    if let Some(f) = s.fill {
                        self.swatch(c, rect(x + w - 40.0, y - 32.0, 40.0, 28.0), f.first(), Ct::Fill);
                        y += self.switch(c, rect(x, y, w - 48.0, 30.0), tr("Gradient"), matches!(f, Paint::Linear(..)), Tg::Gradient);
                        if let Paint::Linear(_, b, _) = f {
                            self.swatch(c, rect(x + w - 40.0, y - 32.0, 40.0, 28.0), b, Ct::Fill2);
                            y += self.slider(c, x, y, w, tr("Gradient angle"), Sl::GradAngle);
                        }
                    }
                }
                y += self.switch(c, rect(x, y, w - 48.0, 30.0), if lineish { tr("Colour") } else { tr("Outline") }, s.stroke.is_some(), Tg::StrokeOn);
                if let Some(sc) = s.stroke {
                    self.swatch(c, rect(x + w - 40.0, y - 32.0, 40.0, 28.0), sc, Ct::Stroke);
                }
                if s.stroke.is_some() || lineish {
                    y += self.slider(c, x, y, w, tr("Thickness"), Sl::StrokeW);
                }
                if s.kind == ShapeKind::Rect {
                    y += self.slider(c, x, y, w, tr("Corner radius"), Sl::Radius);
                }
            }
            Kind::Image(_) => {
                y += 6.0;
                y += self.section(c, x, y, w, tr("Photo"));
                self.pill_icon(c, rect(x, y, w, 36.0), &icons::WAND, tr("Remove background"), true, Hit::Act(super::Act::RemoveBg));
                y += 46.0;
                c.text_block(
                    tr("Works best on a subject in front of a plain background. Not AI: it reads the colours at the edges."),
                    rect(x, y, w, 0.0),
                    Font::Small,
                    p.t3,
                );
                y += c.text_height(tr("Works best on a subject in front of a plain background. Not AI: it reads the colours at the edges."), w, Font::Small)
                    + 8.0;
            }
            Kind::Group(_) => {
                y += 6.0;
                self.pill_icon(c, rect(x, y, w, 34.0), &icons::GROUP, tr("Ungroup"), false, Hit::Act(super::Act::Ungroup));
                y += 46.0;
            }
        }
        y += 4.0;
        y += self.section(c, x, y, w, tr("Layer"));
        y += self.slider(c, x, y, w, tr("Opacity"), Sl::Opacity);
        // blend mode
        c.text(tr("Blend"), rect(x, y, 90.0, 32.0), Font::Body, p.t2, Align::Left);
        let br = rect(x + 90.0, y, w - 90.0, 32.0);
        let hot = self.hovered(&Hit::BlendMenu);
        c.fill_round(br, 10.0, if hot { p.fill2 } else { p.fill });
        c.text(tr(l.blend.label()), br.inset_xy(12.0, 0.0), Font::Body, p.t1, Align::Left);
        c.icon(&ui_icons::CHEVRON_DOWN, rect(br.right() - 26.0, br.y + 8.0, 16.0, 16.0), p.t3);
        self.hits.add(br, Hit::BlendMenu);
        y += 44.0;
        y += self.slider(c, x, y, w, tr("Rotation"), Sl::Rotation);
        let q = (w - 3.0 * 8.0) / 4.0;
        self.icon_btn(c, &icons::FLIP_H, rect(x, y, q, 34.0), Hit::Act(super::Act::FlipH), false);
        self.icon_btn(c, &icons::FLIP_V, rect(x + q + 8.0, y, q, 34.0), Hit::Act(super::Act::FlipV), false);
        self.icon_btn(c, &icons::TO_FRONT, rect(x + 2.0 * (q + 8.0), y, q, 34.0), Hit::Act(super::Act::ToTop), false);
        self.icon_btn(c, &icons::TO_BACK, rect(x + 3.0 * (q + 8.0), y, q, 34.0), Hit::Act(super::Act::ToBottom), false);
        y += 46.0;
        y = self.paint_align(c, x, y, w);
        if !l.is_group() {
            y += 10.0;
            y += self.section(c, x, y, w, tr("Mask"));
            match &l.mask {
                None => {
                    let label = if self.selection.is_some() { tr("Add a mask from the selection") } else { tr("Add a mask") };
                    self.pill_icon(c, rect(x, y, w, 34.0), &icons::MASK, label, false, Hit::Act(super::Act::MaskAdd));
                    y += 46.0;
                }
                Some(mk) => {
                    y += self.switch(c, rect(x, y, w, 30.0), tr("Mask on"), mk.on, Tg::MaskOn);
                    let half = (w - 8.0) / 2.0;
                    self.pill_icon(c, rect(x, y, half, 34.0), &icons::INVERT, tr("Invert"), false, Hit::Act(super::Act::MaskInvert));
                    self.pill_icon(c, rect(x + half + 8.0, y, half, 34.0), &ui_icons::TRASH, tr("Delete"), false, Hit::Act(super::Act::MaskDelete));
                    y += 44.0;
                    c.text_block(
                        tr("Paint on the mask with the Remove background tool (K): Erase hides, Restore shows."),
                        rect(x, y, w, 0.0),
                        Font::Small,
                        p.t3,
                    );
                    y += c.text_height(tr("Paint on the mask with the Remove background tool (K): Erase hides, Restore shows."), w, Font::Small) + 8.0;
                }
            }
        }
        y
    }

    fn paint_align(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32) -> f32 {
        let n = if self.sel.len() > 2 { 8 } else { 6 };
        let b = (w - (n - 1) as f32 * 4.0) / n as f32;
        let items: [(&plume_ui::icons::Icon, super::Act); 8] = [
            (&icons::AL_LEFT, super::Act::Align(AlignTo::Left)),
            (&icons::AL_HCENTER, super::Act::Align(AlignTo::HCenter)),
            (&icons::AL_RIGHT, super::Act::Align(AlignTo::Right)),
            (&icons::AL_TOP, super::Act::Align(AlignTo::Top)),
            (&icons::AL_VCENTER, super::Act::Align(AlignTo::VCenter)),
            (&icons::AL_BOTTOM, super::Act::Align(AlignTo::Bottom)),
            (&icons::DIST_H, super::Act::Distribute(true)),
            (&icons::DIST_V, super::Act::Distribute(false)),
        ];
        for (i, (ic, a)) in items.into_iter().take(n).enumerate() {
            self.icon_btn(c, ic, rect(x + i as f32 * (b + 4.0), y, b, 32.0), Hit::Act(a), false);
        }
        y + 42.0
    }

    fn paint_adjust(&mut self, c: &mut Canvas, x: f32, mut y: f32, w: f32) -> f32 {
        let p = self.pal.clone();
        if self.primary().is_none() {
            c.text_block(
                tr("Select a layer to adjust it. Adjustments never change the layer itself: change them or reset them at any time."),
                rect(x, y, w, 0.0),
                Font::Body,
                p.t3,
            );
            return y + 80.0;
        }
        let groups: [(&str, &[usize]); 4] = [("Tone", &[0, 1, 2]), ("Colour", &[3, 4, 5, 6]), ("Levels", &[7, 8, 9]), ("Details", &[10, 11, 12])];
        for (title, ks) in groups {
            y += self.section(c, x, y, w, tr(title));
            for k in ks {
                let (label, id) = ADJ[*k];
                y += self.slider(c, x, y, w, tr(label), Sl::Adj(id));
            }
            y += 4.0;
        }
        self.pill(c, rect(x, y, w, 34.0), tr("Reset adjustments"), false, Hit::Act(super::Act::ResetAdjust));
        y + 46.0
    }

    fn paint_looks(&mut self, c: &mut Canvas, x: f32, mut y: f32, w: f32) -> f32 {
        let p = self.pal.clone();
        let Some(l) = self.primary().cloned() else {
            c.text_block(tr("Select a layer (a photo works best) to give it a look in one click."), rect(x, y, w, 0.0), Font::Body, p.t3);
            return y + 60.0;
        };
        y += self.slider(c, x, y, w, tr("Strength"), Sl::LookAmount);
        self.ensure_looks(c, &l);
        let cols = 2;
        let tw = (w - 10.0) / cols as f32;
        let th = tw * 0.72;
        let mut all = vec![Look::None];
        all.extend(Look::ALL);
        for (i, look) in all.iter().enumerate() {
            let (cx0, cy0) = (x + (i % cols) as f32 * (tw + 10.0), y + (i / cols) as f32 * (th + 34.0));
            let r = rect(cx0, cy0, tw, th);
            let on = l.adj.look == *look;
            c.fill_round(r, 12.0, p.fill);
            if let Some((_, _, bmps)) = &self.looks
                && let Some((_, b)) = bmps.iter().find(|(lk, _)| lk == look)
            {
                let b = b.clone();
                let size = unsafe { b.GetSize() };
                let k = (r.w / size.width).max(r.h / size.height);
                let (iw, ih) = (size.width * k, size.height * k);
                c.clip(r.inset(1.0));
                draw::bitmap_at(c, &b, rect(r.x + (r.w - iw) / 2.0, r.y + (r.h - ih) / 2.0, iw, ih), 1.0, true, None);
                c.unclip();
            }
            if on {
                c.stroke_round(r, 12.0, p.accent, 2.5);
            } else if self.hovered(&Hit::Look(*look)) {
                c.stroke_round(r, 12.0, p.t3, 1.0);
            }
            c.text(
                tr(look.label()),
                rect(cx0, r.bottom() + 4.0, tw, 22.0),
                if on { Font::BodyBold } else { Font::Body },
                if on { p.ink } else { p.t2 },
                Align::Center,
            );
            self.hits.add(rect(cx0, cy0, tw, th + 26.0), Hit::Look(*look));
        }
        y + all.len().div_ceil(cols) as f32 * (th + 34.0)
    }

    /// Small previews of the document with each look on the selected layer.
    fn ensure_looks(&mut self, c: &mut Canvas, l: &Layer) {
        if self.drag.is_some() || self.looks.as_ref().is_some_and(|(v, id, _)| *v == self.ver && *id == l.id) {
            return;
        }
        let rt = c.target();
        let px = 150.0 * c.scale();
        let s = px / self.doc.w.max(self.doc.h) as f32;
        let region = IRect::new(0, 0, (self.doc.w as f32 * s).ceil() as i32, (self.doc.h as f32 * s).ceil() as i32);
        let mut out = vec![];
        let mut all = vec![Look::None];
        all.extend(Look::ALL);
        for look in all {
            let mut d = self.doc.clone();
            if let Some(t) = d.find_mut(l.id) {
                t.adj.look = look;
                t.adj.look_amount = 1.0;
            }
            let b = self.renderer.render(&d, s, region);
            // on a neutral board: see-through parts show grey
            let mut bg = b.clone();
            for p in &mut bg.px {
                let k = 1.0 - p[3];
                *p = [p[0] + 0.5 * k, p[1] + 0.5 * k, p[2] + 0.5 * k, 1.0];
            }
            if let Some(bmp) = draw::bitmap(&rt, region.w as u32, region.h as u32, &bg.to_bgra()) {
                out.push((look, bmp));
            }
        }
        self.looks = Some((self.ver, l.id, out));
    }

    // ----- the layers list -----

    /// Rows shown: (layer, depth), top layer first.
    pub fn layer_rows(&self) -> Vec<(Id, usize)> {
        fn go(v: &[Layer], depth: usize, out: &mut Vec<(Id, usize)>) {
            for l in v.iter().rev() {
                out.push((l.id, depth));
                if l.is_group() && l.open {
                    go(l.children(), depth + 1, out);
                }
            }
        }
        let mut out = vec![];
        go(&self.doc.layers, 0, &mut out);
        out
    }

    pub fn layers_list_rect(&self) -> Rect {
        let (w, h) = self.size;
        let layers_h = ((h - m::TITLE_H) * 0.42).clamp(200.0, 420.0);
        let y = h - layers_h;
        rect(w - PANEL_W, y + 48.0, PANEL_W, layers_h - 48.0)
    }

    fn paint_layers(&mut self, c: &mut Canvas, area: Rect) {
        let p = self.pal.clone();
        let x = area.x + 16.0;
        c.text(tr("Layers"), rect(x, area.y + 12.0, 100.0, 24.0), Font::Title, p.t1, Align::Left);
        let btns: [(&plume_ui::icons::Icon, super::Act); 5] = [
            (&icons::TEXT, super::Act::AddText),
            (&icons::SH_RECT, super::Act::AddShape),
            (&icons::IMAGE, super::Act::Place),
            (&ui_icons::COPY, super::Act::Duplicate),
            (&ui_icons::TRASH, super::Act::Delete),
        ];
        let mut bx = area.right() - 16.0 - 5.0 * 32.0;
        for (ic, a) in btns {
            self.icon_btn(c, ic, rect(bx, area.y + 8.0, 30.0, 30.0), Hit::Act(a), false);
            bx += 32.0;
        }
        let list = self.layers_list_rect();
        self.hits.add(list, Hit::LayersArea);
        let rows = self.layer_rows();
        let content = rows.len() as f32 * ROW;
        let max = (content - list.h + 8.0).max(0.0);
        self.layers_scroll = self.layers_scroll.clamp(0.0, max);
        c.clip(list);
        let rt = c.target();
        let mut made = 0;
        let mut pending = false;
        for (i, (id, depth)) in rows.iter().enumerate() {
            let r = rect(list.x + 8.0, list.y + i as f32 * ROW - self.layers_scroll, list.w - 16.0, ROW - 2.0);
            if r.bottom() < list.y || r.y > list.bottom() {
                continue;
            }
            let Some(l) = self.doc.find(*id).cloned() else { continue };
            let on = self.sel.contains(id);
            let hit = Hit::LayerRow(*id);
            if on {
                c.fill_round(r, 10.0, p.accent.fade(0.16));
            } else if self.hovered(&hit) {
                c.fill_round(r, 10.0, p.hover);
            }
            self.hits.add(r, hit);
            let mut cx0 = r.x + 4.0 + *depth as f32 * 16.0;
            let eye = rect(cx0, r.y + 7.0, 24.0, 24.0);
            self.icon_btn(c, if l.visible { &icons::EYE } else { &icons::EYE_OFF }, eye, Hit::LayerEye(*id), false);
            cx0 += 28.0;
            if l.is_group() {
                let chev = rect(cx0, r.y + 7.0, 24.0, 24.0);
                self.icon_btn(c, if l.open { &ui_icons::CHEVRON_DOWN } else { &ui_icons::CHEVRON_RIGHT }, chev, Hit::LayerOpen(*id), false);
                cx0 += 26.0;
            }
            // the thumbnail
            let tr_ = rect(cx0, r.y + 4.0, 30.0, 30.0);
            draw::checker(c, tr_, 5.0, p.dark);
            let fresh = self.thumbs.get(id).is_some_and(|(v, _)| *v == self.ver);
            if !fresh && self.drag.is_none() && made < 8 {
                made += 1;
                if let Some(b) = self.make_thumb(&rt, &l, c.scale()) {
                    self.thumbs.insert(*id, (self.ver, b));
                }
            } else if !fresh {
                pending = true;
            }
            if let Some((_, b)) = self.thumbs.get(id) {
                let b = b.clone();
                let size = unsafe { b.GetSize() };
                let k = (tr_.w / size.width).min(tr_.h / size.height);
                let (iw, ih) = (size.width * k, size.height * k);
                draw::bitmap_at(c, &b, rect(tr_.x + (tr_.w - iw) / 2.0, tr_.y + (tr_.h - ih) / 2.0, iw, ih), 1.0, true, None);
            }
            c.stroke_round(tr_, 4.0, p.line, 1.0);
            cx0 += 38.0;
            let right = r.right() - 8.0 - if l.locked { 26.0 } else { 0.0 } - if l.mask.is_some() { 22.0 } else { 0.0 };
            let ink = if on {
                p.ink
            } else if l.visible {
                p.t1
            } else {
                p.t3
            };
            c.text(&l.name, rect(cx0, r.y, right - cx0, r.h), if on { Font::BodyBold } else { Font::Body }, ink, Align::Left);
            let mut ix = r.right() - 8.0;
            if l.locked {
                ix -= 24.0;
                self.icon_btn(c, &icons::LOCK, rect(ix, r.y + 7.0, 24.0, 24.0), Hit::LayerLock(*id), false);
            } else if self.hovered(&Hit::LayerRow(*id)) || self.hovered(&Hit::LayerLock(*id)) {
                // the lock appears on the hovered row
                ix -= 24.0;
                let q = rect(ix, r.y + 7.0, 24.0, 24.0);
                c.icon(&icons::LOCK, q.centered(14.0, 14.0), p.t4);
                self.hits.add(q, Hit::LayerLock(*id));
            }
            if l.mask.is_some() {
                ix -= 22.0;
                c.icon(&icons::MASK, rect(ix + 3.0, r.y + 12.0, 14.0, 14.0), if l.mask.as_ref().is_some_and(|m| m.on) { p.t2 } else { p.t4 });
            }
        }
        // where a dragged layer would go
        if let Some(Drag::Layer { over: Some((_, _, ly)), moved: true, .. }) = &self.drag {
            c.fill_round(rect(list.x + 12.0, *ly - 1.5, list.w - 24.0, 3.0), 1.5, p.accent);
        }
        if rows.is_empty() {
            c.text(tr("No layers"), rect(list.x, list.y + 10.0, list.w, 30.0), Font::Body, p.t3, Align::Center);
        }
        c.unclip();
        if pending {
            self.win.repaint();
        }
    }

    fn make_thumb(
        &self,
        rt: &windows::Win32::Graphics::Direct2D::ID2D1RenderTarget,
        l: &Layer,
        scale: f32,
    ) -> Option<windows::Win32::Graphics::Direct2D::ID2D1Bitmap> {
        // framed on the layer itself (inside the canvas), so small layers still show
        let (dw, dh) = (self.doc.w as f32, self.doc.h as f32);
        let (x0, y0, x1, y1) = l.bounds().unwrap_or((0.0, 0.0, dw, dh));
        let (x0, y0, x1, y1) = (x0.max(0.0), y0.max(0.0), x1.min(dw), y1.min(dh));
        let (bw, bh) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let px = 30.0 * scale;
        let s = px / bw.max(bh);
        let region = IRect::new((x0 * s).floor() as i32, (y0 * s).floor() as i32, (bw * s).ceil().max(1.0) as i32, (bh * s).ceil().max(1.0) as i32);
        let mut solo = l.clone();
        solo.visible = true;
        let b = self.renderer.layer_alone(&solo, s, region);
        draw::bitmap(rt, region.w as u32, region.h as u32, &b.to_bgra())
    }

    /// Where a layer dragged in the list would land: (list it goes in, index, line y).
    pub fn drop_place(&self, y: f32, dragged: Id) -> Option<(Option<Id>, usize, f32)> {
        let list = self.layers_list_rect();
        let rows = self.layer_rows();
        if rows.is_empty() {
            return None;
        }
        let k = (((y - list.y + self.layers_scroll) / ROW).round().max(0.0) as usize).min(rows.len());
        let line_y = list.y + k as f32 * ROW - self.layers_scroll;
        // the row just below the line: the dragged layer goes above it (rows show the top layer first)
        let (target, above) = if k < rows.len() { (rows[k].0, true) } else { (rows[rows.len() - 1].0, false) };
        if target == dragged {
            return None;
        }
        let (parent, i) = self.doc.parent_of(target)?;
        Some((parent, if above { i + 1 } else { i }, line_y))
    }

    pub fn blend_menu_items() -> Vec<Blend> {
        Blend::ALL.to_vec()
    }
}
