import os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

# Search inside files: the `content:` syntax, and the "Search inside files" option of the filter button.
d = fixture("fixture-content")
with open(os.path.join(d, "Alpha", "rdv.txt"), "w", encoding="utf-8") as f:
    f.write("Rendez-vous à la Gare du Nord à midi.")
with open(os.path.join(d, "readme.md"), "w", encoding="utf-8") as f:
    f.write("Nothing about trains here.")
x = Fex(d, port=47826, index=d)
check = Checks()


def search(text):
    if x.state()["edit"] != "Search":
        x.click("Search")
    x.edit(text)
    return x.wait(lambda s: s["load"] == "ready" and not s["stale"] and s["filter"] == text, what="results for " + text)


try:
    x.wait()
    x.wait(lambda s: s["index_ready"], timeout=15, what="index ready")
    s = search("content:gare")
    check("content: finds the file that contains the word, accents and case aside", s["visible"] == ["rdv.txt"], s["visible"])
    s = search("gare")
    check("without it, only names are searched", s["rows"] == 0, s["visible"])
    x.cmd("menu", choose="Search inside files (slower)")
    x.click("Filter")
    s = x.wait(lambda s: s["search_match"]["content"] and s["load"] == "ready" and not s["stale"], what="option on")
    check("the option searches the typed text inside files", s["visible"] == ["rdv.txt"], s["visible"])
    s = search("gare du nord")
    check("the whole text is one phrase", s["visible"] == ["rdv.txt"], s["visible"])
    x.shot("content-search")
    x.cmd("menu", choose="Search inside files (slower)")
    x.click("Filter")
    s = x.wait(lambda s: not s["search_match"]["content"] and s["load"] == "ready" and not s["stale"], what="option off")
    check("turned off, names again", s["rows"] == 0, s["visible"])
finally:
    x.close()
check.done()
