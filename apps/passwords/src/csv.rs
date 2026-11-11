//! Importing the CSV files that browsers and other vaults export: Chrome, Edge, Brave, Opera, Firefox, Bitwarden,
//! Plume Web. The columns are found by their names, whatever their order.

use crate::vault::{Entry, Kind};

/// The rows and cells of a CSV text (quotes, doubled quotes, commas and line breaks inside quotes).
pub fn rows(s: &str) -> Vec<Vec<String>> {
    let (mut rows, mut row, mut cell, mut quoted) = (vec![], vec![], String::new(), false);
    let mut it = s.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = it.next() {
        match (quoted, c) {
            (true, '"') if it.peek() == Some(&'"') => {
                cell.push('"');
                it.next();
            }
            (true, '"') => quoted = false,
            (true, c) => cell.push(c),
            (false, '"') => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut cell)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            (false, c) => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].trim().is_empty()));
    rows
}

/// The host of an address, without "www." (a title for entries that have none).
pub fn host_of(u: &str) -> String {
    let rest = u.split_once("://").map_or(u, |(_, r)| r);
    let h = rest.split(['/', '?', '#']).next().unwrap_or("");
    let h = h.rsplit_once('@').map_or(h, |(_, h)| h);
    h.trim_start_matches("www.").to_ascii_lowercase()
}

pub struct Imported {
    pub entries: Vec<Entry>,
    pub folders: Vec<String>,
    /// Rows left out (no password and nothing else worth keeping).
    pub skipped: usize,
}

/// The entries of a password CSV, or None if the columns are not recognized.
pub fn parse(text: &str) -> Option<Imported> {
    let rows = rows(text);
    let head: Vec<String> = rows.first()?.iter().map(|h| h.trim().to_ascii_lowercase()).collect();
    let col = |names: &[&str]| head.iter().position(|h| names.contains(&h.as_str()));
    let c_url = col(&["url", "login_uri", "website", "origin", "web site", "uri"]);
    let c_user = col(&["username", "login_username", "user", "login", "email", "user name"]);
    let c_pw = col(&["password", "login_password"]);
    let c_name = col(&["name", "title"]);
    let c_note = col(&["note", "notes", "extra", "comment"]);
    let c_totp = col(&["login_totp", "totp", "otp", "otpauth"]);
    let c_folder = col(&["folder", "grouping", "group"]);
    let c_fav = col(&["favorite", "fav"]);
    let c_type = col(&["type"]);
    // a password column, or Bitwarden's layout (which also holds notes)
    if c_pw.is_none() && c_type.is_none() {
        return None;
    }
    let get = |r: &Vec<String>, c: Option<usize>| c.and_then(|i| r.get(i)).map(|s| s.trim().to_string()).unwrap_or_default();
    let now = crate::vault::now();
    let mut out = Imported { entries: vec![], folders: vec![], skipped: 0 };
    for r in &rows[1..] {
        let kind = match get(r, c_type).to_ascii_lowercase().as_str() {
            "note" | "securenote" => Kind::Note,
            "card" => Kind::Card,
            "" | "login" => Kind::Login,
            _ => Kind::Login,
        };
        let mut e = Entry::new(kind);
        e.url = get(r, c_url);
        e.username = get(r, c_user);
        // passwords keep their spaces: they are part of them
        e.password = c_pw.and_then(|i| r.get(i)).cloned().unwrap_or_default();
        e.notes = c_note.and_then(|i| r.get(i)).cloned().unwrap_or_default();
        e.totp = get(r, c_totp);
        e.folder = get(r, c_folder);
        e.favorite = matches!(get(r, c_fav).as_str(), "1" | "true" | "yes");
        e.title = get(r, c_name);
        if e.title.is_empty() {
            e.title = host_of(&e.url);
        }
        if e.title.is_empty() {
            e.title = e.username.clone();
        }
        // Firefox's dates (milliseconds)
        if let Some(t) = head.iter().position(|h| h == "timepasswordchanged").and_then(|i| r.get(i)).and_then(|s| s.parse::<u64>().ok()) {
            e.modified = (t / 1000).min(now);
        }
        let useful = match kind {
            Kind::Login => !e.password.is_empty() || !e.totp.is_empty(),
            _ => !e.notes.is_empty() || !e.title.is_empty(),
        };
        if !useful {
            out.skipped += 1;
            continue;
        }
        if !e.folder.is_empty() && !out.folders.contains(&e.folder) {
            out.folders.push(e.folder.clone());
        }
        out.entries.push(e);
    }
    Some(out)
}

/// Is this entry already in the vault (same address host, same user name, same password)?
pub fn same_login(a: &Entry, b: &Entry) -> bool {
    a.kind == Kind::Login && b.kind == Kind::Login && host_of(&a.url) == host_of(&b.url) && a.username == b.username && a.password == b.password
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_edge_plume() {
        let t = "name,url,username,password,note\r\nexample.com,https://example.com/login,tester,\"p,a\"\"ss\",\"two\nlines\"\r\n,https://www.test.org/,me,x,\r\nempty.com,https://empty.com,nobody,,\r\n";
        let r = parse(t).unwrap();
        assert_eq!(r.entries.len(), 2);
        assert_eq!(r.skipped, 1);
        let e = &r.entries[0];
        assert_eq!((e.title.as_str(), e.username.as_str(), e.password.as_str()), ("example.com", "tester", "p,a\"ss"));
        assert_eq!(e.notes, "two\nlines");
        assert_eq!(r.entries[1].title, "test.org");
    }

    #[test]
    fn firefox() {
        let t = "\u{feff}\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\",\"timeCreated\",\"timeLastUsed\",\"timePasswordChanged\"\n\"https://www.example.net\",\"user\",\"pw 1\",,\"https://www.example.net\",\"{x}\",\"1600000000000\",\"1600000000000\",\"1600000001000\"\n";
        let r = parse(t).unwrap();
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].title, "example.net");
        assert_eq!(r.entries[0].password, "pw 1");
        assert_eq!(r.entries[0].modified, 1_600_000_001);
    }

    #[test]
    fn bitwarden() {
        let t = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
                 Work,1,login,Mail,,,0,https://mail.example.com,tester,pw2,GEZDGNBVGY3TQOJQ\n\
                 ,,note,Wifi,the code is on the router,,0,,,,\n";
        let r = parse(t).unwrap();
        assert_eq!(r.entries.len(), 2);
        let m = &r.entries[0];
        assert!(m.favorite);
        assert_eq!((m.folder.as_str(), m.title.as_str(), m.totp.as_str()), ("Work", "Mail", "GEZDGNBVGY3TQOJQ"));
        assert_eq!(r.entries[1].kind, Kind::Note);
        assert_eq!(r.folders, vec!["Work".to_string()]);
    }

    #[test]
    fn not_a_password_file() {
        assert!(parse("a,b,c\n1,2,3\n").is_none());
        assert!(parse("").is_none());
        assert_eq!(host_of("https://user@www.Example.com:8080/a"), "example.com:8080");
    }
}
