//! English and French. The interface is written in English; `tr` gives the French when Plume speaks French
//! (Plume Calendar's choice, otherwise the Suite's, otherwise Windows'). Dates and times are written the way each
//! language writes them.

use crate::date::Date;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Globalization::GetUserDefaultUILanguage;

static FR: AtomicBool = AtomicBool::new(false);

pub fn set(lang: &str) {
    let fr = match lang {
        "fr" => true,
        "en" => false,
        _ => system_french(),
    };
    FR.store(fr, Ordering::Relaxed);
}

pub fn french() -> bool {
    FR.load(Ordering::Relaxed)
}

fn system_french() -> bool {
    if let Ok(l) = std::env::var("PLUME_SUITE_LANG") {
        return l == "fr";
    }
    let base = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).unwrap_or_default().join("Plume");
    for f in [base.join("Suite").join("settings.json"), base.join("plume.json")] {
        let cfg = std::fs::read_to_string(f).unwrap_or_default();
        if cfg.contains(r#""lang":"fr""#) {
            return true;
        }
        if cfg.contains(r#""lang":"en""#) {
            return false;
        }
    }
    unsafe { GetUserDefaultUILanguage() & 0x3ff == 0x0c }
}

/// `{}` holes filled in order.
pub fn fill(s: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut rest = s;
    let mut i = 0;
    while let Some(a) = rest.find('{') {
        let Some(b) = rest[a..].find('}') else { break };
        out.push_str(&rest[..a]);
        out.push_str(args.get(i).copied().unwrap_or(""));
        i += 1;
        rest = &rest[a + b + 1..];
    }
    out.push_str(rest);
    out
}

const MONTHS_EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const MONTHS_FR: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];
const SHORT_FR: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
const DAYS_EN: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const DAYS_FR: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];

/// "November" / "novembre".
pub fn month(m: u32) -> &'static str {
    let i = (m.clamp(1, 12) - 1) as usize;
    if french() { MONTHS_FR[i] } else { MONTHS_EN[i] }
}

/// "Nov" / "nov.".
pub fn month_short(m: u32) -> String {
    let i = (m.clamp(1, 12) - 1) as usize;
    if french() { SHORT_FR[i].to_string() } else { MONTHS_EN[i][..3].to_string() }
}

/// "Wednesday" / "mercredi" (0 = Monday).
pub fn day(wd: u32) -> &'static str {
    if french() { DAYS_FR[wd as usize % 7] } else { DAYS_EN[wd as usize % 7] }
}

/// "Wed" / "mer.".
pub fn day_short(wd: u32) -> String {
    let d = day(wd);
    if french() { format!("{}.", &d[..3]) } else { d[..3].to_string() }
}

/// "W" / "M" (the mini month's column heads).
pub fn day_letter(wd: u32) -> String {
    day(wd).chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

pub fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// "November 2026" / "Novembre 2026".
pub fn month_year(d: Date) -> String {
    cap(&format!("{} {}", month(d.m), d.y))
}

/// "Wednesday, November 11" / "Mercredi 11 novembre".
pub fn long_date(d: Date) -> String {
    if french() {
        cap(&format!("{} {} {}", day(d.weekday()), if d.d == 1 { "1er".into() } else { d.d.to_string() }, month(d.m)))
    } else {
        format!("{}, {} {}", day(d.weekday()), month(d.m), d.d)
    }
}

/// Like `long_date`, with the year.
pub fn long_date_year(d: Date) -> String {
    if french() { format!("{} {}", long_date(d), d.y) } else { format!("{}, {}", long_date(d), d.y) }
}

/// "Nov 11" / "11 nov.".
pub fn short_date(d: Date) -> String {
    if french() { format!("{} {}", d.d, month_short(d.m)) } else { format!("{} {}", month_short(d.m), d.d) }
}

/// "14:30", or "2:30 PM" / "2 PM" on a 12-hour clock.
pub fn time(secs: u32, h24: bool) -> String {
    let h = secs / 3600 % 24;
    let m = secs / 60 % 60;
    if h24 {
        return format!("{h:02}:{m:02}");
    }
    let (hh, ap) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    if m == 0 { format!("{hh} {ap}") } else { format!("{hh}:{m:02} {ap}") }
}

/// A short hour label for the grid: "14:00" / "2 PM".
pub fn hour_label(h: u32, h24: bool) -> String {
    time(h * 3600, h24)
}

/// "10 minutes before", "1 hour before", "1 day before", "At the start".
pub fn reminder(before: i64) -> String {
    if before == 0 {
        return tr("At the start").into();
    }
    let (n, unit) = if before % 86_400 == 0 {
        (before / 86_400, if before == 86_400 { tr("day") } else { tr("days") })
    } else if before % 3600 == 0 {
        (before / 3600, if before == 3600 { tr("hour") } else { tr("hours") })
    } else {
        (before / 60, if before == 60 { tr("minute") } else { tr("minutes") })
    };
    if before < 0 { fill(tr("{} {} after"), &[&(-n).to_string(), unit]) } else { fill(tr("{} {} before"), &[&n.to_string(), unit]) }
}

pub fn tr(en: &'static str) -> &'static str {
    if !french() {
        return en;
    }
    fr(en).unwrap_or(en)
}

fn fr(en: &str) -> Option<&'static str> {
    Some(match en {
        "Personal" => "Personnel",
        "Today" => "Aujourd'hui",
        "Day" => "Jour",
        "Week" => "Semaine",
        "Month" => "Mois",
        "Agenda" => "Agenda",
        "New event" => "Nouvel événement",
        "Calendars" => "Calendriers",
        "Subscriptions" => "Abonnements",
        "All day" => "Toute la journée",
        "all day" => "toute la journée",
        "At the start" => "Au début",
        "day" => "jour",
        "days" => "jours",
        "hour" => "heure",
        "hours" => "heures",
        "minute" => "minute",
        "minutes" => "minutes",
        "{} {} before" => "{} {} avant",
        "{} {} after" => "{} {} après",
        "Week {}" => "Semaine {}",
        "W" => "S",
        "Add an event: “Lunch with Max tomorrow 12:30”" => "Ajouter : « Déjeuner avec Max demain 12h30 »",
        "Settings" => "Paramètres",
        "Title" => "Titre",
        "Location" => "Lieu",
        "Notes" => "Notes",
        "Starts" => "Début",
        "Ends" => "Fin",
        "Repeat" => "Répétition",
        "Reminder" => "Rappel",
        "Calendar" => "Calendrier",
        "Color" => "Couleur",
        "Save" => "Enregistrer",
        "Cancel" => "Annuler",
        "Delete" => "Supprimer",
        "Edit" => "Modifier",
        "Close" => "Fermer",
        "Untitled" => "Sans titre",
        "None" => "Aucun",
        "Never" => "Jamais",
        "Does not repeat" => "Ne se répète pas",
        "Every day" => "Tous les jours",
        "Every weekday" => "Du lundi au vendredi",
        "Every week" => "Toutes les semaines",
        "Every month" => "Tous les mois",
        "Every year" => "Tous les ans",
        "Every {} days" => "Tous les {} jours",
        "Every {} weeks" => "Toutes les {} semaines",
        "Every {} months" => "Tous les {} mois",
        "Every {} years" => "Tous les {} ans",
        "on {}" => "le {}",
        "on the {}" => "le {}",
        "on day {}" => "le {}",
        "first" => "premier",
        "second" => "deuxième",
        "third" => "troisième",
        "fourth" => "quatrième",
        "fifth" => "cinquième",
        "last" => "dernier",
        "{} times" => "{} fois",
        "until {}" => "jusqu'au {}",
        "Every" => "Tous les",
        "Year" => "Année",
        "Ends on" => "Se termine",
        "After" => "Après",
        "times" => "fois",
        "Until" => "Jusqu'au",
        "Monthly on day {}" => "Tous les mois le {}",
        "Monthly on the {} {}" => "Tous les mois le {} {}",
        "Repeating event" => "Événement répété",
        "Change only this event, or all the events of the series?" => "Modifier seulement cet événement, ou tous ceux de la série ?",
        "Delete only this event, or all the events of the series?" => "Supprimer seulement cet événement, ou tous ceux de la série ?",
        "This event" => "Cet événement",
        "All events" => "Tous les événements",
        "New calendar" => "Nouveau calendrier",
        "Subscribe" => "S'abonner",
        "Subscribe to a calendar" => "S'abonner à un calendrier",
        "Paste the address of a calendar (.ics or webcal://): school, sports, public holidays… It stays read-only and refreshes every hour." => {
            "Collez l'adresse d'un calendrier (.ics ou webcal://) : école, sport, jours fériés… Il reste en lecture seule et se met à jour toutes les heures."
        }
        "Calendar name" => "Nom du calendrier",
        "Add" => "Ajouter",
        "Import .ics" => "Importer un .ics",
        "Export .ics" => "Exporter en .ics",
        "Import" => "Importer",
        "Export" => "Exporter",
        "Refresh" => "Actualiser",
        "Remove" => "Retirer",
        "Rename" => "Renommer",
        "Show" => "Afficher",
        "Hide" => "Masquer",
        "Read-only" => "Lecture seule",
        "Updated {}" => "Mis à jour {}",
        "Downloading…" => "Téléchargement…",
        "Could not download: {}" => "Téléchargement impossible : {}",
        "{} events imported into {}" => "{} événements importés dans {}",
        "Nothing to import in this file" => "Rien à importer dans ce fichier",
        "Subscribed to {} ({} events)" => "Abonné à {} ({} événements)",
        "Exported to {}" => "Exporté vers {}",
        "Event added: {}" => "Événement ajouté : {}",
        "Event deleted" => "Événement supprimé",
        "Undo" => "Annuler",
        "Nothing planned" => "Rien de prévu",
        "Nothing in the next {} days." => "Rien dans les {} prochains jours.",
        "{} more" => "{} de plus",
        "Now" => "Maintenant",
        "Snooze 5 min" => "Rappeler dans 5 min",
        "Dismiss" => "Ignorer",
        "Open" => "Ouvrir",
        "Quit" => "Quitter",
        "In {} min" => "Dans {} min",
        "Starting now" => "Maintenant",
        "Started {} min ago" => "A commencé il y a {} min",
        // settings
        "General" => "Général",
        "Language" => "Langue",
        "System" => "Système",
        "Theme" => "Thème",
        "Dark" => "Sombre",
        "Light" => "Clair",
        "First day of the week" => "Premier jour de la semaine",
        "Automatic" => "Automatique",
        "Monday" => "Lundi",
        "Sunday" => "Dimanche",
        "Saturday" => "Samedi",
        "Time format" => "Format de l'heure",
        "24-hour" => "24 heures",
        "12-hour" => "12 heures",
        "Week numbers" => "Numéros de semaine",
        "Show the week numbers in the month and the week." => "Afficher les numéros de semaine dans le mois et la semaine.",
        "Default reminder" => "Rappel par défaut",
        "Given to new events." => "Donné aux nouveaux événements.",
        "Reminders" => "Rappels",
        "Reminder style" => "Style des rappels",
        "Plume popup" => "Fenêtre Plume",
        "Windows notification" => "Notification Windows",
        "Keep running near the clock" => "Rester près de l'horloge",
        "Closing the window keeps Plume Calendar running for reminders." => "Fermer la fenêtre laisse Plume Calendar ouvert pour les rappels.",
        "Start with Windows" => "Lancer avec Windows",
        "Starts in the background, near the clock, for reminders." => "Démarre en arrière-plan, près de l'horloge, pour les rappels.",
        "About" => "À propos",
        "Your agenda, offline. Only the calendars you subscribe to go online, to download them." => {
            "Votre agenda, hors ligne. Seuls les calendriers auxquels vous vous abonnez passent par Internet, pour les télécharger."
        }
        "Open the data folder" => "Ouvrir le dossier des données",
        "Back" => "Retour",
        "Search" => "Rechercher",
        "Delete this calendar and its events?" => "Supprimer ce calendrier et ses événements ?",
        "Unsubscribe from this calendar?" => "Se désabonner de ce calendrier ?",
        "Event" => "Événement",
        "Edit event" => "Modifier l'événement",
        "Date" => "Date",
        "Time" => "Heure",
        "Not a date" => "Date non reconnue",
        "Not a time" => "Heure non reconnue",
        "No calendar you can write to" => "Aucun calendrier modifiable",
        "This calendar is read-only" => "Ce calendrier est en lecture seule",
        "Time zone: {}" => "Fuseau horaire : {}",
        "Mo" => "Lu",
        "Tu" => "Ma",
        "We" => "Me",
        "Th" => "Je",
        "Fr" => "Ve",
        "Sa" => "Sa",
        "Su" => "Di",
        "Ends never" => "Sans fin",
        "Count" => "Nombre",
        "End date" => "Date de fin",
        "Reminder: {}" => "Rappel : {}",
        "Minimize" => "Réduire",
        "Maximize" => "Agrandir",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes() {
        assert_eq!(fill("{} of {}", &["3", "10"]), "3 of 10");
        assert_eq!(fill("no holes", &["x"]), "no holes");
    }

    #[test]
    fn formats() {
        FR.store(false, Ordering::Relaxed);
        assert_eq!(time(14 * 3600 + 30 * 60, true), "14:30");
        assert_eq!(time(14 * 3600, false), "2 PM");
        assert_eq!(time(30 * 60, false), "12:30 AM");
        assert_eq!(long_date(Date::new(2026, 11, 11)), "Wednesday, November 11");
        assert_eq!(reminder(600), "10 minutes before");
        assert_eq!(reminder(86_400), "1 day before");
    }

    /// Every `tr("…")` of the program has its French.
    #[test]
    fn all_translated() {
        let sources = [
            include_str!("main.rs"),
            include_str!("model.rs"),
            include_str!("reminders.rs"),
            include_str!("popup.rs"),
            include_str!("ui/mod.rs"),
            include_str!("ui/paint.rs"),
            include_str!("ui/views.rs"),
            include_str!("ui/editor.rs"),
            include_str!("ui/settings_view.rs"),
            include_str!("i18n.rs"),
        ];
        let mut missing = Vec::new();
        for src in sources {
            let mut rest = src;
            while let Some(k) = rest.find("tr(\"") {
                rest = &rest[k + 4..];
                let Some(end) = rest.find("\")") else { break };
                let key = &rest[..end];
                if !key.contains('"') && key != "…" && fr(key).is_none() && !missing.contains(&key) {
                    missing.push(key);
                }
            }
        }
        assert!(missing.is_empty(), "no French for {missing:?}");
    }
}
