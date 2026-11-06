/* ===== 0.7 · Apparence : teinte adaptative et verre (#26), fond de la nouvelle page (#27), widgets (#28),
   CSS / JS par site (#29), densité compacte, mode zen et plein écran (#30) ===== */
Object.assign(TXT.en, {
  'w.clock': 'Clock', 'w.weather': 'Weather', 'w.todo': 'To-do list', 'w.freq': 'Most visited sites', 'w.up': 'Move up', 'w.down': 'Move down',
  'wx.setCity': 'Add your city in Settings › Appearance to see the weather.', 'wx.loading': 'Loading the weather…', 'wx.noCity': 'City not found. Check its name in Settings › Appearance.',
  'wx.err': 'Weather unavailable (no connection?)', 'wx.c0': 'Clear', 'wx.c1': 'Mostly clear', 'wx.c2': 'Partly cloudy', 'wx.c3': 'Cloudy', 'wx.fog': 'Fog', 'wx.drizzle': 'Drizzle',
  'wx.rain': 'Rain', 'wx.snow': 'Snow', 'wx.showers': 'Showers', 'wx.storm': 'Thunderstorm',
  'td.title': 'To do', 'td.ph': 'Add a task and press Enter', 'td.clear': 'Clear done', 'td.del': 'Delete task', 'td.empty': 'Nothing to do. Nice.',
  'cc.menu': 'Customize this site…', 'cc.title': 'Customize {h}', 'cc.css': 'CSS', 'cc.cssSub': 'Applied as soon as the page loads.', 'cc.js': 'JavaScript',
  'cc.warn': 'A script added here runs on every page of this site with full access: it can read what you type, including passwords. Only paste code you wrote or fully trust.',
  'cc.ack': 'I understand the risk', 'cc.on': 'Active on this site', 'cc.del': 'Remove customization', 'cc.none': 'No site customized yet. Use the page menu › Customize this site.',
  'cc.export': 'Export', 'cc.import': 'Import', 'cc.edit': 'Edit', 'cc.jsOff': 'JavaScript is off: choose “CSS and JavaScript” above to run it.',
  'toast.cc-saved': 'Customization saved', 'toast.cc-exported': 'Customizations exported', 'toast.cc-imported': '{n} sites imported', 'toast.cc-bad': 'This file is not a Plume export',
  'toast.ntp-big': 'This picture is too large (25 MB max.)',
  'zn.menu': 'Zen mode', 'fs.menu': 'Full screen', 'a.zen': 'Zen mode', 'a.fs': 'Full screen', 'a.cc': 'Customize this site',
});
Object.assign(TXT.fr, {
  'w.clock': 'Horloge', 'w.weather': 'Météo', 'w.todo': 'Liste de tâches', 'w.freq': 'Sites les plus visités', 'w.up': 'Monter', 'w.down': 'Descendre',
  'wx.setCity': 'Ajoutez votre ville dans Paramètres › Apparence pour voir la météo.', 'wx.loading': 'Chargement de la météo…', 'wx.noCity': 'Ville introuvable. Vérifiez son nom dans Paramètres › Apparence.',
  'wx.err': 'Météo indisponible (pas de connexion ?)', 'wx.c0': 'Ciel dégagé', 'wx.c1': 'Plutôt dégagé', 'wx.c2': 'Partiellement nuageux', 'wx.c3': 'Couvert', 'wx.fog': 'Brouillard', 'wx.drizzle': 'Bruine',
  'wx.rain': 'Pluie', 'wx.snow': 'Neige', 'wx.showers': 'Averses', 'wx.storm': 'Orage',
  'td.title': 'À faire', 'td.ph': 'Ajoutez une tâche puis Entrée', 'td.clear': 'Retirer les tâches faites', 'td.del': 'Supprimer la tâche', 'td.empty': 'Rien à faire. Parfait.',
  'cc.menu': 'Personnaliser ce site…', 'cc.title': 'Personnaliser {h}', 'cc.css': 'CSS', 'cc.cssSub': 'Appliqué dès le chargement de la page.', 'cc.js': 'JavaScript',
  'cc.warn': "Un script ajouté ici s'exécute sur toutes les pages de ce site avec un accès complet : il peut lire ce que vous tapez, mots de passe compris. Ne collez que du code écrit par vous ou de toute confiance.",
  'cc.ack': 'Je comprends le risque', 'cc.on': 'Actif sur ce site', 'cc.del': 'Retirer la personnalisation', 'cc.none': 'Aucun site personnalisé. Menu de la page › Personnaliser ce site.',
  'cc.export': 'Exporter', 'cc.import': 'Importer', 'cc.edit': 'Modifier', 'cc.jsOff': 'JavaScript est coupé : choisissez « CSS et JavaScript » ci-dessus pour l\'exécuter.',
  'toast.cc-saved': 'Personnalisation enregistrée', 'toast.cc-exported': 'Personnalisations exportées', 'toast.cc-imported': '{n} sites importés', 'toast.cc-bad': "Ce fichier n'est pas un export de Plume",
  'toast.ntp-big': 'Cette image est trop lourde (25 Mo max.)',
  'zn.menu': 'Mode zen', 'fs.menu': 'Plein écran', 'a.zen': 'Mode zen', 'a.fs': 'Plein écran', 'a.cc': 'Personnaliser ce site',
});

/* ---------- Apparence appliquée à chaque état envoyé par Plume ---------- */
const LOOK = { peek: false, zt: 0, clockT: 0 };
function applyLook() {
  const c = C(), b = document.body.classList, r = document.documentElement.style;
  b.toggle('compact', c.density === 'compact');
  // #26 verre : opacité et flou des surfaces translucides (60 = l'apparence d'origine)
  const g = Math.max(0, Math.min(100, +c.glass || 0)), ga = g <= 60 ? 1 - g / 1000 : 0.94 - (g - 60) * 0.006;
  r.setProperty('--glass-a', ga.toFixed(3));
  r.setProperty('--glass-b', Math.round(g * 0.4) + 'px');
  // #26 teinte adaptative (jamais en navigation privée)
  const tint = c.adaptive && S.tint && !act().p ? S.tint : '';
  b.toggle('tinted', !!tint);
  if (tint) r.setProperty('--tint', tint);
  // #30 zen et plein écran d'une vidéo
  b.toggle('zen', !!S.zen && !S.pageFs);
  b.toggle('pagefs', !!S.pageFs);
  if (!S.zen && LOOK.peek) zenPeek(false);
  // #27 fond de la nouvelle page
  const bg = c.ntpBg || 'gradient';
  for (const k of ['gradient', 'animated', 'color', 'image', 'daily']) b.toggle('ntp-' + k, bg === k);
  b.toggle('onbatt', !!S.batt);
  r.setProperty('--ntpc', /^#[0-9a-f]{3,8}$/i.test(c.ntpColor || '') ? c.ntpColor : '#1c1c1e');
  r.setProperty('--ntpblur', Math.max(0, Math.min(30, +c.ntpBlur || 0)) + 'px');
  r.setProperty('--ntpdim', String(Math.max(0, Math.min(80, +c.ntpDim || 0)) / 100));
  r.setProperty('--ntpimg', bg === 'image' ? `url("/ntp/image?v=${S.ntpV || 0}")` : bg === 'daily' && S.daily ? `url("/ntp/daily?v=${S.ntpV || 0}")` : 'none');
}
const lookOnState0 = window.onState;
window.onState = () => { if (lookOnState0) lookOnState0(); applyLook(); };

/* ---------- #30 Mode zen : la barre réapparaît au survol du haut (ou avec la barre d'adresse) ---------- */
const zenStrip = document.createElement('div');
zenStrip.id = 'zenStrip';
document.body.appendChild(zenStrip);
function zenPeek(want) {
  if (want === LOOK.peek || (want && !document.body.classList.contains('zen'))) return;
  LOOK.peek = want;
  document.body.classList.toggle('peek', want);
  if (!mode) send('ov', { v: want }); // la barre passe au-dessus de la page, seulement sur sa forme
}
zenStrip.addEventListener('mouseenter', () => { clearTimeout(LOOK.zt); LOOK.zt = setTimeout(() => zenPeek(true), Math.max(0, +C().zenDelay || 0)); });
zenStrip.addEventListener('mouseleave', () => { if (!LOOK.peek) clearTimeout(LOOK.zt); });
$('hdr').addEventListener('mouseleave', () => { setTimeout(() => { if (LOOK.peek && !mode && document.activeElement !== $('q') && !$('hdr').matches(':hover')) zenPeek(false); }, 250); });
$('q').addEventListener('focus', () => { if (document.body.classList.contains('zen')) zenPeek(true); });
$('q').addEventListener('blur', () => { setTimeout(() => { if (LOOK.peek && !mode && !$('hdr').matches(':hover')) zenPeek(false); }, 200); });
// un menu se ferme pendant que la barre est montrée : elle reste au-dessus de la page
const lookSetMode0 = setMode;
setMode = function (m, f) { const r = lookSetMode0(m, f); if (!m && LOOK.peek) send('ov', { v: true }); return r; };
FX.menu.push(() => (C().zen && C().zen !== 'off' ? row('zen-toggle', t('zn.menu'), S.zen ? true : C().zenKey || '') : '') + row('fullscreen', t('fs.menu'), S.fs ? true : 'F11'));
FX.act['zen-toggle'] = () => send('zen-toggle');
FX.act.fullscreen = () => send('fullscreen');
FX.pal.push(['zen', () => C().zenKey, () => send('zen-toggle'), () => C().zen && C().zen !== 'off'], ['fs', 'F11', () => send('fullscreen')]);

/* ---------- #28 Widgets de la nouvelle page ---------- */
const WIDGETS = ['clock', 'weather', 'todo', 'freq'];
const wList = () => (Array.isArray(C().widgets) ? C().widgets : []).filter((w, i, a) => WIDGETS.includes(w) && a.indexOf(w) === i);
const homeLive = () => isBlank() && (!mode || mode === 'omni');
// « sites les plus visités » : affichés seulement si le widget est ajouté
window.homeFreq = (o) => (wList().includes('freq') ? o.freq : '');
const W_RENDER = {};

// horloge : mise à jour toutes les 15 s tant qu'elle est affichée
const clockNow = () => {
  const d = new Date(), f = C().wClock24, o = { hour: 'numeric', minute: '2-digit' };
  if (f === '12') o.hour12 = true; else if (f === '24') o.hour12 = false;
  return [d.toLocaleTimeString(LANG, o), d.toLocaleDateString(LANG, { weekday: 'long', day: 'numeric', month: 'long' })];
};
const clockTick = () => {
  const el = $('wct');
  if (!el) { clearInterval(LOOK.clockT); LOOK.clockT = 0; return; }
  const [tm, dt] = clockNow(); el.textContent = tm; $('wcd').textContent = dt;
};
W_RENDER.clock = () => {
  if (!LOOK.clockT) LOOK.clockT = setInterval(clockTick, 15000);
  const [tm, dt] = clockNow();
  return `<div class="wcard wclock"><b id="wct">${esc(tm)}</b><span id="wcd">${esc(dt)}</span></div>`;
};

// météo : Plume la garde 30 min ; l'interface la redemande au plus une fois par minute
const WX = { d: undefined, at: 0 };
const WX_I = {
  sun: '<circle cx="12" cy="12" r="4.2"/><path d="M12 2.5v2.2M12 19.3v2.2M2.5 12h2.2M19.3 12h2.2M5.3 5.3l1.5 1.5M17.2 17.2l1.5 1.5M5.3 18.7l1.5-1.5M17.2 6.8l1.5-1.5"/>',
  moon: '<path d="M19.5 14.5A7.5 7.5 0 0 1 9.5 4.5a7.5 7.5 0 1 0 10 10z"/>',
  cloud: '<path d="M7 18.5h10.3a4 4 0 0 0 .3-8 5.5 5.5 0 0 0-10.6 1.4A3.3 3.3 0 0 0 7 18.5z"/>',
  rain: '<path d="M7 15.5h10.3a4 4 0 0 0 .3-8 5.5 5.5 0 0 0-10.6 1.4A3.3 3.3 0 0 0 7 15.5zM8.5 18.5l-1 2M12.5 18.5l-1 2M16.5 18.5l-1 2"/>',
  snow: '<path d="M7 15h10.3a4 4 0 0 0 .3-8 5.5 5.5 0 0 0-10.6 1.4A3.3 3.3 0 0 0 7 15zM9 18.5h.01M12 20h.01M15 18.5h.01"/>',
  storm: '<path d="M7 15h10.3a4 4 0 0 0 .3-8 5.5 5.5 0 0 0-10.6 1.4A3.3 3.3 0 0 0 7 15zM12.5 15.5l-2 3.5h3l-2 3.5"/>',
  fog: '<path d="M4 10h16M6 14h12M8 18h8"/>',
};
const wxKind = (c) => (c === 0 || c === 1 ? ['c' + c, 'sun'] : c === 2 ? ['c2', 'cloud'] : c === 3 ? ['c3', 'cloud'] : c === 45 || c === 48 ? ['fog', 'fog']
  : c >= 51 && c <= 57 ? ['drizzle', 'rain'] : (c >= 61 && c <= 67) ? ['rain', 'rain'] : (c >= 71 && c <= 77) || c === 85 || c === 86 ? ['snow', 'snow']
    : c >= 80 && c <= 82 ? ['showers', 'rain'] : c >= 95 ? ['storm', 'storm'] : ['c3', 'cloud']);
const wxUnit = () => (/^en-(US|LR)|^my/i.test(navigator.language || '') ? 'f' : 'c');
W_RENDER.weather = () => {
  if (!(C().wCity || '').trim()) return `<button class="wcard wwx wmsg" data-f="wx-set">${t('wx.setCity')}</button>`;
  if (Date.now() - WX.at > 60000) { WX.at = Date.now(); send('wx', { u: wxUnit() }); }
  const d = WX.d;
  if (!d) return `<div class="wcard wwx wmsg">${t('wx.loading')}</div>`;
  if (d.err) return `<button class="wcard wwx wmsg" data-f="wx-set">${t(d.err === 'city' ? 'wx.noCity' : 'wx.err')}</button>`;
  const [k, ic] = wxKind(+d.code), icon2 = ic === 'sun' && !d.day ? 'moon' : ic, n = (v) => Math.round(+v);
  return `<div class="wcard wwx"><span class="wxi">${svg(WX_I[icon2], 24)}</span><div class="wxt"><b>${n(d.t)}°</b><span>${esc(d.city || '')} · ${t('wx.' + k)}</span><small>${n(d.max)}° / ${n(d.min)}°</small></div></div>`;
};
window.wxRes = (d) => { WX.d = d; if (homeLive()) renderPanel(); };
FX.click['wx-set'] = () => { SET.sec = 'look'; setMode('set'); };

// tâches : enregistrées par Plume (todo.json)
const TODO = { l: null };
W_RENDER.todo = () => {
  if (TODO.l === null) { TODO.l = []; send('todo-get'); }
  const done = TODO.l.some((x) => x.d);
  return `<div class="wcard wtodo"><div class="wth"><span>${t('td.title')}</span>${done ? `<button class="mini" data-f="td-clear">${t('td.clear')}</button>` : ''}</div>`
    + `<ul>${TODO.l.map((x, i) => `<li class="${x.d ? 'done' : ''}"><button class="tdc" data-f="td-tog" data-i="${i}" role="checkbox" aria-checked="${!!x.d}" aria-label="${esc(x.t)}">${x.d ? I.check : ''}</button><span>${esc(x.t)}</span><button class="tdx icb" data-f="td-del" data-i="${i}" aria-label="${esc(t('td.del'))}">${I.x}</button></li>`).join('') || `<li class="empty2">${t('td.empty')}</li>`}</ul>`
    + `<form class="tdf" id="tdf"><input class="inp" id="tdi" maxlength="200" placeholder="${esc(t('td.ph'))}" aria-label="${esc(t('td.ph'))}" autocomplete="off"></form></div>`;
};
window.todoRes = (l) => {
  TODO.l = Array.isArray(l) ? l : [];
  if (!homeLive()) return;
  const f = document.activeElement && document.activeElement.id === 'tdi';
  renderPanel();
  if (f) $('tdi')?.focus();
};
document.addEventListener('submit', (e) => {
  if (e.target.id !== 'tdf') return;
  e.preventDefault();
  const v = $('tdi').value.trim();
  if (v) { send('todo-add', { t: v }); $('tdi').value = ''; }
});
FX.click['td-tog'] = (el) => send('todo-tog', { i: +el.dataset.i });
FX.click['td-del'] = (el) => send('todo-del', { i: +el.dataset.i });
FX.click['td-clear'] = () => send('todo-clear');

FX.home.unshift((x) => {
  let h = '';
  const l = wList().filter((w) => w !== 'freq');
  if (l.length && !x.p) h += `<div class="wrow">${l.map((w) => W_RENDER[w]()).join('')}</div>`;
  if (C().ntpBg === 'daily' && S.daily && S.daily.credit) {
    const cr = esc(S.daily.credit);
    h += S.daily.link ? `<button class="ntpcredit" data-u="${esc(S.daily.link)}" title="${cr}">${cr}</button>` : `<p class="ntpcredit">${cr}</p>`;
  }
  return h;
});

// réglages : ajouter, retirer et réordonner les widgets
const W_I = { up: svg('<path d="M4 10l4-4 4 4"/>'), down: svg('<path d="M4 6l4 4 4-4"/>') };
SPECIAL.widgets = () => {
  const l = wList(), all = [...l, ...WIDGETS.filter((w) => !l.includes(w))];
  return `<div class="wlist">${all.map((w) => {
    const k = l.indexOf(w), onW = k >= 0;
    return `<div class="wl${onW ? '' : ' off'}"><span class="tt">${t('w.' + w)}</span>`
      + (onW ? `<button class="icb" data-f="w-mv" data-w="${w}" data-d="-1"${k === 0 ? ' disabled' : ''} aria-label="${esc(t('w.up'))}">${W_I.up}</button><button class="icb" data-f="w-mv" data-w="${w}" data-d="1"${k === l.length - 1 ? ' disabled' : ''} aria-label="${esc(t('w.down'))}">${W_I.down}</button>` : '')
      + sw2(onW, `data-f="w-tog" data-w="${w}"`, t('w.' + w)) + '</div>';
  }).join('')}</div>`;
};
FX.click['w-tog'] = (el) => { const l = wList(), w = el.dataset.w; send('set', { k: 'widgets', v: l.includes(w) ? l.filter((x) => x !== w) : [...l, w] }); };
FX.click['w-mv'] = (el) => {
  const l = wList(), i = l.indexOf(el.dataset.w), j = i + +el.dataset.d;
  if (i < 0 || j < 0 || j >= l.length) return;
  [l[i], l[j]] = [l[j], l[i]];
  send('set', { k: 'widgets', v: l });
};

/* ---------- #27 Image personnelle ---------- */
FX.set['ntp-image'] = () => send('ntp-image');

/* ---------- #29 CSS et JS par site ---------- */
const CCE = { h: '', css: '', js: '', on: true, ack: false, del: false };
FX.menu.push((x, w) => (on('customCss') && w ? row('cc-edit', t('cc.menu')) : ''));
FX.act['cc-edit'] = () => { const h = hostP(act().u || ''); if (h) send('cc-get', { h }); };
FX.pal.push(['cc', '', () => FX.act['cc-edit'](), () => on('customCss') && web()]);
window.ccRes = (e) => {
  Object.assign(CCE, { h: e.h, css: e.css || '', js: e.js || '', on: e.on !== false, ack: !!(e.js || '').trim(), del: false });
  openModal(ccForm);
  setTimeout(() => $('cccss')?.focus(), 40);
};
const ccForm = () => {
  const js = C().ccMode === 'cssjs';
  return `<h3>${esc(t('cc.title', { h: CCE.h }))}</h3>`
    + `<label class="ghd" for="cccss">${t('cc.css')} <small>${t('cc.cssSub')}</small></label><textarea class="code" id="cccss" spellcheck="false" placeholder="body { font-size: 18px; }">${esc(CCE.css)}</textarea>`
    + (js ? `<label class="ghd" for="ccjs">${t('cc.js')}</label><div class="ccwarn" role="note">${t('cc.warn')}</div>`
      + `<label class="ccack"><input type="checkbox" id="ccack"${CCE.ack ? ' checked' : ''}> ${t('cc.ack')}</label>`
      + `<textarea class="code" id="ccjs" spellcheck="false"${CCE.ack ? '' : ' disabled'}>${esc(CCE.js)}</textarea>`
      : (CCE.js.trim() ? `<p class="small2">${t('cc.jsOff')}</p>` : ''))
    + `<div class="opt"><div class="lbl"><b>${t('cc.on')}</b></div>${sw2(CCE.on, 'data-f="cc-on"', t('cc.on'))}</div>`
    + `<div class="acts" style="margin-top:12px">${CCE.css || CCE.js ? `<button class="pill red" data-f="cc-del">${t(CCE.del ? 's.resetConfirm' : 'cc.del')}</button><span style="flex:1"></span>` : ''}`
    + `<button class="pill" data-m="close-ov">${t('b.cancel')}</button><button class="pill pri" data-f="cc-save">${t('b.save')}</button></div>`;
};
const ccKeep = () => { CCE.css = $('cccss').value; if ($('ccjs')) CCE.js = $('ccjs').value; };
document.addEventListener('change', (e) => { if (e.target.id === 'ccack') { ccKeep(); CCE.ack = e.target.checked; renderPanel(); } });
FX.click['cc-on'] = () => { ccKeep(); CCE.on = !CCE.on; renderPanel(); };
FX.click['cc-save'] = () => {
  ccKeep();
  send('cc-set', { h: CCE.h, css: CCE.css, js: CCE.ack ? CCE.js : '', on: CCE.on });
  setMode(null);
  toast('cc-saved');
};
FX.click['cc-del'] = () => {
  if (!CCE.del) { ccKeep(); CCE.del = true; return renderPanel(); }
  send('cc-del', { h: CCE.h }); setMode(null);
};
SPECIAL.ccSites = () => {
  const l = S.ccList || [];
  const rows = l.map((x) => `<div class="crow"><b>${esc(x.h)}</b><span class="chips">${x.css ? '<span class="chip">CSS</span>' : ''}${x.js ? '<span class="chip">JS</span>' : ''}</span>`
    + `<button class="mini" data-f="cc-open" data-h="${esc(x.h)}">${t('cc.edit')}</button>${sw2(x.on, `data-f="cc-tog" data-h="${esc(x.h)}"`, x.h)}</div>`).join('');
  return (rows || `<p class="small2" style="margin:0">${t('cc.none')}</p>`)
    + `<div class="cadd"><button class="mini" data-f="cc-import">${t('cc.import')}</button>${l.length ? `<button class="mini" data-f="cc-export">${t('cc.export')}</button>` : ''}</div>`;
};
FX.click['cc-open'] = (el) => send('cc-get', { h: el.dataset.h });
FX.click['cc-tog'] = (el) => send('cc-tog', { h: el.dataset.h });
FX.click['cc-export'] = () => send('cc-export');
FX.click['cc-import'] = () => send('cc-import');
