<p align="center"><img src="../../assets/brand/plume-clean-256.png" width="96" alt="Plume Clean logo"></p>

<h1 align="center">Plume Clean</h1>

<p align="center"><b>See what fills your drives, and free space safely.</b></p>

Plume Clean shows your drives with their used and free space, scans a drive or a folder in a few seconds, draws a
map of what takes the space, and suggests what can go, each suggestion explained in plain words. Nothing is ticked
for you, and what you remove goes to the Recycle Bin. It is a native Windows app (Rust, Direct2D): no web view, no
account, nothing online.

- **Drives**: every fixed and USB drive, with a bar of its used and free space.
- **Scan** a drive or one folder. Several threads read the folders at once with Plume FEx's fast listing (one call
  for up to ~2,000 entries); links and junctions are shown but not followed, and files that live only in OneDrive
  count for nothing.
- **Map**: each folder and file of the current folder as a block as big as what it holds, the biggest folders
  opened one level. Click a folder to go in; the path above goes back up. Next to it, a list of the folder's
  content or of the 500 biggest files under it, sortable by name, size or date. Open a file, show it in Explorer,
  open the folder in Plume FEx, copy its path.
- **Cleanup**: temporary files, the Recycle Bin (emptied after you confirm), the caches of Edge, Chrome, Firefox and
  Plume Web (only while the browser is closed), the thumbnail cache, crash reports, old files in Downloads, large
  files, duplicate files and empty folders from the last scan. Windows' own temporary files, crash dumps and Windows
  Update leftovers need administrator rights: the card says so and *Restart as administrator* asks Windows first.
- **Duplicates**: same size first, then the same first 64 KB, then the same SHA-256 of the whole file. Hard links of
  one file are not counted as duplicates. *Tick all copies but the oldest* keeps one of each.
- **Settings**: theme (system, dark, light), language (English, French), Recycle Bin on or off, how old an old
  download is, from which size a file is large and duplicates are looked for.

## What it never touches

- Nothing is removed without a tick and a confirmation; a scan only reads.
- Never a drive root, the Windows folder, Program Files, ProgramData, AppData, the user folder itself, or anything
  inside a folder whose name starts with a dot (`.git`…): these are never offered as large files, duplicates or empty
  folders, and the removal itself refuses drive roots and those top folders.
- Files in use are left in place (the result says how many).
- Browser caches only: history, passwords, cookies and sign-ins stay.

## What goes online

Nothing. Scans are kept in memory only. The settings stay in `%LOCALAPPDATA%\Plume\Clean`.

## Good to know

- Sizes are the files' sizes (decimal units, like Windows' drive bars). A file with several hard links (common in
  `C:\Windows\WinSxS`) is counted once per name, so the scan of a system drive can add up to a bit more than what
  the drive bar says; folders the app cannot read are marked.
- Removing to the Recycle Bin frees the space only once the Recycle Bin is emptied (the app says so).

## Layout

```
src/scan.rs        the parallel scan (std threads over fex-core's listing) into one flat tree
src/clean.rs       the suggestions, the places they look at, the removal (IFileOperation) and its safety rules
src/dupes.rs       duplicate files (size, first 64 KB, SHA-256), hard links set aside
src/drives.rs      drives and their space
src/ui/            the window: Drives, Map (treemap + list), Cleanup (cards + review), Settings
```

## Tests

```
cargo test -p plume-clean
cargo build --profile fast -p plume-clean --features test-hook
```

The unit tests cover the scan's sizes and file counts, the treemap layout, duplicates (content, hard links), the
suggestion rules (old downloads, protected folders, empty folders, blocked browsers and administrator rights) and a
whole cleanup, always in a temporary folder.

The test build answers JSON commands on 127.0.0.1 when `PLUME_CLEAN_TEST_PORT` is set (`state`, `hits`, `click`,
`dclick`, `rclick`, `key`, `scan`, `go`, `set`). Other variables:

| Variable | |
|---|---|
| `PLUME_CLEAN_DIR` | the settings folder |
| `PLUME_CLEAN_SANDBOX` | every cleanup place lives inside this folder (`LocalAppData\Temp`, `Windows\…`, `Downloads`, a stand-in `RecycleBin`); nothing outside it can be removed, removals there are permanent and the real Recycle Bin is never filled nor emptied |
| `PLUME_CLEAN_SANDBOX_RUNNING` | in the sandbox, the programs to treat as running (`msedge.exe,chrome.exe`) |
| `PLUME_CLEAN_SANDBOX_ADMIN` | in the sandbox, `1` acts as if running as administrator |
| `PLUME_CLEAN_NOACTIVATE` | the window opens without taking the focus |
| `PLUME_TEST_FOLDER` | what *Choose a folder…* answers in a test build |

A test build never restarts as administrator and never opens Explorer or other programs (it writes what it would
have done in the `log` of `state`).
