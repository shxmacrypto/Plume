import email, http.server, json, os, sys, threading, urllib.parse, urllib.request
from email import policy
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mail import *
from fake_server import FakeMail

# Sign in with Microsoft, end to end: the account dialog opens the provider's page (played here by a fake
# provider on 127.0.0.1, and by this script as the browser), Plume Mail gets a token, keeps it encrypted where the
# password would be, and reads and sends mail with it (XOAUTH2). Never a password, never the real Microsoft.


class Provider(http.server.BaseHTTPRequestHandler):
    """A fake sign-in page and token endpoint, checking what a real one checks: the code, PKCE, the state."""
    seen = []

    def log_message(self, *args):
        pass

    def do_GET(self):
        q = dict(urllib.parse.parse_qsl(urllib.parse.urlparse(self.path).query))
        Provider.seen.append(("authorize", q))
        Provider.challenge = q.get("code_challenge")
        target = q["redirect_uri"] + "?" + urllib.parse.urlencode({"code": "good-code", "state": q["state"]})
        self.send_response(302)
        self.send_header("Location", target)
        self.end_headers()

    def do_POST(self):
        body = dict(urllib.parse.parse_qsl(self.rfile.read(int(self.headers["Content-Length"])).decode()))
        Provider.seen.append(("token", body))
        import base64, hashlib
        if body.get("grant_type") == "authorization_code":
            pkce = base64.urlsafe_b64encode(hashlib.sha256(body.get("code_verifier", "").encode()).digest()).rstrip(b"=").decode()
            ok = body.get("code") == "good-code" and pkce == Provider.challenge
            reply = {"access_token": "tok-1", "refresh_token": "ref-1", "expires_in": 3600} if ok else {"error": "invalid_grant", "error_description": "AADSTS70000: bad code"}
        else:
            ok = body.get("refresh_token") == "ref-1"
            reply = {"access_token": "tok-2", "refresh_token": "ref-2", "expires_in": 3600} if ok else {"error": "invalid_grant", "error_description": "AADSTS700082: expired"}
        data = json.dumps(reply).encode()
        self.send_response(200 if ok else 400)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


provider = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
threading.Thread(target=provider.serve_forever, daemon=True).start()
base = "http://127.0.0.1:%d" % provider.server_address[1]

check = Checks()
server = FakeMail(user="sam@hotmail.com", password=None, tokens={"tok-1", "tok-2"})
server.box["INBOX"].add("From: Ann <ann@example.test>\nTo: sam@hotmail.com\nSubject: Hello from Outlook\nDate: Tue, 10 Nov 2026 09:00:00 -0500\nMessage-ID: <o1@example.test>\n\nHi!\n")
x = Mail(port=47851, profile="prof-mail-oauth", env={"PLUME_MAIL_TEST_OAUTH": base})
try:
    s = x.wait(lambda s: s["sheet"], what="account dialog")
    x.field(0, "Sam")
    x.field(1, "sam@hotmail.com")
    s = x.state()
    check("an Outlook address: Sign in with Microsoft instead of a password", s["sheet"]["microsoft"] and s["sheet"]["oauth"] == "idle" and s["sheet"]["fields"][3] == "outlook.office365.com:993", s["sheet"])
    x.shot("mail-microsoft")
    x.field(3, "127.0.0.1:%d" % server.imap_port)
    x.field(4, "127.0.0.1:%d" % server.smtp_port)
    x.click("SheetOk")
    check("saving before signing in is refused", "Sign in with Microsoft first" in (x.state()["sheet"]["error"] or ""), x.state()["sheet"])
    x.click("SignIn")
    s = x.wait(lambda s: s["sheet"]["oauth"] == "waiting", what="sign-in page")
    url = s["sheet"]["oauth_url"]
    q = dict(urllib.parse.parse_qsl(urllib.parse.urlparse(url).query))
    check("the provider's page is asked with PKCE, a state and the address", url.startswith(base + "/authorize?") and q["code_challenge_method"] == "S256" and q["login_hint"] == "sam@hotmail.com" and q["redirect_uri"].startswith("http://localhost:") and len(q["state"]) > 10, q)
    # the browser: the provider sends it back to Plume Mail, which answers with a page
    page = urllib.request.urlopen(url, timeout=10).read().decode()
    check("the browser comes back to Plume Mail, which says it can be closed", "Plume Mail is signed in" in page, page[:200])
    s = x.wait(lambda s: s["sheet"]["oauth"] == "ready", what="signed in")
    check("the code is exchanged for tokens (checked by the provider)", [k for k, _ in Provider.seen] == ["authorize", "token"], Provider.seen)
    x.shot("mail-microsoft-signed-in")
    x.click("SheetOk")
    s = x.wait(lambda s: s["rows"] == 1 and all(a["idle"] for a in s["accounts"]) and not s["accounts"][0]["error"], timeout=20, what="sync with a token")
    check("the mailbox syncs, signed in with a token (XOAUTH2), never a password", s["messages"][0]["subject"] == "Hello from Outlook" and server.logins >= 1 and not any(c.startswith("LOGIN") for c in server.commands), server.commands[:6])
    check("the token was renewed with the lasting one, which is kept", ("token", {"client_id": "plume-test", "grant_type": "refresh_token", "refresh_token": "ref-1", "scope": "imap smtp offline_access"}) in Provider.seen, Provider.seen[-1])
    with open(os.path.join(x.profile, "accounts.json"), encoding="utf-8") as f:
        accounts = f.read()
    with open(os.path.join(x.profile, "test-secrets.json"), encoding="utf-8") as f:
        secrets = f.read()
    check("the account says Microsoft; the token is kept apart (encrypted by Windows outside tests)", '"sign_in": "microsoft"' in accounts and "ref-" not in accounts and "ref-2" in secrets, (accounts, secrets))
    x.click("New"); x.wait(lambda s: s["sheet"] and s["sheet"]["kind"] == "compose")
    x.edit("ann@example.test"); x.field(2, "Sent with a token"); x.field(3, "Hi Ann"); x.click("SheetOk")
    s = x.wait(lambda s: s["sheet"] is None and len(server.sent) == 1, timeout=20, what="sent with a token")
    check("sending works with the token too (SMTP XOAUTH2)", server.sent[0][1] == ["ann@example.test"], server.sent)
    x.menu("More", "Account settings: sam@hotmail.com…")
    s = x.wait(lambda s: s["sheet"] and s["sheet"]["kind"] == "account", what="settings")
    check("its settings show it is signed in, nothing to type", s["sheet"]["microsoft"] and s["sheet"]["oauth"] == "idle", s["sheet"])
    x.click("SheetCancel")
finally:
    x.close()
    server.stop()
    provider.shutdown()
check.done()
