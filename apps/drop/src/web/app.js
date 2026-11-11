// Plume Drop, the phone page: sends files and text to the PC, and saves what the PC offers.
// The code from the QR code comes after # in the address (browsers never send that part); it is kept on this
// device and sent with every request. This page talks only to the PC that served it.
'use strict';

const TEXTS = {
  en: {
    noCodeTitle: 'Scan the QR code',
    noCodeText: "Open Plume Drop on your PC and scan the code it shows with this device's camera.",
    pendingTitle: 'Waiting for the PC',
    pendingText: 'Accept "{name}" on the PC to start. It is asked only the first time.',
    refusedTitle: 'Not allowed',
    refusedText: 'The PC did not let this device in.',
    revokedTitle: 'Disconnected',
    revokedText: 'The PC disconnected this device.',
    askAgain: 'Ask again',
    badcodeTitle: 'This code has changed',
    badcodeText: 'Scan the new QR code shown in Plume Drop on your PC.',
    offline: "Can't reach the PC. Is it on the same Wi-Fi, with Plume Drop open?",
    online: 'Connected',
    sendLabel: 'Send photos and files',
    sendSub: 'Pick one or several. They are saved on the PC.',
    sendSubDesk: 'Pick files, or drop them anywhere on this page.',
    textTitle: 'Text or link',
    textPlaceholder: 'Type or paste here…',
    textNote: 'A link opens on the PC; text goes to its clipboard.',
    send: 'Send',
    sent: 'Sent',
    textSent: 'Sent to the PC',
    fromTitle: 'From the PC',
    none: 'Nothing yet. Files, text and links sent from the PC show up here.',
    save: 'Save',
    open: 'Open',
    copy: 'Copy',
    copied: 'Copied',
    waiting: 'Waiting…',
    failed: 'Failed',
    retry: 'Retry',
    saved: 'Saved on the PC as {name}',
    drop: 'Drop to send to the PC',
    foot: 'Same Wi-Fi only. Files go straight to the PC, never through the internet.',
  },
  fr: {
    noCodeTitle: 'Scannez le QR code',
    noCodeText: "Ouvrez Plume Drop sur votre PC et scannez le code qu'il affiche avec l'appareil photo.",
    pendingTitle: 'En attente du PC',
    pendingText: 'Acceptez « {name} » sur le PC pour commencer. Ce n’est demandé que la première fois.',
    refusedTitle: 'Non autorisé',
    refusedText: 'Le PC a refusé cet appareil.',
    revokedTitle: 'Déconnecté',
    revokedText: 'Le PC a déconnecté cet appareil.',
    askAgain: 'Redemander',
    badcodeTitle: 'Ce code a changé',
    badcodeText: 'Scannez le nouveau QR code affiché dans Plume Drop sur votre PC.',
    offline: 'PC injoignable. Est-il sur le même Wi-Fi, avec Plume Drop ouvert ?',
    online: 'Connecté',
    sendLabel: 'Envoyer photos et fichiers',
    sendSub: 'Un ou plusieurs. Ils sont enregistrés sur le PC.',
    sendSubDesk: 'Choisissez des fichiers, ou déposez-les sur cette page.',
    textTitle: 'Texte ou lien',
    textPlaceholder: 'Tapez ou collez ici…',
    textNote: 'Un lien s’ouvre sur le PC ; un texte va dans son presse-papiers.',
    send: 'Envoyer',
    sent: 'Envoyé',
    textSent: 'Envoyé au PC',
    fromTitle: 'Depuis le PC',
    none: 'Rien pour l’instant. Les fichiers, textes et liens envoyés depuis le PC apparaissent ici.',
    save: 'Enregistrer',
    open: 'Ouvrir',
    copy: 'Copier',
    copied: 'Copié',
    waiting: 'En attente…',
    failed: 'Échec',
    retry: 'Réessayer',
    saved: 'Enregistré sur le PC : {name}',
    drop: 'Déposez pour envoyer au PC',
    foot: 'Même Wi-Fi uniquement. Les fichiers vont directement au PC, jamais par internet.',
  },
};
const L = (navigator.language || 'en').toLowerCase().startsWith('fr') ? TEXTS.fr : TEXTS.en;
document.documentElement.lang = L === TEXTS.fr ? 'fr' : 'en';

const $ = (id) => document.getElementById(id);
const fill = (s, vars) => s.replace(/\{(\w+)\}/g, (_, k) => (vars[k] ?? ''));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function store(key, value) {
  try {
    if (value === undefined) return localStorage.getItem(key);
    localStorage.setItem(key, value);
  } catch (_) { /* private mode: kept for this visit only */ }
  return value;
}

// ───────── who this device is ─────────

function randomId() {
  const b = new Uint8Array(16);
  crypto.getRandomValues(b);
  return Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
}

function deviceName() {
  const ua = navigator.userAgent;
  if (/iPhone/.test(ua)) return 'iPhone';
  if (/iPad/.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1)) return 'iPad';
  const m = ua.match(/Android[^;)]*;\s*([^;)]+?)(?:\s+Build\/|\))/);
  if (m && m[1] && m[1].length > 1 && !/^(K|wv|Linux|U)$/i.test(m[1].trim())) return m[1].trim();
  if (/Android/.test(ua)) return L === TEXTS.fr ? 'Téléphone Android' : 'Android phone';
  if (/Windows/.test(ua)) return 'Windows PC';
  if (/Macintosh/.test(ua)) return 'Mac';
  if (/CrOS/.test(ua)) return 'Chromebook';
  if (/Linux/.test(ua)) return 'Linux PC';
  return 'Browser';
}

let token = '';
let device = '';
let rev = 0;
let status = '';
let polling = false;
const desktop = !/Android|iPhone|iPad|Mobile/.test(navigator.userAgent) && navigator.maxTouchPoints < 2;

function headers(extra) {
  return Object.assign({ 'X-Drop-Token': token, 'X-Drop-Device': device }, extra || {});
}

function auth(url) {
  const sep = url.includes('?') ? '&' : '?';
  return `${url}${sep}t=${encodeURIComponent(token)}&d=${encodeURIComponent(device)}`;
}

// ───────── state on screen ─────────

let toastTimer = 0;
function toast(text) {
  const t = $('toast');
  t.textContent = text;
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { t.hidden = true; }, 3200);
}

function online(on) {
  const d = $('dot');
  d.className = 'dot ' + (on ? 'on' : 'off');
  d.title = on ? L.online : L.offline;
}

function gate(kind) {
  const titles = { none: 'noCode', pending: 'pending', refused: 'refused', revoked: 'revoked', badcode: 'badcode' };
  const k = titles[kind];
  $('app').hidden = true;
  $('gate').hidden = false;
  $('gateTitle').textContent = L[k + 'Title'];
  $('gateText').textContent = fill(L[k + 'Text'], { name: deviceName() });
  $('spin').hidden = kind !== 'pending';
  const retry = $('retry');
  retry.hidden = !(kind === 'refused' || kind === 'revoked');
  retry.textContent = L.askAgain;
}

function setStatus(s) {
  if (s === status) return;
  status = s;
  if (s === 'allowed') {
    $('gate').hidden = true;
    $('app').hidden = false;
  } else {
    gate(s);
  }
}

// ───────── talking to the PC ─────────

async function hello(retry) {
  try {
    const r = await fetch('/api/hello', {
      method: 'POST',
      headers: headers({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ name: deviceName(), retry: !!retry }),
      cache: 'no-store',
    });
    if (r.status === 401) return setStatus('badcode');
    const j = await r.json();
    online(true);
    if (j.pc) $('pc').textContent = j.pc;
    setStatus(j.status);
  } catch (_) {
    online(false);
  }
  loop();
}

async function loop() {
  if (polling) return;
  polling = true;
  let failures = 0;
  while (true) {
    try {
      const r = await fetch(`/api/poll?since=${rev}`, { headers: headers(), cache: 'no-store' });
      if (r.status === 401) { setStatus('badcode'); break; }
      if (!r.ok) throw new Error(String(r.status));
      const j = await r.json();
      failures = 0;
      if (j.status === 'unknown') {
        // the PC was restarted and forgot this visit: say hello again (it asks the PC if needed)
        polling = false;
        rev = 0;
        return hello(false);
      }
      online(true);
      rev = j.rev;
      if (j.pc) $('pc').textContent = j.pc;
      setStatus(j.status);
      if (j.status === 'allowed') offers(j.offers || []);
      if (j.status === 'refused' || j.status === 'revoked') break;
    } catch (_) {
      failures += 1;
      online(false);
      if (failures === 2) toast(L.offline);
      await sleep(Math.min(2000 * failures, 10000));
    }
  }
  polling = false;
}

// ───────── sending files ─────────

const queue = [];
let sending = false;

function row(name, sub) {
  const li = document.createElement('li');
  li.className = 'item';
  const icon = document.createElement('div');
  icon.className = 'thumb';
  icon.innerHTML = '<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 19V5M5.5 11.5 12 5l6.5 6.5"/></svg>';
  const meta = document.createElement('div');
  meta.className = 'meta';
  const n = document.createElement('div');
  n.className = 'name';
  n.textContent = name;
  const s = document.createElement('div');
  s.className = 'sub';
  s.textContent = sub;
  meta.append(n, s);
  li.append(icon, meta);
  return { li, meta, sub: s };
}

function addFiles(list) {
  for (const file of list) {
    const r = row(file.name, L.waiting);
    const bar = document.createElement('div');
    bar.className = 'bar';
    const fillBar = document.createElement('i');
    bar.append(fillBar);
    r.meta.append(bar);
    $('queue').prepend(r.li);
    queue.push({ file, r, bar, fillBar });
  }
  pump();
}

async function pump() {
  if (sending) return;
  sending = true;
  while (queue.length) await upload(queue.shift());
  sending = false;
}

function upload(it) {
  return new Promise((done) => {
    const x = new XMLHttpRequest();
    x.open('POST', `/api/upload?name=${encodeURIComponent(it.file.name)}`);
    const h = headers();
    for (const k in h) x.setRequestHeader(k, h[k]);
    it.bar.hidden = false;
    it.r.sub.className = 'sub';
    x.upload.onprogress = (e) => {
      if (!e.lengthComputable) return;
      const p = e.total ? e.loaded / e.total : 1;
      it.fillBar.style.width = `${Math.round(p * 100)}%`;
      it.r.sub.textContent = `${size(e.loaded)} / ${size(e.total)}`;
    };
    const fail = () => {
      it.bar.hidden = true;
      it.r.sub.className = 'sub bad';
      it.r.sub.textContent = L.failed + ' · ';
      const again = document.createElement('a');
      again.href = '#';
      again.textContent = L.retry;
      again.onclick = (e) => {
        e.preventDefault();
        again.remove();
        it.r.sub.className = 'sub';
        it.r.sub.textContent = L.waiting;
        queue.push(it);
        pump();
      };
      it.r.sub.append(again);
      done();
    };
    x.onload = () => {
      if (x.status !== 200) {
        if (x.status === 401) setStatus('badcode');
        return fail();
      }
      let saved = it.file.name;
      try { saved = JSON.parse(x.responseText).name || saved; } catch (_) { /* keeps the name */ }
      it.bar.hidden = true;
      it.r.sub.className = 'sub ok';
      it.r.sub.textContent = `${L.sent} · ${size(it.file.size)}`;
      it.r.sub.title = fill(L.saved, { name: saved });
      done();
    };
    x.onerror = fail;
    x.onabort = fail;
    // the file itself is the body: the browser streams it from the disk
    x.send(it.file);
  });
}

function size(n) {
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i += 1; }
  return i === 0 ? `${n} B` : `${n < 10 ? n.toFixed(1) : Math.round(n)} ${u[i]}`;
}

// ───────── sending text ─────────

async function sendText() {
  const area = $('text');
  const text = area.value;
  if (!text.trim()) return;
  const b = $('sendText');
  b.disabled = true;
  try {
    const r = await fetch('/api/text', {
      method: 'POST',
      headers: headers({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ text }),
      cache: 'no-store',
    });
    if (r.status === 401) setStatus('badcode');
    if (!r.ok) throw new Error(String(r.status));
    area.value = '';
    toast(L.textSent);
  } catch (_) {
    toast(L.offline);
  }
  b.disabled = false;
}

// ───────── what the PC offers ─────────

let shown = '';

async function copy(text, button) {
  let ok = false;
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text);
      ok = true;
    }
  } catch (_) { /* falls back below */ }
  if (!ok) {
    // a page on the local network is not a "secure context": the old way still works there
    const t = document.createElement('textarea');
    t.value = text;
    t.setAttribute('readonly', '');
    t.className = 'offscreen';
    t.style.position = 'fixed';
    t.style.opacity = '0';
    document.body.append(t);
    t.select();
    t.setSelectionRange(0, text.length);
    try { ok = document.execCommand('copy'); } catch (_) { ok = false; }
    t.remove();
  }
  if (ok && button) {
    const before = button.textContent;
    button.textContent = L.copied;
    setTimeout(() => { button.textContent = before; }, 1600);
  }
}

function button(label, ghost) {
  const b = document.createElement('button');
  b.type = 'button';
  b.className = 'btn small' + (ghost ? ' ghost' : '');
  b.textContent = label;
  return b;
}

function offers(list) {
  const key = JSON.stringify(list.map((o) => o.id));
  if (key === shown) return;
  shown = key;
  const ul = $('offers');
  ul.textContent = '';
  $('none').hidden = list.length > 0;
  for (const o of list) {
    const li = document.createElement('li');
    li.className = 'item';
    let thumb;
    if (o.image) {
      thumb = document.createElement('img');
      thumb.className = 'thumb';
      thumb.alt = '';
      thumb.loading = 'lazy';
      thumb.src = auth(`/api/file/${o.id}?inline=1`);
    } else {
      thumb = document.createElement('div');
      thumb.className = 'thumb';
      const paths = {
        file: '<path d="M7 3h7l4 4v14H7z"/><path d="M14 3v4h4"/>',
        link: '<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1"/><path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>',
        text: '<path d="M5 6h14M5 11h14M5 16h9"/>',
      };
      thumb.innerHTML = `<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${paths[o.kind] || paths.file}</svg>`;
    }
    const meta = document.createElement('div');
    meta.className = 'meta';
    const n = document.createElement('div');
    const acts = document.createElement('div');
    acts.className = 'acts';
    if (o.kind === 'file') {
      n.className = 'name';
      n.textContent = o.name;
      const s = document.createElement('div');
      s.className = 'sub';
      s.textContent = o.sizeText;
      meta.append(n, s);
      const a = document.createElement('a');
      a.className = 'btn small';
      a.textContent = L.save;
      a.href = auth(`/api/file/${o.id}`);
      a.setAttribute('download', o.name);
      acts.append(a);
    } else {
      n.className = o.kind === 'link' ? 'name' : 'textbody';
      n.textContent = o.name;
      meta.append(n);
      if (o.kind === 'link') {
        const a = document.createElement('a');
        a.className = 'btn small';
        a.textContent = L.open;
        a.href = o.name;
        a.target = '_blank';
        a.rel = 'noopener noreferrer';
        acts.append(a);
      }
      const c = button(L.copy, o.kind === 'link');
      c.onclick = () => copy(o.name, c);
      acts.append(c);
    }
    li.append(thumb, meta, acts);
    ul.append(li);
  }
}

// ───────── start ─────────

function start() {
  for (const [id, key] of [['sendLabel', 'sendLabel'], ['textTitle', 'textTitle'], ['textNote', 'textNote'], ['sendText', 'send'], ['fromTitle', 'fromTitle'], ['none', 'none'], ['foot', 'foot'], ['dropText', 'drop']]) {
    $(id).textContent = L[key];
  }
  $('sendSub').textContent = desktop ? L.sendSubDesk : L.sendSub;
  $('text').placeholder = L.textPlaceholder;

  const code = location.hash.slice(1);
  if (code) {
    store('pd.token', code);
    // the code leaves the address bar (screenshots, shared tabs)
    history.replaceState(null, '', location.pathname);
  }
  token = store('pd.token') || '';
  device = store('pd.device') || store('pd.device', randomId());

  $('files').addEventListener('change', (e) => {
    addFiles(Array.from(e.target.files || []));
    e.target.value = '';
  });
  $('sendText').addEventListener('click', sendText);
  $('text').addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) sendText();
  });
  $('retry').addEventListener('click', () => {
    status = '';
    gate('pending');
    hello(true);
  });

  // a computer's browser: files dropped on the page are sent
  let depth = 0;
  const zone = $('drop');
  document.addEventListener('dragenter', (e) => {
    if (status !== 'allowed' || !e.dataTransfer || !Array.from(e.dataTransfer.types).includes('Files')) return;
    depth += 1;
    zone.hidden = false;
  });
  document.addEventListener('dragleave', () => {
    depth = Math.max(0, depth - 1);
    if (!depth) zone.hidden = true;
  });
  document.addEventListener('dragover', (e) => e.preventDefault());
  document.addEventListener('drop', (e) => {
    e.preventDefault();
    depth = 0;
    zone.hidden = true;
    if (status === 'allowed' && e.dataTransfer && e.dataTransfer.files.length) addFiles(Array.from(e.dataTransfer.files));
  });

  // a new code scanned while the page is open (same address, new #code)
  window.addEventListener('hashchange', () => {
    const fresh = location.hash.slice(1);
    if (!fresh) return;
    token = store('pd.token', fresh);
    history.replaceState(null, '', location.pathname);
    status = '';
    rev = 0;
    gate('pending');
    hello(false);
  });

  if (!token) {
    gate('none');
    return;
  }
  gate('pending');
  hello(false);
}

start();
