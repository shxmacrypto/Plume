# Plume Mail

A fast, native mail app for Windows 10 and 11, with the look of Plume. Part of Plume Suite: install it from
the Plume Suite app.

- Hand-built interface in Rust: Win32, Direct2D and DirectWrite. No web view. One small program, about
  45 MB of memory.
- Works with standard mail accounts: IMAP to read, SMTP to send, over TLS done by Windows itself.
- **Outlook.com, Hotmail and Microsoft 365**: *Sign in with Microsoft* opens Microsoft's own page in your
  browser. Plume Mail never sees your password; it keeps a sign-in token instead, encrypted by Windows.
- Messages never go through an embedded browser. An HTML message is shown as readable text: no remote image
  is loaded (so nobody learns that you opened it) and nothing runs.
- Your password (or token) is kept by Windows' Credential Manager, encrypted for your Windows session. It is
  never written to a file.
- The mailbox is kept on this PC: folders, lists and the messages you have read open without a connection.
- English and French; dark and light.

## What it does

| | |
|---|---|
| Accounts | Several accounts; the servers of the usual providers are filled in from the address |
| Mailbox | Folders, the 500 most recent messages of a folder, unread and flagged marks, attachments |
| New mail | Arrives by itself within seconds on servers that support it (IMAP IDLE); others are checked every three minutes and with F5 |
| Conversations | Messages that answer each other share one line with a count; choosing it opens the older ones under it |
| Reading | Wrapped text, HTML turned into text, attachments saved where you want, links listed |
| Writing | New message, reply, reply to all, forward, attachments, a signature per account, address suggestions from the people already in your mail, a copy in the Sent folder |
| Drafts | A message you close before sending is kept in the server's Drafts folder; *Edit the draft* reopens it, sending removes it |
| Actions | Read or unread, flag, delete (to the Trash, then for good after a confirmation) |
| Search | Subject and people as you type, then the text of the messages: those kept on this PC, and the server (which also finds older ones) |

## What it does not do

- **Gmail, Yahoo and iCloud** need an *app password* (made in the security settings of the account), not your
  usual password. Google only lets apps sign in to Gmail with its own page after a paid security audit.
- No calendar.
- HTML messages lose their layout and images, on purpose: nothing is downloaded or run.

## Build

Rust (stable, `x86_64-pc-windows-gnu`) with `windres` and `dlltool` on the `PATH`, as for Plume Web.

```bash
cargo build --release -p plume-mail
```

The program is `target/release/plume-mail.exe`. *Sign in with Microsoft* needs Plume Mail's application ID at
Microsoft (a public value, not a secret): set `PLUME_MAIL_MS_CLIENT_ID` when building. Without it, the account
dialog says that this build cannot sign in to Microsoft accounts.

## Use

At the first launch Plume Mail asks for an account: your name, your address, your password (or *Sign in with
Microsoft*). The two server lines are filled in for you and can be corrected (`name:port`). The `…` menu holds
each account's settings and signature, conversations on or off, the theme and the language.

| Keys | Action |
|---|---|
| Up, Down, Home, End | previous, next, first, last message |
| Left, Right | close, open the conversation |
| Space, Page Down, Page Up | scroll the message |
| Ctrl+N | new message |
| Ctrl+R, Ctrl+Shift+R, Ctrl+L | reply, reply to all, forward |
| Ctrl+Enter | send |
| Tab, Shift+Tab | next and previous field of a dialog |
| Up, Down, Enter (in To or Cc) | choose a suggested address |
| Esc (writing a message) | close it; it is kept in Drafts |
| Delete | delete |
| Ctrl+U, Ctrl+D | mark unread or read, flag |
| Ctrl+F, Ctrl+E, Ctrl+K | search the folder |
| F5 | check for new mail |

Data lives in `%LOCALAPPDATA%\Plume\Mail`: `accounts.json` (no password in it), `settings.json`, and one
folder per account with the lists and the messages already read (`.eml` files). Removing an account in the
`…` menu deletes that folder and the saved password or token; nothing changes on the mail server.

## Tests

```bash
cargo test -p mail-core -p plume-mail
```

The end-to-end scenarios drive the real window against a mail server and a sign-in page that live inside the
test: see [tests/ui/README.md](tests/ui/README.md).

## Layout

```
src/app/     the window: state, events, painting, dialogs, settings, strings
tests/ui/    the end-to-end scenarios, their mail server and their fake Microsoft sign-in page
```

The interface kit is [`crates/plume-ui`](../../crates/plume-ui), shared with Plume FEx. Everything that talks
to mail servers, reads and writes messages and keeps the mailbox on disk is in
[`crates/mail-core`](../../crates/mail-core), which has no interface.
