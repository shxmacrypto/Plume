<p align="center"><img src="../../assets/brand/plume-music-256.png" width="96" alt="Plume Music logo"></p>

<h1 align="center">Plume Music</h1>

<p align="center"><b>Your music, YouTube Music's catalog and your playlists, in one light player.</b></p>

Plume Music plays the music files on your PC and any song of YouTube Music, imports your playlists from Spotify,
Apple Music, SoundCloud and YouTube by pasting their link, and shows the lyrics line by line as the song goes. It is a
native Windows app (Rust, Direct2D, WASAPI): 4 MB, no web view, no account.

- **Search** YouTube Music as you type (songs with their artist, album, length and cover), with suggestions, recent
  searches and your own library in the same results.
- **Import a playlist** from a link: Spotify (playlist, album, song), Apple Music (playlist, album), SoundCloud (set,
  song), YouTube and YouTube Music playlists. Spotify and Apple Music songs are found on YouTube Music by their title,
  artist and length (100 songs in about 25 seconds; when a song exists only as its music video, that is used). A shared
  Spotify link shows its first 100 songs only: for a whole playlist, select its songs in Spotify (Ctrl+A, Ctrl+C) and
  click *Paste copied songs* (150 songs in about 35 seconds).
- **Play** without waiting: a YouTube song starts in about 3 seconds, and the next one is fetched while the current one
  plays, so songs follow each other without a gap. Gapless playback, crossfade (up to 12 s), volume normalization
  (ReplayGain), a 10-band equalizer with presets, shuffle, repeat, sleep timer.
- **Autoplay**: when the queue ends, similar songs follow (YouTube Music's radio of the last song). Every song also
  has its own *Song radio*.
- **Synced lyrics** (LRCLIB, a .lrc file next to the song, or the song's own tag): the line sung lights up and scrolls,
  in a side panel or in full screen; click a line to jump to it.
- **Now playing** in full window: the cover, animated when Apple Music has a motion cover for the album, over a living
  background: the cover's colors drifting slowly, the cover blurred, or an animated GIF of your choice (also possible
  behind the whole window).
- **Download** a song, an album or a whole playlist to your music folder: M4A as YouTube serves it (no loss) or MP3
  320 kbps, with title, artist, album and cover written in. Only download music you have the right to keep.
- **Your library**: songs, albums, artists, liked songs, downloads, playlists (create, rename, delete, add from any
  menu). The Music folder is scanned (MP3, M4A/AAC, ALAC, FLAC, Ogg Vorbis, WAV); add other folders in the settings.
- **Windows**: the song and its cover in Windows' media overlay, and the keyboard's media keys, even in the background.
- **Settings**: theme (system, dark, light), accent color (from the cover or fixed), backgrounds, animations, lyrics
  size, playback, equalizer, download format and folder, music folders, cache size, language (English, French).

YouTube and SoundCloud sounds are fetched by [yt-dlp](https://github.com/yt-dlp/yt-dlp) (installed on first use, shared
with Plume Web, kept up to date every week); FFmpeg is only needed for animated covers and MP3 downloads.

## Keyboard

| Keys | |
|---|---|
| Space | play / pause |
| Ctrl + → / ← | next / previous |
| Shift + → / ← | 10 s forward / back |
| Ctrl + ↑ / ↓ | volume |
| Ctrl + F | search |
| Ctrl + L | lyrics |
| Ctrl + S / Ctrl + R | shuffle / repeat |
| Ctrl + I | import a playlist |
| Ctrl + N | new playlist |
| L | like the song playing |
| F11 | now playing, full window |

## What goes online

- YouTube Music's search, suggestions and radio (`music.youtube.com`), and the sound through yt-dlp.
- The public page of a playlist you import (Spotify's embed page, Apple Music, SoundCloud).
- Lyrics: `lrclib.net`. Covers: the address given by YouTube Music, Spotify or Apple.
- Animated covers (when on): the iTunes search API and the album's Apple Music page.

Nothing else: no account, no statistics. The library, settings, cache and lyrics stay in `%LOCALAPPDATA%\Plume\Music`.

## Layout

```
src/audio.rs      the engine: a decoding thread per song (Symphonia), resampling (Rubato), crossfade, gapless, EQ
src/wasapi.rs     the sound card (WASAPI, shared mode, event driven)
src/yt.rs         YouTube Music search and radio, matching, fetching (yt-dlp), downloads and tags
src/import.rs     playlist links: Spotify, Apple Music, SoundCloud, YouTube
src/lyrics.rs     LRCLIB, .lrc files, LRC parsing
src/art.rs        covers, their colors, Apple motion covers (FFmpeg), animated GIFs (WIC)
src/library.rs    songs, playlists, likes, play counts, folder scan (lofty)
src/smtc.rs       Windows' media overlay and media keys
src/ui/           the window: sidebar, pages, player bar, now playing, settings
```

## Tests

```
cargo test -p plume-music                      # offline tests
cargo test -p plume-music -- --ignored         # also the network: YouTube Music, Spotify, Apple, LRCLIB
cargo build --profile fast -p plume-music --features test-hook
```

The test build answers JSON commands on 127.0.0.1 when `PLUME_MUSIC_TEST_PORT` is set (`state`, `click`, `search`,
`import`, `play_result`, `set`…); `PLUME_MUSIC_DIR` gives it its own library.
