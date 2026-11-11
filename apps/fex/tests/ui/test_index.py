import os, shutil, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

d = fixture("fixture6")
os.makedirs(os.path.join(d, "Alpha", "Inner", "Deep"))
with open(os.path.join(d, "Alpha", "Inner", "Deep", "old report.txt"), "wb") as f:
    f.write(b"x" * 100)
prof = os.path.join(OUT, "prof-index")
shutil.rmtree(prof, ignore_errors=True)
check = Checks()


def search(x, text):
    if x.state()["edit"] != "Search":
        x.click("Search")
    x.edit(text)
    return x.wait(lambda s: s["load"] == "ready" and not s["stale"], what="results for " + text)


x = Fex(d, port=47816, index=d, profile="prof-index", fresh=False)
try:
    t0 = time.time()
    s = x.wait(lambda s: s["index_ready"], timeout=20, what="index ready (first build)")
    print("     first index ready after %.2f s" % (time.time() - t0))
    s = search(x, "report")
    check("search works after the first build", s["visible"] == ["old report.txt"], s["visible"])
    # the index follows the disk: new file deep in a subfolder, without visiting it
    with open(os.path.join(d, "Alpha", "Inner", "new report.txt"), "wb") as f:
        f.write(b"y" * 5000)
    t0 = time.time()
    s = x.wait(lambda s: s["rows"] == 2 and s["load"] == "ready", timeout=10, what="new file in the results")
    check("a new file shows up in the results by itself (%.1f s)" % (time.time() - t0), sorted(s["visible"]) == ["new report.txt", "old report.txt"], s["visible"])
    # a whole new folder tree
    os.makedirs(os.path.join(d, "beta", "Added", "More"))
    with open(os.path.join(d, "beta", "Added", "More", "report in new tree.txt"), "wb") as f:
        f.write(b"z")
    s = x.wait(lambda s: s["rows"] == 3 and s["load"] == "ready", timeout=10, what="file of a new folder tree")
    check("files inside newly created folders are found", "report in new tree.txt" in s["visible"], s["visible"])
    # deletion and rename
    os.remove(os.path.join(d, "Alpha", "Inner", "Deep", "old report.txt"))
    os.rename(os.path.join(d, "Alpha", "Inner", "new report.txt"), os.path.join(d, "Alpha", "Inner", "final report.txt"))
    s = x.wait(lambda s: sorted(s["visible"]) == ["final report.txt", "report in new tree.txt"] and s["load"] == "ready", timeout=10, what="delete + rename")
    check("deleted and renamed files follow", s["rows"] == 2, s["visible"])
    # a renamed folder: its content is found at the new place
    os.rename(os.path.join(d, "Alpha"), os.path.join(d, "Omega"))
    s = x.wait(lambda s: any(f and f.endswith(os.path.join("Omega", "Inner")) for f in s["folders"]) and s["load"] == "ready", timeout=10, what="renamed folder")
    check("a renamed folder keeps its files in the index", sorted(s["visible"]) == ["final report.txt", "report in new tree.txt"], (s["visible"], s["folders"]))
finally:
    x.close()

saved = os.path.join(prof, "index.bin")
check("the index is saved when the app closes", os.path.exists(saved) and os.path.getsize(saved) > 500, os.path.exists(saved))

# while the app is closed: changes on disk
with open(os.path.join(d, "Omega", "report made offline.txt"), "wb") as f:
    f.write(b"offline")
os.remove(os.path.join(d, "Omega", "Inner", "final report.txt"))
time.sleep(0.5)

x = Fex(d, port=47817, index=d, profile="prof-index", fresh=False)
try:
    t0 = time.time()
    s = x.wait(lambda s: s["index_ready"], timeout=20, what="index ready (from the saved file)")
    print("     saved index ready after %.2f s" % (time.time() - t0))
    s = search(x, "report")
    s = x.wait(lambda s: sorted(s["visible"]) == ["report in new tree.txt", "report made offline.txt"] and s["load"] == "ready", timeout=10, what="caught up with offline changes")
    check("at the next launch the saved index catches up with what changed meanwhile", s["rows"] == 2, s["visible"])
    x.shot("index-results")
finally:
    x.close()
check.done()
