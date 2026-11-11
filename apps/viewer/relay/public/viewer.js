/* Plume Viewer, the viewer side (phone or browser).
 *
 * 1. The relay (this site) puts us in touch with the PC whose ID was given.
 * 2. We prove to the PC that we know its one-time code or its password, without sending it: both sides derive a key
 *    from it and sign the WebRTC offer and answer with it (HMAC-SHA256). The relay sees the signatures, never the key.
 * 3. WebRTC then links us directly (encrypted with DTLS, whose fingerprints are in the signed offer and answer).
 *    The PC sends its screen as JPEG tiles of what changed; we send the mouse, the touches and the keyboard.
 */
'use strict';
const $ = (id) => document.getElementById(id);
const FR = (navigator.language || '').toLowerCase().startsWith('fr');
const T = {
  sub: ['See and control a PC from this device.', 'Voir et contrôler un PC depuis cet appareil.'],
  id: ['PC ID', 'ID du PC'],
  secret: ['Code or password', 'Code ou mot de passe'],
  connect: ['Connect', 'Se connecter'],
  fine: ['End-to-end encrypted. The person at the PC accepts each connection, unless you use its permanent password.',
    'Chiffré de bout en bout. La personne devant le PC accepte chaque connexion, sauf avec son mot de passe permanent.'],
  connecting: ['Connecting to {id}…', 'Connexion à {id}…'],
  offline: ['This PC is not reachable: Plume Viewer is not open on it, or it has no internet.', 'Ce PC est injoignable : Plume Viewer n\'y est pas ouvert, ou il n\'a pas internet.'],
  wait: ['Waiting for the person at the PC to accept…', 'En attente de l\'accord de la personne devant le PC…'],
  wrong: ['Wrong code or password.', 'Code ou mot de passe incorrect.'],
  denied: ['The person at the PC refused the connection.', 'La personne devant le PC a refusé la connexion.'],
  busy: ['This PC is already controlled by someone else.', 'Ce PC est déjà contrôlé par quelqu\'un d\'autre.'],
  locked: ['Too many wrong attempts: try again in a few minutes.', 'Trop d\'essais incorrects : réessayez dans quelques minutes.'],
  noPassword: ['This PC has no permanent password: use its one-time code.', 'Ce PC n\'a pas de mot de passe permanent : utilisez son code à usage unique.'],
  badId: ['An ID has 9 digits.', 'Un ID compte 9 chiffres.'],
  linking: ['Linking the two devices…', 'Liaison des deux appareils…'],
  noLink: ['The two devices could not reach each other directly (a strict network). Try on the same Wi-Fi.', 'Les deux appareils n\'ont pas pu se joindre directement (réseau strict). Essayez sur le même Wi-Fi.'],
  ended: ['The session ended.', 'La session est terminée.'],
  forged: ['The answer was not signed by this PC: connection stopped.', 'La réponse n\'était pas signée par ce PC : connexion arrêtée.'],
  relay: ['The relay cannot be reached.', 'Le relais est injoignable.'],
  t_kbd: ['Keyboard', 'Clavier'], t_win: ['Windows key', 'Touche Windows'], t_rclick: ['Next tap is a right-click', 'Le prochain appui est un clic droit'],
  t_fs: ['Full screen', 'Plein écran'], t_quit: ['Disconnect', 'Se déconnecter'],
};
const t = (k, v = {}) => T[k][FR ? 1 : 0].replace(/\{(\w+)\}/g, (_, n) => v[n] ?? '');
document.documentElement.lang = FR ? 'fr' : 'en';
document.querySelectorAll('[data-i]').forEach((e) => (e.textContent = t(e.dataset.i)));
document.querySelectorAll('[data-i-title]').forEach((e) => (e.title = e.ariaLabel = t(e.dataset.iTitle)));

/* ---------- small crypto helpers (WebCrypto) ---------- */
const enc = new TextEncoder();
const hex = (buf) => [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, '0')).join('');
const unhex = (h) => new Uint8Array((h.match(/../g) || []).map((x) => parseInt(x, 16)));
const rand = (n) => hex(crypto.getRandomValues(new Uint8Array(n)));
async function keyFor(mode, id, secret, saltHex) {
  let raw;
  if (mode === 'password') {
    const base = await crypto.subtle.importKey('raw', enc.encode(secret), 'PBKDF2', false, ['deriveBits']);
    raw = await crypto.subtle.deriveBits({ name: 'PBKDF2', hash: 'SHA-256', salt: unhex(saltHex), iterations: 100000 }, base, 256);
  } else {
    raw = await crypto.subtle.digest('SHA-256', enc.encode('plume-viewer code\n' + id + '\n' + secret.toLowerCase().replace(/[\s-]/g, '')));
  }
  return crypto.subtle.importKey('raw', raw, { name: 'HMAC', hash: 'SHA-256' }, false, ['sign', 'verify']);
}
const mac = async (key, text) => hex(await crypto.subtle.sign('HMAC', key, enc.encode(text)));

/* ---------- the form ---------- */
const digits = (s) => s.replace(/\D/g, '').slice(0, 9);
const pretty = (d) => d.replace(/(\d{3})(?=\d)/g, '$1 ');
$('pcid').addEventListener('input', (e) => (e.target.value = pretty(digits(e.target.value))));
function say(text, err) { $('status').textContent = text; $('status').classList.toggle('err', !!err); }
$('form').addEventListener('submit', (e) => {
  e.preventDefault();
  const id = digits($('pcid').value);
  if (id.length !== 9) return say(t('badId'), true);
  connect(id, $('secret').value.trim());
});

/* ---------- the session ---------- */
let S = null; // { ws, pc, screen, input, key, ... }
const PC_CONFIG = { iceServers: [{ urls: ['stun:stun.cloudflare.com:3478', 'stun:stun.l.google.com:19302'] }] };

function deviceName() {
  const ua = navigator.userAgent;
  const os = /iPhone/.test(ua) ? 'iPhone' : /iPad/.test(ua) ? 'iPad' : /Android/.test(ua) ? 'Android' : /Windows/.test(ua) ? 'Windows' : /Mac OS/.test(ua) ? 'Mac' : /Linux/.test(ua) ? 'Linux' : '?';
  const br = /Edg\//.test(ua) ? 'Edge' : /Plume/.test(ua) ? 'Plume' : /Firefox\//.test(ua) ? 'Firefox' : /Chrome\//.test(ua) ? 'Chrome' : /Safari\//.test(ua) ? 'Safari' : 'Browser';
  return os + ' · ' + br;
}

function connect(id, secret) {
  if (S) return;
  $('go').disabled = true;
  say(t('connecting', { id: pretty(id) }));
  const mode = /^[a-z0-9]{8}$/i.test(secret.replace(/[\s-]/g, '')) && !/[A-Z]/.test(secret) ? 'code' : 'password';
  const nv = rand(16);
  const ws = new WebSocket((location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host + '/ws/join?id=' + id);
  S = { id, ws, nv, secret, mode, tries: [mode, mode === 'code' ? 'password' : null].filter(Boolean) };
  ws.onerror = () => fail(t('relay'));
  ws.onclose = () => { if (S && S.ws === ws && !S.linked) fail(S.lastErr || t('relay')); };
  ws.onopen = () => ws.send(JSON.stringify({ t: 'hello', name: deviceName(), nv }));
  ws.onmessage = async (ev) => {
    let m;
    try { m = JSON.parse(ev.data); } catch { return; }
    try { await onSignal(m); } catch (e) { fail(String(e && e.message || e)); }
  };
}

function fail(text) {
  const s = S;
  S = null;
  if (s) {
    try { s.ws.close(); } catch {}
    try { s.pc && s.pc.close(); } catch {}
  }
  $('session').hidden = true;
  $('connect').hidden = false;
  $('go').disabled = false;
  say(text, true);
}

async function onSignal(m) {
  if (!S) return;
  if (m.t === 'offline') return (S.lastErr = t('offline'), fail(t('offline')));
  if (m.t === 'challenge') {
    S.nh = m.nh; S.salt = m.salt; S.modes = m.modes || ['code'];
    // a code-shaped secret is tried as the code first; a PC without permanent password only takes codes
    let mode = S.mode;
    if (!S.modes.includes(mode)) mode = S.modes.includes('code') ? 'code' : 'password';
    if (mode === 'password' && !S.modes.includes('password')) return fail(t('noPassword'));
    S.mode = mode;
    S.key = await keyFor(mode, S.id, S.secret, S.salt);
    return offer();
  }
  if (m.t === 'wait') return showWait(t('wait'));
  if (m.t === 'refused') return fail(t(m.why in T ? m.why : 'denied'));
  if (m.t === 'answer') {
    const ok = m.mac === (await mac(S.key, 'answer\n' + S.nv + '\n' + S.nh + '\n' + m.sdp));
    if (!ok) return fail(t('forged'));
    showWait(t('linking'));
    await S.pc.setRemoteDescription({ type: 'answer', sdp: m.sdp });
    S.linkTimer = setTimeout(() => { if (S && !S.linked) fail(t('noLink')); }, 15000);
  }
}

async function offer() {
  const pc = new RTCPeerConnection(PC_CONFIG);
  S.pc = pc;
  S.screen = pc.createDataChannel('screen', { ordered: true });
  S.input = pc.createDataChannel('input', { ordered: true });
  S.screen.binaryType = 'arraybuffer';
  S.screen.onmessage = (e) => onScreen(e.data);
  S.input.onopen = () => started();
  // the PC ended the session (Disconnect, Plume Viewer closed)
  S.input.onclose = S.screen.onclose = () => { if (S && S.pc === pc) fail(t('ended')); };
  pc.onconnectionstatechange = () => {
    if (!S || S.pc !== pc) return;
    if (pc.connectionState === 'failed') fail(S.linked ? t('ended') : t('noLink'));
    if (pc.connectionState === 'closed' && S.linked) fail(t('ended'));
  };
  await pc.setLocalDescription(await pc.createOffer());
  // the whole offer at once (with our addresses): the PC answers with all of its own
  await new Promise((done) => {
    if (pc.iceGatheringState === 'complete') return done();
    const tm = setTimeout(done, 2500);
    pc.onicegatheringstatechange = () => { if (pc.iceGatheringState === 'complete') { clearTimeout(tm); done(); } };
  });
  const sdp = pc.localDescription.sdp;
  S.ws.send(JSON.stringify({ t: 'offer', mode: S.mode, sdp, mac: await mac(S.key, 'offer\n' + S.nh + '\n' + S.nv + '\n' + sdp) }));
}

function showWait(text) {
  $('connect').hidden = true;
  $('session').hidden = false;
  $('wait').hidden = false;
  $('waittext').textContent = text;
}

function started() {
  S.linked = true;
  clearTimeout(S.linkTimer);
  $('wait').hidden = true;
  try { S.ws.close(); } catch {} // the relay is no longer needed
  layout();
  $('screen').focus();
}

/* ---------- the screen: JPEG tiles of what changed, then "frame done" ---------- */
const view = $('screen');
const ctx = view.getContext('2d');
const fb = document.createElement('canvas'); // the remote screen, full size
const fctx = fb.getContext('2d');
let pending = []; // tile decodings of the frame being received
const remote = { w: 0, h: 0, cx: -1, cy: -1, cursor: false };
const camera = { zoom: 1, x: 0, y: 0 }; // zoom on the phone: a part of the fitted picture

function onScreen(buf) {
  const d = new DataView(buf);
  const kind = d.getUint8(0);
  if (kind === 1) {
    const x = d.getUint16(5, true), y = d.getUint16(7, true);
    const blob = new Blob([new Uint8Array(buf, 13)], { type: 'image/jpeg' });
    pending.push(createImageBitmap(blob).then((img) => ({ img, x, y })).catch(() => null));
  } else if (kind === 2) {
    const frame = d.getUint32(1, true);
    const w = d.getUint16(5, true), h = d.getUint16(7, true);
    remote.cx = d.getInt16(9, true); remote.cy = d.getInt16(11, true); remote.cursor = !!d.getUint8(13);
    const tiles = pending;
    pending = [];
    Promise.all(tiles).then((list) => {
      if (fb.width !== w || fb.height !== h) { fb.width = w; fb.height = h; remote.w = w; remote.h = h; layout(); }
      for (const tile of list) if (tile) { fctx.drawImage(tile.img, tile.x, tile.y); tile.img.close(); }
      draw();
      if (S && S.input && S.input.readyState === 'open') S.input.send(JSON.stringify({ t: 'ack', f: frame }));
    });
  } else if (kind === 4) {
    fail(t('ended')); // the PC ended the session
  } else if (kind === 3) {
    remote.cx = d.getInt16(1, true); remote.cy = d.getInt16(3, true); remote.cursor = !!d.getUint8(5);
    draw();
  }
}

// fit the remote screen in the window (letterbox), then the phone's zoom
let fit = { s: 1, ox: 0, oy: 0 };
function layout() {
  const dpr = window.devicePixelRatio || 1;
  view.width = Math.round(view.clientWidth * dpr);
  view.height = Math.round(view.clientHeight * dpr);
  if (remote.w) {
    const s = Math.min(view.width / remote.w, view.height / remote.h);
    fit = { s, ox: (view.width - remote.w * s) / 2, oy: (view.height - remote.h * s) / 2 };
  }
  draw();
}
window.addEventListener('resize', layout);

function transform() {
  const s = fit.s * camera.zoom;
  return { s, ox: fit.ox * camera.zoom - camera.x, oy: fit.oy * camera.zoom - camera.y };
}

// the mouse of this computer, over the screen (a phone has none)
let mouseOver = false, lastMouse = 0;
setInterval(() => { if (mouseOver && remote.cursor && performance.now() - lastMouse > 1500 && performance.now() - lastMouse < 1700) draw(); }, 200);

function draw() {
  ctx.fillStyle = '#000';
  ctx.fillRect(0, 0, view.width, view.height);
  if (!remote.w) return;
  const { s, ox, oy } = transform();
  ctx.imageSmoothingQuality = 'high';
  ctx.drawImage(fb, ox, oy, remote.w * s, remote.h * s);
  // with a mouse, your own pointer shows; the PC's one is drawn when yours rests or is elsewhere
  const ownPointer = mouseOver && performance.now() - lastMouse < 1500;
  if (remote.cursor && remote.cx >= 0 && !ownPointer) drawCursor(ox + remote.cx * s, oy + remote.cy * s, Math.max(1, s * 1.1) * (window.devicePixelRatio || 1) * 0.9);
}

function drawCursor(x, y, k) {
  k = Math.min(Math.max(k, 0.9), 2.2);
  ctx.save();
  ctx.translate(x, y);
  ctx.scale(k, k);
  ctx.beginPath();
  ctx.moveTo(0, 0); ctx.lineTo(0, 17); ctx.lineTo(4.2, 13); ctx.lineTo(7.2, 19.5); ctx.lineTo(10, 18.3); ctx.lineTo(7.1, 12); ctx.lineTo(12.5, 12); ctx.closePath();
  ctx.fillStyle = '#fff'; ctx.strokeStyle = '#000'; ctx.lineWidth = 1.2;
  ctx.fill(); ctx.stroke();
  ctx.restore();
}

// a point of the window -> a pixel of the remote screen
function toRemote(clientX, clientY) {
  const r = view.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const { s, ox, oy } = transform();
  const x = Math.round(((clientX - r.left) * dpr - ox) / s), y = Math.round(((clientY - r.top) * dpr - oy) / s);
  return { x: Math.max(0, Math.min(remote.w - 1, x)), y: Math.max(0, Math.min(remote.h - 1, y)) };
}

const send = (m) => { if (S && S.input && S.input.readyState === 'open') S.input.send(JSON.stringify(m)); };

/* ---------- mouse (PC) ---------- */
let rightNext = false;
view.addEventListener('mousemove', (e) => { mouseOver = true; lastMouse = performance.now(); const p = toRemote(e.clientX, e.clientY); send({ t: 'mm', x: p.x, y: p.y }); });
view.addEventListener('mouseleave', () => { mouseOver = false; draw(); });
view.addEventListener('mousedown', (e) => {
  e.preventDefault(); view.focus();
  const p = toRemote(e.clientX, e.clientY);
  send({ t: 'mb', b: e.button === 2 ? 2 : e.button === 1 ? 1 : 0, d: true, x: p.x, y: p.y });
});
view.addEventListener('mouseup', (e) => { const p = toRemote(e.clientX, e.clientY); send({ t: 'mb', b: e.button === 2 ? 2 : e.button === 1 ? 1 : 0, d: false, x: p.x, y: p.y }); });
view.addEventListener('contextmenu', (e) => e.preventDefault());
view.addEventListener('wheel', (e) => { e.preventDefault(); send({ t: 'wh', dx: Math.round(-e.deltaX), dy: Math.round(-e.deltaY) }); }, { passive: false });

/* ---------- keyboard (PC, and the phone's keyboard through the hidden field) ---------- */
const PASS = new Set(['F5', 'F11', 'F12']); // left to this browser
document.addEventListener('keydown', (e) => {
  if (!S || !S.linked || e.target === $('kbd')) return;
  if (PASS.has(e.code) && !e.ctrlKey) return;
  e.preventDefault();
  send({ t: 'kd', c: e.code, k: e.key });
});
document.addEventListener('keyup', (e) => {
  if (!S || !S.linked || e.target === $('kbd')) return;
  if (PASS.has(e.code) && !e.ctrlKey) return;
  e.preventDefault();
  send({ t: 'ku', c: e.code, k: e.key });
});
const kbd = $('kbd');
kbd.addEventListener('input', () => { if (kbd.value) { send({ t: 'tx', s: kbd.value }); kbd.value = ''; } });
kbd.addEventListener('keydown', (e) => {
  const special = { Backspace: 'Backspace', Enter: 'Enter', Tab: 'Tab', Escape: 'Escape', ArrowLeft: 'ArrowLeft', ArrowRight: 'ArrowRight', ArrowUp: 'ArrowUp', ArrowDown: 'ArrowDown', Delete: 'Delete' };
  if (special[e.key]) { e.preventDefault(); send({ t: 'kd', c: special[e.key], k: e.key }); send({ t: 'ku', c: special[e.key], k: e.key }); }
});

/* ---------- touch (phone): tap = click, long press = right click, drag = drag, two fingers = zoom / scroll ---------- */
const touch = { start: null, moved: false, long: null, down: false, pinch: null };
view.addEventListener('touchstart', (e) => {
  e.preventDefault();
  if (e.touches.length === 1) {
    const p = e.touches[0];
    touch.start = { x: p.clientX, y: p.clientY, t: Date.now() };
    touch.moved = false; touch.down = false;
    clearTimeout(touch.long);
    touch.long = setTimeout(() => {
      if (touch.moved || !touch.start) return;
      const r = toRemote(touch.start.x, touch.start.y);
      send({ t: 'mm', x: r.x, y: r.y });
      send({ t: 'mb', b: 2, d: true, x: r.x, y: r.y }); send({ t: 'mb', b: 2, d: false, x: r.x, y: r.y });
      touch.start = null;
      if (navigator.vibrate) navigator.vibrate(15);
    }, 550);
  } else if (e.touches.length === 2) {
    clearTimeout(touch.long);
    if (touch.down) { const r = toRemote(touch.start.x, touch.start.y); send({ t: 'mb', b: 0, d: false, x: r.x, y: r.y }); touch.down = false; }
    touch.start = null;
    const [a, b] = e.touches;
    touch.pinch = { d: Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY), mx: (a.clientX + b.clientX) / 2, my: (a.clientY + b.clientY) / 2, zoom: camera.zoom, cam: { ...camera }, scrolled: 0 };
  }
}, { passive: false });
view.addEventListener('touchmove', (e) => {
  e.preventDefault();
  if (e.touches.length === 1 && touch.start) {
    const p = e.touches[0];
    if (!touch.moved && Math.hypot(p.clientX - touch.start.x, p.clientY - touch.start.y) < 8) return;
    clearTimeout(touch.long);
    const r0 = toRemote(touch.start.x, touch.start.y);
    if (!touch.moved) { touch.moved = true; send({ t: 'mm', x: r0.x, y: r0.y }); send({ t: 'mb', b: 0, d: true, x: r0.x, y: r0.y }); touch.down = true; }
    const r = toRemote(p.clientX, p.clientY);
    send({ t: 'mm', x: r.x, y: r.y });
  } else if (e.touches.length === 2 && touch.pinch) {
    const [a, b] = e.touches;
    const dpr = window.devicePixelRatio || 1;
    const d = Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
    const mx = (a.clientX + b.clientX) / 2, my = (a.clientY + b.clientY) / 2;
    const P = touch.pinch;
    const ratio = d / P.d;
    if (Math.abs(ratio - 1) > 0.08 || camera.zoom > 1.01) {
      // zoom around the middle of the fingers, and pan with them
      const z = Math.max(1, Math.min(6, P.zoom * ratio));
      const r = view.getBoundingClientRect();
      const ax = (P.mx - r.left) * dpr, ay = (P.my - r.top) * dpr;
      const wx = (ax + P.cam.x) / P.zoom, wy = (ay + P.cam.y) / P.zoom; // the point under the fingers, unzoomed
      camera.zoom = z;
      camera.x = wx * z - (mx - r.left) * dpr;
      camera.y = wy * z - (my - r.top) * dpr;
      if (z <= 1.001) { camera.x = 0; camera.y = 0; }
      draw();
    } else {
      // two fingers up or down: the mouse wheel
      const dy = my - P.my - P.scrolled;
      if (Math.abs(dy) >= 12) { send({ t: 'wh', dx: 0, dy: Math.round(dy * 6) }); P.scrolled += dy; }
    }
  }
}, { passive: false });
view.addEventListener('touchend', (e) => {
  e.preventDefault();
  clearTimeout(touch.long);
  if (e.touches.length === 0) {
    if (touch.start && !touch.moved) {
      const r = toRemote(touch.start.x, touch.start.y);
      const b = rightNext ? 2 : 0;
      send({ t: 'mm', x: r.x, y: r.y });
      send({ t: 'mb', b, d: true, x: r.x, y: r.y }); send({ t: 'mb', b, d: false, x: r.x, y: r.y });
      if (rightNext) { rightNext = false; document.querySelector('[data-a=rclick]').classList.remove('on'); }
    } else if (touch.down && touch.start) {
      const p = e.changedTouches[0];
      const r = toRemote(p.clientX, p.clientY);
      send({ t: 'mb', b: 0, d: false, x: r.x, y: r.y });
    }
    touch.start = null; touch.down = false; touch.pinch = null;
  }
}, { passive: false });

/* ---------- the bar ---------- */
$('bar').addEventListener('click', (e) => {
  const b = e.target.closest('button[data-a]');
  if (!b) return;
  const a = b.dataset.a;
  if (a === 'kbd') { kbd.focus(); kbd.click(); }
  if (a === 'win') send({ t: 'combo', k: 'win' });
  if (a === 'esc') { send({ t: 'kd', c: 'Escape', k: 'Escape' }); send({ t: 'ku', c: 'Escape', k: 'Escape' }); }
  if (a === 'rclick') { rightNext = !rightNext; b.classList.toggle('on', rightNext); }
  if (a === 'fs') { if (document.fullscreenElement) document.exitFullscreen(); else document.documentElement.requestFullscreen?.().catch(() => {}); }
  if (a === 'quit') { send({ t: 'bye' }); fail(t('ended')); }
});
// the bar fades while the pointer is on the screen (PC)
let barTimer = 0;
view.addEventListener('mousemove', () => { $('bar').classList.add('dim'); clearTimeout(barTimer); barTimer = setTimeout(() => $('bar').classList.remove('dim'), 1500); });

// tests: the state of the page
window.__viewer = () => ({ linked: !!(S && S.linked), w: remote.w, h: remote.h, status: $('status').textContent, waiting: !$('wait').hidden, wait: $('waittext').textContent });

// the QR code of the PC: #<ID>.<code>  (after #, so it never reaches the server)
(() => {
  const m = /^#(\d{9})\.([a-z0-9]+)$/i.exec(location.hash);
  if (!m) return;
  history.replaceState(null, '', location.pathname);
  $('pcid').value = pretty(m[1]);
  $('secret').value = m[2];
  connect(m[1], m[2]);
})();
