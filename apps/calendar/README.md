# Plume Calendar

A clean calendar that works offline, native on Windows (Rust, Direct2D through `plume-ui`), in English and French.

## What it does

- **Views**: month (six weeks, all-day and multi-day bars, "N more"), week and day (hour grid, all-day row, the
  now line, side-by-side overlapping events), agenda (the next 60 days, day by day). Mini month in the sidebar,
  Today button, week numbers (optional).
- **Keyboard**: ← → (or Page Up / Page Down) previous / next, ↑ ↓ scroll (month: a week), T today, M W D A the
  views, N new event, Q (or Ctrl+F) the quick-add box, Enter / Delete on the selected event, Esc closes.
- **Events**: title, start and end or all day, place, notes, color, calendar, reminder. Created by dragging on the
  grid (or a double click, or the New event button), moved and resized by dragging (week and day), moved to other
  days by dragging (month, all-day row). A click shows the details; a double click opens the card.
- **Quick add** (title bar): "Lunch with Max tomorrow 12:30", "Dentiste jeudi à 14h", "Réunion lundi prochain de
  10h à 11h30", "Holiday 24/12 all day", "Concert Nov 20 8:30pm @ Le Zénith". English and French words are both
  understood, whatever the interface language (see `src/quickadd.rs` for the rules).
- **Repeating events**: the RRULE subset calendars use — DAILY, WEEKLY with BYDAY, MONTHLY by day of the month or by
  weekday (2nd Tuesday, last Friday), YEARLY, INTERVAL, COUNT, UNTIL, BYMONTH, BYSETPOS, WKST, EXDATE, changed
  occurrences (RECURRENCE-ID). Occurrences are computed only for the range on screen, on the wall clock of the
  event's zone (a 9:00 meeting stays at 9:00 across daylight saving changes). Changing, moving or deleting an
  occurrence asks "this event / all events".
- **Reminders**: per event (default for new events in Settings). Shown in the window when it is in front, else as a
  Plume popup in the corner of the screen (above the others, without taking the keyboard) or a Windows
  notification (Settings). Snooze 5 minutes, open the event's day. When the window is closed, Plume Calendar stays
  near the clock (icon menu: Open, Quit); "Start with Windows" starts it in the background. A reminder that fell due
  less than 15 minutes before Plume Calendar started is still shown.
- **Calendars**: several local calendars (show / hide, colors, rename, export .ics, delete), **subscriptions** to
  published calendars (an .ics or webcal:// address: school, sports, public holidays) — read-only, downloaded again
  every hour. **Import** of .ics files (from the + menu, the command line, or a second launch): up to three events go
  to the first calendar you can write to, a whole calendar becomes a new one.
- **Time zones**: events keep their UTC instant and the zone they were written in (TZID), and are shown in the PC's
  zone through Windows' time zone data. IANA names ("Europe/Paris") and Windows names ("Romance Standard Time") are
  both understood (a table maps the common IANA names); an unknown zone described by a VTIMEZONE keeps its standard
  offset. Exported files carry a VTIMEZONE built from Windows' rules.
- **Accounts later**: `model::Provider` is what an account provider (Google, Outlook) will implement; today only
  `IcsUrl` does.
- **Settings**: language, theme, first day of the week, 12 / 24-hour clock, week numbers, default reminder, reminder
  style, keep running near the clock, start with Windows.

## What goes online

Only the calendars you subscribe to: Plume Calendar downloads their address (WinINet, Windows' proxy settings, no
cookies) when you subscribe and then every hour while it runs. Nothing else leaves the PC: no account, no
telemetry. Local calendars never go online.

## Files

`%LOCALAPPDATA%\Plume\Calendar` (or `PLUME_CALENDAR_DIR`): `calendars.json` (the list, colors, subscriptions),
`events\<id>.ics` (each calendar's events, as an iCalendar file), `settings.json`, `reminders.json` (when reminders
were last checked).

## Command line

    plume-calendar.exe [file.ics | webcal://…]   the window (imports the file or subscribes)
    plume-calendar.exe --background             only the icon near the clock (start with Windows)
    plume-calendar.exe --unregister             no more start with Windows, and the running one quits (uninstall)

## Code

| File | What |
|---|---|
| `src/date.rs` | dates as day numbers, wall-clock times, ISO weeks |
| `src/tz.rs` | Windows time zones, IANA names, VTIMEZONE writing |
| `src/recur.rs` | RRULE parsing, writing and expansion (unit tests for each case, DST) |
| `src/ics.rs` | .ics reading and writing (folding, escaping, TZID, VALARM, VTIMEZONE) |
| `src/model.rs` | calendars, events, occurrences, the store, `Provider` |
| `src/quickadd.rs` | the quick-add rules (English and French) |
| `src/reminders.rs` | which reminders are due |
| `src/popup.rs` | the reminder popup window |
| `src/sys.rs` | icon near the clock, Windows notifications, start with Windows |
| `src/net.rs` | WinINet GET (copied from Plume Music's helper) |
| `src/ui/` | the window: state and input (`mod.rs`), frame and overlays (`paint.rs`), views (`views.rs`), event card (`editor.rs`), settings |
| `src/test_hook.rs` | the test socket (feature `test-hook`) |

## Tests

    cargo test -p plume-calendar
    cargo clippy -p plume-calendar --all-targets [--features test-hook]
    cargo build --profile fast -p plume-calendar --features test-hook

The test build is driven through a JSON socket on 127.0.0.1 (commands listed at the top of `src/test_hook.rs`:
state, hits, click, drag, type, key, go, add, quick, import, subscribe, now, tick, set, quit). Environment:

| Variable | Effect |
|---|---|
| `PLUME_CALENDAR_TEST_PORT` | opens the test socket (test-hook builds); reminders then stay in the window and the log, no popup, no notification, no icon near the clock |
| `PLUME_CALENDAR_DIR` | data folder |
| `PLUME_CALENDAR_NOACTIVATE` | the window opens without taking the focus, and never takes the keyboard while in the background |
| `PLUME_CALENDAR_RUN_KEY` | registry key used for "start with Windows" (test-hook builds), instead of the real Run key |
| `PLUME_CALENDAR_TZ` | the zone shown (Windows or IANA name) instead of the PC's |
| `PLUME_CALENDAR_QUIET_REMINDERS` | reminders only in the window and the log |
| `PLUME_CALENDAR_TEST_TRAY` | the icon near the clock also in tests |
| `PLUME_TEST_FILES`, `PLUME_TEST_SAVE` | answers of the open / save dialogs (plume-ui, test-hook builds) |
| `PLUME_SUITE_THEME`, `PLUME_SUITE_LANG` | theme and language when the settings say "system" |
