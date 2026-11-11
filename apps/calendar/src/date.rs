//! Calendar arithmetic without a time zone: dates as day numbers (days since 1970-01-01), wall-clock times as
//! a date plus seconds into the day, ISO week numbers, month lengths.

/// Seconds in a day.
pub const DAY: i64 = 86_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Date {
    pub y: i32,
    pub m: u32,
    pub d: u32,
}

impl Date {
    pub const fn new(y: i32, m: u32, d: u32) -> Date {
        Date { y, m, d }
    }

    /// The date `n` days after 1970-01-01 (negative: before).
    pub fn from_days(n: i64) -> Date {
        // Howard Hinnant's civil_from_days
        let z = n + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        Date { y: (if m <= 2 { y + 1 } else { y }) as i32, m, d }
    }

    /// Days since 1970-01-01. Out-of-range days (31 April) roll over into the next month.
    pub fn days(self) -> i64 {
        let y = if self.m <= 2 { self.y as i64 - 1 } else { self.y as i64 };
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let m = self.m as i64;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + self.d as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// 0 = Monday … 6 = Sunday.
    pub fn weekday(self) -> u32 {
        weekday_of(self.days())
    }

    pub fn add_days(self, n: i64) -> Date {
        Date::from_days(self.days() + n)
    }

    /// Same day `n` months later; the day is kept even if the month is shorter (check with `valid`).
    pub fn add_months_raw(self, n: i32) -> Date {
        let total = self.y * 12 + (self.m as i32 - 1) + n;
        Date { y: total.div_euclid(12), m: total.rem_euclid(12) as u32 + 1, d: self.d }
    }

    /// Same day `n` months later, clamped to the month's last day (31 January + 1 month = 28/29 February).
    pub fn add_months(self, n: i32) -> Date {
        let mut r = self.add_months_raw(n);
        r.d = r.d.min(days_in_month(r.y, r.m));
        r
    }

    pub fn valid(self) -> bool {
        (1..=12).contains(&self.m) && self.d >= 1 && self.d <= days_in_month(self.y, self.m)
    }

    pub fn first_of_month(self) -> Date {
        Date { d: 1, ..self }
    }

    /// ISO 8601 week number (weeks start on Monday, week 1 holds the first Thursday).
    pub fn iso_week(self) -> u32 {
        let n = self.days();
        let thursday = n - self.weekday() as i64 + 3;
        let y = Date::from_days(thursday).y;
        let jan1 = Date::new(y, 1, 1).days();
        ((thursday - jan1) / 7 + 1) as u32
    }

    /// The start (with `first` 0 = Monday … 6 = Sunday) of the week holding this date.
    pub fn week_start(self, first: u32) -> Date {
        let back = (self.weekday() + 7 - first) % 7;
        self.add_days(-(back as i64))
    }

    /// "2026-11-11".
    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.y, self.m, self.d)
    }

    /// "20261111" (iCalendar).
    pub fn compact(self) -> String {
        format!("{:04}{:02}{:02}", self.y, self.m, self.d)
    }

    /// Parses "2026-11-11" or "20261111".
    pub fn parse_iso(s: &str) -> Option<Date> {
        let s = s.trim();
        let d = if s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) {
            Date::new(s[..4].parse().ok()?, s[4..6].parse().ok()?, s[6..8].parse().ok()?)
        } else {
            let mut it = s.split('-');
            let d = Date::new(it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
            if it.next().is_some() {
                return None;
            }
            d
        };
        d.valid().then_some(d)
    }
}

pub fn weekday_of(days: i64) -> u32 {
    // 1970-01-01 was a Thursday (3)
    (days + 3).rem_euclid(7) as u32
}

pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 0,
    }
}

/// A wall-clock moment: a date and seconds since its midnight (no zone).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Wall {
    pub date: Date,
    pub secs: u32,
}

impl Wall {
    pub fn new(date: Date, h: u32, min: u32, s: u32) -> Wall {
        Wall { date, secs: h * 3600 + min * 60 + s }
    }
    /// As if the zone were UTC: seconds since 1970 (for comparing and adding).
    pub fn naive(self) -> i64 {
        self.date.days() * DAY + self.secs as i64
    }
    pub fn from_naive(t: i64) -> Wall {
        Wall { date: Date::from_days(t.div_euclid(DAY)), secs: t.rem_euclid(DAY) as u32 }
    }
    pub fn hour(self) -> u32 {
        self.secs / 3600
    }
    pub fn minute(self) -> u32 {
        self.secs / 60 % 60
    }
    pub fn second(self) -> u32 {
        self.secs % 60
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for n in -800_000..800_000i64 {
            if n % 997 != 0 {
                continue;
            }
            assert_eq!(Date::from_days(n).days(), n);
        }
        assert_eq!(Date::new(1970, 1, 1).days(), 0);
        assert_eq!(Date::new(2000, 3, 1).days(), 11_017);
        assert_eq!(Date::from_days(20_768), Date::new(2026, 11, 11));
    }

    #[test]
    fn weekdays_and_weeks() {
        assert_eq!(Date::new(2026, 11, 11).weekday(), 2); // Wednesday
        assert_eq!(Date::new(2026, 11, 11).week_start(0), Date::new(2026, 11, 9));
        assert_eq!(Date::new(2026, 11, 11).week_start(6), Date::new(2026, 11, 8));
        assert_eq!(Date::new(2026, 1, 1).iso_week(), 1);
        assert_eq!(Date::new(2027, 1, 1).iso_week(), 53);
        assert_eq!(Date::new(2024, 12, 30).iso_week(), 1);
        assert_eq!(Date::new(2026, 11, 11).iso_week(), 46);
    }

    #[test]
    fn months() {
        assert_eq!(Date::new(2026, 1, 31).add_months(1), Date::new(2026, 2, 28));
        assert_eq!(Date::new(2024, 1, 31).add_months(1), Date::new(2024, 2, 29));
        assert_eq!(Date::new(2026, 11, 30).add_months(3), Date::new(2027, 2, 28));
        assert_eq!(Date::new(2026, 1, 15).add_months(-2), Date::new(2025, 11, 15));
        assert!(!Date::new(2026, 4, 31).valid());
        assert_eq!(Date::parse_iso("2026-02-29"), None);
        assert_eq!(Date::parse_iso("20240229"), Some(Date::new(2024, 2, 29)));
    }
}
