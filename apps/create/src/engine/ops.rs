//! Changes to the document as a whole: crop and straighten, order of layers, groups, align and distribute, and the
//! snapping of a moved layer to the canvas, the other layers and the grid.

use super::doc::{Doc, Id, Kind, Layer, Xf};

/// Crops the canvas to a `w` × `h` rectangle centred on (`cx`, `cy`), the picture first turned by `-angle`
/// degrees around that centre (straighten). Layers keep their pixels: nothing is lost outside the frame.
pub fn crop(doc: &mut Doc, cx: f32, cy: f32, w: f32, h: f32, angle: f32) {
    let (nw, nh) = (w.round().max(1.0), h.round().max(1.0));
    for l in &mut doc.layers {
        if angle != 0.0 {
            l.rotate_about(cx, cy, -angle);
        }
        l.translate(-(cx - nw / 2.0), -(cy - nh / 2.0));
    }
    doc.w = nw as u32;
    doc.h = nh as u32;
}

/// Changes the canvas size keeping the layers centred.
pub fn resize_canvas(doc: &mut Doc, w: u32, h: u32) {
    let (dx, dy) = ((w as f32 - doc.w as f32) / 2.0, (h as f32 - doc.h as f32) / 2.0);
    for l in &mut doc.layers {
        l.translate(dx, dy);
    }
    doc.w = w.max(1);
    doc.h = h.max(1);
}

/// Moves a layer one place up (towards the top, `up`) or down in its list. False if it cannot.
pub fn step(doc: &mut Doc, id: Id, up: bool) -> bool {
    let Some((parent, i)) = doc.parent_of(id) else { return false };
    let Some(list) = doc.list_mut(parent) else { return false };
    let j = if up { i + 1 } else { i.wrapping_sub(1) };
    if j >= list.len() {
        return false;
    }
    list.swap(i, j);
    true
}

/// Puts a layer at a place of a list (`parent` None: the stack), counted bottom first. Into its own group: refused.
pub fn move_to(doc: &mut Doc, id: Id, parent: Option<Id>, index: usize) -> bool {
    if let Some(p) = parent
        && (p == id || doc.find(id).is_some_and(|l| l.is_group() && contains(l, p)))
    {
        return false;
    }
    let Some((old_parent, old_i)) = doc.parent_of(id) else { return false };
    let Some(l) = doc.remove(id) else { return false };
    let mut index = index;
    if old_parent == parent && old_i < index {
        index -= 1;
    }
    match doc.list_mut(parent) {
        Some(list) => {
            let at = index.min(list.len());
            list.insert(at, l);
            true
        }
        None => {
            doc.layers.push(l);
            false
        }
    }
}

fn contains(l: &Layer, id: Id) -> bool {
    l.children().iter().any(|c| c.id == id || contains(c, id))
}

/// Puts layers (in the same list) into a new group, where the top-most of them was. Returns the group's id.
pub fn group(doc: &mut Doc, ids: &[Id], name: &str) -> Option<Id> {
    let first = *ids.first()?;
    let (parent, _) = doc.parent_of(first)?;
    if ids.iter().any(|i| doc.parent_of(*i).map(|p| p.0) != Some(parent)) {
        return None;
    }
    let mut places: Vec<(usize, Id)> = ids.iter().filter_map(|i| doc.parent_of(*i).map(|p| (p.1, *i))).collect();
    places.sort();
    let top = places.last()?.0;
    let gid = doc.id();
    let list = doc.list_mut(parent)?;
    let mut members = vec![];
    for (i, _) in places.iter().rev() {
        members.push(list.remove(*i));
    }
    members.reverse();
    let at = top + 1 - members.len();
    let g = Layer::new(gid, name, Xf::new(0.0, 0.0, 0.0, 0.0), Kind::Group(members));
    list.insert(at.min(list.len()), g);
    Some(gid)
}

/// Takes the children out of a group, in its place. Returns their ids.
pub fn ungroup(doc: &mut Doc, id: Id) -> Vec<Id> {
    let Some((parent, i)) = doc.parent_of(id) else { return vec![] };
    let Some(list) = doc.list_mut(parent) else { return vec![] };
    if !list[i].is_group() {
        return vec![];
    }
    let g = list.remove(i);
    let Kind::Group(children) = g.kind else { return vec![] };
    let ids = children.iter().map(|c| c.id).collect();
    for (k, mut c) in children.into_iter().enumerate() {
        // the group's look passes to its children
        c.opacity *= g.opacity;
        c.visible &= g.visible;
        list.insert(i + k, c);
    }
    ids
}

/// A copy of a layer (new ids), just above it. Returns the copy's id.
pub fn duplicate(doc: &mut Doc, id: Id, offset: f32) -> Option<Id> {
    let (parent, i) = doc.parent_of(id)?;
    let mut copy = doc.find(id)?.clone();
    fn renumber(l: &mut Layer, doc: &mut Doc) {
        l.id = doc.id();
        if let Kind::Group(c) = &mut l.kind {
            for x in c {
                renumber(x, doc);
            }
        }
    }
    renumber(&mut copy, doc);
    copy.translate(offset, offset);
    let nid = copy.id;
    doc.list_mut(parent)?.insert(i + 1, copy);
    Some(nid)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignTo {
    Left,
    HCenter,
    Right,
    Top,
    VCenter,
    Bottom,
}

fn bounds_of(doc: &Doc, id: Id) -> Option<(f32, f32, f32, f32)> {
    doc.find(id).and_then(|l| l.frame().bounds(0.0).into())
}

/// Aligns layers: to each other when there are several, to the canvas when there is one.
pub fn align(doc: &mut Doc, ids: &[Id], to: AlignTo) {
    let boxes: Vec<(Id, (f32, f32, f32, f32))> = ids.iter().filter_map(|i| bounds_of(doc, *i).map(|b| (*i, b))).collect();
    if boxes.is_empty() {
        return;
    }
    let target = if boxes.len() == 1 {
        (0.0, 0.0, doc.w as f32, doc.h as f32)
    } else {
        boxes.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |a, (_, b)| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
    };
    for (id, b) in boxes {
        let (dx, dy) = match to {
            AlignTo::Left => (target.0 - b.0, 0.0),
            AlignTo::Right => (target.2 - b.2, 0.0),
            AlignTo::HCenter => ((target.0 + target.2) / 2.0 - (b.0 + b.2) / 2.0, 0.0),
            AlignTo::Top => (0.0, target.1 - b.1),
            AlignTo::Bottom => (0.0, target.3 - b.3),
            AlignTo::VCenter => (0.0, (target.1 + target.3) / 2.0 - (b.1 + b.3) / 2.0),
        };
        if let Some(l) = doc.find_mut(id) {
            l.translate(dx, dy);
        }
    }
}

/// Spreads three or more layers so the gaps between them are equal, across (`horizontal`) or down.
pub fn distribute(doc: &mut Doc, ids: &[Id], horizontal: bool) {
    let mut boxes: Vec<(Id, (f32, f32, f32, f32))> = ids.iter().filter_map(|i| bounds_of(doc, *i).map(|b| (*i, b))).collect();
    if boxes.len() < 3 {
        return;
    }
    let lo = |b: &(f32, f32, f32, f32)| if horizontal { b.0 } else { b.1 };
    let size = |b: &(f32, f32, f32, f32)| if horizontal { b.2 - b.0 } else { b.3 - b.1 };
    boxes.sort_by(|a, b| lo(&a.1).total_cmp(&lo(&b.1)));
    let start = lo(&boxes[0].1);
    let end = boxes.iter().map(|(_, b)| lo(b) + size(b)).fold(f32::MIN, f32::max);
    let total: f32 = boxes.iter().map(|(_, b)| size(b)).sum();
    let gap = (end - start - total) / (boxes.len() - 1) as f32;
    let mut at = start;
    for (id, b) in boxes {
        let d = at - lo(&b);
        if let Some(l) = doc.find_mut(id) {
            if horizontal {
                l.translate(d, 0.0);
            } else {
                l.translate(0.0, d);
            }
        }
        at += size(&b) + gap;
    }
}

/// Where a moved box snaps: the shift to add and the guide lines met (document x and y).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snap {
    pub dx: f32,
    pub dy: f32,
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
}

/// Snaps a box (x0, y0, x1, y1) to the canvas edges and centre, to the other layers' edges and centres, and to the
/// grid (`grid` > 0), within `within` document pixels.
pub fn snap(doc: &Doc, moving: &[Id], b: (f32, f32, f32, f32), within: f32, grid: f32) -> Snap {
    let mut xs = vec![0.0, doc.w as f32 / 2.0, doc.w as f32];
    let mut ys = vec![0.0, doc.h as f32 / 2.0, doc.h as f32];
    for l in doc.all() {
        if moving.contains(&l.id) || !l.visible || l.is_group() || doc.group_of(l.id).is_some_and(|g| moving.contains(&g)) {
            continue;
        }
        let (x0, y0, x1, y1) = l.xf.bounds(0.0);
        xs.extend([x0, (x0 + x1) / 2.0, x1]);
        ys.extend([y0, (y0 + y1) / 2.0, y1]);
    }
    let best = |edges: [f32; 3], lines: &[f32], grid: f32| -> (f32, Vec<f32>) {
        let mut d = f32::MAX;
        for e in edges {
            for l in lines {
                if (l - e).abs() < d.abs() {
                    d = l - e;
                }
            }
            if grid > 0.0 {
                let g = (e / grid).round() * grid;
                if (g - e).abs() < d.abs() {
                    d = g - e;
                }
            }
        }
        if d.abs() > within {
            return (0.0, vec![]);
        }
        // every line the shifted box now touches
        let mut met: Vec<f32> = lines.iter().copied().filter(|l| edges.iter().any(|e| (e + d - l).abs() < 0.01)).collect();
        met.dedup();
        (d, met)
    };
    let (dx, mx) = best([b.0, (b.0 + b.2) / 2.0, b.2], &xs, grid);
    let (dy, my) = best([b.1, (b.1 + b.3) / 2.0, b.3], &ys, grid);
    Snap { dx, dy, xs: mx, ys: my }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::doc::ShapeL;

    fn sq(d: &mut Doc, x: f32, y: f32, s: f32) -> Id {
        let id = d.id();
        d.layers.push(Layer::new(id, "s", Xf::new(x + s / 2.0, y + s / 2.0, s, s), Kind::Shape(ShapeL::default())));
        id
    }

    #[test]
    fn crop_moves_and_turns_layers() {
        let mut d = Doc::new(200, 100);
        let a = sq(&mut d, 90.0, 40.0, 20.0);
        crop(&mut d, 100.0, 50.0, 60.0, 40.0, 0.0);
        assert_eq!((d.w, d.h), (60, 40));
        let l = d.find(a).unwrap();
        assert_eq!((l.xf.cx, l.xf.cy), (30.0, 20.0));
        // straighten: turned around the crop's centre
        let mut d = Doc::new(200, 100);
        let b = sq(&mut d, 140.0, 40.0, 20.0);
        crop(&mut d, 100.0, 50.0, 100.0, 100.0, 90.0);
        let l = d.find(b).unwrap();
        assert!((l.xf.cx - 50.0).abs() < 1e-3 && (l.xf.cy - 0.0).abs() < 1e-3, "{:?}", l.xf);
        assert!((l.xf.rot + 90.0).abs() < 1e-3);
        resize_canvas(&mut d, 120, 100);
        assert!((d.find(b).unwrap().xf.cx - 60.0).abs() < 1e-3);
    }

    #[test]
    fn order_groups_duplicates() {
        let mut d = Doc::new(100, 100);
        let a = sq(&mut d, 0.0, 0.0, 10.0);
        let b = sq(&mut d, 20.0, 0.0, 10.0);
        let c = sq(&mut d, 40.0, 0.0, 10.0);
        assert!(step(&mut d, a, true));
        assert_eq!(d.layers.iter().map(|l| l.id).collect::<Vec<_>>(), vec![b, a, c]);
        assert!(!step(&mut d, c, true));
        assert!(move_to(&mut d, c, None, 0));
        assert_eq!(d.layers[0].id, c);
        let g = group(&mut d, &[b, a], "g").unwrap();
        assert_eq!(d.layers.len(), 2);
        assert_eq!(d.layers[1].id, g);
        assert_eq!(d.find(g).unwrap().children().iter().map(|l| l.id).collect::<Vec<_>>(), vec![b, a]);
        // a group cannot go into itself
        assert!(!move_to(&mut d, g, Some(g), 0));
        assert!(move_to(&mut d, c, Some(g), 1));
        assert_eq!(d.find(g).unwrap().children().len(), 3);
        let n = duplicate(&mut d, g, 10.0).unwrap();
        assert_eq!(d.find(n).unwrap().children().len(), 3);
        assert_eq!(d.all().len(), 8);
        let ids: std::collections::HashSet<Id> = d.all().iter().map(|l| l.id).collect();
        assert_eq!(ids.len(), 8);
        let out = ungroup(&mut d, g);
        assert_eq!(out.len(), 3);
        assert_eq!(d.layers.len(), 4);
    }

    #[test]
    fn align_distribute_snap() {
        let mut d = Doc::new(200, 100);
        let a = sq(&mut d, 0.0, 0.0, 10.0);
        let b = sq(&mut d, 50.0, 30.0, 20.0);
        let c = sq(&mut d, 150.0, 60.0, 10.0);
        align(&mut d, &[a, b, c], AlignTo::Top);
        assert!(d.layers.iter().all(|l| (l.xf.bounds(0.0).1 - 0.0).abs() < 1e-3));
        align(&mut d, &[b], AlignTo::HCenter);
        assert!((d.find(b).unwrap().xf.cx - 100.0).abs() < 1e-3);
        d.find_mut(b).unwrap().translate(-70.0, 0.0);
        distribute(&mut d, &[a, b, c], true);
        let g1 = d.find(b).unwrap().xf.bounds(0.0).0 - 10.0;
        let g2 = 150.0 - d.find(b).unwrap().xf.bounds(0.0).2;
        assert!((g1 - g2).abs() < 1e-3);
        // a box near the canvas centre snaps onto it
        let s = snap(&d, &[a], (95.5, 40.0, 105.5, 50.0), 2.0, 0.0);
        assert!((s.dx + 0.5).abs() < 1e-3, "{s:?}");
        assert!(s.xs.contains(&100.0));
        let far = snap(&d, &[a], (30.0, 37.0, 33.0, 39.0), 0.5, 0.0);
        assert_eq!((far.dx, far.dy), (0.0, 0.0));
        let grid = snap(&d, &[a], (31.0, 33.0, 37.0, 37.0), 2.0, 16.0);
        assert!((grid.dx - 1.0).abs() < 1e-3 || (grid.dx + 1.0).abs() < 1e-3);
    }
}
