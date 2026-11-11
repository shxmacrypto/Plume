// Registre des réglages (src/settings.json, partagé avec l'interface) : valeurs par défaut selon le mode
// (Ultra-léger / Complet), migration des profils 0.6, changement de mode qui respecte les réglages
// modifiés à la main (liste cfg.mod), réinitialisation par section ou globale.
use serde_json::{Value, json};
use std::sync::OnceLock;

pub const JSON: &str = include_str!("settings.json");
/// Version du schéma des réglages (0.6 = 1, 0.7 = 2).
pub const SCHEMA: u64 = 3;

pub struct Item {
    pub k: String,
    pub s: String,
    d: Value,
    dm: Option<[Value; 2]>,
    pub rs: bool,
}

impl Item {
    pub fn default_for(&self, ultra: bool) -> Value {
        match &self.dm {
            Some(m) => m[if ultra { 0 } else { 1 }].clone(),
            None => self.d.clone(),
        }
    }
}

/// Tous les réglages (les boutons d'action du registre n'ont pas de valeur).
pub fn items() -> &'static [Item] {
    static R: OnceLock<Vec<Item>> = OnceLock::new();
    R.get_or_init(|| {
        let v: Value = serde_json::from_str(JSON).expect("settings.json");
        v["items"]
            .as_array()
            .expect("items")
            .iter()
            .filter(|i| i["t"] != "btn")
            .map(|i| Item {
                k: i["k"].as_str().unwrap_or_default().to_string(),
                s: i["s"].as_str().unwrap_or_default().to_string(),
                d: i["d"].clone(),
                dm: i["dm"].as_array().map(|a| [a[0].clone(), a[1].clone()]),
                rs: i["rs"].as_u64() == Some(1),
            })
            .collect()
    })
}

fn find(k: &str) -> Option<&'static Item> {
    items().iter().find(|i| i.k == k)
}

fn ultra(cfg: &Value) -> bool {
    cfg["mode"] == "ultra"
}

fn modified(cfg: &Value) -> Vec<String> {
    cfg["mod"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect()
}

/// À l'ouverture : complète les réglages manquants avec la valeur par défaut du mode choisi.
/// Profil 0.6 : chaque valeur différente du défaut actuel est gardée et marquée « modifiée par l'utilisateur ».
pub fn migrate(cfg: &mut Value) {
    let v = cfg["cfgV"].as_u64().unwrap_or(1);
    // 3 : l'économie de batterie s'allume dès le passage sur batterie en mode Complet aussi (avant : sous 20 %),
    // sauf si l'utilisateur l'avait réglée lui-même
    if v == 2
        && !modified(cfg).iter().any(|k| k == "battWhen")
        && let Some(o) = cfg.as_object_mut()
    {
        o.remove("battWhen");
    }
    let u = ultra(cfg);
    // profil d'avant les réglages 0.7 : une valeur différente du défaut vient de l'utilisateur
    let old = v < 2;
    let mut m = modified(cfg);
    for i in items() {
        let def = i.default_for(u);
        match cfg.get(&i.k) {
            None | Some(Value::Null) => cfg[&i.k] = def,
            Some(v) if old && *v != def && !m.contains(&i.k) => m.push(i.k.clone()),
            _ => {}
        }
    }
    cfg["mod"] = json!(m);
    cfg["cfgV"] = json!(SCHEMA);
}

/// Changement de mode : réapplique ses valeurs par défaut, sauf aux réglages modifiés à la main.
pub fn apply_mode(cfg: &mut Value, mode: &str) {
    cfg["mode"] = json!(if mode == "ultra" { "ultra" } else { "full" });
    let u = ultra(cfg);
    let m = modified(cfg);
    for i in items().iter().filter(|i| i.dm.is_some() && !m.contains(&i.k)) {
        cfg[&i.k] = i.default_for(u);
    }
}

/// Le réglage `k` a été changé à la main : un changement de mode ne le touchera plus.
pub fn mark(cfg: &mut Value, k: &str) {
    if find(k).is_none() {
        return;
    }
    let mut m = modified(cfg);
    if !m.iter().any(|x| x == k) {
        m.push(k.to_string());
        cfg["mod"] = json!(m);
    }
}

/// Remet des réglages à leur valeur par défaut (celle du mode actuel). Renvoie les clés changées.
pub fn reset(cfg: &mut Value, section: Option<&str>) -> Vec<String> {
    let u = ultra(cfg);
    let mut m = modified(cfg);
    let mut changed = vec![];
    for i in items().iter().filter(|i| section.is_none_or(|s| s == i.s)) {
        let def = i.default_for(u);
        if cfg[&i.k] != def {
            cfg[&i.k] = def;
            changed.push(i.k.clone());
        }
        m.retain(|x| *x != i.k);
    }
    cfg["mod"] = json!(m);
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_06(mode: &str) -> Value {
        // plume.json d'une 0.6 : mode Complet, veille réglée à 30 min, thème sombre (ancien défaut)
        json!({"onboarded": true, "name": "Sam", "mode": mode, "lang": "fr", "search": "google", "theme": "dark",
               "sleep": 30, "eco": false, "ext": true, "ab": true, "tracking": "balanced", "motion": false, "updates": true})
    }

    #[test]
    fn registry_is_valid() {
        let v: Value = serde_json::from_str(JSON).unwrap();
        let all = v["items"].as_array().unwrap();
        let keys: Vec<&str> = all.iter().map(|i| i["k"].as_str().unwrap()).collect();
        for (n, k) in keys.iter().enumerate() {
            assert!(!keys[..n].contains(k), "doublon : {k}");
        }
        for i in all {
            if let Some(p) = i["p"].as_str() {
                assert!(keys.contains(&p), "parent inconnu : {p}");
            }
            if i["t"] != "btn" {
                assert!(!i["d"].is_null() || i["dm"].is_array(), "pas de défaut : {}", i["k"]);
            }
        }
    }

    #[test]
    fn migration_keeps_user_values_and_fills_new_ones() {
        let mut c = profile_06("full");
        migrate(&mut c);
        assert_eq!(c["cfgV"], SCHEMA);
        assert_eq!(c["sleep"], 30); // gardé
        assert_eq!(c["name"], "Sam");
        assert_eq!(c["ram"], true); // nouveau, défaut Complet
        assert_eq!(c["prefetch"], true);
        assert_eq!(c["widgets"], json!(["clock", "freq"]));
        let m = modified(&c);
        assert!(m.contains(&"sleep".into()) && m.contains(&"theme".into()) && m.contains(&"lang".into()));
        assert!(!m.contains(&"ext".into())); // égal au défaut Complet
        // une 2e ouverture ne change rien
        let before = c.clone();
        migrate(&mut c);
        assert_eq!(c, before);
    }

    #[test]
    fn migration_uses_the_chosen_mode() {
        let mut c = profile_06("ultra");
        migrate(&mut c);
        assert_eq!(c["ram"], false);
        assert_eq!(c["battWhen"], "battery");
        assert_eq!(c["widgets"], json!([]));
    }

    #[test]
    fn mode_switch_respects_manual_changes() {
        let mut c = profile_06("full");
        migrate(&mut c);
        c["ram"] = json!(true);
        mark(&mut c, "ram"); // choisi à la main
        apply_mode(&mut c, "ultra");
        assert_eq!(c["mode"], "ultra");
        assert_eq!(c["ram"], true); // gardé
        assert_eq!(c["sleep"], 30); // modifié en 0.6 : gardé
        assert_eq!(c["eco"], true); // défaut Ultra-léger appliqué
        assert_eq!(c["prefetch"], false);
        apply_mode(&mut c, "full");
        assert_eq!(c["eco"], false);
    }

    #[test]
    fn reset_section_and_all() {
        let mut c = profile_06("full");
        migrate(&mut c);
        c["glass"] = json!(10);
        mark(&mut c, "glass");
        c["peekSize"] = json!(60);
        mark(&mut c, "peekSize");
        let ch = reset(&mut c, Some("look"));
        assert!(ch.contains(&"glass".into()) && ch.contains(&"theme".into()));
        assert_eq!(c["glass"], 60);
        assert_eq!(c["peekSize"], 60); // autre section : intact
        reset(&mut c, None);
        assert_eq!(c["peekSize"], 80);
        assert_eq!(c["sleep"], 10);
        assert!(modified(&c).is_empty());
        assert_eq!(c["name"], "Sam"); // hors registre : jamais touché
    }

    #[test]
    fn schema_3_battery_saver_default() {
        // profil 0.7 en mode Complet : « sous 20 % » (ancien défaut, non modifié) devient « dès la batterie »
        let mut c = json!({"mode": "full", "cfgV": 2, "battWhen": "below", "mod": []});
        migrate(&mut c);
        assert_eq!(c["battWhen"], "battery");
        assert!(!c["mod"].as_array().unwrap().iter().any(|k| k == "battWhen"));
        // réglé à la main : on n'y touche pas
        let mut c = json!({"mode": "full", "cfgV": 2, "battWhen": "below", "mod": ["battWhen"]});
        migrate(&mut c);
        assert_eq!(c["battWhen"], "below");
        assert_eq!(c["cfgV"], SCHEMA);
    }
}
