// Plume 0.7 · Onglets : espaces de travail (#7), fermeture des onglets inactifs + archive (#8),
// écran partagé (#9), recherche dans les onglets (#10), coup d'œil (#11), sessions (#12), notes (#13),
// raccourcis personnalisables (palette, recherche, écran partagé, zen).
use super::*;

/// État propre à chaque fenêtre (échangé avec les autres fenêtres comme le reste de App).
#[derive(Default)]
pub struct WinX {
    /// espace affiché dans cette fenêtre (0 = Principal)
    pub space: u32,
    /// onglets des autres espaces : (espace, onglets, onglet actif)
    pub stash: Vec<(u32, Vec<Tab>, usize)>,
    pub split: Option<Split>,
    pub peek: Option<WebView>,
    pub peek_url: String,
    /// mode zen (#30) : barre et onglets cachés, ils réapparaissent au survol du haut
    pub zen: bool,
    /// une vidéo est en plein écran ; la fenêtre l'était-elle déjà avant (F11) ?
    pub page_fs: bool,
    pub fs_before: bool,
}

/// Écran partagé : `a` à gauche (ou en haut), `b` à droite (ou en bas).
#[derive(Clone, Copy)]
pub struct Split {
    pub a: u32,
    pub b: u32,
    pub ratio: f64,
}

/// Raccourcis personnalisés (réglages *Key), consultés avant les raccourcis fixes.
pub static KEYS: RwLock<Vec<(u32, bool, bool, bool, Cmd)>> = RwLock::new(Vec::new());

/// « Ctrl+Shift+A » → (touche virtuelle, Ctrl, Maj, Alt).
pub fn parse_combo(s: &str) -> Option<(u32, bool, bool, bool)> {
    let (mut c, mut sh, mut al, mut vk) = (false, false, false, 0u32);
    for p in s.split('+').map(str::trim) {
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => c = true,
            "shift" | "maj" => sh = true,
            "alt" => al = true,
            k if k.len() == 1 => vk = k.to_ascii_uppercase().as_bytes()[0] as u32,
            k if k.starts_with('f') && k[1..].parse::<u32>().is_ok_and(|n| (1..=24).contains(&n)) => vk = 0x6F + k[1..].parse::<u32>().ok()?,
            "escape" | "esc" => vk = 0x1B,
            "space" | " " => vk = 0x20,
            "enter" => vk = 0x0D,
            "tab" => vk = 0x09,
            "arrowleft" => vk = 0x25,
            "arrowup" => vk = 0x26,
            "arrowright" => vk = 0x27,
            "arrowdown" => vk = 0x28,
            _ => return None,
        }
    }
    (vk != 0).then_some((vk, c, sh, al))
}

pub fn custom_accel(vk: u32, ctrl: bool, shift: bool, alt: bool) -> Option<Cmd> {
    KEYS.read().ok()?.iter().find(|k| k.0 == vk && k.1 == ctrl && k.2 == shift && k.3 == alt).map(|k| k.4)
}

fn ms_of(period: &str) -> u64 {
    match period {
        "12h" => 12 * 3600_000,
        "1d" => 86_400_000,
        "3d" => 3 * 86_400_000,
        "30d" => 30 * 86_400_000,
        _ => 7 * 86_400_000,
    }
}

/// Dossier des données de Plume (profil).
pub fn data_file(name: &str) -> PathBuf {
    setup::data_dir().join(name)
}

pub fn load_json(name: &str, def: Value) -> Value {
    std::fs::read_to_string(data_file(name)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(def)
}

pub fn save_json(name: &str, v: &Value) {
    let f = data_file(name);
    let tmp = f.with_extension("tmp");
    if std::fs::write(&tmp, v.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, f);
    }
}

/// Boîte de dialogue « Enregistrer » / « Ouvrir » pour un fichier JSON (export / import).
pub fn pick_json(owner: HWND, save: bool, name: &str) -> Option<PathBuf> {
    let mut buf = [0u16; 1024];
    for (i, c) in name.encode_utf16().take(1000).enumerate() {
        buf[i] = c;
    }
    let filter: Vec<u16> = "JSON (*.json)\0*.json\0\0".encode_utf16().collect();
    let ext: Vec<u16> = "json\0".encode_utf16().collect();
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        lpstrDefExt: PCWSTR(ext.as_ptr()),
        Flags: if save { OFN_OVERWRITEPROMPT | OFN_NOCHANGEDIR } else { OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR },
        ..Default::default()
    };
    let ok = unsafe { if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) } };
    ok.as_bool().then(|| {
        let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
        PathBuf::from(String::from_utf16_lossy(&buf[..n]))
    })
}

impl App {
    /* ---------- Raccourcis personnalisés ---------- */
    pub fn sync_keys(&self) {
        let c = &self.cfg;
        let mut k = vec![];
        for (on, key, cmd) in [("palette", "palKey", Cmd::Palette), ("tabSearch", "tsKey", Cmd::TabSearch), ("split", "splitKey", Cmd::Split)] {
            if c[on] == true && let Some((vk, a, b, d)) = parse_combo(c[key].as_str().unwrap_or("")) {
                k.push((vk, a, b, d, cmd));
            }
        }
        if c["zen"] != "off" && let Some((vk, a, b, d)) = parse_combo(c["zenKey"].as_str().unwrap_or("")) {
            k.push((vk, a, b, d, Cmd::Zen));
        }
        *KEYS.write().unwrap() = k;
    }

    /* ---------- #7 Espaces de travail ---------- */
    pub fn spaces_json(&self) -> Value {
        let mut l = self.spaces.clone();
        if !l.iter().any(|s| s["id"] == 0) {
            l.insert(0, json!({"id": 0, "name": "", "color": "blue", "icon": "home"}));
        }
        json!(l)
    }

    /// Change l'espace affiché dans la fenêtre courante : ses onglets remplacent ceux de l'espace quitté.
    pub fn switch_space(&mut self, sp: u32) {
        if sp == self.x.space || self.cfg["spaces"] != true {
            return;
        }
        self.unsplit();
        self.close_peek();
        let unload = self.cfg["spacesUnload"] == true;
        let mut cur = std::mem::take(&mut self.tabs);
        for t in cur.iter_mut() {
            if let Some(v) = &t.view {
                let _ = v.set_visible(false);
            }
            // un onglet qui télécharge garde sa page (sinon on ne suit plus le téléchargement)
            if unload && !self.dl_running(t.id) {
                t.view = None;
                t.ram = 0;
            }
        }
        let prev = self.x.space;
        self.x.stash.push((prev, cur, self.active));
        let (tabs, active) = match self.x.stash.iter().position(|s| s.0 == sp) {
            Some(i) => {
                let s = self.x.stash.remove(i);
                (s.1, s.2)
            }
            None => (vec![], 0),
        };
        self.x.space = sp;
        self.tabs = tabs;
        self.active = usize::MAX;
        if self.tabs.is_empty() {
            let p = self.private_win;
            self.new_tab(None, true, p);
        } else {
            self.activate(active.min(self.tabs.len() - 1));
        }
        self.save();
        self.push();
    }

    pub fn space_nth(&mut self, n: usize) {
        if let Some(id) = self.spaces_json().as_array().and_then(|l| l.get(n)).and_then(|s| s["id"].as_u64()) {
            self.switch_space(id as u32);
        }
    }

    /* ---------- #8 Fermeture des onglets inactifs → archive ---------- */
    pub fn auto_close(&mut self) {
        let now = now_ms();
        let keep_days = self.cfg["archiveKeep"].as_u64().unwrap_or(30);
        if keep_days > 0 {
            let before = self.archive.len();
            self.archive.retain(|a| a["closed"].as_u64().unwrap_or(now) + keep_days * 86_400_000 > now);
            if self.archive.len() != before {
                save_json("archive.json", &json!(self.archive));
            }
        }
        if self.cfg["autoClose"] != true {
            return;
        }
        let limit = ms_of(self.cfg["autoCloseAfter"].as_str().unwrap_or("7d"));
        let skip = self.cfg["autoCloseSkip"] != false;
        let split = self.x.split.map(|s| [s.a, s.b]).unwrap_or_default();
        // jamais un onglet dont un téléchargement est en cours
        let old = |app: &App, t: &Tab| {
            !t.url.is_empty() && !t.private && t.lv + limit < now && !split.contains(&t.id) && !(skip && (t.pinned || t.group > 0 || app.protected(t)))
                && !app.dl_running(t.id)
        };
        let mut gone: Vec<Value> = vec![];
        let ids: Vec<u32> = self.tabs.iter().enumerate().filter(|(i, t)| *i != self.active && old(self, t)).map(|(_, t)| t.id).collect();
        for id in ids {
            if let Some(i) = self.idx(id) {
                let t = &self.tabs[i];
                gone.push(json!({"u": t.url, "t": t.title, "f": t.fav, "closed": now}));
                self.close_tab(i);
            }
        }
        let mut stash = std::mem::take(&mut self.x.stash);
        for s in stash.iter_mut() {
            let (keep, out): (Vec<Tab>, Vec<Tab>) = std::mem::take(&mut s.1).into_iter().partition(|t| !old(self, t));
            gone.extend(out.iter().map(|t| json!({"u": t.url, "t": t.title, "f": t.fav, "closed": now})));
            s.2 = s.2.min(keep.len().saturating_sub(1));
            s.1 = keep;
        }
        stash.retain(|s| !s.1.is_empty());
        self.x.stash = stash;
        if !gone.is_empty() {
            for g in gone.into_iter().rev() {
                self.archive.insert(0, g);
            }
            self.archive.truncate(2000);
            save_json("archive.json", &json!(self.archive));
            self.push();
        }
    }

    /* ---------- #9 Écran partagé ---------- */
    pub fn split_rects(&self, r: Rect, sp: Split) -> (Rect, Rect) {
        let k = self.ui_scale() * self.scale();
        let gap = (8.0 * k) as i32;
        let (pos, size) = (r.position.to_physical::<i32>(1.0), r.size.to_physical::<u32>(1.0));
        let vert = self.cfg["splitDir"] != "h";
        let total = if vert { size.width as i32 } else { size.height as i32 };
        let first = ((total - gap) as f64 * sp.ratio.clamp(0.15, 0.85)) as i32;
        let second = (total - gap - first).max(0);
        let rect = |x: i32, y: i32, w: i32, h: i32| Rect { position: PhysicalPosition::new(x, y).into(), size: PhysicalSize::new(w.max(0) as u32, h.max(0) as u32).into() };
        if vert {
            (rect(pos.x, pos.y, first, size.height as i32), rect(pos.x + first + gap, pos.y, second, size.height as i32))
        } else {
            (rect(pos.x, pos.y, size.width as i32, first), rect(pos.x, pos.y + first + gap, size.width as i32, second))
        }
    }

    pub fn split_with(&mut self, other: u32) {
        let Some(cur) = self.tabs.get(self.active).map(|t| t.id) else { return };
        if other == cur || self.cfg["split"] != true {
            return;
        }
        let Some(i) = self.idx(other) else { return };
        if self.tabs[i].view.is_none() && !self.tabs[i].url.is_empty() {
            self.build_view(i);
        }
        let ratio = self.cfg["splitRatio"].as_f64().unwrap_or(50.0) / 100.0;
        self.x.split = Some(Split { a: cur, b: other, ratio });
        if let Some(v) = &self.tabs[i].view {
            let _ = v.set_memory_usage_level(MemoryUsageLevel::Normal);
            unsafe {
                if let Ok(c3) = v.webview().cast::<ICoreWebView2_3>() {
                    let _ = c3.Resume();
                }
            }
        }
        self.sync_scroll(true);
        self.relayout();
        self.push();
    }

    /// Raccourci : sépare l'écran avec l'onglet consulté juste avant (ou un nouvel onglet), ou le referme.
    pub fn split_toggle(&mut self) {
        if self.x.split.is_some() {
            return self.unsplit();
        }
        let cur = self.active;
        let best = self.tabs.iter().enumerate().filter(|(i, t)| *i != cur && !t.url.is_empty()).max_by_key(|(_, t)| t.lv).map(|(_, t)| t.id);
        match best {
            Some(id) => self.split_with(id),
            None => {
                let p = self.private_win;
                let keep = self.tabs[cur].id;
                self.new_tab(None, false, p);
                let nid = self.next_id;
                if let Some(a) = self.idx(keep) {
                    self.active = a;
                }
                self.split_with(nid);
            }
        }
    }

    pub fn unsplit(&mut self) {
        let Some(sp) = self.x.split.take() else { return };
        self.sync_scroll(false);
        let act = self.tabs.get(self.active).map(|t| t.id);
        let other = if act == Some(sp.a) { sp.b } else { sp.a };
        if let Some(i) = self.idx(other) {
            if let Some(v) = &self.tabs[i].view {
                let _ = v.set_visible(false);
            }
        }
        self.relayout();
        self.push();
    }

    /// Défilement synchronisé des deux pages (#9, désactivé par défaut).
    fn sync_scroll(&self, on: bool) {
        let Some(sp) = self.x.split else { return };
        let on = on && self.cfg["splitSync"] == true;
        for id in [sp.a, sp.b] {
            if let Some(i) = self.idx(id) {
                self.eval_tab(i, &format!("window.__plume&&__plume.sync({on})"));
            }
        }
    }

    /* ---------- #11 Coup d'œil ---------- */
    pub fn peek_rect(&self) -> Rect {
        let r = self.content_rect();
        let (pos, size) = (r.position.to_physical::<i32>(1.0), r.size.to_physical::<u32>(1.0));
        let pct = self.cfg["peekSize"].as_f64().unwrap_or(80.0).clamp(40.0, 100.0) / 100.0;
        let k = self.ui_scale() * self.scale();
        let bar = (48.0 * k) as i32;
        let w = (size.width as f64 * pct) as i32;
        let h = ((size.height as f64 - bar as f64) * pct) as i32;
        let x = pos.x + (size.width as i32 - w) / 2;
        let y = pos.y + bar + ((size.height as i32 - bar) - h) / 2;
        Rect { position: PhysicalPosition::new(x, y).into(), size: PhysicalSize::new(w.max(100) as u32, h.max(100) as u32).into() }
    }

    pub fn open_peek(&mut self, url: &str) {
        self.close_peek();
        let (wid, p1, p2) = (self.wid, self.proxy.clone(), self.proxy.clone());
        let built = WebViewBuilder::new()
            .with_environment(self.env.clone())
            .with_url(url)
            .with_bounds(self.peek_rect())
            .with_devtools(true)
            .with_initialization_script_for_main_only(
                "addEventListener('keydown',e=>{if(e.key==='Escape'){try{window.ipc.postMessage('{\"c\":\"peek-close\"}')}catch(x){}}},true)",
                true,
            )
            .with_new_window_req_handler(move |u, f| {
                if is_popup(&f) {
                    return NewWindowResponse::Allow;
                }
                let _ = p1.send_event(Ev::Ui(wid, json!({"c": "open", "u": u}).to_string()));
                NewWindowResponse::Deny
            })
            .with_ipc_handler(move |r| {
                if r.body().contains("peek-close") {
                    let _ = p2.send_event(Ev::Ui(wid, r#"{"c":"peek-close"}"#.into()));
                }
            })
            .build_as_child(&self.window);
        if let Ok(v) = built {
            clip_siblings(&v);
            unsafe {
                let _ = hook_accel(&v.controller(), &self.proxy, Who::Win(wid));
            }
            let _ = v.focus();
            self.x.peek = Some(v);
            self.x.peek_url = url.to_string();
            self.js(&format!("peekOpen({})", json!(url)));
        }
    }

    pub fn close_peek(&mut self) {
        if self.x.peek.take().is_some() {
            self.x.peek_url.clear();
            self.js("peekClosed()");
            self.refocus();
        }
    }

    /* ---------- #10 Recherche dans les onglets ---------- */
    fn all_tabs(&self) -> Value {
        let c = &self.cfg;
        let tab = |t: &Tab, w: u32, sp: u32, here: bool| json!({"id": t.id, "t": t.title, "u": t.url, "f": t.fav, "w": w, "sp": sp, "here": here, "c": t.cont});
        let mut l: Vec<Value> = self.tabs.iter().filter(|t| !t.url.is_empty()).map(|t| tab(t, self.wid, self.x.space, true)).collect();
        for (sp, tabs, _) in &self.x.stash {
            l.extend(tabs.iter().filter(|t| !t.url.is_empty()).map(|t| tab(t, self.wid, *sp, false)));
        }
        if c["tsWindows"] != false {
            for w in &self.others {
                l.extend(w.tabs.iter().filter(|t| !t.url.is_empty()).map(|t| tab(t, w.wid, w.x.space, false)));
                for (sp, tabs, _) in &w.x.stash {
                    l.extend(tabs.iter().filter(|t| !t.url.is_empty()).map(|t| tab(t, w.wid, *sp, false)));
                }
            }
        }
        let closed = if c["tsClosed"] != false { json!(self.closed.iter().rev().cloned().collect::<Vec<_>>()) } else { json!([]) };
        let archive = if c["tsArchive"] != false { json!(self.archive) } else { json!([]) };
        json!({"tabs": l, "closed": closed, "archive": archive})
    }

    /// Va à un onglet : autre fenêtre, autre espace ou onglet de la fenêtre.
    fn goto_tab(&mut self, id: u32) {
        if !self.focus_tab(id) {
            // onglet rangé dans un espace d'une fenêtre ?
            if let Some(w) = self.others.iter().position(|w| w.x.stash.iter().any(|s| s.1.iter().any(|t| t.id == id))) {
                self.bring(w);
            }
        }
        if let Some(sp) = self.x.stash.iter().find(|s| s.1.iter().any(|t| t.id == id)).map(|s| s.0) {
            self.switch_space(sp);
        }
        if let Some(i) = self.idx(id) {
            self.activate(i);
        }
        self.window.set_focus();
    }

    /* ---------- #12 Sessions ---------- */
    fn session_tabs(&self, tabs: &[Tab]) -> Vec<Value> {
        let notes = self.cfg["notesInSess"] == true;
        tabs.iter()
            .filter(|t| !t.url.is_empty() && !t.private)
            .map(|t| {
                let mut v = json!({"u": t.url, "t": t.title, "f": t.fav, "g": t.group, "pin": t.pinned});
                if notes && let Some(n) = self.note_of(&t.url) {
                    v["note"] = json!(n);
                }
                v
            })
            .collect()
    }

    pub fn save_session(&mut self, name: &str, tabs: Vec<Value>, auto: bool) {
        if tabs.is_empty() {
            return;
        }
        let id = self.sessions.iter().filter_map(|s| s["id"].as_u64()).max().unwrap_or(0) + 1;
        if auto {
            self.sessions.retain(|s| s["auto"] != true);
        }
        self.sessions.insert(0, json!({"id": id, "n": name, "t": now_ms(), "auto": auto, "tabs": tabs, "groups": self.groups}));
        let max = self.cfg["sessMax"].as_u64().unwrap_or(20) as usize;
        while self.sessions.len() > max {
            // les plus anciennes partent d'abord
            self.sessions.pop();
        }
        save_json("sessions.json", &json!(self.sessions));
    }

    /// Plume se ferme : dernière session gardée automatiquement (#12).
    pub fn on_quit(&mut self) {
        self.close_peek();
        if self.cfg["sessions"] == true && self.cfg["sessAuto"] == true {
            let mut tabs = self.session_tabs(&self.tabs);
            for (_, t, _) in &self.x.stash {
                tabs.extend(self.session_tabs(t));
            }
            for w in &self.others {
                tabs.extend(self.session_tabs(&w.tabs));
            }
            let name = if self.lang() == "fr" { "Dernière session" } else { "Last session" };
            self.save_session(name, tabs, true);
        }
        self.clear_on_exit(); // #20
    }

    /* ---------- #13 Notes d'onglet (chiffrées avec le coffre s'il existe) ---------- */
    pub fn note_key(&self, url: &str) -> String {
        if self.cfg["notesScope"] == "tab" { format!("url:{}", url.split('#').next().unwrap_or(url)) } else { format!("site:{}", import::host_of(url)) }
    }

    pub fn note_of(&self, url: &str) -> Option<String> {
        self.notes.as_ref()?.get(self.note_key(url))?.as_str().map(String::from)
    }

    /// Charge les notes : en clair sans coffre, sinon déchiffrées seulement quand le coffre est ouvert.
    pub fn load_notes(&mut self) {
        let plain = data_file("notes.json");
        let sealed = data_file("notes.bin");
        self.notes = if self.vault.exists() {
            if !self.vault.unlocked() {
                None
            } else if let Ok(b) = std::fs::read(&sealed) {
                self.vault.open(&b).and_then(|p| serde_json::from_slice(&p).ok()).or(Some(json!({})))
            } else {
                // notes écrites avant la création du coffre : chiffrées maintenant
                let v = std::fs::read_to_string(&plain).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({}));
                if plain.exists() {
                    self.notes = Some(v.clone());
                    self.save_notes();
                    let _ = std::fs::remove_file(&plain);
                }
                Some(v)
            }
        } else {
            Some(std::fs::read_to_string(&plain).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({})))
        };
    }

    fn save_notes(&self) {
        let Some(n) = &self.notes else { return };
        if self.vault.exists() {
            if let Some(b) = self.vault.seal(n.to_string().as_bytes()) {
                let f = data_file("notes.bin");
                let _ = std::fs::write(f.with_extension("tmp"), b).and_then(|_| std::fs::rename(f.with_extension("tmp"), &f));
            }
        } else {
            save_json("notes.json", n);
        }
    }

    pub fn has_note(&self, url: &str) -> bool {
        self.cfg["notes"] == true && self.cfg["notesIcon"] == true && !url.is_empty() && self.note_of(url).is_some()
    }

    /* ---------- Messages de l'interface (0.7, onglets) ---------- */
    pub fn on_ui07(&mut self, c: &str, v: &Value) {
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        let id = v["id"].as_u64().unwrap_or(0) as u32;
        match c {
            // espaces
            "space" => self.switch_space(v["sp"].as_u64().unwrap_or(0) as u32),
            "space-new" => {
                let id = self.spaces.iter().filter_map(|s| s["id"].as_u64()).max().unwrap_or(0) + 1;
                self.spaces.push(json!({"id": id, "name": s("name").chars().take(30).collect::<String>(), "color": s("color"), "icon": s("icon")}));
                self.switch_space(id as u32);
                self.save();
                self.push();
            }
            "space-set" => {
                let sid = v["sp"].as_u64();
                if !self.spaces.iter().any(|x| x["id"].as_u64() == sid) && sid == Some(0) {
                    self.spaces.insert(0, json!({"id": 0, "name": "", "color": "blue", "icon": "home"}));
                }
                if let Some(x) = self.spaces.iter_mut().find(|x| x["id"].as_u64() == sid) {
                    for k in ["name", "color", "icon", "cont"] {
                        if !v[k].is_null() {
                            x[k] = v[k].clone();
                        }
                    }
                }
                self.save();
                self.push();
            }
            "space-del" => {
                let sid = v["sp"].as_u64().unwrap_or(0) as u32;
                if sid == 0 {
                    return;
                }
                if self.x.space == sid {
                    self.switch_space(0);
                }
                // ses onglets rejoignent l'espace Principal
                if let Some(i) = self.x.stash.iter().position(|s| s.0 == sid) {
                    let (_, tabs, _) = self.x.stash.remove(i);
                    if self.x.space == 0 {
                        self.tabs.extend(tabs);
                    } else if let Some(m) = self.x.stash.iter_mut().find(|s| s.0 == 0) {
                        m.1.extend(tabs);
                    } else {
                        self.x.stash.push((0, tabs, 0));
                    }
                }
                self.spaces.retain(|x| x["id"].as_u64() != Some(sid as u64));
                self.save();
                self.push();
            }
            "tab-space" => {
                // envoyer un onglet dans un autre espace
                let sp = v["sp"].as_u64().unwrap_or(0) as u32;
                if sp != self.x.space && let Some(i) = self.idx(id).filter(|_| self.tabs.len() > 1) {
                    let mut t = self.tabs.remove(i);
                    if let Some(w) = &t.view {
                        let _ = w.set_visible(false);
                    }
                    if self.cfg["spacesUnload"] == true && !self.dl_running(t.id) {
                        t.view = None;
                    }
                    match self.x.stash.iter_mut().find(|s| s.0 == sp) {
                        Some(s) => s.1.push(t),
                        None => self.x.stash.push((sp, vec![t], 0)),
                    }
                    let a = if i < self.active { self.active - 1 } else { self.active.min(self.tabs.len() - 1) };
                    self.active = usize::MAX;
                    self.activate(a);
                    self.save();
                }
            }
            // archive
            "archive-get" => self.js(&format!("ARCH({})", json!(self.archive))),
            "archive-open" | "archive-del" => {
                let i = v["i"].as_u64().unwrap_or(u64::MAX) as usize;
                if i < self.archive.len() {
                    let a = self.archive.remove(i);
                    save_json("archive.json", &json!(self.archive));
                    if c == "archive-open" {
                        self.new_tab(Some(str_of(&a["u"])), true, false);
                    }
                }
                self.js(&format!("ARCH({})", json!(self.archive)));
            }
            "archive-clear" => {
                self.archive.clear();
                save_json("archive.json", &json!(self.archive));
                self.js("ARCH([])");
            }
            // écran partagé
            "split-with" => self.split_with(id),
            "split-close" => self.unsplit(),
            "split-toggle" => self.split_toggle(),
            "split-ratio" => {
                if let Some(sp) = self.x.split.as_mut() {
                    sp.ratio = v["r"].as_f64().unwrap_or(0.5).clamp(0.15, 0.85);
                    self.relayout();
                }
            }
            "split-swap" => {
                if let Some(sp) = self.x.split.as_mut() {
                    std::mem::swap(&mut sp.a, &mut sp.b);
                    self.relayout();
                    self.push();
                }
            }
            // recherche dans les onglets
            "all-tabs" => self.js(&format!("ALLTABS({})", self.all_tabs())),
            "goto-tab" => self.goto_tab(id),
            "reopen-url" => {
                let u = s("u");
                if let Some(p) = self.closed.iter().rposition(|x| x["u"] == u.as_str()) {
                    self.closed.remove(p);
                }
                self.new_tab(Some(u), true, false);
            }
            // coup d'œil
            "peek-close" => self.close_peek(),
            "peek-tab" => {
                let u = self.x.peek_url.clone();
                self.close_peek();
                if !u.is_empty() {
                    self.new_tab(Some(u), true, false);
                }
            }
            // sessions
            "sess-list" => self.js(&format!("SESS({})", json!(self.sessions))),
            "sess-save" => {
                let g = v["g"].as_u64().unwrap_or(0) as u32;
                let tabs: Vec<Tab> = vec![];
                let _ = tabs;
                let list = if g > 0 {
                    let mine: Vec<&Tab> = self.tabs.iter().filter(|t| t.group == g).collect();
                    mine.iter().map(|t| json!({"u": t.url, "t": t.title, "f": t.fav, "g": t.group, "pin": t.pinned})).collect()
                } else {
                    self.session_tabs(&self.tabs)
                };
                let name = s("name");
                self.save_session(if name.trim().is_empty() { "Session" } else { name.trim() }, list, false);
                self.js(&format!("SESS({})", json!(self.sessions)));
                self.toast("sess-saved");
            }
            "sess-open" => {
                let sid = v["sid"].as_u64();
                if let Some(sess) = self.sessions.iter().find(|x| x["id"].as_u64() == sid).cloned() {
                    let tabs = sess["tabs"].as_array().cloned().unwrap_or_default();
                    // les notes enregistrées avec la session reviennent aussi
                    if let Some(n) = self.notes.as_mut() {
                        for t in &tabs {
                            if let Some(txt) = t["note"].as_str() {
                                let k = if self.cfg["notesScope"] == "tab" { format!("url:{}", str_of(&t["u"]).split('#').next().unwrap_or("")) } else { format!("site:{}", import::host_of(&str_of(&t["u"]))) };
                                n[&k] = json!(txt);
                            }
                        }
                        self.save_notes();
                    }
                    self.want_window.push(WinReq { url: None, private: false, tab: None, restore: tabs, active: 0 });
                }
            }
            "sess-rename" => {
                let sid = v["sid"].as_u64();
                if let Some(x) = self.sessions.iter_mut().find(|x| x["id"].as_u64() == sid) {
                    x["n"] = json!(s("name").chars().take(60).collect::<String>());
                    x["auto"] = json!(false);
                }
                save_json("sessions.json", &json!(self.sessions));
                self.js(&format!("SESS({})", json!(self.sessions)));
            }
            "sess-del" => {
                let sid = v["sid"].as_u64();
                self.sessions.retain(|x| x["id"].as_u64() != sid);
                save_json("sessions.json", &json!(self.sessions));
                self.js(&format!("SESS({})", json!(self.sessions)));
            }
            "sess-export" => {
                let owner = HWND(self.window.hwnd() as _);
                if let Some(p) = pick_json(owner, true, "plume-sessions.json") {
                    let _ = std::fs::write(p, serde_json::to_string_pretty(&json!({"plume": "sessions", "v": 1, "sessions": self.sessions})).unwrap_or_default());
                    self.toast("sess-exported");
                }
            }
            "sess-import" => {
                let owner = HWND(self.window.hwnd() as _);
                if let Some(p) = pick_json(owner, false, "") {
                    let v: Value = std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
                    let list = v["sessions"].as_array().or(v.as_array()).cloned().unwrap_or_default();
                    let mut next = self.sessions.iter().filter_map(|s| s["id"].as_u64()).max().unwrap_or(0);
                    let mut n = 0;
                    for mut x in list.into_iter().filter(|x| x["tabs"].is_array()) {
                        next += 1;
                        n += 1;
                        x["id"] = json!(next);
                        x["auto"] = json!(false);
                        self.sessions.push(x);
                    }
                    save_json("sessions.json", &json!(self.sessions));
                    self.js(&format!("SESS({})", json!(self.sessions)));
                    self.js(&format!("toast('sess-imported',{})", json!({"n": n})));
                }
            }
            // notes
            "note-get" => {
                let u = self.idx(id).map(|i| self.tabs[i].url.clone()).unwrap_or_else(|| s("u"));
                let locked = self.notes.is_none();
                let txt = self.note_of(&u).unwrap_or_default();
                self.js(&format!("NOTE({})", json!({"id": id, "u": u, "key": self.note_key(&u), "text": txt, "locked": locked})));
            }
            "note-set" => {
                let u = self.idx(id).map(|i| self.tabs[i].url.clone()).unwrap_or_else(|| s("u"));
                let key = self.note_key(&u);
                if let Some(n) = self.notes.as_mut() {
                    let t = s("text");
                    if t.trim().is_empty() {
                        if let Some(m) = n.as_object_mut() {
                            m.remove(&key);
                        }
                    } else {
                        n[&key] = json!(t.chars().take(20000).collect::<String>());
                    }
                    self.save_notes();
                }
                self.push();
            }
            _ => self.on_ui07b(c, v),
        }
    }

    /* ---------- Messages des pages (0.7) ---------- */
    pub fn on_page07(&mut self, i: usize, c: &str, v: &Value) {
        match c {
            "peek" => {
                if self.cfg["peek"] == true {
                    let u = str_of(&v["u"]);
                    if u.starts_with("http") {
                        self.open_peek(&u);
                    }
                }
            }
            "scroll" => {
                // défilement synchronisé de l'écran partagé
                if let Some(sp) = self.x.split {
                    let me = self.tabs[i].id;
                    let other = if me == sp.a { sp.b } else if me == sp.b { sp.a } else { return };
                    if let Some(j) = self.idx(other) {
                        self.eval_tab(j, &format!("window.__plume&&__plume.scrollTo({})", v["f"].as_f64().unwrap_or(0.0)));
                    }
                }
            }
            _ => self.on_page07b(i, c, v),
        }
    }
}
