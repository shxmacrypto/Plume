//! Time zones through Windows: events are stored as UTC instants with the TZID they were written in, and shown
//! in the PC's zone. Calendars from elsewhere name their zones the IANA way ("Europe/Paris"), Outlook the Windows
//! way ("Romance Standard Time"): both are understood, through the table below for the common IANA names.

use crate::date::{DAY, Date, Wall};
use std::{collections::HashMap, sync::Mutex};
use windows::Win32::{
    Foundation::SYSTEMTIME,
    System::Time::{
        DYNAMIC_TIME_ZONE_INFORMATION, EnumDynamicTimeZoneInformation, GetDynamicTimeZoneInformation, GetTimeZoneInformationForYear,
        SystemTimeToTzSpecificLocalTimeEx, TIME_ZONE_INFORMATION, TzSpecificLocalTimeToSystemTimeEx,
    },
};

#[derive(Clone)]
pub enum Zone {
    Utc,
    /// A fixed offset east of UTC, in seconds.
    Fixed(i32),
    /// A Windows time zone (with its daylight saving rules, year by year).
    Win(Box<DYNAMIC_TIME_ZONE_INFORMATION>),
}

impl std::fmt::Debug for Zone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Zone::Utc => write!(f, "UTC"),
            Zone::Fixed(o) => write!(f, "Fixed({o})"),
            Zone::Win(d) => write!(f, "Win({})", wide(&d.TimeZoneKeyName)),
        }
    }
}

fn wide(s: &[u16]) -> String {
    let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..n])
}

fn st_of(w: Wall) -> SYSTEMTIME {
    let y = w.date.y.clamp(1601, 30_000);
    SYSTEMTIME {
        wYear: y as u16,
        wMonth: w.date.m as u16,
        wDayOfWeek: 0,
        wDay: w.date.d as u16,
        wHour: w.hour() as u16,
        wMinute: w.minute() as u16,
        wSecond: w.second() as u16,
        wMilliseconds: 0,
    }
}

fn wall_of(st: &SYSTEMTIME) -> Wall {
    Wall::new(Date::new(st.wYear as i32, st.wMonth as u32, st.wDay as u32), st.wHour as u32, st.wMinute as u32, st.wSecond as u32)
}

/// Common IANA names and their Windows zone.
const IANA: &[(&str, &str)] = &[
    ("Europe/Paris", "Romance Standard Time"),
    ("Europe/Brussels", "Romance Standard Time"),
    ("Europe/Madrid", "Romance Standard Time"),
    ("Europe/Copenhagen", "Romance Standard Time"),
    ("Europe/Berlin", "W. Europe Standard Time"),
    ("Europe/Monaco", "W. Europe Standard Time"),
    ("Europe/Luxembourg", "W. Europe Standard Time"),
    ("Europe/Amsterdam", "W. Europe Standard Time"),
    ("Europe/Rome", "W. Europe Standard Time"),
    ("Europe/Vienna", "W. Europe Standard Time"),
    ("Europe/Zurich", "W. Europe Standard Time"),
    ("Europe/Stockholm", "W. Europe Standard Time"),
    ("Europe/Oslo", "W. Europe Standard Time"),
    ("Europe/Prague", "Central Europe Standard Time"),
    ("Europe/Budapest", "Central Europe Standard Time"),
    ("Europe/Belgrade", "Central Europe Standard Time"),
    ("Europe/Warsaw", "Central European Standard Time"),
    ("Europe/Zagreb", "Central European Standard Time"),
    ("Europe/London", "GMT Standard Time"),
    ("Europe/Dublin", "GMT Standard Time"),
    ("Europe/Lisbon", "GMT Standard Time"),
    ("Atlantic/Canary", "GMT Standard Time"),
    ("Europe/Athens", "GTB Standard Time"),
    ("Europe/Bucharest", "GTB Standard Time"),
    ("Europe/Helsinki", "FLE Standard Time"),
    ("Europe/Kiev", "FLE Standard Time"),
    ("Europe/Kyiv", "FLE Standard Time"),
    ("Europe/Istanbul", "Turkey Standard Time"),
    ("Europe/Moscow", "Russian Standard Time"),
    ("Africa/Casablanca", "Morocco Standard Time"),
    ("Africa/Algiers", "W. Central Africa Standard Time"),
    ("Africa/Lagos", "W. Central Africa Standard Time"),
    ("Africa/Tunis", "W. Central Africa Standard Time"),
    ("Africa/Abidjan", "Greenwich Standard Time"),
    ("Africa/Dakar", "Greenwich Standard Time"),
    ("Africa/Cairo", "Egypt Standard Time"),
    ("Africa/Johannesburg", "South Africa Standard Time"),
    ("Africa/Nairobi", "E. Africa Standard Time"),
    ("Indian/Reunion", "Mauritius Standard Time"),
    ("Indian/Mauritius", "Mauritius Standard Time"),
    ("Asia/Dubai", "Arabian Standard Time"),
    ("Asia/Jerusalem", "Israel Standard Time"),
    ("Asia/Beirut", "Middle East Standard Time"),
    ("Asia/Riyadh", "Arab Standard Time"),
    ("Asia/Tehran", "Iran Standard Time"),
    ("Asia/Karachi", "Pakistan Standard Time"),
    ("Asia/Kolkata", "India Standard Time"),
    ("Asia/Calcutta", "India Standard Time"),
    ("Asia/Dhaka", "Bangladesh Standard Time"),
    ("Asia/Bangkok", "SE Asia Standard Time"),
    ("Asia/Ho_Chi_Minh", "SE Asia Standard Time"),
    ("Asia/Jakarta", "SE Asia Standard Time"),
    ("Asia/Shanghai", "China Standard Time"),
    ("Asia/Hong_Kong", "China Standard Time"),
    ("Asia/Singapore", "Singapore Standard Time"),
    ("Asia/Taipei", "Taipei Standard Time"),
    ("Asia/Tokyo", "Tokyo Standard Time"),
    ("Asia/Seoul", "Korea Standard Time"),
    ("Australia/Perth", "W. Australia Standard Time"),
    ("Australia/Adelaide", "Cen. Australia Standard Time"),
    ("Australia/Brisbane", "E. Australia Standard Time"),
    ("Australia/Sydney", "AUS Eastern Standard Time"),
    ("Australia/Melbourne", "AUS Eastern Standard Time"),
    ("Pacific/Noumea", "Central Pacific Standard Time"),
    ("Pacific/Auckland", "New Zealand Standard Time"),
    ("Pacific/Tahiti", "Hawaiian Standard Time"),
    ("Pacific/Honolulu", "Hawaiian Standard Time"),
    ("America/Anchorage", "Alaskan Standard Time"),
    ("America/Los_Angeles", "Pacific Standard Time"),
    ("America/Vancouver", "Pacific Standard Time"),
    ("America/Phoenix", "US Mountain Standard Time"),
    ("America/Denver", "Mountain Standard Time"),
    ("America/Edmonton", "Mountain Standard Time"),
    ("America/Chicago", "Central Standard Time"),
    ("America/Winnipeg", "Central Standard Time"),
    ("America/Mexico_City", "Central Standard Time (Mexico)"),
    ("America/New_York", "Eastern Standard Time"),
    ("America/Toronto", "Eastern Standard Time"),
    ("America/Detroit", "Eastern Standard Time"),
    ("America/Halifax", "Atlantic Standard Time"),
    ("America/Martinique", "SA Western Standard Time"),
    ("America/Guadeloupe", "SA Western Standard Time"),
    ("America/Cayenne", "SA Eastern Standard Time"),
    ("America/St_Johns", "Newfoundland Standard Time"),
    ("America/Sao_Paulo", "E. South America Standard Time"),
    ("America/Argentina/Buenos_Aires", "Argentina Standard Time"),
    ("America/Buenos_Aires", "Argentina Standard Time"),
    ("America/Bogota", "SA Pacific Standard Time"),
    ("America/Lima", "SA Pacific Standard Time"),
    ("America/Santiago", "Pacific SA Standard Time"),
    ("Atlantic/Reykjavik", "Greenwich Standard Time"),
];

static CACHE: Mutex<Option<HashMap<String, Option<Zone>>>> = Mutex::new(None);

// SAFETY: a DYNAMIC_TIME_ZONE_INFORMATION is plain data (numbers and fixed arrays).
unsafe impl Send for Zone {}

fn by_windows_name(name: &str) -> Option<Zone> {
    let want = name.to_ascii_lowercase();
    for i in 0..1000 {
        let mut d = DYNAMIC_TIME_ZONE_INFORMATION::default();
        if unsafe { EnumDynamicTimeZoneInformation(i, &mut d) } != 0 {
            break;
        }
        if wide(&d.TimeZoneKeyName).to_ascii_lowercase() == want {
            return Some(Zone::Win(Box::new(d)));
        }
    }
    None
}

/// "+0100", "-0530", "+01:00" → seconds east of UTC.
pub fn parse_offset(s: &str) -> Option<i32> {
    let s = s.trim();
    let (sign, rest) = match s.as_bytes().first()? {
        b'+' => (1, &s[1..]),
        b'-' => (-1, &s[1..]),
        _ => return None,
    };
    let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 {
        return None;
    }
    let h: i32 = digits[..2].parse().ok()?;
    let m: i32 = digits[2..4].parse().ok()?;
    let sec: i32 = if digits.len() >= 6 { digits[4..6].parse().ok()? } else { 0 };
    Some(sign * (h * 3600 + m * 60 + sec))
}

impl Zone {
    /// The zone of a TZID, if Windows knows it (directly, or through an IANA name).
    pub fn find(tzid: &str) -> Option<Zone> {
        let id = tzid.trim().trim_matches('"');
        if id.is_empty() {
            return None;
        }
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let map = cache.get_or_insert_with(HashMap::new);
        if let Some(z) = map.get(id) {
            return z.clone();
        }
        let z = Self::lookup(id);
        map.insert(id.to_string(), z.clone());
        z
    }

    fn lookup(id: &str) -> Option<Zone> {
        let lower = id.to_ascii_lowercase();
        if matches!(lower.as_str(), "utc" | "z" | "gmt" | "etc/utc" | "etc/gmt" | "etc/zulu" | "zulu" | "universal") {
            return Some(Zone::Utc);
        }
        // "/mozilla.org/20050126_1/Europe/Paris", "/Europe/Paris": the IANA name is at the end
        let mut tail = id;
        for (k, _) in id.match_indices('/') {
            let rest = &id[k + 1..];
            if IANA.iter().any(|(i, _)| i.eq_ignore_ascii_case(rest)) {
                tail = rest;
                break;
            }
        }
        if let Some((_, win)) = IANA.iter().find(|(i, _)| i.eq_ignore_ascii_case(tail)) {
            return by_windows_name(win);
        }
        if let Some(rest) = lower.strip_prefix("etc/gmt") {
            // POSIX sign: Etc/GMT-2 is two hours east
            return rest.parse::<i32>().ok().map(|h| Zone::Fixed(-h * 3600));
        }
        if let Some(o) = lower.strip_prefix("utc").or_else(|| lower.strip_prefix("gmt")).and_then(parse_offset) {
            return Some(Zone::Fixed(o));
        }
        by_windows_name(id)
    }

    /// The PC's zone (PLUME_CALENDAR_TZ, a Windows or IANA name, replaces it for tests).
    pub fn local() -> Zone {
        thread_local! { static LOCAL: std::cell::RefCell<Option<Zone>> = const { std::cell::RefCell::new(None) }; }
        LOCAL.with(|l| {
            l.borrow_mut()
                .get_or_insert_with(|| {
                    if let Some(z) = std::env::var("PLUME_CALENDAR_TZ").ok().and_then(|n| Zone::find(&n)) {
                        return z;
                    }
                    let mut d = DYNAMIC_TIME_ZONE_INFORMATION::default();
                    unsafe { GetDynamicTimeZoneInformation(&mut d) };
                    Zone::Win(Box::new(d))
                })
                .clone()
        })
    }

    /// The Windows name ("Romance Standard Time"), for Windows zones.
    pub fn windows_name(&self) -> Option<String> {
        match self {
            Zone::Win(d) => Some(wide(&d.TimeZoneKeyName)).filter(|n| !n.is_empty()),
            _ => None,
        }
    }

    /// The name written in .ics files: IANA when the table has one, else the Windows name.
    pub fn tzid(&self) -> String {
        match self {
            Zone::Utc => "UTC".into(),
            Zone::Fixed(o) => format!("UTC{}{:02}{:02}", if *o < 0 { '-' } else { '+' }, o.abs() / 3600, o.abs() / 60 % 60),
            Zone::Win(_) => {
                let win = self.windows_name().unwrap_or_default();
                IANA.iter().find(|(_, w)| *w == win).map(|(i, _)| i.to_string()).unwrap_or(win)
            }
        }
    }

    /// Wall-clock time in this zone at the UTC instant `t` (seconds since 1970).
    pub fn to_wall(&self, t: i64) -> Wall {
        match self {
            Zone::Utc => Wall::from_naive(t),
            Zone::Fixed(o) => Wall::from_naive(t + *o as i64),
            Zone::Win(d) => {
                let utc = st_of(Wall::from_naive(t));
                let mut out = SYSTEMTIME::default();
                if unsafe { SystemTimeToTzSpecificLocalTimeEx(Some(&**d), &utc, &mut out) }.is_ok() {
                    wall_of(&out)
                } else {
                    Wall::from_naive(t - d.Bias as i64 * 60)
                }
            }
        }
    }

    /// The UTC instant of a wall-clock time in this zone.
    pub fn to_utc(&self, w: Wall) -> i64 {
        match self {
            Zone::Utc => w.naive(),
            Zone::Fixed(o) => w.naive() - *o as i64,
            Zone::Win(d) => {
                let local = st_of(w);
                let mut out = SYSTEMTIME::default();
                if unsafe { TzSpecificLocalTimeToSystemTimeEx(Some(&**d), &local, &mut out) }.is_ok() {
                    wall_of(&out).naive()
                } else {
                    w.naive() + d.Bias as i64 * 60
                }
            }
        }
    }

    /// Seconds east of UTC at the instant `t`.
    #[cfg(test)]
    pub fn offset_at(&self, t: i64) -> i64 {
        self.to_wall(t).naive() - t
    }

    /// A VTIMEZONE block for .ics files (rules of `year`), or None for UTC.
    pub fn vtimezone(&self, year: i32) -> Option<String> {
        let tzid = self.tzid();
        let fmt_off = |sec: i32| format!("{}{:02}{:02}", if sec < 0 { '-' } else { '+' }, sec.abs() / 3600, sec.abs() / 60 % 60);
        match self {
            Zone::Utc => None,
            Zone::Fixed(o) => Some(format!(
                "BEGIN:VTIMEZONE\r\nTZID:{tzid}\r\nBEGIN:STANDARD\r\nDTSTART:19700101T000000\r\nTZOFFSETFROM:{0}\r\nTZOFFSETTO:{0}\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n",
                fmt_off(*o)
            )),
            Zone::Win(d) => {
                let mut tzi = TIME_ZONE_INFORMATION::default();
                if unsafe { GetTimeZoneInformationForYear(year.clamp(1601, 30_000) as u16, Some(&**d), &mut tzi) }.is_err() {
                    return None;
                }
                let std_off = -(tzi.Bias + tzi.StandardBias) * 60;
                let dst_off = -(tzi.Bias + tzi.DaylightBias) * 60;
                let mut s = format!("BEGIN:VTIMEZONE\r\nTZID:{tzid}\r\n");
                if tzi.StandardDate.wMonth == 0 || tzi.DaylightDate.wMonth == 0 {
                    s += &format!(
                        "BEGIN:STANDARD\r\nDTSTART:19700101T000000\r\nTZOFFSETFROM:{0}\r\nTZOFFSETTO:{0}\r\nEND:STANDARD\r\n",
                        fmt_off(-tzi.Bias * 60)
                    );
                } else {
                    let rule = |st: &SYSTEMTIME| {
                        let n = if st.wDay >= 5 { -1 } else { st.wDay as i32 };
                        let day = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"][st.wDayOfWeek as usize % 7];
                        (format!("19700101T{:02}{:02}00", st.wHour, st.wMinute), format!("FREQ=YEARLY;BYMONTH={};BYDAY={n}{day}", st.wMonth))
                    };
                    let (sd, sr) = rule(&tzi.StandardDate);
                    let (dd, dr) = rule(&tzi.DaylightDate);
                    s += &format!(
                        "BEGIN:STANDARD\r\nDTSTART:{sd}\r\nRRULE:{sr}\r\nTZOFFSETFROM:{}\r\nTZOFFSETTO:{}\r\nEND:STANDARD\r\n",
                        fmt_off(dst_off),
                        fmt_off(std_off)
                    );
                    s += &format!(
                        "BEGIN:DAYLIGHT\r\nDTSTART:{dd}\r\nRRULE:{dr}\r\nTZOFFSETFROM:{}\r\nTZOFFSETTO:{}\r\nEND:DAYLIGHT\r\n",
                        fmt_off(std_off),
                        fmt_off(dst_off)
                    );
                }
                s += "END:VTIMEZONE\r\n";
                Some(s)
            }
        }
    }
}

/// Seconds since 1970, UTC.
pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Midnight (UTC instant) starting `date` in `zone`.
pub fn day_start(zone: &Zone, date: Date) -> i64 {
    zone.to_utc(Wall { date, secs: 0 })
}

/// The date at instant `t` in `zone`.
pub fn date_at(zone: &Zone, t: i64) -> Date {
    zone.to_wall(t).date
}

/// An all-day date stored as an instant (UTC midnight), back to its date.
pub fn floating_date(t: i64) -> Date {
    Date::from_days(t.div_euclid(DAY))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paris_offsets_and_dst() {
        let z = Zone::find("Europe/Paris").expect("Windows knows Paris");
        assert_eq!(z.tzid(), "Europe/Paris");
        // winter: UTC+1, summer: UTC+2
        let jan = z.to_utc(Wall::new(Date::new(2026, 1, 15), 9, 0, 0));
        assert_eq!(Wall::from_naive(jan), Wall::new(Date::new(2026, 1, 15), 8, 0, 0));
        let jul = z.to_utc(Wall::new(Date::new(2026, 7, 15), 9, 0, 0));
        assert_eq!(Wall::from_naive(jul), Wall::new(Date::new(2026, 7, 15), 7, 0, 0));
        assert_eq!(z.to_wall(jul), Wall::new(Date::new(2026, 7, 15), 9, 0, 0));
        assert_eq!(z.offset_at(jul), 7200);
        // the change on 25 October 2026 at 03:00 local
        let before = z.to_utc(Wall::new(Date::new(2026, 10, 24), 12, 0, 0));
        let after = z.to_utc(Wall::new(Date::new(2026, 10, 26), 12, 0, 0));
        assert_eq!(z.offset_at(before), 7200);
        assert_eq!(z.offset_at(after), 3600);
        let vt = z.vtimezone(2026).unwrap();
        assert!(vt.contains("BYMONTH=10;BYDAY=-1SU"), "{vt}");
        assert!(vt.contains("TZOFFSETTO:+0200"));
    }

    #[test]
    fn names() {
        assert!(matches!(Zone::find("UTC"), Some(Zone::Utc)));
        assert!(matches!(Zone::find("Etc/GMT-2"), Some(Zone::Fixed(7200))));
        assert_eq!(Zone::find("Romance Standard Time").unwrap().tzid(), "Europe/Paris");
        assert_eq!(Zone::find("/mozilla.org/20050126_1/America/New_York").unwrap().windows_name().as_deref(), Some("Eastern Standard Time"));
        assert!(Zone::find("Nowhere/Atlantis").is_none());
        assert_eq!(parse_offset("-0530"), Some(-19_800));
        let ny = Zone::find("America/New_York").unwrap();
        assert_eq!(ny.to_wall(Date::new(2026, 3, 9).days() * DAY + 12 * 3600).hour(), 8); // EDT, UTC-4
    }
}
