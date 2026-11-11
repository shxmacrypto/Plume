# Real-disk run: index the fixed drives into a throwaway profile, measure, restart, measure again.
import os, shutil, subprocess, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fex
from fex import *

prof = os.path.join(OUT, "prof-real")
if "--keep" not in sys.argv:
    shutil.rmtree(prof, ignore_errors=True)


def mem(pid):
    r = subprocess.run(["powershell", "-NoProfile", "-Command",
                        "$p = Get-Process -Id %d; '{0:N0} MB working set, {1:N0} MB private' -f ($p.WorkingSet64/1MB), ($p.PrivateMemorySize64/1MB)" % pid],
                       capture_output=True, text=True, stdin=subprocess.DEVNULL)
    return r.stdout.strip()


def search(x, text):
    if x.state()["edit"] != "Search":
        x.click("Search")
    x.edit(text)
    return x.wait(lambda s: s["load"] == "ready" and not s["stale"], what="results for " + text)


def run(label, port):
    e = dict(os.environ)
    e.pop("PLUME_FEX_INDEX_ROOTS", None)
    x = Fex.__new__(Fex)
    env = {"PLUME_FEX_NOACTIVATE": "1", "PLUME_FEX_TEST_PORT": str(port), "PLUME_FEX_PROFILE": prof}
    e.update(env)
    import socket
    x.p = subprocess.Popen([fex.EXE, r"C:\Users"], env=e, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    t0 = time.time()
    while True:
        try:
            x.s = socket.create_connection(("127.0.0.1", port), timeout=30)
            break
        except OSError:
            time.sleep(0.05)
    x.f = x.s.makefile("rw", encoding="utf-8", newline="\n")
    try:
        x.wait()
        last = 0
        while True:
            s = x.state()
            if s["index_ready"]:
                break
            if time.time() - t0 > 240:
                raise RuntimeError("index not ready after 240 s")
            time.sleep(0.25)
        print("%s: search ready %.2f s after launch (%s items walked this run), %s" % (label, time.time() - t0, format(s["index_count"], ","), mem(x.p.pid)))
        for q in ["plume", "*.rs", "report", "ext:mp4 size:>100mb", "e"]:
            s = search(x, q)
            print("   %-22s %9s results in %6.1f ms   first: %s" % (q, format(s["total"], ","), s["took_ms"], s["visible"][:2]))
        x.click("ScopeAll")
        s = x.wait(lambda s: s["load"] == "ready" and s["everywhere"])
        print("   %-22s %9s results in %6.1f ms   (everywhere)" % ("e", format(s["total"], ","), s["took_ms"]))
        x.shot("bench_disk-" + label)
    finally:
        t1 = time.time()
        x.close()
        print("   closed in %.2f s; index file: %s MB" % (time.time() - t1, format(os.path.getsize(os.path.join(prof, "index.bin")) // (1 << 20), ",") if os.path.exists(os.path.join(prof, "index.bin")) else "none"))


run("first-launch", 47818)
run("second-launch", 47819)
