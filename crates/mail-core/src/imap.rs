//! Le client IMAP : ce dont Plume Mail a besoin pour lire une boîte, et rien de plus. Les en-têtes des messages
//! sont demandés tels quels au serveur et lus par le même code que les messages entiers (voir `message`).

use crate::{
    message::{self, Summary},
    net::{Conn, Security, sasl_plain},
};
use std::{
    fmt,
    io::{self, Read, Write},
};

#[derive(Debug)]
pub enum Error {
    /// Le réseau : serveur introuvable, connexion coupée, certificat refusé.
    Io(io::Error),
    /// Le serveur a dit non (mot de passe refusé, dossier absent) : son message.
    No(String),
    /// Une réponse qu'on ne comprend pas.
    Bad(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::No(m) | Error::Bad(m) => f.write_str(m),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Un morceau d'une réponse du serveur.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// Un mot : nombre, drapeau, nom d'élément (« BODY[HEADER.FIELDS (FROM TO)] » compris).
    Atom(String),
    /// Un texte entre guillemets, ou des octets annoncés par leur longueur.
    Text(Vec<u8>),
    List(Vec<Tok>),
}

impl Tok {
    fn atom(&self) -> Option<&str> {
        match self {
            Tok::Atom(a) => Some(a),
            _ => None,
        }
    }

    fn number(&self) -> Option<u32> {
        self.atom()?.parse().ok()
    }

    /// Le texte d'un mot ou d'une chaîne (NIL : rien).
    fn text(&self) -> Option<String> {
        match self {
            Tok::Atom(a) if a.eq_ignore_ascii_case("NIL") => None,
            Tok::Atom(a) => Some(a.clone()),
            Tok::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
            Tok::List(_) => None,
        }
    }
}

/// Ce à quoi sert un dossier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Inbox,
    Drafts,
    Sent,
    Archive,
    Junk,
    Trash,
    /// Tous les messages (Gmail) : un double de tout le reste.
    All,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    /// Le nom tel que le serveur l'écrit : celui qu'on lui redonne.
    pub raw: String,
    /// Le nom lisible, sans les dossiers parents.
    pub name: String,
    /// Le nom lisible en entier, dossiers parents compris.
    pub path: String,
    pub role: Role,
    /// On ne peut pas l'ouvrir (simple conteneur d'autres dossiers).
    pub no_select: bool,
    /// Nombre de dossiers au-dessus de lui.
    pub depth: usize,
}

/// L'état d'un dossier au moment où on l'ouvre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mailbox {
    pub exists: u32,
    /// Change quand les numéros des messages ne valent plus : tout ce qu'on gardait du dossier est à oublier.
    pub uid_validity: u32,
    pub uid_next: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub draft: bool,
    pub deleted: bool,
}

impl Flags {
    fn of(list: &[Tok]) -> Flags {
        let has = |name: &str| list.iter().any(|t| t.atom().is_some_and(|a| a.eq_ignore_ascii_case(name)));
        Flags { seen: has("\\Seen"), flagged: has("\\Flagged"), answered: has("\\Answered"), draft: has("\\Draft"), deleted: has("\\Deleted") }
    }
}

/// Un texte entre guillemets pour une commande. None : il faut l'envoyer en octets annoncés (accents, guillemets).
fn quoted(s: &str) -> Option<String> {
    (s.is_ascii() && !s.bytes().any(|b| b < 0x20 || b == 0x7f)).then(|| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")))
}

/// Un nom de dossier en « UTF-7 modifié » (la façon d'IMAP d'écrire les accents) rendu lisible.
pub fn decode_name(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i + 1..];
        let Some(end) = rest.find('-') else {
            out.push('&');
            break;
        };
        if end == 0 {
            out.push('&');
        } else {
            // base64 (avec « , » pour « / ») d'un texte en UTF-16
            let mut bits = 0u32;
            let mut count = 0;
            let mut units: Vec<u16> = vec![];
            let mut bytes: Vec<u8> = vec![];
            for c in rest[..end].bytes() {
                let v = match c {
                    b'A'..=b'Z' => c - b'A',
                    b'a'..=b'z' => c - b'a' + 26,
                    b'0'..=b'9' => c - b'0' + 52,
                    b'+' => 62,
                    b',' => 63,
                    _ => continue,
                };
                bits = (bits << 6) | v as u32;
                count += 6;
                if count >= 8 {
                    count -= 8;
                    bytes.push((bits >> count) as u8);
                }
            }
            // un octet seul en trop (nom abîmé) est laissé de côté
            for pair in bytes.as_chunks::<2>().0 {
                units.push(u16::from_be_bytes(*pair));
            }
            out.push_str(&String::from_utf16_lossy(&units));
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// À quoi sert un dossier : d'après les marques du serveur, sinon d'après son nom (en anglais et en français).
fn role_of(flags: &[Tok], name: &str) -> Role {
    let has = |f: &str| flags.iter().any(|t| t.atom().is_some_and(|a| a.eq_ignore_ascii_case(f)));
    for (flag, role) in [
        ("\\Sent", Role::Sent),
        ("\\Drafts", Role::Drafts),
        ("\\Trash", Role::Trash),
        ("\\Junk", Role::Junk),
        ("\\Archive", Role::Archive),
        ("\\All", Role::All),
    ] {
        if has(flag) {
            return role;
        }
    }
    let n = name.to_lowercase();
    match n.as_str() {
        "inbox" => Role::Inbox,
        "sent" | "sent items" | "sent mail" | "sent messages" | "envoyés" | "éléments envoyés" | "messages envoyés" => Role::Sent,
        "drafts" | "draft" | "brouillons" => Role::Drafts,
        "trash" | "deleted items" | "deleted messages" | "bin" | "corbeille" | "éléments supprimés" => Role::Trash,
        "junk" | "spam" | "junk e-mail" | "junk email" | "bulk mail" | "courrier indésirable" | "indésirables" | "pourriel" => Role::Junk,
        "archive" | "archives" => Role::Archive,
        "all mail" | "tous les messages" => Role::All,
        _ => Role::Other,
    }
}

/// Une suite de numéros de messages pour une commande : « 3,5:9,12 ».
pub fn uid_set(uids: &[u32]) -> String {
    let mut sorted: Vec<u32> = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut out = String::new();
    let mut i = 0;
    while i < sorted.len() {
        let mut j = i;
        while j + 1 < sorted.len() && sorted[j + 1] == sorted[j] + 1 {
            j += 1;
        }
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(&if j > i { format!("{}:{}", sorted[i], sorted[j]) } else { sorted[i].to_string() });
        i = j + 1;
    }
    out
}

/// Un morceau d'une commande : du texte, ou des octets que le serveur doit accepter avant qu'on les envoie.
enum Part<'a> {
    Text(String),
    Bytes(&'a [u8]),
}

pub struct Imap {
    conn: Conn,
    /// Octets reçus et pas encore lus.
    buf: Vec<u8>,
    at: usize,
    tag: u32,
    /// Ce que le serveur sait faire (« MOVE », « UIDPLUS »…), en majuscules.
    pub caps: Vec<String>,
    /// Le compte rendu de la dernière commande acceptée (« A7 OK [APPENDUID 77 12] done »).
    last_ok: String,
}

impl Imap {
    /// Se connecte et lit le mot d'accueil du serveur. Rien n'est envoyé en clair si `security` n'est pas `Plain`.
    pub fn connect(host: &str, port: u16, security: Security) -> Result<Imap> {
        let mut imap = Imap { conn: Conn::connect(host, port, security)?, buf: Vec::new(), at: 0, tag: 0, caps: vec![], last_ok: String::new() };
        let greeting = imap.line()?;
        if !greeting.starts_with(b"* OK") && !greeting.starts_with(b"* PREAUTH") {
            return Err(Error::Bad(String::from_utf8_lossy(&greeting).into_owned()));
        }
        if security == Security::StartTls {
            imap.command("STARTTLS")?;
            // rien de ce qui a été reçu en clair ne compte plus
            imap.buf.clear();
            imap.at = 0;
            let Imap { conn, .. } = imap;
            imap = Imap { conn: conn.secure(host)?, buf: Vec::new(), at: 0, tag: 1, caps: vec![], last_ok: String::new() };
        }
        imap.capabilities()?;
        Ok(imap)
    }

    fn capabilities(&mut self) -> Result<()> {
        let data = self.command("CAPABILITY")?;
        self.caps = data
            .iter()
            .filter(|r| r.first().and_then(Tok::atom) == Some("CAPABILITY"))
            .flat_map(|r| r[1..].iter().filter_map(Tok::atom).map(str::to_uppercase))
            .collect();
        Ok(())
    }

    pub fn can(&self, what: &str) -> bool {
        self.caps.iter().any(|c| c == what)
    }

    // ───────────────────────── lire ─────────────────────────

    fn fill(&mut self) -> io::Result<()> {
        if self.at > 0 && self.at == self.buf.len() {
            self.buf.clear();
            self.at = 0;
        }
        let mut chunk = [0u8; 16 * 1024];
        let n = self.conn.read(&mut chunk)?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the server closed the connection"));
        }
        self.buf.extend_from_slice(&chunk[..n]);
        Ok(())
    }

    /// Une ligne, sans son retour à la ligne.
    fn line(&mut self) -> io::Result<Vec<u8>> {
        loop {
            if let Some(end) = self.buf[self.at..].iter().position(|&b| b == b'\n') {
                let line = self.buf[self.at..self.at + end].strip_suffix(b"\r").unwrap_or(&self.buf[self.at..self.at + end]).to_vec();
                self.at += end + 1;
                return Ok(line);
            }
            self.fill()?;
        }
    }

    fn exact(&mut self, n: usize) -> io::Result<Vec<u8>> {
        while self.buf.len() - self.at < n {
            self.fill()?;
        }
        let data = self.buf[self.at..self.at + n].to_vec();
        self.at += n;
        Ok(data)
    }

    /// Découpe une réponse (une ligne, et les octets annoncés qu'elle contient) en morceaux.
    fn tokens(&mut self, first: Vec<u8>) -> io::Result<Vec<Tok>> {
        let mut data = first;
        let mut i = 0;
        let mut stack: Vec<Vec<Tok>> = vec![vec![]];
        while i < data.len() {
            match data[i] {
                b' ' => i += 1,
                b'(' => {
                    stack.push(vec![]);
                    i += 1;
                }
                b')' => {
                    // une parenthèse fermante de trop (texte libre) : on passe
                    if stack.len() > 1 {
                        let list = stack.pop().unwrap_or_default();
                        stack.last_mut().expect("the root list").push(Tok::List(list));
                    }
                    i += 1;
                }
                b'"' => {
                    let mut text = vec![];
                    i += 1;
                    while i < data.len() && data[i] != b'"' {
                        if data[i] == b'\\' && i + 1 < data.len() {
                            i += 1;
                        }
                        text.push(data[i]);
                        i += 1;
                    }
                    i += 1;
                    stack.last_mut().expect("the root list").push(Tok::Text(text));
                }
                b'{' if data[i..].ends_with(b"}") && data[i + 1..data.len() - 1].iter().all(|b| b.is_ascii_digit() || *b == b'+') => {
                    // « {n} » en fin de ligne : n octets suivent, puis la suite de la réponse
                    let digits: String = data[i + 1..data.len() - 1].iter().filter(|b| b.is_ascii_digit()).map(|&b| b as char).collect();
                    let n: usize = digits.parse().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad literal"))?;
                    let bytes = self.exact(n)?;
                    stack.last_mut().expect("the root list").push(Tok::Text(bytes));
                    data = self.line()?;
                    i = 0;
                }
                _ => {
                    // un mot ; entre crochets, espaces et parenthèses en font partie : BODY[HEADER.FIELDS (FROM TO)]
                    let (start, mut depth) = (i, 0);
                    while i < data.len() {
                        match data[i] {
                            b'[' => depth += 1,
                            b']' => depth = (depth as i32 - 1).max(0) as usize,
                            b' ' | b'(' | b')' if depth == 0 => break,
                            _ => {}
                        }
                        i += 1;
                    }
                    stack.last_mut().expect("the root list").push(Tok::Atom(String::from_utf8_lossy(&data[start..i]).into_owned()));
                }
            }
        }
        // des listes jamais fermées (texte libre) : leur contenu remonte
        while stack.len() > 1 {
            let list = stack.pop().unwrap_or_default();
            stack.last_mut().expect("the root list").extend(list);
        }
        Ok(stack.pop().unwrap_or_default())
    }

    // ───────────────────────── commander ─────────────────────────

    /// Envoie une commande et renvoie les réponses de données (« * ... ») reçues avant son compte rendu.
    fn command(&mut self, text: &str) -> Result<Vec<Vec<Tok>>> {
        self.send(&[Part::Text(text.to_string())])
    }

    fn send(&mut self, parts: &[Part]) -> Result<Vec<Vec<Tok>>> {
        self.tag += 1;
        let tag = format!("A{}", self.tag);
        let mut data: Vec<Vec<Tok>> = vec![];
        let mut out = format!("{tag} ").into_bytes();
        for part in parts {
            match part {
                Part::Text(t) => out.extend_from_slice(t.as_bytes()),
                Part::Bytes(bytes) => {
                    // on annonce la longueur, le serveur dit « + » : alors seulement on envoie
                    out.extend_from_slice(format!("{{{}}}\r\n", bytes.len()).as_bytes());
                    self.conn.write_all(&out)?;
                    self.conn.flush()?;
                    out.clear();
                    loop {
                        let line = self.line()?;
                        if line.starts_with(b"+") {
                            break;
                        }
                        if let Some(done) = self.status(&tag, &line) {
                            self.last_ok = String::from_utf8_lossy(&line).into_owned();
                            return done.map(|_| data);
                        }
                        if line.starts_with(b"* ") {
                            data.push(self.tokens(line[2..].to_vec())?);
                        }
                    }
                    out.extend_from_slice(bytes);
                }
            }
        }
        out.extend_from_slice(b"\r\n");
        self.conn.write_all(&out)?;
        self.conn.flush()?;
        loop {
            let line = self.line()?;
            if let Some(done) = self.status(&tag, &line) {
                self.last_ok = String::from_utf8_lossy(&line).into_owned();
                return done.map(|_| data);
            }
            if line.starts_with(b"* ") {
                data.push(self.tokens(line[2..].to_vec())?);
            }
        }
    }

    /// Le compte rendu de la commande `tag`, si cette ligne l'est.
    fn status(&self, tag: &str, line: &[u8]) -> Option<Result<()>> {
        let rest = line.strip_prefix(tag.as_bytes())?.strip_prefix(b" ")?;
        let text = String::from_utf8_lossy(rest).into_owned();
        let (word, message) = text.split_once(' ').unwrap_or((text.as_str(), ""));
        Some(match word.to_ascii_uppercase().as_str() {
            "OK" => Ok(()),
            "NO" => Err(Error::No(message.to_string())),
            _ => Err(Error::Bad(text.clone())),
        })
    }

    /// Un nom de dossier ou un mot de passe dans une commande.
    fn arg<'a>(parts: &mut Vec<Part<'a>>, text: &'a str) {
        match quoted(text) {
            Some(q) => parts.push(Part::Text(q)),
            None => parts.push(Part::Bytes(text.as_bytes())),
        }
    }

    // ───────────────────────── ce que l'application demande ─────────────────────────

    /// S'identifie. Le mot de passe ne part jamais sur une connexion en clair, sauf vers un serveur d'essai.
    pub fn login(&mut self, user: &str, password: &str) -> Result<()> {
        if self.can("AUTH=PLAIN") {
            self.tag += 1;
            let tag = format!("A{}", self.tag);
            self.conn.write_all(format!("{tag} AUTHENTICATE PLAIN\r\n").as_bytes())?;
            self.conn.flush()?;
            loop {
                let line = self.line()?;
                if line.starts_with(b"+") {
                    self.conn.write_all(format!("{}\r\n", sasl_plain(user, password)).as_bytes())?;
                    self.conn.flush()?;
                } else if let Some(done) = self.status(&tag, &line) {
                    done?;
                    break;
                }
            }
        } else {
            let mut parts = vec![Part::Text("LOGIN ".into())];
            Imap::arg(&mut parts, user);
            parts.push(Part::Text(" ".into()));
            Imap::arg(&mut parts, password);
            self.send(&parts)?;
        }
        // une fois identifié, le serveur en dit souvent plus sur ce qu'il sait faire
        self.capabilities()
    }

    /// Tous les dossiers du compte, la boîte de réception d'abord, puis par rôle, puis par nom.
    pub fn folders(&mut self) -> Result<Vec<Folder>> {
        let data = self.command("LIST \"\" \"*\"")?;
        let mut folders: Vec<(String, Folder)> = data
            .iter()
            .filter(|r| r.first().and_then(Tok::atom).is_some_and(|a| a.eq_ignore_ascii_case("LIST")))
            .filter_map(|r| {
                let flags = match r.get(1)? {
                    Tok::List(l) => l.as_slice(),
                    _ => &[],
                };
                let delimiter = r.get(2).and_then(Tok::text).and_then(|d| d.chars().next());
                let raw = r.get(3)?.text()?;
                let full = decode_name(&raw);
                let parts: Vec<&str> = match delimiter {
                    Some(d) => full.split(d).collect(),
                    None => vec![full.as_str()],
                };
                let name = parts.last().copied().unwrap_or(full.as_str()).to_string();
                let has = |f: &str| flags.iter().any(|t| t.atom().is_some_and(|a| a.eq_ignore_ascii_case(f)));
                let role = if parts.len() == 1 && full.eq_ignore_ascii_case("INBOX") { Role::Inbox } else { role_of(flags, &name) };
                let depth = parts.len() - 1;
                Some((parts[0].to_lowercase(), Folder { raw, name, role, no_select: has("\\Noselect") || has("\\NonExistent"), depth, path: full.clone() }))
            })
            .collect();
        let rank = |f: &Folder| match f.role {
            Role::Inbox => 0,
            Role::Drafts => 1,
            Role::Sent => 2,
            Role::Archive => 3,
            Role::Junk => 4,
            Role::Trash => 5,
            Role::All => 6,
            Role::Other => 7,
        };
        // un sous-dossier se range avec son dossier de tête (dont il suit le rang), juste après lui
        let tops: std::collections::HashMap<String, u8> = folders.iter().filter(|(_, f)| f.depth == 0).map(|(top, f)| (top.clone(), rank(f))).collect();
        folders.sort_by_cached_key(|(top, f)| (tops.get(top).copied().unwrap_or(7), f.path.to_lowercase()));
        Ok(folders.into_iter().map(|(_, f)| f).collect())
    }

    /// Ouvre un dossier : les commandes suivantes portent sur lui.
    pub fn select(&mut self, folder: &str) -> Result<Mailbox> {
        let mut parts = vec![Part::Text("SELECT ".into())];
        Imap::arg(&mut parts, folder);
        let data = self.send(&parts)?;
        let mut mailbox = Mailbox::default();
        for r in &data {
            match (r.first().and_then(Tok::atom), r.get(1).and_then(Tok::atom)) {
                (Some(n), Some(what)) if what.eq_ignore_ascii_case("EXISTS") => mailbox.exists = n.parse().unwrap_or(0),
                (Some(ok), Some(code)) if ok.eq_ignore_ascii_case("OK") => {
                    let code = code.trim_matches(['[', ']']);
                    if let Some((key, value)) = code.split_once(' ') {
                        match key.to_ascii_uppercase().as_str() {
                            "UIDVALIDITY" => mailbox.uid_validity = value.parse().unwrap_or(0),
                            "UIDNEXT" => mailbox.uid_next = value.parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(mailbox)
    }

    /// Les éléments (nom, valeur) de chaque réponse FETCH.
    fn fetched(data: &[Vec<Tok>]) -> impl Iterator<Item = &[Tok]> {
        data.iter().filter(|r| r.get(1).and_then(Tok::atom).is_some_and(|a| a.eq_ignore_ascii_case("FETCH"))).filter_map(|r| match r.get(2) {
            Some(Tok::List(items)) => Some(items.as_slice()),
            _ => None,
        })
    }

    fn item<'a>(items: &'a [Tok], name: &str) -> Option<&'a Tok> {
        items.chunks(2).find(|pair| pair[0].atom().is_some_and(|a| a.to_ascii_uppercase().starts_with(name))).and_then(|pair| pair.get(1))
    }

    /// Le numéro et les drapeaux de tous les messages du dossier ouvert.
    pub fn all_flags(&mut self) -> Result<Vec<(u32, Flags)>> {
        let data = self.command("UID FETCH 1:* (UID FLAGS)")?;
        let mut all: Vec<(u32, Flags)> = Imap::fetched(&data)
            .filter_map(|items| {
                let uid = Imap::item(items, "UID")?.number()?;
                let flags = match Imap::item(items, "FLAGS") {
                    Some(Tok::List(l)) => Flags::of(l),
                    _ => Flags::default(),
                };
                Some((uid, flags))
            })
            .collect();
        all.sort_unstable_by_key(|(uid, _)| *uid);
        all.dedup_by_key(|(uid, _)| *uid);
        Ok(all)
    }

    /// Ce qu'il faut pour afficher ces messages dans la liste : expéditeur, objet, date, taille, drapeaux.
    pub fn summaries(&mut self, uids: &[u32]) -> Result<Vec<Summary>> {
        if uids.is_empty() {
            return Ok(vec![]);
        }
        let fields = "DATE FROM TO CC SUBJECT MESSAGE-ID IN-REPLY-TO REFERENCES CONTENT-TYPE";
        let data = self.command(&format!("UID FETCH {} (UID FLAGS RFC822.SIZE INTERNALDATE BODY.PEEK[HEADER.FIELDS ({fields})])", uid_set(uids)))?;
        Ok(Imap::fetched(&data)
            .filter_map(|items| {
                let uid = Imap::item(items, "UID")?.number()?;
                let header = match Imap::item(items, "BODY[") {
                    Some(Tok::Text(bytes)) => bytes.as_slice(),
                    _ => &[],
                };
                let mut summary = message::summary(header);
                summary.uid = uid;
                summary.size = Imap::item(items, "RFC822.SIZE").and_then(Tok::number).unwrap_or(0);
                summary.flags = match Imap::item(items, "FLAGS") {
                    Some(Tok::List(l)) => Flags::of(l),
                    _ => Flags::default(),
                };
                // sans date dans l'en-tête : celle où le serveur a reçu le message
                if summary.date == 0 {
                    summary.date = Imap::item(items, "INTERNALDATE").and_then(Tok::text).and_then(|d| message::parse_internal_date(&d)).unwrap_or(0);
                }
                Some(summary)
            })
            .collect())
    }

    /// Le message entier, tel que le serveur le garde. Le lire ne le marque pas comme lu.
    pub fn body(&mut self, uid: u32) -> Result<Vec<u8>> {
        let data = self.command(&format!("UID FETCH {uid} (BODY.PEEK[])"))?;
        Imap::fetched(&data)
            .find_map(|items| match Imap::item(items, "BODY[") {
                Some(Tok::Text(bytes)) => Some(bytes.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::No("The message is no longer on the server.".into()))
    }

    /// Pose ou retire un drapeau (« \Seen », « \Flagged ») sur des messages du dossier ouvert.
    pub fn set_flag(&mut self, uids: &[u32], flag: &str, on: bool) -> Result<()> {
        if uids.is_empty() {
            return Ok(());
        }
        self.command(&format!("UID STORE {} {}FLAGS.SILENT ({flag})", uid_set(uids), if on { '+' } else { '-' })).map(|_| ())
    }

    /// Déplace des messages du dossier ouvert vers un autre.
    pub fn move_to(&mut self, uids: &[u32], folder: &str) -> Result<()> {
        if uids.is_empty() {
            return Ok(());
        }
        let set = uid_set(uids);
        let mut parts = vec![Part::Text(format!("UID {} {set} ", if self.can("MOVE") { "MOVE" } else { "COPY" }))];
        Imap::arg(&mut parts, folder);
        self.send(&parts)?;
        if !self.can("MOVE") {
            // un serveur ancien : copier, marquer l'original supprimé, le retirer
            self.remove(uids)?;
        }
        Ok(())
    }

    /// Retire pour de bon des messages du dossier ouvert.
    pub fn remove(&mut self, uids: &[u32]) -> Result<()> {
        if uids.is_empty() {
            return Ok(());
        }
        let set = uid_set(uids);
        self.command(&format!("UID STORE {set} +FLAGS.SILENT (\\Deleted)"))?;
        // « UID EXPUNGE » ne retire que ceux-là ; sans lui, tous ceux que le dossier marque supprimés partent
        let expunge = if self.can("UIDPLUS") { format!("UID EXPUNGE {set}") } else { "EXPUNGE".to_string() };
        self.command(&expunge).map(|_| ())
    }

    /// Dépose un message dans un dossier (la copie d'un message envoyé, par exemple), déjà lu.
    pub fn append(&mut self, folder: &str, message: &[u8]) -> Result<()> {
        let mut parts = vec![Part::Text("APPEND ".into())];
        Imap::arg(&mut parts, folder);
        parts.push(Part::Text(" (\\Seen) ".into()));
        parts.push(Part::Bytes(message));
        self.send(&parts).map(|_| ())
    }

    /// Dépose un brouillon dans un dossier. Renvoie son numéro, si le serveur le dit (UIDPLUS).
    pub fn append_draft(&mut self, folder: &str, message: &[u8]) -> Result<Option<u32>> {
        let mut parts = vec![Part::Text("APPEND ".into())];
        Imap::arg(&mut parts, folder);
        parts.push(Part::Text(" (\\Draft \\Seen) ".into()));
        parts.push(Part::Bytes(message));
        self.send(&parts)?;
        // « OK [APPENDUID 77 12] » : 12 est le numéro du brouillon
        let upper = self.last_ok.to_ascii_uppercase();
        Ok(upper.find("[APPENDUID ").and_then(|i| upper[i + 11..].split([' ', ']']).nth(1)?.parse().ok()))
    }

    /// Les messages du dossier ouvert dont le texte (en-têtes compris) contient chacun de ces mots.
    pub fn search_text(&mut self, words: &[String]) -> Result<Vec<u32>> {
        if words.is_empty() {
            return Ok(vec![]);
        }
        let run = |imap: &mut Imap, charset: bool| -> Result<Vec<Vec<Tok>>> {
            let mut parts = vec![Part::Text(if charset { "UID SEARCH CHARSET UTF-8".into() } else { "UID SEARCH".into() })];
            for w in words {
                parts.push(Part::Text(" TEXT ".into()));
                Imap::arg(&mut parts, w);
            }
            imap.send(&parts)
        };
        // un serveur qui ne connaît pas « CHARSET UTF-8 » le dit ; des mots sans accent se cherchent alors sans lui
        let data = match run(self, true) {
            Err(Error::No(_) | Error::Bad(_)) if words.iter().all(|w| w.is_ascii()) => run(self, false)?,
            other => other?,
        };
        let mut uids: Vec<u32> = data
            .iter()
            .filter(|r| r.first().and_then(Tok::atom).is_some_and(|a| a.eq_ignore_ascii_case("SEARCH")))
            .flat_map(|r| r[1..].iter().filter_map(Tok::number))
            .collect();
        uids.sort_unstable();
        uids.dedup();
        Ok(uids)
    }

    /// Attend que le dossier ouvert change (IDLE) : un message arrive, part, change de marque. `changed` est appelé
    /// à chaque fois ; l'attente dure tant que `stop` ne dit pas d'arrêter (regardé toutes les deux secondes) et se
    /// renouvelle toutes les 25 minutes, comme le demande la norme. Ne revient qu'à l'arrêt ou sur une panne.
    pub fn idle(&mut self, stop: &dyn Fn() -> bool, changed: &mut dyn FnMut()) -> Result<()> {
        use std::time::{Duration, Instant};
        const TICK: Duration = Duration::from_secs(2);
        const RENEW: Duration = Duration::from_secs(25 * 60);
        self.conn.set_read_timeout(Some(TICK))?;
        let result = (|| -> Result<()> {
            loop {
                self.tag += 1;
                let tag = format!("A{}", self.tag);
                self.conn.write_all(format!("{tag} IDLE\r\n").as_bytes())?;
                self.conn.flush()?;
                let started = Instant::now();
                let mut done_sent = false;
                loop {
                    let line = match self.line() {
                        Ok(line) => line,
                        // rien de neuf pendant deux secondes : faut-il s'arrêter, ou renouveler l'attente ?
                        Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock) => {
                            if !done_sent && (stop() || started.elapsed() >= RENEW) {
                                self.conn.write_all(b"DONE\r\n")?;
                                self.conn.flush()?;
                                done_sent = true;
                            }
                            continue;
                        }
                        Err(e) => return Err(e.into()),
                    };
                    if let Some(done) = self.status(&tag, &line) {
                        done?;
                        break;
                    }
                    if line.starts_with(b"+") {
                        continue;
                    }
                    let upper = String::from_utf8_lossy(&line).to_ascii_uppercase();
                    if upper.starts_with("* ") && (upper.ends_with(" EXISTS") || upper.ends_with(" EXPUNGE") || upper.contains(" FETCH ")) {
                        changed();
                    }
                }
                if stop() {
                    return Ok(());
                }
            }
        })();
        let _ = self.conn.set_read_timeout(Some(Duration::from_secs(90)));
        result
    }

    /// Rien à demander : garde la connexion en vie, et dit si elle l'est encore.
    pub fn noop(&mut self) -> Result<()> {
        self.command("NOOP").map(|_| ())
    }

    pub fn logout(mut self) {
        let _ = self.command("LOGOUT");
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        net::TcpListener,
    };

    /// Un serveur de théâtre : pour chaque commande reçue (qui doit contenir le texte attendu), il récite sa réplique.
    /// « {tag} » y est remplacé par l'étiquette de la commande. Renvoie le port où il écoute.
    pub(crate) fn stage(greeting: &'static str, script: Vec<(&'static str, String)>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;
            writer.write_all(greeting.as_bytes()).unwrap();
            for (expected, reply) in script {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 {
                    return;
                }
                assert!(line.contains(expected), "expected {expected:?}, got {line:?}");
                let tag = line.split(' ').next().unwrap_or("").to_string();
                // des octets annoncés : on dit « + », on les lit, puis la suite de la ligne (qui peut en annoncer d'autres)
                let mut tail = line.clone();
                while let Some(n) = tail.trim_end().strip_suffix('}').and_then(|l| l.rsplit_once('{')).and_then(|(_, n)| n.parse::<usize>().ok()) {
                    writer.write_all(b"+ go ahead\r\n").unwrap();
                    let mut bytes = vec![0u8; n];
                    reader.read_exact(&mut bytes).unwrap();
                    tail.clear();
                    reader.read_line(&mut tail).unwrap();
                }
                writer.write_all(reply.replace("{tag}", &tag).as_bytes()).unwrap();
            }
        });
        port
    }

    #[test]
    fn names_and_sets() {
        assert_eq!(decode_name("INBOX"), "INBOX");
        assert_eq!(decode_name("&AMk-l&AOk-ments envoy&AOk-s"), "Éléments envoyés");
        assert_eq!(decode_name("Tom &- Jerry"), "Tom & Jerry");
        assert_eq!(decode_name("&ZeVnLIqe-"), "日本語");
        assert_eq!(decode_name("broken &AMk"), "broken &AMk");
        assert_eq!(uid_set(&[5, 3, 4, 9, 12, 11, 3]), "3:5,9,11:12");
        assert_eq!(uid_set(&[7]), "7");
        assert_eq!(quoted("Sent Items").as_deref(), Some("\"Sent Items\""));
        assert_eq!(quoted("a\"b\\c").as_deref(), Some("\"a\\\"b\\\\c\""));
        assert_eq!(quoted("Envoyés"), None);
        assert_eq!(quoted("a\r\nb"), None);
    }

    #[test]
    fn drafts_search_and_waiting_for_news() {
        let port = stage(
            "* OK ready\r\n",
            vec![
                ("CAPABILITY", "* CAPABILITY IMAP4rev1 UIDPLUS IDLE\r\n{tag} OK\r\n".into()),
                ("LOGIN", "{tag} OK\r\n".into()),
                ("CAPABILITY", "* CAPABILITY IMAP4rev1 UIDPLUS IDLE\r\n{tag} OK\r\n".into()),
                ("APPEND \"Drafts\" (\\Draft \\Seen) {10}", "{tag} OK [APPENDUID 77 12] appended\r\n".into()),
                ("SELECT \"INBOX\"", "* 3 EXISTS\r\n{tag} OK\r\n".into()),
                ("UID SEARCH CHARSET UTF-8 TEXT \"lien\" TEXT {8}", "* SEARCH 4 2\r\n{tag} OK\r\n".into()),
                ("UID SEARCH CHARSET UTF-8 TEXT \"plan\"", "{tag} NO [BADCHARSET (US-ASCII)] no\r\n".into()),
                ("UID SEARCH TEXT \"plan\"", "* SEARCH\r\n{tag} OK\r\n".into()),
                // l'attente : une nouvelle tout de suite, puis le serveur clôt l'attente quand on dit DONE
                ("IDLE", "+ idling\r\n* 4 EXISTS\r\n".into()),
                // « DONE » n'a pas d'étiquette : la réponse porte celle de « IDLE »
                ("DONE", "A9 OK idle done\r\n".into()),
            ],
        );
        let mut imap = Imap::connect("127.0.0.1", port, Security::Plain).unwrap();
        imap.login("sam", "pw").unwrap();
        assert_eq!(imap.append_draft("Drafts", b"Subject: x").unwrap(), Some(12));
        imap.select("INBOX").unwrap();
        assert_eq!(imap.search_text(&["lien".into(), "réunion".into()]).unwrap(), [2, 4]);
        assert!(imap.search_text(&["plan".into()]).unwrap().is_empty());
        let news = std::cell::Cell::new(0);
        imap.idle(&|| news.get() > 0, &mut || news.set(news.get() + 1)).unwrap();
        assert_eq!(news.get(), 1);
    }

    #[test]
    fn a_whole_session() {
        let header = "Subject: =?utf-8?q?R=C3=A9union_demain?=\r\nFrom: \"Ann Example\" <ann@example.com>\r\nTo: sam@example.com\r\nDate: Tue, 10 Nov 2026 12:00:00 +0000\r\nMessage-ID: <m1@example.com>\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n";
        let body = "Subject: Hi\r\nFrom: ann@example.com\r\n\r\nHello (again)\r\n";
        let port = stage(
            "* OK [CAPABILITY IMAP4rev1] ready (really)\r\n",
            vec![
                ("CAPABILITY", "* CAPABILITY IMAP4rev1 AUTH=PLAIN MOVE UIDPLUS\r\n{tag} OK done\r\n".into()),
                ("AUTHENTICATE PLAIN", "+ \r\n".into()),
                // « \0sam\0p"ss é » en base64
                ("AHNhbQBwInNzIMOp", "A2 OK logged in\r\n".into()),
                ("CAPABILITY", "* CAPABILITY IMAP4rev1 MOVE UIDPLUS\r\n{tag} OK done\r\n".into()),
                (
                    "LIST",
                    "* LIST (\\HasNoChildren) \"/\" INBOX\r\n* LIST (\\HasNoChildren \\Sent) \"/\" \"&AMk-l&AOk-ments envoy&AOk-s\"\r\n* LIST (\\Noselect \\HasChildren) \"/\" Work\r\n* LIST (\\HasNoChildren) \"/\" {12}\r\nWork/Plume 1\r\n* LIST (\\HasNoChildren) \"/\" Trash\r\n{tag} OK done\r\n".into(),
                ),
                ("SELECT \"INBOX\"", "* 2 EXISTS\r\n* OK [UIDVALIDITY 77] ok\r\n* OK [UIDNEXT 9] next\r\n* FLAGS (\\Seen)\r\n{tag} OK [READ-WRITE] done\r\n".into()),
                ("UID FETCH 1:* (UID FLAGS)", "* 1 FETCH (UID 4 FLAGS (\\Seen \\Answered))\r\n* 2 FETCH (FLAGS (\\Flagged) UID 8)\r\n{tag} OK done\r\n".into()),
                (
                    "UID FETCH 4,8 (UID FLAGS RFC822.SIZE INTERNALDATE BODY.PEEK[HEADER.FIELDS",
                    format!("* 1 FETCH (UID 4 FLAGS (\\Seen) RFC822.SIZE 1234 INTERNALDATE \"10-Nov-2026 12:00:05 +0000\" BODY[HEADER.FIELDS (DATE FROM TO)] {{{}}}\r\n{header})\r\n* 2 FETCH (UID 8 FLAGS () RFC822.SIZE 99 INTERNALDATE \"09-Nov-2026 08:30:00 -0500\" BODY[HEADER.FIELDS (DATE FROM TO)] {{2}}\r\n\r\n)\r\n{{tag}} OK done\r\n", header.len()),
                ),
                ("UID FETCH 4 (BODY.PEEK[])", format!("* 1 FETCH (UID 4 BODY[] {{{}}}\r\n{body})\r\n{{tag}} OK done\r\n", body.len())),
                ("UID STORE 4 +FLAGS.SILENT (\\Seen)", "{tag} OK done\r\n".into()),
                ("UID MOVE 4,8 \"Trash\"", "{tag} OK done\r\n".into()),
                ("UID STORE 8 +FLAGS.SILENT (\\Deleted)", "{tag} OK done\r\n".into()),
                ("UID EXPUNGE 8", "{tag} OK done\r\n".into()),
                ("APPEND {19}", "{tag} OK appended\r\n".into()),
                ("SELECT \"Nope\"", "{tag} NO [NONEXISTENT] Unknown Mailbox: Nope\r\n".into()),
            ],
        );
        let mut imap = Imap::connect("127.0.0.1", port, Security::Plain).unwrap();
        imap.login("sam", "p\"ss é").unwrap();
        assert!(imap.can("MOVE") && imap.can("UIDPLUS") && !imap.can("AUTH=PLAIN"));
        let folders = imap.folders().unwrap();
        let seen: Vec<(&str, Role, bool, usize)> = folders.iter().map(|f| (f.name.as_str(), f.role, f.no_select, f.depth)).collect();
        assert_eq!(
            seen,
            [
                ("INBOX", Role::Inbox, false, 0),
                ("Éléments envoyés", Role::Sent, false, 0),
                ("Trash", Role::Trash, false, 0),
                ("Work", Role::Other, true, 0),
                ("Plume 1", Role::Other, false, 1)
            ]
        );
        assert_eq!(folders[1].raw, "&AMk-l&AOk-ments envoy&AOk-s");
        assert_eq!(imap.select("INBOX").unwrap(), Mailbox { exists: 2, uid_validity: 77, uid_next: 9 });
        let flags = imap.all_flags().unwrap();
        assert_eq!(flags.iter().map(|f| f.0).collect::<Vec<_>>(), [4, 8]);
        assert!(flags[0].1.seen && flags[0].1.answered && flags[1].1.flagged && !flags[1].1.seen);
        let list = imap.summaries(&[4, 8]).unwrap();
        assert_eq!(
            (list[0].uid, list[0].subject.as_str(), list[0].from_name.as_str(), list[0].from.as_str(), list[0].size),
            (4, "Réunion demain", "Ann Example", "ann@example.com", 1234)
        );
        assert!(list[0].flags.seen && list[0].attachments && list[0].date == 1_794_312_000);
        // pas d'en-tête : la date est celle où le serveur a reçu le message (08:30 à −5 h : 13:30 UTC)
        assert_eq!((list[1].uid, list[1].subject.as_str(), list[1].date), (8, "", 1_794_231_000));
        assert_eq!(imap.body(4).unwrap(), body.as_bytes());
        imap.set_flag(&[4], "\\Seen", true).unwrap();
        imap.move_to(&[8, 4], "Trash").unwrap();
        imap.remove(&[8]).unwrap();
        imap.append("Éléments envoyés", b"Subject: x\r\n\r\ny\r\n").unwrap();
        match imap.select("Nope") {
            Err(Error::No(m)) => assert!(m.contains("Unknown Mailbox")),
            other => panic!("{other:?}"),
        }
    }
}
