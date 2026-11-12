//! Find and replace: plain text or regular expression, case, whole words. Matches are found line by line.

use crate::text::{Doc, Pos, is_word, pos};
use regex::{Regex, RegexBuilder};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub case: bool,
    pub word: bool,
    pub regex: bool,
}

pub struct Matcher {
    re: Regex,
    word: bool,
    regex: bool,
}

/// Beyond this, matches are not counted while typing (the count would slow typing down).
pub const COUNT_LIMIT: usize = 100_000;

impl Query {
    pub fn compile(&self) -> Result<Matcher, String> {
        if self.text.is_empty() {
            return Err(String::new());
        }
        let pattern = if self.regex { self.text.clone() } else { regex::escape(&self.text) };
        let re = RegexBuilder::new(&pattern).case_insensitive(!self.case).size_limit(50 << 20).build().map_err(|e| {
            // the last line of the regex error says what is wrong
            e.to_string().lines().rfind(|l| !l.trim().is_empty()).unwrap_or("").trim().trim_start_matches("error: ").to_string()
        })?;
        Ok(Matcher { re, word: self.word, regex: self.regex })
    }
}

impl Matcher {
    fn whole(&self, line: &str, a: usize, b: usize) -> bool {
        if !self.word {
            return true;
        }
        let before = line[..a].chars().next_back();
        let after = line[b..].chars().next();
        !before.is_some_and(is_word) && !after.is_some_and(is_word) && a < b
    }

    /// The matches of a line from byte `from` on (empty ones too when `empty`).
    fn iter_line<'a>(&'a self, line: &'a str, from: usize, empty: bool) -> impl Iterator<Item = (usize, usize)> + 'a {
        let mut at = from;
        std::iter::from_fn(move || {
            while at <= line.len() {
                let m = self.re.find_at(line, at)?;
                let (a, b) = (m.start(), m.end());
                // after an empty match, go on one character further
                at = if b > a { b } else { b + line[b..].chars().next().map_or(1, char::len_utf8) };
                if (b > a || empty) && self.whole(line, a, b) {
                    return Some((a, b));
                }
            }
            None
        })
    }

    /// The first match at or after `from`, going round to the start when `wrap`.
    pub fn next(&self, doc: &Doc, from: Pos, wrap: bool) -> Option<(Pos, Pos)> {
        let n = doc.line_count();
        let from = doc.clamp(from);
        for k in 0..=n {
            let line_no = (from.line + k) % n;
            if k > 0 && line_no == 0 && !wrap {
                return None;
            }
            if k == n && from.col == 0 {
                break;
            }
            let line = doc.line(line_no);
            let start = if k == 0 { from.col } else { 0 };
            let found = self.iter_line(line, start, false).next();
            if let Some((a, b)) = found {
                // back at the starting line: only what is before the starting point is new
                if k == n && a >= from.col {
                    return None;
                }
                return Some((pos(line_no, a), pos(line_no, b)));
            }
        }
        None
    }

    /// The last match that starts before `before`, going round to the end when `wrap`.
    pub fn prev(&self, doc: &Doc, before: Pos, wrap: bool) -> Option<(Pos, Pos)> {
        let n = doc.line_count();
        let before = doc.clamp(before);
        for k in 0..=n {
            let line_no = (before.line + n - k % n) % n;
            if k > 0 && line_no == n - 1 && !wrap {
                return None;
            }
            let line = doc.line(line_no);
            let limit = if k == 0 { before.col } else { usize::MAX };
            let found = self.iter_line(line, 0, false).take_while(|&(a, _)| a < limit).last();
            if let Some((a, b)) = found {
                if k == n && a < before.col {
                    return None;
                }
                return Some((pos(line_no, a), pos(line_no, b)));
            }
        }
        None
    }

    /// All the matches (the empty ones too, for replace all), at most `limit`.
    pub fn all(&self, doc: &Doc, limit: usize, empty: bool) -> Vec<(Pos, Pos)> {
        let mut out = Vec::new();
        for (i, line) in doc.lines().iter().enumerate() {
            for (a, b) in self.iter_line(line, 0, empty) {
                out.push((pos(i, a), pos(i, b)));
                if out.len() >= limit {
                    return out;
                }
            }
        }
        out
    }

    /// The matches on one line (to highlight them).
    pub fn in_line(&self, line: &str) -> Vec<(usize, usize)> {
        self.iter_line(line, 0, false).take(1000).collect()
    }

    /// How many matches, and which one `at` is (1-based, 0 if none starts there).
    pub fn count(&self, doc: &Doc, at: Pos) -> (usize, usize) {
        let mut n = 0;
        let mut index = 0;
        for (i, line) in doc.lines().iter().enumerate() {
            for (a, _) in self.iter_line(line, 0, false) {
                n += 1;
                if pos(i, a) == at {
                    index = n;
                }
                if n >= COUNT_LIMIT {
                    return (n, index);
                }
            }
        }
        (n, index)
    }

    /// Is `a..b` exactly a match (the selection that "Replace" replaces)?
    pub fn is_match(&self, doc: &Doc, a: Pos, b: Pos) -> bool {
        a.line == b.line && self.iter_line(doc.line(a.line), a.col, true).next() == Some((a.col, b.col))
    }

    /// The text that replaces a match: with a regular expression, $1 and ${name} are the groups, \n and \t a line
    /// break and a tab.
    pub fn replacement(&self, line: &str, a: usize, with: &str) -> String {
        if !self.regex {
            return with.to_string();
        }
        let with = unescape(with);
        let Some(caps) = self.re.captures_at(line, a) else { return with };
        let mut out = String::new();
        caps.expand(&with, &mut out);
        out
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Replaces every match in one undo step; returns how many.
pub fn replace_all(doc: &mut Doc, m: &Matcher, with: &str) -> usize {
    let list: Vec<(Pos, Pos, String)> =
        m.all(doc, usize::MAX, m.regex).into_iter().map(|(a, b)| (a, b, m.replacement(doc.line(a.line), a.col, with))).collect();
    doc.replace_many(&list, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{Kind, Sel};

    fn q(text: &str, case: bool, word: bool, regex: bool) -> Matcher {
        Query { text: text.into(), case, word, regex }.compile().unwrap()
    }

    #[test]
    fn plain_case_and_words() {
        let d = Doc::new("Cat cat\nconcat CAT");
        let m = q("cat", false, false, false);
        assert_eq!(m.all(&d, 100, false).len(), 4);
        let m = q("cat", true, false, false);
        assert_eq!(m.all(&d, 100, false), vec![(pos(0, 4), pos(0, 7)), (pos(1, 3), pos(1, 6))]);
        let m = q("cat", false, true, false);
        assert_eq!(m.all(&d, 100, false).len(), 3);
        // signs are taken literally without regex
        let d = Doc::new("a.b a+b (x)");
        assert_eq!(q("a.b", true, false, false).all(&d, 10, false).len(), 1);
        assert_eq!(q("(x)", true, true, false).all(&d, 10, false).len(), 1);
    }

    #[test]
    fn next_and_prev_wrap_around() {
        let d = Doc::new("one two\nthree two\nfour");
        let m = q("two", true, false, false);
        assert_eq!(m.next(&d, pos(0, 0), true), Some((pos(0, 4), pos(0, 7))));
        assert_eq!(m.next(&d, pos(0, 7), true), Some((pos(1, 6), pos(1, 9))));
        assert_eq!(m.next(&d, pos(1, 9), true), Some((pos(0, 4), pos(0, 7))));
        assert_eq!(m.next(&d, pos(1, 9), false), None);
        assert_eq!(m.prev(&d, pos(1, 6), true), Some((pos(0, 4), pos(0, 7))));
        assert_eq!(m.prev(&d, pos(0, 4), true), Some((pos(1, 6), pos(1, 9))));
        assert_eq!(m.prev(&d, pos(0, 4), false), None);
        assert_eq!(m.count(&d, pos(1, 6)), (2, 2));
        let single = Doc::new("x two");
        assert_eq!(m.next(&single, pos(0, 5), true), Some((pos(0, 2), pos(0, 5))));
    }

    #[test]
    fn regex_groups_and_escapes() {
        let mut d = Doc::new("2024-01-05\nnot a date\n1999-12-31");
        let m = q(r"(\d{4})-(\d\d)-(\d\d)", true, false, true);
        assert_eq!(replace_all(&mut d, &m, "$3/$2/$1"), 2);
        assert_eq!(d.text(), "05/01/2024\nnot a date\n31/12/1999");
        d.undo();
        assert_eq!(d.text(), "2024-01-05\nnot a date\n1999-12-31");
        let mut d = Doc::new("a,b,c");
        assert_eq!(replace_all(&mut d, &q(",", true, false, true), r"\n"), 2);
        assert_eq!(d.text(), "a\nb\nc");
        // empty matches: a prefix on every line
        let mut d = Doc::new("x\ny");
        assert_eq!(replace_all(&mut d, &q("^", true, false, true), "> "), 2);
        assert_eq!(d.text(), "> x\n> y");
        assert!(Query { text: "(".into(), regex: true, ..Default::default() }.compile().is_err());
    }

    #[test]
    fn replace_one_selected_match() {
        let mut d = Doc::new("foo bar foo");
        let m = q("foo", true, false, false);
        let (a, b) = m.next(&d, pos(0, 1), true).unwrap();
        assert!(m.is_match(&d, a, b));
        d.sel = Sel { anchor: a, caret: b };
        d.insert("baz", Kind::Other);
        assert_eq!(d.text(), "foo bar baz");
        assert!(!m.is_match(&d, pos(0, 0), pos(0, 2)));
    }
}
