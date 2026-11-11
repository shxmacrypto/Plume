import ctypes, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

# Folder sizes from the index, column widths, results sorted by folder, and the maximize button as Windows sees it.
d = fixture("fixture13")
with open(os.path.join(d, "Alpha", "Inner", "deep.bin"), "wb") as f:
    f.write(b"x" * 1000)
x = Fex(d, port=47823, index=d)
check = Checks()
user32 = ctypes.windll.user32
# This script asks Windows where things are on the screen. It must count in real pixels, as Plume FEx does,
# whatever the display scaling: otherwise the answer depends on where the window happens to open.
user32.SetThreadDpiAwarenessContext.restype = ctypes.c_void_p
user32.SetThreadDpiAwarenessContext.argtypes = [ctypes.c_void_p]
user32.SetThreadDpiAwarenessContext(ctypes.c_void_p(-4))  # per monitor, version 2


def more(label):
    x.cmd("menu", choose=label)
    x.click("More")
    return x.cmd("menu_last")


def sizes(s):
    return dict(zip(s["visible"], s["sizes"]))


def rect_of(id):
    return next(h["rect"] for h in reversed(x.cmd("hits")) if h["id"] == id)


def col(s, name):
    return next(c for c in s["cols"] if c[0] == name)


try:
    x.wait()
    s = x.wait(lambda s: s["index_ready"], timeout=15, what="index ready")
    # folder sizes
    z = sizes(s)
    check("folders have no size at first, files do", z["Alpha"] is None and z["beta"] is None and z["a2.txt"] == 2000, z)
    labels = more("Show folder sizes")
    check("the setting is in the menu, off", "Show folder sizes" in labels, labels)
    s = x.wait(lambda s: sizes(s)["Alpha"] is not None, what="folder sizes")
    z = sizes(s)
    check("each folder shows the total of everything inside it", z["Alpha"] == 1005 and z["beta"] == 0 and z["Gamma"] == 0 and s["folder_sizes"], z)
    x.shot("folder-sizes")
    x.click("Header(Size)")
    s = x.wait(lambda s: s["sort"] == "Size")
    check("sorted by size, folders and files rank together", s["visible"][:5] == ["Zed.bin", "photo.png", "a2.txt", "notes.md", "Alpha"], s["visible"][:6])
    with open(os.path.join(d, "beta", "new.bin"), "wb") as f:
        f.write(b"x" * 2000)
    s = x.wait(lambda s: sizes(s)["beta"] == 2000, timeout=12, what="size of beta after a new file")
    v = s["visible"]
    check("a new file updates the size of its folder, which moves up", v.index("photo.png") < v.index("beta") < v.index("notes.md") < v.index("Alpha"), v[:6])
    x.nav(os.path.join(d, "Alpha"))
    s = x.wait(lambda s: s["loc"].endswith("Alpha") and sizes(s).get("Inner") is not None, what="sizes in a subfolder")
    check("sizes follow into subfolders", sizes(s)["Inner"] == 1000, sizes(s))
    x.nav(d); x.wait(lambda s: s["loc"] == d and s["load"] == "ready")
    labels = more("Show folder sizes")
    check("the setting was ticked", "Show folder sizes [x]" in labels, labels)
    s = x.wait(lambda s: sizes(s)["Alpha"] is None, what="sizes hidden again")
    check("turning it off hides the sizes", not s["folder_sizes"] and sizes(s)["a2.txt"] == 2000, sizes(s))
    x.click("Header(Name)"); x.wait(lambda s: s["sort"] == "Name")
    # column widths
    s = x.state()
    date = col(s, "Date")
    r = rect_of("ColSplit(Date)")
    cx, cy = r[0] + r[2] / 2, r[1] + r[3] / 2
    x.mouse("move", x=cx, y=cy); x.mouse("down", x=cx, y=cy); x.mouse("move", x=cx - 60, y=cy); x.mouse("up", x=cx - 60, y=cy)
    s = x.state()
    check("dragging the edge of a column makes it wider", abs(col(s, "Date")[2] - (date[2] + 60)) < 1.5 and abs(col(s, "Name")[2] - (col(s, "Name")[2])) < 1, (date, col(s, "Date")))
    check("the name column gives the room", abs(col(s, "Date")[1] - (date[1] - 60)) < 1.5, (date, col(s, "Date")))
    x.dbl("ColSplit(Date)")
    s = x.state()
    check("a double click puts the width back", abs(col(s, "Date")[2] - 150) < 0.5, col(s, "Date"))
    # search results sorted by folder
    x.click("Search"); x.edit("txt")
    s = x.wait(lambda s: s["loc"] == "search:txt" and s["load"] == "ready" and not s["stale"])
    x.click("Header(Folder)")
    s = x.wait(lambda s: s["search_sort"] == "Path" and s["load"] == "ready")
    tree = [d, d, d, os.path.join(d, "Alpha")]
    check("results sorted by folder, in tree order", s["search_asc"] and s["folders"][:4] == tree and s["visible"][:3] == ["a1.txt", "a2.txt", "a10.txt"], (s["folders"], s["visible"]))
    r = rect_of("ColSplit(Folder)")
    cx, cy = r[0] + r[2] / 2, r[1] + r[3] / 2
    before = col(s, "Folder")[2]
    x.mouse("move", x=cx, y=cy); x.mouse("down", x=cx, y=cy); x.mouse("move", x=cx + 50, y=cy); x.mouse("up", x=cx + 50, y=cy)
    s = x.state()
    check("the folder column of the results can be narrowed", abs(col(s, "Folder")[2] - (before - 50)) < 1.5, (before, col(s, "Folder")))
    x.click("Header(Folder)")
    s = x.wait(lambda s: not s["search_asc"] and s["load"] == "ready")
    check("a second click reverses the order", s["folders"][0] != d, s["folders"][:3])
    # the maximize button, as Windows sees it (this is what brings the snap layouts of Windows 11)
    s = x.state()
    hwnd = s["hwnd"]
    r = rect_of("Max")
    scale = user32.GetDpiForWindow(hwnd) / 96.0
    px, py = int((r[0] + r[2] / 2) * scale), int((r[1] + r[3] / 2) * scale)

    class POINT(ctypes.Structure):
        _fields_ = [("x", ctypes.c_long), ("y", ctypes.c_long)]

    p = POINT(px, py)
    user32.ClientToScreen(hwnd, ctypes.byref(p))
    screen = (p.y & 0xFFFF) << 16 | (p.x & 0xFFFF)
    client = (py & 0xFFFF) << 16 | (px & 0xFFFF)
    user32.SendMessageW.restype = ctypes.c_ssize_t
    user32.SendMessageW.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_size_t, ctypes.c_ssize_t]
    user32.PostMessageW.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_size_t, ctypes.c_ssize_t]
    WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_LBUTTONUP, HTMAXBUTTON = 0x0084, 0x00A1, 0x0202, 9
    check("Windows sees a maximize button there", user32.SendMessageW(hwnd, WM_NCHITTEST, 0, screen) == HTMAXBUTTON, user32.SendMessageW(hwnd, WM_NCHITTEST, 0, screen))
    user32.PostMessageW(hwnd, WM_NCLBUTTONDOWN, HTMAXBUTTON, screen)
    user32.PostMessageW(hwnd, WM_LBUTTONUP, 0, client)
    s = x.wait(lambda s: s["maximized"], what="maximized")
    check("a click on it maximizes the window", s["maximized"])
    r = rect_of("Max")
    scale = user32.GetDpiForWindow(hwnd) / 96.0
    px, py = int((r[0] + r[2] / 2) * scale), int((r[1] + r[3] / 2) * scale)
    p = POINT(px, py)
    user32.ClientToScreen(hwnd, ctypes.byref(p))
    user32.PostMessageW(hwnd, WM_NCLBUTTONDOWN, HTMAXBUTTON, (p.y & 0xFFFF) << 16 | (p.x & 0xFFFF))
    user32.PostMessageW(hwnd, WM_LBUTTONUP, 0, (py & 0xFFFF) << 16 | (px & 0xFFFF))
    s = x.wait(lambda s: not s["maximized"], what="restored")
    check("a second click restores it", not s["maximized"])
finally:
    x.close()
check.done()
