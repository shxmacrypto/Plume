import os, sys, time, shutil
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

# The sidebar: recent folders, categories (kept across restarts), putting rows in order by dragging them, and the
# mini panels (Ctrl+right-click on a folder) with files dragged between a mini panel and the list, then undone.
# Also the date and size filters of the search bar, and "Sort by" in the View menu.
# Everything happens in a throwaway tree under target/fex-ui and a throwaway profile.
d = fixture("fixture_side")
PROFILE = "prof-side"
x = Fex(d, port=47831, profile=PROFILE, index=d)
check = Checks()


def nav_index(s, kind=None, label=None):
    for i, (k, l) in enumerate(zip(s["nav_kinds"], s["nav"])):
        ok_kind = kind is None or k == kind or (kind.endswith("*") and k.startswith(kind[:-1]))
        if ok_kind and (label is None or l == label):
            return i
    raise RuntimeError("no sidebar row %s %s in %s" % (kind, label, list(zip(s["nav"], s["nav_kinds"]))))


def rect_of(id):
    for h in reversed(x.cmd("hits")):
        if h["id"] == id:
            return h["rect"]
    raise RuntimeError("no hit " + id)


def centre(id, dy=0.0, dx=None):
    r = rect_of(id)
    return (r[0] + (r[2] / 2 if dx is None else dx), r[1] + r[3] / 2 + dy)


def drag(src, dst, **mods):
    """Press on `src` (an id), move in two steps to the point `dst`, release there."""
    p = x.mouse("down", src)
    x.cmd("mouse", type="move", x=p["x"] + 8, y=p["y"] + 8)
    x.cmd("mouse", type="move", x=dst[0], y=dst[1], **mods)
    mid = x.state()
    x.cmd("mouse", type="up", x=dst[0], y=dst[1], **mods)
    time.sleep(0.15)
    return mid


def row_of(name):
    return "Row(%d)" % x.names().index(name)


def until(pred, what, timeout=10):
    end = time.time() + timeout
    while time.time() < end:
        if pred():
            return True
        time.sleep(0.05)
    return False


def menu(label, id, **kw):
    x.cmd("menu", choose=label)
    x.mouse("click", id, button="right", **kw) if kw.get("button") is None else x.mouse("click", id, **kw)
    time.sleep(0.15)


def right(id, choose=None, **kw):
    if choose is not None:
        x.cmd("menu", choose=choose)
    x.mouse("click", id, button="right", **kw)
    time.sleep(0.15)


try:
    x.wait()
    # ───────── recent folders
    for sub in ["Alpha", os.path.join("Alpha", "Inner"), "beta"]:
        x.nav(os.path.join(d, sub))
    s = x.nav(d)
    rec = [os.path.basename(p) for p in s["recent"]]
    check("recent folders: last visited first, no duplicate", rec[:4] == ["fixture_side", "beta", "Inner", "Alpha"], rec)
    x.nav(os.path.join(d, "Alpha"))
    s = x.nav(d)
    rec = [os.path.basename(p) for p in s["recent"]]
    check("visiting again moves it to the top, once", rec[:4] == ["fixture_side", "Alpha", "beta", "Inner"] and rec.count("Alpha") == 1, rec)
    check("recent rows under This PC", s["nav_kinds"].index("Section(Recent)") > s["nav"].index("This PC") and "Recent(0)" in s["nav_kinds"], s["nav_kinds"])
    # a folder that disappears is hidden (but stays remembered, in case its drive comes back)
    gone = os.path.join(d, "Temp gone")
    os.makedirs(gone)
    x.nav(gone)
    x.nav(d)
    shutil.rmtree(gone)
    x.key(VK["F5"])
    ok = until(lambda: "Temp gone" not in x.state()["nav"], "missing folder hidden")
    check("a recent folder that no longer exists is hidden", ok and any(p.endswith("Temp gone") for p in x.state()["recent"]), x.state()["nav"])
    # its right-click menu: remove one, then the section folds and can be hidden
    s = x.state()
    i = nav_index(s, label="Inner")
    right("Nav(%d)" % i, choose="Remove from recent")
    s = x.state()
    check("remove from recent", not any(p.endswith("Inner") for p in s["recent"]), s["recent"])
    x.click("Nav(%d)" % nav_index(s, kind="Section(Recent)"))
    s = x.state()
    check("the recent section folds", not any(k.startswith("Recent(") for k in s["nav_kinds"]) and "Section(Recent)" in s["nav_kinds"], s["nav_kinds"])
    x.click("Nav(%d)" % nav_index(s, kind="Section(Recent)"))
    x.cmd("menu", choose="Recent folders in the sidebar"); x.click("More"); time.sleep(0.15)
    s = x.state()
    check("the recent section can be hidden", not s["show_recent"] and "Section(Recent)" not in s["nav_kinds"], s["nav_kinds"])
    x.cmd("menu", choose="Recent folders in the sidebar"); x.click("More"); time.sleep(0.15)
    check("and shown again", x.state()["show_recent"])

    # ───────── categories
    x.click("NavAdd")
    s = x.state()
    check("+ makes a category and opens its name", len(s["categories"]) == 1 and s["edit"] == "Category(0)", (s["categories"], s["edit"]))
    x.edit("Work"); x.editkey(VK["ENTER"])
    s = x.state()
    check("the category is named", s["categories"][0]["name"] == "Work" and s["edit"] is None, s["categories"])
    # a folder dragged from the list onto the category goes in it (nothing moves on disk)
    head = "Nav(%d)" % nav_index(s, kind="CatHeader(0)")
    mid = drag(row_of("Gamma"), centre(head))
    s = x.state()
    check("dragging a folder onto a category adds it", s["categories"][0]["folders"] == [os.path.join(d, "Gamma")] and os.path.isdir(os.path.join(d, "Gamma")), s["categories"])
    check("while dragging, the category is the target", mid["drag"] == "Files", mid["drag"])
    # the right-click menu of the list offers it too
    right(row_of("Alpha"))
    check("list menu: Add to category…", "Add to category…" in x.cmd("menu_last"), x.cmd("menu_last"))
    # a recent folder, through the sidebar menu
    s = x.state()
    right("Nav(%d)" % nav_index(s, kind="Recent(*", label="Alpha"), choose="Add to category > Work")
    s = x.state()
    names = [os.path.basename(p) for p in s["categories"][0]["folders"]]
    check("sidebar menu: Add to category > Work", names == ["Gamma", "Alpha"], names)
    # a file can't go in a category
    s = x.state()
    drag(row_of("a1.txt"), centre("Nav(%d)" % nav_index(s, kind="CatHeader(0)")))
    s = x.state()
    check("files are refused by a category", len(s["categories"][0]["folders"]) == 2 and "folders" in (s["toast"] or ""), (s["categories"], s["toast"]))
    # colour dot
    s = x.state()
    right("Nav(%d)" % nav_index(s, kind="CatHeader(0)"), choose="Colour > Blue")
    check("colour dot", x.state()["categories"][0]["color"] == 4, x.state()["categories"])
    # put Alpha before Gamma by dragging it
    s = x.state()
    target = centre("Nav(%d)" % nav_index(s, kind="CatFolder(0, 0)"), dy=-8)
    drag("Nav(%d)" % nav_index(s, kind="CatFolder(0, 1)"), target)
    names = [os.path.basename(p) for p in x.state()["categories"][0]["folders"]]
    check("folders of a category are put in order by dragging", names == ["Alpha", "Gamma"], names)
    # a second category from the sidebar menu, then moved above the first
    nv = x.state()["lay"]["nav"]
    x.cmd("menu", choose="New category…")
    x.mouse("click", x=nv[0] + 60, y=nv[1] + nv[3] - 20, button="right")
    time.sleep(0.15)
    x.edit("Games"); x.editkey(VK["ENTER"])
    s = x.state()
    check("a second category", [c["name"] for c in s["categories"]] == ["Work", "Games"], s["categories"])
    drag("Nav(%d)" % nav_index(s, kind="CatHeader(1)"), centre("Nav(%d)" % nav_index(s, kind="CatHeader(0)"), dy=-8))
    s = x.state()
    check("categories are put in order by dragging", [c["name"] for c in s["categories"]] == ["Games", "Work"], s["categories"])
    # a folder dragged from one category onto another moves there
    drag("Nav(%d)" % nav_index(s, kind="CatFolder(1, 1)"), centre("Nav(%d)" % nav_index(s, kind="CatHeader(0)")))
    s = x.state()
    check("a folder moves between categories", [os.path.basename(p) for p in s["categories"][0]["folders"]] == ["Gamma"]
          and [os.path.basename(p) for p in s["categories"][1]["folders"]] == ["Alpha"], s["categories"])
    # rename from the menu
    right("Nav(%d)" % nav_index(s, kind="CatHeader(0)"), choose="Rename")
    x.edit("Jeux"); x.editkey(VK["ENTER"])
    check("rename a category", x.state()["categories"][0]["name"] == "Jeux", x.state()["categories"])
    # folding a category hides its folders
    s = x.state()
    x.click("Nav(%d)" % nav_index(s, kind="CatHeader(0)"))
    s = x.state()
    check("a category folds", not s["categories"][0]["open"] and "CatFolder(0, 0)" not in s["nav_kinds"], s["nav_kinds"])
    x.click("Nav(%d)" % nav_index(s, kind="CatHeader(0)"))

    # ───────── the folders at the top: put in order by dragging
    s = x.state()
    quick = [l for l, k in zip(s["nav"], s["nav_kinds"]) if k.startswith("Quick(")]
    if len(quick) >= 3:
        drag("Nav(2)", centre("Nav(0)", dy=-8))
        s = x.state()
        now = [l for l, k in zip(s["nav"], s["nav_kinds"]) if k.startswith("Quick(")]
        check("the top folders are put in order by dragging", now == [quick[2], quick[0], quick[1]] + quick[3:], (quick, now))
    # a recent folder dragged among them is pinned there
    s = x.state()
    drag("Nav(%d)" % nav_index(s, kind="Recent(0)"), centre("Nav(1)", dy=8))
    ok = until(lambda: x.state()["nav"][2] == "fixture_side", "pinned")
    check("a recent folder dragged to the top is pinned at that place", ok and any(p.endswith("fixture_side") for p in x.state()["pinned"]), x.state()["nav"][:4])

    # ───────── kept after a restart
    before = x.state()
    x.close()
    x = Fex(d, port=47831, profile=PROFILE, fresh=False, index=d)
    s = x.wait()
    until(lambda: x.state()["nav"][2] == "fixture_side", "places")
    s = x.state()
    check("categories are kept after a restart", s["categories"] == before["categories"], (s["categories"], before["categories"]))
    check("recent folders and order are kept", s["recent"][:3] == before["recent"][:3] and s["quick_order"] == before["quick_order"], s["recent"])

    # ───────── mini panel: Ctrl+right-click a folder
    right(row_of("Alpha"), ctrl=True)
    s = x.wait(lambda s: s["panels"] and s["panels"][0]["ready"], what="mini panel")
    pan = s["panels"][0]
    check("Ctrl+right-click opens a mini panel", pan["dir"] == os.path.join(d, "Alpha") and pan["rows"] == ["Inner", "inside.txt"], pan)
    pid = pan["id"]
    x.shot("sidebar-panel")
    # into a subfolder and back by its breadcrumb
    x.dbl("PanelRow(%d, 0)" % pid)
    s = x.wait(lambda s: s["panels"][0]["dir"].endswith("Inner") and s["panels"][0]["ready"], what="panel in Inner")
    check("double-click goes into a subfolder in the panel", s["panels"][0]["rows"] == [], s["panels"])
    crumbs = [h["id"] for h in x.cmd("hits") if h["id"].startswith("PanelCrumb(%d," % pid)]
    x.click(crumbs[-2])
    s = x.wait(lambda s: s["panels"][0]["dir"].endswith("Alpha") and s["panels"][0]["ready"], what="panel back in Alpha")
    check("its breadcrumb goes back up", s["panels"][0]["rows"] == ["Inner", "inside.txt"], s["panels"])
    # a file from the panel into the list: moved on disk
    src, dst = os.path.join(d, "Alpha", "inside.txt"), os.path.join(d, "inside.txt")
    mid = drag("PanelRow(%d, 1)" % pid, centre("List"))
    ok = until(lambda: os.path.exists(dst) and not os.path.exists(src), "moved to the list")
    check("panel -> list moves the file", ok and mid["drag"] == "Files", (os.path.exists(src), os.path.exists(dst)))
    ok = until(lambda: "inside.txt" in x.names() and x.state()["panels"][0]["rows"] == ["Inner"], "both views updated")
    check("the list and the panel show it", ok, (x.names(), x.state()["panels"]))
    # and back: from the list into the panel
    drag(row_of("inside.txt"), centre("Panel(%d)" % pid, dy=80))
    ok = until(lambda: os.path.exists(src) and not os.path.exists(dst), "moved into the panel")
    check("list -> panel moves it back", ok, (os.path.exists(src), os.path.exists(dst)))
    # undo both (Ctrl+Z), newest first
    undo_before = x.state()["undo"]
    x.key(L("Z"), ctrl=True)
    ok = until(lambda: os.path.exists(dst) and not os.path.exists(src), "first undo")
    check("Ctrl+Z undoes the last move", ok and undo_before >= 2, (os.path.exists(src), os.path.exists(dst), undo_before))
    x.key(L("Z"), ctrl=True)
    ok = until(lambda: os.path.exists(src) and not os.path.exists(dst), "second undo")
    check("and the one before", ok, (os.path.exists(src), os.path.exists(dst)))
    # Ctrl = copy
    copy = os.path.join(d, "Alpha", "a1.txt")
    drag(row_of("a1.txt"), centre("Panel(%d)" % pid, dy=80), ctrl=True)
    ok = until(lambda: os.path.exists(copy) and os.path.exists(os.path.join(d, "a1.txt")), "copied")
    check("Ctrl while dropping copies", ok)
    if os.path.exists(copy):
        os.remove(copy)
    # a second panel stacks, Esc closes the one in front
    right(row_of("beta"), ctrl=True)
    s = x.wait(lambda s: len(s["panels"]) == 2, what="two panels")
    check("panels stack", s["panels"][1]["dir"].endswith("beta"), s["panels"])
    # moved by its header
    p2 = s["panels"][1]
    q = x.mouse("down", "PanelHead(%d)" % p2["id"])
    hx, hy = q["x"], q["y"]
    x.cmd("mouse", type="move", x=hx + 120, y=hy + 40)
    x.cmd("mouse", type="up", x=hx + 120, y=hy + 40)
    r2 = x.state()["panels"][1]["rect"]
    check("a panel is moved by its header", abs(r2[0] - p2["rect"][0] - 120) < 2 and abs(r2[1] - p2["rect"][1] - 40) < 2, (p2["rect"], r2))
    x.shot("sidebar-two-panels")
    x.key(VK["ESC"])
    s = x.state()
    check("Esc closes the panel in front", len(s["panels"]) == 1 and s["panels"][0]["dir"].endswith("Alpha"), s["panels"])
    x.click("PanelClose(%d)" % pid)
    check("the close button closes it", x.state()["panels"] == [])
    # a click outside closes it (but not a click on a file of the list)
    right(row_of("Alpha"), ctrl=True)
    x.wait(lambda s: len(s["panels"]) == 1, what="panel")
    x.click(row_of("notes.md"))
    check("a click on a file keeps the panel", len(x.state()["panels"]) == 1)
    x.click("List")
    check("a click in the empty list closes it", x.state()["panels"] == [])
    # from the sidebar too
    s = x.state()
    right("Nav(%d)" % nav_index(s, kind="CatFolder(0, 0)"), ctrl=True)
    s = x.wait(lambda s: len(s["panels"]) == 1, what="panel from the sidebar")
    check("Ctrl+right-click in the sidebar opens a panel", s["panels"][0]["dir"].endswith("Gamma"), s["panels"])
    x.key(VK["ESC"])

    # ───────── sorting by date from the View menu, search filters
    x.cmd("menu", choose="Sort by > Date created"); x.click("ViewMenu"); time.sleep(0.15)
    check("View menu: sort by date created", x.state()["sort"] == "Created", x.state()["sort"])
    x.cmd("menu", choose="Sort by > Date modified"); x.click("ViewMenu"); time.sleep(0.15)
    check("View menu: sort by date modified", x.state()["sort"] == "Modified", x.state()["sort"])
    x.wait(lambda s: s["index_ready"], timeout=20, what="index")
    x.click("Search"); x.edit(".")
    s = x.wait(lambda s: s["load"] == "ready" and not s["stale"] and s["loc"].startswith("search:"), what="results")
    total = s["rows"]

    def filt(label):
        x.cmd("menu", choose=label)
        x.click("Filter")
        time.sleep(0.2)
        return x.wait(lambda s: s["load"] == "ready" and not s["stale"], what=label)

    s = filt("Date modified > Today")
    check("date filter: today keeps everything made today", s["rows"] == total and total > 0, (s["rows"], total))
    s = filt("Date modified > Yesterday")
    check("date filter: yesterday finds nothing here", s["rows"] == 0, s["visible"])
    s = filt("Date modified > Any time")
    s = filt("Size > 1 MB to 100 MB")
    check("size filter: 1 MB to 100 MB", s["visible"] == ["Zed.bin"], s["visible"])
    s = filt("Size > Smaller than 1 MB")
    check("size filter: smaller than 1 MB keeps only files", "Zed.bin" not in s["visible"] and "Alpha" not in s["visible"] and s["rows"] > 0, s["visible"])
    x.cmd("menu", choose="Sort by > Size"); x.click("ViewMenu"); time.sleep(0.15)
    s = x.wait(lambda s: s["load"] == "ready", what="sorted")
    check("search results sorted by size from the View menu", s["search_sort"] == "Size", s["search_sort"])
    x.shot("sidebar-search-filters")
    s = filt("Clear filters")
    check("clear filters", s["rows"] == total, (s["rows"], total))
finally:
    x.close()
check.done()
