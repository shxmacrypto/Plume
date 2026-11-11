//! Dates au format de Windows (réglages régionaux de l'utilisateur), gardées en mémoire à la minute près.

use std::{cell::RefCell, collections::HashMap, rc::Rc};
use windows::{
    Win32::{
        Foundation::{FILETIME, SYSTEMTIME},
        Globalization::{DATE_SHORTDATE, GetDateFormatEx, GetTimeFormatEx, TIME_NOSECONDS},
        System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime},
    },
    core::PCWSTR,
};

#[derive(Default)]
pub struct Dates {
    cache: RefCell<HashMap<u64, Rc<str>>>,
}

impl Dates {
    /// Date et heure locales d'un FILETIME (centaines de nanosecondes depuis 1601, UTC) : « 2026-11-10 05:33 ».
    pub fn get(&self, ft: u64) -> Rc<str> {
        if ft == 0 {
            return Rc::from("");
        }
        let key = ft / 600_000_000;
        if let Some(s) = self.cache.borrow().get(&key) {
            return s.clone();
        }
        let s: Rc<str> = Rc::from(format(ft).as_str());
        let mut cache = self.cache.borrow_mut();
        if cache.len() > 20_000 {
            cache.clear();
        }
        cache.insert(key, s.clone());
        s
    }
}

/// Date et heure locales d'un FILETIME sous une forme qui ne dépend pas des réglages régionaux, pour les
/// fichiers exportés : « 2026-11-10 07:00:00 ». Vide si la date est inconnue.
pub fn iso(ft: u64) -> String {
    unsafe {
        let f = FILETIME { dwLowDateTime: ft as u32, dwHighDateTime: (ft >> 32) as u32 };
        let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
        if ft == 0 || FileTimeToSystemTime(&f, &mut utc).is_err() {
            return String::new();
        }
        if SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_err() {
            local = utc;
        }
        format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute, local.wSecond)
    }
}

fn format(ft: u64) -> String {
    unsafe {
        let f = FILETIME { dwLowDateTime: ft as u32, dwHighDateTime: (ft >> 32) as u32 };
        let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
        if FileTimeToSystemTime(&f, &mut utc).is_err() {
            return String::new();
        }
        if SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_err() {
            local = utc;
        }
        let (mut d, mut t) = ([0u16; 64], [0u16; 64]);
        let nd = GetDateFormatEx(PCWSTR::null(), DATE_SHORTDATE, Some(&local), PCWSTR::null(), Some(&mut d), PCWSTR::null());
        let nt = GetTimeFormatEx(PCWSTR::null(), TIME_NOSECONDS, Some(&local), PCWSTR::null(), Some(&mut t));
        // les longueurs renvoyées comptent le zéro final
        format!("{} {}", String::from_utf16_lossy(&d[..(nd.max(1) - 1) as usize]), String::from_utf16_lossy(&t[..(nt.max(1) - 1) as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_a_date() {
        let dates = Dates::default();
        assert_eq!(&*dates.get(0), "");
        // 2026-11-10 12:00 UTC
        let ft = 134_387_856_000_000_000u64;
        let s = dates.get(ft);
        assert!(s.contains("2026") || s.contains("26"), "{s}");
        assert!(s.contains(':') || s.contains('h') || s.contains('.'), "{s}");
        assert!(Rc::ptr_eq(&s, &dates.get(ft + 10)));
        // la forme des fichiers exportés : toujours la même, à l'heure locale
        assert_eq!(iso(0), "");
        let exported = iso(ft);
        assert_eq!(exported.len(), 19, "{exported}");
        assert!(exported.starts_with("2026-11-1") && exported.ends_with(":00:00"), "{exported}");
    }
}
