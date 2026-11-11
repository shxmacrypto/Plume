//! Windows performance counters (PDH): graphics engines per process, and the disks of the whole PC.
//! Created and used on the sampling thread only.

use std::collections::HashMap;
use windows::{
    Win32::System::Performance::{
        PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
        PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhGetFormattedCounterValue, PdhOpenQueryW,
    },
    core::{HSTRING, PCWSTR},
};

/// Values above 100 % are kept (several engines, several disks).
const NOCAP: PDH_FMT = PDH_FMT(PDH_FMT_DOUBLE.0 | 0x8000);

pub struct Query {
    q: PDH_HQUERY,
    counters: Vec<PDH_HCOUNTER>,
}

impl Query {
    /// A query over English counter paths; the ones this PC doesn't have are left out (their slot stays empty).
    pub fn new(paths: &[&str]) -> Option<Query> {
        let mut q = PDH_HQUERY::default();
        if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut q) } != 0 {
            return None;
        }
        let counters = paths
            .iter()
            .map(|p| {
                let mut c = PDH_HCOUNTER::default();
                if unsafe { PdhAddEnglishCounterW(q, &HSTRING::from(*p), 0, &mut c) } != 0 {
                    c = PDH_HCOUNTER::default();
                }
                c
            })
            .collect();
        let me = Query { q, counters };
        me.collect();
        Some(me)
    }

    pub fn has(&self, i: usize) -> bool {
        self.counters.get(i).is_some_and(|c| !c.is_invalid())
    }

    pub fn collect(&self) {
        unsafe {
            PdhCollectQueryData(self.q);
        }
    }

    /// The value of a single counter.
    pub fn value(&self, i: usize) -> Option<f64> {
        let c = *self.counters.get(i).filter(|c| !c.is_invalid())?;
        let mut v = PDH_FMT_COUNTERVALUE::default();
        let st = unsafe { PdhGetFormattedCounterValue(c, NOCAP, None, &mut v) };
        (st == 0 && (v.CStatus == PDH_CSTATUS_VALID_DATA || v.CStatus == PDH_CSTATUS_NEW_DATA)).then_some(unsafe { v.Anonymous.doubleValue })
    }

    /// Every instance of a wildcard counter: (instance name, value). `buf` is reused between calls.
    pub fn array(&self, i: usize, buf: &mut Vec<u8>, out: &mut Vec<(String, f64)>) {
        out.clear();
        let Some(&c) = self.counters.get(i).filter(|c| !c.is_invalid()) else { return };
        loop {
            let (mut size, mut count) = (buf.len() as u32, 0u32);
            let st = unsafe {
                PdhGetFormattedCounterArrayW(
                    c,
                    NOCAP,
                    &mut size,
                    &mut count,
                    if buf.is_empty() { None } else { Some(buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W) },
                )
            };
            if st == PDH_MORE_DATA {
                buf.resize(size as usize + 4096, 0);
                continue;
            }
            if st != 0 {
                return;
            }
            let items = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W, count as usize) };
            for it in items {
                if it.FmtValue.CStatus != PDH_CSTATUS_VALID_DATA && it.FmtValue.CStatus != PDH_CSTATUS_NEW_DATA {
                    continue;
                }
                let name = unsafe { it.szName.to_string() }.unwrap_or_default();
                out.push((name, unsafe { it.FmtValue.Anonymous.doubleValue }));
            }
            return;
        }
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.q);
        }
    }
}

/// "pid_1234_luid_0x0_0x1_phys_0_eng_3_engtype_3D" → (1234, "luid_0x0_0x1_phys_0_eng_3").
pub fn parse_engine(name: &str) -> Option<(u32, &str)> {
    let rest = name.strip_prefix("pid_")?;
    let us = rest.find('_')?;
    let pid = rest[..us].parse().ok()?;
    let engine = &rest[us + 1..];
    let engine = engine.find("_engtype").map(|e| &engine[..e]).unwrap_or(engine);
    Some((pid, engine))
}

/// Graphics use per process (its busiest engine) and of the whole card (the busiest engine, all processes summed).
pub fn gpu_split(items: &[(String, f64)]) -> (HashMap<u32, f32>, f32) {
    let mut per_pid: HashMap<u32, f32> = HashMap::new();
    let mut per_engine: HashMap<&str, f64> = HashMap::new();
    for (name, v) in items {
        let Some((pid, engine)) = parse_engine(name) else { continue };
        let e = per_pid.entry(pid).or_default();
        *e = e.max(*v as f32);
        *per_engine.entry(engine).or_default() += v;
    }
    let total = per_engine.values().copied().fold(0.0f64, f64::max).min(100.0) as f32;
    for v in per_pid.values_mut() {
        *v = v.min(100.0);
    }
    (per_pid, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engines() {
        assert_eq!(parse_engine("pid_1234_luid_0x0_0x1_phys_0_eng_3_engtype_3D"), Some((1234, "luid_0x0_0x1_phys_0_eng_3")));
        assert_eq!(parse_engine("total"), None);
        let items = vec![
            ("pid_1_luid_a_phys_0_eng_0_engtype_3D".to_string(), 30.0),
            ("pid_2_luid_a_phys_0_eng_0_engtype_3D".to_string(), 25.0),
            ("pid_2_luid_a_phys_0_eng_1_engtype_Copy".to_string(), 40.0),
        ];
        let (per, total) = gpu_split(&items);
        assert_eq!(per[&1], 30.0);
        assert_eq!(per[&2], 40.0, "a process's busiest engine");
        assert_eq!(total, 55.0, "the busiest engine, all processes together");
    }
}
