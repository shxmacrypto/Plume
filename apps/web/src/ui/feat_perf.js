/* ===== 0.7 · Performance : mémoire par onglet + gestionnaire de tâches (#1), batterie (#2),
   liste blanche de la veille (#3), mode Lite (#5), statistiques d'économies (#6) ===== */

Object.assign(TXT.en, {
  'pg.none': 'This Plume page does not exist.', 'pg.search': 'Search',
  'pf.shared': 'Shared with other tabs of the same site', 'pf.tasks': 'Task manager',
  'pf.tasksLead': '{n} processes · {m} in total. Ending a page process stops the tabs it shows; reload them to bring them back.',
  'pf.proc': 'Process', 'pf.mem': 'Memory', 'pf.end': 'End', 'pf.unload': 'Unload',
  'pf.kPlume': 'Plume', 'pf.kBrowser': 'Browser engine', 'pf.kGpu': 'Graphics (GPU)', 'pf.kUtility': 'Utility', 'pf.kRenderer': 'Page', 'pf.kOther': 'Other', 'pf.ui': 'Plume interface',
  'pf.battOn': 'Battery saver is on', 'pf.noSleep': 'Never put this site to sleep', 'pf.sleepOk': 'Allow this site to sleep',
  'pf.liteT': 'Lite mode', 'pf.lite.js': 'Turn off JavaScript', 'pf.lite.img': 'Turn off images', 'pf.lite.font': 'Turn off web fonts', 'pf.lite.auto': 'Turn off video autoplay',
  'pf.liteNote': 'The page reloads to apply the change. These rules only apply to this site.',
  'pf.liteS.js': 'JS', 'pf.liteS.img': 'images', 'pf.liteS.font': 'fonts', 'pf.liteS.auto': 'autoplay', 'pf.liteNone': 'nothing off',
  'pf.stats': 'What Plume saved you', 'pf.stFreed': 'memory freed by sleeping tabs', 'pf.stBlocked': 'ads and trackers blocked',
  'pf.stData': 'data not downloaded (estimate)', 'pf.stTime': 'loading time saved (estimate)', 'pf.stDays': 'Ads and trackers blocked per day',
  'pf.stNote': 'Estimates: 25 kB and 40 ms per blocked request. Everything is computed and kept on your PC.', 'pf.stOff': 'Statistics are turned off (Settings › Performance).',
  'pf.stResetQ': 'Reset all statistics?',
});
Object.assign(TXT.fr, {
  'pg.none': "Cette page Plume n'existe pas.", 'pg.search': 'Rechercher',
  'pf.shared': "Partagé avec d'autres onglets du même site", 'pf.tasks': 'Gestionnaire de tâches',
  'pf.tasksLead': "{n} processus · {m} au total. Terminer le processus d'une page arrête les onglets qu'il affiche ; rechargez-les pour les retrouver.",
  'pf.proc': 'Processus', 'pf.mem': 'Mémoire', 'pf.end': 'Terminer', 'pf.unload': 'Décharger',
  'pf.kPlume': 'Plume', 'pf.kBrowser': 'Moteur du navigateur', 'pf.kGpu': 'Graphismes (GPU)', 'pf.kUtility': 'Utilitaire', 'pf.kRenderer': 'Page', 'pf.kOther': 'Autre', 'pf.ui': 'Interface de Plume',
  'pf.battOn': "L'économie de batterie est active", 'pf.noSleep': 'Ne jamais mettre en veille ce site', 'pf.sleepOk': 'Autoriser la veille pour ce site',
  'pf.liteT': 'Mode Lite', 'pf.lite.js': 'Couper JavaScript', 'pf.lite.img': 'Couper les images', 'pf.lite.font': 'Couper les polices web', 'pf.lite.auto': 'Couper la lecture auto des vidéos',
  'pf.liteNote': "La page se recharge pour appliquer le changement. Ces règles ne valent que pour ce site.",
  'pf.liteS.js': 'JS', 'pf.liteS.img': 'images', 'pf.liteS.font': 'polices', 'pf.liteS.auto': 'lecture auto', 'pf.liteNone': 'rien de coupé',
  'pf.stats': "Ce que Plume vous a fait économiser", 'pf.stFreed': 'de mémoire libérée par la veille', 'pf.stBlocked': 'pubs et traqueurs bloqués',
  'pf.stData': 'de données évitées (estimation)', 'pf.stTime': 'de chargement gagné (estimation)', 'pf.stDays': 'Pubs et traqueurs bloqués par jour',
  'pf.stNote': "Estimations : 25 ko et 40 ms par requête bloquée. Tout est calculé et conservé sur votre PC.", 'pf.stOff': 'Les statistiques sont désactivées (Paramètres › Performance).',
  'pf.stResetQ': 'Réinitialiser toutes les statistiques ?',
});

/* ---------- #1 Badge mémoire sur les onglets ---------- */
let RAMS = { tabs: {}, procs: [] };
window.RAM = (o) => {
  RAMS = o;
  document.querySelectorAll('.ram[data-r]').forEach((el) => { const v = RAMS.tabs[el.dataset.r]; el.textContent = v ? fmtMem(v[0]) : ''; el.title = v && v[1] ? t('pf.shared') : ''; });
  if (mode === 'pg' && PG.name === 'tasks') renderPage();
};
FX.tab.push((x, narrow) => {
  if (!on('ram') || narrow || !x.u) return '';
  const v = RAMS.tabs[x.id];
  return `<span class="ram" data-r="${x.id}"${v && v[1] ? ` title="${esc(t('pf.shared'))}"` : ''}>${v ? fmtMem(v[0]) : ''}</span>`;
});

/* ---------- #1 Gestionnaire de tâches (Maj+Échap, plume://tasks) ---------- */
const PF_KIND = { plume: 'pf.kPlume', browser: 'pf.kBrowser', gpu: 'pf.kGpu', utility: 'pf.kUtility', renderer: 'pf.kRenderer', other: 'pf.kOther' };
FX.pages.tasks = {
  wide: 1,
  title: () => t('pf.tasks'),
  open: () => send('tasks', { open: true }),
  close: () => send('tasks', { open: false }),
  body: () => {
    const ps = [...(RAMS.procs || [])].sort((a, b) => b.mem - a.mem);
    if (!ps.length) return `<div class="empty">…</div>`;
    const total = ps.reduce((a, p) => a + p.mem, 0);
    const rows = ps.map((p) => {
      const what = p.ui ? t('pf.ui') : p.titles.length ? p.titles.map((x) => esc(x)).join('<br>') : p.ext.length ? p.ext.map((x) => '🧩 ' + esc(x)).join('<br>') : '';
      const end = (p.kind === 'renderer' || p.kind === 'utility') && !p.ui
        ? `<button class="mini" data-f="proc-end" data-pid="${p.pid}">${t('pf.end')}</button>` : '';
      const unload = p.tabs.length && !p.tabs.includes(S.a) ? `<button class="mini" data-f="tab-unload" data-ids="${p.tabs.join(',')}">${t('pf.unload')}</button>` : '';
      return `<tr><td><b>${t(PF_KIND[p.kind] || 'pf.kOther')}</b>${what ? `<div class="sub2">${what}</div>` : ''}</td><td class="n">${fmtMem(p.mem)}</td><td class="n">${p.cpu.toLocaleString(LANG)} %</td><td class="n">${p.pid}</td><td class="n">${unload}${end}</td></tr>`;
    }).join('');
    return `<p class="lead">${t('pf.tasksLead', { n: ps.length, m: fmtMem(total) })}</p>`
      + `<div class="group"><table class="nt tm"><thead><tr><th>${t('pf.proc')}</th><th class="n">${t('pf.mem')}</th><th class="n">CPU</th><th class="n">PID</th><th></th></tr></thead><tbody>${rows}</tbody></table></div>`;
  },
};
FX.click['proc-end'] = (el) => send('proc-end', { pid: +el.dataset.pid });
FX.click['tab-unload'] = (el) => el.dataset.ids.split(',').forEach((id) => send('tab-unload', { id: +id }));
FX.set.taskmgr = () => openPage('tasks');

/* ---------- #2 Économie de batterie : feuille dans la barre, animations coupées ---------- */
FX.bar.push(() => {
  let b = $('battBtn');
  if (!b) {
    b = document.createElement('button');
    b.className = 'nb'; b.id = 'battBtn'; b.dataset.f = 'batt-info';
    b.innerHTML = svg('<path d="M13 3C6 3 3 6.5 3 11c0 .9.2 1.6.5 2.2C5 9.5 8 7.5 11 6.5 8.5 8 6.5 10.5 5.3 13.3c.9.5 2 .7 3.2.7C12.5 14 14 10 13 3z"/>');
    $('menuBtn').before(b);
  }
  b.hidden = !(S.batt && on('battIcon'));
  b.title = t('pf.battOn'); b.setAttribute('aria-label', t('pf.battOn'));
  // « Réduire les animations » les coupe toutes ; l'économie de batterie (rmb) garde l'écran de démarrage et
  // l'assistant de bienvenue, courts et demandés (« Revoir l'assistant de bienvenue » restait figé sur batterie)
  document.body.classList.toggle('rm', !!C().motion);
  document.body.classList.toggle('rmb', !C().motion && !!S.batt && on('battAnim'));
  document.body.classList.toggle('ramhov', C().ramShow !== 'always');
});
FX.click['batt-info'] = () => { SET.sec = 'perf'; setMode('set'); };

/* ---------- #3 Ne jamais mettre en veille ce site (clic droit sur un onglet) ---------- */
FX.ctx.push((x) => on('sleepWl') && x.u && /^https?:/.test(x.u) ? row('nosleep', t(x.ns ? 'pf.sleepOk' : 'pf.noSleep')) : '');
FX.act.nosleep = (d, id) => send('nosleep', { id });

/* ---------- #5 Mode Lite par site ---------- */
const LITE_BITS = [['js', 1], ['img', 2], ['font', 4], ['auto', 8]];
const litePop = () => {
  const h = host(act().u || ''), f = S.lite || 0;
  return `<h3 class="ph3">${t('pf.liteT')}</h3><p class="lead" style="margin:0 0 8px">${esc(h)}</p>`
    + LITE_BITS.map(([k, b]) => `<div class="opt"><div class="lbl"><b>${t('pf.lite.' + k)}</b></div>${sw2(f & b, `data-f="lite-set" data-k="${k}" data-on="${!(f & b)}"`, t('pf.lite.' + k))}</div>`).join('')
    + `<p class="small2">${t('pf.liteNote')}</p>`;
};
FX.menu.push((x, web) => on('lite') && web ? row('lite', t('pf.liteT'), S.lite ? true : '') : '');
FX.act.lite = () => openPop(litePop);
FX.click['lite-set'] = (el) => { send('lite-set', { k: el.dataset.k, on: el.dataset.on === 'true' }); setTimeout(() => mode === 'pop' && renderPanel(), 120); };
FX.bar.push(() => {
  let b = $('liteBadge');
  if (!b) { b = document.createElement('button'); b.id = 'liteBadge'; b.className = 'litebadge'; b.dataset.f = 'lite-open'; b.textContent = 'Lite'; $('q').before(b); }
  b.hidden = !(on('lite') && on('liteBadge') && S.lite);
});
FX.click['lite-open'] = () => openPop(litePop);
SPECIAL.liteSites = () => {
  const m = C().liteSites || {}, keys = Object.keys(m);
  if (!keys.length) return `<p class="small2">${t('k.liteSites.none')}</p>`;
  return `<div class="chips">${keys.map((h) => `<span class="chip">${esc(h)} <small>${LITE_BITS.filter(([, b]) => m[h] & b).map(([k]) => t('pf.liteS.' + k)).join(' · ') || t('pf.liteNone')}</small><button data-f="lite-rm" data-h="${esc(h)}" aria-label="${esc(t('b.remove'))} ${esc(h)}">${I.x}</button></span>`).join('')}</div>`;
};
FX.click['lite-rm'] = (el) => send('lite-rm', { host: el.dataset.h });

/* ---------- #6 Statistiques d'économies ---------- */
let STATS_D = null;
window.STATS = (o) => { STATS_D = o; if (mode === 'pg' && PG.name === 'stats') renderPage(); else if (!mode && isBlank()) renderPanel(); };
// données évitées et temps gagné : estimations par requête bloquée (25 ko, 40 ms)
function statsSum(period) {
  const days = (STATS_D && STATS_D.days) || {};
  const d0 = new Date(); d0.setHours(0, 0, 0, 0);
  const ymd = (d) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  const from = period === 'today' ? ymd(d0) : period === 'week' ? ymd(new Date(d0 - 6 * 864e5)) : '0000';
  let b = 0, f = 0;
  for (const [k, v] of Object.entries(days)) if (k >= from) { b += v.b || 0; f += v.f || 0; }
  return { b, f, kb: b * 25, s: b * 0.04 };
}
const fmtKb = (kb) => kb >= 1048576 ? `${(kb / 1048576).toLocaleString(LANG, { maximumFractionDigits: 1 })} ${LANG === 'fr' ? 'Go' : 'GB'}` : kb >= 1024 ? `${Math.round(kb / 1024).toLocaleString(LANG)} ${LANG === 'fr' ? 'Mo' : 'MB'}` : `${Math.round(kb)} ${LANG === 'fr' ? 'ko' : 'kB'}`;
const fmtSec = (s) => s >= 3600 ? `${(s / 3600).toLocaleString(LANG, { maximumFractionDigits: 1 })} h` : s >= 60 ? `${Math.round(s / 60)} min` : `${Math.round(s)} s`;
function statsCards(sum) {
  return `<div class="stcards">`
    + `<div><b>${fmtMem(sum.f * 1048576)}</b><span>${t('pf.stFreed')}</span></div>`
    + `<div><b>${fmtNum(sum.b)}</b><span>${t('pf.stBlocked')}</span></div>`
    + `<div><b>${fmtKb(sum.kb)}</b><span>${t('pf.stData')}</span></div>`
    + `<div><b>${fmtSec(sum.s)}</b><span>${t('pf.stTime')}</span></div></div>`;
}
FX.pages.stats = {
  title: () => t('pf.stats'),
  open: () => send('stats-get'),
  actions: () => `<div class="seg" role="radiogroup">${['today', 'week', 'all'].map((p) => `<button role="radio" aria-checked="${(C().statsPeriod || 'week') === p}" data-sk-p="${p}" data-f="st-period" data-p="${p}">${t('k.statsPeriod.' + p)}</button>`).join('')}</div>`,
  body: () => {
    if (!on('stats')) return `<div class="empty">${t('pf.stOff')}</div>`;
    const days = Object.entries((STATS_D && STATS_D.days) || {}).sort().slice(-14);
    const max = Math.max(1, ...days.map(([, v]) => v.b || 0));
    const bars = days.length ? `<div class="ghd">${t('pf.stDays')}</div><div class="group stbars">${days.map(([k, v]) => `<div title="${esc(k)} · ${fmtNum(v.b || 0)}"><i style="height:${Math.round((v.b || 0) / max * 100)}%"></i><span>${k.slice(8)}</span></div>`).join('')}</div>` : '';
    return statsCards(statsSum(C().statsPeriod || 'week')) + bars + `<p class="small2">${t('pf.stNote')}</p>`;
  },
};
FX.click['st-period'] = (el) => send('set', { k: 'statsPeriod', v: el.dataset.p });
FX.set['stats-reset'] = () => { if (confirm(t('pf.stResetQ'))) send('stats-reset'); };
// carte discrète sur la nouvelle page
FX.home.push((x) => {
  if (!on('stats') || !on('statsHome') || x.p) return '';
  if (!STATS_D) { send('stats-get'); return ''; }
  const sum = statsSum(C().statsPeriod || 'week');
  if (!sum.b && !sum.f) return '';
  return `<button class="stmini" data-f="open-stats" title="plume://stats">${statsCards(sum)}<span class="stper">${t('k.statsPeriod.' + (C().statsPeriod || 'week'))}</span></button>`;
});
FX.click['open-stats'] = () => openPage('stats');
