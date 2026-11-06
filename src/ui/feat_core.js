/* ===== 0.7 : socle des fonctionnalités =====
   Chaque module (feat_*.js) s'enregistre dans FX : lignes du menu ⋯, du clic droit sur un onglet, badges
   d'onglet, éléments de la barre, pages internes plume://…, boutons des réglages, petits panneaux (« pop »).
   Tout le JS de l'interface partage une seule portée : noms globaux uniques (préfixe des modules). */
const FX = { menu: [], ctx: [], tab: [], bar: [], home: [], act: {}, click: {}, set: {}, pages: {}, closed: [] };
const on = (k) => !!C()[k];

window.menuExtra = (x, web) => FX.menu.map((f) => f(x, web) || '').join('');
window.ctxExtra = (x) => FX.ctx.map((f) => f(x) || '').join('');
window.tabExtra = (x, narrow) => FX.tab.map((f) => f(x, narrow) || '').join('');
window.renderExtra = (x) => FX.bar.forEach((f) => f(x));
window.featAction = (m, d, id) => { const f = FX.act[m]; if (!f) return false; setMode(null, true); f(d, id); return true; };
window.featClick = (k, el, e) => { const f = FX.click[k]; if (f) f(el, e); return !!f; };
window.setAction = (a, d) => {
  if (a.startsWith('page:')) return openPage(a.slice(5));
  const f = FX.set[a]; if (f) f(d);
};

/* ---------- Pages internes : plume://stats, plume://tasks, plume://archive… ---------- */
const PG = { name: '' };
window.openPage = (name) => {
  if (name === 'settings') return setMode('set');
  if (name === 'passwords') { SET.sec = 'pw'; return setMode('set'); }
  if (name === 'history') return setMode('hist');
  if (name === 'downloads') return setMode('dl');
  const p = FX.pages[name];
  if (!p) return toast('pg.none');
  if (mode === 'pg' && PG.name !== name) pageLeft();
  PG.name = name;
  if (p.open) p.open();
  if (mode === 'pg') renderPage(); else setMode('pg');
};
function pageLeft() { const p = FX.pages[PG.name]; if (p && p.close) p.close(); }
window.renderPage = () => {
  const p = FX.pages[PG.name];
  if (!p) return;
  const prevQ = document.querySelector('.page .pgq');
  const qv = prevQ ? prevQ.value : '';
  const qpos = prevQ && document.activeElement === prevQ ? prevQ.selectionStart : -1;
  const sc = document.querySelector('#ov .page')?.scrollTop || 0;
  ovSet(`<div class="page" data-pg="${esc(PG.name)}"><div class="pgw${p.wide ? ' wide' : ''}"><div class="pgh"><h2>${p.title()}</h2>`
    + (p.search ? `<input class="search pgq" placeholder="${esc(t('pg.search'))}" aria-label="${esc(t('pg.search'))}" value="${esc(qv)}" spellcheck="false">` : '')
    + (p.actions ? p.actions() : '') + `<button class="icb" data-m="close-ov" aria-label="${esc(t('b.close'))}">${I.x}</button></div>`
    + `<p class="pgurl">plume://${esc(PG.name)}</p>${p.body(qv.trim().toLowerCase())}</div></div>`);
  const pg = document.querySelector('#ov .page'); if (pg) pg.scrollTop = sc;
  const qi = document.querySelector('.page .pgq');
  if (qi) { qi.oninput = () => renderPage(); if (qpos >= 0) { qi.focus(); qi.setSelectionRange(qpos, qpos); } }
};
// quitter une page interne (Échap, clic ailleurs, autre panneau) → elle arrête ce qu'elle surveillait
const featSetMode0 = setMode;
setMode = function (m, f) {
  if (mode === 'pg' && m !== 'pg') pageLeft();
  if (mode === 'pop' && m !== 'pop') POP.html = null;
  return featSetMode0(m, f);
};

/* ---------- Petit panneau ancré en haut à droite (mode Lite, QR, minuteur…) ---------- */
const POP = { html: null };
window.openPop = (html) => { POP.html = html; if (mode === 'pop') renderPanel(); else setMode('pop', true); };
window.panelHook = (under) => {
  if (mode === 'pop' && POP.html) { ovSet(under + `<div class="card pop" role="dialog" style="right:10px;top:6px">${POP.html()}</div>`); return true; }
  if (mode === 'modal' && POP.html) { ovSet(under + `<div class="modal"><div class="card wmodal" role="dialog" aria-modal="true">${POP.html()}</div></div>`); return true; }
  return false;
};
window.openModal = (html) => { POP.html = html; if (mode === 'modal') renderPanel(); else setMode('modal', true); };

/* ---------- Formats ---------- */
const fmtMem = (b) => {
  const fr = LANG === 'fr', mb = b / 1048576;
  if (C().ramUnit === 'mb' || mb < 1024) return `${Math.round(mb).toLocaleString(LANG)} ${fr ? 'Mo' : 'MB'}`;
  return `${(mb / 1024).toLocaleString(LANG, { maximumFractionDigits: 1 })} ${fr ? 'Go' : 'GB'}`;
};
const fmtNum = (n) => Math.round(n).toLocaleString(LANG);
const sw2 = (on2, attrs, label) => `<button class="sw" role="switch" aria-checked="${!!on2}" ${attrs}${label ? ` aria-label="${esc(label)}"` : ''}></button>`;

/* ---------- Nouvelle page : cartes ajoutées par les modules (statistiques, widgets…) ---------- */
window.homeHtml = (o) => {
  const extra = FX.home.map((f) => f(o.x) || '').join('');
  return `<div class="start${o.wasSeen ? ' seen' : ''}${o.typing ? ' typing' : ''}${o.x.p ? ' priv' : ''}">${o.hero}${o.x.p ? `<p class="privnote">${t('home.privNote')}</p>` : ''}${extra}<h2>${t('home.fav')}</h2>${o.fav}${window.homeFreq ? homeFreq(o) : o.freq}</div>`;
};
