// Réglages : panneau pleine page avec navigation latérale.
const SET = { find: '', keycap: '', sec: 'general', P: [], rev: {}, copyWant: -1, q: '', add: null, pw1: '', pw2: '', err: '', reset: false, clear: false, ext: { list: [], rec: [], on: true }, link: '' };
const SECS = [
  ['general', svg('<circle cx="8" cy="6" r="2.7"/><path d="M3 13.5c.8-2.4 2.7-3.6 5-3.6s4.2 1.2 5 3.6"/>')],
  ['look', svg('<circle cx="8" cy="8" r="6"/><path d="M8 2a6 6 0 0 0 0 12z" fill="currentColor"/>')],
  ['a11y', svg('<circle cx="8" cy="3" r="1.3"/><path d="M3 5.5h10M8 5.5V10m0 0-2.5 4M8 10l2.5 4"/>')],
  ['privacy', svg('<path d="M8 1.8 13 3.6v4c0 3.2-2.2 5.6-5 6.6-2.8-1-5-3.4-5-6.6v-4z"/>')],
  ['perf', svg('<path d="M9 1.5 3.5 9H8l-1 5.5L12.5 7H8z"/>')],
  ['tabs', svg('<rect x="2" y="4" width="12" height="9.5" rx="2"/><path d="M2 7h12M5.5 4V2.5h5V4"/>')],
  ['nav', svg('<circle cx="8" cy="8" r="6"/><path d="m10.5 5.5-1.6 3.4-3.4 1.6 1.6-3.4z"/>')],
  ['pw', I.key],
  ['ext', I.puzzle],
  ['import', svg('<path d="M8 2v8M4.5 6.5 8 10l3.5-3.5M2.5 11v1.5a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1V11"/>')],
  ['tools', svg('<path d="M10.5 2.5a3 3 0 0 0-2.9 3.8L2.8 11.1a1.4 1.4 0 0 0 2 2l4.8-4.8a3 3 0 0 0 3.8-2.9l-1.7 1.7-1.6-.4-.4-1.6z"/>')],
  ['about', svg('<circle cx="8" cy="8" r="6"/><path d="M8 7v4M8 4.8v.1"/>')],
];
const SECNAME = { general: 's.general', look: 's.look', a11y: 's.a11y', privacy: 's.privacy', pw: 's.pw', ext: 's.ext', import: 's.import', perf: 's.perf', tabs: 's.tabs', nav: 's.nav', tools: 's.tools', about: 's.about' };

window.P = (l) => { SET.P = l; SET.rev = {}; if (mode === 'set') renderSettings(); };
window.R = (i, p) => {
  if (SET.copyWant === i) { SET.copyWant = -1; copyText(p); return; }
  SET.rev[i] = p; if (mode === 'set') renderSettings();
};
window.X = (x) => { SET.ext = x; if (mode === 'set') renderSettings(); };
window.renderImp = () => { if (mode === 'set' && SET.sec === 'import') renderSettings(); };
window.onVaultRes = (r) => {
  if (mode !== 'set') return;
  SET.err = r.ok ? '' : r.err === 'wrong' ? t('unlock.wrong') : r.err === 'short' ? t('pw.short') : String(r.err || '');
  if (r.ok) { SET.pw1 = SET.pw2 = ''; send('vault-list'); }
  renderSettings();
};
async function copyText(p) {
  try {
    await navigator.clipboard.writeText(p);
    toast('copied');
    setTimeout(async () => { try { if ((await navigator.clipboard.readText()) === p) await navigator.clipboard.writeText(''); } catch {} }, 30000);
  } catch {}
}

const sw = (k, label, sub) => `<div class="opt"><div class="lbl"><b id="l-${k}">${label}</b>${sub ? `<span>${sub}</span>` : ''}</div><button class="sw" role="switch" aria-labelledby="l-${k}" aria-checked="${!!C()[k]}" data-sk="${k}" data-sv="${!C()[k]}"></button></div>`;
const seg = (k, label, opts, sub) => `<div class="opt"><div class="lbl"><b>${label}</b>${sub ? `<span>${sub}</span>` : ''}</div><div class="seg" role="radiogroup" aria-label="${esc(label)}">${opts.map(([v, l]) => `<button role="radio" aria-checked="${String(C()[k]) === String(v)}" data-sk="${k}" data-sv='${JSON.stringify(v)}'>${l}</button>`).join('')}</div></div>`;
const selx = (k, label, opts, sub) => `<div class="opt"><div class="lbl"><b id="l-${k}">${label}</b>${sub ? `<span>${sub}</span>` : ''}</div><select class="sel2" aria-labelledby="l-${k}" data-sk="${k}">${opts.map(([v, l]) => `<option value='${JSON.stringify(v)}' ${String(C()[k]) === String(v) ? 'selected' : ''}>${esc(l)}</option>`).join('')}</select></div>`;
const rng = (k, label, min, max, step, fmt, sub) => `<div class="opt"><div class="lbl"><b id="l-${k}">${label}</b>${sub ? `<span>${sub}</span>` : ''}</div><input type="range" min="${min}" max="${max}" step="${step}" value="${C()[k]}" aria-labelledby="l-${k}" aria-valuetext="${fmt(C()[k])}" data-sk="${k}" data-num="1"><span style="width:48px;text-align:right;color:var(--t2)" aria-hidden="true">${fmt(C()[k])}</span></div>`;
const head = (k, lead) => `<h3>${t(SECNAME[k])}</h3><p class="lead">${lead}</p>`;
const box = (inner, title) => `${title ? `<div class="ghd">${title}</div>` : ''}<div class="group">${inner}</div>`;
const restartNote = () => S.restart ? `<div class="note warn">${I.gear}<span>${t('s.restartNote')}</span><button class="pill pri" data-sa="restart">${t('b.restart')}</button></div>` : '';

const SECTION = {
  general: () => head('general', t('g.lead'))
    + box(`<div class="opt"><div class="lbl"><b><label for="sname">${t('g.name2')}</label></b></div><input class="inp" id="sname" maxlength="40" value="${esc(C().name || '')}" style="width:220px"></div>`
      + selx('lang', t('g.lang'), [['auto', t('g.auto')], ['fr', 'Français'], ['en', 'English']])
      + selx('search', t('g.search'), Object.entries(ENGINES))
      + sw('restore', t('g.restore'), t('g.restoreSub'))) + secReset('general'),
  look: () => head('look', t('l.lead'))
    + box(seg('theme', t('l.theme'), [['dark', t('l.dark')], ['light', t('l.light')], ['system', t('l.system')]])
      + `<div class="opt"><div class="lbl"><b>${t('l.accent')}</b></div><div class="sw2" role="radiogroup" aria-label="${esc(t('l.accent'))}">${['#0a84ff', '#5e5ce6', '#bf5af2', '#ff375f', '#ff9f0a', '#30d158', '#40c8e0', '#8e8e93'].map((c) => `<button style="--c:${c}" role="radio" aria-label="${c}" aria-checked="${(C().accent || '#0a84ff') === c}" data-sk="accent" data-sv='"${c}"'></button>`).join('')}</div></div>`
      + seg('tabs', t('l.tabs'), [['h', t('l.h')], ['v', t('l.v')]])
      + sw('bmbar', t('l.bmbar'))) + reg('look') + secReset('look'),
  a11y: () => head('a11y', t('a.lead'))
    + box(rng('uiScale', t('a.ui'), 0.8, 1.6, 0.1, (v) => Math.round(v * 100) + '%', t('a.uiSub'))
      + selx('pageZoom', t('a.page'), [0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2].map((z) => [z, Math.round(z * 100) + '%']))
      + sw('contrast', t('a.contrast'), t('a.contrastSub')) + sw('motion', t('a.motion'), t('a.motionSub'))
      + sw('bigTargets', t('a.big'), t('a.bigSub')) + sw('focusRings', t('a.focus'), t('a.focusSub'))
      + rng('ttsRate', t('a.tts'), 0.5, 2, 0.1, (v) => '×' + (+v).toFixed(1))) + secReset('a11y')
    + box(`<div class="short">${[['Ctrl+T', 'm.new'], ['Ctrl+Shift+N', 'm.priv'], ['Ctrl+W', 'ctx.close'], ['Ctrl+Shift+T', 'm.reopen'], ['Ctrl+Tab', 'Next tab'], ['Ctrl+1…9', 'Tab 1…9'],
      ['Ctrl+L / F6', 'Address bar'], ['Ctrl+D', 'm.star'], ['Ctrl+H', 'm.hist'], ['Ctrl+J', 'm.dl'], ['F9', 'm.reader'], ['F12', 'm.dev'], ['Ctrl+F', 'Find in page'], ['Ctrl +/−/0', 'm.zoom']]
      .map(([k, l]) => `<span>${esc(t(l))}</span><span class="kbd">${k}</span>`).join('')}</div>`, t('a.keys')),
  privacy: () => head('privacy', t('p.lead'))
    + box(sw('ab', t('p.ab'), t('p.abSub', { n: (S.adlist || 0).toLocaleString(LANG) }))
      + seg('tracking', t('p.track'), [['off', t('p.off')], ['basic', t('p.basic')], ['balanced', t('p.balanced')], ['strict', t('p.strict')]])
      + sw('darkAll', t('p.darkAll'), t('p.darkAllSub')))
    + chipsBox('allow', t('p.allow')) + chipsBox('never', t('p.never'))
    + box(`<div class="opt"><div class="lbl"><b>${t('p.clear')}</b><span>${t('p.clearSub')}</span></div><button class="pill red" data-sa="clear">${SET.clear ? t('p.clearConfirm') : t('b.clear')}</button></div>`
      + `<div class="opt"><div class="lbl"><b>${t('m.priv')}</b><span>${t('home.privNote')}</span></div><button class="pill" data-sa="private">Ctrl+Shift+N</button></div>`)
    + reg('privacy') + secReset('privacy'),
  pw: () => head('pw', t('pw.lead')) + pwBody(),
  ext: () => head('ext', t('x.lead')) + extBody(),
  import: () => head('import', t('i.lead')) + impBody(),
  perf: () => head('perf', t('f.lead')) + restartNote()
    + box(`<div class="opt"><div class="lbl"><b>${t('f.mode')}</b><span>${t(C().mode === 'ultra' ? 'f.ultraSub' : 'f.fullSub')}</span></div><div class="seg" role="radiogroup"><button role="radio" aria-checked="${C().mode === 'ultra'}" data-sa="mode" data-v="ultra">${t('f.ultra')}</button><button role="radio" aria-checked="${C().mode !== 'ultra'}" data-sa="mode" data-v="full">${t('f.full')}</button></div></div>`
      + selx('sleep', t('f.sleep'), [[0, t('pw.never')], [3, t('pw.min', { n: 3 })], [5, t('pw.min', { n: 5 })], [10, t('pw.min', { n: 10 })], [30, t('pw.min', { n: 30 })], [60, t('pw.min', { n: 60 })]], t('f.stats', { t: S.tabs.length, s: S.asleep || 0 }))
      + sw('eco', t('f.eco'), t('f.ecoSub')) + sw('ext', t('f.ext'), t('f.extSub'))) + reg('perf') + secReset('perf'),
  tabs: () => head('tabs', t('s.tabsLead')) + reg('tabs') + secReset('tabs'),
  nav: () => head('nav', t('s.navLead')) + reg('nav') + secReset('nav'),
  tools: () => head('tools', t('s.toolsLead')) + reg('tools') + secReset('tools'),
  about: () => `<div style="text-align:center;padding:40px 0">${logo().replace('class="logo"', 'class="logo" style="width:84px;height:84px"')}<h3 style="margin-top:14px">Plume</h3><p class="lead" style="margin:6px auto">${t('ab.version', { v: S.version || '' })}</p><p class="lead" style="margin:0 auto 22px">${t('ab.made')}</p><button class="pill" data-sa="onb-again">${t('ab.onb')}</button></div>`
    + updBox()
    + box(`<div class="opt"><div class="lbl"><b>${t('s.resetAll')}</b><span>${t('s.resetAllSub')}</span></div><button class="pill red" data-sa="reset-all">${SET.resetAll ? t('s.resetConfirm') : t('s.reset')}</button></div>`),
};

/* ---------- Réglages 0.7 : rendus depuis le registre (src/settings.json, global SCHEMA) ---------- */
const REG = SCHEMA.items;
const regItem = (k) => REG.find((i) => i.k === k);
// libellés : k.<clé> ; description : k.<clé>.d ; option : k.<clé>.<valeur> (sinon la valeur + unité)
const optLabel = (i, o) => { const key = `k.${i.k}.${o}`; return TXT.en[key] ? t(key) : (i.u === 'min' ? t('pw.min', { n: o }) : o + (i.u ? ' ' + i.u : '')); };
const unitFmt = (i, v) => i.u === '%' ? Math.round(v) + '%' : i.u ? `${v} ${i.u}` : String(v);
const SPECIAL = {}; // rendus sur mesure (liste des sites Lite, conteneurs, gestes, widgets…), ajoutés par les modules
function regRow(i) {
  const c = C(), lbl = t('k.' + i.k), d = TXT.en['k.' + i.k + '.d'] ? t('k.' + i.k + '.d') : '';
  const net = i.net ? ` <span class="netb" title="${esc(t('s.netSub'))}">${t('s.net')}</span>` : '';
  const rs = i.rs ? ` <span class="netb">${t('s.restartTag')}</span>` : '';
  const dep = i.p ? ` data-dep="${i.p}"` : '';
  const dis = i.p && !c[i.p] ? ' dis' : '';
  const L = `<div class="lbl"><b id="l-${i.k}">${lbl}${net}${rs}</b>${d ? `<span>${d}</span>` : ''}</div>`;
  const v = c[i.k];
  let ctl = '';
  switch (i.t) {
    case 'sw': ctl = `<button class="sw" role="switch" aria-labelledby="l-${i.k}" aria-checked="${!!v}" data-sk="${i.k}" data-sv="${!v}"></button>`; break;
    case 'seg': ctl = `<div class="seg" role="radiogroup" aria-labelledby="l-${i.k}">${i.o.map((o) => `<button role="radio" aria-checked="${String(v) === String(o)}" data-sk="${i.k}" data-sv='${JSON.stringify(o)}'>${esc(optLabel(i, o))}</button>`).join('')}</div>`; break;
    case 'sel': ctl = `<select class="sel2" aria-labelledby="l-${i.k}" data-sk="${i.k}">${i.o.map((o) => `<option value='${JSON.stringify(o)}' ${String(v) === String(o) ? 'selected' : ''}>${esc(optLabel(i, o))}</option>`).join('')}</select>`; break;
    case 'num': ctl = `<input type="range" min="${i.min}" max="${i.max}" step="${i.step}" value="${v}" aria-labelledby="l-${i.k}" aria-valuetext="${unitFmt(i, v)}" data-sk="${i.k}" data-u="${i.u || ''}"><span class="rv" aria-hidden="true">${unitFmt(i, v)}</span>`; break;
    case 'text': ctl = `<input class="inp" data-stext="${i.k}" aria-labelledby="l-${i.k}" value="${esc(v || '')}" placeholder="${esc(TXT.en['k.' + i.k + '.ph'] ? t('k.' + i.k + '.ph') : '')}" style="width:220px">`; break;
    case 'color': ctl = `<input type="color" data-scolor="${i.k}" aria-labelledby="l-${i.k}" value="${esc(v || '#000000')}">`; break;
    case 'key': ctl = `<button class="pill keycap${SET.keycap === i.k ? ' on' : ''}" data-keycap="${i.k}" aria-labelledby="l-${i.k}">${SET.keycap === i.k ? t('s.pressKeys') : esc(v || '—')}</button>`; break;
    case 'btn': ctl = `<button class="pill${i.red ? ' red' : ''}" data-sa="${i.a}" aria-labelledby="l-${i.k}">${t('k.' + i.k + '.b')}</button>`; break;
    case 'list': {
      const l = Array.isArray(v) ? v : [];
      return `<div class="opt col${dis}"${dep}>${L}<div class="chips">${l.map((h) => `<span class="chip">${esc(h)}<button data-schip="${i.k}" data-h="${esc(h)}" aria-label="${esc(t('b.remove'))} ${esc(h)}">${I.x}</button></span>`).join('')}`
        + `<form class="ladd" data-listadd="${i.k}"><input class="inp" placeholder="${esc(TXT.en['k.' + i.k + '.ph'] ? t('k.' + i.k + '.ph') : 'example.com')}" aria-label="${esc(lbl)}"><button class="pill" type="submit">${t('b.add')}</button></form></div></div>`;
    }
    case 'special': return SPECIAL[i.k] ? `<div class="opt col${dis}"${dep}>${L}${SPECIAL[i.k](i)}</div>` : '';
  }
  return `<div class="opt${dis}"${dep}>${L}${ctl}</div>`;
}
// une carte par fonctionnalité : l'interrupteur principal puis ses sous-options
function reg(sec) {
  const list = REG.filter((i) => i.s === sec && i.ui !== 0);
  let out = '', cur = '';
  const flush = () => { if (cur) out += box(cur); cur = ''; };
  for (const i of list) {
    if (!i.p) flush();
    cur += regRow(i);
  }
  flush();
  return out;
}
const secReset = (sec) => `<div class="sreset"><button class="pill" data-sa="reset-sec" data-v="${sec}">${SET.resetSec === sec ? t('s.resetConfirm') : t('s.resetSec')}</button></div>`;
// Recherche : filtre toutes les lignes de toutes les sections (anciennes comme nouvelles)
const norm = (x) => String(x).toLowerCase().normalize('NFD').replace(/[\u0300-\u036f]/g, '');
function searchHtml() {
  const q = norm(SET.find.trim());
  const tmp = document.createElement('div');
  let out = '';
  for (const [k] of SECS) {
    if (['pw', 'ext', 'import'].includes(k)) continue; // listes dynamiques
    tmp.innerHTML = SECTION[k]();
    const rows = [...tmp.querySelectorAll('.opt')].filter((r) => norm(r.textContent).includes(q));
    if (rows.length) out += `<div class="ghd">${t(SECNAME[k])}</div><div class="group">${rows.map((r) => r.outerHTML).join('')}</div>`;
  }
  return `<h3>${t('s.results')}</h3>` + (out || `<div class="empty">${t('s.noResult')}</div>`);
}

function updBox() {
  const st = UPD.state;
  let sub, btn;
  if (UPD.tag && ['available', 'downloading', 'installing', 'ready', 'failed'].includes(st)) {
    sub = updLabel();
    btn = st === 'ready' ? `<button class="pill pri" data-sa="update-restart">${t('u.restart')}</button>`
      : st === 'failed' ? `<button class="pill pri" data-sa="update">${t('u.retry')}</button>`
      : updBusy() ? `<span class="upbar" aria-hidden="true"><i style="width:${UPD.pct || 0}%"></i></span>`
      : `<button class="pill pri" data-sa="update">${t(S.installed ? 'u.install' : 'u.get')}</button>`;
  } else {
    sub = st === 'latest' ? t('u.latest') : st === 'checking' ? t('u.checking') : st === 'error' ? t('u.error') : t('u.sub');
    btn = `<button class="pill" data-sa="check-upd"${st === 'checking' ? ' disabled' : ''}>${t('u.check')}</button>`;
  }
  return box(`<div class="opt"><div class="lbl"><b>${t('u.title')}</b><span>${sub}</span></div>${btn}</div>` + sw('updates', t('u.auto'), t('u.autoSub')));
}

function chipsBox(k, title) {
  const l = C()[k] || [];
  if (!l.length) return '';
  return box(`<div class="opt"><div class="chips">${l.map((h) => `<span class="chip">${esc(h)}<button data-schip="${k}" data-h="${esc(h)}" aria-label="${esc(t('b.remove'))} ${esc(h)}">${I.x}</button></span>`).join('')}</div></div>`, title);
}

function strength(p) {
  let s = 0;
  if (p.length >= 8) s++; if (p.length >= 12) s++; if (p.length >= 16) s++;
  if (/[a-z]/.test(p) && /[A-Z]/.test(p)) s++; if (/\d/.test(p)) s++; if (/[^\w\s]/.test(p) || / /.test(p)) s++;
  return Math.min(4, Math.floor(s * 4 / 6));
}

function pwBody() {
  const v = S.vault || {};
  if (!v.exists) {
    const st = strength(SET.pw1), col = ['var(--red)', 'var(--red)', 'var(--orange)', 'var(--green)', 'var(--green)'][st];
    const how = `<div class="how"><div class="ghd">${t('pw.howT')}</div>${[['1', 'pw.h1', 'pw.h1s'], ['2', 'pw.h2', 'pw.h2s'], ['3', 'pw.h3', 'pw.h3s']].map(([n, a, b]) => `<div class="step"><span class="num">${n}</span><div><b>${t(a)}</b><span>${t(b)}</span></div></div>`).join('')}</div>`;
    return how + box(`<div style="padding:16px 14px"><b style="display:block;font-size:15px;margin-bottom:4px">${t('pw.create')}</b><p class="lead" style="margin-bottom:14px">${t('pw.createSub')}</p>
      <form class="form" id="pwc"><label class="sr" for="pw1">${t('pw.pw')}</label><input class="inp" type="password" id="pw1" placeholder="${esc(t('pw.pw'))}" value="${esc(SET.pw1)}" autocomplete="new-password">
      <div class="meter" aria-hidden="true"><i style="width:${SET.pw1 ? (st + 1) * 20 : 0}%;background:${col}"></i></div>
      <label class="sr" for="pw2">${t('pw.pw2')}</label><input class="inp" type="password" id="pw2" placeholder="${esc(t('pw.pw2'))}" value="${esc(SET.pw2)}" autocomplete="new-password">
      <div class="err" role="alert">${esc(SET.err)}</div><div><button class="pill pri" type="submit">${t('b.create')}</button></div></form></div>`);
  }
  if (!v.unlocked) {
    return box(`<div style="padding:16px 14px"><b style="display:block;font-size:15px;margin-bottom:12px">${t('pw.locked')}</b>
      <form class="form" id="pwu"><label class="sr" for="pwu1">${t('unlock.ph')}</label><input class="inp" type="password" id="pwu1" placeholder="${esc(t('unlock.ph'))}" autocomplete="off">
      <div class="err" role="alert">${esc(SET.err)}</div><div style="display:flex;gap:8px"><button class="pill pri" type="submit">${t('b.unlock')}</button>${v.hello ? `<button class="pill" type="button" data-sa="hello-unlock">${t('hl.use')}</button>` : ''}<button class="pill red" type="button" data-sa="vault-reset">${SET.reset ? t('pw.resetConfirm') : t('pw.forgot')}</button></div></form></div>`);
  }
  const f = SET.q.toLowerCase();
  const list = SET.P.filter((x) => !f || (x.o + ' ' + x.u).toLowerCase().includes(f));
  const rows = list.map((x) => {
    const r = SET.rev[x.i];
    return `<div class="row pwr" style="height:auto;padding:8px 10px">${icon('https://' + x.o, '')}<div class="meta"><b>${esc(x.o)}</b><span>${esc(x.u || '—')}</span>${r != null ? `<br><code>${esc(r)}</code>` : ''}</div>`
      + `<button class="icb" data-spw="${r != null ? 'hide' : 'show'}" data-i="${x.i}" aria-label="${esc(t(r != null ? 'b.hide' : 'b.show'))}">${I.eye}</button>`
      + `<button class="icb" data-spw="copy" data-i="${x.i}" aria-label="${esc(t('b.copy'))}">${I.copy}</button>`
      + `<button class="icb red" data-spw="del" data-i="${x.i}" aria-label="${esc(t('b.delete'))}">${I.trash}</button></div>`;
  }).join('');
  const add = SET.add ? `<form class="form" id="pwa" style="padding:12px;max-width:none;grid-template-columns:1fr 1fr 1fr auto;align-items:center">
      <input class="inp" id="pa-o" placeholder="${esc(t('pw.site'))}" aria-label="${esc(t('pw.site'))}"><input class="inp" id="pa-u" placeholder="${esc(t('pw.user'))}" aria-label="${esc(t('pw.user'))}"><input class="inp" type="password" id="pa-p" placeholder="${esc(t('pw.pw'))}" aria-label="${esc(t('pw.pw'))}"><button class="pill pri" type="submit">${t('b.save')}</button></form>` : '';
  return `<div class="pgh" style="margin-bottom:12px"><input class="search" id="pwq" placeholder="${esc(t('pw.search'))}" aria-label="${esc(t('pw.search'))}" value="${esc(SET.q)}" style="flex:1">`
    + `<button class="pill" data-sa="pw-add">${t('pw.add')}</button><button class="pill" data-sa="pw-csv">${t('pw.csv')}</button><button class="pill" data-sa="vault-lock">${t('b.lock')}</button></div>`
    + box(add + (rows || `<div class="empty">${t('pw.empty')}</div>`))
    + box(selx('autolock', t('pw.autolock'), [[5, t('pw.min', { n: 5 })], [15, t('pw.min', { n: 15 })], [60, t('pw.min', { n: 60 })], [0, t('pw.never')]]))
    + (window.pwExtra ? pwExtra() : '')
    + `<p class="lead" style="font-size:12px">${t('pw.csvHelp')}</p>`;
}

function extBody() {
  const x = SET.ext;
  if (!x.on) return restartNote() + `<div class="note">${I.puzzle}<span>${t('x.off')}</span>${C().ext ? '' : `<button class="pill pri" data-sa="ext-on">${t('x.enable')}</button>`}</div>`;
  const inst = new Set(x.list.map((e) => e.store));
  const rec = x.rec.map((r) => `<div class="rec"><span class="ph extc">${I.puzzle}</span><div class="meta"><b>${esc(r.name)}</b><span>${t('x.' + r.k)}</span></div>`
    + (inst.has(r.store) ? `<span style="color:var(--green)">${I.check}</span>` : `<button class="pill pri" data-sa="ext-install" data-v="${r.store}">${t('b.install')}</button>`) + '</div>').join('');
  const list = x.list.map((e) => `<div class="opt extc">${e.icon ? `<img src="${esc(e.icon)}" alt="">` : `<span class="ph">${I.puzzle}</span>`}<div class="lbl"><b id="xe-${esc(e.id)}">${esc(e.name)}</b><span>${esc(e.id)}</span></div>`
    + (e.options ? `<button class="pill" data-sa="ext-open" data-v="${esc(e.id)}" data-page="${esc(e.options)}">${t('b.options')}</button>` : '')
    + `<button class="icb xpinb${(C().pinExt || []).includes(e.id) ? ' on' : ''}" data-xpin="${esc(e.id)}" aria-pressed="${(C().pinExt || []).includes(e.id)}" title="${esc(t((C().pinExt || []).includes(e.id) ? 'x.unpin' : 'x.pin'))}" aria-label="${esc(t('x.pin'))} ${esc(e.name)}">${I.pin}</button>`
    + `<button class="icb red" data-sa="ext-remove" data-v="${esc(e.id)}" aria-label="${esc(t('b.remove'))} ${esc(e.name)}">${I.trash}</button>`
    + `<button class="sw" role="switch" aria-labelledby="xe-${esc(e.id)}" aria-checked="${e.on}" data-sa="ext-toggle" data-v="${esc(e.id)}" data-on="${!e.on}"></button></div>`).join('');
  return restartNote() + `<form class="pgh" id="extf" style="margin-bottom:18px"><input class="search" id="extl" style="flex:1" placeholder="${esc(t('x.link'))}" aria-label="${esc(t('x.link'))}" value="${esc(SET.link)}"><button class="pill pri" type="submit">${t('b.install')}</button><button class="pill" type="button" data-sa="ext-store">${t('x.store')}</button></form>`
    + `<div class="ghd">${t('x.rec')}</div><div class="recs">${rec}</div>`
    + box(list || `<div class="empty">${t('x.none')}</div>`, t('x.inst'));
}

function impBody() {
  if (!IMP.list) { send('imp-detect'); return `<div class="empty">…</div>`; }
  const rows = IMP.list.length ? IMP.list.map((x) => `<label class="chk2"><input type="checkbox" class="rchk" data-simp="${x.id}" ${IMP.sel.has(x.id) ? 'checked' : ''}>${bTile(x, 'width:28px;height:28px;border-radius:8px;font-size:13px')}<span><b>${esc(x.browser)}</b> <span style="color:var(--t3)">${esc(x.profile)}</span></span></label>`).join('') : `<div class="empty">${t('i.none')}</div>`;
  const res = IMP.done ? `<div class="note">${I.check}<span>${t('toast.imported', IMP.done)}${IMP.done.errors && IMP.done.errors.length ? `<br><small>${t('i.errors')} ${esc(IMP.done.errors.join(' · '))}</small>` : ''}</span></div>` : '';
  return res + box(rows, t('i.found'))
    + box(`<label class="chk2"><input type="checkbox" class="rchk" data-simpk="h" ${IMP.h ? 'checked' : ''}>${t('i.hist')}</label><label class="chk2"><input type="checkbox" class="rchk" data-simpk="b" ${IMP.b ? 'checked' : ''}>${t('i.fav')}</label>`
      + `<div class="opt"><div class="lbl"><b>${t('i.pw')}</b><span>${t('i.pwSub')}</span></div><button class="pill" data-ssec="pw">${t('s.pw')}</button></div>`, t('i.what'))
    + `<button class="pill pri" data-sa="imp-run" ${IMP.busy || !IMP.sel.size || !(IMP.h || IMP.b) ? 'disabled' : ''}>${IMP.busy ? t('i.running') : t('b.import')}</button>`;
}

function renderSettings(fromState) {
  const ov = $('ov');
  const a = document.activeElement;
  if (fromState && ov.querySelector('.set') && a && a.closest('.set') && /INPUT|TEXTAREA|SELECT/.test(a.tagName)) return;
  const prevScroll = ov.querySelector('.set .body')?.scrollTop || 0;
  const findFocus = document.activeElement && document.activeElement.id === 'setq' ? document.activeElement.selectionStart : -1;
  if (SET.sec === 'pw' && S.vault && S.vault.unlocked && !SET.P.length && S.vault.n && !SET.asked) { SET.asked = true; send('vault-list'); }
  if (SET.sec === 'ext' && !SET.extAsked) { SET.extAsked = true; send('ext-list'); }
  const html = `<div class="set"><nav aria-label="${esc(t('s.title'))}"><h2>${t('s.title')}</h2><input class="search" id="setq" placeholder="${esc(t('s.search'))}" aria-label="${esc(t('s.search'))}" value="${esc(SET.find)}" spellcheck="false">${SECS.map(([k, ic]) => `<button data-ssec="${k}" class="${SET.sec === k ? 'on' : ''}" aria-current="${SET.sec === k ? 'page' : 'false'}">${ic}${t(SECNAME[k])}</button>`).join('')}</nav>`
    + `<div class="body"><div class="pgw"><div style="display:flex;justify-content:flex-end;margin:-10px -10px 0 0"><button class="icb" data-m="close-ov" aria-label="${esc(t('b.close'))}">${I.x}</button></div>${SET.find.trim() ? searchHtml() : SECTION[SET.sec]()}</div></div></div>`;
  if (!ovSet(html)) return; // rien n'a changé : on garde la page telle quelle (pas d'animation, pas de saut)
  ov.querySelector('.set .body').scrollTop = fromState ? prevScroll : 0;
  bindSettings();
  if (findFocus >= 0) { const f = $('setq'); f.focus(); f.setSelectionRange(findFocus, findFocus); }
}

function bindSettings() {
  const f = (id, fn) => { const el = $(id); if (el) fn(el); };
  f('sname', (el) => (el.onchange = () => send('set', { k: 'name', v: el.value.trim().slice(0, 40) })));
  f('pw1', (el) => (el.oninput = () => { SET.pw1 = el.value; const m = document.querySelector('.meter i'); const s = strength(el.value); m.style.width = el.value ? (s + 1) * 20 + '%' : 0; m.style.background = ['var(--red)', 'var(--red)', 'var(--orange)', 'var(--green)', 'var(--green)'][s]; }));
  f('pw2', (el) => (el.oninput = () => (SET.pw2 = el.value)));
  f('pwc', (el) => (el.onsubmit = (e) => {
    e.preventDefault();
    if (SET.pw1.length < 8) SET.err = t('pw.short'); else if (SET.pw1 !== SET.pw2) SET.err = t('pw.mismatch'); else { SET.err = ''; send('vault-create', { pw: SET.pw1 }); return; }
    renderSettings();
  }));
  f('pwu', (el) => (el.onsubmit = (e) => { e.preventDefault(); const p = $('pwu1').value; if (p) send('vault-unlock', { pw: p }); }));
  f('pwu1', (el) => setTimeout(() => el.focus(), 30));
  f('pwq', (el) => (el.oninput = () => { SET.q = el.value; const pos = el.selectionStart; renderSettings(); const n = $('pwq'); n.focus(); n.setSelectionRange(pos, pos); }));
  f('pwa', (el) => { setTimeout(() => $('pa-o').focus(), 30); el.onsubmit = (e) => { e.preventDefault(); send('vault-add', { o: $('pa-o').value, u: $('pa-u').value, p: $('pa-p').value }); SET.add = null; }; });
  f('extf', (el) => (el.onsubmit = (e) => { e.preventDefault(); SET.link = $('extl').value; if (SET.link.trim()) send('ext-install', { id: SET.link.trim() }); SET.link = ''; }));
  f('setq', (el) => (el.oninput = () => { SET.find = el.value; renderSettings(); }));
  document.querySelectorAll('.set [data-listadd]').forEach((fm) => (fm.onsubmit = (e) => {
    e.preventDefault();
    const k = fm.dataset.listadd, inp = fm.querySelector('input'), raw = inp.value.trim();
    if (!raw) return;
    const val = /^[a-z]+:\/\//i.test(raw) || raw.includes('.') ? host(/^[a-z]+:\/\//i.test(raw) ? raw : 'https://' + raw) : raw;
    const l = Array.isArray(C()[k]) ? C()[k] : [];
    if (!l.includes(val)) send('set', { k, v: [...l, val] });
    inp.value = '';
  }));
}

$('ov').addEventListener('click', (e) => {
  if (!e.target.closest('.set')) return;
  const el = e.target.closest('[data-sk],[data-sa],[data-ssec],[data-schip],[data-spw],[data-keycap]');
  if (!el || el.tagName === 'SELECT' || el.type === 'range') return;
  const d = el.dataset;
  if (d.ssec) { SET.sec = d.ssec; SET.find = ''; SET.err = ''; SET.reset = SET.clear = SET.resetAll = false; SET.resetSec = ''; return renderSettings(); }
  if (d.keycap) { SET.keycap = SET.keycap === d.keycap ? '' : d.keycap; return renderSettings(); }
  if (d.sk) return send('set', { k: d.sk, v: JSON.parse(d.sv) });
  if (d.schip) return send('set', { k: d.schip, v: (C()[d.schip] || []).filter((h) => h !== d.h) });
  if (d.spw) {
    const i = +d.i;
    if (d.spw === 'show') return send('vault-reveal', { i });
    if (d.spw === 'hide') { delete SET.rev[i]; return renderSettings(); }
    if (d.spw === 'copy') { if (SET.rev[i] != null) return copyText(SET.rev[i]); SET.copyWant = i; return send('vault-reveal', { i }); }
    if (d.spw === 'del') { SET.P = []; SET.asked = false; return send('vault-del', { i }); }
  }
  switch (d.sa) {
    case 'restart': return send('restart');
    case 'reset-sec': if (SET.resetSec !== d.v) { SET.resetSec = d.v; return renderSettings(); } SET.resetSec = ''; return send('reset', { sec: d.v });
    case 'reset-all': if (!SET.resetAll) { SET.resetAll = true; return renderSettings(); } SET.resetAll = false; return send('reset', { sec: '' });
    case 'check-upd': return checkUpdate(true);
    case 'update': return updInstall();
    case 'update-restart': return send('update-restart');
    case 'onb-again': setMode(null); send('ov', { v: true }); Object.assign(ONB, { on: true, step: 0, name: C().name || '', mode: C().mode || 'full' }); $('onb').classList.add('on'); return renderOnb();
    case 'clear': if (!SET.clear) { SET.clear = true; return renderSettings(); } SET.clear = false; send('clear'); return renderSettings();
    case 'private': return send('private');
    case 'mode': return send('mode', { mode: d.v });
    case 'vault-reset': if (!SET.reset) { SET.reset = true; return renderSettings(); } SET.reset = false; return send('vault-reset');
    case 'vault-lock': SET.P = []; SET.asked = false; return send('vault-lock');
    case 'pw-add': SET.add = SET.add ? null : {}; return renderSettings();
    case 'pw-csv': SET.P = []; SET.asked = false; return send('pw-csv');
    case 'ext-on': return send('set', { k: 'ext', v: true });
    case 'ext-install': return send('ext-install', { id: d.v });
    case 'ext-store': setMode(null); return send('open', { u: 'https://chromewebstore.google.com/' });
    case 'ext-open': setMode(null); return send('ext-open', { id: d.v, page: d.page });
    case 'ext-remove': return send('ext-remove', { id: d.v });
    case 'ext-toggle': return send('ext-toggle', { id: d.v, on: d.on === 'true' });
    case 'imp-run': IMP.busy = true; IMP.done = null; renderSettings(); return send('imp-run', { ids: [...IMP.sel], h: IMP.h, b: IMP.b });
    default: if (d.sa && window.setAction) return setAction(d.sa, d);
  }
});
$('ov').addEventListener('change', (e) => {
  const el = e.target;
  if (!el.closest('.set')) return;
  if (el.dataset.sk && el.tagName === 'SELECT') return send('set', { k: el.dataset.sk, v: JSON.parse(el.value) });
  if (el.dataset.sk && el.type === 'range') return send('set', { k: el.dataset.sk, v: +el.value });
  if (el.dataset.stext) return send('set', { k: el.dataset.stext, v: el.value.trim() });
  if (el.dataset.scolor) return send('set', { k: el.dataset.scolor, v: el.value });
  if (el.dataset.simp != null) { el.checked ? IMP.sel.add(+el.dataset.simp) : IMP.sel.delete(+el.dataset.simp); return renderSettings(); }
  if (el.dataset.simpk) { IMP[el.dataset.simpk] = el.checked; return renderSettings(); }
});
$('ov').addEventListener('input', (e) => {
  const el = e.target;
  if (el.type === 'range' && el.dataset.sk) {
    const i = regItem(el.dataset.sk);
    el.nextElementSibling.textContent = i && i.ui !== 0 ? unitFmt(i, +el.value) : el.dataset.sk === 'uiScale' ? Math.round(el.value * 100) + '%' : '×' + (+el.value).toFixed(1);
  }
});

// Raccourci personnalisé : la prochaine combinaison de touches devient la valeur du réglage
document.addEventListener('keydown', (e) => {
  if (!SET.keycap || mode !== 'set') return;
  if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) return;
  e.preventDefault(); e.stopPropagation();
  const k = SET.keycap; SET.keycap = '';
  if (e.key === 'Escape') return renderSettings();
  const name = e.key.length === 1 ? e.key.toUpperCase() : e.key;
  const combo = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', name].filter(Boolean).join('+');
  send('set', { k, v: combo });
}, true);
