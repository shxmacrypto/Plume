/* ---------- Démarrage ---------- */
$('reload').innerHTML = I.reload;
applyLang();
const U0 = window.U;
window.U = (s) => { U0(s); if (!splashDone) setTimeout(endSplash, Math.max(0, 850 - (performance.now() - t0))); };
setTimeout(endSplash, 3000); // sécurité
send('ov', { v: true }); // cache la page restaurée derrière l'écran de démarrage
sendOvr();
send('ready');
