//! Éditeur interactif de remappage des boutons (fenêtre flottante).
//!
//! L'utilisateur clique une **case cible vJoy** (1 à [`NB_CIBLES`]) puis **appuie sur
//! un bouton du volant** : le bouton physique du G27 ainsi capturé est assigné à cette
//! cible. Le remappage est mémorisé dans la config (`[pont].remap_boutons`) et appliqué
//! **à chaud** au pont en cours (`reconfigurer_remap`).
//!
//! La capture lit les boutons **bruts** du dernier rapport du G27
//! ([`Pont::boutons_bruts`]) et ne retient qu'un **front montant** (bouton nouvellement
//! enfoncé depuis le clic), pour ne pas capturer un bouton déjà tenu.

use std::time::Duration;

use eframe::egui::{self, RichText};
use g27_mode_switcher::config::Config;
use g27_mode_switcher::pont::{
    NB_BOUTONS_G27, Pont, REMAP_DEFAUT, RemapBoutons, remap_depuis_liste, remap_vers_liste,
};

use super::log::{LineKind, LogBuffer};
use super::theme;

/// Nombre de cibles vJoy proposées dans la grille (boutons que le jeu peut lire).
const NB_CIBLES: u8 = 28;
/// Nombre de colonnes de la grille de cases.
const COLONNES: u8 = 4;

/// État d'une capture en cours : on attend l'appui d'un bouton du volant.
#[derive(Clone, Copy)]
struct Capture {
    /// Case cible vJoy (1-indexée) en cours d'assignation.
    cible: u8,
    /// Masque des boutons bruts au moment du clic (référence du front montant).
    boutons_init: u32,
}

/// Éditeur de remappage : fenêtre repliable + capture éventuelle.
#[derive(Default)]
pub struct EditeurRemap {
    /// Fenêtre ouverte ?
    ouvert: bool,
    /// Capture en attente d'un appui (sinon `None`).
    capture: Option<Capture>,
}

impl EditeurRemap {
    /// Bouton « Remapper les boutons… » (à placer dans la carte Pont) : ouvre la fenêtre.
    pub fn bouton_ouvrir(&mut self, ui: &mut egui::Ui) {
        let bouton = egui::Button::new(
            RichText::new("Remapper les boutons\u{2026}")
                .color(theme::TEXT)
                .strong(),
        )
        .fill(theme::BG_ELEVATED)
        .stroke(egui::Stroke::new(1.0, theme::BORDER_STRONG))
        .min_size(egui::vec2(ui.available_width(), 34.0));
        if ui.add(bouton).clicked() {
            self.ouvert = true;
        }
    }

    /// Affiche la fenêtre de remappage si elle est ouverte. `pont` (s'il existe) permet
    /// la capture en direct et l'application à chaud.
    pub fn fenetre(
        &mut self,
        ctx: &egui::Context,
        config: &mut Config,
        pont: Option<&Pont>,
        log: &LogBuffer,
    ) {
        if !self.ouvert {
            return;
        }
        let mut ouvert = true;
        egui::Window::new(RichText::new("Remappage des boutons").color(theme::TEXT))
            .open(&mut ouvert)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| self.contenu(ui, config, pont, log));
        if !ouvert {
            self.ouvert = false;
            self.capture = None;
        }
    }

    /// Contenu de la fenêtre : capture éventuelle, consignes, grille, réinitialisation.
    fn contenu(
        &mut self,
        ui: &mut egui::Ui,
        config: &mut Config,
        pont: Option<&Pont>,
        log: &LogBuffer,
    ) {
        let mut remap = remap_depuis_liste(&config.pont.remap_boutons);
        if let Some((bouton, cible)) = self.relever_capture(ui, pont) {
            assigner(&mut remap, bouton, cible);
            config.pont.remap_boutons = remap_vers_liste(&remap);
            if let Some(pont) = pont {
                pont.reconfigurer_remap(remap);
            }
            log.push(
                LineKind::Success,
                format!("Bouton du volant #{bouton} assigné à vJoy #{cible}."),
            );
            self.capture = None;
        }

        self.consignes(ui, pont.is_some());
        ui.add_space(8.0);
        self.grille(ui, &remap, pont);
        ui.add_space(10.0);
        if ui
            .button(RichText::new("Réinitialiser (défaut)").color(theme::TEXT_MUTED))
            .clicked()
        {
            config.pont.remap_boutons = remap_vers_liste(&REMAP_DEFAUT);
            if let Some(pont) = pont {
                pont.reconfigurer_remap(REMAP_DEFAUT);
            }
            self.capture = None;
            log.push(
                LineKind::Info,
                "Remappage réinitialisé aux valeurs par défaut.",
            );
        }
    }

    /// Relève un front montant pendant une capture : renvoie `(bouton_g27, cible)` dès
    /// qu'un bouton du volant est nouvellement enfoncé.
    fn relever_capture(&self, ui: &egui::Ui, pont: Option<&Pont>) -> Option<(u8, u8)> {
        let capture = self.capture?;
        let pont = pont?;
        let nouveaux = pont.boutons_bruts() & !capture.boutons_init;
        if nouveaux == 0 {
            // Toujours en attente : on redemande un rendu pour scruter le volant.
            ui.ctx().request_repaint_after(Duration::from_millis(40));
            return None;
        }
        // Plus petit bit nouvellement armé → numéro de bouton G27 (1-indexé).
        let bouton = u8::try_from(nouveaux.trailing_zeros() + 1).unwrap_or(0);
        let dans_la_plage = (1..=NB_BOUTONS_G27).contains(&(bouton as usize));
        dans_la_plage.then_some((bouton, capture.cible))
    }

    /// Consignes contextuelles (et statut « capture en cours » le cas échéant).
    fn consignes(&self, ui: &mut egui::Ui, pont_present: bool) {
        if let Some(capture) = self.capture {
            ui.label(
                RichText::new(format!(
                    "Appuyez sur un bouton du volant pour l'assigner à vJoy #{}\u{2026}",
                    capture.cible
                ))
                .color(theme::GOLD)
                .strong(),
            );
            ui.label(
                RichText::new("(recliquez la case pour annuler)")
                    .small()
                    .color(theme::TEXT_DIM),
            );
        } else if pont_present {
            ui.label(
                RichText::new(
                    "Cliquez une case puis appuyez sur le bouton du volant à y associer.",
                )
                .small()
                .color(theme::TEXT_MUTED),
            );
        } else {
            ui.label(
                RichText::new("Démarrez le pont pour capturer les boutons du volant.")
                    .small()
                    .color(theme::WARNING),
            );
        }
    }

    /// Grille des cases cibles vJoy : chaque case montre le bouton G27 associé (ou « — »).
    fn grille(&mut self, ui: &mut egui::Ui, remap: &RemapBoutons, pont: Option<&Pont>) {
        egui::Grid::new("grille_remap")
            .spacing([6.0, 6.0])
            .show(ui, |ui| {
                for cible in 1..=NB_CIBLES {
                    self.case(ui, cible, remap, pont);
                    if cible % COLONNES == 0 {
                        ui.end_row();
                    }
                }
            });
    }

    /// Une case de la grille : libellé `vJoy #N` + bouton source, cliquable pour capturer.
    fn case(&mut self, ui: &mut egui::Ui, cible: u8, remap: &RemapBoutons, pont: Option<&Pont>) {
        let source = (1..=NB_BOUTONS_G27).find(|&b| remap[b] == cible);
        let en_capture = self.capture.is_some_and(|c| c.cible == cible);
        let bas = source.map_or_else(|| "—".to_owned(), |b| format!("G27 #{b}"));
        let texte = RichText::new(format!("vJoy #{cible}\n{bas}"))
            .size(14.0)
            .color(if en_capture {
                theme::BG_BASE
            } else {
                theme::TEXT
            });
        let mut bouton = egui::Button::new(texte).min_size(egui::vec2(80.0, 40.0));
        bouton = if en_capture {
            bouton.fill(theme::GOLD)
        } else {
            bouton
                .fill(theme::BG_ELEVATED)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
        };
        if ui.add_enabled(pont.is_some(), bouton).clicked() {
            self.capture = if en_capture {
                None
            } else {
                Some(Capture {
                    cible,
                    boutons_init: pont.map_or(0, Pont::boutons_bruts),
                })
            };
        }
    }
}

/// Assigne le bouton physique G27 `bouton` (1-indexé) à la cible vJoy `cible`, en
/// garantissant **une seule** source par cible (l'ancienne source de `cible` est libérée).
fn assigner(remap: &mut RemapBoutons, bouton: u8, cible: u8) {
    let index = bouton as usize;
    if index == 0 || index > NB_BOUTONS_G27 {
        return;
    }
    for valeur in remap.iter_mut() {
        if *valeur == cible {
            *valeur = 0;
        }
    }
    remap[index] = cible;
}

#[cfg(test)]
mod tests {
    use super::{REMAP_DEFAUT, assigner};

    #[test]
    fn assigner_libere_l_ancienne_source() {
        let mut remap = REMAP_DEFAUT;
        // Bouton 7 → vJoy 2 par défaut ; on réassigne vJoy 2 au bouton 9.
        assigner(&mut remap, 9, 2);
        assert_eq!(remap[9], 2);
        assert_eq!(
            remap[7], 0,
            "l'ancienne source de la cible doit être libérée"
        );
    }

    #[test]
    fn assigner_ignore_un_bouton_hors_plage() {
        let mut remap = REMAP_DEFAUT;
        let avant = remap;
        assigner(&mut remap, 0, 5);
        assigner(&mut remap, 240, 5);
        assert_eq!(remap, avant, "un bouton hors plage ne modifie rien");
    }
}
