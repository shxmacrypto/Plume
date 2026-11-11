//! What Plume Task shows, without Windows: processes, programs (processes grouped by their executable), the
//! numbers of the whole PC, sorting, battery impact, the "what's slowing my PC" summary and the formatting of sizes
//! and rates. Everything here is plain data, so it is unit-tested.

use crate::i18n::{fill, tr};

/// One process, with its rates over the last interval.
#[derive(Clone, Debug, Default)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    /// Executable name ("chrome.exe").
    pub name: String,
    /// Full path of the executable, when Windows lets us read it.
    pub path: Option<String>,
    /// Start time (FILETIME, 100 ns since 1601).
    pub create: i64,
    pub session: u32,
    pub threads: u32,
    pub handles: u32,
    /// Share of the whole processor (all cores), 0..100.
    pub cpu: f32,
    /// Private working set, bytes (the memory only this process uses).
    pub mem: u64,
    /// Bytes per second.
    pub read: f64,
    pub write: f64,
    /// Busiest graphics engine, 0..100.
    pub gpu: f32,
    /// Has a visible window.
    pub windowed: bool,
}

/// A program: every process of the same executable.
#[derive(Clone, Debug, Default)]
pub struct Group {
    /// Lowercased path (or "name:" + name when the path is hidden).
    pub key: String,
    pub name: String,
    pub path: Option<String>,
    pub publisher: String,
    pub procs: Vec<Proc>,
    pub cpu: f32,
    pub mem: u64,
    pub read: f64,
    pub write: f64,
    pub gpu: f32,
    pub windowed: bool,
    /// Battery impact score, smoothed over the last half-minute (see [`impact_score`]).
    pub impact: f32,
}

impl Group {
    pub fn disk(&self) -> f64 {
        self.read + self.write
    }
    /// The process that started the others (the one whose parent is not in the group), else the oldest.
    pub fn main(&self) -> Option<&Proc> {
        self.procs.iter().filter(|p| !self.procs.iter().any(|q| q.pid == p.ppid && q.pid != p.pid)).min_by_key(|p| p.create).or_else(|| self.procs.first())
    }
}

/// The whole PC.
#[derive(Clone, Debug, Default)]
pub struct Sys {
    pub cpu: f32,
    pub ncpu: u32,
    pub cpu_name: String,
    pub mem_used: u64,
    pub mem_total: u64,
    pub commit: u64,
    pub commit_limit: u64,
    pub cached: u64,
    pub disk_read: f64,
    pub disk_write: f64,
    /// Share of the time the disks were busy, 0..100.
    pub disk_active: f32,
    pub net_in: f64,
    pub net_out: f64,
    pub gpu: f32,
    pub gpu_name: String,
    pub gpu_mem: u64,
    pub uptime_s: u64,
    pub procs: u32,
    pub threads: u32,
    pub handles: u32,
    /// Plume Task's own share of the processor over the last interval.
    pub own_cpu: f32,
    /// The graphics counters are readable on this PC.
    pub has_gpu: bool,
}

impl Sys {
    pub fn mem_pct(&self) -> f32 {
        if self.mem_total == 0 { 0.0 } else { (self.mem_used as f64 * 100.0 / self.mem_total as f64) as f32 }
    }
}

/// Share of the processor used between two readings: `busy` is the process's user + kernel time (100 ns units),
/// `wall` the time that passed (100 ns units), `ncpu` the logical processors. 0..100.
pub fn cpu_percent(busy_before: i64, busy_now: i64, wall: i64, ncpu: u32) -> f32 {
    if wall <= 0 || ncpu == 0 || busy_now < busy_before {
        return 0.0;
    }
    ((busy_now - busy_before) as f64 * 100.0 / (wall as f64 * ncpu as f64)).clamp(0.0, 100.0) as f32
}

/// Processor use of the whole PC from two GetSystemTimes readings (kernel time includes idle time).
pub fn system_cpu(idle: (u64, u64), kernel: (u64, u64), user: (u64, u64)) -> f32 {
    let idle = idle.1.saturating_sub(idle.0);
    let total = kernel.1.saturating_sub(kernel.0) + user.1.saturating_sub(user.0);
    if total == 0 { 0.0 } else { ((total.saturating_sub(idle)) as f64 * 100.0 / total as f64).clamp(0.0, 100.0) as f32 }
}

/// What a program costs the battery right now: the processor counts most, then graphics, then the disk (MB/s).
pub fn impact_score(cpu: f32, gpu: f32, disk_bytes: f64) -> f32 {
    cpu + gpu * 0.6 + (disk_bytes / 1_048_576.0) as f32 * 0.5
}

/// 0 very low … 4 very high.
pub fn impact_level(score: f32) -> usize {
    match score {
        s if s < 0.5 => 0,
        s if s < 3.0 => 1,
        s if s < 10.0 => 2,
        s if s < 25.0 => 3,
        _ => 4,
    }
}

pub fn impact_label(level: usize) -> &'static str {
    tr(["Very low", "Low", "Moderate", "High", "Very high"][level.min(4)])
}

/// Groups processes by executable. `describe` gives a program's name and publisher from its path (file
/// description), or None. `impact` gives the smoothed battery score of a key.
pub fn group(procs: Vec<Proc>, describe: &mut dyn FnMut(&str) -> Option<(String, String)>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for p in procs {
        let key = match &p.path {
            Some(path) => path.to_lowercase(),
            None => format!("name:{}", p.name.to_lowercase()),
        };
        let i = *index.entry(key.clone()).or_insert_with(|| {
            let (name, publisher) = p.path.as_deref().and_then(&mut *describe).unwrap_or_else(|| (pretty_name(&p.name), String::new()));
            groups.push(Group { key, name, path: p.path.clone(), publisher, ..Default::default() });
            groups.len() - 1
        });
        let g = &mut groups[i];
        g.cpu += p.cpu;
        g.mem += p.mem;
        g.read += p.read;
        g.write += p.write;
        g.gpu = (g.gpu + p.gpu).min(100.0);
        g.windowed |= p.windowed;
        g.procs.push(p);
    }
    for g in &mut groups {
        g.cpu = g.cpu.min(100.0);
    }
    groups
}

/// "chrome.exe" → "Chrome"; names without an extension stay as they are ("System", "Registry"). Windows' protected
/// processes, whose files can't be read, get the names Windows gives them.
pub fn pretty_name(exe: &str) -> String {
    let known = match exe.to_lowercase().as_str() {
        "msmpeng.exe" => "Microsoft Defender Antivirus",
        "nissrv.exe" => "Microsoft Defender network inspection",
        "mpdefendercoreservice.exe" => "Microsoft Defender core service",
        "csrss.exe" => "Client Server Runtime Process",
        "smss.exe" => "Windows Session Manager",
        "wininit.exe" => "Windows Start-Up Application",
        "services.exe" => "Services and Controller app",
        "lsass.exe" => "Local Security Authority",
        "lsaiso.exe" => "Credential Guard",
        "sgrmbroker.exe" => "System Guard Runtime Monitor Broker",
        "vmmem" | "vmmemwsl" => "Virtual machine memory",
        "memcompression" => "Memory Compression",
        _ => "",
    };
    if !known.is_empty() {
        return known.to_string();
    }
    let stem = exe.strip_suffix(".exe").or_else(|| exe.strip_suffix(".EXE")).unwrap_or(exe);
    let mut c = stem.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => exe.to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Col {
    Name,
    Cpu,
    Mem,
    Disk,
    Gpu,
    Impact,
}

impl Col {
    pub const ALL: [Col; 6] = [Col::Name, Col::Cpu, Col::Mem, Col::Disk, Col::Gpu, Col::Impact];
    pub fn label(self) -> &'static str {
        tr(match self {
            Col::Name => "Name",
            Col::Cpu => "Processor",
            Col::Mem => "Memory",
            Col::Disk => "Disk",
            Col::Gpu => "Graphics",
            Col::Impact => "Battery impact",
        })
    }
}

fn cmp_f(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

fn proc_value(p: &Proc, col: Col) -> f64 {
    match col {
        Col::Name | Col::Cpu => p.cpu as f64,
        Col::Mem => p.mem as f64,
        Col::Disk => p.read + p.write,
        Col::Gpu => p.gpu as f64,
        Col::Impact => impact_score(p.cpu, p.gpu, p.read + p.write) as f64,
    }
}

fn group_value(g: &Group, col: Col) -> f64 {
    match col {
        Col::Name | Col::Cpu => g.cpu as f64,
        Col::Mem => g.mem as f64,
        Col::Disk => g.disk(),
        Col::Gpu => g.gpu as f64,
        Col::Impact => g.impact as f64,
    }
}

/// Sorts programs (and the processes inside each) by a column. Ties keep a stable order by name.
pub fn sort_groups(groups: &mut [Group], col: Col, desc: bool) {
    let by_name = |a: &Group, b: &Group| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.key.cmp(&b.key));
    groups.sort_by(|a, b| {
        let o = if col == Col::Name { by_name(a, b) } else { cmp_f(group_value(a, col), group_value(b, col)).then_with(|| by_name(b, a)) };
        if desc { o.reverse() } else { o }
    });
    for g in groups.iter_mut() {
        g.procs.sort_by(|a, b| {
            let o = if col == Col::Name { a.pid.cmp(&b.pid) } else { cmp_f(proc_value(a, col), proc_value(b, col)).then_with(|| b.pid.cmp(&a.pid)) };
            if desc { o.reverse() } else { o }
        });
    }
}

/// Keeps the programs whose name, executable, path or a PID matches `q` (case-insensitive).
pub fn matches(g: &Group, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    let q = q.to_lowercase();
    g.name.to_lowercase().contains(&q)
        || g.key.contains(&q)
        || g.publisher.to_lowercase().contains(&q)
        || g.procs.iter().any(|p| p.name.to_lowercase().contains(&q) || p.pid.to_string() == q)
}

/// The "what's slowing my PC" card.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    /// 0 all fine, 1 busy, 2 working hard.
    pub level: u8,
    pub title: String,
    pub detail: String,
    /// The program it is about.
    pub key: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Res {
    Mem,
    Cpu,
    Disk,
    Gpu,
}

/// Averages of the last seconds (`sys`) and the programs: one sentence on what slows the PC, and who.
pub fn summary(sys: &Sys, groups: &[Group]) -> Summary {
    let pct = |v: f32| format!("{} %", v.round() as i64);
    let checks = [
        (Res::Mem, sys.mem_pct(), 80.0, 90.0),
        (Res::Cpu, sys.cpu, 50.0, 80.0),
        (Res::Disk, sys.disk_active, 60.0, 90.0),
        (Res::Gpu, if sys.has_gpu { sys.gpu } else { 0.0 }, 70.0, 90.0),
    ];
    // the worst: highest level first, then the furthest past its limit
    let worst = checks
        .iter()
        .filter_map(|&(r, v, busy, hard)| {
            let level = if v >= hard {
                2
            } else if v >= busy {
                1
            } else {
                0
            };
            (level > 0).then_some((r, v, level, v / busy))
        })
        .max_by(|a, b| a.2.cmp(&b.2).then(cmp_f(a.3 as f64, b.3 as f64)));
    let top = |f: &dyn Fn(&Group) -> f64| groups.iter().max_by(|a, b| cmp_f(f(a), f(b)));
    match worst {
        Some((r, v, level, _)) => {
            let hard = level == 2;
            let (title, g, detail) = match r {
                Res::Mem => {
                    let g = top(&|g| g.mem as f64);
                    let d = g.map(|g| fill(tr("{} uses {}. Closing programs you don't need frees memory."), &[&g.name, &size(g.mem)]));
                    (if hard { tr("Memory is almost full") } else { tr("Memory is filling up") }, g, d)
                }
                Res::Cpu => {
                    let g = top(&|g| g.cpu as f64);
                    let d = g.map(|g| fill(tr("{} uses {} of it. End it if you don't need it now."), &[&g.name, &pct(g.cpu)]));
                    (if hard { tr("The processor is working hard") } else { tr("The processor is busy") }, g, d)
                }
                Res::Disk => {
                    let g = top(&|g| g.disk());
                    let d = g.map(|g| fill(tr("{} reads and writes the most ({})."), &[&g.name, &rate(g.disk())]));
                    (if hard { tr("The disk is very busy") } else { tr("The disk is busy") }, g, d)
                }
                Res::Gpu => {
                    let g = top(&|g| g.gpu as f64);
                    let d = g.map(|g| fill(tr("{} uses {} of it."), &[&g.name, &pct(g.gpu)]));
                    (if hard { tr("The graphics card is working hard") } else { tr("The graphics card is busy") }, g, d)
                }
            };
            Summary { level, title: format!("{title} ({})", pct(v)), detail: detail.unwrap_or_default(), key: g.map(|g| g.key.clone()) }
        }
        None => {
            let g = top(&|g| g.cpu as f64).filter(|g| g.cpu >= 1.0);
            Summary {
                level: 0,
                title: tr("Your PC is running smoothly").to_string(),
                detail: match g {
                    Some(g) => fill(tr("Nothing is slowing it down. The busiest program is {} ({} of the processor)."), &[&g.name, &pct(g.cpu)]),
                    None => tr("Nothing is slowing it down right now.").to_string(),
                },
                key: g.map(|g| g.key.clone()),
            }
        }
    }
}

/// 1536 → "1.5 KB"; memory in binary units, written the way Windows writes them.
pub fn size(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    let n = if i == 0 || v >= 100.0 {
        format!("{v:.0}")
    } else if v >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    };
    let n = if crate::i18n::french() { n.replace('.', ",") } else { n };
    format!("{n} {}", if crate::i18n::french() { U[i].replace('B', "o") } else { U[i].to_string() })
}

/// Bytes per second → "1.2 MB/s" ("0 MB/s" when idle, like a quiet column should read).
pub fn rate(bps: f64) -> String {
    if bps < 0.05 * 1_048_576.0 {
        let mb = bps / 1_048_576.0;
        let s = if mb < 0.05 { "0".to_string() } else { format!("{mb:.1}") };
        let s = if crate::i18n::french() { s.replace('.', ",") } else { s };
        return format!("{s} {}", if crate::i18n::french() { "Mo/s" } else { "MB/s" });
    }
    format!("{}/s", size(bps as u64))
}

/// Network speeds are in bits: 1 250 000 B/s → "10 Mbps".
pub fn bits(bps: f64) -> String {
    let b = bps * 8.0;
    let (v, u) = if b >= 1e9 {
        (b / 1e9, "Gbps")
    } else if b >= 1e6 {
        (b / 1e6, "Mbps")
    } else {
        (b / 1e3, "Kbps")
    };
    let n = if v >= 100.0 { format!("{v:.0}") } else { format!("{v:.1}") };
    let n = if crate::i18n::french() { n.replace('.', ",") } else { n };
    format!("{n} {}", if crate::i18n::french() { u.replace("bps", "bit/s") } else { u.to_string() })
}

/// 0.04 → "0 %", 7.46 → "7.5 %", 42.4 → "42 %".
pub fn percent(v: f32) -> String {
    let s = if v < 0.05 {
        "0".to_string()
    } else if v < 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.0}")
    };
    let s = if crate::i18n::french() { s.replace('.', ",") } else { s };
    format!("{s} %")
}

/// 93784 s → "1 d 2 h 3 min".
pub fn uptime(s: u64) -> String {
    let (d, h, m) = (s / 86_400, s / 3600 % 24, s / 60 % 60);
    let (dl, hl, ml) = if crate::i18n::french() { ("j", "h", "min") } else { ("d", "h", "min") };
    if d > 0 {
        format!("{d} {dl} {h} {hl} {m} {ml}")
    } else if h > 0 {
        format!("{h} {hl} {m} {ml}")
    } else {
        format!("{m} {ml} {} s", s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, ppid: u32, name: &str, path: Option<&str>, cpu: f32, mem: u64) -> Proc {
        Proc { pid, ppid, name: name.into(), path: path.map(Into::into), cpu, mem, create: pid as i64, ..Default::default() }
    }

    #[test]
    fn cpu_math() {
        // 1 s of wall time on 4 processors, the process used 0.5 s of processor: 12.5 %
        assert!((cpu_percent(0, 5_000_000, 10_000_000, 4) - 12.5).abs() < 1e-4);
        assert_eq!(cpu_percent(10, 5, 100, 4), 0.0, "a restarted counter is not negative");
        assert_eq!(cpu_percent(0, 100, 0, 4), 0.0);
        assert_eq!(cpu_percent(0, 1_000_000_000, 10, 1), 100.0, "capped");
        // whole PC: kernel 60 (of which idle 50) + user 40 = 100; busy 50 %
        assert!((system_cpu((0, 50), (0, 60), (0, 40)) - 50.0).abs() < 1e-4);
        assert_eq!(system_cpu((5, 5), (5, 5), (5, 5)), 0.0);
    }

    #[test]
    fn grouping_by_executable() {
        let procs = vec![
            p(10, 1, "chrome.exe", Some(r"C:\Apps\Chrome\chrome.exe"), 2.0, 100),
            p(11, 10, "chrome.exe", Some(r"C:\APPS\Chrome\CHROME.EXE"), 3.0, 50),
            p(12, 10, "chrome.exe", Some(r"C:\Apps\Chrome\chrome.exe"), 99.0, 50),
            p(20, 1, "notepad.exe", Some(r"C:\Windows\notepad.exe"), 0.5, 10),
            p(4, 0, "System", None, 1.0, 1),
        ];
        let mut asked = Vec::new();
        let groups = group(procs, &mut |path| {
            asked.push(path.to_string());
            path.ends_with("chrome.exe").then(|| ("Chrome browser".to_string(), "Maker".to_string()))
        });
        assert_eq!(groups.len(), 3);
        let chrome = &groups[0];
        assert_eq!((chrome.name.as_str(), chrome.procs.len(), chrome.mem), ("Chrome browser", 3, 200));
        assert_eq!(chrome.cpu, 100.0, "a program never shows more than the whole processor");
        assert_eq!(chrome.main().map(|p| p.pid), Some(10));
        assert_eq!(groups[1].name, "Notepad", "no description: the executable's name");
        assert_eq!(groups[2].name, "System");
        assert_eq!(groups[2].key, "name:system");
        assert_eq!(asked.len(), 2, "one description per program, not per process");
    }

    #[test]
    fn sorting() {
        let mut groups = group(
            vec![
                p(1, 0, "b.exe", Some("b"), 5.0, 10),
                p(2, 0, "a.exe", Some("a"), 1.0, 30),
                p(3, 0, "c.exe", Some("c"), 9.0, 20),
                p(4, 0, "c.exe", Some("c"), 1.0, 1),
            ],
            &mut |_| None,
        );
        sort_groups(&mut groups, Col::Cpu, true);
        assert_eq!(groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["C", "B", "A"]);
        assert_eq!(groups[0].procs.iter().map(|p| p.pid).collect::<Vec<_>>(), [3, 4], "processes sorted too");
        sort_groups(&mut groups, Col::Mem, false);
        assert_eq!(groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["B", "C", "A"]);
        sort_groups(&mut groups, Col::Name, false);
        assert_eq!(groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["A", "B", "C"]);
        assert!(matches(&groups[2], "C.E") && matches(&groups[2], "4") && !matches(&groups[2], "zzz"));
    }

    #[test]
    fn battery_levels() {
        assert_eq!(impact_level(impact_score(0.1, 0.0, 0.0)), 0);
        assert_eq!(impact_level(impact_score(2.0, 0.0, 0.0)), 1);
        assert_eq!(impact_level(impact_score(5.0, 5.0, 2.0 * 1_048_576.0)), 2);
        assert_eq!(impact_level(impact_score(20.0, 0.0, 0.0)), 3);
        assert_eq!(impact_level(impact_score(30.0, 50.0, 0.0)), 4);
    }

    #[test]
    fn summary_names_the_culprit() {
        let groups = group(vec![p(1, 0, "game.exe", Some("g"), 70.0, 4 << 30), p(2, 0, "note.exe", Some("n"), 2.0, 1 << 20)], &mut |_| None);
        let mut sys = Sys { cpu: 85.0, mem_used: 50, mem_total: 100, ..Default::default() };
        let s = summary(&sys, &groups);
        assert_eq!((s.level, s.key.as_deref()), (2, Some("g")));
        assert!(s.title.contains("processor") && s.detail.starts_with("Game uses 70 %"), "{s:?}");
        // memory almost full beats a busy processor
        sys.cpu = 55.0;
        sys.mem_used = 95;
        let s = summary(&sys, &groups);
        assert!(s.title.starts_with("Memory is almost full") && s.detail.contains("4.00 GB"), "{s:?}");
        sys = Sys { cpu: 3.0, mem_used: 30, mem_total: 100, ..Default::default() };
        let s = summary(&sys, &groups);
        assert_eq!(s.level, 0);
        assert!(s.detail.contains("Game"));
    }

    #[test]
    fn formatting() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(1536), "1.50 KB");
        assert_eq!(size(250 * 1024 * 1024), "250 MB");
        assert_eq!(size(12 * 1024 * 1024 + 300 * 1024), "12.3 MB");
        assert_eq!(rate(0.0), "0 MB/s");
        assert_eq!(rate(30_000.0), "0 MB/s");
        assert_eq!(rate(80_000.0), "78.1 KB/s");
        assert_eq!(rate(3.0 * 1_048_576.0), "3.00 MB/s");
        assert_eq!(bits(1_250_000.0), "10.0 Mbps");
        assert_eq!(percent(0.01), "0 %");
        assert_eq!(percent(7.46), "7.5 %");
        assert_eq!(percent(42.4), "42 %");
        assert_eq!(uptime(93_784), "1 d 2 h 3 min");
        assert_eq!(uptime(125), "2 min 5 s");
        assert_eq!(pretty_name("chrome.exe"), "Chrome");
        assert_eq!(pretty_name("Registry"), "Registry");
        assert_eq!(pretty_name("MsMpEng.exe"), "Microsoft Defender Antivirus");
    }
}
