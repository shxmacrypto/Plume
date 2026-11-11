//! HTTP through WinINet (Windows' proxy settings, no cookies, no cache). Plume Calendar goes online only to download
//! the calendars the user subscribed to (an .ics address). Copied from Plume Music's helper.

use windows::{
    Win32::Networking::WinInet::*,
    core::{HSTRING, PCWSTR},
};

pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
/// Largest answer accepted (a busy calendar is a few MB).
const MAX: usize = 24 * 1024 * 1024;

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

/// (status, body). `headers`: "Name: value" lines.
pub fn request(method: &str, url: &str, headers: &[&str], body: Option<&[u8]>) -> Result<(u16, Vec<u8>), String> {
    let (secure, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err("bad url".into());
    };
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => (h, p.parse::<u16>().unwrap_or(if secure { 443 } else { 80 })),
        _ => (hostport, if secure { 443 } else { 80 }),
    };
    unsafe {
        let net = H(InternetOpenW(&HSTRING::from(UA), INTERNET_OPEN_TYPE_PRECONFIG.0, PCWSTR::null(), PCWSTR::null(), 0));
        if net.0.is_null() {
            return Err("network unavailable".into());
        }
        for opt in [INTERNET_OPTION_CONNECT_TIMEOUT, INTERNET_OPTION_SEND_TIMEOUT, INTERNET_OPTION_RECEIVE_TIMEOUT] {
            let ms = 15_000u32;
            let _ = InternetSetOptionW(Some(net.0 as *const _), opt, Some(&ms as *const u32 as *const _), 4);
        }
        let conn = H(InternetConnectW(net.0, &HSTRING::from(host), port, PCWSTR::null(), PCWSTR::null(), INTERNET_SERVICE_HTTP, 0, None));
        if conn.0.is_null() {
            return Err("cannot connect".into());
        }
        let mut flags = INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_NO_UI | INTERNET_FLAG_NO_COOKIES;
        if secure {
            flags |= INTERNET_FLAG_SECURE;
        }
        let req = H(HttpOpenRequestW(conn.0, &HSTRING::from(method), &HSTRING::from(path), PCWSTR::null(), PCWSTR::null(), None, flags, None));
        if req.0.is_null() {
            return Err("cannot open request".into());
        }
        let mut all = String::new();
        for h in headers {
            all.push_str(h);
            all.push_str("\r\n");
        }
        let wh: Vec<u16> = all.encode_utf16().collect();
        let sent = match body {
            Some(b) => HttpSendRequestW(req.0, Some(&wh), Some(b.as_ptr() as *const _), b.len() as u32),
            None => HttpSendRequestW(req.0, Some(&wh), None, 0),
        };
        sent.map_err(|e| format!("request failed: {}", e.message()))?;
        let (mut status, mut len) = (0u32, 4u32);
        let _ = HttpQueryInfoW(req.0, HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER, Some(&mut status as *mut u32 as _), &mut len, None);
        let (mut out, mut buf) = (Vec::new(), vec![0u8; 32 * 1024]);
        loop {
            let mut n = 0u32;
            InternetReadFile(req.0, buf.as_mut_ptr() as _, buf.len() as u32, &mut n).map_err(|e| e.message().to_string())?;
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n as usize]);
            if out.len() > MAX {
                return Err("answer too large".into());
            }
        }
        Ok((status as u16, out))
    }
}

pub fn get(url: &str) -> Result<Vec<u8>, String> {
    match request("GET", url, &["Accept: text/calendar, */*"], None)? {
        (200, b) => Ok(b),
        (s, _) => Err(format!("HTTP {s}")),
    }
}

pub fn get_text(url: &str) -> Result<String, String> {
    get(url).map(|b| String::from_utf8_lossy(&b).into_owned())
}
