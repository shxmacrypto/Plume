//! La petite boîte de dialogue dessinée dans la fenêtre : le mot de passe d'une archive, à choisir (compresser)
//! ou à donner (extraire) ; l'adresse d'un serveur, son mot de passe, la confirmation d'une suppression.
//! Tant qu'elle est ouverte, le reste de la fenêtre ne réagit pas.

use super::{App, EditKind, Id, i18n::Strings, model::Location};
use crate::{
    shell::archive::{self, Format},
    ui::{Align, Canvas, Color, EditStyle, Font, Rect, Theme, m, rect},
};
use std::{path::PathBuf, sync::mpsc};

pub enum SheetKind {
    /// Créer une archive protégée à partir de ces éléments. `rar` : WinRAR est installé, le format RAR est proposé.
    Lock { paths: Vec<PathBuf>, format: Format, rar: bool },
    /// Donner le mot de passe d'une archive à extraire (dans un dossier à son nom, ou sur place).
    Unlock { archive: PathBuf, to_folder: bool },
    /// Donner l'adresse d'un serveur SFTP auquel se connecter.
    Connect,
    /// Une question de `ssh` pendant la connexion à `server` (mot de passe, phrase d'une clé). La réponse part
    /// par `reply` ; fermer la boîte sans répondre, c'est renoncer à se connecter.
    Secret { server: String, prompt: String, secret: bool, reply: mpsc::Sender<Option<String>> },
    /// Confirmer la suppression, pour de bon, de ces éléments (nom, dossier ?) du dossier `dir` d'un serveur.
    Delete { server: String, dir: String, names: Vec<(String, bool)> },
}

impl SheetKind {
    /// La boîte a-t-elle un champ où taper ?
    fn has_field(&self) -> bool {
        !matches!(self, SheetKind::Delete { .. })
    }

    /// Ce qu'on y tape est-il un secret (masqué, avec le bouton « Afficher ») ?
    fn is_secret(&self) -> bool {
        matches!(self, SheetKind::Lock { .. } | SheetKind::Unlock { .. } | SheetKind::Secret { secret: true, .. })
    }
}

pub struct Sheet {
    pub kind: SheetKind,
    /// Message sous le champ (« Mot de passe incorrect »).
    pub error: Option<String>,
    /// Le texte tapé s'affiche en clair.
    pub reveal: bool,
}

/// Les rectangles de la boîte, les mêmes pour le dessin et pour le champ de saisie.
#[derive(Clone, Copy)]
pub struct SheetLay {
    pub card: Rect,
    pub zip: Rect,
    pub seven: Rect,
    pub rar: Rect,
    pub field: Rect,
    pub text: Rect,
    pub reveal: Rect,
    pub ok: Rect,
    pub cancel: Rect,
}

impl App {
    pub(super) fn sheet_lay(&self) -> Option<SheetLay> {
        let sheet = self.sheet.as_ref()?;
        let (w, h) = (self.lay.w, self.lay.status.bottom());
        let lock = matches!(sheet.kind, SheetKind::Lock { .. });
        let (typed, secret) = (sheet.kind.has_field(), sheet.kind.is_secret());
        let cw = 420.0f32.min(w - 32.0).max(260.0);
        let ch = match (lock, typed) {
            (true, _) => 258.0,
            (false, true) => 198.0,
            (false, false) => 164.0,
        };
        let card = rect(((w - cw) / 2.0).round(), (((h - ch) * 0.36).round()).max(m::TITLE_H + 8.0), cw, ch);
        let pad = 22.0;
        let mut y = card.y + 66.0;
        let (mut zip, mut seven, mut rar) = (Rect::default(), Rect::default(), Rect::default());
        if lock {
            zip = rect(card.x + pad, y, 62.0, 28.0);
            seven = rect(zip.right() + 8.0, y, 62.0, 28.0);
            rar = rect(seven.right() + 8.0, y, 62.0, 28.0);
            y += 60.0;
        }
        let field = if typed { rect(card.x + pad, y, cw - 2.0 * pad, 36.0) } else { Rect::default() };
        let reveal = if secret { rect(field.right() - 82.0, field.y + 4.0, 78.0, 28.0) } else { Rect::default() };
        let text = rect(field.x + 12.0, field.y, (field.w - 12.0 - if secret { 88.0 } else { 12.0 }).max(20.0), field.h);
        let by = card.bottom() - 22.0 - 34.0;
        let ok = rect(card.right() - pad - 112.0, by, 112.0, 34.0);
        let cancel = rect(ok.x - 10.0 - 100.0, by, 100.0, 34.0);
        Some(SheetLay { card, zip, seven, rar, field, text, reveal, ok, cancel })
    }

    /// Demande le mot de passe d'une nouvelle archive pour ces éléments.
    pub(super) fn ask_new_password(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        self.end_edit(true);
        self.sheet = Some(Sheet { kind: SheetKind::Lock { paths, format: Format::Zip, rar: archive::winrar().is_some() }, error: None, reveal: false });
        self.open_sheet_field();
    }

    /// Demande le mot de passe d'une archive protégée. `wrong` : celui qui vient d'être essayé n'était pas le bon.
    pub(super) fn ask_password(&mut self, archive: PathBuf, to_folder: bool, wrong: bool) {
        self.end_edit(true);
        let error = wrong.then(|| self.s.pw_wrong.to_string());
        self.sheet = Some(Sheet { kind: SheetKind::Unlock { archive, to_folder }, error, reveal: false });
        self.open_sheet_field();
    }

    /// Pose le champ de saisie de Windows sur celui de la boîte (si elle en a un).
    pub(super) fn open_sheet_field(&mut self) {
        let Some(l) = self.sheet_lay() else { return };
        let Some(sheet) = self.sheet.as_ref() else { return };
        if !sheet.kind.has_field() {
            return self.win.repaint();
        }
        let placeholder = match &sheet.kind {
            SheetKind::Connect => self.s.sftp_placeholder,
            SheetKind::Secret { .. } => "",
            _ => self.s.pw_placeholder,
        };
        let password = sheet.kind.is_secret() && !sheet.reveal;
        self.edit = Some(EditKind::Password);
        self.edit_rect = l.text;
        let style = EditStyle {
            fg: self.theme.t1,
            bg: self.theme.fill.over(self.theme.solid),
            size: 13.0,
            select: None,
            placeholder: placeholder.to_string(),
            password,
            multiline: false,
        };
        self.win.edit_show(l.text, "", &style);
        self.win.repaint();
    }

    /// Ferme la boîte sans rien faire. Une question de `ssh` restée sans réponse : la connexion est abandonnée.
    pub(super) fn close_sheet(&mut self) {
        self.sheet = None;
        if self.edit == Some(EditKind::Password) {
            self.end_edit(false);
        }
        self.win.repaint();
    }

    /// Clic dans la boîte.
    pub(super) fn sheet_click(&mut self, id: Id) {
        match id {
            Id::SheetCancel => return self.close_sheet(),
            Id::SheetOk => return self.sheet_ok(),
            Id::SheetReveal => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.reveal = !sheet.reveal;
                    self.win.edit_mask(!sheet.reveal);
                }
            }
            Id::SheetZip | Id::Sheet7z | Id::SheetRar => {
                if let Some(Sheet { kind: SheetKind::Lock { format, rar, .. }, .. }) = self.sheet.as_mut() {
                    *format = match id {
                        Id::Sheet7z => Format::SevenZ,
                        Id::SheetRar if *rar => Format::Rar,
                        _ => Format::Zip,
                    };
                }
            }
            _ => {}
        }
        // le clavier reste dans le champ
        self.win.edit_focus();
        self.win.repaint();
    }

    /// Valide la boîte : lance la compression, l'extraction, la connexion, la suppression.
    pub(super) fn sheet_ok(&mut self) {
        let Some(sheet) = self.sheet.as_ref() else { return };
        let text = if sheet.kind.has_field() { self.win.edit_text().unwrap_or_default() } else { String::new() };
        // un champ vide, une adresse qu'on ne comprend pas : la boîte reste ouverte et le dit
        let problem = match &sheet.kind {
            SheetKind::Delete { .. } => None,
            SheetKind::Connect => server_address(&text).is_none().then_some(self.s.sftp_bad_address),
            _ => text.is_empty().then_some(self.s.pw_empty),
        };
        if let Some(problem) = problem {
            if let Some(sheet) = self.sheet.as_mut() {
                sheet.error = Some(problem.to_string());
            }
            self.win.edit_focus();
            return self.win.repaint();
        }
        let Some(sheet) = self.sheet.take() else { return };
        self.end_edit(false);
        match sheet.kind {
            SheetKind::Lock { paths, format, .. } => self.compress_locked(paths, text, format),
            SheetKind::Unlock { archive, to_folder } => self.extract(archive, to_folder, Some(text)),
            SheetKind::Connect => {
                if let Some(loc) = server_address(&text) {
                    self.navigate(loc);
                }
            }
            SheetKind::Secret { reply, .. } => {
                let _ = reply.send(Some(text));
            }
            SheetKind::Delete { server, dir, names } => self.remote_delete(server, dir, names),
        }
        self.win.repaint();
    }

    pub(super) fn paint_sheet(&mut self, c: &mut Canvas, t: &Theme) {
        let Some(l) = self.sheet_lay() else { return };
        let Some(sheet) = self.sheet.as_ref() else { return };
        let s: &Strings = self.s;
        // le reste de la fenêtre s'assombrit et ne réagit plus (la barre de titre reste libre : déplacer, fermer)
        let dim = rect(0.0, m::TITLE_H, self.lay.w, (self.lay.status.bottom() - m::TITLE_H).max(0.0));
        c.fill(dim, Color::hexa(0x000000, if t.dark { 0.5 } else { 0.28 }));
        self.hits.add(dim, Id::SheetDim);
        c.fill_round(l.card, 14.0, t.solid);
        c.stroke_round(l.card, 14.0, t.ring, 1.0);
        self.hits.add(l.card, Id::SheetCard);

        let pad = 22.0;
        let many = |n: usize| s.count(n as u64, s.item);
        let (title, subtitle, action) = match &sheet.kind {
            SheetKind::Lock { paths, .. } => {
                let what = match paths.as_slice() {
                    [one] => super::model::leaf(one),
                    more => many(more.len()),
                };
                (s.pw_title_lock.to_string(), what, s.btn_create)
            }
            SheetKind::Unlock { archive, .. } => (s.pw_title_unlock.to_string(), super::model::leaf(archive), s.btn_extract),
            SheetKind::Connect => (s.sftp_title.to_string(), s.sftp_hint.to_string(), s.btn_connect),
            SheetKind::Secret { server, prompt, .. } => (server.clone(), prompt.clone(), s.btn_ok),
            SheetKind::Delete { names, .. } => {
                let what = match names.as_slice() {
                    [(one, _)] => one.clone(),
                    more => many(more.len()),
                };
                (s.sftp_delete_title.to_string(), what, s.delete)
            }
        };
        let inner = l.card.w - 2.0 * pad;
        c.text(&title, rect(l.card.x + pad, l.card.y + 16.0, inner, 24.0), Font::Title, t.t1, Align::Left);
        c.text(&subtitle, rect(l.card.x + pad, l.card.y + 40.0, inner, 18.0), Font::Small, t.t3, Align::Left);

        if let SheetKind::Lock { format, rar, .. } = &sheet.kind {
            for (id, r, label, on, enabled) in
                [(Id::SheetZip, l.zip, "ZIP", *format == Format::Zip, true), (Id::Sheet7z, l.seven, "7z", *format == Format::SevenZ, true), (Id::SheetRar, l.rar, "RAR", *format == Format::Rar, *rar)]
            {
                if on {
                    c.fill_round(r, 14.0, t.accent.fade(if t.dark { 0.32 } else { 0.18 }));
                } else if enabled && self.hover == Some(id) {
                    c.fill_round(r, 14.0, t.fill);
                } else {
                    c.stroke_round(r, 14.0, t.sep, 1.0);
                }
                let ink = match (on, enabled) {
                    (true, _) => t.t1,
                    (false, true) => t.t2,
                    (false, false) => t.t4,
                };
                c.text(label, r, Font::SmallBold, ink, Align::Center);
                if enabled {
                    self.hits.add(r, id);
                }
            }
            let note = match (format, rar) {
                (Format::Rar, _) => s.pw_note_rar,
                (Format::SevenZ, _) => s.pw_note_7z,
                (Format::Zip, true) => s.pw_note_zip,
                (Format::Zip, false) => s.pw_note_no_rar,
            };
            c.text(note, rect(l.card.x + pad, l.zip.bottom() + 6.0, inner, 18.0), Font::Small, t.t3, Align::Left);
        }

        if sheet.kind.has_field() {
            // le champ : le contrôle de saisie de Windows est posé par-dessus
            c.fill_round(l.field, m::FIELD_R, t.fill);
            c.stroke_round(l.field, m::FIELD_R, t.accent, 1.5);
            if sheet.kind.is_secret() {
                let label = if sheet.reveal { s.pw_hide } else { s.pw_show };
                if self.hover == Some(Id::SheetReveal) {
                    c.fill_round(l.reveal, 8.0, t.fill2);
                }
                c.text(label, l.reveal, Font::Small, if self.hover == Some(Id::SheetReveal) { t.t1 } else { t.t3 }, Align::Center);
                self.hits.add(l.reveal, Id::SheetReveal);
            }
            if let Some(error) = &sheet.error {
                c.text(error, rect(l.field.x + 2.0, l.field.bottom() + 6.0, inner, 18.0), Font::Small, t.red, Align::Left);
            }
        } else {
            // pas de champ : une ligne dit ce qui va se passer
            c.text(s.sftp_delete_text, rect(l.card.x + pad, l.card.y + 68.0, inner, 20.0), Font::Small, t.t2, Align::Left);
        }

        // le bouton d'une suppression est rouge : elle ne se rattrape pas
        let main = if matches!(sheet.kind, SheetKind::Delete { .. }) { t.red } else { t.accent };
        for (id, r, label, primary) in [(Id::SheetCancel, l.cancel, s.btn_cancel, false), (Id::SheetOk, l.ok, action, true)] {
            let hot = self.hover == Some(id);
            let down = self.pressed == Some(id) && hot;
            let bg = match (primary, hot, down) {
                (true, _, true) => main.mix(Color::hex(0x000000), 0.18),
                (true, true, _) => main.mix(Color::hex(0xffffff), 0.1),
                (true, false, _) => main,
                (false, _, true) => t.fill2,
                (false, true, _) => t.fill2,
                (false, false, _) => t.fill,
            };
            c.fill_round(r, 9.0, bg);
            c.text(label, r, Font::BodyBold, if primary { Color::hex(0xffffff) } else { t.t1 }, Align::Center);
            self.hits.add(r, id);
        }
    }
}

/// L'adresse tapée dans la boîte « Se connecter » : « utilisateur@hôte », avec ou sans « sftp:// » devant.
fn server_address(text: &str) -> Option<Location> {
    let text = text.trim();
    Location::parse_remote(text).or_else(|| Location::parse_remote(&format!("sftp://{text}")))
}
