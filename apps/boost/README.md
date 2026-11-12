# Plume Boost

Windows' performance settings in one place, and real tuning of the graphics card through its maker's driver. Each
setting says what it does, what it gains, what it costs, where the evidence comes from and how to check it on your PC;
nothing is applied until it is ticked and Apply (or Try) is pressed; every change can be undone, one by one or all at
once; and before every change Plume Boost takes a snapshot that Plume Backup keeps. No fake "score", no "RAM cleaner",
no registry placebo: only numbers Windows and the drivers can measure.

## What it does

- **Overview.** Live numbers, read every second: the active power plan, the processor clock (PDH *Processor
  Frequency × % Processor Performance*), base clock and highest seen, timer resolution (`NtQueryTimerResolution`),
  memory in use. **Wake-up delay** on demand (how late a 1 ms high-resolution timer wakes a thread, typical and 1-in-100
  worst case over 400 tries). After each Apply, a **before / after** table.
- **Tweaks.** 15 settings Windows itself has (audited in 1.2, see below), three profiles that only tick boxes. A click
  opens *What it does*, *What you gain*, *What it costs*, *Evidence*, *How to check it* and *What Plume Boost writes*.
  At the bottom, **No measurable effect: not offered**: popular "optimizer" switches and why they are left out.
- **Processor.** The active plan's processor settings, plugged in and on battery: minimum and maximum processor state,
  turbo boost mode, **energy performance preference** (EPP, PERFEPP) and **core parking minimum cores** (CPMINCORES),
  each with what it does and its source; **Show them** unhides turbo / EPP / parking in Windows' advanced power options
  (`PowerWriteSettingAttributes`, administrator). And why Plume Boost doesn't undervolt or change multipliers (below).
- **Graphics card.** Readings (clocks, temperature and hotspot, power and limit, fan, load) and, when the driver allows
  it, tuning within the ranges it reports (see *Graphics card tuning*). Profiles, apply at sign-in, a stability test,
  the always-on-top **monitor** window.
- **Changes.** Every change, newest first, with the value each setting gets back and **Undo**; **Undo everything**;
  the **snapshots** (date, why, what they hold) with **Restore this snapshot** and **Open**; the restore points.
- **Settings.** Language, theme, System Restore point before applying, **Keep a copy of each snapshot with Plume
  Backup** (on by default).
- English and French, dark and light, the Plume look, accent = the cyan of the icon (#22d3ee dark / #0891b2 light).

## Snapshots and Plume Backup

Before every Apply, every processor or graphics card change, Undo everything and every snapshot restore, Plume Boost
writes `%LOCALAPPDATA%\Plume\Boost\Snapshots\<yyyy-mm-dd hh-mm-ss>\`:

| File | What |
|---|---|
| `registry.reg` | every registry value Plume Boost can change (all tweaks, retired ones too, the DirectX string, the sign-in Run value), under its key, regedit format (UTF-16); `"Name"=-` when the value didn't exist |
| `power-<GUID>.pow` | each power plan, `powercfg /export` |
| `power.json` | the active plan; min / max state, boost mode, EPP, parking (plugged in, on battery) of every plan; whether turbo / EPP / parking are shown |
| `services.json` | SysMain and WSearch start types |
| `gpu.json` | each tunable card's settings |
| `undo.json` | a copy of the undo log |
| `snapshot.json`, `README.txt` | when, why, what it holds; how to restore it (English and French) |

The 60 newest stay on the PC. Then Plume Boost runs Plume Backup (`plume-backup.exe` next to `plume-boost.exe`):
`--ensure-plan "Plume Boost settings" --source <Snapshots> --dest "<Documents>\Plume Backups" --keep 60` (versions mode,
"only when asked"; an existing plan keeps the destination and schedule the user chose in Plume Backup), then
`--run-plan "Plume Boost settings"`, waiting up to 45 s (the copy finishes by itself after that).

If Plume Backup is missing, Plume Boost says so ("Plume Boost keeps a copy of your settings with Plume Backup") and
offers **Install Plume Backup** (`PlumeSetup.exe --install backup` from the same folder) or **Apply without Plume
Backup** (not asked again until Plume Boost restarts). The snapshot is written locally either way; nothing waits forever.

**Restore this snapshot** compares each saved value with the PC and lists what would change; after confirmation (and
administrator rights when a machine-wide value is involved) it takes a snapshot of its own, then sets the registry
values (removes those marked `=-`), imports deleted plans back from their `.pow` (`powercfg /import <file> <GUID>`),
sets the processor settings and the active plan, service start types and graphics card settings. When everything
worked, the snapshot's undo log replaces the current one (the PC is as it was then).

## Graphics card tuning

Each maker's driver library is loaded at run time from System32 by full path (`LoadLibrary` / `GetProcAddress`), in
pure Rust (no SDK, no C compiler). A value is never sent outside the range the driver reports; the engine clamps
every value again before writing.

| Maker | Library | Readings | Tuning |
|---|---|---|---|
| NVIDIA | `nvapi64.dll` (`nvapi_QueryInterface`, published function ids) + `nvml.dll` (documented NVML) | clocks, temperature, fan %, power and enforced limit, load (NVML) | core and memory clock offsets in P0 through the P-state 2.0 interface (NVAPI); power limit between NVML's min/max (`nvmlDeviceSetPowerManagementLimit`, falls back to `nvidia-smi -pl`; administrator); fixed fan speed or automatic (`nvmlDeviceSetFanSpeed_v2` / `SetDefaultFanSpeed_v2`, driver 520+) |
| AMD | `amdadlx64.dll` (ADLX C interface, function tables) | clocks, temperature, hotspot, power, fan rpm, load | maximum graphics clock and graphics voltage (manual graphics tuning 2, RX 5000+), memory clock, power limit (%), fan curve (5 points, kept by the driver; a fixed speed is a flat curve); "Stock settings" = AMD's reset to factory |
| Intel | `ControlLib.dll` (IGCL) | clock, temperature, power (energy counter), load (activity counter), fan | Arc: frequency offset, voltage offset, power limit, temperature limit (`ctlOverclock*`), fans (automatic, fixed, curve); Intel's overclocking waiver is shown and accepted by the user first. Integrated graphics (Iris Xe, UHD): readings only |

NVIDIA's driver keeps no fan curve of its own (a curve needs a program running all the time): Plume Boost offers a fixed
speed or automatic there. Integrated graphics share the processor's power: their clocks follow the plan and the
processor settings.

**Safety.** Nothing is applied while the controls move. **Try** takes a snapshot, applies, and opens *Keep these
settings?* with a 15-second countdown: without **Keep**, the previous settings come back by themselves (also when the
window closes). **Stability test** (from that question or the page): a Direct3D 11 compute load for 60 s on that card
(shader compiled at run time by `d3dcompiler_47.dll`), each round checked against the processor's own computation
(silent errors are caught), watching for the device being lost (driver reset), the highest temperature, clock and
power, and a clock drop under load (throttling); a failure puts the previous settings back at once. **Crash guard**: each
change writes `gpu_guard.json` (previous settings, time, Windows' start time). For 10 minutes, a driver reset (System
log, Display event 4101) or a restart without a clean shutdown (`Control\Windows\ShutdownTime` older than the change)
puts the previous settings back — while running, at the next window start, or at sign-in — and pauses "apply at
sign-in" with the reason shown on the page.

**Profiles** save every tunable card's settings; **At sign-in** writes `HKCU\…\Run\PlumeBoostGpu =
"plume-boost.exe" --apply-gpu-profile` (logged: Undo and `--undo-all` remove it). **Monitor**: a small always-on-top
window (graphics load, clock, temperature, power, fan; processor load and clock; last 2 minutes). It is its own window:
**no FPS counter**, because an in-game counter has to load code into each game (anti-cheat systems may flag it, it can
crash games); use the game's own counter or the Game Bar's performance widget (Win+G).

## How changes are made and undone

Everything goes through one backend (`src/backend/`): the registry (advapi32), power plans (powrprof), service start
types (Service Control Manager), System Restore (`SRSetRestorePointW`), the graphics drivers, and `powercfg.exe` only
for exporting / importing whole plans in snapshots.

Before any value changes, the value it had (or "not set") is written to `undo.json`, saved after every change. **Undo
everything** walks the log backwards, which restores exactly the original state whatever happened in between (graphics
cards included: their full settings before the first change). **Undo** on one item restores its own entries; if a later
change touched the same setting, the older value is handed over to that later entry. A plan Plume Boost created
(Ultimate Performance) is deleted again on undo.

Command line:

```
plume-boost.exe                       the window
plume-boost.exe --elevated            the same, started again as administrator at the user's request
plume-boost.exe --undo-all            puts back everything (uninstaller): exit 0 when everything is back, 1 otherwise;
                                      takes a snapshot first; asks for administrator rights (UAC) when needed
plume-boost.exe --apply-gpu-profile   the Run value at sign-in: the crash guard first, then the chosen profile
```

## The tweaks (audited in 1.2)

Kept: only settings with a documented effect that can be checked on the PC (each shows *How to check it*).

| Tweak | What it changes | Gain | Cost / risk | Evidence | Admin | Takes effect |
|---|---|---|---|---|---|---|
| Ultimate Performance plan | Copies Windows' hidden plan e9a42b02-… and activates it | Shorter wake-up delays; 0-2 % FPS, fewer idle-to-busy hitches | More power, heat, fan noise; drains laptops; some Modern Standby laptops refuse it | Microsoft, Windows 10 1803 ("for workstations"); comparisons find it ≈ High performance on AC | no | now |
| High performance plan | Activates 8c5e7fda-… | Faster ramp-up, shorter wake-up delay | More power and heat; on recent Ryzen the chipset driver's Balanced is tuned for the chip | Windows' built-in plan | no | now |
| Balanced plan | Activates 381b4222-… | Cooler, quieter, much longer battery life; near-same game speed on Speed Shift / CPPC processors | Slightly longer wake-up delay | Microsoft's recommended default | no | now |
| Gentler turbo on battery | PERFBOOSTMODE = 3 (Efficient enabled) on battery, active plan | Cooler, quieter, more battery | Short bursts a little slower on battery | Microsoft's processor power management options | no | now |
| Game Mode | HKCU\Software\Microsoft\GameBar AutoGameModeEnabled = 1 | Steadier frame times with background work; no update restarts while playing | Almost none (on by default) | Microsoft Support "Use Game Mode while gaming" | no | now |
| Hardware-accelerated GPU scheduling | HKLM\…\GraphicsDrivers HwSchMode = 2 | Less CPU work/latency per frame; required for DLSS Frame Generation | Needs WDDM 2.7+ GPU; a few report stutter with capture tools | DirectX Dev Blog (2020); NVIDIA requirements | yes | restart |
| Optimizations for windowed games | DirectXUserGlobalSettings `SwapEffectUpgradeEnable=1;` | Flip model: lower latency, VRR and Auto HDR in windowed DX10/11 games | Rare glitches in old games (exclude per game in Settings) | DirectX Dev Blog on windowed games (2022) | no | game restart |
| Variable refresh rate for all games | DirectXUserGlobalSettings `VRROptimizeEnable=1;` | No tearing / less stutter below refresh rate on VRR screens | Only useful with a VRR screen | Microsoft Support on VRR for games | no | game restart |
| Turn off Xbox Game Bar captures | GameDVR_Enabled, AppCaptureEnabled, HistoricalCaptureEnabled = 0 | GPU encoder and a few % GPU back if background recording was on; no capture hook | Win+Alt+R / Win+Alt+G recording stop (ShadowPlay, ReLive, OBS still work) | Microsoft Support: background recording "may affect game performance" | no | game restart |
| Visual effects for performance | UserPreferencesMask 90 12 03 80 10 00 00 00, VisualFXSetting 3, MinAnimate 0, TaskbarAnimations 0 (font smoothing and thumbnails kept) | Instant windows, less desktop GPU work (integrated graphics, old PCs) | Plainer look | The values Windows writes for "Adjust for best performance" | no | sign out |
| No transparency effects | EnableTransparency = 0 | Less desktop GPU/power use on integrated graphics | Flat colours | Windows' own setting (Energy saver turns it off too) | no | now |
| No wait for startup apps | Explorer\Serialize StartupDelayInMSec = 0 | Startup apps ready several seconds sooner after sign-in | Busier first seconds, notably on hard disks | Read by Explorer; described in Microsoft WPT talks | no | sign out |
| Fast Startup | HiberbootEnabled = 1 (only when hibernation is on) | Faster start from Shut down (on by default) | Drivers not fully restarted at shutdown; dual-boot sees an unclean disk | Microsoft Learn, Fast Startup vs hibernation | yes | now |
| Turn off SysMain (fix: hard disk stuck at 100 %) | SysMain start = Disabled | Less background disk/CPU on busy hard disks | Apps open slower; Microsoft recommends leaving it on | Microsoft Learn; common fix for 100 % disk on HDDs | yes | restart |
| Turn off MPO (fix) | HKLM\SOFTWARE\Microsoft\Windows\Dwm OverlayTestMode = 5 | Only if affected: flicker, black flashes, stutter with mixed refresh rates | Slightly more GPU/power for video and windowed games; undocumented by Microsoft | NVIDIA's MPO support article (same value, with undo); still reported working on 24H2 | yes | restart |

The 1.2 audit:
- **Removed — Stop background apps**: Windows 11 already suspends Store apps that aren't in use (Process Lifetime
  Manager); the global switch saves no measurable memory or power and stops notifications.
- **Removed — Turn off Windows Search indexing**: the indexer backs off by itself while the user is active, gaming or on
  battery; nothing measurable in games, and file search breaks. Limiting indexed folders is the right tool.
- **Moved to Fixes — Turn off SysMain**: a fix for one problem (a hard disk stuck at 100 % after startup), not a
  speed-up; Microsoft recommends leaving it on.
- Kept, each with *How to check it*: the three plans and gentler turbo on battery (wake-up delay, clocks, battery
  report), Game Mode, GPU scheduling, windowed-game optimizations, VRR, Game Bar captures off, visual effects,
  transparency, startup delay, Fast Startup, MPO off (fix).
- Retired tweaks stay in `RETIRED` (tweaks.rs) only so an old undo log still shows their names and snapshots save their
  values.

Profiles: **Gaming** (Ultimate or High performance, Game Mode, GPU scheduling, windowed optimizations, VRR, Game Bar
captures off), **Balanced** (Balanced plan, Game Mode, windowed optimizations, captures off, no startup delay, Fast
Startup), **Laptop on battery** (Balanced, gentler turbo on battery, Game Mode, no transparency, visual effects,
captures off, Fast Startup).

### No measurable effect: not offered

Shown on the Tweaks page with the reason: the two removed tweaks; "RAM cleaners"; registry cleaners and registry
"speed" values (NetworkThrottlingIndex, SystemResponsiveness 0, Win32PrioritySeparation, TcpAckFrequency/Nagle);
a global 0.5 ms timer resolution (per process since Windows 10 2004); HPET / bcdedit / MSI mode for every device;
anything that weakens security (Defender, memory integrity, UAC, Windows Update); a "performance score".

## The processor: what Plume Boost doesn't do

No undervolting, no multipliers, no MSR writes: those need kernel code. Apps that do it install a driver such as
WinRing0, which Windows now blocks as vulnerable (Microsoft's vulnerable driver blocklist / Defender), since any program
could use it to take over the PC. Plume Boost installs no driver. Many recent Intel laptops also lock undervolting
(Plundervolt fix). Real processor tuning belongs in the BIOS/UEFI (PBO and Curve Optimizer, power limits), Ryzen Master,
Intel XTU or the laptop maker's app. What Plume Boost sets instead is what Windows exposes for every processor in each
plan: min/max state, boost mode, EPP, core parking.

## What goes online

Nothing.

## Files

- `%LOCALAPPDATA%\Plume\Boost\settings.json` — language, theme, page, ticks, restore-point default, Plume Backup copy
  switch, chosen card, monitor open, sign-in paused and why, Intel waiver accepted.
- `undo.json` — the undo log and the restore points made.
- `Snapshots\` — the snapshots (above). `gpu_profiles.json` — graphics profiles and the sign-in one.
  `gpu_guard.json` — the crash guard (removed after 10 minutes without trouble).
- Language/theme also follow `PLUME_SUITE_LANG` / `PLUME_SUITE_THEME` and Plume Suite's settings.json.

## Tests (never on the real PC)

`real.rs` is the PC, `fake.rs` an imaginary PC in `<dir>/fake_system.json`; a `--features test-hook` build always uses
the imaginary PC, and the real backend refuses every write in test builds and unit tests. The imaginary PC has
graphics cards with ranges like real ones (`PLUME_BOOST_FAKE_GPU=nvidia,amd,arc,iris,none`; default `iris`), each
unstable beyond a limit (its stability test then ends with a counted driver reset), Plume-style power plan exports, and
the crash guard's sensors.

- `cargo test -p plume-boost` — tweaks applied and undone, admin refusals, plans, profiles, per-tweak undo, the log
  across restarts, restore points; graphics cards clamped / logged / tried / reverted / undone on the three makers,
  Intel's waiver; processor visibility and the sign-in value undone; snapshots (contents, restore, a deleted plan
  imported back, pruning); .reg files written and read back; the crash guard's verdicts; stress verdicts and the
  processor side of the stress check; Intel telemetry parsing; event-log times.
  `cargo test -p plume-boost probe_ -- --ignored --nocapture` reads (never writes) what Intel's library reports here.
- Test socket (`--features test-hook`): `PLUME_BOOST_TEST_PORT`, `PLUME_BOOST_NOACTIVATE`, `PLUME_BOOST_FAKE=<dir>`,
  `PLUME_BOOST_FAKE_ADMIN=1`, `PLUME_BOOST_FAKE_NO_ULTIMATE=1`, `PLUME_BOOST_FAKE_NO_BATTERY=1`,
  `PLUME_BOOST_FAKE_GPU`, `PLUME_BOOST_STRESS_SECS`, `PLUME_BOOST_DIR`, `PLUME_BOOST_BACKUP_EXE`,
  `PLUME_BOOST_SETUP_EXE`, `PLUME_BOOST_BACKUP_DEST`. A test build always gives Plume Backup its own data folder
  (`PLUME_BACKUP_DIR`) inside the imaginary PC's. Commands: state, hits, click, key, go, expand, set, scroll, knob,
  event (reset / crash / restart), guard (see `src/test_hook.rs`).
