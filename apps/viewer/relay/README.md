# Plume Viewer relay

A Cloudflare Worker that does two small jobs:

1. It serves the **viewer page** (`public/`): the page a phone opens when it scans a PC's QR code.
2. It **puts devices in touch**. Each PC running Plume Viewer keeps a WebSocket open on `/ws/host?id=<its ID>`; a viewer
   opens `/ws/join?id=<ID>`, and one Durable Object per ID passes their messages along until the two devices are linked.

The screen, the mouse and the keyboard never go through it: they travel directly from device to device (WebRTC,
encrypted). The relay never learns a one-time code or a password either. The viewer proves it knows one by signing its
offer with a key derived from it, and the PC signs its answer the same way.

## Deploy (free plan)

1. Create a free account on [dash.cloudflare.com](https://dash.cloudflare.com/sign-up). In *Workers & Pages*, choose
   your `workers.dev` subdomain when asked.
2. In this folder:
   ```
   npm install
   npx wrangler login
   npx wrangler deploy
   ```
   `wrangler login` opens the browser to allow it once. `deploy` prints the address, for example
   `https://plume-viewer.<your-subdomain>.workers.dev`.
3. Build Plume Viewer with that address: set `PLUME_VIEWER_RELAY_URL` before `.\build.ps1`, or change the default
   `RELAY` in `apps/viewer/src/store.rs`. One PC can also use another relay through `"relay"` in
   `%LOCALAPPDATA%\Plume\Viewer\viewer.json`.

The free plan is enough: the relay only carries a few small messages per connection, and the Durable Objects use
SQLite storage, which the free plan includes.

## What stays private

- Nothing secret is in this folder: no account ID, no key. `wrangler login` keeps its sign-in in your Windows profile,
  outside the repository, and `.wrangler/` (local build files) is ignored by git.
- Anyone can read the relay's address (it is compiled into Plume Viewer, like a website's address). Choose a
  `workers.dev` subdomain that does not give your name.
- The free plan never sends a bill: past its daily limits, the relay stops answering until the next day.
- Anyone can run their own relay with the three commands above, and point their Plume Viewer to it with `"relay"` in
  `viewer.json`.

## Test locally

```
npm run dev                      # the relay and the page on http://127.0.0.1:8787
node test/relay.test.mjs         # 17 checks: page, IDs, forwarding, refusals, disconnections
```

A test build of Plume Viewer (`cargo build --profile fast -p plume-viewer --features test-hook`) talks to this local
relay when `PLUME_VIEWER_RELAY=http://127.0.0.1:8787` is set.

## Protocol

PC → relay: `{to, m, close?}` sends `m` to one viewer (and closes it); `{"t":"ping"}` gets `{"t":"pong"}`.
Relay → PC: `{t:"join", from}`, `{from, m}` (a viewer's message), `{t:"gone", from}`.
Relay → viewer: what the PC sent, or `{t:"offline"}` when no PC holds that ID.

The first PC to claim an ID keeps it: the room stores a hash of its token and refuses any other (403). Joins are
limited to 20 a minute per ID, and the PC itself locks for 5 minutes after 5 wrong codes or passwords.
