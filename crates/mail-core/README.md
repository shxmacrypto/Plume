# mail-core

The parts of Plume Mail that have no interface, and their tests.

- `net`: the connection to a mail server: TCP, then TLS done by Windows itself (SChannel), with Windows'
  certificate checks.
- `imap`: the IMAP client, just what Plume Mail needs: sign in (password or token), list folders, open one,
  fetch headers and messages, set flags, move, delete, append (drafts), search the text, wait for news (IDLE).
- `smtp`: sending a message.
- `message`: reading a message (through `mail-parser`) and writing one (headers for names with accents,
  threads, attachments); conversations from `Message-ID` and `References`.
- `oauth`: *Sign in with Microsoft*: OAuth 2 with PKCE, the browser coming back to a loopback address, tokens
  renewed on their own.
- `html`: an HTML message turned into readable text. Nothing is downloaded, nothing runs.
- `store`: what is kept on this PC: accounts, signatures, folders, lists, messages already read, the people
  met in them. Passwords and tokens go to Windows' Credential Manager, never to a file.
- `engine`: one worker thread per account, plus a watcher connection waiting for new mail (IDLE). The window
  sends commands and receives news.

```bash
cargo test -p mail-core
```

The tests talk to scripted servers on `127.0.0.1`. One of them writes, reads and deletes a throwaway entry in
the real Credential Manager.
