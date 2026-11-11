# Plume Mail

A fast, native mail app for Windows 10 and 11, with the look of Plume. Part of the Plume suite (not released
yet). This is a first version: it reads and sends mail well, and says clearly what it does not do yet.

- Hand-built interface in Rust: Win32, Direct2D and DirectWrite. No web view. One 1.1 MB program, about
  45 MB of memory.
- Works with standard mail accounts: IMAP to read, SMTP to send, over TLS done by Windows itself.
- Messages never go through an embedded browser. An HTML message is shown as readable text: no remote image
  is loaded (so nobody learns that you opened it) and nothing runs.
- Your password is kept by Windows' Credential Manager, encrypted for your Windows session. It is never
  written to a file.
- The mailbox is kept on this PC: folders, lists and the messages you have read open without a connection.
- English and French; dark and light.

## What it does

| | |
|---|---|
| Accounts | Several accounts; the servers of the usual providers are filled in from the address |
| Mailbox | Folders, the 500 most recent messages of a folder, unread and flagged marks, attachments |
| Reading | Wrapped text, HTML turned into text, attachments saved where you want, links listed |
| Writing | New message, reply, reply to all, forward, attachments, a copy in the Sent folder |
| Actions | Read or unread, flag, delete (to the Trash, then for good after a confirmation) |
| Search | In the open folder, on the subject and the people, ignoring accents and case |

## What it does not do yet

- **Microsoft accounts** (Outlook.com, Hotmail, Microsoft 365) require signing in through Microsoft's own page.
  Plume Mail cannot do that yet and says so when you add such an account.
- **Gmail, Yahoo and iCloud** need an *app password* (made in the security settings of the account), not your
  usual password.
- No calendar, contacts, conversation threads, signatures, or drafts saved on the server.
- HTML messages lose their layout and images.
- New mail is checked every three minutes and when you ask (F5), not pushed.

## Build

Rust (stable, `x86_64-pc-windows-gnu`) with `windres` and `dlltool` on the `PATH`, as for Plume Web.

```bash
cargo build --release -p plume-mail
```

The program is `target/release/plume-mail.exe`.

## Use

At the first launch Plume Mail asks for an account: your name, your address, your password. The two server
lines are filled in for you and can be corrected (`name:port`).

| Keys | Action |
|---|---|
| Up, Down, Home, End | previous, next, first, last message |
| Space, Page Down, Page Up | scroll the message |
| Ctrl+N | new message |
| Ctrl+R, Ctrl+Shift+R, Ctrl+L | reply, reply to all, forward |
| Ctrl+Enter | send |
| Tab, Shift+Tab | next and previous field of a dialog |
| Delete | delete |
| Ctrl+U, Ctrl+D | mark unread or read, flag |
| Ctrl+F, Ctrl+E, Ctrl+K | search the folder |
| F5 | check for new mail |

Data lives in `%LOCALAPPDATA%\Plume\Mail`: `accounts.json` (no password in it), `settings.json`, and one
folder per account with the lists and the messages already read (`.eml` files). Removing an account in the
`…` menu deletes that folder and the saved password; nothing changes on the mail server.

## Tests

```bash
cargo test -p mail-core -p plume-mail
```

The end-to-end scenario drives the real window against a mail server that lives inside the test:
see [tests/ui/README.md](tests/ui/README.md).

## Layout

```
src/app/     the window: state, events, painting, dialogs, settings, strings
tests/ui/    the end-to-end scenario and its mail server
```

The interface kit is [`crates/plume-ui`](../../crates/plume-ui), shared with Plume FEx. Everything that talks
to mail servers, reads and writes messages and keeps the mailbox on disk is in
[`crates/mail-core`](../../crates/mail-core), which has no interface.
