# Plume FEx UI tests

These scenarios drive the real Plume FEx window and check what it does: browsing, selection, sorting, live
refresh, renaming, the Explorer context menu, copy and paste, drag and drop targets, archives (with and without
a password), instant search and its whole syntax, the filter button and the export, the index following the
disk and leaving memory when idle, folder sizes, column widths, the maximise button as Windows sees it, the
Windows integration, servers over SFTP, and the single-window handoff. 220 checks in 14 scenarios.

They never use the real mouse or keyboard. A test build of Plume FEx (Cargo feature `test-hook`) listens on
`127.0.0.1` when `PLUME_FEX_TEST_PORT` is set, and the scripts send it events and read back its state. In that
build:

- the window opens without taking focus (`PLUME_FEX_NOACTIVATE`);
- menus are not displayed: their entries are recorded and one is "picked";
- a drag is recorded, never performed;
- Windows' file dialogs do not open: they answer from `PLUME_TEST_SAVE`, `PLUME_TEST_FOLDER` and
  `PLUME_TEST_FILES`;
- settings and the search index live in a throwaway folder (`PLUME_FEX_PROFILE`), and the index covers only the
  test folders (`PLUME_FEX_INDEX_ROOTS`);
- the registry is not touched: the Windows integration writes under a scratch key (`PLUME_FEX_CLASSES_ROOT`);
- for SFTP, the window talks to a real SFTP server program run on this PC (`PLUME_FEX_SFTP_DIRECT`), through
  Windows' own `sftp.exe`, without network or password.

The `test-hook` feature is never part of a release build.

## Run

```bash
cargo build --profile fast -p plume-fex --features test-hook
```

```bash
python apps/fex/tests/ui/run.py
```

Each scenario can also be run alone, for example `python apps/fex/tests/ui/test_search.py`. Fixtures, profiles
and screenshots go to `target/fex-ui/`.

`bench_disk.py` is not a test: it indexes the real fixed drives into a throwaway profile and prints timings
(first index, restart from the saved index, a few searches).

## Notes

- `test_files.py` copies a file with Ctrl+C / Ctrl+V and "Copy as path": it overwrites the clipboard.
- Deleting local files is not exercised (it would put test files in the real Recycle Bin).
- `test_sftp.py` needs Windows' OpenSSH client and the `sftp-server.exe` of Git for Windows; without them it
  is skipped. It cannot check the password prompt of a real server.
- `test_archive_passwords.py` checks RAR only when WinRAR is installed.
- Needs Python 3.8 or later and Windows 10 or 11.
