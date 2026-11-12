import os, subprocess, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

d = fixture("fixture3")
x = Fex(d, port=47813)
check = Checks()


def rclick(target, choose=None, **kw):
    x.cmd("menu", choose=choose)
    x.mouse("click", target, button="right", **kw)
    return x.cmd("menu_last")


def clipboard():
    r = subprocess.run(["powershell", "-NoProfile", "-Command", "Get-Clipboard -Raw"], capture_output=True, text=True, stdin=subprocess.DEVNULL)
    return r.stdout.strip()


try:
    s = x.wait()
    # the Explorer context menu, with our entries first
    labels = rclick("Row(4)")
    s = x.state()
    check("right-click selects the file", s["sel"] == ["a1.txt"], s["sel"])
    check("our entries first, then the Explorer menu, with Copy as path only once", labels[0] == "Compress to ZIP file" and labels.index("Compress to ZIP file") < labels.index("Open") and labels.count("Copy as path") == 1 and len(labels) > 6, labels[:12])
    low = [l.lower() for l in labels]
    check("Explorer menu has the usual commands", all(any(w in l for l in low) for w in ["open", "copy", "delete", "rename", "properties"]), labels)
    print("     menu:", " | ".join(labels[:22]))
    labels = rclick("Row(0)")
    check("folder menu offers a new tab and pinning", labels[:3] == ["Open in new tab", "Open in a mini panel", "Pin to sidebar"] and labels.index("Open") > 2, labels[:6])
    # copy as path
    rclick("Row(4)", "Copy as path")
    s = x.state()
    check("copy as path", clipboard() == '"%s"' % os.path.join(d, "a1.txt"), clipboard())
    # rename from the menu
    rclick("Row(4)", "Rename")
    s = x.state()
    check("Rename opens the field on the name", s["edit"] and s["edit"].startswith("Rename") and s["edit_text"] == "a1.txt", (s["edit"], s["edit_text"]))
    x.shot("files-rename")
    x.edit("renamed one.txt"); x.editkey(VK["ENTER"])
    s = x.wait(lambda s: "renamed one.txt" in s["visible"] and s["edit"] is None, what="renamed")
    check("renamed on disk and reselected", os.path.exists(os.path.join(d, "renamed one.txt")) and not os.path.exists(os.path.join(d, "a1.txt")) and s["sel"] == ["renamed one.txt"], s["sel"])
    # F2 + invalid name, then Esc
    x.key(VK["F2"]); x.edit("bad:name?.txt"); x.editkey(VK["ENTER"])
    s = x.state()
    check("invalid name is refused with a message", s["toast"] and "characters" in s["toast"] and os.path.exists(os.path.join(d, "renamed one.txt")), s["toast"])
    x.key(VK["F2"]); x.edit("never.txt"); x.editkey(VK["ESC"])
    time.sleep(0.3)
    check("Esc cancels a rename", os.path.exists(os.path.join(d, "renamed one.txt")) and not os.path.exists(os.path.join(d, "never.txt")))
    # new folder
    x.key(L("N"), ctrl=True, shift=True)
    s = x.wait(lambda s: s["edit"] is not None and "New folder" in s["visible"], what="new folder being named")
    check("Ctrl+Shift+N creates a folder and opens its name", s["edit_text"] == "New folder" and os.path.isdir(os.path.join(d, "New folder")), s["edit_text"])
    x.edit("Projects"); x.editkey(VK["ENTER"])
    s = x.wait(lambda s: "Projects" in s["visible"] and s["edit"] is None, what="folder renamed")
    check("new folder named", os.path.isdir(os.path.join(d, "Projects")) and s["sel"] == ["Projects"], s["sel"])
    # background menu
    labels = rclick("List")
    check("background menu", labels[:2] == ["Refresh", "Sort by > Name [x]"] and "New folder" in labels and "Paste" in labels, labels)
    rclick("List", "Sort by > Size"); s = x.state()
    check("sort from the menu", s["sort"] == "Size", s["sort"])
    rclick("List", "Sort by > Name")
    rclick("List", "Show hidden items"); s = x.wait(lambda s: ".secret" not in s["visible"], what="hidden files hidden")
    check("hidden items can be hidden", not s["show_hidden"] and s["rows"] == 11, s["rows"])
    rclick("List", "Show hidden items"); s = x.wait(lambda s: ".secret" in s["visible"], what="hidden files shown")
    # copy and paste a file into a folder
    row = x.names().index("a2.txt")
    x.click("Row(%d)" % row); x.key(L("C"), ctrl=True)
    time.sleep(0.5)
    x.dbl("Row(0)"); s = x.wait(lambda s: s["loc"].endswith("Alpha") and not s["stale"], what="in Alpha")
    x.key(L("V"), ctrl=True)
    s = x.wait(lambda s: "a2.txt" in s["visible"], timeout=8, what="pasted file")
    check("copy then paste", os.path.exists(os.path.join(d, "Alpha", "a2.txt")) and os.path.exists(os.path.join(d, "a2.txt")), s["visible"])
    x.key(VK["BACK"]); x.wait()
    # the toolbar menu: theme and language
    x.cmd("menu", choose="Theme > Light"); x.click("More"); s = x.state()
    check("light theme", s["dark"] is False, s["dark"])
    x.shot("files-light")
    x.cmd("menu", choose="Language > Français"); x.click("More"); s = x.state()
    check("French", s["lang"] == "fr" and "Ce PC" in s["nav"], s["nav"])
    x.shot("files-fr")
    x.cmd("menu", choose="Thème > Sombre"); x.click("More")
    x.cmd("menu", choose="Langue > English"); x.click("More"); s = x.state()
    check("back to dark and English", s["dark"] is True and s["lang"] == "en", (s["dark"], s["lang"]))
    labels = x.cmd("menu_last")
    # tab menu
    rclick("Tab(0)", "Duplicate tab"); s = x.wait()
    check("duplicate tab", len(s["tabs"]) == 2 and s["tabs"][0]["loc"] == s["tabs"][1]["loc"], s["tabs"])
    rclick("Tab(1)", "Close other tabs"); s = x.state()
    check("close other tabs", len(s["tabs"]) == 1, s["tabs"])
    # pin a folder to the sidebar
    rclick("Row(0)", "Pin to sidebar")
    # (Alpha is already in the recent folders, further down: wait for it at the top)
    s = x.wait(lambda s: "Alpha" in s["nav"] and s["nav"].index("Alpha") < s["nav"].index("This PC"), timeout=5, what="pinned")
    check("pinned folder appears in the sidebar", s["nav"].index("Alpha") < s["nav"].index("This PC"), s["nav"])
    x.shot("files-pinned")
finally:
    x.close()
check.done()
