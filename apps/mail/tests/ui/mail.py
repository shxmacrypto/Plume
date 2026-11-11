"""Drives a test build of Plume Mail through its test hook (JSON lines on 127.0.0.1).

Build first:  cargo build --profile fast -p plume-mail --features test-hook
The window opens without taking the focus; nothing touches the mouse, the keyboard, or the real profile:
the test profile lives under target/mail-ui, and its passwords stay there (never in Windows' Credential Manager).
"""
import json, os, shutil, socket, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
EXE = os.environ.get("MAIL_EXE") or os.path.join(ROOT, "target", "fast", "plume-mail.exe")
CAP = os.path.join(ROOT, "apps", "fex", "tests", "ui", "capture.ps1")
OUT = os.path.join(ROOT, "target", "mail-ui")
os.makedirs(OUT, exist_ok=True)


class Mail:
    def __init__(self, port=47841, profile="prof-mail", fresh=True, env=None):
        prof = os.path.join(OUT, profile)
        if fresh and os.path.isdir(prof):
            shutil.rmtree(prof, ignore_errors=True)
        self.profile = prof
        e = dict(os.environ)
        e.update({"PLUME_MAIL_NOACTIVATE": "1", "PLUME_MAIL_TEST_PORT": str(port), "PLUME_MAIL_PROFILE": prof, "PLUME_MAIL_TEST_SECURITY": "plain"})
        e.update(env or {})
        self.p = subprocess.Popen([EXE], env=e, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.time() + 10
        while True:
            try:
                self.s = socket.create_connection(("127.0.0.1", port), timeout=20)
                break
            except OSError:
                if time.time() > deadline or self.p.poll() is not None:
                    raise RuntimeError("Plume Mail did not start (exit code %s)" % self.p.poll())
                time.sleep(0.05)
        self.f = self.s.makefile("rw", encoding="utf-8", newline="\n")

    def cmd(self, name, **kw):
        kw["cmd"] = name
        self.f.write(json.dumps(kw) + "\n")
        self.f.flush()
        out = json.loads(self.f.readline())
        if isinstance(out, dict) and out.get("error"):
            raise RuntimeError("%s: %s" % (name, out["error"]))
        return out

    def state(self):
        return self.cmd("state")

    def wait(self, pred=None, timeout=15.0, what="ready"):
        end = time.time() + timeout
        while True:
            s = self.state()
            if pred is None or pred(s):
                return s
            if time.time() > end:
                raise RuntimeError("timeout waiting for %s; state: %s" % (what, json.dumps(s, ensure_ascii=False)[:900]))
            time.sleep(0.03)

    def click(self, id, **kw):
        return self.cmd("mouse", type="click", id=id, **kw)

    def wheel(self, id, dy):
        return self.cmd("mouse", type="wheel", id=id, dy=dy)

    def key(self, vk, **kw):
        return self.cmd("key", vk=vk, **kw)

    def edit(self, text):
        return self.cmd("edit", text=text)

    def editkey(self, vk, **kw):
        return self.cmd("editkey", vk=vk, **kw)

    def field(self, i, text=None):
        """Opens a field of the dialog; with `text`, types it there."""
        self.cmd("field", i=i)
        if text is not None:
            self.edit(text)

    def menu(self, button, label):
        """Opens a menu with its button and picks the entry with this label."""
        self.click(button)
        items = self.wait(lambda s: s["menu"], what="menu")["menu"]
        self.click("Menu(%d)" % items.index(label))
        return items

    def shot(self, name):
        path = os.path.join(OUT, name + ".png")
        time.sleep(0.3)
        subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", CAP, "-ProcId", str(self.p.pid), "-Out", path],
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return path

    def close(self):
        try:
            self.cmd("close")
        except Exception:
            pass
        try:
            self.p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.p.kill()
        try:
            self.s.close()
        except OSError:
            pass


VK = dict(BACK=8, TAB=9, ENTER=13, ESC=27, SPACE=32, PGUP=33, PGDN=34, END=35, HOME=36, UP=38, DOWN=40, DELETE=46, F5=0x74)


class Checks:
    def __init__(self):
        self.passed, self.failed = 0, 0

    def __call__(self, label, cond, extra=""):
        if cond:
            self.passed += 1
        else:
            self.failed += 1
        line = "%s %s%s" % ("PASS" if cond else "FAIL", label, "" if cond else "   " + str(extra)[:700])
        print(line.encode(sys.stdout.encoding or "utf-8", "replace").decode(sys.stdout.encoding or "utf-8"))

    def done(self):
        print("ALL %d PASS" % self.passed if not self.failed else "SOME FAILED")
        sys.exit(1 if self.failed else 0)
