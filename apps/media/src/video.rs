//! Videos, through Windows' own Media Foundation:
//! - playback by the Media Engine in frame-server mode: each new frame is copied into a WIC bitmap that the window
//!   draws itself (no child window, so the controls and subtitles are drawn over the picture);
//! - single frames by the Source Reader (library thumbnails, the preview above the seek bar);
//! - the list of audio tracks, read from the file's streams.

use crate::image::Px;
use std::{
    path::Path,
    sync::{Arc, Once},
};
use windows::{
    Win32::{
        Foundation::RECT,
        Graphics::{Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM, Imaging::*},
        Media::MediaFoundation::*,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, StructuredStorage::PROPVARIANT},
    },
    core::{BSTR, HSTRING, Interface, implement},
};

static STARTUP: Once = Once::new();

/// Media Foundation, once per process.
pub fn startup() {
    STARTUP.call_once(|| unsafe {
        let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
    });
}

/// What the Media Engine tells the window (it calls from its own threads).
pub type OnEvent = Arc<dyn Fn(Ev) + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ev {
    /// Duration and size are known.
    Loaded,
    Playing,
    Paused,
    Ended,
    Error(u32),
    Other(u32),
}

#[implement(IMFMediaEngineNotify)]
struct Notify {
    on: OnEvent,
}

impl IMFMediaEngineNotify_Impl for Notify_Impl {
    fn EventNotify(&self, event: u32, param1: usize, param2: u32) -> windows_core::Result<()> {
        let e = MF_MEDIA_ENGINE_EVENT(event as i32);
        let ev = if e == MF_MEDIA_ENGINE_EVENT_LOADEDMETADATA {
            Ev::Loaded
        } else if e == MF_MEDIA_ENGINE_EVENT_PLAYING {
            Ev::Playing
        } else if e == MF_MEDIA_ENGINE_EVENT_PAUSE {
            Ev::Paused
        } else if e == MF_MEDIA_ENGINE_EVENT_ENDED {
            Ev::Ended
        } else if e == MF_MEDIA_ENGINE_EVENT_ERROR {
            // param1: MF_MEDIA_ENGINE_ERR, param2: the HRESULT
            Ev::Error(if param2 != 0 { param2 } else { param1 as u32 })
        } else {
            Ev::Other(event)
        };
        (self.on)(ev);
        Ok(())
    }
}

/// One video being played.
pub struct Player {
    engine: IMFMediaEngine,
    ex: Option<IMFMediaEngineEx>,
    target: Option<IWICBitmap>,
    wic: IWICImagingFactory,
    /// Size of the frames copied into `target`.
    pub w: u32,
    pub h: u32,
    last_pts: i64,
    /// A frame arrived since the window last took it.
    pub fresh: bool,
}

impl Player {
    pub fn open(path: &Path, on: OnEvent, volume: f64, muted: bool) -> windows::core::Result<Player> {
        startup();
        unsafe {
            let factory: IMFMediaEngineClassFactory = CoCreateInstance(&CLSID_MFMediaEngineClassFactory, None, CLSCTX_INPROC_SERVER)?;
            let mut attr = None;
            MFCreateAttributes(&mut attr, 3)?;
            let attr = attr.ok_or_else(windows::core::Error::empty)?;
            let notify: IMFMediaEngineNotify = Notify { on }.into();
            attr.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, &notify)?;
            attr.SetUINT32(&MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM.0 as u32)?;
            let engine = factory.CreateInstance(0, &attr)?;
            // the sound is set before anything can be heard
            engine.SetMuted(muted)?;
            engine.SetVolume(volume.clamp(0.0, 1.0))?;
            engine.SetAutoPlay(false)?;
            engine.SetSource(&BSTR::from(path.to_string_lossy().as_ref()))?;
            let ex = engine.cast::<IMFMediaEngineEx>().ok();
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            Ok(Player { engine, ex, target: None, wic, w: 0, h: 0, last_pts: -1, fresh: false })
        }
    }

    pub fn play(&self) {
        unsafe {
            let _ = self.engine.Play();
        }
    }
    pub fn pause(&self) {
        unsafe {
            let _ = self.engine.Pause();
        }
    }
    pub fn paused(&self) -> bool {
        unsafe { self.engine.IsPaused().as_bool() }
    }
    pub fn ended(&self) -> bool {
        unsafe { self.engine.IsEnded().as_bool() }
    }
    pub fn position(&self) -> f64 {
        unsafe { self.engine.GetCurrentTime() }
    }
    /// Seconds (0 while unknown).
    pub fn duration(&self) -> f64 {
        let d = unsafe { self.engine.GetDuration() };
        if d.is_finite() && d > 0.0 { d } else { 0.0 }
    }
    pub fn seek(&self, t: f64) {
        let t = t.max(0.0);
        unsafe {
            match &self.ex {
                // while dragging, the nearest key frame comes much faster
                Some(ex) => {
                    let _ = ex.SetCurrentTimeEx(t, MF_MEDIA_ENGINE_SEEK_MODE_NORMAL);
                }
                None => {
                    let _ = self.engine.SetCurrentTime(t);
                }
            }
        }
    }
    pub fn seek_fast(&self, t: f64) {
        unsafe {
            match &self.ex {
                Some(ex) => {
                    let _ = ex.SetCurrentTimeEx(t.max(0.0), MF_MEDIA_ENGINE_SEEK_MODE_APPROXIMATE);
                }
                None => self.seek(t),
            }
        }
    }
    pub fn set_rate(&self, r: f64) {
        unsafe {
            let _ = self.engine.SetDefaultPlaybackRate(r);
            let _ = self.engine.SetPlaybackRate(r);
        }
    }
    pub fn set_volume(&self, v: f64, muted: bool) {
        unsafe {
            let _ = self.engine.SetMuted(muted);
            let _ = self.engine.SetVolume(v.clamp(0.0, 1.0));
        }
    }
    #[cfg(feature = "test-hook")]
    pub fn muted(&self) -> bool {
        unsafe { self.engine.GetMuted().as_bool() }
    }
    #[cfg(feature = "test-hook")]
    pub fn volume(&self) -> f64 {
        unsafe { self.engine.GetVolume() }
    }
    #[cfg(feature = "test-hook")]
    pub fn has_video(&self) -> bool {
        unsafe { self.engine.HasVideo().as_bool() }
    }
    /// Whether the stream `index` of the file is being played.
    pub fn selected(&self, index: u32) -> bool {
        self.ex.as_ref().is_some_and(|ex| unsafe { ex.GetStreamSelection(index) }.is_ok_and(|b| b.as_bool()))
    }

    /// Plays only the audio stream `index` (a stream of the file, as [`audio_tracks`] lists them).
    pub fn choose_audio(&self, index: u32, all_audio: &[u32]) -> bool {
        let Some(ex) = &self.ex else { return false };
        unsafe {
            for &i in all_audio {
                let _ = ex.SetStreamSelection(i, i == index);
            }
            ex.ApplyStreamSelections().is_ok()
        }
    }

    /// Takes the newest frame if there is one; true when `pixels` changed.
    pub fn tick(&mut self) -> bool {
        unsafe {
            let Ok(pts) = self.engine.OnVideoStreamTick() else { return false };
            if pts == self.last_pts || pts < 0 {
                return false;
            }
            let (mut w, mut h) = (0u32, 0u32);
            if self.engine.GetNativeVideoSize(Some(&mut w), Some(&mut h)).is_err() || w == 0 || h == 0 {
                return false;
            }
            if self.target.is_none() || w != self.w || h != self.h {
                self.target = self.wic.CreateBitmap(w, h, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad).ok();
                self.w = w;
                self.h = h;
            }
            let Some(target) = &self.target else { return false };
            let r = RECT { left: 0, top: 0, right: w as i32, bottom: h as i32 };
            let black = MFARGB { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbAlpha: 255 };
            if self.engine.TransferVideoFrame(target, None, &r, Some(&black)).is_err() {
                return false;
            }
            self.last_pts = pts;
            self.fresh = true;
            true
        }
    }

    /// The last frame's pixels (BGRA, opaque), row after row.
    pub fn with_pixels<R>(&self, f: impl FnOnce(u32, u32, u32, &[u8]) -> R) -> Option<R> {
        let t = self.target.as_ref()?;
        unsafe {
            let r = WICRect { X: 0, Y: 0, Width: self.w as i32, Height: self.h as i32 };
            let lock = t.Lock(&r, WICBitmapLockRead.0 as u32).ok()?;
            let stride = lock.GetStride().ok()?;
            let (mut size, mut ptr) = (0u32, std::ptr::null_mut());
            lock.GetDataPointer(&mut size, &mut ptr).ok()?;
            if ptr.is_null() {
                return None;
            }
            let data = std::slice::from_raw_parts(ptr, size as usize);
            Some(f(self.w, self.h, stride, data))
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        unsafe {
            let _ = self.engine.Shutdown();
        }
    }
}

/// An audio track of a video file.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioTrack {
    /// Stream index in the file (the Media Engine's numbering too).
    pub index: u32,
    pub label: String,
}

fn source_reader(path: &Path, processing: bool) -> Option<IMFSourceReader> {
    startup();
    unsafe {
        let mut attr = None;
        MFCreateAttributes(&mut attr, 2).ok()?;
        let attr = attr?;
        if processing {
            attr.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1).ok()?;
        }
        MFCreateSourceReaderFromURL(&HSTRING::from(path), &attr).ok()
    }
}

fn duration_of(r: &IMFSourceReader) -> f64 {
    unsafe {
        r.GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
            .ok()
            .and_then(|v: PROPVARIANT| u64::try_from(&v).ok())
            .map_or(0.0, |d| d as f64 / 1e7)
    }
}

/// The audio tracks of a file (empty when it has one or none worth choosing between… or none at all).
pub fn audio_tracks(path: &Path) -> Vec<AudioTrack> {
    let Some(r) = source_reader(path, false) else { return vec![] };
    let mut out = vec![];
    unsafe {
        let mut i = 0u32;
        // the reader answers MF_E_INVALIDSTREAMNUMBER after the last stream
        while let Ok(t) = r.GetNativeMediaType(i, 0) {
            if t.GetGUID(&MF_MT_MAJOR_TYPE).ok() == Some(MFMediaType_Audio) {
                let lang = r.GetPresentationAttribute(i, &MF_SD_LANGUAGE).ok().and_then(|v| BSTR::try_from(&v).ok()).map(|b| b.to_string());
                let name = r.GetPresentationAttribute(i, &MF_SD_STREAM_NAME).ok().and_then(|v| BSTR::try_from(&v).ok()).map(|b| b.to_string());
                let n = out.len() + 1;
                let label = match (name.filter(|s| !s.trim().is_empty()), lang.filter(|s| !s.trim().is_empty())) {
                    (Some(nm), Some(l)) => format!("{nm} ({})", language_name(&l)),
                    (Some(nm), None) => nm,
                    (None, Some(l)) => language_name(&l),
                    (None, None) => format!("{} {n}", crate::i18n::tr("Track")),
                };
                out.push(AudioTrack { index: i, label });
            }
            i += 1;
            if i > 64 {
                break;
            }
        }
    }
    out
}

/// "en", "eng", "fr-FR" → a readable name.
pub fn language_name(code: &str) -> String {
    let base = code.split(['-', '_']).next().unwrap_or(code).to_ascii_lowercase();
    let fr = crate::i18n::french();
    let n = match base.as_str() {
        "en" | "eng" => ["English", "Anglais"],
        "fr" | "fre" | "fra" => ["French", "Français"],
        "de" | "ger" | "deu" => ["German", "Allemand"],
        "es" | "spa" => ["Spanish", "Espagnol"],
        "it" | "ita" => ["Italian", "Italien"],
        "pt" | "por" => ["Portuguese", "Portugais"],
        "ja" | "jpn" => ["Japanese", "Japonais"],
        "ko" | "kor" => ["Korean", "Coréen"],
        "zh" | "chi" | "zho" => ["Chinese", "Chinois"],
        "ru" | "rus" => ["Russian", "Russe"],
        "ar" | "ara" => ["Arabic", "Arabe"],
        "nl" | "dut" | "nld" => ["Dutch", "Néerlandais"],
        "und" => ["Unknown", "Inconnue"],
        _ => return code.to_string(),
    };
    n[fr as usize].to_string()
}

/// Reads single frames of a video, scaled down (thumbnails, seek bar preview). Keep one open to read several.
pub struct Frames {
    r: IMFSourceReader,
    pub w: u32,
    pub h: u32,
    pub duration: f64,
}

impl Frames {
    pub fn open(path: &Path) -> Option<Frames> {
        let r = source_reader(path, true)?;
        unsafe {
            let all = MF_SOURCE_READER_ALL_STREAMS.0 as u32;
            let first = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
            let _ = r.SetStreamSelection(all, false);
            r.SetStreamSelection(first, true).ok()?;
            let t = MFCreateMediaType().ok()?;
            t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ok()?;
            t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).ok()?;
            r.SetCurrentMediaType(first, None, &t).ok()?;
            let cur = r.GetCurrentMediaType(first).ok()?;
            let size = cur.GetUINT64(&MF_MT_FRAME_SIZE).ok()?;
            let (w, h) = ((size >> 32) as u32, (size & 0xffff_ffff) as u32);
            if w == 0 || h == 0 {
                return None;
            }
            let duration = duration_of(&r);
            Some(Frames { r, w, h, duration })
        }
    }

    /// The frame shown at `t` seconds (the first one decoded from there), at most `max` pixels on its long side.
    pub fn at(&self, t: f64, max: u32) -> Option<Px> {
        let first = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        unsafe {
            let pos = PROPVARIANT::from((t.max(0.0) * 1e7) as i64);
            let _ = self.r.SetCurrentPosition(&windows::core::GUID::zeroed(), &pos);
            // a few tries: the first samples after a seek can be empty
            for _ in 0..12 {
                let (mut flags, mut sample) = (0u32, None);
                self.r.ReadSample(first, 0, None, Some(&mut flags), None, Some(&mut sample)).ok()?;
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    return None;
                }
                let Some(sample) = sample else { continue };
                let buf = sample.ConvertToContiguousBuffer().ok()?;
                let px = self.copy(&buf)?;
                return Some(px.scaled_to(max));
            }
            None
        }
    }

    fn copy(&self, buf: &IMFMediaBuffer) -> Option<Px> {
        let (w, h) = (self.w as usize, self.h as usize);
        let mut data = vec![0u8; w * h * 4];
        unsafe {
            if let Ok(b2) = buf.cast::<IMF2DBuffer>() {
                let (mut scan0, mut pitch) = (std::ptr::null_mut(), 0i32);
                b2.Lock2D(&mut scan0, &mut pitch).ok()?;
                for y in 0..h {
                    let src = scan0.offset(y as isize * pitch as isize);
                    std::ptr::copy_nonoverlapping(src, data.as_mut_ptr().add(y * w * 4), w * 4);
                }
                let _ = b2.Unlock2D();
            } else {
                let (mut p, mut len) = (std::ptr::null_mut(), 0u32);
                buf.Lock(&mut p, None, Some(&mut len)).ok()?;
                let n = (len as usize).min(data.len());
                std::ptr::copy_nonoverlapping(p, data.as_mut_ptr(), n);
                let _ = buf.Unlock();
            }
        }
        // RGB32 leaves the fourth byte undefined: the picture is opaque
        for px in data.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        Some(Px { w: self.w, h: self.h, data })
    }
}

/// Length of a video in seconds (0 when unknown).
pub fn duration(path: &Path) -> f64 {
    source_reader(path, false).map_or(0.0, |r| duration_of(&r))
}

/// A thumbnail: the frame at a tenth of the video (past a black opening), and the duration.
pub fn thumbnail(path: &Path, max: u32) -> Option<(Px, f64)> {
    let f = Frames::open(path)?;
    let t = if f.duration > 2.0 { (f.duration * 0.1).min(30.0) } else { 0.0 };
    let px = f.at(t, max).or_else(|| f.at(0.0, max))?;
    Some((px, f.duration))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages() {
        let _l = crate::i18n::TEST_LOCK.lock();
        crate::i18n::set("en");
        assert_eq!(language_name("eng"), "English");
        assert_eq!(language_name("fr-FR"), "French");
        assert_eq!(language_name("xx"), "xx");
    }

    /// With PLUME_MEDIA_TEST_VIDEO pointing at a short video (silent or not: nothing is played aloud here).
    #[test]
    fn frames_and_tracks() {
        let Ok(p) = std::env::var("PLUME_MEDIA_TEST_VIDEO") else { return };
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        let path = Path::new(&p);
        let (px, d) = thumbnail(path, 320).expect("thumbnail");
        assert!(px.w <= 320 && px.h > 0 && d > 1.0, "{} {} {d}", px.w, px.h);
        let tracks = audio_tracks(path);
        eprintln!("tracks: {tracks:?}");
    }
}
