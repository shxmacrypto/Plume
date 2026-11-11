//! Plume FEx dans Windows : l'entrée « Ouvrir dans Plume FEx » du clic droit sur un dossier, et l'option
//! « explorateur par défaut » (les dossiers et Win+E s'ouvrent dans Plume FEx).
//!
//! Tout s'écrit dans le registre de l'utilisateur (`HKCU\Software\Classes`), sans droits d'administrateur, et tout
//! se défait : ce qui existait avant est gardé et remis en place. Rien n'est activé tant que l'utilisateur ne le
//! demande pas.
//!
//! - Entrée du clic droit : un verbe `PlumeFEx` sous `Directory\shell`, `Directory\Background\shell` et `Drive\shell`.
//! - Par défaut : ce verbe devient celui du double-clic sur un dossier ou un lecteur, et la commande de Win+E
//!   (`CLSID\{52205fd8-…}\shell\opennewwindow`) lance Plume FEx. Dans les fenêtres de l'Explorateur déjà ouvertes,
//!   la navigation reste celle de l'Explorateur. Windows 11 25H2 ne lit plus cette commande pour Win+E : le veilleur
//!   de `hotkey.rs` (valeur « Plume FEx Win+E » de la clé Run) attrape alors Win+E au clavier.

use std::path::Path;
use windows::{
    Win32::{
        System::Registry::{HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegDeleteKeyW, RegDeleteTreeW, RegGetValueW, RegSetKeyValueW},
        UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify},
    },
    core::PCWSTR,
};

const VERB: &str = "PlumeFEx";
/// Les objets dont le clic droit reçoit l'entrée, et l'argument que Windows remplace par le dossier.
const PLACES: [(&str, &str); 3] = [("Directory", "%1"), ("Directory\\Background", "%V"), ("Drive", "%1")];
/// L'objet « Explorateur de fichiers » que Win+E ouvre.
const WIN_E: &str = "CLSID\\{52205fd8-5dfb-447d-801a-d0b52f2e83e1}\\shell\\opennewwindow\\command";
/// La valeur de la clé Run qui lance le veilleur de Win+E avec Windows.
const HELPER: &str = "Plume FEx Win+E";

/// Ce qu'il y avait dans le registre avant que Plume FEx ne devienne l'explorateur par défaut.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Backup {
    /// Verbe par défaut des dossiers et des lecteurs (None : aucun n'était choisi).
    pub directory: Option<String>,
    pub drive: Option<String>,
    /// Commande de Win+E propre à l'utilisateur, et sa valeur `DelegateExecute`.
    pub win_e: Option<String>,
    pub win_e_delegate: Option<String>,
}

/// La partie du registre où s'inscrivent les types de fichiers de l'utilisateur.
pub struct Classes {
    base: String,
}

fn wz(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Valeur texte d'une clé de HKCU (`name` vide : la valeur par défaut).
fn get_at(key: &[u16], name: &str) -> Option<String> {
    let name = wz(name);
    let mut buf = vec![0u16; 2048];
    let mut bytes = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(name.as_ptr()), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut bytes))
    };
    r.is_ok().then(|| {
        let len = (bytes as usize / 2).min(buf.len());
        String::from_utf16_lossy(&buf[..len]).trim_end_matches('\0').to_string()
    })
}

fn set_at(key: &[u16], name: &str, value: &str) -> bool {
    let (name, data) = (wz(name), wz(value));
    unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(name.as_ptr()), REG_SZ.0, Some(data.as_ptr() as *const _), (data.len() * 2) as u32)
    }
    .is_ok()
}

fn remove_value_at(key: &[u16], name: &str) {
    let name = wz(name);
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(name.as_ptr()));
    }
}

impl Classes {
    /// Le registre réel, sauf pendant les essais : ceux-ci (reconnus à leur dossier de données à part) n'y touchent
    /// jamais, à moins de désigner une clé d'essai par PLUME_FEX_CLASSES_ROOT.
    pub fn user() -> Option<Classes> {
        match (std::env::var("PLUME_FEX_CLASSES_ROOT"), std::env::var_os("PLUME_FEX_PROFILE")) {
            (Ok(base), _) if !base.trim().is_empty() => Some(Classes { base }),
            (_, Some(_)) => None,
            _ => Some(Classes { base: "Software\\Classes".to_string() }),
        }
    }

    #[cfg(test)]
    fn at(base: &str) -> Classes {
        Classes { base: base.to_string() }
    }

    fn key(&self, sub: &str) -> Vec<u16> {
        if sub.is_empty() { wz(&self.base) } else { wz(&format!("{}\\{sub}", self.base)) }
    }

    /// Le vrai registre de l'utilisateur (et non une clé d'essai).
    fn real(&self) -> bool {
        self.base.eq_ignore_ascii_case("Software\\Classes")
    }

    /// La clé Run (démarrage avec Windows) ; sous la clé d'essai pendant les essais.
    fn run_key(&self) -> Vec<u16> {
        if self.real() { wz("Software\\Microsoft\\Windows\\CurrentVersion\\Run") } else { wz(&format!("{}\\Run", self.base)) }
    }

    /// La commande du veilleur de Win+E inscrite pour le démarrage de Windows, s'il y en a une.
    #[cfg(test)]
    pub fn helper(&self) -> Option<String> {
        get_at(&self.run_key(), HELPER)
    }

    /// Inscrit (et lance) ou retire (et arrête) le veilleur de Win+E.
    fn set_helper(&self, exe: &Path, on: bool) -> bool {
        if on {
            let ok = set_at(&self.run_key(), HELPER, &format!("{} --hotkey", Self::command(exe, "")));
            if self.real() {
                super::hotkey::start(exe);
            }
            ok
        } else {
            remove_value_at(&self.run_key(), HELPER);
            if self.real() {
                super::hotkey::stop();
            }
            true
        }
    }

    /// Prévient Windows que les associations ont changé (sinon il faudrait attendre la prochaine session).
    /// Inutile, et sans effet voulu, pour une clé d'essai.
    fn notify(&self) {
        if self.real() {
            unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
        }
    }

    /// Valeur texte d'une clé (`name` vide : la valeur par défaut).
    fn get(&self, sub: &str, name: &str) -> Option<String> {
        get_at(&self.key(sub), name)
    }

    fn set(&self, sub: &str, name: &str, value: &str) -> bool {
        set_at(&self.key(sub), name, value)
    }

    fn remove_value(&self, sub: &str, name: &str) {
        remove_value_at(&self.key(sub), name)
    }

    /// Supprime une clé et tout ce qu'elle contient.
    fn remove_tree(&self, sub: &str) {
        let key = self.key(sub);
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()));
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()));
        }
    }

    /// Supprime une clé si elle est vide (ni valeur ni sous-clé) : on ne laisse pas de coquilles derrière soi.
    fn remove_if_empty(&self, sub: &str) {
        let key = self.key(sub);
        unsafe {
            // RegDeleteKeyW refuse de supprimer une clé qui a des sous-clés ; une clé qui n'a que des valeurs
            // serait supprimée, d'où la vérification de la valeur par défaut
            if self.get(sub, "").is_none() {
                let _ = RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()));
            }
        }
    }

    fn command(exe: &Path, arg: &str) -> String {
        if arg.is_empty() { format!("\"{}\"", exe.display()) } else { format!("\"{}\" \"{arg}\"", exe.display()) }
    }

    /// L'entrée « Ouvrir dans Plume FEx » est-elle inscrite (pour ce programme-ci) ?
    pub fn has_entry(&self, exe: &Path) -> bool {
        self.get(&format!("Directory\\shell\\{VERB}\\command"), "").is_some_and(|c| c.eq_ignore_ascii_case(&Self::command(exe, "%1")))
    }

    /// Inscrit ou retire l'entrée du clic droit. `label` : son texte, dans la langue de l'interface.
    pub fn set_entry(&self, exe: &Path, label: &str, on: bool) -> bool {
        let mut ok = true;
        for (place, arg) in PLACES {
            let verb = format!("{place}\\shell\\{VERB}");
            if on {
                ok &= self.set(&verb, "", label);
                ok &= self.set(&verb, "Icon", &format!("\"{}\",0", exe.display()));
                ok &= self.set(&format!("{verb}\\command"), "", &Self::command(exe, arg));
            } else {
                self.remove_tree(&verb);
            }
        }
        self.notify();
        ok
    }

    /// Plume FEx est-il l'explorateur par défaut (pour ce programme-ci) ?
    pub fn is_default(&self, exe: &Path) -> bool {
        self.has_entry(exe) && self.get("Directory\\shell", "").as_deref() == Some(VERB) && self.get("Drive\\shell", "").as_deref() == Some(VERB)
    }

    /// Fait de Plume FEx l'explorateur par défaut. Renvoie ce qu'il y avait avant, à garder pour `restore`.
    pub fn make_default(&self, exe: &Path, label: &str) -> Option<Backup> {
        // si c'est déjà fait, ce qu'on lirait maintenant serait notre propre réglage, pas l'ancien
        let previous = |value: Option<String>| value.filter(|v| v != VERB);
        let backup = Backup {
            directory: previous(self.get("Directory\\shell", "")),
            drive: previous(self.get("Drive\\shell", "")),
            win_e: self.get(WIN_E, "").filter(|c| !c.eq_ignore_ascii_case(&Self::command(exe, ""))),
            win_e_delegate: self.get(WIN_E, "DelegateExecute").filter(|d| !d.is_empty()),
        };
        let ok = self.set_entry(exe, label, true)
            && self.set("Directory\\shell", "", VERB)
            && self.set("Drive\\shell", "", VERB)
            && self.set(WIN_E, "", &Self::command(exe, ""))
            // une valeur vide ici prend le pas sur celle de Windows, qui confierait Win+E à l'Explorateur
            && self.set(WIN_E, "DelegateExecute", "")
            // Windows 11 25H2 : Win+E ne passe plus par cette commande, le veilleur l'attrape au clavier
            && self.set_helper(exe, true);
        self.notify();
        if ok {
            Some(backup)
        } else {
            self.restore(&backup);
            None
        }
    }

    /// Rend les dossiers et Win+E à ce qui les ouvrait avant. L'entrée du clic droit reste en place.
    pub fn restore(&self, backup: &Backup) {
        // le veilleur s'arrête d'abord : Win+E redevient aussitôt celui de Windows
        self.set_helper(Path::new(""), false);
        for (place, old) in [("Directory\\shell", &backup.directory), ("Drive\\shell", &backup.drive)] {
            match old {
                Some(verb) => {
                    self.set(place, "", verb);
                }
                None => self.remove_value(place, ""),
            }
        }
        match &backup.win_e {
            Some(command) => {
                self.set(WIN_E, "", command);
                match &backup.win_e_delegate {
                    Some(d) => {
                        self.set(WIN_E, "DelegateExecute", d);
                    }
                    None => self.remove_value(WIN_E, "DelegateExecute"),
                }
            }
            // l'utilisateur n'avait pas de commande à lui : nos clés disparaissent, celle de Windows reprend
            None => {
                self.remove_tree(WIN_E);
                for parent in [
                    "CLSID\\{52205fd8-5dfb-447d-801a-d0b52f2e83e1}\\shell\\opennewwindow",
                    "CLSID\\{52205fd8-5dfb-447d-801a-d0b52f2e83e1}\\shell",
                    "CLSID\\{52205fd8-5dfb-447d-801a-d0b52f2e83e1}",
                ] {
                    self.remove_if_empty(parent);
                }
            }
        }
        self.notify();
    }

    /// Le programme a changé de place depuis l'inscription : les commandes inscrites sont remises à jour.
    /// Sans cela, un double-clic sur un dossier chercherait un programme qui n'est plus là.
    pub fn heal(&self, exe: &Path, label: &str) {
        let Some(command) = self.get(&format!("Directory\\shell\\{VERB}\\command"), "") else { return };
        if command.eq_ignore_ascii_case(&Self::command(exe, "%1")) {
            return;
        }
        self.set_entry(exe, label, true);
        if self.get("Directory\\shell", "").as_deref() == Some(VERB) {
            self.set(WIN_E, "", &Self::command(exe, ""));
            self.set_helper(exe, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Une clé d'essai à part : le vrai registre des types de fichiers n'est jamais touché par les essais.
    fn scratch(tag: &str) -> Classes {
        let c = Classes::at(&format!("Software\\PlumeFEx-Test\\{tag}-{}\\Classes", std::process::id()));
        c.remove_tree("");
        c
    }

    fn cleanup(c: &Classes) {
        let root = c.base.trim_end_matches("\\Classes").to_string();
        Classes::at(&root).remove_tree("");
    }

    #[test]
    fn right_click_entry() {
        let c = scratch("entry");
        let exe = PathBuf::from(r"C:\Apps\Plume\plume-fex.exe");
        assert!(!c.has_entry(&exe));
        assert!(c.set_entry(&exe, "Open in Plume FEx", true));
        assert!(c.has_entry(&exe) && !c.is_default(&exe));
        assert_eq!(c.get("Directory\\shell\\PlumeFEx", "").as_deref(), Some("Open in Plume FEx"));
        assert_eq!(c.get("Directory\\shell\\PlumeFEx\\command", "").as_deref(), Some(r#""C:\Apps\Plume\plume-fex.exe" "%1""#));
        assert_eq!(c.get("Directory\\Background\\shell\\PlumeFEx\\command", "").as_deref(), Some(r#""C:\Apps\Plume\plume-fex.exe" "%V""#));
        assert_eq!(c.get("Drive\\shell\\PlumeFEx\\Icon", "").as_deref(), None);
        assert_eq!(c.get("Drive\\shell\\PlumeFEx", "Icon").as_deref(), Some(r#""C:\Apps\Plume\plume-fex.exe",0"#));
        // un autre exemplaire du programme, ailleurs, ne se croit pas inscrit
        assert!(!c.has_entry(Path::new(r"D:\Other\plume-fex.exe")));
        // le programme a déménagé : les commandes suivent
        let moved = PathBuf::from(r"D:\New Place\plume-fex.exe");
        c.heal(&moved, "Open in Plume FEx");
        assert!(c.has_entry(&moved) && !c.has_entry(&exe));
        assert!(c.set_entry(&moved, "", false));
        assert!(!c.has_entry(&moved));
        assert_eq!(c.get("Directory\\shell\\PlumeFEx", ""), None);
        cleanup(&c);
    }

    #[test]
    fn default_explorer_and_back() {
        let c = scratch("default");
        let exe = PathBuf::from(r"C:\Apps\Plume\plume-fex.exe");
        // l'utilisateur avait déjà choisi un autre gestionnaire pour les dossiers, pas pour les lecteurs
        assert!(c.set("Directory\\shell", "", "OtherManager"));
        let backup = c.make_default(&exe, "Open in Plume FEx").expect("registered");
        assert_eq!(backup, Backup { directory: Some("OtherManager".into()), drive: None, win_e: None, win_e_delegate: None });
        assert!(c.is_default(&exe));
        assert_eq!(c.get("Directory\\shell", "").as_deref(), Some("PlumeFEx"));
        assert_eq!(c.get(WIN_E, "").as_deref(), Some(r#""C:\Apps\Plume\plume-fex.exe""#));
        assert_eq!(c.get(WIN_E, "DelegateExecute").as_deref(), Some(""));
        // le veilleur de Win+E démarre avec Windows (sous la clé d'essai, jamais la vraie clé Run)
        assert_eq!(c.helper().as_deref(), Some(r#""C:\Apps\Plume\plume-fex.exe" --hotkey"#));
        // demandé deux fois : la sauvegarde ne retient pas notre propre réglage
        let again = c.make_default(&exe, "Open in Plume FEx").expect("registered");
        assert_eq!(again, Backup::default());
        // retour en arrière : tout revient comme avant, l'entrée du clic droit reste
        c.restore(&backup);
        assert!(!c.is_default(&exe) && c.has_entry(&exe));
        assert_eq!(c.get("Directory\\shell", "").as_deref(), Some("OtherManager"));
        assert_eq!(c.get("Drive\\shell", ""), None);
        assert_eq!(c.get(WIN_E, ""), None);
        assert_eq!(c.get(WIN_E, "DelegateExecute"), None);
        assert_eq!(c.helper(), None);
        // une commande Win+E déjà personnalisée est remise telle quelle
        assert!(c.set(WIN_E, "", "other.exe") && c.set(WIN_E, "DelegateExecute", "{1234}"));
        let backup = c.make_default(&exe, "Open in Plume FEx").expect("registered");
        assert_eq!((backup.win_e.as_deref(), backup.win_e_delegate.as_deref()), (Some("other.exe"), Some("{1234}")));
        c.restore(&backup);
        assert_eq!((c.get(WIN_E, "").as_deref(), c.get(WIN_E, "DelegateExecute").as_deref()), (Some("other.exe"), Some("{1234}")));
        // le programme déplacé alors qu'il est l'explorateur par défaut : Win+E suit aussi
        let backup = c.make_default(&exe, "Open in Plume FEx").expect("registered");
        let moved = PathBuf::from(r"D:\Plume\plume-fex.exe");
        c.heal(&moved, "Open in Plume FEx");
        assert!(c.is_default(&moved));
        assert_eq!(c.get(WIN_E, "").as_deref(), Some(r#""D:\Plume\plume-fex.exe""#));
        assert_eq!(c.helper().as_deref(), Some(r#""D:\Plume\plume-fex.exe" --hotkey"#));
        c.restore(&backup);
        assert_eq!(c.helper(), None);
        cleanup(&c);
    }

    #[test]
    fn tests_never_get_the_real_registry() {
        // un essai a toujours son dossier de données à part : sans clé d'essai désignée, pas d'accès au registre
        unsafe { std::env::set_var("PLUME_FEX_PROFILE", r"C:\Temp\fex-test-profile") };
        unsafe { std::env::remove_var("PLUME_FEX_CLASSES_ROOT") };
        assert!(Classes::user().is_none());
        unsafe { std::env::set_var("PLUME_FEX_CLASSES_ROOT", r"Software\PlumeFEx-Test\x\Classes") };
        assert_eq!(Classes::user().map(|c| c.base), Some(r"Software\PlumeFEx-Test\x\Classes".to_string()));
        unsafe { std::env::remove_var("PLUME_FEX_CLASSES_ROOT") };
        unsafe { std::env::remove_var("PLUME_FEX_PROFILE") };
    }
}
