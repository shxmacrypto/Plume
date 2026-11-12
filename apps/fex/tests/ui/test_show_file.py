import os, subprocess, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fex
from fex import *

# "Show in folder" from another app (Plume Web's downloads, Plume Convert...): Plume FEx gets a FILE path and opens
# its folder with the file selected, at start and when it is already running.
d = fixture("fixture-showfile")
check = Checks()
inside = os.path.join(d, "Alpha", "inside.txt")
x = Fex(inside, port=47830, profile="prof-showfile")
prof = os.path.join(OUT, "prof-showfile")


def launch(*args):
    e = dict(os.environ)
    e.update({"PLUME_FEX_NOACTIVATE": "1", "PLUME_FEX_TEST_PORT": "47831", "PLUME_FEX_PROFILE": prof, "PLUME_FEX_INDEX_ROOTS": ""})
    return subprocess.Popen([fex.EXE, *args], env=e, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


try:
    s = x.wait(lambda s: not s["stale"] and s["sel"] == ["inside.txt"], timeout=8, what="file selected at start")
    check("started with a file: its folder opens, the file selected", s["loc"] == os.path.join(d, "Alpha") and s["sel"] == ["inside.txt"], (s["loc"], s["sel"]))
    # already running: a second launch with a file hands it over, a new tab shows it selected
    p = launch(os.path.join(d, "notes.md"))
    try:
        code = p.wait(8)
    except subprocess.TimeoutExpired:
        code = None
        p.kill()
    s = x.wait(lambda s: len(s["tabs"]) == 2 and not s["stale"] and s["sel"] == ["notes.md"], timeout=8, what="file handed to the running window")
    check("running window: new tab on the file's folder, the file selected", code == 0 and s["loc"] == d and s["sel"] == ["notes.md"], (code, s["loc"], s["sel"]))
    # a folder still opens as a folder, nothing selected
    p = launch(os.path.join(d, "beta"))
    p.wait(8)
    s = x.wait(lambda s: len(s["tabs"]) == 3 and not s["stale"], timeout=8, what="folder tab")
    check("a folder path still opens the folder itself", s["loc"] == os.path.join(d, "beta") and s["sel"] == [], (s["loc"], s["sel"]))
    # a path that does not exist: nothing opens for it (a bare new tab, as before)
    p = launch(os.path.join(d, "missing.txt"))
    p.wait(8)
    s = x.wait(lambda s: len(s["tabs"]) == 4 and not s["stale"], timeout=8, what="tab for a missing file")
    check("a missing file opens This PC", s["loc"] == "pc", s["loc"])
finally:
    x.close()
check.done()
