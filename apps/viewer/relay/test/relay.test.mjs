// Relay checks against `npm run dev` (wrangler dev on 127.0.0.1:8787). Run: node test/relay.test.mjs [base]
const base = process.argv[2] || 'http://127.0.0.1:8787';
const wsBase = base.replace(/^http/, 'ws');
let pass = 0, fail = 0;
const ok = (c, name) => { if (c) { pass++; console.log('ok  ', name); } else { fail++; console.log('FAIL', name); } };

function open(path) {
  return new Promise((res) => {
    const ws = new WebSocket(wsBase + path);
    const inbox = [];
    const waiters = [];
    ws.onmessage = (e) => { const m = JSON.parse(e.data); const w = waiters.shift(); w ? w(m) : inbox.push(m); };
    ws.next = (ms = 3000) => new Promise((r, j) => { if (inbox.length) return r(inbox.shift()); const t = setTimeout(() => j(new Error('timeout')), ms); waiters.push((m) => { clearTimeout(t); r(m); }); });
    ws.closed = new Promise((r) => (ws.onclose = (e) => r(e.code)));
    ws.onopen = () => res({ ws, ok: true });
    ws.onerror = () => res({ ws, ok: false });
  });
}
const id = String(100000000 + Math.floor(Math.random() * 899999999));
const token = 'a'.repeat(40) + Math.random().toString(16).slice(2);

const page = await fetch(base + '/');
ok(page.status === 200 && (await page.text()).includes('Plume Viewer'), 'page served');
ok((await fetch(base + '/viewer.js')).status === 200, 'viewer.js served');
ok((await (await fetch(base + '/health')).text()) === 'ok', 'health');
ok((await fetch(base + '/ws/join?id=12')).status === 400, 'bad id refused');

// nobody home
const lone = await open('/ws/join?id=' + id);
ok(lone.ok, 'join opens');
ok((await lone.ws.next()).t === 'offline', 'offline when no PC');

const host = await open('/ws/host?id=' + id + '&token=' + token);
ok(host.ok, 'host claims the ID');
const thief = await open('/ws/host?id=' + id + '&token=' + 'b'.repeat(48));
ok(!thief.ok, 'another token is refused');
host.ws.send('{"t":"ping"}');
ok((await host.ws.next()).t === 'pong', 'ping');

const v = await open('/ws/join?id=' + id);
const join = await host.ws.next();
ok(join.t === 'join' && /^c-[0-9a-f]{16}$/.test(join.from), 'host told about the viewer');
v.ws.send(JSON.stringify({ t: 'hello', nv: '00' }));
const got = await host.ws.next();
ok(got.from === join.from && got.m.t === 'hello', 'viewer -> host');
host.ws.send(JSON.stringify({ to: join.from, m: { t: 'challenge', nh: 'x' } }));
ok((await v.ws.next()).t === 'challenge', 'host -> viewer');
v.ws.close();
const gone = await host.ws.next();
ok(gone.t === 'gone' && gone.from === join.from, 'host told when the viewer leaves');

// the PC closes a viewer
const v2 = await open('/ws/join?id=' + id);
const j2 = await host.ws.next();
host.ws.send(JSON.stringify({ to: j2.from, m: { t: 'refused', why: 'denied' }, close: true }));
ok((await v2.ws.next()).why === 'denied', 'refusal reaches the viewer');
ok((await v2.ws.closed) === 4001, 'viewer closed by the PC');
await host.ws.next().catch(() => {}); // its "gone"

// the PC goes away: viewers are told
const v3 = await open('/ws/join?id=' + id);
await host.ws.next();
host.ws.close();
ok((await v3.ws.next()).t === 'offline', 'viewers told when the PC leaves');
v3.ws.close();

// the same PC comes back with its token
const again = await open('/ws/host?id=' + id + '&token=' + token);
ok(again.ok, 'the PC gets its ID back');
again.ws.close();

console.log(`${pass}/${pass + fail}`);
process.exit(fail ? 1 : 0);
