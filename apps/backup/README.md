# Plume Backup

Automatic backups of your folders to a USB drive, another disk or folder, or a network share (`\\PC\Backups`).
A light native Windows app of the Plume Suite, in English and French.

## What it does

- **Plans.** Each plan has its folders to back up, a destination, a schedule and a mode.
- **Schedules.** Every hour, every day from a chosen hour, when the drive is plugged in, or only when you ask.
  A USB drive is recognized by its volume serial number, so it is found again when Windows gives it another letter.
  After a failed or stopped run, a schedule waits 15 minutes before trying again.
- **Two modes.**
  - *Versions*: each run is a complete dated copy (`versions\2026-11-11 14-30-05\…`). Files that did not change are
    hard links to the previous copy, so they take no extra space (on a drive that can't do hard links, FAT32 or
    exFAT, they are copied). Keep N versions, and/or versions up to X days old; the newest one always stays.
  - *Mirror*: one copy (`current\…`) kept identical to the folders.
- **Incremental.** A file is compared by size and modified time; when only the time differs, a SHA-256 of both copies
  decides (pure Rust, `sha2`).
- **Resumes.** Files are written under a temporary name (`*.plume-part`) and renamed once complete. An interrupted
  versions run stays in `versions\<date>.partial` and the next run carries on from it.
- **Leaves out** patterns you choose (`*.iso, node_modules, Videos/Raw`) and, unless you turn it off, temporary files
  (`*.tmp`, Office `~$` files, `Thumbs.db`, unfinished downloads…). Links and junctions are not followed.
- **Optional check**: each copied file is read back and compared with the original.
- **Restore view**: pick a plan, a date (versions, the mirror, and the files removed from the sources), browse, then
  restore a file or a folder to its place or to another folder.
- **In the background**: an icon near the clock (open, back up all now, quit); closing the window keeps it running
  (a setting). Start with Windows is a setting (`--background` in the Run key of the current user).
- **Notifications** on success and on failure (the icon's silent balloon, a toast on Windows 10 and 11).

At the destination:

```
<destination>\Plume Backup\<plan>\
    versions\<date>\<source>\…      versions mode
    current\<source>\…              mirror mode
    deleted\<date>\<source>\…       files that disappeared from the sources
```

Each copy holds a small `.plume-backup.json` (date, which source each folder came from), so the backups can be opened
with any file explorer, and restored to their place even after the plan is gone.

## Safety rules

- The folders being backed up are only ever **read**. Nothing is written, moved or deleted there by a backup.
- A source folder that is missing (renamed, drive unplugged) **stops the run**: it is never read as "everything was
  deleted". A folder that can't be read keeps its previous copy.
- A file removed from a source goes to `deleted\<date>` at the destination **before** any cleanup of the mirror; a
  mirror file is never simply erased.
- Only folders inside the plan's own folder at the destination are ever removed (old versions, by the retention
  rules; old `deleted` folders, after the days chosen, 90 by default, or never).
- A plan whose destination is inside one of its sources, or a source inside the backups, is refused.
- Restoring never overwrites: a different file already in place is first renamed
  `name (before restore <date>).ext`; an identical one is left alone.
- Deleting a plan leaves its backups on the destination.

## What goes online

Nothing. Plume Backup makes no network request of its own. A network share as destination is reached through
Windows' own file sharing, like any folder.

## Files

`%LOCALAPPDATA%\Plume\Backup\settings.json` (language, theme, background, notifications) and `plans.json` (the plans
and how their last run went).

## Command line

```
plume-backup.exe                 the window
plume-backup.exe --background    no window, only the icon near the clock (start with Windows)
plume-backup.exe --run           backs up every plan now (the running Plume Backup does it if there is one)
plume-backup.exe --unregister    no more start with Windows, and the running Plume Backup quits (uninstall)
```

## Tests

```
cargo test -p plume-backup
```

The engine's tests (`src/engine.rs`) run on fresh folders in the system's temp folder: first and incremental runs,
the hash when only the date differs, versions and hard links, mirror and the `deleted` folder, retention by number and
by age, expiry of removed files, resume after an interruption, no half-written file left under its real name,
excludes and temporary files, a missing source, refused plans, restore to its place and elsewhere, browsing, two
sources with the same name, verification. Also the schedules (`ui/mod.rs`), dates and drives (`sys.rs`), texts.

A `--features test-hook` build can be driven through a JSON socket on 127.0.0.1 (see `src/test_hook.rs`). Variables:

| Variable | What |
|---|---|
| `PLUME_BACKUP_TEST_PORT` | the port of the test socket (test-hook builds only) |
| `PLUME_BACKUP_DIR` | the data folder instead of `%LOCALAPPDATA%\Plume\Backup` |
| `PLUME_BACKUP_RUN_KEY` | the registry key used for "Start with Windows" (test-hook builds only) |
| `PLUME_BACKUP_NOACTIVATE` | the window opens without taking the focus |
| `PLUME_TEST_FOLDER` | what the folder dialog answers (test-hook builds; settable with `{"cmd":"env"}`) |
| `PLUME_SUITE_LANG`, `PLUME_SUITE_THEME` | `en`/`fr`, `dark`/`light` |

In a test run, notifications are written to the state's log instead of being shown.
