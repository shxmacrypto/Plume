//! Just enough HTTP/1.1 for the phone page: one request per connection, a body read by its length, answers closed
//! after sending. Limits keep a stray client from holding memory.

use crate::util::url_decode;
use std::io::{self, BufRead, Read, Write};

/// The request line and headers together.
const HEAD_MAX: u64 = 16 * 1024;

#[derive(Debug, Default)]
pub struct Request {
    pub method: String,
    /// Path without the query, decoded.
    pub path: String,
    pub query: Vec<(String, String)>,
    /// Names in lower case.
    pub headers: Vec<(String, String)>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
    pub fn param(&self, name: &str) -> Option<&str> {
        self.query.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
    pub fn content_length(&self) -> Option<u64> {
        self.header("content-length").and_then(|v| v.trim().parse().ok())
    }
}

/// Reads the request line and headers. `None`: the client left, or what it sent is not HTTP.
pub fn read_head(r: &mut impl BufRead) -> io::Result<Option<Request>> {
    let mut limited = r.take(HEAD_MAX);
    let mut line = Vec::new();
    let mut req = Request::default();
    let mut first = true;
    loop {
        line.clear();
        if limited.read_until(b'\n', &mut line)? == 0 {
            return Ok(None);
        }
        if line.last() != Some(&b'\n') {
            // too long, or cut
            return Ok(None);
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim_end_matches(['\r', '\n']);
        if first {
            if text.is_empty() {
                // tolerated before the request line
                continue;
            }
            first = false;
            let mut parts = text.split(' ');
            let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next()) else { return Ok(None) };
            if !version.starts_with("HTTP/1.") || method.is_empty() {
                return Ok(None);
            }
            req.method = method.to_string();
            let (path, query) = target.split_once('?').unwrap_or((target, ""));
            req.path = url_decode(path);
            req.query = query
                .split('&')
                .filter(|p| !p.is_empty())
                .map(|p| {
                    let (k, v) = p.split_once('=').unwrap_or((p, ""));
                    (url_decode(k), url_decode(v))
                })
                .collect();
            continue;
        }
        if text.is_empty() {
            return Ok(Some(req));
        }
        if req.headers.len() >= 100 {
            return Ok(None);
        }
        if let Some((k, v)) = text.split_once(':') {
            req.headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
}

pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

/// Status line and headers of an answer of `len` bytes.
pub fn write_head(w: &mut impl Write, status: u16, ctype: &str, len: u64, extra: &[(&str, String)]) -> io::Result<()> {
    let mut h = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {ctype}\r\nContent-Length: {len}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\nConnection: close\r\n",
        reason(status)
    );
    for (k, v) in extra {
        // a value never carries a line break (it could add a header)
        h.push_str(&format!("{k}: {}\r\n", v.replace(['\r', '\n'], "")));
    }
    h.push_str("\r\n");
    w.write_all(h.as_bytes())
}

pub fn respond(w: &mut impl Write, status: u16, ctype: &str, body: &[u8], extra: &[(&str, String)]) -> io::Result<()> {
    write_head(w, status, ctype, body.len() as u64, extra)?;
    w.write_all(body)?;
    w.flush()
}

pub fn json(w: &mut impl Write, status: u16, v: &serde_json::Value) -> io::Result<()> {
    respond(w, status, "application/json; charset=utf-8", v.to_string().as_bytes(), &[])
}

/// A small body (JSON, text) read whole; `None` if missing or longer than `max`.
pub fn read_body(r: &mut impl Read, req: &Request, max: u64) -> Option<Vec<u8>> {
    let len = req.content_length()?;
    if len > max {
        return None;
    }
    let mut buf = vec![0; len as usize];
    r.read_exact(&mut buf).ok()?;
    Some(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Option<Request> {
        read_head(&mut io::Cursor::new(s.as_bytes().to_vec())).unwrap()
    }

    #[test]
    fn request() {
        let r = parse("POST /api/upload?name=%C3%A9t%C3%A9.jpg&x HTTP/1.1\r\nHost: a\r\nContent-Length: 12\r\nX-Drop-Token:  abc \r\n\r\nbody").unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.path, "/api/upload");
        assert_eq!(r.param("name"), Some("été.jpg"));
        assert_eq!(r.param("x"), Some(""));
        assert_eq!(r.content_length(), Some(12));
        assert_eq!(r.header("x-drop-token"), Some("abc"));
    }

    #[test]
    fn not_http() {
        assert!(parse("hello\r\n\r\n").is_none());
        assert!(parse("GET / SSH-2.0\r\n\r\n").is_none());
        assert!(parse("GET / HTTP/1.1\r\nHost: a\r\n").is_none());
        assert!(parse("").is_none());
        let huge = format!("GET / HTTP/1.1\r\nX: {}\r\n\r\n", "a".repeat(20_000));
        assert!(parse(&huge).is_none());
    }

    #[test]
    fn body() {
        let mut c = io::Cursor::new(b"POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello".to_vec());
        let r = read_head(&mut c).unwrap().unwrap();
        assert_eq!(read_body(&mut c, &r, 10).unwrap(), b"hello");
        let mut c = io::Cursor::new(b"POST / HTTP/1.1\r\nContent-Length: 50\r\n\r\nhello".to_vec());
        let r = read_head(&mut c).unwrap().unwrap();
        assert!(read_body(&mut c, &r, 10).is_none());
    }

    #[test]
    fn no_header_injection() {
        let mut out = Vec::new();
        write_head(&mut out, 200, "text/plain", 0, &[("X-A", "1\r\nSet-Cookie: x".into())]).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("X-A: 1Set-Cookie: x\r\n"));
    }
}
