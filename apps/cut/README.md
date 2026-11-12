# Plume Cut

A video editor for Windows, native and drawn by hand (Direct2D) like the rest of Plume. Import clips, pictures and
music, arrange them on a timeline, add texts and transitions, watch the result and export it, all on this PC.
FFmpeg does the reading and the encoding (the same copy as Plume Convert, in `%LOCALAPPDATA%\Plume\tools`,
downloaded once if missing).

## What it does

- **Start screen:** new project, open a project, recent projects (name, length, folder).
- **Editor:** media bin (Media / Text / Transitions) on the left, preview in the middle, the selected clip's
  properties on the right, the timeline below.
- **Timeline:** Main, Overlay, Text, Sound and Music tracks (more text and music tracks are added when needed).
  Drag media from the bin or from Explorer, move clips (also between tracks of the same kind), trim both edges,
  split at the playhead (S), delete, delete and close the gap (Shift+Delete; with nothing selected, Delete closes
  the gap under the playhead), snapping to clip edges and the playhead (N), zoom (+/−, Ctrl+wheel, slider), scrub
  on the ruler, frame-accurate playhead, J/K/L and Space, multi-select (Ctrl+click, Ctrl+A), undo/redo for every
  edit (Ctrl+Z / Ctrl+Y).
- **Per clip:** speed (0.25× to 4×, the clip keeps the same moment of the video), volume, sound fade in/out,
  opacity, size, position, rotation (handles on the preview), crop, brightness, contrast, saturation.
- **Texts:** font, bold, size, colour, box behind the text, position, animation in and out (fade, rise, slide).
- **Transitions:** cut, crossfade, dip to black, with their length.
- **Music:** on its own track, optionally lowered under the other sounds.
- **Preview:** paused or scrubbing, each clip's exact frame is decoded by FFmpeg at the preview's size and the
  frame is laid out by the window (so handles and sliders answer at once); playing, FFmpeg renders the edit from
  the playhead with the export's own plan, reduced, and plays its sound through the speakers.
- **Export:** 720p, 1080p or 4K; the project's frame, 16:9 YouTube, 9:16 TikTok/Reels or 1:1; H.264 or H.265;
  the graphics card's encoder (NVIDIA, Intel, AMD) with a fallback to the processor; progress, time left, cancel;
  never overwrites a file ("Name.mp4", "Name (2).mp4"…). Exports go to `Videos\Plume Cut` (changeable).
- **Projects:** `.plumecut` files (JSON) in `Documents\Plume Cut`, media paths saved relative to the project
  (and absolute as a fallback), saved by themselves a moment after each change.
- English and French, dark and light, a keyboard sheet (F1 or the keyboard button).

## Command line

```
plume-cut.exe                        start screen
plume-cut.exe project.plumecut       opens a project
plume-cut.exe --add <files…>         a new project with these clips on the timeline (Plume Media's "Open in Plume Cut")
plume-cut.exe <files…>               the same as --add
```

## Build and test

```bash
export PATH="$USERPROFILE/.mingw-binutils:$PATH"
CARGO_TARGET_DIR=target/cut cargo test --offline -p plume-cut
CARGO_TARGET_DIR=target/cut cargo build --offline --profile fast -p plume-cut --features test-hook
python apps/cut/tests/e2e.py target/cut/fast/plume-cut.exe --shots target/shots-cut
```

The end-to-end test makes its own media with FFmpeg's test sources, opens the window without focus and without
sound (`PLUME_CUT_MUTE`, `PLUME_CUT_NOACTIVATE`), drives it through the test socket (`PLUME_CUT_TEST_PORT`, only in
a `test-hook` build), exports 9:16, 16:9 and 1:1 (H.265, graphics card) and checks the files with ffprobe.
Other test variables: `PLUME_CUT_DIR` (settings), `PLUME_CUT_PROJECTS`, `PLUME_CUT_OUT`, `PLUME_TOOLS_DIR`.
