//! Pinned folders: their place at the top of the sidebar (reordered by dragging, renamed, unpinned), the
//! timeline of a pinned folder (sorted by date taken or modified, photos / videos / favorites, with or without
//! its subfolders), folders dropped from File Explorer, and "Open in Plume Cut".

use super::{App, Drag, Hit, Page, SIDEBAR_W, com_mta, post};
use crate::{
    i18n::{fill, tr},
    library::{self, Item, Progress},
    settings::{self, Pin},
};
use plume_ui::{Align, Canvas, Color, Cx, EditStyle, Font, Pt, Rect, Window, icons, rect};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, atomic::Ordering},
};

/// Height of a pin's row in the sidebar, and the step between rows.
pub const PIN_H: f32 = 40.0;
pub const PIN_STEP: f32 = 42.0;

/// Which items of a pinned folder show, newest first, with the date each one is sorted by.
pub fn pin_order(items: &[Item], pin: &Pin, favorite: impl Fn(&Path) -> bool, local: impl Fn(i64) -> i64) -> Vec<(usize, i64)> {
    let modified = pin.sort == "modified";
    let mut v: Vec<(usize, i64)> = items
        .iter()
        .enumerate()
        .filter(|(_, it)| match pin.show.as_str() {
            "photos" => !it.video,
            "videos" => it.video,
            "favorites" => favorite(&it.path),
            _ => true,
        })
        .map(|(i, it)| (i, if modified { local(it.mtime) } else { it.taken }))
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| library::natural_cmp(&items[a.0].path.to_string_lossy(), &items[b.0].path.to_string_lossy())));
    v
}

/// Plume Cut, when it is installed next to Plume Media (PLUME_MEDIA_CUT_EXE: another place, for tests).
pub fn cut_exe() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("PLUME_MEDIA_CUT_EXE") {
        return Some(PathBuf::from(p)).filter(|p| p.is_file());
    }
    std::env::current_exe().ok().map(|e| e.with_file_name("plume-cut.exe")).filter(|p| p.is_file())
}

/// Files and folders dropped from File Explorer reach the app (WM_DROPFILES).
pub fn accept_drops(win: Window) {
    use windows::Win32::{
        Foundation::{LRESULT, POINT},
        UI::{
            HiDpi::GetDpiForWindow,
            Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, DragQueryPoint, HDROP},
            WindowsAndMessaging::WM_DROPFILES,
        },
    };
    let hwnd = win.hwnd;
    let s = win.sender();
    unsafe { DragAcceptFiles(hwnd, true) };
    win.set_hook(Some(Rc::new(move |msg, wp, _| {
        if msg != WM_DROPFILES {
            return None;
        }
        let drop = HDROP(wp.0 as _);
        let mut paths = vec![];
        let mut pt = POINT::default();
        unsafe {
            let n = DragQueryFileW(drop, u32::MAX, None);
            for i in 0..n.min(64) {
                let len = DragQueryFileW(drop, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                DragQueryFileW(drop, i, Some(&mut buf));
                paths.push(PathBuf::from(String::from_utf16_lossy(&buf[..len])));
            }
            let _ = DragQueryPoint(drop, &mut pt);
            DragFinish(drop);
        }
        let k = unsafe { GetDpiForWindow(hwnd) } as f32 / 96.0;
        let at = Pt { x: pt.x as f32 / k, y: pt.y as f32 / k };
        post(&s, move |app, cx| app.dropped(paths, at, cx));
        Some(LRESULT(0))
    })));
}

impl App {
    /// The pinned folder whose page is shown.
    pub fn current_pin(&self) -> Option<&Pin> {
        match &self.page {
            Page::Pin(p) => settings::pin_index(&self.set.pins, p).map(|i| &self.set.pins[i]),
            _ => None,
        }
    }

    fn pin_key(pin: &Pin) -> String {
        format!("{}|{}", library::norm(&pin.path), pin.subfolders)
    }

    pub fn pin_folder(&mut self, path: PathBuf, cx: &mut Cx) {
        if !path.is_dir() {
            return;
        }
        if settings::add_pin(&mut self.set.pins, path) {
            self.set.save();
            self.say(tr("Folder pinned"), cx);
        } else {
            self.say(tr("Already pinned"), cx);
        }
        cx.repaint();
    }

    pub fn unpin(&mut self, path: &Path, cx: &mut Cx) {
        if let Some(i) = settings::pin_index(&self.set.pins, path) {
            self.set.pins.remove(i);
            self.set.save();
            if matches!(&self.page, Page::Pin(p) if settings::same(p, path)) {
                self.go(Page::Timeline, cx);
            }
            self.say(tr("Unpinned"), cx);
        }
        cx.repaint();
    }

    pub fn move_pin_by(&mut self, path: &Path, d: i32, cx: &mut Cx) {
        if let Some(i) = settings::pin_index(&self.set.pins, path) {
            let to = (i as i32 + d).clamp(0, self.set.pins.len() as i32 - 1) as usize;
            settings::move_pin(&mut self.set.pins, i, to);
            self.set.save();
        }
        cx.repaint();
    }

    /// Changes how the shown pinned folder lists its items.
    pub fn set_pin_option(&mut self, f: impl FnOnce(&mut Pin), cx: &mut Cx) {
        let Page::Pin(path) = self.page.clone() else { return };
        let Some(i) = settings::pin_index(&self.set.pins, &path) else { return };
        let before = self.set.pins[i].subfolders;
        f(&mut self.set.pins[i]);
        let rescan = before != self.set.pins[i].subfolders;
        self.set.save();
        self.scroll = 0.0;
        self.scroll_to = 0.0;
        if rescan {
            self.scan_pin();
        }
        self.refresh_list();
        cx.repaint();
    }

    /// Reads the shown pinned folder (in the background; what was read last time shows meanwhile).
    pub fn scan_pin(&mut self) {
        let Some(pin) = self.current_pin().cloned() else { return };
        let key = Self::pin_key(&pin);
        self.extra = self.pin_cache.get(&key).cloned().unwrap_or_default();
        if let Some(p) = &self.pin_scanning {
            p.stop.store(true, Ordering::Relaxed);
        }
        let progress = Arc::new(Progress::default());
        self.pin_scanning = Some(progress.clone());
        // files already read (library, last visit) are not read again
        let mut old: HashMap<PathBuf, Item> = self.lib.iter().map(|i| (i.path.clone(), i.clone())).collect();
        for it in &self.extra {
            old.insert(it.path.clone(), it.clone());
        }
        let s = self.win.sender();
        std::thread::spawn(move || {
            com_mta();
            let depth = if pin.subfolders { 32 } else { 0 };
            let items = library::scan_depth(std::slice::from_ref(&pin.path), &old, &progress, depth);
            if progress.stop.load(Ordering::Relaxed) {
                return;
            }
            post(&s, move |app, cx| {
                if !app.pin_scanning.as_ref().is_some_and(|p| Arc::ptr_eq(p, &progress)) {
                    return;
                }
                app.pin_scanning = None;
                app.pin_cache.insert(key.clone(), items.clone());
                if app.current_pin().is_some_and(|p| Self::pin_key(p) == key) {
                    app.extra = items;
                    app.refresh_list();
                    app.note(format!("pin scan: {} items", app.extra.len()));
                }
                cx.repaint();
            });
        });
    }

    /// The list of the shown pinned folder (indexes past the library's, into `extra`).
    pub fn pin_list(&self) -> (Vec<usize>, Vec<i64>) {
        let Some(pin) = self.current_pin() else { return (vec![], vec![]) };
        let base = self.lib.len();
        let order = pin_order(&self.extra, pin, |p| self.store.is_favorite(p), library::local);
        order.into_iter().map(|(i, d)| (base + i, d)).unzip()
    }

    /// The pins at the top of the sidebar, from `y`; gives where the rest of the sidebar starts.
    pub fn paint_pins(&mut self, c: &mut Canvas, y: f32, bottom: f32) -> f32 {
        let p = self.pal.clone();
        let mut y = y;
        let n = self.set.pins.len();
        if n > 0 {
            c.text(tr("Pinned"), rect(24.0, y, SIDEBAR_W - 48.0, 20.0), Font::SmallBold, p.t3, Align::Left);
            y += 24.0;
        }
        // at most what fits above the library; the rest scrolls with the wheel
        let room = ((bottom - y - PIN_STEP - 8.0) / PIN_STEP).floor().max(2.0) as usize;
        let shown = n.min(room);
        let area = rect(0.0, y, SIDEBAR_W, shown as f32 * PIN_STEP);
        self.pins_area = area;
        let max_scroll = (n.saturating_sub(shown)) as f32 * PIN_STEP;
        self.pin_scroll = self.pin_scroll.clamp(0.0, max_scroll);
        let dragging = match &self.drag {
            Some(Drag::Pin { i, moved: true, .. }) => Some(*i),
            _ => None,
        };
        let dropping = matches!(&self.drag, Some(Drag::Card { moved: true, .. })) && self.mouse.x < SIDEBAR_W;
        c.clip(area);
        for i in 0..n {
            let r = rect(12.0, area.y + i as f32 * PIN_STEP - self.pin_scroll, SIDEBAR_W - 24.0, PIN_H);
            if r.bottom() < area.y || r.y > area.bottom() {
                continue;
            }
            let pin = self.set.pins[i].clone();
            let hit = Hit::Pin(i);
            let on = matches!(&self.page, Page::Pin(q) if settings::same(q, &pin.path));
            if dragging == Some(i) {
                c.fill_round(r, 10.0, p.fill);
            } else if on {
                c.fill_round(r, 10.0, p.accent.fade(0.16));
            } else if self.hovered(&hit) {
                c.fill_round(r, 10.0, p.hover);
            }
            let renaming = self.renaming.as_ref().is_some_and(|q| settings::same(q, &pin.path));
            c.icon(&icons::FOLDER, rect(r.x + 12.0, r.y + 12.0, 16.0, 16.0), if on { p.ink } else { p.t2 });
            if renaming {
                let f = rect(r.x + 36.0, r.y + 6.0, r.w - 42.0, r.h - 12.0);
                c.fill_round(f, 8.0, p.solid);
                c.stroke_round(f, 8.0, p.accent, 1.5);
                self.rename_rect = Some(f.inset_xy(8.0, 3.0));
            } else {
                c.text(
                    &pin.label(),
                    rect(r.x + 40.0, r.y, r.w - 48.0, r.h),
                    if on { Font::BodyBold } else { Font::Body },
                    if on { p.t1 } else { p.t2 },
                    Align::Left,
                );
            }
            let vis = r.intersect(&area);
            if !vis.is_empty() {
                self.hits.add(vis, hit);
            }
        }
        c.unclip();
        // where a dragged pin would land
        if let Some(Drag::Pin { moved: true, .. }) = &self.drag {
            let to = self.pin_slot(self.mouse.y);
            let ly = area.y + to as f32 * PIN_STEP - self.pin_scroll - 1.0;
            c.fill_round(rect(18.0, ly.clamp(area.y - 2.0, area.bottom()), SIDEBAR_W - 36.0, 3.0), 1.5, p.accent);
        }
        y = area.bottom() + if n > 0 { 2.0 } else { 0.0 };
        let add = rect(12.0, y, SIDEBAR_W - 24.0, 36.0);
        if dropping {
            c.fill_round(add, 10.0, p.accent.fade(0.18));
            c.stroke_round(add, 10.0, p.accent, 1.5);
        } else if self.hovered(&Hit::PinAdd) {
            c.fill_round(add, 10.0, p.hover);
        }
        c.icon(&icons::PIN, rect(add.x + 12.0, add.y + 10.0, 16.0, 16.0), if dropping { p.ink } else { p.t3 });
        c.text(
            if dropping { tr("Drop to pin") } else { tr("Pin a folder") },
            rect(add.x + 40.0, add.y, add.w - 48.0, add.h),
            Font::Body,
            if dropping { p.t1 } else { p.t3 },
            Align::Left,
        );
        self.hits.add(add, Hit::PinAdd);
        add.bottom() + 8.0
    }

    /// Which place a pin dragged to `y` takes.
    pub fn pin_slot(&self, y: f32) -> usize {
        let n = self.set.pins.len();
        if n == 0 {
            return 0;
        }
        (((y - self.pins_area.y + self.pin_scroll) / PIN_STEP).floor().max(0.0) as usize).min(n - 1)
    }

    /// Sort, what to show, subfolders: the bar under a pinned folder's title.
    pub fn paint_pin_bar(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32) {
        let Some(pin) = self.current_pin().cloned() else { return };
        let p = self.pal.clone();
        let show: Vec<(String, Hit, bool)> = [("all", tr("All")), ("photos", tr("Photos")), ("videos", tr("Videos")), ("favorites", tr("Favorites"))]
            .into_iter()
            .map(|(k, l)| (l.to_string(), Hit::Show(k), pin.show == k))
            .collect();
        let x1 = self.segmented(c, x, y, &show);
        let sort: Vec<(String, Hit, bool)> =
            [("taken", tr("Date taken")), ("modified", tr("Date modified"))].into_iter().map(|(k, l)| (l.to_string(), Hit::Sort(k), pin.sort == k)).collect();
        let x2 = self.segmented(c, x1 + 12.0, y, &sort);
        // the switch, if there is room on the line
        let label = tr("Include subfolders");
        let lw = c.text_width(label, Font::Body);
        let sx = (x2 + 20.0).max(x);
        if sx + lw + 60.0 <= x + w {
            let r = rect(sx, y, lw + 62.0, 36.0);
            c.text(label, rect(r.x, r.y, lw + 4.0, r.h), Font::Body, p.t2, Align::Left);
            self.switch(c, rect(r.x + lw + 12.0, r.y + 6.0, 44.0, 24.0), pin.subfolders);
            self.hits.add(r, Hit::Subfolders);
        }
    }

    /// An iOS-style switch, on = accent.
    pub fn switch(&mut self, c: &mut Canvas, r: Rect, on: bool) {
        let p = self.pal.clone();
        c.fill_round(r, r.h / 2.0, if on { p.accent } else { p.fill2 });
        let k = r.h - 4.0;
        let kx = if on { r.right() - k - 2.0 } else { r.x + 2.0 };
        c.fill_round(rect(kx, r.y + 2.0, k, k), k / 2.0, Color::hex(0xffffff));
    }

    /// Opens the field to rename a pin.
    pub fn start_rename(&mut self, path: PathBuf, cx: &mut Cx) {
        if settings::pin_index(&self.set.pins, &path).is_none() {
            return;
        }
        self.renaming = Some(path);
        self.rename_rect = None;
        cx.repaint();
    }

    /// Places (or opens) the rename field once its row is drawn.
    pub fn place_rename(&mut self) {
        let Some(path) = self.renaming.clone() else {
            if self.rename_open {
                self.win.edit_hide();
                self.rename_open = false;
            }
            return;
        };
        let Some(r) = self.rename_rect else { return };
        if self.rename_open {
            self.win.edit_place(r);
            return;
        }
        let Some(i) = settings::pin_index(&self.set.pins, &path) else { return };
        let text = self.set.pins[i].label();
        let p = self.pal.clone();
        let style = EditStyle {
            fg: p.t1,
            bg: p.solid,
            size: 13.0,
            select: Some((0, text.encode_utf16().count())),
            placeholder: self.set.pins[i].path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
            password: false,
            multiline: false,
        };
        self.win.edit_show(r, &text, &style);
        self.rename_open = true;
    }

    /// Ends the renaming: the new name is kept (`keep`) or not.
    pub fn end_rename(&mut self, keep: bool, cx: &mut Cx) {
        let Some(path) = self.renaming.take() else { return };
        let text = self.win.edit_text().unwrap_or_default();
        self.win.edit_hide();
        self.rename_open = false;
        if keep && let Some(i) = settings::pin_index(&self.set.pins, &path) {
            let folder = self.set.pins[i].path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let t = text.trim();
            // its own name, or back to the folder's
            self.set.pins[i].name = if t.is_empty() || t == folder { String::new() } else { t.to_string() };
            self.set.save();
        }
        cx.repaint();
    }

    /// Files or folders dropped on the window: folders are pinned, a picture or video opens.
    pub fn dropped(&mut self, paths: Vec<PathBuf>, at: Pt, cx: &mut Cx) {
        self.note(format!("dropped {} at {:.0},{:.0}", paths.len(), at.x, at.y));
        let mut pinned = 0;
        for p in &paths {
            if p.is_dir() && settings::add_pin(&mut self.set.pins, p.clone()) {
                pinned += 1;
            }
        }
        if pinned > 0 {
            self.set.save();
            self.say(if pinned == 1 { tr("Folder pinned").to_string() } else { fill(tr("{n} folders pinned"), &[&pinned.to_string()]) }, cx);
        } else if paths.iter().any(|p| p.is_dir()) {
            self.say(tr("Already pinned"), cx);
        } else if let Some(f) = paths.into_iter().find(|p| p.is_file() && crate::image::is_media(p)) {
            self.open_file(f, None, cx);
        }
        cx.repaint();
    }

    /// The pill that follows the pointer while a folder is dragged to the sidebar.
    pub fn paint_drag_ghost(&mut self, c: &mut Canvas) {
        let Some(Drag::Card { path, moved: true, .. }) = self.drag.clone() else { return };
        let p = self.pal.clone();
        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let w = c.text_width(&name, Font::BodyBold) + 52.0;
        let r = rect(self.mouse.x + 12.0, self.mouse.y + 8.0, w, 36.0);
        c.fill_round(r.offset(0.0, 3.0), 18.0, Color::hex(0x000000).fade(0.25));
        c.fill_round(r, 18.0, p.solid2);
        c.stroke_round(r, 18.0, p.ring, 1.0);
        c.icon(&icons::FOLDER, rect(r.x + 14.0, r.y + 10.0, 16.0, 16.0), p.ink);
        c.text(&name, rect(r.x + 38.0, r.y, w - 46.0, r.h), Font::BodyBold, p.t1, Align::Left);
    }

    /// Opens a video in Plume Cut.
    pub fn open_in_cut(&mut self, path: &Path) {
        if let Some(exe) = cut_exe() {
            let _ = std::process::Command::new(exe).arg(path).spawn();
            self.note(format!("plume cut: {}", path.display()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(name: &str, video: bool, taken: i64, mtime: i64) -> Item {
        Item { path: PathBuf::from(format!(r"C:\p\{name}")), video, taken, mtime, size: 1 }
    }

    #[test]
    fn pinned_folder_order() {
        let items = [it("a.jpg", false, 300, 10), it("b.mp4", true, 100, 900), it("c.jpg", false, 200, 500), it("d.jpg", false, 200, 1)];
        let mut pin = Pin::new(PathBuf::from(r"C:\p"));
        let order = |pin: &Pin| pin_order(&items, pin, |p| p.ends_with("c.jpg"), |t| t).into_iter().map(|(i, _)| i).collect::<Vec<_>>();
        // date taken, newest first; same date: by name
        assert_eq!(order(&pin), [0, 2, 3, 1]);
        pin.sort = "modified".into();
        assert_eq!(order(&pin), [1, 2, 0, 3]);
        pin.show = "videos".into();
        assert_eq!(order(&pin), [1]);
        pin.show = "photos".into();
        assert_eq!(order(&pin), [2, 0, 3]);
        pin.show = "favorites".into();
        assert_eq!(order(&pin), [2]);
    }
}
