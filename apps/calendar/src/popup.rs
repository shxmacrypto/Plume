//! The reminder popup: a small Plume window in the corner of the screen, above the others, that does not take the
//! keyboard. Shown when a reminder rings while the main window is closed or in the background.

use crate::{
    i18n::{self, fill, tr},
    reminders::Due,
    sys,
    tz::{self, Zone},
    ui,
};
use plume_ui::{Align, Area, Button, Canvas, Color, Cursor, Cx, Event, Font, Gfx, Handler, Hits, PaintCx, Pt, Theme, Window, rect, window};
use std::{any::Any, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Snooze(usize),
    Dismiss(usize),
    Open(usize),
    Close,
}

pub struct Popup {
    win: Window,
    main: Window,
    t: Theme,
    items: Vec<Due>,
    h24: bool,
    hits: Hits<Id>,
    hover: Option<Id>,
    pressed: Option<Id>,
}

const W: f32 = 360.0;
const ROW: f32 = 112.0;

/// "In 10 min · 12:30", "Starting now", "Tomorrow · All day".
pub fn when(d: &Due, now: i64, h24: bool) -> String {
    let z = Zone::local();
    let start = z.to_wall(d.start);
    let time = if d.all_day { tr("All day").to_string() } else { i18n::time(start.secs, h24) };
    let mins = (d.start - now) / 60;
    let rel = if d.all_day {
        let days = start.date.days() - tz::date_at(&z, now).days();
        if days == 0 { tr("Today").to_string() } else { i18n::cap(&i18n::long_date(start.date)) }
    } else if mins > 0 {
        if mins >= 120 { i18n::cap(&i18n::long_date(start.date)) } else { fill(tr("In {} min"), &[&mins.to_string()]) }
    } else if mins == 0 {
        tr("Starting now").to_string()
    } else {
        fill(tr("Started {} min ago"), &[&(-mins).to_string()])
    };
    format!("{rel} · {time}")
}

/// Opens the popup with a first reminder.
pub fn open(main: Window, gfx: Rc<Gfx>, t: Theme, d: Due, h24: bool) -> Option<Window> {
    let o = window::Options {
        title: format!("Plume Calendar · {}", d.title),
        size: (W, ROW + 40.0),
        min: (W, 100.0),
        dark: t.dark,
        background: t.bg,
        activate: false,
        placement: None,
    };
    let w = window::create(&gfx, o, move |win| {
        Box::new(Popup { win, main, t, items: vec![d], h24, hits: Hits::default(), hover: None, pressed: None }) as Box<dyn Handler>
    })
    .ok()?;
    place(w, 1);
    Some(w)
}

fn place(w: Window, n: usize) {
    let s = w.scale();
    let h = 40.0 + ROW * n.min(4) as f32;
    sys::place_corner(w.hwnd, (W * s) as i32, (h * s) as i32);
}

impl Popup {
    pub fn add(&mut self, d: Due, h24: bool, cx: &mut Cx) {
        self.h24 = h24;
        if !self.items.iter().any(|x| x.key() == d.key()) {
            self.items.push(d);
        }
        place(self.win, self.items.len());
        cx.repaint();
    }

    fn to_main(&self, f: impl FnOnce(&mut ui::App, &mut Cx) + Send + 'static) {
        self.main.sender().send(move |h, cx| {
            if let Some(app) = ui::with_app(h) {
                f(app, cx);
                cx.repaint();
            }
        });
    }

    fn remove(&mut self, i: usize, cx: &mut Cx) {
        if i < self.items.len() {
            self.items.remove(i);
        }
        if self.items.is_empty() {
            cx.after(|w| w.close());
        } else {
            place(self.win, self.items.len());
        }
        cx.repaint();
    }

    fn act(&mut self, id: Id, cx: &mut Cx) {
        match id {
            Id::Close => {
                self.items.clear();
                cx.after(|w| w.close());
            }
            Id::Dismiss(i) => self.remove(i, cx),
            Id::Snooze(i) => {
                if let Some(d) = self.items.get(i).cloned() {
                    self.to_main(move |app, _| app.snooze(d));
                }
                self.remove(i, cx);
            }
            Id::Open(i) => {
                if let Some(d) = self.items.get(i).cloned() {
                    self.to_main(move |app, _| app.open_due(&d));
                }
                self.remove(i, cx);
            }
        }
    }
}

impl Handler for Popup {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let t = self.t.clone();
        self.hits.clear();
        c.clear(t.solid);
        c.text("Plume Calendar", rect(16.0, 0.0, cx.w - 60.0, 36.0), Font::SmallBold, t.t3, Align::Left);
        let close = rect(cx.w - 40.0, 4.0, 32.0, 28.0);
        if self.hover == Some(Id::Close) {
            c.fill_round(close, 8.0, t.hover);
        }
        c.icon(&plume_ui::icons::CLOSE, close.centered(10.0, 10.0), t.t2);
        self.hits.add(close, Id::Close);
        let now = tz::now();
        let mut y = 36.0;
        for (i, d) in self.items.clone().iter().enumerate().take(4) {
            let row = rect(8.0, y, cx.w - 16.0, ROW - 8.0);
            c.fill_round(row, 12.0, if self.hover == Some(Id::Open(i)) { t.fill2 } else { t.fill });
            self.hits.add(row, Id::Open(i));
            c.fill_round(rect(row.x + 12.0, row.y + 14.0, 4.0, 38.0), 2.0, ui::color(&d.color));
            c.text(&d.title, rect(row.x + 26.0, row.y + 10.0, row.w - 40.0, 22.0), Font::Title, t.t1, Align::Left);
            let mut sub = when(d, now, self.h24);
            if !d.location.is_empty() {
                sub = format!("{sub} · {}", d.location);
            }
            c.text(&sub, rect(row.x + 26.0, row.y + 33.0, row.w - 40.0, 20.0), Font::Body, t.t2, Align::Left);
            let by = row.bottom() - 42.0;
            for (k, (label, id, accent)) in [(tr("Snooze 5 min"), Id::Snooze(i), false), (tr("Dismiss"), Id::Dismiss(i), true)].into_iter().enumerate() {
                let bw = (row.w - 36.0) / 2.0;
                let b = rect(row.x + 12.0 + k as f32 * (bw + 12.0), by, bw, 32.0);
                let hot = self.hover == Some(id);
                let fill = if accent {
                    if hot { t.accent.mix(Color::hex(0xffffff), 0.12) } else { t.accent }
                } else if hot {
                    t.fill2
                } else {
                    t.solid2
                };
                c.fill_round(b, 16.0, fill);
                c.text(label, b, Font::BodyBold, if accent { Color::hex(0xffffff) } else { t.t1 }, Align::Center);
                self.hits.add(b, id);
            }
            y += ROW;
        }
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
            Event::MouseDown { p, button: Button::Left, .. } => {
                self.pressed = self.hits.at(p).copied();
                cx.capture(true);
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                cx.capture(false);
                if let (Some(a), Some(b)) = (self.pressed.take(), self.hits.at(p).copied())
                    && a == b
                {
                    self.act(a, cx);
                }
            }
            Event::Close => {
                self.to_main(|app, _| app.popup = None);
            }
            Event::Resized | Event::Scale => cx.repaint(),
            _ => {}
        }
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(_) => Area::Client,
            None if p.y < 36.0 => Area::Caption,
            None => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        if self.hits.at(p).is_some() { Cursor::Hand } else { Cursor::Arrow }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
