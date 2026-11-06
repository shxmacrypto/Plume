//! Lecture SQLite minimale via winsqlite3.dll, livrée avec Windows 10/11 :
//! aucun moteur SQLite embarqué (0 Ko dans l'exe), chargé seulement pendant un import.
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::core::{PCSTR, w};

type Open = unsafe extern "system" fn(*const c_char, *mut *mut c_void, c_int, *const c_char) -> c_int;
type Prepare = unsafe extern "system" fn(*mut c_void, *const c_char, c_int, *mut *mut c_void, *mut *const c_char) -> c_int;
type Step = unsafe extern "system" fn(*mut c_void) -> c_int;
type Count = unsafe extern "system" fn(*mut c_void) -> c_int;
type Text = unsafe extern "system" fn(*mut c_void, c_int) -> *const u8;
type Fin = unsafe extern "system" fn(*mut c_void) -> c_int;

const SQLITE_ROW: c_int = 100;
const SQLITE_OPEN_READWRITE: c_int = 2;
const SQLITE_OPEN_URI: c_int = 0x40;

/// Exécute une requête en lecture seule et renvoie toutes les lignes (colonnes en texte).
pub fn query(path: &std::path::Path, sql: &str) -> Result<Vec<Vec<String>>, String> {
    unsafe {
        let lib = LoadLibraryW(w!("winsqlite3.dll")).map_err(|_| "winsqlite3.dll introuvable".to_string())?;
        macro_rules! f {
            ($n:literal, $t:ty) => {
                std::mem::transmute::<_, $t>(
                    GetProcAddress(lib, PCSTR(concat!($n, "\0").as_ptr())).ok_or(concat!("sqlite: ", $n))?,
                )
            };
        }
        let open: Open = f!("sqlite3_open_v2", Open);
        let prepare: Prepare = f!("sqlite3_prepare_v2", Prepare);
        let step: Step = f!("sqlite3_step", Step);
        let count: Count = f!("sqlite3_column_count", Count);
        let text: Text = f!("sqlite3_column_text", Text);
        let finalize: Fin = f!("sqlite3_finalize", Fin);
        let close: Fin = f!("sqlite3_close", Fin);

        // On travaille sur une copie temporaire : ouverture en écriture pour que SQLite intègre le journal WAL
        let uri = format!("file:///{}", path.display().to_string().replace('\\', "/").replace(' ', "%20").replace('#', "%23").replace('?', "%3F"));
        let uri = CString::new(uri).map_err(|e| e.to_string())?;
        let mut db = std::ptr::null_mut();
        if open(uri.as_ptr(), &mut db, SQLITE_OPEN_READWRITE | SQLITE_OPEN_URI, std::ptr::null()) != 0 {
            close(db);
            return Err("base illisible".into());
        }
        let csql = CString::new(sql).map_err(|e| e.to_string())?;
        let mut st = std::ptr::null_mut();
        if prepare(db, csql.as_ptr(), -1, &mut st, std::ptr::null_mut()) != 0 || st.is_null() {
            close(db);
            return Err("requête invalide".into());
        }
        let n = count(st);
        let mut rows = vec![];
        while step(st) == SQLITE_ROW {
            let mut row = Vec::with_capacity(n as usize);
            for i in 0..n {
                let p = text(st, i);
                row.push(if p.is_null() { String::new() } else { CStr::from_ptr(p as *const c_char).to_string_lossy().into_owned() });
            }
            rows.push(row);
        }
        finalize(st);
        close(db);
        Ok(rows)
    }
}
