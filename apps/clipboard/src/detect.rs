//! Sensitive copies: passwords, IP addresses, keys and tokens. They are kept masked, sealed and only for a short
//! time. The rules are meant to catch what looks like a secret while leaving words, code, links, paths and file
//! names alone.

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Secret {
    Password,
    Ip,
    Key,
}

/// Which kinds are looked for (the settings).
#[derive(Clone, Copy, Debug)]
pub struct Wanted {
    pub passwords: bool,
    pub ips: bool,
    pub keys: bool,
}

impl Wanted {
    #[cfg(test)]
    pub const ALL: Wanted = Wanted { passwords: true, ips: true, keys: true };

    fn allows(&self, s: Secret) -> bool {
        match s {
            Secret::Password => self.passwords,
            Secret::Ip => self.ips,
            Secret::Key => self.keys,
        }
    }
}

/// What kind of secret a copied text is, if it looks like one.
pub fn detect(text: &str, want: Wanted) -> Option<Secret> {
    let found = if is_key(text) {
        Some(Secret::Key)
    } else if is_ip(text) {
        Some(Secret::Ip)
    } else if is_password(text) {
        Some(Secret::Password)
    } else {
        None
    };
    found.filter(|s| want.allows(*s))
}

/// An IPv4 or IPv6 address, alone or with a port ("192.168.1.20", "10.0.0.1:8080", "fe80::1", "[::1]:443").
pub fn is_ip(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || t.len() > 64 || t.chars().any(char::is_whitespace) {
        return false;
    }
    if let Ok(a) = t.parse::<SocketAddr>() {
        return !a.ip().is_unspecified();
    }
    match t.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => !v4.is_unspecified(),
        // "::" and other bare colons are not worth hiding; an address has at least one hex digit
        Ok(IpAddr::V6(v6)) => !v6.is_unspecified() && t.chars().any(|c| c.is_ascii_hexdigit()),
        Err(_) => false,
    }
}

/// Characters of a token (key, JWT segment).
fn token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '=' | '+' | '/')
}

/// A key or a token: a private key block, a JWT, or one of the well-known prefixes of cloud and API keys.
pub fn is_key(text: &str) -> bool {
    let t = text.trim();
    if t.contains("-----BEGIN") && t.contains("PRIVATE KEY-----") {
        return true;
    }
    if t.is_empty() || t.len() > 4096 || t.chars().any(char::is_whitespace) {
        return false;
    }
    // JWT: three base64url parts, the first one a JSON header ("eyJ")
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() == 3
        && t.starts_with("eyJ")
        && parts.iter().all(|p| p.len() >= 4 && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        && t.len() >= 30
    {
        return true;
    }
    // (prefix, shortest rest)
    const PREFIXES: [(&str, usize); 22] = [
        ("sk-", 20),
        ("sk_live_", 16),
        ("sk_test_", 16),
        ("rk_live_", 16),
        ("rk_test_", 16),
        ("ghp_", 30),
        ("gho_", 30),
        ("ghu_", 30),
        ("ghs_", 30),
        ("ghr_", 30),
        ("github_pat_", 30),
        ("glpat-", 18),
        ("xoxb-", 16),
        ("xoxp-", 16),
        ("xoxa-", 16),
        ("xoxs-", 16),
        ("xapp-", 16),
        ("AIza", 30),
        ("ya29.", 20),
        ("hf_", 30),
        ("npm_", 30),
        ("SG.", 30),
    ];
    for (p, min) in PREFIXES {
        if let Some(rest) = t.strip_prefix(p)
            && rest.len() >= min
            && rest.chars().all(token_char)
        {
            return true;
        }
    }
    // AWS access keys: AKIA / ASIA followed by 16 capitals or digits
    if (t.starts_with("AKIA") || t.starts_with("ASIA")) && t.len() == 20 && t[4..].chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        return true;
    }
    false
}

/// Symbols that passwords use and code rarely does on its own.
const STRONG: &str = "!@#$%^&*?~+=|";

/// One word of letters, possibly in camelCase or PascalCase, with digits only at its end ("getUser2", "HTTPServer").
fn wordy(s: &str) -> bool {
    let body = s.trim_end_matches(|c: char| c.is_ascii_digit());
    !body.is_empty() && body.chars().all(|c| c.is_ascii_alphabetic())
}

/// A name from code or a file name: words joined by `_`, `.`, `-` or `::` ("snake_case_v2", "Report2026.pdf").
fn identifier_like(s: &str) -> bool {
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | ':')) {
        return false;
    }
    let parts: Vec<&str> = s.split(['_', '.', '-', ':']).filter(|p| !p.is_empty()).collect();
    !parts.is_empty() && parts.iter().all(|p| wordy(p) || p.chars().all(|c| c.is_ascii_digit()))
}

/// Something written in a program rather than a password: brackets, quotes, operators.
fn code_like(s: &str) -> bool {
    s.contains(['(', ')', '[', ']', '{', '}', '<', '>', ';', '"', '\'', '`', ','])
        || ["==", "+=", "-=", "=>", "->", "::", "&&", "||", "!="].iter().any(|o| s.contains(o))
}

fn path_like(s: &str) -> bool {
    let b = s.as_bytes();
    s.contains('\\')
        || s.starts_with('/')
        || s.starts_with("~/")
        || s.starts_with("./")
        || (b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
}

fn email_like(s: &str) -> bool {
    match s.split_once('@') {
        Some((user, host)) => !user.is_empty() && host.contains('.') && !host.starts_with('.') && !host.ends_with('.') && !host.contains('@'),
        None => false,
    }
}

/// A password: one token of 8 to 64 characters mixing at least three of lowercase, capitals, digits and
/// symbols, that is not a link, a path, an e-mail address, a name from code or a file name.
pub fn is_password(text: &str) -> bool {
    let t = text.trim();
    let n = t.chars().count();
    if !(8..=64).contains(&n) || t.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    if crate::clip::is_link(t) || t.contains("://") || path_like(t) || email_like(t) || code_like(t) || is_ip(t) {
        return false;
    }
    let lower = t.chars().any(|c| c.is_lowercase());
    let upper = t.chars().any(|c| c.is_uppercase());
    let digit = t.chars().any(|c| c.is_ascii_digit());
    let symbol = t.chars().any(|c| !c.is_alphanumeric());
    let classes = [lower, upper, digit, symbol].iter().filter(|b| **b).count();
    if classes < 3 {
        return false;
    }
    if t.chars().any(|c| STRONG.contains(c)) {
        return true;
    }
    // no password symbol: capitals, small letters and digits all there, and not a name from code
    lower && upper && digit && !identifier_like(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(s: &str) -> Option<Secret> {
        detect(s, Wanted::ALL)
    }

    #[test]
    fn passwords() {
        for s in [
            "Tr0ub4dor&3",
            "x7$Kp!2qLm",
            "Summer2024!",
            "hunter2Hunter2#",
            "P@ssw0rd",
            "aB3xK9pQ2zR7",
            "9fGq-2LmP_x8Rt",
            "Zx8#qL1!",
            "correct-Horse-7battery",
            " qwERTY12$% ",
        ] {
            assert_eq!(kind(s), Some(Secret::Password), "{s}");
        }
    }

    #[test]
    fn not_passwords() {
        for s in [
            // words and sentences
            "hello",
            "Bonjour",
            "Wonderful",
            "Hello world 2026!",
            "Meeting notes",
            "PASSWORD",
            // too short or too long
            "Ab1!",
            &"Ab1!".repeat(20),
            // code
            "getUserById2",
            "HTTPServer2",
            "snake_case_v2",
            "MAX_BUFFER_SIZE",
            "self.Name2",
            "foo(bar)",
            "count+=10",
            "a==B2c",
            "vec![1,2,3]",
            "std::io::Error",
            "Array<String>",
            "x->Next1",
            // links, paths, e-mail, files
            "https://Example.com/Page1",
            "www.Example2.com",
            r"C:\Users\Alex\Doc1.txt",
            "/usr/local/Bin2",
            "~/Projects/App2",
            "alex.martin2@Example.com",
            "Report2026.pdf",
            "Budget_2027-Final.xlsx",
            "v1.2.3-Beta",
            // ids, numbers, dates
            "3F2504E0-4F89-11D3-9A0C-0305E82C3301",
            "3f2504e0-4f89-11d3-9a0c-0305e82c3301",
            "+33612345678",
            "2026-11-11T10:00:00Z",
            "#48213",
            "d41d8cd98f00b204e9800998ecf8427e",
            "1,234,567.89",
        ] {
            assert_eq!(kind(s), None, "{s}");
        }
    }

    #[test]
    fn ips() {
        for s in ["192.168.1.20", "10.0.0.1:8080", " 8.8.8.8 ", "fe80::1", "2001:db8::8a2e:370:7334", "[::1]:443", "::1"] {
            assert_eq!(kind(s), Some(Secret::Ip), "{s}");
        }
        for s in ["1.2.3", "256.1.1.1", "0.0.0.0", "::", "192.168.1.20 and more", "10.0.0.1:99999", "1.2.3.4.5", "v1.2.3.4", "12:30"] {
            assert_ne!(kind(s), Some(Secret::Ip), "{s}");
        }
    }

    #[test]
    fn keys() {
        for s in [
            "sk-proj-AbCdEf1234567890GhIjKl",
            "sk_live_51HxYzAbCdEfGh123456",
            "ghp_1234567890abcdefghijABCDEFGHIJ123456",
            "github_pat_11ABCDEFG0123456789_abcdefghijklmnop",
            "glpat-AbCdEf123456789012",
            "xoxb-123456789012-1234567890123-AbCdEfGh",
            "AKIAIOSFODNN7EXAMPLE",
            "AIzaSyA-1234567890abcdefghijklmnopqrs",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
            "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\n-----END OPENSSH PRIVATE KEY-----",
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA\n-----END RSA PRIVATE KEY-----",
        ] {
            assert_eq!(kind(s), Some(Secret::Key), "{s}");
        }
        for s in [
            "sk-short",
            "ghp_tooshort",
            "AKIA is the prefix",
            "eyJ.not.ajwt",
            "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----",
            "-----BEGIN PUBLIC KEY-----\nMIIB\n-----END PUBLIC KEY-----",
            "sketch-of-the-day",
            "xoxo",
        ] {
            assert_ne!(kind(s), Some(Secret::Key), "{s}");
        }
    }

    #[test]
    fn kinds_can_be_turned_off() {
        let none = Wanted { passwords: false, ips: false, keys: false };
        assert_eq!(detect("x7$Kp!2qLm", none), None);
        assert_eq!(detect("192.168.1.20", Wanted { ips: false, ..Wanted::ALL }), None);
        assert_eq!(detect("AKIAIOSFODNN7EXAMPLE", Wanted { keys: false, ..Wanted::ALL }), None);
        assert_eq!(detect("192.168.1.20", Wanted { passwords: false, keys: false, ips: true }), Some(Secret::Ip));
    }

    #[test]
    fn normal_texts_are_left_alone() {
        for s in [
            "Thanks for the update! Let's meet Thursday at 10:00 to go over the new designs.",
            "fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}",
            "https://github.com/shxmacrypto/Plume",
            "Order #48213 has shipped and should arrive on Monday.",
            "C:\\Users\\Alex\\Documents\\Design brief.pdf",
            "SELECT * FROM users WHERE id = 42;",
            "let x = a + b;",
            "README.md",
            "node_modules",
            "Content-Type",
            "UTF-8",
        ] {
            assert_eq!(kind(s), None, "{s}");
        }
    }
}
