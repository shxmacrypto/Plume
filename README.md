<p align="center">
  <img src="assets/logo256.png" width="112" alt="Plume logo">
</p>

<h1 align="center">Plume</h1>

<p align="center">
  <b>The browser that weighs nothing.</b><br>
  An ultra-light web browser for Windows, written in Rust — one ~2 MB file.<br>
  <i>Le navigateur qui ne pèse rien — ultra-léger, écrit en Rust, un seul fichier de 2 Mo.</i>
</p>

<p align="center">
  <a href="https://github.com/shxmacrypto/plume/releases/latest/download/Plume.exe"><b>⬇ Download Plume.exe</b></a>
  ·
  <a href="https://github.com/shxmacrypto/plume/releases">All releases</a>
  ·
  <a href="CHANGELOG.md">Changelog</a>
</p>

---

## Features

- **Tiny** — a single `Plume.exe` (~2 MB). It renders pages with the WebView2 (Microsoft Edge) engine already on Windows, so it ships no browser engine of its own.
- **Memory saver** — background tabs are suspended, then unloaded after a few minutes (unless they play audio). Restored tabs sleep until you open them.
- **Ad & tracker blocker** — ~3,600 domains plus cosmetic filters, a per-site toggle and Edge tracking prevention.
- **Encrypted passwords** — a vault locked by one master password (Argon2id + AES-256-GCM). Plume offers to save logins, fills them for you and imports CSV exports.
- **Import** — history, favorites and their icons from Chrome, Edge, Brave, Opera, Opera GX, Vivaldi and Firefox.
- **Chrome extensions** — install from the Chrome Web Store, pin them in the toolbar, open them as floating popups or in a side panel.
- **Tabs your way** — groups, pinned tabs, vertical tabs, multiple windows, InPrivate windows, drag to reorder.
- **Reading** — reader mode, read aloud, page translation, dark mode per site.
- **Video & audio download** — MP4, WEBM, MKV, MP3, WAV, FLAC… via yt-dlp + FFmpeg, installed on demand.
- **Plume DevTools** — a docked panel with console, elements, network, storage and performance.
- **For everyone** — French & English, light/dark themes, text size, high contrast, reduced motion, full keyboard support.

## Install

1. Download [`Plume.exe`](https://github.com/shxmacrypto/plume/releases/latest/download/Plume.exe) and run it. There is no installer: Plume unpacks itself to `%LOCALAPPDATA%\Plume\app` on first launch.
2. Windows SmartScreen may warn about an *unknown publisher* because the exe is not code-signed: click **More info → Run anyway**. Each release ships a `Plume.exe.sha256` file so you can check the download.

Requires Windows 10 or 11 (64-bit) with the **WebView2 Runtime**, which is preinstalled on Windows 11 and on up-to-date Windows 10.

Plume tells you when a new version is available (Settings › About). To uninstall, delete `Plume.exe` and the `%LOCALAPPDATA%\Plume` folder.

## Privacy

Plume has no account and no telemetry. Your history, favorites and settings stay in `%LOCALAPPDATA%\Plume`, and passwords are encrypted there. Plume itself only goes online when you ask it to, plus:

| Request | When | Can be turned off |
|---|---|---|
| `api.github.com` — latest release | at most once a day | Settings › About › Check for updates automatically |
| Chrome Web Store download | when you install an extension | — |
| yt-dlp (GitHub) and FFmpeg (gyan.dev) | the first time you download a video, after you agree | — |

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
   The result is `dist\Plume.exe` plus `dist\Plume.exe.sha256`.

Run the tests with `cargo test --release`. Tests that need the network are marked `#[ignore]`.

## Publish a new version

```powershell
.\release.ps1 0.6.0            # bump the version, build, commit, tag, push and publish the release
.\release.ps1 0.6.0 -DryRun    # preview, change nothing
```

1. Describe the changes under a `## 0.6.0` heading in [`CHANGELOG.md`](CHANGELOG.md). That text becomes the release notes.
2. Run the script. It publishes with the [GitHub CLI](https://cli.github.com/) (`gh`) if it is installed. Otherwise it opens the prefilled release page and the `dist` folder, and you drop in the two files.
3. The download link `releases/latest/download/Plume.exe` always points to the newest release, so the website never changes. Existing users are told about the update inside Plume.

The version number has a single source: `version` in `Cargo.toml`.

## Project layout

```
src/main.rs        windows, tabs, events, settings (Rust, wry + tao + WebView2)
src/ui/            the browser interface: style.css, body.html, i18n.js, app.js, settings.js, devtools.js, boot.js
src/page.js        script injected into web pages: password autofill, reader mode, dark mode, read aloud
src/vault.rs       encrypted password vault
src/import.rs      Chromium / Firefox profile import (sqlite.rs reads their databases with Windows' winsqlite3.dll)
src/ext.rs         Chrome Web Store download + CRX/ZIP unpacking        src/bicon.rs  real browser icons
src/adblock.rs     blocker (assets/adlist.txt)                          src/media.rs  yt-dlp / FFmpeg downloads
src/devtools.rs    bridge to the Chrome DevTools Protocol for Plume DevTools
launcher/          tiny launcher that embeds plume.exe + WebView2Loader.dll into one Plume.exe
assets/            logo, icon, ad list
```

## Credits

Built with [wry](https://github.com/tauri-apps/wry) and [tao](https://github.com/tauri-apps/tao) (Tauri), [webview2-com](https://github.com/wravery/webview2-rs), [serde_json](https://github.com/serde-rs/json), [RustCrypto](https://github.com/RustCrypto) `aes-gcm` and `argon2`, and [miniz_oxide](https://github.com/Frommi/miniz_oxide).

- The ad list in `assets/adlist.txt` is based on [Peter Lowe's ad server list](https://pgl.yoyo.org/adservers/); see that site for its terms of use.
- `WebView2Loader.dll` is redistributed from Microsoft's WebView2 SDK.
- yt-dlp and FFmpeg are not bundled. They are downloaded from their official sources only when you use video download, and keep their own licenses.

## License

[MIT](LICENSE)
