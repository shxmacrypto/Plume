import os, struct, sys, zlib
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

# The preview pane (Alt+P): an image, the start of a text file, a folder, a binary file.


def png(path, w, h):
    """A real PNG, red, without any library."""
    raw = b"".join(b"\x00" + b"\xff\x30\x30" * w for _ in range(h))
    chunk = lambda kind, data: struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


d = fixture("fixture-preview")
png(os.path.join(d, "real.png"), 300, 200)
with open(os.path.join(d, "readme.txt"), "w", encoding="utf-8") as f:
    f.write("Hello preview\nLine two, été\n")
with open(os.path.join(d, "blob.bin"), "wb") as f:
    f.write(bytes(range(256)) * 8)
x = Fex(d, port=47824)
check = Checks()
VK_P = 0x50


def pick(name):
    names = x.names()
    x.click("Row(%d)" % names.index(name))
    return x.wait(lambda s: s["preview"]["path"] and s["preview"]["path"].endswith(name) and s["preview"]["kind"] != "loading", what="preview of " + name)


try:
    s = x.wait()
    check("the pane is closed at first", not s["preview"]["on"] and not s["preview"]["shown"], s["preview"])
    x.key(VK_P, alt=True)
    s = x.wait(lambda s: s["preview"]["shown"], what="pane open")
    check("Alt+P opens it", s["preview"]["on"])
    s = pick("readme.txt")
    check("a text file shows its first lines", s["preview"]["kind"] == "text" and s["preview"]["text"].startswith("Hello preview\nLine two, été"), s["preview"])
    s = pick("real.png")
    check("an image is decoded, at its own size when it fits", s["preview"]["kind"] == "image" and s["preview"]["size"] == [300, 200], s["preview"])
    x.shot("preview-image")
    s = pick("Alpha")
    check("a folder shows how many items it holds", s["preview"]["kind"] == "folder" and s["preview"]["count"] == 2, s["preview"])
    s = pick("blob.bin")
    check("a binary file is not shown as text", s["preview"]["kind"] == "info", s["preview"])
    x.shot("preview-info")
    x.key(VK_P, alt=True)
    s = x.wait(lambda s: not s["preview"]["shown"], what="pane closed")
    check("Alt+P closes it again", not s["preview"]["on"])
finally:
    x.close()
check.done()
