# Test client for the Plume FEx test hook (feature "test-hook"): drives the window over 127.0.0.1, never the real mouse.
# The window is shown without taking focus, menus are recorded instead of displayed, drags are recorded instead of
# performed, and each run uses its own settings folder: nothing touches the user's own Plume FEx.
import json, os, shutil, socket, subprocess, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
# the test build: cargo build --profile fast -p plume-fex --features test-hook
EXE = os.environ.get("FEX_EXE") or os.path.join(ROOT, "target", "fast", "plume-fex.exe")
CAP = os.path.join(HERE, "capture.ps1")
# fixtures, throwaway profiles and screenshots
OUT = os.path.join(ROOT, "target", "fex-ui")
os.makedirs(OUT, exist_ok=True)


class Fex:
    def __init__(self, *args, port=47811, profile="prof-test", fresh=True, env=None, index=""):
        # index: folders to index instead of the whole disk ("" = no index: the search box only filters the folder)
        prof = os.path.join(OUT, profile)
        if fresh and os.path.isdir(prof):
            shutil.rmtree(prof, ignore_errors=True)
        e = dict(os.environ)
        e.update({"PLUME_FEX_NOACTIVATE": "1", "PLUME_FEX_TEST_PORT": str(port), "PLUME_FEX_PROFILE": prof, "PLUME_FEX_INDEX_ROOTS": index})
        e.update(env or {})
        self.p = subprocess.Popen([EXE, *args], env=e, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.time() + 10
        while True:
            try:
                self.s = socket.create_connection(("127.0.0.1", port), timeout=20)
                break
            except OSError:
                if time.time() > deadline or self.p.poll() is not None:
                    raise RuntimeError("Plume FEx did not start (exit code %s)" % self.p.poll())
                time.sleep(0.05)
        self.f = self.s.makefile("rw", encoding="utf-8", newline="\n")

    def cmd(self, name, **kw):
        kw["cmd"] = name
        self.f.write(json.dumps(kw) + "\n")
        self.f.flush()
        r = json.loads(self.f.readline())
        if isinstance(r, dict) and r.get("error"):
            raise RuntimeError("%s -> %s" % (kw, r["error"]))
        return r

    def state(self):
        return self.cmd("state")

    def wait(self, pred=None, timeout=10.0, what="ready"):
        pred = pred or (lambda s: s["load"] != "loading" and not s["stale"])
        end = time.time() + timeout
        while True:
            s = self.state()
            if pred(s):
                return s
            if time.time() > end:
                raise RuntimeError("timeout waiting for %s; state: %s" % (what, json.dumps(s)[:600]))
            time.sleep(0.02)

    def mouse(self, type, id=None, **kw):
        if id is not None:
            kw["id"] = id
        return self.cmd("mouse", type=type, **kw)

    def click(self, id, **kw):
        return self.mouse("click", id, **kw)

    def dbl(self, id, **kw):
        return self.mouse("dblclick", id, **kw)

    def key(self, vk, **kw):
        return self.cmd("key", vk=vk, **kw)

    def chars(self, s):
        return self.cmd("char", s=s)

    def edit(self, text):
        return self.cmd("edit", text=text)

    def editkey(self, vk, **kw):
        return self.cmd("editkey", vk=vk, **kw)

    def nav(self, path):
        self.cmd("nav", path=path)
        return self.wait()

    def names(self, start=0, count=100):
        return self.cmd("names", **{"from": start, "count": count})

    def shot(self, name):
        path = os.path.join(OUT, name + ".png")
        time.sleep(0.25)
        subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", CAP, "-ProcId", str(self.p.pid), "-Out", path],
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return path

    def close(self):
        try:
            self.cmd("close")
        except Exception:
            pass
        try:
            self.p.wait(5)
        except subprocess.TimeoutExpired:
            self.p.kill()  # frozen TEST instance only


VK = dict(BACK=8, TAB=9, ENTER=13, ESC=27, SPACE=32, PGUP=33, PGDN=34, END=35, HOME=36, LEFT=37, UP=38, RIGHT=39, DOWN=40, DELETE=46,
          F2=113, F3=114, F4=115, F5=116)


def L(c):
    return ord(c.upper())


class Checks:
    def __init__(self):
        self.ok = True
        self.n = 0

    def __call__(self, label, cond, extra=""):
        self.n += 1
        print(("PASS " if cond else "FAIL ") + label + ("" if cond else "   " + str(extra)))
        self.ok = self.ok and bool(cond)

    def done(self):
        print(("ALL %d PASS" % self.n) if self.ok else "SOME FAILED")


def fixture(name="fixture", big=0):
    """A small known folder tree for the tests."""
    d = os.path.join(OUT, name)
    if os.path.isdir(d):
        shutil.rmtree(d, ignore_errors=True)
    os.makedirs(os.path.join(d, "Alpha", "Inner"))
    os.makedirs(os.path.join(d, "beta"))
    os.makedirs(os.path.join(d, "Gamma"))
    for n, size in [("a1.txt", 10), ("a2.txt", 2000), ("a10.txt", 300), ("notes.md", 1234), ("photo.png", 150000), ("Zed.bin", 1572864), ("data.json", 42)]:
        with open(os.path.join(d, n), "wb") as f:
            f.write(b"x" * size)
    with open(os.path.join(d, "Alpha", "inside.txt"), "wb") as f:
        f.write(b"hello")
    hidden = os.path.join(d, ".secret")
    with open(hidden, "wb") as f:
        f.write(b"s")
    subprocess.run(["attrib", "+h", hidden], stdout=subprocess.DEVNULL)
    if big:
        b = os.path.join(d, "Gamma")
        for i in range(big):
            open(os.path.join(b, "file%05d.dat" % i), "wb").close()
    return d
