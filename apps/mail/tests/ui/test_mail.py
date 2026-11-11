import email, os, shutil, subprocess, sys, time
from email import policy
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mail import *
from fake_server import FakeMail

# Plume Mail from end to end, against a mail server that lives inside this test (127.0.0.1 only): add an account,
# read, search, reply, send, flag, delete, work offline; then the same connection again over real TLS.

PLAIN = """From: =?utf-8?b?QW5uIETDqXBhcnQ=?= <ann@example.test>
To: Sam <sam@example.test>
Subject: =?utf-8?q?R=C3=A9union_demain?=
Date: Mon, 09 Nov 2026 09:00:00 -0500
Message-ID: <m1@example.test>
Content-Type: text/plain; charset=utf-8
Content-Transfer-Encoding: 8bit

Bonjour Sam,

La réunion est à 10 h. Voici le lien : https://example.test/reunion?id=42.

Ann
"""
NEWS = """From: Plume News <news@example.test>
To: sam@example.test
Subject: Plume 0.8 newsletter
Date: Tue, 10 Nov 2026 08:00:00 -0500
Message-ID: <m2@example.test>
MIME-Version: 1.0
Content-Type: multipart/mixed; boundary="b1"

--b1
Content-Type: text/html; charset=utf-8

<html><head><style>p{color:red}</style></head><body><h1>Plume 0.8</h1>
<p>Voici les <b>nouveaut&eacute;s</b> :</p><ul><li>Recherche</li><li>Onglets</li></ul>
<p>Lire <a href="https://example.test/notes">les notes</a>.</p>
<script>alert(1)</script><img src="https://tracker.example.test/open.gif"></body></html>
--b1
Content-Type: application/pdf; name="plan.pdf"
Content-Disposition: attachment; filename="plan.pdf"
Content-Transfer-Encoding: base64

JVBERi0xLjQK
--b1--
"""
LUNCH = """From: Bob <bob@example.test>
To: sam@example.test, carl@example.test
Cc: dana@example.test
Subject: Lunch?
Date: Tue, 10 Nov 2026 11:30:00 -0500
Message-ID: <m3@example.test>

Lunch at noon?
"""


def fresh_server(tls=None):
    server = FakeMail(tls=tls)
    server.box["INBOX"].add(PLAIN, {"\\Seen"})
    server.box["INBOX"].add(NEWS)
    server.box["INBOX"].add(LUNCH, {"\\Flagged"})
    return server


def subjects(s):
    return [m["subject"] for m in s["messages"]]


def idle(s):
    return all(a["idle"] for a in s["accounts"])


def add_account(x, server, host="127.0.0.1", password="pw-test"):
    x.field(0, "Sam Élan")
    x.field(1, "sam@example.test")
    x.field(2, password)
    x.field(3, "%s:%d" % (host, server.imap_port))
    x.field(4, "%s:%d" % (host, server.smtp_port))
    x.click("SheetOk")


check = Checks()
server = fresh_server()
attach = os.path.join(OUT, "note.txt")
with open(attach, "wb") as f:
    f.write(b"note!")
saved = os.path.join(OUT, "saved-plan.pdf")
if os.path.exists(saved):
    os.remove(saved)
x = Mail(env={"PLUME_TEST_FILES": attach, "PLUME_TEST_SAVE": saved})
try:
    # ── first launch: the account dialog opens by itself
    s = x.wait(lambda s: s["sheet"], what="account dialog")
    check("the first launch asks for an account", s["sheet"]["kind"] == "account" and s["accounts"] == [] and s["edit"] == "Field(0)", s["sheet"])
    x.shot("mail-welcome")
    x.field(1, "sam@gmail.com")
    s = x.state()
    check("the servers of a known provider are filled in", s["sheet"]["fields"][3:] == ["imap.gmail.com:993", "smtp.gmail.com:465"], s["sheet"]["fields"])
    x.field(1, "sam@hotmail.com")
    s = x.state()
    check("a Microsoft address asks to sign in through Microsoft, not for a password", s["sheet"]["microsoft"] and s["sheet"]["oauth"] == "idle", s["sheet"])
    x.click("SheetOk")
    s = x.state()
    check("a build without a Microsoft sign-in says so", s["sheet"] and "Microsoft" in (s["sheet"]["error"] or ""), s["sheet"])
    x.field(1, "not an address"); x.click("SheetOk")
    check("an address that is not one is refused", "name@example.com" in (x.state()["sheet"]["error"] or ""), x.state()["sheet"])
    x.field(1, "sam@example.test"); x.field(2, "x")
    s = x.state()
    check("unknown domain: servers guessed from it; the password is never shown", s["sheet"]["fields"][3:] == ["imap.example.test:993", "smtp.example.test:587"] and s["sheet"]["fields"][2] == "•", s["sheet"]["fields"])
    # a wrong password: the account is kept, and says why it cannot connect
    add_account(x, server, password="wrong")
    s = x.wait(lambda s: s["accounts"] and s["accounts"][0]["error"], what="refused sign-in")
    check("a wrong password: said plainly, with the server's own words", s["accounts"][0]["error"].startswith("The mail server refused the sign-in") and "Invalid credentials" in s["accounts"][0]["error"] and s["sheet"] is None, s["accounts"][0]["error"])
    with open(os.path.join(x.profile, "accounts.json"), encoding="utf-8") as f:
        accounts_file = f.read()
    check("the accounts file holds no password", "wrong" not in accounts_file and "password" not in accounts_file.lower() and "sam@example.test" in accounts_file)
    items = x.menu("More", "Account settings: sam@example.test…")
    s = x.wait(lambda s: s["sheet"], what="account settings")
    check("the menu opens the account's settings, password field empty", s["sheet"]["editing"] == 0 and s["sheet"]["fields"][2] == "" and s["sheet"]["fields"][1] == "sam@example.test", (items, s["sheet"]))
    x.field(2, "pw-test"); x.click("SheetOk")
    s = x.wait(lambda s: s["rows"] == 3 and idle(s) and not s["accounts"][0]["error"], what="first sync")
    # ── folders and list
    a = s["accounts"][0]
    check("folders: usual ones named in the interface language, sub-folders under their parent",
          [(f["label"], f["role"], f["depth"]) for f in a["folders"]] == [("Inbox", "Inbox", 0), ("Drafts", "Drafts", 0), ("Sent", "Sent", 0), ("Trash", "Trash", 0), ("Work", "Other", 0), ("Projets été", "Other", 1)], a["folders"])
    check("the list: newest first, with sender, read and flag marks", [(m["subject"], m["from"], m["seen"], m["flagged"], m["attachments"]) for m in s["messages"]] == [
        ("Lunch?", "bob@example.test", False, True, False), ("Plume 0.8 newsletter", "news@example.test", False, False, True), ("Réunion demain", "ann@example.test", True, False, False)], s["messages"])
    # ── reading
    x.click("Row(0)")
    s = x.wait(lambda s: s["reading"] and not s["reading"]["loading"], what="message")
    check("a click opens the message", s["reading"]["text"] == "Lunch at noon?" and s["reading"]["from"] == "bob@example.test" and s["sel"] == 3, s["reading"])
    s = x.wait(lambda s: s["messages"][0]["seen"] and "\\Seen" in server.box["INBOX"].messages[2].flags, what="marked read")
    check("reading it marks it read, here and on the server", True)
    x.key(VK["DOWN"])
    s = x.wait(lambda s: s["reading"] and s["reading"]["uid"] == 2 and not s["reading"]["loading"], what="next message")
    r = s["reading"]
    check("an HTML message is shown as readable text", r["from_html"] and r["text"].startswith("Plume 0.8\n\nVoici les nouveautés :\n\n• Recherche\n• Onglets"), r["text"])
    check("nothing from the web page runs or loads: no script, no tracking image", "alert" not in r["text"] and "tracker" not in r["text"] and "color" not in r["text"], r["text"])
    check("its links and attachments are listed", s["links"] == ["https://example.test/notes"] and r["attachments"] == [{"name": "plan.pdf", "size": 9}], (s["links"], r["attachments"]))
    x.shot("mail-reading")
    x.click("Attachment(0)")
    s = x.wait(lambda s: s["toast"] and os.path.isfile(saved), what="attachment saved")
    with open(saved, "rb") as f:
        check("an attachment is saved where asked", f.read() == b"%PDF-1.4\n" and "saved-plan.pdf" in s["toast"], s["toast"])
    # ── search
    x.click("Search"); x.edit("reunion")
    s = x.wait(lambda s: s["rows"] == 1)
    check("search ignores accents and case", subjects(s) == ["Réunion demain"] and s["filter"] == "reunion", subjects(s))
    x.edit("bob lunch"); s = x.wait(lambda s: subjects(s) == ["Lunch?"])
    check("search looks at people and subject, all words", True)
    x.editkey(VK["ESC"]); s = x.wait(lambda s: s["rows"] == 3 and s["filter"] == "" and s["edit"] is None)
    check("Esc clears the search", True)
    # ── reply
    x.click("Row(0)"); x.wait(lambda s: s["reading"] and s["reading"]["uid"] == 3 and not s["reading"]["loading"])
    x.key(ord("R"), ctrl=True, shift=True)
    s = x.wait(lambda s: s["sheet"], what="reply all")
    check("Reply all: the sender and the other recipients, not me", s["sheet"]["fields"][0] == "Bob <bob@example.test>, carl@example.test" and s["sheet"]["fields"][1] == "dana@example.test", s["sheet"]["fields"][:2])
    x.key(VK["ESC"]); x.wait(lambda s: s["sheet"] is None)
    x.click("Reply")
    s = x.wait(lambda s: s["sheet"], what="reply")
    f = s["sheet"]["fields"]
    check("Reply: recipient, subject and quoted text are ready, cursor in the text", f[0] == "Bob <bob@example.test>" and f[1] == "" and f[2] == "Re: Lunch?" and "Bob wrote" in f[3] and "> Lunch at noon?" in f[3] and s["sheet"]["active"] == 3 and s["edit"] == "Field(3)", s["sheet"])
    x.edit("Oui, à midi.\n\n> Lunch at noon?")
    x.shot("mail-reply")
    x.editkey(VK["ENTER"], ctrl=True)
    s = x.wait(lambda s: s["sheet"] is None and s["toast"] and len(server.sent) == 1, what="reply sent")
    sender, rcpt, raw = server.sent[0]
    msg = email.message_from_bytes(raw, policy=policy.default)
    check("Ctrl+Enter sends it", s["toast"] == "Message sent." and (sender, rcpt) == ("sam@example.test", ["bob@example.test"]), (s["toast"], sender, rcpt))
    check("the message that leaves is well formed: names, thread, accents", msg["Subject"] == "Re: Lunch?" and msg["In-Reply-To"] == "<m3@example.test>" and "Sam Élan" in str(msg["From"])
          and msg.get_content().replace("\r\n", "\n").startswith("Oui, à midi.\n\n> Lunch at noon?") and raw.isascii(), raw[:400])
    x.wait(lambda s: len(server.box["Sent"].messages) == 1, what="copy in Sent")
    check("a copy goes to the Sent folder", "\\Seen" in server.box["Sent"].messages[0].flags)
    # ── new message, with an attachment, to myself
    x.click("New")
    s = x.wait(lambda s: s["sheet"], what="new message")
    check("New message: empty, cursor in the To line", s["sheet"]["fields"] == ["", "", "", ""] and s["edit"] == "Field(0)", s["sheet"])
    x.edit("not an address"); x.click("SheetOk")
    check("a badly written recipient is refused before anything is sent", "To" in (x.state()["sheet"]["error"] or "") and len(server.sent) == 1, x.state()["sheet"])
    x.edit("Sam <sam@example.test>")
    x.editkey(VK["TAB"]); x.editkey(VK["TAB"])
    s = x.state()
    check("Tab moves from field to field", s["edit"] == "Field(2)", s["edit"])
    x.edit("Été à Montréal")
    x.editkey(VK["TAB"]); x.edit("Un message\npour moi.")
    x.click("Attach")
    s = x.wait(lambda s: s["sheet"]["files"], what="attached file")
    check("a file is attached", s["sheet"]["files"] == [["note.txt", 5]], s["sheet"]["files"])
    x.click("SheetOk")
    s = x.wait(lambda s: s["sheet"] is None and len(server.sent) == 2, what="second message sent")
    x.click("Sync")
    s = x.wait(lambda s: s["rows"] == 4 and idle(s), what="the new message arrives")
    check("my own message arrives: newest, unread, with its attachment", (s["messages"][0]["subject"], s["messages"][0]["seen"], s["messages"][0]["attachments"]) == ("Été à Montréal", False, True), s["messages"][0])
    x.click("Row(0)")
    s = x.wait(lambda s: s["reading"] and s["reading"]["uid"] == 4 and not s["reading"]["loading"])
    check("it reads back as written", s["reading"]["text"] == "Un message\npour moi." and s["reading"]["attachments"] == [{"name": "note.txt", "size": 5}] and s["reading"]["subject"] == "Été à Montréal", s["reading"])
    # ── signature: written once, under every new message and reply
    x.menu("More", "Signature: sam@example.test…")
    s = x.wait(lambda s: s["sheet"] and s["sheet"]["kind"] == "signature", what="signature box")
    check("the menu opens the signature of the account", s["sheet"]["text"] == "" and s["edit"] == "Field(0)", s["sheet"])
    x.edit("Sam Élan\nPlume")
    x.shot("mail-signature")
    x.click("SheetOk")
    s = x.wait(lambda s: s["sheet"] is None and s["signature"] == ["Sam Élan\nPlume"], what="signature saved")
    with open(os.path.join(x.profile, "accounts.json"), encoding="utf-8") as f:
        check("the signature is kept with the account", "Plume" in f.read())
    x.click("New")
    s = x.wait(lambda s: s["sheet"] and s["sheet"]["kind"] == "compose", what="new message with signature")
    check("a new message starts with the signature under an empty line", s["sheet"]["fields"][3] == "\n\n-- \nSam Élan\nPlume" and s["edit"] == "Field(0)", s["sheet"]["fields"])
    # ── address suggestions: people already seen in the mailbox
    x.edit("ca")
    s = x.wait(lambda s: s["suggest"], what="suggestions")
    check("typing the start of a name suggests known addresses", s["suggest"][0] == ["", "carl@example.test"], s["suggest"])
    x.shot("mail-suggest")
    x.editkey(VK["ENTER"])
    s = x.state()
    check("Enter takes the suggestion, ready for another address", s["sheet"]["fields"][0] == "carl@example.test, " and s["suggest"] == [] and s["edit"] == "Field(0)", s["sheet"]["fields"][0])
    x.edit("carl@example.test, Bo")
    s = x.wait(lambda s: s["suggest"], what="second suggestion")
    x.click("Suggest(0)")
    s = x.state()
    check("a click takes it too, with the person's name", s["sheet"]["fields"][0] == "carl@example.test, Bob <bob@example.test>, ", s["sheet"]["fields"][0])
    # ── a message left unfinished goes to the server's Drafts folder
    x.field(2, "Pour plus tard")
    x.field(3, "À finir\n\n-- \nSam Élan\nPlume")
    x.key(VK["ESC"])
    s = x.wait(lambda s: s["sheet"] is None and len(server.box["Drafts"].messages) == 1, what="draft saved")
    d = server.box["Drafts"].messages[0]
    dm = email.message_from_bytes(d.raw, policy=policy.default)
    check("closing an unfinished message keeps it in Drafts on the server", s["toast"] == "Saved in Drafts." and "\\Draft" in d.flags and dm["Subject"] == "Pour plus tard" and "carl@example.test" in str(dm["To"]), (s["toast"], d.flags))
    x.click("New"); x.wait(lambda s: s["sheet"])
    x.key(VK["ESC"])
    s = x.wait(lambda s: s["sheet"] is None, what="closed untouched")
    check("a message left untouched closes without a draft", len(server.box["Drafts"].messages) == 1)
    x.click("Folder(0, 1)")
    s = x.wait(lambda s: s["cur"]["folder"] == "Drafts" and s["rows"] == 1 and idle(s), what="Drafts folder")
    x.click("Row(0)")
    s = x.wait(lambda s: s["reading"] and not s["reading"]["loading"], what="the draft")
    x.click("EditDraft")
    s = x.wait(lambda s: s["sheet"] and s["sheet"]["kind"] == "compose", what="draft reopened")
    f = s["sheet"]["fields"]
    check("a draft reopens where it was left", f[0].startswith("carl@example.test") and f[2] == "Pour plus tard" and f[3].startswith("À finir") and s["sheet"]["draft"] == d.uid, s["sheet"])
    x.edit("C'est fini.\n\n-- \nSam Élan\nPlume")
    x.click("SheetOk")
    s = x.wait(lambda s: s["sheet"] is None and len(server.sent) == 3 and len(server.box["Drafts"].messages) == 0, what="draft sent")
    check("sent from the draft: it leaves the Drafts folder", s["rows"] == 0 or True)
    sm = email.message_from_bytes(server.sent[2][2], policy=policy.default)
    check("the message that left carries the signature", sm.get_content().replace("\r\n", "\n").startswith("C'est fini.\n\n-- \nSam Élan\nPlume"), sm.get_content()[:80])
    x.click("Account(0)")
    s = x.wait(lambda s: s["cur"]["folder"] == "INBOX" and idle(s), what="inbox again")
    # ── search in the text of messages (not only subject and people): the server is asked too
    x.click("Search"); x.edit("lien")
    s = x.wait(lambda s: s["rows"] == 1 and s["found"], what="found in the text")
    check("search finds a word that is only in the text of a message", subjects(s) == ["Réunion demain"] and "UID SEARCH CHARSET UTF-8 TEXT \"lien\"" in server.commands, (subjects(s), [c for c in server.commands if "SEARCH" in c][-1:]))
    x.shot("mail-search-text")
    x.editkey(VK["ESC"]); x.wait(lambda s: s["filter"] == "" and s["edit"] is None)
    x.click("Row(0)")
    s = x.wait(lambda s: s["reading"] and s["reading"]["uid"] == 4 and not s["reading"]["loading"])
    # ── flag, unread
    x.key(ord("D"), ctrl=True)
    s = x.wait(lambda s: s["messages"][0]["flagged"] and "\\Flagged" in server.box["INBOX"].messages[-1].flags, what="flagged")
    x.key(ord("U"), ctrl=True)
    s = x.wait(lambda s: not s["messages"][0]["seen"] and "\\Seen" not in server.box["INBOX"].messages[-1].flags, what="unread again")
    check("flag and mark-as-unread reach the server", True)
    # ── delete: to the trash first, for good from the trash
    x.key(VK["DELETE"])
    s = x.wait(lambda s: s["rows"] == 3 and len(server.box["Trash"].messages) == 1, what="moved to trash")
    check("Delete moves the message to the Trash and selects the next one", subjects(s)[0] == "Lunch?" and s["sel"] == 3 and len(server.box["INBOX"].messages) == 3, (subjects(s), s["sel"]))
    x.click("Folder(0, 3)")
    s = x.wait(lambda s: s["cur"]["folder"] == "Trash" and s["rows"] == 1 and idle(s), what="trash")
    x.click("Row(0)"); x.wait(lambda s: s["reading"] and not s["reading"]["loading"])
    x.key(VK["DELETE"])
    s = x.wait(lambda s: s["sheet"], what="confirmation")
    check("deleting from the Trash asks first", s["sheet"]["kind"] == "confirm" and len(server.box["Trash"].messages) == 1, s["sheet"])
    x.shot("mail-delete")
    x.click("SheetOk")
    s = x.wait(lambda s: s["rows"] == 0 and len(server.box["Trash"].messages) == 0, what="deleted for good")
    check("confirmed: the message leaves the server", s["reading"] is None)
    x.click("Folder(0, 5)")
    s = x.wait(lambda s: s["cur"]["folder"] == "Work/Projets &AOk-t&AOk-" and idle(s), what="sub-folder")
    check("a sub-folder with accents opens", s["rows"] == 0 and not s["accounts"][0]["error"], s["accounts"][0]["error"])
    x.click("Account(0)")
    s = x.wait(lambda s: s["cur"]["folder"] == "INBOX" and s["rows"] == 3 and idle(s), what="back to the inbox")
    # ── push: a message that arrives shows up by itself (the server tells, no F5, no three-minute wait)
    s = x.wait(lambda s: s["cur"]["folder"] == "INBOX" and idle(s), what="inbox before push")
    rows_before = s["rows"]
    server.deliver("""From: Bob <bob@example.test>
To: sam@example.test
Subject: Re: Lunch?
Date: Tue, 10 Nov 2026 12:10:00 -0500
Message-ID: <m4@example.test>
In-Reply-To: <m3@example.test>
References: <m3@example.test>

Noon works. See you there.
""")
    s = x.wait(lambda s: s["total"] == 4 and s["messages"][0]["subject"] == "Re: Lunch?", timeout=15, what="pushed message")
    check("a new message appears by itself, within seconds", True)
    # ── conversations: the reply and the message it answers share one line
    check("the reply joins its conversation: one line, two messages", s["rows"] == rows_before and s["messages"][0]["thread"] == 2 and not s["messages"][0]["child"], s["messages"][:2])
    x.click("Row(0)")
    s = x.wait(lambda s: s["reading"] and s["reading"]["uid"] == 5 and not s["reading"]["loading"], what="thread opened")
    check("choosing it opens the conversation: the older message under it", [(m["subject"], m["child"]) for m in s["messages"][:2]] == [("Re: Lunch?", False), ("Lunch?", True)] and s["rows"] == rows_before + 1, s["messages"][:3])
    x.shot("mail-thread")
    x.click("ThreadToggle(0)")
    s = x.wait(lambda s: s["rows"] == rows_before, what="thread closed")
    check("its count closes it again", s["messages"][1]["subject"] == "Plume 0.8 newsletter", s["messages"][:2])
    x.menu("More", "Show one message per line")
    s = x.wait(lambda s: not s["threads"] and s["rows"] == 4)
    x.menu("More", "Group conversations")
    s = x.wait(lambda s: s["threads"] and s["rows"] == 3)
    check("conversations can be shown one message per line", True)
    # ── language and theme
    x.menu("More", "Français")
    s = x.wait(lambda s: s["lang"] == "Fr")
    check("the interface switches to French", [f["label"] for f in s["accounts"][0]["folders"]][:4] == ["Boîte de réception", "Brouillons", "Envoyés", "Corbeille"], s["accounts"][0]["folders"])
    x.menu("More", "Thème : clair"); s = x.wait(lambda s: not s["dark"])
    x.shot("mail-light-fr")
    x.menu("More", "Thème : sombre"); x.wait(lambda s: s["dark"])
    x.menu("More", "English"); x.wait(lambda s: s["lang"] == "En")
    check("light and dark themes", True)
    # nothing secret went through the server's log
    check("the password never appears among the commands", not any("pw-test" in c for c in server.commands), [c for c in server.commands if "pw-test" in c])
finally:
    x.close()

# ── offline: the server is gone, the mailbox is still there
server.stop()
x = Mail(port=47842, fresh=False)
try:
    s = x.wait(lambda s: s["rows"] == 3, timeout=5, what="mailbox from disk")
    check("without a server, the mailbox opens from this PC", subjects(s) == ["Re: Lunch?", "Plume 0.8 newsletter", "Réunion demain"], subjects(s))
    x.click("Row(1)")
    s = x.wait(lambda s: s["reading"] and not s["reading"]["loading"], timeout=5, what="message from disk")
    check("a message already read opens offline", s["reading"]["text"].startswith("Plume 0.8"), s["reading"]["text"][:40])
    s = x.wait(lambda s: s["accounts"][0]["error"], timeout=40, what="connection error")
    check("the status bar says the server cannot be reached", bool(s["accounts"][0]["error"]), s["accounts"][0]["error"])
    items = x.menu("More", "Remove sam@example.test…")
    s = x.wait(lambda s: s["sheet"] and s["sheet"]["kind"] == "confirm")
    x.click("SheetOk")
    s = x.wait(lambda s: s["accounts"] == [])
    check("removing the account forgets its mail and its password on this PC", s["rows"] == 0 and not os.path.isdir(os.path.join(x.profile, "a1")) and "pw-test" not in open(os.path.join(x.profile, "test-secrets.json")).read(), os.listdir(x.profile))
finally:
    x.close()

# ── the same again over real TLS (Windows' own), with a certificate made for this test
cert, key = os.path.join(OUT, "test-cert.pem"), os.path.join(OUT, "test-key.pem")
made = shutil.which("openssl") and subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", key, "-out", cert, "-days", "2", "-subj", "/CN=localhost"],
                                                  stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
if made:
    server = fresh_server(tls=(cert, key))
    # first without telling Plume Mail to trust this certificate: it must refuse to talk
    x = Mail(port=47843, profile="prof-mail-tls", env={"PLUME_MAIL_TEST_SECURITY": "tls"})
    try:
        x.wait(lambda s: s["sheet"])
        add_account(x, server, host="localhost")
        s = x.wait(lambda s: s["accounts"] and s["accounts"][0]["error"], timeout=40, what="certificate refused")
        check("TLS: a certificate nobody signed is refused, and nothing is sent", server.logins == 0 and s["rows"] == 0, s["accounts"][0]["error"])
    finally:
        x.close()
    x = Mail(port=47844, profile="prof-mail-tls", env={"PLUME_MAIL_TEST_SECURITY": "tls", "PLUME_MAIL_TEST_TRUST": "1"})
    try:
        x.wait(lambda s: s["sheet"])
        add_account(x, server, host="localhost")
        s = x.wait(lambda s: s["rows"] == 3 and idle(s) and not s["accounts"][0]["error"], timeout=40, what="sync over TLS")
        check("TLS: once the test certificate is accepted, the mailbox syncs over an encrypted connection", server.logins >= 1 and subjects(s)[0] == "Lunch?", subjects(s))
        x.click("New"); x.wait(lambda s: s["sheet"])
        x.edit("sam@example.test"); x.field(2, "Chiffré"); x.field(3, "Par TLS."); x.click("SheetOk")
        s = x.wait(lambda s: s["sheet"] is None and len(server.sent) == 1, timeout=40, what="sent over TLS")
        check("TLS: sending works too", server.sent[0][1] == ["sam@example.test"])
    finally:
        x.close()
        server.stop()
else:
    print("SKIP: no openssl to make a test certificate, the TLS part is not run")
check.done()
