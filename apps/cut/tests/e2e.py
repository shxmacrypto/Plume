"""Plume Cut end to end, through the test socket (a `--features test-hook` build).

  python apps/cut/tests/e2e.py <plume-cut.exe> [--shots DIR]

Makes its own media with FFmpeg's test sources (in a temp folder, never real files), opens the window without
focus and without sound, builds a 3-clip edit with a text, a crossfade and music (ducked), exports it as 9:16 and as
16:9 and checks both files with ffprobe (length, size, streams). Screenshots (PrintWindow) go to --shots.
Everything runs at below-normal priority; only the window this script started is closed.
"""
import ctypes, json, os, shutil, socket, subprocess, sys, tempfile, time

EXE = os.path.abspath(sys.argv[1])
SHOTS = os.path.abspath(sys.argv[sys.argv.index("--shots") + 1]) if "--shots" in sys.argv else None
TOOLS = os.path.join(os.environ["LOCALAPPDATA"], "Plume", "tools")
FFMPEG, FFPROBE = os.path.join(TOOLS, "ffmpeg.exe"), os.path.join(TOOLS, "ffprobe.exe")
PW = r"C:\Users\samue\AppData\Local\Temp\claude\8c6ed95050c7d508\4b8160d6-36a1-411b-8d0b-39b327acfea0\scratchpad\p\pw.ps1"
LOW = 0x00004000 | 0x08000000  # BELOW_NORMAL_PRIORITY_CLASS | CREATE_NO_WINDOW
PORT = 47613

ok = fail = 0


def check(cond, what):
    global ok, fail
    if cond:
        ok += 1
        print("  ok  ", what)
    else:
        fail += 1
        print("  FAIL", what)


def run(*args):
    subprocess.run(args, check=True, creationflags=LOW, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)


def make_media(d):
    a, b, ph, mu = (os.path.join(d, n) for n in ("beach clip.mp4", "city.mp4", "photo.png", "song.m4a"))
    run(FFMPEG, "-hide_banner", "-y", "-f", "lavfi", "-i", "testsrc2=s=1920x1080:r=30:d=6", "-f", "lavfi", "-i", "sine=f=440:d=6",
        "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", a)
    run(FFMPEG, "-hide_banner", "-y", "-f", "lavfi", "-i", "smptehdbars=s=1280x720:r=25:d=5", "-f", "lavfi", "-i", "sine=f=660:d=5",
        "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", b)
    run(FFMPEG, "-hide_banner", "-y", "-f", "lavfi", "-i", "mandelbrot=s=900x900", "-frames:v", "1", ph)
    run(FFMPEG, "-hide_banner", "-y", "-f", "lavfi", "-i", "aevalsrc=0.2*sin(2*PI*220*t)+0.1*sin(2*PI*330*t):s=48000:d=20", "-c:a", "aac", mu)
    return a, b, ph, mu


class Hook:
    def __init__(self):
        for _ in range(100):
            try:
                self.s = socket.create_connection(("127.0.0.1", PORT), timeout=60)
                self.f = self.s.makefile("rw", encoding="utf-8", newline="\n")
                return
            except OSError:
                time.sleep(0.1)
        raise SystemExit("no test socket")

    def __call__(self, **cmd):
        self.f.write(json.dumps(cmd) + "\n")
        self.f.flush()
        return json.loads(self.f.readline())

    def state(self):
        return self(cmd="state")

    def wait(self, cond, secs=60, what=""):
        end = time.time() + secs
        while time.time() < end:
            st = self.state()
            if cond(st):
                return st
            time.sleep(0.2)
        raise SystemExit("timed out: " + what)


def shot(proc, name):
    if not SHOTS:
        return
    os.makedirs(SHOTS, exist_ok=True)
    time.sleep(0.6)
    subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", PW, str(proc.pid), os.path.join(SHOTS, name + ".png")],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def probe(path):
    out = subprocess.run([FFPROBE, "-v", "error", "-print_format", "json", "-show_format", "-show_streams", path],
                         capture_output=True, text=True, creationflags=LOW).stdout
    return json.loads(out)


def main():
    root = tempfile.mkdtemp(prefix="plume-cut-e2e-")
    media = os.path.join(root, "media")
    os.makedirs(media)
    print("media in", root)
    a, b, ph, mu = make_media(media)
    env = dict(os.environ, PLUME_CUT_TEST_PORT=str(PORT), PLUME_CUT_DIR=os.path.join(root, "data"), PLUME_CUT_PROJECTS=os.path.join(root, "projects"),
               PLUME_CUT_OUT=os.path.join(root, "out"), PLUME_CUT_MUTE="1", PLUME_CUT_NOACTIVATE="1", PLUME_SUITE_LANG="en", PLUME_SUITE_THEME="dark")
    proc = subprocess.Popen([EXE], env=env, creationflags=0x00004000)
    try:
        h = Hook()
        st = h.wait(lambda s: s["screen"] == "start", 20, "start screen")
        check(st["tools"] == "Ready", "FFmpeg found")
        shot(proc, "01-start-empty")

        # ── a project with three clips, in order ──
        h(cmd="new")
        h(cmd="import", paths=[a, b, ph], append=True)
        st = h.wait(lambda s: s["importing"] == 0 and s["duration"] > 0, 60, "import")
        proj = st["project"]
        main = next(i for i, t in enumerate(proj["tracks"]) if t["kind"] == "video")
        clips = proj["tracks"][main]["clips"]
        check(len(clips) == 3, "three clips on the main track")
        check([c["start"] for c in clips] == [0, 180, 330], "placed one after the other: %s" % [c["start"] for c in clips])
        check(proj["media"][0]["rel"].startswith(".."), "media saved relative to the project: " + proj["media"][0]["rel"])
        ids = [c["id"] for c in clips]
        h.wait(lambda s: s["strips"] >= 3, 60, "filmstrips")

        # trims, a split and an undo
        h(cmd="trim", id=ids[0], end=120)
        h(cmd="move", ids=[ids[1], ids[2]], delta=-60)
        h(cmd="seek", frame=60)
        h(cmd="select", ids=[ids[0]])
        h(cmd="split")
        st = h.state()
        check(len(st["project"]["tracks"][main]["clips"]) == 4, "split at the playhead")
        h(cmd="undo")
        st = h.state()
        check(len(st["project"]["tracks"][main]["clips"]) == 3, "undo the split")
        h(cmd="redo")
        h(cmd="undo")

        # speed, colour, picture in the frame
        h(cmd="set", id=ids[1], clip={"speed": 1.25, "saturation": 1.4, "brightness": 0.05})
        st = h.state()
        c1 = next(c for c in st["project"]["tracks"][main]["clips"] if c["id"] == ids[1])
        check(c1["len"] == 120, "speed 1.25 makes the 5 s clip 4 s: %d frames" % c1["len"])
        h(cmd="set", id=ids[2], clip={"scale": 0.8, "rotation": 8})
        # crossfade into the second clip, dip into the third
        r = h(cmd="transition", id=ids[1], kind="crossfade", len=20)
        check(r.get("ok"), "crossfade into clip 2")
        st = h.state()
        clips = sorted(st["project"]["tracks"][main]["clips"], key=lambda c: c["start"])
        h(cmd="move", ids=[ids[2]], delta=clips[1]["start"] + clips[1]["len"] - clips[2]["start"])
        r = h(cmd="transition", id=ids[2], kind="dip", len=16)
        check(r.get("ok"), "dip to black into clip 3")
        # the text and the music
        r = h(cmd="text", at=20, text="Plume Cut", size=0.09, bg_opacity=0.55, anim_in="rise", anim_out="fade", y=-0.3)
        check(r.get("ok"), "text layer")
        text_id = r["id"]
        h(cmd="import", paths=[mu])
        st = h.wait(lambda s: s["importing"] == 0 and len(s["project"]["media"]) == 4, 30, "music import")
        song = st["project"]["media"][-1]["id"]
        r = h(cmd="music", media=song)
        check(r.get("ok"), "music on the music track")
        h(cmd="trim", id=r["id"], end=st["duration"])
        h(cmd="set", id=r["id"], clip={"fade_out": 1.0, "volume": 0.8})
        h(cmd="project", set={"duck": True, "name": "Holiday edit"})
        st = h.state()
        dur = st["duration"]
        check(250 <= dur <= 330, "edit length %d frames" % dur)
        tracks = st["project"]["tracks"]
        check(any(t["kind"] == "music" and t["clips"] for t in tracks), "music saved")
        check(st["project"]["duck"], "ducking on")

        # pictures: editor, text selected, preview frames arrived
        h(cmd="seek", frame=40)
        h(cmd="select", ids=[text_id])
        h(cmd="zoom", fit=True)
        h.wait(lambda s: s["frames_pending"] == 0, 30, "preview frames")
        shot(proc, "02-editor-text-selected")
        h(cmd="select", ids=[ids[2]])
        h(cmd="seek", frame=dur - 30)
        time.sleep(0.5)
        h.wait(lambda s: s["frames_pending"] == 0, 30, "preview frames")
        shot(proc, "03-editor-picture-handles")
        h(cmd="select", ids=[ids[1]])
        h(cmd="prop_tab", i=1)
        shot(proc, "04-colour-tab")
        h(cmd="select", ids=[])
        h(cmd="tab", bin="text")
        shot(proc, "05-text-presets-project")
        h(cmd="tab", bin="media")

        # saved project reloads the same
        st = h(cmd="save")
        pfile = st["file"]
        before = h.state()["project"]
        h(cmd="open", path=pfile)
        after = h.state()["project"]
        check(before["tracks"] == after["tracks"] and before["media"] == after["media"], "the project reloads the same")

        # play a little (silent), then stop
        h(cmd="seek", frame=0)
        r = h(cmd="play")
        check(r.get("ok"), "preview plays")
        time.sleep(2.0)
        st = h.state()
        check(st["playhead"] > 10, "playhead moved while playing: %d" % st["playhead"])
        shot(proc, "06-playing")
        h(cmd="stop")

        # ── exports ──
        outs = []
        for aspect, res, w, hh, codec, hw in (("9:16", 720, 720, 1280, "h264", False), ("16:9", 720, 1280, 720, "h264", False), ("1:1", 720, 720, 720, "hevc", True)):
            h(cmd="dialog", name="export")
            if aspect == "9:16":
                shot(proc, "07-export-dialog")
            h(cmd="export", res=res, aspect=aspect, codec=codec, hw=hw)
            st = h.wait(lambda s: s["export"]["state"] != "running", 300, "export " + aspect)
            check(st["export"]["state"] == "done", "export %s finished (%s)" % (aspect, st["export"]))
            if st["export"]["state"] != "done":
                continue
            if aspect == "9:16":
                shot(proc, "08-export-done")
            path = st["export"]["path"]
            outs.append(path)
            info = probe(path)
            v = [s for s in info["streams"] if s["codec_type"] == "video"]
            au = [s for s in info["streams"] if s["codec_type"] == "audio"]
            d = float(info["format"]["duration"])
            check(len(v) == 1 and len(au) == 1, "%s: one video and one audio stream" % aspect)
            check((v[0]["width"], v[0]["height"]) == (w, hh), "%s: %dx%d" % (aspect, v[0]["width"], v[0]["height"]))
            check(v[0]["codec_name"] == codec and au[0]["codec_name"] == "aac", "%s: %s + aac (graphics card: %s, %s)" % (aspect, codec, hw, st["export"]))
            check(abs(d - dur / 30.0) < 0.15, "%s: %.2f s for %.2f s of edit" % (aspect, d, dur / 30.0))
            check(int(info["format"]["size"]) > 50_000, "%s: %d bytes" % (aspect, int(info["format"]["size"])))
            h(cmd="dialog", name=None)
        check(len(set(outs)) == 3, "three different files (never overwritten)")
        # a frame of each, to look at
        if SHOTS:
            for p in outs:
                tag = "916" if "(" not in p else ("169" if "(2)" in p else "11")
                subprocess.run([FFMPEG, "-v", "error", "-y", "-ss", "1.2", "-i", p, "-frames:v", "1", os.path.join(SHOTS, "export-%s-frame.png" % tag)],
                               creationflags=LOW)

        # help sheet, settings, light and French
        h(cmd="dialog", name="help")
        shot(proc, "09-help")
        h(cmd="dialog", name=None)
        h(cmd="setting", key="theme", value="light")
        h(cmd="setting", key="lang", value="fr")
        h(cmd="select", ids=[text_id])
        h(cmd="seek", frame=40)
        time.sleep(0.4)
        shot(proc, "10-light-french")
        h(cmd="dialog", name="settings")
        shot(proc, "11-settings-fr")
        h(cmd="dialog", name=None)
        h(cmd="setting", key="theme", value="dark")
        h(cmd="setting", key="lang", value="en")
        h(cmd="home")
        shot(proc, "12-start-recent")
        st = h.state()
        check(st["screen"] == "start", "back to the start screen")
        # --add: a new project with these clips (what Plume Media sends)
        h(cmd="args", line='--add "%s" "%s"' % (b, a))
        st = h.wait(lambda s: s["screen"] == "edit" and s["importing"] == 0 and s["duration"] > 0, 60, "--add")
        clips = [c for t in st["project"]["tracks"] for c in t["clips"]]
        check(len(clips) == 2 and st["project"]["name"].startswith("Project"), "--add opens a new project with the two clips")

        # ── the mouse, as a person would use it ──
        def clips_of(st):
            t = next(t for t in st["project"]["tracks"] if t["kind"] == "video")
            return sorted(t["clips"], key=lambda c: c["start"])

        h(cmd="zoom", fit=True)
        time.sleep(0.4)
        st = h.state()
        z = st["zoom"]
        cb, ca = clips_of(st)
        h(cmd="click", id="Clip(%d)" % cb["id"])
        check(h.state()["selected"] == [cb["id"]], "a click selects a clip")
        time.sleep(0.3)
        # the left part of the grip: the right part belongs to the next clip's left edge
        h(cmd="drag", id="TrimR(%d)" % cb["id"], dx=-30 * z, fx=0.3)
        st = h.state()
        cb2, ca2 = clips_of(st)
        check(abs(cb2["len"] - (cb["len"] - 30)) <= 1 and ca2["start"] == ca["start"], "dragging the right edge trims about 30 frames: %d -> %d" % (cb["len"], cb2["len"]))
        time.sleep(0.3)
        # the next clip dragged back sticks to the end of the first (snapping)
        h(cmd="drag", id="Clip(%d)" % ca["id"], dx=-(ca2["start"] - (cb2["start"] + cb2["len"]) - 2) * z, fx=0.3)
        cb3, ca3 = clips_of(h.state())
        check(ca3["start"] == cb3["start"] + cb3["len"], "the dragged clip snaps to the end of the other: %d / %d" % (ca3["start"], cb3["start"] + cb3["len"]))
        h(cmd="seek", frame=ca3["start"] + 20)
        time.sleep(0.3)
        h(cmd="click", id='Tool("split")')
        check(len(clips_of(h.state())) == 3, "the scissors split the selected clip")
        time.sleep(0.3)
        h(cmd="click", id="Undo")
        check(len(clips_of(h.state())) == 2, "the undo button")
        h(cmd="select", ids=[ca["id"]])
        time.sleep(0.4)
        h(cmd="drag", id="Slider(Opacity)", fx=0.98, dx=-400)
        c = next(c for c in clips_of(h.state()) if c["id"] == ca["id"])
        check(c["opacity"] < 0.05, "the opacity slider drags to 0: %.2f" % c["opacity"])
        time.sleep(0.3)
        h(cmd="drag", id="Slider(Opacity)", clicks=2, dx=0, steps=1)
        c = next(c for c in clips_of(h.state()) if c["id"] == ca["id"])
        check(abs(c["opacity"] - 1.0) < 1e-9, "a double click puts the slider back")
        h(cmd="seek", frame=ca3["start"] + 20)
        time.sleep(0.4)
        h(cmd="drag", id="Handle(Body)", dx=60, dy=0)
        c = next(c for c in clips_of(h.state()) if c["id"] == ca["id"])
        check(c["x"] > 0.03, "the picture moves with the mouse in the preview: x=%.3f" % c["x"])
        time.sleep(0.3)
        h(cmd="drag", id="Handle(Rotate)", dx=80, dy=40)
        c = next(c for c in clips_of(h.state()) if c["id"] == ca["id"])
        check(abs(c["rotation"]) > 5, "the round handle turns it: %.1f°" % c["rotation"])
        shot(proc, "13-mouse-edits")
        # a media of the bin dragged onto the timeline lands where it is dropped
        time.sleep(0.3)
        h(cmd="zoom", zoom=2.0)
        time.sleep(0.4)
        st = h.state()
        mid = st["project"]["media"][0]["id"]
        hits = h(cmd="hits")
        row = next(x for x in hits if x["id"] == "TrackArea")["rect"]
        tile = next(x for x in hits if x["id"] == "Media(%d)" % mid)["rect"]
        end = max(c["start"] + c["len"] for c in clips_of(st))
        target_x = st["zoom"] * (end + 15) + row[0] + 144 - 0  # lanes start after the 140 px headers
        h(cmd="drag", id="Media(%d)" % mid, dx=target_x - (tile[0] + tile[2] / 2), dy=(row[1] + 100) - (tile[1] + tile[3] / 2), steps=10)
        n = len(clips_of(h.state()))
        check(n == 3, "a media dropped on the timeline makes a clip (%d clips)" % n)
    finally:
        try:
            hwnd = h.state()["hwnd"]
            ctypes.windll.user32.PostMessageW(hwnd, 0x0010, 0, 0)
            proc.wait(10)
        except Exception:
            proc.kill()
        shutil.rmtree(root, ignore_errors=True)
    print("\n%d ok, %d failed" % (ok, fail))
    sys.exit(1 if fail else 0)


if __name__ == "__main__":
    main()
