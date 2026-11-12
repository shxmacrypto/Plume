# The other tools of Plume Create, through the test socket: brush, eraser, clone, fill, eyedropper, selection,
# groups, masks, blend modes, fonts, colour picker, rotation handle, keyboard. Checks the document's pixels.
#   python test_tools.py <plume-create.exe built with --features test-hook>
import os, shutil, sys, tempfile, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ctl import App
from PIL import Image, ImageDraw

EXE = sys.argv[1]
WORK = tempfile.mkdtemp(prefix="plume-create-tools-")
fails, passes = [], 0


def check(name, ok, info=""):
    global passes
    if ok:
        passes += 1
        print("  ok  ", name)
    else:
        fails.append(name)
        print("  FAIL", name, info)


def centre(a, hit_id):
    for h in a("hits"):
        if h["id"] == hit_id:
            r = h["rect"]
            return r[0] + r[2] / 2, r[1] + r[3] / 2
    raise KeyError(hit_id)


def win(a, x, y):
    p = a("doc_to_win", x=x, y=y)
    return p["x"], p["y"]


def px(a, x, y):
    return a("pixel", x=x, y=y)["rgba"]


# a grey 800 x 600 photo with a black square at (100..200, 100..200)
photo = os.path.join(WORK, "grey.png")
img = Image.new("RGB", (800, 600), (128, 128, 128))
ImageDraw.Draw(img).rectangle([100, 100, 199, 199], fill=(0, 0, 0))
img.save(photo)
a = App(EXE, port=47831, data=os.path.join(WORK, "data"), env={"PLUME_SUITE_THEME": "dark", "PLUME_SUITE_LANG": "en"})
try:
    time.sleep(1.0)
    a("open", path=photo)
    photo_id = a("state")["layers"][0]["id"]
    # brush: white line across the middle, on the photo itself
    a("slider", sl="BrushSize", value=30)
    a("slider", sl="BrushHard", value=100)
    a("click", id="Tool(Brush)")
    # white: through the colour picker's hex field
    a("click", id="Swatch(Brush)")
    a("click", id="Field(Hex)")
    a("type", text="#FFFFFF")
    st = a("state")
    check("colour picked by hex", st["colour"] == "#FFFFFF", st["colour"])
    a("key", vk=27)
    x1, y1 = win(a, 300, 400)
    x2, y2 = win(a, 600, 400)
    a("drag", x=x1, y=y1, x2=x2, y2=y2, steps=20)
    check("brush painted white", px(a, 450, 400)[:3] == [255, 255, 255], px(a, 450, 400))
    check("brush left the rest", px(a, 450, 300)[:3] == [128, 128, 128], px(a, 450, 300))
    check("painted on the photo layer", len(a("state")["layers"]) == 1)
    a.shot("20-brush")
    # eraser on the black square
    a("click", id="Tool(Eraser)")
    x, y = win(a, 150, 150)
    a("drag", x=x, y=y, x2=x + 1, y2=y, steps=2)
    check("eraser made it see-through", px(a, 150, 150)[3] == 0, px(a, 150, 150))
    a("key", vk=ord("Z"), ctrl=True)
    check("undo brings it back", px(a, 150, 150) == [0, 0, 0, 255], px(a, 150, 150))
    # clone: alt+click on the square, paint at (400, 150)
    a("click", id="Tool(Clone)")
    sx, sy = win(a, 150, 150)
    a("drag", x=sx, y=sy, x2=sx, y2=sy, steps=1, alt=True)
    tx, ty = win(a, 400, 150)
    a("drag", x=tx, y=ty, x2=tx + 1, y2=ty, steps=2)
    check("clone copied the black square", px(a, 400, 150)[:3] == [0, 0, 0], px(a, 400, 150))
    # fill: the grey around turns green, the square stays black
    a("click", id="Swatch(Brush)")
    a("click", id="Field(Hex)")
    a("type", text="#00AA00")
    a("key", vk=27)
    a("click", id="Tool(Fill)")
    fx, fy = win(a, 700, 550)
    a("drag", x=fx, y=fy, x2=fx, y2=fy, steps=1)
    check("fill made the grey green", px(a, 700, 550)[:3] == [0, 170, 0], px(a, 700, 550))
    check("fill stopped at the square", px(a, 150, 150)[:3] == [0, 0, 0], px(a, 150, 150))
    # eyedropper on the square
    a("click", id="Tool(Picker)")
    ex, ey = win(a, 150, 150)
    a("drag", x=ex, y=ey, x2=ex, y2=ey, steps=1)
    check("eyedropper took black", a("state")["colour"] == "#000000", a("state")["colour"])
    # selection: a rectangle, filled
    a("click", id="Tool(Select)")
    ax, ay = win(a, 500, 50)
    bx, by = win(a, 700, 90)
    a("drag", x=ax, y=ay, x2=bx, y2=by)
    st = a("state")
    check("rectangle selected", st["selection"] is not None and abs(st["selection"][0] - 500) < 3, st["selection"])
    a.shot("21-selection")
    a("click", id="Act(FillSel)")
    check("selection filled", px(a, 600, 70)[:3] == [0, 0, 0] and px(a, 600, 120)[:3] != [0, 0, 0], (px(a, 600, 70), px(a, 600, 120)))
    # selection to a mask, then the mask hides the rest
    a("click", id="Act(SelToMask)")
    st = a("state")
    check("mask from the selection", st["layers"][0]["mask"])
    check("masked outside", px(a, 300, 300)[3] == 0 and px(a, 600, 70)[3] == 255, (px(a, 300, 300), px(a, 600, 70)))
    a("click", id="Seg(Tab, 0)")
    a("click", id="Act(MaskDelete)")
    a("key", vk=ord("D"), ctrl=True)
    check("deselected", a("state")["selection"] is None)
    # two shapes, grouped, blended
    a("click", id="Act(AddShape)")
    a("click", id="Act(AddText)")
    a("type", text="Group me")
    st = a("state")
    ids = [l["id"] for l in st["layers"] if l["id"] != photo_id]
    a("click", id="LayerRow(%d)" % ids[0])
    a("click", id="LayerRow(%d)" % ids[1], ctrl=True)
    a("key", vk=ord("G"), ctrl=True)
    st = a("state")
    groups = [l for l in st["layers"] if "group" in l["kind"]]
    check("grouped", len(groups) == 1 and groups[0]["kind"]["group"] == 2, [l["kind"] for l in st["layers"]])
    a("click", id="BlendMenu")
    a("click", id="MenuItem(2)")
    st = a("state")
    check("blend mode Screen", [l for l in st["layers"] if "group" in l["kind"]][0]["blend"] == "Screen")
    a.shot("22-group")
    a("key", vk=ord("G"), ctrl=True, shift=True)
    check("ungrouped", not any("group" in l["kind"] for l in a("state")["layers"]))
    # the rotation handle turns the text
    text = [l for l in a("state")["layers"] if "text" in l["kind"]][0]
    a("click", id="LayerRow(%d)" % text["id"])
    a("click", id="Tool(Move)")
    hx, hy = centre(a, "Handle(8)")
    cx, cy = win(a, text["xf"][0], text["xf"][1])
    # a quarter turn clockwise: from above the centre to its right
    a("drag", x=hx, y=hy, x2=cx + (cy - hy), y2=cy, shift=True)
    rot = [l for l in a("state")["layers"] if l["id"] == text["id"]][0]["xf"][4]
    check("turned by the handle", abs(rot - 90) < 0.5, rot)
    # the font list, searched
    a("click", id="FontMenu")
    a("type", text="Georgia")
    hits = [h["id"] for h in a("hits") if h["id"].startswith("FontItem")]
    if hits:
        a("click", id=hits[0])
    st = a("state")
    t = [l for l in st["layers"] if l["id"] == text["id"]][0]
    check("font chosen from the list", t["kind"]["font"] == "Georgia" or not hits, t["kind"])
    # keyboard: duplicate, nudge, delete
    n = len(a("state")["layers"])
    a("key", vk=ord("J"), ctrl=True)
    check("duplicated", len(a("state")["layers"]) == n + 1)
    before = a("state")["layers"][-1]["xf"][0]
    a("key", vk=0x27, shift=True)
    after = a("state")["layers"][-1]["xf"][0]
    check("nudged by 10", abs(after - before - 10) < 0.01, (before, after))
    a("key", vk=0x2E)
    check("deleted", len(a("state")["layers"]) == n)
    # help sheet and settings
    a("key", vk=0x70)
    check("shortcuts sheet", (a("state")["dialog"] or "").startswith("Shortcuts"))
    a.shot("23-shortcuts")
    a("key", vk=27)
    # zoom keys
    z0 = a("state")["view"][0]
    a("key", vk=0xBB, ctrl=True)
    check("zoom in", a("state")["view"][0] > z0)
    a("key", vk=ord("0"), ctrl=True)
    st = a("state")
    print("   last render", round(st["render_ms"], 1), "ms")
finally:
    a.close()
    print()
    print("%d passed, %d failed" % (passes, len(fails)))
    for f in fails:
        print("  failed:", f)
    shutil.rmtree(WORK, ignore_errors=True)
sys.exit(1 if fails else 0)
