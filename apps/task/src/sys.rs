//! Reading Windows: the process list (one NtQuerySystemInformation call gives every process with its times, memory
//! and I/O counters), the processor, memory and network of the PC, program names and icons from the executables,
//! command lines, and ending a process.

use std::{collections::HashSet, ffi::c_void};
use windows::{
    Wdk::System::{
        SystemInformation::{NtQuerySystemInformation, SystemProcessInformation},
        Threading::{NtQueryInformationProcess, ProcessCommandLineInformation},
    },
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE, HWND, LPARAM, STATUS_INFO_LENGTH_MISMATCH},
        Graphics::{
            Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_DESC1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1},
            Imaging::{CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom},
        },
        NetworkManagement::{
            IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2},
            Ndis::{IF_OPER_STATUS, IfOperStatusUp},
        },
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
        Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW},
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION},
            SystemInformation::{GetSystemInfo, GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO},
            Threading::{
                GetCurrentProcessId, GetSystemTimes, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
                QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
            },
        },
        UI::{
            Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW},
            WindowsAndMessaging::{
                DestroyIcon, EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongW, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
                WS_EX_TOOLWINDOW,
            },
        },
    },
    core::{BOOL, HSTRING, PCWSTR, PWSTR},
};

/// The fixed part of SYSTEM_PROCESS_INFORMATION (64-bit layout); the threads follow it in the buffer.
#[repr(C)]
#[allow(non_snake_case, dead_code)]
struct SpiFixed {
    NextEntryOffset: u32,
    NumberOfThreads: u32,
    WorkingSetPrivateSize: i64,
    HardFaultCount: u32,
    NumberOfThreadsHighWatermark: u32,
    CycleTime: u64,
    CreateTime: i64,
    UserTime: i64,
    KernelTime: i64,
    NameLength: u16,
    NameMaximumLength: u16,
    NameBuffer: *const u16,
    BasePriority: i32,
    UniqueProcessId: usize,
    InheritedFromUniqueProcessId: usize,
    HandleCount: u32,
    SessionId: u32,
    UniqueProcessKey: usize,
    PeakVirtualSize: usize,
    VirtualSize: usize,
    PageFaultCount: u32,
    PeakWorkingSetSize: usize,
    WorkingSetSize: usize,
    QuotaPeakPagedPoolUsage: usize,
    QuotaPagedPoolUsage: usize,
    QuotaPeakNonPagedPoolUsage: usize,
    QuotaNonPagedPoolUsage: usize,
    PagefileUsage: usize,
    PeakPagefileUsage: usize,
    PrivatePageCount: usize,
    ReadOperationCount: i64,
    WriteOperationCount: i64,
    OtherOperationCount: i64,
    ReadTransferCount: i64,
    WriteTransferCount: i64,
    OtherTransferCount: i64,
}

/// One process as Windows counts it (totals since it started).
#[derive(Clone, Debug, Default)]
pub struct Raw {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub create: i64,
    /// User + kernel time, 100 ns.
    pub busy: i64,
    pub mem: u64,
    pub read: u64,
    pub write: u64,
    pub threads: u32,
    pub handles: u32,
    pub session: u32,
}

/// Every process, in one call. `buf` is kept between calls (it grows once to fit).
pub fn processes(buf: &mut Vec<u8>) -> Vec<Raw> {
    if buf.len() < 512 * 1024 {
        buf.resize(512 * 1024, 0);
    }
    loop {
        let mut need = 0u32;
        let st = unsafe { NtQuerySystemInformation(SystemProcessInformation, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut need) };
        if st == STATUS_INFO_LENGTH_MISMATCH {
            let n = (need as usize).max(buf.len()) + 64 * 1024;
            buf.resize(n, 0);
            continue;
        }
        if st.is_err() {
            return Vec::new();
        }
        break;
    }
    let mut out = Vec::with_capacity(400);
    let mut off = 0usize;
    loop {
        if off + size_of::<SpiFixed>() > buf.len() {
            break;
        }
        // SAFETY: Windows wrote a chain of entries into `buf`; each one starts with this fixed part
        let e = unsafe { std::ptr::read_unaligned(buf.as_ptr().add(off) as *const SpiFixed) };
        let pid = e.UniqueProcessId as u32;
        if pid != 0 {
            let name = if e.NameBuffer.is_null() || e.NameLength == 0 {
                String::new()
            } else {
                // SAFETY: the name points inside the same buffer
                let s = unsafe { std::slice::from_raw_parts(e.NameBuffer, e.NameLength as usize / 2) };
                String::from_utf16_lossy(s)
            };
            out.push(Raw {
                pid,
                ppid: e.InheritedFromUniqueProcessId as u32,
                name: if pid == 4 && name.is_empty() { "System".into() } else { name },
                create: e.CreateTime,
                busy: e.UserTime + e.KernelTime,
                mem: e.WorkingSetPrivateSize.max(0) as u64,
                read: e.ReadTransferCount.max(0) as u64,
                write: e.WriteTransferCount.max(0) as u64,
                threads: e.NumberOfThreads,
                handles: e.HandleCount,
                session: e.SessionId,
            });
        }
        if e.NextEntryOffset == 0 {
            break;
        }
        off += e.NextEntryOffset as usize;
    }
    out
}

pub fn own_pid() -> u32 {
    unsafe { GetCurrentProcessId() }
}

fn ft(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

/// (idle, kernel, user) times of the whole PC, 100 ns.
pub fn system_times() -> (u64, u64, u64) {
    let (mut i, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe {
        let _ = GetSystemTimes(Some(&mut i), Some(&mut k), Some(&mut u));
    }
    (ft(i), ft(k), ft(u))
}

/// Wall clock now, 100 ns.
pub fn now_100ns() -> i64 {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    // 1601 → 1970
    (d.as_nanos() / 100) as i64 + 116_444_736_000_000_000
}

pub fn ncpu() -> u32 {
    let mut si = SYSTEM_INFO::default();
    unsafe { GetSystemInfo(&mut si) };
    si.dwNumberOfProcessors.max(1)
}

pub fn uptime_s() -> u64 {
    unsafe { GetTickCount64() / 1000 }
}

pub struct Mem {
    pub used: u64,
    pub total: u64,
    pub commit: u64,
    pub commit_limit: u64,
    pub cached: u64,
}

pub fn memory() -> Mem {
    let mut ms = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    let mut pi = PERFORMANCE_INFORMATION { cb: size_of::<PERFORMANCE_INFORMATION>() as u32, ..Default::default() };
    unsafe {
        let _ = GlobalMemoryStatusEx(&mut ms);
        let _ = GetPerformanceInfo(&mut pi, pi.cb);
    }
    let page = pi.PageSize as u64;
    Mem {
        used: ms.ullTotalPhys.saturating_sub(ms.ullAvailPhys),
        total: ms.ullTotalPhys,
        commit: pi.CommitTotal as u64 * page,
        commit_limit: pi.CommitLimit as u64 * page,
        cached: pi.SystemCache as u64 * page,
    }
}

/// Bytes received and sent by the network adapters that are up (loopback and virtual filters left out).
pub fn net_bytes() -> (u64, u64) {
    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    if unsafe { GetIfTable2(&mut table) }.is_err() || table.is_null() {
        return (0, 0);
    }
    let (mut rx, mut tx) = (0u64, 0u64);
    unsafe {
        let t = &*table;
        let rows = std::slice::from_raw_parts(t.Table.as_ptr(), t.NumEntries as usize);
        for r in rows {
            // 24: software loopback. Filter and port drivers repeat the real adapter's numbers: hardware ones only.
            let hardware = r.InterfaceAndOperStatusFlags._bitfield & 1 != 0;
            if r.Type == 24 || !hardware || r.OperStatus != IF_OPER_STATUS(IfOperStatusUp.0) {
                continue;
            }
            rx += r.InOctets;
            tx += r.OutOctets;
        }
        FreeMibTable(table as *const c_void);
    }
    (rx, tx)
}

/// Processes that own a visible window on the taskbar (apps, as opposed to background processes).
pub fn window_pids() -> HashSet<u32> {
    unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
        unsafe {
            let set = &mut *(lp.0 as *mut HashSet<u32>);
            let tool = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0;
            let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|h| !h.is_invalid());
            if IsWindowVisible(hwnd).as_bool() && !tool && !owned && GetWindowTextLengthW(hwnd) > 0 {
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                set.insert(pid);
            }
            true.into()
        }
    }
    let mut set = HashSet::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut set as *mut _ as isize));
    }
    set
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn open(pid: u32, access: windows::Win32::System::Threading::PROCESS_ACCESS_RIGHTS) -> Option<Handle> {
    unsafe { OpenProcess(access, false, pid) }.ok().map(Handle)
}

/// Full path of a process's executable (None for protected system processes).
pub fn image_path(pid: u32) -> Option<String> {
    let h = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut buf = [0u16; 1024];
    let mut n = buf.len() as u32;
    unsafe { QueryFullProcessImageNameW(h.0, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n) }.ok()?;
    Some(String::from_utf16_lossy(&buf[..n as usize]))
}

/// The command line a process was started with, when Windows lets us read it.
pub fn command_line(pid: u32) -> Option<String> {
    let h = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut buf = vec![0u8; 8192];
    loop {
        let mut need = 0u32;
        let st = unsafe { NtQueryInformationProcess(h.0, ProcessCommandLineInformation, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut need) };
        if st.is_ok() {
            break;
        }
        if need as usize > buf.len() && need < 1 << 20 {
            buf.resize(need as usize, 0);
            continue;
        }
        return None;
    }
    // a UNICODE_STRING (length in bytes, max length, pointer into the same buffer)
    let len = u16::from_le_bytes([buf[0], buf[1]]) as usize;
    let ptr = unsafe { std::ptr::read_unaligned(buf.as_ptr().add(8) as *const *const u16) };
    if ptr.is_null() || len == 0 {
        return None;
    }
    let s = unsafe { std::slice::from_raw_parts(ptr, len / 2) };
    Some(String::from_utf16_lossy(s))
}

/// The file description and company of an executable ("Google Chrome", "Google LLC").
pub fn file_info(path: &str) -> Option<(String, String)> {
    let p = HSTRING::from(path);
    let size = unsafe { GetFileVersionInfoSizeW(&p, None) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    unsafe { GetFileVersionInfoW(&p, None, size, data.as_mut_ptr() as *mut c_void) }.ok()?;
    let query = |q: &str| -> Option<(*const c_void, u32)> {
        let (mut ptr, mut n) = (std::ptr::null_mut::<c_void>(), 0u32);
        let ok = unsafe { VerQueryValueW(data.as_ptr() as *const c_void, &HSTRING::from(q), &mut ptr, &mut n) }.as_bool();
        (ok && !ptr.is_null() && n > 0).then_some((ptr as *const c_void, n))
    };
    // the first language the file lists, then English (US), then neutral
    let mut langs = Vec::new();
    if let Some((ptr, n)) = query(r"\VarFileInfo\Translation") {
        let words = unsafe { std::slice::from_raw_parts(ptr as *const u16, (n / 2) as usize) };
        for pair in words.chunks(2) {
            if pair.len() == 2 {
                langs.push(format!("{:04x}{:04x}", pair[0], pair[1]));
            }
        }
    }
    langs.extend(["040904b0".into(), "040904e4".into(), "000004b0".into()]);
    let get = |field: &str| -> Option<String> {
        for l in &langs {
            if let Some((ptr, n)) = query(&format!(r"\StringFileInfo\{l}\{field}")) {
                let s = unsafe { std::slice::from_raw_parts(ptr as *const u16, n as usize) };
                let s = String::from_utf16_lossy(s).trim_end_matches('\0').trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
        None
    };
    let desc = get("FileDescription")?;
    Some((desc, get("CompanyName").unwrap_or_default()))
}

/// A program's icon as premultiplied BGRA pixels (width, height, pixels). Call on a thread where COM is ready.
pub fn icon_pixels(path: &str) -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        let mut fi = SHFILEINFOW::default();
        let r = SHGetFileInfoW(&HSTRING::from(path), Default::default(), Some(&mut fi), size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON);
        if r == 0 || fi.hIcon.is_invalid() {
            return None;
        }
        let out = (|| {
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
            let bmp = wic.CreateBitmapFromHICON(fi.hIcon).ok()?;
            let conv = wic.CreateFormatConverter().ok()?;
            conv.Initialize(&bmp, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
            let (mut w, mut h) = (0, 0);
            conv.GetSize(&mut w, &mut h).ok()?;
            let mut px = vec![0u8; (w * h * 4) as usize];
            conv.CopyPixels(std::ptr::null(), w * 4, &mut px).ok()?;
            Some((w, h, px))
        })();
        let _ = DestroyIcon(fi.hIcon);
        out
    }
}

/// The processor's name from the registry ("AMD Ryzen 7 5800X 8-Core Processor").
pub fn cpu_name() -> String {
    crate::startup::reg_string(windows::Win32::System::Registry::HKEY_LOCAL_MACHINE, r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString")
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

/// The first real graphics card: its name and dedicated memory.
pub fn gpu_info() -> (String, u64) {
    unsafe {
        let Ok(f) = CreateDXGIFactory1::<IDXGIFactory1>() else { return (String::new(), 0) };
        let mut i = 0;
        while let Ok(a) = f.EnumAdapters1(i) {
            i += 1;
            let Ok(d): Result<DXGI_ADAPTER_DESC1, _> = a.GetDesc1() else { continue };
            if d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let n = d.Description.iter().position(|&c| c == 0).unwrap_or(d.Description.len());
            return (String::from_utf16_lossy(&d.Description[..n]), d.DedicatedVideoMemory as u64);
        }
    }
    (String::new(), 0)
}

/// Why a process can't be ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// Runs as administrator: Plume Task would need to run as administrator too.
    Elevated,
    /// Part of Windows (a protected or system process).
    System,
    /// A Windows service (session 0) of another account.
    Service,
    /// Already gone.
    Gone,
}

/// Whether Plume Task may end this process, and if not, why.
pub fn can_end(pid: u32, session: u32) -> Result<(), Why> {
    if pid <= 4 {
        return Err(Why::System);
    }
    if open(pid, PROCESS_TERMINATE).is_some() {
        return Ok(());
    }
    let Some(h) = open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
        return Err(if session == 0 { Why::Service } else { Why::System });
    };
    let mut token = HANDLE::default();
    if unsafe { OpenProcessToken(h.0, TOKEN_QUERY, &mut token) }.is_ok() {
        let token = Handle(token);
        let mut e = TOKEN_ELEVATION::default();
        let mut n = 0u32;
        let ok =
            unsafe { GetTokenInformation(token.0, TokenElevation, Some(&mut e as *mut _ as *mut c_void), size_of::<TOKEN_ELEVATION>() as u32, &mut n) }.is_ok();
        if ok && e.TokenIsElevated != 0 {
            return Err(if session == 0 { Why::Service } else { Why::Elevated });
        }
    }
    Err(if session == 0 { Why::Service } else { Why::System })
}

/// Ends a process (only after the user confirmed). Waits up to a second for it to be gone.
pub fn end(pid: u32, create: i64) -> Result<(), Why> {
    // the PID may have been given to a new process since: check it is still the same one
    let mut buf = Vec::new();
    if !processes(&mut buf).iter().any(|r| r.pid == pid && r.create == create) {
        return Err(Why::Gone);
    }
    let Some(h) = open(pid, PROCESS_TERMINATE | windows::Win32::System::Threading::PROCESS_SYNCHRONIZE) else {
        return Err(Why::System);
    };
    unsafe {
        TerminateProcess(h.0, 1).map_err(|_| Why::System)?;
        WaitForSingleObject(h.0, 1000);
    }
    Ok(())
}

/// FILETIME (100 ns since 1601, UTC) → local "2026-11-11 09:41".
pub fn local_time(create: i64) -> String {
    use windows::Win32::{
        Foundation::SYSTEMTIME,
        System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime},
    };
    let f = FILETIME { dwLowDateTime: create as u32, dwHighDateTime: (create >> 32) as u32 };
    let (mut utc, mut loc) = (SYSTEMTIME::default(), SYSTEMTIME::default());
    unsafe {
        if FileTimeToSystemTime(&f, &mut utc).is_err() || SystemTimeToTzSpecificLocalTime(None, &utc, &mut loc).is_err() {
            return String::new();
        }
    }
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", loc.wYear, loc.wMonth, loc.wDay, loc.wHour, loc.wMinute, loc.wSecond)
}

/// Opens Explorer on the folder of `path`, the file selected.
pub fn show_in_folder(path: &str) {
    #[cfg(feature = "test-hook")]
    {
        let _ = path;
    }
    #[cfg(not(feature = "test-hook"))]
    {
        plume_ui::folder::show(std::path::Path::new(path));
    }
}

#[allow(dead_code)]
const _P: Option<PCWSTR> = None;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_this_process() {
        let mut buf = Vec::new();
        let list = processes(&mut buf);
        let me = list.iter().find(|r| r.pid == own_pid()).expect("this process is in the list");
        assert!(me.threads >= 1 && me.mem > 0 && me.create > 0);
        assert!(list.len() > 5);
        let path = image_path(own_pid()).expect("own path");
        assert!(path.to_lowercase().ends_with(".exe"));
        assert!(command_line(own_pid()).is_some());
        assert_eq!(can_end(4, 0), Err(Why::System));
        assert!(memory().total > 0);
        assert!(!local_time(me.create).is_empty());
    }
}
