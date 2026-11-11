//! The sound card, through WASAPI in shared mode: float stereo at the rate Windows mixes at (so nothing is converted
//! twice), driven by Windows' "buffer ready" event. A lost device (headphones unplugged, another default output) is
//! reopened on the new default one.

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        Media::{
            Audio::{
                AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                IAudioClient, IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0,
                eConsole, eRender,
            },
            KernelStreaming::{SPEAKER_FRONT_LEFT, SPEAKER_FRONT_RIGHT, WAVE_FORMAT_EXTENSIBLE},
            Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
        },
        System::{
            Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree},
            Threading::{CreateEventW, WaitForSingleObject},
        },
    },
    core::Interface,
};

pub struct Output {
    client: IAudioClient,
    render: IAudioRenderClient,
    event: HANDLE,
    /// Frames in Windows' buffer.
    size: u32,
    pub rate: u32,
}

impl Drop for Output {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
            let _ = CloseHandle(self.event);
        }
    }
}

impl Output {
    /// The default output, float stereo at its mixing rate. COM must be initialised on this thread.
    pub fn open() -> windows::core::Result<Output> {
        unsafe {
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
            let mix = client.GetMixFormat()?;
            let rate = (*mix).nSamplesPerSec;
            CoTaskMemFree(Some(mix as *const _));
            let fmt = WAVEFORMATEXTENSIBLE {
                Format: WAVEFORMATEX {
                    wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
                    nChannels: 2,
                    nSamplesPerSec: rate,
                    nAvgBytesPerSec: rate * 8,
                    nBlockAlign: 8,
                    wBitsPerSample: 32,
                    cbSize: (size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()) as u16,
                },
                Samples: WAVEFORMATEXTENSIBLE_0 { wValidBitsPerSample: 32 },
                dwChannelMask: SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
                SubFormat: KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
            };
            // 50 ms: short enough for an instant pause and seek, long enough never to crackle
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                500_000,
                0,
                &fmt as *const _ as *const WAVEFORMATEX,
                None,
            )?;
            let event = CreateEventW(None, false, false, None)?;
            client.SetEventHandle(event)?;
            let size = client.GetBufferSize()?;
            let render: IAudioRenderClient = client.GetService()?;
            Ok(Output { client, render, event, size, rate })
        }
    }

    pub fn start(&self) -> windows::core::Result<()> {
        unsafe { self.client.Start() }
    }

    /// Waits until Windows wants more sound (at most `ms`), then lets `fill` write that many stereo frames.
    /// Err: the device is gone (open it again).
    pub fn feed(&self, ms: u32, fill: &mut dyn FnMut(&mut [f32])) -> windows::core::Result<()> {
        unsafe {
            let w = WaitForSingleObject(self.event, ms);
            let _ = w == WAIT_OBJECT_0;
            let pad = self.client.GetCurrentPadding()?;
            let n = self.size.saturating_sub(pad);
            if n == 0 {
                return Ok(());
            }
            let p = self.render.GetBuffer(n)?;
            let buf = std::slice::from_raw_parts_mut(p as *mut f32, n as usize * 2);
            fill(buf);
            self.render.ReleaseBuffer(n, 0)
        }
    }

    /// Sound still queued in Windows (for the position shown: what is heard, not what was written).
    pub fn queued(&self) -> u32 {
        unsafe { self.client.GetCurrentPadding().unwrap_or(0) }
    }

    #[allow(dead_code)]
    pub fn cast_check(&self) -> bool {
        self.client.cast::<IAudioClient>().is_ok()
    }
}
