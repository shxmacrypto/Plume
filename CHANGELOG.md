# Changelog

All notable changes to Plume. Each `## x.y.z` section becomes the release notes when you run `.\release.ps1 x.y.z`.

## 0.5.0 - 2026-11-06

First public release.

- **Ultra-light browser** for Windows 10/11, written in Rust on the system WebView2 (Edge) engine — a single ~2 MB `Plume.exe`.
- **Memory saver**: background tabs are suspended, then unloaded after a few minutes; restored tabs sleep until opened.
- **Ad & tracker blocker** (~3,600 domains + cosmetic filters, per-site toggle) and Edge tracking prevention.
- **Encrypted password vault** (Argon2id + AES-256-GCM) with save prompt and autofill; CSV import.
- **Import** history, favorites and their icons from Chrome, Edge, Brave, Opera, Opera GX, Vivaldi and Firefox.
- **Chrome extensions** from the Chrome Web Store, toolbar pinning, floating popups and side panel.
- **Tabs**: groups, pinned tabs, vertical tabs, InPrivate windows, multiple windows, drag to reorder, move tab to a new window.
- **Reader mode**, read aloud, page translation, per-site dark mode.
- **Video & audio download** (MP4, WEBM, MKV, MP3, WAV, FLAC…) through yt-dlp + FFmpeg, installed on demand.
- **Plume DevTools** docked panel (console, elements, network, storage, performance).
- Welcome setup (name, Ultra-light / Full mode, import), French & English UI, accessibility settings.
- Update check: Plume tells you when a new release is available on GitHub.
