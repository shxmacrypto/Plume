# Plume Task

A clear task manager for Windows: what uses your processor, memory, disk, graphics card and battery, said in plain
words, and the way to stop it.

## What it does

- **Apps.** Every program on one line: a browser's twenty processes are one row, which opens to show them. Columns:
  processor, memory (private working set), disk (read + write per second), graphics (the busiest engine of the card)
  and battery impact (processor, graphics and disk together, smoothed over about twenty seconds). The busier a cell,
  the warmer its tint. Programs with a window come first, background processes after (can be turned off).
- Sort by any column (click the header again to reverse), search by name, executable, publisher or PID.
- **End task** asks first. When Windows doesn't allow it, the dialog says why: the program runs as administrator,
  it is part of Windows, or it is a Windows service.
- **Details**: path, command line (when Windows lets it be read), PIDs, start time, threads, handles.
  **Open file location** opens Explorer on the executable.
- **What's slowing my PC**: a card at the top names the busiest part of the PC (memory almost full, processor or
  disk or graphics card busy) and the program responsible, from the average of the last five seconds.
- **Performance**: live graphs of the last minute for the processor, memory, disk (active time, read and write),
  network (receive and send) and graphics card, drawn with Direct2D; time since Windows started.
- **Startup apps**: the Run keys of the registry (your account and all accounts), the two Startup folders, and the
  scheduled tasks that run at sign-in (Windows' own, under `\Microsoft`, are left out). Each one has a switch.
  Turning one off works the way Windows does it: a `StartupApproved` value (or the task's Enabled flag); the entry
  stays and can be turned back on. Entries of all accounts need administrator rights; the message says so.
- English and French, dark and light (follows Windows), refresh every ½ s, 1 s (default), 2 s, or paused.

## How it stays light

One `NtQuerySystemInformation` call per refresh gives every process with its times, memory and I/O counters; the
path, description and icon of a program are read once and kept. Graphics come from the "GPU Engine" counters and
the disks from "PhysicalDisk(_Total)" (PDH), the network from `GetIfTable2`. All of it runs on one sampling thread;
the window only sorts and draws, once per refresh. While the window is minimized, only the few numbers of the whole
PC are read (the graphs keep going). Measured: see the report below.

## What goes online

Nothing. Plume Task reads Windows and draws it; it sends nothing anywhere and checks nothing online.

## Files

- `%LOCALAPPDATA%\Plume\Task\settings.json`: language, theme, refresh, sort, last page.

## Code

- `src/sys.rs` — Windows: process list, times, memory, network, windows on the taskbar, names and icons from the
  executables, command lines, ending a process and why it can't be.
- `src/pdh.rs` — performance counters (graphics engines, disks).
- `src/sampler.rs` — the sampling thread: totals into rates, grouping, battery impact, new icons.
- `src/model.rs` — plain data and logic: grouping, CPU % math, sorting, search, battery levels, the summary,
  formatting. Unit-tested.
- `src/startup.rs` — startup entries: list, turn on or off.
- `src/ui/` — the window (`mod.rs` state and events, `paint.rs` frame and dialogs, `apps.rs`, `perf.rs`,
  `startup_view.rs`, `settings_view.rs`, `draw.rs` graphs and icons).
- `src/i18n.rs`, `src/settings.rs`, `src/test_hook.rs` (test builds only).

## Tests

```
cargo test -p plume-task
cargo build --profile fast -p plume-task --features test-hook
```

The test build listens on `127.0.0.1:$PLUME_TASK_TEST_PORT` (one JSON command per line: `state`, `hits`, `click`,
`dclick`, `rclick`, `type`, `key`, `go`, `select_pid`, `set`, `scroll`). Environment variables for tests:

| Variable | Use |
| --- | --- |
| `PLUME_TASK_TEST_PORT` | opens the test socket (test builds only) |
| `PLUME_TASK_NOACTIVATE` | the window opens without taking the focus |
| `PLUME_TASK_DIR` | settings folder instead of `%LOCALAPPDATA%\Plume\Task` |
| `PLUME_TASK_STARTUP_KEY` | Startup apps reads and changes only this key under `HKEY_CURRENT_USER` (`<key>\Run`, `<key>\StartupApproved\…`); scheduled tasks are left out |
| `PLUME_TASK_STARTUP_DIR` | the Startup folder used with the key above |
| `PLUME_SUITE_LANG`, `PLUME_SUITE_THEME` | `en`/`fr`, `dark`/`light` |

Tests end only a dummy process they started themselves, chosen by its PID, and change startup entries only in a
test key and folder. The unit test of `startup.rs` creates and deletes its own key under `HKCU\Software`.
