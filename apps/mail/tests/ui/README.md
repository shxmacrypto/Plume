# Plume Mail UI tests

`test_mail.py` drives the real Plume Mail window from end to end: first launch, adding an account, a refused
password, folders, reading (plain and HTML), saving an attachment, search (subject, people, and the text of
messages on the server), reply, reply to all, a new message with an attachment, a signature, address
suggestions, drafts kept on the server and sent from there, flags, delete, new mail pushed by the server,
conversations, languages and themes, reading without a server, removing the account, and the whole connection
again over real TLS.

`test_oauth.py` does *Sign in with Microsoft* from end to end against a fake sign-in page: PKCE, the browser
coming back to Plume Mail, the token exchange and renewal, then reading and sending with the token (XOAUTH2).

It never uses the real mouse or keyboard, and never a real mail account:

- `fake_server.py` is a small mail server (IMAP and SMTP) that runs inside the test, on `127.0.0.1` only, and
  keeps everything in memory. The test looks at what it holds to check what Plume Mail really sent.
- A test build of Plume Mail (Cargo feature `test-hook`) listens on `127.0.0.1` when `PLUME_MAIL_TEST_PORT` is
  set; the script sends it events and reads back its state. In that build:
  - the window opens without taking focus (`PLUME_MAIL_NOACTIVATE`);
  - everything lives in a throwaway folder (`PLUME_MAIL_PROFILE`), **including the passwords**: Windows'
    Credential Manager is not touched;
  - `PLUME_MAIL_TEST_SECURITY` says how to talk to the test server (`plain` or `tls`) whatever its port;
  - Windows' file dialogs do not open: they answer from `PLUME_TEST_SAVE` and `PLUME_TEST_FILES`.
- For the TLS part, `openssl` makes a certificate that nobody signed. Plume Mail must refuse it; then, with
  `PLUME_MAIL_TEST_TRUST` set (test build, `localhost` only), accept it and work over the encrypted connection.
  Without `openssl` on the `PATH`, this part is skipped.

The `test-hook` feature is never part of a release build.

## Run

```bash
cargo build --profile fast -p plume-mail --features test-hook
```

```bash
python apps/mail/tests/ui/test_mail.py
python apps/mail/tests/ui/test_oauth.py
```

Profiles, the test certificate and screenshots go to `target/mail-ui/`.

## Notes

- What this cannot check: the big providers themselves (Gmail, iCloud…) and their sign-in rules.
- Needs Python 3.8 or later and Windows 10 or 11.
