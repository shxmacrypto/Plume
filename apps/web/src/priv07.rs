// Plume 0.7 · Confidentialité (#19–#25) : conteneurs, effacement à la fermeture, rapport de confidentialité,
// générateur et audit des mots de passe, fuites (HIBP), Windows Hello et DNS sécurisé.
use super::*;
use std::sync::mpsc;
use webview2_com::{CallDevToolsProtocolMethodCompletedHandler, ClearBrowsingDataCompletedHandler, GetCookiesCompletedHandler};

/// Site gardé (liste « Sites gardés ») : le site lui-même ou un de ses sous-domaines.
pub fn kept(list: &[String], host: &str) -> bool {
    let h = host.trim_start_matches('.').trim_start_matches("www.");
    let h = h.split(':').next().unwrap_or(h); // sans le port
    list.iter().any(|k| {
        let k = k.trim().trim_start_matches("www.");
        !k.is_empty() && (h == k || h.ends_with(&format!(".{k}")))
    })
}

/// DNS sécurisé (#25) : (mode du navigateur, adresse du serveur) ; None si désactivé ou adresse personnalisée invalide.
pub fn doh_config(c: &Value) -> Option<(&'static str, String)> {
    if c["doh"] != true {
        return None;
    }
    let t = match c["dohProv"].as_str().unwrap_or("cloudflare") {
        "quad9" => "https://dns.quad9.net/dns-query",
        "nextdns" => "https://dns.nextdns.io",
        "google" => "https://dns.google/dns-query",
        "custom" => c["dohUrl"].as_str().map(str::trim).filter(|u| u.starts_with("https://") && u.len() > 12 && !u.contains(char::is_whitespace))?,
        _ => "https://cloudflare-dns.com/dns-query",
    };
    // « secure » : toujours ce fournisseur (vérifié : 1.1.1.1/help → DoH oui). « automatic » : le moteur
    // chiffre le DNS du réseau si son fournisseur le permet et ignore le fournisseur choisi (comme Chrome).
    Some((if c["dohMode"] == "auto" { "automatic" } else { "secure" }, t.to_string()))
}

/// WebView2 ignore les drapeaux de DoH : on écrit le réglage du navigateur lui-même (« Local State »,
/// lu au démarrage du moteur), seulement s'il change. Désactivé → réglage retiré (défaut du moteur).
pub fn doh_prefs(dir: &std::path::Path, c: &Value) {
    let p = dir.join("EBWebView").join("Local State");
    let mut st: Value = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({}));
    if !st.is_object() {
        return; // fichier illisible : on n'y touche pas
    }
    let want = doh_config(c).map(|(m, t)| json!({"mode": m, "templates": t}));
    let cur = st.get("dns_over_https").cloned();
    match want {
        Some(w) if cur.as_ref() != Some(&w) => st["dns_over_https"] = w,
        None if cur.is_some() => {
            st.as_object_mut().unwrap().remove("dns_over_https");
        }
        _ => return,
    }
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    let tmp = p.with_extension("plume-tmp");
    if std::fs::write(&tmp, st.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
}

/// Pompe les messages de la fenêtre en attendant `n` réponses de WebView2 (au plus jusqu'à `deadline`).
fn pump_until(rx: &mpsc::Receiver<()>, n: usize, deadline: Instant) {
    use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage};
    let mut got = 0;
    let mut msg = MSG::default();
    while got < n && Instant::now() < deadline {
        while rx.try_recv().is_ok() {
            got += 1;
        }
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}

const STORAGE: &str = "local_storage,indexeddb,websql,service_workers,cache_storage,file_systems,shader_cache";

/// Supprime les cookies (tous, ou ceux envoyés à `uri`) sauf ceux des sites gardés. Rend true si l'appel est parti.
unsafe fn drop_cookies(w: &ICoreWebView2, uri: &str, keep: Vec<String>, tx: Option<mpsc::Sender<()>>) -> bool {
    unsafe {
        let Ok(cm) = w.cast::<ICoreWebView2_2>().and_then(|w2| w2.CookieManager()) else { return false };
        let cm2 = cm.clone();
        cm.GetCookies(
            &HSTRING::from(uri),
            &GetCookiesCompletedHandler::create(Box::new(move |_, list| {
                if let Some(l) = list {
                    let mut n = 0u32;
                    l.Count(&mut n)?;
                    for j in 0..n {
                        let ck = l.GetValueAtIndex(j)?;
                        let mut d = PWSTR::null();
                        ck.Domain(&mut d)?;
                        if !kept(&keep, &webview2_com::take_pwstr(d)) {
                            let _ = cm2.DeleteCookie(&ck);
                        }
                    }
                }
                if let Some(tx) = tx {
                    let _ = tx.send(());
                }
                Ok(())
            })),
        )
        .is_ok()
    }
}

/// Efface le stockage d'un site (DevTools : Storage.clearDataForOrigin, valable pour tout le profil).
unsafe fn drop_storage(w: &ICoreWebView2, host: &str, tx: Option<mpsc::Sender<()>>) -> usize {
    let mut n = 0;
    for scheme in ["https", "http"] {
        let p = json!({"origin": format!("{scheme}://{host}"), "storageTypes": STORAGE}).to_string();
        let tx = tx.clone();
        let ok = unsafe {
            w.CallDevToolsProtocolMethod(
                w!("Storage.clearDataForOrigin"),
                &HSTRING::from(p),
                &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |_, _| {
                    if let Some(tx) = tx {
                        let _ = tx.send(());
                    }
                    Ok(())
                })),
            )
        };
        n += ok.is_ok() as usize;
    }
    n
}

impl App {
    /* ---------- #19 Conteneurs ---------- */
    pub fn conts(&self) -> Vec<Value> {
        self.cfg["contList"].as_array().cloned().unwrap_or_default()
    }

    pub fn cont_ok(&self, id: u32) -> bool {
        id > 0 && self.cfg["containers"] == true && self.conts().iter().any(|c| c["id"] == id)
    }

    /// Site toujours ouvert dans un conteneur (« Toujours ouvrir dans un conteneur »).
    pub fn cont_site(&self, url: &str) -> Option<u32> {
        if self.cfg["containers"] != true || !url.starts_with("http") {
            return None;
        }
        let h = import::host_of(url);
        let m = self.cfg["contSites"].as_object()?;
        m.iter().find(|(k, _)| h == **k || h.ends_with(&format!(".{k}"))).and_then(|(_, v)| v.as_u64()).map(|c| c as u32).filter(|c| self.cont_ok(*c))
    }

    /// Conteneur d'un nouvel onglet : demandé (lien ouvert depuis un conteneur, « ouvrir dans… »), site associé, ou celui de l'espace.
    pub fn cont_for(&mut self, url: Option<&str>) -> u32 {
        let asked = self.open_cont.take();
        if self.cfg["containers"] != true {
            return 0;
        }
        if let Some(c) = asked {
            return if self.cont_ok(c) { c } else { 0 };
        }
        if let Some(c) = url.and_then(|u| self.cont_site(u)) {
            return c;
        }
        if self.cfg["spaces"] == true {
            let c = self.spaces.iter().find(|s| s["id"] == self.x.space).and_then(|s| s["cont"].as_u64()).unwrap_or(0) as u32;
            if self.cont_ok(c) {
                return c;
            }
        }
        0
    }

    /// Les extensions installées sont ajoutées au profil d'un conteneur à sa première ouverture de la session.
    pub fn cont_exts(&mut self, cont: u32, v: &WebView) {
        if !self.ext_on || !self.cont_ext.insert(cont) {
            return;
        }
        let dirs: Vec<String> = self.ext_meta.as_object().map(|m| m.values().filter_map(|e| e["dir"].as_str().map(String::from)).collect()).unwrap_or_default();
        unsafe {
            let Some(p7) = v.webview().cast::<ICoreWebView2_13>().ok().and_then(|w| w.Profile().ok()).and_then(|p| p.cast::<ICoreWebView2Profile7>().ok())
            else {
                return;
            };
            for d in dirs {
                let _ = p7.AddBrowserExtension(&HSTRING::from(d), &ProfileAddBrowserExtensionCompletedHandler::create(Box::new(|_, _| Ok(()))));
            }
        }
    }

    /// Données des conteneurs supprimés : effacées au démarrage suivant, avant que WebView2 n'ouvre les profils.
    /// WebView2 range le profil « plume-c<n> » dans EBWebView\WV2Profile_plume-c<n>.
    pub fn cont_gc(dir: &std::path::Path, cfg: &Value) {
        let ids: Vec<String> =
            cfg["contList"].as_array().map(|l| l.iter().filter_map(|c| c["id"].as_u64()).map(|i| format!("plume-c{i}")).collect()).unwrap_or_default();
        let Ok(rd) = std::fs::read_dir(dir.join("EBWebView")) else { return };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            let p = n.strip_prefix("WV2Profile_").unwrap_or(&n);
            if p.starts_with("plume-c") && !ids.iter().any(|i| i == p) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }

    /// Rouvre l'onglet dans un autre conteneur (0 = sans conteneur) à la même place.
    pub fn reopen_in(&mut self, i: usize, cont: u32) {
        let Some(t) = self.tabs.get(i) else { return };
        if t.private || t.cont == cont {
            return;
        }
        let (u, pinned, group, oid) = (t.url.clone(), t.pinned, t.group, t.id);
        if u.is_empty() {
            // onglet vide : pas encore de page, on change simplement de conteneur
            self.tabs[i].cont = cont;
            self.push();
            return;
        }
        if i != self.active {
            self.activate(i);
        }
        self.open_cont = Some(cont);
        self.new_tab(Some(u), true, false);
        let j = self.active;
        self.tabs[j].pinned = pinned;
        self.tabs[j].group = group;
        if let Some(k) = self.idx(oid) {
            self.close_tab(k);
        }
        self.save();
        self.push();
    }

    /* ---------- #20 Effacer à la fermeture ---------- */
    /// Mémorise les sites visités (par conteneur) quand l'effacement est activé : leur stockage sera vidé.
    pub fn note_visit(&mut self, cont: u32, url: &str) {
        if self.cfg["clearExit"] == true && self.cfg["clrStorage"] == true && url.starts_with("http") {
            self.visited.insert((cont, import::host_of(url)));
        }
    }

    /// Une WebView vivante par profil : l'interface pour le profil par défaut, un onglet pour chaque conteneur.
    fn profile_views(&self) -> Vec<(u32, ICoreWebView2)> {
        let mut out = vec![(0, self.ui.webview())];
        let all = self.tabs.iter().chain(self.others.iter().flat_map(|w| w.tabs.iter())).chain(self.x.stash.iter().flat_map(|s| s.1.iter()));
        for t in all {
            if t.cont > 0
                && !t.private
                && !out.iter().any(|(c, _)| *c == t.cont)
                && let Some(v) = &t.view
            {
                out.push((t.cont, v.webview()));
            }
        }
        out
    }

    pub fn clear_on_exit(&mut self) {
        let c = &self.cfg;
        if c["clearExit"] != true {
            return;
        }
        let keep = strs(&c["clearKeep"]);
        let (ck, ca, hi, st) = (c["clrCookies"] == true, c["clrCache"] == true, c["clrHist"] == true, c["clrStorage"] == true);
        self.window.set_visible(false); // la fenêtre disparaît tout de suite
        if hi {
            self.history.retain(|h| kept(&keep, &import::host_of(h["u"].as_str().unwrap_or(""))));
            self.closed.clear();
        }
        // sites encore ouverts
        let open: Vec<(u32, String)> = self
            .tabs
            .iter()
            .chain(self.others.iter().flat_map(|w| w.tabs.iter()))
            .chain(self.x.stash.iter().flat_map(|s| s.1.iter()))
            .filter(|t| !t.private && t.url.starts_with("http"))
            .map(|t| (t.cont, import::host_of(&t.url)))
            .collect();
        let mut sites: Vec<(u32, String)> = self.visited.iter().cloned().chain(open).filter(|(_, h)| !h.is_empty() && !kept(&keep, h)).collect();
        sites.sort();
        sites.dedup();
        let (tx, rx) = mpsc::channel();
        let mut n = 0;
        for (cont, w) in self.profile_views() {
            unsafe {
                if ck && drop_cookies(&w, "", keep.clone(), Some(tx.clone())) {
                    n += 1;
                }
                if st {
                    for (_, h) in sites.iter().filter(|(c, _)| *c == cont) {
                        n += drop_storage(&w, h, Some(tx.clone()));
                    }
                }
                // cache (pas de détail par site) et historique interne de WebView2
                let mut kinds = 0;
                if ca {
                    kinds |= COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE.0;
                }
                if hi && keep.is_empty() {
                    kinds |= COREWEBVIEW2_BROWSING_DATA_KINDS_BROWSING_HISTORY.0;
                }
                if kinds != 0 {
                    let p2 = w.cast::<ICoreWebView2_13>().ok().and_then(|w| w.Profile().ok()).and_then(|p| p.cast::<ICoreWebView2Profile2>().ok());
                    if let Some(p2) = p2 {
                        let tx = tx.clone();
                        let r = p2.ClearBrowsingData(
                            COREWEBVIEW2_BROWSING_DATA_KINDS(kinds),
                            &ClearBrowsingDataCompletedHandler::create(Box::new(move |_| {
                                let _ = tx.send(());
                                Ok(())
                            })),
                        );
                        n += r.is_ok() as usize;
                    }
                }
            }
        }
        pump_until(&rx, n, Instant::now() + Duration::from_millis(2500));
    }

    /// Mode « dernier onglet du site » : le site est oublié dès que son dernier onglet se ferme.
    pub fn clear_if_last(&mut self, t: &Tab) {
        let c = &self.cfg;
        if c["clearExit"] != true || c["clearWhen"] != "lasttab" || t.private || !t.url.starts_with("http") {
            return;
        }
        let keep = strs(&c["clearKeep"]);
        let h = import::host_of(&t.url);
        if kept(&keep, &h) {
            return;
        }
        let same = |x: &Tab| x.cont == t.cont && import::host_of(&x.url) == h;
        if self.tabs.iter().any(same) || self.others.iter().any(|w| w.tabs.iter().any(same)) || self.x.stash.iter().any(|s| s.1.iter().any(same)) {
            return;
        }
        let w = self.profile_views().into_iter().find(|(c, _)| *c == t.cont).map(|x| x.1).or_else(|| t.view.as_ref().map(|v| v.webview()));
        let Some(w) = w else { return };
        unsafe {
            if c["clrCookies"] == true {
                drop_cookies(&w, &format!("https://{h}/"), keep.clone(), None);
                drop_cookies(&w, &format!("http://{h}/"), keep, None);
            }
            if c["clrStorage"] == true {
                drop_storage(&w, &h, None);
            }
        }
        self.visited.remove(&(t.cont, h));
    }

    /* ---------- #21 Rapport de confidentialité ---------- */
    pub fn priv_report(&mut self) {
        let i = self.active;
        if self.cfg["privReport"] == true && self.tabs[i].url.starts_with("http") {
            self.eval_tab(i, "window.__plume&&__plume.report&&__plume.report()");
        }
    }

    /// Réponse de la page (domaines tiers, permissions) + traqueurs bloqués + nombre de cookies du site.
    pub fn priv_page(&mut self, i: usize, v: &Value) {
        let t = &self.tabs[i];
        let mut bl = t.bl.clone();
        bl.sort_by_key(|site| std::cmp::Reverse(site.1));
        let mut rep = json!({"id": t.id, "u": t.url, "https": t.url.starts_with("https://"), "blocked": t.blocked,
                             "bl": bl.iter().map(|(h, n)| json!([h, n])).collect::<Vec<_>>(), "third": v["third"], "perms": v["perms"], "cookies": Value::Null});
        let cm = t.view.as_ref().and_then(|w| unsafe { w.webview().cast::<ICoreWebView2_2>().ok()?.CookieManager().ok() });
        let Some(cm) = cm else { return self.js(&format!("privRep({rep})")) };
        let p = self.proxy.clone();
        unsafe {
            let _ = cm.GetCookies(
                &HSTRING::from(t.url.as_str()),
                &GetCookiesCompletedHandler::create(Box::new(move |_, list| {
                    let mut n = 0u32;
                    if let Some(l) = list {
                        l.Count(&mut n)?;
                    }
                    rep["cookies"] = json!(n);
                    let _ = p.send_event(Ev::Do(Box::new(move |app: &mut App| app.js(&format!("privRep({rep})")))));
                    Ok(())
                })),
            );
        }
    }

    /* ---------- #22 Générateur et audit, #23 fuites ---------- */
    pub fn pw_generate(&self) -> String {
        let c = &self.cfg;
        pwgen::generate(c["pwLen"].as_u64().unwrap_or(20) as usize, c["pwSym"] != false, c["pwDigits"] != false, c["pwPhrase"] == true)
    }

    /// Formulaire d'inscription repéré dans la page : barre « Utiliser un mot de passe solide ».
    pub fn pw_offer(&mut self, i: usize) {
        if self.cfg["pwgen"] != true || self.cfg["pwOffer"] != true || self.tabs[i].private || self.tabs[i].info.is_some() {
            return;
        }
        let pw = self.pw_generate();
        self.tabs[i].info = Some(json!({"k": "gen", "pw": pw}));
        if i == self.active {
            self.push();
        }
    }

    pub fn pw_audit(&self) {
        if !self.vault.unlocked() {
            return;
        }
        let a = pwgen::audit(&self.vault.items);
        let item = |k: &Value| {
            let k = k.as_u64().unwrap_or(0) as usize;
            let x = &self.vault.items[k];
            json!({"i": k, "o": x["o"], "u": x["u"]})
        };
        let weak: Vec<Value> = a["weak"].as_array().map(|l| l.iter().map(item).collect()).unwrap_or_default();
        let reused: Vec<Vec<Value>> = a["reused"]
            .as_array()
            .map(|g| g.iter().map(|l| l.as_array().map(|l| l.iter().map(item).collect()).unwrap_or_default()).collect())
            .unwrap_or_default();
        self.js(&format!("pwAuditRes({})", json!({"weak": weak, "reused": reused, "n": a["n"]})));
    }

    /// Vérifie tout le coffre auprès de Have I Been Pwned (k-anonymat), hors du fil de l'interface.
    pub fn pw_hibp_all(&self) {
        if self.cfg["hibp"] != true || !self.vault.unlocked() {
            return;
        }
        let pws: Vec<String> = self.vault.items.iter().map(|x| str_of(&x["p"])).collect();
        let p = self.proxy.clone();
        std::thread::spawn(move || {
            let mut seen: Vec<(String, std::result::Result<u64, String>)> = vec![];
            let mut out = vec![];
            for (i, pw) in pws.iter().enumerate() {
                if pw.is_empty() {
                    continue;
                }
                let r = match seen.iter().find(|(q, _)| q == pw) {
                    Some((_, r)) => r.clone(),
                    None => {
                        let r = pwgen::hibp_check(pw);
                        seen.push((pw.clone(), r.clone()));
                        r
                    }
                };
                out.push(match r {
                    Ok(n) => json!({"i": i, "n": n}),
                    Err(_) => json!({"i": i, "err": true}),
                });
            }
            let res = json!(out);
            let _ = p.send_event(Ev::Do(Box::new(move |app: &mut App| app.js(&format!("pwBreach({res})")))));
        });
    }

    /// Vérification automatique d'un mot de passe qu'on vient d'enregistrer.
    pub fn pw_hibp_one(&self, pw: &str) {
        if self.cfg["hibp"] != true || self.cfg["hibpAuto"] != true || pw.is_empty() {
            return;
        }
        let pw = pw.to_string();
        let p = self.proxy.clone();
        std::thread::spawn(move || {
            if let Ok(n) = pwgen::hibp_check(&pw)
                && n > 0
            {
                let _ = p.send_event(Ev::Do(Box::new(move |app: &mut App| app.js(&format!("toast('pw-breached',{{n:{n}}})")))));
            }
        });
    }

    /* ---------- #24 Windows Hello ---------- */
    pub fn hello_ready(&self) -> bool {
        self.cfg["hello"] == true && self.vault.hello_path().exists()
    }

    /// Réglage activé : protège une copie de la clé du coffre avec Windows Hello (coffre ouvert nécessaire).
    pub fn hello_toggle(&mut self) {
        let path = self.vault.hello_path();
        if self.cfg["hello"] != true {
            let _ = std::fs::remove_file(&path);
            self.push();
            return;
        }
        let Some(key) = self.vault.key() else {
            self.cfg["hello"] = json!(false);
            self.toast(if self.vault.exists() { "hello-unlock" } else { "hello-novault" });
            self.save();
            self.push();
            return;
        };
        let p = self.proxy.clone();
        std::thread::spawn(move || {
            let r = if hello::supported() { hello::enable(&path, key) } else { Err("unsupported".into()) };
            let _ = p.send_event(Ev::Do(Box::new(move |app: &mut App| {
                match r {
                    Ok(()) => app.toast("hello-on"),
                    Err(e) => {
                        app.cfg["hello"] = json!(false);
                        app.toast(if e == "unsupported" {
                            "hello-unsupported"
                        } else if e == "canceled" {
                            "hello-canceled"
                        } else {
                            "hello-failed"
                        });
                        app.save();
                    }
                }
                app.push();
            })));
        });
    }

    /// Déverrouillage du coffre par Windows Hello (PIN, empreinte, visage).
    pub fn hello_unlock(&mut self) {
        if !self.hello_ready() || self.vault.unlocked() || self.hello_busy {
            return;
        }
        self.hello_busy = true;
        let path = self.vault.hello_path();
        let p = self.proxy.clone();
        std::thread::spawn(move || {
            let r = hello::unlock(&path);
            let _ = p.send_event(Ev::Do(Box::new(move |app: &mut App| {
                app.hello_busy = false;
                let r = r.and_then(|k| app.vault.unlock_with_key(k));
                match &r {
                    Ok(()) => app.after_unlock(),
                    Err(e) if e == "canceled" => {}
                    Err(_) => app.toast("hello-failed"),
                }
                app.js(&format!("vaultRes({})", json!({"ok": r.is_ok(), "err": r.err(), "hello": true})));
                app.push_vault();
                app.push();
            })));
        });
    }

    /// Mode « avant chaque remplissage » : Windows Hello confirme la présence avant de remplir. true = remplissage différé.
    pub fn hello_gate(&mut self, i: usize, item: &Value) -> bool {
        if self.cfg["hello"] != true || self.cfg["helloEach"] != "each" || !self.hello_ready() {
            return false;
        }
        let (id, item) = (self.tabs[i].id, item.clone());
        let hwnd = self.window.hwnd();
        let msg = if self.lang() == "fr" { "Plume : remplir le mot de passe" } else { "Plume: fill in the password" };
        let p = self.proxy.clone();
        std::thread::spawn(move || {
            let ok = hello::verify(HWND(hwnd as _), msg);
            let _ = p.send_event(Ev::Do(Box::new(move |app: &mut App| {
                if ok && let Some(i) = app.idx(id) {
                    app.fill_now(i, &item);
                }
            })));
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_list_matches_subdomains() {
        let k = vec!["example.com".to_string(), "www.bank.fr".to_string()];
        assert!(kept(&k, "example.com") && kept(&k, ".example.com") && kept(&k, "mail.example.com") && kept(&k, "bank.fr"));
        assert!(kept(&["localhost".to_string()], "localhost:4820"));
        assert!(!kept(&k, "notexample.com") && !kept(&k, "example.org") && !kept(&[], "example.com"));
    }

    #[test]
    fn deleted_container_profiles_are_removed() {
        let d = std::env::temp_dir().join(format!("plume-gc-{}", std::process::id()));
        for n in ["Default", "WV2Profile_plume-c1", "WV2Profile_plume-c2", "WV2Profile_other"] {
            std::fs::create_dir_all(d.join("EBWebView").join(n)).unwrap();
        }
        App::cont_gc(&d, &json!({"contList": [{"id": 1, "name": "Work"}]}));
        let mut left: Vec<String> = std::fs::read_dir(d.join("EBWebView")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        left.sort();
        assert_eq!(left, ["Default", "WV2Profile_other", "WV2Profile_plume-c1"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn doh_config_and_local_state() {
        let mut c = json!({"doh": true, "dohProv": "cloudflare", "dohMode": "strict", "dohUrl": ""});
        assert_eq!(doh_config(&c), Some(("secure", "https://cloudflare-dns.com/dns-query".to_string())));
        c["dohMode"] = json!("auto");
        assert_eq!(doh_config(&c).unwrap().0, "automatic");
        c["dohMode"] = json!("strict");
        c["dohProv"] = json!("custom");
        assert!(doh_config(&c).is_none(), "adresse personnalisée vide");
        c["dohUrl"] = json!("http://dns.example/dns-query");
        assert!(doh_config(&c).is_none(), "pas de HTTPS");
        c["dohUrl"] = json!("https://dns.example/dns-query{?dns}");
        assert_eq!(doh_config(&c).unwrap().1, "https://dns.example/dns-query{?dns}");

        // « Local State » : le réglage est ajouté sans toucher au reste, puis retiré quand on désactive
        let d = std::env::temp_dir().join(format!("plume-doh-{}", std::process::id()));
        std::fs::create_dir_all(d.join("EBWebView")).unwrap();
        let ls = d.join("EBWebView").join("Local State");
        std::fs::write(&ls, r#"{"browser":{"x":1}}"#).unwrap();
        doh_prefs(&d, &c);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&ls).unwrap()).unwrap();
        assert_eq!(v["browser"]["x"], 1);
        assert_eq!(v["dns_over_https"], json!({"mode": "secure", "templates": "https://dns.example/dns-query{?dns}"}));
        c["doh"] = json!(false);
        doh_prefs(&d, &c);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&ls).unwrap()).unwrap();
        assert!(v.get("dns_over_https").is_none() && v["browser"]["x"] == 1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
