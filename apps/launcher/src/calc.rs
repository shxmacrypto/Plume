//! The calculator: expressions (+ − × ÷ ^ %, parentheses, functions, constants), percentages ("20% of 150",
//! "80 + 15%") and unit conversions ("5 km in mi", "70 f to c", "1.5 GB in MB").
//! Pure logic, no window: tested below.

/// What the box shows for a calculation: the value to copy, and the line under it.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub value: String,
    pub detail: String,
}

/// Tries the text as a calculation. None: it is not one (a plain word, a lone number, a syntax error).
pub fn eval(input: &str) -> Option<Answer> {
    let s = input.trim();
    let forced = s.starts_with('=');
    let s = s.trim_start_matches('=').trim();
    if s.is_empty() {
        return None;
    }
    if let Some(a) = convert(s) {
        return Some(a);
    }
    let toks = lex(s)?;
    // a lone number is not a calculation (unless "=" asks for one)
    if !forced && !toks.iter().any(|t| !matches!(t, Tok::Num(_))) {
        return None;
    }
    // a single word ("pi", "e") only with "="
    if !forced && toks.len() == 1 {
        return None;
    }
    let mut p = Parser { toks: &toks, at: 0 };
    let v = p.expr()?;
    if p.at != toks.len() || !v.is_finite() {
        return None;
    }
    Some(Answer { value: format_num(v), detail: format!("{s} =") })
}

// ───────────────────────── numbers ─────────────────────────

/// A number as people write it: up to 12 significant digits, no trailing zeros, exponent when very large or small.
pub fn format_num(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    if !(1e-6..1e15).contains(&a) {
        let s = format!("{v:.6e}");
        // 1.500000e3 → 1.5e3
        if let Some((m, e)) = s.split_once('e') {
            let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
            return format!("{m}e{e}");
        }
        return s;
    }
    let digits = (12 - a.log10().floor() as i32 - 1).clamp(0, 12) as usize;
    let s = format!("{v:.digits$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if s == "-0" { "0".into() } else { s }
}

// ───────────────────────── tokens ─────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Word(String),
    Op(char),
}

fn lex(s: &str) -> Option<Vec<Tok>> {
    let cs: Vec<char> = s.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() || c == '\u{a0}' || c == '\u{202f}' {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && cs.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let mut t = String::new();
            while i < cs.len() {
                let d = cs[i];
                if d.is_ascii_digit() {
                    t.push(d);
                } else if (d == '.' || d == ',') && cs.get(i + 1).is_some_and(|n| n.is_ascii_digit()) && !t.contains('.') {
                    // the comma is the decimal point in French
                    t.push('.');
                } else if (d == 'e' || d == 'E')
                    && (cs.get(i + 1).is_some_and(|n| n.is_ascii_digit())
                        || (matches!(cs.get(i + 1), Some('-' | '+')) && cs.get(i + 2).is_some_and(|n| n.is_ascii_digit())))
                    && !t.contains('e')
                {
                    t.push('e');
                    if matches!(cs[i + 1], '-' | '+') {
                        t.push(cs[i + 1]);
                        i += 1;
                    }
                } else {
                    break;
                }
                i += 1;
            }
            out.push(Tok::Num(t.parse().ok()?));
            continue;
        }
        // "2 x 3": an x alone between two numbers is a product
        if matches!(c, 'x' | 'X') && matches!(out.last(), Some(Tok::Num(_)) | Some(Tok::Op(')'))) && !cs.get(i + 1).is_some_and(|n| n.is_alphabetic()) {
            out.push(Tok::Op('*'));
            i += 1;
            continue;
        }
        if c.is_alphabetic() || c == 'π' || c == '°' || c == '√' {
            if c == '√' {
                out.push(Tok::Word("sqrt".into()));
                i += 1;
                continue;
            }
            let mut t = String::new();
            while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == 'π' || cs[i] == '°') {
                t.extend(cs[i].to_lowercase());
                i += 1;
            }
            out.push(Tok::Word(t));
            continue;
        }
        let op = match c {
            '+' | '-' | '*' | '/' | '^' | '%' | '(' | ')' | '!' => c,
            '×' => '*',
            '÷' => '/',
            '−' | '–' => '-',
            '²' => {
                out.push(Tok::Op('^'));
                out.push(Tok::Num(2.0));
                i += 1;
                continue;
            }
            '³' => {
                out.push(Tok::Op('^'));
                out.push(Tok::Num(3.0));
                i += 1;
                continue;
            }
            _ => return None,
        };
        out.push(Tok::Op(op));
        i += 1;
    }
    Some(out)
}

// ───────────────────────── expressions ─────────────────────────

struct Parser<'a> {
    toks: &'a [Tok],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }
    fn eat_op(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::Op(c)) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn eat_word(&mut self, words: &[&str]) -> bool {
        if let Some(Tok::Word(w)) = self.peek()
            && words.contains(&w.as_str())
        {
            self.at += 1;
            return true;
        }
        false
    }

    /// Is a percentage next, with nothing after it but the end, a closing parenthesis or + −?
    fn percent_term(&mut self) -> Option<Option<f64>> {
        let save = self.at;
        let Some(v) = self.primary() else {
            self.at = save;
            return Some(None);
        };
        if self.eat_op('%') && matches!(self.peek(), None | Some(Tok::Op(')' | '+' | '-'))) {
            return Some(Some(v));
        }
        self.at = save;
        Some(None)
    }

    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        loop {
            let sign = if self.eat_op('+') {
                1.0
            } else if self.eat_op('-') {
                -1.0
            } else {
                return Some(v);
            };
            // "80 + 15%": 15 % of 80
            if let Some(p) = self.percent_term()? {
                v += sign * v * p / 100.0;
                continue;
            }
            v += sign * self.term()?;
        }
    }

    fn term(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        loop {
            if self.eat_op('*') {
                v *= self.unary()?;
            } else if self.eat_op('/') {
                let d = self.unary()?;
                if d == 0.0 {
                    return None;
                }
                v /= d;
            } else if self.eat_word(&["mod"]) {
                let d = self.unary()?;
                if d == 0.0 {
                    return None;
                }
                v = v.rem_euclid(d);
            } else if self.eat_word(&["of", "de", "des"]) {
                // "20% of 150": the percentage already made 20 into 0.2
                v *= self.unary()?;
            } else if matches!(self.peek(), Some(Tok::Op('(')) | Some(Tok::Word(_)))
                && !matches!(self.peek(), Some(Tok::Word(w)) if matches!(w.as_str(), "of" | "de" | "des" | "mod"))
            {
                // implicit product: 2(3+4), 2pi
                v *= self.unary()?;
            } else {
                return Some(v);
            }
        }
    }

    fn unary(&mut self) -> Option<f64> {
        if self.eat_op('-') {
            return Some(-self.unary()?);
        }
        if self.eat_op('+') {
            return self.unary();
        }
        self.power()
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.postfix()?;
        if self.eat_op('^') {
            let e = self.unary()?;
            return Some(base.powf(e));
        }
        Some(base)
    }

    fn postfix(&mut self) -> Option<f64> {
        let mut v = self.primary()?;
        loop {
            if self.peek() == Some(&Tok::Op('%')) {
                // "50 % 3" is not a modulo here: % is always a percentage
                self.at += 1;
                v /= 100.0;
            } else if self.eat_op('!') {
                if v < 0.0 || v.fract() != 0.0 || v > 170.0 {
                    return None;
                }
                v = (1..=v as u64).map(|k| k as f64).product();
            } else {
                return Some(v);
            }
        }
    }

    fn primary(&mut self) -> Option<f64> {
        match self.peek()?.clone() {
            Tok::Num(n) => {
                self.at += 1;
                Some(n)
            }
            Tok::Op('(') => {
                self.at += 1;
                let v = self.expr()?;
                // a missing closing parenthesis at the end is forgiven
                if !self.eat_op(')') && self.peek().is_some() {
                    return None;
                }
                Some(v)
            }
            Tok::Word(w) => {
                self.at += 1;
                match w.as_str() {
                    "pi" | "π" => return Some(std::f64::consts::PI),
                    "e" => return Some(std::f64::consts::E),
                    "tau" => return Some(std::f64::consts::TAU),
                    _ => {}
                }
                let f: fn(f64) -> f64 = match w.as_str() {
                    "sqrt" | "racine" => f64::sqrt,
                    "cbrt" => f64::cbrt,
                    "abs" => f64::abs,
                    "round" | "arrondi" => f64::round,
                    "floor" => f64::floor,
                    "ceil" => f64::ceil,
                    "ln" => f64::ln,
                    "log" => f64::log10,
                    "log2" => f64::log2,
                    "exp" => f64::exp,
                    "sin" => f64::sin,
                    "cos" => f64::cos,
                    "tan" => f64::tan,
                    "asin" => f64::asin,
                    "acos" => f64::acos,
                    "atan" => f64::atan,
                    _ => return None,
                };
                let arg = if self.peek() == Some(&Tok::Op('(')) { self.primary()? } else { self.power()? };
                let v = f(arg);
                v.is_finite().then_some(v)
            }
            _ => None,
        }
    }
}

// ───────────────────────── units ─────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dim {
    Length,
    Mass,
    Volume,
    Temp,
    Data,
    Time,
    Speed,
    Area,
}

/// (names, dimension, size in the base unit). Temperatures are handled apart.
const UNITS: &[(&[&str], Dim, f64)] = &[
    (&["mm", "millimeter", "millimeters", "millimetre", "millimetres", "millimètre", "millimètres"], Dim::Length, 0.001),
    (&["cm", "centimeter", "centimeters", "centimetre", "centimetres", "centimètre", "centimètres"], Dim::Length, 0.01),
    (&["m", "meter", "meters", "metre", "metres", "mètre", "mètres"], Dim::Length, 1.0),
    (&["km", "kilometer", "kilometers", "kilometre", "kilometres", "kilomètre", "kilomètres"], Dim::Length, 1000.0),
    (&["in", "inch", "inches", "pouce", "pouces", "\""], Dim::Length, 0.0254),
    (&["ft", "foot", "feet", "pied", "pieds", "'"], Dim::Length, 0.3048),
    (&["yd", "yard", "yards"], Dim::Length, 0.9144),
    (&["mi", "mile", "miles"], Dim::Length, 1609.344),
    (&["nmi", "nautical mile", "nautical miles"], Dim::Length, 1852.0),
    (&["mg", "milligram", "milligrams", "milligramme", "milligrammes"], Dim::Mass, 0.001),
    (&["g", "gram", "grams", "gramme", "grammes"], Dim::Mass, 1.0),
    (&["kg", "kilo", "kilos", "kilogram", "kilograms", "kilogramme", "kilogrammes"], Dim::Mass, 1000.0),
    (&["t", "tonne", "tonnes", "ton", "tons"], Dim::Mass, 1_000_000.0),
    (&["oz", "ounce", "ounces", "once", "onces"], Dim::Mass, 28.349_523_125),
    (&["lb", "lbs", "pound", "pounds", "livre", "livres"], Dim::Mass, 453.592_37),
    (&["st", "stone", "stones"], Dim::Mass, 6_350.293_18),
    (&["ml", "milliliter", "milliliters", "millilitre", "millilitres"], Dim::Volume, 0.001),
    (&["cl", "centiliter", "centiliters", "centilitre", "centilitres"], Dim::Volume, 0.01),
    (&["dl", "deciliter", "deciliters", "décilitre", "décilitres"], Dim::Volume, 0.1),
    (&["l", "liter", "liters", "litre", "litres"], Dim::Volume, 1.0),
    (&["m3", "m³", "cubic meter", "cubic meters"], Dim::Volume, 1000.0),
    (&["gal", "gallon", "gallons"], Dim::Volume, 3.785_411_784),
    (&["qt", "quart", "quarts"], Dim::Volume, 0.946_352_946),
    (&["pt", "pint", "pints", "pinte", "pintes"], Dim::Volume, 0.473_176_473),
    (&["cup", "cups", "tasse", "tasses"], Dim::Volume, 0.24),
    (&["floz", "fl oz", "fluid ounce", "fluid ounces"], Dim::Volume, 0.029_573_529_6),
    (&["tbsp", "tablespoon", "tablespoons"], Dim::Volume, 0.014_786_764_8),
    (&["tsp", "teaspoon", "teaspoons"], Dim::Volume, 0.004_928_921_59),
    (&["b", "byte", "bytes", "octet", "octets", "o"], Dim::Data, 1.0),
    (&["kb", "kilobyte", "kilobytes", "ko"], Dim::Data, 1e3),
    (&["mb", "megabyte", "megabytes", "mo"], Dim::Data, 1e6),
    (&["gb", "gigabyte", "gigabytes", "go"], Dim::Data, 1e9),
    (&["tb", "terabyte", "terabytes", "to"], Dim::Data, 1e12),
    (&["kib", "kio"], Dim::Data, 1024.0),
    (&["mib", "mio"], Dim::Data, 1_048_576.0),
    (&["gib", "gio"], Dim::Data, 1_073_741_824.0),
    (&["tib", "tio"], Dim::Data, 1_099_511_627_776.0),
    (&["ms", "millisecond", "milliseconds", "milliseconde", "millisecondes"], Dim::Time, 0.001),
    (&["s", "sec", "second", "seconds", "seconde", "secondes"], Dim::Time, 1.0),
    (&["min", "mins", "minute", "minutes"], Dim::Time, 60.0),
    (&["h", "hr", "hrs", "hour", "hours", "heure", "heures"], Dim::Time, 3600.0),
    (&["d", "day", "days", "jour", "jours", "j"], Dim::Time, 86_400.0),
    (&["wk", "week", "weeks", "semaine", "semaines"], Dim::Time, 604_800.0),
    (&["yr", "year", "years", "an", "ans", "année", "années"], Dim::Time, 31_557_600.0),
    (&["m/s", "mps"], Dim::Speed, 1.0),
    (&["km/h", "kmh", "kph"], Dim::Speed, 1.0 / 3.6),
    (&["mph", "mi/h"], Dim::Speed, 0.447_04),
    (&["kn", "knot", "knots", "noeud", "noeuds", "nœud", "nœuds"], Dim::Speed, 0.514_444),
    (&["cm2", "cm²"], Dim::Area, 0.0001),
    (&["m2", "m²", "square meter", "square meters"], Dim::Area, 1.0),
    (&["km2", "km²"], Dim::Area, 1e6),
    (&["ha", "hectare", "hectares"], Dim::Area, 10_000.0),
    (&["acre", "acres"], Dim::Area, 4_046.856_422_4),
    (&["ft2", "ft²", "sqft", "sq ft"], Dim::Area, 0.092_903_04),
];

/// The unit of a name; for temperatures, the base is the Kelvin and `Temp` units are recognized apart.
fn unit(name: &str) -> Option<(Dim, f64, &'static str)> {
    let n = name.trim().to_lowercase();
    let n = n.trim_end_matches('.');
    match n {
        "c" | "°c" | "celsius" | "degc" | "degrés" | "degres" => return Some((Dim::Temp, 0.0, "°C")),
        "f" | "°f" | "fahrenheit" | "degf" => return Some((Dim::Temp, 1.0, "°F")),
        "k" | "kelvin" | "kelvins" => return Some((Dim::Temp, 2.0, "K")),
        _ => {}
    }
    UNITS.iter().find(|(names, _, _)| names.contains(&n)).map(|(names, d, f)| (*d, *f, names[0]))
}

fn to_kelvin(v: f64, which: f64) -> f64 {
    match which as u8 {
        0 => v + 273.15,
        1 => (v - 32.0) * 5.0 / 9.0 + 273.15,
        _ => v,
    }
}

fn from_kelvin(k: f64, which: f64) -> f64 {
    match which as u8 {
        0 => k - 273.15,
        1 => (k - 273.15) * 9.0 / 5.0 + 32.0,
        _ => k,
    }
}

/// "5 km in mi", "70f to c", "3 ft 2 in cm" (not the last), "100 kmh en mph".
fn convert(s: &str) -> Option<Answer> {
    let low = s.to_lowercase();
    [" in ", " to ", " en ", " -> ", " → ", " = ", " into ", " vers "].iter().find_map(|sep| low.rsplit_once(sep).and_then(|(l, r)| convert_parts(l, r)))
}

/// Short name of a unit as it is shown ("MB", "KiB", "km").
fn pretty(name: &'static str, dim: Dim) -> String {
    if dim != Dim::Data {
        return name.to_string();
    }
    match name {
        "b" => "B".into(),
        n if n.ends_with("ib") => format!("{}iB", n[..1].to_uppercase()),
        n => n.to_uppercase(),
    }
}

fn convert_parts(left: &str, right: &str) -> Option<Answer> {
    let (to_dim, to_f, to_name) = unit(right)?;
    // the number (an expression, even) is followed by its unit
    let left = left.trim();
    let split = left.char_indices().rev().find(|(_, c)| c.is_ascii_digit() || *c == ')' || *c == '%').map(|(i, c)| i + c.len_utf8())?;
    let (num, from) = left.split_at(split);
    let (from_dim, from_f, from_name) = unit(from)?;
    if from_dim != to_dim {
        return None;
    }
    let v = {
        let toks = lex(num)?;
        let mut p = Parser { toks: &toks, at: 0 };
        let v = p.expr()?;
        (p.at == toks.len()).then_some(v)?
    };
    let out = if from_dim == Dim::Temp { from_kelvin(to_kelvin(v, from_f), to_f) } else { v * from_f / to_f };
    if !out.is_finite() {
        return None;
    }
    let value = format!("{} {}", format_num(out), pretty(to_name, to_dim));
    Some(Answer { value, detail: format!("{} {} =", format_num(v), pretty(from_name, from_dim)) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> String {
        eval(s).map(|a| a.value).unwrap_or_else(|| "none".into())
    }

    #[test]
    fn arithmetic() {
        assert_eq!(v("1+2*3"), "7");
        assert_eq!(v("(1+2)*3"), "9");
        assert_eq!(v("2^10"), "1024");
        assert_eq!(v("2^3^2"), "512");
        assert_eq!(v("-3^2"), "-9");
        assert_eq!(v("10/4"), "2.5");
        assert_eq!(v("7 mod 3"), "1");
        assert_eq!(v("5!"), "120");
        assert_eq!(v("3,5 * 2"), "7");
        assert_eq!(v("2(3+4)"), "14");
        assert_eq!(v("12 × 3 ÷ 4"), "9");
        assert_eq!(v("6 x 7"), "42");
        assert_eq!(v("0.1+0.2"), "0.3");
        assert_eq!(v("1e3 + 1"), "1001");
        assert_eq!(v("(2+3"), "5");
    }

    #[test]
    fn functions_and_constants() {
        assert_eq!(v("sqrt(16)"), "4");
        assert_eq!(v("√9"), "3");
        assert_eq!(v("2pi"), "6.28318530718");
        assert_eq!(v("=pi"), "3.14159265359");
        assert_eq!(v("log(1000)"), "3");
        assert_eq!(v("abs(-4)+1"), "5");
    }

    #[test]
    fn percentages() {
        assert_eq!(v("20% of 150"), "30");
        assert_eq!(v("20% de 150"), "30");
        assert_eq!(v("80 + 15%"), "92");
        assert_eq!(v("200 - 10%"), "180");
        assert_eq!(v("50 * 10%"), "5");
        assert_eq!(v("(100 + 20%) * 2"), "240");
    }

    #[test]
    fn not_calculations() {
        assert_eq!(v("chrome"), "none");
        assert_eq!(v("42"), "none");
        assert_eq!(v("pi"), "none");
        assert_eq!(v("1/0"), "none");
        assert_eq!(v("notes 2024"), "none");
        assert_eq!(v("3 +"), "none");
        assert_eq!(v(""), "none");
        assert_eq!(v("photo.jpg"), "none");
    }

    #[test]
    fn units() {
        assert_eq!(v("5 km in mi"), "3.10685596119 mi");
        assert_eq!(v("1 mile to km"), "1.609344 km");
        assert_eq!(v("100 c to f"), "212 °F");
        assert_eq!(v("70f in c"), "21.1111111111 °C");
        assert_eq!(v("0 k en c"), "-273.15 °C");
        assert_eq!(v("1.5 GB in MB"), "1500 MB");
        assert_eq!(v("2048 kib en mib"), "2 MiB");
        assert_eq!(v("5 in to cm"), "12.7 cm");
        assert_eq!(v("1 gib to mb"), "1073.741824 MB");
        assert_eq!(v("2 h in min"), "120 min");
        assert_eq!(v("10 kg to lb"), "22.0462262185 lb");
        assert_eq!(v("100 kmh in mph"), "62.1371192237 mph");
        assert_eq!(v("2*3 m in cm"), "600 cm");
        assert_eq!(v("5 km in kg"), "none");
        assert_eq!(v("3 litres en ml"), "3000 ml");
    }

    #[test]
    fn number_format() {
        assert_eq!(format_num(1.0 / 3.0), "0.333333333333");
        assert_eq!(format_num(1e20), "1e20");
        assert_eq!(format_num(-0.0), "0");
        assert_eq!(format_num(1234567.0), "1234567");
        assert_eq!(format_num(2.5e-9), "2.5e-9");
    }
}
