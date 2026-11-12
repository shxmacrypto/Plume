# Plume Convert

Drop a file, get it in the format you need. A light native Windows app (Rust, Direct2D), part of Plume Suite.

## What it does

- **Drop zone** and **Add files** (Ctrl+O): videos, sounds and pictures, or whole folders. Files given on the command
  line (or by the Explorer menu) join the queue of the window already open.
- **Queue**: one file at a time, with a progress bar, the time left, cancel one or all, retry, open the result in
  Explorer, clear the finished ones.
- **Presets**, one click each:
  - **For phone**: MP4, H.264 + AAC, at most 1080p, fast start.
  - **For Discord**: stays under 10, 25 or 50 MB. Two passes at a bitrate worked out from the length; if the result
    is still too big, it tries again a little smaller.
  - **Smaller file**: H.264 at a lower constant quality, at most 1080p.
  - **Audio only**: MP3, M4A, WAV, FLAC or OGG.
  - **GIF**: from a video (10, 15 or 24 fps, 320 to 800 px, with its own palette) or from photos. One photo makes
    a still GIF. Several photos make one animated GIF (a slideshow in name order, "photo 2" before "photo 10"), each
    shown 0.5 to 5 s, in a loop or once, centred on black at the size of the largest. "One GIF each" makes a still
    GIF of every photo instead.
  - **Images**: WebP, JPEG, PNG, BMP, TIFF or ICO, with a quality and a longest side.
  - **Extract frames**: every frame, 10 a second, 1 a second, every 5 or 10 s, as PNG or JPEG, in a new folder.
  - **Trim**: start and end typed as `1:23` or seconds; exact (encodes again) or fast (copies).
  - **Mute**, **Rotate** (90° right, 180°, 90° left).
  - **Advanced**: container (MP4, MKV, WebM, MOV), video codec (H.264, H.265, VP9, AV1, copy), audio codec (AAC,
    MP3, Opus, FLAC, copy, none), resolution, frame rate, quality or bitrate, audio bitrate, sample rate.
- **Graphics card encoding**: NVIDIA NVENC, Intel Quick Sync or AMD AMF, found by a half-second test encode. When
  the card's encoder fails, the same job runs again on the processor by itself.
- **Output**: next to the original, or in a chosen folder, named `name (converted).ext`. Nothing is ever
  overwritten: the name is taken by creating the file first, and `(converted 2)`, `(converted 3)`… follow. A
  cancelled or failed job removes only what it created.
- Metadata kept (or stripped, in the settings). Photos are turned upright from their EXIF orientation.
- **Explorer menu** (optional, in the settings): "Convert with Plume" on a right-click, for the current user only
  (HKCU\Software\Classes\\*\shell\PlumeConvert).
- English and French; dark or light (follows Windows by default).

## How it works

- Videos and sounds: FFmpeg and ffprobe run as hidden child processes (`CREATE_NO_WINDOW`) at below-normal priority,
  with `-progress pipe:1` read for the progress bar.
- Pictures: WIC (Windows Imaging Component) reads anything Windows can read (HEIC when its codec is installed) and
  writes PNG, JPEG, BMP, TIFF and ICO (PNG-compressed sizes 16 to 256). WebP is written by FFmpeg from a PNG.
- Photos to GIF: WIC reads each photo (turned upright, so HEIC works too when its codec is installed) into PNG
  frames of one size in a temporary folder; FFmpeg makes the GIF with one palette per frame, then the folder is
  removed.
- Files: `ffmpeg.rs` (probe, command lines of every preset, size-target math, progress, hardware), `image.rs` (WIC),
  `naming.rs` (output names), `queue.rs` (a job on its worker thread), `tools.rs` (FFmpeg download and ZIP),
  `shell.rs` (Explorer menu), `opts.rs`, `settings.rs`, `i18n.rs`, `ui/` (window), `test_hook.rs`.

## What goes online

Only the FFmpeg download, the first time a video or a sound is converted (or with "Download now" in the settings):
`https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip` (about 100 MB), from which `ffmpeg.exe` and
`ffprobe.exe` are taken into `%LOCALAPPDATA%\Plume\tools`. That folder is shared with Plume Web and Plume Music:
what is already there is reused and never replaced. Files are never uploaded anywhere.

## Data

`%LOCALAPPDATA%\Plume\Convert\settings.json`: theme, language, output folder, metadata, graphics card, the last
preset and its choices.

## Tests

```
cargo test -p plume-convert
```

Unit tests cover the arguments of every preset (with and without the graphics card, metadata kept or stripped), the
photo-to-GIF arguments (still, slideshow, loop or once, delays), the size-target math, ffprobe parsing (rotation,
covers in songs), output naming that never overwrites, name order, progress parsing (two passes), the ZIP reader, WIC
round trips (JPEG on white, ICO sizes, GIF frames of one size on black), and that every string shown has its French.

The window is driven by a test build:

```
cargo build --profile fast -p plume-convert --features test-hook
```

with these environment variables:

| Variable | Use |
| --- | --- |
| `PLUME_CONVERT_TEST_PORT` | test socket on 127.0.0.1 (JSON lines, see `src/test_hook.rs`) |
| `PLUME_CONVERT_NOACTIVATE` | the window opens without taking the focus |
| `PLUME_CONVERT_DIR` | settings folder instead of `%LOCALAPPDATA%\Plume\Convert` |
| `PLUME_TOOLS_DIR` | FFmpeg folder instead of `%LOCALAPPDATA%\Plume\tools` |
| `PLUME_CONVERT_REG_ROOT` | registry root for the Explorer menu (a test build writes nowhere else) |
| `PLUME_TEST_FILES`, `PLUME_TEST_FOLDER` | answers of the Add files / folder dialogs in a test build |
| `PLUME_SUITE_LANG`, `PLUME_SUITE_THEME` | language and theme when the settings say "System" |
