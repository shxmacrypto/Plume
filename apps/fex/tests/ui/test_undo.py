import os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

# Undo (Ctrl+Z): rename, new folder, copy then paste, cut then paste, delete to the Recycle Bin.
# The delete goes to the real Recycle Bin and comes back; at the end, anything this test left there is removed.

d = fixture("fixture-undo")
x = Fex(d, port=47825)
check = Checks()


def row(name):
    return x.names().index(name)


def undo(what):
    x.key(L("Z"), ctrl=True)
    return x.wait(lambda s: s["toast"] in ("Undone.", "Nothing to undo.") or (s["toast"] or "").startswith("That could not"), what="undo of " + what)


def clean_recycle_bin():
    """Remove from the Recycle Bin what this test sent there (and only that)."""
    root = os.path.join(os.path.splitdrive(d)[0] + "\\", "$Recycle.Bin")
    for sid in os.listdir(root) if os.path.isdir(root) else []:
        bin_ = os.path.join(root, sid)
        try:
            names = os.listdir(bin_)
        except OSError:
            continue
        for n in names:
            if not n.startswith("$I"):
                continue
            raw = open(os.path.join(bin_, n), "rb").read()
            if len(raw) < 28:
                continue
            length = int.from_bytes(raw[24:28], "little")
            orig = raw[28:28 + length * 2].decode("utf-16-le", "replace").rstrip("\x00")
            if orig.lower().startswith(d.lower()):
                data = os.path.join(bin_, "$R" + n[2:])
                if os.path.isdir(data):
                    shutil.rmtree(data, ignore_errors=True)
                elif os.path.exists(data):
                    os.remove(data)
                os.remove(os.path.join(bin_, n))


try:
    s = x.wait()
    s = undo("nothing")
    check("with nothing done yet, Ctrl+Z says so", s["toast"] == "Nothing to undo.", s["toast"])
    # rename
    x.click("Row(%d)" % row("a1.txt")); x.key(VK["F2"]); x.edit("one.txt"); x.editkey(VK["ENTER"])
    x.wait(lambda s: "one.txt" in s["visible"] and s["edit"] is None, what="renamed")
    s = undo("rename")
    s = x.wait(lambda s: "a1.txt" in s["visible"], what="name back")
    check("Ctrl+Z gives a renamed file its name back", os.path.exists(os.path.join(d, "a1.txt")) and not os.path.exists(os.path.join(d, "one.txt")) and s["toast"] == "Undone.", s["toast"])
    # new folder (named "New folder", left as is)
    x.key(L("N"), ctrl=True, shift=True)
    x.wait(lambda s: s["edit"] is not None, what="new folder rename box"); x.editkey(VK["ESC"])
    x.wait(lambda s: "New folder" in s["visible"], what="new folder")
    undo("new folder")
    s = x.wait(lambda s: "New folder" not in s["visible"], what="new folder gone")
    check("Ctrl+Z removes the folder just created", not os.path.exists(os.path.join(d, "New folder")))
    # copy then paste into Alpha, then undo: the copy goes, the original stays
    x.click("Row(%d)" % row("a2.txt")); x.key(L("C"), ctrl=True); time.sleep(0.5)
    x.dbl("Row(%d)" % row("Alpha")); x.wait(lambda s: s["loc"].endswith("Alpha") and not s["stale"], what="in Alpha")
    x.key(L("V"), ctrl=True)
    x.wait(lambda s: "a2.txt" in s["visible"], timeout=8, what="pasted")
    undo("paste")
    s = x.wait(lambda s: "a2.txt" not in s["visible"], timeout=8, what="copy gone")
    check("Ctrl+Z after a paste removes the copy, not the original", not os.path.exists(os.path.join(d, "Alpha", "a2.txt")) and os.path.exists(os.path.join(d, "a2.txt")))
    x.key(VK["BACK"]); x.wait(lambda s: s["loc"] == d and not s["stale"], what="back")
    # cut then paste into beta, then undo: it goes back where it was
    x.click("Row(%d)" % row("notes.md")); x.key(L("X"), ctrl=True); time.sleep(0.5)
    x.dbl("Row(%d)" % row("beta")); x.wait(lambda s: s["loc"].endswith("beta") and not s["stale"], what="in beta")
    x.key(L("V"), ctrl=True)
    x.wait(lambda s: "notes.md" in s["visible"], timeout=8, what="moved")
    check("cut then paste moves the file", not os.path.exists(os.path.join(d, "notes.md")))
    undo("move")
    s = x.wait(lambda s: "notes.md" not in s["visible"], timeout=8, what="moved back")
    check("Ctrl+Z after a move puts it back", os.path.exists(os.path.join(d, "notes.md")) and not os.path.exists(os.path.join(d, "beta", "notes.md")))
    x.key(VK["BACK"]); x.wait(lambda s: s["loc"] == d and not s["stale"], what="back")
    # delete to the Recycle Bin, then undo
    x.click("Row(%d)" % row("a10.txt")); x.key(VK["DELETE"])
    x.wait(lambda s: "a10.txt" not in s["visible"], timeout=10, what="deleted")
    check("Delete sends the file away", not os.path.exists(os.path.join(d, "a10.txt")))
    s = undo("delete")
    s = x.wait(lambda s: "a10.txt" in s["visible"], timeout=10, what="restored")
    check("Ctrl+Z brings it back from the Recycle Bin, contents intact", open(os.path.join(d, "a10.txt"), "rb").read() == b"x" * 300 and s["toast"] == "Undone.", s["toast"])
    x.shot("undo-done")
finally:
    x.close()
    clean_recycle_bin()
check.done()
