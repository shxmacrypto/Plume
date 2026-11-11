//! Les boîtes de dialogue dessinées dans la fenêtre : ajouter ou régler un compte, écrire un message, confirmer.
//! Leurs champs partagent le champ de saisie de Windows que prête le kit : il se pose sur celui que l'on édite.

use super::{App, Editing, Id, K_ESCAPE, K_RETURN, K_TAB, TOP, text_of};
use mail_core::{
    engine::Command,
    message::{self, Draft},
    net::Security,
    store::Account,
};
use plume_ui::{Align, Canvas, Color, Cx, EditStyle, Font, Mods, Rect, Theme, m, rect};

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
}

pub struct ComposeForm {
    pub account: usize,
    /// À, Cc, objet, texte.
    pub fields: [String; 4],
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
    Confirm { title: String, text: String, action: String, what: Confirm },
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
        self.sheet = Some(Sheet::Account(AccountForm { fields, active: 0, error: None, editing, reveal: false, own_servers: editing.is_some() }));
        self.open_field(0, None);
    }

    /// La boîte « Nouveau message » : vide, ou préparée pour répondre au message ouvert ou le transférer.
    pub fn compose(&mut self, start: Start) {
        let s = self.s;
        let Some(account) = self.cur.as_ref().map(|c| c.0).or((!self.boxes.is_empty()).then_some(0)) else { return self.ask_account(None) };
        let mut form = ComposeForm {
            account,
            fields: Default::default(),
            active: 0,
            files: vec![],
            error: None,
            sending: false,
            in_reply_to: String::new(),
            references: vec![],
        };
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
            form.fields[3] = quoted;
        }
        let first = form.active;
        self.end_edit();
        self.menu = None;
        self.toast = None;
        self.sheet = Some(Sheet::Compose(form));
        // pour répondre, on écrit au-dessus du message cité
        self.open_field(first, (first == 3).then_some((0, 0)));
    }

    pub fn close_sheet(&mut self) {
        self.sheet = None;
        self.end_edit();
        self.win.repaint();
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
                let by = card.bottom() - 22.0 - 34.0;
                let ok = rect(card.right() - pad - 120.0, by, 120.0, 34.0);
                let extra = rect(fields[2].right() - 82.0, fields[2].y + 4.0, 78.0, 26.0);
                SheetLay { card, fields, ok, cancel: rect(ok.x - 10.0 - 100.0, by, 100.0, 34.0), extra }
            }
            Sheet::Compose(_) => {
                let (cw, ch) = ((w - 64.0).clamp(320.0, 780.0), (h - m::TITLE_H - 28.0).clamp(300.0, 640.0));
                let card = rect(((w - cw) / 2.0).round(), (m::TITLE_H + ((h - m::TITLE_H - ch) / 2.0)).round().max(m::TITLE_H + 4.0), cw, ch);
                let label = 74.0;
                let mut fields: Vec<Rect> = (0..3).map(|i| rect(card.x + pad + label, card.y + 58.0 + i as f32 * 40.0, cw - 2.0 * pad - label, 32.0)).collect();
                let by = card.bottom() - 20.0 - 34.0;
                let top = card.y + 58.0 + 3.0 * 40.0 + 6.0;
                fields.push(rect(card.x + pad, top, cw - 2.0 * pad, (by - 30.0 - top).max(60.0)));
                let ok = rect(card.right() - pad - 120.0, by, 120.0, 34.0);
                SheetLay { card, fields, ok, cancel: rect(ok.x - 10.0 - 100.0, by, 100.0, 34.0), extra: rect(card.x + pad, by, 168.0, 34.0) }
            }
            Sheet::Confirm { .. } => {
                let (cw, ch) = (430.0f32.min(w - 32.0).max(260.0), 168.0);
                let card = rect(((w - cw) / 2.0).round(), ((h - ch) * 0.38).round().max(TOP), cw, ch);
                let by = card.bottom() - 22.0 - 34.0;
                let ok = rect(card.right() - pad - 120.0, by, 120.0, 34.0);
                SheetLay { card, fields: vec![], ok, cancel: rect(ok.x - 10.0 - 100.0, by, 100.0, 34.0), extra: Rect::default() }
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
            Sheet::Confirm { .. } => return None,
        })
    }

    /// Pose le champ de saisie sur le champ `i`. `select` : où mettre le curseur (None : à la fin).
    pub fn open_field(&mut self, i: usize, select: Option<(usize, usize)>) {
        let Some((area, mut style, text)) = self.field_box(i) else { return self.win.repaint() };
        style.select = select;
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
                if let Some(field) = f.fields.get_mut(i) {
                    *field = text;
                }
            }
            _ => {}
        }
    }

    /// Une touche que le champ de saisie laisse à la boîte : Tab, Entrée, Échap.
    pub fn field_key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) {
        let (count, active) = match self.sheet.as_ref() {
            Some(Sheet::Account(f)) => (5, f.active),
            Some(Sheet::Compose(f)) => (4, f.active),
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

    fn sheet_error(&mut self, text: &str) {
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
                if account.needs_web_sign_in() {
                    return self.sheet_error(s.account_web_sign_in);
                }
                // un compte déjà là garde son mot de passe si le champ reste vide
                if password.is_empty() && editing.is_none() {
                    return self.sheet_error(s.account_no_password);
                }
                account.id = match editing.and_then(|b| self.boxes.get(b)) {
                    Some(mb) => mb.account.id.clone(),
                    None => self.store.new_id(&self.boxes.iter().map(|mb| mb.account.clone()).collect::<Vec<_>>()),
                };
                if !password.is_empty() {
                    self.store.set_password(&account.id, &account.user, &password);
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
            Some(Sheet::Compose(f)) => {
                if f.sending {
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
                mb.engine.send(Command::Send { draft: Box::new(draft) });
                if let Some(Sheet::Compose(f)) = self.sheet.as_mut() {
                    f.sending = true;
                    f.error = None;
                }
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
        c.fill_round(l.card, 14.0, t.solid);
        c.stroke_round(l.card, 14.0, t.ring, 1.0);
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
                    rect(l.card.x + pad, l.card.y + 16.0, inner, 24.0),
                    Font::Title,
                    t.t1,
                    Align::Left,
                );
                c.text(s.account_hint, rect(l.card.x + pad, l.card.y + 42.0, inner, 18.0), Font::Small, t.t3, Align::Left);
                for (i, label) in [s.f_name, s.f_email, s.f_password, s.f_imap, s.f_smtp].into_iter().enumerate() {
                    c.text(label, rect(l.fields[i].x + 2.0, l.fields[i].y - 19.0, inner, 16.0), Font::Small, t.t3, Align::Left);
                    field(c, &mut self.hits, i, l.fields[i], &f.fields[i], i == 2 && !f.reveal);
                }
                let hot = self.hover == Some(Id::SheetReveal);
                if hot {
                    c.fill_round(l.extra, 8.0, t.fill2);
                }
                c.text(if f.reveal { s.hide } else { s.show }, l.extra, Font::Small, if hot { t.t1 } else { t.t3 }, Align::Center);
                self.hits.add(l.extra, Id::SheetReveal);
                (s.save, f.error.clone(), false, false)
            }
            Sheet::Compose(f) => {
                let title = if f.sending { s.sending } else { s.write_title };
                c.text(title, rect(l.card.x + pad, l.card.y + 16.0, inner, 24.0), Font::Title, t.t1, Align::Left);
                if let Some(mb) = self.boxes.get(f.account) {
                    c.text(&mb.account.email, rect(l.card.x + pad, l.card.y + 16.0, inner, 24.0), Font::Small, t.t3, Align::Right);
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
                c.fill_round(l.extra, 9.0, if hot { t.fill2 } else { t.fill });
                c.text(s.attach, l.extra, Font::Body, t.t1, Align::Center);
                self.hits.add(l.extra, Id::Attach);
                let mut x = l.extra.right() + 10.0;
                for (i, (name, data)) in f.files.iter().enumerate() {
                    let label = format!("{name} ({})  ×", size_text(data.len()));
                    let w = (c.text_width(&label, Font::Small) + 20.0).min(220.0);
                    if x + w > l.cancel.x - 10.0 {
                        break;
                    }
                    let r = rect(x, l.extra.y + 4.0, w, 26.0);
                    c.fill_round(r, 13.0, if self.hover == Some(Id::Chip(i)) { t.fill2 } else { t.fill });
                    c.text(&label, r.inset_xy(10.0, 0.0), Font::Small, t.t2, Align::Left);
                    self.hits.add(r, Id::Chip(i));
                    x += w + 6.0;
                }
                (s.send, f.error.clone(), false, f.sending)
            }
            Sheet::Confirm { title, text, action, .. } => {
                c.text(title, rect(l.card.x + pad, l.card.y + 16.0, inner, 24.0), Font::Title, t.t1, Align::Left);
                c.text_block(text, rect(l.card.x + pad, l.card.y + 48.0, inner, 44.0), Font::Small, t.t2);
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
            c.fill_round(r, 9.0, bg);
            c.text(label, r, Font::BodyBold, if primary { Color::hex(0xffffff) } else { t.t1 }, Align::Center);
            self.hits.add(r, id);
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
