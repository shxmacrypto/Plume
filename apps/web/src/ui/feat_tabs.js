/* ===== 0.7 · Onglets : espaces (#7), archive (#8), écran partagé (#9), recherche dans les onglets (#10),
   coup d'œil (#11), sessions (#12), notes (#13) ===== */
Object.assign(TXT.en, {
  'b.make': 'Create', 'sp.main': 'Main', 'sp.new': 'New space', 'sp.edit': 'Edit space', 'sp.name': 'Name', 'sp.del': 'Delete this space', 'sp.delNote': 'Its tabs move to Main.',
  'sp.title': 'Spaces', 'sp.moveTo': 'Move to “{s}”', 'sp.color': 'Color', 'sp.icon': 'Icon', 'sp.cont': 'Open new tabs in container', 'sp.noCont': 'No container',
  'ar.title': 'Archive', 'ar.lead': 'Tabs closed after {d} without a visit. Open one to bring it back.', 'ar.empty': 'Nothing in the archive.',
  'ar.clear': 'Empty the archive', 'ar.open': 'Open', 'ar.never': 'Inactive tabs are not closed automatically (Settings › Tabs).',
  'spl.with': 'Open in split screen', 'spl.close': 'Close split screen', 'spl.swap': 'Swap sides', 'spl.hint': 'Drag to resize · double-click to swap',
  'ts.title': 'Search tabs', 'ts.ph': 'Search open tabs, recently closed tabs and the archive', 'ts.open': 'Open tabs', 'ts.closed': 'Recently closed', 'ts.archive': 'Archive',
  'ts.none': 'No tab matches.', 'ts.other': 'Other window', 'ts.space': 'Space: {s}',
  'pk.open': 'Open in a tab', 'pk.close': 'Close (Esc)',
  'ss.title': 'Sessions', 'ss.lead': 'Saved sessions reopen in a new window, with their tabs asleep.', 'ss.saveWin': 'Save this window', 'ss.saveGrp': 'Save group as session',
  'ss.import': 'Import', 'ss.export': 'Export', 'ss.empty': 'No session saved yet.', 'ss.open': 'Open', 'ss.tabs': '{n} tabs', 'ss.auto': 'automatic',
  'ss.namePh': 'Session name', 'ss.menu': 'Save session…', 'toast.sess-saved': 'Session saved', 'toast.sess-exported': 'Sessions exported', 'toast.sess-imported': '{n} sessions imported',
  'nt.title': 'Note', 'nt.ctx': 'Note…', 'nt.ph': 'Write a note for this page…', 'nt.site': 'For every page of {h}', 'nt.tab': 'For this page',
  'nt.locked': 'Your notes are encrypted with your password vault. Unlock it to read and write them.', 'nt.saved': 'Saved',
});
Object.assign(TXT.fr, {
  'b.make': 'Créer', 'sp.main': 'Principal', 'sp.new': 'Nouvel espace', 'sp.edit': "Modifier l'espace", 'sp.name': 'Nom', 'sp.del': 'Supprimer cet espace', 'sp.delNote': 'Ses onglets rejoignent Principal.',
  'sp.title': 'Espaces', 'sp.moveTo': 'Déplacer vers « {s} »', 'sp.color': 'Couleur', 'sp.icon': 'Icône', 'sp.cont': 'Ouvrir les nouveaux onglets dans le conteneur', 'sp.noCont': 'Aucun conteneur',
  'ar.title': 'Archive', 'ar.lead': 'Onglets fermés après {d} sans visite. Ouvrez-en un pour le retrouver.', 'ar.empty': "Rien dans l'archive.",
  'ar.clear': "Vider l'archive", 'ar.open': 'Ouvrir', 'ar.never': 'Les onglets inactifs ne sont pas fermés automatiquement (Paramètres › Onglets).',
  'spl.with': 'Ouvrir en écran partagé', 'spl.close': "Fermer l'écran partagé", 'spl.swap': 'Inverser les côtés', 'spl.hint': 'Glisser pour redimensionner · double-clic pour inverser',
  'ts.title': 'Rechercher un onglet', 'ts.ph': "Chercher dans les onglets ouverts, fermés récemment et l'archive", 'ts.open': 'Onglets ouverts', 'ts.closed': 'Fermés récemment', 'ts.archive': 'Archive',
  'ts.none': 'Aucun onglet ne correspond.', 'ts.other': 'Autre fenêtre', 'ts.space': 'Espace : {s}',
  'pk.open': 'Ouvrir dans un onglet', 'pk.close': 'Fermer (Échap)',
  'ss.title': 'Sessions', 'ss.lead': 'Les sessions enregistrées se rouvrent dans une nouvelle fenêtre, onglets en veille.', 'ss.saveWin': 'Enregistrer cette fenêtre', 'ss.saveGrp': 'Enregistrer le groupe comme session',
  'ss.import': 'Importer', 'ss.export': 'Exporter', 'ss.empty': 'Aucune session enregistrée pour le moment.', 'ss.open': 'Ouvrir', 'ss.tabs': '{n} onglets', 'ss.auto': 'automatique',
  'ss.namePh': 'Nom de la session', 'ss.menu': 'Enregistrer la session…', 'toast.sess-saved': 'Session enregistrée', 'toast.sess-exported': 'Sessions exportées', 'toast.sess-imported': '{n} sessions importées',
  'nt.title': 'Note', 'nt.ctx': 'Note…', 'nt.ph': 'Écrivez une note pour cette page…', 'nt.site': 'Pour toutes les pages de {h}', 'nt.tab': 'Pour cette page',
  'nt.locked': 'Vos notes sont chiffrées avec votre coffre de mots de passe. Déverrouillez-le pour les lire et les écrire.', 'nt.saved': 'Enregistré',
});

/* ---------- #7 Espaces de travail ---------- */
const SP_ICONS = {
  home: '<path d="M2.5 7.5 8 3l5.5 4.5V13a.5.5 0 0 1-.5.5H9.5V10h-3v3.5H3a.5.5 0 0 1-.5-.5z"/>',
  work: '<rect x="2" y="5" width="12" height="8.5" rx="1.5"/><path d="M5.5 5V3.5h5V5"/>',
  book: '<path d="M3 3h4.5A1.5 1.5 0 0 1 9 4.5V13a1.5 1.5 0 0 0-1.5-1.5H3zM13 3H9v10"/>',
  star: '<path d="m8 2 1.8 3.8 4.2.5-3.1 2.9.8 4.1L8 11.3l-3.7 2 .8-4.1L2 6.3l4.2-.5z"/>',
  code: '<path d="m5.5 4.5-3 3.5 3 3.5M10.5 4.5l3 3.5-3 3.5"/>',
  game: '<rect x="2" y="5" width="12" height="7" rx="3.5"/><path d="M5 7.5v2M4 8.5h2M10.5 8h.01M12 9.3h.01"/>',
  music: '<path d="M6 12V3.5l7-1.5V10"/><circle cx="4.5" cy="12" r="1.5"/><circle cx="11.5" cy="10.5" r="1.5"/>',
  heart: '<path d="M8 13.5S2.5 10 2.5 6.3A2.8 2.8 0 0 1 8 5a2.8 2.8 0 0 1 5.5 1.3C13.5 10 8 13.5 8 13.5z"/>',
};
const spList = () => S.spaces || [];
const spName = (s) => (s && s.name) || t('sp.main');
const spCur = () => spList().find((s) => s.id === (S.space || 0)) || spList()[0];
const spIcon = (s) => svg(SP_ICONS[(s && s.icon) || 'home'] || SP_ICONS.home);
FX.bar.push(() => {
  let b = $('spaceBtn');
  const want = on('spaces');
  if (!want) { if (b) b.remove(); return; }
  const side = C().spacesPos === 'side' && C().tabs === 'v';
  const host2 = side ? document.querySelector('#side .foot2') : $('tabs');
  if (!b || b.parentNode !== (side ? host2 : $('top'))) {
    if (b) b.remove();
    b = document.createElement('button');
    b.id = 'spaceBtn'; b.className = 'spbtn'; b.dataset.f = 'space-menu';
    if (side) host2.prepend(b); else $('tabs').before(b);
  }
  const s = spCur();
  b.style.setProperty('--gc', GC[(s && s.color) || 'blue']);
  b.innerHTML = spIcon(s) + `<span>${esc(spName(s))}</span>`;
  b.title = t('sp.title');
});
const spaceMenu = () => `<h3 class="ph3">${t('sp.title')}</h3>`
  + spList().map((s, i) => `<button class="row" data-f="space-go" data-sp="${s.id}" style="--gc:${GC[s.color || 'blue']}"><span class="spdot">${spIcon(s)}</span><span class="tt">${esc(spName(s))}</span>${s.id === (S.space || 0) ? `<span class="chk">${I.check}</span>` : `<span class="kb">${i < 9 ? 'Ctrl+Alt+' + (i + 1) : ''}</span>`}</button>`).join('')
  + '<div class="hr"></div>' + `<button class="row" data-f="space-new"><span class="tt">+ ${t('sp.new')}</span></button><button class="row" data-f="space-edit" data-sp="${S.space || 0}"><span class="tt">${t('sp.edit')}</span></button>`;
FX.click['space-menu'] = () => openPop(spaceMenu);
FX.click['space-go'] = (el) => { setMode(null); send('space', { sp: +el.dataset.sp }); };
const SPE = { sp: null };
function spaceForm() {
  const s = SPE.sp, isNew = s.id == null;
  const conts = (C().contList || []);
  return `<h3>${t(isNew ? 'sp.new' : 'sp.edit')}</h3>`
    + `<div class="form" style="margin-top:12px"><label class="sr" for="spn">${t('sp.name')}</label><input class="inp" id="spn" maxlength="30" placeholder="${esc(t('sp.name'))}" value="${esc(s.name || '')}">`
    + `<div class="ghd" style="padding:6px 0 0">${t('sp.color')}</div><div class="sw2" role="radiogroup">${Object.keys(GC).map((k) => `<button style="--c:${GC[k]}" role="radio" aria-label="${k}" aria-checked="${(s.color || 'blue') === k}" data-f="sp-col" data-v="${k}"></button>`).join('')}</div>`
    + `<div class="ghd" style="padding:6px 0 0">${t('sp.icon')}</div><div class="spicons">${Object.keys(SP_ICONS).map((k) => `<button class="icb${(s.icon || 'home') === k ? ' on' : ''}" data-f="sp-ic" data-v="${k}" aria-label="${k}">${svg(SP_ICONS[k])}</button>`).join('')}</div>`
    + (on('containers') && conts.length ? `<div class="ghd" style="padding:6px 0 0">${t('sp.cont')}</div><select class="sel2" id="spc"><option value="">${t('sp.noCont')}</option>${conts.map((c) => `<option value="${c.id}" ${s.cont === c.id ? 'selected' : ''}>${esc(c.name)}</option>`).join('')}</select>` : '')
    + `</div><div class="acts" style="margin-top:16px">${!isNew && s.id !== 0 ? `<button class="pill red" data-f="sp-del" title="${esc(t('sp.delNote'))}">${t('sp.del')}</button><span style="flex:1"></span>` : ''}<button class="pill" data-m="close-ov">${t('b.cancel')}</button><button class="pill pri" data-f="sp-save">${t(isNew ? 'b.make' : 'b.save')}</button></div>`;
}
FX.click['space-new'] = () => { SPE.sp = { name: '', color: Object.keys(GC)[(spList().length) % 8], icon: 'work' }; openModal(spaceForm); setTimeout(() => $('spn')?.focus(), 40); };
FX.click['space-edit'] = (el) => { SPE.sp = Object.assign({}, spList().find((s) => s.id === +el.dataset.sp) || { id: 0 }); openModal(spaceForm); };
FX.click['sp-col'] = (el) => { SPE.sp.name = $('spn').value; SPE.sp.color = el.dataset.v; renderPanel(); };
FX.click['sp-ic'] = (el) => { SPE.sp.name = $('spn').value; SPE.sp.icon = el.dataset.v; renderPanel(); };
FX.click['sp-save'] = () => {
  const s = SPE.sp, name = $('spn').value.trim().slice(0, 30), cont = $('spc') ? $('spc').value : undefined;
  setMode(null, true);
  if (s.id == null) send('space-new', { name, color: s.color, icon: s.icon });
  else send('space-set', { sp: s.id, name, color: s.color, icon: s.icon, cont: cont === undefined ? undefined : (cont ? +cont : null) });
};
FX.click['sp-del'] = () => { const id = SPE.sp.id; setMode(null, true); send('space-del', { sp: id }); };
FX.ctx.push((x) => on('spaces') && spList().length > 1 ? spList().filter((s) => s.id !== (S.space || 0)).map((s) => `<button class="row" role="menuitem" data-m="tab-space" data-sp="${s.id}"><span class="spdot" style="--gc:${GC[s.color || 'blue']}">${spIcon(s)}</span><span class="tt">${esc(t('sp.moveTo', { s: spName(s) }))}</span></button>`).join('') : '');
FX.act['tab-space'] = (d, id) => send('tab-space', { id, sp: +d.sp });

/* ---------- #8 Archive (plume://archive) ---------- */
let ARCH_L = null;
window.ARCH = (l) => { ARCH_L = l; if (mode === 'pg' && PG.name === 'archive') renderPage(); };
FX.pages.archive = {
  search: 1,
  title: () => t('ar.title'),
  open: () => send('archive-get'),
  actions: () => ARCH_L && ARCH_L.length ? `<button class="pill red" data-f="ar-clear">${t('ar.clear')}</button>` : '',
  body: (f) => {
    if (!ARCH_L) return `<div class="empty">…</div>`;
    const lead = on('autoClose') ? `<p class="lead">${t('ar.lead', { d: t('k.autoCloseAfter.' + (C().autoCloseAfter || '7d')) })}</p>` : `<p class="lead">${t('ar.never')}</p>`;
    const rows = ARCH_L.map((a, i) => [a, i]).filter(([a]) => !f || ((a.t || '') + ' ' + a.u).toLowerCase().includes(f)).slice(0, 500)
      .map(([a, i]) => `<div class="row" style="padding-right:4px">${icon(a.u, a.f)}<span class="tt">${esc(a.t || host(a.u))}</span><span class="sub">${esc(host(a.u))} · ${new Date(a.closed).toLocaleDateString(LANG)}</span><button class="mini" data-f="ar-open" data-i="${i}">${t('ar.open')}</button><button class="icb" data-f="ar-del" data-i="${i}" aria-label="${esc(t('b.remove'))}">${I.x}</button></div>`).join('');
    return lead + `<div class="group">${rows || `<div class="empty">${t('ar.empty')}</div>`}</div>`;
  },
};
FX.click['ar-open'] = (el) => { send('archive-open', { i: +el.dataset.i }); setMode(null); };
FX.click['ar-del'] = (el) => send('archive-del', { i: +el.dataset.i });
FX.click['ar-clear'] = () => { if (confirm(t('ar.clear') + ' ?')) send('archive-clear'); };

/* ---------- #9 Écran partagé ---------- */
FX.ctx.push((x) => {
  if (!on('split')) return '';
  if (S.split && (S.split.a === x.id || S.split.b === x.id)) return row('split-close', t('spl.close'), C().splitKey || '');
  return x.id !== S.a ? row('split-with', t('spl.with')) : '';
});
FX.act['split-with'] = (d, id) => send('split-with', { id });
FX.act['split-close'] = () => send('split-close');
// séparateur déplaçable entre les deux pages (dans l'interface, sous les pages)
const SPLIT = { drag: false };
FX.bar.push(() => {
  let b = $('splitbar');
  if (!S.split) { if (b) b.remove(); return; }
  if (!b) {
    b = document.createElement('div'); b.id = 'splitbar'; b.title = t('spl.hint');
    b.innerHTML = `<i></i><button class="icb" data-f="split-x" title="${esc(t('spl.close'))}" aria-label="${esc(t('spl.close'))}">${I.x}</button>`;
    document.body.appendChild(b);
    b.addEventListener('pointerdown', (e) => { if (e.target.closest('button')) return; SPLIT.drag = true; b.setPointerCapture(e.pointerId); });
    b.addEventListener('pointermove', (e) => {
      if (!SPLIT.drag) return;
      const r = splitArea(), vert = C().splitDir !== 'h';
      const ratio = vert ? (e.clientX - r.l) / r.w : (e.clientY - r.t) / r.h;
      S.split.r = Math.min(.85, Math.max(.15, ratio)); placeSplit();
      clearTimeout(SPLIT.tm); SPLIT.tm = setTimeout(() => send('split-ratio', { r: S.split.r }), 16);
    });
    b.addEventListener('pointerup', () => { SPLIT.drag = false; });
    b.addEventListener('dblclick', () => send('split-swap'));
  }
  placeSplit();
});
function splitArea() {
  const cs = getComputedStyle(document.documentElement), px = (v) => parseFloat(cs.getPropertyValue(v)) || 0;
  const l = px('--sw'), tp = px('--h'), rr = px('--sr'), bt = px('--dh');
  return { l, t: tp, w: innerWidth - l - rr, h: innerHeight - tp - bt };
}
function placeSplit() {
  const b = $('splitbar'); if (!b || !S.split) return;
  const r = splitArea(), vert = C().splitDir !== 'h', gap = 8;
  b.classList.toggle('h', !vert);
  if (vert) Object.assign(b.style, { left: (r.l + (r.w - gap) * S.split.r) + 'px', top: r.t + 'px', width: gap + 'px', height: r.h + 'px' });
  else Object.assign(b.style, { left: r.l + 'px', top: (r.t + (r.h - gap) * S.split.r) + 'px', width: r.w + 'px', height: gap + 'px' });
}
addEventListener('resize', placeSplit);
FX.click['split-x'] = () => send('split-close');
// (un onglet glissé hors de la bande s'ouvre dans une nouvelle fenêtre : app.js ; l'écran partagé passe par son menu)

/* ---------- #10 Recherche dans les onglets (Ctrl+Maj+A) ---------- */
const TS = { q: '', data: null, sel: 0, items: [] };
window.ALLTABS = (d) => { TS.data = d; if (mode === 'modal' && POP.html === tabSearchHtml) renderTS(); };
window.openTabSearch = () => {
  if (!on('tabSearch')) return;
  TS.q = ''; TS.sel = 0; TS.data = null;
  send('all-tabs');
  openModal(tabSearchHtml);
  setTimeout(() => { const i = $('tsq'); if (i) { i.focus(); i.oninput = () => { TS.q = i.value; TS.sel = 0; renderTS(); }; i.onkeydown = tsKey; } }, 30);
};
function tabSearchHtml() { return `<input class="inp tsq" id="tsq" placeholder="${esc(t('ts.ph'))}" aria-label="${esc(t('ts.title'))}" autocomplete="off" spellcheck="false"><div id="tsres" class="tsres" role="listbox"></div>`; }
function tsResults() {
  const d = TS.data || { tabs: (S.tabs || []).map((x) => ({ id: x.id, t: x.t, u: x.u, f: x.f, here: true })), closed: [], archive: [] };
  const q = TS.q.trim(), out = [];
  const add = (kind, list, max) => {
    const r = list.map((x) => ({ x, s: fuzzyScore(q, (x.t || '') + ' ' + (x.u || '')) })).filter((r) => r.s > 0).sort((a, b) => b.s - a.s).slice(0, max);
    if (r.length) out.push({ kind, rows: r.map((y) => y.x) });
  };
  add('open', d.tabs.filter((x) => x.u), 30); add('closed', d.closed || [], 8); add('archive', d.archive || [], 10);
  return out;
}
function renderTS() {
  const el = $('tsres'); if (!el) return;
  const groups = tsResults(); TS.items = [];
  let html = '';
  for (const g of groups) {
    html += `<div class="ghd">${t(g.kind === 'open' ? 'ts.open' : g.kind === 'closed' ? 'ts.closed' : 'ts.archive')}</div>`;
    for (const x of g.rows) {
      const k = TS.items.length; TS.items.push({ kind: g.kind, x });
      const sp = g.kind === 'open' && !x.here ? (x.w !== undefined && (S.tabs || []).every((y) => y.id !== x.id) && spList().every((s) => s.id !== x.sp) ? t('ts.other') : t('ts.space', { s: spName(spList().find((s) => s.id === x.sp)) })) : '';
      html += `<button class="row${k === TS.sel ? ' sel' : ''}" role="option" data-f="ts-go" data-k="${k}">${icon(x.u, x.f)}<span class="tt">${esc(x.t || host(x.u))}</span><span class="sub">${esc(sp || host(x.u))}</span></button>`;
    }
  }
  el.innerHTML = html || `<div class="empty">${t('ts.none')}</div>`;
}
function tsKey(e) {
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); const n = TS.items.length; if (!n) return; TS.sel = (TS.sel + (e.key === 'ArrowDown' ? 1 : n - 1)) % n; renderTS(); document.querySelector('#tsres .row.sel')?.scrollIntoView({ block: 'nearest' }); }
  else if (e.key === 'Enter') { e.preventDefault(); tsGo(TS.sel); }
}
function tsGo(k) {
  const it = TS.items[k]; if (!it) return;
  setMode(null);
  if (it.kind === 'open') send('goto-tab', { id: it.x.id });
  else if (it.kind === 'closed') send('reopen-url', { u: it.x.u });
  else { const i = (TS.data.archive || []).indexOf(it.x); send('archive-open', { i }); }
}
FX.click['ts-go'] = (el) => tsGo(+el.dataset.k);
// le modal se remplit dès que la liste arrive
const TS_OBS = new MutationObserver(() => { if ($('tsres') && !$('tsres').childElementCount) renderTS(); });
TS_OBS.observe($('ov'), { childList: true });

/* ---------- #11 Coup d'œil : barre au-dessus de l'aperçu ---------- */
const PEEK = { url: '' };
window.peekOpen = (u) => { PEEK.url = u; POP.html = peekBar; mode = null; setMode('peek'); };
window.peekClosed = () => { PEEK.url = ''; if (mode === 'peek') setMode(null); };
// clic sur la page derrière l'aperçu → il se ferme
const featPageClicked0 = window.pageClicked;
window.pageClicked = () => { if (mode === 'peek') return setMode(null); featPageClicked0(); };
function peekBar() {
  const r = splitArea(), pct = (C().peekSize || 80) / 100, bar = 48;
  const w = r.w * pct, h = (r.h - bar) * pct, x = r.l + (r.w - w) / 2, y = bar + ((r.h - bar) - h) / 2 - 44;
  return `<div class="card peekbar" style="left:${x - (parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--sw')) || 0)}px;top:${Math.max(4, y)}px;width:${w}px">${icon(PEEK.url, '')}<span class="tt">${esc(PEEK.url)}</span>`
    + `<button class="pill pri" data-f="peek-tab">${t('pk.open')}</button><button class="icb" data-f="peek-x" title="${esc(t('pk.close'))}" aria-label="${esc(t('pk.close'))}">${I.x}</button></div>`;
}
FX.click['peek-tab'] = () => send('peek-tab');
FX.click['peek-x'] = () => send('peek-close');
const featSetMode1 = setMode;
setMode = function (m, f) {
  if (mode === 'peek' && m !== 'peek' && PEEK.url) { PEEK.url = ''; send('peek-close'); }
  return featSetMode1(m, f);
};
const featPanel0 = window.panelHook;
window.panelHook = (under) => {
  if (mode === 'peek') { $('ov').innerHTML = under + peekBar(); return true; }
  return featPanel0(under);
};

/* ---------- #12 Sessions (plume://sessions) ---------- */
let SESS_L = null;
window.SESS = (l) => { SESS_L = l; if (mode === 'pg' && PG.name === 'sessions') renderPage(); };
FX.pages.sessions = {
  search: 1,
  title: () => t('ss.title'),
  open: () => send('sess-list'),
  actions: () => `<button class="pill" data-f="ss-import">${t('ss.import')}</button><button class="pill" data-f="ss-export">${t('ss.export')}</button><button class="pill pri" data-f="ss-save">${t('ss.saveWin')}</button>`,
  body: (f) => {
    if (!on('sessions')) return `<div class="empty">${t('k.sessions.d')}</div>`;
    if (!SESS_L) return `<div class="empty">…</div>`;
    const rows = SESS_L.filter((s) => !f || (s.n || '').toLowerCase().includes(f) || (s.tabs || []).some((x) => ((x.t || '') + x.u).toLowerCase().includes(f))).map((s) =>
      `<div class="opt sess"><div class="lbl"><b><input class="inp sname" data-sid="${s.id}" value="${esc(s.n || '')}" aria-label="${esc(t('ss.namePh'))}"></b><span>${t('ss.tabs', { n: (s.tabs || []).length })} · ${new Date(s.t).toLocaleString(LANG, { dateStyle: 'medium', timeStyle: 'short' })}${s.auto ? ' · ' + t('ss.auto') : ''}</span>`
      + `<div class="sfav">${(s.tabs || []).slice(0, 12).map((x) => icon(x.u, x.f, true)).join('')}</div></div>`
      + `<button class="pill pri" data-f="ss-open" data-sid="${s.id}">${t('ss.open')}</button><button class="icb red" data-f="ss-del" data-sid="${s.id}" aria-label="${esc(t('b.delete'))}">${I.trash}</button></div>`).join('');
    return `<p class="lead">${t('ss.lead')}</p><div class="group">${rows || `<div class="empty">${t('ss.empty')}</div>`}</div>`;
  },
};
FX.click['ss-open'] = (el) => { send('sess-open', { sid: +el.dataset.sid }); setMode(null); };
FX.click['ss-del'] = (el) => send('sess-del', { sid: +el.dataset.sid });
FX.click['ss-import'] = () => send('sess-import');
FX.click['ss-export'] = () => send('sess-export');
FX.click['ss-save'] = () => send('sess-save', { name: new Date().toLocaleString(LANG, { dateStyle: 'medium', timeStyle: 'short' }) });
document.addEventListener('change', (e) => { const el = e.target; if (el.classList && el.classList.contains('sname')) send('sess-rename', { sid: +el.dataset.sid, name: el.value.trim() }); });
FX.menu.push(() => on('sessions') ? row('sess-page', t('ss.title'), '') : '');
FX.act['sess-page'] = () => openPage('sessions');
// enregistrer un groupe d'onglets (clic droit sur le nom du groupe)
const featCtx0 = ctxHtml;
ctxHtml = function () {
  const h = featCtx0();
  if (ctx && ctx.g && on('sessions')) return h.replace(/<\/div>$/, row('grp-sess', t('ss.saveGrp')) + '</div>');
  return h;
};
FX.act['grp-sess'] = () => { const g = grp(ctx.g); send('sess-save', { g: ctx.g, name: (g && g.name) || t('g.unnamed') }); };

/* ---------- #13 Notes d'onglet ---------- */
FX.tab.push((x, narrow) => on('notes') && x.nt && !narrow ? `<span class="tnote" title="${esc(t('nt.title'))}">${svg('<path d="M3.5 2.5h9v8l-3 3h-6z"/><path d="M9.5 13.5v-3h3M5.5 6h5M5.5 8.5h3"/>')}</span>` : '');
FX.ctx.push((x) => on('notes') && x.u && /^https?:/.test(x.u) ? row('note', t('nt.ctx')) : '');
const NOTE_S = { id: 0, u: '', text: '', locked: false };
window.NOTE = (n) => { Object.assign(NOTE_S, n); if (mode === 'modal' && POP.html === noteForm) { renderPanel(); setTimeout(() => $('ntx')?.focus(), 30); } };
FX.act.note = (d, id) => { NOTE_S.id = id; NOTE_S.text = ''; NOTE_S.locked = false; send('note-get', { id }); openModal(noteForm); };
function noteForm() {
  const x = (S.tabs || []).find((y) => y.id === NOTE_S.id) || {};
  const scope = C().notesScope === 'tab' ? t('nt.tab') : t('nt.site', { h: host(x.u || NOTE_S.u) });
  if (NOTE_S.locked) return `<h3>${t('nt.title')}</h3><p class="lead" style="margin:8px 0 16px">${t('nt.locked')}</p><div class="acts"><button class="pill" data-m="close-ov">${t('b.close')}</button><button class="pill pri" data-f="nt-unlock">${t('b.unlock')}</button></div>`;
  return `<h3>${t('nt.title')} <span class="lead" style="font-size:13px;font-weight:400">· ${esc(scope)}</span></h3>`
    + `<textarea class="inp ntx" id="ntx" placeholder="${esc(t('nt.ph'))}">${esc(NOTE_S.text)}</textarea>`
    + `<div class="acts"><span class="small2" id="ntst" style="flex:1;margin:0"></span><button class="pill pri" data-m="close-ov">${t('b.done')}</button></div>`;
}
FX.click['nt-unlock'] = () => openUnlock();
document.addEventListener('input', (e) => {
  if (e.target.id !== 'ntx') return;
  clearTimeout(NOTE_S.tm);
  NOTE_S.tm = setTimeout(() => { send('note-set', { id: NOTE_S.id, u: NOTE_S.u, text: e.target.value }); const s2 = $('ntst'); if (s2) s2.textContent = t('nt.saved'); }, 400);
});
