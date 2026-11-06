/* ===== 0.7 · Navigation : palette de commandes (#14), calculs et conversions dans la barre d'adresse (#16),
   gestes de souris (#17 : réglages ; le geste lui-même est dans page.js) ===== */
Object.assign(TXT.en, {
  'pl.ph': 'Type a command, a setting, a tab, a favorite…', 'pl.actions': 'Actions', 'pl.tabs': 'Tabs', 'pl.bm': 'Favorites', 'pl.hist': 'History', 'pl.set': 'Settings', 'pl.none': 'Nothing found.',
  'pl.on': 'on', 'pl.off': 'off', 'pl.open': 'Open in Settings',
  'a.newTab': 'New tab', 'a.newWin': 'New window', 'a.priv': 'New InPrivate window', 'a.reopen': 'Reopen closed tab', 'a.closeTab': 'Close tab', 'a.hist': 'History', 'a.dl': 'Downloads',
  'a.settings': 'Settings', 'a.pw': 'Passwords', 'a.ext': 'Extensions', 'a.tasks': 'Task manager', 'a.stats': 'Savings statistics', 'a.sess': 'Sessions', 'a.arch': 'Archive',
  'a.reader': 'Reader mode', 'a.speak': 'Read aloud', 'a.translate': 'Translate page', 'a.dark': 'Dark mode on this site', 'a.ab': 'Ad blocker on this site', 'a.print': 'Print',
  'a.dev': 'Developer tools', 'a.zoomIn': 'Zoom in', 'a.zoomOut': 'Zoom out', 'a.zoom0': 'Reset zoom', 'a.bmbar': 'Favorites bar', 'a.star': 'Add to favorites', 'a.split': 'Split screen',
  'a.tabsearch': 'Search tabs', 'a.ultra': 'Switch to Ultra-light mode', 'a.full': 'Switch to Full mode', 'a.themeDark': 'Dark theme', 'a.themeLight': 'Light theme', 'a.themeSys': 'System theme',
  'a.sessSave': 'Save this window as a session', 'a.lite': 'Lite mode for this site', 'a.note': 'Note for this page',
  'calc.copy': 'Press Enter to copy', 'calc.fx': 'Rate from {d} (ECB)', 'toast.calc-copied': 'Result copied',
  'g.L': '← left', 'g.R': '→ right', 'g.U': '↑ up', 'g.D': '↓ down', 'g.DR': '↓→ down, right', 'g.DL': '↓← down, left', 'g.UR': '↑→ up, right', 'g.UL': '↑← up, left', 'g.DU': '↓↑ down, up', 'g.UD': '↑↓ up, down', 'g.RL': '→← right, left', 'g.LR': '←→ left, right',
  'ga.none': 'Nothing', 'ga.back': 'Back', 'ga.fwd': 'Forward', 'ga.close': 'Close the tab', 'ga.new': 'New tab', 'ga.reload': 'Reload', 'ga.reopen': 'Reopen closed tab', 'ga.top': 'Scroll to top', 'ga.bottom': 'Scroll to bottom', 'ga.next': 'Next tab', 'ga.prev': 'Previous tab',
});
Object.assign(TXT.fr, {
  'pl.ph': 'Tapez une commande, un réglage, un onglet, un favori…', 'pl.actions': 'Actions', 'pl.tabs': 'Onglets', 'pl.bm': 'Favoris', 'pl.hist': 'Historique', 'pl.set': 'Paramètres', 'pl.none': 'Rien trouvé.',
  'pl.on': 'activé', 'pl.off': 'désactivé', 'pl.open': 'Ouvrir dans les paramètres',
  'a.newTab': 'Nouvel onglet', 'a.newWin': 'Nouvelle fenêtre', 'a.priv': 'Nouvelle fenêtre InPrivate', 'a.reopen': "Rouvrir l'onglet fermé", 'a.closeTab': "Fermer l'onglet", 'a.hist': 'Historique', 'a.dl': 'Téléchargements',
  'a.settings': 'Paramètres', 'a.pw': 'Mots de passe', 'a.ext': 'Extensions', 'a.tasks': 'Gestionnaire de tâches', 'a.stats': "Statistiques d'économies", 'a.sess': 'Sessions', 'a.arch': 'Archive',
  'a.reader': 'Mode lecture', 'a.speak': 'Lire à voix haute', 'a.translate': 'Traduire la page', 'a.dark': 'Mode sombre sur ce site', 'a.ab': 'Bloqueur de pubs sur ce site', 'a.print': 'Imprimer',
  'a.dev': 'Outils de développement', 'a.zoomIn': 'Zoom avant', 'a.zoomOut': 'Zoom arrière', 'a.zoom0': 'Réinitialiser le zoom', 'a.bmbar': 'Barre des favoris', 'a.star': 'Ajouter aux favoris', 'a.split': 'Écran partagé',
  'a.tabsearch': 'Rechercher un onglet', 'a.ultra': 'Passer en mode Ultra-léger', 'a.full': 'Passer en mode Complet', 'a.themeDark': 'Thème sombre', 'a.themeLight': 'Thème clair', 'a.themeSys': 'Thème du système',
  'a.sessSave': 'Enregistrer cette fenêtre comme session', 'a.lite': 'Mode Lite pour ce site', 'a.note': 'Note pour cette page',
  'calc.copy': 'Entrée pour copier', 'calc.fx': 'Taux du {d} (BCE)', 'toast.calc-copied': 'Résultat copié',
  'g.L': '← gauche', 'g.R': '→ droite', 'g.U': '↑ haut', 'g.D': '↓ bas', 'g.DR': '↓→ bas, droite', 'g.DL': '↓← bas, gauche', 'g.UR': '↑→ haut, droite', 'g.UL': '↑← haut, gauche', 'g.DU': '↓↑ bas, haut', 'g.UD': '↑↓ haut, bas', 'g.RL': '→← droite, gauche', 'g.LR': '←→ gauche, droite',
  'ga.none': 'Rien', 'ga.back': 'Retour', 'ga.fwd': 'Avancer', 'ga.close': "Fermer l'onglet", 'ga.new': 'Nouvel onglet', 'ga.reload': 'Recharger', 'ga.reopen': "Rouvrir l'onglet fermé", 'ga.top': 'Haut de la page', 'ga.bottom': 'Bas de la page', 'ga.next': 'Onglet suivant', 'ga.prev': 'Onglet précédent',
});
I.calc = svg('<rect x="3" y="2" width="10" height="12" rx="2"/><path d="M5.5 5h5M5.5 8h1M8 8h.5M10.5 8h.01M5.5 10.5h1M8 10.5h.5M10.5 10.5h.01"/>');
I.cmd = svg('<path d="M5.5 2.5a2 2 0 1 0 0 4h5a2 2 0 1 0 0-4v11a2 2 0 1 0 0-4h-5a2 2 0 1 0 0 4z"/>');

/* ---------- #14 Palette de commandes (Ctrl+K) ---------- */
const web = () => (act().u || '').startsWith('http');
const PAL_ACTIONS = [
  ['newTab', 'Ctrl+T', () => send('new')], ['newWin', 'Ctrl+N', () => send('window')], ['priv', 'Ctrl+Shift+N', () => send('private')],
  ['reopen', 'Ctrl+Shift+T', () => send('reopen'), () => S.closed], ['closeTab', 'Ctrl+W', () => send('close', { id: S.a })],
  ['tabsearch', () => C().tsKey, () => openTabSearch(), () => on('tabSearch')], ['split', () => C().splitKey, () => send('split-toggle'), () => on('split')],
  ['hist', 'Ctrl+H', () => setMode('hist')], ['dl', 'Ctrl+J', () => setMode('dl')], ['settings', '', () => setMode('set')],
  ['pw', '', () => openPage('passwords')], ['ext', '', () => { SET.sec = 'ext'; setMode('set'); }], ['tasks', 'Shift+Esc', () => openPage('tasks')],
  ['stats', '', () => openPage('stats'), () => on('stats')], ['sess', '', () => openPage('sessions'), () => on('sessions')], ['arch', '', () => openPage('archive')],
  ['sessSave', '', () => send('sess-save', { name: new Date().toLocaleString(LANG, { dateStyle: 'medium', timeStyle: 'short' }) }), () => on('sessions')],
  ['reader', 'F9', () => send('reader'), web], ['speak', '', () => send('speak'), web], ['translate', '', () => send('translate'), web],
  ['dark', '', () => send('dark-site'), web], ['ab', '', () => send('ab-site'), web], ['lite', '', () => FX.act.lite(), () => web() && on('lite')],
  ['note', '', () => FX.act.note({}, S.a), () => web() && on('notes')], ['star', 'Ctrl+D', () => send('star'), () => !!act().u], ['bmbar', 'Ctrl+Shift+B', () => send('bmbar')],
  ['print', 'Ctrl+P', () => send('print'), () => !!act().u], ['dev', 'F12', () => send('dev'), () => !!act().u],
  ['zoomIn', 'Ctrl +', () => send('zoom', { d: 1 }), () => !!act().u], ['zoomOut', 'Ctrl −', () => send('zoom', { d: -1 }), () => !!act().u], ['zoom0', 'Ctrl 0', () => send('zoom', { d: 0 }), () => !!act().u],
  ['ultra', '', () => send('mode', { mode: 'ultra' }), () => C().mode !== 'ultra'], ['full', '', () => send('mode', { mode: 'full' }), () => C().mode === 'ultra'],
  ['themeDark', '', () => send('set', { k: 'theme', v: 'dark' })], ['themeLight', '', () => send('set', { k: 'theme', v: 'light' })], ['themeSys', '', () => send('set', { k: 'theme', v: 'system' })],
];
// les autres modules ajoutent leurs actions : FX.pal.push([id, raccourci, run, condition])
FX.pal = [];
const PAL = { q: '', sel: 0, items: [] };
window.openPalette = () => {
  if (!on('palette')) return;
  PAL.q = ''; PAL.sel = 0;
  openModal(paletteHtml);
  setTimeout(() => { const i = $('palq'); if (!i) return; i.focus(); i.oninput = () => { PAL.q = i.value; PAL.sel = 0; renderPal(); }; i.onkeydown = palKey; renderPal(); }, 20);
};
function paletteHtml() { return `<div class="palhd">${I.cmd}<input class="inp tsq" id="palq" placeholder="${esc(t('pl.ph'))}" aria-label="${esc(t('k.palette'))}" autocomplete="off" spellcheck="false"></div><div id="palres" class="tsres" role="listbox"></div>`; }
function palItems() {
  const q = PAL.q.trim(), c = C(), groups = [];
  const rank = (list, max) => list.map((x) => ({ x, s: fuzzyScore(q, x.label + ' ' + (x.sub || '')) })).filter((r) => r.s > 0).sort((a, b) => b.s - a.s).slice(0, max).map((r) => r.x);
  if (c.palActions !== false) {
    const acts = [...PAL_ACTIONS, ...FX.pal].filter((a) => !a[3] || a[3]()).map((a) => ({ kind: 'act', label: t('a.' + a[0]), kb: typeof a[1] === 'function' ? a[1]() : a[1], run: a[2] }));
    groups.push(['pl.actions', rank(acts, q ? 8 : 6)]);
  }
  if (q && c.palTabs !== false) groups.push(['pl.tabs', rank((S.tabs || []).filter((x) => x.u).map((x) => ({ kind: 'tab', label: x.t || host(x.u), sub: host(x.u), u: x.u, f: x.f, id: x.id })), 6)]);
  if (q && c.palBm !== false) groups.push(['pl.bm', rank((S.bm || []).map((b) => ({ kind: 'url', label: b.t, sub: host(b.u), u: b.u, f: b.f })), 5)]);
  if (q && c.palSet !== false) {
    const sets = REG.filter((i) => i.t !== 'special' && TXT.en['k.' + i.k]).map((i) => ({ kind: 'set', label: t('k.' + i.k), sub: t(SECNAME[i.s] || 's.general'), i }));
    groups.push(['pl.set', rank(sets, 6)]);
  }
  if (q && c.palHist !== false) groups.push(['pl.hist', rank(H.slice(0, 400).map((h) => ({ kind: 'url', label: h.t || host(h.u), sub: host(h.u), u: h.u, f: h.f })), 5)]);
  return groups.filter((g) => g[1].length);
}
function renderPal() {
  const el = $('palres'); if (!el) return;
  PAL.items = [];
  let html = '';
  for (const [title, rows] of palItems()) {
    html += `<div class="ghd">${t(title)}</div>`;
    for (const x of rows) {
      const k = PAL.items.length; PAL.items.push(x);
      const ic = x.kind === 'tab' || x.kind === 'url' ? icon(x.u, x.f) : x.kind === 'set' ? I.gear : I.cmd;
      const right = x.kind === 'act' ? (C().palKeys !== false && x.kb ? `<span class="kbd">${esc(x.kb)}</span>` : '')
        : x.kind === 'set' ? (x.i.t === 'sw' ? `<span class="sub">${t(C()[x.i.k] ? 'pl.on' : 'pl.off')}</span>` : `<span class="sub">${esc(x.sub)}</span>`) : `<span class="sub">${esc(x.sub || '')}</span>`;
      html += `<button class="row${k === PAL.sel ? ' sel' : ''}" role="option" data-f="pal-go" data-k="${k}">${ic}<span class="tt">${esc(x.label)}</span>${right}</button>`;
    }
  }
  el.innerHTML = html || `<div class="empty">${t('pl.none')}</div>`;
}
function palKey(e) {
  const n = PAL.items.length;
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); if (!n) return; PAL.sel = (PAL.sel + (e.key === 'ArrowDown' ? 1 : n - 1)) % n; renderPal(); document.querySelector('#palres .row.sel')?.scrollIntoView({ block: 'nearest' }); }
  else if (e.key === 'Enter') { e.preventDefault(); palGo(PAL.sel); }
}
function palGo(k) {
  const x = PAL.items[k]; if (!x) return;
  if (x.kind === 'set' && x.i.t === 'sw') { send('set', { k: x.i.k, v: !C()[x.i.k] }); C()[x.i.k] = !C()[x.i.k]; return renderPal(); }
  setMode(null, true);
  if (x.kind === 'act') return x.run();
  if (x.kind === 'tab') return send('act', { id: x.id });
  if (x.kind === 'url') return go(x.u);
  if (x.kind === 'set') { SET.sec = x.i.s; SET.find = x.label; return setMode('set'); }
}
FX.click['pal-go'] = (el) => palGo(+el.dataset.k);
FX.menu.push(() => on('palette') ? row('palette', t('k.palette'), C().palKey || '') : '');
FX.act.palette = () => setTimeout(openPalette, 0);

/* ---------- #16 Calculs et conversions dans la barre d'adresse ---------- */
const FXR = { busy: false };
function fxRates() {
  const c = store.get('fxRates', null);
  if (on('calcFx') && !FXR.busy && (!c || Date.now() - c.t > 864e5)) {
    FXR.busy = true;
    fetch('https://api.frankfurter.app/latest?from=EUR').then((r) => r.json()).then((j) => {
      if (j && j.rates) { store.set('fxRates', { t: Date.now(), d: j.date, rates: j.rates }); if (mode === 'omni' && typed.trim()) renderSuggest(); }
    }).catch(() => {}).finally(() => { FXR.busy = false; });
  }
  return c;
}
function calcResult(raw) {
  if (!on('calc')) return null;
  const q = raw.trim(), lang = C().calcFmt && C().calcFmt !== 'auto' ? C().calcFmt : LANG;
  if (on('calcMath')) { const v = calcExpr(q); if (v !== null) return { v, text: fmtCalc(v, lang), sub: '= ' + t('calc.copy') }; }
  const cv = parseConv(q);
  if (!cv) return null;
  if (cv.kind === 'fx') {
    if (!on('calcFx')) return null;
    const r = fxRates(); const v = convert(cv, r && r.rates);
    if (v === null) return null;
    return { v, text: `${fmtCalc(v, lang, 2)} ${cv.to}`, sub: t('calc.fx', { d: r.d || '' }) + ' · ' + t('calc.copy') };
  }
  if (!on('calcUnits')) return null;
  const v = convert(cv, null);
  return v === null ? null : { v, text: `${fmtCalc(v, lang, 4)} ${cv.to.replace(/^([cfk])$/, (m) => '°' + m.toUpperCase())}`, sub: t('calc.copy') };
}
window.calcItem = (raw) => { const r = calcResult(raw); return r ? { k: 'calc', t: r.text, u: raw, sub: r.sub, calc: r.text } : null; };

/* ---------- #17 Gestes de souris : tableau geste → action ---------- */
const GESTS = ['L', 'R', 'U', 'D', 'DR', 'DL', 'UR', 'UL', 'DU', 'UD', 'LR', 'RL'];
const GACTS = ['none', 'back', 'fwd', 'close', 'new', 'reload', 'reopen', 'top', 'bottom', 'next', 'prev'];
SPECIAL.gestMap = () => {
  const m = C().gestMap || {};
  return `<div class="gmap">${GESTS.map((g) => `<label><span>${t('g.' + g)}</span><select class="sel2" data-gm="${g}">${GACTS.map((a) => `<option value="${a}" ${(m[g] || 'none') === a ? 'selected' : ''}>${t('ga.' + a)}</option>`).join('')}</select></label>`).join('')}</div>`;
};
document.addEventListener('change', (e) => {
  const g = e.target.dataset && e.target.dataset.gm;
  if (!g) return;
  const m = Object.assign({}, C().gestMap || {});
  if (e.target.value === 'none') delete m[g]; else m[g] = e.target.value;
  send('set', { k: 'gestMap', v: m });
});

/* ---------- Recherche dans un site avec Tab : « yout » + Tab → « Rechercher sur YouTube » ---------- */
Object.assign(TXT.en, {
  'ks.hint': 'Search {s}', 'ks.ph': 'Search {s}', 'ks.in': 'Search on {s}', 'ks.tab': 'Tab',
  'k.siteSearch': 'Tab to search a site', 'k.siteSearch.d': 'Type the start of a site (for example “yout”), then press Tab to search it directly: YouTube, Wikipedia, Amazon, GitHub, Reddit and more.',
});
Object.assign(TXT.fr, {
  'ks.hint': 'Rechercher sur {s}', 'ks.ph': 'Rechercher sur {s}', 'ks.in': 'Rechercher sur {s}', 'ks.tab': 'Tab',
  'k.siteSearch': 'Rechercher dans un site avec Tab', 'k.siteSearch.d': 'Tapez le début d’un site (par exemple « yout »), puis Tab pour y chercher directement : YouTube, Wikipédia, Amazon, GitHub, Reddit et d’autres.',
});
// h : domaines reconnus (le premier sert par défaut) ; u : adresse de recherche, {h} = domaine choisi, %s = recherche
const KW_SITES = [
  { n: 'YouTube', h: ['youtube.com', 'youtu.be', 'm.youtube.com'], u: 'https://www.youtube.com/results?search_query=%s' },
  { n: 'Google', h: ['google.com', 'google.fr', 'google.ca', 'google.be', 'google.ch'], u: 'https://www.{h}/search?q=%s' },
  { n: 'Wikipedia', h: ['wikipedia.org', 'fr.wikipedia.org', 'en.wikipedia.org'], u: 'wiki' },
  { n: 'Amazon', h: ['amazon.com', 'amazon.fr', 'amazon.ca', 'amazon.de', 'amazon.co.uk'], u: 'https://www.{h}/s?k=%s' },
  { n: 'GitHub', h: ['github.com'], u: 'https://github.com/search?q=%s' },
  { n: 'Reddit', h: ['reddit.com'], u: 'https://www.reddit.com/search/?q=%s' },
  { n: 'X', h: ['x.com', 'twitter.com'], u: 'https://x.com/search?q=%s' },
  { n: 'Twitch', h: ['twitch.tv'], u: 'https://www.twitch.tv/search?term=%s' },
  { n: 'TikTok', h: ['tiktok.com'], u: 'https://www.tiktok.com/search?q=%s' },
  { n: 'Spotify', h: ['open.spotify.com', 'spotify.com'], u: 'https://open.spotify.com/search/%s' },
  { n: 'Netflix', h: ['netflix.com'], u: 'https://www.netflix.com/search?q=%s' },
  { n: 'Facebook', h: ['facebook.com'], u: 'https://www.facebook.com/search/top/?q=%s' },
  { n: 'Stack Overflow', h: ['stackoverflow.com'], u: 'https://stackoverflow.com/search?q=%s' },
  { n: 'DuckDuckGo', h: ['duckduckgo.com'], u: 'https://duckduckgo.com/?q=%s' },
  { n: 'Bing', h: ['bing.com'], u: 'https://www.bing.com/search?q=%s' },
  { n: 'Google Maps', h: ['maps.google.com', 'google.com/maps'], u: 'https://www.google.com/maps/search/%s' },
  { n: 'eBay', h: ['ebay.com', 'ebay.fr', 'ebay.ca', 'ebay.de', 'ebay.co.uk'], u: 'https://www.{h}/sch/i.html?_nkw=%s' },
  { n: 'IMDb', h: ['imdb.com'], u: 'https://www.imdb.com/find/?q=%s' },
  { n: 'Pinterest', h: ['pinterest.com', 'pinterest.fr', 'pinterest.ca'], u: 'https://www.{h}/search/pins/?q=%s' },
  { n: 'LinkedIn', h: ['linkedin.com'], u: 'https://www.linkedin.com/search/results/all/?keywords=%s' },
  { n: 'Leboncoin', h: ['leboncoin.fr'], u: 'https://www.leboncoin.fr/recherche?text=%s' },
];
const KW = { s: null, h: '' };
// site reconnu pour ce qui est tapé (avec ou sans la complétion de la barre) : { s: site, h: domaine }
function kwFor(raw) {
  if (!on('siteSearch')) return null;
  const v = String(raw).trim().toLowerCase().replace(/^https?:\/\//, '').replace(/^www\./, '').replace(/\/$/, '');
  if (v.length < 2 || /\s/.test(v)) return null;
  for (const s of KW_SITES) { const h = s.h.find((d) => d === v || d.startsWith(v)); if (h) return { s, h }; }
  for (const s of KW_SITES) if (s.n.toLowerCase().replace(/\s/g, '').startsWith(v)) return { s, h: s.h[0] };
  return null;
}
const kwUrl = (k, text) => {
  const e = encodeURIComponent(text.trim());
  if (k.s.u === 'wiki') return `https://${LANG === 'fr' ? 'fr' : 'en'}.wikipedia.org/w/index.php?search=${e}`;
  const h = k.h.includes('/') || k.h.startsWith('maps.') || k.h.startsWith('open.') || k.h.startsWith('m.') ? k.s.h.find((d) => !d.includes('/')) : k.h;
  return k.s.u.replace('{h}', h).replace('%s', e);
};
const kwChip = document.createElement('span');
kwChip.id = 'kwchip'; kwChip.className = 'kwchip'; kwChip.hidden = true;
$('q').before(kwChip);
function kwEnter(k) {
  KW.s = k.s; KW.h = k.h;
  kwChip.textContent = k.s.n; kwChip.hidden = false;
  const qq = $('q');
  qq.value = ''; typed = ''; sel = 0;
  qq.placeholder = t('ks.ph', { s: k.s.n });
  $('omni').classList.add('kw');
  renderPanel();
}
function kwExit() {
  if (!KW.s) return;
  KW.s = null; KW.h = '';
  kwChip.hidden = true;
  $('omni').classList.remove('kw');
  $('q').placeholder = t('q.ph', { e: engine() });
}
// suggestions : en mode site, une seule ligne « Rechercher sur YouTube » ; sinon un rappel « Tab » quand un site est reconnu
const kwSuggest0 = suggest;
suggest = function (raw) {
  if (KW.s) return raw.trim() ? [{ k: 'search', t: raw.trim(), u: kwUrl({ s: KW.s, h: KW.h }, raw), sub: t('ks.in', { s: KW.s.n }) }] : [];
  const out = kwSuggest0(raw);
  const k = kwFor($('q').value) || kwFor(raw);
  if (k) out.splice(Math.min(1, out.length), 0, { k: 'search', t: t('ks.hint', { s: k.s.n }), u: 'https://' + k.h.split('/')[0], sub: t('ks.tab'), kw: true });
  return out.slice(0, 9);
};
// Tab entre en mode site ; Retour arrière (champ vide) ou Échap en sort ; Entrée recherche dans le site
$('q').addEventListener('keydown', (e) => {
  if (e.key === 'Tab' && !e.shiftKey && !e.ctrlKey && !e.altKey && !KW.s) {
    const k = kwFor($('q').value) || kwFor(typed);
    if (k) { e.preventDefault(); e.stopImmediatePropagation(); kwEnter(k); }
    return;
  }
  if (!KW.s) return;
  if (e.key === 'Backspace' && $('q').selectionStart === 0 && $('q').selectionEnd === 0) { e.preventDefault(); kwExit(); renderPanel(); }
  else if (e.key === 'Escape') kwExit();
  else if (e.key === 'Enter') {
    e.preventDefault(); e.stopImmediatePropagation();
    const v = $('q').value.trim();
    if (!v) return;
    const u = kwUrl({ s: KW.s, h: KW.h }, v);
    kwExit();
    go(u);
  }
}, true);
$('q').addEventListener('blur', () => setTimeout(() => { if (document.activeElement !== $('q')) kwExit(); }, 160));
