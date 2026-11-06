// Plume 0.7 · Navigation, confidentialité, apparence et outils : messages de l'interface et des pages.
use super::*;

impl App {
    pub fn on_ui07b(&mut self, c: &str, v: &Value) {
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        let tab = v["id"].as_u64().and_then(|id| self.idx(id as u32));
        match c {
            /* #19 conteneurs */
            "cont-add" => {
                let mut l = self.conts();
                let id = l.iter().filter_map(|c| c["id"].as_u64()).max().unwrap_or(0) + 1;
                let name: String = s("name").trim().chars().take(24).collect();
                if name.is_empty() || l.len() >= 12 {
                    return;
                }
                l.push(json!({"id": id, "name": name, "color": s("color")}));
                self.set_cfg07("contList", json!(l));
                if let Some(i) = tab.filter(|_| v["open"] == true) {
                    self.reopen_in(i, id as u32);
                }
            }
            "cont-set" => {
                let mut l = self.conts();
                if let Some(x) = l.iter_mut().find(|c| c["id"] == v["cn"]) {
                    for k in ["name", "color"] {
                        if let Some(t) = v[k].as_str().filter(|t| !t.trim().is_empty()) {
                            x[k] = json!(t.trim().chars().take(24).collect::<String>());
                        }
                    }
                }
                self.set_cfg07("contList", json!(l));
            }
            "cont-del" => {
                let cid = v["cn"].as_u64().unwrap_or(0);
                let l: Vec<Value> = self.conts().into_iter().filter(|c| c["id"].as_u64() != Some(cid)).collect();
                self.set_cfg07("contList", json!(l));
                if let Some(m) = self.cfg["contSites"].as_object_mut() {
                    m.retain(|_, x| x.as_u64() != Some(cid));
                }
                // ses onglets reviennent au profil normal (page rechargée, tout de suite pour l'onglet actif)
                let a = self.active;
                let mut rebuild = false;
                for (k, t) in self.tabs.iter_mut().enumerate() {
                    if t.cont as u64 == cid {
                        t.cont = 0;
                        t.view = None;
                        rebuild |= k == a;
                    }
                }
                if rebuild {
                    self.active = usize::MAX;
                    self.activate(a);
                }
                self.save();
                self.push();
            }
            "cont-open" => {
                if let Some(i) = tab {
                    self.reopen_in(i, v["cn"].as_u64().unwrap_or(0) as u32);
                }
            }
            "cont-new" => {
                self.open_cont = Some(v["cn"].as_u64().unwrap_or(0) as u32);
                self.new_tab(None, true, false);
            }
            "cont-site" => {
                let Some(i) = tab else { return };
                let h = import::host_of(&self.tabs[i].url);
                let c = self.tabs[i].cont;
                if h.is_empty() || c == 0 {
                    return;
                }
                if !self.cfg["contSites"].is_object() {
                    self.cfg["contSites"] = json!({});
                }
                let m = self.cfg["contSites"].as_object_mut().unwrap();
                if m.get(&h).and_then(|x| x.as_u64()) == Some(c as u64) {
                    m.remove(&h);
                } else {
                    m.insert(h, json!(c));
                }
                settings::mark(&mut self.cfg, "contSites");
                self.save();
                self.push();
            }
            "cont-site-del" => {
                if let Some(m) = self.cfg["contSites"].as_object_mut() {
                    m.remove(&s("h"));
                }
                self.save();
                self.push();
            }
            /* #21 rapport de confidentialité */
            "priv-report" => self.priv_report(),
            /* #22 générateur et audit, #23 fuites */
            "pw-gen" => {
                let pw = self.pw_generate();
                self.js(&format!("pwGen({})", json!(pw)));
            }
            "pw-audit" => self.pw_audit(),
            "pw-hibp" => self.pw_hibp_all(),
            /* #24 Windows Hello */
            "hello-unlock" => self.hello_unlock(),
            _ => self.on_ui_look(c, v),
        }
    }

    /// Réglage modifié par Plume à la demande de l'utilisateur (marqué « modifié » pour les changements de mode).
    fn set_cfg07(&mut self, k: &str, val: Value) {
        self.cfg[k] = val;
        settings::mark(&mut self.cfg, k);
        self.save();
        self.push();
    }

    pub fn on_page07b(&mut self, i: usize, c: &str, v: &Value) {
        match c {
            "tint" => self.set_tint(i, &v["v"]),
            "cc-js" => self.cc_run(i),
            "pw-new" => self.pw_offer(i),
            "priv-page" => self.priv_page(i, v),
            // #17 geste de souris terminé dans la page → action choisie dans les réglages
            "gesture" if self.cfg["gestures"] == true => {
                let g = str_of(&v["g"]);
                let act = self.cfg["gestMap"][g.as_str()].as_str().unwrap_or("none").to_string();
                let n = self.tabs.len();
                match act.as_str() {
                    "back" => unsafe {
                        if let Some(w) = &self.tabs[i].view {
                            let _ = w.webview().GoBack();
                        }
                    },
                    "fwd" => unsafe {
                        if let Some(w) = &self.tabs[i].view {
                            let _ = w.webview().GoForward();
                        }
                    },
                    "close" => self.close_tab(i),
                    "new" => self.cmd(Cmd::NewTab),
                    "reload" => {
                        if let Some(w) = &self.tabs[i].view {
                            let _ = w.reload();
                        }
                    }
                    "reopen" => self.cmd(Cmd::Reopen),
                    "top" => self.eval_tab(i, "scrollTo({top:0,behavior:'smooth'})"),
                    "bottom" => self.eval_tab(i, "scrollTo({top:document.documentElement.scrollHeight,behavior:'smooth'})"),
                    "next" if n > 1 => self.activate((i + 1) % n),
                    "prev" if n > 1 => self.activate((i + n - 1) % n),
                    _ => {}
                }
            }
            _ => self.on_page_tools(i, c, v),
        }
    }
}
