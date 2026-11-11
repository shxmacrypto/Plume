/* ===== 0.7 · Confidentialité : conteneurs (#19), rapport de confidentialité (#21), générateur et audit (#22),
   fuites (#23), Windows Hello (#24). L'effacement à la fermeture (#20) et le DNS sécurisé (#25) n'ont que des réglages. ===== */
Object.assign(TXT.en, {
  'ct.in': 'Container: {c}', 'ct.reopen': 'Reopen in “{c}”', 'ct.use': 'Use container “{c}”', 'ct.out': 'Reopen without container',
  'ct.always': 'Always open {h} in “{c}”', 'ct.newFrom': 'New container…', 'ct.newTabMenu': 'New container tab…', 'ct.newTab': 'New tab in “{c}”',
  'ct.new': 'New container', 'ct.edit': 'Edit container', 'ct.editB': 'Edit', 'ct.name': 'Name', 'ct.del': 'Delete container', 'ct.open': 'New tab',
  'ct.none': 'No container yet. Each container keeps its own cookies and sign-ins.', 'ct.sitesHelp': 'Right-click a tab in a container to always open its site there.',
  'ct.s1': 'Personal', 'ct.s2': 'Work', 'ct.s3': 'Shopping', 'ct.s4': 'Banking', 'ct.pick': 'Open a new tab in', 'ct.delNote': 'Its tabs go back to normal and its cookies are deleted at the next start.',
  'pr.title': 'Privacy on this site', 'pr.block': 'Block trackers on this site', 'pr.blockSub': 'Turn off if the site does not work properly.', 'pr.abOff': 'Tracker blocking is off in Settings.',
  'pr.https': 'Secure connection (HTTPS)', 'pr.http': 'Connection not secure', 'pr.blocked': 'Trackers blocked', 'pr.third': 'Other sites contacted',
  'pr.cookies': 'Cookies from this site', 'pr.perms': 'Permissions granted', 'pr.none': 'None', 'pr.wait': 'Analyzing the page…',
  'pr.perm.geolocation': 'Location', 'pr.perm.notifications': 'Notifications', 'pr.perm.camera': 'Camera', 'pr.perm.microphone': 'Microphone',
  'pr.more': '+{n} more', 'pr.detail': 'Settings › Privacy › Privacy report shows the full lists.',
  'pg2.gen': 'Password generator', 'pg2.copy': 'Copy', 'pg2.again': 'New password', 'pg2.opts': 'Length, symbols and passphrase are in Settings › Privacy.',
  'pa.title': 'Password health', 'pa.check': 'Check again', 'pa.leaks': 'Check for leaks', 'pa.leaksSub': 'Uses Have I Been Pwned: only 5 characters of a hash are sent.',
  'pa.sum': '{w} weak · {r} reused · {l} leaked', 'pa.sumNoLeak': '{w} weak · {r} reused', 'pa.weak': 'weak', 'pa.reused': 'reused', 'pa.leaked': 'found in {n} leaks', 'pa.ok': 'All your passwords look good.',
  'pa.busy': 'Checking…', 'pa.err': 'Leak check failed (no connection?)',
  'info.gen': 'Use a strong password for this account?', 'b.usePw': 'Use it',
  'hl.use': 'Windows Hello', 'hl.busy': 'Waiting for Windows Hello…',
  'toast.hello-on': 'Windows Hello can now unlock your passwords', 'toast.hello-unlock': 'Unlock your passwords first, then turn on Windows Hello',
  'toast.hello-novault': 'Create your password vault first', 'toast.hello-unsupported': 'Windows Hello is not set up on this PC',
  'toast.hello-canceled': 'Windows Hello was canceled', 'toast.hello-failed': 'Windows Hello did not work', 'toast.pw-breached': 'This password appears in {n} known leaks. Change it.',
  'toast.pw-copied': 'Password copied',
});
Object.assign(TXT.fr, {
  'ct.in': 'Conteneur : {c}', 'ct.reopen': 'Rouvrir dans « {c} »', 'ct.use': 'Utiliser le conteneur « {c} »', 'ct.out': 'Rouvrir sans conteneur',
  'ct.always': 'Toujours ouvrir {h} dans « {c} »', 'ct.newFrom': 'Nouveau conteneur…', 'ct.newTabMenu': 'Nouvel onglet de conteneur…', 'ct.newTab': 'Nouvel onglet dans « {c} »',
  'ct.new': 'Nouveau conteneur', 'ct.edit': 'Modifier le conteneur', 'ct.editB': 'Modifier', 'ct.name': 'Nom', 'ct.del': 'Supprimer le conteneur', 'ct.open': 'Nouvel onglet',
  'ct.none': 'Aucun conteneur pour le moment. Chaque conteneur garde ses propres cookies et connexions.', 'ct.sitesHelp': "Clic droit sur un onglet d'un conteneur pour toujours y ouvrir son site.",
  'ct.s1': 'Personnel', 'ct.s2': 'Travail', 'ct.s3': 'Achats', 'ct.s4': 'Banque', 'ct.pick': 'Ouvrir un nouvel onglet dans', 'ct.delNote': 'Ses onglets redeviennent normaux et ses cookies sont supprimés au prochain démarrage.',
  'pr.title': 'Confidentialité sur ce site', 'pr.block': 'Bloquer les traqueurs sur ce site', 'pr.blockSub': 'À couper si le site ne fonctionne pas bien.', 'pr.abOff': 'Le blocage des traqueurs est coupé dans les paramètres.',
  'pr.https': 'Connexion sécurisée (HTTPS)', 'pr.http': 'Connexion non sécurisée', 'pr.blocked': 'Traqueurs bloqués', 'pr.third': 'Autres sites contactés',
  'pr.cookies': 'Cookies de ce site', 'pr.perms': 'Permissions accordées', 'pr.none': 'Aucune', 'pr.wait': 'Analyse de la page…',
  'pr.perm.geolocation': 'Position', 'pr.perm.notifications': 'Notifications', 'pr.perm.camera': 'Caméra', 'pr.perm.microphone': 'Micro',
  'pr.more': '+{n} autres', 'pr.detail': 'Paramètres › Confidentialité › Rapport de confidentialité affiche les listes complètes.',
  'pg2.gen': 'Générateur de mots de passe', 'pg2.copy': 'Copier', 'pg2.again': 'Nouveau mot de passe', 'pg2.opts': 'Longueur, symboles et phrase de passe : Paramètres › Confidentialité.',
  'pa.title': 'Santé des mots de passe', 'pa.check': 'Vérifier à nouveau', 'pa.leaks': 'Chercher les fuites', 'pa.leaksSub': "Utilise Have I Been Pwned : seuls 5 caractères d'une empreinte sont envoyés.",
  'pa.sum': '{w} faibles · {r} réutilisés · {l} divulgués', 'pa.sumNoLeak': '{w} faibles · {r} réutilisés', 'pa.weak': 'faible', 'pa.reused': 'réutilisé', 'pa.leaked': 'dans {n} fuites', 'pa.ok': 'Tous vos mots de passe ont l’air solides.',
  'pa.busy': 'Vérification…', 'pa.err': 'Vérification des fuites impossible (pas de connexion ?)',
  'info.gen': 'Utiliser un mot de passe solide pour ce compte ?', 'b.usePw': "L'utiliser",
  'hl.use': 'Windows Hello', 'hl.busy': 'En attente de Windows Hello…',
  'toast.hello-on': 'Windows Hello peut maintenant déverrouiller vos mots de passe', 'toast.hello-unlock': "Déverrouillez d'abord vos mots de passe, puis activez Windows Hello",
  'toast.hello-novault': "Créez d'abord votre coffre de mots de passe", 'toast.hello-unsupported': "Windows Hello n'est pas configuré sur ce PC",
  'toast.hello-canceled': 'Windows Hello a été annulé', 'toast.hello-failed': "Windows Hello n'a pas fonctionné", 'toast.pw-breached': 'Ce mot de passe apparaît dans {n} fuites connues. Changez-le.',
  'toast.pw-copied': 'Mot de passe copié',
});

const PV_I = {
  lock: '<rect x="3.5" y="7" width="9" height="6.5" rx="1.5"/><path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2"/>',
  open: '<rect x="3.5" y="7" width="9" height="6.5" rx="1.5"/><path d="M5.5 7V5a2.5 2.5 0 0 1 4.9-.7"/>',
  shield: '<path d="M8 2 3 4v4c0 3 2.2 5.2 5 6 2.8-.8 5-3 5-6V4z"/>',
  globe: '<circle cx="8" cy="8" r="5.5"/><path d="M2.5 8h11M8 2.5c1.6 1.6 2.3 3.4 2.3 5.5S9.6 11.9 8 13.5C6.4 11.9 5.7 10.1 5.7 8S6.4 4.1 8 2.5z"/>',
  cookie: '<path d="M13.5 8.5A5.5 5.5 0 1 1 7.5 2.5a2 2 0 0 0 2.5 2.5 2 2 0 0 0 3.5 3.5z"/><path d="M6 7h.01M9 10h.01M5.5 10.5h.01"/>',
  perm: '<path d="M8 2.5v1M8 12.5v1M3.5 8h-1M13.5 8h-1"/><circle cx="8" cy="8" r="3"/>',
  hello: '<circle cx="8" cy="8" r="5.5"/><path d="M6 6.5h.01M10 6.5h.01M5.8 9.6c1.2 1.2 3.2 1.2 4.4 0"/>',
};
const pvIcon = (k) => svg(PV_I[k]);
// domaine avec le port (même clé que Plume côté Rust pour « toujours ouvrir dans »)
const hostP = (u) => { try { return new URL(u).host.replace(/^www\./, '').toLowerCase(); } catch { return ''; } };

/* ---------- #19 Conteneurs ---------- */
const contAll = () => C().contList || [];
const contList = () => (on('containers') ? contAll() : []);
const contOf = (id) => contList().find((c) => c.id === id);
const contCol = (c) => GC[(c && c.color) || 'blue'] || GC.blue;
FX.tab.push((x) => { const c = x.c && contOf(x.c); return c ? `<i class="cbar" style="--cc:${contCol(c)}" aria-hidden="true"></i><span class="sr">${esc(t('ct.in', { c: c.name }))}</span>` : ''; });
FX.ctx.push((x) => {
  if (!on('containers') || x.p) return '';
  const l = contList(), cur = contOf(x.c), web = x.u && /^https?:/.test(x.u);
  let h = '<div class="hr"></div>' + l.filter((c) => c.id !== x.c).slice(0, 8).map((c) => `<button class="row" role="menuitem" data-m="cont-open" data-cn="${c.id}"><span class="ic" style="background:${contCol(c)}"></span><span class="tt">${esc(t(x.u ? 'ct.reopen' : 'ct.use', { c: c.name }))}</span></button>`).join('');
  if (cur) h += row('cont-out', t('ct.out')) + (web ? row('cont-site', t('ct.always', { h: esc(hostP(x.u)), c: esc(cur.name) }), (C().contSites || {})[hostP(x.u)] === x.c ? true : '') : '');
  return h + row('cont-make', t('ct.newFrom'));
});
FX.act['cont-open'] = (d, id) => send('cont-open', { id, cn: +d.cn });
FX.act['cont-out'] = (d, id) => send('cont-open', { id, cn: 0 });
FX.act['cont-site'] = (d, id) => send('cont-site', { id });
FX.act['cont-make'] = (d, id) => openContForm({ id });
FX.menu.push(() => (contList().length ? row('cont-pick', t('ct.newTabMenu')) : ''));
FX.act['cont-pick'] = () => openPop(() => `<h3 class="ph3">${t('ct.pick')}</h3>` + contList().map((c) => `<button class="row" data-f="ct-tab" data-cn="${c.id}"><span class="ic" style="background:${contCol(c)}"></span><span class="tt">${esc(c.name)}</span></button>`).join(''));
FX.click['ct-tab'] = (el) => { setMode(null); send('cont-new', { cn: +el.dataset.cn }); };

const CTF = { c: null, id: 0, del: false };
function openContForm(o) {
  CTF.c = Object.assign({ name: '', color: Object.keys(GC)[contAll().length % 8] }, o.c || {});
  CTF.id = o.id || 0; CTF.del = false;
  openModal(contForm);
  setTimeout(() => $('ctn')?.focus(), 40);
}
const contForm = () => `<h3>${t(CTF.c.id ? 'ct.edit' : 'ct.new')}</h3><div class="form" style="max-width:none"><label class="sr" for="ctn">${t('ct.name')}</label><input class="inp" id="ctn" maxlength="24" placeholder="${esc(t('ct.name'))}" value="${esc(CTF.c.name)}">`
  + `<div class="sw2" role="radiogroup" aria-label="${esc(t('sp.color'))}" style="margin:12px 2px">${Object.keys(GC).map((k) => `<button style="--c:${GC[k]}" role="radio" aria-label="${k}" aria-checked="${CTF.c.color === k}" data-f="ct-col" data-v="${k}"></button>`).join('')}</div></div>`
  + (CTF.c.id ? `<p class="small2" style="margin:0 2px 8px">${t('ct.delNote')}</p>` : '')
  + `<div class="acts" style="margin-top:12px">${CTF.c.id ? `<button class="pill red" data-f="ct-del">${t(CTF.del ? 's.resetConfirm' : 'ct.del')}</button><span style="flex:1"></span>` : ''}<button class="pill" data-m="close-ov">${t('b.cancel')}</button><button class="pill pri" data-f="ct-save">${t(CTF.c.id ? 'b.save' : 'b.make')}</button></div>`;
FX.click['ct-col'] = (el) => { CTF.c.name = $('ctn').value; CTF.c.color = el.dataset.v; renderPanel(); };
FX.click['ct-save'] = () => {
  const name = $('ctn').value.trim();
  if (!name) return $('ctn').focus();
  if (CTF.c.id) send('cont-set', { cn: CTF.c.id, name, color: CTF.c.color });
  else send('cont-add', Object.assign({ name, color: CTF.c.color }, CTF.id ? { id: CTF.id, open: true } : {}));
  setMode(null);
};
FX.click['ct-del'] = () => {
  if (!CTF.del) { CTF.del = true; CTF.c.name = $('ctn').value; return renderPanel(); }
  send('cont-del', { cn: CTF.c.id }); setMode(null);
};
document.addEventListener('keydown', (e) => { if (e.key === 'Enter' && e.target.id === 'ctn') { e.preventDefault(); FX.click['ct-save'](); } });

// réglages : liste des conteneurs avec leurs sites associés
SPECIAL.contList = () => {
  const l = contAll(), m = C().contSites || {};
  const rows = l.map((c) => {
    const sites = Object.keys(m).filter((h) => m[h] === c.id);
    return `<div class="crow"><span class="cdot" style="background:${contCol(c)}"></span><b>${esc(c.name)}</b><span class="chips">${sites.map((h) => `<span class="chip">${esc(h)}<button data-f="ct-site-rm" data-h="${esc(h)}" aria-label="${esc(t('b.remove'))} ${esc(h)}">${I.x}</button></span>`).join('')}</span>`
      + `<button class="mini" data-f="ct-tab" data-cn="${c.id}">${t('ct.open')}</button><button class="mini" data-f="ct-edit" data-cn="${c.id}">${t('ct.editB')}</button></div>`;
  }).join('');
  const sugg = l.length ? '' : [['ct.s1', 'blue'], ['ct.s2', 'orange'], ['ct.s3', 'green'], ['ct.s4', 'teal']].map(([k, col]) => `<button class="mini" data-f="ct-quick" data-n="${esc(t(k))}" data-v="${col}"><span class="cdot" style="background:${GC[col]}"></span>${t(k)}</button>`).join('');
  return (rows || `<p class="small2" style="margin:0">${t('ct.none')}</p>`)
    + `<div class="cadd"><button class="mini" data-f="ct-add">+ ${t('ct.new')}</button>${sugg}</div><p class="small2" style="margin:2px 0 0">${t('ct.sitesHelp')}</p>`;
};
FX.click['ct-add'] = () => openContForm({});
FX.click['ct-quick'] = (el) => send('cont-add', { name: el.dataset.n, color: el.dataset.v });
FX.click['ct-edit'] = (el) => openContForm({ c: Object.assign({}, contAll().find((c) => c.id === +el.dataset.cn)) });
FX.click['ct-site-rm'] = (el) => send('cont-site-del', { h: el.dataset.h });

/* ---------- #21 Rapport de confidentialité (clic sur le bouclier) ---------- */
const PR_CO = {
  'google-analytics.com': 'Google', 'googletagmanager.com': 'Google', 'doubleclick.net': 'Google', 'googlesyndication.com': 'Google', 'googleadservices.com': 'Google',
  'facebook.net': 'Meta', 'facebook.com': 'Meta', 'instagram.com': 'Meta', 'scorecardresearch.com': 'Comscore', 'hotjar.com': 'Hotjar', 'criteo.com': 'Criteo', 'criteo.net': 'Criteo',
  'taboola.com': 'Taboola', 'outbrain.com': 'Outbrain', 'amazon-adsystem.com': 'Amazon', 'adnxs.com': 'Microsoft', 'clarity.ms': 'Microsoft', 'bing.com': 'Microsoft',
  'tiktok.com': 'TikTok', 'ads-twitter.com': 'X', 'twitter.com': 'X', 'linkedin.com': 'LinkedIn', 'licdn.com': 'LinkedIn', 'quantserve.com': 'Quantcast', 'pubmatic.com': 'PubMatic',
  'rubiconproject.com': 'Magnite', 'mixpanel.com': 'Mixpanel', 'segment.io': 'Segment', 'segment.com': 'Segment', 'yandex.ru': 'Yandex', 'adsrvr.org': 'The Trade Desk', 'casalemedia.com': 'Index Exchange',
};
const prCo = (h) => { const p = h.split('.'); for (let i = 0; i < p.length - 1; i++) { const k = p.slice(i).join('.'); if (PR_CO[k]) return PR_CO[k]; } return ''; };
const PR = { r: null, t: 0 };
window.openPriv = () => {
  PR.r = null;
  clearTimeout(PR.t);
  send('priv-report');
  // page sans réponse (scripts coupés, page interne…) : on affiche ce qu'on sait déjà
  PR.t = setTimeout(() => { if (!PR.r) { PR.r = { id: act().id, partial: true, blocked: S.blocked || 0, bl: [], third: [], perms: [], cookies: null }; prRefresh(); } }, 900);
  openPop(privPop);
};
window.privRep = (r) => { if (r.id !== act().id) return; PR.r = r; prRefresh(); };
const prRefresh = () => { if (mode === 'pop' && POP.html === privPop) renderPanel(); };
const prList = (items, max) => {
  if (!items.length) return '';
  const more = items.length > max ? `<li class="more">${t('pr.more', { n: items.length - max })}</li>` : '';
  return `<ul class="prl">${items.slice(0, max).join('')}${more}</ul>`;
};
const prLine = (ic, label, val, cls, extra) => `<div class="prline${cls ? ' ' + cls : ''}"><span class="pri">${pvIcon(ic)}</span><span class="tt">${label}</span><b>${val}</b></div>${extra || ''}`;
function privPop() {
  const x = act(), u = x.u || '', r = PR.r, ab = !!(C().ab && S.abSite), det = C().prDetail === 'detailed';
  const sec = /^https:/.test(u);
  let h = `<h3 class="ph3">${t('pr.title')}</h3><p class="lead" style="margin:0 4px 6px">${esc(host(u))}</p>`
    + `<div class="opt"><div class="lbl"><b>${t('pr.block')}</b><span>${t(C().ab ? 'pr.blockSub' : 'pr.abOff')}</span></div>${sw2(ab, `data-f="pr-ab"${C().ab ? '' : ' disabled'}`, t('pr.block'))}</div>`
    + prLine(sec ? 'lock' : 'open', t(sec ? 'pr.https' : 'pr.http'), '', sec ? 'ok' : 'bad');
  if (!r) return h + `<p class="small2">${t('pr.wait')}</p>`;
  const bl = (r.bl || []).map(([d, n]) => `<li><span>${esc(d)}</span>${prCo(d) ? `<small>${esc(prCo(d))}</small>` : ''}${n > 1 ? `<em>×${n}</em>` : ''}</li>`);
  const blockedHosts = new Set((r.bl || []).map((b) => b[0]));
  const third = (r.third || []).filter((d) => !blockedHosts.has(d)).map((d) => `<li><span>${esc(d)}</span>${prCo(d) ? `<small>${esc(prCo(d))}</small>` : ''}</li>`);
  const perms = (r.perms || []).map((p) => t('pr.perm.' + p));
  h += prLine('shield', t('pr.blocked'), fmtNum(r.blocked || 0), '', det ? prList(bl, 12) : '')
    + (r.partial ? '' : prLine('globe', t('pr.third'), fmtNum(third.length), '', det ? prList(third, 12) : ''))
    + (r.cookies == null ? '' : prLine('cookie', t('pr.cookies'), fmtNum(r.cookies)))
    + (r.partial ? '' : prLine('perm', t('pr.perms'), perms.length ? esc(perms.join(', ')) : t('pr.none'), perms.length ? 'warn' : ''));
  return h + (det ? '' : `<p class="small2">${t('pr.detail')}</p>`);
}
FX.click['pr-ab'] = () => { send('ab-site'); setTimeout(prRefresh, 150); };

/* ---------- #22 Générateur et audit, #23 fuites (Paramètres › Mots de passe) ---------- */
const PWG = { pw: '' }, PWA = { r: null, b: null, busy: false, err: false, n: -1 };
window.pwGen = (p) => { PWG.pw = p; const el = $('pwgv'); if (el) { el.textContent = p; const m = $('pwgm'); if (m) m.style.width = (strength(p) + 1) * 20 + '%'; } else if (mode === 'set' && SET.sec === 'pw') renderSettings(true); };
window.pwAuditRes = (r) => { PWA.r = r; if (mode === 'set' && SET.sec === 'pw') renderSettings(true); };
window.pwBreach = (l) => { PWA.busy = false; PWA.err = l.length > 0 && l.every((x) => x.err); PWA.b = {}; l.forEach((x) => { if (!x.err) PWA.b[x.i] = x.n; }); if (mode === 'set' && SET.sec === 'pw') renderSettings(true); };
window.pwExtra = () => {
  const v = S.vault || {};
  let h = '';
  if (on('pwgen')) {
    if (!PWG.pw) { PWG.pw = ' '; send('pw-gen'); }
    const st = strength(PWG.pw.trim());
    h += box(`<div class="pwgen"><div class="ghd">${t('pg2.gen')}</div><div class="pwgrow"><code id="pwgv">${esc(PWG.pw)}</code><button class="icb" data-f="pwg-copy" aria-label="${esc(t('pg2.copy'))}" title="${esc(t('pg2.copy'))}">${I.copy}</button><button class="icb" data-f="pwg-new" aria-label="${esc(t('pg2.again'))}" title="${esc(t('pg2.again'))}">${I.reload}</button></div>`
      + `<div class="meter" aria-hidden="true"><i id="pwgm" style="width:${(st + 1) * 20}%;background:${st >= 3 ? 'var(--green)' : 'var(--orange)'}"></i></div><p class="small2" style="margin:6px 0 0">${t('pg2.opts')}</p></div>`);
  }
  if (on('pwgen') && on('pwAudit') && v.unlocked && v.n) {
    if (PWA.n !== v.n) { PWA.n = v.n; PWA.r = null; PWA.b = null; send('pw-audit'); }
    const r = PWA.r;
    let body = `<p class="small2">${t('pa.busy')}</p>`;
    if (r) {
      const tag = {};
      const add = (i, k) => { (tag[i] = tag[i] || { x: null, k: [] }).k.push(k); };
      r.weak.forEach((x) => { add(x.i, t('pa.weak')); tag[x.i].x = x; });
      r.reused.forEach((g) => g.forEach((x) => { add(x.i, t('pa.reused')); tag[x.i].x = x; }));
      const leaks = PWA.b && on('hibpAudit') ? Object.keys(PWA.b).filter((i) => PWA.b[i] > 0) : [];
      leaks.forEach((i) => { const it = (SET.P || []).find((p) => p.i === +i) || { i: +i, o: '?', u: '' }; add(i, t('pa.leaked', { n: fmtNum(PWA.b[i]) })); tag[i].x = tag[i].x || it; });
      const nReused = r.reused.reduce((a, g) => a + g.length, 0);
      const sum = PWA.b && on('hibpAudit') ? t('pa.sum', { w: r.weak.length, r: nReused, l: leaks.length }) : t('pa.sumNoLeak', { w: r.weak.length, r: nReused });
      const list = Object.values(tag).filter((x) => x.x).map((x) => `<div class="row pwr" style="height:auto;padding:7px 10px">${icon('https://' + x.x.o, '')}<div class="meta"><b>${esc(x.x.o)}</b><span>${esc(x.x.u || '—')}</span></div><span class="patag">${esc(x.k.join(' · '))}</span></div>`).join('');
      body = `<p class="pasum">${Object.keys(tag).length ? esc(sum) : t('pa.ok')}</p>${list}`;
    }
    const leakBtn = on('hibp') ? `<button class="mini" data-f="pa-leaks"${PWA.busy ? ' disabled' : ''}>${t(PWA.busy ? 'pa.busy' : 'pa.leaks')}</button>` : '';
    h += box(`<div class="pwaudit"><div class="pah"><div class="ghd" style="flex:1">${t('pa.title')}</div><button class="mini" data-f="pa-check">${t('pa.check')}</button>${leakBtn}</div>${on('hibp') ? `<p class="small2" style="margin:0 0 6px">${t('pa.leaksSub')}</p>` : ''}${PWA.err ? `<p class="small2" style="color:var(--red)">${t('pa.err')}</p>` : ''}${body}</div>`);
  }
  return h;
};
FX.click['pwg-copy'] = () => { const p = PWG.pw.trim(); if (p) navigator.clipboard.writeText(p).then(() => toast('pw-copied')).catch(() => {}); };
FX.click['pwg-new'] = () => send('pw-gen');
FX.click['pa-check'] = () => { PWA.r = null; send('pw-audit'); renderSettings(true); };
FX.click['pa-leaks'] = () => { PWA.busy = true; PWA.err = false; send('pw-hibp'); renderSettings(true); };

/* ---------- #24 Windows Hello : déverrouillage ---------- */
FX.set['hello-unlock'] = () => send('hello-unlock');
