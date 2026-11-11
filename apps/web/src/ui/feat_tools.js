/* ===== 0.7 · Outils : PiP auto (#32), contrôle média (#33), focus + Pomodoro (#34), pipette et règle (#35),
   QR code (#36), export PDF / Markdown (#37), lire plus tard (#38) ===== */
Object.assign(TXT.en, {
  'md2.title': 'Playing', 'md2.play': 'Play / pause', 'md2.next': 'Next', 'md2.prev': 'Previous', 'md2.mute': 'Mute', 'md2.unmute': 'Unmute', 'md2.go': 'Go to tab',
  'md2.btn': 'Media control', 'md2.muted': 'muted', 'md2.paused': 'paused',
  'fo.focus': 'Focus', 'fo.break': 'Break', 'fo.skip': 'Skip', 'fo.stop': 'Stop', 'fo.start': 'Start a focus session', 'fo.left': '{m} left',
  'toast.fo-break': 'Break time. Blocked sites are open for {n} min.', 'toast.fo-focus': 'Back to focus for {n} min.',
  'dq.color': 'Pick a color', 'dq.ruler': 'Measure (ruler)', 'toast.dq-copied': 'Copied {c}', 'toast.dq-shown': 'Color: {c}', 'toast.dq-none': 'The color picker works on web pages',
  'qr.title': 'QR code of this page', 'qr.copy': 'Copy image', 'qr.long': 'This address is too long for a QR code.', 'toast.qr-copied': 'QR code copied', 'qr.btn': 'QR code',
  'ex.pdf': 'Save as PDF…', 'ex.md': 'Save as Markdown…', 'toast.exp-busy': 'Saving…', 'toast.exp-ok': 'Saved: {p}', 'toast.exp-err': 'Could not save the file',
  'rl.save': 'Save for later', 'rl.title': 'Reading list', 'rl.lead': 'Articles saved with their text and pictures, readable without a connection.', 'rl.empty': 'Nothing saved yet. Page menu › Save for later.',
  'rl.unread': 'To read', 'rl.read': 'Read', 'rl.markRead': 'Mark as read', 'rl.markUnread': 'Mark as unread', 'rl.del': 'Delete', 'rl.open': 'Open', 'rl.words': '{n} min read',
  'toast.rl-saving': 'Saving for later…', 'toast.rl-saved': 'Saved to your reading list', 'toast.rl-already': 'Already in your reading list', 'toast.rl-fail': 'This page could not be saved',
  'a.focus': 'Start a focus session', 'a.focusStop': 'Stop the focus session', 'a.dqColor': 'Pick a color', 'a.dqRuler': 'Ruler', 'a.qr': 'QR code of this page',
  'a.pdf': 'Save as PDF', 'a.md': 'Save as Markdown', 'a.rlSave': 'Save for later', 'a.rl': 'Reading list',
});
Object.assign(TXT.fr, {
  'md2.title': 'En lecture', 'md2.play': 'Lecture / pause', 'md2.next': 'Suivant', 'md2.prev': 'Précédent', 'md2.mute': 'Couper le son', 'md2.unmute': 'Rétablir le son', 'md2.go': "Aller à l'onglet",
  'md2.btn': 'Contrôle média', 'md2.muted': 'son coupé', 'md2.paused': 'en pause',
  'fo.focus': 'Focus', 'fo.break': 'Pause', 'fo.skip': 'Passer', 'fo.stop': 'Arrêter', 'fo.start': 'Démarrer une session de focus', 'fo.left': 'encore {m}',
  'toast.fo-break': 'Pause ! Les sites bloqués sont ouverts pendant {n} min.', 'toast.fo-focus': 'On se reconcentre pour {n} min.',
  'dq.color': 'Pipette de couleur', 'dq.ruler': 'Mesurer (règle)', 'toast.dq-copied': '{c} copié', 'toast.dq-shown': 'Couleur : {c}', 'toast.dq-none': 'La pipette fonctionne sur les pages web',
  'qr.title': 'QR code de cette page', 'qr.copy': "Copier l'image", 'qr.long': 'Cette adresse est trop longue pour un QR code.', 'toast.qr-copied': 'QR code copié', 'qr.btn': 'QR code',
  'ex.pdf': 'Enregistrer en PDF…', 'ex.md': 'Enregistrer en Markdown…', 'toast.exp-busy': 'Enregistrement…', 'toast.exp-ok': 'Enregistré : {p}', 'toast.exp-err': "Impossible d'enregistrer le fichier",
  'rl.save': 'Lire plus tard', 'rl.title': 'Liste de lecture', 'rl.lead': 'Articles enregistrés avec leur texte et leurs images, lisibles sans connexion.', 'rl.empty': 'Rien pour le moment. Menu de la page › Lire plus tard.',
  'rl.unread': 'À lire', 'rl.read': 'Lus', 'rl.markRead': 'Marquer comme lu', 'rl.markUnread': 'Marquer comme non lu', 'rl.del': 'Supprimer', 'rl.open': 'Ouvrir', 'rl.words': '{n} min de lecture',
  'toast.rl-saving': 'Enregistrement pour plus tard…', 'toast.rl-saved': 'Ajouté à votre liste de lecture', 'toast.rl-already': 'Déjà dans votre liste de lecture', 'toast.rl-fail': "Cette page n'a pas pu être enregistrée",
  'a.focus': 'Démarrer une session de focus', 'a.focusStop': 'Arrêter la session de focus', 'a.dqColor': 'Pipette de couleur', 'a.dqRuler': 'Règle', 'a.qr': 'QR code de cette page',
  'a.pdf': 'Enregistrer en PDF', 'a.md': 'Enregistrer en Markdown', 'a.rlSave': 'Lire plus tard', 'a.rl': 'Liste de lecture',
});
const TL_I = {
  spk: '<path d="M2.5 6h2.5L8.5 3v10L5 10H2.5z"/><path d="M11 5.5a3.5 3.5 0 0 1 0 5M12.8 3.8a6 6 0 0 1 0 8.4"/>',
  mute: '<path d="M2.5 6h2.5L8.5 3v10L5 10H2.5z"/><path d="m11 6 3.5 4M14.5 6 11 10"/>',
  play: '<path d="M5 3.5v9l7-4.5z"/>', pause: '<path d="M5 3.5v9M11 3.5v9"/>', next: '<path d="M4 3.5v9l6-4.5zM12 3.5v9"/>', prev: '<path d="M12 3.5v9l-6-4.5zM4 3.5v9"/>',
  note: '<path d="M6 12V3.5l7-1.5V10"/><circle cx="4.5" cy="12" r="1.5"/><circle cx="11.5" cy="10.5" r="1.5"/>',
  timer: '<circle cx="8" cy="9" r="5"/><path d="M8 9V6.5M6.5 2h3"/>', coffee: '<path d="M3 6h8v4a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3zM11 7h1a1.5 1.5 0 0 1 0 3h-1M5.5 2.5v1.5M8 2v2"/>',
  drop: '<path d="m10 2.5 3.5 3.5-1.5 1.5L8.5 4zM9.2 4.8 3.5 10.5 3 13l2.5-.5 5.7-5.7"/>', ruler: '<rect x="1.5" y="5.5" width="13" height="5" rx="1"/><path d="M4.5 5.5v2M7 5.5v3M9.5 5.5v2M12 5.5v3"/>',
  qr: '<path d="M2.5 2.5h4v4h-4zM9.5 2.5h4v4h-4zM2.5 9.5h4v4h-4zM9.5 9.5h1.5M13.5 9.5v1.5M9.5 12h1.5v1.5M12.5 12.5h1"/>',
};
const tlIcon = (k) => svg(TL_I[k]);
// élément de la barre d'outils créé / retiré selon l'état (avant le bouton ⋯, ou dans la barre d'adresse)
function barEl(id, want, make, where) {
  let b = $(id);
  if (!want) { if (b) b.remove(); return null; }
  if (!b) { b = make(); b.id = id; (where || $('menuBtn')).before(b); }
  return b;
}

/* ---------- #33 Contrôle média ---------- */
const audioTabs = () => (S.tabs || []).filter((x) => x.au);
FX.tab.push((x) => {
  if (!on('mediaCtl') || !on('muteIcon') || !(x.au === 1 || x.mu)) return '';
  const lbl = t(x.mu ? 'md2.unmute' : 'md2.mute');
  return `<button class="tspk${x.mu ? ' mu' : ''}" data-f="tab-mute" data-id="${x.id}" aria-label="${esc(lbl)}" title="${esc(lbl)}">${tlIcon(x.mu ? 'mute' : 'spk')}</button>`;
});
FX.click['tab-mute'] = (el, e) => { e.stopPropagation(); send('mute', { id: +el.dataset.id }); };
FX.bar.push(() => {
  const b = barEl('mctlBtn', on('mediaCtl') && on('mediaBtn') && audioTabs().length > 0, () => {
    const n = document.createElement('button'); n.className = 'nb'; n.dataset.f = 'media-pop'; n.innerHTML = tlIcon('note');
    n.setAttribute('aria-label', t('md2.btn')); n.title = t('md2.btn'); return n;
  });
  if (b) b.classList.toggle('on', audioTabs().some((x) => x.au === 1));
});
const mediaPop = () => `<h3 class="ph3">${t('md2.title')}</h3>` + audioTabs().map((x) => {
  const st = x.mu ? ` · ${t('md2.muted')}` : x.au === 2 ? ` · ${t('md2.paused')}` : '';
  return `<div class="mrow"><div class="mt"><b>${esc(x.t || host(x.u))}</b><span>${esc(host(x.u))}${st}</span></div><div class="mb">`
    + `<button class="icb" data-f="media-act" data-id="${x.id}" data-a="prev" aria-label="${esc(t('md2.prev'))}" title="${esc(t('md2.prev'))}">${tlIcon('prev')}</button>`
    + `<button class="icb" data-f="media-act" data-id="${x.id}" data-a="toggle" aria-label="${esc(t('md2.play'))}" title="${esc(t('md2.play'))}">${tlIcon(x.au === 1 ? 'pause' : 'play')}</button>`
    + `<button class="icb" data-f="media-act" data-id="${x.id}" data-a="next" aria-label="${esc(t('md2.next'))}" title="${esc(t('md2.next'))}">${tlIcon('next')}</button>`
    + `<button class="icb" data-f="tab-mute" data-id="${x.id}" aria-label="${esc(t(x.mu ? 'md2.unmute' : 'md2.mute'))}">${tlIcon(x.mu ? 'mute' : 'spk')}</button>`
    + `<button class="mini" data-f="media-go" data-id="${x.id}">${t('md2.go')}</button></div></div>`;
}).join('');
FX.click['media-pop'] = () => (mode === 'pop' && POP.html === mediaPop ? setMode(null) : openPop(mediaPop));
FX.click['media-act'] = (el) => send('media-act', { id: +el.dataset.id, a: el.dataset.a });
FX.click['media-go'] = (el) => { setMode(null); send('act', { id: +el.dataset.id }); };

/* ---------- #34 Mode focus + Pomodoro ---------- */
const FO = { t: 0 };
const foLeft = () => Math.max(0, Math.round(((S.focus && S.focus.end) || 0) - Date.now()) / 1000);
const foClock = () => { const s = Math.round(foLeft()); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`; };
FX.bar.push(() => {
  const f = S.focus, b = barEl('focusBtn', !!f && on('focus'), () => {
    const n = document.createElement('button'); n.className = 'nb fopill'; n.dataset.f = 'focus-pop'; return n;
  });
  if (!b) { clearInterval(FO.t); FO.t = 0; return; }
  b.classList.toggle('brk', f.phase === 'break');
  const lbl = t(f.phase === 'break' ? 'fo.break' : 'fo.focus');
  b.innerHTML = `${tlIcon(f.phase === 'break' ? 'coffee' : 'timer')}<span>${foClock()}</span>`;
  b.setAttribute('aria-label', `${lbl} · ${foClock()}`);
  if (!FO.t) FO.t = setInterval(() => { const el = $('focusBtn'); if (!el || !S.focus) { clearInterval(FO.t); FO.t = 0; return; } el.querySelector('span').textContent = foClock(); if (mode === 'pop' && POP.html === focusPop) { const c = document.querySelector('.fobig'); if (c) c.textContent = foClock(); } }, 1000);
});
const focusPop = () => {
  const f = S.focus || {};
  return `<h3 class="ph3">${t(f.phase === 'break' ? 'fo.break' : 'fo.focus')}</h3><p class="fobig">${foClock()}</p>`
    + `<div class="acts" style="justify-content:flex-start"><button class="pill" data-f="focus-skip">${t('fo.skip')}</button><button class="pill red" data-f="focus-stop">${t('fo.stop')}</button></div>`;
};
FX.click['focus-pop'] = () => (mode === 'pop' && POP.html === focusPop ? setMode(null) : openPop(focusPop));
FX.click['focus-skip'] = () => { send('focus-skip'); setMode(null); };
FX.click['focus-stop'] = () => { send('focus-stop'); setMode(null); };
FX.set['focus-start'] = () => send('focus-start');
// fin d'une phase : message et petit son (Web Audio, rien à télécharger)
window.focusEnd = (next) => {
  toast(next === 'break' ? 'fo-break' : 'fo-focus', { n: C()[next === 'break' ? 'breakMin' : 'focusMin'] });
  if (!on('focusSound')) return;
  try {
    const ac = new AudioContext(), g = ac.createGain();
    g.connect(ac.destination);
    [[660, 0], [880, 0.22]].forEach(([f, at]) => {
      const o = ac.createOscillator(); o.frequency.value = f; o.type = 'sine'; o.connect(g);
      g.gain.setValueAtTime(0.0001, ac.currentTime + at); g.gain.exponentialRampToValueAtTime(0.2, ac.currentTime + at + 0.02); g.gain.exponentialRampToValueAtTime(0.0001, ac.currentTime + at + 0.2);
      o.start(ac.currentTime + at); o.stop(ac.currentTime + at + 0.21);
    });
    setTimeout(() => ac.close(), 800);
  } catch {}
};

/* ---------- #35 Pipette et règle ---------- */
const dqFmt = (hex) => {
  const n = parseInt(hex.slice(1), 16), r = n >> 16, g = (n >> 8) & 255, b = n & 255, f = C().dqFmt || 'hex';
  if (f === 'rgb') return `rgb(${r}, ${g}, ${b})`;
  if (f === 'hsl') {
    const R = r / 255, G = g / 255, B = b / 255, mx = Math.max(R, G, B), mn = Math.min(R, G, B), l = (mx + mn) / 2, d = mx - mn;
    let h = 0, s = 0;
    if (d) { s = d / (1 - Math.abs(2 * l - 1)); h = mx === R ? ((G - B) / d) % 6 : mx === G ? (B - R) / d + 2 : (R - G) / d + 4; h = Math.round(h * 60); if (h < 0) h += 360; }
    return `hsl(${h}, ${Math.round(s * 100)}%, ${Math.round(l * 100)}%)`;
  }
  return hex.toLowerCase();
};
// couleur choisie dans la page (déjà copiée par la page, qui a eu le clic) ; sinon on essaie d'ici
window.dqColor = (c, copied) => {
  if (c === 'unsupported') return toast('dq-none');
  if (!c) return;
  if (copied) return toast('dq-copied', { c });
  navigator.clipboard.writeText(c).then(() => toast('dq-copied', { c }), () => toast('dq-shown', { c }));
};
const dqOk = () => on('devQuick') && web();
FX.pal.push(['dqColor', '', () => send('dq-color'), dqOk], ['dqRuler', '', () => send('dq-ruler', { lang: LANG }), dqOk]);
// boutons dans les DevTools de Plume
window.dqDevButtons = () => (on('devQuick') ? `<button class="ib" data-dta="dq-color" aria-label="${esc(t('dq.color'))}" title="${esc(t('dq.color'))}">${tlIcon('drop')}</button><button class="ib" data-dta="dq-ruler" aria-label="${esc(t('dq.ruler'))}" title="${esc(t('dq.ruler'))}">${tlIcon('ruler')}</button>` : '');
FX.click['dq-color'] = () => send('dq-color');
FX.click['dq-ruler'] = () => send('dq-ruler', { lang: LANG });
FX.bar.push((x) => {
  const want = on('devQuick') && on('dqButtons') && !!x.u && x.u.startsWith('http');
  const mk = (f, ic, lbl) => () => { const n = document.createElement('button'); n.className = 'nb'; n.dataset.f = f; n.innerHTML = tlIcon(ic); n.setAttribute('aria-label', t(lbl)); n.title = t(lbl); return n; };
  barEl('dqColorBtn', want, mk('dq-color', 'drop', 'dq.color'));
  barEl('dqRulerBtn', want, mk('dq-ruler', 'ruler', 'dq.ruler'));
});

/* ---------- #36 QR code de la page (généré ici, sans service externe) ---------- */
FX.bar.push((x) => {
  const want = on('qr') && !!x.u && /^https?:/.test(x.u) && !x.p;
  barEl('qrBtn', want, () => {
    const n = document.createElement('button'); n.className = 'ob'; n.dataset.f = 'qr-pop'; n.innerHTML = tlIcon('qr');
    n.setAttribute('aria-label', t('qr.btn')); n.title = t('qr.btn'); return n;
  }, $('star'));
  document.body.classList.toggle('qrhover', C().qrShow !== 'always');
});
const qrPop = () => {
  const u = act().u || '', q = qrEncode(u);
  if (!q) return `<h3 class="ph3">${t('qr.title')}</h3><p class="small2">${t('qr.long')}</p>`;
  return `<h3 class="ph3">${t('qr.title')}</h3><div class="qrbox">${qrSvg(q, 240)}</div><p class="qru">${esc(u)}</p>`
    + `<div class="acts"><button class="pill pri" data-f="qr-copy">${t('qr.copy')}</button></div>`;
};
FX.click['qr-pop'] = () => (mode === 'pop' && POP.html === qrPop ? setMode(null) : openPop(qrPop));
FX.click['qr-copy'] = () => {
  const q = qrEncode(act().u || '');
  if (!q) return;
  const px = Math.max(128, Math.min(512, +C().qrSize || 256)), n = q.size + 8, k = Math.max(1, Math.floor(px / n));
  const cv = document.createElement('canvas');
  cv.width = cv.height = n * k;
  const g = cv.getContext('2d');
  g.fillStyle = '#fff'; g.fillRect(0, 0, cv.width, cv.height); g.fillStyle = '#000';
  for (let y = 0; y < q.size; y++) for (let x = 0; x < q.size; x++) if (q.dark(x, y)) g.fillRect((x + 4) * k, (y + 4) * k, k, k);
  cv.toBlob((b) => navigator.clipboard.write([new ClipboardItem({ 'image/png': b })]).then(() => toast('qr-copied'), () => {}), 'image/png');
};
FX.pal.push(['qr', '', () => FX.click['qr-pop'](), () => on('qr') && web()]);

/* ---------- #37 Export PDF et Markdown ---------- */
FX.menu.push((x, w) => {
  if (!on('export') || !w) return '';
  const rows = { pdf: row('exp-pdf', t('ex.pdf')), md: row('exp-md', t('ex.md')) };
  return C().expFmt === 'md' ? rows.md + rows.pdf : rows.pdf + rows.md;
});
FX.act['exp-pdf'] = () => send('export', { fmt: 'pdf' });
FX.act['exp-md'] = () => send('export', { fmt: 'md' });
window.expDone = (r) => toast(r.ok ? 'exp-ok' : 'exp-err', { p: r.path });
FX.pal.push(['pdf', '', () => send('export', { fmt: 'pdf' }), () => on('export') && web()], ['md', '', () => send('export', { fmt: 'md' }), () => on('export') && web()]);

/* ---------- #38 Lire plus tard (plume://reading-list) ---------- */
FX.menu.push((x, w) => (on('readLater') && w ? row('rl-save', t('rl.save')) : ''));
FX.act['rl-save'] = () => send('rl-save');
const RL = { l: null };
window.rlRes = (l) => { RL.l = l; if (mode === 'pg' && PG.name === 'reading-list') renderPage(); };
FX.pages['reading-list'] = {
  title: () => t('rl.title'),
  search: true,
  open: () => send('rl-list'),
  body: (q) => {
    if (!RL.l) return `<p class="lead">${t('rl.lead')}</p>`;
    const match = (x) => !q || ((x.t || '') + ' ' + (x.site || '') + ' ' + (x.ex || '')).toLowerCase().includes(q);
    const item = (x) => `<div class="row rlr" style="height:auto;padding:10px 12px">${icon('https://' + x.site, x.f || '')}<div class="meta"><b>${esc(x.t || x.u)}</b><span>${esc(x.site)}${x.n ? ' · ' + t('rl.words', { n: Math.max(1, Math.round(x.n / 230)) }) : ''}</span>${x.ex ? `<p class="rlex">${esc(x.ex)}</p>` : ''}</div>`
      + `<button class="mini" data-f="rl-open" data-rid="${x.id}">${t('rl.open')}</button><button class="mini" data-f="rl-read" data-rid="${x.id}" data-r="${!x.read}">${t(x.read ? 'rl.markUnread' : 'rl.markRead')}</button>`
      + `<button class="icb red" data-f="rl-del" data-rid="${x.id}" aria-label="${esc(t('rl.del'))}">${I.trash}</button></div>`;
    const un = RL.l.filter((x) => !x.read && match(x)), rd = RL.l.filter((x) => x.read && match(x));
    if (!RL.l.length) return `<p class="lead">${t('rl.lead')}</p><div class="empty">${t('rl.empty')}</div>`;
    return `<p class="lead">${t('rl.lead')}</p>` + (un.length ? `<h3>${t('rl.unread')}</h3>` + box(un.map(item).join('')) : '') + (rd.length ? `<h3>${t('rl.read')}</h3>` + box(rd.map(item).join('')) : '');
  },
};
FX.click['rl-open'] = (el) => { setMode(null); send('rl-open', { rid: +el.dataset.rid }); };
FX.click['rl-read'] = (el) => send('rl-read', { rid: +el.dataset.rid, read: el.dataset.r === 'true' });
FX.click['rl-del'] = (el) => send('rl-del', { rid: +el.dataset.rid });
FX.pal.push(['rlSave', '', () => send('rl-save'), () => on('readLater') && web()], ['rl', '', () => openPage('reading-list'), () => on('readLater')],
  ['focus', '', () => send('focus-start'), () => on('focus') && !S.focus], ['focusStop', '', () => send('focus-stop'), () => !!S.focus]);
