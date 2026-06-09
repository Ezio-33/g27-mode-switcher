//! Carte GUI « Mode Forza » : retour de force synthétisé depuis la télémétrie Data Out.
//!
//! Distincte du pont vJoy : ici le G27 **n'est pas masqué** (le jeu le reconnaît, la
//! navigation menus/map reste native), l'application écoute le flux UDP « Data Out » de
//! Forza et écrit la force calculée au volant. Aucun vJoy/HidHide requis.
// « Data Out » est un nom de fonctionnalité Forza, pas un identifiant de code.
#![allow(clippy::doc_markdown)]

use std::time::Duration;

use eframe::egui::{self, RichText};
use g27_mode_switcher::config::Config;
use g27_mode_switcher::telemetrie::{PontTelemetrie, ReglagesForza};

use super::log::{LineKind, LogBuffer};
use super::theme;

/// État de la carte « Mode Forza ».
pub struct CarteForza {
    /// `Some` quand l'écoute télémétrie + l'écriture de force sont actives.
    pont: Option<PontTelemetrie>,
}

impl CarteForza {
    /// Crée la carte (inactive).
    #[must_use]
    pub fn new() -> Self {
        Self { pont: None }
    }

    /// Arrête le mode Forza (le `Drop` du pont remet le volant au neutre). Appelé à la
    /// fermeture et lors d'un changement de mode de jeu.
    pub fn arreter(&mut self) {
        self.pont = None;
    }

    /// Affiche la carte et gère les interactions.
    pub fn afficher(&mut self, ui: &mut egui::Ui, config: &mut Config, log: &LogBuffer) {
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            etiquette_section(ui, "RETOUR DE FORCE FORZA (TÉLÉMÉTRIE)");
            ui.add_space(4.0);
            ui.label(
                RichText::new("Aucun prérequis — ni vJoy ni HidHide, tout passe par le jeu.")
                    .small()
                    .color(theme::SUCCESS),
            );
            ui.add_space(10.0);
            if self.pont.is_some() {
                self.afficher_actif(ui, config, log);
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            } else {
                self.afficher_inactif(ui, config, log);
            }
        });
    }

    /// Mode actif : statut en direct + réglages à chaud + bouton « Arrêter ».
    fn afficher_actif(&mut self, ui: &mut egui::Ui, config: &mut Config, log: &LogBuffer) {
        let statut = self.pont.as_ref().map(PontTelemetrie::statut);
        if let Some(statut) = statut {
            ligne_statut(ui, statut.reception, statut.course_active);
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "Dérive avant : {:+.3} rad · couple : {:+} · paquets : {}",
                    statut.derive_avant, statut.couple, statut.paquets
                ))
                .small()
                .color(theme::TEXT_DIM),
            );
        }
        if reglages_force(ui, config)
            && let Some(pont) = self.pont.as_ref()
        {
            pont.reconfigurer(reglages_depuis_config(config));
        }
        ui.add_space(12.0);
        let bouton = egui::Button::new(
            RichText::new("Arrêter le mode Forza")
                .color(theme::TEXT)
                .strong(),
        )
        .fill(theme::BG_ELEVATED)
        .stroke(egui::Stroke::new(1.0, theme::BORDER_STRONG))
        .min_size(egui::vec2(ui.available_width(), 38.0));
        if ui.add(bouton).clicked() {
            self.pont = None; // Drop → volant remis au neutre.
            log.push(
                LineKind::Info,
                "Mode Forza arrêté — volant remis au neutre.",
            );
        }
    }

    /// Mode inactif : procédure de configuration + réglages + bouton « Démarrer ».
    fn afficher_inactif(&mut self, ui: &mut egui::Ui, config: &mut Config, log: &LogBuffer) {
        aide_configuration(ui, config);
        ui.add_space(8.0);
        let _ = reglages_force(ui, config);
        ui.add_space(12.0);
        if bouton_or(ui, "Démarrer le mode Forza").clicked() {
            self.demarrer(config, log);
        }
    }

    /// Lance l'écoute télémétrie + l'écriture de force (rapide : pas de fenêtrage vJoy).
    fn demarrer(&mut self, config: &Config, log: &LogBuffer) {
        let port = config.forza.port;
        match PontTelemetrie::demarrer(port, reglages_depuis_config(config)) {
            Ok(actif) => {
                self.pont = Some(actif);
                log.push(
                    LineKind::Success,
                    format!("Mode Forza démarré — écoute de la télémétrie sur le port {port}."),
                );
            }
            Err(erreur) => log.push(
                LineKind::Error,
                format!("Démarrage du mode Forza impossible : {erreur}"),
            ),
        }
    }
}

/// Construit les réglages de force depuis la config.
fn reglages_depuis_config(config: &Config) -> ReglagesForza {
    ReglagesForza {
        gain: config.forza.gain,
        inverser: config.forza.inverser,
    }
}

/// Ligne de statut colorée (réception du flux + gameplay actif).
fn ligne_statut(ui: &mut egui::Ui, reception: bool, course_active: bool) {
    let (couleur, texte) = if !reception {
        (theme::WARNING, "En attente du flux télémétrie\u{2026}")
    } else if course_active {
        (
            theme::SUCCESS,
            "Réception OK — course active, retour de force appliqué",
        )
    } else {
        (
            theme::TEXT_MUTED,
            "Réception OK — hors course (aucune force)",
        )
    };
    ui.label(RichText::new(texte).color(couleur).strong());
}

/// Réglages communs (port si inactif, gain, inversion). Renvoie `true` si un réglage a
/// changé (pour reconfigurer à chaud / persister). Le port n'est éditable qu'à l'arrêt.
fn reglages_force(ui: &mut egui::Ui, config: &mut Config) -> bool {
    let mut change = false;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Intensité").size(15.0).color(theme::TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let curseur = egui::Slider::new(&mut config.forza.gain, 0..=100).suffix(" %");
            change |= ui.add(curseur).changed();
        });
    });
    ui.add_space(4.0);
    let mut inverser = config.forza.inverser;
    if ui
        .checkbox(
            &mut inverser,
            RichText::new("Inverser le sens du couple")
                .size(15.0)
                .color(theme::TEXT),
        )
        .on_hover_text(
            "Si le volant « fuit » au lieu de résister en virage, cochez pour inverser le \
             sens du retour de force.",
        )
        .changed()
    {
        config.forza.inverser = inverser;
        change = true;
    }
    change
}

/// Bloc d'aide : procédure d'activation de « Data Out » côté Forza + port d'écoute.
fn aide_configuration(ui: &mut egui::Ui, config: &mut Config) {
    ui.label(
        RichText::new(
            "G27 non masqué : navigation native. La force vient de la télémétrie du jeu.",
        )
        .small()
        .color(theme::TEXT_MUTED),
    );
    ui.add_space(6.0);
    for ligne in [
        "1. Forza : Réglages > HUD et Gameplay > « Data Out » → Activé (On).",
        "2. IP de sortie des données : 127.0.0.1",
        "3. Port de sortie des données : identique au port ci-dessous.",
    ] {
        ui.label(RichText::new(ligne).small().color(theme::TEXT_DIM));
    }
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Port d'écoute").size(15.0).color(theme::TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(egui::DragValue::new(&mut config.forza.port).range(1..=65535));
        });
    });
    ui.label(
        RichText::new("4. Lancez une course, puis « Démarrer ».")
            .small()
            .color(theme::TEXT_DIM),
    );
}

/// Bouton d'action doré pleine largeur.
fn bouton_or(ui: &mut egui::Ui, texte: &str) -> egui::Response {
    let bouton = egui::Button::new(RichText::new(texte).color(theme::BG_BASE).strong())
        .fill(theme::GOLD)
        .min_size(egui::vec2(ui.available_width(), 40.0));
    ui.add(bouton)
}

/// Petit label de section (majuscules, atténué), identique aux autres cartes.
fn etiquette_section(ui: &mut egui::Ui, texte: &str) {
    ui.label(RichText::new(texte).small().strong().color(theme::TEXT_DIM));
}
