//! Lire un message reçu (en-têtes, texte, pièces jointes) et écrire un message à envoyer.
//! La lecture s'appuie sur `mail-parser` : encodages, jeux de caractères, messages mal formés compris.

use crate::{html, imap::Flags, net::base64};
use mail_parser::{MessageParser, PartKind};

/// Ce qu'il faut d'un message pour l'afficher dans la liste.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub uid: u32,
    pub size: u32,
    pub flags: Flags,
    /// Date d'envoi, en secondes depuis 1970 (0 : inconnue).
    pub date: i64,
    pub subject: String,
    pub from_name: String,
    pub from: String,
    /// Les destinataires, comme on les afficherait : « Ann <ann@example.com>, bob@example.com ».
    pub to: String,
    pub message_id: String,
    /// Les messages auxquels celui-ci répond, du plus ancien au plus récent.
    pub references: Vec<String>,
    /// A sans doute des pièces jointes (d'après sa forme ; on le sait vraiment une fois le message lu).
    pub attachments: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub name: String,
    /// « image/png », « application/pdf »…
    pub kind: String,
    pub size: usize,
    /// Sa place parmi les pièces jointes du message.
    pub index: usize,
}

/// Un message lu en entier.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub summary: Summary,
    pub cc: String,
    pub reply_to: String,
    /// Le texte à lire. Un message en HTML y est rendu en texte : aucune image distante n'est chargée, aucun
    /// script n'est exécuté.
    pub text: String,
    /// Le message n'existait qu'en HTML : sa mise en page n'est pas reproduite.
    pub from_html: bool,
    pub attachments: Vec<Attachment>,
}

fn show(name: Option<&str>, address: Option<&str>) -> String {
    match (name.map(str::trim).filter(|n| !n.is_empty()), address.unwrap_or_default()) {
        (Some(name), "") => name.to_string(),
        (Some(name), address) => format!("{name} <{address}>"),
        (None, address) => address.to_string(),
    }
}

fn people(list: Option<mail_parser::AddressList<'_>>) -> String {
    list.map(|l| l.mailboxes().map(|m| show(m.name(), m.address())).collect::<Vec<_>>().join(", ")).unwrap_or_default()
}

fn summary_of(m: &mail_parser::Message<'_>) -> Summary {
    let from = m.from().and_then(|f| f.first());
    let mut references: Vec<String> = m.references().map(|r| r.iter().map(str::to_string).collect()).unwrap_or_default();
    // un message qui ne cite que celui auquel il répond
    if let Some(parent) = m.in_reply_to().and_then(|r| r.iter().last().map(str::to_string))
        && !references.contains(&parent)
    {
        references.push(parent);
    }
    Summary {
        date: m.date().filter(|d| d.is_valid()).map_or(0, |d| d.to_timestamp()),
        subject: m.subject().unwrap_or_default().trim().to_string(),
        from_name: from.and_then(|f| f.name()).unwrap_or_default().trim().to_string(),
        from: from.and_then(|f| f.address()).unwrap_or_default().to_string(),
        to: people(m.to()),
        message_id: m.message_id().unwrap_or_default().to_string(),
        references,
        attachments: m.is_content_type("multipart", "mixed") || m.has_attachments(),
        ..Summary::default()
    }
}

/// Lit les en-têtes d'un message (ceux que le serveur donne pour la liste).
pub fn summary(header: &[u8]) -> Summary {
    MessageParser::new().parse(header).map(|m| summary_of(&m)).unwrap_or_default()
}

/// Lit un message entier.
pub fn parse(raw: &[u8]) -> Parsed {
    let Some(m) = MessageParser::new().parse(raw) else { return Parsed::default() };
    // sans version en texte, le « texte » du message est son HTML : on le rend lisible nous-mêmes
    let is_html = |part: mail_parser::MessagePart<'_>| part.content_type().is_some_and(|c| c.subtype().is_some_and(|s| s.eq_ignore_ascii_case("html")));
    let from_html = m.text_body().len() > 0 && m.text_body().all(is_html);
    let text = if from_html {
        m.html_bodies().map(|h| html::to_text(&h)).collect::<Vec<_>>().join("\n\n")
    } else {
        m.text_bodies().collect::<Vec<_>>().join("\n").replace("\r\n", "\n")
    };
    let attachments = m
        .attachments()
        .enumerate()
        .map(|(index, part)| {
            let kind = part.content_type().map(|c| match c.subtype() {
                Some(sub) => format!("{}/{sub}", c.ctype()),
                None => c.ctype().to_string(),
            });
            let fallback = if matches!(part.kind(), PartKind::Message(_)) { "message.eml" } else { "attachment" };
            Attachment {
                name: part.attachment_name().unwrap_or(fallback).to_string(),
                kind: kind.unwrap_or_default().to_lowercase(),
                size: part.decoded_len(),
                index,
            }
        })
        .collect::<Vec<_>>();
    let mut summary = summary_of(&m);
    summary.attachments = !attachments.is_empty();
    summary.size = raw.len() as u32;
    Parsed { summary, cc: people(m.cc()), reply_to: people(m.reply_to()), text: text.trim_end().to_string(), from_html, attachments }
}

/// Le contenu d'une pièce jointe d'un message.
pub fn attachment(raw: &[u8], index: usize) -> Option<Vec<u8>> {
    let m = MessageParser::new().parse(raw)?;
    m.attachments().nth(index).map(|part| part.decoded().into_owned())
}

/// « 10-Nov-2026 12:00:05 +0000 » (la date où le serveur a reçu un message) en secondes depuis 1970.
pub fn parse_internal_date(text: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let mut parts = text.split_whitespace();
    let mut date = parts.next()?.split('-');
    let (day, month, year): (i64, &str, i64) = (date.next()?.parse().ok()?, date.next()?, date.next()?.parse().ok()?);
    let month = MONTHS.iter().position(|m| month.eq_ignore_ascii_case(m))? as i64 + 1;
    let mut clock = parts.next()?.split(':');
    let (h, mi, s): (i64, i64, i64) = (clock.next()?.parse().ok()?, clock.next()?.parse().ok()?, clock.next().and_then(|s| s.parse().ok()).unwrap_or(0));
    let zone = parts.next().unwrap_or("+0000");
    let offset = zone.get(1..3).and_then(|h| h.parse::<i64>().ok()).unwrap_or(0) * 3600 + zone.get(3..5).and_then(|m| m.parse::<i64>().ok()).unwrap_or(0) * 60;
    let offset = if zone.starts_with('-') { -offset } else { offset };
    Some(days_from_civil(year, month, day) * 86_400 + h * 3600 + mi * 60 + s - offset)
}

/// Jours depuis le 1er janvier 1970.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// (année, mois, jour, heure, minute, seconde, jour de la semaine avec dimanche = 0) d'un instant en UTC.
fn civil(t: i64) -> (i64, i64, i64, i64, i64, i64, i64) {
    let (days, rest) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    (yoe + era * 400 + (m <= 2) as i64, m, d, rest / 3600, rest % 3600 / 60, rest % 60, (days + 4).rem_euclid(7))
}

/// La date d'un message à envoyer : « Tue, 10 Nov 2026 12:00:00 +0000 ».
fn rfc5322_date(t: i64) -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (y, mo, d, h, mi, s, weekday) = civil(t);
    format!("{}, {d:02} {} {y} {h:02}:{mi:02}:{s:02} +0000", DAYS[weekday as usize], MONTHS[mo as usize - 1])
}

// ───────────────────────── écrire ─────────────────────────

/// Un message à envoyer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    pub from_name: String,
    pub from: String,
    /// Adresses séparées par des virgules ou des points-virgules, avec ou sans nom : « Ann <ann@example.com>, bob@example.com ».
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub text: String,
    /// Le message auquel on répond, et ceux auxquels il répondait lui-même.
    pub in_reply_to: String,
    pub references: Vec<String>,
    /// Pièces jointes : (nom, contenu).
    pub files: Vec<(String, Vec<u8>)>,
}

/// Découpe un champ d'adresses aux virgules et aux points-virgules, sauf ceux d'un nom entre guillemets.
fn pieces(field: &str) -> Vec<&str> {
    let (mut out, mut start, mut quoted) = (vec![], 0, false);
    for (i, c) in field.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ',' | ';' if !quoted => {
                out.push(&field[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&field[start..]);
    out.into_iter().map(str::trim).filter(|p| !p.is_empty()).collect()
}

/// Les adresses d'un champ « À » : ce que le serveur d'envoi doit recevoir. Vide si l'une d'elles est mal écrite.
pub fn addresses(field: &str) -> Vec<String> {
    let mut out = vec![];
    for piece in pieces(field) {
        // « Nom <adresse> » ou l'adresse seule
        let address = match (piece.rfind('<'), piece.rfind('>')) {
            (Some(a), Some(b)) if a < b => &piece[a + 1..b],
            _ => piece,
        }
        .trim();
        let ok = address.split_once('@').is_some_and(|(user, host)| !user.is_empty() && host.contains('.') && !host.starts_with('.') && !host.ends_with('.'))
            && !address.chars().any(|c| c.is_whitespace() || c.is_control() || "<>,;\"()".contains(c));
        if !ok {
            return vec![];
        }
        out.push(address.to_string());
    }
    out
}

/// Un texte d'en-tête : tel quel s'il est en ASCII, sinon sous la forme « =?UTF-8?B?...?= », par mots de 36 octets
/// au plus pour que les lignes restent courtes.
fn encoded(text: &str) -> String {
    if text.is_ascii() && !text.contains("=?") {
        return text.to_string();
    }
    let mut words = vec![];
    let mut chunk = String::new();
    for c in text.chars() {
        if chunk.len() + c.len_utf8() > 36 {
            words.push(format!("=?UTF-8?B?{}?=", base64(chunk.as_bytes())));
            chunk.clear();
        }
        chunk.push(c);
    }
    words.push(format!("=?UTF-8?B?{}?=", base64(chunk.as_bytes())));
    words.join("\r\n ")
}

/// Un champ d'adresses pour l'en-tête : les noms qui ont des accents sont encodés, les adresses restent telles quelles.
fn address_header(field: &str) -> String {
    pieces(field)
        .into_iter()
        .map(|piece| match (piece.rfind('<'), piece.rfind('>')) {
            (Some(a), Some(b)) if a < b => {
                let name = piece[..a].trim().trim_matches('"');
                let address = piece[a + 1..b].trim();
                if name.is_empty() {
                    address.to_string()
                } else if name.is_ascii() {
                    format!("\"{}\" <{address}>", name.replace(['"', '\\'], ""))
                } else {
                    format!("{} <{address}>", encoded(name))
                }
            }
            _ => piece.to_string(),
        })
        .collect::<Vec<_>>()
        .join(",\r\n ")
}

/// Un texte en base64, par lignes de 76 caractères.
fn base64_lines(data: &[u8]) -> String {
    let all = base64(data);
    let mut out = String::with_capacity(all.len() + all.len() / 38);
    for line in all.as_bytes().chunks(76) {
        out.push_str(std::str::from_utf8(line).unwrap_or_default());
        out.push_str("\r\n");
    }
    out
}

fn plain_part(text: &str) -> String {
    let body = text.replace("\r\n", "\n").replace('\n', "\r\n");
    // du texte simple aux lignes courtes part tel quel ; le reste (accents, longues lignes) en base64
    if body.is_ascii() && body.lines().all(|l| l.len() <= 78) {
        format!("Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 7bit\r\n\r\n{body}\r\n")
    } else {
        format!("Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}", base64_lines(body.as_bytes()))
    }
}

/// Le message tel qu'il part : en-têtes puis corps. `now` : l'instant de l'envoi (secondes depuis 1970) ; `unique` :
/// de quoi rendre son identifiant unique.
pub fn build(draft: &Draft, now: i64, unique: u64) -> Vec<u8> {
    let domain = draft.from.split_once('@').map_or("localhost", |(_, d)| d);
    let id = format!("{now:x}.{unique:016x}@{domain}");
    let mut head = String::new();
    let from = if draft.from_name.trim().is_empty() { draft.from.clone() } else { address_header(&format!("{} <{}>", draft.from_name.trim(), draft.from)) };
    head.push_str(&format!("From: {from}\r\nTo: {}\r\n", address_header(&draft.to)));
    if !addresses(&draft.cc).is_empty() {
        head.push_str(&format!("Cc: {}\r\n", address_header(&draft.cc)));
    }
    // les destinataires en copie cachée ne figurent nulle part dans le message
    head.push_str(&format!("Subject: {}\r\nDate: {}\r\nMessage-ID: <{id}>\r\n", encoded(draft.subject.trim()), rfc5322_date(now)));
    if !draft.in_reply_to.is_empty() {
        head.push_str(&format!("In-Reply-To: <{}>\r\n", draft.in_reply_to));
        let mut chain = draft.references.clone();
        if !chain.contains(&draft.in_reply_to) {
            chain.push(draft.in_reply_to.clone());
        }
        head.push_str(&format!("References: {}\r\n", chain.iter().map(|r| format!("<{r}>")).collect::<Vec<_>>().join("\r\n ")));
    }
    head.push_str("MIME-Version: 1.0\r\nUser-Agent: Plume Mail\r\n");
    if draft.files.is_empty() {
        head.push_str(&plain_part(&draft.text));
        return head.into_bytes();
    }
    let boundary = format!("plume-{unique:016x}-{now:x}");
    head.push_str(&format!("Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\r\n--{boundary}\r\n{}", plain_part(&draft.text)));
    for (name, data) in &draft.files {
        let shown = name.replace(['"', '\\', '\r', '\n'], "_");
        // un nom à accents s'écrit deux fois : en clair pour les lecteurs récents, encodé pour les autres
        let file = if shown.is_ascii() { format!("\"{shown}\"") } else { format!("\"{}\"", encoded(&shown)) };
        head.push_str(&format!(
            "--{boundary}\r\nContent-Type: application/octet-stream;\r\n name={file}\r\nContent-Disposition: attachment;\r\n filename={file}\r\nContent-Transfer-Encoding: base64\r\n\r\n{}",
            base64_lines(data)
        ));
    }
    head.push_str(&format!("--{boundary}--\r\n"));
    head.into_bytes()
}

/// Les adresses bien écrites d'un champ, en passant celles qui ne le sont pas (encore en train d'être tapées).
pub fn addresses_lenient(field: &str) -> Vec<String> {
    people_in(field).into_iter().map(|(_, address)| address).collect()
}

/// Les personnes d'un champ d'adresses : (nom, adresse), dans l'ordre, sans celles qui sont mal écrites.
pub fn people_in(field: &str) -> Vec<(String, String)> {
    pieces(field)
        .into_iter()
        .filter_map(|piece| {
            let (name, address) = match (piece.rfind('<'), piece.rfind('>')) {
                (Some(a), Some(b)) if a < b => (piece[..a].trim().trim_matches('"').trim(), piece[a + 1..b].trim()),
                _ => ("", piece),
            };
            (addresses(address).len() == 1).then(|| (name.to_string(), address.to_string()))
        })
        .collect()
}

/// Minuscules et sans les accents les plus courants : pour chercher sans y penser.
pub fn fold(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ñ' => 'n',
            'ÿ' => 'y',
            other => other,
        })
        .collect()
}

/// Les conversations d'une liste de messages : pour chacun, le numéro de sa conversation (le même pour tous les
/// messages qui se répondent, de près ou de loin, d'après « Message-ID » et « References »). Les numéros suivent
/// l'ordre de la liste : la conversation du premier message est la 0.
pub fn threads(messages: &[Summary]) -> Vec<usize> {
    use std::collections::HashMap;
    // chaque identifiant rencontré est un nœud ; un message relie le sien à tous ceux qu'il cite
    let mut ids: HashMap<&str, usize> = HashMap::new();
    let mut parent: Vec<usize> = vec![];
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut own: Vec<usize> = Vec::with_capacity(messages.len());
    for m in messages {
        // un message sans identifiant est seul dans sa conversation
        let me = parent.len();
        parent.push(me);
        if !m.message_id.is_empty() {
            match ids.get(m.message_id.as_str()) {
                Some(&other) => {
                    let (a, b) = (find(&mut parent, me), find(&mut parent, other));
                    parent[a] = b;
                }
                None => {
                    ids.insert(m.message_id.as_str(), me);
                }
            }
        }
        for r in &m.references {
            let other = match ids.get(r.as_str()) {
                Some(&n) => n,
                None => {
                    let n = parent.len();
                    parent.push(n);
                    ids.insert(r.as_str(), n);
                    n
                }
            };
            let (a, b) = (find(&mut parent, me), find(&mut parent, other));
            if a != b {
                parent[a] = b;
            }
        }
        own.push(me);
    }
    // des numéros dans l'ordre de la liste
    let mut number: HashMap<usize, usize> = HashMap::new();
    own.into_iter()
        .map(|n| {
            let root = find(&mut parent, n);
            let next = number.len();
            *number.entry(root).or_insert(next)
        })
        .collect()
}

/// L'objet d'une réponse ou d'un transfert : un seul « Re: », un seul « Fwd: ».
pub fn subject_with(prefix: &str, subject: &str) -> String {
    let s = subject.trim();
    if s.get(..prefix.len()).is_some_and(|p| p.eq_ignore_ascii_case(prefix)) { s.to_string() } else { format!("{prefix} {s}") }
}

/// Le texte d'un message, cité : chaque ligne précédée de « > », sous une ligne qui dit de qui et de quand il est.
pub fn quote(who: &str, when: &str, text: &str) -> String {
    let lines: Vec<String> = text.lines().map(|l| if l.is_empty() { ">".to_string() } else { format!("> {l}") }).collect();
    format!("\n\n{who}, {when}:\n{}\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RICH: &str = "From: =?utf-8?b?QW5uIETDqXBhcnQ=?= <ann@example.com>\r\nTo: Sam <sam@example.com>, bob@example.com\r\nCc: carl@example.com\r\nSubject: =?iso-8859-1?q?R=E9union_=E0_Montr=E9al?=\r\nDate: Tue, 10 Nov 2026 07:00:00 -0500\r\nMessage-ID: <m2@example.com>\r\nIn-Reply-To: <m1@example.com>\r\nReferences: <m0@example.com> <m1@example.com>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"b1\"\r\n\r\n--b1\r\nContent-Type: multipart/alternative; boundary=\"b2\"\r\n\r\n--b2\r\nContent-Type: text/plain; charset=iso-8859-1\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nBonjour,\r\nLa r=E9union est =E0 10 h.\r\n--b2\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Bonjour,<br>La r\u{e9}union est \u{e0} 10 h.</p>\r\n--b2--\r\n--b1\r\nContent-Type: application/pdf; name=\"plan =?utf-8?q?=C3=A9t=C3=A9?=.pdf\"\r\nContent-Disposition: attachment; filename=\"plan.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQK\r\n--b1--\r\n";

    #[test]
    fn reads_a_message() {
        let p = parse(RICH.as_bytes());
        let s = &p.summary;
        assert_eq!((s.subject.as_str(), s.from_name.as_str(), s.from.as_str()), ("Réunion à Montréal", "Ann Départ", "ann@example.com"));
        assert_eq!((s.to.as_str(), p.cc.as_str()), ("Sam <sam@example.com>, bob@example.com", "carl@example.com"));
        assert_eq!((s.date, s.message_id.as_str()), (1_794_312_000, "m2@example.com"));
        assert_eq!(s.references, ["m0@example.com", "m1@example.com"]);
        assert_eq!(p.text, "Bonjour,\nLa réunion est à 10 h.");
        assert!(!p.from_html && s.attachments);
        assert_eq!(p.attachments, [Attachment { name: "plan.pdf".into(), kind: "application/pdf".into(), size: 9, index: 0 }]);
        assert_eq!(attachment(RICH.as_bytes(), 0).as_deref(), Some(b"%PDF-1.4\n".as_slice()));
        assert_eq!(attachment(RICH.as_bytes(), 1), None);
        // les en-têtes seuls donnent la même ligne de liste
        let head = RICH.split("\r\n\r\n").next().unwrap();
        assert_eq!(summary(format!("{head}\r\n\r\n").as_bytes()), Summary { size: 0, ..s.clone() });
    }

    #[test]
    fn html_only_and_broken_messages() {
        let html = "From: news@example.com\r\nSubject: Hello\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body><h1>Title</h1><p>One &amp; two<br>three</p><script>alert(1)</script><img src=\"http://tracker.example/x.png\"><a href=\"https://example.com\">link</a></body></html>";
        let p = parse(html.as_bytes());
        assert_eq!(p.text, "Title\n\nOne & two\nthree\n\nlink (https://example.com)");
        assert!(p.from_html);
        assert!(!p.text.contains("alert") && !p.text.contains("<p>"), "{}", p.text);
        // n'importe quoi ne fait rien planter
        assert_eq!(parse(b"").text, "");
        let odd = parse(b"\xff\xfe not a message at all");
        assert_eq!(odd.summary.subject, "");
        assert_eq!(summary(b"Subject: only this\r\n\r\n").subject, "only this");
    }

    #[test]
    fn dates() {
        assert_eq!(parse_internal_date("10-Nov-2026 12:00:05 +0000"), Some(1_794_312_005));
        assert_eq!(parse_internal_date(" 9-Nov-2026 08:30:00 -0500"), Some(1_794_231_000));
        assert_eq!(parse_internal_date("01-Jan-1970 01:00:00 +0100"), Some(0));
        assert_eq!(parse_internal_date("nonsense"), None);
        assert_eq!(rfc5322_date(1_794_312_000), "Tue, 10 Nov 2026 12:00:00 +0000");
        assert_eq!(rfc5322_date(0), "Thu, 01 Jan 1970 00:00:00 +0000");
        assert_eq!(rfc5322_date(951_782_400), "Tue, 29 Feb 2000 00:00:00 +0000");
    }

    #[test]
    fn recipients() {
        assert_eq!(addresses("ann@example.com"), ["ann@example.com"]);
        assert_eq!(
            addresses(" Ann Example <ann@example.com>; \"Bob, Jr\" <bob@example.co.uk> ,carl@x.org,"),
            ["ann@example.com", "bob@example.co.uk", "carl@x.org"]
        );
        for bad in ["ann", "ann@", "@example.com", "ann@example", "ann @example.com", "ann@example.com, oops", "ann@.com", "a<b@example.com"] {
            assert!(addresses(bad).is_empty(), "{bad}");
        }
        assert!(addresses("").is_empty());
    }

    #[test]
    fn writes_what_it_can_read_back() {
        let draft = Draft {
            from_name: "Sam Élan".into(),
            from: "sam@example.com".into(),
            to: "Ann Départ <ann@example.com>, bob@example.com".into(),
            cc: "carl@example.com".into(),
            bcc: "secret@example.com".into(),
            subject: "Été à Montréal : le plan, les dates et tout ce qu'il faut savoir avant de partir".into(),
            text: "Bonjour,\n\nVoici le plan. Une très longue ligne suit pour vérifier qu'elle n'est pas coupée n'importe comment quand elle dépasse la largeur permise.\n\nSam".into(),
            in_reply_to: "m2@example.com".into(),
            references: vec!["m0@example.com".into(), "m1@example.com".into()],
            files: vec![("plan été.txt".into(), b"du contenu".to_vec()), ("data.bin".into(), (0..=255u8).collect())],
        };
        let raw = build(&draft, 1_794_312_000, 0xabc);
        let text = String::from_utf8(raw.clone()).unwrap();
        // rien que de l'ASCII, des lignes courtes, pas de copie cachée
        assert!(text.is_ascii() && text.lines().all(|l| l.len() <= 78), "{text}");
        assert!(!text.contains("secret@example.com") && text.contains("Date: Tue, 10 Nov 2026 12:00:00 +0000") && text.contains("User-Agent: Plume Mail"));
        let p = parse(&raw);
        assert_eq!(
            (p.summary.subject.as_str(), p.summary.from_name.as_str(), p.summary.from.as_str()),
            (draft.subject.as_str(), "Sam Élan", "sam@example.com")
        );
        assert_eq!(
            (p.summary.to.as_str(), p.cc.as_str(), p.summary.date),
            ("Ann Départ <ann@example.com>, bob@example.com", "carl@example.com", 1_794_312_000)
        );
        assert_eq!(p.summary.references, ["m0@example.com", "m1@example.com", "m2@example.com"]);
        assert_eq!(p.text, draft.text);
        assert_eq!(p.attachments.iter().map(|a| (a.name.as_str(), a.size)).collect::<Vec<_>>(), [("plan été.txt", 10), ("data.bin", 256)]);
        assert_eq!(attachment(&raw, 1).unwrap(), (0..=255u8).collect::<Vec<_>>());
        assert!(p.summary.message_id.ends_with("@example.com") && p.summary.message_id.contains("abc"));
        // un message simple part tel quel
        let simple = build(
            &Draft { from: "sam@example.com".into(), to: "ann@example.com".into(), subject: "Hi".into(), text: "Hello\nthere".into(), ..Draft::default() },
            0,
            1,
        );
        let simple = String::from_utf8(simple).unwrap();
        assert!(
            simple.contains("Content-Transfer-Encoding: 7bit\r\n\r\nHello\r\nthere\r\n")
                && simple.starts_with("From: sam@example.com\r\nTo: ann@example.com\r\nSubject: Hi\r\n"),
            "{simple}"
        );
    }

    #[test]
    fn conversations_people_and_folding() {
        let m = |uid: u32, id: &str, refs: &[&str]| Summary { uid, message_id: id.into(), references: refs.iter().map(|r| r.to_string()).collect(), ..Summary::default() };
        // du plus récent au plus ancien : deux réponses à un message absent du dossier, une réponse à une réponse
        let list = [
            m(9, "c@x", &["a@x", "b@x"]),
            m(8, "z@x", &[]),
            m(7, "b@x", &["a@x"]),
            m(6, "", &[]),
            m(5, "d@x", &["root@x"]),
            m(4, "e@x", &["root@x"]),
        ];
        assert_eq!(threads(&list), [0, 1, 0, 2, 3, 3]);
        assert!(threads(&[]).is_empty());
        assert_eq!(
            people_in("Ann Départ <ann@example.com>, \"Bob, Jr\" <bob@example.com>; carl@x.org, oops"),
            [("Ann Départ".to_string(), "ann@example.com".to_string()), ("Bob, Jr".into(), "bob@example.com".into()), ("".into(), "carl@x.org".into())]
        );
        assert_eq!(addresses_lenient("ann@example.com, bo"), ["ann@example.com"]);
        assert_eq!(fold("Réunion À MONTRÉAL ñ"), "reunion a montreal n");
    }

    #[test]
    fn replies() {
        assert_eq!(subject_with("Re:", "Hello"), "Re: Hello");
        assert_eq!(subject_with("Re:", "RE: Hello"), "RE: Hello");
        assert_eq!(subject_with("Fwd:", " Re: Hello "), "Fwd: Re: Hello");
        assert_eq!(quote("Ann", "2026-11-10 07:00", "Hi\n\nBye"), "\n\nAnn, 2026-11-10 07:00:\n> Hi\n>\n> Bye\n");
    }
}
