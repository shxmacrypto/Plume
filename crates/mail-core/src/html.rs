//! Un message en HTML rendu en texte à lire : paragraphes, listes, liens, citations. Rien n'est exécuté, rien
//! n'est téléchargé : les scripts et les styles sont ignorés, une image n'est que son texte de remplacement.

/// Les balises qui commencent une nouvelle ligne.
#[rustfmt::skip]
const BLOCKS: [&str; 26] = [
    "p", "div", "br", "tr", "table", "ul", "ol", "li", "h1", "h2", "h3", "h4", "h5", "h6", "blockquote", "pre", "hr",
    "section", "article", "header", "footer", "center", "address", "dl", "dt", "dd",
];
/// Celles dont le contenu ne s'affiche pas.
const HIDDEN: [&str; 6] = ["script", "style", "head", "title", "template", "noscript"];

/// La valeur d'un attribut d'une balise : `attribute("a href=\"x\" id=y", "href")` donne `x`.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = tag[at + name.len()..].trim_start();
        if before_ok && let Some(value) = rest.strip_prefix('=') {
            let value = value.trim_start();
            return Some(match value.chars().next() {
                Some(q @ ('"' | '\'')) => value[1..].split(q).next().unwrap_or_default().to_string(),
                _ => value.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or_default().to_string(),
            });
        }
        from = at + name.len();
    }
    None
}

/// Le caractère d'une entité : « &eacute; », « &#233; », « &#xE9; ».
fn entity(name: &str) -> Option<char> {
    if let Some(code) = name.strip_prefix('#') {
        let n = match code.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => code.parse().ok()?,
        };
        // les codes de l'ancien Windows que beaucoup de messages emploient pour les guillemets et les tirets
        return char::from_u32(match n {
            0x91 => 0x2018,
            0x92 => 0x2019,
            0x93 => 0x201c,
            0x94 => 0x201d,
            0x96 => 0x2013,
            0x97 => 0x2014,
            0x85 => 0x2026,
            0x80 => 0x20ac,
            n => n,
        });
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "eacute" => 'é',
        "egrave" => 'è',
        "ecirc" => 'ê',
        "euml" => 'ë',
        "agrave" => 'à',
        "acirc" => 'â',
        "auml" => 'ä',
        "ccedil" => 'ç',
        "icirc" => 'î',
        "iuml" => 'ï',
        "ocirc" => 'ô',
        "ouml" => 'ö',
        "ugrave" => 'ù',
        "ucirc" => 'û',
        "uuml" => 'ü',
        "Eacute" => 'É',
        "Egrave" => 'È',
        "Ecirc" => 'Ê',
        "Agrave" => 'À',
        "Ccedil" => 'Ç',
        "oelig" => 'œ',
        "OElig" => 'Œ',
        "aelig" => 'æ',
        "ntilde" => 'ñ',
        "szlig" => 'ß',
        "laquo" => '«',
        "raquo" => '»',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "ndash" => '–',
        "mdash" => '—',
        "hellip" => '…',
        "bull" => '•',
        "middot" => '·',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "euro" => '€',
        "pound" => '£',
        "deg" => '°',
        "times" => '×',
        "rarr" => '→',
        "larr" => '←',
        "zwnj" | "zwj" | "shy" => '\u{200b}',
        _ => return None,
    })
}

/// Le texte entre deux balises : entités remplacées, espaces et retours à la ligne ramenés à un espace.
fn push_text(out: &mut String, raw: &str, keep_lines: bool) {
    let mut rest = raw;
    while !rest.is_empty() {
        let c = rest.chars().next().unwrap_or(' ');
        if c == '&' {
            // une entité, si elle se termine vite par un point-virgule
            if let Some(end) = rest[1..].find(';').filter(|&e| e <= 10)
                && let Some(decoded) = entity(&rest[1..1 + end])
            {
                if decoded != '\u{200b}' {
                    out.push(decoded);
                }
                rest = &rest[end + 2..];
                continue;
            }
        }
        if keep_lines && c.is_whitespace() {
            // texte préformaté : espaces et retours à la ligne comptent
            if c != '\r' {
                out.push(if c == '\t' { ' ' } else { c });
            }
        } else if c.is_whitespace() {
            if !out.ends_with([' ', '\n']) && !out.is_empty() {
                out.push(' ');
            }
        } else if c != '\u{200b}' && c != '\u{feff}' && c != '\r' {
            out.push(c);
        }
        rest = &rest[c.len_utf8()..];
    }
}

/// Commence une nouvelle ligne (une seule ligne vide de suite au plus).
fn new_line(out: &mut String, blank: bool) {
    while out.ends_with(' ') {
        out.pop();
    }
    if out.is_empty() {
        return;
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    if blank && !out.ends_with("\n\n") {
        out.push('\n');
    }
}

/// Rend un message en HTML en texte à lire.
pub fn to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    // la balise dont on ignore le contenu, le lien en cours (adresse, début de son texte), « pre »
    let (mut hidden, mut link, mut pre): (Option<String>, Option<(String, usize)>, bool) = (None, None, false);
    let mut quote = 0usize;
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            if hidden.is_none() {
                push_text(&mut out, rest, pre);
            }
            break;
        };
        if hidden.is_none() {
            push_text(&mut out, &rest[..open], pre);
        }
        rest = &rest[open..];
        // un commentaire : jusqu'à « --> »
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |end| &rest[end + 3..]);
            continue;
        }
        let Some(close) = rest.find('>') else { break };
        let tag = &rest[1..close];
        rest = &rest[close + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        if let Some(waiting) = &hidden {
            if closing && *waiting == name {
                hidden = None;
            }
            continue;
        }
        if HIDDEN.contains(&name.as_str()) {
            if !closing && !tag.ends_with('/') {
                hidden = Some(name);
            }
            continue;
        }
        match name.as_str() {
            "br" => new_line(&mut out, false),
            "hr" => {
                new_line(&mut out, false);
                out.push_str("────────\n");
            }
            "li" if !closing => {
                new_line(&mut out, false);
                out.push_str("• ");
            }
            "blockquote" => {
                new_line(&mut out, true);
                quote = if closing { quote.saturating_sub(1) } else { quote + 1 };
            }
            "pre" => {
                new_line(&mut out, true);
                pre = !closing;
            }
            "a" if !closing => link = attribute(tag, "href").map(|href| (href, out.len())),
            "a" => {
                // l'adresse d'un lien s'affiche si son texte ne la montre pas déjà
                if let Some((href, start)) = link.take() {
                    let shown = out[start.min(out.len())..].trim().to_string();
                    let web = href.starts_with("http://") || href.starts_with("https://");
                    let bare = |s: &str| s.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_lowercase();
                    if web && shown.is_empty() {
                        out.push_str(&href);
                    } else if web && bare(&shown) != bare(&href) {
                        out.push_str(&format!(" ({href})"));
                    }
                }
            }
            "img" => {
                if let Some(alt) = attribute(tag, "alt").filter(|a| !a.trim().is_empty()) {
                    push_text(&mut out, &format!("[{}]", alt.trim()), false);
                }
            }
            "td" | "th" if closing => {
                if !out.ends_with([' ', '\n']) {
                    out.push(' ');
                }
            }
            "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "table" | "ul" | "ol" => new_line(&mut out, true),
            n if BLOCKS.contains(&n) => new_line(&mut out, false),
            _ => {}
        }
        let _ = quote;
    }
    // les lignes sans rien, et pas plus d'une ligne vide de suite
    let mut clean = String::with_capacity(out.len());
    let mut blank = 0;
    for line in out.lines().map(str::trim_end) {
        blank = if line.trim().is_empty() { blank + 1 } else { 0 };
        if blank <= 1 {
            clean.push_str(line);
            clean.push('\n');
        }
    }
    clean.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newsletter_becomes_readable_text() {
        let html = r#"<!DOCTYPE html><html><head><title>Ignored</title><style>p { color: red }</style></head>
<body><!-- preview --><h1>Plume&nbsp;0.8</h1>
<p>Bonjour  Sam,<br>voici   les <b>nouveaut&eacute;s</b> &amp; plus&#8230;</p>
<ul><li>Recherche &#x2192; rapide</li><li>Onglets</li></ul>
<table><tr><td>Prix</td><td>10&nbsp;&euro;</td></tr><tr><td>Total</td><td>12 $</td></tr></table>
<p>Voir <a href="https://example.com/notes?id=1">les notes</a>, ou <a href="https://example.com/">example.com</a>.
<a href="mailto:sam@example.com">Écrire</a> <a href="https://example.com/x"><img src="https://tracker.example/p.gif" alt="Logo"></a></p>
<script>alert("no")</script><hr><div>Fin</div><img src="x.png"></body></html>"#;
        let text = to_text(html);
        assert_eq!(
            text,
            "Plume 0.8\n\nBonjour Sam,\nvoici les nouveautés & plus…\n\n• Recherche → rapide\n• Onglets\n\nPrix 10 €\nTotal 12 $\n\nVoir les notes (https://example.com/notes?id=1), ou example.com. Écrire [Logo] (https://example.com/x)\n\n────────\nFin"
        );
        // rien de ce qui ne se lit pas ne passe
        assert!(!text.contains("alert") && !text.contains("color") && !text.contains("Ignored") && !text.contains("tracker"));
    }

    #[test]
    fn odd_html_does_not_break() {
        assert_eq!(to_text(""), "");
        assert_eq!(to_text("just text &unknown; &amp; <unclosed"), "just text &unknown; &");
        assert_eq!(to_text("a<br/>b<BR>c"), "a\nb\nc");
        assert_eq!(to_text("<p>one</p><p></p><p></p><p>two</p>"), "one\n\ntwo");
        assert_eq!(to_text("<pre>a\n  b</pre>"), "a\n  b");
        assert_eq!(to_text("x &#147;quoted&#148; &#150; y"), "x “quoted” – y");
        assert_eq!(to_text("<a href='javascript:alert(1)'>click</a>"), "click");
        assert_eq!(to_text("<style>a{}</style><STYLE>b{}</STYLE>seen"), "seen");
        assert_eq!(attribute("a id=x href=\"https://e.com/a b\"", "href").as_deref(), Some("https://e.com/a b"));
        assert_eq!(attribute("img data-alt=no alt=yes", "alt").as_deref(), Some("yes"));
    }
}
