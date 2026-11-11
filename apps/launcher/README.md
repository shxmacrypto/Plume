# Plume Launcher

Press **Alt+Space** anywhere: a search box comes up over everything. Type, and the results come as you type.

- **Apps**: the Start menu's shortcuts (for everyone and for you) and the apps of Windows' Apps folder, Store apps included.
- **Files and folders**: in your own folders (Desktop, Documents, Downloads, Pictures, Music, Videos, OneDrive). The index
  is Plume FEx's (`crates/fex-core`), built for these folders only, without admin rights; folders full of generated files
  (`node_modules`, Rust's `target`, folders starting with a dot) are left out. It is kept on disk and rebuilt in the
  background every half hour, or with **Rebuild** in the settings.
- **Windows settings**: about 65 pages (Wi-Fi, Bluetooth, display, dark mode, default apps…), found by their name or by the
  words people type, in English and in French.
- **Calculator**: `12*3+15%`, `20% of 150`, `2^10`, `sqrt(2)`, `5!`, and units: `5 km in mi`, `70 f to c`, `1.5 GB in MB`,
  `2 h in min`, `10 kg en lb`. Enter copies the result.
- **Web**: when nothing else fits, the last line searches the web with the default browser (Google, Bing, DuckDuckGo or Ecosia).

What you open comes first the next time: results picked often and lately go up, and a query remembers what you picked for it.
The empty box shows your recent picks.

| Key | Does |
| --- | --- |
| Enter | opens the selected result (copies a calculation) |
| Ctrl+Enter | opens the result's folder, with it selected |
| Up, Down, Tab, Page Up, Page Down | move |
| Esc | closes the box (so does clicking elsewhere, or the shortcut again) |

The launcher runs in the background with an icon near the clock (click: the box; right click: Settings, Quit) and starts
with Windows (turned on at the first start; a setting). The settings window: the shortcut (click it, press the new one),
the sources on or off, the search engine, the theme, the language, the start with Windows, the history.

```
plume-launcher.exe                 starts (if needed) and opens the box
plume-launcher.exe --background    starts with only the icon near the clock (start with Windows)
plume-launcher.exe --settings      opens the settings window
plume-launcher.exe --unregister    no more start with Windows; the running launcher quits (uninstall)
```

## What goes online

Nothing, unless you pick the web search line: then your default browser opens the search engine's page with what you typed.
Apps, files, settings and calculations stay on the PC. No update check of its own (the Plume Suite hub does that).

## Where things are

`%LOCALAPPDATA%\Plume\Launcher`: `settings.json`, `history.json` (your picks), `files.bin` (the file index). Start with
Windows: the `Plume Launcher` value of `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

## How it stays fast

The box's window is created once, hidden, and drawn once out of sight at start (`sys::warm_up`), so the shortcut only
places it, shows it and paints it: 2 to 35 ms from the shortcut to the first image on this PC. Apps are read on a worker
thread at start (and again when the box opens if the list is more than 3 minutes old); their icons are fetched ahead of time,
on another thread, the icons of what is on screen first. File searches run on the index's own thread; what you typed
before is dropped when you type more.

## Building and testing

```
cargo build --profile fast -p plume-launcher [--features test-hook]
cargo test -p plume-launcher
```

The unit tests cover the calculator, the matching and the ranking (frecency, remembered queries, sources on and off), the
settings pages in both languages, the shortcut names, the file index (generated folders left out) and the French strings.

The `test-hook` build opens a test socket when `PLUME_LAUNCHER_TEST_PORT` is set (commands in `src/test_hook.rs`). In that
mode nothing is opened, copied or registered for real: what would have been opened is listed in `state.actions`, and the
global shortcut is only taken if `PLUME_LAUNCHER_HOTKEY` names one (never Alt+Space in tests).

| Variable | Use |
| --- | --- |
| `PLUME_LAUNCHER_TEST_PORT` | test socket port (test-hook builds only) |
| `PLUME_LAUNCHER_DIR` | data folder instead of `%LOCALAPPDATA%\Plume\Launcher` |
| `PLUME_LAUNCHER_FILE_ROOTS` | folders to index, separated by `;` (empty: none) |
| `PLUME_LAUNCHER_APPS_DIR` | one folder of shortcuts instead of the Start menu, and no Apps folder |
| `PLUME_LAUNCHER_HOTKEY` | the shortcut to take in tests, like `Ctrl+Alt+Shift+F11` |
| `PLUME_LAUNCHER_RUN_KEY` | registry key (under HKCU) used instead of the Run key; tests without it use `Software\Plume\LauncherTestRun` |
| `PLUME_SUITE_THEME`, `PLUME_SUITE_LANG` | theme (`dark`, `light`) and language (`en`, `fr`) of the Plume Suite |
