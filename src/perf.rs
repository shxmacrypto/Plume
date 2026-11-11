// Mesures système des fonctions « Performance » : batterie, connexion limitée, mémoire et CPU d'un
// processus WebView2, arrêt d'un processus (gestionnaire de tâches). Tout est appelé à la demande.
use windows::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE},
    NetworkManagement::IpHelper::GetNetworkConnectivityHint,
    Networking::WinSock::{NL_NETWORK_CONNECTIVITY_HINT, NetworkConnectivityCostHintFixed, NetworkConnectivityCostHintVariable},
    System::{
        Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX},
        Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, TerminateProcess},
    },
};

pub struct Power {
    pub on_battery: bool,
    /// 0..=100, ou 255 si inconnu (PC fixe)
    pub percent: u8,
}

pub fn power() -> Power {
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe {
        if GetSystemPowerStatus(&mut s).is_err() {
            return Power { on_battery: false, percent: 255 };
        }
    }
    // ACLineStatus : 0 = sur batterie ; BatteryFlag 128 = pas de batterie
    Power { on_battery: s.ACLineStatus == 0 && s.BatteryFlag != 128, percent: s.BatteryLifePercent }
}

/// Connexion facturée au volume (partage de connexion, forfait mobile…).
pub fn metered() -> bool {
    let mut h = NL_NETWORK_CONNECTIVITY_HINT::default();
    unsafe {
        GetNetworkConnectivityHint(&mut h).is_ok()
            && (h.ConnectivityCost == NetworkConnectivityCostHintFixed || h.ConnectivityCost == NetworkConnectivityCostHintVariable || h.OverDataLimit)
    }
}

struct Proc(HANDLE);
impl Drop for Proc {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Mémoire privée (octets) et temps CPU cumulé (unités de 100 ns) d'un processus.
pub fn stat(pid: u32) -> Option<(u64, u64)> {
    unsafe {
        let h = Proc(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
        let mut m = PROCESS_MEMORY_COUNTERS_EX { cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32, ..Default::default() };
        let mem = if K32GetProcessMemoryInfo(h.0, &mut m as *mut _ as *mut PROCESS_MEMORY_COUNTERS, m.cb).as_bool() { m.PrivateUsage as u64 } else { 0 };
        let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        let ft = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
        let cpu = if GetProcessTimes(h.0, &mut c, &mut e, &mut k, &mut u).is_ok() { ft(k) + ft(u) } else { 0 };
        Some((mem, cpu))
    }
}

/// « Terminer » dans le gestionnaire de tâches (jamais le processus principal de WebView2 : voir l'appelant).
pub fn kill(pid: u32) -> bool {
    unsafe { OpenProcess(PROCESS_TERMINATE, false, pid).is_ok_and(|h| TerminateProcess(Proc(h).0, 1).is_ok()) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn measures_itself() {
        // un peu de calcul d'abord : le temps CPU est compté par tranches de ~16 ms
        let t = std::time::Instant::now();
        let mut x = 0u64;
        while t.elapsed().as_millis() < 40 {
            x = x.wrapping_mul(31).wrapping_add(1);
        }
        std::hint::black_box(x);
        let (mem, cpu) = super::stat(std::process::id()).expect("stat");
        assert!(mem > 0 && cpu > 0);
        let p = super::power();
        assert!(p.percent <= 100 || p.percent == 255);
        let _ = super::metered();
    }
}
