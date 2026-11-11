<p align="center"><img src="../../assets/brand/plume-viewer-256.png" width="96" alt="Plume Viewer logo"></p>

<h1 align="center">Plume Viewer</h1>

<p align="center"><b>Control this PC from your phone.</b> Scan a QR code, and the screen is yours.</p>

Plume Viewer shows this PC's **ID** (9 digits), a **one-time code** and a **QR code** holding both. Scan it with a
phone's camera, or type the ID and code on the viewer page in any browser: the person at the PC accepts, and the phone
sees the screen and uses the mouse and the keyboard. Nothing to install on the phone.

- **Fast**: only the parts of the screen that change are sent (256 px tiles, JPEG made by Windows), straight from the PC
  to the phone. Each frame is acknowledged, so a slow network gets fewer images instead of a growing delay.
- **Touch made for a PC**: tap to click, long press for a right-click, drag to drag, two fingers to zoom or scroll,
  the phone's keyboard to type (any language, accents included), plus Windows key, Esc and full screen.
- **From a computer too**: in a browser, the mouse, the wheel and the keys work as on the PC itself.
- **Permanent password** (optional): for your own devices, connect without anyone accepting. Plume Viewer then keeps
  running near the clock when its window is closed, and can start with Windows.
- **Control another PC**: *Open the viewer* opens the viewer page; type the other PC's ID and code.

## Security

- The screen, mouse and keys travel **directly between the two devices**, encrypted (WebRTC: DTLS).
- The relay only puts the devices in touch. It never sees the code or the password: the viewer proves it knows them
  by signing its offer (HMAC-SHA-256 with a key derived from the code, or PBKDF2 from the password), and the PC signs
  its answer. The signed offer and answer carry the encryption fingerprints, so nobody in between can take over.
- A one-time code needs **the person at the PC to accept** (60 seconds to answer), and changes after each session.
- **Five wrong codes or passwords lock the PC for 5 minutes.** The relay also limits attempts per ID.
- The permanent password is never stored: only the key derived from it, sealed by Windows for this user (DPAPI).
- While someone is connected, a red banner says who, with a **Disconnect** button. The icon near the clock shows that
  the PC can be reached.

Choose a long password: someone holding the relay could try to guess a weak one from a recorded connection.

## Limits of this version

- The **main screen** is shared (on several screens, the one with the taskbar).
- Windows does not let a program control windows run **as administrator** (UAC prompts, some installers): they are
  visible but do not take clicks, unless Plume Viewer itself runs as administrator.
- Some networks (mobile data with shared addresses, strict company networks) block direct links between devices.
  A TURN relay (Cloudflare offers one) is the next step for those.

## Layout

```
src/main.rs      start, single instance, the icon near the clock, --background, --unregister
src/ui.rs        the window: ID, code, QR, password, accept prompt, session banner
src/gate.rs      who may connect: challenge, signatures, accept, lock after failures
src/relay.rs     the WebSocket to the relay (reconnects by itself)
src/rtc.rs       a session: WebRTC data channels (str0m), STUN, frame pacing
src/capture.rs   the screen: Desktop Duplication (GDI fallback), changed tiles, JPEG
src/input.rs     the viewer's mouse, wheel, keys and text, played with SendInput
src/crypto.rs    keys, signatures, DPAPI
relay/           the Cloudflare Worker and the viewer page (see relay/README.md)
```

## Tests

```
cargo test -p plume-viewer                                      # 12 unit tests
cd relay && npm run dev  &&  node test/relay.test.mjs            # 17 relay checks
cargo build --profile fast -p plume-viewer --features test-hook  # build driven by a test socket
```

With `PLUME_VIEWER_TEST_PORT`, the test build answers JSON commands on 127.0.0.1 (`state`, `click`, `type`, `quit`);
with `PLUME_VIEWER_FAKE_INPUT=<file>` it writes the viewer's input to that file instead of moving the mouse.
