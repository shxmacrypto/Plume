"""Screenshots of Plume FEx for the website, on a demo folder with ordinary names (no developer paths).

    python apps/fex/tests/ui/demo_shots.py

The demo folder is made in %PUBLIC%\\Documents\\Plume Demo (a short path that names nobody) and removed at the end.
The pictures go to target/fex-ui/demo-*.png.
"""
import os, random, shutil, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *
from PIL import Image, ImageDraw

DEMO = os.path.join(os.environ.get("PUBLIC", r"C:\Users\Public"), "Documents", "Plume Demo")
random.seed(7)


def write(rel, size=0, data=None):
    p = os.path.join(DEMO, rel)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "wb") as f:
        f.write(data if data is not None else os.urandom(min(size, 1 << 16)) * max(1, size >> 16))
    return p


def picture(rel, hue):
    """A small landscape: sky, sun, hills, in colours that differ from one picture to the next."""
    w, h = 640, 427
    im = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(im)
    top, low = [(hue[0], hue[1], 255), (255, 170 + hue[1] // 4, 120)]
    for y in range(h):
        t = y / h
        d.line([(0, y), (w, y)], fill=tuple(int(top[i] * (1 - t) + low[i] * t) for i in range(3)))
    d.ellipse([w * 0.62, h * 0.18, w * 0.62 + 90, h * 0.18 + 90], fill=(255, 236, 170))
    for k, shade in enumerate([(40, 90, 70), (30, 70, 55), (20, 50, 40)]):
        base = h * (0.62 + k * 0.1)
        pts = [(0, h)] + [(x, base - 40 * abs(((x / w * (2 + k)) % 2) - 1) * (1 + k * 0.3)) for x in range(0, w + 1, 40)] + [(w, h)]
        d.polygon(pts, fill=shade)
    p = os.path.join(DEMO, rel)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    im.save(p, quality=88)


def make_demo():
    shutil.rmtree(DEMO, ignore_errors=True)
    for i, hue in enumerate([(70, 140), (120, 90), (40, 170), (150, 120), (90, 60), (60, 200), (130, 150), (100, 110)]):
        picture(r"Photos\Iceland 2026\IMG_%04d.jpg" % (412 + i * 7), hue)
    for i, hue in enumerate([(160, 100), (80, 180), (110, 130)]):
        picture(r"Photos\Montreal\IMG_%04d.jpg" % (1180 + i), hue)
    write(r"Photos\Montreal\Notes.txt", data="Old Port at sunset, 6 pm.\nBagels on Saint-Viateur.\n".encode())
    write(r"Work\2026\Report 2026.docx", 48_211)
    write(r"Work\2026\Report 2026 - final.pdf", 1_250_000)
    write(r"Work\2026\Budget 2026.xlsx", 92_400)
    write(r"Work\2026\Meeting notes.md", data=b"# Kickoff\n\n- Launch date: November\n- Owner: Lea\n- Report due Friday\n")
    write(r"Work\2025\Annual report 2025.pdf", 2_310_000)
    write(r"Work\2025\Invoices\Invoice 1042.pdf", 84_000)
    write(r"Work\2025\Invoices\Invoice 1043.pdf", 81_500)
    write(r"Projects\Plume\README.md", data=b"# Plume\n\nLight apps for Windows.\n")
    write(r"Projects\Plume\src\main.rs", data=b"fn main() {\n    println!(\"Hello, Plume!\");\n}\n")
    write(r"Projects\Plume\src\search.rs", data=b"// the report of a search\npub fn report() {}\n")
    write(r"Projects\Website\index.html", data=b"<!doctype html><title>Studio Nord</title>\n")
    write(r"Projects\Website\style.css", data=b"body { font: 16px system-ui }\n")
    write(r"Music\Night Drive.mp3", 4_820_000)
    write(r"Music\Morning Coffee.flac", 21_400_000)
    write(r"Videos\Iceland - Glacier walk.mp4", 182_000_000 // 64)
    write(r"Videos\Birthday party.mp4", 96_000_000 // 64)
    write(r"Backups\Photos 2025.zip", 64_000_000 // 32)
    write(r"Recipes.txt", data=b"Pancakes: flour, eggs, milk.\n")
    write(r"Travel checklist.txt", data=b"Passport\nCharger\nRain jacket\n")


def wait_search(x, text):
    if x.state()["edit"] != "Search":
        x.click("Search")
    x.edit(text)
    return x.wait(lambda s: s["load"] == "ready" and not s["stale"] and s["filter"] == text, what="results for " + text)


make_demo()
shots = []
try:
    x = Fex(DEMO, port=47891, profile="prof-demo", index=DEMO)
    try:
        x.wait()
        x.wait(lambda s: s["index_ready"], timeout=30, what="index ready")
        # 1. the whole folder with folder sizes
        x.cmd("menu", choose="Show folder sizes"); x.click("More")
        x.wait(lambda s: any(v is not None for k, v in zip(s["visible"], s["sizes"]) if k == "Photos"), what="folder sizes")
        x.click("Header(Size)"); x.click("Header(Size)")
        time.sleep(0.3)
        x.shot("demo-sizes"); shots.append("demo-sizes")
        x.click("Header(Name)")
        # 2. a search everywhere
        s = wait_search(x, "report")
        if not s["everywhere"]:
            x.click("ScopeAll")
            x.wait(lambda s: s["load"] == "ready" and s["everywhere"], what="everywhere")
        time.sleep(0.3)
        x.shot("demo-search"); shots.append("demo-search")
        # 3. a regular expression
        wait_search(x, r"regex:^IMG_\d+\.jpg$")
        time.sleep(0.3)
        x.shot("demo-regex"); shots.append("demo-regex")
        x.editkey(VK["ESC"])
        # 4. thumbnails of a photo folder
        x.nav(os.path.join(DEMO, "Photos", "Iceland 2026"))
        x.wait(lambda s: s["load"] == "ready", what="photos")
        x.key(L("2"), ctrl=True, shift=True)
        x.wait(lambda s: s["thumbs"] and s["thumbs_ready"] >= 8, timeout=30, what="thumbnails")
        time.sleep(0.4)
        x.shot("demo-thumbs"); shots.append("demo-thumbs")
        x.key(L("6"), ctrl=True, shift=True)
        # 5. the preview pane on a picture
        names = x.names()
        x.click("Row(%d)" % names.index("IMG_0426.jpg"))
        x.key(ord("P"), alt=True)
        x.wait(lambda s: s["preview"]["path"] and s["preview"]["path"].endswith("IMG_0426.jpg") and s["preview"]["kind"] != "loading", timeout=15, what="preview")
        time.sleep(0.4)
        x.shot("demo-preview"); shots.append("demo-preview")
        x.key(ord("P"), alt=True)
        # 6. a 7z with a password
        x.nav(os.path.join(DEMO, "Work", "2026"))
        x.wait(lambda s: s["load"] == "ready", what="work")
        names = x.names()
        x.click("Row(%d)" % names.index("Budget 2026.xlsx"))
        x.click("Row(%d)" % names.index("Report 2026.docx"), ctrl=True)
        row = names.index("Report 2026.docx")
        x.cmd("menu", choose="Compress with a password…")
        x.mouse("click", "Row(%d)" % row, button="right")
        x.wait(lambda s: s["sheet"] is not None, what="password box")
        for _ in range(30):
            try:
                x.click("Sheet7z")
                break
            except RuntimeError:
                time.sleep(0.05)
        x.edit("correct horse")
        time.sleep(0.3)
        x.shot("demo-archive"); shots.append("demo-archive")
    finally:
        x.close()
finally:
    shutil.rmtree(DEMO, ignore_errors=True)
print("shots:", ", ".join(os.path.join(OUT, n + ".png") for n in shots))
