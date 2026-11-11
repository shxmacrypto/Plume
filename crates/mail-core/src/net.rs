//! La connexion à un serveur de courrier : TCP, puis TLS par Windows lui-même (SChannel). Les certificats sont
//! vérifiés par Windows, avec ses autorités de confiance ; rien n'est embarqué ni à tenir à jour.

use schannel::{
    schannel_cred::{Direction, SchannelCred},
    tls_stream::{Builder, TlsStream},
};
use std::{
    io::{self, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

/// Comment la connexion est protégée.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Security {
    /// Chiffrée dès le premier octet (ports 993 et 465).
    Tls,
    /// Commencée en clair, chiffrée avant de s'identifier (ports 143 et 587).
    StartTls,
    /// Jamais chiffrée : seulement pour un serveur d'essai sur ce PC.
    Plain,
}

impl Security {
    pub fn name(self) -> &'static str {
        match self {
            Security::Tls => "tls",
            Security::StartTls => "starttls",
            Security::Plain => "plain",
        }
    }

    pub fn parse(name: &str) -> Security {
        match name {
            "starttls" => Security::StartTls,
            "plain" => Security::Plain,
            _ => Security::Tls,
        }
    }
}

const CONNECT: Duration = Duration::from_secs(20);
const PATIENCE: Duration = Duration::from_secs(90);

/// La connexion, chiffrée ou pas encore.
pub enum Conn {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl Conn {
    /// Ouvre la connexion TCP (sans rien dire encore au serveur).
    pub fn open(host: &str, port: u16) -> io::Result<Conn> {
        let mut last = io::Error::new(io::ErrorKind::NotFound, format!("{host}: no address"));
        for addr in (host, port).to_socket_addrs()? {
            match TcpStream::connect_timeout(&addr, CONNECT) {
                Ok(tcp) => {
                    tcp.set_read_timeout(Some(PATIENCE))?;
                    tcp.set_write_timeout(Some(PATIENCE))?;
                    let _ = tcp.set_nodelay(true);
                    return Ok(Conn::Plain(tcp));
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// Passe en TLS. `host` : le nom que le certificat du serveur doit porter.
    pub fn secure(self, host: &str) -> io::Result<Conn> {
        let Conn::Plain(tcp) = self else { return Ok(self) };
        let cred = SchannelCred::builder().acquire(Direction::Outbound)?;
        let mut builder = Builder::new();
        builder.domain(host);
        // Essais automatiques : le serveur d'essai de ce PC a un certificat que personne n'a signé.
        #[cfg(any(test, feature = "test-hook"))]
        if std::env::var_os("PLUME_MAIL_TEST_TRUST").is_some() && matches!(host, "localhost" | "127.0.0.1") {
            builder.verify_callback(|_| Ok(()));
        }
        let tls = builder.connect(cred, tcp).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{host}: {e}")))?;
        Ok(Conn::Tls(Box::new(tls)))
    }

    /// Ouvre la connexion comme le veut `security` (pour StartTls, elle reste en clair : au protocole de demander
    /// le chiffrement, puis d'appeler `secure`).
    pub fn connect(host: &str, port: u16, security: Security) -> io::Result<Conn> {
        let conn = Conn::open(host, port)?;
        if security == Security::Tls { conn.secure(host) } else { Ok(conn) }
    }

    pub fn is_secure(&self) -> bool {
        matches!(self, Conn::Tls(_))
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Conn::Plain(s) => s.read(buf),
            Conn::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Conn::Plain(s) => s.write(buf),
            Conn::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Conn::Plain(s) => s.flush(),
            Conn::Tls(s) => s.flush(),
        }
    }
}

/// Base64 (pour s'identifier et pour les pièces jointes).
pub fn base64(data: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            out.push(if k <= chunk.len() { ABC[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// Ce que le serveur attend pour s'identifier par nom et mot de passe (SASL PLAIN).
pub fn sasl_plain(user: &str, password: &str) -> String {
    base64(format!("\0{user}\0{password}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_text() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64("été".as_bytes()), "w6l0w6k=");
        assert_eq!(sasl_plain("sam", "p@ss"), base64(b"\0sam\0p@ss"));
        assert_eq!((Security::parse("starttls"), Security::parse("plain"), Security::parse("?")), (Security::StartTls, Security::Plain, Security::Tls));
    }
}
