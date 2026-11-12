//! Canvas sizes to start a design from, and a few starter designs drawn with Plume Create's own shapes and text
//! (no outside pictures or fonts: Windows' own).

use super::doc::{Doc, Kind, Layer, Paint, Rgba, ShapeKind, ShapeL, TextAlign, TextL, Xf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    pub key: &'static str,
    pub name: &'static str,
    pub w: u32,
    pub h: u32,
}

pub const PRESETS: [Preset; 7] = [
    Preset { key: "post", name: "Instagram post", w: 1080, h: 1080 },
    Preset { key: "portrait", name: "Instagram portrait", w: 1080, h: 1350 },
    Preset { key: "story", name: "Instagram story", w: 1080, h: 1920 },
    Preset { key: "tiktok", name: "TikTok video cover", w: 1080, h: 1920 },
    Preset { key: "thumb", name: "YouTube thumbnail", w: 1280, h: 720 },
    Preset { key: "a4", name: "Poster A4", w: 2480, h: 3508 },
    Preset { key: "banner", name: "Banner", w: 1500, h: 500 },
];

#[cfg(any(test, feature = "test-hook"))]
pub fn preset(key: &str) -> Option<Preset> {
    PRESETS.iter().copied().find(|p| p.key == key)
}

/// A new empty design: a white background layer.
pub fn blank(w: u32, h: u32, background: &str) -> Doc {
    let mut d = Doc::new(w, h);
    let id = d.id();
    let sh = ShapeL { kind: ShapeKind::Rect, fill: Some(Paint::Solid(Rgba::WHITE)), stroke: None, stroke_w: 0.0, radius: 0.0 };
    let mut bg = Layer::new(id, background, Xf::new(w as f32 / 2.0, h as f32 / 2.0, w as f32, h as f32), Kind::Shape(sh));
    bg.locked = true;
    d.layers.push(bg);
    d
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Template {
    pub key: &'static str,
    pub name: &'static str,
    pub w: u32,
    pub h: u32,
}

pub const TEMPLATES: [Template; 5] = [
    Template { key: "story-sale", name: "Bold story", w: 1080, h: 1920 },
    Template { key: "quote", name: "Quote post", w: 1080, h: 1080 },
    Template { key: "thumb-bold", name: "Video thumbnail", w: 1280, h: 720 },
    Template { key: "poster-event", name: "Event poster", w: 2480, h: 3508 },
    Template { key: "banner-clean", name: "Clean banner", w: 1500, h: 500 },
];

struct B<'a> {
    d: Doc,
    measure: &'a dyn Fn(&TextL, f32) -> f32,
}

impl B<'_> {
    fn shape(&mut self, name: &str, x: f32, y: f32, w: f32, h: f32, sh: ShapeL) -> &mut Layer {
        let id = self.d.id();
        self.d.layers.push(Layer::new(id, name, Xf::new(x + w / 2.0, y + h / 2.0, w, h), Kind::Shape(sh)));
        self.d.layers.last_mut().unwrap()
    }
    fn rect(&mut self, name: &str, (x, y, w, h): (f32, f32, f32, f32), fill: Paint, radius: f32) -> &mut Layer {
        self.shape(name, x, y, w, h, ShapeL { kind: ShapeKind::Rect, fill: Some(fill), stroke: None, stroke_w: 0.0, radius })
    }
    fn ellipse(&mut self, name: &str, (x, y, w, h): (f32, f32, f32, f32), fill: Option<Paint>, stroke: Option<(Rgba, f32)>) -> &mut Layer {
        self.shape(name, x, y, w, h, ShapeL { kind: ShapeKind::Ellipse, fill, stroke: stroke.map(|s| s.0), stroke_w: stroke.map_or(0.0, |s| s.1), radius: 0.0 })
    }
    /// Text whose box starts at (x, y), `w` wide; its height comes from the letters.
    fn text(&mut self, name: &str, x: f32, y: f32, w: f32, t: TextL) -> &mut Layer {
        let h = (self.measure)(&t, w).max(t.size);
        let id = self.d.id();
        self.d.layers.push(Layer::new(id, name, Xf::new(x + w / 2.0, y + h / 2.0, w, h), Kind::Text(t)));
        self.d.layers.last_mut().unwrap()
    }
}

fn t(text: &str, size: f32, weight: u16, color: Rgba, align: TextAlign) -> TextL {
    TextL { text: text.into(), font: "Segoe UI".into(), size, weight, color, align, ..TextL::default() }
}

/// A starter design, its words in English or French.
pub fn template(key: &str, fr: bool, measure: &dyn Fn(&TextL, f32) -> f32) -> Option<Doc> {
    let tp = TEMPLATES.iter().find(|t| t.key == key)?;
    let tr = |en: &'static str, f: &'static str| if fr { f } else { en };
    let mut b = B { d: Doc::new(tp.w, tp.h), measure };
    let (w, h) = (tp.w as f32, tp.h as f32);
    match key {
        "story-sale" => {
            let bg = b.rect(tr("Background", "Fond"), (0.0, 0.0, w, h), Paint::Linear(Rgba::hex(0xdb2b9a), Rgba::hex(0xff8a3d), 90.0), 0.0);
            bg.locked = true;
            b.ellipse(tr("Circle", "Cercle"), (-220.0, -160.0, 760.0, 760.0), Some(Paint::Solid(Rgba([255, 255, 255, 46]))), None);
            b.ellipse(tr("Ring", "Anneau"), (640.0, 1320.0, 620.0, 620.0), None, Some((Rgba([255, 255, 255, 120]), 18.0)));
            b.text(
                tr("Kicker", "Accroche"),
                96.0,
                520.0,
                w - 192.0,
                TextL { spacing: 12.0, ..t(tr("NEW SEASON", "NOUVELLE SAISON"), 54.0, 600, Rgba::WHITE, TextAlign::Left) },
            );
            let mut big = t(tr("SUMMER\nSALE", "SOLDES\nD'ÉTÉ"), 176.0, 900, Rgba::WHITE, TextAlign::Left);
            big.line = 0.98;
            big.shadow = Some((Rgba([90, 10, 60, 110]), 0.0, 14.0, 26.0));
            b.text(tr("Title", "Titre"), 90.0, 620.0, w - 150.0, big);
            b.rect(tr("Badge", "Pastille"), (96.0, 1150.0, 420.0, 150.0), Paint::Solid(Rgba::hex(0x2b0a24)), 75.0);
            b.text(tr("Badge text", "Texte de la pastille"), 96.0, 1185.0, 420.0, t("-50 %", 76.0, 800, Rgba::WHITE, TextAlign::Center));
            b.text(
                tr("Details", "Détails"),
                96.0,
                1360.0,
                700.0,
                t(tr("On everything, this weekend only.", "Sur tout, ce week-end seulement."), 50.0, 500, Rgba::WHITE, TextAlign::Left),
            );
            b.rect(tr("Button", "Bouton"), (96.0, 1640.0, 520.0, 120.0), Paint::Solid(Rgba::WHITE), 60.0);
            b.text(
                tr("Button text", "Texte du bouton"),
                96.0,
                1668.0,
                520.0,
                t(tr("Shop now", "J'en profite"), 52.0, 700, Rgba::hex(0xc0267f), TextAlign::Center),
            );
        }
        "quote" => {
            let bg = b.rect(tr("Background", "Fond"), (0.0, 0.0, w, h), Paint::Solid(Rgba::hex(0x151826)), 0.0);
            bg.locked = true;
            b.rect(tr("Accent", "Accent"), (120.0, 150.0, 140.0, 14.0), Paint::Solid(Rgba::hex(0xffc857)), 7.0);
            b.text(tr("Quote mark", "Guillemet"), 110.0, 170.0, 300.0, t("\u{201C}", 300.0, 800, Rgba::hex(0xffc857), TextAlign::Left));
            let mut q = t(tr("Make it simple, but significant.", "Faire simple, mais marquant."), 96.0, 700, Rgba::WHITE, TextAlign::Left);
            q.line = 1.12;
            b.text(tr("Quote", "Citation"), 120.0, 470.0, w - 240.0, q);
            b.text(
                tr("Author", "Auteur"),
                120.0,
                850.0,
                w - 240.0,
                TextL { spacing: 4.0, ..t(tr("— YOUR NAME", "— VOTRE NOM"), 40.0, 600, Rgba([255, 255, 255, 170]), TextAlign::Left) },
            );
        }
        "thumb-bold" => {
            let bg = b.rect(tr("Background", "Fond"), (0.0, 0.0, w, h), Paint::Linear(Rgba::hex(0x0f172a), Rgba::hex(0x1e3a8a), 0.0), 0.0);
            bg.locked = true;
            let slab = b.rect(tr("Slab", "Bande"), (760.0, -100.0, 700.0, 940.0), Paint::Solid(Rgba::hex(0xfacc15)), 0.0);
            slab.xf.rot = 12.0;
            let mut title = t(tr("I TRIED\nIT ALL", "J'AI TOUT\nESSAYÉ"), 132.0, 900, Rgba::WHITE, TextAlign::Left);
            title.line = 0.98;
            title.outline = Some((Rgba::hex(0x0b1020), 10.0));
            b.text(tr("Title", "Titre"), 70.0, 150.0, 780.0, title);
            b.shape(
                tr("Arrow", "Flèche"),
                620.0,
                520.0,
                260.0,
                90.0,
                ShapeL { kind: ShapeKind::Arrow, fill: None, stroke: Some(Rgba::hex(0xef4444)), stroke_w: 22.0, radius: 0.0 },
            );
            b.rect(tr("Tag", "Étiquette"), (70.0, 540.0, 300.0, 84.0), Paint::Solid(Rgba::hex(0xef4444)), 16.0);
            b.text(tr("Tag text", "Texte de l'étiquette"), 70.0, 556.0, 300.0, t(tr("PART 1", "PARTIE 1"), 44.0, 800, Rgba::WHITE, TextAlign::Center));
        }
        "poster-event" => {
            let bg = b.rect(tr("Background", "Fond"), (0.0, 0.0, w, h), Paint::Solid(Rgba::hex(0xf5efe6)), 0.0);
            bg.locked = true;
            b.ellipse(tr("Sun", "Soleil"), (700.0, 500.0, 1080.0, 1080.0), Some(Paint::Linear(Rgba::hex(0xff6b35), Rgba::hex(0xdb2b9a), 90.0)), None);
            b.rect(tr("Ground", "Sol"), (0.0, 1500.0, w, 2008.0), Paint::Solid(Rgba::hex(0x1f2933)), 0.0);
            b.text(
                tr("Date", "Date"),
                200.0,
                1700.0,
                w - 400.0,
                TextL { spacing: 30.0, ..t(tr("SATURDAY 14 JUNE", "SAMEDI 14 JUIN"), 110.0, 600, Rgba::hex(0xffc857), TextAlign::Center) },
            );
            let mut name = t(tr("Open Air\nNight", "Nuit en\nplein air"), 330.0, 900, Rgba::WHITE, TextAlign::Center);
            name.line = 1.0;
            b.text(tr("Event", "Évènement"), 150.0, 1950.0, w - 300.0, name);
            b.text(
                tr("Place", "Lieu"),
                200.0,
                2800.0,
                w - 400.0,
                t(
                    tr("Music · Food · Friends\nCity park, from 7 pm", "Musique · Cuisine · Amis\nParc de la ville, dès 19 h"),
                    90.0,
                    500,
                    Rgba([255, 255, 255, 200]),
                    TextAlign::Center,
                ),
            );
        }
        "banner-clean" => {
            let bg = b.rect(tr("Background", "Fond"), (0.0, 0.0, w, h), Paint::Solid(Rgba::hex(0xffffff)), 0.0);
            bg.locked = true;
            b.rect(tr("Band", "Bande"), (0.0, 0.0, 26.0, h), Paint::Solid(Rgba::hex(0xdb2b9a)), 0.0);
            b.ellipse(tr("Dot", "Point"), (1180.0, 90.0, 320.0, 320.0), Some(Paint::Solid(Rgba::hex(0xfce7f3))), None);
            b.text(
                tr("Title", "Titre"),
                110.0,
                150.0,
                1000.0,
                t(tr("Your name or brand", "Votre nom ou marque"), 96.0, 800, Rgba::hex(0x111827), TextAlign::Left),
            );
            b.text(
                tr("Tagline", "Slogan"),
                114.0,
                290.0,
                1000.0,
                t(tr("What you do, in one short line.", "Ce que vous faites, en une ligne."), 44.0, 400, Rgba::hex(0x6b7280), TextAlign::Left),
            );
        }
        _ => return None,
    }
    Some(b.d)
}

/// Colour palettes offered next to the colour picker.
pub const PALETTES: [(&str, [u32; 6]); 6] = [
    ("Plume", [0xdb2b9a, 0xff8a3d, 0xffc857, 0x2b0a24, 0xfce7f3, 0xffffff]),
    ("Ocean", [0x0f172a, 0x1e3a8a, 0x0ea5e9, 0x5eead4, 0xe0f2fe, 0xffffff]),
    ("Forest", [0x14532d, 0x15803d, 0x84cc16, 0xd9f99d, 0xfef9c3, 0x1c1917]),
    ("Sunset", [0x7c2d12, 0xea580c, 0xf59e0b, 0xfde68a, 0xbe185d, 0x312e81]),
    ("Pastel", [0xfbcfe8, 0xddd6fe, 0xbfdbfe, 0xbbf7d0, 0xfef08a, 0xfed7aa]),
    ("Mono", [0x000000, 0x404040, 0x808080, 0xbfbfbf, 0xe5e5e5, 0xffffff]),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::render::{NoText, Renderer};

    #[test]
    fn every_template_builds() {
        let m = |t: &TextL, _w: f32| t.size * t.line * t.text.lines().count().max(1) as f32;
        for tp in TEMPLATES {
            for fr in [false, true] {
                let d = template(tp.key, fr, &m).unwrap();
                assert_eq!((d.w, d.h), (tp.w, tp.h));
                assert!(d.layers.len() >= 4);
                assert!(d.layers[0].locked);
            }
        }
        assert!(template("nope", false, &m).is_none());
        // the story's colours are where expected
        let d = template("story-sale", false, &m).unwrap();
        let img = Renderer::new(Box::new(NoText)).image(&d, 0.25);
        assert_eq!((img.w, img.h), (270, 480));
        let top = img.get(250, 4);
        assert!(top[0] > 200 && top[1] < 80, "{top:?}");
        let bottom = img.get(250, 476);
        assert!(bottom[0] > 230 && bottom[1] > 110, "{bottom:?}");
        assert_eq!(blank(100, 50, "bg").layers.len(), 1);
        assert_eq!(preset("story").map(|p| (p.w, p.h)), Some((1080, 1920)));
    }
}
