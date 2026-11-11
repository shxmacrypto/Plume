//! Changes to a PDF, in pure Rust (lopdf): page order, rotation, deletion, extraction, merging and splitting,
//! annotations written as real /Annot objects (with their own appearance, so every reader shows them), form
//! filling, the outline, and saving (a protected file stays protected with the same password).
//!
//! Positions on a page are given the way the window shows it: points from the top-left corner of the page as
//! displayed, after its rotation ("view points"). [`PageGeom`] turns them into the PDF's own coordinates.

use lopdf::{
    Dictionary, Document, EncryptionState, EncryptionVersion, LoadOptions, Object, ObjectId, Permissions, Stream, StringFormat,
    content::{Content, Operation},
    decode_text_string, dictionary, text_string,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

pub type Res<T> = Result<T, String>;

fn e(x: impl std::fmt::Display) -> String {
    x.to_string()
}

// ───────────────────────── loading and saving ─────────────────────────

/// Why a file could not be read.
#[derive(Clone, Debug, PartialEq)]
pub enum LoadError {
    /// Protected: a (better) password is needed.
    Password,
    Other(String),
}

/// Reads a PDF from memory. `password`: for a protected file.
pub fn load(bytes: &[u8], password: Option<&str>) -> Result<Document, LoadError> {
    let doc = match password {
        Some(pw) => Document::load_mem_with_options(bytes, LoadOptions::with_password(pw)),
        None => Document::load_mem(bytes),
    };
    match doc {
        Ok(d) if d.is_encrypted() => Err(LoadError::Password),
        Ok(d) => Ok(d),
        Err(err) => {
            let s = err.to_string();
            let l = s.to_lowercase();
            if l.contains("password") || l.contains("decrypt") || l.contains("encrypt") { Err(LoadError::Password) } else { Err(LoadError::Other(s)) }
        }
    }
}

/// A file ID (needed to protect a file with a password).
fn ensure_id(doc: &mut Document) {
    if doc.trailer.get(b"ID").is_ok() {
        return;
    }
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut id = Vec::with_capacity(16);
    let mut x = nanos as u64 ^ 0x9E37_79B9_7F4A_7C15 ^ (doc.objects.len() as u64).rotate_left(17);
    for _ in 0..16 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        id.push((x >> 24) as u8);
    }
    let s = Object::String(id, StringFormat::Hexadecimal);
    doc.trailer.set("ID", vec![s.clone(), s]);
}

/// The file's bytes. With a password, the file is protected with it (128-bit key).
pub fn to_bytes(doc: &Document, password: Option<&str>) -> Res<Vec<u8>> {
    let mut d = doc.clone();
    d.compress();
    if let Some(pw) = password.filter(|p| !p.is_empty()) {
        ensure_id(&mut d);
        let state = EncryptionState::try_from(EncryptionVersion::V2 {
            document: &d,
            owner_password: pw,
            user_password: pw,
            key_length: 128,
            permissions: Permissions::all(),
        })
        .map_err(e)?;
        d.encrypt(&state).map_err(e)?;
    }
    let mut out = Vec::new();
    d.save_to(&mut out).map_err(e)?;
    Ok(out)
}

/// Writes the file next to its place first, then puts it in place: a failure never leaves half a file.
pub fn save(doc: &Document, path: &Path, password: Option<&str>) -> Res<()> {
    let bytes = to_bytes(doc, password)?;
    write_file(path, &bytes)
}

pub fn write_file(path: &Path, bytes: &[u8]) -> Res<()> {
    let tmp = path.with_extension("pdf.plume-tmp");
    std::fs::write(&tmp, bytes).map_err(e)?;
    if let Err(err) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e(err));
    }
    Ok(())
}

// ───────────────────────── page geometry ─────────────────────────

/// A page's visible box (in PDF units: points, y going up) and its rotation (clockwise, 0/90/180/270).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageGeom {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub rotate: i32,
}

impl Default for PageGeom {
    fn default() -> Self {
        PageGeom { x0: 0.0, y0: 0.0, x1: 612.0, y1: 792.0, rotate: 0 }
    }
}

impl PageGeom {
    /// Size as displayed (points), rotation applied.
    pub fn size(self) -> (f32, f32) {
        let (w, h) = (self.x1 - self.x0, self.y1 - self.y0);
        if self.rotate % 180 == 90 { (h, w) } else { (w, h) }
    }

    /// View points (from the displayed top-left corner) → PDF coordinates.
    pub fn pdf_point(self, x: f32, y: f32) -> (f32, f32) {
        match self.rotate {
            90 => (self.x0 + y, self.y0 + x),
            180 => (self.x1 - x, self.y0 + y),
            270 => (self.x1 - y, self.y1 - x),
            _ => (self.x0 + x, self.y1 - y),
        }
    }

    /// PDF coordinates → view points.
    pub fn view_point(self, px: f32, py: f32) -> (f32, f32) {
        match self.rotate {
            90 => (py - self.y0, px - self.x0),
            180 => (self.x1 - px, py - self.y0),
            270 => (self.y1 - py, self.x1 - px),
            _ => (px - self.x0, self.y1 - py),
        }
    }

    /// A view rectangle (x, y, w, h) → PDF rectangle (x0, y0, x1, y1).
    pub fn pdf_rect(self, r: [f32; 4]) -> [f32; 4] {
        let a = self.pdf_point(r[0], r[1]);
        let b = self.pdf_point(r[0] + r[2], r[1] + r[3]);
        [a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)]
    }

    /// A PDF rectangle (x0, y0, x1, y1) → view rectangle (x, y, w, h).
    pub fn view_rect(self, r: [f32; 4]) -> [f32; 4] {
        let a = self.view_point(r[0], r[1]);
        let b = self.view_point(r[2], r[3]);
        let (x, y) = (a.0.min(b.0), a.1.min(b.1));
        [x, y, a.0.max(b.0) - x, a.1.max(b.1) - y]
    }
}

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    doc.dereference(o).map(|x| x.1).unwrap_or(o)
}

/// A page attribute, looked up the page tree (MediaBox, CropBox, Rotate and Resources are inherited).
fn inherited(doc: &Document, page: ObjectId, key: &[u8]) -> Option<Object> {
    let mut id = page;
    for _ in 0..64 {
        let d = doc.get_dictionary(id).ok()?;
        if let Ok(v) = d.get(key) {
            return Some(v.clone());
        }
        id = d.get(b"Parent").and_then(Object::as_reference).ok()?;
    }
    None
}

fn num(o: &Object) -> Option<f32> {
    match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

fn rect_of(doc: &Document, o: &Object) -> Option<[f32; 4]> {
    let a = deref(doc, o).as_array().ok()?;
    if a.len() != 4 {
        return None;
    }
    let v: Vec<f32> = a.iter().filter_map(|x| num(deref(doc, x))).collect();
    (v.len() == 4).then(|| [v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
}

pub fn page_ids(doc: &Document) -> Vec<ObjectId> {
    doc.page_iter().collect()
}

pub fn geometry(doc: &Document, page: ObjectId) -> PageGeom {
    let media = inherited(doc, page, b"MediaBox").and_then(|o| rect_of(doc, &o)).unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let mut b = inherited(doc, page, b"CropBox").and_then(|o| rect_of(doc, &o)).unwrap_or(media);
    // the crop box is cut to the media box
    b = [b[0].max(media[0]), b[1].max(media[1]), b[2].min(media[2]), b[3].min(media[3])];
    if b[2] - b[0] < 1.0 || b[3] - b[1] < 1.0 {
        b = media;
    }
    let r = inherited(doc, page, b"Rotate").and_then(|o| num(deref(doc, &o))).unwrap_or(0.0) as i32;
    let rotate = (((r % 360) + 360) % 360 + 45) / 90 * 90 % 360;
    PageGeom { x0: b[0], y0: b[1], x1: b[2], y1: b[3], rotate }
}

pub fn geometries(doc: &Document) -> Vec<PageGeom> {
    page_ids(doc).into_iter().map(|p| geometry(doc, p)).collect()
}

// ───────────────────────── pages ─────────────────────────

fn root_pages(doc: &Document) -> Res<ObjectId> {
    doc.catalog().and_then(|c| c.get(b"Pages")).and_then(Object::as_reference).map_err(e)
}

/// Makes every page a direct child of the root of the page tree, with its inherited attributes copied into it:
/// reordering and deleting are then just changes to one list.
pub fn flatten(doc: &mut Document) -> Res<ObjectId> {
    let root = root_pages(doc)?;
    let pages = page_ids(doc);
    for &p in &pages {
        let mut add = Vec::new();
        for key in [&b"MediaBox"[..], b"CropBox", b"Rotate", b"Resources"] {
            let has = doc.get_dictionary(p).map(|d| d.has(key)).unwrap_or(true);
            if !has && let Some(v) = inherited(doc, p, key) {
                add.push((key.to_vec(), v));
            }
        }
        let d = doc.get_dictionary_mut(p).map_err(e)?;
        for (k, v) in add {
            d.set(k, v);
        }
        d.set("Parent", root);
    }
    set_kids(doc, root, &pages)?;
    Ok(root)
}

fn set_kids(doc: &mut Document, root: ObjectId, pages: &[ObjectId]) -> Res<()> {
    let d = doc.get_dictionary_mut(root).map_err(e)?;
    d.set("Kids", pages.iter().map(|&p| Object::Reference(p)).collect::<Vec<_>>());
    d.set("Count", pages.len() as i64);
    Ok(())
}

/// Keeps the pages `order` (indices of the current pages), in that order.
pub fn set_order(doc: &mut Document, order: &[usize]) -> Res<()> {
    let pages = page_ids(doc);
    if order.is_empty() {
        return Err("A PDF needs at least one page.".into());
    }
    let mut seen = vec![false; pages.len()];
    for &i in order {
        if i >= pages.len() || std::mem::replace(&mut seen[i], true) {
            return Err("Bad page order.".into());
        }
    }
    let root = flatten(doc)?;
    let kept: Vec<ObjectId> = order.iter().map(|&i| pages[i]).collect();
    set_kids(doc, root, &kept)
}

/// The new order once the pages `moving` are put before page `to` (0..=n, n: at the end).
pub fn moved_order(n: usize, moving: &[usize], to: usize) -> Vec<usize> {
    let mut mv: Vec<usize> = moving.iter().copied().filter(|&i| i < n).collect();
    mv.sort_unstable();
    mv.dedup();
    let rest: Vec<usize> = (0..n).filter(|i| !mv.contains(i)).collect();
    let at = rest.iter().filter(|&&i| i < to).count();
    let mut out = rest[..at].to_vec();
    out.extend(&mv);
    out.extend(&rest[at..]);
    out
}

#[cfg(test)]
pub fn move_pages(doc: &mut Document, moving: &[usize], to: usize) -> Res<()> {
    let n = page_ids(doc).len();
    set_order(doc, &moved_order(n, moving, to))
}

pub fn delete_pages(doc: &mut Document, del: &[usize]) -> Res<()> {
    let n = page_ids(doc).len();
    let keep: Vec<usize> = (0..n).filter(|i| !del.contains(i)).collect();
    set_order(doc, &keep)
}

/// Turns pages by `delta` degrees (clockwise; a multiple of 90).
pub fn rotate_pages(doc: &mut Document, pages: &[usize], delta: i32) -> Res<()> {
    flatten(doc)?;
    let ids = page_ids(doc);
    for &i in pages {
        let Some(&p) = ids.get(i) else { continue };
        let r = geometry(doc, p).rotate;
        doc.get_dictionary_mut(p).map_err(e)?.set("Rotate", ((r + delta).rem_euclid(360)) as i64);
    }
    Ok(())
}

/// A new document holding only `pages` (in that order).
pub fn extract(doc: &Document, pages: &[usize]) -> Res<Document> {
    let mut d = doc.clone();
    let n = page_ids(&d).len();
    set_order(&mut d, pages)?;
    if pages.len() < n {
        // bookmarks would keep the removed pages alive in the file
        if let Ok(c) = d.catalog_mut() {
            c.remove(b"Outlines");
        }
        d.prune_objects();
    }
    Ok(d)
}

/// "1-3, 5, 8-" (pages from 1) → ranges of page indices (from 0), each (first, last).
pub fn parse_ranges(s: &str, n: usize) -> Res<Vec<(usize, usize)>> {
    let mut out = Vec::new();
    for part in s.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        let bad = || format!("“{part}”");
        let p = |x: &str| x.trim().parse::<usize>().map_err(|_| bad());
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (if a.trim().is_empty() { 1 } else { p(a)? }, if b.trim().is_empty() { n } else { p(b)? }),
            None => {
                let v = p(part)?;
                (v, v)
            }
        };
        if a == 0 || b < a || b > n {
            return Err(bad());
        }
        out.push((a - 1, b - 1));
    }
    if out.is_empty() {
        return Err(String::new());
    }
    Ok(out)
}

pub fn split(doc: &Document, ranges: &[(usize, usize)]) -> Res<Vec<Document>> {
    ranges.iter().map(|&(a, b)| extract(doc, &(a..=b).collect::<Vec<_>>())).collect()
}

/// Adds the pages of `other` at the end of `doc` (its form fields too).
pub fn append(doc: &mut Document, mut other: Document) -> Res<()> {
    let root = flatten(doc)?;
    flatten(&mut other)?;
    let start = doc.objects.keys().map(|k| k.0).max().unwrap_or(0).max(doc.max_id) + 1;
    other.renumber_objects_with(start);
    let o_cat = other.trailer.get(b"Root").and_then(Object::as_reference).map_err(e)?;
    let o_root = root_pages(&other)?;
    let o_pages = page_ids(&other);
    let o_form = other.catalog().ok().and_then(|c| c.get(b"AcroForm").ok()).map(|f| deref(&other, f).clone());
    let o_fields: Vec<Object> = o_form
        .as_ref()
        .and_then(|f| f.as_dict().ok())
        .and_then(|f| f.get(b"Fields").ok())
        .map(|f| deref(&other, f).clone())
        .and_then(|f| f.as_array().ok().cloned())
        .unwrap_or_default();
    let o_dr = o_form.as_ref().and_then(|f| f.as_dict().ok()).and_then(|f| f.get(b"DR").ok()).cloned();
    let o_encrypt = other.trailer.get(b"Encrypt").and_then(Object::as_reference).ok();
    for (id, obj) in std::mem::take(&mut other.objects) {
        if id == o_cat || id == o_root || Some(id) == o_encrypt {
            continue;
        }
        doc.objects.insert(id, obj);
    }
    doc.max_id = doc.objects.keys().map(|k| k.0).max().unwrap_or(0);
    let mut pages = page_ids(doc);
    for &p in &o_pages {
        doc.get_dictionary_mut(p).map_err(e)?.set("Parent", root);
        pages.push(p);
    }
    set_kids(doc, root, &pages)?;
    if !o_fields.is_empty() {
        let form_id = acroform_id(doc)?;
        let f = doc.get_dictionary_mut(form_id).map_err(e)?;
        let mut fields = f.get(b"Fields").and_then(|a| a.as_array()).cloned().unwrap_or_default();
        fields.extend(o_fields);
        f.set("Fields", fields);
        if !f.has(b"DR")
            && let Some(dr) = o_dr
        {
            f.set("DR", dr);
        }
    }
    Ok(())
}

/// Merges documents in order into one.
pub fn merge(mut docs: Vec<Document>) -> Res<Document> {
    if docs.is_empty() {
        return Err("Nothing to merge.".into());
    }
    let mut first = docs.remove(0);
    for d in docs {
        append(&mut first, d)?;
    }
    Ok(first)
}

// ───────────────────────── outline ─────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OutlineItem {
    pub level: usize,
    pub title: String,
    pub page: usize,
}

pub fn outline(doc: &Document) -> Vec<OutlineItem> {
    doc.get_toc()
        .map(|t| t.toc.into_iter().map(|i| OutlineItem { level: i.level.max(1) - 1, title: i.title.trim().to_string(), page: i.page.max(1) - 1 }).collect())
        .unwrap_or_default()
}

// ───────────────────────── annotations ─────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Highlight,
    Note,
    Ink,
    Text,
    Signature,
}

/// An annotation being made in the window, in view points of its page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Annot {
    pub kind: Kind,
    pub page: usize,
    /// x, y, w, h.
    pub rect: [f32; 4],
    /// Highlight: the marked line boxes (x, y, w, h).
    #[serde(default)]
    pub boxes: Vec<[f32; 4]>,
    /// Ink and signature: strokes as fractions of `rect` (0..1, y down), so moving or resizing keeps the drawing.
    #[serde(default)]
    pub strokes: Vec<Vec<[f32; 2]>>,
    #[serde(default)]
    pub text: String,
    pub color: [f32; 3],
    /// Line width (ink, signature) or font size (text box), in points.
    pub width: f32,
}

/// "D:20261111093000Z" for now.
fn pdf_date() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // days since 1970-01-01 → civil date
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("D:{y:04}{m:02}{d:02}{:02}{:02}{:02}Z", rem / 3600, rem / 60 % 60, rem % 60)
}

fn real(v: f32) -> Object {
    Object::Real((v * 1000.0).round() / 1000.0)
}

fn op(name: &str, args: &[f32]) -> Operation {
    Operation::new(name, args.iter().map(|&v| real(v)).collect())
}

fn op_o(name: &str, args: Vec<Object>) -> Operation {
    Operation::new(name, args)
}

fn rect_obj(r: [f32; 4]) -> Vec<Object> {
    r.iter().map(|&v| real(v)).collect()
}

fn helv() -> Dictionary {
    dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" }
}

/// Helvetica's advance widths (thousandths of the font size) for the characters typed most.
pub fn helv_width(ch: char) -> f32 {
    const W: [u16; 95] = [
        278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584,
        584, 556, 1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278,
        278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500,
        500, 334, 260, 334, 584,
    ];
    let c = ch as u32;
    if (32..127).contains(&c) { W[(c - 32) as usize] as f32 } else { latin_width(ch) }
}

/// Helvetica-Bold's advance widths (thousandths of the font size).
pub fn helv_bold_width(ch: char) -> f32 {
    const W: [u16; 95] = [
        278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584,
        584, 611, 975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333,
        278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556,
        500, 389, 280, 389, 584,
    ];
    let c = ch as u32;
    if (32..127).contains(&c) { W[(c - 32) as usize] as f32 } else { latin_width(ch) }
}

/// Widths of the Windows Latin characters beyond ASCII that text often holds (same in both weights).
fn latin_width(ch: char) -> f32 {
    match ch {
        '\u{2014}' | '\u{2026}' | '\u{2030}' => 1000.0,
        '\u{2022}' => 350.0,
        '\u{b7}' | '\u{a0}' => 278.0,
        '\u{2018}' | '\u{2019}' | '\u{201a}' => 222.0,
        '\u{201c}' | '\u{201d}' | '\u{201e}' => 333.0,
        _ => 556.0,
    }
}

pub fn text_width_pt(s: &str, size: f32) -> f32 {
    s.chars().map(helv_width).sum::<f32>() * size / 1000.0
}

/// Text cut into lines that fit `width` points.
pub fn wrap(text: &str, width: f32, size: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let cand = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && text_width_pt(&cand, size) > width {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = cand;
            }
        }
        lines.push(line);
    }
    lines
}

/// Text in Windows' Latin encoding (what Helvetica with /WinAnsiEncoding shows); other characters become "?".
pub fn winansi(s: &str) -> Vec<u8> {
    s.chars()
        .map(|ch| match ch as u32 {
            c @ (0x20..=0x7e | 0xa0..=0xff) => c as u8,
            _ => match ch {
                '€' => 0x80,
                '‚' => 0x82,
                'ƒ' => 0x83,
                '„' => 0x84,
                '…' => 0x85,
                '†' => 0x86,
                '‡' => 0x87,
                'ˆ' => 0x88,
                '‰' => 0x89,
                'Š' => 0x8a,
                '‹' => 0x8b,
                'Œ' => 0x8c,
                'Ž' => 0x8e,
                '‘' => 0x91,
                '’' => 0x92,
                '“' => 0x93,
                '”' => 0x94,
                '•' => 0x95,
                '–' => 0x96,
                '—' => 0x97,
                '˜' => 0x98,
                '™' => 0x99,
                'š' => 0x9a,
                '›' => 0x9b,
                'œ' => 0x9c,
                'ž' => 0x9e,
                'Ÿ' => 0x9f,
                _ => b'?',
            },
        })
        .collect()
}

/// An appearance (form XObject) for an annotation.
fn appearance(doc: &mut Document, bbox: [f32; 4], matrix: Option<[f32; 6]>, resources: Dictionary, ops: Vec<Operation>) -> Res<ObjectId> {
    let content = Content { operations: ops }.encode().map_err(e)?;
    let mut d = dictionary! { "Type" => "XObject", "Subtype" => "Form", "FormType" => 1, "BBox" => rect_obj(bbox), "Resources" => resources };
    if let Some(m) = matrix {
        d.set("Matrix", m.iter().map(|&v| real(v)).collect::<Vec<_>>());
    }
    Ok(doc.add_object(Stream::new(d, content)))
}

/// Matrix turning a drawing made upright (as the page is displayed) back into the page's own direction.
fn upright(rotate: i32) -> Option<[f32; 6]> {
    match rotate {
        90 => Some([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]),
        180 => Some([-1.0, 0.0, 0.0, -1.0, 0.0, 0.0]),
        270 => Some([0.0, -1.0, 1.0, 0.0, 0.0, 0.0]),
        _ => None,
    }
}

fn add_to_page(doc: &mut Document, page: ObjectId, annot: ObjectId) -> Res<()> {
    let annots = doc.get_dictionary(page).map_err(e)?.get(b"Annots").ok().cloned();
    match annots {
        Some(Object::Reference(r)) => {
            if let Ok(Object::Array(a)) = doc.get_object_mut(r) {
                a.push(Object::Reference(annot));
            } else {
                doc.get_dictionary_mut(page).map_err(e)?.set("Annots", vec![Object::Reference(annot)]);
            }
        }
        Some(Object::Array(mut a)) => {
            a.push(Object::Reference(annot));
            doc.get_dictionary_mut(page).map_err(e)?.set("Annots", a);
        }
        _ => doc.get_dictionary_mut(page).map_err(e)?.set("Annots", vec![Object::Reference(annot)]),
    }
    Ok(())
}

fn union(rs: &[[f32; 4]]) -> [f32; 4] {
    let mut u = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for r in rs {
        u = [u[0].min(r[0]), u[1].min(r[1]), u[2].max(r[2]), u[3].max(r[3])];
    }
    u
}

/// Strokes of an ink drawing or a signature, in PDF coordinates.
fn strokes_pdf(a: &Annot, g: &PageGeom) -> Vec<Vec<(f32, f32)>> {
    a.strokes
        .iter()
        .map(|s| s.iter().map(|p| g.pdf_point(a.rect[0] + p[0] * a.rect[2], a.rect[1] + p[1] * a.rect[3])).collect())
        .filter(|s: &Vec<(f32, f32)>| !s.is_empty())
        .collect()
}

fn stroke_ops(strokes: &[Vec<(f32, f32)>], color: [f32; 3], width: f32) -> Vec<Operation> {
    let mut ops = vec![op("q", &[]), op("RG", &color), op("w", &[width]), op("J", &[1.0]), op("j", &[1.0])];
    for s in strokes {
        ops.push(op("m", &[s[0].0, s[0].1]));
        if s.len() == 1 {
            ops.push(op("l", &[s[0].0 + 0.01, s[0].1]));
        }
        for p in &s[1..] {
            ops.push(op("l", &[p.0, p.1]));
        }
        ops.push(op("S", &[]));
    }
    ops.push(op("Q", &[]));
    ops
}

/// Writes annotations into the document, as /Annot objects each with its appearance.
pub fn add_annots(doc: &mut Document, annots: &[Annot]) -> Res<()> {
    let pages = page_ids(doc);
    let date = pdf_date();
    for (i, a) in annots.iter().enumerate() {
        let Some(&page) = pages.get(a.page) else { continue };
        let g = geometry(doc, page);
        let c = a.color;
        let mut d = dictionary! {
            "Type" => "Annot",
            "P" => page,
            "F" => 4,
            "C" => c.iter().map(|&v| real(v)).collect::<Vec<_>>(),
            "M" => Object::string_literal(date.clone()),
            "NM" => Object::string_literal(format!("plume-{}-{i}", &date[2..16])),
        };
        let (rect, ap) = match a.kind {
            Kind::Highlight => {
                let boxes: Vec<[f32; 4]> = if a.boxes.is_empty() { vec![a.rect] } else { a.boxes.clone() };
                let pdf: Vec<[f32; 4]> = boxes.iter().map(|b| g.pdf_rect(*b)).collect();
                let rect = union(&pdf);
                let mut quads = Vec::new();
                for r in &pdf {
                    quads.extend([r[0], r[3], r[2], r[3], r[0], r[1], r[2], r[1]].map(real));
                }
                d.set("Subtype", "Highlight");
                d.set("QuadPoints", quads);
                d.set("CA", real(0.45));
                if !a.text.is_empty() {
                    d.set("Contents", text_string(&a.text));
                }
                let gs = dictionary! { "Type" => "ExtGState", "BM" => "Multiply", "ca" => real(0.45), "CA" => real(0.45) };
                let res = dictionary! { "ExtGState" => dictionary! { "GS0" => gs } };
                let mut ops = vec![op("q", &[]), op_o("gs", vec![Object::Name(b"GS0".to_vec())]), op("rg", &c)];
                for r in &pdf {
                    ops.push(op("re", &[r[0], r[1], r[2] - r[0], r[3] - r[1]]));
                }
                ops.extend([op("f", &[]), op("Q", &[])]);
                let ap = appearance(doc, rect, None, res, ops)?;
                (rect, ap)
            }
            Kind::Note => {
                let rect = g.pdf_rect(a.rect);
                let (w, h) = (a.rect[2], a.rect[3]);
                d.set("Subtype", "Text");
                d.set("Name", "Comment");
                d.set("Open", false);
                d.set("Contents", text_string(&a.text));
                let ops = vec![
                    op("q", &[]),
                    op("rg", &c),
                    op("RG", &[c[0] * 0.55, c[1] * 0.55, c[2] * 0.55]),
                    op("w", &[1.0]),
                    op("re", &[0.5, 0.5, w - 1.0, h - 1.0]),
                    op("B", &[]),
                    op("w", &[1.2]),
                    op("m", &[w * 0.22, h * 0.68]),
                    op("l", &[w * 0.78, h * 0.68]),
                    op("m", &[w * 0.22, h * 0.5]),
                    op("l", &[w * 0.78, h * 0.5]),
                    op("m", &[w * 0.22, h * 0.32]),
                    op("l", &[w * 0.6, h * 0.32]),
                    op("S", &[]),
                    op("Q", &[]),
                ];
                let ap = appearance(doc, [0.0, 0.0, w, h], upright(g.rotate), Dictionary::new(), ops)?;
                (rect, ap)
            }
            Kind::Ink | Kind::Signature => {
                let strokes = strokes_pdf(a, &g);
                if strokes.is_empty() {
                    continue;
                }
                let pts: Vec<[f32; 4]> = strokes.iter().flatten().map(|p| [p.0, p.1, p.0, p.1]).collect();
                let u = union(&pts);
                let pad = a.width;
                let rect = [u[0] - pad, u[1] - pad, u[2] + pad, u[3] + pad];
                if a.kind == Kind::Ink {
                    d.set("Subtype", "Ink");
                    d.set("InkList", strokes.iter().map(|s| Object::Array(s.iter().flat_map(|p| [real(p.0), real(p.1)]).collect())).collect::<Vec<_>>());
                    d.set("BS", dictionary! { "W" => real(a.width) });
                } else {
                    d.set("Subtype", "Stamp");
                    d.set("Name", "PlumeSignature");
                    d.set("Contents", text_string("Signature"));
                }
                let ap = appearance(doc, rect, None, Dictionary::new(), stroke_ops(&strokes, c, a.width))?;
                (rect, ap)
            }
            Kind::Text => {
                let rect = g.pdf_rect(a.rect);
                let (w, h) = (a.rect[2], a.rect[3]);
                let size = a.width.clamp(4.0, 96.0);
                d.set("Subtype", "FreeText");
                d.set("Contents", text_string(&a.text));
                d.set("DA", Object::string_literal(format!("/Helv {size} Tf {} {} {} rg", c[0], c[1], c[2])));
                d.set("BS", dictionary! { "W" => 0 });
                let res = dictionary! { "Font" => dictionary! { "Helv" => helv() } };
                let mut ops = vec![op("q", &[]), op("BT", &[]), op_o("Tf", vec![Object::Name(b"Helv".to_vec()), real(size)]), op("rg", &c)];
                let lead = size * 1.2;
                ops.push(op("Td", &[2.0, h - 2.0 - size * 0.9]));
                for (k, line) in wrap(&a.text, (w - 4.0).max(size), size).iter().enumerate() {
                    if k > 0 {
                        ops.push(op("Td", &[0.0, -lead]));
                    }
                    ops.push(op_o("Tj", vec![Object::String(winansi(line), StringFormat::Hexadecimal)]));
                }
                ops.extend([op("ET", &[]), op("Q", &[])]);
                let ap = appearance(doc, [0.0, 0.0, w, h], upright(g.rotate), res, ops)?;
                (rect, ap)
            }
        };
        d.set("Rect", rect_obj(rect));
        d.set("AP", dictionary! { "N" => ap });
        let id = doc.add_object(d);
        add_to_page(doc, page, id)?;
    }
    Ok(())
}

/// An annotation found in a file.
#[cfg(any(test, feature = "test-hook"))]
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FileAnnot {
    pub page: usize,
    pub subtype: String,
    /// PDF coordinates (x0, y0, x1, y1).
    pub rect: [f32; 4],
    pub contents: String,
    pub has_appearance: bool,
}

#[cfg(any(test, feature = "test-hook"))]
pub fn read_annots(doc: &Document) -> Vec<FileAnnot> {
    let mut out = Vec::new();
    for (i, p) in page_ids(doc).into_iter().enumerate() {
        let Ok(list) = doc.get_page_annotations(p) else { continue };
        for a in list {
            out.push(FileAnnot {
                page: i,
                subtype: a.get(b"Subtype").and_then(Object::as_name).map(|n| String::from_utf8_lossy(n).into_owned()).unwrap_or_default(),
                rect: a.get(b"Rect").ok().and_then(|r| rect_of(doc, r)).unwrap_or_default(),
                contents: a.get(b"Contents").ok().and_then(|c| decode_text_string(deref(doc, c)).ok()).unwrap_or_default(),
                has_appearance: a.has(b"AP"),
            });
        }
    }
    out
}

// ───────────────────────── forms ─────────────────────────

/// A form field's place on a page (one per widget): a text field or a checkbox.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Field {
    /// The field (holds the value).
    pub id: ObjectId,
    /// The box drawn on the page.
    pub widget: ObjectId,
    pub name: String,
    pub check: bool,
    pub page: usize,
    /// PDF coordinates (x0, y0, x1, y1).
    pub rect: [f32; 4],
    pub value: String,
    pub on: bool,
    pub multiline: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue {
    Text(String),
    Check(bool),
}

fn acroform_id(doc: &mut Document) -> Res<ObjectId> {
    let cur = doc.catalog().map_err(e)?.get(b"AcroForm").ok().cloned();
    match cur {
        Some(Object::Reference(r)) => Ok(r),
        Some(Object::Dictionary(d)) => {
            let id = doc.add_object(d);
            doc.catalog_mut().map_err(e)?.set("AcroForm", id);
            Ok(id)
        }
        _ => {
            let id = doc.add_object(dictionary! { "Fields" => Vec::<Object>::new(), "DA" => Object::string_literal("/Helv 0 Tf 0 g") });
            doc.catalog_mut().map_err(e)?.set("AcroForm", id);
            Ok(id)
        }
    }
}

fn name_of(o: Option<&Object>) -> Option<Vec<u8>> {
    o.and_then(|o| o.as_name().ok()).map(<[u8]>::to_vec)
}

/// The name of a checkbox's "on" state (often "Yes").
fn on_name(doc: &Document, widget: &Dictionary) -> Vec<u8> {
    widget
        .get(b"AP")
        .ok()
        .and_then(|ap| deref(doc, ap).as_dict().ok())
        .and_then(|ap| ap.get(b"N").ok())
        .and_then(|n| deref(doc, n).as_dict().ok())
        .and_then(|n| n.iter().map(|(k, _)| k.clone()).find(|k| k != b"Off"))
        .unwrap_or_else(|| b"Yes".to_vec())
}

pub fn fields(doc: &Document) -> Vec<Field> {
    let pages = page_ids(doc);
    let index: HashMap<ObjectId, usize> = pages.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    // widget → page, from the pages' annotation lists
    let mut on_page: HashMap<ObjectId, usize> = HashMap::new();
    for (i, p) in pages.iter().enumerate() {
        if let Ok(d) = doc.get_dictionary(*p)
            && let Ok(a) = d.get(b"Annots")
            && let Ok(a) = deref(doc, a).as_array()
        {
            for r in a {
                if let Ok(id) = r.as_reference() {
                    on_page.insert(id, i);
                }
            }
        }
    }
    let Some(roots) = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .and_then(|f| deref(doc, f).as_dict().ok())
        .and_then(|f| f.get(b"Fields").ok())
        .and_then(|f| deref(doc, f).as_array().ok())
        .cloned()
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // (field, parent's name, inherited type, inherited flags, depth)
    type Node = (ObjectId, String, Option<Vec<u8>>, i64, u32);
    let mut stack: Vec<Node> = roots.iter().rev().filter_map(|r| r.as_reference().ok()).map(|id| (id, String::new(), None, 0, 0)).collect();
    while let Some((id, parent, ft, ff, depth)) = stack.pop() {
        let Ok(d) = doc.get_dictionary(id) else { continue };
        if depth > 32 {
            continue;
        }
        let t = d.get(b"T").ok().and_then(|t| decode_text_string(deref(doc, t)).ok());
        let name = match (&t, parent.is_empty()) {
            (Some(t), true) => t.clone(),
            (Some(t), false) => format!("{parent}.{t}"),
            (None, _) => parent.clone(),
        };
        let ft = name_of(d.get(b"FT").ok()).or(ft);
        let ff = d.get(b"Ff").and_then(Object::as_i64).unwrap_or(ff);
        let kids: Vec<ObjectId> = d
            .get(b"Kids")
            .ok()
            .and_then(|k| deref(doc, k).as_array().ok())
            .map(|k| k.iter().filter_map(|r| r.as_reference().ok()).collect())
            .unwrap_or_default();
        let kid_fields = kids.iter().any(|k| doc.get_dictionary(*k).is_ok_and(|kd| kd.has(b"T")));
        if kid_fields {
            for k in kids.into_iter().rev() {
                stack.push((k, name.clone(), ft.clone(), ff, depth + 1));
            }
            continue;
        }
        let check = match ft.as_deref() {
            Some(b"Tx") => false,
            // not push buttons (1 << 16) nor radio buttons (1 << 15)
            Some(b"Btn") if ff & (1 << 16) == 0 && ff & (1 << 15) == 0 => true,
            _ => continue,
        };
        let widgets = if kids.is_empty() { vec![id] } else { kids };
        let value = d.get(b"V").ok().map(|v| deref(doc, v));
        for w in widgets {
            let Ok(wd) = doc.get_dictionary(w) else { continue };
            let Some(rect) = wd.get(b"Rect").ok().and_then(|r| rect_of(doc, r)) else { continue };
            let page = wd.get(b"P").and_then(Object::as_reference).ok().and_then(|p| index.get(&p).copied()).or_else(|| on_page.get(&w).copied());
            let Some(page) = page else { continue };
            let (text, on) = if check {
                let v = value.and_then(|v| v.as_name().ok()).map(<[u8]>::to_vec).or_else(|| name_of(wd.get(b"AS").ok()));
                (String::new(), v.is_some_and(|v| v != b"Off"))
            } else {
                (value.and_then(|v| decode_text_string(v).ok()).unwrap_or_default(), false)
            };
            out.push(Field { id, widget: w, name: name.clone(), check, page, rect, value: text, on, multiline: ff & (1 << 12) != 0 });
        }
    }
    out
}

/// Fills form fields (`id`: the field, as [`fields`] gives it) and draws their new look.
pub fn fill(doc: &mut Document, values: &[(ObjectId, FieldValue)]) -> Res<()> {
    let all = fields(doc);
    for (id, v) in values {
        let id: ObjectId = *id;
        let widgets: Vec<&Field> = all.iter().filter(|f| f.id == id).collect();
        match v {
            FieldValue::Text(text) => {
                doc.get_dictionary_mut(id).map_err(e)?.set("V", text_string(text));
                for f in widgets {
                    let (w, h) = (f.rect[2] - f.rect[0], f.rect[3] - f.rect[1]);
                    let size = if f.multiline { 10.0f32.min(h * 0.8) } else { (h * 0.62).clamp(4.0, 12.0) };
                    let mut ops = vec![
                        op_o("BMC", vec![Object::Name(b"Tx".to_vec())]),
                        op("q", &[]),
                        op("re", &[1.0, 1.0, w - 2.0, h - 2.0]),
                        op("W", &[]),
                        op("n", &[]),
                        op("BT", &[]),
                        op_o("Tf", vec![Object::Name(b"Helv".to_vec()), real(size)]),
                        op("g", &[0.0]),
                    ];
                    let lines = if f.multiline { wrap(text, w - 4.0, size) } else { vec![text.replace('\n', " ")] };
                    let y0 = if f.multiline { h - 2.0 - size * 0.9 } else { (h - size * 0.7) / 2.0 };
                    ops.push(op("Td", &[2.0, y0]));
                    for (k, line) in lines.iter().enumerate() {
                        if k > 0 {
                            ops.push(op("Td", &[0.0, -size * 1.15]));
                        }
                        ops.push(op_o("Tj", vec![Object::String(winansi(line), StringFormat::Hexadecimal)]));
                    }
                    ops.extend([op("ET", &[]), op("Q", &[]), op("EMC", &[])]);
                    let res = dictionary! { "Font" => dictionary! { "Helv" => helv() } };
                    let ap = appearance(doc, [0.0, 0.0, w, h], None, res, ops)?;
                    doc.get_dictionary_mut(f.widget).map_err(e)?.set("AP", dictionary! { "N" => ap });
                }
            }
            FieldValue::Check(on) => {
                let mut first_on = None;
                for f in widgets {
                    let wd = doc.get_dictionary(f.widget).map_err(e)?;
                    let name = on_name(doc, wd);
                    // a box with no look of its own gets one: a check mark
                    let has_ap = wd.get(b"AP").ok().and_then(|ap| deref(doc, ap).as_dict().ok()).is_some_and(|ap| ap.has(b"N"));
                    if !has_ap {
                        let (w, h) = (f.rect[2] - f.rect[0], f.rect[3] - f.rect[1]);
                        let yes = appearance(
                            doc,
                            [0.0, 0.0, w, h],
                            None,
                            Dictionary::new(),
                            vec![
                                op("q", &[]),
                                op("w", &[(w.min(h) * 0.12).max(1.0)]),
                                op("J", &[1.0]),
                                op("j", &[1.0]),
                                op("RG", &[0.0, 0.0, 0.0]),
                                op("m", &[w * 0.2, h * 0.52]),
                                op("l", &[w * 0.42, h * 0.28]),
                                op("l", &[w * 0.8, h * 0.76]),
                                op("S", &[]),
                                op("Q", &[]),
                            ],
                        )?;
                        let off = appearance(doc, [0.0, 0.0, w, h], None, Dictionary::new(), vec![])?;
                        doc.get_dictionary_mut(f.widget).map_err(e)?.set("AP", dictionary! { "N" => dictionary! { name.clone() => yes, "Off" => off } });
                    }
                    let state = if *on { name.clone() } else { b"Off".to_vec() };
                    doc.get_dictionary_mut(f.widget).map_err(e)?.set("AS", Object::Name(state));
                    first_on.get_or_insert(name);
                }
                let v = if *on { first_on.unwrap_or_else(|| b"Yes".to_vec()) } else { b"Off".to_vec() };
                doc.get_dictionary_mut(id).map_err(e)?.set("V", Object::Name(v));
            }
        }
    }
    if !values.is_empty() {
        let form = acroform_id(doc)?;
        let f = doc.get_dictionary_mut(form).map_err(e)?;
        f.set("NeedAppearances", true);
        if !f.has(b"DR") {
            f.set("DR", dictionary! { "Font" => dictionary! { "Helv" => helv() } });
        }
    }
    Ok(())
}

// ───────────────────────── sample documents (tests) ─────────────────────────

/// A document with one page of text per entry (A4, Helvetica 14, one line per "\n"), for the tests.
#[cfg(any(test, feature = "test-hook"))]
pub fn sample(pages: &[&str]) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let widths: Vec<Object> = (32..127u8).map(|c| Object::Integer(helv_width(c as char) as i64)).collect();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        "FirstChar" => 32, "LastChar" => 126, "Widths" => widths,
    });
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
    let mut kids = Vec::new();
    for text in pages {
        let mut ops = vec![Operation::new("BT", vec![]), Operation::new("Tf", vec!["F1".into(), 14.into()]), Operation::new("Td", vec![60.into(), 760.into()])];
        for (k, line) in text.split('\n').enumerate() {
            if k > 0 {
                ops.push(Operation::new("Td", vec![0.into(), (-22).into()]));
            }
            ops.push(Operation::new("Tj", vec![Object::String(winansi(line), StringFormat::Literal)]));
        }
        ops.push(Operation::new("ET", vec![]));
        let content = doc.add_object(Stream::new(dictionary! {}, Content { operations: ops }.encode().unwrap()));
        let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => content });
        kids.push(Object::Reference(page));
    }
    let n = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => n, "Resources" => resources,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    doc
}

/// A one-page form: a text field "name" and a checkbox "agree".
#[cfg(any(test, feature = "test-hook"))]
pub fn sample_form() -> Document {
    let mut doc = sample(&["Name:\n\nI agree:"]);
    let page = page_ids(&doc)[0];
    let tx = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "T" => Object::string_literal("name"),
        "Rect" => vec![120.into(), 752.into(), 360.into(), 776.into()], "P" => page, "F" => 4,
        "DA" => Object::string_literal("/Helv 12 Tf 0 g"), "MK" => dictionary! { "BC" => vec![0.into()] },
    });
    let cb = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "T" => Object::string_literal("agree"),
        "Rect" => vec![120.into(), 708.into(), 136.into(), 724.into()], "P" => page, "F" => 4, "V" => "Off", "AS" => "Off",
        "MK" => dictionary! { "BC" => vec![0.into()] },
    });
    doc.get_dictionary_mut(page).unwrap().set("Annots", vec![Object::Reference(tx), Object::Reference(cb)]);
    let form = doc.add_object(dictionary! { "Fields" => vec![Object::Reference(tx), Object::Reference(cb)], "DA" => Object::string_literal("/Helv 0 Tf 0 g") });
    doc.catalog_mut().unwrap().set("AcroForm", form);
    doc
}

/// A document whose bookmarks point to its pages ("Part 1" → page 1…).
#[cfg(any(test, feature = "test-hook"))]
pub fn with_outline(mut doc: Document) -> Document {
    for (i, p) in page_ids(&doc).into_iter().enumerate() {
        doc.add_bookmark(lopdf::Bookmark::new(format!("Part {}", i + 1), [0.0, 0.0, 0.0], 0, p), None);
    }
    if let Some(o) = doc.build_outline() {
        doc.catalog_mut().unwrap().set("Outlines", o);
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(doc: &Document) -> Vec<String> {
        let n = page_ids(doc).len() as u32;
        (1..=n).map(|i| doc.extract_text(&[i]).unwrap_or_default().trim().to_string()).collect()
    }

    fn reload(doc: &Document) -> Document {
        load(&to_bytes(doc, None).unwrap(), None).unwrap()
    }

    fn five() -> Document {
        sample(&["one", "two", "three", "four", "five"])
    }

    #[test]
    fn geometry_round_trips_every_rotation() {
        for rotate in [0, 90, 180, 270] {
            let g = PageGeom { x0: 10.0, y0: 20.0, x1: 610.0, y1: 820.0, rotate };
            let (w, h) = g.size();
            assert_eq!((w, h), if rotate % 180 == 0 { (600.0, 800.0) } else { (800.0, 600.0) });
            for (x, y) in [(0.0, 0.0), (w, h), (12.5, 40.0)] {
                let (px, py) = g.pdf_point(x, y);
                assert!((10.0 - 1e-3..=610.0 + 1e-3).contains(&px) && (20.0 - 1e-3..=820.0 + 1e-3).contains(&py));
                let (bx, by) = g.view_point(px, py);
                assert!((bx - x).abs() < 1e-3 && (by - y).abs() < 1e-3, "{rotate}: {x},{y} → {bx},{by}");
            }
            let r = g.view_rect(g.pdf_rect([5.0, 6.0, 30.0, 40.0]));
            assert!((r[0] - 5.0).abs() < 1e-3 && (r[3] - 40.0).abs() < 1e-3);
        }
        // the top-left corner of an upright page is the top of the PDF box
        assert_eq!(PageGeom::default().pdf_point(0.0, 0.0), (0.0, 792.0));
        // turned a quarter clockwise, the displayed top-left is the PDF's bottom-left
        assert_eq!(PageGeom { rotate: 90, ..Default::default() }.pdf_point(0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn geometry_is_inherited_from_the_page_tree() {
        let doc = five();
        let g = geometry(&doc, page_ids(&doc)[0]);
        assert_eq!((g.x1, g.y1, g.rotate), (595.0, 842.0, 0));
    }

    #[test]
    fn order_move_delete() {
        assert_eq!(moved_order(5, &[3], 0), vec![3, 0, 1, 2, 4]);
        assert_eq!(moved_order(5, &[0], 5), vec![1, 2, 3, 4, 0]);
        assert_eq!(moved_order(5, &[1, 2], 4), vec![0, 3, 1, 2, 4]);
        assert_eq!(moved_order(5, &[2], 2), vec![0, 1, 2, 3, 4]);
        let mut doc = five();
        move_pages(&mut doc, &[4], 0).unwrap();
        assert_eq!(texts(&reload(&doc)), ["five", "one", "two", "three", "four"]);
        delete_pages(&mut doc, &[0, 2]).unwrap();
        let back = reload(&doc);
        assert_eq!(texts(&back), ["one", "three", "four"]);
        // geometry survives the flattening (MediaBox was on the tree's root)
        assert_eq!(geometry(&back, page_ids(&back)[0]).x1, 595.0);
        assert!(delete_pages(&mut doc, &[0, 1, 2]).is_err());
        assert!(set_order(&mut doc, &[0, 0]).is_err());
    }

    #[test]
    fn rotate_turns_and_wraps() {
        let mut doc = five();
        rotate_pages(&mut doc, &[1], 90).unwrap();
        rotate_pages(&mut doc, &[1, 2], -90).unwrap();
        let back = reload(&doc);
        let g = geometries(&back);
        assert_eq!(g.iter().map(|g| g.rotate).collect::<Vec<_>>(), [0, 0, 270, 0, 0]);
        assert_eq!(g[2].size(), (842.0, 595.0));
    }

    #[test]
    fn extract_and_split() {
        let doc = with_outline(five());
        let x = extract(&doc, &[3, 1]).unwrap();
        assert_eq!(texts(&reload(&x)), ["four", "two"]);
        assert!(x.catalog().unwrap().get(b"Outlines").is_err());
        assert_eq!(parse_ranges("1-2, 4, 5-", 5).unwrap(), vec![(0, 1), (3, 3), (4, 4)]);
        assert_eq!(parse_ranges("-2", 5).unwrap(), vec![(0, 1)]);
        assert!(parse_ranges("0-2", 5).is_err() && parse_ranges("3-9", 5).is_err() && parse_ranges("x", 5).is_err() && parse_ranges(" ", 5).is_err());
        let parts = split(&doc, &parse_ranges("1-2,3-5", 5).unwrap()).unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(texts(&reload(&parts[0])), ["one", "two"]);
        assert_eq!(texts(&reload(&parts[1])), ["three", "four", "five"]);
        // the original is untouched
        assert_eq!(page_ids(&doc).len(), 5);
    }

    #[test]
    fn merge_keeps_every_page_and_form() {
        let a = sample(&["a1", "a2"]);
        let b = sample(&["b1"]);
        let f = sample_form();
        let m = merge(vec![a, b, f]).unwrap();
        let back = reload(&m);
        let t = texts(&back);
        assert_eq!(&t[..3], ["a1", "a2", "b1"]);
        assert!(t[3].starts_with("Name"));
        let fl = fields(&back);
        assert_eq!(fl.len(), 2);
        assert!(fl.iter().all(|f| f.page == 3));
    }

    #[test]
    fn outline_is_read() {
        let doc = reload(&with_outline(sample(&["x", "y", "z"])));
        let o = outline(&doc);
        assert_eq!(o.len(), 3);
        assert_eq!((o[2].title.as_str(), o[2].page, o[2].level), ("Part 3", 2, 0));
    }

    fn all_kinds() -> Vec<Annot> {
        let base =
            Annot { kind: Kind::Highlight, page: 0, rect: [0.0; 4], boxes: vec![], strokes: vec![], text: String::new(), color: [1.0, 0.85, 0.2], width: 2.0 };
        vec![
            Annot { boxes: vec![[60.0, 66.0, 40.0, 18.0], [60.0, 88.0, 30.0, 18.0]], ..base.clone() },
            Annot { kind: Kind::Note, rect: [300.0, 100.0, 22.0, 20.0], text: "Check this — été".into(), ..base.clone() },
            Annot {
                kind: Kind::Ink,
                rect: [100.0, 200.0, 50.0, 30.0],
                strokes: vec![vec![[0.0, 0.0], [0.5, 1.0], [1.0, 0.2]]],
                color: [0.0, 0.3, 0.9],
                ..base.clone()
            },
            Annot {
                kind: Kind::Text,
                page: 1,
                rect: [80.0, 300.0, 160.0, 60.0],
                text: "Hello (world) \\ café".into(),
                color: [0.0, 0.0, 0.0],
                width: 12.0,
                ..base.clone()
            },
            Annot {
                kind: Kind::Signature,
                page: 1,
                rect: [300.0, 600.0, 120.0, 40.0],
                strokes: vec![vec![[0.0, 0.8], [0.3, 0.1], [0.6, 0.9], [1.0, 0.3]]],
                color: [0.05, 0.1, 0.4],
                width: 1.6,
                ..base
            },
        ]
    }

    #[test]
    fn annotations_are_written_and_read_back() {
        let mut doc = sample(&["first page", "second page"]);
        add_annots(&mut doc, &all_kinds()).unwrap();
        let back = reload(&doc);
        let a = read_annots(&back);
        let kinds: Vec<(usize, &str)> = a.iter().map(|a| (a.page, a.subtype.as_str())).collect();
        assert_eq!(kinds, [(0, "Highlight"), (0, "Text"), (0, "Ink"), (1, "FreeText"), (1, "Stamp")]);
        assert!(a.iter().all(|a| a.has_appearance));
        assert_eq!(a[1].contents, "Check this — été");
        assert_eq!(a[3].contents, "Hello (world) \\ café");
        // the highlight covers both lines: x 60..100, y from 842-106 to 842-66
        let r = a[0].rect;
        assert!((r[0] - 60.0).abs() < 0.01 && (r[2] - 100.0).abs() < 0.01 && (r[1] - 736.0).abs() < 0.01 && (r[3] - 776.0).abs() < 0.01);
        // the signature is where it was placed: strokes from y 604 to 636 (view), so 206..238 in PDF, padded by 1.6
        let s = a[4].rect;
        assert!((s[1] - 204.4).abs() < 0.01 && (s[3] - 239.6).abs() < 0.01, "{s:?}");
        assert!((s[0] - (300.0 - 1.6)).abs() < 0.01 && (s[2] - (420.0 + 1.6)).abs() < 0.01);
        // a second save keeps them (and adds)
        let mut again = back.clone();
        add_annots(&mut again, &all_kinds()[..1]).unwrap();
        assert_eq!(read_annots(&reload(&again)).len(), 6);
    }

    #[test]
    fn annotations_on_a_turned_page() {
        let mut doc = sample(&["x"]);
        rotate_pages(&mut doc, &[0], 90).unwrap();
        let note = Annot {
            kind: Kind::Note,
            page: 0,
            rect: [10.0, 20.0, 22.0, 20.0],
            boxes: vec![],
            strokes: vec![],
            text: "n".into(),
            color: [1.0, 0.8, 0.2],
            width: 1.0,
        };
        add_annots(&mut doc, &[note]).unwrap();
        let a = read_annots(&reload(&doc));
        // view (10, 20) on a page turned 90°: PDF x = 20, y = 10
        assert_eq!(a[0].rect, [20.0, 10.0, 40.0, 32.0]);
    }

    #[test]
    fn forms_are_found_and_filled() {
        let mut doc = sample_form();
        let f = fields(&doc);
        assert_eq!(f.len(), 2);
        let name = f.iter().find(|f| f.name == "name").unwrap().clone();
        let agree = f.iter().find(|f| f.name == "agree").unwrap().clone();
        assert!(!name.check && agree.check && !agree.on && name.value.is_empty());
        fill(&mut doc, &[(name.id, FieldValue::Text("Ada Lovelace".into())), (agree.id, FieldValue::Check(true))]).unwrap();
        let back = reload(&doc);
        let f = fields(&back);
        assert_eq!(f.iter().find(|f| f.name == "name").unwrap().value, "Ada Lovelace");
        assert!(f.iter().find(|f| f.name == "agree").unwrap().on);
        // unchecking writes "Off"
        let mut doc = back;
        fill(&mut doc, &[(agree.id, FieldValue::Check(false))]).unwrap();
        assert!(!fields(&reload(&doc)).iter().find(|f| f.name == "agree").unwrap().on);
    }

    #[test]
    fn protected_files_need_their_password_and_stay_protected() {
        let doc = sample(&["secret page"]);
        let bytes = to_bytes(&doc, Some("hunter2")).unwrap();
        assert_eq!(load(&bytes, None).unwrap_err(), LoadError::Password);
        assert_eq!(load(&bytes, Some("wrong")).unwrap_err(), LoadError::Password);
        let open = load(&bytes, Some("hunter2")).unwrap();
        assert_eq!(texts(&open), ["secret page"]);
        // saved again with the password: still protected
        let again = to_bytes(&open, Some("hunter2")).unwrap();
        assert_eq!(load(&again, None).unwrap_err(), LoadError::Password);
    }

    #[test]
    fn text_helpers() {
        assert_eq!(winansi("a€é—✓"), vec![b'a', 0x80, 0xe9, 0x97, b'?']);
        assert_eq!(wrap("aaa bbb ccc", text_width_pt("aaa bbb", 10.0) + 0.1, 10.0), vec!["aaa bbb", "ccc"]);
        assert_eq!(wrap("one\ntwo", 1000.0, 10.0), vec!["one", "two"]);
        assert!(pdf_date().starts_with("D:20") && pdf_date().len() == 17);
    }

    #[test]
    fn saving_goes_through_a_temporary_file() {
        let dir = std::env::temp_dir().join(format!("plume-pdf-edit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("out.pdf");
        save(&five(), &p, None).unwrap();
        assert_eq!(texts(&load(&std::fs::read(&p).unwrap(), None).unwrap()).len(), 5);
        assert!(!dir.join("out.pdf.plume-tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
