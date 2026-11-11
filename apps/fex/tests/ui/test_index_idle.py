import os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

d = fixture("fixture-idle")
os.makedirs(os.path.join(d, "Alpha", "Inner", "Deep"))
with open(os.path.join(d, "Alpha", "Inner", "Deep", "old report.txt"), "wb") as f:
    f.write(b"x" * 100)
# the index leaves memory after 1.2 s without a search (90 s in normal use)
x = Fex(d, port=47831, index=d, env={"PLUME_FEX_INDEX_IDLE_MS": "1200"})
check = Checks()
try:
    x.wait()
    s = x.wait(lambda s: s["index_ready"] or s["index_asleep"], timeout=20, what="index built")
    s = x.wait(lambda s: s["index_asleep"], timeout=10, what="index asleep")
    check("unused, the index leaves memory", not s["index_ready"] and s["index_asleep"], (s["index_ready"], s["index_asleep"]))
    # a file created while it sleeps
    with open(os.path.join(d, "Alpha", "report made while asleep.txt"), "wb") as f:
        f.write(b"zzz")
    time.sleep(0.3)
    # searching wakes it up, and it has caught up with the disk
    x.click("Search")
    x.edit("report")
    t0 = time.time()
    s = x.wait(lambda s: s["loc"] == "search:report" and s["load"] == "ready" and not s["stale"] and s["rows"] == 2, timeout=15, what="results after waking up")
    check("searching wakes the index and it has caught up (%.2f s)" % (time.time() - t0), sorted(s["visible"]) == ["old report.txt", "report made while asleep.txt"], s["visible"])
    # while results are on screen it stays awake
    time.sleep(3.0)
    s = x.state()
    check("it stays in memory while results are shown", s["index_ready"] and not s["index_asleep"], (s["index_ready"], s["index_asleep"]))
    # back to the folder: it goes to sleep again, and the folder view does not care
    x.editkey(VK["ESC"])
    s = x.wait(lambda s: s["index_asleep"], timeout=10, what="asleep again")
    check("back in the folder, it sleeps again", s["loc"] == d and s["rows"] == 11, (s["loc"], s["rows"]))
finally:
    x.close()
check.done()
