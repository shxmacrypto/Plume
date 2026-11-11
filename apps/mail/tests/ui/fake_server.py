"""A small mail server for the tests of Plume Mail: IMAP to read, SMTP to send, on 127.0.0.1 only.

It keeps everything in memory, understands the commands Plume Mail sends, and can speak TLS with a certificate
nobody signed (the test build of Plume Mail is told to accept it). It runs in threads inside the test, so the
test can look at what the server holds: `server.box["INBOX"]`, `server.sent`.
"""
import base64, re, socket, socketserver, ssl, threading, time


class Message:
    def __init__(self, uid, raw, flags=()):
        self.uid, self.raw, self.flags = uid, raw, set(flags)

    def header(self):
        return self.raw.split(b"\r\n\r\n", 1)[0] + b"\r\n\r\n"


class Mailbox:
    def __init__(self, flags=""):
        self.flags, self.messages, self.next, self.validity = flags, [], 1, 77

    def add(self, raw, flags=()):
        if isinstance(raw, str):
            raw = raw.replace("\r\n", "\n").replace("\n", "\r\n").encode("utf-8")
        m = Message(self.next, raw, flags)
        self.next += 1
        self.messages.append(m)
        return m

    def pick(self, uid_set):
        top = max([m.uid for m in self.messages], default=0)
        wanted = set()
        for part in uid_set.split(","):
            a, _, b = part.partition(":")
            lo = top if a == "*" else int(a)
            hi = lo if not b else (top if b == "*" else int(b))
            wanted.update(range(min(lo, hi), max(lo, hi) + 1))
        return [m for m in self.messages if m.uid in wanted]


class FakeMail:
    def __init__(self, user="sam@example.test", password="pw-test", tls=None):
        self.user, self.password, self.tls = user, password, tls
        self.lock = threading.RLock()
        # the names are the ones the server gives: accents written the IMAP way
        self.box = {"INBOX": Mailbox(), "Sent": Mailbox("\\Sent"), "Trash": Mailbox("\\Trash"), "Work": Mailbox("\\Noselect"), "Work/Projets &AOk-t&AOk-": Mailbox()}
        self.sent = []          # what SMTP received: (from, [to], raw)
        self.commands = []      # every IMAP command, without the passwords
        self.logins = 0
        self.imap = self._serve(self._imap)
        self.smtp = self._serve(self._smtp)
        self.imap_port, self.smtp_port = self.imap.server_address[1], self.smtp.server_address[1]

    def _serve(self, handle):
        outer = self

        class Handler(socketserver.BaseRequestHandler):
            def handle(self):
                sock = self.request
                try:
                    if outer.tls:
                        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
                        ctx.load_cert_chain(*outer.tls)
                        sock = ctx.wrap_socket(sock, server_side=True)
                    handle(sock)
                except (OSError, ssl.SSLError, ValueError):
                    pass
                finally:
                    try:
                        sock.close()
                    except OSError:
                        pass

        class Server(socketserver.ThreadingTCPServer):
            daemon_threads, allow_reuse_address = True, True

        server = Server(("127.0.0.1", 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        return server

    def stop(self):
        for s in (self.imap, self.smtp):
            s.shutdown()
            s.server_close()

    # ───────────────────────── IMAP ─────────────────────────

    def _imap(self, sock):
        f = sock.makefile("rb")
        send = lambda text: sock.sendall(text if isinstance(text, bytes) else text.encode("utf-8"))
        send("* OK fake mail ready\r\n")
        selected, authed = None, False

        def text_arg(rest):
            """A quoted string, or bytes announced by their length: returns (value, what follows)."""
            rest = rest.lstrip()
            m = re.match(r'\{(\d+)\}\s*$', rest)
            if m:
                send("+ go ahead\r\n")
                data = f.read(int(m.group(1)))
                return data.decode("utf-8"), f.readline().decode("utf-8").rstrip("\r\n")
            m = re.match(r'"((?:[^"\\]|\\.)*)"(.*)$', rest)
            if m:
                return re.sub(r'\\(.)', r'\1', m.group(1)), m.group(2)
            word, _, tail = rest.partition(" ")
            return word, tail

        while True:
            line = f.readline()
            if not line:
                return
            line = line.decode("utf-8").rstrip("\r\n")
            tag, _, rest = line.partition(" ")
            cmd, _, args = rest.partition(" ")
            cmd = cmd.upper()
            if cmd == "UID":
                sub, _, args = args.partition(" ")
                cmd = "UID " + sub.upper()
            with self.lock:
                self.commands.append(cmd if cmd in ("LOGIN", "AUTHENTICATE") else rest)
            if cmd == "CAPABILITY":
                send("* CAPABILITY IMAP4rev1 AUTH=PLAIN MOVE UIDPLUS\r\n%s OK done\r\n" % tag)
            elif cmd == "AUTHENTICATE":
                send("+ \r\n")
                parts = base64.b64decode(f.readline().strip()).decode("utf-8").split("\0")
                authed = parts[-2:] == [self.user, self.password]
                with self.lock:
                    self.logins += authed
                send("%s OK logged in\r\n" % tag if authed else "%s NO [AUTHENTICATIONFAILED] Invalid credentials\r\n" % tag)
            elif cmd == "LOGIN":
                user, tail = text_arg(args)
                password, _ = text_arg(tail)
                authed = (user, password) == (self.user, self.password)
                send("%s OK logged in\r\n" % tag if authed else "%s NO [AUTHENTICATIONFAILED] Invalid credentials\r\n" % tag)
            elif cmd == "LOGOUT":
                send("* BYE\r\n%s OK bye\r\n" % tag)
                return
            elif cmd == "NOOP":
                send("%s OK\r\n" % tag)
            elif not authed:
                send("%s NO not logged in\r\n" % tag)
            elif cmd == "LIST":
                with self.lock:
                    for name, box in self.box.items():
                        send('* LIST (%s) "/" "%s"\r\n' % (box.flags, name))
                send("%s OK done\r\n" % tag)
            elif cmd == "SELECT":
                name, _ = text_arg(args)
                with self.lock:
                    box = self.box.get(name)
                    if box is None or "Noselect" in box.flags:
                        send("%s NO [NONEXISTENT] Unknown mailbox\r\n" % tag)
                        continue
                    selected = box
                    send("* %d EXISTS\r\n* OK [UIDVALIDITY %d] ok\r\n* OK [UIDNEXT %d] ok\r\n%s OK [READ-WRITE] done\r\n" % (len(box.messages), box.validity, box.next, tag))
            elif selected is None and cmd != "APPEND":
                send("%s BAD no mailbox selected\r\n" % tag)
            elif cmd == "UID FETCH":
                uid_set, _, items = args.partition(" ")
                out = b""
                with self.lock:
                    for n, m in enumerate(selected.messages, 1):
                        if m not in selected.pick(uid_set):
                            continue
                        flags = " ".join(sorted(m.flags))
                        if "BODY.PEEK[]" in items:
                            out += b"* %d FETCH (UID %d BODY[] {%d}\r\n%s)\r\n" % (n, m.uid, len(m.raw), m.raw)
                        elif "HEADER.FIELDS" in items:
                            h = m.header()
                            out += b"* %d FETCH (UID %d FLAGS (%s) RFC822.SIZE %d INTERNALDATE \"10-Nov-2026 12:00:00 +0000\" BODY[HEADER.FIELDS (X)] {%d}\r\n%s)\r\n" % (
                                n, m.uid, flags.encode(), len(m.raw), len(h), h)
                        else:
                            out += b"* %d FETCH (UID %d FLAGS (%s))\r\n" % (n, m.uid, flags.encode())
                send(out + ("%s OK done\r\n" % tag).encode())
            elif cmd == "UID STORE":
                m = re.match(r"(\S+) ([+-])FLAGS(?:\.SILENT)? \(([^)]*)\)", args)
                with self.lock:
                    for msg in selected.pick(m.group(1)):
                        for flag in m.group(3).split():
                            (msg.flags.add if m.group(2) == "+" else msg.flags.discard)(flag)
                send("%s OK done\r\n" % tag)
            elif cmd in ("UID MOVE", "UID COPY"):
                uid_set, _, tail = args.partition(" ")
                name, _ = text_arg(tail)
                with self.lock:
                    dest = self.box.get(name)
                    if dest is None:
                        send("%s NO [TRYCREATE] Unknown mailbox\r\n" % tag)
                        continue
                    for msg in selected.pick(uid_set):
                        dest.add(msg.raw, msg.flags)
                        if cmd == "UID MOVE":
                            selected.messages.remove(msg)
                send("%s OK done\r\n" % tag)
            elif cmd in ("UID EXPUNGE", "EXPUNGE"):
                with self.lock:
                    only = selected.pick(args) if cmd == "UID EXPUNGE" else selected.messages
                    selected.messages[:] = [m for m in selected.messages if not ("\\Deleted" in m.flags and m in only)]
                send("%s OK done\r\n" % tag)
            elif cmd == "APPEND":
                name, tail = text_arg(args)
                m = re.search(r"\{(\d+)\}\s*$", tail)
                send("+ go ahead\r\n")
                raw = f.read(int(m.group(1)))
                f.readline()
                with self.lock:
                    if name in self.box:
                        self.box[name].add(raw, {"\\Seen"})
                send("%s OK appended\r\n" % tag)
            else:
                send("%s BAD unknown command\r\n" % tag)

    # ───────────────────────── SMTP ─────────────────────────

    def _smtp(self, sock):
        f = sock.makefile("rb")
        send = lambda text: sock.sendall(text.encode("utf-8"))
        send("220 fake mail ready\r\n")
        sender, rcpt, authed = "", [], False
        while True:
            line = f.readline()
            if not line:
                return
            line = line.decode("utf-8").rstrip("\r\n")
            upper = line.upper()
            if upper.startswith("EHLO"):
                send("250-fake\r\n250-AUTH PLAIN LOGIN\r\n250 8BITMIME\r\n")
            elif upper.startswith("AUTH PLAIN"):
                parts = base64.b64decode(line.split(" ", 2)[2]).decode("utf-8").split("\0")
                authed = parts[-2:] == [self.user, self.password]
                send("235 ok\r\n" if authed else "535 5.7.8 Authentication credentials invalid\r\n")
            elif not authed and not upper.startswith("QUIT"):
                send("530 5.7.0 Authentication required\r\n")
            elif upper.startswith("MAIL FROM:"):
                sender, rcpt = line[10:].strip("<> "), []
                send("250 ok\r\n")
            elif upper.startswith("RCPT TO:"):
                address = line[8:].strip("<> ")
                if address.startswith("nobody@"):
                    send("550 5.1.1 No such user here\r\n")
                else:
                    rcpt.append(address)
                    send("250 ok\r\n")
            elif upper == "DATA":
                send("354 go\r\n")
                data = b""
                while True:
                    part = f.readline()
                    if part in (b".\r\n", b""):
                        break
                    data += part[1:] if part.startswith(b"..") else part
                with self.lock:
                    self.sent.append((sender, list(rcpt), data))
                    # mail for our own address lands in the inbox, unread, like on a real server
                    if self.user in rcpt:
                        self.box["INBOX"].add(data)
                send("250 queued\r\n")
            elif upper.startswith("QUIT"):
                send("221 bye\r\n")
                return
            else:
                send("250 ok\r\n")


if __name__ == "__main__":
    server = FakeMail()
    print("IMAP on 127.0.0.1:%d, SMTP on 127.0.0.1:%d, user %s, password %s" % (server.imap_port, server.smtp_port, server.user, server.password))
    while True:
        time.sleep(1)
