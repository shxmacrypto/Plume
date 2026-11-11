import os, shutil, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

# Drives the change journal does not follow (USB sticks, cards, network drives) are indexed too: read when they
# appear, read again from time to time, gone from the results when unplugged. A folder plays the drive here.
d = fixture("fixture-extra-main")
usb = os.path.join(OUT, "fixture-extra-usb")
shutil.rmtree(usb, ignore_errors=True)
shutil.rmtree(usb + "-away", ignore_errors=True)
os.makedirs(os.path.join(usb, "Holidays"))
with open(os.path.join(usb, "Holidays", "beach-2026.jpg"), "wb") as f:
    f.write(b"x" * 100)
x = Fex(d, port=47828, index=d, env={"PLUME_FEX_EXTRA_ROOTS": usb, "PLUME_FEX_EXTRA_CHECK_MS": "300", "PLUME_FEX_EXTRA_EVERY_MS": "1500"})
check = Checks()


def search(text):
    if x.state()["edit"] != "Search":
        x.click("Search")
    x.edit(text)
    return x.wait(lambda s: s["load"] == "ready" and not s["stale"] and s["filter"] == text, what="results for " + text)


def until(pred, what, timeout=15):
    end = time.time() + timeout
    while True:
        x.cmd("key", vk=VK["F5"])  # refresh the results
        s = x.wait(lambda s: s["load"] == "ready" and not s["stale"], what=what)
        if pred(s) or time.time() > end:
            return s
        time.sleep(0.3)


try:
    x.wait()
    x.wait(lambda s: s["index_ready"], timeout=15, what="index ready")
    s = search("beach")
    if not s["everywhere"]:
        x.click("ScopeAll")
        s = x.wait(lambda s: s["load"] == "ready" and s["everywhere"], what="everywhere")
    s = until(lambda s: s["visible"] == ["beach-2026.jpg"], "file on the stick")
    check("a file on the plugged drive is found from anywhere", s["visible"] == ["beach-2026.jpg"], s["visible"])
    with open(os.path.join(usb, "Holidays", "beach-2027.jpg"), "wb") as f:
        f.write(b"y")
    s = until(lambda s: len(s["visible"]) == 2, "new file after a re-read")
    check("a file added later shows up once the drive is read again", sorted(s["visible"]) == ["beach-2026.jpg", "beach-2027.jpg"], s["visible"])
    os.rename(usb, usb + "-away")  # unplugged
    s = until(lambda s: s["rows"] == 0, "drive gone")
    check("unplugged, its files leave the results", s["rows"] == 0, s["visible"])
    os.rename(usb + "-away", usb)  # plugged in again
    s = until(lambda s: len(s["visible"]) == 2, "drive back")
    check("plugged in again, they come back", len(s["visible"]) == 2, s["visible"])
finally:
    x.close()
check.done()
