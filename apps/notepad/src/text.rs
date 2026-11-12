//! The text of a tab or a pad: its lines, the selection, and the undo history.
//!
//! The text is kept as lines without their line endings (those are chosen when saving). A position is a line and
//! a byte offset in that line, always on a character boundary.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

pub const fn pos(line: usize, col: usize) -> Pos {
    Pos { line, col }
}

/// The selection: from `anchor` (where it started) to `caret` (where the cursor is). Empty when they are equal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sel {
    pub anchor: Pos,
    pub caret: Pos,
}

impl Sel {
    pub fn at(p: Pos) -> Sel {
        Sel { anchor: p, caret: p }
    }
    pub fn start(&self) -> Pos {
        self.anchor.min(self.caret)
    }
    pub fn end(&self) -> Pos {
        self.anchor.max(self.caret)
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.caret
    }
}

/// What made a change: typing and deleting merge into one undo step while they go on at the same place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Typing,
    Delete,
    Other,
}

/// One replacement: `removed` was at `start` and `inserted` took its place.
#[derive(Clone, Debug)]
struct Change {
    start: Pos,
    removed: String,
    inserted: String,
}

/// One undo step: the changes in the order they were made, and the selection before and after.
#[derive(Clone, Debug)]
struct Step {
    id: u64,
    changes: Vec<Change>,
    before: Sel,
    after: Sel,
    kind: Kind,
    at: Instant,
}

/// How long typing keeps merging into the same undo step.
const MERGE: Duration = Duration::from_millis(2000);

pub struct Doc {
    lines: Vec<String>,
    pub sel: Sel,
    undo: Vec<Step>,
    redo: Vec<Step>,
    next_id: u64,
    /// The undo step the file on disk matches (0: the text as it was opened).
    saved: u64,
    /// Counted as changes come, to show it at once even for a big file.
    chars: usize,
    /// Goes up with every change: views compare it to know what to compute again.
    pub version: u64,
    /// The next typing may not merge with the last step (the caret moved, a word ended).
    seal: bool,
}

fn char_count(s: &str) -> usize {
    // fast path for plain ASCII
    if s.is_ascii() { s.len() } else { s.chars().count() }
}

/// Where a text inserted at `start` ends.
fn end_of(start: Pos, text: &str) -> Pos {
    match text.rfind('\n') {
        None => pos(start.line, start.col + text.len()),
        Some(i) => pos(start.line + text.matches('\n').count(), text.len() - i - 1),
    }
}

pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Default for Doc {
    fn default() -> Self {
        Doc::new("")
    }
}

impl Doc {
    /// A text whose lines end in '\n' (no '\r').
    pub fn new(text: &str) -> Doc {
        let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
        Doc { lines, sel: Sel::default(), undo: vec![], redo: vec![], next_id: 1, saved: 0, chars: char_count(text), version: 0, seal: true }
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }
    pub fn line(&self, i: usize) -> &str {
        self.lines.get(i).map_or("", |s| s.as_str())
    }
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
    /// Characters of the whole text, line breaks counted as one.
    pub fn chars(&self) -> usize {
        self.chars
    }
    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
    }

    /// The text with '\n' between lines.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn end(&self) -> Pos {
        let last = self.lines.len() - 1;
        pos(last, self.lines[last].len())
    }

    /// The nearest valid position.
    pub fn clamp(&self, p: Pos) -> Pos {
        let line = p.line.min(self.lines.len() - 1);
        let s = &self.lines[line];
        let mut col = p.col.min(s.len());
        while !s.is_char_boundary(col) {
            col -= 1;
        }
        pos(line, col)
    }

    pub fn slice(&self, a: Pos, b: Pos) -> String {
        let (a, b) = (a.min(b), a.max(b));
        if a.line == b.line {
            return self.lines[a.line][a.col..b.col].to_string();
        }
        let mut out = String::with_capacity(self.lines[a.line].len() - a.col + b.col + 1);
        out.push_str(&self.lines[a.line][a.col..]);
        for l in &self.lines[a.line + 1..b.line] {
            out.push('\n');
            out.push_str(l);
        }
        out.push('\n');
        out.push_str(&self.lines[b.line][..b.col]);
        out
    }

    pub fn selected(&self) -> String {
        self.slice(self.sel.start(), self.sel.end())
    }

    // ---- changes ----

    /// Replaces `a..b` with `text`, without history. Returns where the inserted text ends.
    fn raw_replace(&mut self, a: Pos, b: Pos, text: &str) -> Pos {
        let (a, b) = (a.min(b), a.max(b));
        let removed_chars = if a == b {
            0
        } else if a.line == b.line {
            char_count(&self.lines[a.line][a.col..b.col])
        } else {
            let mut n = char_count(&self.lines[a.line][a.col..]) + char_count(&self.lines[b.line][..b.col]) + (b.line - a.line);
            for l in &self.lines[a.line + 1..b.line] {
                n += char_count(l);
            }
            n
        };
        let tail = self.lines[b.line][b.col..].to_string();
        let end = if !text.contains('\n') {
            let line = &mut self.lines[a.line];
            line.truncate(a.col);
            line.push_str(text);
            let col = line.len();
            line.push_str(&tail);
            if b.line > a.line {
                self.lines.drain(a.line + 1..=b.line);
            }
            pos(a.line, col)
        } else {
            let mut parts = text.split('\n');
            let first = parts.next().unwrap_or("");
            let mut new: Vec<String> = parts.map(str::to_string).collect();
            let last = new.last_mut().expect("at least two parts");
            let col = last.len();
            last.push_str(&tail);
            let line = &mut self.lines[a.line];
            line.truncate(a.col);
            line.push_str(first);
            let end = pos(a.line + new.len(), col);
            self.lines.splice(a.line + 1..=b.line, new);
            end
        };
        self.chars = self.chars + char_count(text) - removed_chars;
        self.version += 1;
        end
    }

    fn apply(&mut self, c: &Change) -> Pos {
        let b = end_of(c.start, &c.removed);
        self.raw_replace(c.start, b, &c.inserted)
    }

    fn unapply(&mut self, c: &Change) -> Pos {
        let b = end_of(c.start, &c.inserted);
        self.raw_replace(c.start, b, &c.removed)
    }

    fn push(&mut self, changes: Vec<Change>, before: Sel, kind: Kind) {
        let now = Instant::now();
        self.redo.clear();
        let after = self.sel;
        // typing (or deleting) at the place where the last step stopped joins that step
        if !self.seal
            && kind != Kind::Other
            && let Some(top) = self.undo.last_mut()
            && top.kind == kind
            && top.id != self.saved
            && top.after == before
            && now.duration_since(top.at) < MERGE
            && changes.len() == 1
            && top.changes.len() == 1
        {
            let (last, new) = (&mut top.changes[0], &changes[0]);
            let merged = match kind {
                // the new text follows the last one
                Kind::Typing if new.removed.is_empty() && end_of(last.start, &last.inserted) == new.start => {
                    last.inserted.push_str(&new.inserted);
                    true
                }
                // backspace: the removed text is just before the last one
                Kind::Delete if new.inserted.is_empty() && end_of(new.start, &new.removed) == last.start && last.inserted.is_empty() => {
                    last.removed.insert_str(0, &new.removed);
                    last.start = new.start;
                    true
                }
                // delete key: the removed text is at the same place
                Kind::Delete if new.inserted.is_empty() && new.start == last.start && last.inserted.is_empty() => {
                    last.removed.push_str(&new.removed);
                    true
                }
                _ => false,
            };
            if merged {
                top.after = after;
                top.at = now;
                return;
            }
        }
        let id = self.next_id;
        self.next_id += 1;
        self.undo.push(Step { id, changes, before, after, kind, at: now });
        self.seal = kind == Kind::Other;
    }

    /// Replaces `a..b` with `text` as one change of the history, and puts the caret after it.
    pub fn replace(&mut self, a: Pos, b: Pos, text: &str, kind: Kind) {
        let (a, b) = (self.clamp(a.min(b)), self.clamp(a.max(b)));
        if a == b && text.is_empty() {
            return;
        }
        let before = self.sel;
        let removed = self.slice(a, b);
        let end = self.raw_replace(a, b, text);
        self.sel = Sel::at(end);
        self.push(vec![Change { start: a, removed, inserted: text.to_string() }], before, kind);
        // a line break ends the step (a new word after a space does too: see `insert`)
        if kind == Kind::Typing && text.contains('\n') {
            self.seal = true;
        }
    }

    /// Typing or pasting: the selection is replaced.
    pub fn insert(&mut self, text: &str, kind: Kind) {
        let (a, b) = (self.sel.start(), self.sel.end());
        // after a space, a new word starts a new undo step
        if kind == Kind::Typing
            && let Some(prev) = self.line(a.line)[..a.col].chars().last()
            && prev.is_whitespace()
            && text.chars().next().is_some_and(|c| !c.is_whitespace())
        {
            self.seal = true;
        }
        if a != b {
            self.seal = true;
        }
        self.replace(a, b, text, kind);
    }

    /// Several replacements in one undo step (replace all). `list` is sorted and does not overlap; the selection
    /// becomes `after`.
    pub fn replace_many(&mut self, list: &[(Pos, Pos, String)], after: Option<Sel>) -> usize {
        if list.is_empty() {
            return 0;
        }
        let before = self.sel;
        let mut changes = Vec::with_capacity(list.len());
        // from the end, so that the positions before stay valid
        for (a, b, text) in list.iter().rev() {
            let removed = self.slice(*a, *b);
            self.raw_replace(*a, *b, text);
            changes.push(Change { start: *a, removed, inserted: text.clone() });
        }
        self.sel = after.map(|s| Sel { anchor: self.clamp(s.anchor), caret: self.clamp(s.caret) }).unwrap_or(Sel::at(self.clamp(self.sel.caret)));
        self.seal = true;
        self.push(changes, before, Kind::Other);
        list.len()
    }

    /// Replaces the whole text (reload from disk): one undo step.
    pub fn set_text(&mut self, text: &str) {
        let end = self.end();
        self.seal = true;
        let before = self.sel;
        let removed = self.text();
        self.raw_replace(pos(0, 0), end, text);
        self.sel = Sel::at(self.clamp(before.caret));
        self.push(vec![Change { start: pos(0, 0), removed, inserted: text.to_string() }], before, Kind::Other);
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(step) = self.undo.pop() else { return false };
        for c in step.changes.iter().rev() {
            self.unapply(c);
        }
        self.sel = step.before;
        self.redo.push(step);
        self.seal = true;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(step) = self.redo.pop() else { return false };
        for c in &step.changes {
            self.apply(c);
        }
        self.sel = step.after;
        self.undo.push(step);
        self.seal = true;
        true
    }

    /// The next typing starts its own undo step (the caret was moved by the user).
    pub fn seal(&mut self) {
        self.seal = true;
    }

    /// The text now matches the file.
    pub fn mark_saved(&mut self) {
        self.saved = self.undo.last().map_or(0, |s| s.id);
        self.seal = true;
    }

    /// The text has changes that are not in the file.
    pub fn dirty(&self) -> bool {
        self.undo.last().map_or(0, |s| s.id) != self.saved
    }

    /// Marks the text as changed (restored from a backup of unsaved work).
    pub fn mark_unsaved(&mut self) {
        self.saved = u64::MAX;
    }

    // ---- moving ----

    pub fn prev(&self, p: Pos) -> Pos {
        if p.col > 0 {
            let s = &self.lines[p.line][..p.col];
            let c = s.chars().next_back().map_or(1, char::len_utf8);
            pos(p.line, p.col - c)
        } else if p.line > 0 {
            pos(p.line - 1, self.lines[p.line - 1].len())
        } else {
            p
        }
    }

    pub fn next(&self, p: Pos) -> Pos {
        let s = &self.lines[p.line];
        if p.col < s.len() {
            let c = s[p.col..].chars().next().map_or(1, char::len_utf8);
            pos(p.line, p.col + c)
        } else if p.line + 1 < self.lines.len() {
            pos(p.line + 1, 0)
        } else {
            p
        }
    }

    fn char_before(&self, p: Pos) -> Option<char> {
        if p.col == 0 { if p.line > 0 { Some('\n') } else { None } } else { self.lines[p.line][..p.col].chars().next_back() }
    }

    fn char_after(&self, p: Pos) -> Option<char> {
        let s = &self.lines[p.line];
        if p.col < s.len() {
            s[p.col..].chars().next()
        } else if p.line + 1 < self.lines.len() {
            Some('\n')
        } else {
            None
        }
    }

    /// Ctrl+Left: the start of the word before (spaces skipped first).
    pub fn word_left(&self, mut p: Pos) -> Pos {
        while let Some(c) = self.char_before(p)
            && c.is_whitespace()
            && c != '\n'
        {
            p = self.prev(p);
        }
        match self.char_before(p) {
            Some('\n') => self.prev(p),
            Some(c) if is_word(c) => {
                while self.char_before(p).is_some_and(is_word) {
                    p = self.prev(p);
                }
                p
            }
            Some(_) => {
                while self.char_before(p).is_some_and(|c| !is_word(c) && !c.is_whitespace()) {
                    p = self.prev(p);
                }
                p
            }
            None => p,
        }
    }

    /// Ctrl+Right: past the word, then past the spaces after it.
    pub fn word_right(&self, mut p: Pos) -> Pos {
        match self.char_after(p) {
            Some('\n') => return self.next(p),
            Some(c) if is_word(c) => {
                while self.char_after(p).is_some_and(is_word) {
                    p = self.next(p);
                }
            }
            Some(c) if !c.is_whitespace() => {
                while self.char_after(p).is_some_and(|c| !is_word(c) && !c.is_whitespace()) {
                    p = self.next(p);
                }
            }
            _ => {}
        }
        while let Some(c) = self.char_after(p)
            && c.is_whitespace()
            && c != '\n'
        {
            p = self.next(p);
        }
        p
    }

    /// The word (or run of spaces, or of signs) under a position: double-click.
    pub fn word_at(&self, p: Pos) -> (Pos, Pos) {
        let s = &self.lines[p.line];
        if s.is_empty() {
            return (p, p);
        }
        let probe = if p.col < s.len() { s[p.col..].chars().next() } else { s[..p.col].chars().next_back() };
        let Some(probe) = probe else { return (p, p) };
        let class = |c: char| {
            if is_word(c) {
                0
            } else if c.is_whitespace() {
                1
            } else {
                2
            }
        };
        let k = class(probe);
        let mut a = if p.col < s.len() { p.col } else { p.col - probe.len_utf8() };
        while let Some(c) = s[..a].chars().next_back()
            && class(c) == k
        {
            a -= c.len_utf8();
        }
        let mut b = a;
        while let Some(c) = s[b..].chars().next()
            && class(c) == k
        {
            b += c.len_utf8();
        }
        (pos(p.line, a), pos(p.line, b))
    }

    /// The spaces and tabs at the start of a line (kept on the next line when Enter is pressed).
    pub fn indent_of(&self, line: usize) -> &str {
        let s = self.line(line);
        let n = s.len() - s.trim_start_matches([' ', '\t']).len();
        &s[..n]
    }

    /// Backspace (or Ctrl+Backspace): the selection, or what is before the caret.
    pub fn delete_back(&mut self, word: bool) {
        if !self.sel.is_empty() {
            self.seal = true;
            self.replace(self.sel.start(), self.sel.end(), "", Kind::Delete);
            self.seal = true;
            return;
        }
        let c = self.sel.caret;
        let a = if word { self.word_left(c) } else { self.prev(c) };
        if word {
            self.seal = true;
        }
        self.replace(a, c, "", Kind::Delete);
    }

    /// Delete (or Ctrl+Delete).
    pub fn delete_fwd(&mut self, word: bool) {
        if !self.sel.is_empty() {
            self.seal = true;
            self.replace(self.sel.start(), self.sel.end(), "", Kind::Delete);
            self.seal = true;
            return;
        }
        let c = self.sel.caret;
        let b = if word { self.word_right(c) } else { self.next(c) };
        if word {
            self.seal = true;
        }
        self.replace(c, b, "", Kind::Delete);
        self.sel = Sel::at(c);
        if let Some(top) = self.undo.last_mut() {
            top.after = self.sel;
        }
    }

    /// Tab or Shift+Tab on several lines: each selected line gets (or loses) one indent.
    pub fn indent_lines(&mut self, out: bool, unit: &str) {
        let (a, b) = (self.sel.start(), self.sel.end());
        let last = if b.col == 0 && b.line > a.line { b.line - 1 } else { b.line };
        let mut list = Vec::new();
        for l in a.line..=last {
            if out {
                let s = self.line(l);
                let n = if s.starts_with('\t') { 1 } else { s.len() - s.trim_start_matches(' ').len() };
                let n = n.min(unit.len().max(1));
                if n > 0 {
                    list.push((pos(l, 0), pos(l, n), String::new()));
                }
            } else if !self.line(l).is_empty() {
                list.push((pos(l, 0), pos(l, 0), unit.to_string()));
            }
        }
        let after = Sel { anchor: pos(a.line, 0), caret: pos(last, usize::MAX) };
        self.replace_many(&list, None);
        self.sel = Sel { anchor: self.clamp(after.anchor), caret: self.clamp(after.caret) };
        if let Some(top) = self.undo.last_mut() {
            top.after = self.sel;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(s: &str) -> Doc {
        Doc::new(s)
    }

    #[test]
    fn insert_and_lines() {
        let mut d = doc("hello");
        d.sel = Sel::at(pos(0, 5));
        d.insert(" world\nsecond", Kind::Other);
        assert_eq!(d.text(), "hello world\nsecond");
        assert_eq!(d.sel.caret, pos(1, 6));
        assert_eq!(d.line_count(), 2);
        assert_eq!(d.chars(), 18);
        d.sel = Sel { anchor: pos(0, 3), caret: pos(1, 2) };
        assert_eq!(d.selected(), "lo world\nse");
        d.insert("X", Kind::Other);
        assert_eq!(d.text(), "helXcond");
        assert_eq!(d.chars(), 8);
    }

    #[test]
    fn typing_merges_into_words() {
        let mut d = doc("");
        for ch in "one two".chars() {
            d.insert(&ch.to_string(), Kind::Typing);
        }
        assert_eq!(d.text(), "one two");
        assert!(d.dirty());
        d.undo();
        assert_eq!(d.text(), "one ");
        d.undo();
        assert_eq!(d.text(), "");
        assert!(!d.dirty());
        d.redo();
        d.redo();
        assert_eq!(d.text(), "one two");
        assert_eq!(d.sel.caret, pos(0, 7));
    }

    #[test]
    fn backspace_merges_and_undo_restores_selection() {
        let mut d = doc("abcdef");
        d.sel = Sel::at(pos(0, 6));
        d.delete_back(false);
        d.delete_back(false);
        d.delete_back(false);
        assert_eq!(d.text(), "abc");
        d.undo();
        assert_eq!(d.text(), "abcdef");
        assert_eq!(d.sel, Sel::at(pos(0, 6)));
        // a selection deleted is its own step and comes back selected
        d.sel = Sel { anchor: pos(0, 1), caret: pos(0, 4) };
        d.delete_back(false);
        assert_eq!(d.text(), "aef");
        d.undo();
        assert_eq!(d.sel, Sel { anchor: pos(0, 1), caret: pos(0, 4) });
    }

    #[test]
    fn delete_forward_joins_lines() {
        let mut d = doc("ab\ncd");
        d.sel = Sel::at(pos(0, 2));
        d.delete_fwd(false);
        assert_eq!(d.text(), "abcd");
        assert_eq!(d.sel.caret, pos(0, 2));
        d.delete_fwd(false);
        assert_eq!(d.text(), "abd");
        d.undo();
        assert_eq!(d.text(), "ab\ncd");
    }

    #[test]
    fn unicode_moves() {
        let mut d = doc("é😀x");
        let p = d.next(pos(0, 0));
        assert_eq!(p, pos(0, 2));
        let p = d.next(p);
        assert_eq!(p, pos(0, 6));
        assert_eq!(d.prev(p), pos(0, 2));
        assert_eq!(d.chars(), 3);
        d.sel = Sel::at(pos(0, 6));
        d.delete_back(false);
        assert_eq!(d.text(), "éx");
        assert_eq!(d.clamp(pos(0, 1)), pos(0, 0));
    }

    #[test]
    fn words() {
        let d = doc("let x = foo_bar(1);  next");
        assert_eq!(d.word_right(pos(0, 0)), pos(0, 4));
        assert_eq!(d.word_right(pos(0, 8)), pos(0, 15));
        assert_eq!(d.word_left(pos(0, 15)), pos(0, 8));
        assert_eq!(d.word_left(pos(0, 21)), pos(0, 17));
        assert_eq!(d.word_at(pos(0, 10)), (pos(0, 8), pos(0, 15)));
        let d = doc("a\nb");
        assert_eq!(d.word_right(pos(0, 1)), pos(1, 0));
        assert_eq!(d.word_left(pos(1, 0)), pos(0, 1));
    }

    #[test]
    fn replace_many_is_one_step() {
        let mut d = doc("a1 a2\na3");
        let list = vec![(pos(0, 0), pos(0, 1), "bb".to_string()), (pos(0, 3), pos(0, 4), "bb".to_string()), (pos(1, 0), pos(1, 1), "b\nb".to_string())];
        assert_eq!(d.replace_many(&list, None), 3);
        assert_eq!(d.text(), "bb1 bb2\nb\nb3");
        d.undo();
        assert_eq!(d.text(), "a1 a2\na3");
        d.redo();
        assert_eq!(d.text(), "bb1 bb2\nb\nb3");
    }

    #[test]
    fn saved_state() {
        let mut d = doc("x");
        assert!(!d.dirty());
        d.sel = Sel::at(pos(0, 1));
        d.insert("y", Kind::Typing);
        d.mark_saved();
        assert!(!d.dirty());
        // typing after a save does not merge into the saved step
        d.insert("z", Kind::Typing);
        assert!(d.dirty());
        d.undo();
        assert!(!d.dirty());
        assert_eq!(d.text(), "xy");
        d.undo();
        assert!(d.dirty());
    }

    #[test]
    fn indent_lines() {
        let mut d = doc("a\nb\n\nc");
        d.sel = Sel { anchor: pos(0, 0), caret: pos(3, 1) };
        d.indent_lines(false, "\t");
        assert_eq!(d.text(), "\ta\n\tb\n\n\tc");
        d.indent_lines(true, "\t");
        assert_eq!(d.text(), "a\nb\n\nc");
        d.undo();
        d.undo();
        assert_eq!(d.text(), "a\nb\n\nc");
    }

    #[test]
    fn set_text_undoable() {
        let mut d = doc("old");
        d.set_text("new\ntext");
        assert_eq!(d.text(), "new\ntext");
        d.undo();
        assert_eq!(d.text(), "old");
    }
}
