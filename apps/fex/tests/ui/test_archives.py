import os, sys, zipfile
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

d = fixture("fixture-archives")
with zipfile.ZipFile(os.path.join(d, "Photos 2026.zip"), "w") as z:
    z.writestr("summer/beach.txt", "sand")
    z.writestr("readme.txt", "hello")
with open(os.path.join(d, "broken.zip"), "wb") as f:
    f.write(b"this is not an archive")
x = Fex(d, port=47830)
check = Checks()


def rclick(name, choose=None):
    row = x.names().index(name)
    x.cmd("menu", choose=choose)
    x.mouse("click", "Row(%d)" % row, button="right")
    return x.cmd("menu_last")


try:
    x.wait()
    labels = rclick("Photos 2026.zip")
    check("an archive offers to extract", labels[:3] == ["Extract here", "Extract to “Photos 2026\\”", "Compress to ZIP file"], labels[:4])
    labels = rclick("notes.md")
    check("a plain file offers to compress, not to extract", labels[0] == "Compress to ZIP file" and "Extract here" not in labels, labels[:3])
    # extract to a folder
    rclick("Photos 2026.zip", "Extract to “Photos 2026\\”")
    s = x.wait(lambda s: "Photos 2026" in s["visible"] and s["busy"] is None, timeout=15, what="extracted folder")
    check("extract to a folder named after the archive", open(os.path.join(d, "Photos 2026", "summer", "beach.txt")).read() == "sand" and s["sel"] == ["Photos 2026"], s["sel"])
    # again: a second folder, nothing overwritten
    rclick("Photos 2026.zip", "Extract to “Photos 2026\\”")
    s = x.wait(lambda s: "Photos 2026 (2)" in s["visible"] and s["busy"] is None, timeout=15, what="second folder")
    check("a second extraction gets its own folder", os.path.isfile(os.path.join(d, "Photos 2026 (2)", "readme.txt")))
    # extract here keeps what is already there
    with open(os.path.join(d, "readme.txt"), "w") as f:
        f.write("mine")
    rclick("Photos 2026.zip", "Extract here")
    s = x.wait(lambda s: "summer" in s["visible"] and s["busy"] is None, timeout=15, what="extracted here")
    check("extract here does not overwrite existing files", open(os.path.join(d, "readme.txt")).read() == "mine" and os.path.isfile(os.path.join(d, "summer", "beach.txt")))
    # a file that is not an archive
    rclick("broken.zip", "Extract to “broken\\”")
    s = x.wait(lambda s: s["toast"] is not None and s["busy"] is None, timeout=15, what="error message")
    check("a broken archive gives a message and leaves no empty folder", not os.path.exists(os.path.join(d, "broken")), s["toast"])
    # compress a selection
    names = x.names()
    x.click("Row(%d)" % names.index("a1.txt")); x.click("Row(%d)" % names.index("Alpha"), ctrl=True)
    x.cmd("menu", choose="Compress to ZIP file")
    x.mouse("click", "Row(%d)" % names.index("a1.txt"), button="right")
    out = os.path.basename(d) + ".zip"
    s = x.wait(lambda s: out in s["visible"] and s["busy"] is None, timeout=15, what="zip created")
    with zipfile.ZipFile(os.path.join(d, out)) as z:
        inside = sorted(z.namelist())
    check("compress a selection to a ZIP", "a1.txt" in inside and any(n.startswith("Alpha/") for n in inside) and s["sel"] == [out], inside)
    x.shot("archives")
finally:
    x.close()
check.done()
