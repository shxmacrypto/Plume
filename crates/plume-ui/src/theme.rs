//! Le thème de Plume : les mêmes couleurs que le navigateur (src/ui/style.css), en sombre et en clair.

use super::geom::Color;

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub dark: bool,
    /// Fond des panneaux et fond de la fenêtre.
    pub bg: Color,
    pub bg2: Color,
    /// Dégradé de l'en-tête (haut, bas).
    pub hdr1: Color,
    pub hdr2: Color,
    pub card: Color,
    pub solid: Color,
    pub solid2: Color,
    /// Remplissages translucides : champs, pastilles, survol.
    pub fill: Color,
    pub fill2: Color,
    pub hover: Color,
    /// Textes, du plus marqué au plus discret.
    pub t1: Color,
    pub t2: Color,
    pub t3: Color,
    pub t4: Color,
    pub accent: Color,
    pub green: Color,
    pub red: Color,
    pub orange: Color,
    /// Filets de séparation et liseré des surfaces.
    pub sep: Color,
    pub ring: Color,
    /// Ligne d'ombre sous l'en-tête.
    pub edge: Color,
    /// Survol du bouton Fermer de la fenêtre.
    pub close_hover: Color,
}

impl Theme {
    pub fn dark() -> Theme {
        Theme {
            dark: true,
            bg: Color::hex(0x1c1c1e),
            bg2: Color::hex(0x141416),
            hdr1: Color::hex(0x262628),
            hdr2: Color::hex(0x1e1e20),
            card: Color::hexa(0x242426, 0.94),
            solid: Color::hex(0x2c2c2e),
            solid2: Color::hex(0x3a3a3c),
            fill: Color::hexa(0x767680, 0.24),
            fill2: Color::hexa(0x767680, 0.34),
            hover: Color::hexa(0x767680, 0.16),
            t1: Color::hex(0xffffff),
            t2: Color::hexa(0xebebf5, 0.74),
            t3: Color::hexa(0xebebf5, 0.56),
            t4: Color::hexa(0xebebf5, 0.32),
            accent: Color::hex(0x0a84ff),
            green: Color::hex(0x30d158),
            red: Color::hex(0xff453a),
            orange: Color::hex(0xff9f0a),
            sep: Color::hexa(0xffffff, 0.08),
            ring: Color::hexa(0xffffff, 0.08),
            edge: Color::hexa(0x000000, 0.4),
            close_hover: Color::hex(0xc42b1c),
        }
    }

    pub fn light() -> Theme {
        Theme {
            dark: false,
            bg: Color::hex(0xf2f2f7),
            bg2: Color::hex(0xe9e9ee),
            hdr1: Color::hex(0xf7f7f9),
            hdr2: Color::hex(0xececf0),
            card: Color::hexa(0xffffff, 0.96),
            solid: Color::hex(0xffffff),
            solid2: Color::hex(0xe5e5ea),
            fill: Color::hexa(0x767680, 0.12),
            fill2: Color::hexa(0x767680, 0.2),
            hover: Color::hexa(0x767680, 0.1),
            t1: Color::hex(0x000000),
            t2: Color::hexa(0x3c3c43, 0.85),
            t3: Color::hexa(0x3c3c43, 0.62),
            t4: Color::hexa(0x3c3c43, 0.36),
            accent: Color::hex(0x007aff),
            green: Color::hex(0x248a3d),
            red: Color::hex(0xd70015),
            orange: Color::hex(0xc93400),
            sep: Color::hexa(0x000000, 0.1),
            ring: Color::hexa(0x000000, 0.08),
            edge: Color::hexa(0x000000, 0.12),
            close_hover: Color::hex(0xc42b1c),
        }
    }

    pub fn new(dark: bool, accent: Option<Color>) -> Theme {
        let mut t = if dark { Theme::dark() } else { Theme::light() };
        if let Some(a) = accent {
            t.accent = a;
        }
        t
    }

    /// Fond d'une ligne sélectionnée (fenêtre active ou non).
    pub fn selection(&self, active: bool) -> Color {
        if active { self.accent.fade(if self.dark { 0.34 } else { 0.22 }) } else { self.fill }
    }
}

/// Mesures communes de l'interface, en DIP (reprises de Plume).
pub mod m {
    /// Bande des onglets (barre de titre).
    pub const TITLE_H: f32 = 42.0;
    /// Barre d'outils : navigation, adresse, recherche.
    pub const BAR_H: f32 = 44.0;
    pub const TAB_H: f32 = 30.0;
    pub const TAB_W: f32 = 232.0;
    pub const TAB_MIN_W: f32 = 72.0;
    pub const TAB_R: f32 = 9.0;
    /// Boutons de la barre d'outils.
    pub const NB_W: f32 = 34.0;
    pub const NB_H: f32 = 30.0;
    pub const NB_R: f32 = 8.0;
    /// Champ d'adresse et de recherche.
    pub const FIELD_H: f32 = 32.0;
    pub const FIELD_R: f32 = 10.0;
    /// Boutons de fenêtre (réduire, agrandir, fermer).
    pub const CAPTION_W: f32 = 46.0;
    pub const ROW_H: f32 = 30.0;
    pub const ROW_R: f32 = 7.0;
    pub const HEADER_H: f32 = 30.0;
    pub const STATUS_H: f32 = 28.0;
    pub const NAV_ROW_H: f32 = 30.0;
    pub const ICON: f32 = 16.0;
}
