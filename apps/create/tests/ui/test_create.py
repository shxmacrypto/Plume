# End-to-end run of Plume Create through its test socket.
#   python test_create.py <plume-create.exe built with --features test-hook>
# A generated photo is edited (crop, adjustments, a look, text) and exported; a 1080 x 1920 story is built from a
# template and exported; both exports are checked (size, colours where they should be). Screenshots go to
# target/shots-create/. Nothing outside a temporary folder is touched.
import os, shutil, sys, tempfile, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ctl import App, SHOTS
from PIL import Image, ImageDraw

EXE = sys.argv[1]
WORK = tempfile.mkdtemp(prefix="plume-create-e2e-")
DATA = os.path.join(WORK, "data")
OUT = os.path.join(WORK, "out")
os.makedirs(OUT)
fails = []
passes = 0


def check(name, ok, info=""):
    global passes
    if ok:
        passes += 1
        print("  ok  ", name)
    else:
        fails.append(name)
        print("  FAIL", name, info)


def near(c, want, tol):
    return all(abs(int(a) - int(b)) <= tol for a, b in zip(c[:3], want))


def make_photo(path):
    # 1600 x 1200: blue sky fading down, green ground, a red disc (the subject), a yellow square top left
    img = Image.new("RGB", (1600, 1200))
    d = ImageDraw.Draw(img)
    for y in range(800):
        t = y / 800
        d.line([(0, y), (1600, y)], fill=(int(90 + 80 * t), int(150 + 60 * t), int(235 - 20 * t)))
    d.rectangle([0, 800, 1600, 1200], fill=(60, 140, 60))
    d.ellipse([650, 400, 950, 700], fill=(210, 40, 40))
    d.rectangle([40, 40, 140, 140], fill=(250, 220, 30))
    img.save(path, quality=95)


def win_of(a, x, y):
    p = a("doc_to_win", x=x, y=y)
    return p["x"], p["y"]


def centre(a, hit_id):
    for h in a("hits"):
        if h["id"] == hit_id:
            r = h["rect"]
            return r[0] + r[2] / 2, r[1] + r[3] / 2
    raise KeyError(hit_id)


photo = os.path.join(WORK, "garden.jpg")
make_photo(photo)
a = App(EXE, port=47821, data=DATA, env={"PLUME_SUITE_THEME": "dark", "PLUME_SUITE_LANG": "en", "PLUME_TEST_SAVE": os.path.join(OUT, "dialog-export.png")})
try:
    time.sleep(1.0)
    a.shot("01-home")
    print("Photo")
    check("open photo", a("open", path=photo).get("ok"))
    st = a("state")
    check("canvas is the photo's size", st["doc"] == [1600, 1200], st["doc"])
    check("one picture layer", len(st["layers"]) == 1 and "image" in st["layers"][0]["kind"])
    a.shot("02-photo-open")

    # crop: the tool, then the top left corner dragged in by (200, 100)
    a("click", id="Tool(Crop)")
    hx, hy = centre(a, "CropHandle(0)")
    tx, ty = win_of(a, 200, 100)
    a("drag", x=hx, y=hy, x2=tx, y2=ty)
    st = a("state")
    crop = st["crop"]
    check("crop frame moved", crop and abs(crop[2] - 1400) < 6 and abs(crop[3] - 1100) < 6, crop)
    a.shot("03-crop")
    a("click", id="Act(CropApply)")
    st = a("state")
    cw, ch = st["doc"]
    check("cropped canvas", abs(cw - 1400) < 6 and abs(ch - 1100) < 6, st["doc"])
    check("crop is one undo step", st["history"][0] == 1, st["history"])

    # adjustments: exposure by the test socket, saturation by dragging its slider
    a("click", id="Seg(Tab, 1)")
    a("slider", sl="Adj(0)", value=25)
    sx, sy = centre(a, "Slider(Adj(3))")
    hits = {h["id"]: h["rect"] for h in a("hits")}
    r = hits["Slider(Adj(3))"]
    a("drag", x=r[0] + r[2] / 2, y=sy, x2=r[0] + r[2] * 0.8, y2=sy)
    st = a("state")
    check("adjusted", st["layers"][0]["adjusted"])
    a.shot("04-adjust")

    # a look
    a("click", id="Seg(Tab, 2)")
    time.sleep(0.3)
    # the Film tile is further down the panel: scroll until it shows
    for _ in range(12):
        hits = {h["id"]: h["rect"] for h in a("hits")}
        r = hits.get("Look(Film)")
        top = [h for h in a("hits") if h["id"] == "PanelArea"]
        if r and a("click", id="Look(Film)") and a("state")["layers"][0]["look"] == "Film":
            break
        px, py = centre(a, "Seg(Tab, 2)")
        a("wheel", x=px, y=py + 200, dy=-2)
    a("click", id="Look(Film)")
    st = a("state")
    check("look Film", st["layers"][0]["look"] == "Film", st["layers"][0]["look"])
    a.shot("05-looks")

    # text: the tool, a click low on the photo, words typed
    a("click", id="Tool(Text)")
    x, y = win_of(a, 80, 900)
    st = a("state")
    n_before = len(st["layers"])
    a("move", x=x, y=y)
    a("drag", x=x, y=y, x2=x, y2=y, steps=1)
    st = a("state")
    check("text field open", st["field"] and st["field"].startswith("LayerText"), st["field"])
    a("type", text="Hello Plume")
    st = a("state")
    texts = [l for l in st["layers"] if "text" in l["kind"]]
    check("text layer added", any(l["kind"]["text"] == "Hello Plume" for l in texts), [l["kind"] for l in texts])
    a("click", id="Seg(Tab, 0)")
    a.shot("06-text")
    # make the words big and white so they are easy to find in the export
    tl = [l for l in a("state")["layers"] if l["kind"].get("text") == "Hello Plume"][-1]
    a("click", id="LayerRow(%d)" % tl["id"])
    a("slider", sl="TextSize", value=120)
    st = a("state")
    tl = [l for l in st["layers"] if l["kind"].get("text") == "Hello Plume"][-1]
    print("   text box", tl["xf"])

    # export: JPEG at 700 px wide, twice (the second never replaces the first)
    p1 = a("export", path=os.path.join(OUT, "photo.jpg"), format="Jpeg", w=700, quality=90)
    p2 = a("export", path=os.path.join(OUT, "photo.jpg"), format="Jpeg", w=700, quality=90)
    check("export written", p1.get("ok") and os.path.isfile(p1["path"]), p1)
    check("second export beside the first", p2.get("ok") and p2["path"].endswith("photo (2).jpg"), p2)
    im = Image.open(p1["path"]).convert("RGB")
    k = 700 / cw
    check("photo export size", im.size == (700, round(ch * k)), im.size)
    # the disc was at (800, 550) before the crop moved everything by (-200, -100)
    disc = im.getpixel((int(600 * k), int(450 * k)))
    check("subject still red", disc[0] > 150 and disc[0] > disc[1] + 60 and disc[0] > disc[2] + 40, disc)
    sky = im.getpixel((int(1100 * k), int(60 * k)))
    check("sky still blue-ish", sky[2] > sky[0], sky)
    grass = im.getpixel((int(1200 * k), int(1050 * k)))
    check("grass still green-ish", grass[1] > grass[0] and grass[1] > grass[2], grass)
    # the words: white pixels inside the text box
    cx, cy, bw, bh = tl["xf"][:4]
    box = (int((cx - bw / 2) * k), int((cy - bh / 2) * k), int((cx + bw / 2) * k), int((cy + bh / 2) * k))
    region = im.crop(box)
    # the words take the current colour (Plume Create's magenta at first)
    ink = sum(1 for p in region.get_flattened_data() if near(p, (0xDB, 0x2B, 0x9A), 60)) if region.size[0] > 0 else 0
    check("words in the export", ink > 200, (box, ink))
    # the yellow square was cropped away (it was at 40..140, the crop starts at 200, 100)
    tl_px = im.getpixel((2, 2))
    check("cropped part gone", not (tl_px[0] > 200 and tl_px[1] > 180 and tl_px[2] < 90), tl_px)

    # export dialog (the save dialog answers PLUME_TEST_SAVE in a test build)
    a("click", id="Act(Export)")
    a.shot("07-export-dialog")
    a("click", id="Seg(Format, 0)")
    a("click", id="Act(ExportNow)")
    check("export from the dialog", os.path.isfile(os.path.join(OUT, "dialog-export.png")))

    # undo all the way, redo all the way
    st = a("state")
    steps = st["history"][0]
    for _ in range(steps):
        a("key", vk=ord("Z"), ctrl=True)
    st = a("state")
    check("undo back to the photo", st["doc"] == [1600, 1200] and len(st["layers"]) == 1 and not st["layers"][0]["adjusted"], (st["doc"], len(st["layers"])))
    for _ in range(steps):
        a("key", vk=ord("Y"), ctrl=True)
    st = a("state")
    check("redo to the end", abs(st["doc"][0] - cw) < 1 and len(st["layers"]) == n_before + 1, (st["doc"], len(st["layers"])))

    print("Story")
    check("template", a("template", key="story-sale").get("ok"))
    st = a("state")
    check("story size", st["doc"] == [1080, 1920], st["doc"])
    title = [l for l in st["layers"] if l["name"] == "Title"][0]
    a("click", id="LayerRow(%d)" % title["id"])
    a("click", id="Field(LayerText(%d))" % title["id"])
    a("type", text="PLUME\nDAYS")
    st = a("state")
    title = [l for l in st["layers"] if l["id"] == title["id"]][0]
    check("title changed", title["kind"]["text"] == "PLUME\nDAYS", title["kind"])
    # a shape drawn with the tool, then moved by the move tool (snapping on)
    a("click", id="Tool(Shape)")
    a("click", id="Seg(NewShape, 1)")
    x1, y1 = win_of(a, 700, 1380)
    x2, y2 = win_of(a, 940, 1620)
    a("drag", x=x1, y=y1, x2=x2, y2=y2)
    st = a("state")
    ell = [l for l in st["layers"] if l["kind"].get("shape") == "Ellipse" and abs(l["xf"][2] - 240) < 8]
    check("ellipse drawn", len(ell) == 1, [l["xf"] for l in st["layers"] if "shape" in l["kind"]])
    a("click", id="Tool(Move)")
    ex, ey = win_of(a, 820, 1500)
    # towards the canvas' middle line: it snaps onto x = 540
    tx, ty = win_of(a, 543, 1500)
    a("drag", x=ex, y=ey, x2=tx, y2=ty)
    st = a("state")
    e = [l for l in st["layers"] if l["id"] == ell[0]["id"]][0] if ell else None
    check("snapped to the centre", e and abs(e["xf"][0] - 540) < 0.6, e and e["xf"])
    a.shot("08-story")
    # export the story and check it
    ps = a("export", path=os.path.join(OUT, "story.png"), format="Png", w=1080)
    check("story export", ps.get("ok"), ps)
    im = Image.open(ps["path"]).convert("RGBA")
    check("story export size", im.size == (1080, 1920), im.size)
    top = im.getpixel((900, 20))
    check("top is the magenta end", near(top, (0xDB, 0x2B, 0x9A), 40), top)
    bottom = im.getpixel((900, 1900))
    check("bottom is the orange end", near(bottom, (0xFF, 0x8A, 0x3D), 40), bottom)
    button = im.getpixel((120, 1700))
    check("the white button", near(button, (255, 255, 255), 8), button)
    badge = im.getpixel((130, 1180))
    check("the dark badge", near(badge, (0x2B, 0x0A, 0x24), 12), badge)
    centre_px = im.getpixel((540, 1500))
    check("the ellipse in the middle", near(centre_px, tuple(int(st["colour"][i:i + 2], 16) for i in (1, 3, 5)), 30), (centre_px, st["colour"]))
    t = [l for l in st["layers"] if l["id"] == title["id"]][0]
    cx, cy, bw, bh = t["xf"][:4]
    region = im.crop((int(cx - bw / 2), int(cy - bh / 2), int(cx + bw / 2), int(cy + bh / 2)))
    whites = sum(1 for p in region.get_flattened_data() if p[0] > 235 and p[1] > 235 and p[2] > 235)
    check("title letters in the export", whites > 5000, whites)

    # the project file keeps it all
    proj = os.path.join(OUT, "story.plumecreate")
    check("save project", a("save", path=proj).get("ok"))
    n = len(a("state")["layers"])
    check("open project", a("open", path=proj).get("ok"))
    st = a("state")
    check("project layers back", len(st["layers"]) == n and st["doc"] == [1080, 1920], (len(st["layers"]), n))
    pb = a("export", path=os.path.join(OUT, "story-again.png"), format="Png", w=1080)
    im2 = Image.open(pb["path"]).convert("RGBA")
    diff = sum(1 for p, q in zip(im.get_flattened_data(), im2.get_flattened_data()) if max(abs(p[i] - q[i]) for i in range(4)) > 2)
    check("same picture after reopening", diff < 50, diff)

    # light theme, French: a look at the editor
    a.close()
    time.sleep(0.5)
    b = App(EXE, port=47822, data=DATA, env={"PLUME_SUITE_THEME": "light", "PLUME_SUITE_LANG": "fr"})
    try:
        time.sleep(1.0)
        b.shot("09-home-light-fr")
        b("open", path=proj)
        b("click", id="Seg(Tab, 1)")
        b.shot("10-story-light-fr")
        b("open", path=photo)
        b("click", id="Tool(Cutout)")
        b("click", id="Act(RemoveBg)")
        st = b("state")
        check("background removed (mask)", st["layers"][0]["mask"], st["layers"][0])
        b.shot("11-cutout-light-fr")
        st = b("state")
        print("   log:", [l for l in st["log"] if "cutout" in l or "export" in l])
    finally:
        b.close()
finally:
    a.close()
    print()
    print("%d passed, %d failed" % (passes, len(fails)))
    for f in fails:
        print("  failed:", f)
    shutil.rmtree(WORK, ignore_errors=True)
sys.exit(1 if fails else 0)
