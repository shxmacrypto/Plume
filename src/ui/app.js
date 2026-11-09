// wry déclare déjà une variable globale `ipc` : ne pas la redéclarer.
const send = (c, o) => window.ipc.postMessage(JSON.stringify(Object.assign({ c }, o || {})));
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => '&#' + c.charCodeAt(0) + ';');
const host = (u) => { try { return new URL(u).hostname.replace(/^www\./, ''); } catch { return u || ''; } };
const store = { get(k, d) { try { const v = localStorage.getItem('plume.' + k); return v == null ? d : JSON.parse(v); } catch { return d; } }, set(k, v) { try { localStorage.setItem('plume.' + k, JSON.stringify(v)); } catch {} } };
const svg = (p, vb = 16) => `<svg viewBox="0 0 ${vb} ${vb}" aria-hidden="true">${p}</svg>`;
const I = {
  x: svg('<path d="M2 2l6 6M8 2 2 8"/>', 10),
  upd: svg('<path d="M8 2.5v8M4.5 7 8 10.5 11.5 7M3 13.5h10"/>'),
  reload: svg('<path d="M13 8a5 5 0 1 1-1.5-3.6M13 2.5v3h-3"/>'),
  stop: svg('<path d="M4 4l8 8M12 4l-8 8"/>'),
  search: svg('<circle cx="7" cy="7" r="4.5"/><path d="m10.5 10.5 3 3"/>'),
  globe: svg('<circle cx="8" cy="8" r="6"/><path d="M2 8h12M8 2c2 2 2 10 0 12M8 2c-2 2-2 10 0 12"/>'),
  clock: svg('<circle cx="8" cy="8" r="6"/><path d="M8 4.5V8l2.5 1.5"/>'),
  plusTab: '<svg class="newic" viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="3.5"/><path d="M8 5.5v5M5.5 8h5"/></svg>',
  star: svg('<path d="m8 2 1.8 3.8 4.2.5-3.1 2.9.8 4.1L8 11.3l-3.7 2 .8-4.1L2 6.3l4.2-.5z"/>'),
  key: svg('<circle cx="5.5" cy="10.5" r="3"/><path d="m7.6 8.4 5.9-5.9M11.5 4.5l1.5 1.5M10 6l1.2 1.2"/>'),
  puzzle: svg('<path d="M6 2.5h2.5a1 1 0 0 1 1 1V5h1.3a1.6 1.6 0 1 1 0 3.2H9.5V10H8.2a1.6 1.6 0 1 0-3.2 0H2.5V7.2h1.3a1.6 1.6 0 1 0 0-3.2H2.5v-.5a1 1 0 0 1 1-1z"/>'),
  check: svg('<path d="m3 8.5 3 3 7-7"/>'),
  eye: svg('<path d="M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z"/><circle cx="8" cy="8" r="2"/>'),
  copy: svg('<rect x="5" y="5" width="8.5" height="8.5" rx="2"/><path d="M10.5 5V3.5a1.5 1.5 0 0 0-1.5-1.5H3.5A1.5 1.5 0 0 0 2 3.5V9a1.5 1.5 0 0 0 1.5 1.5H5"/>'),
  trash: svg('<path d="M2.5 4h11M6 4V2.5h4V4M4 4l.7 9.5h6.6L12 4"/>'),
  folder: svg('<path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z"/>'),
  file: svg('<path d="M4 2h5l3 3v9H4z"/><path d="M9 2v3h3"/>'),
  chev: svg('<path d="m4 6 4 4 4-4"/>'),
  side: svg('<rect x="2" y="2.5" width="12" height="11" rx="2"/><path d="M10 2.5v11"/>'),
  newtab: svg('<path d="M9 2.5h4.5V7M13.5 2.5 7.5 8.5M12 9.5v3a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h3"/>'),
  pin: svg('<path d="M9.5 2.5 13.5 6.5 11 8l-1 3.5-5.5-5.5L8 5z"/><path d="M6.8 9.2 3 13"/>'),
  film: svg('<rect x="2" y="3" width="12" height="10" rx="2"/><path d="m6.5 6 3.5 2-3.5 2z"/>'),
  gear: svg('<circle cx="8" cy="8" r="2.2"/><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4"/>'),
  g: '<svg class="g" viewBox="0 0 48 48" aria-hidden="true"><path fill="#EA4335" d="M24 9.5c3.54 0 6.71 1.22 9.21 3.6l6.85-6.85C35.9 2.38 30.47 0 24 0 14.62 0 6.51 5.38 2.56 13.22l7.98 6.19C12.43 13.72 17.74 9.5 24 9.5z"/><path fill="#4285F4" d="M46.98 24.55c0-1.57-.15-3.09-.38-4.55H24v9.02h12.94c-.58 2.96-2.26 5.48-4.78 7.18l7.73 6c4.51-4.18 7.09-10.36 7.09-17.65z"/><path fill="#FBBC05" d="M10.53 28.59c-.48-1.45-.76-2.99-.76-4.59s.27-3.14.76-4.59l-7.98-6.19C.92 16.46 0 20.12 0 24c0 3.88.92 7.54 2.56 10.78l7.97-6.19z"/><path fill="#34A853" d="M24 48c6.48 0 11.93-2.13 15.89-5.81l-7.73-6c-2.15 1.45-4.92 2.3-8.16 2.3-6.26 0-11.57-4.22-13.47-9.91l-7.98 6.19C6.51 42.62 14.62 48 24 48z"/></svg>',
};
const ENGINES = { google: 'Google', bing: 'Bing', ddg: 'DuckDuckGo', brave: 'Brave', ecosia: 'Ecosia', qwant: 'Qwant' };
const GC = { blue: '#0a84ff', purple: '#bf5af2', pink: '#ff375f', red: '#ff453a', orange: '#ff9f0a', yellow: '#e5b800', green: '#30d158', teal: '#40c8e0' };
let logoN = 0;
const logo = (fixed) => {
  const n = fixed || ++logoN;
  return `<svg class="logo" viewBox="0 0 64 64" aria-hidden="true"><defs><linearGradient id="lg${n}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#64d2ff"/><stop offset=".5" stop-color="#0a84ff"/><stop offset="1" stop-color="#5e5ce6"/></linearGradient>`
    + `<linearGradient id="ls${n}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".28"/><stop offset=".5" stop-color="#fff" stop-opacity="0"/></linearGradient></defs>`
    + `<rect x="2" y="2" width="60" height="60" rx="15" fill="url(#lg${n})"/><rect x="2" y="2" width="60" height="60" rx="15" fill="url(#ls${n})"/>`
    + `<g class="fe"><path fill="#fff" d="M48.5 12.5c-12.6 1.2-23.4 9.6-26.6 22.4l-1.4 6.6 6.4-1.6c4.6-1.2 8.6-3.6 11.6-7.2l-6.2-.6 8.8-3.6c3.8-4.6 7-10 7.4-16z"/>`
    + `<path fill="none" stroke="#2f7bf5" stroke-width="1.5" stroke-linecap="round" d="M25 37.5C30 28 36 21 44.5 15.5"/>`
    + `<path fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" d="M15 50.5 33.5 29"/></g></svg>`;
};

let S = { tabs: [], bm: [], a: 0, cfg: {}, groups: [], exts: [], vault: {} }, H = [], DLS = [], mode = null, sel = 0, items = [], typed = '', drag = null, confirmClear = false, ctx = null;
const known = new Set(), loaded = new Set();
const act = () => S.tabs.find((x) => x.id === S.a) || {};
const isBlank = () => !act().u;
const C = () => S.cfg || {};
const grp = (id) => S.groups.find((g) => g.id === id);
// favoris séparés par espace (#7) : ceux sans espace appartiennent à l'espace Principal
const bmShow = (b) => !(C().spaces && C().spacesBm === 'separate') || (b.sp || 0) === (S.space || 0);
const engine = () => ENGINES[C().search] || 'Google';

/* Icône de site : lettre colorée, remplacée par le favicon dès qu'il est chargé */
function icon(u, f, fb) {
  if (!f && fb && /^https?:/.test(u)) { try { f = new URL(u).origin + '/favicon.ico'; } catch {} }
  const h = host(u), ch = (h[0] || '?').toUpperCase();
  let hu = 0; for (const c of h) hu = (hu * 31 + c.charCodeAt(0)) % 360;
  return `<span class="ic${f && loaded.has(f) ? ' ok' : ''}" style="--hu:${hu}" aria-hidden="true"><b>${esc(ch)}</b>${f ? `<img src="${esc(f)}" alt="">` : ''}</span>`;
}
document.addEventListener('load', (e) => { const x = e.target; if (x.tagName === 'IMG' && x.parentNode.classList.contains('ic')) { loaded.add(x.getAttribute('src')); x.parentNode.classList.add('ok'); } }, true);
document.addEventListener('error', (e) => { const x = e.target; if (x.tagName === 'IMG' && x.parentNode.classList.contains('ic')) x.remove(); }, true);

function toast(k, v) {
  const el = $('toast');
  el.textContent = TXT.en['toast.' + k] ? t('toast.' + k, v) : k;
  el.classList.add('on');
  clearTimeout(toast.t); toast.t = setTimeout(() => el.classList.remove('on'), 3200);
}

/* ---------- Réglages appliqués à l'interface ---------- */
function applyCfg() {
  const c = C(), b = document.body.classList;
  const l = c.lang && c.lang !== 'auto' ? c.lang : S.lang || (navigator.language || 'en').slice(0, 2);
  const nl = TXT[l] ? l : 'en';
  if (nl !== LANG) { LANG = nl; document.documentElement.lang = nl; applyLang(); }
  const light = c.theme === 'light' || (c.theme === 'system' && matchMedia('(prefers-color-scheme: light)').matches);
  b.toggle('light', light); b.toggle('hc', !!c.contrast); b.toggle('rm', !!c.motion); b.toggle('big', !!c.bigTargets); b.toggle('fr', !!c.focusRings);
  b.toggle('vt', c.tabs === 'v'); b.toggle('vtn', c.tabs === 'v' && store.get('vtn', false));
  const acc = c.accent || '#0a84ff';
  document.documentElement.style.setProperty('--acc', acc);
  const n = parseInt(acc.slice(1), 16);
  document.documentElement.style.setProperty('--acc-ring', `rgba(${n >> 16},${(n >> 8) & 255},${n & 255},.55)`);
  $('q').placeholder = t('q.ph', { e: engine() });
  sendH();
}
function applyLang() {
  document.querySelectorAll('[data-t]').forEach((el) => { el.title = t(el.dataset.t); el.setAttribute('aria-label', t(el.dataset.t)); });
  $('vnew').querySelector('span').textContent = t('tab.new');
  $('vnew').setAttribute('aria-label', t('tab.new'));
  $('vcol').setAttribute('aria-label', 'Collapse');
}

/* ---------- État poussé par Rust ---------- */
window.U = (s) => { S = s; applyCfg(); render(); if (mode === 'ext') renderPanel(); if (window.onState) onState(); };
window.L = (l) => { H = l.hist || []; if (mode === 'hist' || (!mode && isBlank()) || (mode === 'omni' && !typed.trim())) renderPanel(); };
window.D = (l) => {
  const prev = DLS; DLS = l; renderDlBtn();
  if (mode !== 'dl' && !dlPopOpen()) return;
  // seule la progression change : lignes mises à jour sur place (la barre glisse, un clic en cours n'est pas perdu)
  const onlyProgress = prev.length === l.length && l.every((d, i) => prev[i].id === d.id && prev[i].state === d.state && prev[i].name === d.name);
  if (onlyProgress && document.querySelector('#ov .dlr')) {
    for (const d of l) {
      const r = document.querySelector(`#ov .dlr[data-dlid="${d.id}"]`);
      if (!r) continue;
      r.querySelector('.meta span').textContent = dlStatus(d);
      const b = r.querySelector('.bar2 i'); if (b) b.style.width = dlPct(d) + '%';
    }
    return;
  }
  renderPanel();
};
// Barre d'adresse : jamais volée à un panneau ouvert, sauf demande explicite (Ctrl+L)
window.focusBar = (force) => { if (!force && ['set', 'hist', 'dl', 'media', 'unlock', 'menu', 'ctx', 'ext', 'pg', 'modal', 'pop'].includes(mode)) return; $('q').focus(); $('q').select(); };
window.openPanel = (m, sub) => { if (sub) SET.sec = sub; setMode(mode === m && !sub ? null : m, true); };
window.openUnlock = () => setMode('unlock');
// clic dans la page pendant qu'un petit menu est ouvert → il se ferme
window.pageClicked = () => { if (['menu', 'ext', 'ctx', 'pop'].includes(mode)) setMode(null); };
/* Zones visibles de l'interface au-dessus de la page (le reste laisse voir la page, sans clignotement) */
let lastOvr = '';
function sendOvr() {
  // écran de démarrage / assistant / animation de bienvenue : l'interface couvre tout
  const sp = document.getElementById('splash');
  const cover = (sp && !sp.classList.contains('gone')) || ONB.on;
  if (!mode && !cover) { lastOvr = ''; return; }
  const full = cover || ['set', 'hist', 'dl', 'media', 'unlock', 'pg', 'modal'].includes(mode) || isBlank();
  const rects = full ? [] : [...document.querySelectorAll('#ov .card, #sug .card')].map((c) => { const r = c.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)]; });
  const k = JSON.stringify([full, rects]);
  if (k !== lastOvr) { lastOvr = k; send('ovr', { full, rects }); }
}
let ovrRaf = 0;
const ovrSoon = () => { cancelAnimationFrame(ovrRaf); ovrRaf = requestAnimationFrame(sendOvr); };
new MutationObserver(ovrSoon).observe(document.body, { childList: true, subtree: true });
document.addEventListener('animationend', ovrSoon, true);
window.blk = (n) => { S.blocked = n; renderShield(); };
// début d'un téléchargement : la bulle s'ouvre sous le bouton, sauf si un autre panneau est ouvert (simple message)
window.dlStarted = (n) => { if (!mode || dlPopOpen() || (mode === 'omni' && !typed.trim() && !prevMode)) openDlPop(true); else toast(t('dl.started', { n })); };
window.extAdded = (n) => { toast('extAdded', { n }); act().info = null; };
window.extErr = (e) => toast(String(e).slice(0, 160));
// Fin d'import : on laisse l'animation se jouer au moins 2,4 s pour qu'on la voie
window.impDone = (r) => {
  const wait = Math.max(0, 2400 - (performance.now() - (IMP.t0 || 0)));
  setTimeout(() => { IMP.busy = false; IMP.done = r; if (!ONB.on) toast('imported', r); if (ONB.on) renderOnb(); if (window.renderImp) renderImp(); }, ONB.on ? wait : 0);
};
const B_COLORS = { Google: '#1a73e8', Microsoft: '#0f8a8a', Brave: '#fb542b', Opera: '#e0182d', Vivaldi: '#ef3939', Firefox: '#ff7139', Chromium: '#4a7bd6' };
// Tuile de navigateur : sa vraie icône si on l'a trouvée sur le PC, sinon une lettre colorée
const bTile = (x, style) => x.icon ? `<img class="btile bimg" src="${esc(x.icon)}" alt="" style="${style || ''}">` : `<span class="btile" style="--bc:${bColor(x.browser)};${style || ''}" aria-hidden="true">${esc(x.browser.split(' ').pop()[0])}</span>`;
const bColor = (n) => B_COLORS[n.split(' ')[0]] || '#636366';
// Compteurs qui défilent (nombre importés)
function countUp(root) {
  root.querySelectorAll('[data-count]').forEach((el) => {
    const end = +el.dataset.count, t0 = performance.now(), d = 900;
    const step = (now) => { const p = Math.min(1, (now - t0) / d); el.textContent = Math.round(end * (1 - Math.pow(1 - p, 3))).toLocaleString(LANG); if (p < 1) requestAnimationFrame(step); };
    requestAnimationFrame(step);
  });
}
window.pwImported = (n) => toast('pwImported', { n });

function render() {
  const x = act();
  document.body.classList.toggle('priv', !!x.p);
  document.body.classList.toggle('max', !!S.max);
  renderTabs();
  renderSide();
  $('back').disabled = !S.back; $('fwd').disabled = !S.fwd;
  $('reload').innerHTML = x.l ? I.stop : I.reload;
  const rl = t(x.l ? 't.stop' : 't.reload'); $('reload').title = rl; $('reload').setAttribute('aria-label', rl);
  $('reload').disabled = !x.u;
  $('prog').classList.toggle('on', !!x.l);
  $('star').classList.toggle('on', !!S.star);
  $('star').style.display = x.u ? '' : 'none';
  $('readerBtn').style.display = x.u && x.u.startsWith('http') ? '' : 'none';
  $('mediaBtn').style.display = isVideoSite() ? '' : 'none';
  if (document.activeElement !== $('q')) showUrl();
  renderShield();
  renderInfo();
  const bar = $('bmb');
  bar.classList.toggle('on', !!C().bmbar && S.bm.length > 0);
  bar.innerHTML = S.bm.map((b, i) => !bmShow(b) ? '' : `<button class="bm" draggable="true" data-bi="${i}" data-u="${esc(b.u)}" title="${esc(b.u)}">${icon(b.u, b.f, true)}<span>${esc(b.t)}</span></button>`).join('');
  $('extBtn').hidden = !(S.exts || []).length;
  document.documentElement.style.setProperty('--sr', (S.sideR || 0) + 'px');
  const sp = S.sidep;
  $('sidep').classList.toggle('on', !!(sp && sp.id));
  if (sp && sp.id) $('sidep').innerHTML = `${sp.icon ? `<img src="${esc(sp.icon)}" alt="">` : I.puzzle}<b>${esc(sp.name || '')}</b><button class="icb" data-sp="side-tab" title="${esc(t('x.toTab'))}" aria-label="${esc(t('x.toTab'))}">${I.newtab}</button><button class="icb" data-sp="side-close" title="${esc(t('b.close'))}" aria-label="${esc(t('b.close'))}">${I.x}</button>`;
  const pinx = C().pinExt || [];
  $('pinx').innerHTML = (S.exts || []).filter((e) => pinx.includes(e.id)).map((e) => `<button class="nb xpin" data-xo="${esc(e.id)}" data-page="${esc(e.popup || e.options || '')}" title="${esc(e.name)}" aria-label="${esc(e.name)}">${e.icon ? `<img src="${esc(e.icon)}" alt="">` : I.puzzle}</button>`).join('');
  renderDlBtn();
  document.title = x.t || 'Plume';
  $('menuBtn').classList.toggle('upd', !!UPD.tag);
  $('menuBtn').classList.toggle('updready', UPD.state === 'ready');
  $('maxBtn').innerHTML = S.max ? '&#xE923;' : '&#xE922;';
  $('maxBtn').title = t(S.max ? 't.restore' : 't.max');
  if (window.renderExtra) renderExtra(x);
  if (mode === 'set' && window.renderSettings) return renderSettings(true);
  if (mode || isBlank()) renderPanel(); else $('ov').classList.remove('on');
}

function renderShield() {
  const on = C().ab && S.abSite && act().u && act().u.startsWith('http');
  $('shield').classList.toggle('on', !!on);
  $('shield').classList.toggle('hide', !act().u || !act().u.startsWith('http'));
  $('blk').textContent = on && S.blocked && !(C().privReport && C().prCount === false) ? S.blocked : '';
  $('shield').title = on ? t('m.blocked', { n: S.blocked || 0 }) : t('t.shield');
}

function renderDlBtn() {
  const b = $('dlBtn');
  b.hidden = !S.dlAny && !DLS.length;
  const run = DLS.filter((d) => d.state === 'run');
  const tot = run.reduce((a, d) => a + (d.total || 0), 0), got = run.reduce((a, d) => a + (d.recv || 0), 0);
  const pct = run.length ? (tot ? Math.round(got / tot * 100) : 30) : 0;
  // progression seule : l'anneau avance sur place (remplacer le bouton pendant un clic le ferait perdre)
  const ring = b.querySelector('.ring');
  if (run.length && ring) ring.style.setProperty('--p', pct);
  else if (run.length || ring) b.innerHTML = svg('<path d="M8 2.5v8M4.5 7 8 10.5 11.5 7M3 13.5h10"/>') + (run.length ? `<i class="ring" style="--p:${pct}"></i>` : '');
  b.classList.toggle('on', run.length > 0);
}

function showUrl() {
  const u = act().u || '';
  $('q').value = u.startsWith('https://') ? host(u) : u.startsWith('http://') ? u.slice(7).replace(/\/$/, '') : u;
  $('lock').classList.toggle('hide', !u.startsWith('https://'));
}

/* ---------- Onglets (horizontaux) ---------- */
function tabHtml(x, narrow) {
  const pop = !known.has(x.id) && known.size ? ' pop' : '';
  const title = x.t || (x.u ? host(x.u) : t('tab.new'));
  const g = x.g && grp(x.g);
  const on = x.id === S.a;
  if (x.pin) return `<div class="tab pin${on ? ' on' : ''}${x.s ? ' zz' : ''}${pop}" role="tab" tabindex="${on ? 0 : -1}" aria-selected="${on}" data-id="${x.id}" title="${esc(title)}">${x.l ? '<i class="spin" aria-hidden="true"></i>' : x.u ? icon(x.u, x.f, true) : I.plusTab}</div>`;
  return `<div class="tab${on ? ' on' : ''}${x.s ? ' zz' : ''}${x.p ? ' pv' : ''}${pop}" role="tab" tabindex="${on ? 0 : -1}" aria-selected="${on}" data-id="${x.id}"`
    + `${g ? ` data-gc style="--gc:${GC[g.color]}"` : ''} title="${esc(title)}${x.s ? ' — ' + t('tab.asleep') : ''}${x.p ? ' — InPrivate' : ''}">`
    + (x.l ? '<i class="spin" aria-hidden="true"></i>' : x.u ? icon(x.u, x.f) : I.plusTab)
    + (narrow && !on ? '' : `<span class="tt">${esc(title)}</span>`) + (window.tabExtra ? tabExtra(x, narrow && !on) : '')
    + (narrow && !on ? '' : `<button class="x" data-x="${x.id}" aria-label="${esc(t('tab.closeTab'))}" tabindex="-1">${I.x}</button>`) + '</div>';
}
function renderTabs() {
  if (drag && drag.moved) return;
  const el = $('tabs');
  if (C().tabs === 'v') { el.innerHTML = ''; return; }
  const vis = S.tabs.filter((x) => !x.pin && !(x.g && grp(x.g)?.col && x.id !== S.a));
  const pinW = S.tabs.filter((x) => x.pin).length * 44;
  const narrow = vis.length > 0 && ($('top').clientWidth - 260 - pinW) / vis.length < 70;
  let html = '', last = 0;
  for (const x of S.tabs) {
    if (x.g && x.g !== last) {
      const g = grp(x.g);
      if (g) html += `<button class="gchip${g.col ? ' col' : ''}" data-g="${g.id}" style="--gc:${GC[g.color]}" aria-expanded="${!g.col}" title="${esc(g.name || t('g.unnamed'))}">${esc(g.name)}</button>`;
    }
    last = x.g;
    if (x.g && grp(x.g)?.col && x.id !== S.a) continue;
    html += tabHtml(x, narrow);
  }
  el.innerHTML = html;
  S.tabs.forEach((x) => known.add(x.id));
}

/* ---------- Onglets verticaux ---------- */
function renderSide() {
  if (C().tabs !== 'v') return;
  const pins = S.tabs.filter((x) => x.pin);
  let html = pins.length ? `<div class="vpins">${pins.map((x) => `<div class="vpin${x.id === S.a ? ' on' : ''}" role="tab" tabindex="${x.id === S.a ? 0 : -1}" aria-selected="${x.id === S.a}" data-id="${x.id}" title="${esc(x.t || host(x.u))}">${x.u ? icon(x.u, x.f, true) : I.plusTab}</div>`).join('')}</div>` : '', last = 0;
  for (const x of S.tabs) {
    if (x.pin) continue;
    const g = x.g && grp(x.g);
    if (x.g && x.g !== last && g) html += `<button class="vgrp${g.col ? ' col' : ''}" data-g="${g.id}" style="--gc:${GC[g.color]}" aria-expanded="${!g.col}">${I.chev}<span>${esc(g.name || t('g.unnamed'))}</span></button>`;
    last = x.g;
    if (g && g.col && x.id !== S.a) continue;
    const title = x.t || (x.u ? host(x.u) : t('tab.new')), on = x.id === S.a;
    html += `<div class="vtab${on ? ' on' : ''}${x.s ? ' zz' : ''}${x.p ? ' pv' : ''}${g ? ' ing' : ''}" role="tab" tabindex="${on ? 0 : -1}" aria-selected="${on}" data-id="${x.id}"${g ? ` style="--gc:${GC[g.color]}"` : ''} title="${esc(title)}">`
      + (x.l ? '<i class="spin" aria-hidden="true"></i>' : x.u ? icon(x.u, x.f) : I.plusTab)
      + `<span class="tt">${esc(title)}</span>` + (window.tabExtra ? tabExtra(x, false) : '') + `<button class="x" data-x="${x.id}" aria-label="${esc(t('tab.closeTab'))}" tabindex="-1">${I.x}</button></div>`;
  }
  $('vlist').innerHTML = html;
}

/* ---------- Barre d'information ---------- */
function renderInfo() {
  const el = $('info'), i = S.info;
  const ub = i ? '' : updBar() || defBar();
  el.classList.toggle('on', !!i || !!ub);
  if (!i) { el.innerHTML = ub; return; }
  const close = `<button class="icb" data-info="dismiss" aria-label="${esc(t('b.close'))}">${I.x}</button>`;
  let ic = I.key, msg = '', btns = '';
  const v = { u: esc(i.u || '—'), h: esc(i.host || '') };
  if (i.k === 'save') {
    msg = t(i.upd ? 'info.upd' : 'info.save', v);
    btns = `<button class="pill pri" data-info="save">${t(i.upd ? 'b.update' : 'b.save')}</button><button class="pill" data-info="never">${t('b.never')}</button>`;
  } else if (i.k === 'unlock') {
    msg = t('info.unlock', v); btns = `<button class="pill pri" data-info="unlock">${t('b.unlock')}</button>`;
  } else if (i.k === 'choose') {
    msg = t('info.choose', v); btns = i.list.map((u, k) => `<button class="pill" data-info="fill" data-i="${k}">${esc(u || '—')}</button>`).join('');
  } else if (i.k === 'gen') {
    msg = `${t('info.gen')} <code class="genpw">${esc(i.pw || '')}</code>`; btns = `<button class="pill pri" data-info="gen">${t('b.usePw')}</button>`;
  } else if (i.k === 'store') {
    ic = I.puzzle;
    msg = S.extOn ? t('info.store') : t('info.storeOff');
    btns = S.extOn ? `<button class="pill pri" data-info="store">${t('b.add')}</button>` : `<button class="pill pri" data-info="ext-enable">${t('b.enable')}</button>`;
  }
  el.innerHTML = `<span class="ii">${ic}</span><span class="msg">${msg}</span>${btns}${close}`;
}

/* ---------- Panneaux ---------- */
let prevMode = null; // panneau (réglages, historique, téléchargements) gardé sous la barre d'adresse
function setMode(m, focusPage) {
  if (m === mode) return;
  const was = !!mode;
  mode = m; sel = 0; confirmClear = false;
  if (!!m !== was) send('ov', { v: !!m, f: !!focusPage });
  if (m !== 'omni' && document.activeElement === $('q')) $('q').blur();
  if (m === 'omni' && prevMode) return renderPanel(); // le panneau reste affiché dessous
  renderPanel();
  ovrSoon();
  if (m === 'menu' || m === 'ctx' || m === 'ext') setTimeout(() => document.querySelector('#ov .card .row:not(:disabled)')?.focus(), 30);
}

/* Panneau affiché dans #ov : on ne remplace le contenu que s'il a changé. Une mise à jour du même panneau
   ne rejoue pas son animation d'entrée, et un clic en cours n'atterrit pas sur un élément qui vient d'être remplacé. */
let ovLast = { mode: null, html: '', node: null };
// bouton pressé dans le panneau : le remplacer maintenant ferait perdre le clic, il est redessiné juste après
let ovHeldAt = 0, ovSkipped = false;
$('ov').addEventListener('mousedown', () => { ovHeldAt = performance.now(); }, true);
const ovRelease = () => {
  if (!ovHeldAt) return;
  ovHeldAt = 0;
  if (ovSkipped) { ovSkipped = false; setTimeout(() => { if (mode || isBlank()) renderPanel(); }, 0); }
};
for (const ev of ['mouseup', 'dragend', 'drop']) document.addEventListener(ev, ovRelease, true);
window.addEventListener('blur', ovRelease);
function ovSet(html) {
  const ov = $('ov');
  const same = ovLast.mode === mode && ovLast.node && ovLast.node.isConnected && ov.firstElementChild === ovLast.node;
  if (same && ovLast.html === html) return false;
  if (same && ovHeldAt && performance.now() - ovHeldAt < 1500) { ovSkipped = true; return false; }
  ov.classList.toggle('again', same);
  ov.innerHTML = html;
  ovLast = { mode, html, node: ov.firstElementChild };
  return true;
}
function renderPanel() {
  const ov = $('ov');
  if (!(mode === 'omni' && typed.trim())) $('sug').innerHTML = '';
  if (mode === 'omni' && prevMode) { if (typed.trim()) renderSuggest(); return; }
  const show = !!mode || isBlank();
  ov.classList.toggle('on', show);
  ov.classList.toggle('dim', isBlank() && !['hist', 'dl', 'set', 'pg'].includes(mode));
  ov.classList.toggle('full', ['hist', 'dl', 'set', 'pg'].includes(mode));
  if (!show) return;
  const under = isBlank() ? startHtml() : '';
  switch (mode) {
    case 'menu': return ovSet(under + menuHtml());
    case 'ext': return ovSet(under + extMenuHtml());
    case 'ctx': return ovSet(under + ctxHtml());
    case 'unlock': return renderUnlock(under);
    case 'media': return renderMedia(under);
    case 'hist': return renderHist();
    case 'dl': return renderDl();
    case 'set': return renderSettings();
    case 'pg': return renderPage();
  }
  if (window.panelHook && panelHook(under)) return;
  ovSet(isBlank() || mode !== 'omni' ? startHtml() : '');
  if (mode === 'omni' && typed.trim()) renderSuggest();
}

let startSeen = false;
function startHtml() {
  const x = act();
  const fav = S.bm.length
    ? `<div class="grid favgrid">${S.bm.map((b, i) => !bmShow(b) ? '' : `<button class="tile" draggable="true" data-bi="${i}" data-u="${esc(b.u)}" title="${esc(b.u)}">${icon(b.u, b.f, true)}<span>${esc(b.t)}</span><i class="x" data-del="${esc(b.u)}" role="button" aria-label="${esc(t('b.remove'))}">${I.x}</i></button>`).join('')}</div>`
    : `<p class="hint">${t('home.favHint')}</p>`;
  const seen = new Set(), top = [];
  for (const h of [...H].sort((a, b) => (b.c || 1) - (a.c || 1))) {
    const k = host(h.u); if (seen.has(k) || top.length >= 8) continue; seen.add(k); top.push(h);
  }
  const freq = top.length && !x.p ? `<h2>${t('home.freq')}</h2><div class="grid">${top.map((h) => `<button class="tile" data-u="${esc(h.u)}" title="${esc(h.u)}">${icon(h.u, h.f, true)}<span>${esc(h.t || host(h.u))}</span></button>`).join('')}</div>` : '';
  const name = (C().name || '').trim();
  const title = x.p ? t('home.priv') : name ? esc(t('home.hi', { n: name })) : 'Plume';
  const typing = mode === 'omni';
  const hero = `<div class="hero">${logo('home')}<h1>${title}</h1><button class="fake" data-fake="1">${I.g.replace('class="g"', 'class="g"')}<span>${typing ? '' : esc(t('q.ph', { e: engine() }))}</span><kbd>Ctrl L</kbd></button></div>`;
  const wasSeen = startSeen; startSeen = true;
  if (window.homeHtml) return homeHtml({ hero, fav, freq, x, wasSeen, typing });
  return `<div class="start${wasSeen ? ' seen' : ''}${typing ? ' typing' : ''}${x.p ? ' priv' : ''}">${hero}${x.p ? `<p class="privnote">${t('home.privNote')}</p>` : ''}<h2>${t('home.fav')}</h2>${fav}${freq}</div>`;
}

function suggest(raw) {
  const q = raw.trim().toLowerCase(), out = [];
  const url = /^[^\s]+\.[^\s]{2,}$|^localhost|^[\w.-]+:\d+|:\/\//.test(q);
  const ci = window.calcItem && calcItem(raw);
  if (ci) out.push(ci); // résultat d'un calcul / d'une conversion : Entrée le copie
  out.push(url ? { k: 'globe', t: raw.trim(), u: raw.trim(), sub: t('q.go') } : { k: C().search === 'google' || !C().search ? 'g' : 'search', t: raw.trim(), u: raw.trim(), sub: t('q.search', { e: engine() }) });
  const seen = new Set();
  for (const [k, arr] of [['star', S.bm], ['clock', H]]) {
    for (const h of arr) {
      if (out.length >= 8) break;
      if (seen.has(h.u) || !((h.t || '') + ' ' + h.u).toLowerCase().includes(q)) continue;
      seen.add(h.u); out.push({ k, t: h.t || host(h.u), u: h.u, sub: host(h.u), f: h.f });
    }
  }
  return out;
}

function renderSuggest() {
  items = suggest(typed);
  const r = $('omni').getBoundingClientRect(), top = $('hdr').offsetHeight, sw = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--sw')) || 0;
  $('sug').innerHTML = `<div class="card" role="listbox" style="left:${r.left - 6 - sw}px;top:${r.bottom - top + 8}px;width:${r.width + 12}px">`
    + items.map((it, i) => `<button class="row${i === sel ? ' sel' : ''}" role="option" aria-selected="${i === sel}" data-i="${i}">${it.f ? icon(it.u, it.f) : I[it.k]}<span class="tt">${esc(it.t)}</span><span class="sub">${esc(it.sub)}</span></button>`).join('') + '</div>';
}

const row = (c, label, kb, dis, cls, extra) => `<button class="row${cls ? ' ' + cls : ''}" role="menuitem" data-m="${c}"${dis ? ' disabled' : ''}${extra || ''}><span class="tt">${label}</span>${kb === true ? `<span class="chk">${I.check}</span>` : `<span class="kb">${kb || ''}</span>`}</button>`;
function menuHtml() {
  const x = act(), web = x.u && x.u.startsWith('http'), z = Math.round((S.z || 1) * 100);
  return `<div class="card" role="menu" style="right:10px;top:6px;width:300px;max-height:calc(100% - 12px);overflow:auto">`
    + updRow()
    + row('new', t('m.new'), 'Ctrl+T') + row('window', t('m.win'), 'Ctrl+N') + row('private', t('m.privWin'), 'Ctrl+Shift+N') + row('reopen', t('m.reopen'), 'Ctrl+Shift+T', !S.closed)
    + '<div class="hr"></div>'
    + row('media', t('m.media'), '', !web) + row('reader', t('m.reader'), 'F9', !web) + row('speak', t(S.speaking ? 'm.stopSpeak' : 'm.speak'), '', !web) + row('translate', t('m.translate'), '', !web)
    + row('dark-site', t('m.darkSite'), S.darkSite ? true : '', !web) + row('ab-site', t('m.abSite'), S.abSite && C().ab ? true : '', !web || !C().ab)
    + '<div class="hr"></div>'
    + row('star', t(S.star ? 'm.unstar' : 'm.star'), 'Ctrl+D', !x.u) + row('bmbar', t('m.bmbar'), C().bmbar ? true : 'Ctrl+Shift+B')
    + row('hist', t('m.hist'), 'Ctrl+H') + row('dl', t('m.dl'), 'Ctrl+J') + row('pw', t('m.pw'), '') + (S.extOn ? row('exts', t('m.exts'), '') : '')
    + '<div class="hr"></div>'
    + `<div class="zoom"><span>${t('m.zoom')}</span><button data-z="-1"${!x.u ? ' disabled' : ''} aria-label="Zoom −">−</button><button data-z="0"${!x.u ? ' disabled' : ''}>${z}%</button><button data-z="1"${!x.u ? ' disabled' : ''} aria-label="Zoom +">+</button></div>`
    + row('print', t('m.print'), 'Ctrl+P', !x.u) + row('dev', t('m.dev'), 'F12', !x.u)
    + (window.menuExtra ? menuExtra(x, web) : '')
    + '<div class="hr"></div>'
    + row('settings', t('m.settings'), '')
    + `<div class="foot"><i class="dot"></i>${t('m.saver', { n: S.asleep || 0 })}${S.eco ? ' · Eco' : ''}</div></div>`;
}

function extMenuHtml() {
  const r = $('extBtn').getBoundingClientRect(), sw = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--sw')) || 0;
  return `<div class="card" role="menu" style="left:${Math.max(6, r.right - 280 - sw)}px;top:6px;width:280px">`
    + (S.exts || []).map((e) => { const on = (C().pinExt || []).includes(e.id); return `<div class="row xrow"><button class="row" style="padding:0;flex:1;min-width:0" role="menuitem" data-xo="${esc(e.id)}" data-page="${esc(e.popup || e.options || '')}" ${e.popup || e.options ? '' : 'disabled'}>${e.icon ? `<img src="${esc(e.icon)}" alt="" width="18" height="18">` : I.puzzle}<span class="tt">${esc(e.name)}</span></button><button class="icb xpinb" data-xside="${esc(e.id)}" data-page="${esc(e.side || e.popup || '')}" ${e.side || e.popup ? '' : 'disabled'} title="${esc(t('x.side'))}" aria-label="${esc(t('x.side'))}">${I.side}</button><button class="icb xpinb${on ? ' on' : ''}" data-xpin="${esc(e.id)}" aria-pressed="${on}" title="${esc(t(on ? 'x.unpin' : 'x.pin'))}" aria-label="${esc(t(on ? 'x.unpin' : 'x.pin'))}">${I.pin}</button></div>`; }).join('')
    + '<div class="hr"></div>' + row('exts', t('m.exts'), '') + '</div>';
}

/* Menu contextuel d'un onglet / d'un groupe */
function ctxHtml() {
  const hh = $('hdr').offsetHeight, sw = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--sw')) || 0;
  const pos = `left:${Math.max(6, Math.min(ctx.x - sw, innerWidth - sw - 290))}px;top:${Math.max(6, ctx.y - hh)}px;width:280px`;
  if (ctx.g) {
    const g = grp(ctx.g); if (!g) return '';
    return `<div class="card" role="dialog" style="${pos};padding:12px"><input class="inp" id="gname" style="width:100%" value="${esc(g.name)}" placeholder="${esc(t('g.name'))}" aria-label="${esc(t('g.name'))}">`
      + `<div class="sw2" style="margin:12px 2px" role="radiogroup">${Object.keys(GC).map((k) => `<button style="--c:${GC[k]}" role="radio" aria-checked="${g.color === k}" aria-label="${k}" data-gcol="${k}"></button>`).join('')}</div>`
      + '<div class="hr"></div>' + row('grp-ungroup', t('g.ungroup')) + row('grp-close', t('g.close'), '', false, 'red') + '</div>';
  }
  const x = S.tabs.find((y) => y.id === ctx.id) || {};
  const others = S.groups.filter((g) => g.id !== x.g);
  return `<div class="card" role="menu" style="${pos}">`
    + row('ctx-new', t('ctx.newRight')) + row('pin', t(x.pin ? 'ctx.unpin' : 'ctx.pin')) + row('reload-tab', t('ctx.reload'), '', !x.u) + row('dup', t('ctx.dup'), '', !x.u) + row('tab-window', t('ctx.toWin'), '', S.tabs.length < 2)
    + '<div class="hr"></div>' + row('grp-new', t('ctx.newGroup'))
    + others.map((g) => `<button class="row" role="menuitem" data-m="grp-add" data-g="${g.id}"><span class="ic" style="background:${GC[g.color]}"></span><span class="tt">${esc(t('ctx.toGroup', { g: g.name || t('g.unnamed') }))}</span></button>`).join('')
    + (x.g ? row('grp-rm', t('ctx.ungroup')) : '')
    + (window.ctxExtra ? ctxExtra(x) : '')
    + '<div class="hr"></div>' + row('close-others', t('ctx.others'), '', S.tabs.length < 2) + row('close-right', t('ctx.right'), '', S.tabs.indexOf(x) === S.tabs.length - 1)
    + row('close', t('ctx.close'), 'Ctrl+W') + '</div>';
}
window.renameGroup = (g) => { const el = document.querySelector(`[data-g="${g}"]`); const r = el ? el.getBoundingClientRect() : { left: 100, bottom: 40 }; ctx = { g, x: r.left, y: r.bottom + 4 }; mode = null; setMode('ctx'); setTimeout(() => $('gname')?.focus(), 40); };

/* ---------- Télécharger vidéo / audio (yt-dlp + FFmpeg) ---------- */
const MED = { state: 'idle', info: null, url: '', h: 0, all: false, msg: '', tool: '' };
const VIDEO_SITES = /(^|\.)(youtube\.com|youtu\.be|vimeo\.com|dailymotion\.com|twitch\.tv|tiktok\.com|instagram\.com|x\.com|twitter\.com|facebook\.com|reddit\.com|soundcloud\.com|bandcamp\.com|bilibili\.com|rumble\.com|kick\.com|streamable\.com)$/;
const isVideoSite = () => { const u = act().u || ''; return /^https?:/.test(u) && VIDEO_SITES.test(host(u)); };
function openMedia() {
  const u = act().u || '';
  if (!/^https?:/.test(u)) return;
  if (MED.url !== u || MED.state === 'error') { MED.state = 'loading'; MED.info = null; MED.url = u; MED.all = false; send('media-probe', { u }); }
  setMode('media');
}
window.MEDIA = (v) => {
  MED.state = v.state;
  if (v.info) { MED.info = v.info; MED.h = 0; }
  if (v.msg) MED.msg = v.msg;
  if (v.tool) MED.tool = v.tool;
  if (mode === 'media') renderPanel();
};
const fmtDur = (s) => { s = Math.round(s || 0); const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60, x = s % 60; return (h ? h + ':' + String(m).padStart(2, '0') : m) + ':' + String(x).padStart(2, '0'); };
function renderMedia(under) {
  const close = `<button class="icb" data-m="close-ov" aria-label="${esc(t('b.close'))}" style="position:absolute;top:12px;right:12px">${I.x}</button>`;
  let body = '';
  const spin = (txt) => `<div class="medwait"><i class="spin" style="width:28px;height:28px;border-width:3px"></i><p>${txt}</p></div>`;
  if (MED.state === 'need') {
    body = `<div class="medhead"><span class="medic">${I.film}</span><div><h3>${t('md.setupT')}</h3><p>${t('md.setupP')}</p></div></div>
      <ul class="medlist"><li><b>yt-dlp</b> · 23 MB — ${t('md.ytdlp')}</li><li><b>FFmpeg</b> · 115 MB — ${t('md.ffmpeg')}</li></ul>
      <p class="mednote">${t('md.legal')}</p>
      <div class="acts"><button class="pill" data-m="close-ov">${t('b.cancel')}</button><button class="pill pri" data-md="install">${t('md.install')}</button></div>`;
  } else if (MED.state === 'installing') body = spin(t('md.installing', { t: MED.tool || 'yt-dlp' }));
  else if (MED.state === 'loading') body = spin(t('md.loading'));
  else if (MED.state === 'error') body = `<h3>${t('md.err')}</h3><p class="mednote" style="user-select:text">${esc(MED.msg)}</p><div class="acts"><button class="pill" data-m="close-ov">${t('b.close')}</button><button class="pill pri" data-md="retry">${t('md.retry')}</button></div>`;
  else if (MED.state === 'info' && MED.info) {
    const i = MED.info, max = i.maxh || 0;
    const qs = [0, 2160, 1440, 1080, 720, 480, 360].filter((h) => h === 0 || h <= max);
    const chip = (k, label, sub) => `<button class="medfmt" data-md="get" data-k="${k}"><b>${label}</b><span>${sub}</span></button>`;
    const all = (i.formats || []).filter((f) => f.v || f.a).sort((a, b) => (b.h || 0) - (a.h || 0) || (b.abr || 0) - (a.abr || 0));
    body = `<div class="medinfo">${i.thumb ? `<img src="${esc(i.thumb)}" alt="">` : ''}<div><h3>${esc(i.title)}</h3><p>${esc(i.by || '')}${i.dur ? ' · ' + fmtDur(i.dur) : ''}${i.site ? ' · ' + esc(i.site) : ''}</p></div></div>
      <div class="medsec"><div class="ghd">${t('md.video')}</div><div class="seg" role="radiogroup" aria-label="${esc(t('md.quality'))}" style="margin-bottom:10px">${qs.map((h) => `<button role="radio" aria-checked="${MED.h === h}" data-md="h" data-h="${h}">${h ? h + 'p' : t('md.best')}</button>`).join('')}</div>
      <div class="medgrid">${chip('mp4', 'MP4', t('md.mp4'))}${chip('webm', 'WEBM', t('md.webm'))}${chip('mkv', 'MKV', t('md.mkv'))}${chip('mov', 'MOV', t('md.mov'))}</div></div>
      <div class="medsec"><div class="ghd">${t('md.audio')}</div><div class="medgrid">${chip('mp3', 'MP3', t('md.mp3'))}${chip('m4a', 'M4A', 'AAC')}${chip('wav', 'WAV', t('md.wav'))}${chip('flac', 'FLAC', t('md.flac'))}${chip('opus', 'OPUS', t('md.opus'))}${chip('ogg', 'OGG', 'Vorbis')}</div></div>
      <button class="pill" data-md="all" style="margin-top:4px">${t('md.all', { n: all.length })} ${MED.all ? '▴' : '▾'}</button>
      ${MED.all ? `<div class="medall"><table class="nt"><thead><tr><th>ID</th><th>${t('dt.type')}</th><th>${t('md.res')}</th><th>Codec</th><th>${t('dt.size')}</th><th></th></tr></thead><tbody>${all.map((f) => `<tr><td>${esc(f.id)}</td><td>${esc(f.ext)}</td><td>${f.v ? esc(f.res || '') + (f.fps ? ' ' + Math.round(f.fps) + 'fps' : '') : t('md.audioOnly')}</td><td>${esc([f.v ? f.vc : '', f.a ? f.ac : ''].filter((x) => x && x !== 'none').join(' + ').slice(0, 40))}</td><td>${f.size ? fmtB(f.size) : ''}</td><td><button class="mini" data-md="get" data-k="raw" data-f="${esc(f.id)}">${I.chev}</button></td></tr>`).join('')}</tbody></table></div>` : ''}
      <p class="mednote">${t('md.legal')}</p>`;
  }
  $('ov').innerHTML = under + `<div class="modal"><div class="card med" role="dialog" aria-modal="true" aria-label="${esc(t('md.title'))}">${close}${body}</div></div>`;
}
$('ov').addEventListener('click', (e) => {
  const b = e.target.closest('[data-md]');
  if (!b) return;
  const d = b.dataset;
  if (d.md === 'install') { MED.state = 'installing'; renderPanel(); return send('media-install', { u: MED.url }); }
  if (d.md === 'retry') { MED.state = 'loading'; renderPanel(); return send('media-probe', { u: MED.url }); }
  if (d.md === 'h') { MED.h = +d.h; return renderPanel(); }
  if (d.md === 'all') { MED.all = !MED.all; return renderPanel(); }
  if (d.md === 'get') {
    send('media-get', { u: MED.info.url || MED.url, kind: d.k, fmt: d.f || '', h: MED.h, title: MED.info.title || '' });
    toast('md.started');
    setMode('dl');
  }
});

function renderUnlock(under) {
  $('ov').innerHTML = under + `<div class="modal"><div class="card" role="dialog" aria-modal="true" aria-labelledby="ult"><h3 id="ult">${t('unlock.t')}</h3><p>${t('unlock.p')}</p>`
    + `<form id="ulf" class="form"><input class="inp" type="password" id="ulp" placeholder="${esc(t('unlock.ph'))}" aria-label="${esc(t('unlock.ph'))}" autocomplete="off"><div class="err" id="ule" role="alert"></div></form>`
    + `<div class="acts">${S.vault && S.vault.hello ? `<button class="pill" id="ulh">${t('hl.use')}</button><span style="flex:1"></span>` : ''}<button class="pill" data-m="close-ov">${t('b.cancel')}</button><button class="pill pri" id="ulb">${t('b.unlock')}</button></div></div></div>`;
  const go = (e) => { e && e.preventDefault(); if ($('ulp').value) send('vault-unlock', { pw: $('ulp').value }); };
  $('ulf').onsubmit = go; $('ulb').onclick = go;
  if ($('ulh')) $('ulh').onclick = () => send('hello-unlock');
  setTimeout(() => $('ulp')?.focus(), 30);
  // Windows Hello configuré : demandé tout de suite (le mot de passe maître reste possible)
  if (S.vault && S.vault.hello && !renderUnlock.asked) { renderUnlock.asked = true; send('hello-unlock'); }
}
window.vaultRes = (r) => {
  if (r.hello && !r.ok) return; // Windows Hello annulé : on reste sur le mot de passe
  renderUnlock.asked = false;
  if (window.onVaultRes) onVaultRes(r);
  if (mode !== 'unlock') return;
  if (r.ok) setMode(null, true);
  else { $('ule').textContent = t('unlock.wrong'); $('ulp').select(); }
};

function renderHist() {
  const prev = document.querySelector('.hist-q');
  const f = (prev ? prev.value : '').toLowerCase();
  const list = H.filter((h) => !f || ((h.t || '') + h.u).toLowerCase().includes(f));
  let day = '', rows = '';
  for (const h of list.slice(0, 400)) {
    const d = new Date(h.n || 0), lbl = dayLabel(d);
    if (lbl !== day) { day = lbl; rows += `<div class="day">${lbl}</div>`; }
    rows += `<div class="row" style="padding-right:4px"><button class="row" style="padding:0;flex:1;min-width:0" data-u="${esc(h.u)}">${icon(h.u, h.f)}<span class="tt">${esc(h.t || host(h.u))}</span><span class="sub">${esc(host(h.u))}</span></button><span class="when">${d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</span><button class="icb" data-hrm="${esc(h.u)}" aria-label="${esc(t('b.remove'))}">${I.x}</button></div>`;
  }
  if (!prev) {
    $('ov').innerHTML = `<div class="page"><div class="pgw"><div class="pgh"><h2>${t('hist.title')}</h2><input class="search hist-q" placeholder="${esc(t('hist.search'))}" aria-label="${esc(t('hist.search'))}" spellcheck="false"><button class="pill red" data-m="histdel">${t('hist.clear')}</button><button class="icb" data-m="close-ov" aria-label="${esc(t('b.close'))}">${I.x}</button></div><div class="group hl"></div></div></div>`;
    document.querySelector('.hist-q').oninput = renderHist;
    setTimeout(() => document.querySelector('.hist-q')?.focus(), 30);
  }
  document.querySelector('.hl').innerHTML = rows || `<div class="empty">${t('hist.empty')}</div>`;
}
function dayLabel(d) {
  const n = new Date(); n.setHours(0, 0, 0, 0);
  const diff = Math.round((n - new Date(d).setHours(0, 0, 0, 0)) / 864e5);
  return diff <= 0 ? t('hist.today') : diff === 1 ? t('hist.yesterday') : d.toLocaleDateString(LANG, { weekday: 'long', day: 'numeric', month: 'long' });
}

const fmtB = (b) => b > 1e9 ? (b / 1e9).toFixed(1) + ' GB' : b > 1e6 ? (b / 1e6).toFixed(1) + ' MB' : b > 1e3 ? Math.round(b / 1e3) + ' kB' : (b || 0) + ' B';
const dlBtnHtml = (a, id, label, ic, cls) => `<button class="icb${cls || ''}" data-dl="${a}" data-id="${id}" aria-label="${esc(label)}" title="${esc(label)}">${ic}</button>`;
const dlPct = (d) => d.total > 0 ? Math.min(100, d.recv / d.total * 100) : 0;
const dlStatus = (d) => (d.state === 'run' ? `${fmtB(d.recv)}${d.total > 0 ? ' / ' + fmtB(d.total) : ''}` : d.state === 'done' ? `${t('dl.done')} · ${fmtB(d.total || d.recv)}` : t('dl.fail')) + ' · ' + host(d.url);
function dlRow(d) {
  const acts = d.state === 'run' ? dlBtnHtml('cancel', d.id, t('dl.cancel'), I.x, ' red')
    : d.state === 'done' ? dlBtnHtml('open', d.id, t('dl.open'), I.file) + dlBtnHtml('show', d.id, t('dl.show'), I.folder) : '';
  return `<div class="row dlr" data-dlid="${d.id}" style="height:auto;padding:10px 12px">${I.file}<div class="meta"><b>${esc(d.name)}</b><span>${esc(dlStatus(d))}</span>${d.state === 'run' ? `<div class="bar2"><i style="width:${dlPct(d)}%"></i></div>` : ''}</div>${acts}</div>`;
}
function renderDl() {
  ovSet(`<div class="page"><div class="pgw"><div class="pgh"><h2>${t('dl.title')}</h2><button class="pill" data-m="dl-clear"${DLS.length ? '' : ' disabled'}>${t('dl.clear')}</button><button class="icb" data-m="close-ov" aria-label="${esc(t('b.close'))}">${I.x}</button></div><div class="group">${DLS.map(dlRow).join('') || `<div class="empty">${t('dl.empty')}</div>`}</div></div></div>`);
}
/* Bulle des téléchargements sous le bouton : les 4 derniers, ouverte au début d'un téléchargement ou par le bouton.
   Ouverte toute seule, elle se referme seule (vite une fois tout terminé), jamais pendant qu'on la survole. */
const DLP = { auto: false, t0: 0, timer: 0 };
const dlPop = () => `<div class="dlpop"><h3 class="ph3">${t('dl.title')}</h3><div class="group">${DLS.slice(0, 4).map(dlRow).join('') || `<div class="empty">${t('dl.empty')}</div>`}</div>`
  + `<button class="dlall" data-m="dl">${t('dl.all')}<kbd>Ctrl+J</kbd></button></div>`;
const dlPopOpen = () => mode === 'pop' && POP.html === dlPop;
function openDlPop(auto) {
  Object.assign(DLP, { auto, t0: Date.now() });
  openPop(dlPop);
  clearTimeout(DLP.timer);
  const tick = () => {
    if (!dlPopOpen()) return;
    const age = Date.now() - DLP.t0, run = DLS.some((d) => d.state === 'run');
    if (DLP.auto && !document.querySelector('#ov .pop:hover') && (age > 8000 || (!run && age > 3500))) return setMode(null);
    DLP.timer = setTimeout(tick, 600);
  };
  DLP.timer = setTimeout(tick, 600);
}
// rappel « navigateur par défaut » : version installée, pas encore par défaut, au plus une fois par semaine
function defBar() {
  const c = C();
  if (!S.installed || S.isDefault || c.defAsk === false || !c.onboarded || ONB.on || act().p) return '';
  if (Date.now() - (+c.defLast || 0) < 7 * 864e5) return '';
  return `<span class="ii">${I.globe}</span><span class="msg">${t('def.bar')}</span><button class="pill pri" data-m="def-set">${t('def.btn')}</button>`
    + `<button class="pill" data-m="def-later">${t('def.later')}</button><button class="pill" data-m="def-never">${t('def.never')}</button>`;
}

function go(u) {
  if (!u.trim()) return;
  const pg = /^plume:\/\/([\w-]+)/i.exec(u.trim());
  if (pg && window.openPage) { typed = ''; prevMode = null; $('sug').innerHTML = ''; $('q').blur(); mode = null; return openPage(pg[1].toLowerCase()); }
  if (window.goHook && goHook(u)) return;
  mode = null; prevMode = null; typed = ''; $('sug').innerHTML = ''; $('q').blur(); send('go', { u });
}

/* ---------- Barre d'adresse ---------- */
const q = $('q');
q.addEventListener('focus', () => {
  q.value = act().u || ''; q.select(); typed = ''; $('lock').classList.add('hide');
  if (['set', 'hist', 'dl'].includes(mode)) prevMode = mode;
  setMode('omni');
});
// Quitter la barre d'adresse : retour au panneau ouvert avant (même section, même défilement)
function leaveOmni(focusPage) {
  typed = ''; $('sug').innerHTML = '';
  if (prevMode) { mode = prevMode; prevMode = null; }
  else if (mode === 'omni') setMode(null, focusPage);
  showUrl();
}
q.addEventListener('blur', () => setTimeout(() => {
  if (document.activeElement === q) return;
  if (mode === 'omni') leaveOmni(false); else showUrl();
}, 150));
q.addEventListener('input', (e) => {
  typed = q.value; sel = 0;
  if (e.inputType === 'insertText' && q.selectionStart === q.value.length) {
    const s = q.value.toLowerCase();
    if (s.length > 1 && !s.includes(' ')) {
      const hit = [...S.bm, ...H].map((h) => host(h.u)).find((h) => h.startsWith(s));
      if (hit && hit !== s) { q.value = q.value + hit.slice(s.length); q.setSelectionRange(s.length, q.value.length); typed = q.value; }
    }
  }
  renderPanel();
});
q.addEventListener('keydown', (e) => {
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    if (!items.length || !typed.trim()) return;
    e.preventDefault();
    sel = (sel + (e.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length;
    renderSuggest();
  } else if (e.key === 'Enter') {
    e.preventDefault();
    if (items[sel] && items[sel].calc) { navigator.clipboard.writeText(items[sel].calc.replace(/[  ]/g, ' ')).then(() => toast('calc-copied')).catch(() => {}); return; }
    go(sel > 0 && items[sel] ? items[sel].u : q.value);
  } else if (e.key === 'Escape') {
    e.preventDefault(); leaveOmni(true); q.blur();
  }
});
$('omni').addEventListener('mousedown', (e) => {
  if (e.target === $('omni')) { e.preventDefault(); q.focus(); }
  // bouclier, QR, lecteur, favori… : pas de prise de focus, sinon la barre se met en mode saisie, cache ces icônes et le clic se perd
  else if (e.target.closest('.ob')) e.preventDefault();
});

/* ---------- Clics ---------- */
document.addEventListener('mousedown', (e) => {
  if (e.target.closest('#sug .row[data-i],[data-fake]')) e.preventDefault();
  // clic dans l'interface → la popup d'extension se ferme (sauf sur son propre bouton)
  if (S.popup && !e.target.closest('[data-xo]')) { S.popup = false; send('popup-close'); }
});
document.addEventListener('click', (e) => {
  if (e.target.closest('#onb,#dt')) return;
  const fe = e.target.closest('[data-f]');
  if (fe && window.featClick && featClick(fe.dataset.f, fe, e)) { e.preventDefault(); return; } // data-f d'un module (pas les formats vidéo)
  if (e.target.closest('[data-fake]')) return q.focus();
  const spb = e.target.closest('[data-sp]');
  if (spb) return send(spb.dataset.sp);
  const xs = e.target.closest('[data-xside]');
  if (xs) { setMode(null); send('ext-open', { id: xs.dataset.xside, page: xs.dataset.page, how: 'side' }); return; }
  const xp = e.target.closest('[data-xpin]');
  if (xp) { const l = C().pinExt || [], id = xp.dataset.xpin; send('set', { k: 'pinExt', v: l.includes(id) ? l.filter((x) => x !== id) : [...l, id] }); return; }
  const el = e.target.closest('[data-c],[data-x],[data-del],[data-hrm],[data-u],[data-i],[data-m],[data-z],[data-g],[data-gcol],[data-info],[data-upd],[data-dl],[data-xo],#new,#vnew,#vcol,#back,#fwd,#reload,#star,#menuBtn,#shield,#readerBtn,#extBtn,#dlBtn,#mediaBtn');
  if (!el) { if (e.target === $('ov') && mode) setMode(null, true); return; }
  if (el.closest('.set') && !el.dataset.m) return; // les réglages gèrent leurs propres clics
  const d = el.dataset;
  if (d.c) return send(d.c);
  if (d.x) { e.stopPropagation(); return send('close', { id: +d.x }); }
  if (d.del) { e.stopPropagation(); return send('bmdel', { u: d.del }); }
  if (d.hrm) return send('histrm', { u: d.hrm });
  if (d.info) return send('info', { a: d.info, i: +(d.i || 0) });
  if (d.upd) return updAct(d.upd);
  if (d.dl) { send('dl-' + d.dl, { id: +d.id }); if (d.dl !== 'cancel' && dlPopOpen()) setMode(null); return; }
  if (d.xo) {
    const anchor = el.classList.contains('xpin') ? el : $('extBtn');
    if (mode) setMode(null);
    return send('ext-open', { id: d.xo, page: d.page, how: 'popup', x: anchor.getBoundingClientRect().right });
  }
  if (d.gcol) { send('grp-set', { g: ctx.g, color: d.gcol }); return; }
  if (d.i && mode === 'omni') { const it = items[+d.i]; if (it && it.calc) { navigator.clipboard.writeText(it.calc).then(() => toast('calc-copied')).catch(() => {}); return; } return go(it.u); }
  if (d.u) return go(d.u);
  if (d.z) return send('zoom', { d: +d.z });
  if (d.g && !d.m) { const g = grp(+d.g); if (g) send('grp-set', { g: g.id, col: !g.col }); return; }
  if (d.m) return menuAction(d.m, d);
  switch (el.id) {
    case 'new': case 'vnew': return send('new');
    case 'vcol': store.set('vtn', !store.get('vtn', false)); return applyCfg();
    case 'back': return send('back');
    case 'fwd': return send('fwd');
    case 'reload': return send(act().l ? 'stop' : 'reload');
    case 'star': return send('star');
    case 'shield': return C().privReport && window.openPriv ? (mode === 'pop' ? setMode(null, true) : openPriv()) : setMode(mode === 'menu' ? null : 'menu', true);
    case 'readerBtn': return send('reader');
    case 'mediaBtn': return openMedia();
    case 'extBtn': return setMode(mode === 'ext' ? null : 'ext', true);
    case 'dlBtn': return dlPopOpen() || mode === 'dl' ? setMode(null) : openDlPop(false);
    case 'menuBtn': return setMode(mode === 'menu' ? null : 'menu', true);
  }
});
function menuAction(m, d) {
  const id = ctx && ctx.id;
  const close = () => setMode(null, true);
  switch (m) {
    case 'hist': return setMode('hist');
    case 'dl': return setMode('dl');
    case 'settings': return setMode('set');
    case 'update': setMode(null); return updInstall();
    case 'update-restart': setMode(null); return send('update-restart');
    case 'media': mode = null; return openMedia();
    case 'pw': SET.sec = 'pw'; return setMode('set');
    case 'exts': SET.sec = 'ext'; return setMode('set');
    case 'close-ov': return close();
    case 'clear': if (!confirmClear) { confirmClear = true; return renderPanel(); } break;
    case 'histdel': return send('histdel');
    case 'dl-clear': return send('dl-clear');
    case 'def-set': case 'def-later': case 'def-never': return send(m);
    case 'ctx-new': close(); send('act', { id }); return send('new');
    case 'reload-tab': case 'dup': case 'close-others': case 'close-right': case 'grp-new': case 'tab-window': case 'pin': close(); return send(m, { id });
    case 'close': close(); return send('close', { id });
    case 'grp-add': close(); return send('grp-add', { id, g: +d.g });
    case 'grp-rm': close(); return send('grp-add', { id, g: 0 });
    case 'grp-ungroup': case 'grp-close': { const g = ctx.g; close(); return send(m, { g }); }
  }
  if (window.featAction && featAction(m, d, id)) return;
  send(m);
  close();
}
// Clic droit : menu des onglets / groupes ; clic molette : fermer / ouvrir en arrière-plan
document.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  const tb = e.target.closest('.tab,.vtab,.vpin'), g = e.target.closest('.gchip,.vgrp');
  if (tb) { ctx = { id: +tb.dataset.id, x: e.clientX, y: e.clientY }; mode = null; setMode('ctx'); }
  else if (g) { ctx = { g: +g.dataset.g, x: e.clientX, y: e.clientY }; mode = null; setMode('ctx'); }
});
document.addEventListener('change', (e) => { if (e.target.id === 'gname') send('grp-set', { g: ctx.g, name: e.target.value.trim().slice(0, 30) }); });
document.addEventListener('keydown', (e) => {
  if (e.target.id === 'gname' && e.key === 'Enter') { e.target.blur(); setMode(null, true); }
});
document.addEventListener('auxclick', (e) => {
  if (e.button !== 1) return;
  const tb = e.target.closest('.tab,.vtab'); if (tb) return send('close', { id: +tb.dataset.id });
  const u = e.target.closest('[data-u]'); if (u) send('open', { u: u.dataset.u });
});
// Clavier : Échap ferme, flèches dans les menus et les onglets
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape' && mode && mode !== 'omni') { setMode(null, true); return; }
  const card = e.target.closest && e.target.closest('#ov .card[role=menu]');
  if (card && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
    e.preventDefault();
    const rows = [...card.querySelectorAll('.row:not(:disabled),.zoom button:not(:disabled)')];
    const i = rows.indexOf(document.activeElement);
    rows[(i + (e.key === 'ArrowDown' ? 1 : rows.length - 1)) % rows.length]?.focus();
  }
  const tb = e.target.closest && e.target.closest('[role=tab]');
  if (tb) {
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); send('act', { id: +tb.dataset.id }); }
    if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.key)) {
      e.preventDefault();
      const all = [...tb.parentNode.querySelectorAll('[role=tab]')], i = all.indexOf(tb);
      all[(i + (e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : all.length - 1)) % all.length]?.focus();
    }
    if (e.key === 'Delete') send('close', { id: +tb.dataset.id });
  }
});

/* ---------- Fenêtre : déplacer, agrandir, redimensionner ---------- */
document.querySelectorAll('.rz').forEach((r) => r.addEventListener('mousedown', (e) => { e.preventDefault(); send('resize', { d: r.dataset.d }); }));
for (const id of ['top', 'tabs', 'spacer', 'bar', 'bmb', 'brand', 'info']) {
  $(id).addEventListener('mousedown', (e) => {
    if ((e.target !== $(id) && !(id === 'brand' && e.target.closest('#brand'))) || e.button !== 0) return;
    e.preventDefault();
    send(e.detail === 2 ? 'max' : 'drag');
  });
}

/* ---------- Onglets : activer au clic, réordonner par glisser ---------- */
for (const listId of ['tabs', 'vlist']) {
  const list = $(listId), vert = listId === 'vlist';
  list.addEventListener('pointerdown', (e) => {
    const tb = e.target.closest('.tab,.vtab,.vpin');
    if (!tb || e.button !== 0 || e.target.closest('.x')) return;
    const id = +tb.dataset.id;
    if (id !== S.a) send('act', { id });
    drag = { id, el: tb, p0: vert ? e.clientY : e.clientX, moved: false, vert, list, rects: [...list.querySelectorAll('.tab,.vtab')].map((c) => c.getBoundingClientRect()) };
    tb.setPointerCapture(e.pointerId);
  });
  list.addEventListener('pointermove', (e) => {
    if (!drag || drag.list !== list) return;
    const dp = (vert ? e.clientY : e.clientX) - drag.p0;
    if (!drag.moved && Math.abs(dp) < 6) return;
    if (!drag.moved) { drag.el = list.querySelector(`[data-id="${drag.id}"]`) || drag.el; drag.el.classList.add('drag'); }
    drag.moved = true;
    drag.el.style.transform = vert ? `translateY(${dp}px)` : `translateX(${dp}px)`;
  });
  const endDrag = () => {
    if (!drag || drag.list !== list) return;
    const d = drag; drag = null;
    if (!d.moved) return;
    const r = d.el.getBoundingClientRect(), mid = d.vert ? r.top + r.height / 2 : r.left + r.width / 2;
    const ids = [...list.querySelectorAll('.tab,.vtab')].map((c) => +c.dataset.id);
    let k = d.rects.findIndex((rc) => mid < (d.vert ? rc.top + rc.height / 2 : rc.left + rc.width / 2));
    const fromK = ids.indexOf(d.id);
    if (k < 0) k = ids.length - 1; else if (k > fromK) k -= 1;
    d.el.style.transform = '';
    const target = ids[k], to = S.tabs.findIndex((x) => x.id === target);
    send('move', { id: d.id, to: to < 0 ? S.tabs.length - 1 : to });
    renderTabs(); renderSide();
  };
  list.addEventListener('pointerup', endDrag);
  list.addEventListener('pointercancel', endDrag);
}

/* ---------- Dimensions de l'en-tête / barre latérale / DevTools → Rust ---------- */
let lastDim = '';
function sendH() {
  const vt = C().tabs === 'v';
  const w = vt ? (store.get('vtn', false) ? 56 : 240) : 0;
  const dh = window.DT && DT.open ? DT.h : 0;
  document.documentElement.style.setProperty('--sw', w + 'px');
  document.documentElement.style.setProperty('--dh', dh + 'px');
  const h = $('hdr').offsetHeight;
  document.documentElement.style.setProperty('--h', h + 'px');
  const k = `${h}|${w}|${dh}`;
  if (k !== lastDim) { lastDim = k; send('h', { h, w, dh }); }
}
new ResizeObserver(sendH).observe($('hdr'));
addEventListener('resize', () => { if (mode === 'omni' && typed.trim()) renderSuggest(); renderTabs(); });
matchMedia('(prefers-color-scheme: light)').addEventListener('change', applyCfg);

/* ---------- Écran de démarrage & assistant de bienvenue ---------- */
const IMP = { list: null, sel: new Set(), h: true, b: true, busy: false, done: null };
window.IMPL = (l) => { IMP.list = l; IMP.sel = new Set(l.map((x) => x.id)); if (ONB.on) renderOnb(); if (window.renderImp) renderImp(); };
const ONB = { on: false, step: 0, name: '', mode: 'full' };
// dernière étape : navigateur par défaut (version installée, pas déjà par défaut)
const onbDef = () => !!S.installed && !S.isDefault;
function renderOnb() {
  const el = $('onb');
  const dots = `<div class="steps" aria-hidden="true">${[0, 1, 2, ...(onbDef() ? [3] : [])].map((i) => `<i class="${i === ONB.step ? 'on' : ''}"></i>`).join('')}</div>`;
  let html = '';
  if (ONB.step === 3) {
    html = `<div class="defbig" aria-hidden="true">${I.globe}</div><h1>${t('onb.defT')}</h1><p class="l">${t('onb.defSub')}</p>`
      + `<div class="acts"><button class="pill" data-o="def-later">${t('def.later')}</button><button class="pill pri" data-o="def-set">${t('onb.defBtn')}</button></div>`;
  } else if (ONB.step === 0) {
    html = `<div class="seg" role="radiogroup" aria-label="Langue / Language" style="display:inline-flex;margin-bottom:26px;background:rgba(118,118,128,.24)">${[['fr', 'Français'], ['en', 'English']].map(([k, l]) => `<button role="radio" aria-checked="${LANG === k}" data-olang="${k}" style="color:#fff">${l}</button>`).join('')}</div><br>${logo()}<h1>${t('onb.hi')}</h1><p class="l">${t('onb.hiSub')}</p>`
      + `<form id="onbf"><label class="sr" for="onbn">${t('onb.name')}</label><input class="inp" id="onbn" maxlength="40" placeholder="${esc(t('onb.name'))}" value="${esc(ONB.name)}" autocomplete="given-name"></form>`
      + `<div class="acts" style="margin-top:22px"><button class="pill pri" data-o="next">${t('b.next')}</button></div>`;
  } else if (ONB.step === 1) {
    const card = (m, ic, list, tag) => `<button class="mode" role="radio" aria-checked="${ONB.mode === m}" data-om="${m}"><b>${ic}${t('f.' + m)}</b><span class="tag">${t(tag)}</span><ul>${list.map((k) => `<li>${t(k)}</li>`).join('')}</ul></button>`;
    html = `<h1>${t('onb.modeT')}</h1><p class="l">${t('onb.modeSub')}</p><div class="modes" role="radiogroup">`
      + card('ultra', svg('<path d="M9 1.5 3.5 9H8l-1 5.5L12.5 7H8z"/>'), ['onb.u1', 'onb.u2', 'onb.u3', 'onb.u4'], 'onb.uTag')
      + card('full', svg('<path d="m8 2 1.8 3.8 4.2.5-3.1 2.9.8 4.1L8 11.3l-3.7 2 .8-4.1L2 6.3l4.2-.5z"/>'), ['onb.f1', 'onb.f2', 'onb.f3', 'onb.f4'], 'onb.fTag')
      + `</div><div class="acts"><button class="pill" data-o="back">${t('b.back')}</button><button class="pill pri" data-o="next">${t('b.next')}</button></div>`
      + (ONB.mode === 'ultra' ? `<p class="small">${t('onb.later')}</p>` : '');
  } else {
    if (!IMP.list) send('imp-detect');
    const list = IMP.list || [];
    if (IMP.busy) {
      // Animation : les navigateurs choisis envoient favoris et historique vers Plume
      const src = list.filter((x) => IMP.sel.has(x.id)).slice(0, 3);
      html = `<h1>${t('onb.impRun')}</h1><p class="l">${t('onb.impRunSub')}</p>`
        + `<div class="flow" role="img" aria-label="${esc(t('i.running'))}"><div class="src">${src.map((x, i) => bTile(x, `animation-delay:${i * 0.2}s`)).join('')}</div>`
        + `<div class="lane"><i></i><i class="it">${I.star}</i><i></i><i class="it">${I.clock}</i><i></i><i class="it">${I.star}</i></div><div class="dst">${logo()}</div></div>`
        + `<p class="impstat" aria-live="polite">${t('i.running')}</p>`;
    } else if (IMP.done) {
      const e = IMP.done.errors || [];
      html = `<div class="okbig" aria-hidden="true"><svg viewBox="0 0 24 24"><path d="m5 12.5 4.5 4.5L19 7.5"/></svg></div><h1>${t('onb.impOk')}</h1>`
        + `<div class="impnums"><div><b data-count="${IMP.done.b}">0</b><span>${t('i.fav')}</span></div><div><b data-count="${IMP.done.h}">0</b><span>${t('i.hist')}</span></div></div>`
        + (e.length ? `<p class="small" style="margin:-10px 0 18px">${t('i.errors')} ${esc(e.join(' · '))}</p>` : '')
        + `<div class="acts"><button class="pill pri" data-o="finish">${t('b.next')}</button></div>`;
    } else {
      const rows = list.length ? list.map((x) => `<label class="bcard">${bTile(x)}<span class="nm"><b>${esc(x.browser)}</b><span>${esc(x.profile)}</span></span><input type="checkbox" class="rchk" data-imp="${x.id}" ${IMP.sel.has(x.id) ? 'checked' : ''}></label>`).join('')
        : `<div class="empty">${IMP.list ? t('i.none') : '…'}</div>`;
      html = `<h1>${t('onb.impT')}</h1><p class="l">${t('onb.impSub')}</p><div class="implist">${rows}</div>`
        + `<div class="acts"><button class="pill" data-o="back">${t('b.back')}</button>`
        + (list.length ? `<button class="pill" data-o="finish">${t('b.skip')}</button><button class="pill pri" data-o="imp" ${IMP.sel.size ? '' : 'disabled'}>${t('b.import')}</button>` : `<button class="pill pri" data-o="finish">${t('b.next')}</button>`)
        + '</div>';
    }
  }
  el.innerHTML = `<div class="onbcap"><i class="dr" data-oc="drag"></i><button data-oc="min" aria-label="${esc(t('t.min'))}">&#xE921;</button><button data-oc="quit" class="cl" aria-label="${esc(t('t.close'))}">&#xE8BB;</button></div><div class="box">${html}${dots}</div>`;
  countUp(el);
  if (ONB.step === 0) {
    const n = $('onbn'); setTimeout(() => n.focus(), 50);
    n.oninput = () => (ONB.name = n.value);
    $('onbf').onsubmit = (e) => { e.preventDefault(); ONB.step = 1; renderOnb(); };
  }
}
$('onb').addEventListener('mousedown', (e) => { if (e.target.dataset.oc === 'drag' && e.button === 0) { e.preventDefault(); send(e.detail === 2 ? 'max' : 'drag'); } });
$('onb').addEventListener('click', (e) => {
  const oc = e.target.closest('button[data-oc]'); if (oc) return send(oc.dataset.oc);
  const b = e.target.closest('[data-o],[data-om],[data-olang]');
  if (!b) return;
  if (b.dataset.olang) { send('set', { k: 'lang', v: b.dataset.olang }); C().lang = b.dataset.olang; applyCfg(); return renderOnb(); }
  if (b.dataset.om) { ONB.mode = b.dataset.om; return renderOnb(); }
  const o = b.dataset.o;
  if (o === 'next') ONB.step++;
  if (o === 'back') ONB.step--;
  if (o === 'imp') { IMP.busy = true; IMP.done = null; IMP.t0 = performance.now(); send('imp-run', { ids: [...IMP.sel], h: true, b: true }); }
  if (o === 'finish') { if (ONB.step !== 2 || !onbDef()) return finishOnb(); ONB.step = 3; }
  // Paramètres Windows › Applications par défaut › Plume (ou plus tard) : l'assistant se termine
  if (o === 'def-set' || o === 'def-later') { send(o); return finishOnb(); }
  renderOnb();
});
$('onb').addEventListener('change', (e) => { const id = e.target.dataset.imp; if (id != null) { e.target.checked ? IMP.sel.add(+id) : IMP.sel.delete(+id); renderOnb(); } });
function finishOnb() {
  const name = ONB.name.trim();
  send('onboard', { name, mode: ONB.mode });
  ONB.on = false;
  $('onb').classList.remove('on');
  // Animation de bienvenue (plus longue que le démarrage normal)
  const sp = document.createElement('div');
  sp.id = 'splash'; sp.className = 'welcome';
  sp.innerHTML = `<div class="in">${logo()}<h1>${esc(name ? t('onb.welcome', { n: name }) : t('onb.welcome0'))}<span>${t('onb.ready')}</span></h1><i class="ld"></i></div>`;
  document.body.appendChild(sp);
  setTimeout(() => sp.classList.add('out'), 3800);
  setTimeout(() => { sp.remove(); lastOvr = ''; send('ov', { v: false, f: !isBlank() }); if (isBlank()) focusBar(); }, 4260);
}

$('splashLogo').outerHTML = logo();
$('brand').innerHTML = logo() + '<span>Plume</span>';
const t0 = performance.now();
let splashDone = false;
function endSplash() {
  if (splashDone) return;
  splashDone = true;
  $('splash').classList.add('out');
  if (!C().onboarded) {
    ONB.on = true; $('onb').classList.add('on'); renderOnb();
  }
  // la page n'apparaît qu'une fois le fondu terminé (pas de coupure)
  setTimeout(() => { $('splash')?.remove(); lastOvr = ''; if (!mode && !ONB.on) send('ov', { v: false, f: !isBlank() }); else sendOvr(); }, 460);
}

/* ---------- Favoris : glisser-déposer pour changer leur place ---------- */
let bmDrag = -1;
document.addEventListener('dragstart', (e) => {
  const t = e.target.closest && e.target.closest('[data-bi]');
  if (!t) return;
  bmDrag = +t.dataset.bi;
  e.dataTransfer.effectAllowed = 'move';
  e.dataTransfer.setData('text/plain', t.dataset.u);
  setTimeout(() => t.classList.add('dragging'), 0);
});
document.addEventListener('dragover', (e) => {
  const t = e.target.closest && e.target.closest('[data-bi]');
  if (bmDrag < 0 || !t) return;
  e.preventDefault();
  document.querySelectorAll('[data-bi].over').forEach((x) => x !== t && x.classList.remove('over'));
  t.classList.add('over');
});
document.addEventListener('drop', (e) => {
  const t = e.target.closest && e.target.closest('[data-bi]');
  if (bmDrag < 0 || !t) return;
  e.preventDefault();
  const to = +t.dataset.bi;
  if (to !== bmDrag) send('bm-move', { from: bmDrag, to });
});
document.addEventListener('dragend', () => {
  bmDrag = -1;
  document.querySelectorAll('[data-bi].over,[data-bi].dragging').forEach((x) => x.classList.remove('over', 'dragging'));
});

/* ---------- Mises à jour : Plume vérifie GitHub au plus une fois par jour (désactivable),
   puis télécharge, vérifie (SHA-256) et installe la nouvelle version en un clic ---------- */
const UPD = { tag: '', page: '', state: '', pct: 0, err: '', hide: false };
const verNum = (v) => String(v || '').replace(/^v/, '').split(/[.-]/).slice(0, 3).map((x) => parseInt(x, 10) || 0);
const newer = (a, b) => { const x = verNum(a), y = verNum(b); for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i]; return false; };
const updBusy = () => UPD.state === 'downloading' || UPD.state === 'installing';
function checkUpdate(force) {
  if (!S.version || updBusy() || UPD.state === 'ready' || (!force && C().updates === false)) return;
  if (!force && Date.now() - store.get('updChecked', 0) < 864e5) {
    const c = store.get('updLatest', null);
    if (c && newer(c.tag, S.version)) { Object.assign(UPD, { tag: String(c.tag).replace(/^v/i, '').replace(/[^\w.+-]/g, ''), page: c.page || c.url || '', state: 'available' }); render(); }
    return;
  }
  UPD.state = 'checking'; if (mode === 'set') renderSettings(true);
  send('update-check');
}
window.UPDS = (o) => {
  o.tag = String(o.tag || '').replace(/^v/i, '').replace(/[^\w.+-]/g, ''); // affiché tel quel : « Plume 0.6.1 »
  if (o.state === 'available' || o.state === 'latest') { store.set('updChecked', Date.now()); store.set('updLatest', { tag: o.tag, page: o.page }); }
  if (o.state === 'latest') o.tag = '';
  if (o.state === 'ready' && UPD.state !== 'ready') { UPD.hide = false; toast('upd-ready', { v: o.tag }); }
  Object.assign(UPD, o);
  render(); if (mode === 'set') renderSettings(true);
};
function updInstall() {
  if (!S.installed) return send('open', { u: UPD.page || `https://github.com/${S.repo}/releases/latest` });
  Object.assign(UPD, { state: 'downloading', pct: 0, err: '' }); render(); if (mode === 'set') renderSettings(true);
  send('update-install');
}
function updAct(a) {
  if (a === 'install') return updInstall();
  if (a === 'restart') return send('update-restart');
  if (a === 'page') return send('open', { u: UPD.page || `https://github.com/${S.repo}/releases/latest` });
  if (a === 'later') {
    if (UPD.state === 'available') store.set('updLater', UPD.tag);
    else UPD.hide = true;
    if (UPD.state === 'failed') UPD.state = 'available';
    render();
  }
}
const updLabel = () => updBusy() ? t(UPD.state === 'installing' ? 'u.installing' : 'u.downloading', { v: UPD.tag, p: UPD.pct || 0 })
  : UPD.state === 'ready' ? t('u.ready', { v: UPD.tag }) : UPD.state === 'failed' ? t('u.failed', { e: esc(UPD.err || '') }) : t('u.avail', { v: UPD.tag });
// barre sous la barre d'adresse : nouvelle version, progression, redémarrage
function updBar() {
  const s = UPD.state;
  if (!UPD.tag || UPD.hide) return '';
  if (s === 'available' && store.get('updLater', '') === UPD.tag) return '';
  if (!['available', 'downloading', 'installing', 'ready', 'failed'].includes(s)) return '';
  const btns = s === 'ready' ? `<button class="pill pri" data-upd="restart">${t('u.restart')}</button>`
    : s === 'failed' ? `<button class="pill pri" data-upd="install">${t('u.retry')}</button><button class="pill" data-upd="page">${t('u.manual')}</button>`
    : s === 'available' ? `<button class="pill pri" data-upd="install">${t(S.installed ? 'u.install' : 'u.get')}</button><button class="pill" data-upd="page">${t('u.notes')}</button>`
    : `<span class="upbar" aria-hidden="true"><i style="width:${UPD.pct || 0}%"></i></span>`;
  const close = updBusy() ? '' : `<button class="icb" data-upd="later" aria-label="${esc(t('u.later'))}">${I.x}</button>`;
  return `<span class="ii up">${I.upd}</span><span class="msg">${updLabel()}</span>${btns}${close}`;
}
function updRow() {
  const s = UPD.state, hr = '<div class="hr"></div>';
  if (s === 'ready') return row('update-restart', t('u.restartMenu', { v: UPD.tag }), '', false, 'updrow') + hr;
  if (updBusy()) return row('noop', updLabel(), '', true, 'updrow') + hr;
  return UPD.tag ? row('update', t('u.menu', { v: UPD.tag }), '', false, 'updrow') + hr : '';
}
setTimeout(() => checkUpdate(false), 8000);
