# Plume Clipboard

A fast, private clipboard history for Windows: everything you copy (text, links, pictures, files), pins you keep
forever, snippets you type often, and a small panel that pastes them into any app.

## What it does

- **Watches the clipboard** the moment it changes (Windows tells it: `AddClipboardFormatListener`, no polling) and
  keeps text (with its HTML and RTF formatting), links (detected), pictures (a PNG and a small PNG thumbnail, made
  with Windows Imaging Component) and copied files (their paths).
- **The panel**: **Ctrl+Shift+V** (any shortcut you like, registered with `RegisterHotKey`: no keyboard hook) opens a
  small window near the text cursor, or the mouse. Type to search, arrows to choose, **Enter** pastes into the app
  you were in (the item goes on the clipboard, that app gets the keyboard back, Plume presses Ctrl+V with
  `SendInput`), **Shift+Enter** pastes the text without its formatting, **Del** removes, **1–9** paste at once,
  **Ctrl+P** pins, **Esc** closes.
- **Pins**: kept forever, in the order you choose (arrows on the Pinned page). Right-click an item to pin it.
- **Snippets**: named texts with a trigger like `;sig`. Type the trigger in the panel and Enter pastes the snippet.
- **The window**: the whole history with filters (all, text, links, pictures, files, pinned) and a search, the
  snippet editor, and the settings (shortcut, how many items and how long, largest picture, pictures and files on or
  off, encryption, pause, ignored apps, paste after picking, start with Windows, theme, language).
- **Limits**: items kept (100 to 5000), days kept (1 day to always), largest picture (2 to 50 MB). Copying the same
  thing again moves it to the top instead of adding it twice. Pins never count and never expire.
- Closing the window keeps it running with its icon near the clock (Open, Open the panel, Pause, Quit).
  It can start with Windows, in the background.

## What is never kept

- Anything an app marks as private. Password managers add `ExcludeClipboardContentFromMonitorProcessing`, or set
  `CanIncludeInClipboardHistory` or `CanUploadToCloudClipboard` to 0; `Clipboard Viewer Ignore` counts too. Plume
  does not even read such content.
- Copies made in the ignored apps (by exe name of the clipboard's owner). By default: KeePass, KeePassXC, 1Password,
  Bitwarden and Plume Passwords.
- Anything copied while it is paused, texts over 4 MB, and pictures over the size you chose.

## Where it lives, what goes online

- Everything stays in `%LOCALAPPDATA%\Plume\Clipboard`: `history.dat` (sealed by Windows for your account with DPAPI,
  the default) or `history.json`, `images\` (the pictures, sealed the same way), and `settings.json`.
- **Nothing goes online.** Plume Clipboard opens no network connection.

## Running it

```
plume-clipboard.exe                 the window
plume-clipboard.exe --background    only the icon near the clock (start with Windows)
plume-clipboard.exe --panel         opens the panel of the running Plume Clipboard
plume-clipboard.exe --unregister    no more start with Windows, and the running Plume Clipboard quits (uninstall)
```

## Building and testing

```
cargo build --release -p plume-clipboard
cargo test -p plume-clipboard
cargo build --profile fast -p plume-clipboard --features test-hook
```

The unit tests cover the history (dedupe, limits, pins and their order, search, snippets, the panel's list, saving
plain and sealed), the clipboard formats (private formats, link detection, file lists), pictures (PNG, thumbnails,
DIBs both ways), the shortcut's parsing, the panel's placement, and that every text has its French. They never touch
the real clipboard.

A `test-hook` build is driven by a script through a JSON socket on 127.0.0.1 (see `src/test_hook.rs`). The test
environment keeps the user's clipboard, keyboard and settings out of it:

| Variable | Effect |
| --- | --- |
| `PLUME_CLIPBOARD_TEST_PORT` | the test socket's port (test builds) |
| `PLUME_CLIPBOARD_DIR` | the data folder instead of `%LOCALAPPDATA%\Plume\Clipboard` |
| `PLUME_CLIPBOARD_HOTKEY` | the shortcut instead of the setting (`none`: no shortcut; a test build then still shows the configured one, for pictures of the window) |
| `PLUME_CLIPBOARD_FAKE_INPUT` | a file: clipboard writes and Ctrl+V are written there instead of done (test builds) |
| `PLUME_CLIPBOARD_RUN_KEY` | the registry key for "start with Windows" (test builds) |
| `PLUME_CLIPBOARD_NOACTIVATE` | windows and panel show without taking the keyboard |
| `PLUME_CLIPBOARD_LISTEN=0` | the clipboard is not watched (the user's copies stay out of the test) |
| `PLUME_SUITE_THEME`, `PLUME_SUITE_LANG` | theme and language |

The socket's `roundtrip` command is the one moment a test touches the real clipboard: it saves every format on it,
writes an item (marked private, so Windows' own clipboard history skips it), reads it back, and puts everything
back as it was, checking that it is identical.
