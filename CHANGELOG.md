# Changelog

All notable changes to Plume. Each `## x.y.z` section becomes the release notes when you run `.\release.ps1 x.y.z`.

## 1.1.0 - 2026-11-11

Thirteen new apps join Plume Suite. **Plume Music** plays your music and YouTube Music, with your Spotify and Apple Music playlists. **Plume Viewer** lets you control your PC from your phone. Eleven everyday tools come with them: Drop, Launcher, Passwords, Media, Task, Clean, Backup, Clipboard, PDF, Calendar and Convert.

Already on Plume Suite 1.0? Update it, then install the new apps from Plume Suite in one click.

### Plume Music
- **Everything in one player**: the music files on your PC, any song of YouTube Music (search as you type, with suggestions), SoundCloud, and your own playlists.
- **Import a playlist by pasting its link**: Spotify (playlist, album, song), Apple Music, SoundCloud and YouTube. Spotify and Apple songs are found on YouTube Music by title, artist and length (100 songs in about 25 seconds). A whole Spotify playlist, beyond the 100 songs a shared link shows: select its songs in Spotify (Ctrl+A, Ctrl+C) and click **Paste copied songs**.
- **No waiting**: a song starts in about 3 seconds and the next one is fetched while the current one plays. Gapless playback, crossfade, volume normalization, a 10-band equalizer with presets, shuffle, repeat, sleep timer, and autoplay of similar songs when the queue ends.
- **Synced lyrics** that light up line by line, in a side panel or in full window; click a line to go there.
- **Now playing in full window**: animated covers when Apple Music has one, over a moving background (the cover's colors, the blurred cover, or your animated GIF, which can also go behind the whole window).
- **Downloads** of a song, an album or a playlist, in M4A (no loss) or MP3 320 kbps, with title, artist, album and cover.
- Albums, artists, liked songs, playlists, light and dark themes, an accent color taken from the cover, Windows' media overlay and media keys, English and French.

### Plume Viewer
- **Scan and you're in**: Plume Viewer shows the PC's ID, a one-time code and a QR code. Scan it with the phone's camera (or type the ID and code in any browser), the person at the PC accepts, and the phone sees the screen and uses the mouse and keyboard. Nothing to install on the phone.
- **Fast**: only what changes on the screen is sent, directly from the PC to the phone, encrypted. A slow network gets fewer images, never a growing delay.
- **Made for touch**: tap to click, long press for a right-click, drag, two fingers to zoom or scroll, the phone's keyboard to type, plus Windows key, Esc and full screen. From a computer, the mouse and keys work as on the PC.
- **Permanent password** for your own devices: connect without anyone accepting. Plume Viewer then stays near the clock and can start with Windows.
- **Safe by design**: the code and the password never leave the devices; five wrong tries lock the PC for 5 minutes; a red banner shows who is connected, with a Disconnect button.

### Eleven more apps
- **Plume Drop**: send files, photos, text and links between the PC and your phone. Scan the QR code, and the phone's browser does the rest. Nothing to install on the phone, files go straight to `Downloads\Plume Drop`, and a new phone needs your OK once.
- **Plume Launcher**: press **Alt+Space** and type. It finds apps, the files of your own folders (indexed with Plume FEx's engine), Windows settings pages and quick sums (units and percentages too). The box shows up in a few milliseconds, and recent picks come first.
- **Plume Passwords**: an encrypted vault for logins, 2FA codes (with their countdown), cards and notes. It has a password generator, a report on weak and reused passwords, auto-type into the right window, unlock with Windows Hello, CSV import and encrypted backups. It never goes online.
- **Plume Media**: an instant photo viewer (a 12 MP photo on screen in half a second) and a video player. It adds a timeline by month that stays smooth with thousands of photos, lossless rotation of JPEG photos, photo details, subtitles, audio tracks and resume where you stopped.
- **Plume Task**: the apps and background processes, grouped by program. A card names what slows the PC down, and **End task** stops it. Live graphs show the processor, memory, disk, network and graphics card, and startup apps can be switched on and off. It uses less than 0.1 % of the processor itself.
- **Plume Clean**: a map of what fills each drive, the biggest files, and safe suggestions: temporary files, caches, old downloads, duplicates and empty folders. Every suggestion says what it is and frees nothing until you confirm.
- **Plume Backup**: automatic backups of your folders to a USB drive, another disk or a network share, every hour, every day or when the drive is plugged in. Backups are incremental and versioned (unchanged files take no extra space) and resume after a stop. Nothing is ever deleted at the source, and a file removed from the source is kept in a dated folder. You can restore any version, to its place or elsewhere.
- **Plume Clipboard**: **Ctrl+Shift+V** opens your clipboard history (text, links, pictures, files) next to the cursor, with search, pins, snippets you call with a short trigger (`;sig`) and paste as plain text. Passwords copied from password managers are never kept, and the history can be sealed by Windows for your account.
- **Plume PDF**: read PDFs with search, the outline, thumbnails and dark pages. Highlight, add notes, draw, add text and sign. Merge, split, reorder, rotate and extract pages, and fill forms. Changes are saved as real PDF annotations that other readers show too.
- **Plume Calendar**: month, week, day and agenda views, with drag to create, move and resize. Quick-add understands "Lunch with Max tomorrow 12:30" in English and French. It handles recurring events, reminders, several calendars, `.ics` import and export, and subscriptions to calendar links (school, sports, public holidays).
- **Plume Convert**: drop a video, a song or a picture and pick a preset: for phone, for Discord (10, 25 or 50 MB), smaller file, audio only, GIF, images (WebP, JPEG, PNG, ICO), trim, mute or rotate. It uses the graphics card's encoder when it has one, never overwrites a file, and downloads FFmpeg the first time it's needed.

### Plume Suite
- The thirteen new apps join the list: install, open, update and remove them like the other apps. The window shows them on two columns and scrolls when they don't all fit.

### Plume Music
- Fixed: on a PC without FFmpeg, downloading it (needed for MP3 downloads) failed because the archive was larger than the app allowed.

## 1.0.1 - 2026-11-11

Bigger icons in Plume FEx, folders that open in it once installed, and a fix for the welcome setup on battery.

Already on Plume Suite 1.0.0? Plume Suite and each app show the update: one click installs it.

### Plume FEx
- **Explorer's icon sizes**: extra large, large, medium and small icons beside Details. Choose them with the new **View** button next to the `…` menu, with Ctrl+Shift+1 to 4 (Ctrl+Shift+6 for Details), or with **Ctrl + mouse wheel**. Photos and videos show their real thumbnail at every size; the size is remembered.
- **Installed from Plume Suite, Plume FEx opens your folders**: double-clicking a folder or a drive, and Win+E, open it in Plume FEx. Plume Suite says so and how to undo it: Plume FEx's `…` menu, *Open folders and Win+E with Plume FEx*. Removing Plume FEx gives everything back to Windows' Explorer.

### Plume Web
- Fixed: *Replay the welcome setup* (Settings › About) stayed frozen on a laptop on battery. The battery saver still stops the interface's animations, but no longer the startup screen and the welcome setup; *Reduce motion* still stops everything.

### Inside
- The browser's code moved to `apps/web`, next to the other apps; every logo is in `assets/brand`.

## 1.0.0 - 2026-11-11

Plume becomes **Plume Suite**: small native apps with one look, installed and updated together.

- **Plume Web**, the browser you know.
- **Plume FEx** (new), a file explorer with instant search of the whole PC.
- **Plume Mail** is coming soon: it is ready, and waits for its *Sign in with Microsoft*. Plume Suite already shows it.

Already on Plume 0.7? Accept the update bar as usual: it installs Plume Suite with Plume Web inside, keeps your profile, and you add FEx from the Plume Suite app if you want it.

### Plume Suite
- **One app to install the others**: Plume Suite lists the Plume apps with *Install*, *Open* and *Remove*. Nothing is downloaded: the installer already holds them.
- **One update for everything**: a new version shows up in Plume Suite and in each app (a button in its title bar). One click updates the suite and every installed app, checked against the SHA-256 published with the release.
- **One entry in Windows Settings › Apps**: *Plume Suite*. Uninstalling removes every app; your data stays in `%LOCALAPPDATA%\Plume`.

### Plume FEx (new)
- Folders open in a few milliseconds, and instant search as you type, in the open folder or everywhere, with Everything's search syntax (words, wildcards, regular expressions, types, sizes, dates, duplicates).
- No administrator rights: the index follows the NTFS change journal. USB sticks, memory cards and network drives are indexed too, and leave the results when unplugged.
- **Search inside files** (`content:`, or the filter menu), a **preview pane** (Alt+P), a **thumbnails** view (Ctrl+Shift+2) and **undo** (Ctrl+Z) for rename, new folder, move and delete.
- Windows does the file work: the real Explorer context menu, Recycle Bin, copy dialogs, drag and drop.
- Archives: extract ZIP, 7z, RAR and tar; create **ZIP or 7z with an AES-256 password** (a 7z hides the file names too), or RAR when WinRAR is installed.
- Folder sizes from the index, servers over **SFTP** (beta, with the progress of each transfer), tabs, dark and light, English and French.
- Optional: *Open in Plume FEx* in the folder menu, and a reversible switch that makes folders and Win+E open in Plume FEx.

### Plume Web
- **Tabs between windows**: drag a tab out of the tab bar to open it in a new window, or onto another Plume window to move it there, without reloading. Other tabs slide aside while you reorder.
- **Plume pages as tabs** (Settings › Tabs, off by default): Settings, History, Downloads and the other Plume pages open as real tabs (`plume://settings`…).
- The video and audio download panel opens at once; the analysis fills it in.
- The *Save password* bar survives the redirects that follow a login.
- Fixed: Plume 0.7.0 and 0.7.1 could close at start when the PC ran on battery.

## 0.7.1 - 2026-11-10

Updates now work when GitHub is slow, and you can always see what they are doing.

Already on Plume 0.6 or 0.7? It installs this update by itself: accept the bar, then *Restart now*.

### Updates
- **Checked at every start**: Plume compares its version with the latest release each time it starts, instead of reusing an answer up to a day old, so a new version shows up right away. The check reads where GitHub's "latest release" page points: one light request that doesn't use GitHub's API (limited for anonymous users, and the first part to fail when GitHub is overloaded). The API is only used when you install.
- **No more silent hangs**: requests to GitHub give up after about 15 s without a connection or 30 s without an answer, and are retried once after a server error (5xx), a timeout or a cut connection. An incomplete download is detected and started again. The SHA-256 check of the installer is unchanged.
- **You can see what is happening**: "Checking for updates…" and "Connecting to GitHub…" in the update bar and in Settings › About. After 10 seconds without progress: "GitHub is slow to answer…" with a *Download from GitHub* button.
- **Failures are shown**: a failed check or installation shows the reason, with *Try again* and *Download from GitHub*. Before, a failed installation hid the update bar, so it looked like nothing had happened.
- A check that finishes while an update is installing no longer hides its progress or the *Restart now* button.
- The weather, the image of the day, the leaked-password check and the other online features get the same timeouts.

## 0.7.0 - 2026-11-09

36 new features, reworked downloads, and Plume can now be your default browser. Every feature can be switched off, and a switched-off feature does no work in the background: no timer, no page script, no network call. In **Ultra-light** mode with its default settings, Plume uses about as much memory as 0.6.1.

Already on Plume 0.6? It installs this update by itself: accept the bar, then *Restart now*.

Defaults are written *Ultra-light / Full*. 🌐 = contacts an online service, only when you turn it on.

### Settings
- Rebuilt settings: every option has a one-line description and its own default in Ultra-light and Full mode. Options you change yourself are remembered, so switching modes never overwrites them.
- Search across all settings, reset one section or everything, and new sections: Performance, Tabs, Navigation, Privacy, Appearance, Tools.
- Your 0.6 profile is migrated automatically.

### Performance
- **Memory badge and task manager** (off / on): RAM used by each tab. `Shift+Esc` opens a task manager listing tabs, extensions and WebView2 processes with their CPU and RAM, with *End process* and *Unload*.
- **Battery saver** (on / on): as soon as you unplug, tabs fall asleep sooner and interface animations stop; a leaf shows in the toolbar. It can wait for a battery level instead.
- **Never-sleep list** (on / on): chosen sites, tabs playing sound, tabs with an unsent form and tabs in a call are never put to sleep.
- **Smart preloading** (off / on): hovering a link warms up the connection (or the whole page) so it opens faster; paused on battery or metered connections.
- **Lite mode per site** (available, nothing blocked by default): turn off JavaScript, images, web fonts or video autoplay for one site, with a badge in the address bar.
- **"Plume saved you" statistics** (on / on): memory freed, trackers blocked, data and time saved, on the new tab page and in `plume://stats`. Everything stays on your PC.

### Tabs
- **Workspaces** (off / on): separate sets of tabs (Work, Personal…) with their own icon and color, `Ctrl+Alt+1…9`, optionally their own favorites and a container.
- **Close inactive tabs** (off / off): tabs not visited for a while move to an archive (`plume://archive`) that you can reopen.
- **Split screen** (on / on): two tabs side by side or stacked, adjustable divider, optional synchronized scrolling (`Ctrl+Shift+S`).
- **Tab search** (on / on): fuzzy search in open tabs, recently closed tabs and the archive (`Ctrl+Shift+A`).
- **Peek** (off / on): `Shift`+click (or long press) opens a link in a floating preview without leaving the page.
- **Sessions** (on / on): save a window or a tab group as a session, reopen, rename, export and import it; the last session is kept automatically.
- **Tab notes** (off / on): a note per page or per site, encrypted with your password vault when you have one.

### Navigation
- **Command palette** (on / on): `Ctrl+K` for actions, tabs, favorites, history and settings in one place.
- **Calculator and conversions in the address bar** (on / on): `12*7`, `10 km in miles`, `20 °C to F`; currencies 🌐 (daily rates from frankfurter.app).
- **Search a site with Tab** (on / on): type the start of a site such as `yout`, press `Tab`, and the address bar searches YouTube directly. Works with about 20 popular sites (Wikipedia, Amazon, GitHub, Reddit, Twitch…).
- **Mouse gestures** (off / off): right-click and draw to go back, forward, reload, close or reopen a tab, with a visible trail.
- **Mouse side buttons and touchpad swipe** (on / on) for back and forward.

### Privacy
- **Containers** (off / on): tabs with their own cookies and sign-ins, to use two accounts at once. A colored line marks them; right-click a tab to reopen it in a container or to always open a site in one; a workspace can use a container; extensions are added to each container.
- **Clear data when closing** (off / off): cookies, site storage, cache and history deleted when Plume closes, or when the last tab of a site closes, except for the sites you keep.
- **Privacy report** (on / on): click the shield to see the trackers blocked on this site (with their company), other sites contacted, cookies, permissions granted and HTTPS.
- **Password generator and audit** (on / on): a strong password (or passphrase) is offered on sign-up forms; Passwords shows weak and reused ones.
- **Data breach check** 🌐 (off / off): Have I Been Pwned with k-anonymity, so only 5 characters of a SHA-1 hash leave your PC; optional check when a password is saved.
- **Windows Hello** (off / off): unlock your passwords with your PIN, fingerprint or face, once per unlock or before every autofill. The master password always works too.
- **Secure DNS** 🌐 (off / off): DNS over HTTPS with Cloudflare, Quad9, NextDNS, Google or your own server, applied at the next start.

### Appearance
- **Adaptive tint** (off / on): the toolbar takes the color of the site. **Glass effect** intensity can be adjusted.
- **New tab background**: gradient (default), animated gradient (stops on battery), solid color, your own picture with blur and dimming, or picture of the day 🌐 (Bing, off by default).
- **New tab widgets** (none / clock and most visited sites): clock, weather 🌐 (Open-Meteo, only with a city you enter), to-do list and most visited sites; add, remove and reorder them.
- **Custom CSS and JavaScript per site** (off / off): page menu › *Customize this site*. CSS applies as the page loads, even on sites with a strict security policy. JavaScript must be allowed separately and shows a clear warning. Export and import.
- **Compact density** and **Zen mode** (off / off): Zen hides the tabs and address bar; they come back when the mouse touches the top of the window, with `Ctrl+Shift+Z`, or automatically in full screen.
- **Full screen**: `F11`, and videos that go full screen now fill the whole screen.

### Tools
- **Automatic picture-in-picture** (off / on): leave a tab playing a video and it keeps playing in a floating window; it returns when you come back. Sites can be excluded.
- **Media control** (on / on): a toolbar button appears when a tab plays sound (play / pause, next, go to tab), a speaker icon on tabs mutes them, and tabs opened in the background can start muted.
- **Background tabs**: `Ctrl`+click or middle-click on a link now opens it in the background, as in Chrome.
- **Focus mode and Pomodoro** (off / off): blocks a list of sites for 25 minutes with a timer in the toolbar and a gentle "come back in 12 min" page, breaks in between, *5 more minutes* if allowed, and a sound at the end.
- **Quick developer tools** (on / on): color picker with a magnifier (copied as HEX, RGB or HSL) and ruler, from the command palette and Plume DevTools.
- **QR code of the page** (on / on): generated on your PC, no online service, from an icon in the address bar, with *Copy image*.
- **Save as PDF or Markdown** (on / on): from the page menu; Markdown uses the reader-mode text.
- **Read later** (on / on): saves the article text and pictures for offline reading in `plume://reading-list`, with read / unread and automatic cleanup.

### Downloads
- **Save as each time** (on / on): choose the folder and the name of each file before it downloads; Plume remembers the last folder you picked. Turn it off in Settings › General to save straight to your Downloads folder.
- **Downloads bubble**: the toolbar button opens your latest downloads right under it, with their progress, *Open*, *Show in folder*, *Cancel* and *All downloads*. It also opens for a moment when a download starts.
- **No more empty tabs**: a link that only downloads a file no longer leaves an empty tab behind.
- **Downloads keep going** when you close their tab or when Plume puts the tab to sleep (they used to stay stuck at *in progress*).
- **Closing Plume** while a download is running now asks you first.

### Default browser
- **Make Plume your default browser**: offered at the end of the welcome setup. Windows opens its settings on Plume, where you click *Set default*.
- **Reminder** (on / on): a small bar suggests it about once a week until Plume is your default browser, with *Don't ask again*.
- **Settings › General** shows whether Plume is your default browser, with a button to make it so (installed version).
- **Links from other apps**: links and `.html` files opened from other apps open in a new tab of the Plume window that is already open, instead of starting a second Plume.

### Fixes
- The shield, reader and other icons in the address bar now respond to the first click (the bar used to hide them as it took focus).
- Open menus, settings and pages no longer replay their animation or miss a click when a tab updates in the background.
- Opening a local `.html` file could crash Plume.

### Under the hood
- Plume now builds with a small patched copy of wry (`vendor/wry`) that adds WebView2 profiles (used by containers) and no longer crashes on messages from local pages.
- Unit tests for settings migration, calculator and conversions, fuzzy search, password generation, k-anonymity, QR codes, secure DNS and containers: `cargo test --release` and `node --test tests/ui07.test.js`.

## 0.6.1 - 2026-11-06

- **Fix**: Windows Defender wrongly flagged `PlumeSetup.exe` 0.6.0 as `Trojan:Win32/Bearfoos.A!ml` (a machine-learning false positive) and deleted it. The installer no longer checks for a running Plume through a named mutex, the pattern that triggered it.
- `PlumeSetup.exe` and `plume.exe` now show their name, publisher and version in Properties › Details, and declare that they run without admin rights.
- Plume 0.6.0 installs this update by itself: accept the bar, then *Restart now*.

## 0.6.0 - 2026-11-06

- **Installer**: `PlumeSetup.exe` replaces the single-file `Plume.exe`. Plume installs for your account in `%LOCALAPPDATA%\Programs\Plume` without admin rights, adds Start menu and desktop shortcuts, and appears in Windows Settings › Apps, where you can uninstall it (keeping your data or not).
- **Updates in one click**: when a new version is out, a bar tells you. Plume downloads the installer from GitHub, checks its SHA-256, installs it in the background, and *Restart now* reopens your tabs in the new version.
- Your profile, favorites and passwords carry over from 0.5 automatically.

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
