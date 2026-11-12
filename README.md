<p align="center">
  <img src="assets/brand/plume-suite-256.png" width="112" alt="Plume Suite logo">
</p>

<h1 align="center">Plume Suite</h1>

<p align="center">
  <b>Light, native apps for Windows, with one look.</b><br>
  A web browser, a file explorer, a music player, a video editor, a photo editor, remote control from your phone and more (a mail app is coming), written in Rust, installed and updated together.<br>
  <i>Des applications légères et natives pour Windows : un navigateur, un explorateur de fichiers, un lecteur de musique, un éditeur vidéo, un éditeur photo, le contrôle du PC depuis le téléphone et plus, et bientôt une messagerie.</i>
</p>

<p align="center">
  <a href="https://github.com/shxmacrypto/plume/releases/latest/download/PlumeSetup.exe"><b>⬇ Download PlumeSetup.exe</b></a>
  ·
  <a href="https://github.com/shxmacrypto/plume/releases">All releases</a>
  ·
  <a href="CHANGELOG.md">Changelog</a>
</p>

<p align="center">
  <img src="assets/brand/family.png" width="560" alt="The Plume Suite family: Plume Web, Plume FEx, Plume Mail, Plume Viewer, Plume Music and the tools">
</p>

---

| App | What it is |
|---|---|
| **Plume Suite** | The app that installs, opens, updates and removes the others. Nothing to download: the installer holds them all. |
| **Plume Web** | An ultra-light web browser on the WebView2 engine already in Windows. |
| **[Plume FEx](apps/fex)** | A file explorer that opens folders in milliseconds and searches the whole PC as you type, without administrator rights. |
| **[Plume Viewer](apps/viewer)** | Control this PC from your phone or any browser: scan a QR code, accept, and the screen, mouse and keyboard are yours. Direct and encrypted. |
| **[Plume Music](apps/music)** | Your music files and YouTube Music, playlists imported from Spotify, Apple Music and SoundCloud, synced lyrics, animated covers and backgrounds, downloads. |
| **[Plume Drop](apps/drop)** | Send files, photos and links between this PC and a phone: scan a QR code, the phone's browser does the rest. |
| **[Plume Launcher](apps/launcher)** | Alt+Space, then type: apps, files, Windows settings pages and quick sums, in a few milliseconds. |
| **[Plume Passwords](apps/passwords)** | An encrypted password vault with 2FA codes, a generator, a security report, auto-type and Windows Hello. Never online. |
| **[Plume Media](apps/media)** | An instant photo viewer with a timeline, lossless JPEG rotation and photo details, and a video player with subtitles. |
| **[Plume Task](apps/task)** | What slows the PC down, grouped by program, with End task, live graphs and startup apps. Under 0.1 % of the processor. |
| **[Plume Clean](apps/clean)** | A map of what fills each drive and safe, explained cleanup: temporary files, caches, old downloads, duplicates. |
| **[Plume Backup](apps/backup)** | Automatic, incremental and versioned backups of your folders to a drive or a network share, with restore by date. |
| **[Plume Clipboard](apps/clipboard)** | Ctrl+Shift+V for your clipboard history: text, links, pictures and files, with search, pins and snippets. |
| **[Plume PDF](apps/pdf)** | Read, search, highlight, annotate, sign, fill, merge, split and reorder PDF files. |
| **[Plume Calendar](apps/calendar)** | Month, week, day and agenda views, recurring events, reminders, `.ics` files and subscribed calendars. |
| **[Plume Convert](apps/convert)** | Drop a video, song or picture and get it for phone, for Discord (10/25/50 MB), as a GIF, audio only or another picture format. |
| **[Plume Notepad](apps/notepad)** | Replaces Windows Notepad: tabs that come back after a restart, find and replace with regular expressions, every encoding, huge files in a blink, and sticky pads that stay on top. |
| **[Plume Cut](apps/cut)** | A video editor with a timeline (video, text, overlay, sound and music tracks), cuts, text, transitions, colour and speed, exported in 9:16, 16:9 or 1:1 with the graphics card. |
| **[Plume Create](apps/create)** | Photo editing and graphic design in one app: layers, crop, adjustments and looks, background remover, text effects, templates for stories and posts. |
| **[Plume Boost](apps/boost)** | Measured Windows performance tweaks and graphics card tuning (NVIDIA, AMD, Intel Arc) with a keep-or-revert countdown, a stability test and a crash guard; every change undoable and backed up with Plume Backup. |
| **[Plume Mail](apps/mail)** *(coming soon)* | A mail app for IMAP accounts, Outlook.com and Microsoft 365, that never loads a remote image. It is built and tested, and joins the installer once its *Sign in with Microsoft* is registered. |

Each app is a single program drawn by Windows itself (FEx and Mail use Direct2D, no web view). When a new version
is out, every app shows it in its title bar and Plume Suite updates them all in one click.

<p align="center">
  <img src="docs/screens/home.png" width="860" alt="Plume Web's new tab page: tabs, address bar, search, a clock, favorites and frequent sites">
</p>

## Plume Web

The browser that weighs nothing.

- **Tiny** — no admin rights needed. It renders pages with the WebView2 (Microsoft Edge) engine already on Windows, so it ships no browser engine of its own.
- **Memory saver** — background tabs are suspended, then unloaded after a few minutes (unless they play audio). Restored tabs sleep until you open them.
- **Ad & tracker blocker** — ~3,600 domains plus cosmetic filters, a per-site toggle and Edge tracking prevention.
- **Encrypted passwords** — a vault locked by one master password (Argon2id + AES-256-GCM). Plume offers to save logins, fills them for you and imports CSV exports.
- **Import** — history, favorites and their icons from Chrome, Edge, Brave, Opera, Opera GX, Vivaldi and Firefox.
- **Chrome extensions** — install from the Chrome Web Store, pin them in the toolbar, open them as floating popups or in a side panel.
- **Tabs your way** — groups, pinned tabs, vertical tabs, multiple windows, InPrivate windows, drag to reorder or into another window.
- **Reading** — reader mode, read aloud, page translation, dark mode per site.
- **Video & audio download** — MP4, WEBM, MKV, MP3, WAV, FLAC… via yt-dlp + FFmpeg, installed on demand.
- **Plume DevTools** — a docked panel with console, elements, network, storage and performance.
- **Updates in one click** — Plume checks GitHub each time it starts (or right away with *Settings › About › Check now*), then downloads, verifies (SHA-256) and installs new versions itself. If GitHub is slow or down, it says so instead of hanging.
- **For everyone** — French & English, light/dark themes, text size, high contrast, reduced motion, full keyboard support.

And much more, each feature optional and free when switched off:

- **Tabs** — workspaces, split screen, tab search, Peek previews, sessions, notes, archive of inactive tabs.
- **Navigation** — command palette (`Ctrl+K`), calculator and unit conversions in the address bar, Tab to search a site (YouTube, Wikipedia, Amazon…), mouse gestures.
- **Privacy** — containers (two accounts at once), clear data on close, a privacy report under the shield, password generator and audit, breach check, Windows Hello, secure DNS.
- **Appearance** — adaptive tint, new tab backgrounds and widgets (clock, weather, to-do), custom CSS/JS per site, compact and Zen modes, full screen.
- **Tools** — automatic picture-in-picture, media control, focus mode with Pomodoro, color picker and ruler, QR code, save as PDF or Markdown, read later offline.
- **Performance** — memory badge and task manager (`Shift+Esc`), battery saver, never-sleep list, smart preloading, Lite mode per site, statistics.
- **Downloads** — choose where to save each file, a downloads bubble with progress, *Open* and *Show in folder*, and downloads that keep going when you close their tab.
- **Default browser** — make Plume your default browser from the welcome setup or Settings; links and `.html` files from other apps then open in a new tab of your open Plume window.

<p align="center">
  <img src="docs/screens/split.png" width="420" alt="Split screen: a Wikipedia article next to the Plume website">
  <img src="docs/screens/downloads.png" width="420" alt="The downloads bubble under the toolbar button">
</p>

## Plume FEx, Plume Viewer, Plume Music and Plume Mail

- **[Plume FEx](apps/fex)**: instant search with Everything's syntax, search inside files, a preview pane, thumbnails,
  undo, the real Explorer context menu, archives (ZIP and 7z with a password), folder sizes, SFTP servers, tabs.
- **[Plume Viewer](apps/viewer)**: the PC's ID, a one-time code and a QR code; the phone sees only what changes on the screen,
  taps, long-presses, drags, zooms and types (accents included). A permanent password for your own devices, five wrong
  tries lock the PC, and the screen goes straight from device to device, encrypted.
- **[Plume Music](apps/music)**: search YouTube Music, paste a Spotify, Apple Music or SoundCloud link to import a playlist,
  synced lyrics, Apple Music's animated covers, a moving background (cover colors, blur or your GIF), gapless and
  crossfade, a 10-band equalizer, autoplay of similar songs, downloads in M4A or MP3, media keys. 4 MB, native.
- **[Plume Mail](apps/mail)** (coming soon, not in the 1.0.0 installer): several accounts, *Sign in with Microsoft*, new mail pushed by the server,
  conversations, drafts kept on the server, signatures, address suggestions, search in the text of messages,
  HTML shown as text with nothing loaded or run, the mailbox kept offline.

Their READMEs list what they do, their keys and what they don't do yet.

## Install

1. Download [`PlumeSetup.exe`](https://github.com/shxmacrypto/plume/releases/latest/download/PlumeSetup.exe) and run it. Plume Suite installs for your account in `%LOCALAPPDATA%\Programs\Plume` (no admin rights), adds Start menu and desktop shortcuts, and opens: pick the apps you want and click *Install*.
2. Windows SmartScreen may warn about an *unknown publisher* because the installer is not code-signed: click **More info → Run anyway**. Each release ships a `PlumeSetup.exe.sha256` file so you can check the download.
3. Windows Security may also block the installer or Plume the first time and call it a threat. It's a false positive: Microsoft's automatic scan flags new programs that aren't code-signed yet. Open **Windows Security → Virus & threat protection → Protection history**, select the Plume entry, choose **Actions → Allow on device**, then run the installer again. You only need to do this once.

Silent install: `PlumeSetup.exe --quiet` (add `--no-desktop` to skip the desktop shortcut). `PlumeSetup.exe --install web` (or `fex`, `mail`) installs one app without a window.

Requires Windows 10 or 11 (64-bit). Plume Web also needs the **WebView2 Runtime**, which is preinstalled on Windows 11 and on up-to-date Windows 10.

**Updates:** when a new version is out, Plume Suite and each app say so (Plume Web with a bar under the address bar, FEx and Mail with a button in their title bar). One click downloads `PlumeSetup.exe` from this repository's releases, checks it against the SHA-256 that GitHub publishes, and updates the suite and every installed app. If GitHub is slow or unreachable, it says so and offers *Try again* and *Download from GitHub*. Plume 0.7 installs Plume Suite 1.0 the same way, through its update bar.

**Uninstall:** one app from Plume Suite (*Remove*), or everything from Windows Settings › Apps › Plume Suite. Your data stays in `%LOCALAPPDATA%\Plume`.

## Privacy

No Plume app has an account or telemetry. Everything stays in `%LOCALAPPDATA%\Plume`. Plume FEx goes online only for the SFTP servers you connect to; Plume Mail (coming soon) only talks to your mail servers (and to Microsoft's sign-in page for a Microsoft account). Plume Viewer keeps a small link to its relay (a Cloudflare Worker, [apps/viewer/relay](apps/viewer/relay)) so phones can find the PC by its ID: the relay sees the ID and the devices' addresses, never the screen, the code or the password (the screen goes directly between the two devices, encrypted); STUN (`stun.cloudflare.com`) tells the PC its public address when a session starts. Plume Music talks to YouTube Music (search, radio, and the sound through yt-dlp), to LRCLIB for lyrics, to the pages of the playlists you import, and to Apple's iTunes search for animated covers when they are on (details in [apps/music](apps/music)). Plume Drop only listens on your local network, for the phones you allow. Plume Calendar downloads only the calendars you subscribe to, and Plume Convert and Plume Cut only FFmpeg, the first time they need it. Plume Launcher opens your browser only when you pick its web search line. Plume Passwords, Media, Task, Clean, Backup, Clipboard, PDF, Notepad, Create and Boost make no network connection at all (apart from the update check). Plume Suite and each app check this repository's latest release at start (the same request as below). Plume Web only goes online when you ask it to, plus:

| Request | When | Can be turned off |
|---|---|---|
| `github.com` — where the latest release page points (its version number) | each time Plume starts | Settings › About › Check for updates automatically |
| `api.github.com` and `github.com` — the new installer and its SHA-256 | when you click *Install update* | — |
| Chrome Web Store download | when you install an extension | — |
| yt-dlp (GitHub) and FFmpeg (gyan.dev) | the first time you download a video, after you agree | — |
| `api.frankfurter.app` — currency rates | when you convert a currency in the address bar, at most once a day | Settings › Navigation › Currencies |
| `api.pwnedpasswords.com` — first 5 characters of a SHA-1 hash | only if you turn on the data breach check | off by default |
| your secure DNS provider | every site address lookup, only if you turn on secure DNS | off by default |
| `bing.com` — picture of the day | once a day, only if you choose it as new tab background | off by default |
| `open-meteo.com` — weather | only with the weather widget and a city you enter | off by default |

Pages are rendered by Microsoft's WebView2 runtime; Plume starts it with Edge's background services, sync and crash reporting disabled.

## Build from source

Plume builds with the **GNU** Rust toolchain, so Visual Studio is not needed.

1. Install Rust and the GNU toolchain:
   ```powershell
   rustup toolchain install stable-x86_64-pc-windows-gnu
   rustup default stable-x86_64-pc-windows-gnu
   ```
2. Get GNU **binutils** from MSYS2's `mingw-w64-x86_64-binutils` package: `dlltool.exe`, `windres.exe`, `as.exe` and their DLLs (`libintl-8.dll`, `libiconv-2.dll`, `zlib1.dll`, `libzstd.dll`). Put them in `%USERPROFILE%\.mingw-binutils`, or anywhere else and point `PLUME_BINUTILS` at that folder.
3. Build:
   ```powershell
   .\build.ps1
   ```
   The result is the installer `dist\PlumeSetup.exe` plus `dist\PlumeSetup.exe.sha256`. It holds Plume Suite, Plume Web and Plume FEx; set `PLUME_WITH_MAIL=1` to add Plume Mail (Plume Suite then offers it too).

Run the tests with `cargo test --workspace` (Rust) and `node --test apps/web/tests/ui07.test.js` (Plume Web's interface functions: calculator, fuzzy search, QR codes). Tests that need the network are marked `#[ignore]`. FEx, Mail and the installer also have end-to-end scenarios that drive the real windows (`apps/*/tests`).

Before a commit, these two commands must have nothing left to say (`rustup component add clippy rustfmt` installs them):

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets
```

The code style is set in `rustfmt.toml` (lines up to 160 columns). `vendor/wry` is left out of both.

`vendor/wry` is a copy of wry 0.53.5 with two small changes: the WebView2 profile name used by containers, and page messages from addresses that `http::Uri` can't parse (such as local `file:///` pages) no longer crash Plume (see `vendor/wry/PLUME.md`). The root `Cargo.toml` points to it with `[patch.crates-io]`.

## Publish a new version

```powershell
.\release.ps1 1.2.0            # bump the version, build, commit, tag, push and publish the release
.\release.ps1 1.2.0 -DryRun    # preview, change nothing
```

1. Describe the changes under a `## 1.0.1 - YYYY-MM-DD` heading in [`CHANGELOG.md`](CHANGELOG.md), and the same in French in [`CHANGELOG.fr.md`](CHANGELOG.fr.md). The English text becomes the GitHub release notes; both are shown on the website's *Release notes* page (`site-notes.ps1`, run by the script, copies them there).
2. Run the script. It publishes with the [GitHub CLI](https://cli.github.com/) (`gh`) if it is installed. Otherwise it opens the prefilled release page and the `dist` folder, and you drop in the two files. When the notes are too long for the page address, the script copies them to the clipboard instead: paste them into the description.
3. The download link `releases/latest/download/PlumeSetup.exe` always points to the newest release. The script also writes the new version into the website's `js/config.js` (`..\..\PlumeSite`); refresh the site's *What's new* section, its screenshots and this README (`docs/screens`), then push the site.
4. Installed copies see the new version at their next start (right away with *Check now* in Plume Suite) and install it in one click.

The version number has a single source: `version` in `Cargo.toml` (`[workspace.package]`, shared by every app).

## Project layout

```
apps/web/          Plume Web, the browser (Rust, wry + tao + WebView2)
  src/main.rs        windows, tabs, events
  src/settings.json  every setting: default per mode, section, network use (read by settings.rs and the interface)
  src/*.rs           settings, performance, tabs, privacy, look, tools, vault, import, extensions, ad blocker,
                     media downloads, DevTools bridge, setup, default browser, updates (one file per subject)
  src/ui/            the browser interface: HTML, CSS, JavaScript (one feat_*.js per theme)
  src/page.js        script injected into web pages (autofill, reader, dark mode, read aloud...)
  assets/adlist.txt  the ad list            tests/   interface functions (node --test)
apps/suite/        Plume Suite: install, open, update and remove the apps
apps/fex/          Plume FEx, the file explorer (see its README)
apps/viewer/       Plume Viewer, remote control from a phone (see its README); relay/ is its Cloudflare Worker
apps/music/        Plume Music, the music player (see its README)
apps/drop/ apps/launcher/ apps/passwords/ apps/media/ apps/task/ apps/clean/ apps/backup/ apps/clipboard/
apps/pdf/ apps/calendar/ apps/convert/ apps/notepad/ apps/cut/ apps/create/ apps/boost/
                   the tools, one native program each (each README lists what it does and how it is tested)
apps/mail/         Plume Mail, the mail app (see its README; not in the installer yet)
crates/plume-ui/   the native interface kit of every app but the browser (Win32, Direct2D, DirectWrite)
crates/fex-core/   FEx without interface: listing, index, change journal, query language
crates/mail-core/  Mail without interface: IMAP, SMTP, messages, OAuth, the kept mailbox
crates/plume-update/  the update check and download shared by every app
crates/plume-defaults/  "Set as default": the file types an app can open instead of Windows' own apps
launcher/          PlumeSetup.exe: holds the programs and installs, updates and removes them
assets/brand/      every logo and icon (SVG sources, PNG, ICO)
tools/             icon_res.rs (icons and version info of every .exe), site-notes.ps1 (release notes for the website)
vendor/wry/        wry 0.53.5 with two small changes (see vendor/wry/PLUME.md)
docs/              the suite plan, screenshots
build.ps1          builds dist\PlumeSetup.exe        release.ps1   publishes a version
```

## Credits

Built with [mail-parser](https://github.com/stalwartlabs/mail-parser), [zip](https://github.com/zip-rs/zip2), [sevenz-rust2](https://github.com/hasenbanck/sevenz-rust2), [schannel](https://github.com/steffengy/schannel-rs), [regex-automata](https://github.com/rust-lang/regex) and [memchr](https://github.com/BurntSushi/memchr), [wry](https://github.com/tauri-apps/wry) and [tao](https://github.com/tauri-apps/tao) (Tauri), [webview2-com](https://github.com/wravery/webview2-rs), [windows-rs](https://github.com/microsoft/windows-rs), [serde_json](https://github.com/serde-rs/json), [RustCrypto](https://github.com/RustCrypto) `aes-gcm`, `argon2`, `sha1` and `sha2`, and [miniz_oxide](https://github.com/Frommi/miniz_oxide).

- QR codes follow the ISO/IEC 18004 algorithm as described by Project Nayuki's reference implementation.
- Optional online services: [Have I Been Pwned](https://haveibeenpwned.com/Passwords) (Pwned Passwords), [Frankfurter](https://www.frankfurter.app/) (exchange rates), [Open-Meteo](https://open-meteo.com/) (weather), Bing (picture of the day).

- The ad list in `assets/adlist.txt` is based on [Peter Lowe's ad server list](https://pgl.yoyo.org/adservers/); see that site for its terms of use.
- `WebView2Loader.dll` is redistributed from Microsoft's WebView2 SDK.
- yt-dlp and FFmpeg are not bundled. They are downloaded from their official sources only when you use video download, and keep their own licenses.

## License

[MIT](LICENSE)
