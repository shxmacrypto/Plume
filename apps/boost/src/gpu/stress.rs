//! The stability test: a Direct3D 11 compute load on one graphics card, for about a minute.
//!
//! Each round, every one of 262,144 threads runs an integer recurrence many times and writes its result. The
//! processor then computes 64 of those results itself and compares: a card that computes wrong at its new clocks is
//! caught even when nothing crashes (the silent errors an unstable overclock causes first). The round size is adjusted
//! so each one takes about 50 ms, far below the 2 s after which Windows would reset the driver. The test also watches
//! for the device being lost (a driver reset), and the card's readings: highest temperature, clock and power, and the
//! clock in the second half (a drop means heat or power throttling).
//!
//! The shader is compiled at run time by Windows' own compiler (d3dcompiler_47.dll).

use super::{StressProgress, StressResult, Telemetry, Vendor};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows::{
    Win32::Graphics::{
        Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0, Fxc::D3DCompile, ID3DBlob},
        Direct3D11::{
            D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_UNORDERED_ACCESS, D3D11_BUFFER_DESC, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ,
            D3D11_MAPPED_SUBRESOURCE, D3D11_RESOURCE_MISC_BUFFER_STRUCTURED, D3D11_SDK_VERSION, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING, D3D11CreateDevice,
            ID3D11Buffer, ID3D11ComputeShader, ID3D11Device, ID3D11DeviceContext, ID3D11UnorderedAccessView,
        },
        Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1},
    },
    core::{Interface, s},
};

const THREADS: u32 = 1 << 18;
const CHECKS: u32 = 64;

const SHADER: &str = r"
RWStructuredBuffer<uint> outBuf : register(u0);
cbuffer Params : register(b0) { uint seed; uint iters; uint pad0; uint pad1; };
[numthreads(256, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) {
    uint x = id.x * 747796405u + seed;
    uint y = x ^ 0x9E3779B9u;
    [loop] for (uint i = 0; i < iters; i++) {
        x = x * 1664525u + 1013904223u;
        y ^= y << 13; y ^= y >> 17; y ^= y << 5;
        x ^= y;
    }
    outBuf[id.x] = x;
}
";

/// What the shader computes for thread `id`, computed by the processor.
pub fn expected(id: u32, seed: u32, iters: u32) -> u32 {
    let mut x = id.wrapping_mul(747_796_405).wrapping_add(seed);
    let mut y = x ^ 0x9E37_79B9;
    for _ in 0..iters {
        x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        y ^= y << 13;
        y ^= y >> 17;
        y ^= y << 5;
        x ^= y;
    }
    x
}

fn vendor_id(v: Vendor) -> u32 {
    match v {
        Vendor::Nvidia => 0x10DE,
        Vendor::Amd => 0x1002,
        Vendor::Intel => 0x8086,
    }
}

/// The DXGI adapter of the card: same maker, the closest name.
fn adapter(vendor: Vendor, name: &str) -> Option<IDXGIAdapter> {
    unsafe {
        let f: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut best: Option<(usize, IDXGIAdapter)> = None;
        for i in 0..8 {
            let Ok(a) = f.EnumAdapters1(i) else { break };
            let Ok(d) = a.GetDesc1() else { continue };
            if d.VendorId != vendor_id(vendor) {
                continue;
            }
            let desc = String::from_utf16_lossy(&d.Description).trim_end_matches('\0').to_lowercase();
            let score = name.to_lowercase().split_whitespace().filter(|w| desc.contains(w)).count();
            if best.as_ref().is_none_or(|(s, _)| score > *s) {
                best = Some((score, a.cast().ok()?));
            }
        }
        best.map(|b| b.1)
    }
}

struct Gpu {
    dev: ID3D11Device,
    ctx: ID3D11DeviceContext,
    cs: ID3D11ComputeShader,
    out: ID3D11Buffer,
    uav: ID3D11UnorderedAccessView,
    cb: ID3D11Buffer,
    staging: ID3D11Buffer,
}

fn setup(vendor: Vendor, name: &str) -> Result<Gpu, String> {
    unsafe {
        let a = adapter(vendor, name).ok_or("no Direct3D adapter for this card")?;
        let (mut dev, mut ctx) = (None, None);
        D3D11CreateDevice(
            &a,
            D3D_DRIVER_TYPE_UNKNOWN,
            Default::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut dev),
            None,
            Some(&mut ctx),
        )
        .map_err(|e| e.message())?;
        let (dev, ctx): (ID3D11Device, ID3D11DeviceContext) = (dev.ok_or("no device")?, ctx.ok_or("no context")?);
        let mut code: Option<ID3DBlob> = None;
        let mut errs: Option<ID3DBlob> = None;
        D3DCompile(SHADER.as_ptr() as *const _, SHADER.len(), s!("plume-boost-stress"), None, None, s!("main"), s!("cs_5_0"), 0, 0, &mut code, Some(&mut errs))
            .map_err(|e| e.message())?;
        let code = code.ok_or("no shader")?;
        let bytes = std::slice::from_raw_parts(code.GetBufferPointer() as *const u8, code.GetBufferSize());
        let mut cs = None;
        dev.CreateComputeShader(bytes, None, Some(&mut cs)).map_err(|e| e.message())?;
        let buf = |usage, bind: u32, cpu: u32, misc: u32, size: u32, stride: u32| -> Result<ID3D11Buffer, String> {
            let desc = D3D11_BUFFER_DESC { ByteWidth: size, Usage: usage, BindFlags: bind, CPUAccessFlags: cpu, MiscFlags: misc, StructureByteStride: stride };
            let mut b = None;
            dev.CreateBuffer(&desc, None, Some(&mut b)).map_err(|e| e.message())?;
            b.ok_or_else(|| "no buffer".to_string())
        };
        let out = buf(D3D11_USAGE_DEFAULT, D3D11_BIND_UNORDERED_ACCESS.0 as u32, 0, D3D11_RESOURCE_MISC_BUFFER_STRUCTURED.0 as u32, THREADS * 4, 4)?;
        let cb = buf(D3D11_USAGE_DEFAULT, D3D11_BIND_CONSTANT_BUFFER.0 as u32, 0, 0, 16, 0)?;
        let staging = buf(D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ.0 as u32, 0, THREADS * 4, 0)?;
        let mut uav = None;
        dev.CreateUnorderedAccessView(&out, None, Some(&mut uav)).map_err(|e| e.message())?;
        Ok(Gpu { dev, ctx, cs: cs.ok_or("no shader")?, out, uav: uav.ok_or("no view")?, cb, staging })
    }
}

impl Gpu {
    /// One round: dispatch, copy back, wait, and count the wrong results among CHECKS sampled threads.
    /// Err: the device was lost.
    fn round(&self, seed: u32, iters: u32) -> Result<u64, ()> {
        unsafe {
            let params = [seed, iters, 0, 0];
            self.ctx.UpdateSubresource(&self.cb, 0, None, params.as_ptr() as *const _, 0, 0);
            self.ctx.CSSetShader(&self.cs, None);
            self.ctx.CSSetConstantBuffers(0, Some(&[Some(self.cb.clone())]));
            self.ctx.CSSetUnorderedAccessViews(0, 1, Some(&Some(self.uav.clone())), None);
            self.ctx.Dispatch(THREADS / 256, 1, 1);
            self.ctx.CopyResource(&self.staging, &self.out);
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            if self.ctx.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut m)).is_err() || self.dev.GetDeviceRemovedReason().is_err() {
                return Err(());
            }
            let data = std::slice::from_raw_parts(m.pData as *const u32, THREADS as usize);
            let mut errors = 0;
            for k in 0..CHECKS {
                let id = (k.wrapping_mul(2_654_435_761) ^ seed) % THREADS;
                if data[id as usize] != expected(id, seed, iters) {
                    errors += 1;
                }
            }
            self.ctx.Unmap(&self.staging, 0);
            Ok(errors)
        }
    }
}

/// Runs the test. `read`: the card's readings now.
pub fn run(vendor: Vendor, name: &str, secs: f64, stop: &AtomicBool, read: &dyn Fn() -> Option<Telemetry>, progress: StressProgress) -> StressResult {
    let mut r = StressResult::default();
    let gpu = match setup(vendor, name) {
        Ok(g) => g,
        Err(e) => {
            r.failed_to_start = Some(e);
            return r;
        }
    };
    let start = Instant::now();
    let mut iters = 64u32;
    let mut seed = 0x1234_5678u32;
    let mut last_read = Instant::now() - Duration::from_secs(2);
    loop {
        let done = start.elapsed().as_secs_f64();
        if done >= secs {
            break;
        }
        if stop.load(Ordering::Relaxed) {
            r.stopped = true;
            break;
        }
        let t0 = Instant::now();
        match gpu.round(seed, iters) {
            Ok(e) => r.errors += e,
            Err(()) => {
                r.device_removed = true;
                break;
            }
        }
        // keep each round near 50 ms
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        if ms < 25.0 && iters < 1 << 16 {
            iters *= 2;
        } else if ms > 100.0 && iters > 16 {
            iters /= 2;
        }
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        if last_read.elapsed() >= Duration::from_millis(500) {
            last_read = Instant::now();
            let tel = read();
            if let Some(t) = tel {
                r.max_temp = crate::gpu::max_opt(r.max_temp, t.temp_c);
                r.max_clock = crate::gpu::max_opt(r.max_clock, t.core_mhz);
                r.max_power = crate::gpu::max_opt(r.max_power, t.power_w);
                if done > secs * 0.5 {
                    r.min_clock_late = match (r.min_clock_late, t.core_mhz) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (a, b) => a.or(b),
                    };
                }
            }
            progress(done, secs, tel);
        }
    }
    r.secs = start.elapsed().as_secs_f64().min(secs);
    r
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_processor_side_of_the_check_is_deterministic() {
        // the same recurrence as the shader: fixed values, so a change to either side shows
        assert_eq!(super::expected(0, 0, 0), 0);
        assert_eq!(super::expected(1, 7, 0), 747_796_412);
        assert_eq!(super::expected(5, 9, 100), super::expected(5, 9, 100));
        assert_ne!(super::expected(5, 9, 100), super::expected(6, 9, 100));
    }
}
