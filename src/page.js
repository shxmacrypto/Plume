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

  Object.defineProperty(window, '__plume', {
    value: Object.freeze({
      apply,
      reader: (o) => toggleReader(o),
      speak: (r) => speak(r),
      fill(u, p) {
        const pw = loginPw(); if (!pw) return;
        const us = userOf(pw);
        if (us && u) setVal(us, u);
        setVal(pw, p);
        lastSent = (u || '') + '\n' + p; // ne pas proposer d'enregistrer ce qu'on vient de remplir
      },
    }),
  });
})();
