import json, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

# SFTP servers inside Plume FEx (beta). The window talks to a real SFTP server program run on this PC (the one
# shipped with Git for Windows), through Windows' own sftp.exe: no network, no password. What this cannot test:
# the password prompt of a real server.
GIT = r"C:\Program Files\Git"
SERVER = r"C:\PROGRA~1\Git\usr\lib\ssh\sftp-server.exe"
CLIENT = os.path.join(os.environ.get("SystemRoot", r"C:\Windows"), "System32", "OpenSSH", "sftp.exe")
check = Checks()
if not (os.path.isfile(SERVER) and os.path.isfile(CLIENT)):
    print("SKIP: needs Windows' sftp.exe and the sftp-server.exe of Git for Windows")
    check.done()
    sys.exit(0)

d = fixture("fixture14")
with open(os.path.join(d, "Alpha", "Inner", "deep.bin"), "wb") as f:
    f.write(b"x" * 1000)
BIG = 400 * 1024 * 1024  # assez gros pour voir l'avancement d'un téléchargement
with open(os.path.join(d, "Gamma", "big.bin"), "wb") as f:
    f.truncate(BIG)
local = os.path.join(OUT, "sftp-local")
shutil.rmtree(local, ignore_errors=True)
os.makedirs(os.path.join(local, "Folder up", "sub"))
os.makedirs(os.path.join(local, "downloads"))
for rel, data in [(("up load é.txt",), b"sent"), (("Folder up", "one.txt"), b"1"), (("Folder up", "sub", "two [2].txt"), b"22")]:
    with open(os.path.join(local, *rel), "wb") as f:
        f.write(data)


def seen(path):
    """A path of this PC as the test server shows it: /c/Users/..."""
    p = os.path.abspath(path).replace("\\", "/")
    return "/" + p[0].lower() + p[2:]


R = seen(d)
URL = "sftp://test@local" + R
env = {
    "PLUME_FEX_SFTP_DIRECT": SERVER,
    "PLUME_TEST_FOLDER": os.path.join(local, "downloads"),
    "PLUME_TEST_FILES": os.pathsep.join([os.path.join(local, "up load é.txt"), os.path.join(local, "Folder up")]),
    "PATH": os.path.join(GIT, "usr", "bin") + os.pathsep + os.environ.get("PATH", ""),
}
x = Fex(local, port=47824, env=env)


def ready(pred=lambda s: True, what="ready"):
    return x.wait(lambda s: s["load"] == "ready" and not s["stale"] and not s["busy"] and pred(s), timeout=20, what=what)


def menu(target, choose, **kw):
    x.cmd("menu", choose=choose)
    x.mouse("down", target, button="right", **kw); x.mouse("up", target, button="right", **kw)
    return x.cmd("menu_last")


def row(name):
    return "Row(%d)" % x.state()["visible"].index(name)


try:
    x.wait()
    # the dialog: an address, checked before anything is started
    x.cmd("menu", choose="Connect to a server (SFTP, beta)…"); x.click("More")
    s = x.wait(lambda s: s["sheet"], what="connect dialog")
    check("the menu opens the connect dialog", s["sheet"]["kind"] == "connect" and s["edit"] == "Password", s["sheet"])
    x.shot("sftp-connect")
    x.edit("not an address"); x.click("SheetOk")
    s = x.state()
    check("an address that is not user@host is refused, the dialog stays", s["sheet"] and "user@host" in (s["sheet"]["error"] or ""), s["sheet"])
    x.edit("test@-oProxyCommand=calc"); x.click("SheetOk")
    check("nothing that could pass for an option reaches sftp.exe", x.state()["sheet"] is not None)
    x.edit("test@local"); x.click("SheetOk")
    s = ready(lambda s: s["loc"].startswith("sftp://test@local/"), "connected")
    check("connected: the tab shows the folder the server gives", s["sheet"] is None and s["rows"] > 0 and s["loc"] != "sftp://test@local/", s["loc"])
    check("the server is kept in the side bar", "test@local" in s["nav"], s["nav"])
    with open(os.path.join(OUT, "prof-test", "settings.json"), encoding="utf-8") as f:
        saved = json.load(f)
    check("only the address is saved, never a password", saved.get("servers") == ["test@local"] and "password" not in json.dumps(saved).lower(), saved.get("servers"))
    # browsing
    x.key(ord("L"), ctrl=True); x.edit(URL); x.editkey(VK["ENTER"])
    s = ready(lambda s: s["loc"] == URL, "the fixture on the server")
    # a name starting with a dot is a hidden file there: shown only when Windows is set to show hidden files
    dot = [".secret"] if s["show_hidden"] else []
    check("the address bar takes sftp:// addresses", s["visible"] == ["Alpha", "beta", "Gamma"] + dot + ["a1.txt", "a2.txt", "a10.txt", "data.json", "notes.md", "photo.png", "Zed.bin"], s["visible"])
    z = dict(zip(s["visible"], s["sizes"]))
    check("sizes come from the server", z["a2.txt"] == 2000 and z["Zed.bin"] == 1572864 and z["Alpha"] is None, z)
    x.shot("sftp-browse")
    x.dbl(row("Alpha"))
    s = ready(lambda s: s["loc"] == URL + "/Alpha", "inside Alpha")
    check("a double click enters a folder", s["visible"] == ["Inner", "inside.txt"], s["visible"])
    x.click("Up")
    s = ready(lambda s: s["loc"] == URL, "back up")
    check("Up returns to the parent, on the folder we left", s["focus_name"] == "Alpha", s["focus_name"])
    x.click("Back")
    s = ready(lambda s: s["loc"] == URL + "/Alpha", "history")
    x.click("Back"); s = ready(lambda s: s["loc"] == URL, "history back")
    x.click("Header(Size)"); s = x.wait(lambda s: s["sort"] == "Size")
    check("columns sort the server folder too", s["visible"][:3] == ["Zed.bin", "photo.png", "a2.txt"], s["visible"][:5])
    x.click("Header(Name)"); x.wait(lambda s: s["sort"] == "Name")
    # new folder, rename
    x.key(ord("N"), ctrl=True, shift=True)
    s = x.wait(lambda s: s["edit"] and s["edit"].startswith("Rename") and not s["busy"], timeout=20, what="new folder being renamed")
    check("Ctrl+Shift+N creates a folder on the server and opens its name", os.path.isdir(os.path.join(d, "New folder")) and s["focus_name"] == "New folder", s["focus_name"])
    x.edit("Créé [1]"); x.editkey(VK["ENTER"])
    s = ready(lambda s: "Créé [1]" in s["visible"], "renamed")
    check("the new name reaches the server, accents and brackets included", os.path.isdir(os.path.join(d, "Créé [1]")) and not os.path.exists(os.path.join(d, "New folder")), s["visible"])
    x.click(row("a1.txt")); x.key(VK["F2"])
    x.wait(lambda s: s["edit"] and s["edit"].startswith("Rename"))
    x.edit("renamed.txt"); x.editkey(VK["ENTER"])
    s = ready(lambda s: "renamed.txt" in s["visible"], "file renamed")
    check("F2 renames a file", os.path.isfile(os.path.join(d, "renamed.txt")) and not os.path.exists(os.path.join(d, "a1.txt")) and s["focus_name"] == "renamed.txt", s["focus_name"])
    # upload: a file and a whole folder
    labels = menu("List", "Upload files…")
    check("the menu of the folder: paste, upload, new folder, refresh", labels[:5] == ["Paste", "Upload files…", "New folder", "Refresh", "Copy address"], labels)
    s = ready(lambda s: "up load é.txt" in s["visible"] and "Folder up" in s["visible"], "uploaded")
    with open(os.path.join(d, "up load é.txt"), "rb") as f:
        sent = f.read()
    check("a file and a folder are uploaded with their content", sent == b"sent" and os.path.isfile(os.path.join(d, "Folder up", "sub", "two [2].txt")), sent)
    check("a message says what was sent", "2 items" in (s["toast"] or ""), s["toast"])
    # download: one file, then a folder, into a chosen folder
    x.click(row("notes.md"))
    labels = menu(row("notes.md"), "Download to…")
    check("the menu of an item: open, download, rename, delete", [l for l in labels if l] == ["Open", "Download", "Download to…", "Rename", "Delete", "Copy address"], labels)
    got = os.path.join(local, "downloads", "notes.md")
    s = ready(lambda s: os.path.isfile(got) and "downloads" in (s["toast"] or ""), "downloaded")
    check("a file is downloaded where asked", os.path.getsize(got) == 1234, s["toast"])
    x.click(row("Alpha")); menu(row("Alpha"), "Download to…")
    deep = os.path.join(local, "downloads", "Alpha", "Inner", "deep.bin")
    ready(lambda s: os.path.isfile(deep), "folder downloaded")
    check("a folder is downloaded with everything inside", os.path.getsize(deep) == 1000 and os.path.isfile(os.path.join(local, "downloads", "Alpha", "inside.txt")))
    # a big download shows how far it is
    x.dbl(row("Gamma")); ready(lambda s: s["loc"] == URL + "/Gamma", "in Gamma")
    menu(row("big.bin"), "Download to…")
    seen_progress = None
    end = time.time() + 120
    while time.time() < end:
        st = x.state()
        tr = st.get("transfer")
        if tr and tr["total"] == BIG and 0 < tr["done"] < BIG:
            seen_progress = tr
            x.shot("sftp-progress")
            break
        if os.path.isfile(os.path.join(local, "downloads", "big.bin")) and os.path.getsize(os.path.join(local, "downloads", "big.bin")) == BIG and not st["busy"]:
            break
        time.sleep(0.1)
    check("a download shows its progress (bytes received out of the size)", seen_progress is not None, seen_progress)
    end = time.time() + 180
    while time.time() < end and (x.state()["busy"] or os.path.getsize(os.path.join(local, "downloads", "big.bin")) != BIG):
        time.sleep(0.3)
    st = x.state()
    check("once done, the progress goes away", st.get("transfer") is None and not st["busy"], st.get("transfer"))
    os.remove(os.path.join(local, "downloads", "big.bin"))
    x.click("Up"); ready(lambda s: s["loc"] == URL, "back up")
    # delete: asks first, because a server has no recycle bin
    x.click(row("a2.txt")); x.click(row("Folder up"), ctrl=True)
    x.key(VK["DELETE"])
    s = x.wait(lambda s: s["sheet"], what="delete confirmation")
    check("Delete asks before removing anything", s["sheet"]["kind"] == "delete:2" and os.path.exists(os.path.join(d, "a2.txt")), s["sheet"])
    x.shot("sftp-delete")
    x.key(VK["ESC"])
    s = x.wait(lambda s: not s["sheet"])
    check("Esc cancels: nothing is deleted", os.path.exists(os.path.join(d, "a2.txt")) and os.path.isdir(os.path.join(d, "Folder up")))
    x.key(VK["DELETE"]); x.wait(lambda s: s["sheet"]); x.click("SheetOk")
    s = ready(lambda s: "a2.txt" not in s["visible"] and "Folder up" not in s["visible"], "deleted")
    check("confirmed: the file and the folder (with its content) are gone from the server", not os.path.exists(os.path.join(d, "a2.txt")) and not os.path.exists(os.path.join(d, "Folder up")), s["visible"])
    # a folder that does not exist: a message, not a crash; the address of a folder can be copied back
    x.key(ord("L"), ctrl=True); x.edit(URL + "/nope"); x.editkey(VK["ENTER"])
    s = x.wait(lambda s: s["load"].startswith("failed"), timeout=20, what="missing folder")
    check("a missing folder says so", "not found" in s["load"].lower() or "no such" in s["load"].lower(), s["load"])
    x.click("Back"); s = ready(lambda s: s["loc"] == URL, "back from the error")
    # the side bar entry: disconnect, remove
    k = s["nav"].index("test@local")
    labels = menu("Nav(%d)" % k, "Remove from the list")
    check("the server in the side bar can be disconnected or removed", "Disconnect" in labels and "Remove from the list" in labels, labels)
    s = x.wait(lambda s: "test@local" not in s["nav"])
    with open(os.path.join(OUT, "prof-test", "settings.json"), encoding="utf-8") as f:
        check("removed from the list and from the settings", json.load(f).get("servers") == [], s["nav"])
finally:
    x.close()
check.done()
