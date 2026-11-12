# Plume Notepad

A fast plain-text editor for Windows that can take the place of Windows Notepad, with sticky pads that stay over
every window. Part of Plume Suite. Native (Direct2D + DirectWrite through `plume-ui`), nothing goes online.

## The editor

- Tabs; open, save, save as, save all; drag and drop files on the window; files on the command line.
- Encodings: UTF-8 with or without BOM, UTF-16 LE / BE (with a BOM, or found by its zero bytes), and the ANSI
  code page of the PC (Windows-1252 in western Europe) when the bytes are not valid UTF-8. "Reopen as" reads the
  file again in another encoding; "Save as" another encoding from the status bar. Saving characters ANSI does not
  have asks first (save anyway, or as UTF-8).
- Line endings: CRLF, LF or CR, found when opening (the most used one) and chosen from the status bar.
- Find and replace: match case, whole words, regular expressions (`$1` and `${name}` for groups, `\n` and `\t` in
  the replacement), "3 of 12", F3 / Shift+F3, replace all in one undo step. Go to line (`12` or `12:5`).
- Word wrap (Alt+Z), line numbers, zoom (Ctrl+wheel, Ctrl+plus / minus / 0), fixed-width or sans-serif font,
  status bar (line, column, characters, selection, zoom, line endings, encoding), F5 inserts the time and date.
- Its own text control: caret, selection with keyboard and mouse (double-click word, triple-click or gutter line),
  undo / redo by words, clipboard, Tab / Shift+Tab on several lines, input methods (the composition window
  follows the caret), scroll bars. Only the rows on screen are laid out: a 33 MB, 300 000-line log opens in about
  0.1 s and scrolls and types in a few milliseconds.
- Nothing is lost: the text of tabs that are not saved is written to `tabs\<id>.txt` at most 1.5 s after a change,
  and the open tabs come back at the next start (closing the window never asks). Closing a single tab with
  changes asks "Save changes?".

## Replacing Windows Notepad

Settings › Windows Notepad:

- **Open Plume Notepad instead of Windows Notepad** writes `Debugger = "<exe>" --ifeo` under
  `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\notepad.exe` (the method
  Notepad++ documents). Windows asks for an administrator's permission: Plume Notepad starts a copy of itself
  with `--set-default on|off` through the UAC prompt. Windows then starts Plume Notepad with the path of
  notepad.exe and notepad's own arguments after `--ifeo`; that path and the switches `/A /W /P /PT` are skipped,
  and a file name with spaces may come unquoted. On Windows 11 the key also holds filters for the Store Notepad
  (subkeys with `FilterFullPath`): the ones for notepad.exe get the same value. Turning it off removes only Plume
  Notepad's values, and the key only if Plume Notepad created it.
- **Default app for text files**: registers a ProgID (`PlumeNotepad.txt`), "Open with" entries for .txt .log .ini
  .md .csv .json .xml .cfg .conf .nfo and the app's capabilities (HKCU), then opens Windows' Default apps page
  (Windows lets only the user choose the default).

## Sticky pads

Small coloured notes (yellow, green, pink, blue, purple, sand), each its own window: over every window (pin to
turn it off per pad), resizable, minimal bar (new, colour, pin, menu, close), saved as they are typed
(`pads.json`). Closing a pad keeps it in the list near the clock; Ctrl+D or the menu deletes it (asks if it has
text). New pad: the toolbar, Ctrl+Shift+N, the icon near the clock, or **Ctrl+Alt+N anywhere**. "Open my pads
when I sign in" adds `HKCU\...\Run` value `Plume Notepad` = `"<exe>" --background`; while pads are open (or that
is on) closing the main window only hides it.

## Command line

| | |
|---|---|
| `plume-notepad.exe [file…]` | the window; a running Plume Notepad opens the files in tabs (WM_COPYDATA, mark `PlumeNotepadWindow`) |
| `--pad` | a new sticky pad |
| `--background` | the pads and the icon near the clock only (the Run value) |
| `--ifeo <notepad.exe> [args]` | started by Windows in place of notepad.exe |
| `--set-default on\|off` | (administrator) the notepad.exe redirection on or off; exit code 0 if done |
| `--unregister` | for the uninstaller: removes the Run value, the file types and (through the UAC prompt if it is on) the notepad.exe redirection, and makes the running Plume Notepad quit |

Data: `%LOCALAPPDATA%\Plume\Notepad` (settings.json, session.json, tabs\, pads.json).

## Tests

- `cargo test -p plume-notepad`: editing, undo merging, words, encodings and line endings, find / replace /
  regex, wrapping and tab stops, session and backups, the registry plans on a test root under
  `HKCU\Software\PlumeNotepadUnitTest` (never the real keys), command lines.
- `--features test-hook` build + `PLUME_NOTEPAD_TEST_PORT`: a JSON-lines socket on 127.0.0.1 (see
  `src/test_hook.rs`). In that build the registry roots are always test roots and no UAC prompt is shown.
- Environment: `PLUME_NOTEPAD_DIR` (data folder), `PLUME_NOTEPAD_TEST_PORT`, `PLUME_NOTEPAD_NOACTIVATE`
  (windows never take the focus), `PLUME_NOTEPAD_HOTKEY=0` (no Ctrl+Alt+N), `PLUME_NOTEPAD_TEST_HKLM` and
  `PLUME_NOTEPAD_TEST_HKCU` (test registry roots under HKCU), `PLUME_SUITE_THEME`, `PLUME_SUITE_LANG`,
  `PLUME_TEST_SAVE` / `PLUME_TEST_FILES` (the file dialogs, from plume-ui).
