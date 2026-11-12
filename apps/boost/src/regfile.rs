//! Registry files (.reg, "Windows Registry Editor Version 5.00"), written the way regedit exports them (UTF-16 with
//! its byte-order mark) so a snapshot can also be restored with a double click, and read back by Plume Boost to
//! restore it. `"Name"=-` removes a value: a setting that didn't exist at the time of the snapshot.

use crate::backend::{Hive, Raw};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Set(Raw),
    /// The value didn't exist: restoring removes it.
    Delete(String),
}

impl Line {
    pub fn name(&self) -> &str {
        match self {
            Line::Set(r) => &r.name,
            Line::Delete(n) => n,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub hive: Hive,
    pub path: String,
    pub lines: Vec<Line>,
}

fn root(h: Hive) -> &'static str {
    match h {
        Hive::User => "HKEY_CURRENT_USER",
        Hive::Machine => "HKEY_LOCAL_MACHINE",
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(",")
}

/// The text of a .reg file.
pub fn text(keys: &[Key]) -> String {
    let mut out = String::from("Windows Registry Editor Version 5.00\r\n");
    for k in keys {
        out.push_str(&format!("\r\n[{}\\{}]\r\n", root(k.hive), k.path));
        for l in &k.lines {
            let name = if l.name().is_empty() { "@".to_string() } else { quote(l.name()) };
            let value = match l {
                Line::Delete(_) => "-".to_string(),
                Line::Set(r) => match r.ty {
                    1 => {
                        let w: Vec<u16> = r.data.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&c| c != 0).collect();
                        quote(&String::from_utf16_lossy(&w))
                    }
                    4 if r.data.len() == 4 => format!("dword:{:08x}", u32::from_le_bytes([r.data[0], r.data[1], r.data[2], r.data[3]])),
                    3 => format!("hex:{}", hex(&r.data)),
                    t => format!("hex({t:x}):{}", hex(&r.data)),
                },
            };
            out.push_str(&format!("{name}={value}\r\n"));
        }
    }
    out
}

/// The file's bytes: UTF-16 little-endian with a byte-order mark, like regedit's exports.
pub fn bytes(keys: &[Key]) -> Vec<u8> {
    let mut b = vec![0xff, 0xfe];
    b.extend(text(keys).encode_utf16().flat_map(|c| c.to_le_bytes()));
    b
}

/// Reads a .reg file (UTF-16 with BOM, or UTF-8).
pub fn parse_bytes(b: &[u8]) -> Vec<Key> {
    let s = if b.starts_with(&[0xff, 0xfe]) {
        let w: Vec<u16> = b[2..].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        String::from_utf16_lossy(&w)
    } else {
        String::from_utf8_lossy(b).trim_start_matches('\u{feff}').to_string()
    };
    parse(&s)
}

/// A quoted string at the start of `s`: (its text, the rest after it).
fn unquote(s: &str) -> Option<(String, &str)> {
    let mut chars = s.strip_prefix('"')?.char_indices();
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => out.push(chars.next()?.1),
            '"' => return Some((out, &s[i + 2..])),
            c => out.push(c),
        }
    }
    None
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    s.split(',').map(str::trim).filter(|x| !x.is_empty()).map(|x| u8::from_str_radix(x, 16).ok()).collect()
}

pub fn parse(s: &str) -> Vec<Key> {
    // join lines continued with a final backslash
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for raw in s.lines() {
        let t = raw.trim();
        if let Some(head) = t.strip_suffix('\\')
            && !t.starts_with('[')
            && !t.ends_with("\\\\\"")
        {
            cur.push_str(head);
            continue;
        }
        cur.push_str(t);
        lines.push(std::mem::take(&mut cur));
    }
    let mut keys: Vec<Key> = Vec::new();
    for l in lines {
        if let Some(k) = l.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
            let (hive, path) = match k.split_once('\\') {
                Some(("HKEY_CURRENT_USER" | "HKCU", p)) => (Hive::User, p),
                Some(("HKEY_LOCAL_MACHINE" | "HKLM", p)) => (Hive::Machine, p),
                _ => continue,
            };
            keys.push(Key { hive, path: path.to_string(), lines: Vec::new() });
            continue;
        }
        let Some(key) = keys.last_mut() else { continue };
        let (name, rest) = if let Some(r) = l.strip_prefix('@') {
            (String::new(), r)
        } else if let Some(x) = unquote(&l) {
            x
        } else {
            continue;
        };
        let Some(v) = rest.trim_start().strip_prefix('=') else { continue };
        let v = v.trim();
        let line = if v == "-" {
            Line::Delete(name)
        } else if v.starts_with('"') {
            let Some((text, _)) = unquote(v) else { continue };
            Line::Set(Raw { name, ty: 1, data: text.encode_utf16().chain([0]).flat_map(|c| c.to_le_bytes()).collect() })
        } else if let Some(d) = v.strip_prefix("dword:") {
            let Ok(n) = u32::from_str_radix(d.trim(), 16) else { continue };
            Line::Set(Raw { name, ty: 4, data: n.to_le_bytes().to_vec() })
        } else if let Some(h) = v.strip_prefix("hex:") {
            let Some(data) = unhex(h) else { continue };
            Line::Set(Raw { name, ty: 3, data })
        } else if let Some(h) = v.strip_prefix("hex(") {
            let Some((t, d)) = h.split_once("):") else { continue };
            let (Ok(ty), Some(data)) = (u32::from_str_radix(t, 16), unhex(d)) else { continue };
            Line::Set(Raw { name, ty, data })
        } else {
            continue;
        };
        key.lines.push(line);
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Val;

    #[test]
    fn a_reg_file_reads_back_as_written() {
        let keys = vec![
            Key {
                hive: Hive::User,
                path: r"Control Panel\Desktop".into(),
                lines: vec![
                    Line::Set(Raw::from_val("UserPreferencesMask", &Val::Bin(vec![0x9e, 0x1e, 0x07, 0x80]))),
                    Line::Set(Raw::from_val("WallPaper", &Val::Sz(r#"C:\Users\a "b"\img.jpg"#.into()))),
                    Line::Set(Raw { name: "Q".into(), ty: 11, data: vec![1, 0, 0, 0, 0, 0, 0, 0] }),
                    Line::Set(Raw::from_val("", &Val::Sz("default".into()))),
                ],
            },
            Key {
                hive: Hive::Machine,
                path: r"SOFTWARE\Microsoft\Windows\Dwm".into(),
                lines: vec![Line::Set(Raw::from_val("Big", &Val::Dword(0xdead_beef))), Line::Delete("OverlayTestMode".into())],
            },
        ];
        let t = text(&keys);
        assert!(t.starts_with("Windows Registry Editor Version 5.00\r\n"));
        assert!(t.contains("\"Big\"=dword:deadbeef") && t.contains("\"OverlayTestMode\"=-") && t.contains("hex(b):01,00"));
        assert!(t.contains(r#""WallPaper"="C:\\Users\\a \"b\"\\img.jpg""#));
        assert_eq!(parse_bytes(&bytes(&keys)), keys);
        // a value continued over two lines, as regedit writes long ones
        let k = parse("Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\X]\r\n\"B\"=hex:01,02,\\\r\n  03\r\n");
        assert_eq!(k[0].lines, vec![Line::Set(Raw { name: "B".into(), ty: 3, data: vec![1, 2, 3] })]);
    }
}
