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
check("--install viewer", setup("--install", "viewer", exe=CACHED) == 0 and there("plume-viewer.exe"), files())
check("--install music", setup("--install", "music", exe=CACHED) == 0 and there("plume-music.exe"), files())
NEW = ['drop', 'launcher', 'passwords', 'media', 'task', 'clean', 'backup', 'clipboard', 'pdf', 'calendar', 'convert', 'notepad', 'cut', 'create', 'boost']
for key in NEW:
    check("--install " + key, setup("--install", key, exe=CACHED) == 0 and there("plume-%s.exe" % key), files())
# an app installs what it needs: Plume Boost keeps a copy of its settings with Plume Backup
check("--uninstall backup and boost", setup("--uninstall", "backup", exe=CACHED) == 0 and setup("--uninstall", "boost", exe=CACHED) == 0
      and not there("plume-backup.exe") and not there("plume-boost.exe"), files())
check("--install boost brings Plume Backup with it", setup("--install", "boost", exe=CACHED) == 0 and there("plume-boost.exe", "plume-backup.exe"), files())
check("an unknown app is refused", setup("--uninstall", "nope", exe=CACHED) != 0)

# ── 3. an update rewrites every installed app, and only those
open(os.path.join(DIR, "plume-fex.exe"), "wb").write(b"old")
check("--update succeeds", setup("--update") == 0)
check("an update brings installed apps up to date", os.path.getsize(os.path.join(DIR, "plume-fex.exe")) > 1000)
check("an update does not add apps", not there("plume-mail.exe"), files())

# ── 4. removing
check("--uninstall fex removes it", setup("--uninstall", "fex", exe=CACHED) == 0 and not there("plume-fex.exe"), files())
check("--uninstall viewer removes it", setup("--uninstall", "viewer", exe=CACHED) == 0 and not there("plume-viewer.exe"), files())
check("--uninstall music removes it", setup("--uninstall", "music", exe=CACHED) == 0 and not there("plume-music.exe"), files())
for key in NEW:
    check("--uninstall %s removes it" % key, setup("--uninstall", key, exe=CACHED) == 0 and not there("plume-%s.exe" % key), files())
check("removing an app that is not there is fine", setup("--uninstall", "mail", exe=CACHED) == 0)
check("Plume Mail is not in this version: it cannot be installed", setup("--install", "mail", exe=CACHED) != 0 and not there("plume-mail.exe"), files())
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


# "Set as default" writes under this test key of HKCU (never the real classes), and the window then only notes the
# Windows Settings link instead of opening it
DEFROOT = r"Software\PlumeSuiteDefaultsTest"
subprocess.run(["reg", "delete", "HKCU\\" + DEFROOT, "/f"], capture_output=True)
UENV = dict(ENV, PLUME_SUITE_TEST_PORT=str(PORT_UI), PLUME_SUITE_NOACTIVATE="1", PLUME_DEFAULTS_ROOT=DEFROOT)


def reg_value(key, name):
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, DEFROOT + "\\" + key) as k:
            return winreg.QueryValueEx(k, name)[0]
    except OSError:
        return None
COUNT = 20
NONE = [False] * COUNT
FEX_ONLY = [False, True] + [False] * (COUNT - 2)
ui = Ui(UENV)
try:
    s = ui.wait(lambda s: s["installed"] is not None, "window")
    check("the window sees what is installed (nothing now)", s["installed"] == NONE, s)
    ui.shot("suite-empty.png")
    hits = [h["id"] for h in ui.cmd(cmd="hits")]
    check("Plume Mail shows as coming soon, without an Install button", "Install(2)" not in hits and "Install(1)" in hits, hits)
    check("Plume Viewer can be installed from the window", "Install(3)" in hits, hits)
    check("Plume Music can be installed from the window", "Install(4)" in hits, hits)
    # the cards further down come into view when the list scrolls; each new app has its Install button
    for i in range(5, COUNT):
        ui.cmd(cmd="reveal", app=i)
        ok = False
        for _ in range(20):
            if "Install(%d)" % i in [h["id"] for h in ui.cmd(cmd="hits")]:
                ok = True
                break
            time.sleep(0.25)
        check("app %d (%s) can be installed from the window" % (i, NEW[i - 5]), ok)
    s = ui.cmd(cmd="state")
    check("the list scrolled to show the last cards", s["scroll"] > 0, s["scroll"])
    ui.shot("suite-scrolled.png")
    ui.cmd(cmd="reveal", app=8)
    time.sleep(0.5)
    ui.cmd(cmd="click", id="Install(8)")
    s = ui.wait(lambda s: s["busy"] is None and s["installed"][8], "Media installed")
    check("Install on a card further down installs that app (Plume Media)", there("plume-media.exe"), files())
    # ── 5b. Set as default (Plume Media): the app registers its types, Windows' page would open
    check("Plume Media can open files: its card knows it is not offered to Windows yet", s["defaults"][8] == "not-registered", s["defaults"])
    check("apps that open no files have no default state", s["defaults"][5] is None and s["defaults"][1] is None, s["defaults"])
    time.sleep(0.4)
    check("the card shows Set as default", "SetDefault(8)" in [h["id"] for h in ui.cmd(cmd="hits")])
    ui.shot("suite-default-pill.png")
    ui.cmd(cmd="click", id="SetDefault(8)")
    s = ui.cmd(cmd="state")
    check("the first time, it explains what Windows will ask", s["explain"] == 8 and s["settings_link"] is None, s)
    time.sleep(0.4)
    ui.shot("suite-default-explain.png")
    ui.cmd(cmd="click", id="DialogOk")
    s = ui.wait(lambda s: s["defaulting"] is None and s["settings_link"], "Plume Media registered")
    check("Continue registers Plume Media and goes to its Default apps page",
          s["settings_link"] == "ms-settings:defaultapps?registeredAppUser=Plume%20Media" and s["explain"] is None, s)
    check("it says what to click in Windows", s["toast"] and "Set default" in s["toast"], s["toast"])
    check("Plume Media is listed for Default apps (test key)", reg_value(r"Software\RegisteredApplications", "Plume Media") == r"Software\Plume\Media\Capabilities")
    check("its capabilities claim photos and videos",
          reg_value(r"Software\Plume\Media\Capabilities\FileAssociations", ".jpg") == "PlumeMedia.Photo"
          and reg_value(r"Software\Plume\Media\Capabilities\FileAssociations", ".mp4") == "PlumeMedia.Video")
    check("the open command is the installed Plume Media",
          (reg_value(r"Software\Classes\PlumeMedia.Photo\shell\open\command", "") or "").lower() == ('"%s" "%%1"' % os.path.join(DIR, "plume-media.exe")).lower())
    check("the card now says registered, not default yet", s["defaults"][8] == "registered", s["defaults"])
    check("the explanation is remembered", os.path.isfile(os.path.join(LOCAL, "Plume", "Suite", "defaults-explained")))
    # what Windows writes once the user clicks Set default (only ever under the test key)
    for ext, pid in [("jpg", "Photo"), ("jpeg", "Photo"), ("png", "Photo"), ("gif", "Photo"), ("webp", "Photo"), ("mp4", "Video"), ("mov", "Video"), ("mkv", "Video")]:
        with winreg.CreateKey(winreg.HKEY_CURRENT_USER, DEFROOT + r"\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.%s\UserChoice" % ext) as k:
            winreg.SetValueEx(k, "ProgId", 0, winreg.REG_SZ, "PlumeMedia." + pid)
    s = ui.cmd(cmd="refresh")
    check("once Windows opens them with it, the card says Default", s["defaults"][8] == "default", s["defaults"])
    time.sleep(0.4)
    ui.shot("suite-default-done.png")
    code = subprocess.run([os.path.join(DIR, "plume-media.exe"), "--unregister-defaults"], env=UENV, timeout=30).returncode
    check("--unregister-defaults (what removal runs) takes everything back",
          code == 0 and reg_value(r"Software\RegisteredApplications", "Plume Media") is None
          and reg_value(r"Software\Classes\PlumeMedia.Photo", "") is None
          and reg_value(r"Software\Plume\Media\Capabilities", "ApplicationName") is None, code)
    subprocess.run(["reg", "delete", "HKCU\\" + DEFROOT, "/f"], capture_output=True)
    setup("--uninstall", "media", exe=CACHED)
    ui.cmd(cmd="reveal", app=0)
    ui.cmd(cmd="refresh")
    ui.cmd(cmd="click", id="Install(1)")
    s = ui.wait(lambda s: s["busy"] is None and s["installed"][1], "FEx installed")
    check("Install in the window installs Plume FEx", there("plume-fex.exe") and s["installed"] == FEX_ONLY, s)
    check("it says folders now open in Plume FEx, and how to undo it", s["toast"] and "Plume FEx" in s["toast"] and "undo" in s["toast"], s["toast"])
    ui.cmd(cmd="click", id="Install(0)")
    s = ui.wait(lambda s: s["busy"] is None and s["installed"][0], "Web installed")
    ui.shot("suite-two.png")
    # removing asks first (a Windows dialog): the test removes Plume Web with the installer, the window follows
    setup("--uninstall", "web", exe=CACHED)
    s = ui.cmd(cmd="refresh")
    check("the window follows a change made elsewhere", s["installed"] == FEX_ONLY, s)
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
check("Start menu: Plume Suite and the installed app (FEx), not the others",
      sorted(os.listdir(menu)) == ["Plume FEx.lnk", "Plume Suite.lnk"], os.listdir(menu) if os.path.isdir(menu) else None)
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
open(os.path.join(DIR, "plume-fex.exe"), "wb").write(b"old")
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
    except (RuntimeError, OSError, ValueError, KeyError):
        # the window restarts itself into the new version once the update is installed
        s = None
    check("the installed apps were updated", os.path.getsize(os.path.join(DIR, "plume-fex.exe")) > 1000)
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
