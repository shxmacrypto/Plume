// Plume 0.7 · Apparence (#26–#30) : teinte adaptative, fond de la nouvelle page (image personnelle, image du jour),
// widgets (météo, tâches), CSS / JS par site, mode zen et plein écran (F11 et vidéos).
use super::*;
use tao::window::Fullscreen;

/// Bande du haut laissée à l'interface en mode zen : la survoler fait réapparaître la barre.
pub const ZEN_STRIP: f64 = 4.0;

/// Couleur venant d'une page (theme-color) : courte, sans caractère qui sortirait d'une valeur CSS.
pub fn css_color(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty() && s.len() <= 40 && s.chars().all(|c| c.is_ascii_alphanumeric() || "#(),.% -".contains(c))).then(|| s.to_string())
}

/// Encodage d'un paramètre d'URL (ville de la météo).
pub fn url_enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

const IMG_EXT: [&str; 5] = ["jpg", "jpeg", "png", "webp", "gif"];

fn ntp_image_path() -> Option<std::path::PathBuf> {
    IMG_EXT.iter().map(|e| setup::data_dir().join(format!("ntp-bg.{e}"))).find(|p| p.exists())
}

/// Images de la nouvelle page servies à l'interface : plume://ui/ntp/image et plume://ui/ntp/daily.
pub fn ntp_file(name: &str) -> Option<(Vec<u8>, &'static str)> {
    let p = match name {
        "image" => ntp_image_path()?,
        "daily" => setup::data_dir().join("ntp-daily.jpg"),
        _ => return None,
    };
    let mime = match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/jpeg",
    };
    Some((std::fs::read(&p).ok()?, mime))
}

/// Météo (Open-Meteo, sans clé) : ville → coordonnées → température et temps actuels.
fn weather(city: &str, lang: &str, unit: &str) -> std::result::Result<Value, String> {
    let get = |u: String| -> std::result::Result<Value, String> { serde_json::from_slice(&update::http_get(&u)?).map_err(|e| e.to_string()) };
    let g = get(format!("https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language={lang}&format=json", url_enc(city)))?;
    let r = &g["results"][0];
    let (lat, lon) = (r["latitude"].as_f64().ok_or("city")?, r["longitude"].as_f64().ok_or("city")?);
    let f = get(format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,weather_code,is_day&daily=temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=1{}",
        if unit == "f" { "&temperature_unit=fahrenheit" } else { "" }
    ))?;
    let cur = &f["current"];
    if !cur["temperature_2m"].is_number() {
        return Err("forecast".into());
    }
    Ok(json!({"city": r["name"], "cc": r["country_code"], "t": cur["temperature_2m"], "code": cur["weather_code"], "day": cur["is_day"],
              "max": f["daily"]["temperature_2m_max"][0], "min": f["daily"]["temperature_2m_min"][0], "u": unit}))
}

/// Image du jour de Bing : téléchargée une fois par jour, seulement si ce fond est choisi.
fn daily_image(lang: &str) -> std::result::Result<Value, String> {
    let mkt = if lang == "fr" { "fr-FR" } else { "en-US" };
    let v: Value = serde_json::from_slice(&update::http_get(&format!("https://www.bing.com/HPImageArchive.aspx?format=js&idx=0&n=1&mkt={mkt}"))?).map_err(|e| e.to_string())?;
    let im = &v["images"][0];
    let u = im["url"].as_str().ok_or("no image")?;
    let img = update::http_get(&if u.starts_with("http") { u.to_string() } else { format!("https://www.bing.com{u}") })?;
    if img.len() < 2000 {
        return Err("image".into());
    }
    let dir = setup::data_dir();
    std::fs::write(dir.join("ntp-daily.jpg"), &img).map_err(|e| e.to_string())?;
    let link = im["copyrightlink"].as_str().filter(|l| l.starts_with("https://")).unwrap_or("");
    let meta = json!({"date": today(), "credit": im["copyright"], "link": link});
    let _ = std::fs::write(dir.join("ntp-daily.json"), meta.to_string());
    Ok(meta)
}

impl App {
    /* ---------- #30 Zen et plein écran ---------- */
    /// Hauteur de l'interface au-dessus de la page (0 en plein écran d'une vidéo, une fine bande en zen).
    pub fn top_h(&self) -> f64 {
        if self.x.page_fs {
            0.0
        } else if self.x.zen {
            ZEN_STRIP
        } else {
            self.ui_h
        }
    }

    pub fn toggle_zen(&mut self) {
        if self.cfg["zen"] == "off" {
            return;
        }
        self.x.zen = !self.x.zen;
        self.relayout();
        self.push();
    }

    /// F11 : plein écran de la fenêtre (le mode zen suit si « en plein écran » est choisi).
    pub fn toggle_fullscreen(&mut self) {
        if self.x.page_fs {
            return; // une vidéo en plein écran : Échap la fait sortir
        }
        let on = self.window.fullscreen().is_none();
        self.window.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
        if self.cfg["zen"] == "fullscreen" {
            self.x.zen = on;
        }
        self.relayout();
        self.push();
    }

    /// Une page passe en plein écran (vidéo) : la fenêtre aussi, sans barre ni onglets.
    pub fn page_fullscreen(&mut self, id: u32, on: bool) {
        let Some(i) = self.idx(id) else { return };
        if on == self.x.page_fs || (on && i != self.active) {
            return;
        }
        self.x.page_fs = on;
        if on {
            self.x.fs_before = self.window.fullscreen().is_some();
            if !self.x.fs_before {
                self.window.set_fullscreen(Some(Fullscreen::Borderless(None)));
            }
        } else if !self.x.fs_before {
            self.window.set_fullscreen(None);
        }
        self.relayout();
        self.push();
    }

    /// Changement d'onglet pendant qu'une vidéo est en plein écran : on revient à la fenêtre normale.
    pub fn leave_page_fs(&mut self) {
        if self.x.page_fs {
            self.x.page_fs = false;
            if !self.x.fs_before {
                self.window.set_fullscreen(None);
            }
        }
    }

    /* ---------- #26 Teinte adaptative ---------- */
    pub fn set_tint(&mut self, i: usize, v: &Value) {
        let t = css_color(v.as_str().unwrap_or(""));
        if self.tabs[i].tint != t {
            self.tabs[i].tint = t;
            if i == self.active && self.cfg["adaptive"] == true {
                self.push();
            }
        }
    }

    /* ---------- #27 Fond de la nouvelle page ---------- */
    /// Image personnelle : copiée dans le dossier de Plume (ntp-bg.<ext>), l'original peut être déplacé.
    pub fn ntp_pick(&mut self) {
        let Some(src) = pick_image(HWND(self.window.hwnd() as _)) else { return };
        let ext = src.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).filter(|e| IMG_EXT.contains(&e.as_str())).unwrap_or_else(|| "jpg".into());
        let size = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
        if size == 0 || size > 25 * 1024 * 1024 {
            return self.toast("ntp-big");
        }
        for e in IMG_EXT {
            let _ = std::fs::remove_file(setup::data_dir().join(format!("ntp-bg.{e}")));
        }
        if std::fs::copy(&src, setup::data_dir().join(format!("ntp-bg.{ext}"))).is_ok() {
            self.ntp_v += 1;
            self.cfg["ntpBg"] = json!("image");
            settings::mark(&mut self.cfg, "ntpBg");
            self.save();
            self.push();
        }
    }

    /// Image du jour : au démarrage et une fois par jour (minuteur d'une minute), seulement si ce fond est choisi.
    pub fn daily_check(&mut self) {
        if self.cfg["ntpBg"] != "daily" || self.daily_busy || (self.daily["date"] == today().as_str() && setup::data_dir().join("ntp-daily.jpg").exists()) {
            return;
        }
        if self.daily_try.is_some_and(|t| t.elapsed() < Duration::from_secs(30 * 60)) {
            return; // pas de connexion : on réessaie dans 30 min
        }
        self.daily_busy = true;
        self.daily_try = Some(Instant::now());
        let (lang, p) = (self.lang().to_string(), self.proxy.clone());
        std::thread::spawn(move || {
            let r = daily_image(&lang);
            let _ = p.send_event(Ev::Do(Box::new(move |app: &mut App| {
                app.daily_busy = false;
                if let Ok(m) = r {
                    app.daily = m;
                    app.ntp_v += 1;
                    app.push();
                }
            })));
        });
    }

    /* ---------- #28 Widgets : météo et tâches ---------- */
    pub fn wx_get(&mut self, unit: &str) {
        let city = self.cfg["wCity"].as_str().unwrap_or("").trim().to_string();
        let on = self.cfg["widgets"].as_array().is_some_and(|l| l.iter().any(|w| w == "weather"));
        if city.is_empty() || !on {
            return self.js("wxRes(null)");
        }
        let unit = if unit == "f" { "f" } else { "c" };
        let key = format!("{city}|{unit}|{}", self.lang());
        if let Some((k, t, v)) = &self.wx {
            if *k == key && t.elapsed() < Duration::from_secs(30 * 60) {
                return self.js(&format!("wxRes({v})"));
            }
        }
        if self.wx_busy {
            return;
        }
        self.wx_busy = true;
        let (lang, p, unit) = (self.lang().to_string(), self.proxy.clone(), unit.to_string());
        std::thread::spawn(move || {
            let r = weather(&city, &lang, &unit);
            let _ = p.send_event(Ev::Do(Box::new(move |app: &mut App| {
                app.wx_busy = false;
                match r {
                    Ok(v) => {
                        app.js(&format!("wxRes({v})"));
                        app.wx = Some((key, Instant::now(), v));
                    }
                    Err(e) => app.js(&format!("wxRes({})", json!({"err": if e == "city" { "city" } else { "net" }}))),
                }
            })));
        });
    }

    fn todos(&mut self) -> &mut Vec<Value> {
        if !self.todo.is_array() {
            self.todo = tabs07::load_json("todo.json", json!([]));
            if !self.todo.is_array() {
                self.todo = json!([]);
            }
        }
        self.todo.as_array_mut().unwrap()
    }

    fn todo_done(&mut self) {
        let l = json!(self.todos().clone());
        tabs07::save_json("todo.json", &l);
        self.js(&format!("todoRes({l})"));
    }

    /* ---------- #29 CSS et JS par site ---------- */
    pub fn cc_load(&mut self) {
        self.custom = tabs07::load_json("custom.json", json!({}));
        if !self.custom.is_object() {
            self.custom = json!({});
        }
    }

    /// Ce que les pages reçoivent au démarrage : le CSS des sites actifs, et les sites dont le JS doit tourner.
    pub fn cc_page_cfg(&self) -> (Value, Value) {
        if self.cfg["customCss"] != true {
            return (json!({}), json!([]));
        }
        let js_ok = self.cfg["ccMode"] == "cssjs";
        let (mut css, mut js) = (serde_json::Map::new(), vec![]);
        for (h, e) in self.custom.as_object().into_iter().flatten() {
            if e["on"] == false {
                continue;
            }
            if let Some(c) = e["css"].as_str().filter(|c| !c.trim().is_empty()) {
                css.insert(h.clone(), json!(c));
            }
            if js_ok && e["js"].as_str().is_some_and(|j| !j.trim().is_empty()) {
                js.push(json!(h));
            }
        }
        (Value::Object(css), json!(js))
    }

    pub fn cc_list(&self) -> Value {
        if self.cfg["customCss"] != true {
            return json!([]);
        }
        let mut l: Vec<Value> = self
            .custom
            .as_object()
            .into_iter()
            .flatten()
            .map(|(h, e)| json!({"h": h, "on": e["on"] != false, "css": e["css"].as_str().is_some_and(|c| !c.trim().is_empty()), "js": e["js"].as_str().is_some_and(|j| !j.trim().is_empty())}))
            .collect();
        l.sort_by(|a, b| a["h"].as_str().cmp(&b["h"].as_str()));
        json!(l)
    }

    fn cc_changed(&mut self) {
        tabs07::save_json("custom.json", &self.custom);
        self.push_page_cfg();
        self.push();
    }

    /// La page signale qu'elle est prête : son JS personnalisé est exécuté par Plume (non bloqué par la CSP du site).
    pub fn cc_run(&mut self, i: usize) {
        if self.cfg["customCss"] != true || self.cfg["ccMode"] != "cssjs" || self.tabs[i].private {
            return;
        }
        let h = import::host_of(&self.tabs[i].url);
        let e = self.custom.as_object().and_then(|m| m.iter().find(|(k, _)| h == **k || h.ends_with(&format!(".{k}"))).map(|x| x.1.clone()));
        if let Some(js) = e.filter(|e| e["on"] != false).and_then(|e| e["js"].as_str().map(String::from)).filter(|j| !j.trim().is_empty()) {
            self.eval_tab(i, &format!("try{{\n{js}\n}}catch(e){{console.error('Plume custom JS:',e)}}"));
        }
    }

    /* ---------- Messages de l'interface ---------- */
    pub fn on_ui_look(&mut self, c: &str, v: &Value) {
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        match c {
            "zen-toggle" => self.toggle_zen(),
            "fullscreen" => self.toggle_fullscreen(),
            "ntp-image" => self.ntp_pick(),
            "wx" => self.wx_get(&s("u")),
            "todo-get" => self.todo_done(),
            "todo-add" => {
                let t: String = s("t").trim().chars().take(200).collect();
                if !t.is_empty() && self.todos().len() < 50 {
                    self.todos().push(json!({"t": t, "d": false}));
                }
                self.todo_done();
            }
            "todo-tog" | "todo-del" => {
                let k = v["i"].as_u64().unwrap_or(u64::MAX) as usize;
                let l = self.todos();
                if k < l.len() {
                    if c == "todo-del" {
                        l.remove(k);
                    } else {
                        let d = l[k]["d"] == true;
                        l[k]["d"] = json!(!d);
                    }
                }
                self.todo_done();
            }
            "todo-clear" => {
                self.todos().retain(|x| x["d"] != true);
                self.todo_done();
            }
            "cc-get" => {
                let h = s("h");
                let e = self.custom.get(&h).cloned().unwrap_or(json!({"css": "", "js": "", "on": true}));
                self.js(&format!("ccRes({})", json!({"h": h, "css": e["css"], "js": e["js"], "on": e["on"] != false})));
            }
            "cc-set" => {
                let h = import::host_of(&s("h"));
                if h.is_empty() || !self.custom.is_object() {
                    return;
                }
                let (css, js): (String, String) = (s("css").chars().take(100_000).collect(), s("js").chars().take(100_000).collect());
                if css.trim().is_empty() && js.trim().is_empty() {
                    self.custom.as_object_mut().unwrap().remove(&h);
                } else {
                    self.custom[&h] = json!({"css": css, "js": js, "on": v["on"] != false});
                }
                self.cc_changed();
            }
            "cc-del" => {
                if let Some(m) = self.custom.as_object_mut() {
                    m.remove(&s("h"));
                }
                self.cc_changed();
            }
            "cc-tog" => {
                let h = s("h");
                if self.custom[&h].is_object() {
                    let on = self.custom[&h]["on"] != false;
                    self.custom[&h]["on"] = json!(!on);
                    self.cc_changed();
                }
            }
            "cc-export" => {
                if let Some(p) = tabs07::pick_json(HWND(self.window.hwnd() as _), true, "plume-custom-sites.json") {
                    if std::fs::write(&p, serde_json::to_string_pretty(&self.custom).unwrap_or_default()).is_ok() {
                        self.toast("cc-exported");
                    }
                }
            }
            "cc-import" => {
                let Some(p) = tabs07::pick_json(HWND(self.window.hwnd() as _), false, "") else { return };
                let Some(m) = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_object().cloned()) else {
                    return self.toast("cc-bad");
                };
                let mut n = 0;
                for (h, e) in m {
                    let h = import::host_of(&h);
                    if h.is_empty() || !e.is_object() {
                        continue;
                    }
                    let str_of_k = |k: &str| e[k].as_str().unwrap_or("").chars().take(100_000).collect::<String>();
                    self.custom[&h] = json!({"css": str_of_k("css"), "js": str_of_k("js"), "on": e["on"] != false});
                    n += 1;
                }
                self.cc_changed();
                self.js(&format!("toast('cc-imported',{{n:{n}}})"));
            }
            _ => self.on_ui_tools(c, v),
        }
    }
}

/// Choix d'une image (fond de la nouvelle page).
fn pick_image(owner: HWND) -> Option<std::path::PathBuf> {
    use windows::Win32::UI::Controls::Dialogs::{GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW};
    let mut buf = [0u16; 1024];
    let filter: Vec<u16> = "Images (*.jpg;*.jpeg;*.png;*.webp;*.gif)\0*.jpg;*.jpeg;*.png;*.webp;*.gif\0\0".encode_utf16().collect();
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    unsafe { GetOpenFileNameW(&mut ofn) }.as_bool().then(|| {
        let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
        std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_colors_are_sanitized() {
        assert_eq!(css_color(" #1a73e8 ").as_deref(), Some("#1a73e8"));
        assert_eq!(css_color("rgb(12, 34, 56)").as_deref(), Some("rgb(12, 34, 56)"));
        assert!(css_color("red;background:url(x)").is_none());
        assert!(css_color("").is_none());
        assert!(css_color(&"a".repeat(41)).is_none());
    }

    #[test]
    fn url_encoding() {
        assert_eq!(url_enc("Montréal"), "Montr%C3%A9al");
        assert_eq!(url_enc("New York"), "New%20York");
    }
}
