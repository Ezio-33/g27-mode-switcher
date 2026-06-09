//! Thème sombre « La Confrérie des Ombres » et police Cinzel embarquée.
//!
//! Palette reprise des variables CSS (`:root`) du site de l'auteur : fonds
//! bleu-nuit profonds, accent or, texte parchemin. La police de titres Cinzel
//! (SIL Open Font License) est embarquée dans le binaire ; le corps reste sur
//! les polices par défaut d'egui.

use std::sync::Arc;

use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle};

// Palette — cf. `styles.css` du site (bloc `:root`).
pub const BG_DEEP: Color32 = Color32::from_rgb(0x05, 0x08, 0x0b);
pub const BG_BASE: Color32 = Color32::from_rgb(0x0a, 0x10, 0x14);
pub const BG_CARD: Color32 = Color32::from_rgb(0x0f, 0x16, 0x20);
pub const BG_CARD_HOVER: Color32 = Color32::from_rgb(0x13, 0x1c, 0x28);
pub const BG_ELEVATED: Color32 = Color32::from_rgb(0x1a, 0x25, 0x32);
pub const BG_PANEL: Color32 = Color32::from_rgb(0x0d, 0x16, 0x20);
pub const BORDER: Color32 = Color32::from_rgb(0x1f, 0x2a, 0x38);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x2c, 0x3a, 0x4c);
pub const GOLD: Color32 = Color32::from_rgb(0xd4, 0xa8, 0x43);
pub const GOLD_LIGHT: Color32 = Color32::from_rgb(0xf0, 0xc7, 0x5e);
pub const GOLD_DARK: Color32 = Color32::from_rgb(0xa0, 0x7f, 0x2c);
pub const TEXT: Color32 = Color32::from_rgb(0xf5, 0xec, 0xd1);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0xb8, 0xc2, 0xd0);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x8a, 0x96, 0xaa);
pub const SUCCESS: Color32 = Color32::from_rgb(0x6f, 0xd1, 0x7a);
pub const WARNING: Color32 = Color32::from_rgb(0xf0, 0xc7, 0x5e);
pub const LIVE: Color32 = Color32::from_rgb(0xff, 0x3b, 0x46);

/// Clé de la famille de police des titres (Cinzel).
const CINZEL: &str = "cinzel";
/// Clé de la police de corps (Inter), proportionnelle par défaut.
const INTER: &str = "inter";

/// Famille de police des titres (Cinzel) pour composer un `RichText`.
#[must_use]
pub fn heading_family() -> FontFamily {
    FontFamily::Name(CINZEL.into())
}

/// Cadre d'une carte de contenu (fond carte, bordure, coins arrondis, marge).
pub fn card_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(16, 14))
}

/// Cadre du journal : identique à une carte mais sur le fond le plus profond.
pub fn journal_frame() -> egui::Frame {
    card_frame().fill(BG_DEEP)
}

/// Cadre arrondi (forme pilule) de la pastille de statut.
pub fn pill_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(BG_ELEVATED)
        .stroke(Stroke::new(1.0, BORDER_STRONG))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin::symmetric(16, 7))
}

/// Installe la police Cinzel et le thème sombre sur le contexte egui.
pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    install_style(ctx);
}

/// Embarque Cinzel (titres) et Inter (corps) dans le binaire.
///
/// Cinzel est exposé comme famille nommée dédiée aux titres ; Inter devient la
/// police proportionnelle prioritaire (les polices par défaut d'egui restent en
/// repli pour les glyphes manquants).
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        CINZEL.to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/Cinzel-Variable.ttf"
        ))),
    );
    fonts
        .families
        .entry(FontFamily::Name(CINZEL.into()))
        .or_default()
        .insert(0, CINZEL.to_owned());

    fonts.font_data.insert(
        INTER.to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter-Variable.ttf"
        ))),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, INTER.to_owned());

    ctx.set_fonts(fonts);
}

/// Applique la palette (couleurs, bordures, espacements) et la typographie.
fn install_style(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    let mut visuals = egui::Visuals::dark();

    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG_BASE;
    visuals.window_fill = BG_PANEL;
    visuals.window_stroke = Stroke::new(1.0, BORDER_STRONG);
    visuals.extreme_bg_color = BG_DEEP;
    visuals.faint_bg_color = BG_CARD;
    visuals.hyperlink_color = GOLD_LIGHT;

    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = BG_CARD;
    w.noninteractive.weak_bg_fill = BG_CARD;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.inactive.bg_fill = BG_CARD;
    w.inactive.weak_bg_fill = BG_CARD;
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    w.hovered.bg_fill = BG_CARD_HOVER;
    w.hovered.weak_bg_fill = BG_CARD_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    w.active.bg_fill = BG_ELEVATED;
    w.active.weak_bg_fill = BG_ELEVATED;
    w.active.bg_stroke = Stroke::new(1.0, GOLD);

    visuals.selection.bg_fill = GOLD.gamma_multiply(0.30);
    visuals.selection.stroke = Stroke::new(1.0, GOLD);

    style.visuals = visuals;

    // Tailles relevées pour la lisibilité (accessibilité RGAA/WCAG) : corps ~16 px,
    // texte secondaire ~14 px — confortable pour tous, sans fenêtre démesurée.
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(28.0, FontFamily::Name(CINZEL.into())),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(16.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(16.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(14.0, FontFamily::Proportional),
    );

    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(14.0, 9.0);
    ctx.set_global_style(style);
}
