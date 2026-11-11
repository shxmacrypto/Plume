//! Two-factor codes (RFC 6238, time-based one-time passwords): from a base32 secret or an otpauth:// address.

use hmac::{Hmac, Mac};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algo {
    Sha1,
    Sha256,
    Sha512,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Totp {
    pub secret: Zeroizing<Vec<u8>>,
    pub algo: Algo,
    pub digits: u32,
    pub period: u64,
    /// From an otpauth:// address: who issues it, and the account.
    pub issuer: String,
    pub account: String,
}

/// RFC 4648 base32, forgiving: lower case, spaces, dashes and the padding are accepted.
pub fn base32(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u64, 0u32);
    for c in s.chars() {
        let v = match c.to_ascii_uppercase() {
            c @ 'A'..='Z' => c as u64 - 'A' as u64,
            c @ '2'..='7' => c as u64 - '2' as u64 + 26,
            ' ' | '-' | '=' | '\t' => continue,
            _ => return None,
        };
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    (!out.is_empty()).then_some(out)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                Ok(v) => {
                    out.push(v);
                    i += 3;
                    continue;
                }
                Err(_) => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Totp {
    /// A base32 secret, or an otpauth://totp/… address.
    pub fn parse(text: &str) -> Option<Totp> {
        let t = text.trim();
        if t.len() >= 10 && t[..10].eq_ignore_ascii_case("otpauth://") {
            return Self::parse_uri(t);
        }
        Some(Totp { secret: Zeroizing::new(base32(t)?), algo: Algo::Sha1, digits: 6, period: 30, issuer: String::new(), account: String::new() })
    }

    fn parse_uri(uri: &str) -> Option<Totp> {
        let rest = &uri[10..];
        let (kind, rest) = rest.split_once('/')?;
        if !kind.eq_ignore_ascii_case("totp") {
            return None;
        }
        let (label, query) = rest.split_once('?').unwrap_or((rest, ""));
        let label = percent_decode(label);
        let (mut issuer, account) = match label.split_once(':') {
            Some((i, a)) => (i.trim().to_string(), a.trim().to_string()),
            None => (String::new(), label.trim().to_string()),
        };
        let mut t = Totp { secret: Zeroizing::new(Vec::new()), algo: Algo::Sha1, digits: 6, period: 30, issuer: String::new(), account };
        for pair in query.split('&') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let v = percent_decode(v);
            match k.to_ascii_lowercase().as_str() {
                "secret" => t.secret = Zeroizing::new(base32(&v)?),
                "issuer" if !v.is_empty() => issuer = v,
                "algorithm" => {
                    t.algo = match v.to_ascii_uppercase().as_str() {
                        "SHA1" => Algo::Sha1,
                        "SHA256" => Algo::Sha256,
                        "SHA512" => Algo::Sha512,
                        _ => return None,
                    }
                }
                "digits" => t.digits = v.parse().ok().filter(|d| (6..=8).contains(d))?,
                "period" => t.period = v.parse().ok().filter(|p| (1..=300).contains(p))?,
                _ => {}
            }
        }
        t.issuer = issuer;
        (!t.secret.is_empty()).then_some(t)
    }

    /// The code at this Unix time.
    pub fn code_at(&self, unix: u64) -> String {
        let counter = (unix / self.period).to_be_bytes();
        let mac: Vec<u8> = match self.algo {
            Algo::Sha1 => hmac_of::<Hmac<sha1::Sha1>>(&self.secret, &counter),
            Algo::Sha256 => hmac_of::<Hmac<sha2::Sha256>>(&self.secret, &counter),
            Algo::Sha512 => hmac_of::<Hmac<sha2::Sha512>>(&self.secret, &counter),
        };
        let off = (mac[mac.len() - 1] & 0x0f) as usize;
        let bin = u32::from_be_bytes([mac[off] & 0x7f, mac[off + 1], mac[off + 2], mac[off + 3]]);
        let code = bin % 10u32.pow(self.digits);
        format!("{code:0width$}", width = self.digits as usize)
    }

    pub fn now(&self) -> String {
        self.code_at(crate::vault::now())
    }

    /// Seconds before the code changes.
    pub fn left(&self, unix: u64) -> u64 {
        self.period - unix % self.period
    }
}

fn hmac_of<M: Mac + hmac::digest::KeyInit>(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut m = <M as hmac::digest::KeyInit>::new_from_slice(key).expect("hmac accepts any key");
    m.update(msg);
    m.finalize().into_bytes().to_vec()
}

/// "123456" → "123 456"; "12345678" → "1234 5678".
pub fn grouped(code: &str) -> String {
    let half = code.len() / 2;
    format!("{} {}", &code[..half], &code[half..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(secret: &[u8], algo: Algo) -> Totp {
        Totp { secret: Zeroizing::new(secret.to_vec()), algo, digits: 8, period: 30, issuer: String::new(), account: String::new() }
    }

    #[test]
    fn rfc6238_vectors() {
        // RFC 6238, appendix B
        let s1 = with(b"12345678901234567890", Algo::Sha1);
        let s256 = with(b"12345678901234567890123456789012", Algo::Sha256);
        let s512 = with(b"1234567890123456789012345678901234567890123456789012345678901234", Algo::Sha512);
        let table: [(u64, &str, &str, &str); 6] = [
            (59, "94287082", "46119246", "90693936"),
            (1111111109, "07081804", "68084774", "25091201"),
            (1111111111, "14050471", "67062674", "99943326"),
            (1234567890, "89005924", "91819424", "93441116"),
            (2000000000, "69279037", "90698825", "38618901"),
            (20000000000, "65353130", "77737706", "47863826"),
        ];
        for (t, a, b, c) in table {
            assert_eq!(s1.code_at(t), a, "sha1 at {t}");
            assert_eq!(s256.code_at(t), b, "sha256 at {t}");
            assert_eq!(s512.code_at(t), c, "sha512 at {t}");
        }
    }

    #[test]
    fn base32_and_uri() {
        // "12345678901234567890" in base32
        let b = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        assert_eq!(base32(b).unwrap(), b"12345678901234567890");
        assert_eq!(base32("gezd gnbv-gy3t qojq gezd gnbv gy3t qojq").unwrap(), b"12345678901234567890");
        assert!(base32("not base32!").is_none());
        let t = Totp::parse(b).unwrap();
        assert_eq!((t.digits, t.period, t.algo), (6, 30, Algo::Sha1));
        assert_eq!(t.code_at(59), "287082");

        let uri = "otpauth://totp/Example%20Co:alice%40example.com?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Example+Co&digits=8&algorithm=SHA1&period=30";
        let t = Totp::parse(uri).unwrap();
        assert_eq!(t.issuer, "Example Co");
        assert_eq!(t.account, "alice@example.com");
        assert_eq!(t.code_at(1111111109), "07081804");
        assert_eq!(t.left(1111111109), 1);
        assert!(Totp::parse("otpauth://hotp/x?secret=GEZDGNBV&counter=1").is_none());
        assert!(Totp::parse("otpauth://totp/x?secret=GEZDGNBV&digits=12").is_none());
        assert_eq!(grouped("123456"), "123 456");
    }
}
