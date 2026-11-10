// Mises à jour intégrées : Plume lit sa dernière version publiée sur GitHub, télécharge PlumeSetup.exe,
// vérifie son empreinte SHA-256 (publiée par GitHub) puis le lance avec --update. L'installeur remplace
// les fichiers (même pendant que Plume tourne) ; un redémarrage de Plume termine la mise à jour.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{GetLastError, WIN32_ERROR},
        Networking::WinInet::{
            ERROR_HTTP_INVALID_SERVER_RESPONSE, ERROR_INTERNET_CANNOT_CONNECT, ERROR_INTERNET_CONNECTION_ABORTED,
            ERROR_INTERNET_CONNECTION_RESET, ERROR_INTERNET_NAME_NOT_RESOLVED, ERROR_INTERNET_TIMEOUT, HTTP_QUERY_CONTENT_LENGTH,
            HTTP_QUERY_FLAG_NUMBER, HTTP_QUERY_LOCATION, HTTP_QUERY_STATUS_CODE, HttpQueryInfoW, INTERNET_FLAG_NO_AUTO_REDIRECT,
            INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_NO_COOKIES, INTERNET_FLAG_NO_UI, INTERNET_FLAG_RELOAD, INTERNET_OPEN_TYPE_PRECONFIG,
            INTERNET_OPTION_CONNECT_TIMEOUT,
            INTERNET_OPTION_RECEIVE_TIMEOUT, INTERNET_OPTION_SEND_TIMEOUT, InternetCloseHandle, InternetOpenUrlW, InternetOpenW,
            InternetReadFile, InternetSetOptionW,
        },
    },
    core::{HSTRING, PCWSTR},
};

pub const ASSET: &str = "PlumeSetup.exe";
const MAX: usize = 64 << 20;

/// Délais réseau (ms). Sans eux, une requête à un GitHub surchargé pouvait rester bloquée plusieurs minutes.
#[derive(Clone, Copy)]
struct Limits {
    connect: u32,
    send: u32,
    /// attente de la réponse, puis de chaque bloc de données
    recv: u32,
    /// pause avant le nouvel essai
    pause: Duration,
}
/// (WinINet compte par pas de 4 s : un serveur muet est abandonné au bout de 32 s, 66 s avec le nouvel essai.)
const NET: Limits = Limits { connect: 15_000, send: 30_000, recv: 30_000, pause: Duration::from_secs(2) };
/// Un essai, plus un nouvel essai si l'échec est passager.
const TRIES: u32 = 2;

/// Échec d'une requête.
#[derive(Debug, PartialEq)]
enum Fail {
    /// le serveur a répondu avec ce code
    Http(u32),
    /// pas de réponse, ou plus de données, dans les délais
    Timeout,
    /// connexion coupée ou réponse tronquée
    Dropped,
    Other(String),
}

impl Fail {
    fn wininet(code: u32) -> Fail {
        match code {
            ERROR_INTERNET_TIMEOUT => Fail::Timeout,
            ERROR_INTERNET_CONNECTION_ABORTED | ERROR_INTERNET_CONNECTION_RESET | ERROR_HTTP_INVALID_SERVER_RESPONSE => Fail::Dropped,
            ERROR_INTERNET_NAME_NOT_RESOLVED => Fail::Other("no internet connection".into()),
            ERROR_INTERNET_CANNOT_CONNECT => Fail::Other("could not connect to the server".into()),
            c => Fail::Other(format!("network error {c}")),
        }
    }

    /// Incident passager (serveur surchargé, délai dépassé, connexion coupée) : un nouvel essai a une chance.
    /// Les autres erreurs (404, 403 de limite d'appels, pas de réseau) donneraient le même résultat.
    fn transient(&self) -> bool {
        matches!(self, Fail::Http(500..=599) | Fail::Timeout | Fail::Dropped)
    }
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Fail::Http(c @ 500..=599) => write!(f, "server error (HTTP {c})"),
            Fail::Http(c) => write!(f, "HTTP {c}"),
            Fail::Timeout => f.write_str("the server did not answer in time"),
            Fail::Dropped => f.write_str("the connection was interrupted"),
            Fail::Other(s) => f.write_str(s),
        }
    }
}

/// Faut-il refaire la requête après l'essai n° `tries` ?
fn retry(f: &Fail, tries: u32) -> bool {
    tries < TRIES && f.transient()
}

#[derive(Clone)]
pub struct Asset {
    pub url: String,
    pub size: u64,
    sha_url: String,
    digest: String,
}

#[derive(Clone)]
pub struct Release {
    pub tag: String,
    pub page: String,
    pub asset: Option<Asset>,
}

fn local(u: &str) -> bool {
    u.starts_with("http://127.0.0.1:") || u.starts_with("http://localhost:")
}

/// Adresse de la dernière version. PLUME_UPDATE_FEED permet de tester avec un serveur local (uniquement).
fn feed() -> String {
    match std::env::var("PLUME_UPDATE_FEED") {
        Ok(u) if local(&u) => u,
        _ => format!("https://api.github.com/repos/{}/releases/latest", crate::REPO),
    }
}

/// « http(s)://hôte:port » d'une adresse.
fn origin(u: &str) -> String {
    u.splitn(4, '/').take(3).collect::<Vec<_>>().join("/")
}

/// Les fichiers ne peuvent venir que des versions publiées de ce dépôt (ou du serveur de test local).
fn trusted(url: &str, feed: &str) -> bool {
    if local(feed) {
        return url.starts_with(&format!("{}/", origin(feed)));
    }
    url.to_lowercase().starts_with(&format!("https://github.com/{}/releases/download/", crate::REPO.to_lowercase()))
}

/// Dernière version publiée, sans l'API de GitHub (limitée pour les appels anonymes, et la première à lâcher
/// quand GitHub est surchargé) : sa page « dernière version » renvoie vers /releases/tag/<version>. C'est une
/// requête légère, faite à chaque démarrage ; l'API n'est appelée qu'au moment d'installer. Rend (version, page).
pub fn latest_tag() -> Result<(String, String), String> {
    let feed = feed();
    let home = if local(&feed) { origin(&feed) } else { format!("https://github.com/{}", crate::REPO) };
    find_tag(format!("{home}/releases/latest"), &format!("{home}/releases/tag/"), &NET)
}

/// Suit au plus 4 redirections depuis `url` jusqu'à une page de version (`prefix` = …/releases/tag/).
fn find_tag(mut url: String, prefix: &str, lim: &Limits) -> Result<(String, String), String> {
    for _ in 0..4 {
        let next = with_retry(lim, || locate_once(&url, lim))?;
        if let Some(tag) = tag_in(&next, prefix) {
            return Ok((tag, next));
        }
        if next == url {
            break;
        }
        url = next;
    }
    Err("no published release found".into())
}

/// Version d'une page de version publiée de ce dépôt (majuscules du nom de dépôt indifférentes).
fn tag_in(url: &str, prefix: &str) -> Option<String> {
    let head = url.get(..prefix.len()).filter(|h| h.eq_ignore_ascii_case(prefix))?;
    let tag = url[head.len()..].split(['?', '#', '/']).next()?;
    (!tag.is_empty() && tag.bytes().all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))).then(|| tag.to_string())
}

fn ver(v: &str) -> (u32, u32, u32) {
    let mut n = v.trim_start_matches(['v', 'V']).split(['.', '-', '+']).map(|x| x.parse().unwrap_or(0));
    (n.next().unwrap_or(0), n.next().unwrap_or(0), n.next().unwrap_or(0))
}

pub fn newer(tag: &str, current: &str) -> bool {
    ver(tag) > ver(current)
}

/// Dernière réponse de l'API. « Réessayer » après un téléchargement raté la réutilise au lieu de redemander
/// l'API, la partie de GitHub qui lâche la première quand il est surchargé (les fichiers passent par son CDN).
static LAST: Mutex<Option<(Instant, Release)>> = Mutex::new(None);
const FRESH: Duration = Duration::from_secs(30 * 60);

fn last() -> std::sync::MutexGuard<'static, Option<(Instant, Release)>> {
    LAST.lock().unwrap_or_else(|e| e.into_inner())
}

/// Lit la dernière version publiée (toujours une vraie requête).
pub fn latest() -> Result<Release, String> {
    let r = fetch_latest()?;
    *last() = Some((Instant::now(), r.clone()));
    Ok(r)
}

/// Comme `latest`, mais reprend une réponse de moins de 30 minutes.
pub fn latest_cached() -> Result<Release, String> {
    let hit = last().as_ref().filter(|(at, _)| at.elapsed() < FRESH).map(|(_, r)| r.clone());
    match hit {
        Some(r) => Ok(r),
        None => latest(),
    }
}

fn fetch_latest() -> Result<Release, String> {
    let feed = feed();
    let j: Value = serde_json::from_slice(&get(&feed, &mut |_| {})?).map_err(|e| e.to_string())?;
    let tag = j["tag_name"].as_str().unwrap_or("").to_string();
    if tag.is_empty() {
        return Err("no release".into());
    }
    let assets = j["assets"].as_array().cloned().unwrap_or_default();
    let named = |n: &str| assets.iter().find(|a| a["name"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(n))).cloned();
    let asset = named(ASSET).and_then(|a| {
        let url = a["browser_download_url"].as_str()?.to_string();
        let sha_url = named(&format!("{ASSET}.sha256")).and_then(|s| s["browser_download_url"].as_str().map(String::from)).unwrap_or_default();
        trusted(&url, &feed).then(|| Asset {
            url,
            size: a["size"].as_u64().unwrap_or(0),
            digest: a["digest"].as_str().unwrap_or("").to_string(),
            sha_url: if trusted(&sha_url, &feed) { sha_url } else { String::new() },
        })
    });
    Ok(Release { tag, page: j["html_url"].as_str().unwrap_or("").to_string(), asset })
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Empreinte attendue : celle que GitHub calcule pour chaque fichier publié, sinon PlumeSetup.exe.sha256.
fn expected_sha(a: &Asset) -> Result<String, String> {
    if let Some(d) = a.digest.strip_prefix("sha256:").filter(|d| is_hex64(d)) {
        return Ok(d.to_lowercase());
    }
    if !a.sha_url.is_empty() {
        let t = String::from_utf8_lossy(&get(&a.sha_url, &mut |_| {})?).to_string();
        if let Some(h) = t.split_whitespace().next().filter(|h| is_hex64(h)) {
            return Ok(h.to_lowercase());
        }
    }
    Err("no SHA-256 published for the installer".into())
}

/// Télécharge l'installeur dans `dir` et vérifie son empreinte. `progress` reçoit 0..=100.
pub fn download(a: &Asset, tag: &str, dir: &Path, progress: &mut dyn FnMut(u32)) -> Result<PathBuf, String> {
    let want = expected_sha(a)?;
    let (mut shown, mut at) = (u32::MAX, Instant::now());
    let size = a.size.max(1);
    // à chaque pour cent, et au moins chaque seconde tant que des données arrivent :
    // l'interface distingue ainsi un téléchargement lent d'un téléchargement bloqué
    let body = get(&a.url, &mut |n| {
        let p = ((n as u64 * 100) / size).min(100) as u32;
        if p != shown || at.elapsed() >= Duration::from_secs(1) {
            (shown, at) = (p, Instant::now());
            progress(p);
        }
    })?;
    let got: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();
    if got != want {
        *last() = None; // version mémorisée peut-être périmée : le prochain essai relira GitHub
        return Err("the download is damaged (SHA-256 mismatch)".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("PlumeSetup-{}.exe", tag.trim_start_matches('v')));
    std::fs::write(&file, &body).map_err(|e| e.to_string())?;
    Ok(file)
}

/// Lance l'installeur en mode mise à jour et attend la fin (quelques dixièmes de seconde).
pub fn install(setup: &Path) -> Result<(), String> {
    let st = std::process::Command::new(setup).arg("--update").status().map_err(|e| e.to_string())?;
    if st.success() { Ok(()) } else { Err(format!("the installer failed (code {})", st.code().unwrap_or(-1))) }
}

/// GET simple pour les autres fonctions réseau (fuites de mots de passe, météo…).
pub fn http_get(url: &str) -> Result<Vec<u8>, String> {
    get(url, &mut |_| {})
}

/// GET via WinINet (réglages proxy de Windows, suit les redirections de GitHub, pas de cache), avec des
/// délais et un nouvel essai si le serveur répond 5xx, ne répond pas à temps ou coupe la connexion.
fn get(url: &str, progress: &mut dyn FnMut(usize)) -> Result<Vec<u8>, String> {
    get_with(url, &NET, progress)
}

fn get_with(url: &str, lim: &Limits, progress: &mut dyn FnMut(usize)) -> Result<Vec<u8>, String> {
    with_retry(lim, || get_once(url, lim, &mut *progress))
}

/// Refait une fois une requête qui a échoué pour une raison passagère.
fn with_retry<T>(lim: &Limits, mut once: impl FnMut() -> Result<T, Fail>) -> Result<T, String> {
    let mut tries = 0;
    loop {
        tries += 1;
        match once() {
            Err(f) if retry(&f, tries) => std::thread::sleep(lim.pause),
            r => return r.map_err(|f| f.to_string()),
        }
    }
}

/// Poignée WinINet, fermée automatiquement.
struct H(*mut core::ffi::c_void);

impl Drop for H {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = InternetCloseHandle(self.0);
            }
        }
    }
}

/// Requête ouverte : en-têtes de la réponse reçus, contenu pas encore lu.
struct Conn {
    req: H, // fermée avant la session
    _net: H,
}

impl Conn {
    fn num(&self, what: u32) -> Option<u32> {
        let (mut v, mut len) = (0u32, 4u32);
        unsafe { HttpQueryInfoW(self.req.0, what | HTTP_QUERY_FLAG_NUMBER, Some(&mut v as *mut u32 as _), &mut len, None) }.ok().map(|_| v)
    }

    fn text(&self, what: u32) -> Option<String> {
        let mut buf = vec![0u16; 2048];
        let mut len = (buf.len() * 2) as u32;
        unsafe { HttpQueryInfoW(self.req.0, what, Some(buf.as_mut_ptr() as _), &mut len, None) }.ok()?;
        Some(String::from_utf16_lossy(&buf[..(len as usize / 2).min(buf.len())]))
    }
}

/// Ouvre `url` (réglages proxy de Windows, sans cache ni cookies) avec les délais de `lim`.
fn open(url: &str, lim: &Limits, flags: u32, accept: &str) -> Result<Conn, Fail> {
    unsafe {
        let net = H(InternetOpenW(&HSTRING::from(format!("Plume/{}", crate::VERSION)), INTERNET_OPEN_TYPE_PRECONFIG.0, PCWSTR::null(), PCWSTR::null(), 0));
        if net.0.is_null() {
            return Err(Fail::Other("network unavailable".into()));
        }
        // réglés sur la session : ils valent pour la requête et pour les redirections de GitHub
        for (opt, ms) in [(INTERNET_OPTION_CONNECT_TIMEOUT, lim.connect), (INTERNET_OPTION_SEND_TIMEOUT, lim.send), (INTERNET_OPTION_RECEIVE_TIMEOUT, lim.recv)] {
            let _ = InternetSetOptionW(Some(net.0 as *const _), opt, Some(&ms as *const u32 as *const _), 4);
        }
        let headers: Vec<u16> = format!("Accept: {accept}\r\n").encode_utf16().collect();
        let flags = flags | INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_NO_UI | INTERNET_FLAG_NO_COOKIES;
        let wurl = HSTRING::from(url);
        let req = H(InternetOpenUrlW(net.0, &wurl, Some(&headers), flags, None));
        if req.0.is_null() {
            return Err(Fail::wininet(GetLastError().0));
        }
        Ok(Conn { req, _net: net })
    }
}

fn get_once(url: &str, lim: &Limits, progress: &mut dyn FnMut(usize)) -> Result<Vec<u8>, Fail> {
    let c = open(url, lim, 0, "application/vnd.github+json")?;
    match c.num(HTTP_QUERY_STATUS_CODE) {
        Some(200) => {}
        Some(code) => return Err(Fail::Http(code)),
        None => return Err(Fail::Dropped),
    }
    let size = c.num(HTTP_QUERY_CONTENT_LENGTH);
    let (mut out, mut buf) = (Vec::new(), vec![0u8; 16 * 1024]);
    loop {
        let mut n = 0u32;
        unsafe { InternetReadFile(c.req.0, buf.as_mut_ptr() as _, buf.len() as u32, &mut n) }
            .map_err(|e| Fail::wininet(WIN32_ERROR::from_error(&e).map_or(0, |w| w.0)))?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
        if out.len() > MAX {
            return Err(Fail::Other("download too large".into()));
        }
        progress(out.len());
    }
    // fin des données avant la taille annoncée : la connexion a été coupée en route
    if size.is_some_and(|s| out.len() < s as usize) {
        return Err(Fail::Dropped);
    }
    Ok(out)
}

/// Adresse vers laquelle `url` renvoie (sans suivre la redirection), ou `url` elle-même si c'est une page.
fn locate_once(url: &str, lim: &Limits) -> Result<String, Fail> {
    let c = open(url, lim, INTERNET_FLAG_NO_AUTO_REDIRECT, "text/html")?;
    match c.num(HTTP_QUERY_STATUS_CODE) {
        Some(301 | 302 | 303 | 307 | 308) => {
            let to = c.text(HTTP_QUERY_LOCATION).filter(|t| !t.is_empty()).ok_or(Fail::Dropped)?;
            Ok(if to.starts_with('/') { format!("{}{to}", origin(url)) } else { to })
        }
        Some(200) => Ok(url.to_string()),
        Some(code) => Err(Fail::Http(code)),
        None => Err(Fail::Dropped),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    /// Délais courts pour les tests (les vrais : 15 s pour se connecter, 30 s pour envoyer ou recevoir).
    const FAST: Limits = Limits { connect: 2_000, send: 2_000, recv: 1_000, pause: Duration::from_millis(100) };

    #[test]
    fn retry_decision() {
        for f in [Fail::Http(500), Fail::Http(502), Fail::Http(503), Fail::Http(504), Fail::Timeout, Fail::Dropped] {
            assert!(retry(&f, 1), "{f:?} mérite un nouvel essai");
            assert!(!retry(&f, 2), "{f:?} : un seul nouvel essai");
        }
        // 404, limite d'appels de GitHub, pas de réseau : un nouvel essai donnerait la même chose
        for f in [Fail::Http(404), Fail::Http(403), Fail::Http(429), Fail::wininet(12007), Fail::wininet(12029), Fail::Other("x".into())] {
            assert!(!retry(&f, 1), "{f:?} ne mérite pas de nouvel essai");
        }
        assert_eq!(Fail::wininet(12002), Fail::Timeout);
        for c in [12030, 12031, 12152] {
            assert_eq!(Fail::wininet(c), Fail::Dropped);
        }
        assert_eq!(Fail::Http(504).to_string(), "server error (HTTP 504)");
        assert_eq!(Fail::Http(404).to_string(), "HTTP 404");
    }

    /// Réponse du serveur de test à une connexion.
    #[derive(Clone, Copy)]
    enum Reply {
        /// ce code HTTP, sans contenu
        Code(u32),
        /// 200 et tout le contenu
        Full,
        /// lit la requête et ne répond jamais
        Silent,
        /// 200, taille complète annoncée, `n` octets, puis plus rien (connexion gardée ouverte)
        Stall(usize),
        /// pareil, puis la connexion est fermée
        Cut(usize),
        /// 302 vers cette adresse
        Moved(&'static str),
    }

    /// Serveur local : la connexion n° i reçoit `replies[i]` (la dernière se répète). Rend l'adresse et le compteur.
    fn serve(body: Vec<u8>, replies: Vec<Reply>) -> (String, Arc<AtomicUsize>) {
        let srv = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/file", srv.local_addr().unwrap().port());
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        std::thread::spawn(move || {
            for s in srv.incoming().flatten() {
                let r = replies[h.fetch_add(1, Ordering::SeqCst).min(replies.len() - 1)];
                let body = body.clone();
                std::thread::spawn(move || {
                    let mut s = s;
                    let _ = s.read(&mut [0u8; 4096]);
                    let head = |code: u32, len: usize| format!("HTTP/1.1 {code} X\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n");
                    let hang = || std::thread::sleep(Duration::from_secs(20));
                    match r {
                        Reply::Code(c) => {
                            let _ = s.write_all(head(c, 0).as_bytes());
                        }
                        Reply::Full => {
                            let _ = s.write_all(head(200, body.len()).as_bytes()).and_then(|_| s.write_all(&body));
                        }
                        Reply::Silent => hang(),
                        Reply::Moved(to) => {
                            let _ = write!(s, "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        }
                        Reply::Stall(n) | Reply::Cut(n) => {
                            let _ = s.write_all(head(200, body.len()).as_bytes()).and_then(|_| s.write_all(&body[..n]));
                            if matches!(r, Reply::Stall(_)) {
                                hang();
                            }
                        }
                    }
                });
            }
        });
        (url, hits)
    }

    fn payload() -> Vec<u8> {
        (0..200_000u32).map(|i| (i % 251) as u8).collect()
    }

    /// WinINet limite le nombre de connexions par serveur dans un même processus : les tests qui parlent
    /// à 127.0.0.1 passent un par un, sinon ils attendent une place et dépassent leurs délais.
    fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
        static NET_TESTS: Mutex<()> = Mutex::new(());
        NET_TESTS.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn tag_from_release_page() {
        let p = "https://github.com/shxmacrypto/plume/releases/tag/";
        assert_eq!(tag_in("https://github.com/shxmacrypto/Plume/releases/tag/v0.7.1", p).as_deref(), Some("v0.7.1"));
        assert_eq!(tag_in("https://github.com/shxmacrypto/plume/releases/tag/v1.2.3?x=1#y", p).as_deref(), Some("v1.2.3"));
        assert_eq!(tag_in("https://github.com/shxmacrypto/plume/releases", p), None); // aucune version publiée
        assert_eq!(tag_in("https://github.com/someone/else/releases/tag/v9.9.9", p), None);
        assert_eq!(tag_in("https://evil.example/shxmacrypto/plume/releases/tag/v9.9.9", p), None);
        assert_eq!(tag_in("https://github.com/shxmacrypto/plume/releases/tag/v1<script>", p), None);
    }

    /// La vérification suit les redirections de releases/latest jusqu'à la page de version, avec un nouvel
    /// essai si GitHub répond 504 en route.
    #[test]
    fn latest_tag_through_redirects() {
        let _one = one_at_a_time();
        let (url, hits) = serve(vec![], vec![Reply::Moved("/Releases/latest"), Reply::Code(504), Reply::Moved("/releases/tag/v9.9.9")]);
        let home = origin(&url);
        let (tag, page) = find_tag(format!("{home}/releases/latest"), &format!("{home}/releases/tag/"), &FAST).unwrap();
        assert_eq!((tag.as_str(), page), ("v9.9.9", format!("{home}/releases/tag/v9.9.9")));
        assert_eq!(hits.load(Ordering::SeqCst), 3);
        // une page sans version (aucune publiée) : erreur claire, pas de boucle
        let (url, _) = serve(b"<html>".to_vec(), vec![Reply::Full]);
        let home = origin(&url);
        assert_eq!(find_tag(format!("{home}/releases/latest"), &format!("{home}/releases/tag/"), &FAST).unwrap_err(), "no published release found");
    }

    #[test]
    fn retries_once_after_504() {
        let _one = one_at_a_time();
        let (url, hits) = serve(b"ok".to_vec(), vec![Reply::Code(504), Reply::Full]);
        assert_eq!(get_with(&url, &FAST, &mut |_| {}).unwrap(), b"ok");
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn gives_up_after_second_5xx() {
        let _one = one_at_a_time();
        let (url, hits) = serve(vec![], vec![Reply::Code(503)]);
        assert_eq!(get_with(&url, &FAST, &mut |_| {}).unwrap_err(), "server error (HTTP 503)");
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn no_retry_on_404() {
        let _one = one_at_a_time();
        let (url, hits) = serve(vec![], vec![Reply::Code(404)]);
        assert_eq!(get_with(&url, &FAST, &mut |_| {}).unwrap_err(), "HTTP 404");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    /// Serveur qui ne répond jamais (GitHub pendant l'incident) : erreur au bout des délais, plus de blocage.
    /// WinINet arrondit le délai de réception au multiple de 4 s supérieur : 1 s → 4 s par essai (30 s → 32 s).
    #[test]
    fn silent_server_times_out() {
        let _one = one_at_a_time();
        let (url, hits) = serve(vec![], vec![Reply::Silent]);
        let t = Instant::now();
        assert_eq!(get_with(&url, &FAST, &mut |_| {}).unwrap_err(), "the server did not answer in time");
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert!(t.elapsed() < Duration::from_secs(15), "{:?}", t.elapsed());
    }

    /// Téléchargement qui cale en route : délai dépassé, nouvel essai, fichier complet.
    #[test]
    fn stalled_download_is_retried() {
        let _one = one_at_a_time();
        let body = payload();
        let (url, hits) = serve(body.clone(), vec![Reply::Stall(50_000), Reply::Full]);
        let t = Instant::now();
        let mut seen = 0;
        assert_eq!(get_with(&url, &FAST, &mut |n| seen = n).unwrap(), body);
        assert_eq!((hits.load(Ordering::SeqCst), seen), (2, body.len()));
        assert!(t.elapsed() < Duration::from_secs(15), "{:?}", t.elapsed());
    }

    /// Connexion fermée avant la taille annoncée : ce n'est pas un fichier complet, on recommence.
    #[test]
    fn cut_download_is_retried() {
        let _one = one_at_a_time();
        let body = payload();
        let (url, hits) = serve(body.clone(), vec![Reply::Cut(50_000), Reply::Full]);
        assert_eq!(get_with(&url, &FAST, &mut |_| {}).unwrap(), body);
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn versions() {
        assert!(newer("v0.6.0", "0.5.0"));
        assert!(newer("v1.0.0", "0.9.9"));
        assert!(newer("0.5.10", "0.5.9"));
        assert!(!newer("v0.5.0", "0.5.0"));
        assert!(!newer("v0.4.9", "0.5.0"));
    }

    #[test]
    fn only_this_repo() {
        let gh = format!("https://api.github.com/repos/{}/releases/latest", crate::REPO);
        assert!(trusted(&format!("https://github.com/{}/releases/download/v1.0.0/PlumeSetup.exe", crate::REPO), &gh));
        assert!(trusted(&format!("https://github.com/{}/releases/download/v1.0.0/PlumeSetup.exe", crate::REPO.to_uppercase()), &gh));
        assert!(!trusted("https://github.com/someone/else/releases/download/v1.0.0/PlumeSetup.exe", &gh));
        assert!(!trusted("https://evil.example/PlumeSetup.exe", &gh));
        assert!(trusted("http://127.0.0.1:4810/PlumeSetup.exe", "http://127.0.0.1:4810/latest.json"));
        assert!(!trusted("http://127.0.0.1:9999/PlumeSetup.exe", "http://127.0.0.1:4810/latest.json"));
    }

    /// Flux local : release JSON + « installeur » ; vérifie le choix du fichier, la progression et l'empreinte.
    #[test]
    fn feed_download_and_checksum() {
        let _one = one_at_a_time();
        let srv = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", srv.local_addr().unwrap().port());
        let payload: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let sha: String = Sha256::digest(&payload).iter().map(|b| format!("{b:02x}")).collect();
        let json = format!(
            r#"{{"tag_name":"v9.9.9","html_url":"{base}/rel","assets":[{{"name":"Plume.exe","browser_download_url":"{base}/old.exe","size":1}},{{"name":"PlumeSetup.exe","browser_download_url":"{base}/PlumeSetup.exe","size":{},"digest":"sha256:{sha}"}}]}}"#,
            payload.len()
        );
        let body = payload.clone();
        std::thread::spawn(move || {
            for s in srv.incoming().take(3).flatten() {
                let mut s = s;
                let mut req = [0u8; 2048];
                let n = s.read(&mut req).unwrap_or(0);
                let path = String::from_utf8_lossy(&req[..n]).split_whitespace().nth(1).unwrap_or("").to_string();
                let data: &[u8] = if path == "/latest.json" { json.as_bytes() } else { &body };
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", data.len());
                let _ = s.write_all(data);
            }
        });
        unsafe { std::env::set_var("PLUME_UPDATE_FEED", format!("{base}/latest.json")) };
        let r = latest().unwrap();
        assert_eq!(r.tag, "v9.9.9");
        let a = r.asset.expect("installer asset");
        assert!(a.url.ends_with("/PlumeSetup.exe"));
        let dir = std::env::temp_dir().join(format!("plume-upd-test-{}", std::process::id()));
        let mut seen = vec![];
        let file = download(&a, &r.tag, &dir, &mut |p| seen.push(p)).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), payload);
        assert_eq!(seen.last(), Some(&100));
        let bad = Asset { digest: format!("sha256:{}", "0".repeat(64)), ..a };
        assert!(download(&bad, &r.tag, &dir, &mut |_| {}).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
