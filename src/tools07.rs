// Plume 0.7 · Outils (#32–#38) : PiP automatique, contrôle média, mode focus + Pomodoro, pipette et règle,
// export PDF / Markdown, lire plus tard hors ligne. (Le QR code #36 est généré dans l'interface.)
use super::*;
use std::sync::RwLock;
use webview2_com::{CallDevToolsProtocolMethodCompletedHandler, CapturePreviewCompletedHandler, IsDocumentPlayingAudioChangedEventHandler, PrintToPdfCompletedHandler};
use windows::Win32::{
    System::Com::{IStream, STREAM_SEEK_SET},
    UI::Shell::SHCreateMemStream,
};

/// Contenu d'un flux mémoire (capture de la page).
fn stream_bytes(s: &IStream) -> Vec<u8> {
    let mut out = vec![];
    unsafe {
        if s.Seek(0, STREAM_SEEK_SET, None).is_err() {
            return out;
        }
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let mut n = 0u32;
            if s.Read(buf.as_mut_ptr() as *mut _, buf.len() as u32, Some(&mut n)).is_err() || n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n as usize]);
        }
    }
    out
}

/* ---------- #34 Sites bloqués pendant une session de focus (lu par NavigationStarting, hors de App) ---------- */
pub struct Focus {
    pub sites: Vec<String>,
    /// « 5 minutes de plus » : domaine → fin du sursis
    pub snooze: Vec<(String, Instant)>,
}
pub static FOCUS: RwLock<Option<Focus>> = RwLock::new(None);

/// Le site est-il bloqué en ce moment ? (domaine ou sous-domaine d'un site de la liste, hors sursis)
pub fn focus_blocked(host: &str) -> bool {
    let Ok(f) = FOCUS.read() else { return false };
    let Some(f) = f.as_ref() else { return false };
    let h = host.split(':').next().unwrap_or(host).trim_start_matches("www.");
    let hit = |k: &str| {
        let k = k.trim().trim_start_matches("www.");
        let k = k.split(':').next().unwrap_or(k); // sans le port
        !k.is_empty() && (h == k || h.ends_with(&format!(".{k}")))
    };
    f.sites.iter().any(|s| hit(s)) && !f.snooze.iter().any(|(s, t)| hit(s) && Instant::now() < *t)
}

/// Page de blocage douce (affichée à la place du site), dans la langue de Plume.
pub fn block_page(url: &str, host: &str, end_ms: u64, snooze: bool, fr: bool) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let (t1, t2, more, back, left) = if fr {
        ("On reste concentré", "est en pause pendant votre session de focus.", "5 minutes de plus", "Revenir en arrière", "Reviens dans")
    } else {
        ("Stay focused", "is paused during your focus session.", "5 more minutes", "Go back", "Come back in")
    };
    format!(
        r#"<!doctype html><html lang="{lang}"><head><meta charset="utf-8"><meta name="color-scheme" content="dark light"><title>{t1}</title><style>
:root{{color-scheme:dark light;--bg:#141416;--fg:#f5f5f7;--mut:#a1a1a6;--acc:#30d158}}
@media (prefers-color-scheme:light){{:root{{--bg:#f2f2f7;--fg:#1d1d1f;--mut:#6e6e73;--acc:#248a3d}}}}
html,body{{height:100%;margin:0;background:var(--bg);color:var(--fg);font:16px/1.5 "Segoe UI",system-ui,sans-serif}}
main{{height:100%;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:10px;text-align:center;padding:24px}}
.ring{{width:88px;height:88px;border-radius:50%;border:6px solid var(--acc);display:grid;place-items:center;font:700 22px "Segoe UI";margin-bottom:8px}}
h1{{margin:0;font:600 30px/1.2 "Segoe UI Variable Display","Segoe UI",sans-serif}} p{{margin:0;color:var(--mut)}} b{{color:var(--fg)}}
.acts{{display:flex;gap:10px;margin-top:22px}} button{{font:600 14px "Segoe UI";height:38px;padding:0 18px;border-radius:19px;border:0;cursor:pointer;background:rgba(127,127,127,.2);color:var(--fg)}}
button.pri{{background:var(--acc);color:#fff}} button:focus-visible{{outline:2px solid var(--acc);outline-offset:2px}}
</style></head><body><main><div class="ring" id="r">–</div><h1>{t1}</h1><p><b>{host}</b> {t2}</p><p>{left} <b id="m">–</b></p>
<div class="acts">{more_btn}<button class="pri" id="back">{back}</button></div></main><script>
const end={end_ms},u={url_js};
const post=(o)=>{{try{{window.ipc.postMessage(JSON.stringify(o))}}catch(e){{}}}};
function tick(){{const s=Math.max(0,Math.round((end-Date.now())/1000)),m=Math.ceil(s/60);document.getElementById('m').textContent=m+' min';document.getElementById('r').textContent=m;if(s<=0)location.replace(u);}}
tick();setInterval(tick,5000);
document.getElementById('back').onclick=()=>history.length>1?history.back():post({{c:'focus-close'}});
const mb=document.getElementById('more');if(mb)mb.onclick=()=>post({{c:'focus-more',u}});
</script></body></html>"#,
        lang = if fr { "fr" } else { "en" },
        host = esc(host),
        more_btn = if snooze { format!(r#"<button id="more">{more}</button>"#) } else { String::new() },
        url_js = serde_json::to_string(url).unwrap_or_else(|_| "\"\"".into()).replace("</", "<\\/"),
    )
}

/// Évalue du JS avec un « geste utilisateur » (PiP, pipette) via le protocole DevTools, sans le port de débogage.
unsafe fn eval_gesture(w: &ICoreWebView2, expr: &str, done: impl FnOnce(Value) + 'static) {
    let p = json!({"expression": expr, "awaitPromise": true, "userGesture": true, "returnByValue": true}).to_string();
    unsafe {
        let _ = w.CallDevToolsProtocolMethod(
            w!("Runtime.evaluate"),
            &HSTRING::from(p),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |_, out| {
                let v: Value = serde_json::from_str(&out).unwrap_or_default();
                done(v["result"]["value"].clone());
                Ok(())
            })),
        );
    }
}

/// Nom de fichier sûr à partir du titre d'une page.
pub fn file_name(title: &str, ext: &str) -> String {
    let mut n: String = title.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { ' ' } else { c }).collect();
    n = n.split_whitespace().collect::<Vec<_>>().join(" ");
    let n: String = n.trim_matches(['.', ' ']).chars().take(80).collect();
    format!("{}.{ext}", if n.is_empty() { "page".to_string() } else { n })
}

fn html_esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Boîte « Enregistrer sous » (PDF / Markdown).
fn save_dialog(owner: HWND, name: &str, filter: &str, ext: &str) -> Option<std::path::PathBuf> {
    use windows::Win32::UI::Controls::Dialogs::{GetSaveFileNameW, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OPENFILENAMEW};
    let mut buf = [0u16; 1024];
    for (i, c) in name.encode_utf16().take(1000).enumerate() {
        buf[i] = c;
    }
    let filter: Vec<u16> = filter.encode_utf16().collect();
    let ext: Vec<u16> = format!("{ext}\0").encode_utf16().collect();
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        lpstrDefExt: PCWSTR(ext.as_ptr()),
        Flags: OFN_OVERWRITEPROMPT | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    unsafe { GetSaveFileNameW(&mut ofn) }.as_bool().then(|| {
        let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
        std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n]))
    })
}

/// Fichier libre dans un dossier : « titre.pdf », sinon « titre (2).pdf »…
fn free_path(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    (2..1000).map(|i| dir.join(format!("{stem} ({i}).{ext}"))).find(|p| !p.exists()).unwrap_or(p)
}

const READER_CSS: &str = ":root{color-scheme:light dark;--bg:#fbfaf7;--fg:#1d1d1f;--mut:#6e6e73;--link:#0066cc}@media (prefers-color-scheme:dark){:root{--bg:#1c1c1e;--fg:#e8e8ed;--mut:#98989d;--link:#64a8ff}}html{background:var(--bg);color:var(--fg)}main{max-width:68ch;margin:0 auto;padding:28px 28px 120px;font:20px/1.65 Georgia,'Iowan Old Style',serif}h1{font:700 1.7em/1.2 system-ui,'Segoe UI',sans-serif;margin:.2em 0 .3em}.src{font:14px system-ui;color:var(--mut)}.src a{color:var(--mut)}img,figure{max-width:100%;height:auto;border-radius:10px}a{color:var(--link)}pre{white-space:pre-wrap;font-size:.8em}blockquote{border-left:3px solid var(--mut);margin-left:0;padding-left:1em;color:var(--mut)}";

/// Enregistrement d'un article pour plus tard (#38), hors du fil de l'interface : images téléchargées puis page autonome.
fn rl_write(dir: std::path::PathBuf, id: u64, v: Value, imgs_on: bool, date: String) -> std::result::Result<(), String> {
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut html = str_of(&v["html"]);
    let imgs: Vec<String> = v["imgs"].as_array().map(|l| l.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    for (n, u) in imgs.iter().enumerate().take(60) {
        let mut src = html_esc(u);
        if imgs_on && n < 40 && u.starts_with("http") {
            if let Ok(b) = update::http_get(u) {
                if !b.is_empty() && b.len() < 8 << 20 {
                    let ext = match b.get(..4) {
                        Some([0x89, b'P', b'N', b'G']) => "png",
                        Some([b'G', b'I', b'F', _]) => "gif",
                        Some([b'R', b'I', b'F', b'F']) => "webp",
                        Some([b'<', ..]) => "svg",
                        _ => "jpg",
                    };
                    let sub = dir.join(id.to_string());
                    let _ = std::fs::create_dir_all(&sub);
                    if std::fs::write(sub.join(format!("{n}.{ext}")), &b).is_ok() {
                        src = format!("{id}/{n}.{ext}");
                    }
                }
            }
        }
        html = html.replace(&format!("__PLUME_IMG_{n}__"), &src);
    }
    let (t, u) = (str_of(&v["t"]), str_of(&v["u"]));
    let site = import::host_of(&u);
    let page = format!(
        "<!doctype html><html lang=\"{lang}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src 'self' data: https: http:; style-src 'unsafe-inline'\"><title>{te}</title><style>{READER_CSS}</style></head><body><main><p class=\"src\"><a href=\"{ue}\">{se}</a> · {date}</p><h1>{te}</h1>{html}</main></body></html>",
        lang = html_esc(&str_of(&v["lang"])),
        te = html_esc(&t),
        ue = html_esc(&u),
        se = html_esc(&site),
    );
    std::fs::write(dir.join(format!("{id}.html")), page).map_err(|e| e.to_string())
}

impl App {
    /* ---------- #32 Picture-in-Picture automatique ---------- */
    /// On quitte l'onglet `i` : sa vidéo en lecture passe en PiP (avec du son si demandé, hors sites exclus).
    pub fn auto_pip(&mut self, i: usize) {
        let c = &self.cfg;
        let Some(t) = self.tabs.get(i) else { return };
        if c["autoPip"] != true || !t.url.starts_with("http") || strs(&c["pipExclude"]).iter().any(|h| priv07::kept(&[h.clone()], &import::host_of(&t.url))) {
            return;
        }
        let sound = c["pipSound"] != false;
        if sound && t.audio != 1 {
            return; // pas de son en cours : rien à faire (aucun appel)
        }
        let Some(w) = t.view.as_ref().map(|v| v.webview()) else { return };
        let (id, p) = (t.id, self.proxy.clone());
        self.tabs[i].pip = true; // protégé de la mise en veille tant que la page n'a pas répondu
        unsafe {
            eval_gesture(&w, &format!("window.__plume&&__plume.pipAuto&&__plume.pipAuto({sound})"), move |r| {
                if r != "on" {
                    let _ = p.send_event(Ev::Do(Box::new(move |app: &mut App| {
                        if let Some(k) = app.idx(id) {
                            app.tabs[k].pip = false;
                        }
                    })));
                }
            });
        }
    }

    /// Retour sur un onglet en PiP : la vidéo revient dans la page.
    pub fn pip_back(&mut self, i: usize) {
        if self.tabs[i].pip {
            self.tabs[i].pip = false;
            self.eval_tab(i, "document.pictureInPictureElement&&document.exitPictureInPicture().catch(()=>{})");
        }
    }

    /* ---------- #33 Contrôle média ---------- */
    pub fn hook_audio(v: &WebView, id: u32, proxy: &EventLoopProxy<Ev>) {
        let p = proxy.clone();
        unsafe {
            let Ok(c8) = v.webview().cast::<ICoreWebView2_8>() else { return };
            let mut tok = 0i64;
            let _ = c8.add_IsDocumentPlayingAudioChanged(
                &IsDocumentPlayingAudioChangedEventHandler::create(Box::new(move |s, _| {
                    if let Some(s) = s.and_then(|s| s.cast::<ICoreWebView2_8>().ok()) {
                        let mut b = BOOL(0);
                        s.IsDocumentPlayingAudio(&mut b)?;
                        let _ = p.send_event(Ev::Audio(id, b.as_bool()));
                    }
                    Ok(())
                })),
                &mut tok,
            );
        }
    }

    /// Son qui démarre ou s'arrête : 1 = en lecture, 2 = en pause après avoir joué (reste dans le contrôle média).
    pub fn on_audio(&mut self, id: u32, on: bool) {
        let Some(i) = self.idx(id) else { return };
        let t = &mut self.tabs[i];
        let a = if on { 1 } else if t.audio != 0 { 2 } else { 0 };
        if a != t.audio {
            t.audio = a;
            if self.cfg["mediaCtl"] == true {
                self.push();
            }
        }
    }

    pub fn set_mute(&mut self, i: usize, m: bool) {
        let Some(v) = &self.tabs[i].view else { return };
        unsafe {
            if let Ok(c8) = v.webview().cast::<ICoreWebView2_8>() {
                if c8.SetIsMuted(m).is_ok() {
                    self.tabs[i].muted = m;
                    self.tabs[i].auto_muted = false;
                }
            }
        }
    }

    /// Nouvel onglet ouvert en arrière-plan : coupé d'office si demandé (il retrouve le son à sa première visite).
    pub fn bg_mute(&mut self, i: usize) {
        if self.cfg["mediaCtl"] == true && self.cfg["autoMuteBg"] == true {
            self.set_mute(i, true);
            self.tabs[i].auto_muted = true;
        }
    }

    /* ---------- #34 Mode focus + Pomodoro ---------- */
    pub fn focus_start(&mut self, phase: &str) {
        let mins = if phase == "break" { self.cfg["breakMin"].as_u64().unwrap_or(5) } else { self.cfg["focusMin"].as_u64().unwrap_or(25) }.clamp(1, 180);
        let end = now_ms() + mins * 60_000;
        self.focus_gen += 1;
        let snooze = FOCUS.read().ok().and_then(|f| f.as_ref().map(|f| f.snooze.clone())).unwrap_or_default();
        *FOCUS.write().unwrap() = (phase == "focus").then(|| Focus { sites: strs(&self.cfg["focusSites"]), snooze });
        self.focus = json!({"phase": phase, "end": end});
        // minuteur : un fil qui dort jusqu'à la fin de la phase
        let (g, p) = (self.focus_gen, self.proxy.clone());
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(mins * 60_000));
            let _ = p.send_event(Ev::Do(Box::new(move |app: &mut App| app.focus_next(g))));
        });
        if phase == "focus" {
            // onglets déjà ouverts sur un site bloqué : page de blocage
            let ids: Vec<u32> = self.tabs.iter().filter(|t| t.view.is_some() && t.url.starts_with("http") && focus_blocked(&import::host_of(&t.url))).map(|t| t.id).collect();
            for id in ids {
                let u = self.idx(id).map(|i| self.tabs[i].url.clone()).unwrap_or_default();
                self.focus_block(id, &u);
            }
        }
        self.push();
    }

    fn focus_next(&mut self, g: u64) {
        if g != self.focus_gen || self.focus.is_null() {
            return; // minuteur d'une ancienne phase
        }
        let next = if self.focus["phase"] == "focus" { "break" } else { "focus" };
        self.focus_start(next);
        self.js(&format!("focusEnd({})", json!(next)));
    }

    pub fn focus_stop(&mut self) {
        self.focus_gen += 1;
        self.focus = Value::Null;
        *FOCUS.write().unwrap() = None;
        self.push();
    }

    /// Navigation vers un site bloqué : la page de blocage la remplace.
    pub fn focus_block(&mut self, id: u32, url: &str) {
        let Some(i) = self.idx(id) else { return };
        let end = self.focus["end"].as_u64().unwrap_or(0);
        let html = block_page(url, &import::host_of(url), end, self.cfg["focusSnooze"] != false, self.lang() == "fr");
        if let Some(v) = &self.tabs[i].view {
            unsafe {
                let _ = v.webview().NavigateToString(&HSTRING::from(html));
            }
        }
        self.stat_add("fb", 1.0);
    }

    fn focus_more(&mut self, i: usize, u: &str) {
        if self.cfg["focusSnooze"] == false {
            return;
        }
        let h = import::host_of(u);
        if let Ok(mut f) = FOCUS.write() {
            if let Some(f) = f.as_mut() {
                f.snooze.retain(|(_, t)| Instant::now() < *t);
                f.snooze.push((h, Instant::now() + Duration::from_secs(5 * 60)));
            }
        }
        if u.starts_with("http") {
            if let Some(v) = &self.tabs[i].view {
                let _ = v.load_url(u);
            }
        }
    }

    /* ---------- #35 Pipette ---------- */
    /// La pipette système (EyeDropper) est annulée d'office dans WebView2 : Plume capture la partie visible
    /// de la page et la page l'affiche par-dessus elle-même, avec une loupe ; un clic copie la couleur.
    pub fn dq_color(&mut self) {
        let i = self.active;
        if !self.tabs[i].url.starts_with("http") {
            return self.js("dqColor('unsupported')");
        }
        let Some(w) = self.tabs[i].view.as_ref().map(|v| v.webview()) else { return };
        let Some(stream) = (unsafe { SHCreateMemStream(None) }) else { return };
        if let Some(v) = &self.tabs[i].view {
            let _ = v.focus();
        }
        let (id, p, fmt, lang, s2) = (self.tabs[i].id, self.proxy.clone(), str_of(&self.cfg["dqFmt"]), self.lang().to_string(), stream.clone());
        unsafe {
            let _ = w.CapturePreview(
                COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
                &stream,
                &CapturePreviewCompletedHandler::create(Box::new(move |res| {
                    let png = if res.is_ok() { stream_bytes(&s2) } else { vec![] };
                    let _ = p.send_event(Ev::Do(Box::new(move |app: &mut App| {
                        let Some(i) = app.idx(id) else { return };
                        if png.is_empty() {
                            return app.js("dqColor('')");
                        }
                        app.eval_tab(i, &format!("window.__plume&&__plume.pickColor({},{},{})", json!(ext::b64(&png)), json!(fmt), json!(lang)));
                    })));
                    Ok(())
                })),
            );
        }
    }

    /* ---------- #37 Export PDF et Markdown ---------- */
    fn export_target(&self, title: &str, ext: &str, filter: &str) -> Option<std::path::PathBuf> {
        let name = file_name(title, ext);
        if self.cfg["expAsk"] != false {
            return save_dialog(HWND(self.window.hwnd() as _), &name, filter, ext);
        }
        let d = self.cfg["expDir"].as_str().map(str::trim).filter(|d| !d.is_empty()).map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::var_os("USERPROFILE").map(|u| std::path::PathBuf::from(u).join("Downloads")).unwrap_or_else(std::env::temp_dir));
        let _ = std::fs::create_dir_all(&d);
        Some(free_path(&d, &name))
    }

    pub fn export(&mut self, fmt: &str) {
        let i = self.active;
        if self.cfg["export"] != true || !self.tabs[i].url.starts_with("http") {
            return;
        }
        if fmt == "md" {
            let img = self.cfg["expImg"] != false;
            return self.eval_tab(i, &format!("window.__plume&&__plume.mdExport&&__plume.mdExport({img})"));
        }
        let title = self.tabs[i].title.clone();
        let Some(path) = self.export_target(&title, "pdf", "PDF (*.pdf)\0*.pdf\0\0") else { return };
        let Some(v) = &self.tabs[i].view else { return };
        let p = self.proxy.clone();
        let shown = path.display().to_string();
        unsafe {
            let Ok(c7) = v.webview().cast::<ICoreWebView2_7>() else { return };
            self.toast("exp-busy");
            let _ = c7.PrintToPdf(
                &HSTRING::from(path.as_path()),
                None,
                &PrintToPdfCompletedHandler::create(Box::new(move |res, ok| {
                    let ok = res.is_ok() && ok;
                    let _ = p.send_event(Ev::Do(Box::new(move |app: &mut App| app.js(&format!("expDone({})", json!({"ok": ok, "path": shown}))))));
                    Ok(())
                })),
            );
        }
    }

    fn export_md(&mut self, v: &Value) {
        let title = str_of(&v["t"]);
        let Some(path) = self.export_target(&title, "md", "Markdown (*.md)\0*.md\0\0") else { return };
        let ok = std::fs::write(&path, str_of(&v["md"])).is_ok();
        self.js(&format!("expDone({})", json!({"ok": ok, "path": path.display().to_string()})));
    }

    /* ---------- #38 Lire plus tard (hors ligne) ---------- */
    pub fn rl_dir() -> std::path::PathBuf {
        setup::data_dir().join("reading")
    }

    fn rl_index(&self) -> Vec<Value> {
        tabs07::load_json("reading.json", json!([])).as_array().cloned().unwrap_or_default()
    }

    /// Liste, après avoir retiré les articles lus depuis plus de N jours (si demandé).
    pub fn rl_list(&mut self) {
        let mut l = self.rl_index();
        let days = self.cfg["rlPurge"].as_u64().unwrap_or(0);
        if days > 0 {
            let lim = now_ms().saturating_sub(days * 86_400_000);
            let (old, keep): (Vec<Value>, Vec<Value>) = l.into_iter().partition(|x| x["read"].as_u64().is_some_and(|r| r < lim));
            for x in &old {
                self.rl_files_rm(x["id"].as_u64().unwrap_or(0));
            }
            if !old.is_empty() {
                tabs07::save_json("reading.json", &json!(keep));
            }
            l = keep;
        }
        self.js(&format!("rlRes({})", json!(l)));
    }

    fn rl_files_rm(&self, id: u64) {
        if id == 0 {
            return;
        }
        let d = Self::rl_dir();
        let _ = std::fs::remove_file(d.join(format!("{id}.html")));
        let _ = std::fs::remove_dir_all(d.join(id.to_string()));
    }

    /// La page a extrait son article : il est enregistré avec ses images (dans un fil), puis ajouté à la liste.
    fn rl_save(&mut self, i: usize, v: &Value) {
        if self.cfg["readLater"] != true || str_of(&v["html"]).trim().is_empty() {
            return self.toast("rl-fail");
        }
        let u = str_of(&v["u"]);
        let mut l = self.rl_index();
        if l.iter().any(|x| x["u"] == u.as_str()) {
            return self.toast("rl-already");
        }
        let id = now_ms();
        l.insert(0, json!({"id": id, "u": u, "t": str_of(&v["t"]).chars().take(300).collect::<String>(), "ex": str_of(&v["ex"]).chars().take(300).collect::<String>(),
                           "site": import::host_of(&u), "saved": id, "read": Value::Null, "n": v["n"], "f": self.tabs[i].fav}));
        tabs07::save_json("reading.json", &json!(l));
        let (imgs, p, v, date) = (self.cfg["rlImages"] != false, self.proxy.clone(), v.clone(), today());
        self.toast("rl-saving");
        std::thread::spawn(move || {
            let ok = rl_write(Self::rl_dir(), id, v, imgs, date).is_ok();
            let _ = p.send_event(Ev::Do(Box::new(move |app: &mut App| app.toast(if ok { "rl-saved" } else { "rl-fail" }))));
        });
    }

    /// Ouvrir un article : version hors ligne (https://reading.plume/…) ou l'adresse d'origine.
    fn rl_open(&mut self, id: u64) {
        let l = self.rl_index();
        let Some(x) = l.iter().find(|x| x["id"] == id) else { return };
        let offline = Self::rl_dir().join(format!("{id}.html"));
        let u = if self.cfg["rlReader"] != false && offline.exists() { format!("https://reading.plume/{id}.html") } else { str_of(&x["u"]) };
        self.new_tab(Some(u), true, false);
    }

    /* ---------- Messages ---------- */
    pub fn on_ui_tools(&mut self, c: &str, v: &Value) {
        let tab = v["id"].as_u64().and_then(|id| self.idx(id as u32));
        match c {
            "mute" => {
                if let Some(i) = tab {
                    let m = !self.tabs[i].muted;
                    self.set_mute(i, m);
                    self.push();
                }
            }
            "media-act" => {
                if let Some(i) = tab {
                    let a = v["a"].as_str().unwrap_or("toggle");
                    if matches!(a, "toggle" | "next" | "prev") {
                        self.eval_tab(i, &format!("window.__plume&&__plume.mediaCtl&&__plume.mediaCtl('{a}')"));
                    }
                }
            }
            "focus-start" => {
                if self.cfg["focus"] == true {
                    self.focus_start("focus")
                }
            }
            "focus-stop" => self.focus_stop(),
            "focus-skip" => {
                if !self.focus.is_null() {
                    let next = if self.focus["phase"] == "focus" { "break" } else { "focus" };
                    self.focus_start(next);
                }
            }
            "dq-color" => self.dq_color(),
            "dq-ruler" => {
                let i = self.active;
                if self.tabs[i].url.starts_with("http") {
                    if let Some(w) = &self.tabs[i].view {
                        let _ = w.focus();
                    }
                    self.eval_tab(i, &format!("window.__plume&&__plume.ruler&&__plume.ruler({})", json!(self.lang())));
                }
            }
            "export" => {
                let f = v["fmt"].as_str().map(String::from).unwrap_or_else(|| str_of(&self.cfg["expFmt"]));
                self.export(&f);
            }
            "rl-save" => {
                let i = self.active;
                if self.cfg["readLater"] == true && self.tabs[i].url.starts_with("http") {
                    self.eval_tab(i, "window.__plume&&__plume.rlExtract&&__plume.rlExtract()");
                }
            }
            "rl-list" => self.rl_list(),
            "rl-open" => self.rl_open(v["rid"].as_u64().unwrap_or(0)),
            "rl-read" => {
                let (rid, read) = (v["rid"].as_u64().unwrap_or(0), v["read"] == true);
                let mut l = self.rl_index();
                if let Some(x) = l.iter_mut().find(|x| x["id"] == rid) {
                    x["read"] = if read { json!(now_ms()) } else { Value::Null };
                }
                tabs07::save_json("reading.json", &json!(l));
                self.rl_list();
            }
            "rl-del" => {
                let rid = v["rid"].as_u64().unwrap_or(0);
                let mut l = self.rl_index();
                l.retain(|x| x["id"] != rid);
                tabs07::save_json("reading.json", &json!(l));
                self.rl_files_rm(rid);
                self.rl_list();
            }
            _ => {}
        }
    }

    pub fn on_page_tools(&mut self, i: usize, c: &str, v: &Value) {
        match c {
            "pip" => {
                self.tabs[i].pip = v["on"] == true;
                if !self.tabs[i].pip && i != self.active && self.tabs[i].audio != 1 {
                    self.sleep(i); // la vidéo est revenue dans un onglet caché : il peut dormir
                }
            }
            "focus-more" => self.focus_more(i, &str_of(&v["u"])),
            "focus-close" => self.close_tab(i),
            // lien ouvert en arrière-plan : même conteneur que la page (sauf site associé à un autre)
            "open-bg" => {
                let u = str_of(&v["u"]);
                if u.starts_with("http") {
                    let (oc, p) = (self.tabs[i].cont, self.tabs[i].private);
                    self.open_cont = self.cont_site(&u).or((oc > 0).then_some(oc));
                    self.new_tab(Some(u), false, p);
                    self.push();
                }
            }
            "md-out" => self.export_md(v),
            "dq-picked" => self.js(&format!("dqColor({},{})", json!(str_of(&v["v"])), v["copied"] == true)),
            "rl-data" => self.rl_save(i, v),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_list_matching_and_snooze() {
        *FOCUS.write().unwrap() = Some(Focus { sites: vec!["youtube.com".into(), "x.com".into()], snooze: vec![] });
        assert!(focus_blocked("www.youtube.com") && focus_blocked("m.youtube.com") && focus_blocked("x.com"));
        assert!(!focus_blocked("notyoutube.com") && !focus_blocked("example.org"));
        FOCUS.write().unwrap().as_mut().unwrap().snooze.push(("youtube.com".into(), Instant::now() + Duration::from_secs(60)));
        assert!(!focus_blocked("youtube.com") && focus_blocked("x.com"));
        // le sursis enregistré avec un port vaut pour le site
        FOCUS.write().unwrap().as_mut().unwrap().snooze.push(("x.com:8080".into(), Instant::now() + Duration::from_secs(60)));
        assert!(!focus_blocked("x.com"));
        *FOCUS.write().unwrap() = None;
        assert!(!focus_blocked("youtube.com"));
    }

    #[test]
    fn export_file_names() {
        assert_eq!(file_name("Hello: World? <test>", "pdf"), "Hello World test.pdf");
        assert_eq!(file_name("   ", "md"), "page.md");
        assert_eq!(file_name(&"a".repeat(200), "md").len(), 83);
    }

    #[test]
    fn block_page_escapes() {
        let h = block_page("https://x.com/\"</script>", "x.com<b>", 0, true, false);
        assert!(h.contains("x.com&lt;b&gt;") && !h.contains("</script>\"") && h.contains("5 more minutes"));
    }
}
