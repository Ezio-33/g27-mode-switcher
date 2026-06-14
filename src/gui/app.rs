//! Application graphique : mise en page en cartes et contrôles du G27.

use std::time::Duration;

use eframe::egui::{self, RichText, Stroke};
use g27_mode_switcher::config::{Config, ModeJeu, ModeSouhaite};
use g27_mode_switcher::device::{Command, DeviceSession, Event, OpError, OpKind, OpReport, Status};

use super::carte_forza::CarteForza;
use super::carte_pont::CartePont;
use super::log::{self, LineKind, LogBuffer};
use super::theme;

const URL_SITE: &str = "https://la-confrerie-des-ombres.vercel.app/index.html";
const URL_DISCORD: &str = "https://discord.gg/zckGmdg";
const URL_TIP: &str = "https://streamelements.com/ezio_33/tip";
const AUTHOR: &str = "Samuel.V (Ezio_33)";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Cadence de rafraîchissement pour refléter le statut en temps réel.
const REPAINT_INTERVAL: Duration = Duration::from_millis(350);

/// Bornes valides de l'angle de rotation (degrés), cf. `lg4ff_set_range_g25`.
const RANGE_MIN: u16 = 40;
const RANGE_MAX: u16 = 900;

/// Préréglages d'angle proposés (libellé, degrés).
const RANGE_PRESETS: [(&str, u16); 4] = [
    ("F1 360°", 360),
    ("GT 540°", 540),
    ("Rallye 720°", 720),
    ("Voiture/Camion 900°", 900),
];

/// État de l'interface graphique.
// Plusieurs drapeaux d'UI indépendants (fenêtres ouvertes, états de session) : des
// `bool` distincts sont plus lisibles qu'un type bitflags artificiel.
#[allow(clippy::struct_excessive_bools)]
pub struct App {
    session: DeviceSession,
    log: LogBuffer,
    status: Status,
    range_deg: u16,
    autocenter_disabled: bool,
    about_open: bool,
    conditions_open: bool,
    config: Config,
    carte_pont: CartePont,
    carte_forza: CarteForza,
    /// Garde une seule restauration auto du mode par session (au premier statut connu).
    auto_restore_fait: bool,
    /// Dernier état « un pont FFB gère l'autocentrage » signalé à la session (anti-spam).
    pont_ffb_actif: bool,
}

impl App {
    /// Crée l'application et démarre la session matérielle, en initialisant les
    /// contrôles depuis la configuration chargée.
    #[must_use]
    pub fn new(config: Config, log: LogBuffer) -> Self {
        Self {
            session: DeviceSession::spawn(),
            log,
            status: Status::Absent,
            range_deg: config.volant.angle_par_defaut,
            autocenter_disabled: config.volant.desactiver_autocentrage_au_switch,
            about_open: false,
            conditions_open: false,
            config,
            carte_pont: CartePont::new(),
            carte_forza: CarteForza::new(),
            auto_restore_fait: false,
            pont_ffb_actif: false,
        }
    }

    /// Signale à la session si un **pont FFB** (vJoy ou Forza) gère l'autocentrage, pour
    /// qu'elle suspende/reprenne son rafraîchissement périodique. Envoyé seulement sur
    /// changement d'état (la session est sinon inutilement sollicitée).
    fn synchroniser_pont_ffb(&mut self) {
        let actif = self.carte_pont.pont_alimente() || self.carte_forza.est_actif();
        if actif != self.pont_ffb_actif {
            self.pont_ffb_actif = actif;
            self.session.send(Command::PontFfbActif(actif));
        }
    }

    /// Envoie la commande de bascule en mode natif avec les réglages courants.
    fn envoyer_bascule(&self) {
        self.session.send(Command::Switch {
            apply_range: self.config.volant.appliquer_angle_au_switch,
            range_degrees: self.range_deg,
            disable_autocenter: self.config.volant.desactiver_autocentrage_au_switch,
        });
    }

    /// Restaure le mode souhaité au **premier statut matériel connu** (une seule fois
    /// par session) : si l'utilisateur était en natif et que le volant a redémarré en
    /// compatibilité (cycle USB), on rebascule automatiquement. S'il préférait la
    /// compatibilité, on n'y touche pas.
    fn restaurer_mode_si_besoin(&mut self) {
        if self.auto_restore_fait || self.status == Status::Absent {
            return;
        }
        self.auto_restore_fait = true; // décision prise dès qu'un mode réel est connu.
        if self.status == Status::Compatibility
            && self.config.volant.mode_souhaite == ModeSouhaite::Natif
        {
            self.envoyer_bascule();
            self.log.push(
                LineKind::Info,
                "Mode natif mémorisé : bascule automatique au démarrage\u{2026}",
            );
        }
    }

    /// Mémorise la géométrie courante de la fenêtre dans la configuration.
    fn capturer_geometrie(&mut self, ctx: &egui::Context) {
        ctx.input(|entree| {
            let viewport = entree.viewport();
            if let Some(rect) = viewport.inner_rect {
                self.config.fenetre.largeur = rect.width();
                self.config.fenetre.hauteur = rect.height();
            }
            if let Some(rect) = viewport.outer_rect {
                self.config.fenetre.pos_x = Some(rect.min.x);
                self.config.fenetre.pos_y = Some(rect.min.y);
            }
        });
    }

    /// Reporte les réglages courants dans la configuration et l'enregistre.
    fn persister_reglages(&mut self) {
        self.config.volant.angle_par_defaut = self.range_deg;
        self.config.volant.desactiver_autocentrage_au_switch = self.autocenter_disabled;
        self.sauvegarder_config();
    }

    /// Enregistre la configuration ; un échec est journalisé sans bloquer l'app.
    fn sauvegarder_config(&self) {
        if let Err(erreur) = self.config.enregistrer() {
            tracing::warn!(%erreur, "Échec d'enregistrement de la configuration.");
        }
    }

    /// Draine les événements de la session vers le journal et programme un repaint.
    fn poll(&mut self, ctx: &egui::Context) {
        for event in self.session.drain_events() {
            match event {
                Event::Status(status) if status != self.status => {
                    self.status = status;
                    self.log.push(LineKind::Info, status_line(status));
                    self.restaurer_mode_si_besoin();
                }
                Event::Status(_) => {}
                Event::Op(report) => {
                    let (kind, text) = journal_for_op(report);
                    self.log.push(kind, text);
                }
            }
        }
        self.capturer_geometrie(ctx);
        ctx.request_repaint_after(REPAINT_INTERVAL);
    }

    /// En-tête : titre Cinzel + pastille de statut, tous deux centrés.
    fn header(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("G27 MODE SWITCHER")
                    .family(theme::heading_family())
                    .size(25.0)
                    .color(theme::GOLD),
            );
        });
        ui.add_space(8.0);

        let (color, text) = match self.status {
            Status::Native => (
                theme::SUCCESS,
                "Mode natif — 900° — retour de force complet",
            ),
            Status::Compatibility => (theme::WARNING, "Mode compatibilité (200°)"),
            Status::Absent => (theme::TEXT_DIM, "Aucun G27 détecté"),
        };
        centered_row(ui, "pastille_statut", |ui| {
            theme::pill_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 9.0;
                    status_dot(ui, color);
                    ui.label(RichText::new(text).color(color).strong());
                });
            });
        });
    }

    /// Carte « Mode du volant » : bascule (compat) ou état natif.
    fn card_mode(&mut self, ui: &mut egui::Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_label(ui, "🎮  MODE DU VOLANT");
            ui.add_space(8.0);
            match self.status {
                Status::Compatibility => {
                    let button = egui::Button::new(
                        RichText::new("🔓  Basculer en mode 900° natif")
                            .color(theme::BG_BASE)
                            .strong(),
                    )
                    .fill(theme::GOLD)
                    .min_size(egui::vec2(ui.available_width(), 40.0));
                    if ui.add(button).clicked() {
                        // Mémorise le choix « natif » pour le restaurer aux prochains
                        // démarrages (le firmware repart en compat à chaque cycle USB).
                        self.config.volant.mode_souhaite = ModeSouhaite::Natif;
                        self.sauvegarder_config();
                        self.envoyer_bascule();
                        self.log.push(LineKind::Info, "Bascule demandée\u{2026}");
                    }
                }
                Status::Native => {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Volant en mode natif G27 (C29B)")
                                .color(theme::TEXT_MUTED)
                                .size(15.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let revert = egui::Button::new(
                                RichText::new("Revenir en compatibilité")
                                    .small()
                                    .color(theme::TEXT_MUTED),
                            )
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
                            .corner_radius(egui::CornerRadius::same(7));
                            if ui.add(revert).clicked() {
                                // Mémorise le choix « compatibilité » : plus de bascule
                                // auto au prochain démarrage.
                                self.config.volant.mode_souhaite = ModeSouhaite::Compatibilite;
                                self.sauvegarder_config();
                                self.log.push(
                                    LineKind::Info,
                                    "Mode compatibilité mémorisé. Débranchez puis rebranchez le volant pour y revenir maintenant.",
                                );
                            }
                        });
                    });
                }
                Status::Absent => {
                    ui.label(
                        RichText::new("Branchez un Logitech G27 pour commencer.")
                            .color(theme::TEXT_MUTED),
                    );
                }
            }
        });
    }

    /// Carte « Angle de rotation » : slider + préréglages (actif si natif).
    fn card_angle(&mut self, ui: &mut egui::Ui) {
        let is_native = self.status == Status::Native;
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_label(ui, "🔄  ANGLE DE ROTATION");
            ui.add_space(8.0);
            // `commit` : l'utilisateur a fini de régler l'angle (relâché slider,
            // validé la saisie, ou choisi un préréglage) → on envoie et on
            // persiste une seule fois (anti-spam disque/HID).
            let mut commit = false;
            ui.add_enabled_ui(is_native, |ui| {
                ui.horizontal(|ui| {
                    // La valeur éditable (Cinzel or) est collée à droite ; le
                    // slider custom occupe tout le reste de la largeur.
                    const VALUE_WIDTH: f32 = 72.0;
                    let slider_w = (ui.available_width() - VALUE_WIDTH).max(80.0);
                    let slider = angle_slider(ui, &mut self.range_deg, slider_w);
                    if slider.drag_stopped() || slider.clicked() {
                        commit = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Champ de saisie : clic-glisser ou double-clic pour taper
                        // un angle précis, stylé en or façon titre.
                        let s = ui.style_mut();
                        s.visuals.override_text_color = Some(theme::GOLD);
                        s.text_styles.insert(
                            egui::TextStyle::Button,
                            egui::FontId::new(18.0, theme::heading_family()),
                        );
                        s.visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                        s.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
                        let drag = egui::DragValue::new(&mut self.range_deg)
                            .range(RANGE_MIN..=RANGE_MAX)
                            .suffix("\u{b0}")
                            .speed(1.0);
                        let response = ui.add(drag);
                        if response.drag_stopped() || response.lost_focus() {
                            commit = true;
                        }
                    });
                });
                ui.add_space(8.0);
                // Enroulé : à la taille de police relevée, les préréglages passent à la
                // ligne au lieu de déborder à largeur réduite.
                ui.horizontal_wrapped(|ui| {
                    for (label, degrees) in RANGE_PRESETS {
                        let active = self.range_deg == degrees;
                        let (fill, text_color) = if active {
                            (theme::GOLD, theme::BG_BASE)
                        } else {
                            (theme::BG_ELEVATED, theme::TEXT_MUTED)
                        };
                        let preset =
                            egui::Button::new(RichText::new(label).small().color(text_color))
                                .fill(fill)
                                .stroke(Stroke::new(1.0, theme::BORDER_STRONG));
                        if ui.add(preset).clicked() {
                            self.range_deg = degrees;
                            commit = true;
                        }
                    }
                });
            });
            if commit {
                self.session.send(Command::SetRange(self.range_deg));
                self.persister_reglages();
            }
        });
    }

    /// Carte « Autocentrage » : ressort de rappel matériel du G27 (mode natif).
    ///
    /// Distincte du pont vJoy. Le nom « Retour de force » est réservé au vrai FFB
    /// dynamique (Phase 5).
    fn card_autocentrage(&mut self, ui: &mut egui::Ui) {
        let is_native = self.status == Status::Native;
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_label(ui, "🎯  AUTOCENTRAGE");
            ui.add_space(10.0);

            // Désactivation et réactivation (pleine force) sont fonctionnelles en direct.
            let autocenter_on = !self.autocenter_disabled;
            let sub = if autocenter_on {
                "Actif — seule force de centrage sans retour de force dynamique"
            } else {
                "Désactivé — le jeu gère le retour de force"
            };
            let toggled = control_row(ui, "Autocentrage matériel", sub, |ui| {
                toggle_switch(ui, autocenter_on, is_native)
            });
            if toggled.clicked() {
                self.autocenter_disabled = !self.autocenter_disabled;
                self.session.send(Command::SetAutocenter {
                    enable: !self.autocenter_disabled,
                });
                self.persister_reglages();
            }
        });
    }

    /// Carte « Journal » : zone scrollable des messages.
    fn card_journal(&self, ui: &mut egui::Ui) {
        theme::journal_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(ui.available_height());
            log::render(ui, &self.log);
        });
    }

    /// Pied de page : liens centrés + « À propos » (la bordure haute est fournie
    /// par le séparateur du `TopBottomPanel`).
    fn footer(&mut self, ui: &mut egui::Ui) {
        let mut about_clicked = false;
        let mut conditions_clicked = false;
        centered_row(ui, "liens_pied", |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            footer_link(ui, "Site", URL_SITE);
            footer_sep(ui);
            footer_link(ui, "Discord", URL_DISCORD);
            footer_sep(ui);
            footer_link(ui, "Soutenir", URL_TIP);
            footer_sep(ui);
            // Liens (pas des boutons) pour avoir l'effet de survol identique aux autres.
            if ui.link(footer_texte("À propos")).clicked() {
                about_clicked = true;
            }
            footer_sep(ui);
            if ui.link(footer_texte("Conditions")).clicked() {
                conditions_clicked = true;
            }
        });
        if about_clicked {
            self.about_open = true;
        }
        if conditions_clicked {
            self.conditions_open = true;
        }
    }

    /// Barre de menus supérieure : menu « Jeux » (choix du mode) + libellé du mode actif.
    fn menu_jeux(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(
                RichText::new("Jeux").size(16.0).strong().color(theme::TEXT),
                |ui| {
                    self.choix_mode(ui, ModeJeu::General, "Général — pont vJoy (tous jeux)");
                    self.choix_mode(ui, ModeJeu::Forza, "Forza Horizon — télémétrie");
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(libelle_mode(self.config.mode_jeu))
                        .size(15.0)
                        .strong()
                        .color(theme::GOLD_DARK),
                );
            });
        });
    }

    /// Entrée de menu pour un mode de jeu ; bascule (et arrête l'autre pont) si choisi.
    fn choix_mode(&mut self, ui: &mut egui::Ui, mode: ModeJeu, libelle: &str) {
        if ui.radio(self.config.mode_jeu == mode, libelle).clicked() {
            if self.config.mode_jeu != mode {
                self.changer_mode(mode);
            }
            ui.close();
        }
    }

    /// Change le mode de jeu : arrête le pont du mode quitté, mémorise et journalise.
    fn changer_mode(&mut self, mode: ModeJeu) {
        match self.config.mode_jeu {
            ModeJeu::General => self.carte_pont.arreter(),
            ModeJeu::Forza => self.carte_forza.arreter(),
        }
        self.config.mode_jeu = mode;
        self.sauvegarder_config();
        self.log.push(
            LineKind::Info,
            format!("Mode de jeu : {}.", libelle_mode(mode)),
        );
    }

    /// Fenêtre « À propos ».
    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let mut open = self.about_open;
        // Cadre explicite : fond carte, bordure forte, titre or — on ne laisse
        // pas le style de fenêtre par défaut (rendu clair illisible).
        let frame = egui::Frame::window(&ctx.global_style())
            .fill(theme::BG_CARD)
            .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
            .inner_margin(egui::Margin::same(16));
        egui::Window::new(RichText::new("À propos").color(theme::GOLD).strong())
            .collapsible(false)
            .resizable(false)
            .frame(frame)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("G27 Mode Switcher")
                        .family(theme::heading_family())
                        .size(20.0)
                        .color(theme::GOLD),
                );
                ui.label(RichText::new(format!("Version {APP_VERSION}")).color(theme::TEXT_MUTED));
                ui.add_space(8.0);
                ui.label(RichText::new(format!("Auteur : {AUTHOR}")).color(theme::TEXT));
                ui.hyperlink_to(
                    RichText::new("Site — la Confrérie des Ombres").color(theme::GOLD),
                    URL_SITE,
                );
                ui.add_space(8.0);
                ui.label(RichText::new("Licence : MIT").color(theme::TEXT_MUTED));
                ui.label(
                    RichText::new("Police Cinzel : SIL Open Font License 1.1")
                        .color(theme::TEXT_MUTED),
                );
            });
        self.about_open = open;
    }

    /// Fenêtre « Conditions d'utilisation » (licence, attribution, garantie, responsabilité).
    fn conditions_window(&mut self, ctx: &egui::Context) {
        if !self.conditions_open {
            return;
        }
        let mut open = self.conditions_open;
        let frame = egui::Frame::window(&ctx.global_style())
            .fill(theme::BG_CARD)
            .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
            .inner_margin(egui::Margin::same(16));
        egui::Window::new(
            RichText::new("Conditions d'utilisation")
                .color(theme::GOLD)
                .strong(),
        )
        .collapsible(false)
        .resizable(false)
        .frame(frame)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(440.0)
        .show(ctx, |ui| {
            ui.set_max_width(440.0);
            conditions_paragraphe(
                ui,
                "Licence",
                "G27 Mode Switcher est un logiciel libre publié sous licence MIT.",
            );
            conditions_paragraphe(
                ui,
                "Attribution obligatoire",
                &format!(
                    "Toute réutilisation, modification ou redistribution du code — total ou \
                     partiel — doit créditer l'auteur ({AUTHOR}) et mentionner son site."
                ),
            );
            conditions_paragraphe(
                ui,
                "Aucune garantie",
                "Le logiciel est fourni « EN L'ÉTAT », sans aucune garantie, expresse ou \
                 implicite (qualité marchande, adéquation à un usage particulier, absence de \
                 défaut ou d'interruption).",
            );
            conditions_paragraphe(
                ui,
                "Limitation de responsabilité",
                "L'utilisation se fait à vos propres risques. L'auteur ne peut en aucun cas \
                 être tenu responsable d'un quelconque dommage direct ou indirect, \
                 dysfonctionnement, perte de données ou problème matériel résultant de \
                 l'utilisation du logiciel.",
            );
            ui.add_space(10.0);
            ui.hyperlink_to(
                RichText::new("Site — la Confrérie des Ombres").color(theme::GOLD),
                URL_SITE,
            );
        });
        self.conditions_open = open;
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::BG_BASE.to_normalized_gamma_f32()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Sûreté : on arrête le pont AVANT tout le reste pour garantir le
        // démasquage du G27 et la libération du device vJoy à la fermeture (croix).
        // Le `Drop` du champ `carte_pont` en serait un dernier filet, mais on le
        // fait ici explicitement et tôt.
        self.carte_pont.arreter();
        self.carte_forza.arreter(); // remet le volant au neutre (mode Forza).
        // Puis on persiste la dernière géométrie de fenêtre et les réglages.
        self.persister_reglages();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll(ui.ctx());

        // Barre de menus en haut : choix du mode de jeu (« Jeux »).
        egui::Panel::top("menu_jeux")
            .frame(
                egui::Frame::default()
                    .fill(theme::BG_PANEL)
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show_separator_line(true)
            .show_inside(ui, |ui| {
                self.menu_jeux(ui);
            });

        // Pied de page ancré en bas (réservé avant le panneau central pour rester
        // toujours visible), fond panneau + bordure haute comme la maquette.
        egui::Panel::bottom("pied_de_page")
            .frame(
                egui::Frame::default()
                    .fill(theme::BG_PANEL)
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show_separator_line(true)
            .show_inside(ui, |ui| {
                self.footer(ui);
            });

        // Journal ancré en bas (au-dessus du pied de page), **redimensionnable** : ses
        // lignes défilent en interne, et il ne mange pas la place des cartes défilables.
        egui::Panel::bottom("journal")
            .resizable(true)
            .default_size(150.0)
            .frame(egui::Frame::default().inner_margin(egui::Margin {
                left: 16,
                right: 16,
                top: 6,
                bottom: 8,
            }))
            .show_separator_line(true)
            .show_inside(ui, |ui| {
                self.card_journal(ui);
            });

        // Reste de la fenêtre : en-tête pleine largeur (toujours visible), puis cartes
        // **défilables** en retrait latéral (16 px) pour se détacher.
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| {
                ui.add_space(8.0);
                self.header(ui);
                ui.add_space(10.0);
                ui.separator();

                // Cartes défilables : si la fenêtre est trop petite, une barre de
                // défilement apparaît au lieu de couper le contenu.
                let sortie = egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Frame::NONE
                            .inner_margin(egui::Margin {
                                left: 16,
                                right: 16,
                                top: 12,
                                bottom: 12,
                            })
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                self.card_mode(ui);
                                ui.add_space(12.0);
                                self.card_angle(ui);
                                ui.add_space(12.0);
                                self.card_autocentrage(ui);
                                ui.add_space(12.0);
                                match self.config.mode_jeu {
                                    ModeJeu::General => {
                                        self.carte_pont.afficher(ui, &mut self.config, &self.log);
                                    }
                                    ModeJeu::Forza => {
                                        self.carte_forza.afficher(ui, &mut self.config, &self.log);
                                    }
                                }
                            });
                    });
                indicateurs_defilement(ui, &sortie);
            });

        // Tient la session au courant : un pont FFB gère-t-il l'autocentrage ? (sinon la
        // session le rafraîchit elle-même pour ne pas le laisser relâcher).
        self.synchroniser_pont_ffb();

        self.about_window(ui.ctx());
        self.conditions_window(ui.ctx());
    }
}

/// Dessine des **chevrons de défilement** discrets (▲ en haut / ▼ en bas), **à droite**
/// (juste avant la barre de défilement), qui **pulsent** doucement pour attirer l'œil —
/// affichés **uniquement** s'il reste du contenu dans cette direction.
fn indicateurs_defilement<R>(ui: &egui::Ui, sortie: &egui::scroll_area::ScrollAreaOutput<R>) {
    let vue = sortie.inner_rect;
    let decalage = sortie.state.offset.y;
    let haut = decalage > 1.0;
    let bas = decalage + vue.height() < sortie.content_size.y - 1.0;
    if !haut && !bas {
        return;
    }
    let intensite = pulsation(ui.ctx());
    ui.ctx().request_repaint_after(Duration::from_millis(40)); // anime la pulsation
    let x = vue.right() - 16.0; // collé à droite, juste avant la barre de défilement
    let painter = ui.painter();
    if haut {
        chevron_defilement(painter, egui::pos2(x, vue.top() + 12.0), false, intensite);
    }
    if bas {
        chevron_defilement(painter, egui::pos2(x, vue.bottom() - 12.0), true, intensite);
    }
}

/// Facteur de pulsation (≈ `0.45..1.0`) dérivé du temps, pour faire « respirer » un repère.
fn pulsation(ctx: &egui::Context) -> f32 {
    let t = ctx.input(|entree| entree.time);
    // `(t * 3.2).sin()` ∈ [−1, 1] → résultat borné ; la conversion f64→f32 est sûre.
    #[allow(clippy::cast_possible_truncation)]
    {
        (0.45 + 0.55 * (0.5 + 0.5 * (t * 3.2).sin())) as f32
    }
}

/// Petit chevron doré (alpha modulé par `intensite`) sur un halo sombre, visible sur tout
/// fond. `vers_le_bas` → ▼, sinon ▲.
fn chevron_defilement(
    painter: &egui::Painter,
    centre: egui::Pos2,
    vers_le_bas: bool,
    intensite: f32,
) {
    const DEMI_L: f32 = 6.0;
    const DEMI_H: f32 = 4.0;
    painter.circle_filled(centre, 10.0, egui::Color32::from_black_alpha(150));
    let dir = if vers_le_bas { 1.0 } else { -1.0 };
    let gauche = egui::pos2(centre.x - DEMI_L, centre.y - dir * DEMI_H);
    let pointe = egui::pos2(centre.x, centre.y + dir * DEMI_H);
    let droite = egui::pos2(centre.x + DEMI_L, centre.y - dir * DEMI_H);
    let crayon = egui::Stroke::new(2.0, theme::GOLD.gamma_multiply(intensite));
    painter.line_segment([gauche, pointe], crayon);
    painter.line_segment([pointe, droite], crayon);
}

/// Pastille ronde pleine dessinée au painter (le glyphe « ● » manque aux polices).
fn status_dot(ui: &mut egui::Ui, color: egui::Color32) {
    const DIAMETER: f32 = 9.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(DIAMETER, DIAMETER), egui::Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), DIAMETER / 2.0, color);
}

/// Interrupteur à bascule dessiné au painter (egui n'en a pas nativement).
///
/// `enabled == false` le grise et le rend non cliquable. Le `Response` renvoyé
/// permet à l'appelant de réagir au clic.
fn toggle_switch(ui: &mut egui::Ui, on: bool, enabled: bool) -> egui::Response {
    const SIZE: egui::Vec2 = egui::vec2(42.0, 23.0);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(SIZE, sense);

    let how_on = ui.ctx().animate_bool(response.id, on);
    let (track, track_stroke, knob) = if enabled && on {
        (theme::GOLD, theme::GOLD, theme::BG_BASE)
    } else {
        (theme::BG_ELEVATED, theme::BORDER_STRONG, theme::TEXT_DIM)
    };

    let painter = ui.painter();
    painter.rect(
        rect,
        egui::CornerRadius::same(12),
        track,
        Stroke::new(1.0, track_stroke),
        egui::StrokeKind::Inside,
    );
    let knob_radius = rect.height() / 2.0 - 3.0;
    let min_x = rect.left() + 3.0 + knob_radius;
    let max_x = rect.right() - 3.0 - knob_radius;
    let center = egui::pos2(egui::lerp(min_x..=max_x, how_on), rect.center().y);
    painter.circle_filled(center, knob_radius, knob);

    response
}

/// Slider d'angle : s'appuie sur le slider doré partagé ([`super::widgets::curseur_dore`])
/// en convertissant l'angle (`RANGE_MIN..=RANGE_MAX`) en fraction `0..1` et inversement.
fn angle_slider(ui: &mut egui::Ui, value: &mut u16, width: f32) -> egui::Response {
    let span = f32::from(RANGE_MAX - RANGE_MIN);
    let mut fraction = (f32::from(*value) - f32::from(RANGE_MIN)) / span;
    let response = super::widgets::curseur_dore(ui, &mut fraction, width, 22.0, 8.0);
    if response.changed() {
        // `fraction * span` ∈ [0, 860] : la conversion ne peut ni tronquer ni perdre de signe.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let pas = (fraction * span).round() as u16;
        *value = RANGE_MIN + pas;
    }
    response
}

/// Centre horizontalement une rangée de widgets dans la largeur disponible.
///
/// `vertical_centered` ne centre pas un scope imbriqué (`horizontal`/`Frame`) :
/// egui étend le cadre enfant à toute la largeur (cf. `layout.rs`) et le contenu
/// reste collé à gauche. On mémorise donc la largeur réelle du contenu d'une
/// frame sur l'autre (mémoire egui) et on décale le contenu de la moitié de
/// l'espace restant. Une passe de dimensionnement avancerait le curseur vertical
/// (`scope_dyn` → `advance_cursor_after_rect`) et décalerait la mise en page.
fn centered_row(ui: &mut egui::Ui, id_salt: &str, mut add: impl FnMut(&mut egui::Ui)) {
    let id = ui.make_persistent_id(id_salt);
    let previous: f32 = ui.data(|data| data.get_temp(id)).unwrap_or(0.0);
    let offset = ((ui.available_width() - previous) * 0.5).max(0.0);
    let rect = ui
        .horizontal(|ui| {
            ui.add_space(offset);
            add(ui);
        })
        .response
        .rect;
    ui.data_mut(|data| data.insert_temp(id, rect.width() - offset));
}

/// Ligne d'un contrôle : titre + sous-texte à gauche, interrupteur à droite.
fn control_row(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    add_toggle: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> egui::Response {
    let mut toggle = None;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(16.0).color(theme::TEXT));
            ui.add_space(2.0);
            ui.label(RichText::new(subtitle).small().color(theme::TEXT_MUTED));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            toggle = Some(add_toggle(ui));
        });
    });
    toggle.expect("control_row : interrupteur non fourni")
}

/// Label de section (majuscules, atténué) — taille relevée pour la lisibilité.
fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(15.0)
            .strong()
            .color(theme::TEXT_DIM),
    );
}

/// Paragraphe des conditions d'utilisation : titre fort + texte atténué (avec retour
/// à la ligne automatique dans la largeur de la fenêtre).
fn conditions_paragraphe(ui: &mut egui::Ui, titre: &str, texte: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(titre).size(17.0).strong().color(theme::GOLD));
    ui.add_space(2.0);
    // Corps en taille normale (pas `small`) pour la lisibilité (texte légal).
    ui.label(RichText::new(texte).color(theme::TEXT));
}

/// Taille de police du pied de page (relevée pour la lisibilité).
const FOOTER_SIZE: f32 = 15.0;

/// Style commun du texte des liens de pied de page (taille relevée, gras, or atténué).
fn footer_texte(label: &str) -> RichText {
    RichText::new(label)
        .size(FOOTER_SIZE)
        .strong()
        .color(theme::GOLD_DARK)
}

/// Lien de pied de page (or atténué, gras, avec effet de survol).
fn footer_link(ui: &mut egui::Ui, label: &str, url: &str) {
    ui.hyperlink_to(footer_texte(label), url);
}

/// Séparateur discret entre deux liens (même taille, non gras).
fn footer_sep(ui: &mut egui::Ui) {
    ui.label(
        RichText::new("\u{b7}")
            .size(FOOTER_SIZE)
            .color(theme::TEXT_DIM),
    );
}

/// Libellé court du mode de jeu (menu + indicateur).
fn libelle_mode(mode: ModeJeu) -> &'static str {
    match mode {
        ModeJeu::General => "Mode Général (vJoy)",
        ModeJeu::Forza => "Mode Forza (télémétrie)",
    }
}

/// Ligne de journal pour un changement de statut.
fn status_line(status: Status) -> &'static str {
    match status {
        Status::Native => "G27 détecté en mode natif (C29B).",
        Status::Compatibility => "G27 détecté en mode compatibilité (C294).",
        Status::Absent => "G27 déconnecté.",
    }
}

/// Convertit le résultat d'une opération en ligne de journal (FR).
fn journal_for_op(report: OpReport) -> (LineKind, String) {
    match (report.kind, report.result) {
        (OpKind::Switch, Ok(())) => (
            LineKind::Success,
            "Bascule envoyée — le volant se reconnecte en mode natif.".to_owned(),
        ),
        (OpKind::Range(degrees), Ok(())) => (
            LineKind::Success,
            format!("Angle de rotation réglé sur {degrees}°."),
        ),
        (OpKind::DisableAutocenter, Ok(())) => (
            LineKind::Success,
            "Autocentrage matériel désactivé.".to_owned(),
        ),
        (OpKind::EnableAutocenter, Ok(())) => (
            LineKind::Success,
            "Autocentrage matériel réactivé (pleine force).".to_owned(),
        ),
        (kind, Err(error)) => (
            LineKind::Error,
            format!("{} : {}", op_label(kind), op_error_fr(error)),
        ),
    }
}

/// Libellé court d'une opération pour les messages d'erreur.
fn op_label(kind: OpKind) -> &'static str {
    match kind {
        OpKind::Switch => "Bascule",
        OpKind::Range(_) => "Angle",
        OpKind::DisableAutocenter | OpKind::EnableAutocenter => "Autocentrage",
    }
}

/// Traduit une cause d'échec en message destiné à l'utilisateur.
fn op_error_fr(error: OpError) -> String {
    match error {
        OpError::NoG27 => "aucun G27 détecté.".to_owned(),
        OpError::NotNative => {
            "le G27 est en mode compatibilité, basculez d'abord en mode natif.".to_owned()
        }
        OpError::AlreadyNative => "le G27 est déjà en mode natif.".to_owned(),
        OpError::OutOfRange(value) => {
            format!("angle invalide ({value}°), attendu entre 40 et 900.")
        }
        OpError::Unsupported => "non disponible dans cette version.".to_owned(),
        OpError::Hardware => "échec d'accès au matériel (HID).".to_owned(),
    }
}
