//! The password generator (random characters or a passphrase of words) and the strength meter.

use crate::vault::random_below;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GenOptions {
    pub passphrase: bool,
    pub length: u32,
    pub upper: bool,
    pub lower: bool,
    pub digits: bool,
    pub symbols: bool,
    /// Leave out characters that look alike (0 O o, 1 l I |).
    pub no_lookalikes: bool,
    pub words: u32,
    pub separator: String,
    pub capitalize: bool,
    /// A digit after one of the words.
    pub number: bool,
}

impl Default for GenOptions {
    fn default() -> Self {
        GenOptions {
            passphrase: false,
            length: 20,
            upper: true,
            lower: true,
            digits: true,
            symbols: true,
            no_lookalikes: false,
            words: 5,
            separator: "-".into(),
            capitalize: true,
            number: true,
        }
    }
}

const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.?/~";
const LOOKALIKES: &str = "0Oo1lI|";

pub const MIN_LEN: u32 = 6;
pub const MAX_LEN: u32 = 64;
pub const MIN_WORDS: u32 = 3;
pub const MAX_WORDS: u32 = 10;

fn pick(set: &[char]) -> char {
    set[random_below(set.len() as u32) as usize]
}

pub fn generate(o: &GenOptions) -> String {
    if o.passphrase { passphrase(o) } else { password(o) }
}

fn password(o: &GenOptions) -> String {
    let keep = |s: &str| -> Vec<char> { s.chars().filter(|c| !(o.no_lookalikes && LOOKALIKES.contains(*c))).collect() };
    let mut sets: Vec<Vec<char>> = Vec::new();
    for (on, s) in [(o.upper, UPPER), (o.lower, LOWER), (o.digits, DIGITS), (o.symbols, SYMBOLS)] {
        if on {
            sets.push(keep(s));
        }
    }
    if sets.is_empty() {
        sets.push(keep(LOWER));
    }
    let len = o.length.clamp(MIN_LEN, MAX_LEN) as usize;
    let all: Vec<char> = sets.concat();
    // one of each chosen kind, the rest from all of them, then shuffled
    let mut out: Vec<char> = sets.iter().map(|s| pick(s)).collect();
    while out.len() < len {
        out.push(pick(&all));
    }
    for i in (1..out.len()).rev() {
        let j = random_below(i as u32 + 1) as usize;
        out.swap(i, j);
    }
    out.into_iter().collect()
}

fn passphrase(o: &GenOptions) -> String {
    let n = o.words.clamp(MIN_WORDS, MAX_WORDS) as usize;
    let mut words: Vec<String> = (0..n)
        .map(|_| {
            let w = WORDS[random_below(WORDS.len() as u32) as usize];
            if o.capitalize {
                let mut c = w.chars();
                c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
            } else {
                w.to_string()
            }
        })
        .collect();
    if o.number {
        let i = random_below(n as u32) as usize;
        words[i].push(char::from(b'0' + random_below(10) as u8));
    }
    words.join(&o.separator)
}

/// Bits of randomness of what the generator makes with these options (shown next to the result).
pub fn generator_bits(o: &GenOptions) -> f64 {
    if o.passphrase {
        let n = o.words.clamp(MIN_WORDS, MAX_WORDS) as f64;
        n * (WORDS.len() as f64).log2() + if o.number { (10.0 * n).log2() } else { 0.0 }
    } else {
        let mut pool = 0usize;
        for (on, s) in [(o.upper, UPPER), (o.lower, LOWER), (o.digits, DIGITS), (o.symbols, SYMBOLS)] {
            if on {
                pool += s.chars().filter(|c| !(o.no_lookalikes && LOOKALIKES.contains(*c))).count();
            }
        }
        if pool == 0 {
            pool = 26;
        }
        o.length.clamp(MIN_LEN, MAX_LEN) as f64 * (pool as f64).log2()
    }
}

/// 0 very weak, 1 weak, 2 fair, 3 strong, 4 very strong.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Strength(pub u8);

impl Strength {
    pub fn weak(self) -> bool {
        self.0 <= 1
    }
}

/// Passwords seen everywhere: a guesser tries them first.
const COMMON: &[&str] = &[
    "password",
    "passw0rd",
    "123456",
    "12345678",
    "123456789",
    "1234567890",
    "qwerty",
    "azerty",
    "abc123",
    "111111",
    "000000",
    "letmein",
    "welcome",
    "monkey",
    "dragon",
    "football",
    "baseball",
    "iloveyou",
    "admin",
    "login",
    "master",
    "sunshine",
    "princess",
    "starwars",
    "whatever",
    "trustno1",
    "shadow",
    "superman",
    "michael",
    "soleil",
    "bonjour",
    "motdepasse",
    "secret",
    "qwertyuiop",
    "azertyuiop",
    "asdfgh",
    "zxcvbn",
    "654321",
    "121212",
];

/// Rows of keyboards and runs a guesser walks along.
const RUNS: &[&str] =
    &["abcdefghijklmnopqrstuvwxyz", "0123456789", "qwertyuiop", "asdfghjkl", "zxcvbnm", "azertyuiop", "qsdfghjklm", "wxcvbn", "1234567890", "9876543210"];

/// An estimate of how hard a password is to guess: the size of the alphabet it uses, minus what a guesser would
/// try early (common passwords, repeats, keyboard runs, dictionary words, years).
pub fn strength(pw: &str) -> (Strength, f64) {
    let bits = bits(pw);
    let s = match bits {
        b if b < 28.0 => 0,
        b if b < 40.0 => 1,
        b if b < 60.0 => 2,
        b if b < 80.0 => 3,
        _ => 4,
    };
    (Strength(s), bits)
}

fn bits(pw: &str) -> f64 {
    if pw.is_empty() {
        return 0.0;
    }
    let lower = pw.to_lowercase();
    let base: String = lower
        .chars()
        .map(|c| match c {
            '0' => 'o',
            '1' | '!' => 'i',
            '3' => 'e',
            '4' | '@' => 'a',
            '5' | '$' => 's',
            '7' => 't',
            c => c,
        })
        .collect();
    // with and without the digits and signs added at the end ("password1", "dragon2024!")
    let strip = |s: &str| s.trim_end_matches(|c: char| c.is_ascii_digit() || c.is_ascii_punctuation()).to_string();
    let forms = [lower.clone(), base.clone(), strip(&lower), strip(&base)];
    if COMMON.iter().any(|c| forms.iter().any(|f| f == c)) {
        return 5.0;
    }
    // a passphrase from the generator's own word list: counted as words, not letters
    let parts: Vec<&str> = lower.split(|c: char| !c.is_ascii_alphabetic()).filter(|p| !p.is_empty()).collect();
    if parts.len() >= 3 && parts.iter().all(|p| WORDS.contains(p)) {
        return parts.len() as f64 * (WORDS.len() as f64).log2();
    }
    let chars: Vec<char> = pw.chars().collect();
    let mut pool = 0u32;
    if chars.iter().any(|c| c.is_ascii_lowercase()) {
        pool += 26;
    }
    if chars.iter().any(|c| c.is_ascii_uppercase()) {
        pool += 26;
    }
    if chars.iter().any(|c| c.is_ascii_digit()) {
        pool += 10;
    }
    if chars.iter().any(|c| c.is_ascii_punctuation() || *c == ' ') {
        pool += 33;
    }
    if chars.iter().any(|c| !c.is_ascii()) {
        pool += 100;
    }
    let per = (pool.max(10) as f64).log2();
    // count the characters a guesser would not have to guess one by one
    let mut effective = 0.0;
    let mut i = 0;
    let lc: Vec<char> = lower.chars().collect();
    while i < lc.len() {
        // the same character again and again
        let mut j = i + 1;
        while j < lc.len() && lc[j] == lc[i] {
            j += 1;
        }
        if j - i >= 3 {
            effective += 1.0 + ((j - i) as f64).log2() / per;
            i = j;
            continue;
        }
        // a run along the alphabet, the digits or a keyboard row (either way)
        let mut best = 0;
        for run in RUNS {
            for r in [run.to_string(), run.chars().rev().collect::<String>()] {
                let rc: Vec<char> = r.chars().collect();
                if let Some(start) = rc.iter().position(|c| *c == lc[i]) {
                    let mut k = 0;
                    while i + k < lc.len() && start + k < rc.len() && lc[i + k] == rc[start + k] {
                        k += 1;
                    }
                    best = best.max(k);
                }
            }
        }
        if best >= 3 {
            effective += 1.5;
            i += best;
            continue;
        }
        // a dictionary word (4 letters or more)
        let mut word = 0;
        for w in WORDS.iter().filter(|w| w.len() >= 4) {
            if lc[i..].iter().take(w.len()).collect::<String>() == **w {
                word = word.max(w.len());
            }
        }
        if word > 0 {
            effective += 2.0;
            i += word;
            continue;
        }
        // a year
        if i + 4 <= lc.len() {
            let s: String = lc[i..i + 4].iter().collect();
            if let Ok(y) = s.parse::<u32>()
                && (1900..=2099).contains(&y)
            {
                effective += 1.5;
                i += 4;
                continue;
            }
        }
        effective += 1.0;
        i += 1;
    }
    effective * per
}

/// Words for passphrases: short, common, easy to type and to tell apart.
pub const WORDS: &[&str] = &[
    "able",
    "acid",
    "acorn",
    "actor",
    "adapt",
    "aisle",
    "alarm",
    "album",
    "alley",
    "alpine",
    "amber",
    "angle",
    "ankle",
    "apple",
    "apron",
    "arena",
    "armor",
    "arrow",
    "atlas",
    "attic",
    "audio",
    "autumn",
    "avenue",
    "badge",
    "bagel",
    "baker",
    "bamboo",
    "banjo",
    "barn",
    "basil",
    "basin",
    "beach",
    "beacon",
    "beard",
    "beaver",
    "bench",
    "berry",
    "bicycle",
    "binder",
    "birch",
    "bison",
    "blade",
    "blank",
    "blaze",
    "blend",
    "bloom",
    "board",
    "bonus",
    "boot",
    "border",
    "bottle",
    "boulder",
    "bounce",
    "brave",
    "bread",
    "breeze",
    "brick",
    "bridge",
    "brook",
    "brush",
    "bubble",
    "bucket",
    "buffalo",
    "bundle",
    "burrow",
    "butter",
    "button",
    "cabin",
    "cable",
    "cactus",
    "camel",
    "camera",
    "canal",
    "candle",
    "canoe",
    "canvas",
    "canyon",
    "carbon",
    "cargo",
    "carpet",
    "carrot",
    "castle",
    "cedar",
    "cellar",
    "cement",
    "chalk",
    "chapel",
    "cherry",
    "chess",
    "chimney",
    "cider",
    "cinema",
    "circle",
    "citrus",
    "clay",
    "cliff",
    "climb",
    "clock",
    "cloud",
    "clover",
    "coast",
    "cobalt",
    "cocoa",
    "coffee",
    "comet",
    "copper",
    "coral",
    "corner",
    "cotton",
    "couch",
    "cougar",
    "crane",
    "crayon",
    "creek",
    "cricket",
    "crisp",
    "crown",
    "crystal",
    "cube",
    "curtain",
    "cushion",
    "cycle",
    "daisy",
    "dance",
    "dawn",
    "delta",
    "denim",
    "desert",
    "diary",
    "dinner",
    "dock",
    "dolphin",
    "donkey",
    "dragon",
    "drift",
    "drum",
    "dune",
    "eagle",
    "early",
    "earth",
    "easel",
    "echo",
    "eclipse",
    "elbow",
    "elder",
    "elegant",
    "elk",
    "ember",
    "empire",
    "engine",
    "envoy",
    "epic",
    "equal",
    "ethic",
    "falcon",
    "fancy",
    "farm",
    "feather",
    "fence",
    "fern",
    "ferry",
    "fiber",
    "fiddle",
    "field",
    "fig",
    "film",
    "finch",
    "fjord",
    "flame",
    "flannel",
    "flask",
    "fleet",
    "flint",
    "float",
    "flute",
    "foam",
    "focus",
    "forest",
    "fork",
    "fossil",
    "fountain",
    "fox",
    "frame",
    "fresh",
    "frost",
    "fruit",
    "gadget",
    "galaxy",
    "garden",
    "garlic",
    "gate",
    "gazelle",
    "gecko",
    "gem",
    "giant",
    "ginger",
    "glacier",
    "glass",
    "glide",
    "globe",
    "glove",
    "goat",
    "gold",
    "gondola",
    "goose",
    "gorge",
    "grain",
    "granite",
    "grape",
    "gravel",
    "green",
    "grove",
    "guitar",
    "gull",
    "hammer",
    "harbor",
    "harp",
    "harvest",
    "hazel",
    "heart",
    "hedge",
    "helmet",
    "herb",
    "heron",
    "hiking",
    "hill",
    "hollow",
    "honey",
    "horizon",
    "hornet",
    "hotel",
    "humble",
    "husky",
    "igloo",
    "index",
    "indigo",
    "inlet",
    "iris",
    "island",
    "ivory",
    "jacket",
    "jaguar",
    "jasmine",
    "jelly",
    "jewel",
    "jigsaw",
    "jolly",
    "journal",
    "jungle",
    "juniper",
    "kayak",
    "kettle",
    "kiwi",
    "koala",
    "label",
    "ladder",
    "lagoon",
    "lake",
    "lamp",
    "lantern",
    "laptop",
    "lark",
    "latch",
    "lava",
    "lemon",
    "lens",
    "lily",
    "linen",
    "lion",
    "lizard",
    "llama",
    "lobster",
    "locket",
    "lodge",
    "lotus",
    "lucky",
    "lumber",
    "lunar",
    "lynx",
    "magnet",
    "mango",
    "manor",
    "maple",
    "marble",
    "market",
    "marsh",
    "meadow",
    "melody",
    "melon",
    "mesa",
    "metal",
    "meteor",
    "mirror",
    "mitten",
    "monsoon",
    "moose",
    "mosaic",
    "moss",
    "motor",
    "mount",
    "muffin",
    "museum",
    "mustard",
    "napkin",
    "native",
    "nectar",
    "needle",
    "nest",
    "nickel",
    "noble",
    "noodle",
    "north",
    "nugget",
    "nutmeg",
    "oak",
    "oasis",
    "ocean",
    "octave",
    "olive",
    "omelet",
    "onion",
    "opal",
    "orbit",
    "orchard",
    "orchid",
    "otter",
    "outdoor",
    "owl",
    "oxygen",
    "oyster",
    "paddle",
    "palace",
    "panda",
    "panther",
    "paper",
    "parade",
    "parrot",
    "pasta",
    "pastel",
    "patio",
    "peach",
    "peanut",
    "pearl",
    "pebble",
    "pecan",
    "pelican",
    "pencil",
    "pepper",
    "piano",
    "pickle",
    "pigeon",
    "pillow",
    "pilot",
    "pine",
    "pirate",
    "pistachio",
    "pixel",
    "planet",
    "plaza",
    "plum",
    "pocket",
    "poem",
    "polar",
    "pond",
    "poplar",
    "poppy",
    "potato",
    "prairie",
    "prism",
    "pulse",
    "pumpkin",
    "puzzle",
    "quail",
    "quartz",
    "quest",
    "quiet",
    "quilt",
    "rabbit",
    "raccoon",
    "radar",
    "radio",
    "rain",
    "raisin",
    "ranch",
    "raven",
    "reef",
    "relay",
    "ribbon",
    "ridge",
    "river",
    "robin",
    "rocket",
    "rodeo",
    "rose",
    "rover",
    "ruby",
    "saddle",
    "saffron",
    "sage",
    "salmon",
    "salt",
    "sand",
    "satin",
    "saturn",
    "savanna",
    "scarf",
    "scout",
    "season",
    "seed",
    "shadow",
    "shell",
    "shelter",
    "shield",
    "shore",
    "signal",
    "silk",
    "silver",
    "sketch",
    "sled",
    "slope",
    "smooth",
    "snow",
    "socket",
    "solar",
    "sonnet",
    "spark",
    "sparrow",
    "spice",
    "spider",
    "spiral",
    "spoon",
    "spring",
    "spruce",
    "square",
    "squid",
    "stable",
    "stamp",
    "starling",
    "statue",
    "steam",
    "stone",
    "storm",
    "stream",
    "studio",
    "sugar",
    "summit",
    "sunny",
    "swan",
    "sweater",
    "syrup",
    "table",
    "tango",
    "teapot",
    "temple",
    "tender",
    "thistle",
    "thunder",
    "ticket",
    "tiger",
    "timber",
    "toast",
    "tomato",
    "topaz",
    "torch",
    "tower",
    "trail",
    "tram",
    "tree",
    "trout",
    "tulip",
    "tundra",
    "tunnel",
    "turtle",
    "tusk",
    "twig",
    "umbrella",
    "unicorn",
    "union",
    "valley",
    "vanilla",
    "velvet",
    "violet",
    "violin",
    "vivid",
    "voyage",
    "waffle",
    "wagon",
    "walnut",
    "walrus",
    "water",
    "wave",
    "whale",
    "wheat",
    "whisper",
    "willow",
    "window",
    "winter",
    "wizard",
    "wolf",
    "wombat",
    "wonder",
    "wool",
    "yacht",
    "yarn",
    "yellow",
    "yogurt",
    "zebra",
    "zenith",
    "zephyr",
    "zinc",
    "zipper",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_list() {
        let mut seen = std::collections::HashSet::new();
        for w in WORDS {
            assert!(w.chars().all(|c| c.is_ascii_lowercase()), "{w}");
            assert!((3..=9).contains(&w.len()), "{w}");
            assert!(seen.insert(*w), "twice: {w}");
        }
        assert!(WORDS.len() >= 500, "{} words", WORDS.len());
    }

    #[test]
    fn passwords_follow_the_options() {
        let mut o = GenOptions::default();
        for len in [6, 12, 20, 64] {
            o.length = len;
            let p = generate(&o);
            assert_eq!(p.chars().count(), len as usize);
            assert!(p.chars().any(|c| c.is_ascii_uppercase()));
            assert!(p.chars().any(|c| c.is_ascii_lowercase()));
            assert!(p.chars().any(|c| c.is_ascii_digit()));
            assert!(p.chars().any(|c| SYMBOLS.contains(c)));
        }
        o.length = 200;
        assert_eq!(generate(&o).len(), MAX_LEN as usize);
        o = GenOptions { symbols: false, upper: false, no_lookalikes: true, length: 40, ..GenOptions::default() };
        for _ in 0..50 {
            let p = generate(&o);
            assert!(p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
            assert!(!p.chars().any(|c| LOOKALIKES.contains(c)));
        }
        // nothing chosen: still a password
        o = GenOptions { upper: false, lower: false, digits: false, symbols: false, ..GenOptions::default() };
        assert_eq!(generate(&o).len(), 20);
        // two in a row are different
        let o = GenOptions::default();
        assert_ne!(generate(&o), generate(&o));
    }

    #[test]
    fn passphrases() {
        let o = GenOptions { passphrase: true, words: 6, separator: ".".into(), capitalize: true, number: true, ..GenOptions::default() };
        let p = generate(&o);
        let parts: Vec<&str> = p.split('.').collect();
        assert_eq!(parts.len(), 6);
        assert_eq!(p.chars().filter(|c| c.is_ascii_digit()).count(), 1);
        for w in parts {
            assert!(w.chars().next().unwrap().is_ascii_uppercase());
            let bare: String = w.chars().filter(|c| c.is_ascii_alphabetic()).collect::<String>().to_lowercase();
            assert!(WORDS.contains(&bare.as_str()));
        }
        assert!(generator_bits(&o) > 50.0);
        assert!(generator_bits(&GenOptions::default()) > 120.0);
    }

    #[test]
    fn strength_meter() {
        let s = |p: &str| strength(p).0.0;
        for weak in ["", "password", "password1", "Password123!", "P@ssw0rd", "123456", "azerty", "aaaaaaaa", "abcdefgh", "qwerty123", "dragon2024"] {
            assert!(s(weak) <= 1, "{weak} scored {}", s(weak));
        }
        assert!(s("Tr0ub4dor&3") >= 2);
        assert!(s("correct-horse-battery-staple") >= 2);
        assert!(s("q8#Lz!v2Rk@9Wm$pX4") >= 4);
        let g = generate(&GenOptions::default());
        assert!(s(&g) >= 4);
        let pp = generate(&GenOptions { passphrase: true, ..GenOptions::default() });
        assert!(s(&pp) >= 2, "passphrase scored {}", s(&pp));
        assert!(Strength(1).weak() && !Strength(2).weak());
    }
}
