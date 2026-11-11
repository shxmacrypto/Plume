# Plume FEx

A fast, native file explorer for Windows 10 and 11, with the look of Plume and instant search of the whole PC.
Part of Plume Suite: install it from the Plume Suite app.

- Hand-built interface in Rust: Win32, Direct2D and DirectWrite. No web view. One 2.7 MB program.
- Folders open in a few milliseconds; the list stays current while files change.
- Instant search as you type, in the open folder or everywhere, with Everything's search syntax: words,
  phrases, wildcards, regular expressions, types of files, sizes, dates, duplicates, empty folders.
- Search inside files too (`content:` or *Search inside files* in the filters): text, code, documents.
- No administrator rights: the index follows the NTFS change journal. USB sticks, memory cards and network
  drives are indexed when they appear and read again from time to time.
- A preview pane (Alt+P) for pictures, text and folders, and a thumbnails view (Ctrl+Shift+2).
- Undo (Ctrl+Z) for a rename, a new folder, a move or paste, and a delete to the Recycle Bin.
- Windows does the file work: the real Explorer context menu, Recycle Bin, copy and move dialogs, drag and drop.
- Archives: extract ZIP, 7z, RAR and tar; create ZIP or 7z, with a password (AES-256) if you want one (a 7z hides the file names too).
- Folder sizes, straight from the index.
- Servers over SFTP (beta), through the OpenSSH client that ships with Windows, with the progress of each
  transfer.
- Optional: "Open in Plume FEx" in the right-click menu of folders, and a switch that makes folders and Win+E
  open in Plume FEx (off by default, reversible).
- English and French; dark and light.

## Build

Rust (stable, `x86_64-pc-windows-gnu`) with `windres` and `dlltool` on the `PATH`, as for Plume Web.

```bash
cargo build --release -p plume-fex
```

The program is `target/release/plume-fex.exe`. `cargo build --profile fast -p plume-fex` compiles faster
while developing.

## Use

```bash
plume-fex.exe "C:\Some\Folder"
```

Starting it again while it runs opens the folder in a new tab of the same window.

| Keys | Action |
|---|---|
| Alt+Left, Alt+Right, Alt+Up, Backspace | back, forward, parent folder, back |
| Ctrl+L, Alt+D, F4 | edit the address (a folder, or `sftp://user@host`) |
| Ctrl+F, Ctrl+E, F3 | search |
| Ctrl+T, Ctrl+W, Ctrl+Tab, Ctrl+1..9 | new tab, close tab, switch tab |
| F2, Delete, Shift+Delete | rename, recycle, delete for good |
| Ctrl+C, Ctrl+X, Ctrl+V, Ctrl+Shift+C | copy, cut, paste, copy as path |
| Ctrl+Shift+N | new folder |
| Ctrl+Z | undo the last change |
| Alt+P | preview pane |
| Ctrl+Shift+2, Ctrl+Shift+6 | thumbnails, details |
| Alt+Enter | Properties |
| Ctrl+H | show or hide hidden items |
| F5, Ctrl+R | refresh |
| Menu key, Shift+F10 | context menu |

The `…` menu holds the settings: hidden items, extensions, folder sizes, theme, language, the connection to a
server, and the Windows integration. In a search, the button on the right of the "Search in" bar chooses a
type of items and how the text is matched, and exports the results.

The search syntax, the measurements and the design notes are in [docs/suite/PLAN.md](../../docs/suite/PLAN.md).

### Servers (SFTP, beta)

`…` > *Connect to a server*, or type `sftp://user@host` in the address bar. Plume FEx drives the `sftp.exe` of
Windows (Settings > System > Optional features > OpenSSH Client, installed by default), so your SSH keys, your
`~/.ssh/config` and your known hosts work as usual. Passwords are asked in a Plume dialog and are not saved.
On a server, Delete asks first: there is no recycle bin there.

## Tests

```bash
cargo test -p fex-core -p plume-fex
```

The UI scenarios drive the real window without touching the mouse or the keyboard:
see [tests/ui/README.md](tests/ui/README.md).

## Layout

```
src/app/     the explorer: state, events, painting, loading, search, actions, servers, settings, strings
src/shell/   Windows: icons, places, menus, file operations, drag and drop, archives, SFTP, integration
tests/ui/    UI scenarios
```

The interface kit is [`crates/plume-ui`](../../crates/plume-ui), shared with Plume Mail. The parts without an
interface (folder listing, sorting, index, query language, change journal) are in
[`crates/fex-core`](../../crates/fex-core).
