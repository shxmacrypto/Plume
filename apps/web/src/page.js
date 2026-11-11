// Plume — script injecté dans chaque page (cadre principal uniquement).
// Mots de passe (détection / remplissage), mode lecture, mode sombre forcé, lecture à voix haute,
// masquage visuel des emplacements publicitaires. Tout est inactif tant qu'on ne s'en sert pas.
(() => {
  if (window.__plume || location.protocol === 'chrome-extension:') return;
  const post = (o) => { try { window.ipc.postMessage(JSON.stringify(o)); } catch {} };
  const cfg = () => window.__plumeCfg || {};
  const host = location.hostname.replace(/^www\./, '');

  /* ---------- Styles : mode sombre forcé + masquage des pubs ---------- */
  const ADS = ['ins.adsbygoogle', '[id^="google_ads"]', '[id^="div-gpt-ad"]', '[class*="ad-slot"]', '[class*="adslot"]',
    '[data-ad-slot]', '[data-ad-unit]', '[data-google-query-id]', 'iframe[src*="doubleclick"]', 'iframe[src*="googlesyndication"]',
    '.adsbox', '.ad-banner', '.ad-container', '.advert', '.advertisement', '.sponsored-content', '[aria-label="Advertisement"]',
    '#taboola-below-article-thumbnails', '.trc_related_container', '.OUTBRAIN', '[id^="taboola-"]'].join(',');
  let styleEl;
  function apply() {
    const c = cfg();
    let css = '';
    if (c.ab && !(c.allow || []).includes(host)) css += ADS + '{display:none!important}';
    const dark = c.darkAll ? !(c.light || []).includes(host) : (c.dark || []).includes(host);
    if (dark && !nativeDark()) {
      css += 'html{filter:invert(.92) hue-rotate(180deg)!important;background:#fff!important}'
        + 'img,video,picture,canvas,iframe,svg image,[style*="background-image"],embed,object{filter:invert(1) hue-rotate(180deg)!important}';
    }
    if (!styleEl) { styleEl = document.createElement('style'); styleEl.id = '__plume_css'; }
    styleEl.textContent = css;
    const root = document.head || document.documentElement;
    if (root) { if (!styleEl.isConnected) root.appendChild(styleEl); }
    else if (!apply.wait) { // document pas encore créé : on attend son élément racine
      apply.wait = new MutationObserver(() => { if (document.documentElement) { apply.wait.disconnect(); apply.wait = null; apply(); } });
      apply.wait.observe(document, { childList: true });
    }
  }
  // Site déjà sombre ? (évite d'inverser un thème sombre en thème clair)
  function nativeDark() {
    if (!document.body) return false;
    const bg = getComputedStyle(document.body).backgroundColor.match(/\d+/g) || [255, 255, 255];
    const bg2 = getComputedStyle(document.documentElement).backgroundColor.match(/\d+/g);
    const c = (bg[3] === '0' && bg2) ? bg2 : bg;
    return (0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]) < 90 && c[3] !== '0';
  }
  document.addEventListener('DOMContentLoaded', apply);
  apply();

  /* ---------- Mots de passe ---------- */
  const isPw = (el) => el && el.tagName === 'INPUT' && el.type === 'password';
  const userOf = (pw) => {
    const scope = pw.form || pw.closest('div,section,main,body') || document;
    const ins = [...scope.querySelectorAll('input')].filter((i) => /^(text|email|tel|)$/.test(i.type) && i.offsetParent !== null);
    const before = ins.filter((i) => i.compareDocumentPosition(pw) & Node.DOCUMENT_POSITION_FOLLOWING);
    return (before.pop() || ins[0] || null);
  };
  const loginPw = () => [...document.querySelectorAll('input[type=password]')].find((i) => i.offsetParent !== null);
  let asked = false, lastSent = '';
  document.addEventListener('focusin', (e) => {
    const t = e.target;
    if (asked || t.tagName !== 'INPUT') return;
    const pw = isPw(t) ? t : loginPw();
    if (!pw || (t !== pw && t !== userOf(pw))) return;
    asked = true;
    post({ c: 'pw-get' });
  }, true);
  function capture(pw) {
    if (!pw || !pw.value) return;
    const pws = [...(pw.form || document).querySelectorAll('input[type=password]')].filter((i) => i.value);
    if (pws.length > 1 && pws.some((i) => i.value !== pws[0].value)) pw = pws[pws.length - 1]; // changement de mot de passe : le nouveau
    const u = userOf(pw);
    const key = (u ? u.value : '') + '\n' + pw.value;
    if (key === lastSent) return;
    lastSent = key;
    post({ c: 'pw-save', u: u ? u.value.trim() : '', p: pw.value });
  }
  document.addEventListener('submit', (e) => capture([...e.target.querySelectorAll('input[type=password]')].find((i) => i.value)), true);
  document.addEventListener('keydown', (e) => { if (e.key === 'Enter' && isPw(e.target)) capture(e.target); }, true);
  document.addEventListener('click', (e) => {
    const b = e.target.closest && e.target.closest('button,input[type=submit],[role=button]');
    if (!b) return;
    const pw = loginPw();
    if (pw && pw.value) setTimeout(() => capture(pw), 0);
  }, true);
  function setVal(el, v) {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    set.call(el, v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }

  /* ---------- Mode lecture ---------- */
  const JUNK = /(^|[\s_-])(ad|ads|advert|share|social|comment|related|promo|newsletter|subscribe|sidebar|footer|nav|menu|cookie|banner|popup|recommend)([\s_-]|$)/i;
  function article() {
    const pick = document.querySelector('article, [itemprop=articleBody], main, [role=main]');
    let best = pick, score = pick ? pick.innerText.length : 0;
    if (!pick || score < 600) {
      for (const p of document.querySelectorAll('p')) {
        const el = p.parentElement; if (!el || el.__s) continue;
        el.__s = 1;
        const s = [...el.children].filter((c) => c.tagName === 'P').reduce((a, c) => a + c.innerText.length, 0);
        if (s > score) { score = s; best = el; }
      }
    }
    return best || document.body;
  }
  function clean(root) {
    const c = root.cloneNode(true);
    c.querySelectorAll('script,style,noscript,iframe,form,button,input,select,nav,aside,footer,header,svg,video,audio,[hidden],[aria-hidden=true]').forEach((n) => n.remove());
    c.querySelectorAll('*').forEach((n) => {
      if (JUNK.test((n.className && n.className.baseVal === undefined ? n.className : '') + ' ' + n.id) && n.innerText.length < 600) return n.remove();
      for (const a of [...n.attributes]) if (!['src', 'href', 'alt', 'srcset'].includes(a.name)) n.removeAttribute(a.name);
    });
    c.querySelectorAll('img').forEach((i) => { const s = i.getAttribute('srcset'); if (!i.getAttribute('src') && s) i.src = s.split(',').pop().trim().split(' ')[0]; i.removeAttribute('srcset'); i.loading = 'lazy'; });
    return c.innerHTML;
  }

  /* ---------- 0.7 · Outils ---------- */
  // Article nettoyé pour l'export et la lecture hors ligne : seulement des balises de texte, liens http(s), images http(s)
  const SAFE_TAGS = new Set(['p', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'a', 'img', 'ul', 'ol', 'li', 'blockquote', 'pre', 'code', 'em', 'strong', 'b', 'i', 'figure', 'figcaption',
    'table', 'thead', 'tbody', 'tr', 'td', 'th', 'br', 'hr', 'sup', 'sub', 'small', 'mark', 'dl', 'dt', 'dd']);
  const DROP_TAGS = new Set(['object', 'embed', 'link', 'meta', 'base', 'template', 'canvas', 'map', 'area', 'math', 'source', 'track', 'style', 'script', 'iframe', 'svg']);
  const absUrl = (v) => { try { const u = new URL(v, location.href); return /^https?:$/.test(u.protocol) ? u.href : ''; } catch { return ''; } };
  function safeArticle() {
    const t = document.createElement('template');
    t.innerHTML = clean(article()); // analysé sans rien charger ni exécuter
    const walk = (el) => {
      for (const c of [...el.children]) {
        const tag = c.tagName.toLowerCase();
        if (DROP_TAGS.has(tag)) { c.remove(); continue; }
        walk(c);
        if (!SAFE_TAGS.has(tag)) { c.replaceWith(...c.childNodes); continue; }
        for (const a of [...c.attributes]) {
          if (a.name === 'href' && tag === 'a') { const u = absUrl(a.value); if (u) { c.setAttribute('href', u); continue; } }
          else if (a.name === 'src' && tag === 'img') { const u = absUrl(a.value); if (u) { c.setAttribute('src', u); continue; } }
          else if (a.name === 'alt' || a.name === 'title') continue;
          c.removeAttribute(a.name);
        }
        if (tag === 'img' && !c.getAttribute('src')) c.remove();
      }
    };
    walk(t.content);
    const div = document.createElement('div');
    div.append(t.content);
    return div;
  }
  const articleTitle = () => (((document.querySelector('h1') || {}).innerText || document.title || location.hostname) + '').trim().slice(0, 300);
  // #37 Markdown
  function toMd(n, img) {
    if (n.nodeType === 3) return n.nodeValue.replace(/\s+/g, ' ');
    if (n.nodeType !== 1 && n.nodeType !== 11) return '';
    const tag = n.nodeType === 11 ? '' : n.tagName.toLowerCase(), kids = () => [...n.childNodes].map((c) => toMd(c, img)).join('');
    switch (tag) {
      case 'h1': case 'h2': case 'h3': case 'h4': case 'h5': case 'h6': return '\n\n' + '#'.repeat(+tag[1]) + ' ' + kids().trim() + '\n\n';
      case 'p': case 'figure': case 'dl': return '\n\n' + kids().trim() + '\n\n';
      case 'figcaption': case 'dd': case 'dt': return '\n' + kids().trim() + '\n';
      case 'br': return '  \n';
      case 'hr': return '\n\n---\n\n';
      case 'strong': case 'b': { const k = kids().trim(); return k ? `**${k}**` : ''; }
      case 'em': case 'i': { const k = kids().trim(); return k ? `*${k}*` : ''; }
      case 'code': return n.closest('pre') ? n.textContent : '`' + n.textContent.replace(/`/g, '\u02cb') + '`';
      case 'pre': return '\n\n```\n' + n.textContent.replace(/\n+$/, '') + '\n```\n\n';
      case 'a': { const k = kids().trim(), h = n.getAttribute('href'); return h && k ? `[${k.replace(/[[\]]/g, '')}](${h})` : k; }
      case 'img': { const s = n.getAttribute('src'); return img && s ? `![${(n.getAttribute('alt') || '').replace(/[[\]]/g, '')}](${s})` : ''; }
      case 'ul': case 'ol': { let k = 0; return '\n\n' + [...n.children].filter((c) => c.tagName === 'LI').map((li) => (tag === 'ol' ? `${++k}. ` : '- ') + toMd(li, img).trim().replace(/\n+/g, '\n   ')).join('\n') + '\n\n'; }
      case 'blockquote': return '\n\n' + kids().trim().split('\n').map((l) => '> ' + l).join('\n') + '\n\n';
      case 'table': {
        const rows = [...n.querySelectorAll('tr')].map((tr) => [...tr.children].map((td) => toMd(td, img).trim().replace(/\|/g, '\\|').replace(/\n+/g, ' ')));
        if (!rows.length) return '';
        const w = Math.max(...rows.map((r) => r.length)), line = (r) => '| ' + Array.from({ length: w }, (_, k) => r[k] || '').join(' | ') + ' |';
        return '\n\n' + [line(rows[0]), '|' + ' --- |'.repeat(w), ...rows.slice(1).map(line)].join('\n') + '\n\n';
      }
      default: return kids();
    }
  }
  // #33 lecture / pause / piste suivante (les sites n'exposent pas tous leurs commandes : au mieux)
  function mediaCtl(a) {
    const ms = [...document.querySelectorAll('video,audio')];
    if (a === 'toggle') {
      const playing = ms.filter((m) => !m.paused && !m.ended);
      if (playing.length) return playing.forEach((m) => m.pause());
      const m = ms.filter((x) => !x.ended).sort((x, y) => y.currentTime - x.currentTime || y.offsetWidth * y.offsetHeight - x.offsetWidth * x.offsetHeight)[0];
      if (m) m.play().catch(() => {});
      return;
    }
    const nx = a === 'next';
    const sel = nx ? '.ytp-next-button,[data-testid=control-button-skip-forward],.skipControl__next,.player-controls__next' : '.ytp-prev-button,[data-testid=control-button-skip-back],.skipControl__previous,.player-controls__previous';
    const re = nx ? /^(next|suivant|skip|piste suivante|titre suivant)/i : /^(previous|prev|précédent|piste précédente|titre précédent)/i;
    const b = document.querySelector(sel) || [...document.querySelectorAll('button,[role=button]')].find((x) => re.test((x.getAttribute('aria-label') || x.title || '').trim()));
    if (b) b.click();
  }
  // #32 Picture-in-Picture : la vidéo en lecture la plus grande
  async function pipAuto(sound) {
    if (document.pictureInPictureElement) return 'on';
    const v = [...document.querySelectorAll('video')]
      .filter((x) => !x.paused && !x.ended && x.readyState > 2 && !x.disablePictureInPicture && (!sound || (!x.muted && x.volume > 0)))
      .sort((p, q) => q.offsetWidth * q.offsetHeight - p.offsetWidth * p.offsetHeight)[0];
    if (!v || !document.pictureInPictureEnabled) return 'no';
    try { await v.requestPictureInPicture(); } catch { return 'no'; }
    v.addEventListener('leavepictureinpicture', () => post({ c: 'pip', on: false }), { once: true });
    return 'on';
  }
  // #35 Règle : glisser pour mesurer (px CSS), Échap ou clic droit pour quitter
  function ruler(lang) {
    if (document.querySelector('plume-ruler')) return;
    const fr = lang === 'fr', host = document.createElement('plume-ruler'), sh = host.attachShadow({ mode: 'closed' });
    sh.innerHTML = `<style>:host{all:initial;position:fixed;inset:0;z-index:2147483647;cursor:crosshair;display:block}
      .h,.v{position:fixed;background:rgba(255,55,95,.55);pointer-events:none}.h{left:0;right:0;height:1px}.v{top:0;bottom:0;width:1px}
      .box{position:fixed;border:1px solid #ff375f;background:rgba(255,55,95,.12);pointer-events:none}
      .lbl{position:fixed;font:600 12px/1 system-ui,sans-serif;color:#fff;background:#ff375f;padding:5px 7px;border-radius:6px;pointer-events:none;white-space:nowrap}
      .tip{position:fixed;left:50%;top:12px;transform:translateX(-50%);font:600 12px system-ui,sans-serif;color:#fff;background:rgba(0,0,0,.78);padding:7px 11px;border-radius:9px;pointer-events:none}</style>
      <div class="h"></div><div class="v"></div><div class="box" hidden></div><div class="lbl" hidden></div><div class="tip">${fr ? 'Glissez pour mesurer · Échap pour quitter' : 'Drag to measure · Esc to exit'}</div>`;
    const [h, v, box, lbl] = ['.h', '.v', '.box', '.lbl'].map((q) => sh.querySelector(q));
    let st = null;
    const show = (x, y, txt) => { lbl.hidden = false; lbl.textContent = txt; lbl.style.left = Math.min(x + 12, innerWidth - 160) + 'px'; lbl.style.top = Math.min(y + 12, innerHeight - 30) + 'px'; };
    const move = (e) => {
      h.style.top = e.clientY + 'px'; v.style.left = e.clientX + 'px';
      if (!st) return show(e.clientX, e.clientY, `${e.clientX}, ${e.clientY}`);
      const x = Math.min(st.x, e.clientX), y = Math.min(st.y, e.clientY), w = Math.abs(e.clientX - st.x), hh = Math.abs(e.clientY - st.y);
      Object.assign(box.style, { left: x + 'px', top: y + 'px', width: w + 'px', height: hh + 'px' }); box.hidden = false;
      show(e.clientX, e.clientY, `${w} × ${hh} px · ${Math.round(Math.hypot(w, hh))} px`);
    };
    const down = (e) => { if (e.button === 2) return quit(); e.preventDefault(); st = { x: e.clientX, y: e.clientY }; };
    const up = () => { st = null; };
    const key = (e) => { if (e.key === 'Escape') { e.preventDefault(); quit(); } };
    function quit() { host.remove(); removeEventListener('keydown', key, true); }
    host.addEventListener('mousemove', move); host.addEventListener('mousedown', down); host.addEventListener('mouseup', up);
    host.addEventListener('contextmenu', (e) => { e.preventDefault(); quit(); });
    addEventListener('keydown', key, true);
    document.documentElement.appendChild(host);
  }

  // #35 Pipette : Plume capture la page et l'affiche par-dessus ; une loupe suit la souris, un clic copie la couleur
  const copyText = (s) => {
    const ta = document.createElement('textarea');
    ta.value = s; ta.setAttribute('readonly', ''); ta.style.cssText = 'position:fixed;left:-9999px;top:0;opacity:0';
    document.documentElement.appendChild(ta); ta.select();
    let ok = false; try { ok = document.execCommand('copy'); } catch {}
    ta.remove();
    return ok;
  };
  const colorFmt = (hex, f) => {
    const n = parseInt(hex.slice(1), 16), r = n >> 16, g = (n >> 8) & 255, b = n & 255;
    if (f === 'rgb') return `rgb(${r}, ${g}, ${b})`;
    if (f === 'hsl') {
      const R = r / 255, G = g / 255, B = b / 255, mx = Math.max(R, G, B), mn = Math.min(R, G, B), l = (mx + mn) / 2, d = mx - mn;
      let h = 0, s = 0;
      if (d) { s = d / (1 - Math.abs(2 * l - 1)); h = mx === R ? ((G - B) / d) % 6 : mx === G ? (B - R) / d + 2 : (R - G) / d + 4; h = Math.round(h * 60); if (h < 0) h += 360; }
      return `hsl(${h}, ${Math.round(s * 100)}%, ${Math.round(l * 100)}%)`;
    }
    return hex;
  };
  async function pickColor(b64, fmt, lang) {
    if (document.querySelector('plume-picker')) return;
    const fr = lang === 'fr';
    const bytes = Uint8Array.from(atob(b64), (ch) => ch.charCodeAt(0));
    const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' })); // pas d'<img> : la CSP du site ne s'applique pas
    const host = document.createElement('plume-picker'), sh = host.attachShadow({ mode: 'closed' });
    sh.innerHTML = `<style>:host{all:initial;position:fixed;inset:0;z-index:2147483647;cursor:crosshair;display:block}
      canvas.bg{position:fixed;inset:0;width:100vw;height:100vh}
      .lens{position:fixed;width:116px;height:116px;border-radius:50%;border:3px solid #fff;box-shadow:0 6px 22px rgba(0,0,0,.45);overflow:hidden;pointer-events:none}
      .lens canvas{width:100%;height:100%;image-rendering:pixelated}
      .lens i{position:absolute;left:50%;top:50%;width:8px;height:8px;margin:-4px 0 0 -4px;border:1.5px solid #fff;outline:1px solid rgba(0,0,0,.6)}
      .lbl{position:fixed;display:flex;align-items:center;gap:6px;font:600 12px/1 system-ui,sans-serif;color:#fff;background:rgba(0,0,0,.82);padding:6px 8px;border-radius:8px;pointer-events:none;white-space:nowrap}
      .lbl b{width:14px;height:14px;border-radius:4px;box-shadow:inset 0 0 0 1px rgba(255,255,255,.5)}
      .tip{position:fixed;left:50%;top:12px;transform:translateX(-50%);font:600 12px system-ui,sans-serif;color:#fff;background:rgba(0,0,0,.78);padding:7px 11px;border-radius:9px;pointer-events:none}</style>
      <canvas class="bg"></canvas><div class="lens" hidden><canvas width="15" height="15"></canvas><i></i></div><div class="lbl" hidden><b></b><span></span></div>
      <div class="tip">${fr ? 'Cliquez sur une couleur · Échap pour annuler' : 'Click a color · Esc to cancel'}</div>`;
    const bg = sh.querySelector('canvas.bg'), g = bg.getContext('2d', { willReadFrequently: true });
    bg.width = bmp.width; bg.height = bmp.height; g.drawImage(bmp, 0, 0);
    const lens = sh.querySelector('.lens'), lc = sh.querySelector('.lens canvas').getContext('2d'), lbl = sh.querySelector('.lbl');
    lc.imageSmoothingEnabled = false;
    const sx = bmp.width / innerWidth, sy = bmp.height / innerHeight;
    const at = (e) => {
      const x = Math.min(bmp.width - 1, Math.max(0, Math.floor(e.clientX * sx))), y = Math.min(bmp.height - 1, Math.max(0, Math.floor(e.clientY * sy)));
      const d = g.getImageData(x, y, 1, 1).data;
      return { x, y, hex: '#' + [d[0], d[1], d[2]].map((v) => v.toString(16).padStart(2, '0')).join('') };
    };
    const move = (e) => {
      const p = at(e);
      lc.clearRect(0, 0, 15, 15); lc.drawImage(bg, p.x - 7, p.y - 7, 15, 15, 0, 0, 15, 15);
      lens.hidden = false; lbl.hidden = false;
      const lx = e.clientX + 132 > innerWidth ? e.clientX - 132 : e.clientX + 16, ly = e.clientY + 150 > innerHeight ? e.clientY - 150 : e.clientY + 16;
      lens.style.left = lx + 'px'; lens.style.top = ly + 'px';
      lbl.style.left = lx + 'px'; lbl.style.top = ly + 122 + 'px';
      lbl.querySelector('b').style.background = p.hex; lbl.querySelector('span').textContent = colorFmt(p.hex, fmt);
    };
    const key = (e) => { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); finish(null); } };
    function finish(hex) {
      host.remove(); removeEventListener('keydown', key, true); bmp.close();
      if (!hex) return;
      const v = colorFmt(hex, fmt);
      post({ c: 'dq-picked', v, copied: copyText(v) });
    }
    host.addEventListener('mousemove', move);
    host.addEventListener('mousedown', (e) => { e.preventDefault(); e.stopPropagation(); });
    host.addEventListener('click', (e) => { e.preventDefault(); e.stopPropagation(); finish(at(e).hex); });
    host.addEventListener('contextmenu', (e) => { e.preventDefault(); finish(null); });
    addEventListener('keydown', key, true);
    document.documentElement.appendChild(host);
  }
  let reader = null;
  function toggleReader(opts) {
    if (reader) { reader.remove(); reader = null; document.documentElement.style.overflow = ''; return false; }
    const o = Object.assign({ size: 20, theme: 'auto', lang: 'en' }, opts || {});
    const art = article(), words = art.innerText.split(/\s+/).length;
    const title = (document.querySelector('h1') || {}).innerText || document.title;
    reader = document.createElement('plume-reader');
    const sh = reader.attachShadow({ mode: 'closed' });
    const fr = o.lang === 'fr';
    sh.innerHTML = `<style>
      :host{all:initial;position:fixed;inset:0;z-index:2147483647;overflow:auto;display:block;
        --bg:#fbfaf7;--fg:#1d1d1f;--mut:#6e6e73;--link:#0066cc;background:var(--bg);color:var(--fg)}
      :host([t=sepia]){--bg:#f6efe1;--fg:#3b2f22;--mut:#7d6b55;--link:#9a5b13}
      :host([t=dark]){--bg:#1c1c1e;--fg:#e8e8ed;--mut:#98989d;--link:#64a8ff}
      .bar{position:sticky;top:0;display:flex;gap:6px;justify-content:flex-end;padding:10px 16px;background:var(--bg);opacity:.96}
      .bar button{font:600 13px system-ui;color:var(--fg);background:rgba(127,127,127,.16);border:0;border-radius:9px;height:32px;min-width:36px;padding:0 12px;cursor:pointer}
      .bar button:hover{background:rgba(127,127,127,.28)}
      .bar button:focus-visible{outline:2px solid var(--link);outline-offset:2px}
      main{max-width:68ch;margin:0 auto;padding:12px 28px 120px;font:var(--s)/1.65 Georgia,'Iowan Old Style',serif}
      h1{font:700 1.7em/1.2 system-ui,'Segoe UI',sans-serif;margin:.2em 0 .3em;letter-spacing:-.01em}
      .meta{font:14px system-ui;color:var(--mut);margin-bottom:2em}
      img,figure{max-width:100%;height:auto;border-radius:10px}
      a{color:var(--link)} pre{white-space:pre-wrap;font-size:.8em} blockquote{border-left:3px solid var(--mut);margin-left:0;padding-left:1em;color:var(--mut)}
      @media (prefers-reduced-motion:no-preference){:host{animation:in .25s ease-out}}@keyframes in{from{opacity:0;transform:translateY(8px)}}
    </style>
    <div class="bar" role="toolbar"><button data-a="-" aria-label="${fr ? 'Texte plus petit' : 'Smaller text'}">A−</button><button data-a="+" aria-label="${fr ? 'Texte plus grand' : 'Larger text'}">A+</button>
      <button data-a="t">${fr ? 'Thème' : 'Theme'}</button><button data-a="s">${fr ? 'Écouter' : 'Listen'}</button><button data-a="x" aria-label="${fr ? 'Fermer le mode lecture' : 'Close reader'}">✕</button></div>
    <main lang="${document.documentElement.lang || ''}"><h1></h1><div class="meta"></div><div class="c">${clean(art)}</div></main>`;
    sh.querySelector('h1').textContent = title;
    sh.querySelector('.meta').textContent = `${host} · ${Math.max(1, Math.round(words / 220))} min`;
    const themes = ['light', 'sepia', 'dark'];
    let t = o.theme === 'auto' ? (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light') : o.theme, size = o.size;
    const upd = () => { reader.setAttribute('t', t); sh.querySelector('main').style.setProperty('--s', size + 'px'); post({ c: 'reader-pref', size, theme: t }); };
    sh.addEventListener('click', (e) => {
      const a = e.target.closest('button')?.dataset.a; if (!a) return;
      if (a === 'x') return toggleReader();
      if (a === 's') return speak(o.rate, sh.querySelector('.c').innerText);
      if (a === '+') size = Math.min(34, size + 2); else if (a === '-') size = Math.max(14, size - 2); else t = themes[(themes.indexOf(t) + 1) % 3];
      upd();
    });
    upd();
    document.documentElement.style.overflow = 'hidden';
    document.documentElement.appendChild(reader);
    sh.querySelector('button').focus();
    return true;
  }
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && reader) toggleReader(); }, true);

  /* ---------- Lecture à voix haute (voix Windows) ---------- */
  function speak(rate, text) {
    const s = speechSynthesis;
    if (s.speaking) { s.cancel(); post({ c: 'speak', on: false }); return; }
    text = text || String(getSelection()) || article().innerText;
    const lang = document.documentElement.lang || navigator.language;
    const chunks = text.replace(/\s+/g, ' ').match(/[^.!?]{1,220}[.!?]?/g) || [];
    chunks.forEach((c, i) => {
      const u = new SpeechSynthesisUtterance(c);
      u.lang = lang; u.rate = rate || 1;
      if (i === chunks.length - 1) u.onend = () => post({ c: 'speak', on: false });
      s.speak(u);
    });
    post({ c: 'speak', on: true });
  }

  /* ---------- 0.7 : fonctions qui n'ajoutent un écouteur que lorsqu'elles sont activées ---------- */
  const feats = {};
  // active / coupe une fonction : `add` installe ses écouteurs et renvoie de quoi les retirer
  function feat(k, want, add) {
    if (want && !feats[k]) feats[k] = add();
    else if (!want && feats[k]) { feats[k](); delete feats[k]; }
  }
  function features() {
    const c = cfg();
    // #26 Teinte adaptative : couleur du site (theme-color, sinon le fond du haut de la page) → barre d'outils
    feat('tint', !!c.tint, () => {
      let last = null;
      const clear = (b) => !b || b === 'transparent' || /^rgba\(.*,\s*0\)$/.test(b);
      const pick = () => {
        let col = '';
        for (const m of document.querySelectorAll('meta[name="theme-color"]')) {
          const md = m.getAttribute('media');
          if (!md || matchMedia(md).matches) { col = (m.getAttribute('content') || '').trim(); if (col) break; }
        }
        if (!col && document.body) {
          let el = document.elementFromPoint(innerWidth / 2, 3);
          while (el && el !== document.documentElement && !col) { const b = getComputedStyle(el).backgroundColor; if (!clear(b)) col = b; el = el.parentElement; }
          if (!col) { const b = getComputedStyle(document.body).backgroundColor; if (!clear(b)) col = b; }
        }
        if (col !== last) { last = col; post({ c: 'tint', v: col }); }
      };
      let tm2 = 0;
      const later = () => { clearTimeout(tm2); tm2 = setTimeout(pick, 300); };
      // les sites ajoutent sans arrêt des scripts et des styles dans <head> : on ne regarde que les balises theme-color
      const isTc = (n) => n.nodeName === 'META' && (n.getAttribute('name') || '').toLowerCase() === 'theme-color';
      const mo = new MutationObserver((list) => { if (list.some((m) => isTc(m.target) || [...m.addedNodes, ...m.removedNodes].some(isTc))) later(); });
      const start = () => { pick(); if (document.head) mo.observe(document.head, { subtree: true, childList: true, attributes: true, attributeFilter: ['content'] }); };
      if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start, { once: true }); else start();
      addEventListener('load', pick, { once: true });
      return () => { mo.disconnect(); clearTimeout(tm2); document.removeEventListener('DOMContentLoaded', start); removeEventListener('load', pick); };
    });
    // #22 Formulaire d'inscription : Plume propose un mot de passe solide (barre d'information)
    feat('pwo', !!c.pwo, () => {
      let done = false;
      const signup = (pw) => {
        if (pw.autocomplete === 'new-password') return true;
        const f = pw.form || pw.closest('form,section,main');
        if (!f) return false;
        if (f.querySelectorAll('input[type=password]').length > 1) return true;
        const s = ((f.textContent || '').slice(0, 3000) + ' ' + (f.getAttribute('action') || '') + ' ' + (pw.name || '') + ' ' + (pw.id || '')).toLowerCase();
        return /sign ?up|register|create (an |your )?account|join now|inscri|cr[ée]er (un |votre )?compte|new.?password|nouveau mot de passe/.test(s);
      };
      const fn = (e) => { const t = e.target; if (done || !isPw(t) || t.value || !signup(t)) return; done = true; post({ c: 'pw-new' }); };
      document.addEventListener('focusin', fn, true);
      return () => document.removeEventListener('focusin', fn, true);
    });
    // #4 Préchargement : un lien survolé quelques instants est préconnecté (ou préchargé entièrement)
    feat('pf', c.pf && c.pf !== 'off', () => {
      let tm = 0;
      const done = new Set();
      const over = (e) => {
        const a = e.target.closest && e.target.closest('a[href]');
        if (!a) return;
        clearTimeout(tm);
        tm = setTimeout(() => {
          let u; try { u = new URL(a.href, location.href); } catch { return; }
          if (!/^https?:$/.test(u.protocol) || u.href.split('#')[0] === location.href.split('#')[0] || done.has(u.href) || !document.head) return;
          done.add(u.href);
          const l = document.createElement('link'); l.rel = 'preconnect'; l.href = u.origin; document.head.appendChild(l);
          if (cfg().pf === 'full' && HTMLScriptElement.supports && HTMLScriptElement.supports('speculationrules')) {
            const sr = document.createElement('script'); sr.type = 'speculationrules';
            sr.textContent = JSON.stringify({ prerender: [{ source: 'list', urls: [u.href] }] });
            document.head.appendChild(sr);
          }
        }, cfg().pfd || 250);
      };
      const out = (e) => { if (e.target.closest && e.target.closest('a[href]')) clearTimeout(tm); };
      document.addEventListener('mouseover', over, true);
      document.addEventListener('mouseout', out, true);
      return () => { clearTimeout(tm); document.removeEventListener('mouseover', over, true); document.removeEventListener('mouseout', out, true); };
    });
    // #3 Formulaire commencé : l'onglet n'est pas mis en veille tant qu'il n'est pas envoyé
    feat('form', !!c.form, () => {
      let dirty = false;
      const input = (e) => {
        const el = e.target;
        if (dirty || !el || /^(password|search|checkbox|radio)$/.test(el.type || '')) return;
        if (!(el.form || el.tagName === 'TEXTAREA' || el.isContentEditable)) return;
        dirty = true; post({ c: 'form', on: true });
      };
      const sent = () => { if (dirty) { dirty = false; post({ c: 'form', on: false }); } };
      document.addEventListener('input', input, true);
      document.addEventListener('submit', sent, true);
      return () => { document.removeEventListener('input', input, true); document.removeEventListener('submit', sent, true); sent(); };
    });
    // #3 Appel en cours : caméra ou micro ouverts par la page
    feat('call', !!c.call && navigator.mediaDevices && navigator.mediaDevices.getUserMedia, () => {
      const md = navigator.mediaDevices, gum = md.getUserMedia;
      const streams = new Set();
      let tm = 0;
      const check = () => {
        for (const st of streams) if (!st.getTracks().some((x) => x.readyState === 'live')) streams.delete(st);
        if (!streams.size) { clearInterval(tm); tm = 0; post({ c: 'call', on: false }); }
      };
      md.getUserMedia = function (...a) {
        return gum.apply(md, a).then((st) => { streams.add(st); post({ c: 'call', on: true }); if (!tm) tm = setInterval(check, 15000); return st; });
      };
      return () => { md.getUserMedia = gum; clearInterval(tm); };
    });
    // #5 Mode Lite : pas de lecture automatique tant qu'on n'a pas cliqué dans la page
    const na = c.noAuto || [];
    feat('auto', na.includes(host) || (na.includes('*') && !na.includes('-' + host)), () => {
      const play = (e) => { if (!(navigator.userActivation && navigator.userActivation.hasBeenActive)) e.target.pause(); };
      document.addEventListener('play', play, true);
      return () => document.removeEventListener('play', play, true);
    });
    // #17 Gestes de souris : clic droit maintenu + tracé (← → ↑ ↓), avec une trace facultative
    feat('gest', !!c.gest, () => {
      let last = null, path = '', block = false, cv = null, g2 = null, pts = [];
      const sens = () => ((cfg().gest || {}).sens || 50);
      const trail = () => !!(cfg().gest || {}).trail;
      const draw = () => {
        if (!trail() || pts.length < 2) return;
        if (!cv) {
          cv = document.createElement('canvas');
          cv.style.cssText = 'position:fixed;inset:0;z-index:2147483647;pointer-events:none';
          cv.width = innerWidth * devicePixelRatio; cv.height = innerHeight * devicePixelRatio;
          cv.style.width = innerWidth + 'px'; cv.style.height = innerHeight + 'px';
          (document.body || document.documentElement).appendChild(cv);
          g2 = cv.getContext('2d'); g2.scale(devicePixelRatio, devicePixelRatio);
          g2.lineWidth = 4; g2.lineCap = g2.lineJoin = 'round'; g2.strokeStyle = 'rgba(10,132,255,.75)';
        }
        const [a, b] = pts.slice(-2);
        g2.beginPath(); g2.moveTo(a[0], a[1]); g2.lineTo(b[0], b[1]); g2.stroke();
      };
      const clear = () => { if (cv) { cv.remove(); cv = g2 = null; } pts = []; };
      const down = (e) => { if (e.button === 2) { last = [e.clientX, e.clientY]; path = ''; pts = [last]; } };
      const move = (e) => {
        if (!last || !(e.buttons & 2)) return;
        pts.push([e.clientX, e.clientY]); draw();
        const dx = e.clientX - last[0], dy = e.clientY - last[1];
        if (Math.hypot(dx, dy) < sens()) return;
        const d = Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? 'R' : 'L') : (dy > 0 ? 'D' : 'U');
        if (path.slice(-1) !== d) path += d;
        last = [e.clientX, e.clientY];
      };
      const up = (e) => {
        if (e.button !== 2 || !last) return;
        last = null; clear();
        if (path) { block = true; post({ c: 'gesture', g: path.slice(0, 4) }); }
      };
      const menu = (e) => { if (block) { e.preventDefault(); block = false; } };
      document.addEventListener('mousedown', down, true);
      document.addEventListener('mousemove', move, true);
      document.addEventListener('mouseup', up, true);
      document.addEventListener('contextmenu', menu, true);
      return () => { clear(); document.removeEventListener('mousedown', down, true); document.removeEventListener('mousemove', move, true); document.removeEventListener('mouseup', up, true); document.removeEventListener('contextmenu', menu, true); };
    });
    // #18 Boutons latéraux de la souris : WebView2 les gère lui-même ; option coupée → on les neutralise
    feat('navOff', c.navSide === false, () => {
      const stop = (e) => { if (e.button === 3 || e.button === 4) { e.preventDefault(); e.stopPropagation(); } };
      for (const ty of ['mousedown', 'mouseup', 'auxclick']) document.addEventListener(ty, stop, true);
      return () => { for (const ty of ['mousedown', 'mouseup', 'auxclick']) document.removeEventListener(ty, stop, true); };
    });
    // #11 Coup d'œil : Maj+clic / Alt+clic / appui long sur un lien, ou liens externes de certains sites
    feat('peek', !!c.peek, () => {
      const linkOf = (e) => e.target.closest && e.target.closest('a[href]');
      const ext = (a) => { try { const u = new URL(a.href, location.href); return /^https?:$/.test(u.protocol) && u.hostname.replace(/^www\./, '') !== host; } catch { return false; } };
      let lp = 0, fired = false;
      const click = (e) => {
        const a = linkOf(e);
        if (!a || e.button !== 0) return;
        const m = cfg().peek, auto = (cfg().peekSites || []).includes(host) && ext(a) && !e.ctrlKey && !e.shiftKey && !e.altKey;
        const want = fired || (m === 'shift' && e.shiftKey) || (m === 'alt' && e.altKey) || auto;
        fired = false;
        if (!want) return;
        let u; try { u = new URL(a.href, location.href); } catch { return; }
        if (!/^https?:$/.test(u.protocol)) return;
        e.preventDefault(); e.stopImmediatePropagation();
        post({ c: 'peek', u: u.href });
      };
      const down = (e) => { if (cfg().peek === 'long' && e.button === 0 && linkOf(e)) lp = setTimeout(() => { fired = true; }, 500); };
      const up = () => clearTimeout(lp);
      document.addEventListener('click', click, true);
      document.addEventListener('mousedown', down, true);
      document.addEventListener('mouseup', up, true);
      return () => { document.removeEventListener('click', click, true); document.removeEventListener('mousedown', down, true); document.removeEventListener('mouseup', up, true); };
    });
    ccApply();
    ccJs();
  }
  // #29 CSS personnalisé du site : feuille « construite » (pas de balise <style>, donc pas bloquée par la CSP du site)
  const ccHost = () => location.host.replace(/^www\./, '').toLowerCase();
  const ccKey = (keys) => { const h = ccHost(); return (keys || []).find((d) => h === d || h.endsWith('.' + d)); };
  let ccSheet = null, ccJsDone = false;
  // JS personnalisé : Plume l'exécute quand la page est prête, une fois par page (la CSP du site ne le bloque pas).
  // Appelé aussi après une mise à jour de la config : le script de config le plus récent passe après page.js.
  function ccJs() {
    if (ccJsDone || !ccKey(cfg().ccjs)) return;
    ccJsDone = true;
    const go = () => post({ c: 'cc-js' });
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', go, { once: true }); else go();
  }
  function ccApply() {
    const m = cfg().cc || {}, k = ccKey(Object.keys(m)), css = k ? m[k] : '';
    try {
      if (!css) {
        if (ccSheet) { document.adoptedStyleSheets = document.adoptedStyleSheets.filter((x) => x !== ccSheet); ccSheet = null; }
        return;
      }
      if (!ccSheet) { ccSheet = new CSSStyleSheet(); document.adoptedStyleSheets = [...document.adoptedStyleSheets, ccSheet]; }
      ccSheet.replaceSync(css);
    } catch {}
  }
  // Ctrl+clic ou clic molette sur un lien : onglet en arrière-plan (comme Chrome), sauf si le site gère le clic lui-même
  const bgOpen = (e) => {
    if (e.defaultPrevented) return;
    const mid = e.type === 'auxclick' && e.button === 1, ctrl = e.type === 'click' && e.button === 0 && (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey;
    if (!mid && !ctrl) return;
    const a = e.target.closest && e.target.closest('a[href]');
    if (!a || a.hasAttribute('download')) return;
    let u; try { u = new URL(a.href, location.href); } catch { return; }
    if (!/^https?:$/.test(u.protocol)) return;
    e.preventDefault();
    post({ c: 'open-bg', u: u.href });
  };
  addEventListener('click', bgOpen);
  addEventListener('auxclick', bgOpen);
  // #9 Écran partagé : défilement synchronisé (activé par Plume pour les deux pages)
  let syncOn = false, syncLock = 0, syncRaf = 0;
  const onScroll = () => {
    if (Date.now() < syncLock || syncRaf) return;
    syncRaf = requestAnimationFrame(() => { syncRaf = 0; const max = document.documentElement.scrollHeight - innerHeight; post({ c: 'scroll', f: max > 0 ? scrollY / max : 0 }); });
  };
  function sync(on2) { if (on2 === syncOn) return; syncOn = on2; if (on2) addEventListener('scroll', onScroll, { passive: true }); else removeEventListener('scroll', onScroll); }
  function scrollToF(f) { syncLock = Date.now() + 200; const max = document.documentElement.scrollHeight - innerHeight; window.scrollTo(0, f * max); }
  const apply0 = apply;
  apply = function () { apply0(); features(); };
  features();

  Object.defineProperty(window, '__plume', {
    value: Object.freeze({
      apply,
      sync: (on2) => sync(!!on2),
      scrollTo: (f) => scrollToF(+f || 0),
      reader: (o) => toggleReader(o),
      speak: (r) => speak(r),
      fill(u, p) {
        const pw = loginPw(); if (!pw) return;
        const us = userOf(pw);
        if (us && u) setVal(us, u);
        setVal(pw, p);
        lastSent = (u || '') + '\n' + p; // ne pas proposer d'enregistrer ce qu'on vient de remplir
      },
      // #22 mot de passe généré : tous les champs mot de passe du formulaire (mot de passe + confirmation)
      fillNew(p) {
        const a = document.activeElement, cur = isPw(a) ? a : [...document.querySelectorAll('input[type=password]')].find((i) => i.offsetParent !== null);
        if (!cur) return;
        const scope = cur.form || cur.closest('form,section,main') || document;
        [...scope.querySelectorAll('input[type=password]')].filter((i) => i.offsetParent !== null && (!i.value || i === cur)).forEach((i) => setVal(i, p));
      },
      // 0.7 · Outils
      pipAuto: (sound) => pipAuto(!!sound),
      mediaCtl: (a) => mediaCtl(String(a)),
      ruler: (lang) => ruler(String(lang || '')),
      pickColor: (b64, fmt, lang) => { pickColor(String(b64), String(fmt), String(lang)).catch(() => post({ c: 'dq-picked', v: '' })); },
      mdExport(img) {
        const div = safeArticle(), title = articleTitle();
        const md = toMd(div, !!img).replace(/[ \t]+\n/g, '\n').replace(/\n{3,}/g, '\n\n').trim();
        post({ c: 'md-out', t: title, md: `# ${title}\n\n<${location.href}>\n\n${md}\n` });
      },
      rlExtract() {
        const div = safeArticle(), imgs = [];
        div.querySelectorAll('img').forEach((im) => { imgs.push(im.getAttribute('src') || ''); im.setAttribute('src', `__PLUME_IMG_${imgs.length - 1}__`); });
        const blocks = [...div.querySelectorAll('p,li,h1,h2,h3,h4,h5,h6,blockquote,pre,figcaption,td')];
        const text = (blocks.length ? blocks.map((e) => e.textContent).join(' ') : div.textContent).replace(/\s+/g, ' ').trim();
        post({ c: 'rl-data', t: articleTitle(), u: location.href, html: div.innerHTML, imgs, ex: text.slice(0, 240), n: text ? text.split(' ').length : 0, lang: document.documentElement.lang || '' });
      },
      // #21 rapport de confidentialité : autres sites contactés + permissions accordées
      report() {
        const site = (h) => { const p = h.split('.'); return p.slice(p.length > 2 && /^(co|com|org|net|gov|ac|edu|gouv)$/.test(p[p.length - 2]) ? -3 : -2).join('.'); };
        const me = site(location.hostname), hosts = new Set();
        for (const e of performance.getEntriesByType('resource')) { try { const h = new URL(e.name).hostname; if (h && site(h) !== me) hosts.add(h); } catch {} }
        const names = ['geolocation', 'notifications', 'camera', 'microphone'];
        Promise.all(names.map((n) => navigator.permissions.query({ name: n }).then((r) => (r.state === 'granted' ? n : '')).catch(() => '')))
          .then((g) => post({ c: 'priv-page', third: [...hosts].slice(0, 300), perms: g.filter(Boolean) }));
      },
    }),
  });
})();
