"""End-to-end test of PlumeSetup.exe and Plume Suite, in a throwaway profile.

    python apps/suite/tests/test_setup.py <PlumeSetup.exe built with test programs>

Everything happens in a sandbox: its own LOCALAPPDATA and APPDATA, PLUME_SETUP_SANDBOX (no registry, no
shortcuts) for the install steps. The registration step writes a test key (PLUME_SUITE_TEST_KEY) and shortcuts in
the sandbox only, then removes them. Updates come from a local server that plays GitHub (PLUME_UPDATE_FEED).
Needs a Plume Suite built with `--features test-hook` inside the installer.
"""
import hashlib
import http.server
import json
import os
import shutil
import socket
import subprocess
import sys
import threading
import time
import winreg

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", "..", ".."))
OUT = os.path.join(ROOT, "target", "suite-test")
SETUP = os.path.abspath(sys.argv[1])
PORT_UI, PORT_FEED = 47862, 47863
fails = []
passed = []


def check(label, ok, extra=""):
    print(("PASS " if ok else "FAIL ") + label + ("" if ok else "   " + str(extra)))
    (passed if ok else fails).append(label)


shutil.rmtree(OUT, ignore_errors=True)
LOCAL, ROAMING = os.path.join(OUT, "local"), os.path.join(OUT, "roaming")
os.makedirs(LOCAL)
os.makedirs(ROAMING)
DIR = os.path.join(LOCAL, "Programs", "Plume")
ENV = dict(os.environ, LOCALAPPDATA=LOCAL, APPDATA=ROAMING, PLUME_SETUP_SANDBOX="1", PLUME_SUITE_LANG="en")
for k in ("PLUME_UPDATE_FEED", "PLUME_SUITE_TEST_KEY"):
    ENV.pop(k, None)


def setup(*args, env=None, exe=None):
    return subprocess.run([exe or SETUP, *args], env=env or ENV, timeout=120).returncode


def there(*names):
    return all(os.path.isfile(os.path.join(DIR, n)) for n in names)


def files():
    return sorted(os.listdir(DIR)) if os.path.isdir(DIR) else []


# ── 1. a fresh install: Plume Suite and the copy of the installer, no app yet
check("a fresh install exits cleanly", setup("--quiet") == 0)
check("it installs Plume Suite and keeps a copy of the installer, nothing else", files() == ["PlumeSetup.exe", "plume-suite.exe"], files())
check("the kept installer is this one", open(os.path.join(DIR, "PlumeSetup.exe"), "rb").read() == open(SETUP, "rb").read())
CACHED = os.path.join(DIR, "PlumeSetup.exe")

# ── 2. apps one by one, through the kept installer (as Plume Suite does)
check("--install fex", setup("--install", "fex", exe=CACHED) == 0 and there("plume-fex.exe"), files())
check("--install web brings the browser and its WebView2 loader", setup("--install", "web", exe=CACHED) == 0 and there("plume.exe", "WebView2Loader.dll"), files())
check("an unknown app is refused", setup("--uninstall", "nope", exe=CACHED) != 0)

# ── 3. an update rewrites every installed app, and only those
open(os.path.join(DIR, "plume-fex.exe"), "wb").write(b"old")
check("--update succeeds", setup("--update") == 0)
check("an update brings installed apps up to date", os.path.getsize(os.path.join(DIR, "plume-fex.exe")) > 1000)
check("an update does not add apps", not there("plume-mail.exe"), files())

# ── 4. removing
check("--uninstall fex removes it", setup("--uninstall", "fex", exe=CACHED) == 0 and not there("plume-fex.exe"), files())
check("removing an app that is not there is fine", setup("--uninstall", "mail", exe=CACHED) == 0)
check("the browser can be removed too (sandbox: its files only)", setup("--uninstall", "web", exe=CACHED) == 0 and not there("plume.exe"), files())


# ── 5. the Plume Suite window drives the installer
class Ui:
    def __init__(self, env):
        self.p = subprocess.Popen([os.path.join(DIR, "plume-suite.exe")], env=env)
        for _ in range(80):
            try:
                self.s = socket.create_connection(("127.0.0.1", PORT_UI), timeout=3)
                break
            except OSError:
                time.sleep(0.25)
        self.f = self.s.makefile("rw")

    def cmd(self, **c):
        self.f.write(json.dumps(c) + "\n")
        self.f.flush()
        return json.loads(self.f.readline())

    def wait(self, pred, what, timeout=30):
        end = time.time() + timeout
        while time.time() < end:
            s = self.cmd(cmd="state")
            if pred(s):
                return s
            time.sleep(0.25)
        raise RuntimeError("timeout waiting for %s: %s" % (what, s))

    def shot(self, name):
        capture = os.path.join(ROOT, "apps", "fex", "tests", "ui", "capture.ps1")
        subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", capture, "-ProcId", str(self.p.pid), "-Out", os.path.join(OUT, name)], capture_output=True)

    def close(self):
        self.p.terminate()  # the sandbox window only
        self.p.wait(10)


UENV = dict(ENV, PLUME_SUITE_TEST_PORT=str(PORT_UI), PLUME_SUITE_NOACTIVATE="1")
ui = Ui(UENV)
try:
    s = ui.wait(lambda s: s["installed"] is not None, "window")
    check("the window sees what is installed (nothing now)", s["installed"] == [False, False, False], s)
    ui.shot("suite-empty.png")
    ui.cmd(cmd="click", id="Install(2)")
    s = ui.wait(lambda s: s["busy"] is None and s["installed"][2], "Mail installed")
    check("Install in the window installs Plume Mail", there("plume-mail.exe") and s["installed"] == [False, False, True], s)
    check("no error is shown", s["toast"] is None, s)
    ui.cmd(cmd="click", id="Install(1)")
    s = ui.wait(lambda s: s["busy"] is None and s["installed"][1], "FEx installed")
    ui.shot("suite-two.png")
    # removing FEx asks first (a Windows dialog): the test removes it with the installer, the window follows
    setup("--uninstall", "fex", exe=CACHED)
    s = ui.cmd(cmd="refresh")
    check("the window follows a change made elsewhere", s["installed"] == [False, False, True], s)
finally:
    ui.close()

# ── 6. registration: test key, sandbox shortcuts (no PLUME_SETUP_SANDBOX here, so nothing is skipped)
DESK = os.path.join(OUT, "desktop")
RENV = dict(os.environ, LOCALAPPDATA=LOCAL, APPDATA=ROAMING, PLUME_SUITE_TEST_KEY="1", PLUME_SUITE_DESKTOP=DESK)
RENV.pop("PLUME_SETUP_SANDBOX", None)
KEY = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\PlumeSuiteTest"
suite_exe = os.path.join(DIR, "plume-suite.exe")
code = subprocess.run([suite_exe, "--register", "--desktop"], env=RENV, timeout=60).returncode
check("--register succeeds", code == 0, code)
try:
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, KEY) as k:
        name = winreg.QueryValueEx(k, "DisplayName")[0]
        unin = winreg.QueryValueEx(k, "UninstallString")[0]
        ver = winreg.QueryValueEx(k, "DisplayVersion")[0]
    check("Settings › Apps shows Plume Suite with its version", name == "Plume Suite" and ver, (name, ver))
    check("its Uninstall runs the kept installer", unin.lower() == ('"%s" --uninstall-all' % CACHED).lower(), unin)
except OSError as e:
    check("the test key exists", False, e)
menu = os.path.join(ROAMING, r"Microsoft\Windows\Start Menu\Programs")
check("Start menu: Plume Suite and the installed app (Mail), not the others",
      sorted(os.listdir(menu)) == ["Plume Mail.lnk", "Plume Suite.lnk"], os.listdir(menu) if os.path.isdir(menu) else None)
check("desktop shortcut on request", os.path.isfile(os.path.join(DESK, "Plume Suite.lnk")))
subprocess.run([suite_exe, "--unregister"], env=RENV, timeout=60)
gone = True
try:
    winreg.OpenKey(winreg.HKEY_CURRENT_USER, KEY).Close()
    gone = False
except OSError:
    pass
check("--unregister removes the key and the shortcuts", gone and not os.listdir(menu) and not os.path.isfile(os.path.join(DESK, "Plume Suite.lnk")))


# ── 7. an update found on "GitHub" (a local server), installed from the window
class Feed(http.server.BaseHTTPRequestHandler):
    body = open(SETUP, "rb").read()
    digest = hashlib.sha256(body).hexdigest()

    def log_message(self, *a):
        pass

    def do_GET(self):
        base = "http://127.0.0.1:%d" % PORT_FEED
        if self.path == "/releases/latest":
            self.send_response(302)
            self.send_header("Location", base + "/releases/tag/v9.9.9")
            self.end_headers()
            return
        if self.path == "/latest.json":
            data = json.dumps({"tag_name": "v9.9.9", "html_url": base + "/rel", "assets": [
                {"name": "PlumeSetup.exe", "browser_download_url": base + "/PlumeSetup.exe", "size": len(self.body), "digest": "sha256:" + self.digest}]}).encode()
        elif self.path == "/PlumeSetup.exe":
            data = self.body
        else:
            data = b"ok"
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


server = http.server.ThreadingHTTPServer(("127.0.0.1", PORT_FEED), Feed)
threading.Thread(target=server.serve_forever, daemon=True).start()
open(os.path.join(DIR, "plume-mail.exe"), "wb").write(b"old")
FENV = dict(UENV, PLUME_UPDATE_FEED="http://127.0.0.1:%d/latest.json" % PORT_FEED)
ui = Ui(FENV)
try:
    s = ui.wait(lambda s: s["update"]["kind"] == "available", "update found")
    check("the window finds the new version", s["update"].get("version") == "v9.9.9", s)
    ui.shot("suite-update.png")
    ui.cmd(cmd="click", id="Update")
    try:
        s = ui.wait(lambda s: s["update"]["kind"] in ("done", "failed"), "update installed", timeout=90)
        check("Update everything downloads, checks and installs", s["update"]["kind"] == "done", s)
    except (RuntimeError, OSError, ValueError):
        # the window restarts itself into the new version once the update is installed
        s = None
    check("the installed apps were updated", os.path.getsize(os.path.join(DIR, "plume-mail.exe")) > 1000)
finally:
    try:
        ui.close()
    except Exception:
        pass
    server.shutdown()
# the window may have restarted into the new version: close it too (sandbox window only)
time.sleep(3)
try:
    s = socket.create_connection(("127.0.0.1", PORT_UI), timeout=3)
    f = s.makefile("rw")
    f.write(json.dumps({"cmd": "state"}) + "\n")
    f.flush()
    st = json.loads(f.readline())
    check("after the update, the new Plume Suite opens and says it is done", st["update"]["kind"] == "done", st)
    subprocess.run(["taskkill", "/PID", str(subprocess.run(["powershell", "-NoProfile", "-Command",
                   "(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq '%s' }).ProcessId" % os.path.join(DIR, "plume-suite.exe")],
                   capture_output=True, text=True).stdout.strip() or "0")], capture_output=True)
except OSError:
    print("note: the window did not restart (it does so only when it runs from its install folder)")

# ── 8. uninstall everything
check("--uninstall-all --quiet", setup("--uninstall-all", "--quiet", exe=CACHED) == 0)
for _ in range(20):
    if not os.path.isdir(DIR):
        break
    time.sleep(0.5)
check("the whole Plume folder is gone", not os.path.isdir(DIR), files())

print("ALL %d PASS" % len(passed) if not fails else "FAILED: " + ", ".join(fails))
sys.exit(1 if fails else 0)
