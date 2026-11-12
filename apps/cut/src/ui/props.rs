//! The right panel: what the selected clip can change (picture, colour, sound, speed, text, style, motion,
//! transition), or the project's own settings when nothing is selected. Values change with sliders (double-click
//! puts one back), segmented controls, switches and swatches.

use super::{App, Hit, Prop, SWATCHES, Tab, draw, draw::ic, swatch_color};
use crate::{
    i18n::{fill, tr},
    model::{self, Anim, Clip, Id, MediaKind, Project, TrackKind, clock},
    render::FONTS,
};
use plume_ui::{Align, Canvas, Color, Rect, rect};

pub const SPEEDS: [f64; 6] = [0.25, 0.5, 1.0, 1.5, 2.0, 3.0];
pub const ASPECTS: [[u32; 2]; 4] = [[16, 9], [9, 16], [1, 1], [4, 5]];
pub const RATES: [u32; 4] = [24, 25, 30, 60];

impl Prop {
    /// (lowest, highest, default)
    pub fn range(self) -> (f64, f64, f64) {
        match self {
            Prop::Scale => (0.1, 4.0, 1.0),
            Prop::X | Prop::Y => (-1.0, 1.0, 0.0),
            Prop::Rotation => (-180.0, 180.0, 0.0),
            Prop::Opacity => (0.0, 1.0, 1.0),
            Prop::CropL | Prop::CropT | Prop::CropR | Prop::CropB => (0.0, 0.9, 0.0),
            Prop::Volume => (0.0, 2.0, 1.0),
            Prop::FadeIn | Prop::FadeOut => (0.0, 5.0, 0.0),
            Prop::Speed => (0.25, 4.0, 1.0),
            Prop::Brightness => (-0.5, 0.5, 0.0),
            Prop::Contrast | Prop::Saturation => (0.0, 2.0, 1.0),
            Prop::TextSize => (0.02, 0.3, 0.08),
            Prop::BgOpacity => (0.0, 1.0, 0.0),
            Prop::AnimLen => (0.1, 2.0, 0.4),
            Prop::TransLen => (0.1, 3.0, 0.5),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Prop::Scale => tr("Size"),
            Prop::X => tr("Left / right"),
            Prop::Y => tr("Up / down"),
            Prop::Rotation => tr("Rotation"),
            Prop::Opacity => tr("Opacity"),
            Prop::CropL => tr("Crop left"),
            Prop::CropT => tr("Crop top"),
            Prop::CropR => tr("Crop right"),
            Prop::CropB => tr("Crop bottom"),
            Prop::Volume => tr("Volume"),
            Prop::FadeIn => tr("Fade in"),
            Prop::FadeOut => tr("Fade out"),
            Prop::Speed => tr("Speed"),
            Prop::Brightness => tr("Brightness"),
            Prop::Contrast => tr("Contrast"),
            Prop::Saturation => tr("Saturation"),
            Prop::TextSize => tr("Size"),
            Prop::BgOpacity => tr("Box opacity"),
            Prop::AnimLen => tr("Animation length"),
            Prop::TransLen => tr("Transition length"),
        }
    }

    pub fn show(self, v: f64) -> String {
        match self {
            Prop::Rotation => format!("{v:.0}°"),
            Prop::FadeIn | Prop::FadeOut | Prop::AnimLen | Prop::TransLen => format!("{v:.1} s"),
            Prop::Speed => format!("{v:.2}×"),
            Prop::Brightness => format!("{:+.0}", v * 200.0),
            Prop::TextSize => format!("{:.0}", v * 100.0),
            _ => format!("{:.0} %", v * 100.0),
        }
    }
}

/// A clip's value of a slider.
pub fn get(p: &Project, c: &Clip, prop: Prop) -> f64 {
    let st = c.text.clone().unwrap_or_default();
    match prop {
        Prop::Scale => c.scale,
        Prop::X => c.x,
        Prop::Y => c.y,
        Prop::Rotation => c.rotation,
        Prop::Opacity => c.opacity,
        Prop::CropL => c.crop[0],
        Prop::CropT => c.crop[1],
        Prop::CropR => c.crop[2],
        Prop::CropB => c.crop[3],
        Prop::Volume => c.volume,
        Prop::FadeIn => c.fade_in,
        Prop::FadeOut => c.fade_out,
        Prop::Speed => c.speed,
        Prop::Brightness => c.brightness,
        Prop::Contrast => c.contrast,
        Prop::Saturation => c.saturation,
        Prop::TextSize => st.size,
        Prop::BgOpacity => st.bg_opacity,
        Prop::AnimLen => st.anim_len,
        Prop::TransLen => c.trans.map_or(0.5, |t| t.len as f64 / p.fps()),
    }
}

/// Sets a clip's value (kept in its range).
pub fn set(p: &mut Project, id: Id, prop: Prop, v: f64) {
    let (lo, hi, _) = prop.range();
    let v = v.clamp(lo, hi);
    if prop == Prop::Speed {
        p.set_speed(id, v);
        return;
    }
    if prop == Prop::TransLen {
        let fps = p.fps();
        if let Some(t) = p.clip(id).and_then(|c| c.trans) {
            let _ = p.set_transition(id, Some(model::Transition { kind: t.kind, len: (v * fps).round() as i64 }));
        }
        return;
    }
    let Some(c) = p.clip_mut(id) else { return };
    match prop {
        Prop::Scale => c.scale = v,
        Prop::X => c.x = v,
        Prop::Y => c.y = v,
        Prop::Rotation => c.rotation = v,
        Prop::Opacity => c.opacity = v,
        Prop::CropL => c.crop[0] = v,
        Prop::CropT => c.crop[1] = v,
        Prop::CropR => c.crop[2] = v,
        Prop::CropB => c.crop[3] = v,
        Prop::Volume => c.volume = v,
        Prop::FadeIn => c.fade_in = v,
        Prop::FadeOut => c.fade_out = v,
        Prop::Brightness => c.brightness = v,
        Prop::Contrast => c.contrast = v,
        Prop::Saturation => c.saturation = v,
        Prop::TextSize | Prop::BgOpacity | Prop::AnimLen => {
            if let Some(st) = c.text.as_mut() {
                match prop {
                    Prop::TextSize => st.size = v,
                    Prop::BgOpacity => st.bg_opacity = v,
                    _ => st.anim_len = v,
                }
            }
        }
        Prop::Speed | Prop::TransLen => {}
    }
}

/// The tabs a clip shows.
pub fn tabs_of(p: &Project, c: &Clip) -> Vec<Tab> {
    if c.text.is_some() {
        return vec![Tab::Text, Tab::Style, Tab::Motion];
    }
    match p.media_of(c) {
        Some(m) if m.kind == MediaKind::Audio => vec![Tab::Sound, Tab::Speed],
        Some(m) => {
            let mut v = vec![Tab::Picture, Tab::Colour];
            if m.has_audio {
                v.push(Tab::Sound);
            }
            if m.kind == MediaKind::Video {
                v.push(Tab::Speed);
            }
            v
        }
        None => vec![],
    }
}

fn tab_label(t: Tab) -> &'static str {
    match t {
        Tab::Picture => tr("Picture"),
        Tab::Sound => tr("Sound"),
        Tab::Speed => tr("Speed"),
        Tab::Colour => tr("Colour"),
        Tab::Text => tr("Text"),
        Tab::Style => tr("Style"),
        Tab::Motion => tr("Motion"),
    }
}

impl App {
    pub fn paint_props(&mut self, c: &mut Canvas, r: Rect, fields: &mut Vec<(&'static str, Rect)>) {
        self.card(c, r);
        self.slider_rects.clear();
        let inner = r.inset(16.0);
        c.clip(rect(r.x, r.y + 1.0, r.w, r.h - 2.0));
        let top = inner.y - self.props_scroll;
        let end = match (self.sel().cloned(), self.selected.len()) {
            (Some(clip), _) => self.paint_clip_props(c, inner, top, &clip, fields),
            (None, 0) => self.paint_project_props(c, inner, top, fields),
            (None, n) => {
                let p = self.pal.clone();
                draw::text(c, &fill(tr("{} clips selected"), &[&n.to_string()]), rect(inner.x, top, inner.w, 26.0), 17.0, 600, p.t1, Align::Left);
                draw::text_wrap(
                    c,
                    tr("Drag them together on the timeline, or delete them."),
                    rect(inner.x, top + 34.0, inner.w, 40.0),
                    13.0,
                    400,
                    p.t3,
                    Align::Left,
                );
                self.pill(c, rect(inner.x, top + 84.0, inner.w, 34.0), tr("Delete"), false, Hit::Tool("delete"), true);
                self.pill(c, rect(inner.x, top + 126.0, inner.w, 34.0), tr("Delete and close the gaps"), false, Hit::Tool("ripple"), true);
                top + 170.0
            }
        };
        c.unclip();
        self.props_h = (end + self.props_scroll - inner.bottom()).max(0.0);
    }

    /// The panel's title row at the top of `r` (only its x, y and width count).
    fn header(&mut self, c: &mut Canvas, r: Rect, icon: &plume_ui::icons::Icon, title: &str, sub: &str) -> f32 {
        let (x, y, w) = (r.x, r.y, r.w);
        let p = self.pal.clone();
        c.fill_round(rect(x, y, 36.0, 36.0), 11.0, p.accent.fade(0.16));
        c.icon(icon, rect(x + 9.0, y + 9.0, 18.0, 18.0), p.accent);
        draw::text(c, title, rect(x + 48.0, y - 1.0, w - 48.0, 20.0), 15.0, 600, p.t1, Align::Left);
        draw::text(c, sub, rect(x + 48.0, y + 19.0, w - 48.0, 18.0), 12.0, 400, p.t3, Align::Left);
        y + 52.0
    }

    /// A slider row: label and value above, the track below. Returns the y under it.
    fn slider(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, prop: Prop, v: f64) -> f32 {
        let p = self.pal.clone();
        draw::text(c, prop.label(), rect(x, y, w - 70.0, 18.0), 12.0, 600, p.t2, Align::Left);
        draw::text(c, &prop.show(v), rect(x + w - 80.0, y, 80.0, 18.0), 12.0, 400, p.t3, Align::Right);
        let (lo, hi, def) = prop.range();
        let track = rect(x + 8.0, y + 30.0, w - 16.0, 4.0);
        let k = ((v - lo) / (hi - lo)).clamp(0.0, 1.0) as f32;
        let kd = ((def - lo) / (hi - lo)).clamp(0.0, 1.0) as f32;
        c.fill_round(track, 2.0, p.fill2);
        let (a, b) = if k >= kd { (kd, k) } else { (k, kd) };
        c.fill_round(rect(track.x + a * track.w, track.y, (b - a) * track.w, 4.0), 2.0, p.accent);
        let hit = Hit::Slider(prop);
        let knob = rect(track.x + k * track.w - 8.0, track.y - 6.0, 16.0, 16.0);
        c.fill_round(knob, 8.0, Color::hex(0xffffff));
        c.stroke_round(knob, 8.0, if self.hovered(&hit) { p.accent } else { Color::hexa(0x000000, 0.25) }, 1.0);
        self.hits.add(rect(x, y + 20.0, w, 24.0), hit);
        self.slider_rects.insert(prop, track);
        y + 52.0
    }

    fn sliders(&mut self, c: &mut Canvas, x: f32, mut y: f32, w: f32, clip: &Clip, props: &[Prop]) -> f32 {
        for pr in props {
            let v = get(&self.project, clip, *pr);
            y = self.slider(c, x, y, w, *pr, v);
        }
        y
    }

    fn section(&self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str) -> f32 {
        c.hline(x, y + 4.0, w, self.pal.line);
        draw::text(c, title, rect(x, y + 14.0, w, 20.0), 13.0, 600, self.pal.t1, Align::Left);
        y + 44.0
    }

    fn paint_clip_props(&mut self, c: &mut Canvas, r: Rect, top: f32, clip: &Clip, fields: &mut Vec<(&'static str, Rect)>) -> f32 {
        let p = self.pal.clone();
        let (x, w) = (r.x, r.w);
        let fps = self.project.fps();
        let media = self.project.media_of(clip).cloned();
        let track = self.project.track_of(clip.id).map(|t| t.kind).unwrap_or_default();
        let (icon, title) = match (&clip.text, &media) {
            (Some(_), _) => (&ic::TEXT, tr("Text").to_string()),
            (None, Some(m)) => (
                match m.kind {
                    MediaKind::Audio => &ic::NOTE,
                    MediaKind::Image => &ic::IMAGE,
                    MediaKind::Video => &ic::FILM,
                },
                m.name(),
            ),
            _ => (&ic::FILM, String::new()),
        };
        let sub = format!("{} · {}", super::track_label(track), clock(clip.len as f64 / fps));
        let mut y = self.header(c, rect(x, top, w, 0.0), icon, &title, &sub);
        let tabs = tabs_of(&self.project, clip);
        if !tabs.contains(&self.tab) {
            self.tab = tabs.first().copied().unwrap_or(Tab::Picture);
        }
        let labels: Vec<&str> = tabs.iter().map(|t| tab_label(*t)).collect();
        let cur = tabs.iter().position(|t| *t == self.tab).unwrap_or(0);
        self.segmented(c, rect(x, y, w, 32.0), &labels, cur, "tab");
        y += 48.0;
        let st = clip.text.clone().unwrap_or_default();
        match self.tab {
            Tab::Picture => {
                y = self.sliders(c, x, y, w, clip, &[Prop::Scale, Prop::X, Prop::Y, Prop::Rotation, Prop::Opacity]);
                let bw = (w - 16.0) / 3.0;
                self.pill(c, rect(x, y, bw, 32.0), tr("Fit"), false, Hit::Action("fit"), true);
                self.pill(c, rect(x + bw + 8.0, y, bw, 32.0), tr("Fill"), false, Hit::Action("fill"), true);
                self.pill(c, rect(x + 2.0 * (bw + 8.0), y, bw, 32.0), tr("Reset"), false, Hit::Action("reset_pos"), true);
                y += 48.0;
                y = self.section(c, x, y, w, tr("Crop"));
                y = self.sliders(c, x, y, w, clip, &[Prop::CropL, Prop::CropR, Prop::CropT, Prop::CropB]);
                if matches!(track, TrackKind::Video | TrackKind::Overlay) {
                    y = self.trans_section(c, x, y, w, clip);
                }
            }
            Tab::Colour => {
                y = self.sliders(c, x, y, w, clip, &[Prop::Brightness, Prop::Contrast, Prop::Saturation]);
                self.pill(c, rect(x, y, 120.0, 32.0), tr("Reset"), false, Hit::Action("reset_color"), true);
                y += 48.0;
            }
            Tab::Sound => {
                y = self.sliders(c, x, y, w, clip, &[Prop::Volume, Prop::FadeIn, Prop::FadeOut]);
                if track == TrackKind::Music {
                    draw::text(c, tr("Lower the music under voices"), rect(x, y, w - 56.0, 24.0), 13.0, 400, p.t1, Align::Left);
                    self.switch(c, x + w - 40.0, y, self.project.duck, Hit::Toggle("duck"));
                    y += 30.0;
                    y +=
                        draw::text_wrap(c, tr("While other sounds play, the music gets quieter by itself."), rect(x, y, w, 40.0), 12.0, 400, p.t3, Align::Left);
                    y += 12.0;
                }
            }
            Tab::Speed => {
                y = self.sliders(c, x, y, w, clip, &[Prop::Speed]);
                let labels: Vec<String> = SPEEDS.iter().map(|s| format!("{s}×")).collect();
                let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
                let cur = SPEEDS.iter().position(|s| (s - clip.speed).abs() < 1e-3).unwrap_or(usize::MAX);
                self.segmented(c, rect(x, y, w, 32.0), &refs, cur, "speed");
                y += 44.0;
                y += draw::text_wrap(
                    c,
                    tr("The clip keeps the same moment of the video: faster makes it shorter."),
                    rect(x, y, w, 40.0),
                    12.0,
                    400,
                    p.t3,
                    Align::Left,
                );
                y += 12.0;
            }
            Tab::Text => {
                draw::text(c, tr("Text"), rect(x, y, w, 18.0), 12.0, 600, p.t2, Align::Left);
                y += 24.0;
                let field = rect(x, y, w, 88.0);
                c.fill_round(field, 10.0, p.fill2);
                if self.edit_for == Some("text") {
                    c.stroke_round(field, 10.0, p.accent, 1.5);
                } else {
                    draw::text_wrap(c, &st.text, field.inset_xy(10.0, 8.0), 13.0, 400, p.t1, Align::Left);
                    self.hits.add(field, Hit::Field("text"));
                }
                fields.push(("text", field));
                y += 100.0;
                draw::text(c, tr("Font"), rect(x, y, w, 18.0), 12.0, 600, p.t2, Align::Left);
                y += 24.0;
                // two rows of four
                for row in 0..2 {
                    let names: Vec<&str> = FONTS[row * 4..row * 4 + 4]
                        .iter()
                        .map(|f| {
                            if *f == "Comic Sans MS" {
                                "Comic"
                            } else if *f == "Times New Roman" {
                                "Times"
                            } else {
                                f
                            }
                        })
                        .collect();
                    let cur = FONTS.iter().position(|f| *f == st.font).filter(|i| i / 4 == row).map_or(usize::MAX, |i| i % 4);
                    self.segmented(c, rect(x, y, w, 32.0), &names, cur, if row == 0 { "font0" } else { "font1" });
                    y += 38.0;
                }
                y += 6.0;
                draw::text(c, tr("Bold"), rect(x, y, w - 56.0, 24.0), 13.0, 400, p.t1, Align::Left);
                self.switch(c, x + w - 40.0, y, st.bold, Hit::Toggle("bold"));
                y += 38.0;
                y = self.sliders(c, x, y, w, clip, &[Prop::TextSize]);
            }
            Tab::Style => {
                y = self.swatches(c, rect(x, y, w, 0.0), tr("Colour"), "color", &st.color);
                y = self.swatches(c, rect(x, y, w, 0.0), tr("Box behind the text"), "bg", &st.bg);
                y = self.sliders(c, x, y, w, clip, &[Prop::BgOpacity]);
                y = self.section(c, x, y, w, tr("Position"));
                y = self.sliders(c, x, y, w, clip, &[Prop::X, Prop::Y]);
                self.pill(c, rect(x, y, 120.0, 32.0), tr("Centre"), false, Hit::Action("reset_pos"), true);
                y += 48.0;
            }
            Tab::Motion => {
                let names = [tr("None"), tr("Fade"), tr("Rise"), tr("Slide")];
                draw::text(c, tr("Coming in"), rect(x, y, w, 18.0), 12.0, 600, p.t2, Align::Left);
                y += 24.0;
                let cur = Anim::ALL.iter().position(|a| *a == st.anim_in).unwrap_or(0);
                self.segmented(c, rect(x, y, w, 32.0), &names, cur, "anim_in");
                y += 48.0;
                draw::text(c, tr("Going out"), rect(x, y, w, 18.0), 12.0, 600, p.t2, Align::Left);
                y += 24.0;
                let cur = Anim::ALL.iter().position(|a| *a == st.anim_out).unwrap_or(0);
                self.segmented(c, rect(x, y, w, 32.0), &names, cur, "anim_out");
                y += 48.0;
                y = self.sliders(c, x, y, w, clip, &[Prop::AnimLen]);
                y = self.trans_section(c, x, y, w, clip);
            }
        }
        y + 8.0
    }

    fn trans_section(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, clip: &Clip) -> f32 {
        let p = self.pal.clone();
        let mut y = self.section(c, x, y, w, tr("Transition in"));
        if self.project.prev_adjacent(clip.id).is_none() {
            y += draw::text_wrap(c, tr("Place this clip right after another one to add a transition."), rect(x, y, w, 40.0), 12.0, 400, p.t3, Align::Left);
            return y + 12.0;
        }
        let cur = match clip.trans.map(|t| t.kind) {
            None => 0,
            Some(model::TransKind::Crossfade) => 1,
            Some(model::TransKind::Dip) => 2,
        };
        self.segmented(c, rect(x, y, w, 32.0), &[tr("Cut"), tr("Crossfade"), tr("Dip to black")], cur, "trans");
        y += 46.0;
        if clip.trans.is_some() {
            y = self.sliders(c, x, y, w, clip, &[Prop::TransLen]);
        }
        y
    }

    /// A row of colours under a label, at the top of `r` (only its x, y and width count).
    fn swatches(&mut self, c: &mut Canvas, r: Rect, label: &str, key: &'static str, cur: &str) -> f32 {
        let (x, y, w) = (r.x, r.y, r.w);
        let p = self.pal.clone();
        draw::text(c, label, rect(x, y, w, 18.0), 12.0, 600, p.t2, Align::Left);
        let mut sx = x;
        let size = ((w - 9.0 * 6.0) / 10.0).min(26.0);
        for (i, s) in SWATCHES.iter().enumerate() {
            let q = rect(sx, y + 26.0, size, size);
            let on = s.eq_ignore_ascii_case(cur);
            if on {
                c.stroke_round(q.inset(-3.0), size / 2.0 + 3.0, p.accent, 2.0);
            }
            c.fill_round(q, size / 2.0, swatch_color(s));
            c.stroke_round(q, size / 2.0, p.ring.mix(p.t3, 0.4), 1.0);
            self.hits.add(q, Hit::Swatch(key, i));
            sx += size + 6.0;
        }
        y + 26.0 + size + 18.0
    }

    fn paint_project_props(&mut self, c: &mut Canvas, r: Rect, top: f32, fields: &mut Vec<(&'static str, Rect)>) -> f32 {
        let p = self.pal.clone();
        let (x, w) = (r.x, r.w);
        let fps = self.project.fps();
        let clips: usize = self.project.tracks.iter().map(|t| t.clips.len()).sum();
        let sub = format!("{} · {}", clock(self.project.duration() as f64 / fps), fill(tr("{} clips"), &[&clips.to_string()]));
        let mut y = self.header(c, rect(x, top, w, 0.0), &ic::FILM, tr("Project"), &sub);
        draw::text(c, tr("Name"), rect(x, y, w, 18.0), 12.0, 600, p.t2, Align::Left);
        y += 24.0;
        let field = rect(x, y, w, 36.0);
        c.fill_round(field, 10.0, p.fill2);
        if self.edit_for == Some("name") {
            c.stroke_round(field, 10.0, p.accent, 1.5);
        } else {
            let name = self.project.name.clone();
            draw::text(c, &name, field.inset_xy(10.0, 0.0), 13.0, 400, p.t1, Align::Left);
            self.hits.add(field, Hit::Field("name"));
        }
        fields.push(("name", field));
        y += 50.0;
        draw::text(c, tr("Frame"), rect(x, y, w, 18.0), 12.0, 600, p.t2, Align::Left);
        y += 24.0;
        let cur = ASPECTS.iter().position(|a| *a == self.project.aspect).unwrap_or(usize::MAX);
        self.segmented(c, rect(x, y, w, 32.0), &["16:9", "9:16", "1:1", "4:5"], cur, "p_aspect");
        y += 48.0;
        draw::text(c, tr("Frames per second"), rect(x, y, w, 18.0), 12.0, 600, p.t2, Align::Left);
        y += 24.0;
        let cur = RATES.iter().position(|f| *f == self.project.fps).unwrap_or(usize::MAX);
        self.segmented(c, rect(x, y, w, 32.0), &["24", "25", "30", "60"], cur, "fps");
        y += 48.0;
        draw::text(c, tr("Lower the music under voices"), rect(x, y, w - 56.0, 24.0), 13.0, 400, p.t1, Align::Left);
        self.switch(c, x + w - 40.0, y, self.project.duck, Hit::Toggle("duck"));
        y += 40.0;
        y += draw::text_wrap(
            c,
            tr("Select a clip on the timeline to change it. Projects save by themselves."),
            rect(x, y, w, 40.0),
            12.0,
            400,
            p.t3,
            Align::Left,
        );
        y + 12.0
    }

    /// A choice of a segmented control.
    pub fn chip(&mut self, key: &str, i: usize, cx: &mut plume_ui::Cx) {
        let id = self.selected.first().copied();
        match key {
            "bin" => {
                self.bin_tab = [super::BinTab::Media, super::BinTab::Text, super::BinTab::Transitions][i.min(2)];
                self.bin_scroll = 0.0;
            }
            "tab" => {
                if let Some(c) = self.sel() {
                    let tabs = tabs_of(&self.project, c);
                    if let Some(t) = tabs.get(i) {
                        self.tab = *t;
                        self.props_scroll = 0.0;
                    }
                }
            }
            "speed" => {
                if let Some(id) = id {
                    self.edit(|p| p.set_speed(id, SPEEDS[i.min(SPEEDS.len() - 1)]));
                }
            }
            "font0" | "font1" => {
                let f = FONTS[(if key == "font1" { 4 } else { 0 } + i).min(FONTS.len() - 1)];
                self.edit_text_style(|st| st.font = f.to_string());
            }
            "anim_in" => self.edit_text_style(|st| st.anim_in = Anim::ALL[i.min(3)]),
            "anim_out" => self.edit_text_style(|st| st.anim_out = Anim::ALL[i.min(3)]),
            "trans" => {
                let kind = [None, Some(model::TransKind::Crossfade), Some(model::TransKind::Dip)][i.min(2)];
                self.set_transition(kind, cx);
            }
            "p_aspect" => {
                let a = ASPECTS[i.min(ASPECTS.len() - 1)];
                self.edit(|p| p.aspect = a);
                self.cache.clear();
            }
            "fps" => {
                let f = RATES[i.min(RATES.len() - 1)];
                self.edit(|p| p.set_fps(f));
                let fps = f as i64;
                self.playhead = self.playhead.min(self.project.duration()).max(0) / fps * fps;
                self.fit_zoom();
            }
            "res" => {
                self.set.export_res = [720, 1080, 2160][i.min(2)];
                self.set.save();
            }
            "aspect" => {
                self.set.export_aspect = crate::export::ASPECTS[i.min(3)].0.to_string();
                self.set.save();
            }
            "codec" => {
                self.set.export_codec = if i == 1 { "hevc".into() } else { "h264".into() };
                self.set.save();
            }
            "theme" => {
                self.set.theme = ["system", "dark", "light"][i.min(2)].into();
                self.settings_changed(cx);
            }
            "lang" => {
                self.set.lang = ["system", "en", "fr"][i.min(2)].into();
                self.settings_changed(cx);
            }
            _ => {}
        }
    }

    pub fn edit_text_style(&mut self, f: impl FnOnce(&mut model::TextStyle)) {
        let Some(id) = self.selected.first().copied() else { return };
        self.edit(|p| {
            if let Some(st) = p.clip_mut(id).and_then(|c| c.text.as_mut()) {
                f(st);
            }
        });
    }

    pub fn swatch(&mut self, key: &str, i: usize) {
        let s = SWATCHES[i.min(SWATCHES.len() - 1)].to_string();
        match key {
            "color" => self.edit_text_style(|st| st.color = s),
            "bg" => self.edit_text_style(|st| {
                st.bg = s;
                if st.bg_opacity < 0.05 {
                    st.bg_opacity = 0.6;
                }
            }),
            _ => {}
        }
    }

    /// The buttons of the panel: fit, fill, reset.
    pub fn clip_action(&mut self, what: &str) {
        let Some(id) = self.selected.first().copied() else { return };
        let ratio = self.project.ratio();
        self.edit(|p| {
            let m = p.clip(id).and_then(|c| p.media_of(c)).map(|m| (m.width.max(1) as f64, m.height.max(1) as f64));
            let Some(c) = p.clip_mut(id) else { return };
            match what {
                "fit" => {
                    c.scale = 1.0;
                    c.x = 0.0;
                    c.y = 0.0;
                }
                "fill" => {
                    if let Some((w, h)) = m {
                        let r = w / h;
                        c.scale = if r > ratio { r / ratio } else { ratio / r };
                        c.x = 0.0;
                        c.y = 0.0;
                    }
                }
                "reset_pos" => {
                    c.x = 0.0;
                    c.y = if c.text.is_some() { c.y.clamp(-0.0, 0.0) } else { 0.0 };
                    if c.text.is_none() {
                        c.scale = 1.0;
                        c.rotation = 0.0;
                        c.opacity = 1.0;
                        c.crop = [0.0; 4];
                    }
                }
                "reset_color" => {
                    c.brightness = 0.0;
                    c.contrast = 1.0;
                    c.saturation = 1.0;
                }
                _ => {}
            }
        });
    }
}
