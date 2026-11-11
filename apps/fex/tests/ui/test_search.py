import os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

d = fixture("fixture5")
os.makedirs(os.path.join(d, "Alpha", "Inner", "Deep"))
for rel in [("Alpha", "Inner", "report 2026.txt"), ("Alpha", "Inner", "Deep", "Report final.TXT"), ("beta", "été.txt")]:
    with open(os.path.join(d, *rel), "wb") as f:
        f.write(b"x" * 2048)
x = Fex(d, port=47815, index=d)
check = Checks()


def search(text):
    if x.state()["edit"] != "Search":
        x.click("Search")
    x.edit(text)
    return x.wait(lambda s: s["load"] == "ready" and not s["stale"], what="results for " + text)


try:
    s = x.wait()
    s = x.wait(lambda s: s["index_ready"], timeout=15, what="index ready")
    check("the index is built in the background", s["index_count"] >= 15, s["index_count"])
    # search in the open folder and its subfolders
    s = search("report")
    check("instant search finds files in subfolders", s["visible"] == ["Report final.TXT", "report 2026.txt"] or s["visible"] == ["report 2026.txt", "Report final.TXT"], s["visible"])
    check("each result shows its folder", sorted(s["folders"]) == sorted([os.path.join(d, "Alpha", "Inner"), os.path.join(d, "Alpha", "Inner", "Deep")]), s["folders"])
    check("the tab becomes a search tab", s["loc"] == "search:report" and s["total"] == 2 and s["can_back"], (s["loc"], s["total"]))
    x.shot("search-results")
    s = search("a1"); check("typing more refines", s["visible"] == ["a1.txt", "a10.txt"], s["visible"])
    s = search("*.txt"); check("wildcards", s["total"] == 7 and all(n.lower().endswith(".txt") for n in s["visible"]), s["visible"])
    s = search("ext:png"); check("ext:", s["visible"] == ["photo.png"], s["visible"])
    s = search("size:>1mb"); check("size:", s["visible"] == ["Zed.bin"], s["visible"])
    s = search("alpha\\"); check("path search", sorted(s["visible"]) == sorted(["Inner", "inside.txt", "Deep", "report 2026.txt", "Report final.TXT"]), s["visible"])
    s = search("ÉTÉ"); check("accents and case", s["visible"] == ["été.txt"], s["visible"])
    s = search("folder: e"); check("folder:", sorted(s["visible"]) == sorted(["beta", "Inner", "Deep"]), s["visible"])
    s = search("zzzz"); check("nothing found", s["rows"] == 0 and s["total"] == 0, s["rows"])
    # sorting results
    s = search("txt")
    x.click("Header(Size)"); s = x.wait(lambda s: s["load"] == "ready")
    sizes_first = s["visible"][0]
    check("results sorted by size, biggest first", sizes_first in ("report 2026.txt", "Report final.TXT", "été.txt", "a2.txt"), s["visible"])
    x.click("Header(Name)"); s = x.wait(lambda s: s["load"] == "ready")
    check("results sorted by name", s["visible"][:3] == ["a1.txt", "a2.txt", "a10.txt"], s["visible"])
    # open a result's folder, come back to the results
    s = search("deep")
    x.editkey(VK["ENTER"]); x.key(VK["ENTER"]); s = x.wait(lambda s: s["loc"].endswith("Deep") and not s["stale"], what="inside Deep")
    check("Enter opens the folder found", s["visible"] == ["Report final.TXT"], s["visible"])
    x.click("Back"); s = x.wait(lambda s: s["loc"] == "search:deep" and s["load"] == "ready" and not s["stale"], what="back to results")
    check("Back returns to the results", s["visible"] == ["Deep"] and s["filter"] == "deep", (s["visible"], s["filter"]))
    # leaving the search
    x.click("Search"); x.editkey(VK["ESC"]); s = x.wait(lambda s: not s["loc"].startswith("search:") and not s["stale"], what="left the search")
    check("Esc leaves the search and returns to the folder", s["loc"] == d and s["filter"] == "" and s["rows"] == 11, (s["loc"], s["rows"]))
    # scope: this folder or everywhere
    x.nav(os.path.join(d, "Alpha"))
    s = search("txt")
    check("search from a subfolder stays in it", s["total"] == 3 and not s["everywhere"], (s["total"], s["visible"]))
    x.click("ScopeAll"); s = x.wait(lambda s: s["load"] == "ready" and s["everywhere"])
    check("Everywhere searches the whole index", s["total"] == 7, s["total"])
    x.shot("search-everywhere")
    x.click("ScopeHere"); s = x.wait(lambda s: s["load"] == "ready" and not s["everywhere"])
    check("back to this folder only", s["total"] == 3, s["total"])
    # context menu on a result
    x.click("Row(0)")
    labels = x.cmd("shell_labels")
    check("Explorer menu works on a result", any("open" in l.lower() for l in labels) and any("propert" in l.lower() for l in labels), labels[:6])
finally:
    x.close()
check.done()
