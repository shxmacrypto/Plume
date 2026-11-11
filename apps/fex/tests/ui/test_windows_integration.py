import json, os, subprocess, sys, winreg
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fex
from fex import *

# Everything is written under a scratch registry key: the real file-type registry is never touched by the tests.
ROOT = r"Software\PlumeFEx-Test\ui-integration"
CLASSES = ROOT + r"\Classes"
subprocess.run(["reg", "delete", "HKCU\\" + ROOT, "/f"], capture_output=True)
with winreg.CreateKey(winreg.HKEY_CURRENT_USER, CLASSES + r"\Directory\shell") as k:
    winreg.SetValueEx(k, "", 0, winreg.REG_SZ, "SomethingElse")


def value(sub, name=""):
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, CLASSES + "\\" + sub) as k:
            return winreg.QueryValueEx(k, name)[0]
    except OSError:
        return None


WIN_E = r"CLSID\{52205fd8-5dfb-447d-801a-d0b52f2e83e1}\shell\opennewwindow\command"
d = fixture("fixture-integration")
x = Fex(d, port=47833, env={"PLUME_FEX_CLASSES_ROOT": CLASSES})
check = Checks()


def more(choose=None):
    x.cmd("menu", choose=choose)
    x.click("More")
    return x.cmd("menu_last")


try:
    x.wait()
    labels = more()
    entry = "Add “Open in Plume FEx” to the right-click menu of folders"
    default = "Open folders and Win+E with Plume FEx"
    check("the menu offers both Windows options, off", entry in labels and default in labels, labels)
    # right-click entry
    more(entry)
    cmd = value(r"Directory\shell\PlumeFEx\command")
    check("the right-click entry is registered for folders, backgrounds and drives",
          cmd == '"%s" "%%1"' % fex.EXE and value(r"Directory\Background\shell\PlumeFEx\command") == '"%s" "%%V"' % fex.EXE and value(r"Drive\shell\PlumeFEx") == "Open in Plume FEx", cmd)
    check("folders still open with what opened them before", value(r"Directory\shell") == "SomethingElse", value(r"Directory\shell"))
    labels = more()
    check("the entry shows as on", entry + " [x]" in labels and default in labels, labels)
    # default explorer
    more(default)
    s = x.state()
    check("default explorer: folders, drives and Win+E point to Plume FEx",
          value(r"Directory\shell") == "PlumeFEx" and value(r"Drive\shell") == "PlumeFEx" and value(WIN_E) == '"%s"' % fex.EXE and value(WIN_E, "DelegateExecute") == "", (value(r"Directory\shell"), value(WIN_E)))
    check("a message says how to undo it", s["toast"] and "Turn this off" in s["toast"], s["toast"])
    saved = json.load(open(os.path.join(OUT, "prof-test", "settings.json"), encoding="utf-8"))
    check("what opened folders before is remembered", saved.get("explorer_backup", {}).get("directory") == "SomethingElse", saved.get("explorer_backup"))
    labels = more()
    check("both show as on", entry + " [x]" in labels and default + " [x]" in labels, labels)
    x.shot("windows-integration")
    # and back
    more(default)
    check("turning it off puts everything back", value(r"Directory\shell") == "SomethingElse" and value(r"Drive\shell") is None and value(WIN_E) is None and value(r"Directory\shell\PlumeFEx\command") is not None,
          (value(r"Directory\shell"), value(r"Drive\shell"), value(WIN_E)))
    # removing the entry while default is on undoes both
    more(default); more(entry)
    check("removing the right-click entry also gives folders back", value(r"Directory\shell") == "SomethingElse" and value(r"Directory\shell\PlumeFEx") is None and value(WIN_E) is None,
          (value(r"Directory\shell"), value(r"Directory\shell\PlumeFEx")))
finally:
    x.close()
    subprocess.run(["reg", "delete", "HKCU\\" + ROOT, "/f"], capture_output=True)
check.done()
