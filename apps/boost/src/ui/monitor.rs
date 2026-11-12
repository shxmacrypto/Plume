//! The monitor: a small window that stays on top of the others (games in a window or borderless included), with the
//! last two minutes of the graphics card and the processor as graphs. It reads what Plume Boost already reads; it
//! puts nothing inside games, so it has no frame counter (see the Graphics page for why).

use super::Pal;
use crate::{
    i18n::tf,
    live::{HISTORY, Live},
};
use plume_ui::{Align, Area, Button, Canvas, Color, Cursor, Cx, Event, Font, Gfx, Handler, Hits, PaintCx, Pt, Window, rect, window};
use std::{any::Any, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    Close,
}

pub struct Monitor {
    win: Window,
    pal: Pal,
    live: Live,
    hits: Hits<Hit>,
    hover: Option<Hit>,
}

/// Opens the monitor (not activated: the game keeps the focus) and puts it above other windows.
pub fn open(gfx: &Rc<Gfx>, pal: Pal, live: Live) -> Option<Window> {
    let o = window::Options {
        title: "Plume Boost monitor".into(),
        size: (330.0, 420.0),
        min: (280.0, 300.0),
        dark: true,
        background: pal.bg,
        activate: false,
        placement: None,
    };
    let w = window::create(gfx, o, move |win| Box::new(Monitor { win, pal, live, hits: Hits::default(), hover: None })).ok()?;
    w.mark("PlumeBoostMonitor");
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos};
        let _ = SetWindowPos(w.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
    Some(w)
}

pub fn alive(w: &Window) -> bool {
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(w.hwnd)).as_bool() }
}

/// New readings (and colours) from the main window.
pub fn update(h: &mut dyn Handler, live: Live, pal: Pal, cx: &mut Cx) {
    if let Some(m) = h.as_any().downcast_mut::<Monitor>() {
        m.live = live;
        m.pal = pal;
        cx.repaint();
    }
}

fn last(v: &[f32]) -> Option<f32> {
    v.last().copied().filter(|x| x.is_finite())
}

impl Monitor {
    /// One graph: its name, the value now, the history as an area.
    #[allow(clippy::too_many_arguments)]
    fn graph(&self, c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, label: &str, value: String, data: &[f32], max: f32, color: Color) {
        let p = &self.pal;
        c.text(label, rect(x, y, w * 0.6, 18.0), Font::Small, p.t3, Align::Left);
        c.text(&value, rect(x + w * 0.4, y, w * 0.6, 18.0), Font::SmallBold, p.t1, Align::Right);
        let g = rect(x, y + 20.0, w, h - 24.0);
        c.fill_round(g, 8.0, p.fill);
        let n = HISTORY as f32;
        let bw = g.w / n;
        let top = data.iter().copied().filter(|v| v.is_finite()).fold(max, f32::max).max(1.0);
        let start = HISTORY.saturating_sub(data.len());
        for (i, v) in data.iter().enumerate() {
            if !v.is_finite() {
                continue;
            }
            let k = (v / top).clamp(0.0, 1.0);
            let bh = (g.h - 4.0) * k;
            let bx = g.x + (start + i) as f32 * bw;
            c.fill(rect(bx, g.bottom() - 2.0 - bh, bw + 0.3, bh), color.fade(0.28));
            c.fill(rect(bx, g.bottom() - 2.0 - bh, bw + 0.3, 1.6), color);
        }
        if data.iter().all(|v| !v.is_finite()) {
            c.text(tf("not reported by the driver", "non fourni par le pilote"), g, Font::Small, p.t4, Align::Center);
        }
    }
}

impl Handler for Monitor {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.hits.clear();
        let p = self.pal.clone();
        c.clear(p.bg);
        let (w, h) = (cx.w, cx.h);
        // title bar
        c.text(tf("Monitor", "Moniteur"), rect(14.0, 0.0, w - 60.0, 34.0), Font::BodyBold, p.t1, Align::Left);
        let close = rect(w - 40.0, 2.0, 34.0, 30.0);
        if self.hover == Some(Hit::Close) {
            c.fill_round(close, 8.0, p.close_hover);
        }
        c.glyph('\u{E8BB}', close, if self.hover == Some(Hit::Close) { Color::hex(0xffffff) } else { p.t2 });
        self.hits.add(close, Hit::Close);
        let l = &self.live;
        let hst = &l.hist;
        let t = l.tel.unwrap_or_default();
        let x = 14.0;
        let gw = w - 28.0;
        let rows = 7.0;
        let gh = ((h - 40.0 - 26.0) / rows).max(36.0);
        let mut y = 38.0;
        let f0 = |v: Option<f64>, unit: &str| v.map(|v| format!("{v:.0} {unit}")).unwrap_or("—".into());
        let fan = match (t.fan_pct, t.fan_rpm) {
            (Some(pc), _) => format!("{pc:.0} %"),
            (None, Some(r)) => format!("{r:.0} rpm"),
            _ => "—".into(),
        };
        let temp = match (t.temp_c, t.hotspot_c) {
            (Some(a), Some(b)) => format!("{a:.0} °C · {b:.0}"),
            (Some(a), None) => format!("{a:.0} °C"),
            _ => "—".into(),
        };
        let power = match (t.power_w, t.power_limit_w) {
            (Some(a), Some(b)) => format!("{a:.0} / {b:.0} W"),
            (Some(a), None) => format!("{a:.0} W"),
            _ => "—".into(),
        };
        let items: [(&str, String, &Vec<f32>, f32, Color); 7] = [
            (tf("Graphics load", "Charge graphique"), f0(t.usage_pct, "%"), &hst.gpu_pct, 100.0, p.accent),
            (tf("Graphics clock", "Fréquence graphique"), f0(t.core_mhz, "MHz"), &hst.gpu_mhz, 500.0, p.accent),
            (tf("Graphics temperature", "Température graphique"), temp, &hst.gpu_temp, 90.0, p.orange),
            (tf("Graphics power", "Puissance graphique"), power, &hst.gpu_w, 20.0, p.orange),
            (tf("Fan", "Ventilateur"), fan, &hst.gpu_fan, 100.0, p.green),
            (tf("Processor load", "Charge processeur"), last(&hst.cpu_pct).map(|v| format!("{v:.0} %")).unwrap_or("—".into()), &hst.cpu_pct, 100.0, p.t2),
            (
                tf("Processor clock", "Fréquence processeur"),
                last(&hst.cpu_mhz).map(|v| format!("{:.2} GHz", v / 1000.0)).unwrap_or("—".into()),
                &hst.cpu_mhz,
                1000.0,
                p.t2,
            ),
        ];
        for (label, value, data, max, col) in items {
            self.graph(c, x, y, gw, gh, label, value, data, max, col);
            y += gh;
        }
        c.text(
            tf("Last 2 minutes · no in-game counter, nothing injected", "2 dernières minutes · pas de compteur en jeu, rien d'injecté"),
            rect(x, h - 24.0, gw, 18.0),
            Font::Small,
            p.t4,
            Align::Left,
        );
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::MouseMove { p, .. } => {
                let h = self.hits.at(p).copied();
                if h != self.hover {
                    self.hover = h;
                    cx.repaint();
                }
            }
            Event::MouseLeave => {
                self.hover = None;
                cx.repaint();
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                if self.hits.at(p) == Some(&Hit::Close) {
                    self.win.close();
                }
            }
            Event::Resized | Event::Scale | Event::Active(_) => cx.repaint(),
            _ => {}
        }
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(Hit::Close) => Area::Control,
            None if p.y < 34.0 => Area::Caption,
            None => Area::Client,
        }
    }

    fn cursor(&self, _p: Pt) -> Cursor {
        Cursor::Arrow
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
