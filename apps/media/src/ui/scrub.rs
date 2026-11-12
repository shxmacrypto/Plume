//! The date rail on the right of every timeline: the years along it, a handle where the page is; hovering shows
//! the month at that height, dragging scrolls there. The whole timeline maps onto the rail's height.

use super::{App, Drag, Hit, Row};
use plume_ui::{Align, Canvas, Font, Rect, rect};

/// Width of the rail (the grid leaves it room).
pub const RAIL_W: f32 = 52.0;

/// A month's title in the timeline: where it starts (content y), and which month.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub y: f32,
    pub year: i64,
    pub month: u32,
}

/// The months of a laid-out timeline.
pub fn marks(rows: &[Row]) -> Vec<Mark> {
    rows.iter()
        .filter_map(|r| match r {
            Row::Head { y, ym, .. } => Some(Mark { y: *y, year: ym.0, month: ym.1 }),
            _ => None,
        })
        .collect()
}

/// The scroll position for a point of the rail (0 top, 1 bottom).
pub fn scroll_for(f: f32, max_scroll: f32) -> f32 {
    f.clamp(0.0, 1.0) * max_scroll.max(0.0)
}

/// Where the rail's handle stands for a scroll position.
pub fn frac_for(scroll: f32, max_scroll: f32) -> f32 {
    if max_scroll <= 0.0 { 0.0 } else { (scroll / max_scroll).clamp(0.0, 1.0) }
}

/// The month shown at the top of the page when it is scrolled to `scroll`: the last title above it.
pub fn month_at(marks: &[Mark], scroll: f32) -> Option<(i64, u32)> {
    let i = marks.partition_point(|m| m.y <= scroll + 1.0);
    marks.get(i.saturating_sub(1)).map(|m| (m.year, m.month))
}

/// The years along a rail `track_h` high: where each year's first month lands, never closer than `gap`.
pub fn year_labels(marks: &[Mark], max_scroll: f32, track_h: f32, gap: f32) -> Vec<(f32, i64)> {
    let mut out: Vec<(f32, i64)> = vec![];
    let mut last_year = None;
    for m in marks {
        if last_year == Some(m.year) {
            continue;
        }
        last_year = Some(m.year);
        let y = frac_for(m.y, max_scroll) * track_h;
        if out.last().is_none_or(|(ly, _)| y - ly >= gap) {
            out.push((y, m.year));
        }
    }
    out
}

impl App {
    /// The rail along `r` (grid pages with months only).
    pub fn paint_rail(&mut self, c: &mut Canvas, r: Rect) {
        if self.content_h <= self.view_h + 1.0 || self.rows.is_empty() {
            return;
        }
        let p = self.pal.clone();
        let marks = marks(&self.rows);
        let max = self.max_scroll();
        let track = rect(r.x, r.y + 10.0, r.w, (r.h - 20.0).max(20.0));
        let active = self.hovered(&Hit::Rail) || matches!(self.drag, Some(Drag::Rail));
        if active {
            c.fill_round(rect(track.x + 4.0, track.y - 6.0, track.w - 8.0, track.h + 12.0), 12.0, p.hover);
        }
        for (y, year) in year_labels(&marks, max, track.h, 22.0) {
            c.text(&year.to_string(), rect(track.x, track.y + y - 9.0, track.w - 10.0, 18.0), Font::Small, p.t3, Align::Right);
        }
        // the handle: where the page is
        let hy = track.y + frac_for(self.scroll, max) * track.h;
        c.fill_round(rect(track.right() - 7.0, hy - 14.0, 4.0, 28.0), 2.0, if active { p.ink } else { p.t3.fade(0.6) });
        self.hits.add(rect(r.x, r.y, r.w, r.h), Hit::Rail);
        // the month under the pointer (or being dragged to)
        let at = if matches!(self.drag, Some(Drag::Rail)) {
            Some(self.scroll)
        } else if self.hovered(&Hit::Rail) {
            Some(scroll_for((self.mouse.y - track.y) / track.h, max))
        } else {
            None
        };
        if let Some(s) = at
            && let Some((y, m)) = month_at(&marks, s)
        {
            let label = crate::i18n::month_year(y, m);
            let by = if matches!(self.drag, Some(Drag::Rail)) { hy } else { self.mouse.y.clamp(track.y, track.bottom()) };
            let w = c.text_width(&label, Font::BodyBold) + 32.0;
            let b = rect(track.x - w - 6.0, by - 17.0, w, 34.0);
            c.fill_round(b, 17.0, p.solid2);
            c.stroke_round(b, 17.0, p.ring, 1.0);
            c.text(&label, b, Font::BodyBold, p.t1, Align::Center);
        }
    }

    /// The rail was pressed or dragged at `y`.
    pub fn rail_to(&mut self, y: f32) {
        let Some(r) = self.hits.rect_of(|h| *h == Hit::Rail) else { return };
        let track = (r.y + 10.0, (r.h - 20.0).max(20.0));
        self.scroll = scroll_for((y - track.0) / track.1, self.max_scroll());
        self.scroll_to = self.scroll;
    }

    /// The month at the top of the page (for the test socket).
    #[cfg(feature = "test-hook")]
    pub fn shown_month(&self) -> Option<(i64, u32)> {
        month_at(&marks(&self.rows), self.scroll)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(y: f32, year: i64, month: u32) -> Mark {
        Mark { y, year, month }
    }

    #[test]
    fn rail_math() {
        assert_eq!(scroll_for(0.5, 1000.0), 500.0);
        assert_eq!(scroll_for(-1.0, 1000.0), 0.0);
        assert_eq!(scroll_for(2.0, 1000.0), 1000.0);
        assert_eq!(scroll_for(0.5, -5.0), 0.0);
        assert_eq!(frac_for(250.0, 1000.0), 0.25);
        assert_eq!(frac_for(10.0, 0.0), 0.0);
        // there and back
        for f in [0.0, 0.1, 0.37, 0.9, 1.0] {
            assert!((frac_for(scroll_for(f, 4321.0), 4321.0) - f).abs() < 1e-5);
        }
    }

    #[test]
    fn months_and_years() {
        let ms = [m(0.0, 2026, 11), m(400.0, 2026, 3), m(900.0, 2025, 12), m(1500.0, 2025, 6), m(2600.0, 2024, 1)];
        assert_eq!(month_at(&ms, 0.0), Some((2026, 11)));
        // a title within a pixel counts as reached (the rail lands exactly on it)
        assert_eq!(month_at(&ms, 398.0), Some((2026, 11)));
        assert_eq!(month_at(&ms, 399.5), Some((2026, 3)));
        assert_eq!(month_at(&ms, 400.0), Some((2026, 3)));
        assert_eq!(month_at(&ms, 1498.0), Some((2025, 12)));
        assert_eq!(month_at(&ms, 99_999.0), Some((2024, 1)));
        assert_eq!(month_at(&[], 10.0), None);
        // one label per year, where its first month lands
        let ys = year_labels(&ms, 2600.0, 260.0, 20.0);
        assert_eq!(ys, vec![(0.0, 2026), (90.0, 2025), (260.0, 2024)]);
        // too close together: the later one is left out
        let ys = year_labels(&ms, 2600.0, 26.0, 20.0);
        assert_eq!(ys, vec![(0.0, 2026), (26.0, 2024)]);
    }
}
