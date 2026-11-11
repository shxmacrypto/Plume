//! Windows' media controls: the song in the volume overlay and on the lock screen, with its cover, and the
//! keyboard's media keys (play/pause, next, previous) even when Plume Music is in the background.

use crate::ui::with_app;
use plume_ui::Window;
use std::path::Path;
use windows::{
    Foundation::TypedEventHandler,
    Media::{
        MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls, SystemMediaTransportControlsButton,
        SystemMediaTransportControlsButtonPressedEventArgs,
    },
    Storage::{StorageFile, Streams::RandomAccessStreamReference},
    Win32::System::WinRT::ISystemMediaTransportControlsInterop,
    core::{HSTRING, factory},
};

pub struct Smtc {
    c: SystemMediaTransportControls,
    last: (String, bool, Option<std::path::PathBuf>),
}

impl Smtc {
    pub fn new(win: &Window) -> Option<Smtc> {
        unsafe {
            let interop = factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>().ok()?;
            let c: SystemMediaTransportControls = interop.GetForWindow(win.hwnd).ok()?;
            c.SetIsEnabled(true).ok()?;
            let _ = c.SetIsPlayEnabled(true);
            let _ = c.SetIsPauseEnabled(true);
            let _ = c.SetIsNextEnabled(true);
            let _ = c.SetIsPreviousEnabled(true);
            let sender = win.sender();
            let handler = TypedEventHandler::<SystemMediaTransportControls, SystemMediaTransportControlsButtonPressedEventArgs>::new(move |_, args| {
                if let Some(a) = args.as_ref() {
                    let b = a.Button()?;
                    sender.send(move |h, cx| {
                        if let Some(app) = with_app(h) {
                            match b {
                                SystemMediaTransportControlsButton::Play | SystemMediaTransportControlsButton::Pause => app.toggle_play(cx),
                                SystemMediaTransportControlsButton::Next => app.skip(1, cx),
                                SystemMediaTransportControlsButton::Previous => app.skip(-1, cx),
                                _ => {}
                            }
                        }
                    });
                }
                Ok(())
            });
            let _ = c.ButtonPressed(&handler);
            Some(Smtc { c, last: (String::new(), false, None) })
        }
    }

    pub fn update(&mut self, title: &str, artist: &str, album: &str, playing: bool, cover: Option<&Path>) {
        let id = format!("{title}\u{1}{artist}");
        let cover = cover.map(Path::to_path_buf);
        if self.last == (id.clone(), playing, cover.clone()) {
            return;
        }
        let new_song = self.last.0 != id || self.last.2 != cover;
        self.last = (id, playing, cover.clone());
        let _ = self.c.SetPlaybackStatus(if playing { MediaPlaybackStatus::Playing } else { MediaPlaybackStatus::Paused });
        if !new_song {
            return;
        }
        let Ok(u) = self.c.DisplayUpdater() else { return };
        let _ = u.SetType(MediaPlaybackType::Music);
        if let Ok(mp) = u.MusicProperties() {
            let _ = mp.SetTitle(&HSTRING::from(title));
            let _ = mp.SetArtist(&HSTRING::from(artist));
            let _ = mp.SetAlbumTitle(&HSTRING::from(album));
        }
        if let Some(p) = cover
            && let Ok(op) = StorageFile::GetFileFromPathAsync(&HSTRING::from(p.as_path()))
            && let Ok(f) = op.get()
            && let Ok(r) = RandomAccessStreamReference::CreateFromFile(&f)
        {
            let _ = u.SetThumbnail(&r);
        }
        let _ = u.Update();
    }
}
