import os, subprocess, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fex
from fex import *

d = fixture("fixture8")
check = Checks()
x = Fex(d, port=47820, profile="prof-single")
prof = os.path.join(OUT, "prof-single")


def launch(*args, profile=prof, port=47821):
    e = dict(os.environ)
    e.update({"PLUME_FEX_NOACTIVATE": "1", "PLUME_FEX_TEST_PORT": str(port), "PLUME_FEX_PROFILE": profile, "PLUME_FEX_INDEX_ROOTS": ""})
    return subprocess.Popen([fex.EXE, *args], env=e, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


try:
    s = x.wait()
    check("first window", len(s["tabs"]) == 1 and s["loc"] == d, s["tabs"])
    # launched again with a folder: the running window opens it in a new tab, the second process ends
    p = launch(os.path.join(d, "Alpha"))
    try:
        code = p.wait(8)
    except subprocess.TimeoutExpired:
        code = None
        p.kill()
    s = x.wait(lambda s: len(s["tabs"]) == 2 and not s["stale"], timeout=5, what="tab opened by the second launch")
    check("second launch hands its folder to the running window and exits", code == 0 and s["cur"] == 1 and s["loc"] == os.path.join(d, "Alpha"), (code, s["tabs"]))
    # launched again with nothing: a new tab
    p = launch()
    try:
        code = p.wait(8)
    except subprocess.TimeoutExpired:
        code = None
        p.kill()
    s = x.wait(lambda s: len(s["tabs"]) == 3 and not s["stale"], timeout=5, what="new tab from a bare launch")
    check("bare second launch opens a new tab", code == 0 and s["loc"] == "pc", (code, s["tabs"]))
    # several folders on the command line
    p = launch(os.path.join(d, "beta"), os.path.join(d, "Gamma"))
    p.wait(8)
    s = x.wait(lambda s: len(s["tabs"]) == 5 and not s["stale"], timeout=5, what="two folders at once")
    check("several folders: one tab each", [t["loc"] for t in s["tabs"]][-2:] == [os.path.join(d, "beta"), os.path.join(d, "Gamma")], s["tabs"])
    # another data folder is another installation: it gets its own window
    other = os.path.join(OUT, "prof-single-other")
    y = None
    q = launch(d, profile=other, port=47822)
    time.sleep(1.5)
    alive = q.poll() is None
    check("a different profile starts its own window", alive and len(x.state()["tabs"]) == 5, alive)
    if alive:
        import socket, json
        sock = socket.create_connection(("127.0.0.1", 47822), timeout=10)
        f = sock.makefile("rw", encoding="utf-8", newline="\n")
        f.write(json.dumps({"cmd": "close"}) + "\n"); f.flush(); f.readline()
        try:
            q.wait(5)
        except subprocess.TimeoutExpired:
            q.kill()
finally:
    x.close()
check.done()
