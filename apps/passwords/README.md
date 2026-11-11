# Plume Passwords

A password vault for Windows: logins, 2FA codes, secure notes and cards, in one encrypted file on this PC.
A light native app of Plume Suite (Rust, Direct2D), in English and French.

## What it does

- **Vault**: one file, `%LOCALAPPDATA%\Plume\Passwords\vault.ppw`, opened with a master password.
- **Entries**: logins (name, user name, password, website, notes), 2FA codes, secure notes, cards (the number and
  the security code stay hidden until you show them). Favorites, folders, tags, search.
- **2FA codes** (RFC 6238, SHA-1/256/512, 6 to 8 digits): from a secret key, an `otpauth://` address, or a picture
  of the QR code (PNG, JPEG, BMP, GIF; read on this PC). A login can hold its own 2FA key.
- **Generator**: random passwords (length, capital and small letters, digits, symbols, no look-alikes) or
  passphrases (3 to 10 words from a built-in list, separator, capitals, a digit). A strength meter everywhere.
- **Security report**: weak passwords, passwords used for more than one site, logins without 2FA.
- **Copy**: the clipboard is emptied after 30 s (15 s to 2 min), unless you copied something else since. Copied
  secrets are marked so Windows keeps them out of the clipboard history and the cloud clipboard
  (`ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory` = 0,
  `CanUploadToCloudClipboard` = 0).
- **Auto-type**: press Ctrl+Alt+A (or Ctrl+Shift+L, Ctrl+Alt+P) in a sign-in window: the login that fits its title
  is typed there (user name, Tab, password, and Enter if you want). Several fit, or none: Plume Passwords shows the
  list, you choose, and it types. The Auto-type button of a login does the same for the window you used last.
- **Locks itself** after a few minutes without use (1 min to 1 h, or never), when Windows locks, and from the icon
  near the clock.
- **Windows Hello**: unlock with face, fingerprint or PIN instead of the master password.
- **Import** the CSV files of Chrome, Edge, Brave, Opera, Firefox, Bitwarden and Plume Web (columns found by name;
  logins already in the vault are skipped). **Export** an encrypted backup; **restore** one (it adds what is
  missing, newer versions win).
- Keeps running near the clock when its window is closed (so auto-type keeps working), and can start with Windows.

```
plume-passwords.exe                 the window
plume-passwords.exe --background    only the icon near the clock (start with Windows)
plume-passwords.exe --unregister    no more start with Windows; a running Plume Passwords quits (uninstall)
```

## Security model

- **Key derivation**: Argon2id, 64 MiB, 3 passes, 1 lane, 16-byte random salt. The parameters are stored in the
  file, so they can be raised later.
- **Two keys**: a random 256-bit data key encrypts the entries (AES-256-GCM, a new 96-bit nonce at each save). The
  key derived from the master password encrypts the data key (AES-256-GCM). Changing the master password makes a
  new salt and a new data key, and the vault is sealed again.
- **File**: `PPW1` | Argon2 parameters | salt | key nonce | sealed data key | data nonce | sealed JSON. The first 92
  bytes are authenticated with both seals: a changed parameter, a flipped byte or a cut file is refused, never
  read with weaker settings. A damaged file cannot make the app allocate unbounded memory.
- **Writes** go to a temporary file first, which then replaces the vault in one step.
- **In memory**: keys and decrypted text are wiped when the vault locks or the app quits (`zeroize`); the native
  text field is emptied when it closes. Secrets are never written to logs.
- **Windows Hello**: when turned on (Hello must confirm it), a copy of the data key is sealed by Windows for this
  user account only (DPAPI, with an app-specific entropy) in `hello.bin`; unlocking asks Windows Hello
  (`UserConsentVerifier`) before that copy is unsealed. Note the limit: DPAPI protects against other accounts and
  other PCs, not against a program already running as you. Turning Hello off overwrites and deletes `hello.bin`.
- **Backups** are vault files: they open only with the master password they were made with.
- **What goes online: nothing.** Plume Passwords makes no network connection. (The test socket below exists only
  in test builds, on 127.0.0.1.)

## Files

`%LOCALAPPDATA%\Plume\Passwords\`: `vault.ppw` (the vault), `hello.bin` (only with Windows Hello on),
`settings.json` (nothing secret: lock delay, shortcut, generator options, theme, language).

## Code

| File | What |
| --- | --- |
| `src/vault.rs` | entries, file format, Argon2id + AES-256-GCM, merge of backups |
| `src/totp.rs` | base32, `otpauth://` addresses, RFC 6238 codes |
| `src/qr.rs` | picture decoding (Windows Imaging Component) + QR reading (`rqrr`) |
| `src/generator.rs` | password and passphrase generator, word list, strength meter |
| `src/csv.rs` | CSV import |
| `src/clip.rs` | copying with the history/cloud marks, clearing |
| `src/autotype.rs` | typing into the window in front (`SendInput`, Unicode), matching a window title |
| `src/hello.rs` | DPAPI and Windows Hello |
| `src/sys.rs` | icon near the clock, start with Windows, global shortcut, Windows lock notice |
| `src/ui/` | the window: lock screen, list, details, form, generator, report, settings |
| `src/i18n.rs` | French texts |
| `src/test_hook.rs` | test socket (feature `test-hook` only) |

## Tests

```
cargo test -p plume-passwords
```

Vault round trip, wrong password, tampered or cut files, change of master password, data-key unlock, backup
merge; TOTP against the RFC 6238 test vectors (SHA-1, SHA-256, SHA-512) and `otpauth://` parsing; a QR code read
from a picture file; CSV files of Chrome/Edge, Firefox and Bitwarden; the generator's options and the word list;
the strength meter; DPAPI round trip; auto-type matching; every interface text has its French.

The real clipboard is tested only when asked (it saves your clipboard and puts it back exactly):

```
PLUME_PASSWORDS_CLIPBOARD_TEST=1 cargo test -p plume-passwords clip::
```

**Test builds** (`cargo build --profile fast -p plume-passwords --features test-hook`) are driven through a JSON
socket on 127.0.0.1 (`src/test_hook.rs` lists the commands; answers show secrets only as lengths). Environment:

| Variable | Effect |
| --- | --- |
| `PLUME_PASSWORDS_DIR` | data folder (vault, settings, hello.bin) instead of `%LOCALAPPDATA%\Plume\Passwords` |
| `PLUME_PASSWORDS_TEST_PORT` | opens the test socket; no global shortcut is registered; a second instance may run |
| `PLUME_PASSWORDS_NOACTIVATE` | the window never takes the focus |
| `PLUME_PASSWORDS_FAKE_INPUT` | auto-type writes what it would type to this file (lengths only), types nothing |
| `PLUME_PASSWORDS_FAKE_CLIPBOARD` | copying is only noted, the clipboard is not touched |
| `PLUME_PASSWORDS_FAKE_HELLO` | `yes`/`no`: Windows Hello's answer, without asking |
| `PLUME_PASSWORDS_RUN_KEY` | registry key used for "Start with Windows" |
| `PLUME_TEST_FILES`, `PLUME_TEST_SAVE` | what the open and save dialogs answer (plume-ui) |
| `PLUME_SUITE_LANG`, `PLUME_SUITE_THEME` | language and theme |
