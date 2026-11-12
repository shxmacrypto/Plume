import os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fex import *

# Everything-style search: regular expressions, groups, file types, duplicates, empty folders, accents,
# the filter button of the search bar, and the export of the results.
d = fixture("fixture12")
os.makedirs(os.path.join(d, "dup"))
os.makedirs(os.path.join(d, "Vide"))
for rel, size in [
    (("song.mp3",), 3001), (("movie.mkv",), 4001), (("archive.zip",), 601), (("Été photo.JPG",), 701),
    (("Main.rs",), 81), (("main_window.rs",), 91), (("IMG_0042.jpg",), 1001), (("IMG_0043.jpeg",), 1101),
    (("same1.bin",), 5000), (("same2.bin",), 5000), (("dup", "a1.txt"), 11),
]:
    with open(os.path.join(d, *rel), "wb") as f:
        f.write(b"x" * size)
csv = os.path.join(OUT, "export-test.csv")
if os.path.exists(csv):
    os.remove(csv)
x = Fex(d, port=47822, index=d, env={"PLUME_TEST_SAVE": csv})
check = Checks()


def search(text):
    if x.state()["edit"] != "Search":
        x.click("Search")
    x.edit(text)
    return x.wait(lambda s: s["load"] == "ready" and not s["stale"] and s["filter"] == text, what="results for " + text)


def choose(label):
    """Pick an entry of the filter menu; returns the state once the results are back."""
    x.cmd("menu", choose=label)
    x.click("Filter")
    time.sleep(0.15)
    return x.wait(lambda s: s["load"] == "ready" and not s["stale"], what="results after " + label)


try:
    x.wait()
    x.wait(lambda s: s["index_ready"], timeout=15, what="index ready")
    s = search(r"regex:^img_\d+\.jpe?g$"); check("regular expression", s["visible"] == ["IMG_0042.jpg", "IMG_0043.jpeg"], s["visible"])
    s = search("regex:(unclosed"); check("a broken expression finds nothing, without error", s["rows"] == 0, s["rows"])
    s = search("<a1|notes> txt|md"); check("groups and OR", sorted(s["visible"]) == ["a1.txt", "a1.txt", "a10.txt", "notes.md"], s["visible"])
    s = search("audio:"); check("audio:", s["visible"] == ["song.mp3"], s["visible"])
    s = search("video:"); check("video:", s["visible"] == ["movie.mkv"], s["visible"])
    s = search("zip:"); check("zip:", s["visible"] == ["archive.zip"], s["visible"])
    s = search("pic:"); check("pic:", sorted(s["visible"]) == sorted(["photo.png", "Été photo.JPG", "IMG_0042.jpg", "IMG_0043.jpeg"]), s["visible"])
    s = search("ete"); check("accents are ignored: ete finds Été", s["visible"] == ["Été photo.JPG"], s["visible"])
    s = search("diacritics:ete"); check("diacritics: makes accents count", s["rows"] == 0, s["visible"])
    s = search("empty:"); check("empty folders", sorted(s["visible"]) == sorted(["beta", "Gamma", "Inner", "Vide"]), s["visible"])
    s = search("dupe:"); check("same name twice", s["visible"] == ["a1.txt", "a1.txt"] and len(set(s["folders"])) == 2, (s["visible"], s["folders"]))
    s = search("sizedupe:"); check("same size twice", s["visible"] == ["same1.bin", "same2.bin"], s["visible"])
    s = search("child:inside"); check("folders by what they contain", s["visible"] == ["Alpha"], s["visible"])
    s = search("wfn:main.rs"); check("whole file name", s["visible"] == ["Main.rs"], s["visible"])
    s = search("txt count:2"); check("count: limits the results", s["rows"] == 2 and s["total"] == 2, (s["rows"], s["total"]))
    # the filter button: one type of items
    s = search("p")
    check("the button shows no filter at first", s["search_kind"] == "All" and not any(s["search_match"].values()), (s["search_kind"], s["search_match"]))
    before = s["total"]
    s = choose("Picture")
    check("Picture keeps only pictures", s["search_kind"] == "Files(4)" and sorted(s["visible"]) == sorted(["photo.png", "Été photo.JPG", "IMG_0042.jpg", "IMG_0043.jpeg"]), (s["search_kind"], s["visible"]))
    labels = x.cmd("menu_last")
    wanted = ["All types", "Audio", "Compressed", "Document", "Executable", "Folder", "Picture", "Video",
              "Date modified > Any time", "Date modified > Today", "Date modified > Yesterday", "Date modified > This week",
              "Date modified > Last week", "Date modified > This month", "Date modified > This year",
              "Size > Any size", "Size > Empty (0 bytes)", "Size > Smaller than 1 MB", "Size > 1 MB to 100 MB",
              "Size > 100 MB to 1 GB", "Size > Larger than 1 GB", "Match case", "Match whole word", "Match path", "Regular expression", "Search inside files (slower)", "Export results…"]
    check("the menu lists the types like Everything, then the ways to match", [l.replace(" [x]", "") for l in labels] == wanted, labels)
    x.shot("search-filter-picture")
    s = choose("Folder"); check("Folder keeps only folders", s["search_kind"] == "Folders" and sorted(s["visible"]) == ["Alpha", "dup"], s["visible"])
    labels = choose("Folder") and x.cmd("menu_last")
    check("the current type is ticked", "Folder [x]" in labels and "All types" in labels, labels)
    s = choose("All types"); check("All types brings everything back", s["search_kind"] == "All" and s["total"] == before, (s["total"], before))
    # the ways to match
    s = search("main"); check("case is ignored by default", s["visible"] == ["Main.rs", "main_window.rs"], s["visible"])
    s = choose("Match case"); check("Match case", s["search_match"]["case"] and s["visible"] == ["main_window.rs"], (s["search_match"], s["visible"]))
    s = choose("Match case"); check("Match case off again", not s["search_match"]["case"] and s["rows"] == 2, s["rows"])
    s = choose("Match whole word"); check("whole word: main_window still has the word main", s["rows"] == 2, s["visible"])
    s = search("mai"); check("whole word: half a word finds nothing", s["rows"] == 0, s["visible"])
    s = choose("Match whole word")
    s = search("alpha"); check("without Match path, only names", s["visible"] == ["Alpha"], s["visible"])
    s = choose("Match path"); check("Match path also finds what the folder holds", sorted(s["visible"]) == ["Alpha", "Inner", "inside.txt"], s["visible"])
    s = choose("Match path")
    s = choose("Regular expression")
    s = search(r"^a\d+\.txt$"); check("the whole text as one regular expression", sorted(s["visible"]) == ["a1.txt", "a1.txt", "a10.txt", "a2.txt"], s["visible"])
    x.shot("search-filter-regex")
    # export of every result
    x.cmd("menu", choose="Export results…"); x.click("Filter")
    for _ in range(100):
        if os.path.exists(csv) and os.path.getsize(csv) > 40:
            break
        time.sleep(0.05)
    time.sleep(0.2)
    with open(csv, encoding="utf-8-sig", newline="") as f:
        rows = [line.rstrip("\r\n") for line in f]
    check("the export has a header and one line per result", rows[0] == "Name,Path,Size,Date Modified" and len(rows) == 5, rows[:2])
    line = [r for r in rows if r.startswith('"a10.txt"')]
    check("each line gives name, folder, size and date", len(line) == 1 and line[0].startswith('"a10.txt","%s",300,20' % d), line)
    s = x.wait(lambda s: s["toast"], what="export toast")
    check("a message confirms the export", "4" in s["toast"], s["toast"])
    s = choose("Regular expression"); check("regular expression off: the text is words again", s["rows"] == 0 and not s["search_match"]["regex"], s["rows"])
finally:
    x.close()
check.done()
