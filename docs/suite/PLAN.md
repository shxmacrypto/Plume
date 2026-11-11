# Plume suite: plan

Plume is becoming a suite of small, fast, native Windows apps with one look:

- **Plume Web**, the browser (the original Plume);
- **Plume FEx**, a file explorer with instant search of the whole PC;
- **Plume Mail**, a mail app.

They will ship in one installer named *Plume*, with a checkbox per app. This file records the decisions, what
is built and what comes next. It lives on the `suite` branch, which is not published yet.

## Decisions

| Topic | Decision |
|---|---|
| Interface of the new apps | Fully native and hand-built (Win32 + Direct2D + DirectWrite), from one shared kit. No WebView2. They must look like Plume and start instantly. |
| FEx search | Everything-style: whole-PC index, results as you type, the same search syntax. |
| Administrator rights | None, anywhere. The first index is a parallel folder walk; afterwards the NTFS change journal keeps it current. |
| FEx in Windows | "Open in Plume FEx" in the right-click menu, and an optional switch (off by default, reversible) that makes folders and Win+E open in Plume FEx. |
| Archives | Open and create ZIP (with AES-256 passwords) in the program itself; other formats through Windows' own `tar.exe`; RAR with a password through WinRAR when it is installed. |
| SFTP | Through the OpenSSH client that ships with Windows: nothing to install, and the encryption, keys and known hosts are Windows' own. |
| Mail | Messages never go through an embedded browser. An HTML message is shown as text: nothing is downloaded, nothing runs. Passwords are kept by Windows' Credential Manager. |
| Logos | Plume Web keeps the original feather on blue. The suite, FEx and Mail each have their own mark from the same family (`assets/brand`). |
| Installer | One `PlumeSetup.exe` with the apps inside and a checkbox per app. No web downloader. |
| Languages | English and French. |
| Publishing | Nothing is pushed until the suite is ready. |

### Why no administrator rights

Measured on Windows 11 without elevation: a volume handle opened with `FILE_TRAVERSE | SYNCHRONIZE` accepts
`FSCTL_QUERY_USN_JOURNAL` and `FSCTL_READ_UNPRIVILEGED_USN_JOURNAL`. The records carry file IDs, parent IDs,
reasons and times, but **no file names**. Reading the master file table (`FSCTL_ENUM_USN_DATA`, what Everything
uses for its instant first index) is refused.

So Plume FEx stores the NTFS file ID of every folder it walks. To refresh, it reads the journal, collects the
parent folders that changed and lists only those again. If the journal no longer reaches back to the last
read, it walks the drive again in the background.

## Repository layout

```
Cargo.toml            workspace root (the browser package still lives at the root for now)
rustfmt.toml          the code style, applied by `cargo fmt --all`
apps/
  fex/                Plume FEx (plume-fex.exe)
    src/app/          the explorer: state, events, painting, loading, search, actions, servers, settings, strings
    src/shell/        Windows: icons, places, menus, file operations, drag and drop, archives, SFTP, integration
    tests/ui/         UI scenarios that drive the real window (see its README)
  mail/               Plume Mail (plume-mail.exe)
    src/app/          the window: state, events, painting, dialogs, settings, strings
    tests/ui/         end-to-end scenario against a mail server that lives in the test
crates/
  plume-ui/           the interface kit shared by the apps: window, canvas, text, icons, text fields, dialogs
  fex-core/           no interface: folder listing, sorting, index, query language, change journal
  mail-core/          no interface: IMAP, SMTP, message reading and writing, the local mailbox, the account worker
assets/brand/         logos of the suite, Plume FEx and Plume Mail (SVG sources, generated PNG and ICO)
docs/suite/           this plan
src/, launcher/ ...   Plume Web and its installer, unchanged for now
```

Planned for the suite phase: move the browser to `apps/browser`, turn `launcher/` into the suite installer
(`installer/`), and share the update code in `crates/`.

## What is built

Everything below runs and is covered by tests: **80 unit tests** (`cargo test -p plume-ui -p fex-core
-p mail-core -p plume-fex -p plume-mail`), **220 checks** in the Plume FEx UI scenarios and **45 checks** in
the Plume Mail scenario.

### Plume FEx

- **Window**: Plume look in dark and light, the real Windows frame (shadow, rounded corners, snapping, the
  snap layouts of Windows 11 on the maximise button), tabs, toolbar, breadcrumbs that turn into an address
  field, search field, side pane, details list with columns you can resize, status bar.
- **Browsing**: folders are read on a worker thread and shown sorted like Explorer (natural order, accents);
  history per tab with the selection restored; type-ahead; keyboard and mouse selection like Explorer,
  including the rubber band; Windows icons with exact icons for programs and shortcuts.
- **Live refresh**: the open folder is checked again in the background and redrawn only if it changed. No
  handle is kept on the folder.
- **File work**: the Explorer context menu itself (Open with, Send to, other programs' entries), cut, copy,
  paste, delete, Properties, rename in place, new folder, copy as path, drag out, drop in.
- **Archives**: extract here or to a folder (ZIP, 7z, RAR, tar, gz and what Windows' `tar.exe` reads); create
  ZIP or 7z; ZIP and 7z with a password (AES-256; a 7z hides the names too) read and written in the program; RAR
  with a password through WinRAR.
- **Comfort**: preview pane (Alt+P: pictures decoded by WIC, text, folder contents), thumbnails view
  (Ctrl+Shift+2, Windows' own thumbnails), undo (Ctrl+Z) for rename, new folder, paste or move, and delete to the
  Recycle Bin (restored from its `$I` and `$R` files).
- **Instant search**: an index of every fixed drive; results as you type, from the open folder or everywhere;
  the search syntax below; a filter button (type of items, match case, whole word, path, regular expression);
  sorting by name, folder, date or size over all results; export of every result to CSV. The index leaves
  memory after 90 seconds without a search and comes back in half a second. `content:` (or the filter menu)
  searches inside files: text, code and documents up to 64 MB, binaries and offline files skipped, checked
  last so the name tests narrow the work first. USB sticks, cards and network drives join the index when they
  appear, are read again every 15 minutes and leave the results when unplugged.
- **Folder sizes** (optional): the size of each folder, straight from the index; sorting by size then ranks
  folders and files together.
- **Servers (SFTP, beta)**: connect with `user@host`, browse, upload (menu, paste or drop), download, rename,
  new folder, delete (after a confirmation: a server has no recycle bin). The status bar shows the progress of a
  transfer (bytes received for a download, items sent for an upload).
- **Windows integration**: "Open in Plume FEx" on folders, and the optional default-explorer switch with a
  backup of what it replaces.
- **One window per profile**: a second launch hands its folders to the running window as new tabs.
- **Settings**: `%LOCALAPPDATA%\Plume\FEx\settings.json`. Language, theme and accent follow Plume Web until
  set here; hidden files and extensions follow Explorer until set here.

### Plume Mail

- **Accounts**: IMAP to read, SMTP to send, over TLS done by Windows itself (SChannel) with Windows'
  certificate checks. The servers of the usual providers are filled in from the address. The password goes to
  Windows' Credential Manager, never to a file. **Sign in with Microsoft** (OAuth 2 with PKCE, the browser
  coming back to a loopback address): the lasting token takes the password's place, split over several
  Credential Manager entries when long, and IMAP and SMTP sign in with XOAUTH2.
- **Push**: a second connection per account waits on the inbox (IDLE, renewed every 25 minutes) and new mail
  shows up within seconds; without IDLE, the inbox is still checked every three minutes.
- **Conversations** from `Message-ID` and `References` (a union of everything a message cites), one line per
  conversation with a count, opened under its head. **Drafts** kept in the server's Drafts folder (APPEND with
  `\Draft`, replaced by number with UIDPLUS, removed once sent). **Signature** per account. **Address
  suggestions** from the people in the kept mailbox. **Search** in the text: the kept messages first, then
  `UID SEARCH TEXT` on the server, which also brings older messages into the list.
- **Mailbox**: folders (the usual ones named in the interface language), the 500 most recent messages of a
  folder, unread and flagged marks, attachments, search in the folder (accents and case ignored).
- **Reading**: text with wrapped lines; an HTML message is turned into readable text (lists, links, quotes)
  without loading images or running anything; attachments can be saved; links are listed and open in the
  browser; the text can be copied.
- **Writing**: new message, reply, reply to all, forward, attachments; correct headers for names with accents
  and for threads; a copy in the Sent folder.
- **Actions**: mark read or unread, flag, delete (to the Trash, then for good after a confirmation).
- **Offline**: folders, lists and the messages already read are kept in `%LOCALAPPDATA%\Plume\Mail` and shown
  before the connection is even made.

### The shared kit (`plume-ui`)

A frameless window that keeps the real Windows frame, Direct2D drawing, DirectWrite text (one line or wrapped),
Plume's theme and vector icons, hit regions per frame, a native `EDIT` control placed over the window for text
entry (one line, password, or several lines), Windows' file dialogs, the clipboard.

## Search syntax (Plume FEx)

By default neither case nor accents count: `ete` finds `Été`.

| Example | Meaning |
|---|---|
| `report 2026` | both words, any order |
| `"final report"` | the exact phrase |
| `jpg\|png` | one or the other (binds tighter than a space: `main rs\|cpl` is main and (rs or cpl)) |
| `!draft`, `!<a\|b>` | without this word; without this group |
| `<photo 01>\|readme` | angle brackets group terms |
| `*.rs`, `photo??.jpg` | wildcards, on the whole name |
| `regex:^img_\d+\.jpe?g$` | regular expression |
| `src\main`, `path:projects` | in the full path |
| `ext:jpg;png` | by extension |
| `audio:`, `video:`, `pic:`, `doc:`, `exe:`, `zip:` | by type of file |
| `size:>10mb`, `size:1mb..1gb`, `size:empty`, `size:huge` | by size |
| `dm:today`, `dm:thisweek`, `dm:last2weeks`, `dm:>2026-01-01`, `dm:2026-10` | by date modified |
| `file:`, `folder:` | only files, only folders |
| `attrib:h` (letters `r` `h` `s` `d` `l` `o`) | by attributes |
| `len:>100`, `depth:2`, `root:`, `parent:C:\folder` | name length, depth, at the root of a drive, directly inside a folder |
| `empty:`, `childcount:>100`, `childfilecount:0`, `childfoldercount:1`, `child:setup.exe` | folders by what they contain |
| `dupe:`, `namepartdupe:`, `sizedupe:`, `dmdupe:` | duplicates by name, name without extension, size, date |
| `startwith:`, `endwith:`, `ww:`, `wfn:` | start, end, whole word, whole file name |
| `case:`, `diacritics:` | match case; make accents count |
| `count:100` | no more than this many results |

Prefixes can be stacked: `file:case:regex:^Main`.

## Measured on the development PC (1.79 million items, NVMe, 20 threads)

| What | Result |
|---|---|
| List `C:\Windows` (115 items) | 2.6 ms |
| List a folder of 3,000 files | 5 ms |
| First index of the whole drive | 6 to 13 s with a warm disk cache, about 50 s cold (right after a boot) |
| Search ready at later launches | 0.5 s (saved index, then the journal) |
| Typical search (`plume`, `report`, `ete`) | 3 to 4 ms |
| Regular expression over every name | 4 to 11 ms |
| Worst case, one letter (1.27 million results) | 24 ms |
| `dupe:` over the whole drive | about 120 ms |
| A file created anywhere on the drive appears in results | about 1 s |
| Plume FEx memory while searching / once the index has left memory | about 160 MB / 70 MB |
| Index file | 84 MB |
| `plume-fex.exe` / `plume-mail.exe` (release) | 2.7 MB / 1.1 MB |
| Plume Mail memory, idle | about 45 MB |
| An SFTP command on an open session (listing, rename) | 40 to 100 ms plus the network |

## Architecture notes

- **Drawing**: a window repaints entirely from its state with Direct2D; every drawn control registers a hit
  region for the frame. Nothing is repainted while nothing changes.
- **Text fields** are native `EDIT` controls placed over the window, so IME, selection and the clipboard behave
  exactly as in Windows.
- **Anything that can open a dialog or a menu** runs after the event handler has returned (`Cx::after`), so the
  window keeps painting during modal loops.
- **Windows does the file work**: `IContextMenu` verbs for cut, copy, paste, delete and Properties,
  `IFileOperation` for rename, `SHDoDragDrop` to drag, the folder's shell `IDropTarget` to drop.
- **No folder handles.** An open handle on a folder makes Windows refuse to rename any of its parent folders,
  so the open folder is polled with a content signature, and the index opens the volume only for the time of a
  journal read.
- **Index** (`fex-core::index`): flat arrays, about 50 bytes per file. Names sit end to end in one buffer, each
  ending with a line feed, so one substring search over the whole buffer cannot straddle two names; there is no
  lowercase copy. An element stores only its parent's number; paths are rebuilt on demand. A precomputed
  alphabetical rank (Windows' own collation) lets results come out sorted without comparing names.
- **Queries** (`fex-core::query`): an expression tree. The longest word every result must contain is searched
  in one sweep over the name buffer; names with accents are checked apart. Tests that need the whole index
  (paths, duplicates, folder contents) are computed once before the sweep. Each thread has its own regular
  expression scratch memory: sharing it made regular expressions ten times slower.
- **SFTP** (`apps/fex/src/shell/sftp.rs`): one `sftp.exe` stays open per server and receives commands on its
  standard input. Its answers are framed by the echo of each command; because Windows' `sftp.exe` writes its
  output in 4 KB blocks, each command is followed by a few `help` commands that push the answer out. When the
  server asks for a password, `ssh` starts the program named by `SSH_ASKPASS`: Plume FEx itself, which passes
  the question to the window through a named pipe.
- **Mail** (`mail-core`): each account has a worker thread that owns the IMAP connection. The window sends it
  commands and receives news; everything it learns is written to disk first. Message headers are fetched as
  raw header blocks and read by the same parser as whole messages.
- **Tests**: with the `test-hook` feature and a port in the environment, a local socket reports the interface
  state and injects events. Menus and Windows dialogs are recorded or answered from the environment instead of
  shown; drags are recorded instead of performed. That feature is never part of a release build.
- **Code checks**: `cargo clippy --workspace --all-targets` reports nothing and `cargo fmt --all -- --check`
  prints nothing, for the six packages, the browser included. The style is in `rustfmt.toml` (160 columns, and
  whatever fits on one line stays on one line): it is the style the code already had, so adopting the tool moved
  about one line in a hundred. `vendor/wry` keeps its own layout (`vendor/wry/rustfmt.toml` turns the tool off
  there).
- **The browser on this branch**: its source differs from `main` only by what those two tools produced, plus
  four small fixes by hand (`Shortcut` type and a number in `tabs07.rs`, one initializer in `bicon.rs`, one sort
  in `priv07.rs`) and one real fix (below). If `main` moves before the branches are joined: take `main`'s
  version of the browser files, then run `cargo clippy --fix` and `cargo fmt --all` again.
- **Crash at start on battery (found 2026-11-10, fixed here and on `main` for release 0.7.2)**: Plume Web 0.7.0 and 0.7.1
  stop at once ("index out of bounds") when started on battery with the battery saver on (the default), or on
  a metered connection with preloading on: `update_power` runs before the session is restored and asks
  `push()` to refresh the tab bar while there is no tab yet. Fix: `push()` returns while `tabs` is empty
  (`src/main.rs`). Checked on battery, with a throwaway profile: the original code crashes on a first launch
  and with a saved profile, the fixed code starts and closes cleanly. The same three lines are on `main`,
  with the 0.7.2 notes; they reach users when `release.ps1 0.7.2` is run there.

## Known limits

Plume FEx:

- Fixed drives without a change journal (FAT, exFAT) are indexed at the first launch but not followed
  afterwards; removable and network drives are read again every 15 minutes rather than followed live.
- The list shows the first 10,000 results of a search (the total is always counted, the export has them all).
- Search inside files reads plain text (UTF-8, UTF-16): it does not open the inside of Office documents or PDFs.
  Search does not know dates of creation or last access.
- The tab strip does not reorder. Undo covers the last changes made in FEx itself, not those of other programs.
- Creating a RAR needs WinRAR; a RAR password with characters outside ASCII goes through WinRAR's command line.
- SFTP is a beta. Tested against a real SFTP server program on the development PC, without network or
  password: **the password prompt of a real server has not been tested**. A new server's key is accepted and
  remembered at the first connection; a changed key is refused. An upload replaces a file of the same name.
- The default-explorer switch was tested on a scratch registry key only.

Plume Mail:

- *Sign in with Microsoft* needs Plume Mail's application ID at Microsoft, built in with
  `PLUME_MAIL_MS_CLIENT_ID`; it was tested against a fake sign-in page, **not yet against Microsoft itself**.
- Gmail, Yahoo and iCloud need an app password. Google only allows its own sign-in page for Gmail after a paid
  security audit, and limits unaudited apps to 100 users who must sign in again every 7 days.
- No calendar, no address book (suggestions come from the mail itself).
- HTML messages are shown as text: no layout, no images.
- Conversations group the messages of one folder (a reply you sent stays in Sent).
- Tested against a mail server written for the tests (IMAP, SMTP, IDLE, SEARCH, XOAUTH2, with and without TLS),
  not against the big providers.

## Roadmap

- [x] **1. Browse**: window and theme, tabs, toolbar with breadcrumbs, side pane, details view, sorting,
      keyboard navigation, live refresh.
- [x] **2. File work**: selection, cut/copy/paste, delete, rename, new folder, context menus, drag and drop,
      Properties, archives with passwords.
- [x] **3. Search**: index, journal refresh, Everything's query language, filters, export, folder sizes.
- [x] **4. Comfort**: thumbnail view, preview pane, undo, search inside files, column resizing, snap layouts,
      folder sizes. *Still open: a settings page, tab reordering.*
- [x] **5. Windows**: right-click entry, default-explorer switch, single-window handoff.
- [ ] **6. Servers**: SFTP out of beta (password prompt verified on real servers, host key shown before it is
      trusted, drag out). *Done: transfer progress.*
- [x] **7. Mail**: sign-in through Microsoft's page, conversation threads, drafts on the server, signatures,
      address suggestions, push, search in the text. *Still open: calendar, an HTML view that stays safe, Google
      sign-in (blocked by Google's audit).*
- [x] **8. Suite**: Plume Suite (install, open, update, remove the apps), one installer holding every app, one
      update for all, website with a page per app, release notes. Released as **1.0.0** with Plume Web and
      Plume FEx; **Plume Mail is held back** (`PLUME_WITH_MAIL=1` adds it to the installer and to Plume Suite)
      until its Microsoft application ID exists. *Still open: the browser moved to `apps/browser`.*

## Third-party code

Everything in `apps/` and `crates/` is original code under the MIT license of this repository. The new apps add
these libraries, all under licenses compatible with it:

| Library | Used for | License |
|---|---|---|
| `windows`, `windows-core`, `windows-numerics` | Windows APIs | MIT or Apache-2.0 |
| `serde_json` | settings and the local mailbox | MIT or Apache-2.0 |
| `memchr` | substring search in the index | Unlicense or MIT |
| `regex-automata` | regular expressions in the search | MIT or Apache-2.0 |
| `zip` | ZIP archives with AES passwords | MIT |
| `schannel` | TLS through Windows | MIT |
| `mail-parser` | reading messages (MIME, encodings) | Apache-2.0 or MIT |
| `sevenz-rust2` | 7z archives with AES passwords | Apache-2.0 |
| `sha2` | the PKCE of *Sign in with Microsoft* | MIT or Apache-2.0 |
