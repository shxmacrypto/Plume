//! Se connecter par la page du fournisseur (OAuth 2, « code d'autorisation » avec PKCE) : c'est ce que Microsoft
//! exige pour Outlook.com, Hotmail et Microsoft 365. Le navigateur de l'utilisateur ouvre la page de Microsoft ;
//! une fois la connexion faite, il revient sur une petite adresse de ce PC (`http://localhost:port/`) que Plume
//! Mail écoute le temps de la connexion. Plume Mail ne voit jamais le mot de passe : il reçoit un jeton, qu'il
//! renouvelle seul, gardé chiffré par Windows comme l'était le mot de passe.

use crate::net::{Conn, Security, base64};
use serde_json::Value;
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// Un fournisseur : ses adresses, l'identifiant de Plume Mail chez lui, ce que Plume Mail demande.
#[derive(Clone, Debug)]
pub struct Provider {
    pub authorize: String,
    pub token: String,
    pub client_id: String,
    pub scope: String,
}

/// L'identifiant de Plume Mail chez Microsoft, donné à la compilation (`PLUME_MAIL_MS_CLIENT_ID`). Sans lui, la
/// connexion Microsoft n'est pas proposée.
const MS_CLIENT_ID: Option<&str> = option_env!("PLUME_MAIL_MS_CLIENT_ID");

/// Microsoft (comptes personnels et professionnels). None si cette version n'a pas d'identifiant chez Microsoft.
pub fn microsoft() -> Option<Provider> {
    // essais automatiques : un faux fournisseur sur ce PC
    #[cfg(feature = "test-hook")]
    if let Ok(base) = std::env::var("PLUME_MAIL_TEST_OAUTH") {
        return Some(Provider {
            authorize: format!("{base}/authorize"),
            token: format!("{base}/token"),
            client_id: "plume-test".into(),
            scope: "imap smtp offline_access".into(),
        });
    }
    let client_id = MS_CLIENT_ID.filter(|id| !id.trim().is_empty())?;
    Some(Provider {
        authorize: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
        token: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
        client_id: client_id.trim().into(),
        scope: "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access".into(),
    })
}

/// Pourquoi un échange avec le fournisseur n'a pas abouti.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Le réseau : réessayer plus tard suffira.
    Network(String),
    /// Le fournisseur a dit non (connexion retirée, expirée) : il faut se connecter de nouveau.
    Refused(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Network(m) | Failure::Refused(m) => f.write_str(m),
        }
    }
}

/// Ce que le fournisseur donne : un jeton d'accès (une heure environ) et de quoi en obtenir d'autres.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tokens {
    pub access: String,
    /// Vide si le fournisseur n'en a pas donné de nouveau (garder l'ancien).
    pub refresh: String,
    pub expires: Duration,
}

/// Des octets au hasard, de la source de Windows.
fn random(n: usize) -> Vec<u8> {
    use windows::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};
    let mut out = vec![0u8; n];
    let ok = unsafe { BCryptGenRandom(None, &mut out, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }.is_ok();
    assert!(ok, "Windows gave no random bytes");
    out
}

/// Base64 pour les adresses web : « - » et « _ », sans « = ».
fn base64url(data: &[u8]) -> String {
    base64(data).trim_end_matches('=').replace('+', "-").replace('/', "_")
}

/// Un texte pour une adresse web ou un formulaire.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz"), 16) {
                Ok(v) => {
                    out.push(v);
                    i += 2;
                }
                Err(_) => out.push(b'%'),
            },
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect::<Vec<_>>().join("&")
}

/// Une connexion commencée : l'adresse à ouvrir dans le navigateur, et ce qu'il faut pour la finir.
pub struct Pending {
    pub url: String,
    listeners: Vec<TcpListener>,
    redirect: String,
    verifier: String,
    state: String,
}

/// Prépare la connexion : la petite adresse de ce PC où le navigateur reviendra, et la page du fournisseur.
/// `email` : l'adresse du compte, proposée d'avance sur la page.
pub fn begin(p: &Provider, email: &str) -> io::Result<Pending> {
    let v4 = TcpListener::bind("127.0.0.1:0")?;
    let port = v4.local_addr()?.port();
    let mut listeners = vec![v4];
    // « localhost » peut désigner ::1 dans le navigateur : on y écoute aussi, au même port, si c'est possible
    if let Ok(v6) = TcpListener::bind(("::1", port)) {
        listeners.push(v6);
    }
    for l in &listeners {
        l.set_nonblocking(true)?;
    }
    let redirect = format!("http://localhost:{port}/");
    let verifier = base64url(&random(32));
    let challenge = {
        use sha2::{Digest, Sha256};
        base64url(&Sha256::digest(verifier.as_bytes()))
    };
    let state = base64url(&random(16));
    let query = form(&[
        ("client_id", &p.client_id),
        ("response_type", "code"),
        ("redirect_uri", &redirect),
        ("response_mode", "query"),
        ("scope", &p.scope),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("state", &state),
        ("login_hint", email),
        ("prompt", "select_account"),
    ]);
    Ok(Pending { url: format!("{}?{query}", p.authorize), listeners, redirect, verifier, state })
}

/// La page que le navigateur affiche une fois revenu.
fn answer(mut stream: TcpStream, ok: bool) {
    let text = if ok {
        "Plume Mail is signed in. You can close this tab and go back to Plume Mail.<br>Plume Mail est connect&eacute;. Vous pouvez fermer cet onglet."
    } else {
        "The sign-in did not finish. Go back to Plume Mail to try again.<br>La connexion n'a pas abouti. Revenez dans Plume Mail pour r&eacute;essayer."
    };
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>Plume Mail</title><body style=\"font:16px Segoe UI,sans-serif;margin:15vh auto;max-width:34em;line-height:1.6;color:#222\"><p>{text}</p>"
    );
    let _ = stream.write_all(
        format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes(),
    );
}

/// Attend le retour du navigateur (jusqu'à `wait`, ou jusqu'à ce que `cancel` le dise), puis échange le code
/// reçu contre les jetons.
pub fn finish(p: &Provider, pending: Pending, wait: Duration, cancel: &AtomicBool) -> Result<Tokens, String> {
    let start = Instant::now();
    let code = loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        if start.elapsed() > wait {
            return Err("The sign-in took too long. Try again.".into());
        }
        let stream = pending.listeners.iter().find_map(|l| l.accept().ok().map(|(s, _)| s));
        let Some(mut stream) = stream else {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        // la première ligne suffit : « GET /?code=...&state=... HTTP/1.1 »
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while head.len() < 16 * 1024 && !head.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(1) => head.push(byte[0]),
                _ => break,
            }
        }
        let head = String::from_utf8_lossy(&head).into_owned();
        let target = head.split_whitespace().nth(1).unwrap_or_default().to_string();
        let query = target.split_once('?').map(|(_, q)| q).unwrap_or_default();
        let params: Vec<(String, String)> = query.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (decode(k), decode(v))).collect();
        let get = |k: &str| params.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        // un autre appel (l'icône du site, par exemple) : on attend le bon
        if get("code").is_none() && get("error").is_none() {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            continue;
        }
        if get("state").as_deref() != Some(pending.state.as_str()) {
            answer(stream, false);
            return Err("The answer of the sign-in page did not match. Try again.".into());
        }
        if let Some(error) = get("error") {
            answer(stream, false);
            return Err(get("error_description").unwrap_or(error));
        }
        answer(stream, true);
        break get("code").unwrap_or_default();
    };
    exchange(
        p,
        &[
            ("client_id", &p.client_id),
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &pending.redirect),
            ("code_verifier", &pending.verifier),
            ("scope", &p.scope),
        ],
    )
    .map_err(|e| e.to_string())
}

/// Un nouveau jeton d'accès, à partir de celui qui sert à les renouveler.
pub fn refresh(p: &Provider, refresh_token: &str) -> Result<Tokens, Failure> {
    exchange(p, &[("client_id", &p.client_id), ("grant_type", "refresh_token"), ("refresh_token", refresh_token), ("scope", &p.scope)])
}

fn exchange(p: &Provider, pairs: &[(&str, &str)]) -> Result<Tokens, Failure> {
    let (status, body) = post(&p.token, &form(pairs)).map_err(Failure::Network)?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Failure::Network(format!("The sign-in server answered {status} without a readable reply.")))?;
    let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    if status != 200 || text("access_token").is_empty() {
        let why = text("error_description");
        let why = if why.is_empty() { text("error") } else { why };
        // la première ligne suffit (Microsoft ajoute des identifiants de suivi)
        return Err(Failure::Refused(why.lines().next().unwrap_or("The sign-in was refused.").to_string()));
    }
    Ok(Tokens {
        access: text("access_token"),
        refresh: text("refresh_token"),
        expires: Duration::from_secs(v.get("expires_in").and_then(|e| e.as_u64().or_else(|| e.as_str()?.parse().ok())).unwrap_or(3600)),
    })
}

/// Une requête POST d'un formulaire, en HTTPS (Windows chiffre et vérifie le certificat). En HTTP seulement vers
/// un serveur d'essai de ce PC. Renvoie le code de la réponse et son corps.
fn post(url: &str, body: &str) -> Result<(u16, Vec<u8>), String> {
    let (secure, rest) = match url.split_once("://") {
        Some(("https", rest)) => (true, rest),
        Some(("http", rest)) => (false, rest),
        _ => return Err(format!("{url}: not a web address")),
    };
    let (authority, path) = rest.split_once('/').map_or((rest, "/".to_string()), |(a, p)| (a, format!("/{p}")));
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().map_err(|_| format!("{url}: bad port"))?),
        None => (authority, if secure { 443 } else { 80 }),
    };
    if !secure && !matches!(host, "127.0.0.1" | "localhost") {
        return Err("Sign-in pages are only reached over HTTPS.".into());
    }
    let mut conn = Conn::connect(host, port, if secure { Security::Tls } else { Security::Plain }).map_err(|e| e.to_string())?;
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nAccept: application/json\r\nUser-Agent: Plume Mail\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    conn.write_all(request.as_bytes()).and_then(|_| conn.flush()).map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        match conn.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&chunk[..n]),
            // une fermeture sans adieu (fréquente en TLS) après une réponse complète : ce qui est reçu compte
            Err(_) if !raw.is_empty() => break,
            Err(e) => return Err(e.to_string()),
        }
        if raw.len() > 4 * 1024 * 1024 {
            return Err("The sign-in server answered too much.".into());
        }
    }
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("The sign-in server gave no complete answer.")?;
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut body = raw[split + 4..].to_vec();
    let status: u16 = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let chunked = head.lines().any(|l| l.to_ascii_lowercase().replace(' ', "") == "transfer-encoding:chunked");
    if chunked {
        body = dechunk(&body);
    }
    Ok((status, body))
}

/// Le corps d'une réponse envoyée par morceaux (« Transfer-Encoding: chunked »).
fn dechunk(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(end) = data[at..].windows(2).position(|w| w == b"\r\n") {
        let size_text = String::from_utf8_lossy(&data[at..at + end]).into_owned();
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16).unwrap_or(0);
        at += end + 2;
        if size == 0 || at + size > data.len() {
            break;
        }
        out.extend_from_slice(&data[at..at + size]);
        at += size + 2;
    }
    out
}

/// Ce qu'IMAP et SMTP attendent pour s'identifier par jeton (SASL XOAUTH2).
pub fn xoauth2(user: &str, token: &str) -> String {
    base64(format!("user={user}\x01auth=Bearer {token}\x01\x01").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};

    #[test]
    fn texts_for_the_web() {
        assert_eq!(encode("a b&c=d/é~"), "a%20b%26c%3Dd%2F%C3%A9~");
        assert_eq!(decode("a%20b+c%26%C3%A9%zz"), "a b c&é%zz");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        assert_eq!(dechunk(b"4\r\nWiki\r\n5;x=1\r\npedia\r\n0\r\n\r\n"), b"Wikipedia");
        assert_eq!(xoauth2("sam@x.com", "t"), base64(b"user=sam@x.com\x01auth=Bearer t\x01\x01"));
        assert_ne!(random(16), random(16));
    }

    /// Le parcours entier contre un faux fournisseur : la page, le retour du navigateur, l'échange, le renouvellement.
    #[test]
    fn signs_in_through_the_browser() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", server.local_addr().unwrap().port());
        // le faux fournisseur : il vérifie le code et le PKCE, et répond par morceaux comme le font les vrais
        let check = std::thread::spawn(move || {
            let mut seen = vec![];
            for _ in 0..2 {
                let (stream, _) = server.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut len = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                    if line == "\r\n" {
                        break;
                    }
                }
                let mut body = vec![0u8; len];
                reader.read_exact(&mut body).unwrap();
                seen.push(String::from_utf8(body).unwrap());
                let json = if seen.len() == 1 {
                    r#"{"access_token":"tok-1","refresh_token":"ref-1","expires_in":3599}"#
                } else {
                    r#"{"access_token":"tok-2","expires_in":"60"}"#
                };
                let mut w = stream;
                write!(w, "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\n\r\n{:x}\r\n{json}\r\n0\r\n\r\n", json.len())
                    .unwrap();
            }
            seen
        });
        let p = Provider {
            authorize: format!("{base}/authorize"),
            token: format!("{base}/token"),
            client_id: "plume-test".into(),
            scope: "imap offline_access".into(),
        };
        let pending = begin(&p, "sam@hotmail.com").unwrap();
        assert!(pending.url.starts_with(&format!("{base}/authorize?client_id=plume-test&response_type=code&redirect_uri=http%3A%2F%2Flocalhost%3A")));
        assert!(pending.url.contains("code_challenge_method=S256") && pending.url.contains("login_hint=sam%40hotmail.com"));
        let state = pending.url.split("state=").nth(1).unwrap().split('&').next().unwrap().to_string();
        let port = pending.redirect.trim_start_matches("http://localhost:").trim_end_matches('/').to_string();
        // le navigateur : d'abord l'icône du site, puis le retour avec le code
        let browser = std::thread::spawn(move || {
            let get = |path: String| {
                let mut s = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
                write!(s, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
                let mut page = String::new();
                let _ = s.read_to_string(&mut page);
                page
            };
            let icon = get("/favicon.ico".into());
            let page = get(format!("/?code=the-code&state={state}&session_state=x"));
            (icon, page)
        });
        let tokens = finish(&p, pending, Duration::from_secs(10), &AtomicBool::new(false)).unwrap();
        let (icon, page) = browser.join().unwrap();
        assert!(icon.starts_with("HTTP/1.1 404") && page.contains("Plume Mail is signed in"), "{page}");
        assert_eq!(tokens, Tokens { access: "tok-1".into(), refresh: "ref-1".into(), expires: Duration::from_secs(3599) });
        let again = refresh(&p, "ref-1").unwrap();
        assert_eq!((again.access.as_str(), again.refresh.as_str(), again.expires), ("tok-2", "", Duration::from_secs(60)));
        let seen = check.join().unwrap();
        assert!(seen[0].contains("grant_type=authorization_code") && seen[0].contains("code=the-code") && seen[0].contains("code_verifier="), "{}", seen[0]);
        assert!(seen[1].contains("grant_type=refresh_token&refresh_token=ref-1"), "{}", seen[1]);
    }

    #[test]
    fn a_refusal_or_a_wrong_state_is_reported() {
        let p = Provider { authorize: "http://127.0.0.1:9/a".into(), token: "http://127.0.0.1:9/t".into(), client_id: "c".into(), scope: "s".into() };
        let pending = begin(&p, "sam@hotmail.com").unwrap();
        let port = pending.redirect.trim_start_matches("http://localhost:").trim_end_matches('/').to_string();
        std::thread::spawn(move || {
            let mut s = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
            write!(s, "GET /?error=access_denied&error_description=The+user+said+no HTTP/1.1\r\n\r\n").unwrap();
            let _ = s.read_to_string(&mut String::new());
        });
        // l'état ne correspond pas : refusé avant même de regarder l'erreur
        let err = finish(&p, pending, Duration::from_secs(10), &AtomicBool::new(false)).unwrap_err();
        assert!(err.contains("did not match"), "{err}");
        let pending = begin(&p, "sam@hotmail.com").unwrap();
        let cancel = AtomicBool::new(true);
        assert_eq!(finish(&p, pending, Duration::from_secs(10), &cancel).unwrap_err(), "cancelled");
    }
}
