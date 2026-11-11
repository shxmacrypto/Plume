//! L'envoi d'un message (SMTP) : se connecter, s'identifier, donner l'expéditeur, les destinataires, le message.

use crate::net::{Conn, Security, base64, sasl_plain};
use std::io::{self, Read, Write};

/// Ce que le serveur a refusé, dans ses mots ; ou la panne de réseau.
pub type Result<T> = std::result::Result<T, String>;

struct Smtp {
    conn: Conn,
    buf: Vec<u8>,
}

impl Smtp {
    /// Une réponse du serveur : son code et son texte (toutes ses lignes).
    fn reply(&mut self) -> io::Result<(u16, String)> {
        let mut text = String::new();
        loop {
            // une ligne entière
            let end = loop {
                if let Some(end) = self.buf.iter().position(|&b| b == b'\n') {
                    break end;
                }
                let mut chunk = [0u8; 4096];
                let n = self.conn.read(&mut chunk)?;
                if n == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the server closed the connection"));
                }
                self.buf.extend_from_slice(&chunk[..n]);
            };
            let line: Vec<u8> = self.buf.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line).trim_end().to_string();
            let code = line.get(..3).and_then(|c| c.parse().ok()).unwrap_or(0);
            text.push_str(line.get(4..).unwrap_or_default());
            // « 250-... » : d'autres lignes suivent ; « 250 ... » : c'est la dernière
            if line.as_bytes().get(3) != Some(&b'-') {
                return Ok((code, text));
            }
            text.push('\n');
        }
    }

    /// Envoie une ligne et attend une réponse dont le code commence par `want` (2 : accepté, 3 : continuez).
    fn say(&mut self, line: &str, want: u16) -> Result<String> {
        self.conn.write_all(format!("{line}\r\n").as_bytes()).and_then(|_| self.conn.flush()).map_err(|e| e.to_string())?;
        self.expect(want)
    }

    fn expect(&mut self, want: u16) -> Result<String> {
        let (code, text) = self.reply().map_err(|e| e.to_string())?;
        if code / 100 == want { Ok(text) } else { Err(format!("{code} {text}")) }
    }
}

/// Le nom sous lequel ce PC se présente : rien qui le désigne (« [127.0.0.1] » est permis par la norme).
const HELLO: &str = "EHLO [127.0.0.1]";

/// Le serveur d'envoi et de quoi s'y identifier (`user` vide : sans identification).
pub struct Server<'a> {
    pub host: &'a str,
    pub port: u16,
    pub security: Security,
    pub user: &'a str,
    pub password: &'a str,
}

/// Envoie un message. `from` et `to` : les adresses seules (l'enveloppe) ; `message` : en-têtes et corps.
pub fn send(server: &Server, from: &str, to: &[String], message: &[u8]) -> Result<()> {
    let Server { host, port, security, user, password } = *server;
    if to.is_empty() {
        return Err("No recipient.".into());
    }
    let conn = Conn::connect(host, port, security).map_err(|e| e.to_string())?;
    let mut smtp = Smtp { conn, buf: Vec::new() };
    smtp.expect(2)?;
    let mut features = smtp.say(HELLO, 2)?;
    if security == Security::StartTls {
        smtp.say("STARTTLS", 2)?;
        // rien de ce qui a été reçu en clair ne compte plus
        let Smtp { conn, .. } = smtp;
        smtp = Smtp { conn: conn.secure(host).map_err(|e| e.to_string())?, buf: Vec::new() };
        features = smtp.say(HELLO, 2)?;
    }
    // le mot de passe ne part jamais sur une connexion en clair, sauf vers un serveur d'essai de ce PC
    if !user.is_empty() {
        let auth = features.lines().find(|l| l.to_ascii_uppercase().starts_with("AUTH")).unwrap_or_default().to_ascii_uppercase();
        if auth.contains("PLAIN") || !auth.contains("LOGIN") {
            smtp.say(&format!("AUTH PLAIN {}", sasl_plain(user, password)), 2)?;
        } else {
            smtp.say("AUTH LOGIN", 3)?;
            smtp.say(&base64(user.as_bytes()), 3)?;
            smtp.say(&base64(password.as_bytes()), 2)?;
        }
    }
    smtp.say(&format!("MAIL FROM:<{from}>"), 2)?;
    for rcpt in to {
        smtp.say(&format!("RCPT TO:<{rcpt}>"), 2)?;
    }
    smtp.say("DATA", 3)?;
    // une ligne qui commence par un point en reçoit un second : seul, il marquerait la fin du message
    let mut body = Vec::with_capacity(message.len() + 64);
    for line in message.split_inclusive(|&b| b == b'\n') {
        if line.starts_with(b".") {
            body.push(b'.');
        }
        body.extend_from_slice(line);
    }
    if !body.ends_with(b"\r\n") {
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b".\r\n");
    smtp.conn.write_all(&body).and_then(|_| smtp.conn.flush()).map_err(|e| e.to_string())?;
    smtp.expect(2)?;
    let _ = smtp.say("QUIT", 2);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        net::TcpListener,
        sync::mpsc,
    };

    /// Un serveur d'envoi de théâtre : il accepte tout (sauf le destinataire « nobody ») et rend ce qu'il a reçu.
    fn stage(auth_line: &'static str) -> (u16, mpsc::Receiver<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut w = stream;
            let mut seen = vec![];
            w.write_all(b"220 stage ready\r\n").unwrap();
            let mut data = false;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let line = line.trim_end_matches(['\r', '\n']).to_string();
                seen.push(line.clone());
                if data {
                    if line == "." {
                        data = false;
                        w.write_all(b"250 queued\r\n").unwrap();
                    }
                    continue;
                }
                let upper = line.to_ascii_uppercase();
                let reply = if upper.starts_with("EHLO") {
                    format!("250-stage\r\n250-SIZE 1000000\r\n250-{auth_line}\r\n250 8BITMIME\r\n")
                } else if upper.starts_with("AUTH LOGIN") {
                    "334 VXNlcm5hbWU6\r\n".into()
                } else if upper.starts_with("AUTH PLAIN") {
                    if line.ends_with(&sasl_plain("sam", "pw")) { "235 ok\r\n".into() } else { "535 5.7.8 Authentication credentials invalid\r\n".into() }
                } else if line == base64(b"sam") {
                    "334 UGFzc3dvcmQ6\r\n".into()
                } else if line == base64(b"pw") {
                    "235 ok\r\n".into()
                } else if upper.starts_with("RCPT TO:<NOBODY@") {
                    "550 5.1.1 No such user here\r\n".into()
                } else if upper.starts_with("DATA") {
                    data = true;
                    "354 go\r\n".into()
                } else if upper.starts_with("QUIT") {
                    w.write_all(b"221 bye\r\n").unwrap();
                    break;
                } else {
                    "250 ok\r\n".into()
                };
                w.write_all(reply.as_bytes()).unwrap();
            }
            let _ = tx.send(seen);
        });
        (port, rx)
    }

    fn server(port: u16, password: &'static str) -> Server<'static> {
        Server { host: "127.0.0.1", port, security: Security::Plain, user: "sam", password }
    }

    #[test]
    fn sends_a_message() {
        let (port, seen) = stage("AUTH PLAIN LOGIN");
        let message = b"Subject: Hi\r\n\r\nLine one\r\n.hidden dot\r\n..two\r\nlast";
        send(&server(port, "pw"), "sam@example.com", &["ann@example.com".into(), "bob@example.com".into()], message).unwrap();
        let seen = seen.recv().unwrap();
        assert_eq!(seen[0], "EHLO [127.0.0.1]");
        assert!(seen[1].starts_with("AUTH PLAIN "));
        assert_eq!(&seen[2..5], ["MAIL FROM:<sam@example.com>", "RCPT TO:<ann@example.com>", "RCPT TO:<bob@example.com>"]);
        // les points en début de ligne sont doublés, le message finit par un point seul
        assert_eq!(&seen[5..], ["DATA", "Subject: Hi", "", "Line one", "..hidden dot", "...two", "last", ".", "QUIT"]);
    }

    #[test]
    fn older_login_and_refusals() {
        let (port, seen) = stage("AUTH LOGIN");
        send(&server(port, "pw"), "sam@example.com", &["ann@example.com".into()], b"x\r\n").unwrap();
        assert_eq!(&seen.recv().unwrap()[1..4], ["AUTH LOGIN", base64(b"sam").as_str(), base64(b"pw").as_str()]);
        // un mauvais mot de passe, un destinataire inconnu : le message du serveur revient tel quel
        let (port, _) = stage("AUTH PLAIN");
        let refused = send(&server(port, "wrong"), "sam@example.com", &["ann@example.com".into()], b"x\r\n").unwrap_err();
        assert!(refused.starts_with("535") && refused.contains("credentials invalid"), "{refused}");
        let (port, _) = stage("AUTH PLAIN");
        let unknown = send(&server(port, "pw"), "sam@example.com", &["nobody@example.com".into()], b"x\r\n").unwrap_err();
        assert!(unknown.contains("No such user"), "{unknown}");
        assert!(send(&Server { user: "", ..server(1, "") }, "a@b.c", &[], b"x").is_err());
    }
}
