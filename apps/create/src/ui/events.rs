//! What the mouse and the keyboard do: clicks routed by what is under the pointer, drags, the wheel, shortcuts,
//! and the edit box's text.

use super::{Act, App, Dialog, Drag, Field, Hit, Menu, Popup, Sl, Tab, Tool};
use crate::{
    engine::doc::{Kind, Rgba},
    i18n::tr,
};
use plume_ui::{Button, Cx, Event, Mods, Pt, rect};

const VK_BACK: u16 = 0x08;
const VK_RETURN: u16 = 0x0D;
const VK_ESCAPE: u16 = 0x1B;
const VK_DELETE: u16 = 0x2E;
const VK_LEFT: u16 = 0x25;
const VK_UP: u16 = 0x26;
const VK_RIGHT: u16 = 0x27;
const VK_DOWN: u16 = 0x28;
const VK_F1: u16 = 0x70;
const VK_OEM_4: u16 = 0xDB; // [
const VK_OEM_6: u16 = 0xDD; // ]
const VK_OEM_PLUS: u16 = 0xBB;
const VK_OEM_MINUS: u16 = 0xBD;
const VK_OEM_2: u16 = 0xBF; // / ?
const VK_OEM_7: u16 = 0xDE; // '
const VK_OEM_1: u16 = 0xBA; // ;

impl App {
    pub fn handle(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::MouseMove { p, mods } => {
                self.mouse = p;
                let hover = self.hits.at(p).cloned();
                if hover != self.hover {
                    self.hover = hover;
                }
                if self.drag.is_some() {
                    self.drag_move(p, mods, cx);
                }
                cx.repaint();
            }
            Event::MouseLeave => {
                self.hover = None;
                cx.repaint();
            }
            Event::MouseDown { p, button, clicks, mods } => {
                self.mouse = p;
                self.drag_origin = p;
                let hit = self.hits.at(p).cloned();
                self.hover = hit.clone();
                if button == Button::Middle && self.editing && self.stage().contains(p) {
                    self.drag = Some(Drag::Pan { start: p, ox: self.view.ox, oy: self.view.oy });
                    cx.capture(true);
                    return;
                }
                if button != Button::Left {
                    return;
                }
                self.pressed = hit.clone();
                cx.capture(true);
                self.down(hit, p, mods, clicks, cx);
                cx.repaint();
            }
            Event::MouseUp { p, mods, .. } => {
                self.mouse = p;
                cx.capture(false);
                let was = self.pressed.take();
                if self.drag.is_some() {
                    self.canvas_up(mods, cx);
                }
                let _ = was;
                cx.repaint();
            }
            Event::Wheel { p, dx, dy, mods } => self.wheel(p, dx, dy, mods, cx),
            Event::Key { vk, mods, .. } => self.key(vk, mods, cx),
            Event::EditChanged(s) => self.edit_changed(s, cx),
            Event::EditKey { vk, mods } => {
                if vk == VK_RETURN && !(self.field_is_multiline() && !mods.ctrl) {
                    self.field_enter(cx);
                } else if vk == VK_ESCAPE || (vk == VK_RETURN && mods.ctrl) {
                    self.end_field();
                }
                cx.repaint();
            }
            Event::EditBlur => {
                // a click on another field opens that one; anything else just closes this one
                if self.field.is_some() {
                    if matches!(
                        self.field,
                        Some(Field::Value(_))
                            | Some(Field::CustomW)
                            | Some(Field::CustomH)
                            | Some(Field::ExportW)
                            | Some(Field::ExportH)
                            | Some(Field::CanvasW)
                            | Some(Field::CanvasH)
                    ) {
                        self.field_enter(cx);
                    } else {
                        self.end_field();
                    }
                }
                cx.repaint();
            }
            Event::Resized | Event::Scale => cx.repaint(),
            Event::Active(a) => {
                self.active = a;
                cx.repaint();
            }
            Event::SystemTheme => {
                if self.set.theme == "system" {
                    self.apply_theme();
                }
                cx.repaint();
            }
            Event::Opened(line) => {
                let path = line.trim().trim_matches('"').to_string();
                let p = std::path::PathBuf::from(path);
                if p.is_file() {
                    if self.editing && crate::wic::is_picture(&p) {
                        self.place_path(&p, cx);
                    } else {
                        self.open_path(&p, cx);
                    }
                }
                self.win.bring_to_front();
            }
            Event::Close => self.set.save(),
            _ => {}
        }
    }

    fn down(&mut self, hit: Option<Hit>, p: Pt, mods: Mods, clicks: u8, cx: &mut Cx) {
        // a field open elsewhere closes (its value kept)
        if self.field.is_some() && !matches!(&hit, Some(Hit::Field(f)) if Some(f) == self.field.as_ref()) {
            if matches!(
                self.field,
                Some(Field::Value(_))
                    | Some(Field::CustomW)
                    | Some(Field::CustomH)
                    | Some(Field::ExportW)
                    | Some(Field::ExportH)
                    | Some(Field::CanvasW)
                    | Some(Field::CanvasH)
            ) {
                self.field_enter(cx);
            } else if !matches!(hit, Some(Hit::FontItem(_))) {
                self.end_field();
            }
        }
        // popups: a click inside works, outside closes them
        if self.popup.is_some() {
            let inside =
                matches!(hit, Some(Hit::Popup | Hit::MenuItem(_) | Hit::FontItem(_) | Hit::PickSv | Hit::PickHue | Hit::PickAlpha | Hit::PaletteColour(_)))
                    || matches!(&hit, Some(Hit::Field(Field::Hex | Field::FontSearch)));
            if !inside {
                self.close_popup();
                return;
            }
        }
        // a dialog takes all clicks
        if self.dialog.is_some() && !matches!(hit, Some(Hit::Min | Hit::Max | Hit::Close)) {
            let ok = matches!(
                hit,
                Some(
                    Hit::Dialog
                        | Hit::Seg(..)
                        | Hit::Slider(_)
                        | Hit::Field(_)
                        | Hit::Act(_)
                        | Hit::Popup
                        | Hit::MenuItem(_)
                        | Hit::PickSv
                        | Hit::PickHue
                        | Hit::PickAlpha
                        | Hit::PaletteColour(_)
                )
            );
            if !ok {
                return;
            }
        }
        let Some(hit) = hit else { return };
        match hit {
            Hit::Min => self.win.minimize(),
            Hit::Max => self.win.toggle_maximize(),
            Hit::Close => self.win.close(),
            Hit::Bar | Hit::Rail | Hit::PanelArea | Hit::LayersArea | Hit::Dialog | Hit::Popup | Hit::Scrim => {}
            Hit::AppMenu => self.open_app_menu(),
            Hit::Act(a) => {
                if !self.editing && self.home_click(&Hit::Act(a.clone()), cx) {
                    return;
                }
                self.act(a, cx);
            }
            Hit::Preset(_) | Hit::Template(_) | Hit::Recent(_) => {
                self.home_click(&hit, cx);
            }
            Hit::Tool(t) => self.set_tool(t),
            Hit::Slider(sl) => {
                if Self::slider_on_doc(sl) && self.field_before.is_none() {
                    self.field_before = Some(self.doc.clone());
                }
                let r = self.hits.rect_of(|h| *h == Hit::Slider(sl)).unwrap_or(rect(0.0, 0.0, 1.0, 1.0));
                // the track is 6 DIP inside the hit area on each side
                let track = rect(r.x + 6.0, r.y, r.w - 12.0, r.h);
                self.drag = Some(Drag::Slider { sl, r: track });
                self.slider_from_x(sl, track, p.x);
            }
            Hit::Swatch(ct) => {
                let anchor = self.hits.rect_of(|h| *h == Hit::Swatch(ct)).unwrap_or_default();
                self.open_colour(ct, anchor);
            }
            Hit::Toggle(tg) => self.toggle(tg),
            Hit::Seg(sg, i) => self.segment(sg, i, cx),
            Hit::Field(f) => {
                if self.field.as_ref() != Some(&f) {
                    self.open_field(f, cx);
                }
            }
            Hit::Look(l) => self.set_look(l),
            Hit::BlendMenu => {
                let anchor = self.hits.rect_of(|h| *h == Hit::BlendMenu).unwrap_or_default();
                self.popup = Some(Popup::Blend(anchor));
            }
            Hit::FontMenu => {
                let anchor = self.hits.rect_of(|h| *h == Hit::FontMenu).unwrap_or_default();
                self.popup = Some(Popup::Fonts { anchor, filter: String::new(), scroll: self.font_scroll_for_current() });
                // typing filters the list at once
                self.open_field(Field::FontSearch, cx);
            }
            Hit::LayerRow(id) => {
                if mods.ctrl {
                    if let Some(i) = self.sel.iter().position(|s| *s == id) {
                        self.sel.remove(i);
                    } else {
                        self.sel.push(id);
                    }
                } else if mods.shift && !self.sel.is_empty() {
                    let rows = self.layer_rows();
                    let a = rows.iter().position(|r| Some(r.0) == self.sel.first().copied());
                    let b = rows.iter().position(|r| r.0 == id);
                    if let (Some(a), Some(b)) = (a, b) {
                        let (lo, hi) = (a.min(b), a.max(b));
                        let first = self.sel[0];
                        self.sel = vec![first];
                        self.sel.extend(rows[lo..=hi].iter().map(|r| r.0).filter(|i| *i != first));
                    }
                } else {
                    if !self.sel.contains(&id) || self.sel.len() > 1 {
                        self.sel = vec![id];
                    }
                    if clicks >= 2 {
                        self.open_field(Field::LayerName(id), cx);
                        self.tab = Tab::Layer;
                        return;
                    }
                }
                self.drag = Some(Drag::Layer { id, start: p, over: None, moved: false });
            }
            Hit::LayerEye(id) => self.change("visibility", |d| {
                if let Some(l) = d.find_mut(id) {
                    l.visible = !l.visible;
                }
            }),
            Hit::LayerLock(id) => {
                self.change("lock", |d| {
                    if let Some(l) = d.find_mut(id) {
                        l.locked = !l.locked;
                    }
                });
            }
            Hit::LayerOpen(id) => {
                if let Some(l) = self.doc.find_mut(id) {
                    l.open = !l.open;
                }
            }
            Hit::Canvas => {
                if self.editing {
                    self.canvas_down(p, mods, clicks, cx);
                }
            }
            Hit::Handle(h) => self.handle_down(h, p),
            Hit::CropHandle(h) => {
                if let Some(c) = self.crop {
                    let (x, y) = self.to_doc(p);
                    self.drag = Some(Drag::Crop { handle: h, start: (x, y), orig: c });
                }
            }
            Hit::MenuItem(i) => self.menu_pick(i, cx),
            Hit::FontItem(i) => self.font_pick(i),
            Hit::PickSv => {
                let r = self.hits.rect_of(|h| *h == Hit::PickSv).unwrap_or_default();
                self.drag = Some(Drag::PickSv(r));
                self.pick_at(p);
            }
            Hit::PickHue => {
                let r = self.hits.rect_of(|h| *h == Hit::PickHue).unwrap_or_default();
                self.drag = Some(Drag::PickHue(r));
                self.pick_at(p);
            }
            Hit::PickAlpha => {
                let r = self.hits.rect_of(|h| *h == Hit::PickAlpha).unwrap_or_default();
                self.drag = Some(Drag::PickAlpha(r));
                self.pick_at(p);
            }
            Hit::PaletteColour(c) => self.picker_set(c),
        }
    }

    pub fn set_tool(&mut self, t: Tool) {
        if self.tool == Tool::Crop && t != Tool::Crop {
            self.crop = None;
        }
        self.tool = t;
        if t == Tool::Crop && self.crop.is_none() {
            self.start_crop();
        }
        self.note(format!("tool {t:?}"));
    }

    fn drag_move(&mut self, p: Pt, mods: Mods, cx: &mut Cx) {
        match &mut self.drag {
            Some(Drag::Slider { sl, r, .. }) => {
                let (sl, r) = (*sl, *r);
                self.slider_from_x(sl, r, p.x);
            }
            Some(Drag::Layer { id, start, moved, .. }) => {
                if !*moved && (p.y - start.y).abs() < 4.0 {
                    return;
                }
                *moved = true;
                let id = *id;
                let place = self.drop_place(p.y, id);
                if let Some(Drag::Layer { over, .. }) = &mut self.drag {
                    *over = place;
                }
            }
            Some(Drag::PickSv(_) | Drag::PickHue(_) | Drag::PickAlpha(_)) => self.pick_at(p),
            Some(_) => self.canvas_move(p, mods),
            None => {}
        }
        cx.repaint();
    }

    pub fn slider_from_x(&mut self, sl: Sl, track: plume_ui::Rect, x: f32) {
        let (_, lo, hi) = self.slider_get(sl);
        let t = ((x - track.x) / track.w.max(1.0)).clamp(0.0, 1.0);
        let mut v = lo + (hi - lo) * t;
        // whole numbers, except the small ranges
        if hi - lo > 10.0 {
            v = v.round();
        } else {
            v = (v * 100.0).round() / 100.0;
        }
        self.slider_set(sl, v);
    }

    fn wheel(&mut self, p: Pt, dx: f32, dy: f32, mods: Mods, cx: &mut Cx) {
        if let Some(Popup::Fonts { scroll, .. }) = &mut self.popup {
            *scroll = (*scroll - dy * 96.0).max(0.0);
            cx.repaint();
            return;
        }
        if self.dialog.is_some() {
            return;
        }
        let hit = self.hits.at(p).cloned();
        match hit {
            Some(Hit::Canvas | Hit::Handle(_) | Hit::CropHandle(_)) if self.editing => {
                if mods.ctrl {
                    let k = 1.15f32.powf(dy);
                    self.zoom_to(self.view.zoom * k, Some(p));
                } else if mods.shift {
                    self.view.ox += dy * 60.0;
                } else {
                    self.view.ox += dx * 60.0;
                    self.view.oy += dy * 60.0;
                }
            }
            Some(Hit::LayersArea | Hit::LayerRow(_) | Hit::LayerEye(_) | Hit::LayerLock(_) | Hit::LayerOpen(_)) => {
                self.layers_scroll = (self.layers_scroll - dy * 60.0).max(0.0);
            }
            Some(_) if self.editing && p.x > self.size.0 - super::PANEL_W => {
                self.panel_scroll = (self.panel_scroll - dy * 60.0).max(0.0);
            }
            _ => {}
        }
        cx.repaint();
    }

    fn key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) {
        cx.repaint();
        if vk == VK_ESCAPE {
            if self.popup.is_some() {
                self.close_popup();
            } else if self.dialog.is_some() {
                self.dialog = None;
            } else if self.crop.is_some() {
                self.act(Act::CropCancel, cx);
            } else if self.selection.is_some() {
                self.selection = None;
            } else if self.drag.is_some() {
                self.drag = None;
            } else {
                self.sel.clear();
            }
            cx.handled();
            return;
        }
        if self.dialog.is_some() {
            if vk == VK_RETURN {
                match self.dialog {
                    Some(Dialog::Export { .. }) => self.act(Act::ExportNow, cx),
                    Some(Dialog::Custom { .. } | Dialog::Canvas { .. }) => self.act(Act::StartCustom, cx),
                    _ => self.dialog = None,
                }
            }
            return;
        }
        let ch = char::from_u32(vk as u32).unwrap_or('\0');
        if mods.ctrl {
            match (ch, mods.shift) {
                ('Z', false) => self.act(Act::Undo, cx),
                ('Z', true) | ('Y', _) => self.act(Act::Redo, cx),
                ('S', false) => self.act(Act::Save, cx),
                ('S', true) => self.act(Act::SaveAs, cx),
                ('O', _) => self.act(Act::Open, cx),
                ('N', _) => self.act(Act::Home, cx),
                ('E', _) if self.editing => self.act(Act::Export, cx),
                ('J', _) if self.editing => self.act(Act::Duplicate, cx),
                ('G', false) if self.editing => self.act(Act::Group, cx),
                ('G', true) if self.editing => self.act(Act::Ungroup, cx),
                ('A', _) if self.editing => self.act(Act::SelectAll, cx),
                ('D', _) if self.editing => self.act(Act::Deselect, cx),
                ('I', true) if self.editing => self.act(Act::InvertSel, cx),
                ('R', _) if self.editing => self.toggle(super::Tg::Rulers),
                ('0', _) if self.editing => self.act(Act::Fit, cx),
                ('1', _) if self.editing => self.act(Act::Actual, cx),
                _ => match vk {
                    VK_OEM_PLUS if self.editing => self.act(Act::ZoomIn, cx),
                    VK_OEM_MINUS if self.editing => self.act(Act::ZoomOut, cx),
                    VK_OEM_6 if self.editing => self.step_layer(true),
                    VK_OEM_4 if self.editing => self.step_layer(false),
                    VK_OEM_7 if self.editing => self.toggle(super::Tg::Grid),
                    VK_OEM_1 if self.editing => self.toggle(super::Tg::Snap),
                    _ => return,
                },
            }
            cx.handled();
            return;
        }
        if !self.editing {
            if vk == VK_F1 {
                self.act(Act::Shortcuts, cx);
            }
            return;
        }
        match vk {
            VK_DELETE | VK_BACK => self.act(Act::Delete, cx),
            VK_RETURN if self.crop.is_some() => self.act(Act::CropApply, cx),
            VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN => {
                let d = if mods.shift { 10.0 } else { 1.0 };
                let (dx, dy) = match vk {
                    VK_LEFT => (-d, 0.0),
                    VK_RIGHT => (d, 0.0),
                    VK_UP => (0.0, -d),
                    _ => (0.0, d),
                };
                let ids: Vec<_> = self.sel.clone();
                if !ids.is_empty() {
                    self.change("nudge", |doc| {
                        for id in ids {
                            if let Some(l) = doc.find_mut(id)
                                && !l.locked
                            {
                                l.translate(dx, dy);
                            }
                        }
                    });
                }
            }
            VK_OEM_4 => self.set.brush_size = (self.set.brush_size / 1.2).max(1.0).round(),
            VK_OEM_6 => self.set.brush_size = (self.set.brush_size * 1.2).min(500.0).round(),
            VK_F1 | VK_OEM_2 => self.act(Act::Shortcuts, cx),
            _ => {
                let tool = match ch {
                    'V' => Some(Tool::Move),
                    'M' => Some(Tool::Select),
                    'L' => {
                        self.sel_mode = super::SelMode::Lasso;
                        Some(Tool::Select)
                    }
                    'C' => Some(Tool::Crop),
                    'B' => Some(Tool::Brush),
                    'E' => Some(Tool::Eraser),
                    'G' => Some(Tool::Fill),
                    'I' => Some(Tool::Picker),
                    'T' => Some(Tool::Text),
                    'U' => Some(Tool::Shape),
                    'S' => Some(Tool::Clone),
                    'K' => Some(Tool::Cutout),
                    'H' => Some(Tool::Hand),
                    'X' => {
                        self.act(Act::SwapColours, cx);
                        None
                    }
                    _ => None,
                };
                if let Some(t) = tool {
                    self.set_tool(t);
                }
            }
        }
    }

    fn step_layer(&mut self, up: bool) {
        let Some(id) = self.sel.first().copied() else { return };
        self.change("order", |d| {
            crate::engine::ops::step(d, id, up);
        });
    }

    fn open_app_menu(&mut self) {
        let anchor = self.hits.rect_of(|h| *h == Hit::AppMenu).unwrap_or_default();
        let mut items: Vec<(String, bool, Act)> = vec![];
        if self.editing {
            items.push((tr("New design or photo…").into(), false, Act::Home));
        }
        items.push((tr("Open…").into(), false, Act::Open));
        if self.editing {
            items.push((tr("Save project").into(), false, Act::Save));
            items.push((tr("Save project as…").into(), false, Act::SaveAs));
            items.push((tr("Export…").into(), false, Act::Export));
            items.push((tr("Place a photo…").into(), false, Act::Place));
            items.push((tr("Canvas size…").into(), false, Act::CanvasSize));
        }
        items.push((tr("Keyboard shortcuts").into(), false, Act::Shortcuts));
        items.push((tr("Settings…").into(), false, Act::Settings));
        self.popup = Some(Popup::Menu(Menu { items, anchor }));
    }

    fn menu_pick(&mut self, i: usize, cx: &mut Cx) {
        match self.popup.take() {
            Some(Popup::Menu(m)) => {
                if let Some((_, _, a)) = m.items.get(i).cloned() {
                    self.act(a, cx);
                }
            }
            Some(Popup::Blend(_)) => {
                let modes = Self::blend_menu_items();
                if let Some(b) = modes.get(i).copied() {
                    let ids = self.sel.clone();
                    self.change("blend", |d| {
                        for id in ids {
                            if let Some(l) = d.find_mut(id) {
                                l.blend = b;
                            }
                        }
                    });
                }
            }
            other => self.popup = other,
        }
    }

    pub fn close_popup(&mut self) {
        if matches!(self.popup, Some(Popup::Colour { .. })) {
            // the colour chosen is remembered
            if let Some(Popup::Colour { target, .. }) = &self.popup {
                let c = self.colour_get(*target);
                self.set.add_colour(c.to_hex());
            }
            self.settle();
        }
        if matches!(self.field, Some(Field::Hex | Field::FontSearch)) {
            self.field = None;
            self.win.edit_hide();
        }
        self.popup = None;
    }

    // ----- the edit box -----

    pub fn field_is_multiline(&self) -> bool {
        matches!(self.field, Some(Field::LayerText(_)))
    }

    /// The text a field starts with.
    pub fn field_text(&self, f: &Field) -> String {
        match f {
            Field::LayerText(id) => self.doc.find(*id).and_then(|l| if let Kind::Text(t) = &l.kind { Some(t.text.clone()) } else { None }).unwrap_or_default(),
            Field::LayerName(id) => self.doc.find(*id).map(|l| l.name.clone()).unwrap_or_default(),
            Field::Hex => match &self.popup {
                Some(Popup::Colour { target, .. }) => self.colour_get(*target).to_hex(),
                _ => String::new(),
            },
            Field::Value(sl) => {
                let (v, ..) = self.slider_get(*sl);
                if matches!(sl, Sl::TextLine | Sl::Adj(9)) { format!("{v:.2}") } else { format!("{}", v.round()) }
            }
            Field::FontSearch => match &self.popup {
                Some(Popup::Fonts { filter, .. }) => filter.clone(),
                _ => String::new(),
            },
            Field::CustomW | Field::ExportW | Field::CanvasW => match &self.dialog {
                Some(Dialog::Custom { w, .. } | Dialog::Export { w, .. } | Dialog::Canvas { w, .. }) => w.to_string(),
                _ => String::new(),
            },
            Field::CustomH | Field::ExportH | Field::CanvasH => match &self.dialog {
                Some(Dialog::Custom { h, .. } | Dialog::Export { h, .. } | Dialog::Canvas { h, .. }) => h.to_string(),
                _ => String::new(),
            },
        }
    }

    /// Opens Windows' edit box over the field's place (once its place is known: after painting).
    pub fn place_field(&mut self, c: &mut plume_ui::Canvas) {
        let Some(f) = self.field.clone() else { return };
        let Some(r) = self.hits.rect_of(|h| *h == Hit::Field(f.clone())) else {
            // the field is no longer on screen
            self.end_field();
            return;
        };
        let _ = c;
        let multi = self.field_is_multiline();
        let inner = if matches!(f, Field::Value(_)) {
            r.inset_xy(4.0, 1.0)
        } else if multi {
            r.inset_xy(10.0, 8.0)
        } else {
            r.inset_xy(10.0, 7.0)
        };
        if self.win.edit_visible() {
            self.win.edit_place(inner);
            return;
        }
        let text = self.field_text(&f);
        let p = self.pal.clone();
        let bg = if matches!(f, Field::Value(_)) {
            p.fill.over(p.side)
        } else {
            p.fill.over(if self.dialog.is_some() || self.popup.is_some() { p.solid } else { p.side })
        };
        let len = text.encode_utf16().count();
        let style = plume_ui::EditStyle {
            fg: p.t1,
            bg,
            size: 13.0,
            select: Some((0, len)),
            placeholder: match f {
                Field::FontSearch => tr("Search fonts").into(),
                Field::LayerText(_) => tr("Type your text").into(),
                _ => String::new(),
            },
            password: false,
            multiline: multi,
        };
        self.win.edit_show(inner, &text, &style);
    }

    fn edit_changed(&mut self, s: String, cx: &mut Cx) {
        let Some(f) = self.field.clone() else { return };
        match f {
            Field::LayerText(id) => {
                if self.field_before.is_none() {
                    self.field_before = Some(self.doc.clone());
                }
                if let Some(l) = self.doc.find_mut(id)
                    && let Kind::Text(t) = &mut l.kind
                {
                    // a layer named after its words keeps following them
                    let auto = l.name == t.text.lines().next().unwrap_or("").chars().take(24).collect::<String>();
                    t.text = s.replace("\r\n", "\n");
                    if auto {
                        l.name = t.text.lines().next().unwrap_or("").chars().take(24).collect();
                    }
                }
                self.refit_text(id);
                self.touch();
            }
            Field::LayerName(id) => {
                if self.field_before.is_none() {
                    self.field_before = Some(self.doc.clone());
                }
                if let Some(l) = self.doc.find_mut(id) {
                    l.name = s;
                }
            }
            Field::Hex => {
                if let Some(c) = Rgba::parse(&s) {
                    let a = match &self.popup {
                        Some(Popup::Colour { a, .. }) => (*a * 255.0).round() as u8,
                        _ => 255,
                    };
                    self.picker_set(c.with_alpha(a));
                }
            }
            Field::FontSearch => {
                if let Some(Popup::Fonts { filter, scroll, .. }) = &mut self.popup {
                    *filter = s;
                    *scroll = 0.0;
                }
            }
            _ => {}
        }
        cx.repaint();
    }

    /// Enter in a field: its value is taken.
    fn field_enter(&mut self, cx: &mut Cx) {
        let Some(f) = self.field.clone() else { return };
        let text = self.win.edit_text().unwrap_or_default();
        let num = text.trim().replace(',', ".").trim_end_matches(['%', '°', ' ', 'p', 'x']).parse::<f32>().ok();
        match f {
            Field::Value(sl) => {
                if let Some(v) = num {
                    if Self::slider_on_doc(sl) && self.field_before.is_none() {
                        self.field_before = Some(self.doc.clone());
                    }
                    self.slider_set(sl, v);
                }
            }
            Field::CustomW | Field::CustomH | Field::ExportW | Field::ExportH | Field::CanvasW | Field::CanvasH => {
                if let Some(v) = num {
                    let v = (v.round() as u32).clamp(1, 16000);
                    let (dw, dh) = (self.doc.w as f32, self.doc.h as f32);
                    match (&mut self.dialog, &f) {
                        (Some(Dialog::Custom { w, .. } | Dialog::Canvas { w, .. }), Field::CustomW | Field::CanvasW) => *w = v,
                        (Some(Dialog::Custom { h, .. } | Dialog::Canvas { h, .. }), Field::CustomH | Field::CanvasH) => *h = v,
                        // export keeps the proportions
                        (Some(Dialog::Export { w, h, .. }), Field::ExportW) => {
                            *w = v;
                            *h = ((v as f32 * dh / dw).round() as u32).max(1);
                        }
                        (Some(Dialog::Export { w, h, .. }), Field::ExportH) => {
                            *h = v;
                            *w = ((v as f32 * dw / dh).round() as u32).max(1);
                        }
                        _ => {}
                    }
                }
            }
            Field::Hex => {
                if let Some(c) = Rgba::parse(&text) {
                    self.picker_set(c);
                }
            }
            Field::FontSearch => {
                // Enter takes the first font that matches
                self.end_field();
                if !self.font_list().is_empty() {
                    self.font_pick(0);
                }
                return;
            }
            _ => {}
        }
        self.end_field();
        self.settle();
        cx.repaint();
    }

    fn font_scroll_for_current(&self) -> f32 {
        let cur = self.primary().and_then(|l| if let Kind::Text(t) = &l.kind { Some(t.font.clone()) } else { None }).unwrap_or_default();
        let i = self.fonts.iter().position(|f| *f == cur).unwrap_or(0);
        (i as f32 * 32.0 - 120.0).max(0.0)
    }

    /// The fonts the list shows (filtered by the search).
    pub fn font_list(&self) -> Vec<String> {
        let filter = match &self.popup {
            Some(Popup::Fonts { filter, .. }) => filter.to_lowercase(),
            _ => String::new(),
        };
        self.fonts.iter().filter(|f| filter.is_empty() || f.to_lowercase().contains(&filter)).cloned().collect()
    }

    fn font_pick(&mut self, i: usize) {
        let Some(name) = self.font_list().get(i).cloned() else { return };
        let ids = self.sel.clone();
        self.change("font", |d| {
            for id in ids {
                if let Some(l) = d.find_mut(id)
                    && let Kind::Text(t) = &mut l.kind
                {
                    t.font = name.clone();
                }
            }
        });
        for id in self.sel.clone() {
            self.refit_text(id);
        }
        self.close_popup();
    }
}
