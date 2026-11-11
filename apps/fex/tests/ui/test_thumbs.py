import os, struct, sys, time, zlib
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

# The thumbnails view (Ctrl+Shift+2, back with Ctrl+Shift+6): a grid, real thumbnails, keyboard and mouse.


def png(path, w, h, rgb):
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    chunk = lambda kind, data: struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


d = fixture("fixture-thumbs")
png(os.path.join(d, "red.png"), 320, 200, (230, 40, 40))
png(os.path.join(d, "blue.png"), 200, 320, (40, 90, 230))
x = Fex(d, port=47827)
check = Checks()
try:
    s = x.wait()
    check("details view at first", not s["thumbs"])
    x.key(L("2"), ctrl=True, shift=True)
    s = x.wait(lambda s: s["thumbs"], what="thumbnails view")
    check("Ctrl+Shift+2 shows a grid of several columns", s["grid_cols"] >= 3, s["grid_cols"])
    s = x.wait(lambda s: s["thumbs_ready"] >= 2, timeout=20, what="thumbnails")
    check("the two pictures get real thumbnails", s["thumbs_ready"] >= 2, s["thumbs_ready"])
    x.shot("thumbs-grid")
    names = x.names()
    x.click("Row(%d)" % names.index("red.png"))
    s = x.state()
    check("a click on a tile selects it", s["sel"] == ["red.png"], s["sel"])
    x.key(VK["RIGHT"])
    s = x.state()
    check("Right goes to the next tile", s["sel"] == [names[names.index("red.png") + 1]], s["sel"])
    x.key(VK["HOME"]); x.key(VK["DOWN"])
    s = x.state()
    check("Down goes one line of the grid down", s["sel"] == [names[s["grid_cols"]]], (s["sel"], s["grid_cols"]))
    x.key(L("6"), ctrl=True, shift=True)
    s = x.wait(lambda s: not s["thumbs"], what="details again")
    check("Ctrl+Shift+6 goes back to details", s["grid_cols"] == 1)
finally:
    x.close()
check.done()
