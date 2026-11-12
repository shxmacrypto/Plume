//! Les boîtes de dialogue dessinées dans la fenêtre : ajouter ou régler un compte, écrire un message, confirmer.
//! Leurs champs partagent le champ de saisie de Windows que prête le kit : il se pose sur celui que l'on édite.

use super::{App, Editing, Id, K_ESCAPE, K_RETURN, K_TAB, TOP, text_of};
use mail_core::{
    engine::Command,
    message::{self, Draft},
    net::Security,
    oauth,
    store::{Account, SignIn},
};
use plume_ui::{Align, Canvas, Color, Cx, EditStyle, Font, Mods, Rect, Theme, m, rect};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// Où en est la connexion par la page de Microsoft.
pub enum OAuth {
    Idle,
    /// La page est ouverte dans le navigateur ; `cancel` arrête l'attente.
    Waiting {
        /// La page ouverte (les essais automatiques jouent le navigateur avec).
        #[cfg_attr(not(feature = "test-hook"), allow(dead_code))]
        url: String,
        cancel: Arc<AtomicBool>,
    },
    /// Connecté : le jeton qui sert à renouveler l'accès.
    Ready(String),
}

/// D'où part un message que l'on écrit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    New,
    Reply { all: bool },
    Forward,
}

pub struct AccountForm {
    /// Nom, adresse, mot de passe, serveur de réception, serveur d'envoi.
    pub fields: [String; 5],
    pub active: usize,
    pub error: Option<String>,
    /// Le compte que l'on règle (None : un nouveau).
    pub editing: Option<usize>,
    pub reveal: bool,
    /// Les serveurs ont été écrits à la main : on ne les devine plus à partir de l'adresse.
    pub own_servers: bool,
    /// Un compte Microsoft : la connexion par sa page, à la place du mot de passe.
    pub oauth: OAuth,
}

pub struct ComposeForm {
    pub account: usize,
    /// À, Cc, objet, texte.
    pub fields: [String; 4],
    /// Ce que contenait la boîte à l'ouverture (ou au dernier brouillon gardé) : y a-t-il du neuf à garder ?
    pub initial: [String; 4],
    pub initial_files: usize,
    /// Le brouillon gardé sur le serveur que ce message remplacera : (dossier, numéro).
    pub draft: Option<(String, u32)>,
    /// Le brouillon est en route vers le serveur ; `close_after` : la boîte se ferme ensuite.
    pub saving: bool,
    pub close_after: bool,
    /// Le brouillon n'a pas pu être gardé : Annuler ferme maintenant sans le garder.
    pub discard: bool,
    pub active: usize,
    /// Pièces jointes : (nom, contenu).
    pub files: Vec<(String, Vec<u8>)>,
    pub error: Option<String>,
    /// Le message est en route : la boîte attend la réponse du serveur.
    pub sending: bool,
    pub in_reply_to: String,
    pub references: Vec<String>,
}

pub enum Confirm {
    RemoveAccount(usize),
    DeleteMessage { account: usize, folder: String, uid: u32 },
}

pub enum Sheet {
    Account(AccountForm),
    Compose(ComposeForm),
    /// La signature d'un compte, ajoutée sous les messages qu'il écrit.
    Signature {
        account: usize,
        text: String,
    },
    Confirm {
        title: String,
        text: String,
        action: String,
        what: Confirm,
    },
}

/// Les rectangles de la boîte ouverte, les mêmes pour le dessin et pour le champ de saisie.
pub struct SheetLay {
    pub card: Rect,
    pub fields: Vec<Rect>,
    pub ok: Rect,
    pub cancel: Rect,
    /// « Afficher » le mot de passe, ou « Joindre des fichiers ».
    pub extra: Rect,
}

/// « hôte:port » : le serveur et son port.
fn server(text: &str) -> Option<(String, u16)> {
    let (host, port) = text.trim().rsplit_once(':')?;
    let ok = !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    (ok && port.parse::<u16>().is_ok_and(|p| p > 0)).then(|| (host.to_ascii_lowercase(), port.parse().unwrap_or(0)))
}

/// La protection d'une connexion d'après son port : chiffrée dès le début sur le port prévu pour, sinon chiffrée
/// avant de s'identifier. Pendant les essais automatiques, PLUME_MAIL_TEST_SECURITY dit comment parler au serveur
/// d'essai de ce PC, quel que soit son port : « plain » (sans chiffrement) ou « tls ».
fn security(port: u16, encrypted: u16) -> Security {
    #[cfg(feature = "test-hook")]
    if let Ok(forced) = std::env::var("PLUME_MAIL_TEST_SECURITY") {
        return Security::parse(&forced);
    }
    if port == encrypted { Security::Tls } else { Security::StartTls }
}

impl App {
    // ───────────────────────── ouvrir une boîte ─────────────────────────

    /// La boîte « Ajouter un compte », ou les réglages du compte `editing`.
    pub fn ask_account(&mut self, editing: Option<usize>) {
        self.end_edit();
        self.menu = None;
        let fields = match editing.and_then(|b| self.boxes.get(b)) {
            Some(mb) => {
                let a = &mb.account;
                [a.name.clone(), a.email.clone(), String::new(), format!("{}:{}", a.imap_host, a.imap_port), format!("{}:{}", a.smtp_host, a.smtp_port)]
            }
            None => Default::default(),
        };
        self.toast = None;
        self.sheet =
            Some(Sheet::Account(AccountForm { fields, active: 0, error: None, editing, reveal: false, own_servers: editing.is_some(), oauth: OAuth::Idle }));
        self.open_field(0, None);
    }

    /// La boîte « Nouveau message » : vide, ou préparée pour répondre au message ouvert ou le transférer.
    pub fn compose(&mut self, start: Start) {
        let s = self.s;
        let Some(account) = self.cur.as_ref().map(|c| c.0).or((!self.boxes.is_empty()).then_some(0)) else { return self.ask_account(None) };
        let mut form = ComposeForm {
            account,
            fields: Default::default(),
            initial: Default::default(),
            initial_files: 0,
            draft: None,
            saving: false,
            close_after: false,
            discard: false,
            active: 0,
            files: vec![],
            error: None,
            sending: false,
            in_reply_to: String::new(),
            references: vec![],
        };
        let signature = self.signature_block(account);
        form.fields[3] = signature.clone();
        if start != Start::New {
            let Some(msg) = self.reading.as_ref().and_then(|r| r.message.as_ref()) else { return };
            let me = self.boxes[account].account.email.to_lowercase();
            let sum = &msg.summary;
            let sender = if sum.from_name.is_empty() { sum.from.clone() } else { format!("{} <{}>", sum.from_name, sum.from) };
            let others = |field: &str| {
                message::addresses(field).into_iter().filter(|a| a.to_lowercase() != me && !a.eq_ignore_ascii_case(&sum.from)).collect::<Vec<_>>().join(", ")
            };
            let who = if sum.from_name.is_empty() { sum.from.as_str() } else { sum.from_name.as_str() };
            let quoted = message::quote(&s.fill(s.wrote, &[who]), &self.date_text(sum.date, true), &msg.text);
            match start {
                Start::Reply { all } => {
                    let to = if msg.reply_to.is_empty() { sender } else { msg.reply_to.clone() };
                    let more = if all { others(&sum.to) } else { String::new() };
                    form.fields[0] = if more.is_empty() { to } else { format!("{to}, {more}") };
                    form.fields[1] = if all { others(&msg.cc) } else { String::new() };
                    form.fields[2] = message::subject_with("Re:", &sum.subject);
                    form.in_reply_to = sum.message_id.clone();
                    form.references = sum.references.clone();
                    form.active = 3;
                }
                _ => form.fields[2] = message::subject_with("Fwd:", &sum.subject),
            }
            // la signature sous la réponse, le message cité en dessous
            form.fields[3] = format!("{signature}{quoted}");
        }
        form.initial = form.fields.clone();
        let first = form.active;
        // pour répondre, on écrit au-dessus de la signature et du message cité ; un nouveau message commence par « À »
        self.open_compose(form, (first == 3).then_some((0, 0)));
    }

    /// Le compte de la boîte « Ajouter un compte » se connecte par la page de Microsoft (d'après son adresse, ou
    /// son serveur de réception).
    pub fn microsoft_form(f: &AccountForm) -> bool {
        Account::guess("", &f.fields[1]).needs_web_sign_in() || server(&f.fields[3]).is_some_and(|(host, _)| host == "outlook.office365.com")
    }

    /// Ouvre la page de Microsoft dans le navigateur et attend qu'il revienne (sur un autre fil) ; un second clic
    /// pendant l'attente l'annule.
    fn sign_in(&mut self, cx: &mut Cx) {
        let s = self.s;
        let Some(Sheet::Account(f)) = self.sheet.as_mut() else { return };
        if let OAuth::Waiting { cancel, .. } = &f.oauth {
            cancel.store(true, Ordering::Relaxed);
            f.oauth = OAuth::Idle;
            return;
        }
        let Some(provider) = oauth::microsoft() else { return self.sheet_error(s.account_web_sign_in) };
        let email = f.fields[1].trim().to_string();
        if message::addresses(&email).len() != 1 {
            return self.sheet_error(s.account_bad_email);
        }
        let pending = match oauth::begin(&provider, &email) {
            Ok(p) => p,
            Err(e) => return self.sheet_error(&e.to_string()),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let url = pending.url.clone();
        f.oauth = OAuth::Waiting { url: url.clone(), cancel: cancel.clone() };
        f.error = None;
        // essais automatiques : le script joue le navigateur
        let browser = !(cfg!(feature = "test-hook") && std::env::var_os("PLUME_MAIL_TEST_OAUTH").is_some());
        if browser {
            cx.after(move |w| {
                w.open(&url);
            });
        }
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let result = oauth::finish(&provider, pending, Duration::from_secs(600), &cancel);
            sender.send(move |h, cx| {
                if let Some(app) = h.as_any().downcast_mut::<App>() {
                    app.signed_in(result);
                    cx.repaint();
                }
            });
        });
    }

    /// Le navigateur est revenu (ou la connexion a échoué, ou a été annulée).
    fn signed_in(&mut self, result: Result<oauth::Tokens, String>) {
        let Some(Sheet::Account(f)) = self.sheet.as_mut() else { return };
        if !matches!(f.oauth, OAuth::Waiting { .. }) {
            return;
        }
        match result {
            Ok(tokens) if !tokens.refresh.is_empty() => {
                f.oauth = OAuth::Ready(tokens.refresh);
                f.error = None;
            }
            Ok(_) => {
                f.oauth = OAuth::Idle;
                f.error = Some("Microsoft gave no lasting sign-in (offline_access).".into());
            }
            Err(why) if why == "cancelled" => f.oauth = OAuth::Idle,
            Err(why) => {
                f.oauth = OAuth::Idle;
                f.error = Some(why);
            }
        }
        // le navigateur a pris la place : la fenêtre revient au premier plan (pas pendant les essais automatiques)
        if std::env::var_os("PLUME_MAIL_NOACTIVATE").is_none() {
            self.win.bring_to_front();
        }
    }

    /// Ferme la boîte ouverte. Un message commencé n'est pas perdu : il part dans les brouillons du serveur, et la
    /// boîte se ferme quand il y est.
    pub fn close_sheet(&mut self) {
        if let Some(Sheet::Compose(f)) = self.sheet.as_ref()
            && !f.sending
            && !f.discard
            && (f.fields != f.initial || f.files.len() != f.initial_files)
        {
            if !f.saving {
                self.save_draft(true);
            }
            return self.win.repaint();
        }
        // une connexion Microsoft en attente s'arrête avec la boîte
        if let Some(Sheet::Account(AccountForm { oauth: OAuth::Waiting { cancel, .. }, .. })) = self.sheet.as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.sheet = None;
        self.suggest.clear();
        self.end_edit();
        self.win.repaint();
    }

    /// Le bloc de signature d'un compte, à mettre sous le texte (vide : pas de signature).
    fn signature_block(&self, account: usize) -> String {
        let sig = self.boxes.get(account).map(|mb| mb.account.signature.trim_end().to_string()).unwrap_or_default();
        if sig.is_empty() { String::new() } else { format!("\n\n-- \n{sig}") }
    }

    /// Reprend le brouillon ouvert (dans le dossier « Brouillons ») dans la boîte « Nouveau message ».
    pub fn edit_draft(&mut self) {
        let Some(r) = self.reading.as_ref() else { return };
        let Some(msg) = r.message.as_ref() else { return };
        let (account, folder, uid) = (r.account, r.folder.clone(), r.uid);
        let sum = &msg.summary;
        let mut references = sum.references.clone();
        let in_reply_to = references.pop().unwrap_or_default();
        // les pièces jointes du brouillon repartent avec lui
        let raw = self.boxes.get(account).and_then(|mb| self.store.body(&mb.account.id, &folder, uid));
        let files: Vec<(String, Vec<u8>)> =
            msg.attachments.iter().filter_map(|a| Some((a.name.clone(), message::attachment(raw.as_deref()?, a.index)?))).collect();
        let fields = [sum.to.clone(), msg.cc.clone(), sum.subject.clone(), msg.text.clone()];
        let form = ComposeForm {
            account,
            initial: fields.clone(),
            initial_files: files.len(),
            fields,
            active: 3,
            files,
            error: None,
            sending: false,
            saving: false,
            close_after: false,
            discard: false,
            in_reply_to,
            references,
            draft: Some((folder, uid)),
        };
        self.open_compose(form, Some((0, 0)));
    }

    /// Ouvre la boîte « Nouveau message » préparée, le curseur dans son champ actif.
    fn open_compose(&mut self, form: ComposeForm, select: Option<(usize, usize)>) {
        let first = form.active;
        self.contacts = self.boxes.get(form.account).map(|mb| self.store.contacts(&mb.account.id, &mb.account.email)).unwrap_or_default();
        self.suggest.clear();
        self.end_edit();
        self.menu = None;
        self.toast = None;
        self.sheet = Some(Sheet::Compose(form));
        self.open_field(first, select);
    }

    /// La boîte de la signature d'un compte.
    pub fn ask_signature(&mut self, account: usize) {
        let Some(mb) = self.boxes.get(account) else { return };
        let text = mb.account.signature.clone();
        self.end_edit();
        self.menu = None;
        self.sheet = Some(Sheet::Signature { account, text });
        self.open_field(0, None);
    }

    /// Garde le message en cours dans les brouillons du serveur. `close` : la boîte se ferme une fois qu'il y est.
    pub fn save_draft(&mut self, close: bool) {
        let Some(Sheet::Compose(f)) = self.sheet.as_ref() else { return };
        if f.sending || f.saving {
            return;
        }
        let Some(mb) = self.boxes.get(f.account) else { return };
        let [to, cc, subject, text] = f.fields.clone();
        let draft = Draft {
            from_name: mb.account.name.clone(),
            from: mb.account.email.clone(),
            to,
            cc,
            bcc: String::new(),
            subject,
            text,
            in_reply_to: f.in_reply_to.clone(),
            references: f.references.clone(),
            files: f.files.clone(),
        };
        mb.engine.send(Command::SaveDraft { draft: Box::new(draft), replace: f.draft.clone() });
        if let Some(Sheet::Compose(f)) = self.sheet.as_mut() {
            f.saving = true;
            f.close_after = close;
            f.error = None;
        }
        self.suggest.clear();
        self.end_edit();
    }

    /// Le serveur a gardé le brouillon (ou pas).
    pub fn on_draft_saved(&mut self, result: Result<(String, Option<u32>), String>) {
        let s = self.s;
        let Some(Sheet::Compose(f)) = self.sheet.as_mut().filter(|sheet| matches!(sheet, Sheet::Compose(f) if f.saving)) else {
            // la boîte n'est plus là : on dit seulement ce qu'il en est
            match result {
                Ok(_) => self.toast(s.draft_saved.to_string()),
                Err(why) => self.toast(s.fill(s.draft_failed, &[&why])),
            }
            return;
        };
        f.saving = false;
        match result {
            Ok((folder, uid)) => {
                f.draft = uid.map(|u| (folder, u));
                f.initial = f.fields.clone();
                f.initial_files = f.files.len();
                if f.close_after {
                    self.sheet = None;
                    self.end_edit();
                } else {
                    let active = f.active;
                    self.open_field(active, None);
                }
                self.toast(s.draft_saved.to_string());
            }
            Err(why) => {
                // rien n'est perdu : la boîte reste ouverte ; Annuler une seconde fois ferme sans garder
                f.error = Some(format!("{} {}", s.fill(s.draft_failed, &[&why]), if f.close_after { s.draft_discard } else { "" }).trim().to_string());
                f.discard = f.close_after;
                let active = f.active;
                self.open_field(active, None);
            }
        }
    }

    /// Les adresses connues qui commencent comme ce que l'on tape dans le champ d'adresse `i`.
    fn update_suggestions(&mut self, i: usize) {
        self.suggest.clear();
        self.suggest_at = 0;
        let Some(Sheet::Compose(f)) = self.sheet.as_ref() else { return };
        if i > 1 {
            return;
        }
        let text = &f.fields[i];
        let typed = text.rsplit([',', ';']).next().unwrap_or_default().trim();
        if typed.chars().count() < 2 {
            return;
        }
        let already: Vec<String> = message::addresses_lenient(text).into_iter().map(|a| a.to_lowercase()).collect();
        self.suggest = mail_core::store::matching(&self.contacts, typed, 12)
            .into_iter()
            .filter(|c| !already.contains(&c.email.to_lowercase()) || c.email.eq_ignore_ascii_case(typed))
            .filter(|c| !c.email.eq_ignore_ascii_case(typed))
            .take(5)
            .cloned()
            .collect();
    }

    /// Met l'adresse proposée `k` à la place de ce qui était en train d'être tapé.
    fn accept_suggestion(&mut self, k: usize) {
        let Some(c) = self.suggest.get(k).cloned() else { return };
        let Some(Sheet::Compose(f)) = self.sheet.as_mut() else { return };
        let i = f.active.min(1);
        let text = &f.fields[i];
        let cut = text.rfind([',', ';']).map_or(0, |p| p + 1);
        let before = text[..cut].trim_end();
        let who = if c.name.is_empty() { c.email.clone() } else { format!("{} <{}>", c.name, c.email) };
        f.fields[i] = if before.is_empty() { format!("{who}, ") } else { format!("{before} {who}, ") };
        self.suggest.clear();
        self.open_field(i, None);
    }

    // ───────────────────────── les champs ─────────────────────────

    pub fn sheet_lay(&self) -> Option<SheetLay> {
        let (w, h) = (self.lay.w, self.lay.h);
        let pad = 24.0;
        Some(match self.sheet.as_ref()? {
            Sheet::Account(_) => {
                let (cw, ch) = (460.0f32.min(w - 32.0).max(280.0), 474.0);
                let card = rect(((w - cw) / 2.0).round(), ((h - ch) * 0.4).round().max(m::TITLE_H + 6.0), cw, ch);
                let fields: Vec<Rect> = (0..5).map(|i| rect(card.x + pad, card.y + 78.0 + i as f32 * 62.0 + 18.0, cw - 2.0 * pad, 34.0)).collect();
                let by = card.bottom() - 24.0 - 36.0;
                let ok = rect(card.right() - pad - 120.0, by, 120.0, 36.0);
                let extra = rect(fields[2].right() - 82.0, fields[2].y + 4.0, 78.0, 26.0);
                SheetLay { card, fields, ok, cancel: rect(ok.x - 12.0 - 100.0, by, 100.0, 36.0), extra }
            }
            Sheet::Compose(_) => {
                let (cw, ch) = ((w - 64.0).clamp(320.0, 780.0), (h - m::TITLE_H - 28.0).clamp(300.0, 640.0));
                let card = rect(((w - cw) / 2.0).round(), (m::TITLE_H + ((h - m::TITLE_H - ch) / 2.0)).round().max(m::TITLE_H + 4.0), cw, ch);
                let label = 74.0;
                let mut fields: Vec<Rect> = (0..3).map(|i| rect(card.x + pad + label, card.y + 58.0 + i as f32 * 40.0, cw - 2.0 * pad - label, 32.0)).collect();
                let by = card.bottom() - 24.0 - 36.0;
                let top = card.y + 58.0 + 3.0 * 40.0 + 6.0;
                fields.push(rect(card.x + pad, top, cw - 2.0 * pad, (by - 30.0 - top).max(60.0)));
                let ok = rect(card.right() - pad - 120.0, by, 120.0, 36.0);
                SheetLay { card, fields, ok, cancel: rect(ok.x - 12.0 - 100.0, by, 100.0, 36.0), extra: rect(card.x + pad, by, 168.0, 36.0) }
            }
            Sheet::Signature { .. } => {
                let (cw, ch) = (520.0f32.min(w - 32.0).max(280.0), 330.0);
                let card = rect(((w - cw) / 2.0).round(), ((h - ch) * 0.4).round().max(m::TITLE_H + 6.0), cw, ch);
                let by = card.bottom() - 24.0 - 36.0;
                let field = rect(card.x + pad, card.y + 70.0, cw - 2.0 * pad, (by - 34.0 - card.y - 70.0).max(60.0));
                let ok = rect(card.right() - pad - 120.0, by, 120.0, 36.0);
                SheetLay { card, fields: vec![field], ok, cancel: rect(ok.x - 12.0 - 100.0, by, 100.0, 36.0), extra: Rect::default() }
            }
            Sheet::Confirm { .. } => {
                let (cw, ch) = (430.0f32.min(w - 32.0).max(260.0), 168.0);
                let card = rect(((w - cw) / 2.0).round(), ((h - ch) * 0.38).round().max(TOP), cw, ch);
                let by = card.bottom() - 24.0 - 36.0;
                let ok = rect(card.right() - pad - 120.0, by, 120.0, 36.0);
                SheetLay { card, fields: vec![], ok, cancel: rect(ok.x - 12.0 - 100.0, by, 100.0, 36.0), extra: Rect::default() }
            }
        })
    }

    /// La zone où se pose le champ de saisie pour le champ `i` de la boîte ouverte, et son style.
    fn field_box(&self, i: usize) -> Option<(Rect, EditStyle, String)> {
        let l = self.sheet_lay()?;
        let s = self.s;
        Some(match self.sheet.as_ref()? {
            Sheet::Account(f) => {
                let secret = i == 2;
                let hint = if secret && f.editing.is_some() { s.password_keep } else { "" };
                let mut area = text_of(*l.fields.get(i)?);
                if secret {
                    area.w -= 84.0;
                }
                (area, self.edit_style(hint, secret && !f.reveal, false), f.fields[i].clone())
            }
            Sheet::Compose(f) => {
                let body = i == 3;
                let area = if body { l.fields.get(i)?.inset(8.0) } else { text_of(*l.fields.get(i)?) };
                (area, self.edit_style(if body { s.body_placeholder } else { "" }, false, body), f.fields.get(i)?.clone())
            }
            Sheet::Signature { text, .. } => (l.fields.first()?.inset(8.0), self.edit_style(s.signature_placeholder, false, true), text.clone()),
            Sheet::Confirm { .. } => return None,
        })
    }

    /// Pose le champ de saisie sur le champ `i`. `select` : où mettre le curseur (None : à la fin).
    pub fn open_field(&mut self, i: usize, select: Option<(usize, usize)>) {
        // un compte Microsoft n'a pas de mot de passe à taper : on passe au champ d'à côté
        if let Some(Sheet::Account(f)) = self.sheet.as_ref()
            && i == 2
            && App::microsoft_form(f)
        {
            let next = if f.active == 3 { 1 } else { 3 };
            return self.open_field(next, None);
        }
        let Some((area, mut style, text)) = self.field_box(i) else { return self.win.repaint() };
        style.select = select;
        self.suggest.clear();
        match self.sheet.as_mut() {
            Some(Sheet::Account(f)) => f.active = i,
            Some(Sheet::Compose(f)) => f.active = i,
            _ => {}
        }
        self.edit = Some(Editing::Field(i));
        self.win.edit_show(area, &text, &style);
        self.win.repaint();
    }

    /// La fenêtre a changé de taille : le champ ouvert suit sa case.
    pub fn place_edit(&mut self) {
        match self.edit {
            Some(Editing::Field(i)) => {
                if let Some((area, ..)) = self.field_box(i) {
                    self.win.edit_place(area);
                }
            }
            Some(Editing::Search) => self.win.edit_place(self.lay.search_text),
            None => {}
        }
    }

    pub fn field_changed(&mut self, i: usize, text: String) {
        match self.sheet.as_mut() {
            Some(Sheet::Account(f)) => {
                f.error = None;
                match i {
                    // les serveurs se devinent à partir de l'adresse, tant qu'on ne les a pas écrits soi-même
                    1 if !f.own_servers => {
                        let guess = Account::guess("", &text);
                        let known = text.contains('@') && text.rsplit_once('@').is_some_and(|(_, d)| d.contains('.'));
                        f.fields[3] = if known { format!("{}:{}", guess.imap_host, guess.imap_port) } else { String::new() };
                        f.fields[4] = if known { format!("{}:{}", guess.smtp_host, guess.smtp_port) } else { String::new() };
                    }
                    3 | 4 => f.own_servers = true,
                    _ => {}
                }
                if let Some(field) = f.fields.get_mut(i) {
                    *field = text;
                }
            }
            Some(Sheet::Compose(f)) => {
                f.error = None;
                f.discard = false;
                if let Some(field) = f.fields.get_mut(i) {
                    *field = text;
                }
                self.update_suggestions(i);
            }
            Some(Sheet::Signature { text: sig, .. }) => *sig = text,
            _ => {}
        }
    }

    /// Une touche que le champ de saisie laisse à la boîte : Tab, Entrée, Échap.
    pub fn field_key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) {
        // des adresses proposées : les flèches les parcourent, Entrée ou Tab prend celle choisie, Échap les cache
        if !self.suggest.is_empty() && !mods.ctrl {
            let n = self.suggest.len();
            match vk {
                super::K_DOWN => self.suggest_at = (self.suggest_at + 1) % n,
                super::K_UP => self.suggest_at = (self.suggest_at + n - 1) % n,
                K_RETURN | K_TAB => self.accept_suggestion(self.suggest_at),
                K_ESCAPE => self.suggest.clear(),
                _ => {}
            }
            return self.win.repaint();
        }
        let (count, active) = match self.sheet.as_ref() {
            Some(Sheet::Account(f)) => (5, f.active),
            Some(Sheet::Compose(f)) => (4, f.active),
            Some(Sheet::Signature { .. }) => (1, 0),
            _ => return,
        };
        match vk {
            K_ESCAPE => self.close_sheet(),
            K_TAB => self.open_field(if mods.shift { (active + count - 1) % count } else { (active + 1) % count }, None),
            // Ctrl+Entrée valide de partout ; Entrée passe au champ suivant, et valide sur le dernier
            K_RETURN if mods.ctrl || active + 1 == count => self.sheet_ok(cx),
            K_RETURN => self.open_field(active + 1, None),
            _ => {}
        }
    }

    pub fn sheet_click(&mut self, id: Id, cx: &mut Cx) {
        match id {
            Id::SheetCancel => return self.close_sheet(),
            Id::SheetOk => return self.sheet_ok(cx),
            Id::Field(i) => return self.open_field(i, None),
            Id::SignIn => return self.sign_in(cx),
            Id::SheetReveal => {
                if let Some(Sheet::Account(f)) = self.sheet.as_mut() {
                    f.reveal = !f.reveal;
                    if f.active == 2 {
                        self.win.edit_mask(!f.reveal);
                    }
                }
            }
            Id::Attach => {
                let (hwnd, title) = (self.win.hwnd, self.s.attach_title);
                cx.after(move |win| {
                    // lus tout de suite : le message part avec ce que les fichiers contiennent maintenant
                    let files: Vec<(String, Vec<u8>)> = plume_ui::dialogs::pick_files(hwnd, title)
                        .into_iter()
                        .filter_map(|p| Some((p.file_name()?.to_string_lossy().into_owned(), std::fs::read(&p).ok()?)))
                        .collect();
                    win.with(|h, _| {
                        if let Some(app) = h.as_any().downcast_mut::<App>()
                            && let Some(Sheet::Compose(f)) = app.sheet.as_mut()
                        {
                            f.files.extend(files);
                        }
                    });
                    win.repaint();
                });
            }
            Id::SaveDraft => return self.save_draft(false),
            Id::Suggest(k) => return self.accept_suggestion(k),
            Id::Chip(i) => {
                if let Some(Sheet::Compose(f)) = self.sheet.as_mut()
                    && i < f.files.len()
                {
                    f.files.remove(i);
                }
            }
            _ => {}
        }
        self.win.edit_focus();
        self.win.repaint();
    }

    // ───────────────────────── valider ─────────────────────────

    pub fn sheet_error(&mut self, text: &str) {
        match self.sheet.as_mut() {
            Some(Sheet::Account(f)) => f.error = Some(text.to_string()),
            Some(Sheet::Compose(f)) => f.error = Some(text.to_string()),
            _ => {}
        }
        self.win.edit_focus();
        self.win.repaint();
    }

    pub fn sheet_ok(&mut self, cx: &mut Cx) {
        let s = self.s;
        match self.sheet.as_ref() {
            Some(Sheet::Account(f)) => {
                let [name, email, password, imap, smtp] = f.fields.clone();
                let editing = f.editing;
                let microsoft = App::microsoft_form(f);
                let token = match &f.oauth {
                    OAuth::Ready(token) => Some(token.clone()),
                    _ => None,
                };
                let email = email.trim().to_string();
                if message::addresses(&email).len() != 1 {
                    return self.sheet_error(s.account_bad_email);
                }
                let (Some((imap_host, imap_port)), Some((smtp_host, smtp_port))) = (server(&imap), server(&smtp)) else {
                    return self.sheet_error(s.account_bad_server);
                };
                let mut account = Account::guess(&name, &email);
                (account.imap_host, account.imap_port, account.imap_security) = (imap_host, imap_port, security(imap_port, 993));
                (account.smtp_host, account.smtp_port, account.smtp_security) = (smtp_host, smtp_port, security(smtp_port, 465));
                if microsoft {
                    if oauth::microsoft().is_none() {
                        return self.sheet_error(s.account_web_sign_in);
                    }
                    // un compte Microsoft déjà là garde sa connexion ; un nouveau doit d'abord se connecter
                    let known = editing.and_then(|b| self.boxes.get(b)).is_some_and(|mb| mb.account.sign_in == SignIn::Microsoft);
                    if token.is_none() && !known {
                        return self.sheet_error(s.ms_first);
                    }
                    account.sign_in = SignIn::Microsoft;
                } else if password.is_empty() && editing.is_none() {
                    // un compte déjà là garde son mot de passe si le champ reste vide
                    return self.sheet_error(s.account_no_password);
                }
                account.id = match editing.and_then(|b| self.boxes.get(b)) {
                    Some(mb) => mb.account.id.clone(),
                    None => self.store.new_id(&self.boxes.iter().map(|mb| mb.account.clone()).collect::<Vec<_>>()),
                };
                // le jeton de Microsoft prend la place du mot de passe, au même endroit (chiffré par Windows)
                match (&token, microsoft) {
                    (Some(token), true) => {
                        self.store.set_password(&account.id, &account.user, token);
                    }
                    (_, false) if !password.is_empty() => {
                        self.store.set_password(&account.id, &account.user, &password);
                    }
                    _ => {}
                }
                if let Some(mb) = editing.and_then(|b| self.boxes.get(b)) {
                    account.signature = mb.account.signature.clone();
                }
                self.close_sheet();
                let b = match editing.filter(|b| *b < self.boxes.len()) {
                    // le compte repart avec ses nouveaux réglages : un nouveau fil, une nouvelle connexion
                    Some(b) => {
                        self.boxes.remove(b);
                        let fresh = self.start_box(account);
                        let mb = self.boxes.remove(fresh);
                        self.boxes.insert(b, mb);
                        b
                    }
                    None => self.start_box(account),
                };
                let _ = self.store.save_accounts(&self.boxes.iter().map(|mb| mb.account.clone()).collect::<Vec<_>>());
                let inbox = self.inbox_of(b);
                self.open_folder(b, inbox);
            }
            Some(Sheet::Signature { account, text }) => {
                let (b, text) = (*account, text.trim_end().to_string());
                if let Some(mb) = self.boxes.get_mut(b) {
                    mb.account.signature = text;
                }
                let _ = self.store.save_accounts(&self.boxes.iter().map(|mb| mb.account.clone()).collect::<Vec<_>>());
                self.sheet = None;
                self.end_edit();
                self.toast(s.signature_saved.to_string());
            }
            Some(Sheet::Compose(f)) => {
                if f.sending || f.saving {
                    return;
                }
                let [to, cc, subject, text] = f.fields.clone();
                if message::addresses(&to).is_empty() || (!cc.trim().is_empty() && message::addresses(&cc).is_empty()) {
                    return self.sheet_error(s.recipients_bad);
                }
                let Some(mb) = self.boxes.get(f.account) else { return };
                let draft = Draft {
                    from_name: mb.account.name.clone(),
                    from: mb.account.email.clone(),
                    to,
                    cc,
                    bcc: String::new(),
                    subject,
                    text,
                    in_reply_to: f.in_reply_to.clone(),
                    references: f.references.clone(),
                    files: f.files.clone(),
                };
                mb.engine.send(Command::Send { draft: Box::new(draft), drop_draft: f.draft.clone() });
                if let Some(Sheet::Compose(f)) = self.sheet.as_mut() {
                    f.sending = true;
                    f.error = None;
                }
                self.suggest.clear();
                // le texte reste affiché sous la boîte, mais on n'y tape plus tant que le serveur n'a pas répondu
                self.end_edit();
            }
            Some(Sheet::Confirm { .. }) => {
                let Some(Sheet::Confirm { what, .. }) = self.sheet.take() else { return };
                match what {
                    Confirm::RemoveAccount(b) if b < self.boxes.len() => {
                        let gone = self.boxes.remove(b);
                        self.store.remove_account(&gone.account.id);
                        let _ = self.store.save_accounts(&self.boxes.iter().map(|mb| mb.account.clone()).collect::<Vec<_>>());
                        (self.cur, self.sel, self.reading) = (None, None, None);
                        self.messages.clear();
                        self.shown.clear();
                        if !self.boxes.is_empty() {
                            let inbox = self.inbox_of(0);
                            self.open_folder(0, inbox);
                        }
                        self.update_title();
                    }
                    Confirm::DeleteMessage { account, folder, uid } => {
                        if let Some(mb) = self.boxes.get(account) {
                            mb.engine.send(Command::Remove { folder, uids: vec![uid] });
                        }
                        (self.sel, self.reading) = (None, None);
                    }
                    Confirm::RemoveAccount(_) => {}
                }
            }
            None => {}
        }
        cx.repaint();
    }

    /// Le serveur a répondu à l'envoi : la boîte se ferme, ou dit ce qui n'a pas été.
    pub fn on_sent(&mut self, result: Result<(), String>) {
        let Some(Sheet::Compose(f)) = self.sheet.as_mut().filter(|sheet| matches!(sheet, Sheet::Compose(f) if f.sending)) else { return };
        match result {
            Ok(()) => {
                self.sheet = None;
                self.toast(self.s.sent_ok.to_string());
            }
            Err(why) => {
                f.sending = false;
                f.error = Some(why);
                let active = f.active;
                self.open_field(active, None);
            }
        }
    }

    // ───────────────────────── dessiner ─────────────────────────

    pub fn paint_sheet(&mut self, c: &mut Canvas, t: &Theme) {
        let Some(l) = self.sheet_lay() else { return };
        let Some(sheet) = self.sheet.as_ref() else { return };
        let s = self.s;
        // le reste de la fenêtre s'assombrit et ne réagit plus (la barre de titre reste libre : déplacer, fermer)
        let dim = rect(0.0, m::TITLE_H, self.lay.w, (self.lay.h - m::TITLE_H).max(0.0));
        c.fill(dim, Color::hexa(0x000000, if t.dark { 0.5 } else { 0.28 }));
        self.hits.add(dim, Id::SheetDim);
        c.fill_round(l.card, 20.0, t.solid);
        c.stroke_round(l.card, 20.0, t.ring, 1.0);
        self.hits.add(l.card, Id::SheetCard);
        let (pad, inner) = (24.0, l.card.w - 48.0);
        let active = match self.edit {
            Some(Editing::Field(i)) => Some(i),
            _ => None,
        };
        // un champ : son cadre, et son texte quand le champ de saisie n'est pas posé dessus
        let field = |c: &mut Canvas, hits: &mut plume_ui::Hits<Id>, i: usize, r: Rect, text: &str, masked: bool| {
            c.fill_round(r, m::FIELD_R, t.fill);
            if active == Some(i) {
                c.stroke_round(r, m::FIELD_R, t.accent, 1.5);
            } else {
                let shown = if masked { "•".repeat(text.chars().count().min(40)) } else { text.to_string() };
                c.text(shown.lines().next().unwrap_or_default(), text_of(r), Font::Body, t.t1, Align::Left);
            }
            hits.add(r, Id::Field(i));
        };
        let (action, error, danger, busy) = match sheet {
            Sheet::Account(f) => {
                c.text(
                    if f.editing.is_some() { s.account_edit_title } else { s.account_title },
                    rect(l.card.x + pad, l.card.y + 20.0, inner, 24.0),
                    Font::Large,
                    t.t1,
                    Align::Left,
                );
                let microsoft = App::microsoft_form(f);
                c.text(if microsoft { s.ms_hint } else { s.account_hint }, rect(l.card.x + pad, l.card.y + 46.0, inner, 18.0), Font::Small, t.t3, Align::Left);
                for (i, label) in [s.f_name, s.f_email, if microsoft { s.f_microsoft } else { s.f_password }, s.f_imap, s.f_smtp].into_iter().enumerate() {
                    c.text(label, rect(l.fields[i].x + 2.0, l.fields[i].y - 19.0, inner, 16.0), Font::Small, t.t3, Align::Left);
                    if !(microsoft && i == 2) {
                        field(c, &mut self.hits, i, l.fields[i], &f.fields[i], i == 2 && !f.reveal);
                    }
                }
                if microsoft {
                    // à la place du mot de passe : le bouton qui ouvre la page de Microsoft, et où l'on en est
                    let r = l.fields[2];
                    let hot = self.hover == Some(Id::SignIn);
                    let known = f.editing.and_then(|b| self.boxes.get(b)).is_some_and(|mb| mb.account.sign_in == SignIn::Microsoft);
                    let (label, done) = match &f.oauth {
                        OAuth::Waiting { .. } => (s.ms_waiting, false),
                        OAuth::Ready(_) => (s.ms_signed_in, true),
                        OAuth::Idle if known => (s.ms_signed_in, true),
                        OAuth::Idle => (s.ms_sign_in, false),
                    };
                    c.fill_round(r, m::FIELD_R, if hot { t.fill2 } else { t.fill });
                    if done {
                        c.fill_round(rect(r.x + 12.0, r.y + r.h / 2.0 - 4.0, 8.0, 8.0), 4.0, t.green);
                    }
                    c.text(
                        label,
                        rect(r.x + if done { 28.0 } else { 12.0 }, r.y, r.w - 40.0, r.h),
                        Font::Body,
                        if done { t.t1 } else { t.accent },
                        Align::Left,
                    );
                    self.hits.add(r, Id::SignIn);
                } else {
                    let hot = self.hover == Some(Id::SheetReveal);
                    if hot {
                        c.fill_round(l.extra, l.extra.h / 2.0, t.fill2);
                    }
                    c.text(if f.reveal { s.hide } else { s.show }, l.extra, Font::Small, if hot { t.t1 } else { t.t3 }, Align::Center);
                    self.hits.add(l.extra, Id::SheetReveal);
                }
                (s.save, f.error.clone(), false, false)
            }
            Sheet::Signature { account, text } => {
                c.text(s.signature_title, rect(l.card.x + pad, l.card.y + 20.0, inner, 24.0), Font::Large, t.t1, Align::Left);
                let email = self.boxes.get(*account).map(|mb| mb.account.email.clone()).unwrap_or_default();
                c.text(&s.fill(s.signature_hint, &[&email]), rect(l.card.x + pad, l.card.y + 46.0, inner, 18.0), Font::Small, t.t3, Align::Left);
                let body = l.fields[0];
                c.fill_round(body, m::FIELD_R, t.fill);
                if active == Some(0) {
                    c.stroke_round(body, m::FIELD_R, t.accent, 1.5);
                } else {
                    c.clip(body.inset(8.0));
                    c.text_block(text, body.inset(8.0), Font::Body, t.t1);
                    c.unclip();
                }
                self.hits.add(body, Id::Field(0));
                (s.save, None, false, false)
            }
            Sheet::Compose(f) => {
                let title = if f.sending {
                    s.sending
                } else if f.saving {
                    s.save_draft
                } else {
                    s.write_title
                };
                c.text(title, rect(l.card.x + pad, l.card.y + 20.0, inner, 24.0), Font::Large, t.t1, Align::Left);
                if let Some(mb) = self.boxes.get(f.account) {
                    c.text(&mb.account.email, rect(l.card.x + pad, l.card.y + 20.0, inner, 24.0), Font::Small, t.t3, Align::Right);
                }
                for (i, label) in [s.to, s.cc, s.subject].into_iter().enumerate() {
                    c.text(label, rect(l.card.x + pad, l.fields[i].y, 70.0, l.fields[i].h), Font::Small, t.t3, Align::Left);
                    field(c, &mut self.hits, i, l.fields[i], &f.fields[i], false);
                }
                // le texte : sur plusieurs lignes quand le champ de saisie n'y est pas
                let body = l.fields[3];
                c.fill_round(body, m::FIELD_R, t.fill);
                if active == Some(3) {
                    c.stroke_round(body, m::FIELD_R, t.accent, 1.5);
                } else {
                    c.clip(body.inset(8.0));
                    c.text_block(&f.fields[3], body.inset(8.0), Font::Body, if f.sending { t.t3 } else { t.t1 });
                    c.unclip();
                }
                self.hits.add(body, Id::Field(3));
                // joindre des fichiers, et ceux qui le sont déjà (un clic les retire)
                let hot = self.hover == Some(Id::Attach);
                c.fill_round(l.extra, l.extra.h / 2.0, if hot { t.fill2 } else { t.fill });
                c.text(s.attach, l.extra, Font::BodyBold, t.t1, Align::Center);
                self.hits.add(l.extra, Id::Attach);
                // garder en brouillon, sans fermer
                let label = s.save_draft;
                let dw = c.text_width(label, Font::BodyBold) + 32.0;
                let draft = rect(l.cancel.x - 10.0 - dw, l.extra.y, dw, l.extra.h);
                let hot = self.hover == Some(Id::SaveDraft);
                c.fill_round(draft, draft.h / 2.0, if hot { t.fill2 } else { t.fill });
                c.text(label, draft, Font::BodyBold, if f.saving || f.sending { t.t3 } else { t.t1 }, Align::Center);
                self.hits.add(draft, Id::SaveDraft);
                let mut x = l.extra.right() + 10.0;
                for (i, (name, data)) in f.files.iter().enumerate() {
                    let label = format!("{name} ({})  ×", size_text(data.len()));
                    let w = (c.text_width(&label, Font::Small) + 20.0).min(220.0);
                    if x + w > draft.x - 10.0 {
                        break;
                    }
                    let r = rect(x, l.extra.y + 5.0, w, 26.0);
                    c.fill_round(r, 13.0, if self.hover == Some(Id::Chip(i)) { t.fill2 } else { t.fill });
                    c.text(&label, r.inset_xy(10.0, 0.0), Font::Small, t.t2, Align::Left);
                    self.hits.add(r, Id::Chip(i));
                    x += w + 6.0;
                }
                (s.send, f.error.clone(), false, f.sending || f.saving)
            }
            Sheet::Confirm { title, text, action, .. } => {
                c.text(title, rect(l.card.x + pad, l.card.y + 20.0, inner, 24.0), Font::Large, t.t1, Align::Left);
                c.text_block(text, rect(l.card.x + pad, l.card.y + 52.0, inner, 44.0), Font::Body, t.t2);
                (action.as_str(), None, true, false)
            }
        };
        if let Some(error) = error {
            c.text(&error, rect(l.card.x + pad, l.ok.y - 26.0, inner, 18.0), Font::Small, t.red, Align::Left);
        }
        let main = if danger { t.red } else { t.accent };
        for (id, r, label, primary) in [(Id::SheetCancel, l.cancel, s.cancel, false), (Id::SheetOk, l.ok, action, true)] {
            let hot = self.hover == Some(id);
            let down = self.pressed == Some(id) && hot;
            let bg = match (primary, hot, down) {
                (true, _, _) if busy => main.fade(0.45),
                (true, _, true) => main.mix(Color::hex(0x000000), 0.18),
                (true, true, _) => main.mix(Color::hex(0xffffff), 0.1),
                (true, false, _) => main,
                (false, true, _) => t.fill2,
                (false, false, _) => t.fill,
            };
            c.fill_round(r, r.h / 2.0, bg);
            c.text(label, r, Font::BodyBold, if primary { Color::hex(0xffffff) } else { t.t1 }, Align::Center);
            self.hits.add(r, id);
        }
        // les adresses proposées, sous le champ que l'on remplit
        if let Some(Sheet::Compose(f)) = self.sheet.as_ref()
            && !self.suggest.is_empty()
            && active.is_some_and(|i| i <= 1)
        {
            let field = l.fields[f.active.min(1)];
            let row_h = 34.0;
            let list = rect(field.x, field.bottom() + 4.0, field.w, self.suggest.len() as f32 * row_h + 8.0);
            c.fill_round(list, 10.0, t.solid2);
            c.stroke_round(list, 10.0, t.ring, 1.0);
            for (k, contact) in self.suggest.iter().enumerate() {
                let r = rect(list.x + 4.0, list.y + 4.0 + k as f32 * row_h, list.w - 8.0, row_h);
                if k == self.suggest_at || self.hover == Some(Id::Suggest(k)) {
                    c.fill_round(r, 7.0, t.fill2);
                }
                let name = if contact.name.is_empty() { contact.email.as_str() } else { contact.name.as_str() };
                let nw = c.text_width(name, Font::Body).min(r.w * 0.55);
                c.text(name, rect(r.x + 10.0, r.y, nw + 2.0, r.h), Font::Body, t.t1, Align::Left);
                if !contact.name.is_empty() {
                    c.text(&contact.email, rect(r.x + 22.0 + nw, r.y, (r.w - nw - 32.0).max(10.0), r.h), Font::Small, t.t3, Align::Left);
                }
                self.hits.add(r, Id::Suggest(k));
            }
        }
    }
}

/// Une taille lisible : « 12 Ko » en français se dit comme en anglais ici (« KB »), les unités sont celles de Windows.
pub fn size_text(bytes: usize) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.0} KB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servers_and_sizes() {
        assert_eq!(server(" imap.Example.com:993 "), Some(("imap.example.com".into(), 993)));
        assert_eq!(server("127.0.0.1:1143"), Some(("127.0.0.1".into(), 1143)));
        for bad in ["", "imap.example.com", "imap.example.com:", ":993", "imap.example.com:0", "imap example.com:993", "a/b:993", "x:70000"] {
            assert_eq!(server(bad), None, "{bad}");
        }
        assert_eq!((size_text(900), size_text(2048), size_text(3 * 1024 * 1024 / 2)), ("900 B".to_string(), "2 KB".to_string(), "1.5 MB".to_string()));
    }
}
