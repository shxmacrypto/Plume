/* ===== 0.7 : fonctions pures (sans DOM), testées avec node (apps/web/tests/ui07.test.js) ===== */
const pureNorm = (x) => String(x).toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '');

/* #10 Recherche floue : toutes les lettres dans l'ordre ; bonus aux débuts de mots et aux lettres qui se suivent */
function fuzzyScore(q, text) {
  q = pureNorm(q).trim(); text = pureNorm(text);
  if (!q) return 1;
  const at = text.indexOf(q);
  if (at >= 0) return 100 - Math.min(at, 50) + (at === 0 || /[\s/.\-_]/.test(text[at - 1]) ? 30 : 0);
  let ti = 0, score = 0, run = 0;
  for (const ch of q) {
    if (ch === ' ') continue;
    const j = text.indexOf(ch, ti);
    if (j < 0) return 0;
    run = j === ti ? run + 1 : 0;
    score += 1 + run * 2 + (j === 0 || /[\s/.\-_]/.test(text[j - 1]) ? 3 : 0);
    ti = j + 1;
  }
  return Math.max(1, score - Math.floor(text.length / 40));
}

/* #16 Calculatrice : expression arithmétique sans eval (+ − × ÷ ^ %, parenthèses, fonctions, π, e) */
const CALC_FN = { sqrt: Math.sqrt, abs: Math.abs, round: Math.round, floor: Math.floor, ceil: Math.ceil, sin: Math.sin, cos: Math.cos, tan: Math.tan,
  asin: Math.asin, acos: Math.acos, atan: Math.atan, log: Math.log10, ln: Math.log, exp: Math.exp };
function calcExpr(src) {
  const s = String(src).replace(/×/g, '*').replace(/÷/g, '/').replace(/−/g, '-').replace(/\s+/g, '');
  if (!s || !/^[\d.,+\-*/^%()a-zπ]+$/i.test(s) || !/\d|π|pi|\be\b/i.test(s)) return null;
  let i = 0;
  const peek = () => s[i], eat = (c) => (s[i] === c ? (i++, true) : false);
  function num() {
    const m = /^(\d+(?:[.,]\d+)?|[.,]\d+)/.exec(s.slice(i));
    if (!m) return null;
    i += m[0].length;
    return parseFloat(m[0].replace(',', '.'));
  }
  function atom() {
    if (eat('(')) { const v = add(); if (!eat(')')) throw 0; return v; }
    if (eat('-')) return -pow();
    if (eat('+')) return pow();
    const w = /^([a-z]+|π)/i.exec(s.slice(i));
    if (w) {
      const n = w[0].toLowerCase(); i += w[0].length;
      if (n === 'pi' || n === 'π') return Math.PI;
      if (n === 'e') return Math.E;
      const f = CALC_FN[n]; if (!f) throw 0;
      if (!eat('(')) throw 0; const v = add(); if (!eat(')')) throw 0;
      return f(v);
    }
    const v = num(); if (v === null) throw 0;
    return v;
  }
  function post() { let v = atom(); while (peek() === '%') { i++; v /= 100; } return v; }
  function pow() { const b = post(); return eat('^') ? Math.pow(b, pow()) : b; }
  function mul() { let v = pow(); for (;;) { if (eat('*')) v *= pow(); else if (eat('/')) v /= pow(); else return v; } }
  function add() { let v = mul(); for (;;) { if (eat('+')) v += mul(); else if (eat('-')) v -= mul(); else return v; } }
  try {
    const v = add();
    if (i !== s.length || !isFinite(v)) return null;
    // un simple nombre n'est pas un calcul
    if (/^[+-]?(\d+(?:[.,]\d+)?|[.,]\d+)$/.test(s)) return null;
    return v;
  } catch { return null; }
}

/* #16 Conversions d'unités : longueur, masse, volume, vitesse, données, temps, température */
const UNITS = {
  len: { m: 1, meter: 1, metre: 1, meters: 1, metres: 1, mètre: 1, mètres: 1, km: 1000, cm: 0.01, mm: 0.001, mi: 1609.344, mile: 1609.344, miles: 1609.344, yd: 0.9144, yard: 0.9144, yards: 0.9144, ft: 0.3048, foot: 0.3048, feet: 0.3048, pied: 0.3048, pieds: 0.3048, in: 0.0254, inch: 0.0254, inches: 0.0254, pouce: 0.0254, pouces: 0.0254, nmi: 1852 },
  mass: { kg: 1, g: 0.001, mg: 1e-6, t: 1000, lb: 0.45359237, lbs: 0.45359237, livre: 0.45359237, livres: 0.45359237, oz: 0.028349523125, once: 0.028349523125, onces: 0.028349523125, st: 6.35029318 },
  vol: { l: 1, litre: 1, litres: 1, liter: 1, liters: 1, ml: 0.001, cl: 0.01, dl: 0.1, m3: 1000, gal: 3.785411784, gallon: 3.785411784, gallons: 3.785411784, qt: 0.946352946, pt: 0.473176473, cup: 0.2365882365, cups: 0.2365882365, tasse: 0.25, tasses: 0.25, floz: 0.0295735295625 },
  speed: { 'km/h': 1, kmh: 1, 'm/s': 3.6, mph: 1.609344, kn: 1.852, noeud: 1.852, noeuds: 1.852, knot: 1.852, knots: 1.852 },
  data: { b: 1, byte: 1, bytes: 1, o: 1, octet: 1, octets: 1, kb: 1e3, ko: 1e3, mb: 1e6, mo: 1e6, gb: 1e9, go: 1e9, tb: 1e12, to: 1e12, kib: 1024, mib: 1048576, gib: 1073741824 },
  time: { s: 1, sec: 1, seconde: 1, secondes: 1, second: 1, seconds: 1, min: 60, minute: 60, minutes: 60, h: 3600, hr: 3600, heure: 3600, heures: 3600, hour: 3600, hours: 3600, j: 86400, d: 86400, jour: 86400, jours: 86400, day: 86400, days: 86400, sem: 604800, week: 604800, weeks: 604800, semaine: 604800, semaines: 604800, an: 31557600, ans: 31557600, year: 31557600, years: 31557600 },
};
const TEMP = { c: 'c', '°c': 'c', celsius: 'c', f: 'f', '°f': 'f', fahrenheit: 'f', k: 'k', kelvin: 'k' };
const CURR = { '€': 'EUR', eur: 'EUR', euro: 'EUR', euros: 'EUR', '$': 'USD', usd: 'USD', dollar: 'USD', dollars: 'USD', 'us$': 'USD', cad: 'CAD', 'ca$': 'CAD', '£': 'GBP', gbp: 'GBP', '¥': 'JPY', jpy: 'JPY', chf: 'CHF',
  aud: 'AUD', cny: 'CNY', inr: 'INR', mxn: 'MXN', brl: 'BRL', sek: 'SEK', nok: 'NOK', dkk: 'DKK', pln: 'PLN', czk: 'CZK', huf: 'HUF', krw: 'KRW', sgd: 'SGD', hkd: 'HKD', nzd: 'NZD', try: 'TRY', zar: 'ZAR', ils: 'ILS', ron: 'RON', bgn: 'BGN', isk: 'ISK', php: 'PHP', thb: 'THB', idr: 'IDR', myr: 'MYR' };
function unitKind(u) {
  u = u.toLowerCase();
  if (TEMP[u]) return ['temp', TEMP[u]];
  for (const [k, m] of Object.entries(UNITS)) if (m[u] !== undefined) return [k, u];
  return null;
}
// « 10 km en miles », « 20 °C to F », « 50 € en $ », « 100 usd in cad » → { kind, from, to, n } (null sinon)
function parseConv(q) {
  const m = /^\s*(-?[\d\s.,]*\d|[.,]\d+)\s*([^\d\s][^\s]*?|°\s?[cfk])\s+(?:in|en|to|vers|->|→|=)\s+([^\s]+)\s*$/i.exec(String(q).replace(/(\d)([€$£¥])/g, '$1 $2').replace(/([€$£¥])(\d)/g, '$2 $1'));
  if (!m) return null;
  const n = parseFloat(m[1].replace(/\s/g, '').replace(/,(?=\d{3}\b)/g, '').replace(',', '.'));
  const a = m[2].replace(/\s/g, '').toLowerCase(), b = m[3].toLowerCase();
  if (!isFinite(n)) return null;
  if (CURR[a] && CURR[b]) return { kind: 'fx', from: CURR[a], to: CURR[b], n };
  const ka = unitKind(a), kb = unitKind(b);
  if (!ka || !kb || ka[0] !== kb[0]) return null;
  return { kind: ka[0], from: ka[1], to: kb[1], n };
}
function convert(c, rates) {
  if (c.kind === 'temp') {
    const toC = { c: (v) => v, f: (v) => (v - 32) * 5 / 9, k: (v) => v - 273.15 }, fromC = { c: (v) => v, f: (v) => v * 9 / 5 + 32, k: (v) => v + 273.15 };
    return fromC[c.to](toC[c.from](c.n));
  }
  if (c.kind === 'fx') {
    if (!rates) return null;
    const r = (x) => (x === 'EUR' ? 1 : rates[x]);
    if (!r(c.from) || !r(c.to)) return null;
    return c.n / r(c.from) * r(c.to);
  }
  const t = UNITS[c.kind];
  return c.n * t[c.from] / t[c.to];
}
// nombre lisible : 1 234,5 (fr) / 1,234.5 (en), jusqu'à 10 chiffres significatifs
function fmtCalc(v, lang, frac) {
  const loc = lang === 'fr' ? 'fr-FR' : 'en-US';
  const a = Math.abs(v);
  if (a !== 0 && (a >= 1e15 || a < 1e-6)) return v.toExponential(6).replace('.', lang === 'fr' ? ',' : '.');
  if (frac !== undefined) return v.toLocaleString(loc, { maximumFractionDigits: a >= 1 ? frac : Math.max(frac, 6) });
  return v.toLocaleString(loc, { maximumFractionDigits: a >= 1 ? 6 : 10, maximumSignificantDigits: 12 });
}

/* #36 QR code généré en local (aucun service externe) : mode octets (UTF-8), correction M (L si le texte est long),
   versions 1 à 40, choix du meilleur masque. Algorithme de la norme ISO/IEC 18004. */
const QR_ECC = { L: [7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28, 30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
  M: [10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28] };
const QR_BLK = { L: [1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13, 14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25],
  M: [1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49] };
const QR_FMT = { L: 1, M: 0, Q: 3, H: 2 };
function qrRawModules(ver) {
  let r = (16 * ver + 128) * ver + 64;
  if (ver >= 2) {
    const n = Math.floor(ver / 7) + 2;
    r -= (25 * n - 10) * n - 55;
    if (ver >= 7) r -= 36;
  }
  return r;
}
const qrDataCodewords = (ver, ecl) => Math.floor(qrRawModules(ver) / 8) - QR_ECC[ecl][ver - 1] * QR_BLK[ecl][ver - 1];
function qrAlign(ver) {
  if (ver === 1) return [];
  const n = Math.floor(ver / 7) + 2, step = Math.floor((ver * 8 + n * 3 + 5) / (n * 4 - 4)) * 2, r = [6];
  for (let pos = ver * 4 + 10; r.length < n; pos -= step) r.splice(1, 0, pos);
  return r;
}
function rsMul(x, y) {
  let z = 0;
  for (let i = 7; i >= 0; i--) { z = (z << 1) ^ ((z >>> 7) * 0x11d); z ^= ((y >>> i) & 1) * x; }
  return z;
}
function rsDivisor(deg) {
  const r = new Array(deg).fill(0);
  r[deg - 1] = 1;
  let root = 1;
  for (let i = 0; i < deg; i++) {
    for (let j = 0; j < deg; j++) { r[j] = rsMul(r[j], root); if (j + 1 < deg) r[j] ^= r[j + 1]; }
    root = rsMul(root, 2);
  }
  return r;
}
function rsRemainder(data, div) {
  const r = div.map(() => 0);
  for (const b of data) {
    const f = b ^ r.shift();
    r.push(0);
    div.forEach((c, i) => { r[i] ^= rsMul(c, f); });
  }
  return r;
}
function qrFormatBits(ecl, mask) {
  const d = (QR_FMT[ecl] << 3) | mask;
  let rem = d;
  for (let i = 0; i < 10; i++) rem = (rem << 1) ^ ((rem >>> 9) * 0x537);
  return ((d << 10) | rem) ^ 0x5412;
}
function qrVersionBits(ver) {
  let rem = ver;
  for (let i = 0; i < 12; i++) rem = (rem << 1) ^ ((rem >>> 11) * 0x1f25);
  return (ver << 12) | rem;
}
/** QR d'un texte : { size, ver, ecl, mask, dark(x, y) } ou null s'il est trop long (> 2 331 octets). */
function qrEncode(text) {
  const bytes = [...new TextEncoder().encode(String(text))];
  let ver = 0, ecl = 'M';
  for (const e of ['M', 'L']) {
    for (let v = 1; v <= 40; v++) {
      if (4 + (v < 10 ? 8 : 16) + bytes.length * 8 <= qrDataCodewords(v, e) * 8) { ver = v; break; }
    }
    if (ver) { ecl = e; break; }
  }
  if (!ver) return null;
  // bits : mode octets, longueur, données, terminaison, bourrage
  const bits = [];
  const put = (val, len) => { for (let i = len - 1; i >= 0; i--) bits.push((val >>> i) & 1); };
  put(4, 4); put(bytes.length, ver < 10 ? 8 : 16); bytes.forEach((b) => put(b, 8));
  const cap = qrDataCodewords(ver, ecl) * 8;
  put(0, Math.min(4, cap - bits.length));
  put(0, (8 - bits.length % 8) % 8);
  for (let pad = 0xec; bits.length < cap; pad ^= 0xec ^ 0x11) put(pad, 8);
  const data = [];
  for (let i = 0; i < bits.length; i += 8) data.push(bits.slice(i, i + 8).reduce((a, b) => (a << 1) | b, 0));
  // blocs + correction d'erreurs, entrelacés
  const nb = QR_BLK[ecl][ver - 1], el = QR_ECC[ecl][ver - 1], raw = Math.floor(qrRawModules(ver) / 8);
  const nShort = nb - raw % nb, shortLen = Math.floor(raw / nb), div = rsDivisor(el), blocks = [];
  for (let i = 0, k = 0; i < nb; i++) {
    const d = data.slice(k, k + shortLen - el + (i < nShort ? 0 : 1));
    k += d.length;
    const ecc = rsRemainder(d, div);
    if (i < nShort) d.push(0);
    blocks.push(d.concat(ecc));
  }
  const cw = [];
  for (let i = 0; i < blocks[0].length; i++) blocks.forEach((b, j) => { if (i !== shortLen - el || j >= nShort) cw.push(b[i]); });
  // matrice : motifs fixes puis données
  const size = ver * 4 + 17, m = new Uint8Array(size * size), fn = new Uint8Array(size * size);
  const set = (x, y, d) => { m[y * size + x] = d ? 1 : 0; fn[y * size + x] = 1; };
  for (let i = 0; i < size; i++) { set(6, i, i % 2 === 0); set(i, 6, i % 2 === 0); }
  for (const [cx, cy] of [[3, 3], [size - 4, 3], [3, size - 4]]) {
    for (let dy = -4; dy <= 4; dy++) for (let dx = -4; dx <= 4; dx++) {
      const x = cx + dx, y = cy + dy, dist = Math.max(Math.abs(dx), Math.abs(dy));
      if (x >= 0 && x < size && y >= 0 && y < size) set(x, y, dist !== 2 && dist !== 4);
    }
  }
  const al = qrAlign(ver), na = al.length;
  for (let i = 0; i < na; i++) for (let j = 0; j < na; j++) {
    if ((i === 0 && j === 0) || (i === 0 && j === na - 1) || (i === na - 1 && j === 0)) continue;
    for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) set(al[i] + dx, al[j] + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1);
  }
  const drawFormat = (mask) => {
    const b = qrFormatBits(ecl, mask), bit = (i) => ((b >>> i) & 1) === 1;
    for (let i = 0; i <= 5; i++) set(8, i, bit(i));
    set(8, 7, bit(6)); set(8, 8, bit(7)); set(7, 8, bit(8));
    for (let i = 9; i < 15; i++) set(14 - i, 8, bit(i));
    for (let i = 0; i < 8; i++) set(size - 1 - i, 8, bit(i));
    for (let i = 8; i < 15; i++) set(8, size - 15 + i, bit(i));
    set(8, size - 8, true); // module toujours sombre
  };
  drawFormat(0);
  if (ver >= 7) {
    const b = qrVersionBits(ver);
    for (let i = 0; i < 18; i++) { const d = ((b >>> i) & 1) === 1, a = size - 11 + i % 3, c = Math.floor(i / 3); set(a, c, d); set(c, a, d); }
  }
  for (let i = 0, right = size - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5;
    for (let vert = 0; vert < size; vert++) for (let j = 0; j < 2; j++) {
      const x = right - j, y = ((right + 1) & 2) === 0 ? size - 1 - vert : vert;
      if (!fn[y * size + x] && i < cw.length * 8) { m[y * size + x] = (cw[i >>> 3] >>> (7 - (i & 7))) & 1; i++; }
    }
  }
  const MASKS = [(x, y) => (x + y) % 2 === 0, (x, y) => y % 2 === 0, (x) => x % 3 === 0, (x, y) => (x + y) % 3 === 0,
    (x, y) => (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0, (x, y) => (x * y) % 2 + (x * y) % 3 === 0,
    (x, y) => ((x * y) % 2 + (x * y) % 3) % 2 === 0, (x, y) => ((x + y) % 2 + (x * y) % 3) % 2 === 0];
  const applyMask = (k) => { for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) if (!fn[y * size + x] && MASKS[k](x, y)) m[y * size + x] ^= 1; };
  // pénalités de la norme : suites, carrés 2×2, motifs de repère, équilibre clair / sombre
  const penalty = () => {
    let p = 0, dark = 0;
    const line = (get) => {
      let run = 1, s = '';
      for (let i = 0; i < size; i++) {
        const v = get(i); s += v;
        if (i > 0) { if (v === get(i - 1)) { run++; if (run === 5) p += 3; else if (run > 5) p++; } else run = 1; }
      }
      for (const pat of ['10111010000', '00001011101']) for (let k = s.indexOf(pat); k >= 0; k = s.indexOf(pat, k + 1)) p += 40;
    };
    for (let y = 0; y < size; y++) line((x) => m[y * size + x]);
    for (let x = 0; x < size; x++) line((y) => m[y * size + x]);
    for (let y = 0; y < size - 1; y++) for (let x = 0; x < size - 1; x++) {
      const c = m[y * size + x];
      if (c === m[y * size + x + 1] && c === m[(y + 1) * size + x] && c === m[(y + 1) * size + x + 1]) p += 3;
    }
    for (const v of m) dark += v;
    return p + (Math.ceil(Math.abs(dark * 20 - size * size * 10) / (size * size)) - 1) * 10;
  };
  let best = 0, bestP = Infinity;
  for (let k = 0; k < 8; k++) {
    applyMask(k); drawFormat(k);
    const p = penalty();
    if (p < bestP) { bestP = p; best = k; }
    applyMask(k); // annule (XOR)
  }
  applyMask(best); drawFormat(best);
  return { size, ver, ecl, mask: best, dark: (x, y) => m[y * size + x] === 1, isFn: (x, y) => fn[y * size + x] === 1 };
}
/** Dessin vectoriel (net à toutes les tailles), marge de 4 modules. */
function qrSvg(q, px) {
  let d = '';
  for (let y = 0; y < q.size; y++) for (let x = 0; x < q.size; x++) if (q.dark(x, y)) d += `M${x + 4},${y + 4}h1v1h-1z`;
  const n = q.size + 8;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${n} ${n}" width="${px}" height="${px}" shape-rendering="crispEdges" role="img" style="fill:#000;stroke:none"><rect width="${n}" height="${n}" fill="#fff" stroke="none"/><path d="${d}" fill="#000" stroke="none"/></svg>`;
}
if (typeof module !== 'undefined') module.exports = { fuzzyScore, calcExpr, parseConv, convert, fmtCalc, qrEncode, qrSvg, rsDivisor, rsRemainder, qrFormatBits, qrVersionBits, qrRawModules, qrDataCodewords, QR_ECC, QR_BLK };
