import os, struct, sys, time, zlib
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

# The icon views (Ctrl+Shift+1 to 4, Ctrl+wheel, the View button; back with Ctrl+Shift+6): a grid, real thumbnails,
# keyboard and mouse.


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
    # Explorer's sizes: Ctrl+Shift+1 to 4
    sizes = {}
    for key, name in [("1", "extra large"), ("3", "medium"), ("4", "small"), ("2", "large")]:
        x.key(L(key), ctrl=True, shift=True)
        s = x.wait(lambda s, k=int(key) - 1: s["thumbs"] and s["icon_size"] == k, what=name + " icons")
        sizes[name] = (s["grid_cols"], s["tile"])
        if name in ("extra large", "medium", "small"):
            if name != "small":
                x.wait(lambda s: s["thumbs_ready"] >= 2, timeout=20, what="thumbnails at this size")
            x.shot("icons-" + name.replace(" ", "-"))
    check("Ctrl+Shift+1, 2, 3, 4: extra large, large, medium, small icons",
          sizes["extra large"][1][0] > sizes["large"][1][0] > sizes["medium"][1][0] and sizes["small"][1][1] < sizes["medium"][1][1]
          and sizes["extra large"][0] < sizes["large"][0] < sizes["medium"][0], sizes)
    # Ctrl + mouse wheel: up for bigger icons, down for smaller, then details
    x.mouse("wheel", "List", dy=1.0, ctrl=True)
    s = x.wait(lambda s: s["icon_size"] == 0, what="bigger with Ctrl+wheel")
    check("Ctrl + wheel up makes the icons bigger", s["thumbs"] and s["icon_size"] == 0, s["icon_size"])
    for _ in range(4):
        x.mouse("wheel", "List", dy=-1.0, ctrl=True)
    s = x.wait(lambda s: not s["thumbs"], what="details with Ctrl+wheel")
    check("Ctrl + wheel down goes to smaller icons, then details", not s["thumbs"], s["thumbs"])
    # the View button: Explorer's menu
    x.cmd("menu", choose="Medium icons")
    x.click("ViewMenu")
    labels = x.cmd("menu_last")
    s = x.wait(lambda s: s["thumbs"] and s["icon_size"] == 2, what="medium from the View button")
    check("the View button offers the sizes and Details, like Explorer",
          [l.replace(" [x]", "") for l in labels] == ["Extra large icons", "Large icons", "Medium icons", "Small icons", "Details"] and "Details [x]" in labels, labels)
    x.close()
    x = Fex(d, port=47827, fresh=False)
    s = x.wait()
    check("the size is remembered", s["thumbs"] and s["icon_size"] == 2, (s["thumbs"], s["icon_size"]))
    x.key(L("6"), ctrl=True, shift=True)
    s = x.wait(lambda s: not s["thumbs"], what="details again")
    check("Ctrl+Shift+6 goes back to details", s["grid_cols"] == 1)
finally:
    x.close()
check.done()
