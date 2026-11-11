# Plume PDF

A fast PDF reader and editor for Windows, part of Plume Suite. App key `pdf`, crate `plume-pdf`.

## What it does

**Reading**
- Pages drawn by Windows' own PDF engine (`Windows.Data.Pdf`) on a thread of their own, decoded by WIC and shown
  with Direct2D. Only the pages near the screen are drawn; while the zoom changes the last picture is stretched,
  and the page is drawn sharp again once the zoom settles.
- Continuous scrolling, zoom (Ctrl+wheel around the pointer, Ctrl+= / Ctrl+-, Ctrl+0 for 100 %), fit width
  (Ctrl+2), whole page (Ctrl+1), a page box to jump to a page, thumbnails and the outline (bookmarks) in the sidebar.
- Dark pages (colors turned around, the file is not changed), tabs, recent files, the last page read in each file.
- Files open from the command line (`plume-pdf.exe a.pdf b.pdf`, so it can be the default PDF app), by dropping
  them on the window, or from a second launch (the open window takes them).
- Search in the text (Ctrl+F, Enter / F3 for the next match, Shift for the previous): matches are highlighted.
  The text and the place of each character are read from the page's drawing instructions (lopdf).
- Password-protected files ask for their password. A copy saved from them is protected again with the same password.
- Print (Ctrl+P): Windows' print dialog, then each page drawn at the printer's resolution (300 dpi at most).

**Editing** (pure Rust, `lopdf`): nothing is written without being asked. "Save a copy…" (Ctrl+Shift+S) writes a new
file; Save (Ctrl+S) on the original asks first ("Replace the original file?", with "Save a copy…" offered).
- Merge PDFs (into the open file, or into a new one from the start screen), split into files by page ranges
  ("1-3, 4-6, 7"), save chosen pages as a new file, reorder pages by dragging thumbnails, rotate, delete.
- Annotations written as real PDF annotation objects (`/Annot`, each with its own appearance stream, so every
  reader shows them): highlight (snaps to the text under the drag), sticky note, freehand ink, text box and a
  **signature** (drawn once in a pad, kept on this computer, then placed with a click, moved and resized).
- Form filling: AcroForm text fields and checkboxes (values and appearances written, `NeedAppearances` set).
- Undo (Ctrl+Z) for annotations, form values and page changes. Delete removes the selected annotation or pages.

**Settings**: theme (system, dark, light), language (English, French), zoom when a file opens, dark pages,
reopen files where they were left, the signature, and a button that opens Windows' default apps page (Plume PDF
never changes file associations itself).

## What goes online

Nothing. Plume PDF makes no network request. Settings, recent files, last pages and the signature stay in
`%LOCALAPPDATA%\Plume\PDF` (`settings.json`, `state.json`, `signature.json`).

## Files

| File | What |
| --- | --- |
| `src/main.rs` | start, single window, command line |
| `src/render.rs` | the render thread (Windows.Data.Pdf → WIC → pixels), dark pages |
| `src/edit.rs` | lopdf: page geometry, order, rotation, merge, split, extract, annotations, forms, outline, save |
| `src/text.rs` | positioned text of each page: search, highlight snapping |
| `src/print.rs` | print dialog and printing |
| `src/ui/` | the window: state and events (`mod.rs`), painting (`paint.rs`), settings, drawing helpers and icons |
| `src/i18n.rs` | French (English is in the code); a test checks every `tr("…")` has its French |
| `src/settings.rs` | settings, memory, signature |
| `src/test_hook.rs` | the test socket (feature `test-hook` only) |

## Build and test

```
cargo build --profile fast -p plume-pdf
cargo test -p plume-pdf
cargo clippy -p plume-pdf --all-targets
cargo build --release -p plume-pdf
```

Only pure-Rust crates (lopdf without its default features: no chrono, no rayon).

### Driving a test build

`cargo build --profile fast -p plume-pdf --features test-hook`, then start `plume-pdf.exe` with:

| Variable | Use |
| --- | --- |
| `PLUME_PDF_TEST_PORT` | port of the JSON test socket on 127.0.0.1 (also allows a second window) |
| `PLUME_PDF_DIR` | data folder instead of `%LOCALAPPDATA%\Plume\PDF` |
| `PLUME_PDF_NOACTIVATE` | the window opens without taking the focus |
| `PLUME_SUITE_LANG`, `PLUME_SUITE_THEME` | `en`/`fr`, `dark`/`light` |
| `PLUME_TEST_FILES`, `PLUME_TEST_SAVE`, `PLUME_TEST_FOLDER` | answers of the open / save / folder dialogs (plume-ui) |

The socket's commands are listed at the top of `src/test_hook.rs` (`make_samples` writes test PDFs: a report with
an outline, an appendix, a form and a protected file, password `plume`). A test build never opens the print
dialog and never prints.
