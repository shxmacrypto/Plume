//! Import des profils d'autres navigateurs : historique + favoris (Chromium & Firefox),
//! et mots de passe depuis un export CSV (Chrome/Edge/Brave/Opera/Firefox/Bitwarden…).
use serde_json::{Value, json};
use std::{env, fs, path::{Path, PathBuf}};

use crate::sqlite;

/// Navigateurs Chromium connus : (nom, dossier de données relatif à %LOCALAPPDATA% ou %APPDATA%, roaming ?)
const CHROMIUMS: &[(&str, &str, bool)] = &[
    ("Google Chrome", r"Google\Chrome\User Data", false),
    ("Microsoft Edge", r"Microsoft\Edge\User Data", false),
    ("Brave", r"BraveSoftware\Brave-Browser\User Data", false),
    ("Vivaldi", r"Vivaldi\User Data", false),
    ("Chromium", r"Chromium\User Data", false),
    ("Opera", r"Opera Software\Opera Stable", true),
    ("Opera GX", r"Opera Software\Opera GX Stable", true),
];

/// Liste les profils trouvés sur ce PC.
pub fn detect() -> Vec<Value> {
    let local = env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default();
    let roaming = env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    let mut out = vec![];
    for (name, rel, roam) in CHROMIUMS {
        let root = if *roam { roaming.join(rel) } else { local.join(rel) };
        if !root.exists() {
            continue;
        }
        if *roam {
            // Opera : le dossier est directement le profil
            if root.join("History").exists() || root.join("Bookmarks").exists() {
                out.push(json!({"browser": name, "profile": "", "kind": "chromium", "path": root}));
            }
            continue;
        }
        let names: Value = fs::read_to_string(root.join("Local State"))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .map(|v| v["profile"]["info_cache"].clone())
            .unwrap_or_default();
        let Ok(dirs) = fs::read_dir(&root) else { continue };
        for d in dirs.flatten() {
            let dn = d.file_name().to_string_lossy().into_owned();
            if !(dn == "Default" || dn.starts_with("Profile ")) {
                continue;
            }
            let p = d.path();
            if !(p.join("History").exists() || p.join("Bookmarks").exists()) {
                continue;
            }
            let label = names[&dn]["name"].as_str().unwrap_or(&dn).to_string();
            out.push(json!({"browser": name, "profile": label, "kind": "chromium", "path": p}));
        }
    }
    // Firefox : profiles.ini
    let ff = roaming.join(r"Mozilla\Firefox");
    if let Ok(ini) = fs::read_to_string(ff.join("profiles.ini")) {
        let (mut name, mut path, mut rel) = (String::new(), String::new(), true);
        let flush = |name: &str, path: &str, rel: bool, out: &mut Vec<Value>| {
            if path.is_empty() {
                return;
            }
            let p = if rel { ff.join(path.replace('/', "\\")) } else { PathBuf::from(path) };
            if p.join("places.sqlite").exists() {
                out.push(json!({"browser": "Firefox", "profile": name, "kind": "firefox", "path": p}));
            }
        };
        for line in ini.lines().map(str::trim) {
            if line.starts_with('[') {
                flush(&name, &path, rel, &mut out);
                (name, path, rel) = (String::new(), String::new(), true);
            } else if let Some(v) = line.strip_prefix("Name=") {
                name = v.into();
            } else if let Some(v) = line.strip_prefix("Path=") {
                path = v.into();
            } else if let Some(v) = line.strip_prefix("IsRelative=") {
                rel = v == "1";
            }
        }
        flush(&name, &path, rel, &mut out);
    }
    for (i, v) in out.iter_mut().enumerate() {
        v["id"] = json!(i);
    }
    out
}

/// Copie un fichier de base (et son journal WAL) dans %TEMP% : le navigateur peut être ouvert.
fn temp_copy(src: &Path, tag: &str) -> Option<PathBuf> {
    let dst = env::temp_dir().join(format!("plume-import-{tag}-{}", std::process::id()));
    fs::copy(src, &dst).ok()?;
    for ext in ["-wal", "-journal"] {
        let s = PathBuf::from(format!("{}{ext}", src.display()));
        if s.exists() {
            let _ = fs::copy(&s, format!("{}{ext}", dst.display()));
        }
    }
    Some(dst)
}

fn cleanup(p: &Path) {
    for ext in ["", "-wal", "-journal", "-shm"] {
        let _ = fs::remove_file(format!("{}{ext}", p.display()));
    }
}

fn http(u: &str) -> bool {
    u.starts_with("http://") || u.starts_with("https://")
}

/// Importe depuis les profils choisis. Renvoie {history, bookmarks, errors}.
pub fn run(sources: &[Value], hist: bool, bm: bool) -> Value {
    let (mut history, mut bookmarks, mut errors) = (vec![], vec![], vec![]);
    for (n, s) in sources.iter().enumerate() {
        let path = PathBuf::from(s["path"].as_str().unwrap_or(""));
        let label = format!("{} {}", s["browser"].as_str().unwrap_or(""), s["profile"].as_str().unwrap_or("")).trim().to_string();
        let r = match s["kind"].as_str() {
            Some("firefox") => firefox(&path, hist, bm, n, &mut history, &mut bookmarks),
            _ => chromium(&path, hist, bm, n, &mut history, &mut bookmarks),
        };
        if let Err(e) = r {
            errors.push(format!("{label} : {e}"));
        }
    }
    json!({"history": history, "bookmarks": bookmarks, "errors": errors})
}

fn chromium(p: &Path, hist: bool, bm: bool, n: usize, h: &mut Vec<Value>, b: &mut Vec<Value>) -> Result<(), String> {
    let b0 = b.len();
    if bm {
        if let Ok(s) = fs::read_to_string(p.join("Bookmarks")) {
            let v: Value = serde_json::from_str(&s).map_err(|_| "favoris illisibles")?;
            fn walk(v: &Value, b: &mut Vec<Value>) {
                if v["type"] == "url" {
                    let u = v["url"].as_str().unwrap_or("");
                    if http(u) {
                        b.push(json!({"u": u, "t": v["name"], "f": ""}));
                    }
                }
                for c in v["children"].as_array().into_iter().flatten() {
                    walk(c, b);
                }
            }
            for root in ["bookmark_bar", "other", "synced"] {
                walk(&v["roots"][root], b);
            }
        }
        // Favicons des favoris : lus dans la base « Favicons » du navigateur (image ≤ 64 px la plus grande)
        if b.len() > b0 && p.join("Favicons").exists() {
            if let Some(tmp) = temp_copy(&p.join("Favicons"), &format!("i{n}")) {
                let list: Vec<String> = b[b0..].iter().filter_map(|x| x["u"].as_str()).map(|u| format!("'{}'", u.replace('\x27', "''"))).collect();
                let sql = format!(
                    "SELECT m.page_url, hex(f.image_data) FROM icon_mapping m JOIN favicon_bitmaps f ON f.icon_id = m.icon_id                      WHERE m.page_url IN ({}) AND f.width <= 64 AND length(f.image_data) < 20000 ORDER BY f.width ASC",
                    list.join(",")
                );
                if let Ok(rows) = sqlite::query(&tmp, &sql) {
                    let mut icons = std::collections::HashMap::new();
                    for r in rows {
                        icons.insert(r[0].clone(), r[1].clone()); // la plus grande gagne (tri croissant)
                    }
                    for x in &mut b[b0..] {
                        if let Some(hex) = x["u"].as_str().and_then(|u| icons.get(u)) {
                            let bytes: Vec<u8> = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()).collect();
                            x["f"] = json!(format!("data:image/png;base64,{}", crate::ext::b64(&bytes)));
                        }
                    }
                }
                cleanup(&tmp);
            }
        }
    }
    if hist && p.join("History").exists() {
        let tmp = temp_copy(&p.join("History"), &format!("c{n}")).ok_or("historique verrouillé (fermez le navigateur)")?;
        let rows = sqlite::query(
            &tmp,
            "SELECT url, title, visit_count, last_visit_time FROM urls WHERE hidden = 0 ORDER BY last_visit_time DESC LIMIT 2000",
        );
        cleanup(&tmp);
        for r in rows? {
            if !http(&r[0]) {
                continue;
            }
            // microsecondes depuis 1601 → millisecondes Unix
            let t = r[3].parse::<i64>().unwrap_or(0) / 1000 - 11_644_473_600_000;
            h.push(json!({"u": r[0], "t": r[1], "f": "", "c": r[2].parse::<u64>().unwrap_or(1).max(1), "n": t.max(0)}));
        }
    }
    Ok(())
}

fn firefox(p: &Path, hist: bool, bm: bool, n: usize, h: &mut Vec<Value>, b: &mut Vec<Value>) -> Result<(), String> {
    let tmp = temp_copy(&p.join("places.sqlite"), &format!("f{n}")).ok_or("places.sqlite verrouillé (fermez Firefox)")?;
    let res = (|| -> Result<(), String> {
        if hist {
            for r in sqlite::query(
                &tmp,
                "SELECT url, IFNULL(title,''), visit_count, last_visit_date FROM moz_places \
                 WHERE last_visit_date IS NOT NULL AND hidden = 0 ORDER BY last_visit_date DESC LIMIT 2000",
            )? {
                if http(&r[0]) {
                    let t = r[3].parse::<i64>().unwrap_or(0) / 1000; // µs Unix → ms
                    h.push(json!({"u": r[0], "t": r[1], "f": "", "c": r[2].parse::<u64>().unwrap_or(1).max(1), "n": t}));
                }
            }
        }
        if bm {
            for r in sqlite::query(
                &tmp,
                "SELECT p.url, IFNULL(b.title, p.title) FROM moz_bookmarks b JOIN moz_places p ON b.fk = p.id \
                 WHERE b.type = 1 ORDER BY b.position",
            )? {
                if http(&r[0]) {
                    b.push(json!({"u": r[0], "t": r[1], "f": ""}));
                }
            }
        }
        Ok(())
    })();
    cleanup(&tmp);
    res
}

/// CSV de mots de passe exporté par un navigateur : détecte les colonnes url / username / password.
pub fn parse_csv(text: &str) -> Vec<Value> {
    let rows = csv_rows(text.trim_start_matches('\u{feff}'));
    let Some(head) = rows.first() else { return vec![] };
    let col = |names: &[&str]| head.iter().position(|h| names.contains(&h.trim().to_ascii_lowercase().as_str()));
    let (Some(cu), Some(cn), Some(cp)) = (
        col(&["url", "login_uri", "website", "origin"]),
        col(&["username", "login_username", "user", "login", "email"]),
        col(&["password", "login_password"]),
    ) else {
        return vec![];
    };
    rows[1..]
        .iter()
        .filter_map(|r| {
            let (u, n, p) = (r.get(cu)?, r.get(cn)?, r.get(cp)?);
            let host = host_of(u);
            (!host.is_empty() && !p.is_empty()).then(|| json!({"o": host, "u": n, "p": p, "t": crate::now_ms()}))
        })
        .collect()
}

/// Hôte (sans www.) d'une URL ou d'un domaine nu.
pub fn host_of(u: &str) -> String {
    let rest = u.split_once("://").map_or(u, |(_, r)| r);
    let h = rest.split(['/', '?', '#']).next().unwrap_or("");
    let h = h.rsplit_once('@').map_or(h, |(_, h)| h);
    h.trim_start_matches("www.").to_ascii_lowercase()
}

fn csv_rows(s: &str) -> Vec<Vec<String>> {
    let (mut rows, mut row, mut cell, mut q) = (vec![], vec![], String::new(), false);
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match (q, c) {
            (true, '"') if it.peek() == Some(&'"') => {
                cell.push('"');
                it.next();
            }
            (true, '"') => q = false,
            (true, c) => cell.push(c),
            (false, '"') => q = true,
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
    rows
}

#[cfg(test)]
mod tests {
    #[test]
    fn csv_chrome_and_firefox() {
        let chrome = "name,url,username,password,note\r\ngithub.com,https://github.com/login,sam,\"p,a\"\"ss\",\r\n";
        let r = super::parse_csv(chrome);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0]["o"], "github.com");
        assert_eq!(r[0]["p"], "p,a\"ss");
        let ff = "\u{feff}\"url\",\"username\",\"password\",\"httpRealm\"\n\"https://www.discord.com\",\"me\",\"x\",\"\"\n";
        let r = super::parse_csv(ff);
        assert_eq!(r[0]["o"], "discord.com");
        assert_eq!(super::host_of("https://user@www.Example.com:8080/a"), "example.com:8080");
    }

    #[test]
    fn detect_runs() {
        let d = super::detect();
        let r = super::run(&d, true, true);
        println!("DETECT {} profils: {:?}", d.len(), d.iter().map(|x| format!("{} {}", x["browser"], x["profile"])).collect::<Vec<_>>());
        println!("IMPORT history={} bookmarks={} errors={}", r["history"].as_array().unwrap().len(), r["bookmarks"].as_array().unwrap().len(), r["errors"]);
    }
}
