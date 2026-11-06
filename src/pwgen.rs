// Mots de passe (#22, #23) : générateur (caractères ou phrase de passe), audit (faibles / réutilisés)
// et vérification des fuites via Have I Been Pwned (k-anonymat : seuls 5 caractères du SHA-1 partent).
use serde_json::{Value, json};
use sha1::{Digest, Sha1};

const WORDS: &[&str] = &[
    "acid", "acorn", "actor", "agent", "album", "alley", "amber", "angle", "apple", "april", "arena", "arrow", "atlas", "audio", "autumn", "badge", "bagel", "baker",
    "bamboo", "banjo", "basil", "beach", "beard", "berry", "bison", "blade", "blink", "bloom", "board", "bonus", "boost", "brave", "bread", "brick", "brook", "brush",
    "bubble", "bucket", "cabin", "cable", "cactus", "camel", "candy", "canoe", "canvas", "cargo", "carrot", "castle", "cedar", "chalk", "charm", "cheese", "cherry", "chess",
    "chief", "cider", "cinema", "circle", "citrus", "clay", "cliff", "cloud", "clover", "coast", "cobalt", "cocoa", "comet", "coral", "cotton", "couch", "crane", "crayon",
    "cream", "crisp", "crown", "crystal", "cube", "daisy", "dance", "delta", "denim", "desert", "diary", "dinner", "disco", "dolphin", "donut", "dragon", "dream", "drift",
    "drum", "eagle", "earth", "echo", "elbow", "ember", "engine", "epic", "fabric", "falcon", "fancy", "feather", "fence", "fern", "fiber", "field", "flame", "flash",
    "fleet", "flute", "focus", "forest", "fossil", "frame", "frost", "fruit", "galaxy", "garden", "garlic", "gecko", "giant", "ginger", "glade", "globe", "glove", "goose",
    "grain", "grape", "gravel", "guitar", "habit", "hammer", "harbor", "harvest", "hazel", "honey", "hotel", "humble", "igloo", "island", "ivory", "jacket", "jaguar", "jelly",
    "jewel", "joke", "jungle", "kayak", "kettle", "kiwi", "koala", "label", "ladder", "lagoon", "lamp", "laser", "lemon", "level", "light", "lilac", "linen", "lion",
    "llama", "lobster", "lotus", "lucky", "lunar", "magnet", "mango", "maple", "marble", "meadow", "melon", "mint", "mirror", "mocha", "model", "monkey", "moose", "mosaic",
    "motor", "muffin", "music", "nectar", "needle", "night", "noble", "noodle", "north", "nutmeg", "oasis", "ocean", "olive", "onion", "opal", "orbit", "orchid", "otter",
    "owl", "oyster", "paddle", "palace", "panda", "paper", "parade", "parrot", "pasta", "peach", "pearl", "pebble", "pencil", "pepper", "piano", "pickle", "pilot", "pine",
    "pixel", "pizza", "planet", "plaza", "plum", "polar", "poppy", "prism", "pulse", "puzzle", "quartz", "quest", "quiet", "quilt", "rabbit", "radar", "radio", "rain",
    "raven", "ribbon", "river", "robin", "rocket", "rose", "ruby", "saddle", "safari", "salmon", "sand", "satin", "scarf", "shadow", "shell", "silver", "sketch", "sky",
    "sloth", "smile", "snow", "solar", "sonic", "spark", "spice", "spider", "spring", "square", "squid", "stamp", "star", "stone", "storm", "sugar", "summer", "sunny",
    "swan", "syrup", "table", "taco", "tango", "tea", "tempo", "tiger", "timber", "toast", "token", "topaz", "torch", "tower", "trail", "tulip", "tundra", "turtle",
    "twist", "umbra", "union", "unity", "urban", "valley", "vapor", "velvet", "venus", "violet", "vivid", "voyage", "waffle", "walnut", "water", "wave", "whale", "wheat",
    "willow", "window", "winter", "wizard", "wolf", "wonder", "yacht", "yarn", "yellow", "yogurt", "zebra", "zenith", "zephyr", "zinc",
];

/// Nombre aléatoire uniforme dans 0..n (tirage avec rejet, aucun biais).
fn below(n: u32) -> u32 {
    let lim = u32::MAX - u32::MAX % n;
    loop {
        let b = crate::vault::Vault::random(4);
        let x = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        if x < lim {
            return x % n;
        }
    }
}

fn pick<T: Copy>(s: &[T]) -> T {
    s[below(s.len() as u32) as usize]
}

/// Mot de passe aléatoire. `phrase` : des mots séparés par « - » (longueur ≈ nombre de caractères / 4).
pub fn generate(len: usize, sym: bool, digits: bool, phrase: bool) -> String {
    let len = len.clamp(8, 64);
    if phrase {
        let n = (len / 4).clamp(4, 10);
        let mut w: Vec<String> = (0..n)
            .map(|_| {
                let x = pick(WORDS);
                x[..1].to_uppercase() + &x[1..]
            })
            .collect();
        if digits {
            let i = below(n as u32) as usize;
            w[i].push(char::from(b'0' + below(10) as u8));
        }
        if sym {
            let i = below(n as u32) as usize;
            w[i].push(pick(&['!', '?', '#', '%', '&', '*', '+', '=']));
        }
        return w.join("-");
    }
    // pas de caractères ambigus (0/O, 1/l/I)
    const LOWER: &[u8] = b"abcdefghijkmnopqrstuvwxyz";
    const UPPER: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
    const DIG: &[u8] = b"23456789";
    const SYM: &[u8] = b"!#$%&*+-=?@^_~";
    let mut sets: Vec<&[u8]> = vec![LOWER, UPPER];
    if digits {
        sets.push(DIG);
    }
    if sym {
        sets.push(SYM);
    }
    let all: Vec<u8> = sets.concat();
    loop {
        let p: Vec<u8> = (0..len).map(|_| pick(&all)).collect();
        // au moins un caractère de chaque famille demandée
        if sets.iter().all(|s| p.iter().any(|c| s.contains(c))) {
            return String::from_utf8(p).unwrap_or_default();
        }
    }
}

/// Solidité 0..=4 (même barème que l'interface).
pub fn strength(p: &str) -> u8 {
    let mut s = 0;
    let n = p.chars().count();
    s += (n >= 8) as u8 + (n >= 12) as u8 + (n >= 16) as u8;
    s += (p.chars().any(|c| c.is_lowercase()) && p.chars().any(|c| c.is_uppercase())) as u8;
    s += p.chars().any(|c| c.is_ascii_digit()) as u8;
    s += p.chars().any(|c| !c.is_alphanumeric()) as u8;
    (s * 4 / 6).min(4)
}

/// Audit du coffre : indices des mots de passe faibles, et groupes d'indices qui partagent un même mot de passe.
pub fn audit(items: &[Value]) -> Value {
    let pw = |i: &Value| i["p"].as_str().unwrap_or("").to_string();
    let weak: Vec<usize> = items.iter().enumerate().filter(|(_, x)| strength(&pw(x)) < 3).map(|(i, _)| i).collect();
    let mut groups: Vec<Vec<usize>> = vec![];
    let mut seen: Vec<(String, usize)> = vec![];
    for (i, x) in items.iter().enumerate() {
        let p = pw(x);
        if p.is_empty() {
            continue;
        }
        match seen.iter().find(|(q, _)| *q == p).map(|(_, g)| *g) {
            Some(g) => groups[g].push(i),
            None => {
                seen.push((p, groups.len()));
                groups.push(vec![i]);
            }
        }
    }
    groups.retain(|g| g.len() > 1);
    json!({"weak": weak, "reused": groups, "n": items.len()})
}

/// SHA-1 en majuscules découpé pour le k-anonymat : (5 premiers caractères envoyés, 35 restants gardés).
pub fn hibp_split(pw: &str) -> (String, String) {
    let h: String = Sha1::digest(pw.as_bytes()).iter().map(|b| format!("{b:02X}")).collect();
    (h[..5].to_string(), h[5..].to_string())
}

/// Combien de fois ce suffixe apparaît dans la réponse de l'API (« SUFFIXE:COMPTE » par ligne).
pub fn hibp_count(body: &str, suffix: &str) -> u64 {
    body.lines()
        .filter_map(|l| l.trim().split_once(':'))
        .find(|(s, _)| s.eq_ignore_ascii_case(suffix))
        .and_then(|(_, n)| n.trim().parse().ok())
        .unwrap_or(0)
}

/// Appel réseau (à faire hors du fil de l'interface) : seul le préfixe de 5 caractères est envoyé.
pub fn hibp_check(pw: &str) -> Result<u64, String> {
    let (pre, suf) = hibp_split(pw);
    let body = crate::update::http_get(&format!("https://api.pwnedpasswords.com/range/{pre}"))?;
    Ok(hibp_count(&String::from_utf8_lossy(&body), &suf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_respects_options() {
        for _ in 0..50 {
            let p = generate(20, true, true, false);
            assert_eq!(p.chars().count(), 20);
            assert!(p.chars().any(|c| c.is_ascii_digit()) && p.chars().any(|c| !c.is_alphanumeric()));
            assert!(!p.contains(['0', 'O', 'l', '1', 'I']));
            let q = generate(12, false, false, false);
            assert!(q.chars().all(|c| c.is_ascii_alphabetic()));
        }
        let ph = generate(24, true, true, true);
        assert_eq!(ph.split('-').count(), 6);
        assert!(ph.chars().any(|c| c.is_ascii_digit()));
        assert_ne!(generate(20, true, true, false), generate(20, true, true, false));
        assert!(strength(&generate(20, true, true, false)) >= 3);
    }

    #[test]
    fn audit_finds_weak_and_reused() {
        let items = vec![json!({"o": "a.com", "p": "azerty"}), json!({"o": "b.com", "p": "Tr0ub4dor&3xYz!"}), json!({"o": "c.com", "p": "Tr0ub4dor&3xYz!"})];
        let a = audit(&items);
        assert_eq!(a["weak"], json!([0]));
        assert_eq!(a["reused"], json!([[1, 2]]));
    }

    #[test]
    fn hibp_k_anonymity() {
        // SHA-1("password") = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8
        let (pre, suf) = hibp_split("password");
        assert_eq!(pre, "5BAA6");
        assert_eq!(suf, "1E4C9B93F3F0682250B6CF8331B7EE68FD8");
        let body = "1D2DA4053E34E76F6576ED1DA63134B5E2A:2\r\n1E4C9B93F3F0682250B6CF8331B7EE68FD8:9545824\r\n";
        assert_eq!(hibp_count(body, &suf), 9545824);
        assert_eq!(hibp_count(body, "0000000000000000000000000000000000A"), 0);
    }
}
