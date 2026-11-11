import os, shutil, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

d = fixture("fixture2")
inner = os.path.join(d, "Alpha", "Inner")
x = Fex(d, port=47812)
check = Checks()
try:
    s = x.wait()
    x.cmd("active", on=True)
    x.click("Row(5)"); x.click("Row(7)", ctrl=True)
    s = x.state(); check("two files selected", s["sel"] == ["a2.txt", "data.json"], s["sel"])
    # a file appears on disk
    with open(os.path.join(d, "a05-new.txt"), "wb") as f:
        f.write(b"new")
    t0 = time.time()
    s = x.wait(lambda s: s["rows"] == 12, timeout=6, what="new file shown")
    check("new file appears by itself (%.1f s)" % (time.time() - t0), "a05-new.txt" in s["visible"], s["visible"])
    check("selection kept after the refresh", s["sel"] == ["a2.txt", "data.json"], s["sel"])
    # a file disappears, another changes size
    os.remove(os.path.join(d, "a1.txt"))
    with open(os.path.join(d, "notes.md"), "ab") as f:
        f.write(b"y" * 5000)
    s = x.wait(lambda s: "a1.txt" not in s["visible"], timeout=6, what="deleted file gone")
    check("deleted file disappears", s["rows"] == 11, s["rows"])
    # the open folder's parent can be renamed from outside (no handle is kept on the folder)
    s = x.nav(inner)
    check("inside Alpha\\Inner", s["loc"] == inner, s["loc"])
    try:
        os.rename(os.path.join(d, "Alpha"), os.path.join(d, "Alpha2"))
        renamed = True
    except OSError as e:
        renamed = False
        print("     rename error:", e)
    check("parent of the open folder can be renamed by another program", renamed)
    s = x.wait(lambda s: s["loc"] == d and not s["stale"] and s["load"] == "ready", timeout=8, what="left the missing folder")
    check("when the open folder disappears, FEx goes to the nearest parent", s["loc"] == d and "Alpha2" in s["visible"], (s["loc"], s["visible"]))
    # the open folder itself is deleted
    s = x.nav(os.path.join(d, "beta"))
    shutil.rmtree(os.path.join(d, "beta"))
    s = x.wait(lambda s: s["loc"] == d and not s["stale"] and s["load"] == "ready", timeout=8, what="left the deleted folder")
    check("deleted open folder: back to the parent", "beta" not in s["visible"], s["visible"])
    # nothing changes: no reload flicker, selection and scroll untouched
    x.click("Row(2)")
    before = x.state()
    time.sleep(3.5)
    after = x.state()
    check("idle: selection untouched by background checks", before["sel"] == after["sel"] and before["scroll"] == after["scroll"], (before["sel"], after["sel"]))
    x.shot("refresh-a")
finally:
    x.close()
check.done()
