// Tests des fonctions pures de l'interface 0.7 (node --test tests/)
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { fuzzyScore, calcExpr, parseConv, convert, fmtCalc } = require('../src/ui/pure07.js');

test('#10 recherche floue', () => {
  assert.ok(fuzzyScore('pgb', 'PAGE-B http://127.0.0.1/b') > 0);
  assert.equal(fuzzyScore('xyz', 'PAGE-B'), 0);
  assert.ok(fuzzyScore('git', 'GitHub — Plume') > fuzzyScore('git', 'Le digital'));
  assert.ok(fuzzyScore('yt', 'YouTube') > 0);
  assert.ok(fuzzyScore('éte', 'Été en France') > 0); // accents ignorés
  assert.equal(fuzzyScore('', 'tout'), 1);
});

test('#16 calculs', () => {
  assert.equal(calcExpr('12*7'), 84);
  assert.equal(calcExpr('2+3*4'), 14);
  assert.equal(calcExpr('(2+3)*4'), 20);
  assert.equal(calcExpr('2^10'), 1024);
  assert.equal(calcExpr('2^3^2'), 512); // puissance associative à droite
  assert.equal(calcExpr('50%'), 0.5);
  assert.equal(calcExpr('1,5*2'), 3); // virgule décimale
  assert.equal(calcExpr('-3+5'), 2);
  assert.equal(calcExpr('sqrt(16)+abs(-2)'), 6);
  assert.ok(Math.abs(calcExpr('2*pi') - 2 * Math.PI) < 1e-12);
  assert.equal(calcExpr('10÷4'), 2.5);
  assert.equal(calcExpr('42'), null); // un nombre seul n'est pas un calcul
  assert.equal(calcExpr('hello'), null);
  assert.equal(calcExpr('2+'), null);
  assert.equal(calcExpr('alert(1)'), null);
  assert.equal(calcExpr('1/0'), null);
});

test('#16 conversions', () => {
  const near = (a, b) => assert.ok(Math.abs(a - b) < 1e-6, `${a} ≈ ${b}`);
  near(convert(parseConv('10 km in miles'), null), 6.2137119224);
  near(convert(parseConv('10 km en miles'), null), 6.2137119224);
  near(convert(parseConv('20 °C en F'), null), 68);
  near(convert(parseConv('100 F to C'), null), 37.7777777778);
  near(convert(parseConv('5 ft to m'), null), 1.524);
  near(convert(parseConv('3 lb in kg'), null), 1.36077711);
  near(convert(parseConv('1 gb in mb'), null), 1000);
  near(convert(parseConv('2 h en min'), null), 120);
  assert.equal(parseConv('10 km in kg'), null); // grandeurs différentes
  assert.equal(parseConv('bonjour en anglais'), null);
});

test('#16 devises (taux BCE, base EUR)', () => {
  const rates = { USD: 1.1, CAD: 1.5 };
  const c = parseConv('50 € en $');
  assert.deepEqual([c.kind, c.from, c.to, c.n], ['fx', 'EUR', 'USD', 50]);
  assert.ok(Math.abs(convert(c, rates) - 55) < 1e-9);
  assert.ok(Math.abs(convert(parseConv('110 usd to cad'), rates) - 150) < 1e-9);
  assert.equal(convert(parseConv('50 eur in usd'), null), null); // pas de taux = pas de résultat
});

test('#16 format des nombres', () => {
  assert.equal(fmtCalc(1234.5, 'en'), '1,234.5');
  assert.equal(fmtCalc(1234.5, 'fr').replace(/\s/g, ' '), '1 234,5');
  assert.equal(fmtCalc(84, 'en'), '84');
});

/* #36 QR code : valeurs de référence de la norme + relecture complète de la matrice produite */
const { qrEncode, qrSvg, rsDivisor, rsRemainder, qrFormatBits, qrVersionBits, qrRawModules, qrDataCodewords, QR_ECC, QR_BLK } = require('../src/ui/pure07.js');

test('#36 QR : Reed-Solomon, informations de format et de version', () => {
  // « HELLO WORLD » en 1-Q (exemple classique) : 13 octets de données → 13 octets de correction
  const data = [32, 91, 11, 120, 209, 114, 220, 77, 67, 64, 236, 17, 236];
  assert.deepEqual(rsRemainder(data, rsDivisor(13)), [168, 72, 22, 82, 217, 54, 156, 0, 46, 15, 180, 122, 16]);
  assert.equal(qrFormatBits('M', 0), 0b101010000010010);
  assert.equal(qrFormatBits('L', 0), 0b111011111000100);
  assert.equal(qrVersionBits(7), 0b000111110010010100);
  assert.equal(qrRawModules(1) / 8, 26);
  assert.equal(qrDataCodewords(1, 'M'), 16);
  assert.equal(qrDataCodewords(10, 'M'), 216);
  assert.equal(qrDataCodewords(40, 'L'), 2956);
});

// lecteur indépendant : zigzag, démasquage, désentrelacement, mode octets
const MASKS_T = [(x, y) => (x + y) % 2 === 0, (x, y) => y % 2 === 0, (x) => x % 3 === 0, (x, y) => (x + y) % 3 === 0,
  (x, y) => (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0, (x, y) => (x * y) % 2 + (x * y) % 3 === 0,
  (x, y) => ((x * y) % 2 + (x * y) % 3) % 2 === 0, (x, y) => ((x + y) % 2 + (x * y) % 3) % 2 === 0];
function qrRead(q) {
  const n = q.size, bits = [];
  let fmt = 0;
  [[8, 0], [8, 1], [8, 2], [8, 3], [8, 4], [8, 5], [8, 7], [8, 8], [7, 8], [5, 8], [4, 8], [3, 8], [2, 8], [1, 8], [0, 8]].forEach(([x, y], i) => { if (q.dark(x, y)) fmt |= 1 << i; });
  assert.equal(fmt, qrFormatBits(q.ecl, q.mask), 'format lu = format attendu');
  for (let right = n - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5;
    for (let vert = 0; vert < n; vert++) for (let j = 0; j < 2; j++) {
      const x = right - j, y = ((right + 1) & 2) === 0 ? n - 1 - vert : vert;
      if (!q.isFn(x, y)) bits.push((q.dark(x, y) ? 1 : 0) ^ (MASKS_T[q.mask](x, y) ? 1 : 0));
    }
  }
  const cw = [];
  for (let i = 0; i + 8 <= bits.length; i += 8) cw.push(parseInt(bits.slice(i, i + 8).join(''), 2));
  const nb = QR_BLK[q.ecl][q.ver - 1], el = QR_ECC[q.ecl][q.ver - 1], raw = Math.floor(qrRawModules(q.ver) / 8);
  const nShort = nb - raw % nb, shortData = Math.floor(raw / nb) - el, blocks = Array.from({ length: nb }, () => []);
  let k = 0;
  for (let i = 0; i <= shortData; i++) for (let b = 0; b < nb; b++) if (i < shortData || b >= nShort) blocks[b].push(cw[k++]);
  // vérifie aussi la correction d'erreurs de chaque bloc
  const ecc = Array.from({ length: nb }, () => []);
  for (let i = 0; i < el; i++) for (let b = 0; b < nb; b++) ecc[b].push(cw[k++]);
  blocks.forEach((d, b) => assert.deepEqual(rsRemainder(d, rsDivisor(el)), ecc[b], 'correction du bloc ' + b));
  const data = blocks.flat(), dbits = data.flatMap((v) => [7, 6, 5, 4, 3, 2, 1, 0].map((s) => (v >>> s) & 1));
  const num = (p, l) => parseInt(dbits.slice(p, p + l).join(''), 2);
  assert.equal(num(0, 4), 4, 'mode octets');
  const lenBits = q.ver < 10 ? 8 : 16, len = num(4, lenBits);
  return new TextDecoder().decode(Uint8Array.from({ length: len }, (_, i) => num(4 + lenBits + 8 * i, 8)));
}

test('#36 QR : chaque texte se relit à l\'identique', () => {
  for (const s of ['https://example.com', 'Plume', 'https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PL123', 'é😀 ' + 'x'.repeat(300), 'https://github.com/shxmacrypto/Plume/releases/download/v0.6.1/PlumeSetup.exe?' + 'a=1&'.repeat(200)]) {
    const q = qrEncode(s);
    assert.ok(q, 'encodé : ' + s.slice(0, 30));
    assert.equal(q.size, q.ver * 4 + 17);
    assert.equal(qrRead(q), s);
    // repères des trois coins : 7×7 avec un centre plein
    for (const [cx, cy] of [[3, 3], [q.size - 4, 3], [3, q.size - 4]]) { assert.ok(q.dark(cx, cy) && q.dark(cx - 3, cy - 3) && !q.dark(cx - 2, cy)); }
  }
  assert.equal(qrEncode('https://example.com').ver, 2);
  assert.equal(qrEncode('x'.repeat(3000)), null);
  assert.ok(qrSvg(qrEncode('a'), 200).startsWith('<svg'));
});
