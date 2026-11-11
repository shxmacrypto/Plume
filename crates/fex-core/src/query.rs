//! La recherche dans l'index, avec la syntaxe d'Everything :
//!
//! - `rapport 2026` : les deux mots, dans n'importe quel ordre ; `"rapport final"` : la suite exacte ;
//! - `jpg|png` : l'un ou l'autre ; `!brouillon` : sans ce mot ; `<a b>|c` : des chevrons pour grouper ;
//! - `*.rs`, `photo??.jpg` : jokers, sur le nom entier ; `regex:^img_\d+` : expression régulière ;
//! - `src\main` ou `path:projets` : dans le chemin complet ;
//! - filtres : `ext:jpg;png`, `size:>10mb`, `dm:today`, `dm:last2weeks`, `file:`, `folder:`, `attrib:h`, `len:>100`,
//!   `depth:2`, `parent:C:\dossier`, `root:`, `count:100` ;
//! - types : `audio:`, `video:`, `pic:`, `doc:`, `exe:`, `zip:` ;
//! - dossiers : `empty:`, `childcount:>100`, `childfilecount:0`, `childfoldercount:1`, `child:setup.exe` ;
//! - doublons : `dupe:` (même nom), `namepartdupe:` (même nom sans l'extension), `sizedupe:`, `dmdupe:` ;
//! - façon de comparer : `case:`, `diacritics:`, `wfn:` (nom entier), `ww:` (mot entier), `startwith:`, `endwith:`.
//!
//! Par défaut, ni la casse ni les accents ne comptent : « ete » trouve « Été ». Sans rien d'autre qu'un mot, la
//! recherche est une recherche de texte sur la zone des noms (voir `index`), sans copie des noms en minuscules.

use crate::index::{F_DELETED, F_DENIED, F_DIR, F_HIDDEN, F_LINK, F_OFFLINE, F_READONLY, F_SYSTEM, Index, NO_PARENT, fold_into};
use memchr::{memchr, memchr2, memmem};
use regex_automata::{Input, meta, util::syntax};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

/// L'heure qu'il est, pour les filtres de date (`dm:today`). En FILETIME ; `bias` : heure locale − UTC.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    pub now_utc: u64,
    pub bias: i64,
}

const DAY: u64 = 864_000_000_000;
const SECOND: u64 = 10_000_000;

impl Clock {
    /// L'heure de l'ordinateur et son décalage horaire actuel.
    pub fn system() -> Clock {
        use windows::Win32::{Foundation::FILETIME, Storage::FileSystem::FileTimeToLocalFileTime, System::SystemInformation::GetSystemTimeAsFileTime};
        let value = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
        unsafe {
            let now = GetSystemTimeAsFileTime();
            let mut local = FILETIME::default();
            let _ = FileTimeToLocalFileTime(&now, &mut local);
            Clock { now_utc: value(now), bias: value(local) as i64 - value(now) as i64 }
        }
    }
}

/// Les types de fichiers d'Everything : (préfixe, extensions).
pub const KINDS: [(&str, &str); 6] = [
    ("audio", "aac;ac3;aif;aifc;aiff;au;cda;dts;fla;flac;it;m1a;m2a;m3u;m4a;mid;midi;mka;mod;mp2;mp3;mpa;ogg;opus;ra;rmi;snd;spc;umx;voc;wav;wma;xm"),
    ("zip", "7z;ace;arj;bz2;cab;gz;gzip;iso;jar;lzh;rar;tar;tbz2;tgz;txz;xz;z;zip;zst"),
    (
        "doc",
        "c;chm;cpp;csv;cxx;doc;docm;docx;dot;dotm;dotx;h;hpp;htm;html;hxx;ini;java;json;lua;md;mht;mhtml;odp;ods;odt;pdf;potm;potx;ppam;pps;ppsm;ppsx;ppt;pptm;pptx;rs;rtf;sldm;sldx;thmx;toml;txt;vsd;wpd;wps;wri;xlam;xls;xlsb;xlsm;xlsx;xltm;xltx;xml;yaml;yml",
    ),
    ("exe", "bat;cmd;com;exe;msi;msp;ps1;scr"),
    ("pic", "ani;avif;bmp;gif;heic;ico;jpe;jpeg;jpg;pcx;png;psd;svg;tga;tif;tiff;webp;wmf"),
    ("video", "3g2;3gp;3gp2;3gpp;amv;asf;avi;divx;f4v;flv;m1v;m2t;m2ts;m2v;m4v;mkv;mov;mp4;mp4v;mpe;mpeg;mpg;mts;ogm;ogv;qt;rm;rmvb;ts;vob;webm;wmv"),
];

// ───────────────────────── casse et accents ─────────────────────────

/// Comment les noms se comparent au texte cherché.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Fold {
    /// Sans tenir compte de la casse ni des accents : « ete » trouve « Été ».
    #[default]
    Plain,
    /// Sans tenir compte de la casse, mais les accents comptent (`diacritics:`).
    Marks,
    /// Tel que c'est écrit (`case:`).
    Exact,
}

/// La lettre latine sans son accent ; None : caractère à garder tel quel. Attend une minuscule.
fn unaccent(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'ď' | 'đ' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'ĥ' | 'ħ' => "h",
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
        'ĵ' => "j",
        'ķ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'ñ' | 'ń' | 'ņ' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => "o",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'ś' | 'ŝ' | 'ş' | 'š' | 'ș' => "s",
        'ţ' | 'ť' | 'ŧ' | 'ț' => "t",
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'ŵ' => "w",
        'ý' | 'ÿ' | 'ŷ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        'æ' => "ae",
        'œ' => "oe",
        'ß' => "ss",
        // accent écrit à part de sa lettre (noms venus d'un Mac) : il disparaît
        '\u{300}'..='\u{36f}' => "",
        _ => return None,
    })
}

/// Ajoute à `out` le texte en minuscules et sans accents : la forme sous laquelle noms et requêtes se comparent.
pub fn plain_into(s: &str, out: &mut Vec<u8>) {
    if s.is_ascii() {
        out.extend(s.bytes().map(|b| b.to_ascii_lowercase()));
        return;
    }
    let mut buf = [0u8; 4];
    for c in s.chars().flat_map(char::to_lowercase) {
        match if c.is_ascii() { None } else { unaccent(c) } {
            Some(base) => out.extend_from_slice(base.as_bytes()),
            None => out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes()),
        }
    }
}

/// Le texte en minuscules et sans accents.
pub fn plain(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    plain_into(s, &mut out);
    out
}

impl Fold {
    /// Un texte de la requête sous la forme où il se compare aux noms.
    fn text(self, s: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(s.len());
        match self {
            Fold::Exact => out.extend_from_slice(s.as_bytes()),
            Fold::Marks => fold_into(s, &mut out),
            Fold::Plain => plain_into(s, &mut out),
        }
        out
    }

    /// Le nom d'un élément sous la forme où il se compare à la requête (écrit dans `buf` s'il faut le transformer).
    fn name<'a>(self, ix: &'a Index, i: u32, buf: &'a mut Vec<u8>) -> &'a [u8] {
        let name = ix.name(i);
        match self {
            Fold::Exact => name.as_bytes(),
            Fold::Plain if !name.is_ascii() => {
                buf.clear();
                plain_into(name, buf);
                buf
            }
            _ => ix.folded(i, buf),
        }
    }
}

// ───────────────────────── la requête ─────────────────────────

/// Sur quoi deux éléments se ressemblent, pour la recherche de doublons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Same {
    Name,
    /// Le nom sans son extension.
    Stem,
    Size,
    Modified,
}

#[derive(Clone, Debug)]
enum Test {
    /// Le nom contient ce texte.
    Contains(Vec<u8>),
    /// Le nom entier correspond au motif (`*`, `?`).
    Glob(Vec<u8>),
    Starts(Vec<u8>),
    Ends(Vec<u8>),
    /// Le nom contient ce mot entier.
    Word(Vec<u8>),
    /// Le nom entier est ce texte.
    Whole(Vec<u8>),
    /// Le nom, tel qu'il est écrit, correspond à l'expression régulière (son numéro dans `Query::regexes`).
    Regex(usize),
    /// Le chemin complet contient ce texte (morceaux séparés par `\`).
    Path(Vec<Vec<u8>>),
    Ext(Vec<Vec<u8>>),
    Size(u64, u64),
    Modified(u64, u64),
    Dir(bool),
    /// Tous ces attributs sont présents (bits de l'index).
    Attr(u8),
    /// Longueur du nom, en caractères.
    Len(u64, u64),
    /// Nombre de dossiers au-dessus de l'élément.
    Depth(u64, u64),
    /// Directement à la racine d'un disque.
    Root,
    /// Enfant direct de ce dossier.
    Parent(String),
    /// Dossier qui contient ce nombre d'éléments (None), de dossiers (Some(true)) ou de fichiers (Some(false)).
    Children(Option<bool>, u64, u64),
    /// Dossier qui contient un élément dont le nom contient ce texte.
    Child(Vec<u8>),
    /// Élément qui en a un autre semblable.
    Dupe(Same),
    /// Résultat calculé avant la recherche : un bit par élément (voir `prepare`).
    Bits(usize),
    /// Jamais vrai (filtre incompris).
    Nothing,
}

#[derive(Clone, Debug)]
enum Node {
    Test(Test),
    Not(Box<Node>),
    And(Vec<Node>),
    Or(Vec<Node>),
}

#[derive(Clone, Debug, Default)]
pub struct Query {
    root: Option<Node>,
    fold: Fold,
    /// `count:` : nombre de résultats à ne pas dépasser.
    count: Option<usize>,
    /// Les expressions régulières de la requête ; les tests les désignent par leur numéro.
    regexes: Vec<meta::Regex>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Les noms qui commencent par le texte cherché d'abord, puis l'ordre alphabétique.
    Name,
    Size,
    Modified,
    /// Par dossier, dans l'ordre de l'arbre ; dans un même dossier, par nom.
    Path,
}

#[derive(Debug, Default)]
pub struct Found {
    /// Numéros des premiers éléments trouvés (`limit` au plus), dans l'ordre demandé.
    pub ids: Vec<u32>,
    /// Nombre total d'éléments qui correspondent.
    pub total: usize,
}

/// Un type d'éléments à garder seul, choisi dans l'interface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    All,
    Folders,
    /// Un type de fichiers : son numéro dans [`KINDS`].
    Files(usize),
}

/// Les réglages de recherche choisis dans l'interface, en plus du texte tapé.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Respecter la casse (comme `case:`).
    pub case: bool,
    /// Chaque mot doit être un mot entier (comme `ww:`).
    pub word: bool,
    /// Chercher dans le chemin complet (comme `path:`).
    pub path: bool,
    /// Tout le texte est une seule expression régulière.
    pub regex: bool,
    pub kind: Kind,
}

// ───────────────────────── découpage de la requête ─────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Or,
    Not,
    Open,
    Close,
}

/// Comment un mot se compare au nom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Plain,
    Regex,
    Whole,
    Word,
    Path,
    Starts,
    Ends,
}

/// Un préfixe qui dit comment comprendre le mot qui suit. Plusieurs peuvent se suivre : `file:case:regex:^Main`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prefix {
    Case,
    Marks,
    /// Compris, mais c'est déjà ce qui se fait sans lui.
    Usual,
    Files,
    Folders,
    /// Un type de fichiers (numéro dans `KINDS`).
    Kind(usize),
    Mode(Mode),
}

/// Retire `prefix` du début de `word`, sans tenir compte de la casse.
fn strip<'a>(word: &'a str, prefix: &str) -> Option<&'a str> {
    (word.len() >= prefix.len() && word.is_char_boundary(prefix.len()) && word[..prefix.len()].eq_ignore_ascii_case(prefix)).then(|| &word[prefix.len()..])
}

fn strip_any<'a>(word: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes.iter().find_map(|p| strip(word, p))
}

/// Retire du début de `w` un préfixe qui dit comment comprendre la suite.
fn prefix(w: &str) -> Option<(Prefix, &str)> {
    const TABLE: [(&str, Prefix); 20] = [
        ("case:", Prefix::Case),
        ("diacritics:", Prefix::Marks),
        ("nocase:", Prefix::Usual),
        ("nodiacritics:", Prefix::Usual),
        ("nopath:", Prefix::Usual),
        ("wildcards:", Prefix::Usual),
        ("files:", Prefix::Files),
        ("file:", Prefix::Files),
        ("folders:", Prefix::Folders),
        ("folder:", Prefix::Folders),
        ("regex:", Prefix::Mode(Mode::Regex)),
        ("wfn:", Prefix::Mode(Mode::Whole)),
        ("wholefilename:", Prefix::Mode(Mode::Whole)),
        ("ww:", Prefix::Mode(Mode::Word)),
        ("wholeword:", Prefix::Mode(Mode::Word)),
        ("path:", Prefix::Mode(Mode::Path)),
        ("startwith:", Prefix::Mode(Mode::Starts)),
        ("startswith:", Prefix::Mode(Mode::Starts)),
        ("endwith:", Prefix::Mode(Mode::Ends)),
        ("endswith:", Prefix::Mode(Mode::Ends)),
    ];
    TABLE
        .iter()
        .find_map(|(p, kind)| strip(w, p).map(|rest| (*kind, rest)))
        .or_else(|| KINDS.iter().enumerate().find_map(|(k, (name, _))| strip(w, name)?.strip_prefix(':').map(|rest| (Prefix::Kind(k), rest))))
}

/// Les préfixes cumulés devant un mot, et ce qui reste. Après `regex:`, tout est l'expression.
fn prefixes(word: &str) -> (Vec<Prefix>, &str) {
    let (mut list, mut rest) = (vec![], word);
    while let Some((p, r)) = prefix(rest) {
        list.push(p);
        rest = r;
        if p == Prefix::Mode(Mode::Regex) {
            break;
        }
    }
    (list, rest)
}

/// Le mot en cours est-il une expression régulière ? Tout ce qui suit `regex:` est alors du texte, `|` et `<` compris.
fn in_regex(cur: &str) -> bool {
    prefixes(cur.trim_start_matches('!')).0.last() == Some(&Prefix::Mode(Mode::Regex))
}

/// Découpe la requête : les guillemets protègent les espaces, `|` sépare des alternatives, `!` exclut, `<` et `>`
/// groupent (sauf après un `:`, où ils comparent : `size:>1mb`).
fn tokens(s: &str) -> Vec<Tok> {
    fn flush(cur: &mut String, out: &mut Vec<Tok>) {
        match cur.as_str() {
            "" => {}
            "!" => out.push(Tok::Not),
            _ => out.push(Tok::Word(cur.clone())),
        }
        cur.clear();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = vec![];
    let mut cur = String::new();
    let (mut quoted, mut depth) = (false, 0usize);
    for (i, &c) in chars.iter().enumerate() {
        let next_ends = chars.get(i + 1).is_none_or(|n| n.is_whitespace() || *n == '|' || *n == '>');
        match c {
            '"' => {
                quoted = !quoted;
                cur.push('"');
            }
            c if c.is_whitespace() && !quoted => flush(&mut cur, &mut out),
            '|' if !quoted && !in_regex(&cur) => {
                flush(&mut cur, &mut out);
                out.push(Tok::Or);
            }
            '<' if !quoted && (cur.is_empty() || cur == "!") => {
                flush(&mut cur, &mut out);
                out.push(Tok::Open);
                depth += 1;
            }
            '>' if !quoted && depth > 0 && next_ends && !cur.ends_with([':', '<', '>', '=']) && !in_regex(&cur) => {
                flush(&mut cur, &mut out);
                out.push(Tok::Close);
                depth -= 1;
            }
            c => cur.push(c),
        }
    }
    flush(&mut cur, &mut out);
    out
}

fn unquote(s: &str) -> String {
    s.replace('"', "")
}

fn number(s: &str) -> Option<(f64, &str)> {
    let end = s.find(|c: char| !c.is_ascii_digit() && c != '.' && c != ',').unwrap_or(s.len());
    let n = s[..end].replace(',', ".").parse().ok()?;
    Some((n, &s[end..]))
}

/// « 10mb », « 1.5 gb », « 300 » (octets).
fn size_value(s: &str) -> Option<u64> {
    let (n, unit) = number(s.trim())?;
    let k: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" | "o" => 1.0,
        "k" | "kb" | "ko" => 1024.0,
        "m" | "mb" | "mo" => 1024.0 * 1024.0,
        "g" | "gb" | "go" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" | "to" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * k) as u64)
}

/// Sépare l'opérateur de comparaison de sa valeur : « >=10 » donne (">=", "10") ; sans opérateur : égalité.
fn compare(v: &str) -> (&str, &str) {
    ["<=", ">=", "<", ">", "="].iter().find_map(|op| v.strip_prefix(op).map(|r| (*op, r))).unwrap_or(("=", v))
}

fn size_test(v: &str) -> Test {
    let v = v.trim().to_ascii_lowercase();
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    let named = match v.as_str() {
        "empty" | "vide" => Some((0, 0)),
        "tiny" => Some((1, 10 * KB)),
        "small" => Some((10 * KB + 1, 100 * KB)),
        "medium" => Some((100 * KB + 1, MB)),
        "large" => Some((MB + 1, 16 * MB)),
        "huge" => Some((16 * MB + 1, 128 * MB)),
        "gigantic" => Some((128 * MB + 1, u64::MAX)),
        _ => None,
    };
    if let Some((a, b)) = named {
        return Test::Size(a, b);
    }
    if let Some((a, b)) = v.split_once("..") {
        return match (size_value(a), size_value(b)) {
            (Some(a), Some(b)) => Test::Size(a.min(b), a.max(b)),
            _ => Test::Nothing,
        };
    }
    let (op, rest) = compare(&v);
    match (op, size_value(rest)) {
        (">", Some(n)) => Test::Size(n.saturating_add(1), u64::MAX),
        (">=", Some(n)) => Test::Size(n, u64::MAX),
        ("<", Some(n)) => Test::Size(0, n.saturating_sub(1)),
        ("<=", Some(n)) => Test::Size(0, n),
        // « size:10mb » : à peu près cette taille (à 5 % près)
        ("=", Some(n)) => Test::Size(n - n / 20, n.saturating_add(n / 20)),
        _ => Test::Nothing,
    }
}

/// Intervalle de nombres entiers : « 5 », « >5 », « <=5 », « 2..8 ».
fn range(v: &str) -> Option<(u64, u64)> {
    let v = v.trim();
    if let Some((a, b)) = v.split_once("..") {
        let (a, b): (u64, u64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
        return Some((a.min(b), a.max(b)));
    }
    let (op, rest) = compare(v);
    let n: u64 = rest.trim().parse().ok()?;
    Some(match op {
        ">" => (n.checked_add(1)?, u64::MAX),
        ">=" => (n, u64::MAX),
        "<" => (0, n.checked_sub(1)?),
        "<=" => (0, n),
        _ => (n, n),
    })
}

/// Jours depuis 1601-01-01 pour une date du calendrier grégorien.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    // 1970-01-01 est le jour 719468 de ce calcul ; 1601-01-01 est 134774 jours avant 1970
    era * 146097 + doe - 719468 + 134774
}

/// (année, mois, jour, jour de la semaine avec lundi = 0) d'un nombre de jours depuis 1601.
fn civil_from_days(days: i64) -> (i64, i64, i64, i64) {
    let z = days - 134774 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    // le 1er janvier 1601 était un lundi
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d, days.rem_euclid(7))
}

fn next_month(y: i64, m: i64) -> (i64, i64) {
    if m == 12 { (y + 1, 1) } else { (y, m + 1) }
}

/// « 2026 », « 2026-11 », « 2026-11-10 » (ou avec `/`) : premier jour et lendemain du dernier, en jours depuis 1601.
fn date_span(s: &str) -> Option<(i64, i64)> {
    let parts: Vec<i64> = s.split(['-', '/']).map(|p| p.parse().ok()).collect::<Option<_>>()?;
    match parts.as_slice() {
        [y] if (1601..=9999).contains(y) => Some((days_from_civil(*y, 1, 1), days_from_civil(y + 1, 1, 1))),
        [y, m] if (1..=12).contains(m) => {
            let (ny, nm) = next_month(*y, *m);
            Some((days_from_civil(*y, *m, 1), days_from_civil(ny, nm, 1)))
        }
        [y, m, d] if (1..=12).contains(m) && (1..=31).contains(d) => {
            let a = days_from_civil(*y, *m, *d);
            Some((a, a + 1))
        }
        _ => None,
    }
}

/// « last2weeks », « past30minutes », « last3days » : une durée qui se termine maintenant, en secondes.
fn recent(v: &str) -> Option<u64> {
    let rest = ["last", "past", "prev"].iter().find_map(|p| v.strip_prefix(p))?;
    let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    let n: u64 = if digits == 0 { 1 } else { rest[..digits].parse().ok()? };
    let unit = match rest[digits..].trim_end_matches('s') {
        "second" | "sec" => 1,
        "minute" | "min" => 60,
        "hour" => 3600,
        "day" => 86_400,
        "week" => 7 * 86_400,
        "month" => 30 * 86_400,
        "year" => 365 * 86_400,
        _ => return None,
    };
    // « lastweek », « lastmonth », « lastyear » sans nombre désignent la période précédente du calendrier
    (digits > 0 || unit < 7 * 86_400).then_some(n.saturating_mul(unit))
}

fn date_test(v: &str, clock: &Clock) -> Test {
    let v = v.trim().to_ascii_lowercase();
    let local_now = (clock.now_utc as i64 + clock.bias).max(0);
    let today = local_now / DAY as i64;
    let (y, m, _, weekday) = civil_from_days(today);
    // bornes en jours locaux : [début, fin[
    let span = |a: i64, b: i64| {
        let to_utc = |day: i64| (day * DAY as i64 - clock.bias).max(0) as u64;
        Test::Modified(to_utc(a), to_utc(b).saturating_sub(1))
    };
    let week = today - weekday;
    match v.as_str() {
        "today" | "aujourdhui" => return span(today, today + 1),
        "yesterday" | "hier" => return span(today - 1, today),
        "thisweek" => return span(week, week + 7),
        "lastweek" => return span(week - 7, week),
        "thismonth" => {
            let (ny, nm) = next_month(y, m);
            return span(days_from_civil(y, m, 1), days_from_civil(ny, nm, 1));
        }
        "lastmonth" => {
            let (py, pm) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
            return span(days_from_civil(py, pm, 1), days_from_civil(y, m, 1));
        }
        "thisyear" => return span(days_from_civil(y, 1, 1), days_from_civil(y + 1, 1, 1)),
        "lastyear" => return span(days_from_civil(y - 1, 1, 1), days_from_civil(y, 1, 1)),
        _ => {}
    }
    if let Some(seconds) = recent(&v) {
        return Test::Modified(clock.now_utc.saturating_sub(seconds.saturating_mul(SECOND)), clock.now_utc);
    }
    if let Some((a, b)) = v.split_once("..") {
        return match (date_span(a), date_span(b)) {
            (Some(a), Some(b)) => span(a.0.min(b.0), a.1.max(b.1)),
            _ => Test::Nothing,
        };
    }
    let (op, rest) = compare(&v);
    let far = days_from_civil(9999, 1, 1);
    match (op, date_span(rest)) {
        (">", Some((_, b))) => span(b, far),
        (">=", Some((a, _))) => span(a, far),
        ("<", Some((a, _))) => span(0, a),
        ("<=", Some((_, b))) => span(0, b),
        ("=", Some((a, b))) => span(a, b),
        _ => Test::Nothing,
    }
}

/// Lettres d'attributs d'Everything → bits de l'index. Une lettre que l'index ne connaît pas : None.
fn attributes(v: &str) -> Option<u8> {
    v.trim().chars().try_fold(0u8, |bits, c| {
        Some(
            bits | match c.to_ascii_lowercase() {
                'd' => F_DIR,
                'h' => F_HIDDEN,
                's' => F_SYSTEM,
                'l' => F_LINK,
                'r' => F_READONLY,
                'o' => F_OFFLINE,
                _ => return None,
            },
        )
    })
}

fn path_test(t: &[u8]) -> Test {
    let parts: Vec<Vec<u8>> = t.split(|&b| b == b'\\' || b == b'/').map(<[u8]>::to_vec).collect();
    if parts.iter().all(Vec::is_empty) { Test::Nothing } else { Test::Path(parts) }
}

/// Les tests d'un type de fichiers : un fichier, dont l'extension est dans la liste.
fn kind_tests(k: usize) -> [Test; 2] {
    [Test::Dir(false), Test::Ext(KINDS[k].1.split(';').map(|e| e.as_bytes().to_vec()).collect())]
}

fn and_of(mut items: Vec<Node>) -> Option<Node> {
    match items.len() {
        0 => None,
        1 => items.pop(),
        _ => Some(Node::And(items)),
    }
}

fn or_of(mut items: Vec<Node>) -> Option<Node> {
    match items.len() {
        0 => None,
        1 => items.pop(),
        _ => Some(Node::Or(items)),
    }
}

struct Parser<'a> {
    toks: &'a [Tok],
    at: usize,
    fold: Fold,
    count: Option<usize>,
    regexes: Vec<meta::Regex>,
    clock: &'a Clock,
    /// Façon de comparer un mot qui n'en dit rien lui-même (réglage de l'interface).
    mode: Mode,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    /// Une suite de termes, tous exigés. Dans un groupe, elle s'arrête au chevron fermant.
    fn sequence(&mut self, grouped: bool) -> Option<Node> {
        let mut items = vec![];
        while let Some(t) = self.peek() {
            if *t == Tok::Close {
                if grouped {
                    break;
                }
                // un chevron fermant de trop : on passe
                self.at += 1;
                continue;
            }
            if let Some(n) = self.alternatives() {
                items.push(n);
            }
        }
        and_of(items)
    }

    /// Des termes séparés par `|` : un seul suffit. Le `|` lie plus fort que l'espace, comme dans Everything.
    fn alternatives(&mut self) -> Option<Node> {
        let mut items = vec![];
        loop {
            while self.peek() == Some(&Tok::Or) {
                self.at += 1;
            }
            if matches!(self.peek(), None | Some(Tok::Close)) {
                break;
            }
            if let Some(n) = self.term() {
                items.push(n);
            }
            if self.peek() != Some(&Tok::Or) {
                break;
            }
        }
        or_of(items)
    }

    fn term(&mut self) -> Option<Node> {
        let tok = self.toks.get(self.at)?.clone();
        self.at += 1;
        match tok {
            Tok::Not => self.term().map(|n| Node::Not(Box::new(n))),
            Tok::Open => {
                let inner = self.sequence(true);
                if self.peek() == Some(&Tok::Close) {
                    self.at += 1;
                }
                inner
            }
            Tok::Word(w) => self.word(&w),
            Tok::Or | Tok::Close => None,
        }
    }

    fn text(&self, v: &str) -> Vec<u8> {
        self.fold.text(&unquote(v))
    }

    /// Une expression régulière ; mal formée, elle ne trouve rien.
    fn regex(&mut self, pattern: &str) -> Test {
        match meta::Regex::builder().syntax(syntax::Config::new().case_insensitive(self.fold != Fold::Exact)).build(pattern) {
            Ok(re) => {
                self.regexes.push(re);
                Test::Regex(self.regexes.len() - 1)
            }
            Err(_) => Test::Nothing,
        }
    }

    /// Un filtre : son nom, puis sa valeur jusqu'au bout du mot.
    fn function(&self, w: &str) -> Option<Test> {
        let span = |v: &str, make: fn(u64, u64) -> Test| range(&unquote(v)).map_or(Test::Nothing, |(a, b)| make(a, b));
        Some(if let Some(v) = strip(w, "ext:") {
            Test::Ext(unquote(v).split(';').map(|e| self.fold.text(e.trim().trim_start_matches('.'))).filter(|e| !e.is_empty()).collect())
        } else if let Some(v) = strip(w, "size:") {
            size_test(&unquote(v))
        } else if let Some(v) = strip_any(w, &["dm:", "datemodified:"]) {
            date_test(&unquote(v), self.clock)
        } else if let Some(v) = strip_any(w, &["parent:", "infolder:", "nosubfolders:"]) {
            Test::Parent(unquote(v).replace('/', "\\").trim_end_matches('\\').to_string())
        } else if let Some(v) = strip_any(w, &["attrib:", "attributes:"]) {
            attributes(&unquote(v)).map_or(Test::Nothing, Test::Attr)
        } else if let Some(v) = strip(w, "len:") {
            span(v, Test::Len)
        } else if let Some(v) = strip_any(w, &["depth:", "parents:"]) {
            span(v, Test::Depth)
        } else if let Some(v) = strip(w, "childcount:") {
            span(v, |a, b| Test::Children(None, a, b))
        } else if let Some(v) = strip(w, "childfilecount:") {
            span(v, |a, b| Test::Children(Some(false), a, b))
        } else if let Some(v) = strip(w, "childfoldercount:") {
            span(v, |a, b| Test::Children(Some(true), a, b))
        } else if let Some(v) = strip(w, "child:") {
            let t = self.text(v);
            if t.is_empty() { Test::Nothing } else { Test::Child(t) }
        } else if strip(w, "empty:").is_some() {
            Test::Children(None, 0, 0)
        } else if strip(w, "dupe:").is_some() {
            Test::Dupe(Same::Name)
        } else if strip(w, "namepartdupe:").is_some() {
            Test::Dupe(Same::Stem)
        } else if strip(w, "sizedupe:").is_some() {
            Test::Dupe(Same::Size)
        } else if strip(w, "dmdupe:").is_some() {
            Test::Dupe(Same::Modified)
        } else if strip(w, "root:").is_some() {
            Test::Root
        } else {
            return None;
        })
    }

    /// Un mot de la requête : des préfixes qui disent comment comparer, puis un filtre ou un texte.
    fn word(&mut self, word: &str) -> Option<Node> {
        let (not, word) = match word.strip_prefix('!') {
            Some(rest) if !rest.is_empty() => (true, rest),
            _ => (false, word),
        };
        let (list, w) = prefixes(word);
        if let Some(v) = strip(w, "count:") {
            self.count = unquote(v).trim().parse().ok().or(self.count);
            return None;
        }
        let mut tests = vec![];
        let mut mode = None;
        for p in list {
            match p {
                Prefix::Files => tests.push(Test::Dir(false)),
                Prefix::Folders => tests.push(Test::Dir(true)),
                Prefix::Kind(k) => tests.extend(kind_tests(k)),
                Prefix::Mode(m) => mode = Some(m),
                Prefix::Case | Prefix::Marks | Prefix::Usual => {}
            }
        }
        // un mot précédé d'une façon de comparer (« ww: », « regex: »...) est un texte, pas un filtre
        let function = if mode.is_none() { self.function(w) } else { None };
        match function {
            Some(t) => tests.push(t),
            None => {
                let quoted = w.contains('"');
                let t = self.text(w);
                if !t.is_empty() {
                    // sans façon de comparer écrite devant lui, un mot à jokers ou à barres se reconnaît tout seul
                    let free = mode.is_none();
                    tests.push(match mode.unwrap_or(self.mode) {
                        Mode::Regex => self.regex(&unquote(w)),
                        _ if free && !quoted && (t.contains(&b'*') || t.contains(&b'?')) => Test::Glob(t),
                        _ if free && (t.contains(&b'\\') || t.contains(&b'/')) => path_test(&t),
                        Mode::Whole => Test::Whole(t),
                        Mode::Word => Test::Word(t),
                        Mode::Path => path_test(&t),
                        Mode::Starts => Test::Starts(t),
                        Mode::Ends => Test::Ends(t),
                        Mode::Plain => Test::Contains(t),
                    });
                }
            }
        }
        let node = and_of(tests.into_iter().map(Node::Test).collect())?;
        Some(if not { Node::Not(Box::new(node)) } else { node })
    }
}

impl Query {
    pub fn parse(s: &str, clock: &Clock) -> Query {
        Query::parse_with(s, clock, &Options::default())
    }

    /// Comme [`Query::parse`], avec les réglages choisis dans l'interface.
    pub fn parse_with(s: &str, clock: &Clock, o: &Options) -> Query {
        // en mode « expression régulière », tout le texte en est une : rien d'autre n'y est reconnu
        let toks = if o.regex { vec![] } else { tokens(s) };
        // « case: » et « diacritics: », où qu'ils soient, valent pour toute la requête
        let has = |p: Prefix| toks.iter().any(|t| matches!(t, Tok::Word(w) if prefixes(w.trim_start_matches('!')).0.contains(&p)));
        let fold = if o.case || has(Prefix::Case) {
            Fold::Exact
        } else if has(Prefix::Marks) {
            Fold::Marks
        } else {
            Fold::Plain
        };
        let mode = if o.path {
            Mode::Path
        } else if o.word {
            Mode::Word
        } else {
            Mode::Plain
        };
        let mut parser = Parser { toks: &toks, at: 0, fold, count: None, regexes: vec![], clock, mode };
        let typed = if o.regex { Some(s.trim()).filter(|t| !t.is_empty()).map(|t| Node::Test(parser.regex(t))) } else { parser.sequence(false) };
        // le type choisi s'ajoute à ce qui est tapé ; sans texte, il ne cherche rien à lui seul
        let root = typed.map(|typed| {
            let kind: Vec<Node> = match o.kind {
                Kind::Folders => vec![Node::Test(Test::Dir(true))],
                Kind::Files(k) if k < KINDS.len() => kind_tests(k).into_iter().map(Node::Test).collect(),
                Kind::All | Kind::Files(_) => vec![],
            };
            if kind.is_empty() { typed } else { Node::And(kind.into_iter().chain([typed]).collect()) }
        });
        Query { root, fold, count: parser.count, regexes: parser.regexes }
    }

    /// Rien à chercher (requête vide).
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// Les tests que tout résultat doit passer (ceux qui ne sont ni sous un « ou » ni sous un « sauf »).
    fn required(&self) -> Vec<&Test> {
        fn walk<'a>(n: &'a Node, out: &mut Vec<&'a Test>) {
            match n {
                Node::Test(t) => out.push(t),
                Node::And(items) => items.iter().for_each(|i| walk(i, out)),
                Node::Not(_) | Node::Or(_) => {}
            }
        }
        let mut out = vec![];
        if let Some(root) = &self.root {
            walk(root, &mut out);
        }
        out
    }

    /// Le texte à comparer au début des noms pour le classement : le premier mot simple de la requête.
    fn lead(&self) -> Option<&[u8]> {
        self.required().into_iter().find_map(|t| match t {
            Test::Contains(t) | Test::Starts(t) => Some(t.as_slice()),
            _ => None,
        })
    }
}

// ───────────────────────── évaluation ─────────────────────────

/// `*` : n'importe quelle suite ; `?` : un caractère (UTF-8 compris).
fn glob(pattern: &[u8], name: &[u8]) -> bool {
    let (mut p, mut n) = (0, 0);
    let (mut star, mut mark) = (usize::MAX, 0);
    while n < name.len() {
        if p < pattern.len() && pattern[p] == b'?' {
            p += 1;
            n += 1;
            while n < name.len() && name[n] & 0xC0 == 0x80 {
                n += 1;
            }
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = p;
            mark = n;
            p += 1;
        } else if p < pattern.len() && pattern[p] == name[n] {
            p += 1;
            n += 1;
        } else if star != usize::MAX {
            p = star + 1;
            mark += 1;
            n = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&b| b == b'*')
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b >= 0x80
}

fn has_word(name: &[u8], word: &[u8]) -> bool {
    memmem::find_iter(name, word).any(|at| {
        let before = at == 0 || !is_word_byte(name[at - 1]);
        let after = at + word.len() >= name.len() || !is_word_byte(name[at + word.len()]);
        before && after
    })
}

fn ext_of(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|&b| b == b'.') {
        Some(i) if i > 0 => &name[i + 1..],
        _ => &[],
    }
}

/// Le nom sans son extension.
fn stem_of(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|&b| b == b'.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

/// Recherche d'un texte ASCII (déjà en minuscules) sans tenir compte de la casse : on cherche d'abord sa lettre
/// la plus rare, sous ses deux formes, puis on compare autour.
struct CiFinder<'a> {
    needle: &'a [u8],
    at: usize,
    lo: u8,
    up: u8,
}

impl<'a> CiFinder<'a> {
    fn new(needle: &'a [u8]) -> Self {
        // du plus courant au plus rare dans des noms de fichiers ; ce qui n'y figure pas est plus rare encore
        const COMMON: &[u8] = b"e.aitonrs_-lc0d1mup2hg3bf456789ywkvxzjq ";
        let rarity = |b: u8| COMMON.iter().position(|&c| c == b).unwrap_or(usize::MAX);
        let at = (0..needle.len()).max_by_key(|&k| rarity(needle[k])).unwrap_or(0);
        let lo = needle.get(at).copied().unwrap_or(0);
        CiFinder { needle, at, lo, up: lo.to_ascii_uppercase() }
    }

    fn find(&self, hay: &[u8]) -> Option<usize> {
        let n = self.needle.len();
        let mut pos = self.at;
        while pos < hay.len() {
            let hit = if self.lo != self.up { memchr2(self.lo, self.up, &hay[pos..]) } else { memchr(self.lo, &hay[pos..]) }?;
            let start = pos + hit - self.at;
            if start + n <= hay.len() && hay[start..start + n].eq_ignore_ascii_case(self.needle) {
                return Some(start);
            }
            pos += hit + 1;
        }
        None
    }
}

/// Ce dont un fil a besoin pour tester des éléments, et qu'il ne partage pas : la place où transformer un nom,
/// et la mémoire de travail de chaque expression régulière. Chacune tient sur ses propres lignes de cache : deux
/// fils qui écriraient sur la même ligne se ralentiraient l'un l'autre plus de dix fois.
struct Scratch {
    buf: Vec<u8>,
    caches: Vec<Apart<meta::Cache>>,
}

#[repr(align(128))]
struct Apart<T>(T);

/// Tout ce qu'il faut pour tester un élément, en plus de la requête préparée.
struct Eval<'a> {
    ix: &'a Index,
    regexes: &'a [meta::Regex],
    fold: Fold,
    /// Un bit par élément pour chaque test qui se calcule d'un coup sur tout l'index.
    bits: Vec<Vec<bool>>,
    /// Dans le dossier de départ (None : partout).
    scope: Option<Vec<bool>>,
    /// Un des tests lit-il le nom ? (sinon on ne le transforme pas pour rien)
    names: bool,
}

impl Eval<'_> {
    fn scratch(&self) -> Scratch {
        Scratch { buf: Vec::new(), caches: self.regexes.iter().map(|re| Apart(re.create_cache())).collect() }
    }

    fn test(&self, t: &Test, i: u32, name: &[u8], caches: &mut [Apart<meta::Cache>]) -> bool {
        let ix = self.ix;
        match t {
            Test::Contains(n) => memmem::find(name, n).is_some(),
            Test::Glob(p) => glob(p, name),
            Test::Starts(n) => name.starts_with(n),
            Test::Ends(n) => name.ends_with(n),
            Test::Word(w) => has_word(name, w),
            Test::Whole(n) => name == n.as_slice(),
            Test::Regex(k) => self.regexes[*k].search_half_with(&mut caches[*k].0, &Input::new(ix.name(i)).earliest(true)).is_some(),
            Test::Ext(list) => !ix.is_dir(i) && list.iter().any(|e| ext_of(name).eq_ignore_ascii_case(e)),
            Test::Size(a, b) => !ix.is_dir(i) && (*a..=*b).contains(&ix.size_of(i)),
            Test::Modified(a, b) => (*a..=*b).contains(&ix.mtime_of(i)),
            Test::Dir(d) => ix.is_dir(i) == *d,
            Test::Attr(bits) => ix.flags[i as usize] & bits == *bits,
            Test::Len(a, b) => (*a..=*b).contains(&(ix.name(i).chars().count() as u64)),
            Test::Root => {
                let p = ix.parent[i as usize];
                p != NO_PARENT && ix.parent[p as usize] == NO_PARENT
            }
            Test::Bits(k) => self.bits[*k][i as usize],
            // remplacés par `Bits` avant la recherche
            Test::Path(_) | Test::Parent(_) | Test::Depth(..) | Test::Children(..) | Test::Child(_) | Test::Dupe(_) => false,
            Test::Nothing => false,
        }
    }

    fn node(&self, n: &Node, i: u32, name: &[u8], caches: &mut [Apart<meta::Cache>]) -> bool {
        match n {
            Node::Test(t) => self.test(t, i, name, caches),
            Node::Not(inner) => !self.node(inner, i, name, caches),
            Node::And(items) => items.iter().all(|x| self.node(x, i, name, caches)),
            Node::Or(items) => items.iter().any(|x| self.node(x, i, name, caches)),
        }
    }

    fn matches(&self, root: &Node, i: u32, s: &mut Scratch) -> bool {
        if self.ix.flags[i as usize] & F_DELETED != 0 || self.scope.as_ref().is_some_and(|s| !s[i as usize]) {
            return false;
        }
        let name: &[u8] = if self.names { self.fold.name(self.ix, i, &mut s.buf) } else { &[] };
        self.node(root, i, name, &mut s.caches)
    }
}

/// Compare les noms de l'index à des morceaux de requête (déjà sous la forme où ils se comparent), sans copier
/// le nom tant qu'il est en ASCII pur : c'est le cas de presque tous les noms.
struct Names<'a> {
    ix: &'a Index,
    fold: Fold,
    buf: Vec<u8>,
}

impl Names<'_> {
    fn starts(&mut self, i: u32, t: &[u8]) -> bool {
        let name = self.ix.name(i).as_bytes();
        match self.fold {
            Fold::Exact => name.starts_with(t),
            _ if name.is_ascii() => name.len() >= t.len() && name[..t.len()].eq_ignore_ascii_case(t),
            fold => fold.name(self.ix, i, &mut self.buf).starts_with(t),
        }
    }

    fn ends(&mut self, i: u32, t: &[u8]) -> bool {
        let name = self.ix.name(i).as_bytes();
        match self.fold {
            Fold::Exact => name.ends_with(t),
            _ if name.is_ascii() => name.len() >= t.len() && name[name.len() - t.len()..].eq_ignore_ascii_case(t),
            fold => fold.name(self.ix, i, &mut self.buf).ends_with(t),
        }
    }

    fn is(&mut self, i: u32, t: &[u8]) -> bool {
        let name = self.ix.name(i).as_bytes();
        match self.fold {
            Fold::Exact => name == t,
            _ if name.is_ascii() => name.eq_ignore_ascii_case(t),
            fold => fold.name(self.ix, i, &mut self.buf) == t,
        }
    }

    fn contains(&mut self, i: u32, t: &[u8], loose: &CiFinder) -> bool {
        let name = self.ix.name(i).as_bytes();
        match self.fold {
            Fold::Exact => memmem::find(name, t).is_some(),
            // un texte hors ASCII ne peut pas figurer dans un nom en ASCII pur
            _ if name.is_ascii() => t.is_ascii() && loose.find(name).is_some(),
            fold => memmem::find(fold.name(self.ix, i, &mut self.buf), t).is_some(),
        }
    }
}

/// Un bit par élément : son chemin complet contient-il la suite de morceaux `parts` (`a\b\c`) ?
/// Un élément correspond si la suite se termine dans son nom, ou si l'un de ses dossiers parents correspond.
fn path_bits(ix: &Index, parts: &[Vec<u8>], fold: Fold) -> Vec<bool> {
    let mut names = Names { ix, fold, buf: Vec::new() };
    let loose = CiFinder::new(&parts[0]);
    let n = ix.len();
    let mut bits = vec![false; n];
    let last = parts.len() - 1;
    for i in 0..n as u32 {
        let p = ix.parent[i as usize];
        if p != NO_PARENT && bits[p as usize] {
            bits[i as usize] = true;
            continue;
        }
        // le dernier morceau commence le nom (ou y figure, s'il est seul) ; les précédents sont les dossiers parents
        let tail_ok = if last == 0 { names.contains(i, &parts[0], &loose) } else { names.starts(i, &parts[last]) };
        if !tail_ok {
            continue;
        }
        let mut cur = p;
        let mut ok = true;
        for k in (0..last).rev() {
            if cur == NO_PARENT {
                // au-delà de la racine : seul un morceau vide (« \dossier ») peut encore correspondre
                ok = parts[..=k].iter().all(Vec::is_empty);
                break;
            }
            // le premier morceau peut n'être que la fin d'un nom de dossier
            ok = if k == 0 { names.ends(cur, &parts[0]) } else { names.is(cur, &parts[k]) };
            if !ok {
                break;
            }
            cur = ix.parent[cur as usize];
        }
        bits[i as usize] = ok;
    }
    bits
}

/// Un bit par élément : est-il dans le dossier `root` (ou un de ses sous-dossiers) ?
pub fn subtree(ix: &Index, root: u32) -> Vec<bool> {
    let mut bits = vec![false; ix.len()];
    for i in root as usize + 1..ix.len() {
        let p = ix.parent[i];
        bits[i] = p == root || (p != NO_PARENT && p > root && bits[p as usize]);
    }
    bits
}

/// La place de chaque dossier dans l'arbre parcouru en profondeur, les dossiers d'un même niveau dans l'ordre
/// alphabétique : trier les résultats par la place de leur dossier, c'est les trier par chemin.
fn dir_places(ix: &Index) -> Vec<u32> {
    let n = ix.len();
    let rank = |i: u32| ix.rank.get(i as usize).copied().unwrap_or(i);
    // les dossiers rangés par parent (les racines d'abord), puis par nom : les enfants d'un dossier se suivent
    let mut dirs: Vec<u32> = (0..n as u32).filter(|&i| ix.is_dir(i) && live(ix, i as usize)).collect();
    dirs.sort_unstable_by_key(|&i| (ix.parent[i as usize].wrapping_add(1), rank(i)));
    // pour chaque dossier, où commencent ses sous-dossiers dans cette liste, et combien il y en a
    let (mut start, mut count) = (vec![0u32; n], vec![0u32; n]);
    let roots = dirs.iter().take_while(|&&d| ix.parent[d as usize] == NO_PARENT).count();
    for (k, &d) in dirs.iter().enumerate().skip(roots) {
        let p = ix.parent[d as usize] as usize;
        if count[p] == 0 {
            start[p] = k as u32;
        }
        count[p] += 1;
    }
    let mut place = vec![0u32; n];
    let mut next = 0;
    // parcours en profondeur : le dernier empilé sort le premier, donc on empile à l'envers
    let mut stack: Vec<u32> = dirs[..roots].iter().rev().copied().collect();
    while let Some(d) = stack.pop() {
        place[d as usize] = next;
        next += 1;
        let (s, c) = (start[d as usize] as usize, count[d as usize] as usize);
        stack.extend(dirs[s..s + c].iter().rev());
    }
    place
}

fn live(ix: &Index, i: usize) -> bool {
    ix.flags[i] & F_DELETED == 0
}

/// Empreinte d'un texte (FNV-1a).
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// Un bit par élément : sa clé est-elle portée par un autre élément aussi ? `keys` : (clé, numéro).
fn repeated(n: usize, mut keys: Vec<(u64, u32)>) -> Vec<bool> {
    keys.sort_unstable();
    let mut bits = vec![false; n];
    for run in keys.chunk_by(|a, b| a.0 == b.0).filter(|run| run.len() > 1) {
        run.iter().for_each(|&(_, i)| bits[i as usize] = true);
    }
    bits
}

/// Calcule d'un coup, sur tout l'index, un test qui ne peut pas se décider élément par élément.
fn whole_index(ix: &Index, t: &Test, fold: Fold) -> Option<Vec<bool>> {
    let n = ix.len();
    Some(match t {
        Test::Path(parts) => path_bits(ix, parts, fold),
        Test::Parent(dir) => {
            let parent = ix.find(Path::new(dir));
            (0..n).map(|i| parent.is_some() && Some(ix.parent[i]) == parent).collect()
        }
        Test::Depth(a, b) => {
            // un enfant suit toujours son dossier parent dans l'index
            let mut depth = vec![0u16; n];
            (0..n)
                .map(|i| {
                    let p = ix.parent[i];
                    if p != NO_PARENT {
                        depth[i] = depth[p as usize].saturating_add(1);
                    }
                    (*a..=*b).contains(&(depth[i] as u64))
                })
                .collect()
        }
        Test::Children(kind, a, b) => {
            let mut count = vec![0u32; n];
            for i in 0..n {
                let p = ix.parent[i];
                if p != NO_PARENT && live(ix, i) && kind.is_none_or(|dirs| (ix.flags[i] & F_DIR != 0) == dirs) {
                    count[p as usize] += 1;
                }
            }
            // un lien, un dossier qu'on n'a pas le droit de lire : on ne sait pas ce qu'il contient
            (0..n).map(|i| ix.flags[i] & (F_DIR | F_LINK | F_DENIED) == F_DIR && (*a..=*b).contains(&(count[i] as u64))).collect()
        }
        Test::Child(text) => {
            let mut names = Names { ix, fold, buf: Vec::new() };
            let loose = CiFinder::new(text);
            let mut bits = vec![false; n];
            for i in 0..n as u32 {
                let p = ix.parent[i as usize];
                if p != NO_PARENT && live(ix, i as usize) && !bits[p as usize] && names.contains(i, text, &loose) {
                    bits[p as usize] = true;
                }
            }
            bits
        }
        Test::Dupe(same) => {
            let mut buf = Vec::new();
            let keys = (0..n as u32)
                .filter(|&i| live(ix, i as usize) && ix.parent[i as usize] != NO_PARENT)
                .filter_map(|i| {
                    let file = !ix.is_dir(i);
                    let key = match same {
                        Same::Name => hash(ix.folded(i, &mut buf)),
                        Same::Stem => {
                            let name = ix.folded(i, &mut buf);
                            hash(if file { stem_of(name) } else { name })
                        }
                        // les fichiers vides se ressemblent tous : ils ne comptent pas
                        Same::Size => Some(ix.size_of(i)).filter(|&s| file && s > 0)?,
                        Same::Modified => Some(ix.mtime_of(i)).filter(|&m| m > 0)?,
                    };
                    Some((key, i))
                })
                .collect();
            repeated(n, keys)
        }
        _ => return None,
    })
}

/// Copie l'arbre de la requête en remplaçant par `Bits` les tests qui se calculent sur tout l'index.
fn prepare(ix: &Index, n: &Node, fold: Fold, bits: &mut Vec<Vec<bool>>) -> Node {
    match n {
        Node::Test(t) => match whole_index(ix, t, fold) {
            Some(b) => {
                bits.push(b);
                Node::Test(Test::Bits(bits.len() - 1))
            }
            None => Node::Test(t.clone()),
        },
        Node::Not(inner) => Node::Not(Box::new(prepare(ix, inner, fold, bits))),
        Node::And(items) => Node::And(items.iter().map(|x| prepare(ix, x, fold, bits)).collect()),
        Node::Or(items) => Node::Or(items.iter().map(|x| prepare(ix, x, fold, bits)).collect()),
    }
}

fn reads_name(n: &Node) -> bool {
    match n {
        Node::Test(t) => matches!(t, Test::Contains(_) | Test::Glob(_) | Test::Starts(_) | Test::Ends(_) | Test::Word(_) | Test::Whole(_) | Test::Ext(_)),
        Node::Not(inner) => reads_name(inner),
        Node::And(items) | Node::Or(items) => items.iter().any(reads_name),
    }
}

/// Nombre de noms regardés d'un bloc quand on cherche ceux qui ne sont pas en ASCII pur.
const BLOCK: usize = 256;

impl Index {
    /// Cherche dans l'index. `scope` : numéro du dossier où chercher (None : partout). Seuls les `limit` premiers
    /// résultats dans l'ordre demandé sont renvoyés, avec le nombre total. La recherche se répartit sur `threads`
    /// fils ; `cancel` l'interrompt (le résultat est alors partiel).
    #[allow(clippy::too_many_arguments)]
    pub fn search(&self, q: &Query, scope: Option<u32>, order: Order, descending: bool, limit: usize, threads: usize, cancel: &AtomicBool) -> Found {
        let Some(root) = &q.root else { return Found::default() };
        if self.is_empty() {
            return Found::default();
        }
        let limit = q.count.map_or(limit, |c| c.min(limit));
        let mut bits = vec![];
        let root = prepare(self, root, q.fold, &mut bits);
        let eval = Eval { ix: self, regexes: &q.regexes, fold: q.fold, names: reads_name(&root), bits, scope: scope.map(|r| subtree(self, r)) };
        // Le mot le plus long que tout résultat doit contenir guide la recherche : on le cherche d'un trait dans
        // toute la zone des noms, et seuls les éléments où il apparaît passent les autres tests.
        let driver = q
            .required()
            .into_iter()
            .filter_map(|t| match t {
                Test::Contains(t) | Test::Starts(t) | Test::Ends(t) | Test::Word(t) | Test::Whole(t) => Some(t.as_slice()),
                _ => None,
            })
            .max_by_key(|t| t.len());
        // Un mot hors ASCII ne se cherche pas d'un trait (sa casse ne se compare pas octet par octet), et un mot
        // sans accent doit aussi trouver les noms accentués : ces noms-là, peu nombreux, se testent un par un.
        let sweep = driver.filter(|d| q.fold == Fold::Exact || d.is_ascii());
        let wide = match q.fold {
            Fold::Exact => false,
            Fold::Marks => driver.is_some() && sweep.is_none(),
            Fold::Plain => driver.is_some(),
        };
        let arena = &self.names;
        let n = self.len();
        let threads = threads.clamp(1, 64);
        let chunk = n.div_ceil(threads).max(1);
        let lead = q.lead();
        let starts = |i: u32, prefix: &[u8], buf: &mut Vec<u8>| -> bool { q.fold.name(self, i, buf).starts_with(prefix) };
        let rank = |i: u32| self.rank.get(i as usize).copied().unwrap_or(i);
        let places = if order == Order::Path { dir_places(self) } else { vec![] };
        // Clé de tri d'un résultat : l'essentiel dans les 64 bits du haut, puis la place alphabétique (elle
        // départage), puis le numéro de l'élément (pour le retrouver sans le garder à côté).
        let key = |i: u32, starts: bool| -> u128 {
            let main = match order {
                Order::Name => (lead.is_some() && !starts) as u64,
                Order::Size => self.size_of(i),
                Order::Modified => self.mtime_of(i),
                Order::Path => match self.parent[i as usize] {
                    NO_PARENT => 0,
                    p => places[p as usize] as u64 + 1,
                },
            };
            ((main as u128) << 64) | ((rank(i) as u128) << 32) | i as u128
        };
        let mut parts: Vec<Vec<u128>> = vec![];
        std::thread::scope(|s| {
            let handles: Vec<_> = (0..n)
                .step_by(chunk)
                .map(|a| {
                    let b = (a + chunk).min(n);
                    let (eval, root) = (&eval, &root);
                    s.spawn(move || {
                        let (mut out, mut scratch) = (Vec::new(), eval.scratch());
                        let keep = |i: u32, at_start: bool, s: &mut Scratch, out: &mut Vec<u128>| {
                            if eval.matches(root, i, s) {
                                out.push(key(i, at_start || lead.is_some_and(|l| starts(i, l, &mut s.buf))));
                            }
                        };
                        if let Some(needle) = sweep {
                            let (exact, loose) = (memmem::Finder::new(needle), CiFinder::new(needle));
                            let find = |hay: &[u8]| if q.fold == Fold::Exact { exact.find(hay) } else { loose.find(hay) };
                            let leads = lead.is_some_and(|l| std::ptr::eq(l, needle));
                            let (start, end) = (self.off[a] as usize, self.off[b] as usize);
                            let (mut pos, mut i, mut seen) = (start, a, 0u32);
                            while let Some(at) = find(&arena[pos..end]) {
                                seen += 1;
                                if seen % 4096 == 0 && cancel.load(Ordering::Relaxed) {
                                    break;
                                }
                                let at = pos + at;
                                // l'élément dont le nom contient cette position : on avance depuis le précédent,
                                // à pas doublés puis par dichotomie (les trouvailles se suivent souvent de près)
                                let mut step = 1;
                                while i + step < b && self.off[i + step] as usize <= at {
                                    step *= 2;
                                }
                                let hi = (i + step).min(b);
                                i += self.off[i..=hi].partition_point(|&o| o as usize <= at) - 1;
                                // les noms hors ASCII, quand ils se testent à part, ne sont pas comptés deux fois
                                if !wide || self.name(i as u32).is_ascii() {
                                    keep(i as u32, leads && at == self.off[i] as usize, &mut scratch, &mut out);
                                }
                                pos = self.off[i + 1] as usize;
                                if pos >= end {
                                    break;
                                }
                            }
                        }
                        if driver.is_none() || wide {
                            // sans mot pour guider : tous les noms ; sinon seulement ceux qui ne sont pas en ASCII pur,
                            // et un bloc de noms tout en ASCII se saute d'un coup
                            let only_wide = driver.is_some();
                            for block in (a..b).step_by(BLOCK) {
                                if cancel.load(Ordering::Relaxed) {
                                    break;
                                }
                                let stop = (block + BLOCK).min(b);
                                if only_wide && arena[self.off[block] as usize..self.off[stop] as usize].is_ascii() {
                                    continue;
                                }
                                for i in block..stop {
                                    if !only_wide || !self.name(i as u32).is_ascii() {
                                        keep(i as u32, false, &mut scratch, &mut out);
                                    }
                                }
                            }
                        }
                        out
                    })
                })
                .collect();
            parts = handles.into_iter().map(|h| h.join().unwrap_or_default()).collect();
        });
        let mut keys: Vec<u128> = parts.concat();
        let total = q.count.map_or(keys.len(), |c| c.min(keys.len()));
        let cmp = |a: &u128, b: &u128| if descending { b.cmp(a) } else { a.cmp(b) };
        // seuls les premiers résultats sont rangés : choisir les `limit` plus petits coûte bien moins qu'un tri complet
        if limit < keys.len() {
            if limit > 0 {
                keys.select_nth_unstable_by(limit, cmp);
            }
            keys.truncate(limit);
        }
        keys.sort_unstable_by(cmp);
        Found { ids: keys.iter().map(|k| (*k & 0xffff_ffff) as u32).collect(), total }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-10 12:00 UTC, heure locale UTC−5.
    const CLOCK: Clock = Clock { now_utc: 134_387_856_000_000_000, bias: -5 * 36_000_000_000 };
    const HOUR: u64 = 36_000_000_000;

    fn sample() -> Index {
        let mut ix = Index::default();
        let now = CLOCK.now_utc;
        let c = ix.push("C:", NO_PARENT, 0, 0, F_DIR);
        let users = ix.push("Users", c, 0, now - 400 * DAY, F_DIR);
        let sam = ix.push("samue", users, 0, now - 300 * DAY, F_DIR);
        let docs = ix.push("Documents", sam, 0, now - 30 * DAY, F_DIR);
        let proj = ix.push("Projets Plume", docs, 0, now - 2 * DAY, F_DIR);
        let src = ix.push("src", proj, 0, now - DAY, F_DIR);
        ix.push("main.rs", src, 4_000, now - 2 * HOUR, 0);
        ix.push("Main_Window.rs", src, 60_000, now - 30 * HOUR, 0);
        ix.push("README.md", proj, 1_200, now - 10 * DAY, 0);
        ix.push("Rapport final 2026.docx", docs, 2 * 1024 * 1024, now - 3 * DAY, 0);
        ix.push("rapport brouillon.docx", docs, 300_000, now - 40 * DAY, 0);
        ix.push("Été à Montréal.JPG", docs, 5 * 1024 * 1024, now - 100 * DAY, 0);
        ix.push("photo01.png", docs, 900_000, now - 100 * DAY, 0);
        ix.push("photo02.png", docs, 0, now - 100 * DAY, 0);
        let win = ix.push("Windows", c, 0, now - 500 * DAY, F_DIR);
        ix.push("main.cpl", win, 77_000, now - 500 * DAY, 0);
        ix.push("deleted main.txt", win, 1, now, F_DELETED);
        ix.compute_rank(2);
        ix
    }

    fn docs(ix: &Index) -> u32 {
        ix.find(Path::new(r"C:\Users\samue\Documents")).unwrap()
    }

    fn find(ix: &Index, q: &str) -> Vec<String> {
        find_in(ix, q, None)
    }

    fn find_in(ix: &Index, q: &str, scope: Option<u32>) -> Vec<String> {
        let query = Query::parse(q, &CLOCK);
        let cancel = AtomicBool::new(false);
        // un fil ou plusieurs : même résultat
        let one = ix.search(&query, scope, Order::Name, false, usize::MAX, 1, &cancel);
        let many = ix.search(&query, scope, Order::Name, false, usize::MAX, 5, &cancel);
        assert_eq!(one.ids, many.ids, "{q}");
        assert_eq!(one.total, one.ids.len());
        // limité aux deux premiers : les mêmes, et le total reste entier
        let two = ix.search(&query, scope, Order::Name, false, 2, 3, &cancel);
        assert_eq!(two.ids, one.ids[..one.ids.len().min(2)], "{q}");
        assert_eq!(two.total, one.total);
        one.ids.iter().map(|&i| ix.name(i).to_string()).collect()
    }

    const NONE: [&str; 0] = [];

    #[test]
    fn words_phrases_and_case() {
        let ix = sample();
        // les noms qui commencent par le mot d'abord, puis l'ordre alphabétique ; l'élément supprimé est ignoré
        assert_eq!(find(&ix, "main"), ["main.cpl", "main.rs", "Main_Window.rs"]);
        assert_eq!(find(&ix, "MAIN rs"), ["main.rs", "Main_Window.rs"]);
        assert_eq!(find(&ix, "case:Main"), ["Main_Window.rs"]);
        assert_eq!(find(&ix, "Main case:"), ["Main_Window.rs"]);
        assert_eq!(find(&ix, "rapport"), ["rapport brouillon.docx", "Rapport final 2026.docx"]);
        assert_eq!(find(&ix, "\"rapport final\""), ["Rapport final 2026.docx"]);
        assert_eq!(find(&ix, "rapport !brouillon"), ["Rapport final 2026.docx"]);
        assert_eq!(find(&ix, "rapport ! brouillon"), ["Rapport final 2026.docx"]);
        assert_eq!(find(&ix, "été"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "MONTRÉAL"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "zzz"), NONE);
        assert_eq!(find(&ix, "   "), NONE);
    }

    #[test]
    fn accents() {
        let mut ix = sample();
        let docs = docs(&ix);
        // un « é » écrit en deux caractères (lettre puis accent), comme dans les noms venus d'un Mac
        ix.push("Cafe\u{301} crème.TXT", docs, 5, CLOCK.now_utc, 0);
        ix.push("ŒUVRE.txt", docs, 5, CLOCK.now_utc, 0);
        ix.compute_rank(2);
        // sans accents dans la requête : les noms accentués sont trouvés aussi
        assert_eq!(find(&ix, "ete"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "montreal jpg"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "cafe creme"), ["Cafe\u{301} crème.TXT"]);
        assert_eq!(find(&ix, "café CRÈME"), ["Cafe\u{301} crème.TXT"]);
        assert_eq!(find(&ix, "oeuvre"), ["ŒUVRE.txt"]);
        assert_eq!(find(&ix, "œuvre"), ["ŒUVRE.txt"]);
        // toutes les façons de comparer passent par la même forme
        assert_eq!(find(&ix, "startwith:ete"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "endwith:real.jpg"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "ww:ete"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "wfn:\"ete a montreal.jpg\""), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "*montreal.j?g"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "documents\\ete"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "folder:child:montreal"), ["Documents"]);
        // « diacritics: » : les accents comptent ; « case: » : tout compte
        assert_eq!(find(&ix, "diacritics:ete"), NONE);
        assert_eq!(find(&ix, "diacritics:été"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "diacritics:MONTRÉAL"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "case:Été"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "case:été"), NONE);
        assert_eq!(find(&ix, "case:Ete"), NONE);
        assert_eq!(plain("Ça, c'est l'ÉTÉ à Zürich : Œuvre straße"), "ca, c'est l'ete a zurich : oeuvre strasse".as_bytes());
        assert_eq!(plain("Привет 東京"), "привет 東京".as_bytes());
    }

    #[test]
    fn or_wildcards_and_filters() {
        let ix = sample();
        assert_eq!(find(&ix, "readme|brouillon"), ["rapport brouillon.docx", "README.md"]);
        assert_eq!(find(&ix, "readme | brouillon"), ["rapport brouillon.docx", "README.md"]);
        assert_eq!(find(&ix, "*.rs"), ["main.rs", "Main_Window.rs"]);
        assert_eq!(find(&ix, "photo??.png"), ["photo01.png", "photo02.png"]);
        assert_eq!(find(&ix, "?t?*.jpg"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, "ext:png;md"), ["photo01.png", "photo02.png", "README.md"]);
        assert_eq!(find(&ix, "ext:.DOCX rapport"), ["rapport brouillon.docx", "Rapport final 2026.docx"]);
        assert_eq!(find(&ix, "folder: s"), ["samue", "src", "Documents", "Projets Plume", "Users", "Windows"]);
        assert_eq!(find(&ix, "folder:plume"), ["Projets Plume"]);
        assert_eq!(find(&ix, "file: main"), ["main.cpl", "main.rs", "Main_Window.rs"]);
        assert_eq!(find(&ix, "startwith:photo"), ["photo01.png", "photo02.png"]);
        assert_eq!(find(&ix, "endwith:.md"), ["README.md"]);
        assert_eq!(find(&ix, "ww:main"), ["main.cpl", "main.rs", "Main_Window.rs"]);
        assert_eq!(find(&ix, "ww:window"), ["Main_Window.rs"]);
        assert_eq!(find(&ix, "ww:wind"), NONE);
        assert_eq!(find(&ix, "wfn:src"), ["src"]);
        assert_eq!(find(&ix, "wfn:README.MD"), ["README.md"]);
        assert_eq!(find(&ix, "wfn:readme"), NONE);
        // « count: » : pas plus de résultats que ça
        assert_eq!(find(&ix, "main count:2"), ["main.cpl", "main.rs"]);
        assert_eq!(find(&ix, "count:1 ext:png"), ["photo01.png"]);
    }

    #[test]
    fn groups_and_precedence() {
        let ix = sample();
        // le « ou » lie plus fort que l'espace : main ET (rs OU cpl)
        assert_eq!(find(&ix, "main rs|cpl"), ["main.cpl", "main.rs", "Main_Window.rs"]);
        assert_eq!(find(&ix, "main .rs|.cpl !window"), ["main.cpl", "main.rs"]);
        // les chevrons groupent : (photo ET 01) OU readme
        assert_eq!(find(&ix, "<photo 01>|readme"), ["photo01.png", "README.md"]);
        assert_eq!(find(&ix, "< photo 01 > | readme"), ["photo01.png", "README.md"]);
        assert_eq!(find(&ix, "!<photo|rapport|main> file:"), ["Été à Montréal.JPG", "README.md"]);
        assert_eq!(find(&ix, "<ext:png size:>1kb>|<ext:md>"), ["photo01.png", "README.md"]);
        // un chevron fermant de trop, un groupe jamais fermé, des barres en trop : pas d'erreur
        assert_eq!(find(&ix, "readme>"), NONE);
        assert_eq!(find(&ix, "<readme"), ["README.md"]);
        assert_eq!(find(&ix, "| readme |"), ["README.md"]);
        assert_eq!(find(&ix, "!"), NONE);
        assert_eq!(find(&ix, "<>"), NONE);
    }

    #[test]
    fn interface_options() {
        let ix = sample();
        let with = |q: &str, o: Options| -> Vec<String> {
            let query = Query::parse_with(q, &CLOCK, &o);
            ix.search(&query, None, Order::Name, false, usize::MAX, 3, &AtomicBool::new(false)).ids.iter().map(|&i| ix.name(i).to_string()).collect()
        };
        let none = Options::default();
        assert_eq!(with("main", none), ["main.cpl", "main.rs", "Main_Window.rs"]);
        assert_eq!(with("Main", Options { case: true, ..none }), ["Main_Window.rs"]);
        assert_eq!(with("main", Options { word: true, ..none }), ["main.cpl", "main.rs", "Main_Window.rs"]);
        assert_eq!(with("wind", Options { word: true, ..none }), NONE);
        assert_eq!(with("window", Options { word: true, ..none }), ["Main_Window.rs"]);
        // un filtre reste un filtre, des jokers restent des jokers
        assert_eq!(with("ext:md", Options { word: true, ..none }), ["README.md"]);
        assert_eq!(with("*.md", Options { word: true, ..none }), ["README.md"]);
        // dans le chemin : « windows » trouve aussi ce que le dossier Windows contient
        assert_eq!(with("windows", none), ["Windows"]);
        assert_eq!(with("windows", Options { path: true, ..none }), ["main.cpl", "Windows"]);
        // tout le texte est une expression régulière, espaces et barres compris
        assert_eq!(with(r"^(photo|main)\d*\.(png|rs)$", Options { regex: true, ..none }), ["main.rs", "photo01.png", "photo02.png"]);
        assert_eq!(with("rapport (final|brouillon)", Options { regex: true, ..none }), ["rapport brouillon.docx", "Rapport final 2026.docx"]);
        assert_eq!(with("^Main", Options { regex: true, case: true, ..none }), ["Main_Window.rs"]);
        // un type choisi s'ajoute au texte ; sans texte, il ne cherche rien
        assert_eq!(with("o", Options { kind: Kind::Folders, ..none }), ["Documents", "Projets Plume", "Windows"]);
        assert_eq!(with("o", Options { kind: Kind::Files(4), ..none }), ["Été à Montréal.JPG", "photo01.png", "photo02.png"]);
        assert_eq!(with("", Options { kind: Kind::Folders, ..none }), NONE);
        assert_eq!(KINDS[4].0, "pic");
    }

    #[test]
    fn regular_expressions() {
        let ix = sample();
        assert_eq!(find(&ix, r"regex:^photo\d+\.png$"), ["photo01.png", "photo02.png"]);
        assert_eq!(find(&ix, r"regex:^main\.(rs|cpl)$"), ["main.cpl", "main.rs"]);
        assert_eq!(find(&ix, r"regex:^MAIN"), ["main.cpl", "main.rs", "Main_Window.rs"]);
        assert_eq!(find(&ix, r"case:regex:^Main"), ["Main_Window.rs"]);
        assert_eq!(find(&ix, r#"regex:"rapport (final|brouillon)""#), ["rapport brouillon.docx", "Rapport final 2026.docx"]);
        assert_eq!(find(&ix, r"file:regex:\d{4}"), ["Rapport final 2026.docx"]);
        assert_eq!(find(&ix, r"regex:été"), ["Été à Montréal.JPG"]);
        assert_eq!(find(&ix, r"<regex:^photo ext:png>|regex:^read"), ["photo01.png", "photo02.png", "README.md"]);
        // une expression mal formée ne trouve rien, sans erreur
        assert_eq!(find(&ix, r"regex:(unclosed"), NONE);
    }

    #[test]
    fn kinds_attributes_and_shapes() {
        let mut ix = sample();
        let docs = docs(&ix);
        ix.push("chanson.MP3", docs, 3_000_000, CLOCK.now_utc, 0);
        ix.push("film.mkv", docs, 9_000_000_000, CLOCK.now_utc, F_READONLY);
        ix.push("secret.txt", docs, 10, CLOCK.now_utc, F_HIDDEN | F_SYSTEM);
        ix.push("Vide", docs, 0, CLOCK.now_utc, F_DIR);
        ix.push("Interdit", docs, 0, CLOCK.now_utc, F_DIR | F_DENIED);
        ix.compute_rank(2);
        assert_eq!(find(&ix, "audio:"), ["chanson.MP3"]);
        assert_eq!(find(&ix, "video:"), ["film.mkv"]);
        assert_eq!(find(&ix, "pic:"), ["Été à Montréal.JPG", "photo01.png", "photo02.png"]);
        assert_eq!(find(&ix, "pic:photo"), ["photo01.png", "photo02.png"]);
        assert_eq!(find(&ix, "doc: rapport"), ["rapport brouillon.docx", "Rapport final 2026.docx"]);
        assert_eq!(find(&ix, "exe:|zip:"), NONE);
        assert_eq!(find(&ix, "attrib:h"), ["secret.txt"]);
        assert_eq!(find(&ix, "attrib:hs"), ["secret.txt"]);
        assert_eq!(find(&ix, "attrib:r"), ["film.mkv"]);
        assert_eq!(find(&ix, "attrib:hr"), NONE);
        assert_eq!(find(&ix, "attrib:x"), NONE);
        assert_eq!(find(&ix, "size:>4gb"), ["film.mkv"]);
        assert_eq!(find(&ix, "len:>20"), ["rapport brouillon.docx", "Rapport final 2026.docx"]);
        assert_eq!(find(&ix, "len:3"), ["src"]);
        assert_eq!(find(&ix, "len:2..4"), ["C:", "src", "Vide"]);
        // un dossier qu'on n'a pas pu lire n'est pas « vide » : on ne sait pas
        assert_eq!(find(&ix, "empty:"), ["Vide"]);
        assert_eq!(find(&ix, "childcount:2"), ["C:", "Projets Plume", "src"]);
        assert_eq!(find(&ix, "childcount:>8"), ["Documents"]);
        assert_eq!(find(&ix, "childfoldercount:3"), ["Documents"]);
        assert_eq!(find(&ix, "childfilecount:1"), ["Projets Plume", "Windows"]);
        assert_eq!(find(&ix, "child:main"), ["src", "Windows"]);
        assert_eq!(find(&ix, "child:README"), ["Projets Plume"]);
        assert_eq!(find(&ix, "root:"), ["Users", "Windows"]);
        assert_eq!(find(&ix, "depth:1"), ["Users", "Windows"]);
        assert_eq!(find(&ix, "depth:0"), ["C:"]);
        assert_eq!(find(&ix, "parents:>=5 ext:rs"), ["main.rs", "Main_Window.rs"]);
    }

    #[test]
    fn duplicates() {
        let mut ix = sample();
        let win = ix.find(Path::new(r"C:\Windows")).unwrap();
        ix.push("README.MD", win, 1_200, CLOCK.now_utc, 0);
        ix.push("copie.bin", win, 900_000, CLOCK.now_utc - 500 * DAY, 0);
        ix.push("Main.txt", win, 3, CLOCK.now_utc - 7 * DAY, 0);
        ix.compute_rank(2);
        let sorted = |q: &str| {
            let mut names = find(&ix, q);
            names.sort();
            names
        };
        // même nom à la casse près ; même nom sans l'extension ; même taille (les fichiers vides ne comptent pas)
        assert_eq!(sorted("dupe:"), ["README.MD", "README.md"]);
        assert_eq!(sorted("namepartdupe:"), ["Main.txt", "README.MD", "README.md", "main.cpl", "main.rs"]);
        assert_eq!(sorted("sizedupe:"), ["README.MD", "README.md", "copie.bin", "photo01.png"]);
        assert_eq!(sorted("sizedupe: !dupe:"), ["copie.bin", "photo01.png"]);
        // même date de modification : les trois de 100 jours, et les trois de 500 jours
        assert_eq!(sorted("dmdupe: file:"), ["copie.bin", "main.cpl", "photo01.png", "photo02.png", "Été à Montréal.JPG"]);
    }

    #[test]
    fn sizes_and_dates() {
        let ix = sample();
        assert_eq!(find(&ix, "size:>1mb"), ["Été à Montréal.JPG", "Rapport final 2026.docx"]);
        assert_eq!(find(&ix, "size:empty"), ["photo02.png"]);
        assert_eq!(find(&ix, "size:50kb..100kb"), ["main.cpl", "Main_Window.rs"]);
        assert_eq!(find(&ix, "size:<2kb ext:md"), ["README.md"]);
        assert_eq!(find(&ix, "size:gigantic"), NONE);
        assert_eq!(find(&ix, "size:banana"), NONE);
        // il est 07:00 heure locale le 10 novembre : main.rs (05:00) est d'aujourd'hui, Main_Window.rs d'hier
        assert_eq!(find(&ix, "dm:today file:"), ["main.rs"]);
        assert_eq!(find(&ix, "dm:yesterday file:"), ["Main_Window.rs"]);
        assert_eq!(find(&ix, "dm:thisweek file:"), ["main.rs", "Main_Window.rs"]);
        assert_eq!(find(&ix, "dm:thismonth file:"), ["main.rs", "Main_Window.rs", "Rapport final 2026.docx"]);
        assert_eq!(find(&ix, "dm:2026-10 file:"), ["rapport brouillon.docx", "README.md"]);
        assert_eq!(find(&ix, "dm:>=2026-11-07 file:"), ["main.rs", "Main_Window.rs", "Rapport final 2026.docx"]);
        assert_eq!(find(&ix, "dm:<2026 file:"), ["main.cpl"]);
        // des durées qui finissent maintenant
        assert_eq!(find(&ix, "dm:last3hours file:"), ["main.rs"]);
        assert_eq!(find(&ix, "dm:last2days file:"), ["main.rs", "Main_Window.rs"]);
        assert_eq!(find(&ix, "dm:past2weeks file:"), ["main.rs", "Main_Window.rs", "Rapport final 2026.docx", "README.md"]);
        assert_eq!(find(&ix, "dm:last90minutes"), NONE);
    }

    #[test]
    fn paths_and_scope() {
        let ix = sample();
        assert_eq!(find(&ix, "src\\main"), ["main.rs", "Main_Window.rs"]);
        // « dossier\ » : ce que le dossier contient (son propre chemin ne finit pas par une barre)
        assert_eq!(find(&ix, "plume\\src\\"), ["main.rs", "Main_Window.rs"]);
        assert_eq!(find(&ix, "windows\\"), ["main.cpl"]);
        assert_eq!(find(&ix, "path:windows main"), ["main.cpl"]);
        assert_eq!(find(&ix, "documents\\ ext:png"), ["photo01.png", "photo02.png"]);
        assert_eq!(find(&ix, "c:\\windows\\main"), ["main.cpl"]);
        assert_eq!(find(&ix, "nope\\main"), NONE);
        assert_eq!(find(&ix, "parent:c:\\users\\samue\\documents ext:png"), ["photo01.png", "photo02.png"]);
        assert_eq!(find(&ix, "parent:\"C:\\Users\\samue\\Documents\\Projets Plume\""), ["README.md", "src"]);
        assert_eq!(find(&ix, "parent:c: folder:"), ["Users", "Windows"]);
        assert_eq!(find(&ix, "parent:c:\\nope"), NONE);
        assert_eq!(find_in(&ix, "main", Some(docs(&ix))), ["main.rs", "Main_Window.rs"]);
        let win = ix.find(Path::new(r"c:\windows")).unwrap();
        assert_eq!(find_in(&ix, "main", Some(win)), ["main.cpl"]);
        assert_eq!(find_in(&ix, "rapport", Some(win)), NONE);
    }

    #[test]
    fn orders() {
        let ix = sample();
        let q = Query::parse("ext:docx;png;jpg", &CLOCK);
        let cancel = AtomicBool::new(false);
        let names = |order, desc| -> Vec<String> { ix.search(&q, None, order, desc, 100, 3, &cancel).ids.iter().map(|&i| ix.name(i).to_string()).collect() };
        assert_eq!(names(Order::Size, true)[..2], ["Été à Montréal.JPG", "Rapport final 2026.docx"]);
        assert_eq!(names(Order::Size, false)[0], "photo02.png");
        assert_eq!(names(Order::Modified, true)[0], "Rapport final 2026.docx");
        assert_eq!(names(Order::Name, true)[0], "Rapport final 2026.docx");
        // par dossier : l'ordre de l'arbre (Documents, Projets Plume, src, puis Windows), et le nom dans un dossier
        let q = Query::parse("ext:rs;png;md;cpl", &CLOCK);
        let names = |desc| -> Vec<String> { ix.search(&q, None, Order::Path, desc, 100, 3, &cancel).ids.iter().map(|&i| ix.name(i).to_string()).collect() };
        assert_eq!(names(false), ["photo01.png", "photo02.png", "README.md", "main.rs", "Main_Window.rs", "main.cpl"]);
        assert_eq!(names(true), ["main.cpl", "Main_Window.rs", "main.rs", "README.md", "photo02.png", "photo01.png"]);
    }

    #[test]
    fn calendar() {
        assert_eq!(days_from_civil(1601, 1, 1), 0);
        assert_eq!(days_from_civil(1970, 1, 1), 134774);
        assert_eq!(civil_from_days(0), (1601, 1, 1, 0));
        let d = days_from_civil(2026, 11, 10);
        assert_eq!(civil_from_days(d), (2026, 11, 10, 1)); // un mardi
        assert_eq!(d as u64 * DAY + 12 * HOUR, CLOCK.now_utc);
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29, 3));
        assert_eq!(recent("last2weeks"), Some(14 * 86_400));
        assert_eq!(recent("past30minutes"), Some(1800));
        assert_eq!(recent("lasthour"), Some(3600));
        assert_eq!(recent("lastweek"), None);
        assert_eq!(recent("lastbananas"), None);
    }

    #[test]
    fn globs() {
        assert!(glob(b"*.rs", b"main.rs"));
        assert!(!glob(b"*.rs", b"main.rsx"));
        assert!(glob(b"a*b*c", b"aXXbYYc"));
        assert!(glob(b"*", b""));
        assert!(glob(b"?", "é".as_bytes()));
        assert!(!glob(b"??", "é".as_bytes()));
        assert!(glob(b"ph*o??.png", b"photo01.png"));
    }

    #[test]
    fn token_rules() {
        let word = |s: &str| Tok::Word(s.into());
        assert_eq!(tokens("a b"), [word("a"), word("b")]);
        assert_eq!(tokens("a|b"), [word("a"), Tok::Or, word("b")]);
        assert_eq!(tokens("<a b>|c"), [Tok::Open, word("a"), word("b"), Tok::Close, Tok::Or, word("c")]);
        assert_eq!(tokens("!<a> ! b !c"), [Tok::Not, Tok::Open, word("a"), Tok::Close, Tok::Not, word("b"), word("!c")]);
        // après un deux-points, un chevron compare ; dans une expression régulière ou entre guillemets, tout est texte
        assert_eq!(tokens("size:>1mb"), [word("size:>1mb")]);
        assert_eq!(tokens("<size:>1mb dm:<2026>"), [Tok::Open, word("size:>1mb"), word("dm:<2026"), Tok::Close]);
        assert_eq!(tokens("regex:a|b<c>"), [word("regex:a|b<c>")]);
        assert_eq!(tokens("<file:regex:a|(?<n>b)>"), [Tok::Open, word("file:regex:a|(?<n>b)>")]);
        assert_eq!(tokens("\"a | b\" \"!\""), [word("\"a | b\""), word("\"!\"")]);
        assert_eq!(tokens("a>b"), [word("a>b")]);
    }
}
