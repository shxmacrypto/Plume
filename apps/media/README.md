# Plume Media

Photos and videos in one light native app: an instant photo viewer, a library of your pictures, and a video player.
Rust, Direct2D and Windows' own codecs (WIC for pictures, Media Foundation for videos). English and French.

## What it does

**Photos**
- Opens a picture at once from File Explorer (`plume-media.exe <file>`: decoding starts before the window is made).
  ← → go through the pictures and videos of its folder, in Explorer's order; the next and previous ones are decoded
  ahead so they show without waiting.
- Zoom with the wheel or the touchpad's pinch, where the pointer is; drag to move; double click: whole picture ↔ 100 %.
  Past the screen-size version, the picture is decoded again at full size.
- Rotate: JPEG turns **without loss** (only its EXIF orientation changes, the picture data stays byte for byte the
  same); PNG, BMP and TIFF are rewritten (lossless formats); other formats turn in Plume Media only. The file keeps
  its date.
- Full screen, slideshow (3 to 20 s per photo, videos skipped), favorites, info panel (size, date taken, camera,
  lens, exposure, flash, GPS position) read from EXIF by kamadak-exif.
- Formats: JPEG, PNG, GIF (animated), BMP, TIFF, WebP, ICO, JPEG XR; HEIC, AVIF and camera RAW when Windows has the
  codec (the free Microsoft Store extensions), with a clear message otherwise.

**Library**
- Pictures and Videos by default, plus any folder you add (with its subfolders).
- Timeline grouped by month (date taken, else the file's date), Videos, Folders, Favorites.
- Thumbnails made by WIC's scaler (videos: a frame by the Source Reader) and cached on disk; only the visible tiles
  are drawn, the screens around are prepared ahead, and bitmaps far from view are released. Tested with 2,000+ items.
- The list is kept in the data folder, so the window opens on it at once; a scan then updates it in the background,
  reading again only the files that changed.

**Videos**
- Media Foundation's Media Engine in frame-server mode: frames are copied into a bitmap the window draws itself, so
  controls and subtitles are drawn over the picture.
- Play/pause, seek bar with a preview frame above it, volume, mute, speed 0.5–2×, subtitles from an `.srt` next to
  the file (`Movie.srt`, `Movie.fr.srt`…), audio track choice, full screen, resume where you stopped.
- Shortcuts: Space/K play-pause, ← → 5 s (Shift: 30 s), ↑ ↓ volume, M mute, F/F11 full screen, C subtitles,
  [ ] speed, Page Up/Down previous/next file, Esc back.

**"Open with"**: Settings → Open with (or `plume-media.exe --register-open-with` / `--unregister-open-with`) adds
two file types, `PlumeMedia.Photo` and `PlumeMedia.Video`, under `HKCU\Software\Classes` for every supported
extension, plus `Applications\plume-media.exe`. Windows keeps the user's default app.

**Settings**: theme, language, library folders, opening zoom (fit / fill / actual size), slideshow speed,
thumbnail size, thumbnail cache.

## What goes online

Nothing. No account, no telemetry, no update check from this app.

## Data

`%LOCALAPPDATA%\Plume\Media`: `settings.json`, `library.tsv` (the list), `store.json` (favorites, where each video
stopped, view-only rotations), `thumbs\` (thumbnail cache).

## Environment variables

| Variable | Use |
| --- | --- |
| `PLUME_MEDIA_DIR` | moves the data folder (tests); a new library then starts with no folder |
| `PLUME_MEDIA_LIBRARY` | library folders, `;`-separated (tests) |
| `PLUME_MEDIA_REG_ROOT` | "Open with" writes under `HKCU\<this>` instead of `Software\Classes` (tests) |
| `PLUME_MEDIA_MUTE` | videos never make a sound (a `test-hook` build never does either) |
| `PLUME_MEDIA_NOACTIVATE` | the window shows without taking the focus |
| `PLUME_MEDIA_TEST_PORT` | test socket port (`--features test-hook` builds only) |
| `PLUME_SUITE_LANG`, `PLUME_SUITE_THEME` | language and theme from the suite |
| `PLUME_MEDIA_TEST_VIDEO` | a video for the `frames_and_tracks` unit test |

## Build and test

```
cargo build --profile fast -p plume-media [--features test-hook]
cargo test -p plume-media
cargo clippy -p plume-media --all-targets [--features test-hook]
```

The test socket (`src/test_hook.rs`) takes one JSON command per line on 127.0.0.1: `state`, `hits`, `click`,
`rclick`, `move`, `key`, `wheel`, `open`, `go`, `set`, `seek`, `scroll`, `rescan`, `back`. Tests use pictures and
videos generated in a temp folder (WIC/FFmpeg), a moved data folder, a test registry key, and a muted player.
