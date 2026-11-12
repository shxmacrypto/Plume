# Drives a Plume Create test build (cargo build --profile fast -p plume-create --features test-hook) through its test socket; screenshots by PrintWindow (never takes focus).
import json, os, socket, subprocess, sys, time
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
PW = os.path.join(HERE, "printwindow.ps1")
SHOTS = os.path.join(ROOT, "target", "shots-create")
class App:
    def __init__(self, exe, port=47811, data=None, env=None):
        os.makedirs(SHOTS, exist_ok=True)
        e = dict(os.environ)
        e["PLUME_CREATE_TEST_PORT"] = str(port)
        e["PLUME_CREATE_NOACTIVATE"] = "1"
        e["PLUME_CREATE_DIR"] = data or os.path.join(os.environ["TEMP"], "plume-create-test-data")
        e.update(env or {})
        self.p = subprocess.Popen([exe], env=e)
        self.port = port
        for _ in range(100):
            try:
                self.s = socket.create_connection(("127.0.0.1", port), timeout=120)
                break
            except OSError:
                time.sleep(0.2)
        self.f = self.s.makefile("rw", encoding="utf-8", newline="\n")
    def __call__(self, cmd, **kw):
        kw["cmd"] = cmd
        self.f.write(json.dumps(kw) + "\n"); self.f.flush()
        r = json.loads(self.f.readline())
        # the window paints between commands: the next click finds what this one opened
        if cmd in ("click", "drag", "key", "type", "open", "template", "preset", "wheel"):
            time.sleep(0.15)
        return r
    def shot(self, name):
        time.sleep(0.4)
        self("repaint"); time.sleep(0.3)
        out = os.path.join(SHOTS, name + ".png")
        r = subprocess.run(["powershell", "-ExecutionPolicy", "Bypass", "-File", PW, "-ProcId", str(self.p.pid), "-Out", out], capture_output=True, text=True)
        return out
    def hit(self, prefix):
        return [h for h in self("hits") if h["id"].startswith(prefix)]
    def close(self):
        try:
            self.p.terminate()
        except Exception:
            pass
