# mail-core

The parts of Plume Mail that have no interface, and their tests.

- `net`: the connection to a mail server: TCP, then TLS done by Windows itself (SChannel), with Windows'
  certificate checks.
- `imap`: the IMAP client, just what Plume Mail needs: sign in, list folders, open one, fetch headers and
  messages, set flags, move, delete, append.
- `smtp`: sending a message.
- `message`: reading a message (through `mail-parser`) and writing one (headers for names with accents,
  threads, attachments).
- `html`: an HTML message turned into readable text. Nothing is downloaded, nothing runs.
- `store`: what is kept on this PC: accounts, folders, lists, messages already read. Passwords go to Windows'
  Credential Manager, never to a file.
- `engine`: one worker thread per account. The window sends it commands and receives news.

```bash
cargo test -p mail-core
```

The tests talk to scripted servers on `127.0.0.1`. One of them writes, reads and deletes a throwaway entry in
the real Credential Manager.
