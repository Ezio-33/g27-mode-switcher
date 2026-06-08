//! Carte GUI « Pont vJoy » : détection des prérequis, démarrage/arrêt du pont
//! (feeder + masquage) et affichage de son état.
//!
//! Distincte de l'autocentrage. Le device vJoy est acquis **une seule fois** au
//! premier démarrage et conservé jusqu'à la fermeture de l'application : « Démarrer »
//! et « Arrêter » ne font que (ré)activer l'alimentation des axes et le masquage,
//! sans ré-acquérir vJoy. Le `Drop` du `Pont` (fermeture via `on_exit`) démasque le
//! G27 et libère le device vJoy.
//!
//! ⚠️ Le **démarrage initial** (qui acquiert vJoy) tourne sur un **thread auxiliaire**
//! et le résultat est récupéré via un canal : le thread GUI n'appelle jamais vJoy et
//! ne se bloque jamais — l'application reste réactive (et fermable) même si vJoy
//! tarde ou affiche une fenêtre.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};
use g27_mode_switcher::config::Config;
use g27_mode_switcher::pont::{self, Composant, ErreurPont, Pont, Prerequis};

use super::log::{LineKind, LogBuffer};
use super::theme;

/// Intervalle de re-détection des prérequis quand le pont est inactif.
const INTERVALLE_DETECTION: Duration = Duration::from_secs(3);

/// État de la carte « Pont vJoy ».
pub struct CartePont {
    /// `Some` dès le premier démarrage réussi (device vJoy acquis), jusqu'à la
    /// fermeture.
    pont: Option<Pont>,
    /// `Some` pendant un démarrage en cours (thread auxiliaire qui acquiert vJoy).
    demarrage: Option<Receiver<Result<Pont, ErreurPont>>>,
    prerequis: Prerequis,
    derniere_detection: Instant,
}

impl CartePont {
    /// Crée la carte et fait une première détection des prérequis.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pont: None,
            demarrage: None,
            prerequis: pont::detecter(),
            derniere_detection: Instant::now(),
        }
    }

    /// Libère le pont s'il existe (le `Drop` démasque le G27 et relâche vJoy) et
    /// abandonne tout démarrage en cours. Appelé à la fermeture (`on_exit`).
    pub fn arreter(&mut self) {
        self.pont = None;
        // Le receiver est lâché : si le thread auxiliaire finit, le `Pont` qu'il
        // produit est détruit chez lui (démasquage + RelinquishVJD).
        self.demarrage = None;
    }

    /// Affiche la carte et gère les interactions (démarrer/arrêter, sélecteur).
    pub fn afficher(&mut self, ui: &mut egui::Ui, config: &mut Config, log: &LogBuffer) {
        self.sonder_demarrage(ui, log);

        // Re-détection périodique tant qu'aucun pont n'est en jeu ET qu'un prérequis
        // manque (pour réagir à une installation de vJoy/HidHide). Une fois tout
        // disponible, on cesse de sonder.
        if self.pont.is_none()
            && self.demarrage.is_none()
            && !self.prerequis.tout_disponible()
            && self.derniere_detection.elapsed() >= INTERVALLE_DETECTION
        {
            self.prerequis = pont::detecter();
            self.derniere_detection = Instant::now();
        }

        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            etiquette_section(ui, "PONT VJOY");
            ui.add_space(10.0);
            if self.demarrage.is_some() {
                afficher_demarrage(ui);
            } else {
                match self.pont.as_ref().map(Pont::actif) {
                    Some(true) => self.afficher_marche(ui, config, log),
                    Some(false) => self.afficher_pause(ui, config, log),
                    None if self.prerequis.tout_disponible() => self.afficher_pret(ui, config, log),
                    None => afficher_aide(ui, &self.prerequis),
                }
            }
        });
    }

    /// Récupère (sans bloquer) le résultat d'un démarrage lancé en arrière-plan.
    fn sonder_demarrage(&mut self, ui: &egui::Ui, log: &LogBuffer) {
        let Some(rx) = self.demarrage.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(pont)) => {
                log.push(
                    LineKind::Success,
                    format!(
                        "Pont démarré — device vJoy #{}, G27 {}.",
                        pont.id_vjoy(),
                        etat_masquage(&pont),
                    ),
                );
                self.pont = Some(pont);
            }
            Ok(Err(erreur)) => log.push(
                LineKind::Error,
                format!("Démarrage du pont impossible : {erreur}"),
            ),
            Err(TryRecvError::Empty) => {
                // Toujours en cours : on garde le receiver et on redemande un rendu.
                self.demarrage = Some(rx);
                ui.ctx().request_repaint_after(Duration::from_millis(120));
            }
            Err(TryRecvError::Disconnected) => log.push(
                LineKind::Error,
                "Démarrage du pont interrompu (thread auxiliaire arrêté).",
            ),
        }
    }

    /// Pont en marche : état + réglages à chaud + bouton « Arrêter ».
    fn afficher_marche(&mut self, ui: &mut egui::Ui, config: &mut Config, log: &LogBuffer) {
        let (id, masque) = self
            .pont
            .as_ref()
            .map_or((0, false), |pont| (pont.id_vjoy(), pont.g27_masque()));
        ui.label(
            RichText::new(format!("Pont actif — device vJoy #{id}"))
                .color(theme::SUCCESS)
                .strong(),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "G27 réel {} au jeu",
                if masque { "masqué" } else { "visible" }
            ))
            .small()
            .color(theme::TEXT_MUTED),
        );
        controles_options(ui, config, self.pont.as_mut(), log);
        ui.add_space(12.0);
        let bouton =
            egui::Button::new(RichText::new("Arrêter le pont").color(theme::TEXT).strong())
                .fill(theme::BG_ELEVATED)
                .stroke(egui::Stroke::new(1.0, theme::BORDER_STRONG))
                .min_size(egui::vec2(ui.available_width(), 38.0));
        if ui.add(bouton).clicked()
            && let Some(pont) = self.pont.as_mut()
        {
            pont.suspendre(); // coupe l'alimentation + démasque, vJoy reste acquis.
            log.push(
                LineKind::Info,
                format!(
                    "Pont arrêté — G27 démasqué, alimentation coupée \
                     (device vJoy #{id} réservé jusqu'à la fermeture).",
                ),
            );
        }
    }

    /// Pont en pause : device vJoy réservé + réglages à chaud + bouton « Démarrer ».
    fn afficher_pause(&mut self, ui: &mut egui::Ui, config: &mut Config, log: &LogBuffer) {
        let id = self.pont.as_ref().map_or(0, Pont::id_vjoy);
        ui.label(
            RichText::new(format!("Pont arrêté — device vJoy #{id} réservé"))
                .color(theme::WARNING)
                .strong(),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Alimentation coupée, G27 visible. Le device vJoy reste réservé \
                 jusqu'à la fermeture de l'application.",
            )
            .small()
            .color(theme::TEXT_DIM),
        );
        controles_options(ui, config, self.pont.as_mut(), log);
        ui.add_space(12.0);
        if bouton_or(ui, "Démarrer le pont").clicked()
            && let Some(pont) = self.pont.as_mut()
        {
            match pont.reprendre() {
                Ok(()) => log.push(
                    LineKind::Success,
                    format!(
                        "Pont redémarré — device vJoy #{}, G27 {}.",
                        pont.id_vjoy(),
                        etat_masquage(pont),
                    ),
                ),
                Err(erreur) => log.push(
                    LineKind::Error,
                    format!("Reprise du pont impossible : {erreur}"),
                ),
            }
        }
    }

    /// Prérequis OK, pont jamais démarré : sélecteur de device + bouton Démarrer.
    ///
    /// Le démarrage (acquisition vJoy) part sur un **thread auxiliaire** ; on stocke
    /// le receiver et la carte passe en état « Démarrage… » sans bloquer la GUI.
    fn afficher_pret(&mut self, ui: &mut egui::Ui, config: &mut Config, log: &LogBuffer) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Device vJoy").size(14.0).color(theme::TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut id = config.pont.id_vjoy;
                if ui
                    .add(egui::DragValue::new(&mut id).range(1..=16).prefix("#"))
                    .changed()
                {
                    config.pont.id_vjoy = id;
                }
            });
        });
        controles_options(ui, config, None, log);
        ui.add_space(12.0);
        if bouton_or(ui, "Démarrer le pont").clicked() {
            let id = config.pont.id_vjoy;
            let masquer = config.pont.masquer_g27_au_demarrage;
            let options = pont::OptionsPont {
                couper_autocentrage: config.pont.couper_autocentrage_ffb,
                chapeau_clavier: config.pont.chapeau_vers_clavier,
                bouton_valider: config.pont.bouton_valider,
                bouton_retour: config.pont.bouton_retour,
            };
            let (tx, rx) = mpsc::channel();
            // Acquisition vJoy hors du thread GUI (cf. en-tête du module). Pont FFB
            // complet : le retour de force du jeu est recopié vers le G27.
            std::thread::spawn(move || {
                let _ = tx.send(Pont::demarrer_pont_ffb(id, masquer, options));
            });
            self.demarrage = Some(rx);
            log.push(LineKind::Info, "Démarrage du pont demandé\u{2026}");
        }
    }
}

/// Ligne de réglage d'un bouton clavier : libellé + sélecteur du numéro de bouton vJoy
/// (`0` = aucun). Renvoie la réponse du sélecteur (pour détecter un changement).
fn champ_bouton_clavier(ui: &mut egui::Ui, libelle: &str, numero: &mut u8) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(RichText::new(libelle).size(12.0).color(theme::TEXT_MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::DragValue::new(numero)
                    .range(0..=32)
                    .custom_formatter(|n, _| {
                        if n == 0.0 {
                            "aucun".to_owned()
                        } else {
                            format!("#{n}")
                        }
                    }),
            )
        })
        .inner
    })
    .inner
}

/// Bloc de réglages du pont (masquage + traductions clavier), affiché dans tous les
/// états. Si `pont` existe, les changements sont **appliqués à chaud** ; sinon ils ne
/// font que mettre à jour la config (lue au prochain démarrage).
fn controles_options(
    ui: &mut egui::Ui,
    config: &mut Config,
    mut pont_actif: Option<&mut Pont>,
    log: &LogBuffer,
) {
    ui.add_space(8.0);
    let mut masquer = config.pont.masquer_g27_au_demarrage;
    if ui
        .checkbox(
            &mut masquer,
            RichText::new("Masquer le G27 au jeu")
                .size(13.0)
                .color(theme::TEXT),
        )
        .on_hover_text(
            "Masqué : le jeu ne voit que la manette vJoy (FFB OK). Décoché : le vrai G27 \
             reste visible (son D-pad navigue les menus Forza, mais le FFB est alors envoyé \
             au G27 et perdu). Pour FFB + navigation : masqué + « D-pad → flèches clavier ».",
        )
        .changed()
    {
        config.pont.masquer_g27_au_demarrage = masquer;
        if let Some(pont) = pont_actif.as_deref_mut()
            && let Err(erreur) = pont.definir_masquer(masquer)
        {
            log.push(LineKind::Error, format!("Masquage impossible : {erreur}"));
        }
    }

    ui.add_space(6.0);
    let mut clavier_change = false;
    let mut chapeau = config.pont.chapeau_vers_clavier;
    if ui
        .checkbox(
            &mut chapeau,
            RichText::new("D-pad → flèches clavier")
                .size(13.0)
                .color(theme::TEXT),
        )
        .on_hover_text(
            "Traduit la croix directionnelle en flèches ↑↓←→ du clavier (navigation des \
             menus/map Forza quand le G27 est masqué). Frappes clavier globales.",
        )
        .changed()
    {
        config.pont.chapeau_vers_clavier = chapeau;
        clavier_change = true;
    }
    if config.pont.chapeau_vers_clavier {
        ui.add_space(4.0);
        if champ_bouton_clavier(ui, "Valider → Entrée", &mut config.pont.bouton_valider).changed()
        {
            clavier_change = true;
        }
        if champ_bouton_clavier(ui, "Retour → Échap", &mut config.pont.bouton_retour).changed() {
            clavier_change = true;
        }
    }
    if clavier_change && let Some(pont) = pont_actif {
        pont.reconfigurer_clavier(pont::OptionsClavier {
            chapeau: config.pont.chapeau_vers_clavier,
            valider: config.pont.bouton_valider,
            retour: config.pont.bouton_retour,
        });
    }
}

/// Bouton d'action doré pleine largeur (style « Démarrer »).
fn bouton_or(ui: &mut egui::Ui, texte: &str) -> egui::Response {
    let bouton = egui::Button::new(RichText::new(texte).color(theme::BG_BASE).strong())
        .fill(theme::GOLD)
        .min_size(egui::vec2(ui.available_width(), 40.0));
    ui.add(bouton)
}

/// Démarrage en cours : message d'attente (la GUI reste réactive).
fn afficher_demarrage(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.add_space(6.0);
        ui.label(
            RichText::new("Démarrage du pont\u{2026}")
                .color(theme::TEXT)
                .strong(),
        );
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new("Acquisition du device vJoy en cours.")
            .small()
            .color(theme::TEXT_DIM),
    );
}

/// Libellé « masqué »/« visible » selon l'état de masquage du pont.
fn etat_masquage(pont: &Pont) -> &'static str {
    if pont.g27_masque() {
        "masqué"
    } else {
        "visible"
    }
}

/// Prérequis manquants : message d'aide concis (quoi installer).
fn afficher_aide(ui: &mut egui::Ui, prerequis: &Prerequis) {
    ui.label(
        RichText::new("Le pont d'entrée nécessite vJoy (x64) et HidHide.")
            .color(theme::WARNING)
            .strong(),
    );
    ui.add_space(8.0);
    ligne_composant(ui, "vJoy", &prerequis.vjoy);
    ligne_composant(ui, "HidHide", &prerequis.hidhide);
    ui.add_space(8.0);
    ui.label(
        RichText::new("Installez les composants manquants (x64) ; l'état se met à jour tout seul.")
            .small()
            .color(theme::TEXT_DIM),
    );
}

/// Une ligne d'état de composant (vert si disponible, sinon raison abrégée).
fn ligne_composant(ui: &mut egui::Ui, nom: &str, composant: &Composant) {
    match composant.raison() {
        None => {
            ui.label(
                RichText::new(format!("{nom} : disponible"))
                    .small()
                    .color(theme::SUCCESS),
            );
        }
        Some(raison) => {
            let abrege = raison.lines().next().unwrap_or(raison);
            ui.label(
                RichText::new(format!("{nom} : {abrege}"))
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        }
    }
}

/// Petit label de section (majuscules, atténué), identique aux autres cartes.
fn etiquette_section(ui: &mut egui::Ui, texte: &str) {
    ui.label(RichText::new(texte).small().strong().color(theme::TEXT_DIM));
}
