import os, sys, time, zipfile
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

d = fixture("fixture-passwords")
x = Fex(d, port=47832)
check = Checks()


def rclick(name, choose=None):
    row = x.names().index(name)
    x.cmd("menu", choose=choose)
    x.mouse("click", "Row(%d)" % row, button="right")
    return x.cmd("menu_last")


def press(target, timeout=3.0):
    """Click a button of the password box (it exists once the box has been painted)."""
    end = time.time() + timeout
    while True:
        try:
            return x.click(target)
        except RuntimeError:
            if time.time() > end:
                raise
            time.sleep(0.05)


def sheet(pred, what, timeout=15):
    return x.wait(lambda s: s["busy"] is None and pred(s["sheet"]), timeout=timeout, what=what)


try:
    x.wait()
    names = x.names()
    x.click("Row(%d)" % names.index("a1.txt")); x.click("Row(%d)" % names.index("Alpha"), ctrl=True)
    labels = rclick("a1.txt")
    check("the menu offers to compress with a password", "Compress with a password…" in labels[:4], labels[:5])
    # the password box
    rclick("a1.txt", "Compress with a password…")
    s = sheet(lambda sh: sh is not None, "password box")
    has_rar = s["sheet"]["kind"].endswith("true")
    check("a password box opens, ZIP by default", s["sheet"]["kind"].startswith("lock:Zip") and s["edit"] == "Password", s["sheet"])
    press("SheetOk")
    s = x.state(); check("an empty password is refused", s["sheet"] and s["sheet"]["error"] == "Enter a password.", s["sheet"])
    press("SheetReveal"); s = x.state(); check("Show reveals the password", s["sheet"]["reveal"] is True, s["sheet"])
    x.edit("hunter2 é")
    x.shot("archive-password")
    press("SheetOk")
    out = os.path.basename(d) + ".zip"
    s = x.wait(lambda s: out in s["visible"] and s["busy"] is None and s["sheet"] is None, timeout=20, what="protected zip")
    raw = open(os.path.join(d, out), "rb").read()
    with zipfile.ZipFile(os.path.join(d, out)) as z:
        listed = sorted(z.namelist())
        try:
            z.read("a1.txt")
            readable = True
        except Exception:
            readable = False
    check("the ZIP is created, encrypted with AES", "a1.txt" in listed and not readable and b"x" * 10 not in raw and s["sel"] == [out], (listed, readable))
    # extracting asks for the password
    rclick(out, "Extract to “%s\\”" % os.path.basename(d))
    s = sheet(lambda sh: sh is not None and sh["kind"] == "unlock", "password asked for extraction")
    check("extracting a protected ZIP asks for its password", s["sheet"]["error"] is None, s["sheet"])
    x.edit("not it"); press("SheetOk")
    s = sheet(lambda sh: sh is not None and sh["error"], "wrong password message")
    check("a wrong password is reported and nothing is extracted", s["sheet"]["error"].startswith("Wrong password") and not os.path.exists(os.path.join(d, os.path.basename(d))), s["sheet"])
    x.edit("hunter2 é"); press("SheetOk")
    folder = os.path.basename(d)
    s = x.wait(lambda s: folder in s["visible"] and s["busy"] is None and s["sheet"] is None, timeout=20, what="extracted with the password")
    check("the right password extracts everything", open(os.path.join(d, folder, "a1.txt"), "rb").read() == b"x" * 10 and os.path.isfile(os.path.join(d, folder, "Alpha", "inside.txt")), os.listdir(os.path.join(d, folder)))
    # cancel
    rclick(out, "Extract here")
    sheet(lambda sh: sh is not None, "password box again")
    x.editkey(VK["ESC"])
    s = x.state(); check("Esc closes the box and extracts nothing", s["sheet"] is None and s["edit"] is None and not any(n.startswith(".plume-extract") for n in os.listdir(d)), os.listdir(d))
    # RAR, when WinRAR is installed on this PC
    if has_rar:
        names = x.names()
        x.click("Row(%d)" % names.index("notes.md"))
        rclick("notes.md", "Compress with a password…")
        sheet(lambda sh: sh is not None, "password box for RAR")
        press("SheetRar")
        s = x.state(); check("RAR can be chosen when WinRAR is installed", s["sheet"]["kind"].startswith("lock:Rar"), s["sheet"])
        x.edit("rar pass"); press("SheetOk")
        s = x.wait(lambda s: "notes.rar" in s["visible"] and s["busy"] is None, timeout=30, what="protected rar")
        raw = open(os.path.join(d, "notes.rar"), "rb").read()
        check("a protected RAR is created, names hidden", raw[:4] == b"Rar!" and b"notes.md" not in raw, raw[:8])
        rclick("notes.rar", "Extract to “notes\\”")
        sheet(lambda sh: sh is not None and sh["kind"] == "unlock", "password asked for the RAR")
        x.edit("nope"); press("SheetOk")
        s = sheet(lambda sh: sh is not None and sh["error"], "wrong RAR password", timeout=30)
        x.edit("rar pass"); press("SheetOk")
        s = x.wait(lambda s: "notes" in s["visible"] and s["busy"] is None and s["sheet"] is None, timeout=30, what="rar extracted")
        check("the protected RAR extracts with its password", os.path.getsize(os.path.join(d, "notes", "notes.md")) == 1234)
    else:
        print("     (WinRAR is not installed: RAR checks skipped)")
    x.shot("archive-password-done")
finally:
    x.close()
check.done()
