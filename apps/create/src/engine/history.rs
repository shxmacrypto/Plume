//! Undo and redo: whole copies of the document. Pictures are shared between copies, so a step costs only what it
//! changed; old steps go when they hold too much.

use super::doc::{Doc, Kind, Layer};
use std::{collections::HashSet, sync::Arc};

pub struct History {
    undo: Vec<(Doc, String)>,
    redo: Vec<(Doc, String)>,
    pub max_steps: usize,
    pub max_bytes: usize,
}

impl Default for History {
    fn default() -> History {
        History { undo: vec![], redo: vec![], max_steps: 80, max_bytes: 1_500_000_000 }
    }
}

fn pictures(doc: &Doc, out: &mut HashSet<usize>, bytes: &mut usize) {
    fn go(v: &[Layer], out: &mut HashSet<usize>, bytes: &mut usize) {
        for l in v {
            if let Some(m) = &l.mask
                && out.insert(Arc::as_ptr(&m.px) as usize)
            {
                *bytes += m.px.data.len();
            }
            match &l.kind {
                Kind::Image(i) => {
                    if out.insert(Arc::as_ptr(&i.px) as usize) {
                        *bytes += i.px.data.len();
                    }
                }
                Kind::Group(c) => go(c, out, bytes),
                _ => {}
            }
        }
    }
    go(&doc.layers, out, bytes);
}

impl History {
    /// Keeps `before` (the document before a change) as a step to go back to.
    pub fn push(&mut self, before: Doc, label: &str) {
        self.undo.push((before, label.to_string()));
        self.redo.clear();
        self.trim();
    }
    fn trim(&mut self) {
        while self.undo.len() > self.max_steps {
            self.undo.remove(0);
        }
        loop {
            let (mut seen, mut bytes) = (HashSet::new(), 0usize);
            for (d, _) in self.undo.iter().chain(&self.redo) {
                pictures(d, &mut seen, &mut bytes);
            }
            if bytes <= self.max_bytes || self.undo.len() <= 1 {
                break;
            }
            self.undo.remove(0);
        }
    }
    pub fn undo(&mut self, cur: &mut Doc) -> Option<String> {
        let (d, label) = self.undo.pop()?;
        self.redo.push((std::mem::replace(cur, d), label.clone()));
        Some(label)
    }
    pub fn redo(&mut self, cur: &mut Doc) -> Option<String> {
        let (d, label) = self.redo.pop()?;
        self.undo.push((std::mem::replace(cur, d), label.clone()));
        Some(label)
    }
    /// Drops the last step without going back to it (a change that will be kept another way).
    pub fn forget_last(&mut self) {
        self.undo.pop();
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    #[cfg(any(test, feature = "test-hook"))]
    pub fn len(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{
        doc::{ImageL, Xf},
        img::Img,
    };

    #[test]
    fn undo_redo_and_limits() {
        let mut h = History::default();
        let mut d = Doc::new(10, 10);
        h.push(d.clone(), "a");
        d.w = 20;
        h.push(d.clone(), "b");
        d.w = 30;
        assert_eq!(h.undo(&mut d).as_deref(), Some("b"));
        assert_eq!(d.w, 20);
        assert_eq!(h.undo(&mut d).as_deref(), Some("a"));
        assert_eq!(d.w, 10);
        assert!(h.undo(&mut d).is_none());
        assert_eq!(h.redo(&mut d).as_deref(), Some("a"));
        assert_eq!(d.w, 20);
        // a new change forgets what could be redone
        h.push(d.clone(), "c");
        assert!(!h.can_redo());
        // too many pictures: old steps go
        let mut h = History { max_bytes: 1000, ..History::default() };
        for i in 0..5u64 {
            let mut d = Doc::new(10, 10);
            d.layers.push(Layer::new(i, "p", Xf::new(0.0, 0.0, 10.0, 10.0), Kind::Image(ImageL { px: Arc::new(Img::new(10, 10)), key: 0 })));
            h.push(d, "p");
        }
        assert_eq!(h.len().0, 2);
    }
}
