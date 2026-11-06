// Plume DevTools — panneau intégré (F12). Parle à l'onglet via le protocole Chrome DevTools (pont Rust).
const DT = {
  open: false, h: store.get('dtH', 320), tab: store.get('dtTab', 'console'), rid: 0, wait: new Map(),
  logs: [], errs: 0, warns: 0, hist: store.get('dtHist', []), hi: -1,
  nodes: new Map(), root: null, exp: new Set(), selN: null, picking: false, side: null,
  net: new Map(), netOrder: [], netSel: null, netF: '', noCache: false, preserve: false,
  perf: null, perfHist: [], perfTimer: 0, storeData: null,
  call(m, p) { const r = ++this.rid; send('dt', { m, p: p || {}, r }); return new Promise((res) => this.wait.set(r, res)); },
};
window.DT = DT;
DT.res = (r, j) => { const f = DT.wait.get(r); if (f) { DT.wait.delete(r); f(j); } };
window.devOpen = (on) => { DT.open = on; $('dt').classList.toggle('on', on); if (on) { DT.reset(); dtRender(); } else stopPerf(); sendH(); };
window.devReset = () => { DT.reset(); dtRender(); };
DT.reset = () => {
  if (!DT.preserve) { DT.logs = []; DT.errs = DT.warns = 0; DT.net.clear(); DT.netOrder = []; }
  DT.nodes.clear(); DT.root = null; DT.exp = new Set(); DT.selN = null; DT.side = null; DT.storeData = null;
  if (DT.tab === 'elements') loadDoc();
  if (DT.noCache) DT.call('Network.setCacheDisabled', { cacheDisabled: true });
};

/* ---------- Événements CDP ---------- */
DT.ev = (name, p) => {
  switch (name) {
    case 'Runtime.consoleAPICalled': {
      const lvl = p.type === 'error' || p.type === 'assert' ? 'error' : p.type === 'warning' ? 'warning' : p.type === 'info' || p.type === 'debug' ? 'info' : 'log';
      const fr = p.stackTrace && p.stackTrace.callFrames && p.stackTrace.callFrames[0];
      if (p.type === 'clear') { DT.logs = []; DT.errs = DT.warns = 0; break; }
      addLog({ lvl, args: p.args || [], src: fr ? `${fileOf(fr.url)}:${fr.lineNumber + 1}` : '' });
      break;
    }
    case 'Runtime.exceptionThrown': {
      const d = p.exceptionDetails || {};
      addLog({ lvl: 'error', text: (d.exception && d.exception.description) || d.text || 'Error', src: d.url ? `${fileOf(d.url)}:${(d.lineNumber || 0) + 1}` : '' });
      break;
    }
    case 'Log.entryAdded': {
      const e = p.entry || {};
      addLog({ lvl: e.level === 'error' ? 'error' : e.level === 'warning' ? 'warning' : 'info', text: e.text, src: e.url ? fileOf(e.url) : e.source });
      break;
    }
    case 'Runtime.executionContextsCleared': if (!DT.preserve) { DT.logs = []; DT.errs = DT.warns = 0; } break;
    case 'Page.frameNavigated': if (!p.frame.parentId && !DT.preserve) { DT.net.clear(); DT.netOrder = []; } break;
    case 'Network.requestWillBeSent': {
      if (!DT.net.has(p.requestId)) DT.netOrder.push(p.requestId);
      DT.net.set(p.requestId, Object.assign(DT.net.get(p.requestId) || {}, { id: p.requestId, url: p.request.url, method: p.request.method, type: p.type || 'Other', t0: p.timestamp, reqH: p.request.headers }));
      if (DT.netOrder.length > 1500) DT.net.delete(DT.netOrder.shift());
      break;
    }
    case 'Network.responseReceived': { const r = DT.net.get(p.requestId); if (r) Object.assign(r, { status: p.response.status, mime: p.response.mimeType, resH: p.response.headers, type: p.type || r.type, proto: p.response.protocol, ip: p.response.remoteIPAddress }); break; }
    case 'Network.loadingFinished': { const r = DT.net.get(p.requestId); if (r) Object.assign(r, { size: p.encodedDataLength, t1: p.timestamp }); break; }
    case 'Network.loadingFailed': { const r = DT.net.get(p.requestId); if (r) Object.assign(r, { fail: p.blockedReason ? 'blocked' : (p.errorText || 'failed'), t1: p.timestamp }); break; }
    case 'DOM.setChildNodes': { const par = DT.nodes.get(p.parentId); if (par) { par.children = p.nodes; p.nodes.forEach((n) => indexNode(n, p.parentId)); } break; }
    case 'DOM.documentUpdated': DT.nodes.clear(); DT.root = null; if (DT.tab === 'elements') loadDoc(); break;
    case 'Overlay.inspectNodeRequested': revealBackend(p.backendNodeId); break;
  }
  scheduleRender(name);
};
let rTimer = 0;
function scheduleRender(name) {
  // les événements réseau arrivent en rafales : on regroupe les rendus
  if (rTimer) return;
  rTimer = setTimeout(() => { rTimer = 0; if (!DT.open) return; if (name.startsWith('Network') && DT.tab !== 'network') return dtHeadCounts(); dtRender(true); }, name.startsWith('Network') ? 120 : 16);
}
const fileOf = (u) => { try { const x = new URL(u); return x.pathname.split('/').pop() || x.host; } catch { return u || ''; } };
function addLog(l) {
  DT.logs.push(l);
  if (l.lvl === 'error') DT.errs++; if (l.lvl === 'warning') DT.warns++;
  if (DT.logs.length > 1000) DT.logs.shift();
}

/* ---------- Rendu ---------- */
const TABS = ['console', 'elements', 'network', 'storage', 'perf'];
function dtHeadCounts() {
  const c = $('dt').querySelector('.tb[data-dtt=console] .cnt');
  if (c) c.innerHTML = (DT.errs ? `<span class="n">${DT.errs}</span>` : '') + (DT.warns ? `<span class="n w">${DT.warns}</span>` : '');
}
function dtRender(soft) {
  const el = $('dt');
  if (!DT.open) return;
  const pane = el.querySelector('.pane');
  const stick = pane && DT.tab === 'console' && pane.scrollHeight - pane.scrollTop - pane.clientHeight < 30;
  const keepScroll = pane ? pane.scrollTop : 0;
  const input = $('dtin') && $('dtin').querySelector('textarea');
  const draft = input ? input.value : '';
  const hadFocus = input && document.activeElement === input;
  if (!soft || !el.querySelector('.dth')) {
    el.innerHTML = `<div class="grip" aria-hidden="true"></div><div class="dth" role="tablist">`
      + `<button class="ib${DT.picking ? ' on' : ''}" data-dta="pick" aria-label="${esc(t('dt.pick'))}" title="${esc(t('dt.pick'))}">${svg('<path d="M3 3h6M3 3v6M3 3l9.5 9.5M13 9v4H9"/>')}</button>`
      + (window.dqDevButtons ? dqDevButtons() : '')
      + TABS.map((k) => `<button class="tb${DT.tab === k ? ' on' : ''}" role="tab" aria-selected="${DT.tab === k}" data-dtt="${k}">${t('dt.' + k)}${k === 'console' ? '<span class="cnt"></span>' : ''}</button>`).join('')
      + `<span class="sp"></span><button class="ib" data-dta="edge" aria-label="${esc(t('dt.edge'))}" title="${esc(t('dt.edge'))}">${svg('<path d="M9 2.5h4.5V7M13.5 2.5 7.5 8.5M12 9.5v3a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h3"/>')}</button>`
      + `<button class="ib" data-dta="close" aria-label="${esc(t('dt.close'))}" title="${esc(t('dt.close'))}">${I.x.replace('viewBox="0 0 10 10"', 'viewBox="-3 -3 16 16"')}</button></div><div class="dtb"></div>`;
    el.querySelector('.grip').addEventListener('pointerdown', gripDrag);
  }
  el.querySelectorAll('.tb').forEach((b) => { b.classList.toggle('on', b.dataset.dtt === DT.tab); b.setAttribute('aria-selected', b.dataset.dtt === DT.tab); });
  el.querySelector('[data-dta=pick]').classList.toggle('on', DT.picking);
  dtHeadCounts();
  const body = el.querySelector('.dtb');
  body.innerHTML = ({ console: consoleHtml, elements: elementsHtml, network: networkHtml, storage: storageHtml, perf: perfHtml })[DT.tab]();
  const np = el.querySelector('.pane');
  if (np) np.scrollTop = stick ? np.scrollHeight : keepScroll;
  const ni = $('dtin') && $('dtin').querySelector('textarea');
  if (ni) { ni.value = draft; if (hadFocus || !soft) { ni.focus(); ni.setSelectionRange(draft.length, draft.length); } }
}
function gripDrag(e) {
  e.preventDefault();
  const y0 = e.clientY, h0 = DT.h;
  const mv = (ev) => { DT.h = Math.max(120, Math.min(innerHeight - 160, h0 + (y0 - ev.clientY))); sendH(); };
  const up = () => { removeEventListener('pointermove', mv); removeEventListener('pointerup', up); store.set('dtH', DT.h); };
  addEventListener('pointermove', mv); addEventListener('pointerup', up);
}

/* Console */
function fmtObj(o, deep) {
  if (!o) return '<span class="tok-u">undefined</span>';
  switch (o.type) {
    case 'string': return deep ? `<span class="tok-s">"${esc(o.value)}"</span>` : esc(o.value);
    case 'number': case 'bigint': return `<span class="tok-n">${esc(o.description ?? o.value)}</span>`;
    case 'boolean': return `<span class="tok-k">${o.value}</span>`;
    case 'undefined': return '<span class="tok-u">undefined</span>';
    case 'symbol': return `<span class="tok-s">${esc(o.description)}</span>`;
    case 'function': return `<span class="tok-f">${esc((o.description || 'ƒ').split('\n')[0].slice(0, 120))}</span>`;
    case 'object': {
      if (o.subtype === 'null') return '<span class="tok-k">null</span>';
      if (o.subtype === 'error') return esc(o.description);
      const pv = o.preview;
      let s = esc(o.description || o.className || 'Object');
      if (pv && pv.properties) {
        const arr = pv.subtype === 'array';
        const inner = pv.properties.map((p) => (arr ? '' : `<span class="tok-u">${esc(p.name)}</span>: `) + (p.type === 'string' ? `<span class="tok-s">"${esc(p.value)}"</span>` : p.type === 'number' ? `<span class="tok-n">${esc(p.value)}</span>` : esc(p.value ?? p.subtype ?? p.type))).join(', ');
        s = `${arr ? esc(o.description) + ' ' : o.className && o.className !== 'Object' ? esc(o.className) + ' ' : ''}${arr ? '[' : '{'}${inner}${pv.overflow ? ', …' : ''}${arr ? ']' : '}'}`;
      }
      return o.objectId ? `<span class="obj" data-oid="${esc(o.objectId)}">${s}</span>` : s;
    }
  }
  return esc(o.description ?? o.value ?? '');
}
function consoleHtml() {
  const rows = DT.logs.map((l) => `<div class="cn ${l.lvl}">${l.args ? l.args.map((a, i) => fmtObj(a, i > 0 || a.type !== 'string')).join(' ') : esc(l.text)}${l.src ? `<span class="src">${esc(l.src)}</span>` : ''}</div>`).join('');
  return `<div class="ccol"><div class="dtbar"><button class="mini" data-dta="clog">${t('dt.clear')}</button><label><input type="checkbox" data-dta="preserve" ${DT.preserve ? 'checked' : ''}>${t('dt.preserve')}</label></div>`
    + `<div class="pane" role="log" aria-live="off">${rows}</div><div id="dtin"><textarea rows="1" spellcheck="false" placeholder="${esc(t('dt.eval'))}" aria-label="${esc(t('dt.eval'))}"></textarea></div></div>`;
}
async function evalCmd(src) {
  addLog({ lvl: 'cmd', text: src });
  DT.hist = [src, ...DT.hist.filter((x) => x !== src)].slice(0, 50); store.set('dtHist', DT.hist); DT.hi = -1;
  const r = await DT.call('Runtime.evaluate', { expression: src, replMode: true, generatePreview: true, includeCommandLineAPI: true, userGesture: true, awaitPromise: true });
  if (r.error) addLog({ lvl: 'error', text: r.error });
  else if (r.exceptionDetails) addLog({ lvl: 'error', text: (r.exceptionDetails.exception && r.exceptionDetails.exception.description) || r.exceptionDetails.text });
  else DT.logs.push({ lvl: 'ret', args: [r.result] });
  dtRender(true);
}
async function expandObj(el) {
  if (el.nextElementSibling && el.nextElementSibling.classList.contains('kids')) return el.nextElementSibling.remove();
  const r = await DT.call('Runtime.getProperties', { objectId: el.dataset.oid, ownProperties: true, generatePreview: true });
  const kids = document.createElement('div');
  kids.className = 'kids';
  kids.innerHTML = (r.result || []).filter((p) => p.enumerable || p.name === '__proto__' || p.name === 'length').slice(0, 200).map((p) => `<div><span class="tok-u">${esc(p.name)}</span>: ${p.value ? fmtObj(p.value, true) : '<span class="tok-u">(…)</span>'}</div>`).join('');
  el.after(kids);
}

/* Éléments */
function indexNode(n, parentId) {
  n.parentId = parentId;
  DT.nodes.set(n.nodeId, n);
  (n.children || []).forEach((c) => indexNode(c, n.nodeId));
  if (n.contentDocument) indexNode(n.contentDocument, n.nodeId);
}
async function loadDoc() {
  const r = await DT.call('DOM.getDocument', { depth: 3 });
  if (!r.root) return;
  DT.root = r.root; indexNode(r.root, 0);
  const html = (r.root.children || []).find((c) => c.nodeName === 'HTML');
  if (html) { DT.exp.add(html.nodeId); const body = (html.children || []).find((c) => c.nodeName === 'BODY'); if (body) DT.exp.add(body.nodeId); }
  dtRender(true);
}
function nodeHtml(n, depth) {
  if (!n) return '';
  const pad = `padding-left:${8 + depth * 14}px`;
  if (n.nodeType === 3) { const v = n.nodeValue.trim(); return v ? `<div class="el" style="${pad}"><span class="tw"></span><span class="tx">${esc(v.slice(0, 200))}</span></div>` : ''; }
  if (n.nodeType === 8) return `<div class="el" style="${pad}"><span class="tw"></span><span class="cm">&lt;!--${esc(n.nodeValue.slice(0, 120))}--&gt;</span></div>`;
  if (n.nodeType === 9) return (n.children || []).map((c) => nodeHtml(c, depth)).join('');
  if (n.nodeType === 10) return `<div class="el" style="${pad}"><span class="tw"></span><span class="cm">&lt;!DOCTYPE ${esc(n.nodeName.toLowerCase())}&gt;</span></div>`;
  if (n.nodeType !== 1) return '';
  const name = n.localName || n.nodeName.toLowerCase();
  const a = n.attributes || [];
  let attrs = '';
  for (let i = 0; i < a.length; i += 2) attrs += ` <span class="at">${esc(a[i])}</span>${a[i + 1] !== '' ? `=<span class="av">"${esc(a[i + 1].slice(0, 120))}"</span>` : ''}`;
  const has = n.childNodeCount > 0 || (n.children && n.children.length) || n.contentDocument;
  const open = DT.exp.has(n.nodeId);
  const kids = open ? [...(n.children || []), ...(n.contentDocument ? [n.contentDocument] : [])] : [];
  const oneText = kids.length === 1 && kids[0].nodeType === 3 && kids[0].nodeValue.length < 80;
  let s = `<div class="el${DT.selN === n.nodeId ? ' sel' : ''}" style="${pad}" data-nid="${n.nodeId}"><span class="tw" data-tw="${n.nodeId}">${has ? (open ? '▾' : '▸') : ''}</span><span class="tg">&lt;${esc(name)}</span>${attrs}<span class="tg">&gt;</span>`;
  if (oneText) return s + `<span class="tx">${esc(kids[0].nodeValue)}</span><span class="tg">&lt;/${esc(name)}&gt;</span></div>`;
  if (!open) return s + (has ? '…' : '') + `<span class="tg">&lt;/${esc(name)}&gt;</span></div>`;
  s += '</div>' + kids.map((c) => nodeHtml(c, depth + 1)).join('');
  return s + `<div class="el" style="${pad}"><span class="tw"></span><span class="tg">&lt;/${esc(name)}&gt;</span></div>`;
}
function elementsHtml() {
  if (!DT.root) { loadDoc(); return `<div class="pane"><div class="empty">…</div></div>`; }
  let side = `<div class="empty" style="font-family:'Segoe UI'">${t('dt.selectEl')}</div>`;
  if (DT.side) {
    const s = DT.side;
    const bm = s.box ? (() => { const q = (a) => [a[0], a[1], a[2], a[5]]; const [c, p, b, m] = [s.box.content, s.box.padding, s.box.border, s.box.margin].map(q); const w = Math.round(c[2] - c[0]), h = Math.round(c[3] - c[1]); return `<div class="sh">${t('dt.box')}</div><div class="boxm"><div class="m">margin<div class="b">border<div class="p">padding<div class="c">${w} × ${h}</div></div></div></div></div>`; })() : '';
    side = `<div class="sh">${t('dt.attrs')}</div><div class="kv">${s.attrs.map(([k, v]) => `<b>${esc(k)}</b><span>${esc(v)}</span>`).join('') || '<span class="tok-u">—</span>'}</div>${bm}`
      + `<div class="sh">${t('dt.styles')}</div><div class="kv">${s.css.map(([k, v]) => `<b>${esc(k)}</b><span>${esc(v)}</span>`).join('')}</div>`;
  }
  return `<div class="pane" role="tree">${nodeHtml(DT.root, 0)}</div><div class="side2">${side}</div>`;
}
const HL = { showInfo: true, showStyles: true, contentColor: { r: 10, g: 132, b: 255, a: 0.3 }, paddingColor: { r: 48, g: 209, b: 88, a: 0.25 }, borderColor: { r: 255, g: 214, b: 10, a: 0.3 }, marginColor: { r: 255, g: 159, b: 10, a: 0.25 } };
const IMPORTANT = ['display', 'position', 'width', 'height', 'margin', 'padding', 'box-sizing', 'color', 'background-color', 'font-family', 'font-size', 'font-weight', 'line-height', 'text-align', 'border', 'border-radius', 'flex-direction', 'justify-content', 'align-items', 'gap', 'grid-template-columns', 'overflow', 'opacity', 'z-index', 'top', 'left', 'transform', 'box-shadow', 'cursor', 'visibility'];
async function selectNode(id) {
  DT.selN = id;
  const n = DT.nodes.get(id);
  const attrs = [];
  for (let i = 0; i < ((n && n.attributes) || []).length; i += 2) attrs.push([n.attributes[i], n.attributes[i + 1]]);
  const [cs, box] = await Promise.all([DT.call('CSS.getComputedStyleForNode', { nodeId: id }), DT.call('DOM.getBoxModel', { nodeId: id })]);
  const map = new Map((cs.computedStyle || []).map((p) => [p.name, p.value]));
  const sh = (a, b, c, d) => [a, b, c, d].every((x) => x === a) ? a : `${a} ${b} ${c} ${d}`;
  const get = (k) => k === 'margin' || k === 'padding' ? sh(map.get(k + '-top'), map.get(k + '-right'), map.get(k + '-bottom'), map.get(k + '-left')) : k === 'border' ? `${map.get('border-top-width')} ${map.get('border-top-style')} ${map.get('border-top-color')}` : k === 'border-radius' ? map.get('border-top-left-radius') : map.get(k);
  DT.side = { attrs, css: IMPORTANT.map((k) => [k, get(k)]).filter(([, v]) => v && v !== 'none' && v !== 'normal' && v !== 'auto' && v !== '0px' && v !== 'visible' || ['display', 'position', 'width', 'height', 'color', 'font-size'].includes(k)), box: box.model };
  dtRender(true);
}
async function revealBackend(bid) {
  DT.picking = false;
  const r = await DT.call('DOM.pushNodesByBackendIdsToFrontend', { backendNodeIds: [bid] });
  const id = r.nodeIds && r.nodeIds[0];
  if (!id) return;
  let n = DT.nodes.get(id);
  for (let p = n && n.parentId; p; p = DT.nodes.get(p)?.parentId) DT.exp.add(p);
  DT.tab = 'elements'; store.set('dtTab', 'elements');
  await selectNode(id);
  setTimeout(() => $('dt').querySelector('.el.sel')?.scrollIntoView({ block: 'center' }), 20);
}

/* Réseau */
function networkHtml() {
  const f = DT.netF.toLowerCase();
  const reqs = DT.netOrder.map((id) => DT.net.get(id)).filter((r) => r && (!f || r.url.toLowerCase().includes(f)));
  const total = reqs.reduce((a, r) => a + (r.size || 0), 0);
  const first = reqs.length ? Math.min(...reqs.map((r) => r.t0)) : 0, last = reqs.length ? Math.max(...reqs.map((r) => r.t1 || r.t0)) : 0;
  const rows = reqs.map((r) => `<tr class="${r.fail || r.status >= 400 ? 'bad' : ''}${DT.netSel === r.id ? ' sel' : ''}" data-rq="${esc(r.id)}"><td title="${esc(r.url)}">${esc(fileOf(r.url) || r.url)}</td><td>${r.fail ? esc(r.fail) : r.status || '…'}</td><td>${esc(r.type)}</td><td>${r.size != null ? fmtB(r.size) : ''}</td><td>${r.t1 ? Math.round((r.t1 - r.t0) * 1000) + ' ms' : ''}</td></tr>`).join('');
  let side = '';
  const s = DT.netSel && DT.net.get(DT.netSel);
  if (s) {
    const kv = (o) => Object.entries(o || {}).map(([k, v]) => `<b>${esc(k)}</b><span>${esc(v)}</span>`).join('');
    side = `<div class="side2"><div class="sh">${t('dt.general')}</div><div class="kv"><b>URL</b><span>${esc(s.url)}</span><b>Method</b><span>${esc(s.method)}</span><b>Status</b><span>${esc(s.status || s.fail || '')}</span>${s.ip ? `<b>IP</b><span>${esc(s.ip)}</span>` : ''}${s.proto ? `<b>Protocol</b><span>${esc(s.proto)}</span>` : ''}</div>`
      + `<div class="sh">${t('dt.resH')}</div><div class="kv">${kv(s.resH)}</div><div class="sh">${t('dt.reqH')}</div><div class="kv">${kv(s.reqH)}</div></div>`;
  }
  return `<div class="ccol"><div class="dtbar"><input type="text" id="netf" placeholder="${esc(t('dt.filter'))}" aria-label="${esc(t('dt.filter'))}" value="${esc(DT.netF)}"><button class="mini" data-dta="cnet">${t('dt.clear')}</button>`
    + `<label><input type="checkbox" data-dta="nocache" ${DT.noCache ? 'checked' : ''}>${t('dt.nocache')}</label><label><input type="checkbox" data-dta="preserve" ${DT.preserve ? 'checked' : ''}>${t('dt.preserve')}</label></div>`
    + `<div style="display:flex;flex:1;min-height:0"><div class="pane"><table class="nt"><thead><tr><th>${t('dt.name')}</th><th>${t('dt.status')}</th><th>${t('dt.type')}</th><th>${t('dt.size')}</th><th>${t('dt.time')}</th></tr></thead><tbody>${rows}</tbody></table></div>${side}</div>`
    + `<div class="dtfoot"><span>${t('dt.reqs', { n: reqs.length })}</span><span>${t('dt.transferred', { s: fmtB(total) })}</span>${reqs.length ? `<span>${Math.round((last - first) * 1000)} ms</span>` : ''}</div></div>`;
}

/* Stockage */
async function loadStorage() {
  const js = `JSON.stringify({l:Object.entries(localStorage),s:Object.entries(sessionStorage),u:location.href})`;
  const r = await DT.call('Runtime.evaluate', { expression: js, returnByValue: true });
  let d = { l: [], s: [], u: '' };
  try { d = JSON.parse(r.result.value); } catch {}
  const c = await DT.call('Network.getCookies', { urls: d.u ? [d.u] : [] });
  DT.storeData = { l: d.l, s: d.s, c: c.cookies || [] };
  dtRender(true);
}
function storageHtml() {
  if (!DT.storeData) { loadStorage(); return `<div class="pane"><div class="empty">…</div></div>`; }
  const d = DT.storeData;
  const tbl = (kind, rows) => `<table class="nt"><thead><tr><th>${t('dt.key')}</th><th>${t('dt.value')}</th><th></th></tr></thead><tbody>${rows.map(([k, v]) => `<tr><td>${esc(k)}</td><td title="${esc(v)}">${esc(String(v).slice(0, 300))}</td><td style="width:30px"><button class="mini" data-del-st="${kind}" data-k="${esc(k)}" aria-label="${esc(t('b.delete'))}">✕</button></td></tr>`).join('')}</tbody></table>`;
  return `<div class="ccol"><div class="dtbar"><button class="mini" data-dta="rst">${t('dt.refresh')}</button></div><div class="pane">`
    + `<div class="sh">${t('dt.local')} (${d.l.length})</div>${tbl('l', d.l)}<div class="sh">${t('dt.session')} (${d.s.length})</div>${tbl('s', d.s)}`
    + `<div class="sh">${t('dt.cookies')} (${d.c.length})</div>${tbl('c', d.c.map((c) => [c.name, c.value]))}</div></div>`;
}

/* Performances */
async function pollPerf() {
  const r = await DT.call('Performance.getMetrics');
  if (!r.metrics) return;
  DT.perf = Object.fromEntries(r.metrics.map((m) => [m.name, m.value]));
  DT.perfHist.push(DT.perf.JSHeapUsedSize || 0);
  if (DT.perfHist.length > 60) DT.perfHist.shift();
  if (DT.tab === 'perf') dtRender(true);
}
function stopPerf() { clearInterval(DT.perfTimer); DT.perfTimer = 0; }
function perfHtml() {
  if (!DT.perfTimer) { pollPerf(); DT.perfTimer = setInterval(() => { if (DT.open && DT.tab === 'perf') pollPerf(); else stopPerf(); }, 1000); }
  const p = DT.perf;
  if (!p) return `<div class="pane"><div class="empty">…</div></div>`;
  const max = Math.max(...DT.perfHist, 1), pts = DT.perfHist.map((v, i) => `${(i / 59) * 100},${36 - (v / max) * 32}`).join(' ');
  const card = (k, v, extra) => `<div class="metric"><span>${t(k)}</span><b>${v}</b>${extra || ''}</div>`;
  return `<div class="ccol"><div class="dtbar"><button class="mini" data-dta="gc">${t('dt.gc')}</button></div><div class="pane" style="font-family:'Segoe UI'"><div class="metrics">`
    + card('dt.heap', `${fmtB(p.JSHeapUsedSize)} <small style="font-size:12px;color:var(--t3)">/ ${fmtB(p.JSHeapTotalSize)}</small>`, `<svg viewBox="0 0 100 36" preserveAspectRatio="none" aria-hidden="true"><polyline points="${pts}"/></svg>`)
    + card('dt.nodes', Math.round(p.Nodes).toLocaleString(LANG)) + card('dt.docs', p.Documents) + card('dt.listeners', Math.round(p.JSEventListeners).toLocaleString(LANG))
    + card('dt.layouts', Math.round(p.LayoutCount).toLocaleString(LANG)) + card('dt.script', (p.ScriptDuration || 0).toFixed(2) + ' s')
    + '</div></div></div>';
}

/* ---------- Interactions ---------- */
$('dt').addEventListener('click', (e) => {
  const b = e.target.closest('[data-dtt],[data-dta],[data-tw],[data-nid],[data-oid],[data-rq],[data-del-st]');
  if (!b) return;
  const d = b.dataset;
  if (d.dtt) { DT.tab = d.dtt; store.set('dtTab', d.dtt); return dtRender(); }
  if (d.oid) return expandObj(b);
  if (d.tw) {
    const id = +d.tw, n = DT.nodes.get(id);
    if (DT.exp.has(id)) DT.exp.delete(id); else { DT.exp.add(id); if (n && n.childNodeCount && !(n.children && n.children.length)) DT.call('DOM.requestChildNodes', { nodeId: id, depth: 1 }); }
    return dtRender(true);
  }
  if (d.nid) return selectNode(+d.nid);
  if (d.rq) { DT.netSel = DT.netSel === d.rq ? null : d.rq; return dtRender(true); }
  if (d.delSt) {
    if (d.delSt === "c") { const c = DT.storeData.c.find((x) => x.name === d.k) || {}; DT.call("Network.deleteCookies", { name: d.k, domain: c.domain, path: c.path }).then(loadStorage); }
    else DT.call('Runtime.evaluate', { expression: `${d.delSt === 'l' ? 'localStorage' : 'sessionStorage'}.removeItem(${JSON.stringify(d.k)})` }).then(loadStorage);
    return;
  }
  switch (d.dta) {
    case 'close': return send('dt-close');
    case 'edge': return send('dt-edge');
    case 'pick':
      DT.picking = !DT.picking;
      DT.call('Overlay.setInspectMode', { mode: DT.picking ? 'searchForNode' : 'none', highlightConfig: HL });
      return dtRender(true);
    case 'clog': DT.logs = []; DT.errs = DT.warns = 0; DT.call('Runtime.discardConsoleEntries'); return dtRender(true);
    case 'cnet': DT.net.clear(); DT.netOrder = []; DT.netSel = null; return dtRender(true);
    case 'rst': DT.storeData = null; return dtRender(true);
    case 'gc': return DT.call('HeapProfiler.collectGarbage').then(pollPerf);
    case 'dq-color': case 'dq-ruler': return FX.click[d.dta]();
  }
});
$('dt').addEventListener('change', (e) => {
  const d = e.target.dataset;
  if (d.dta === 'preserve') DT.preserve = e.target.checked;
  if (d.dta === 'nocache') { DT.noCache = e.target.checked; DT.call('Network.setCacheDisabled', { cacheDisabled: DT.noCache }); }
});
$('dt').addEventListener('input', (e) => {
  if (e.target.id === 'netf') { DT.netF = e.target.value; const pos = e.target.selectionStart; dtRender(true); const n = $('netf'); n.focus(); n.setSelectionRange(pos, pos); }
  if (e.target.closest('#dtin')) { e.target.style.height = 'auto'; e.target.style.height = e.target.scrollHeight + 'px'; }
});
$('dt').addEventListener('keydown', (e) => {
  const ta = e.target.closest('#dtin textarea');
  if (!ta) return;
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); const v = ta.value.trim(); if (v) { ta.value = ''; evalCmd(v); } }
  else if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && !ta.value.includes('\n')) {
    e.preventDefault();
    DT.hi = Math.max(-1, Math.min(DT.hist.length - 1, DT.hi + (e.key === 'ArrowUp' ? 1 : -1)));
    ta.value = DT.hi < 0 ? '' : DT.hist[DT.hi];
  } else if (e.key === 'l' && e.ctrlKey) { e.preventDefault(); DT.logs = []; DT.errs = DT.warns = 0; dtRender(true); }
});
// Survol d'un élément → surbrillance dans la page
let lastHl = 0;
$("dt").addEventListener("mouseover", (e) => {
  const n = e.target.closest("[data-nid]");
  if (n && +n.dataset.nid !== lastHl && (lastHl = +n.dataset.nid)) DT.call('Overlay.highlightNode', { nodeId: +n.dataset.nid, highlightConfig: HL });
});
$('dt').addEventListener('mouseleave', () => DT.open && DT.call('Overlay.hideHighlight'));
