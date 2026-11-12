# Renders the Plume logos (SVG) to PNG and ICO.
#   python assets/brand/make-icons.py
# Needs Microsoft Edge (to draw the SVG, headless, with a throwaway profile) and Pillow (to resize and write .ico).
import os, shutil, subprocess, sys, tempfile
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
# name -> SVG source ; Plume Web keeps the original logo
LOGOS = {
    "plume-web": os.path.join(HERE, "plume-web.svg"),
    "plume-suite": os.path.join(HERE, "plume-suite.svg"),
    "plume-fex": os.path.join(HERE, "plume-fex.svg"),
    "plume-mail": os.path.join(HERE, "plume-mail.svg"),
    "plume-viewer": os.path.join(HERE, "plume-viewer.svg"),
    "plume-music": os.path.join(HERE, "plume-music.svg"),
    "plume-drop": os.path.join(HERE, "plume-drop.svg"),
    "plume-launcher": os.path.join(HERE, "plume-launcher.svg"),
    "plume-passwords": os.path.join(HERE, "plume-passwords.svg"),
    "plume-media": os.path.join(HERE, "plume-media.svg"),
    "plume-notepad": os.path.join(HERE, "plume-notepad.svg"),
    "plume-cut": os.path.join(HERE, "plume-cut.svg"),
    "plume-create": os.path.join(HERE, "plume-create.svg"),
    "plume-boost": os.path.join(HERE, "plume-boost.svg"),
    "plume-task": os.path.join(HERE, "plume-task.svg"),
    "plume-clean": os.path.join(HERE, "plume-clean.svg"),
    "plume-backup": os.path.join(HERE, "plume-backup.svg"),
    "plume-clipboard": os.path.join(HERE, "plume-clipboard.svg"),
    "plume-pdf": os.path.join(HERE, "plume-pdf.svg"),
    "plume-calendar": os.path.join(HERE, "plume-calendar.svg"),
    "plume-convert": os.path.join(HERE, "plume-convert.svg"),
}
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]
BIG = 1024


def edge():
    for base in [os.environ.get("ProgramFiles(x86)", ""), os.environ.get("ProgramFiles", "")]:
        exe = os.path.join(base, "Microsoft", "Edge", "Application", "msedge.exe")
        if os.path.isfile(exe):
            return exe
    sys.exit("Microsoft Edge not found")


def render(svg, png, work):
    # a page with a transparent background that shows the drawing edge to edge
    page = os.path.join(work, "page.html")
    with open(svg, encoding="utf-8") as f:
        drawing = f.read()
    with open(page, "w", encoding="utf-8") as f:
        f.write("<!doctype html><html><body style='margin:0;background:transparent;overflow:hidden'>"
                "<div style='width:%dpx;height:%dpx'>%s</div></body></html>" % (BIG, BIG, drawing.replace("<svg ", "<svg width='100%%' height='100%%' " % ())))
    subprocess.run([edge(), "--headless=new", "--disable-gpu", "--hide-scrollbars", "--user-data-dir=" + os.path.join(work, "profile"),
                    "--default-background-color=00000000", "--window-size=%d,%d" % (BIG, BIG), "--screenshot=" + png, "file:///" + page.replace("\\", "/")],
                   stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=120)
    if not os.path.isfile(png):
        sys.exit("Edge did not produce " + png)


def main():
    work = tempfile.mkdtemp(prefix="plume-icons-")
    try:
        sheet = []
        for name, svg in LOGOS.items():
            big = os.path.join(work, name + ".png")
            render(svg, big, work)
            img = Image.open(big).convert("RGBA")
            if img.getpixel((2, 2))[3] != 0:
                sys.exit("the background of %s is not transparent" % name)
            sheet.append((name, img))
            if name == "plume-web":
                continue  # its files already exist: plume-web-256.png and plume-web.ico
            img.resize((256, 256), Image.LANCZOS).save(os.path.join(HERE, name + "-256.png"))
            img.resize((256, 256), Image.LANCZOS).save(os.path.join(HERE, name + ".ico"), sizes=[(s, s) for s in ICO_SIZES])
            print("wrote", name + ".ico", "and", name + "-256.png")
        # a contact sheet to compare the family at real sizes, on dark and on light
        w = 40 + len(sheet) * 300
        out = Image.new("RGBA", (w, 760), (28, 28, 30, 255))
        light = Image.new("RGBA", (w, 380), (242, 242, 247, 255))
        out.paste(light, (0, 380))
        for row in (0, 380):
            for k, (_, img) in enumerate(sheet):
                x = 40 + k * 300
                out.alpha_composite(img.resize((192, 192), Image.LANCZOS), (x, row + 30))
                cx = x
                for s in (64, 48, 32, 24, 16):
                    out.alpha_composite(img.resize((s, s), Image.LANCZOS), (cx, row + 250 + (64 - s)))
                    cx += s + 12
        out.convert("RGB").save(os.path.join(HERE, "family.png"))
        print("wrote family.png")
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
