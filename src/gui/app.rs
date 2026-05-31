//! Application graphique : mise en page en cartes et contrôles du G27.

use std::time::Duration;

use eframe::egui::{self, RichText, Stroke};
use g27_mode_switcher::device::{Command, DeviceSession, Event, OpError, OpKind, OpReport, Status};

use super::log::{self, LineKind, LogBuffer};
use super::theme;

const URL_SITE: &str = "https://la-confrerie-des-ombres.vercel.app/index.html";
const URL_DISCORD: &str = "https://discord.gg/zckGmdg";
const URL_TIP: &str = "https://streamelements.com/ezio_33/tip";
const AUTHOR: &str = "Samuel.V (Ezio_33)";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Cadence de rafraîchissement pour refléter le statut en temps réel.
const REPAINT_INTERVAL: Duration = Duration::from_millis(350);

/// Préréglages d'angle proposés (libellé, degrés).
const RANGE_PRESETS: [(&str, u16); 4] = [
    ("F1 360°", 360),
    ("GT 540°", 540),
    ("Rallye 720°", 720),
    ("Camion 900°", 900),
];

/// État de l'interface graphique.
pub struct App {
    session: DeviceSession,
    log: LogBuffer,
    status: Status,
    range_deg: u16,
    autocenter_disabled: bool,
    about_open: bool,
}

impl App {
    /// Crée l'application et démarre la session matérielle.
    #[must_use]
    pub fn new(log: LogBuffer) -> Self {
        Self {
            session: DeviceSession::spawn(),
            log,
            status: Status::Absent,
            range_deg: 900,
            autocenter_disabled: false,
            about_open: false,
        }
    }

    /// Draine les événements de la session vers le journal et programme un repaint.
    fn poll(&mut self, ctx: &egui::Context) {
        for event in self.session.drain_events() {
            match event {
                Event::Status(status) if status != self.status => {
                    self.status = status;
                    self.log.push(LineKind::Info, status_line(status));
                }
                Event::Status(_) => {}
                Event::Op(report) => {
                    let (kind, text) = journal_for_op(report);
                    self.log.push(kind, text);
                }
            }
        }
        ctx.request_repaint_after(REPAINT_INTERVAL);
    }

    /// En-tête : titre Cinzel + pastille de statut.
    fn header(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("G27 MODE SWITCHER")
                    .family(theme::heading_family())
                    .size(25.0)
                    .color(theme::GOLD),
            );
            ui.add_space(8.0);

            let (color, text) = match self.status {
                Status::Native => (
                    theme::SUCCESS,
                    "Mode natif — 900° — retour de force complet",
                ),
                Status::Compatibility => (theme::WARNING, "Mode compatibilité (200°)"),
                Status::Absent => (theme::TEXT_DIM, "Aucun G27 détecté"),
            };
            theme::pill_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("\u{25cf}").color(color));
                    ui.label(RichText::new(text).color(color).strong());
                });
            });
        });
    }

    /// Carte « Mode du volant » : bascule (compat) ou état natif.
    fn card_mode(&mut self, ui: &mut egui::Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_label(ui, "MODE DU VOLANT");
            ui.add_space(8.0);
            match self.status {
                Status::Compatibility => {
                    let button = egui::Button::new(
                        RichText::new("Basculer en mode 900° natif")
                            .color(theme::BG_BASE)
                            .strong(),
                    )
                    .fill(theme::GOLD)
                    .min_size(egui::vec2(ui.available_width(), 40.0));
                    if ui.add(button).clicked() {
                        self.session.send(Command::Switch {
                            apply_range: true,
                            disable_autocenter: false,
                        });
                        self.log.push(LineKind::Info, "Bascule demandée\u{2026}");
                    }
                }
                Status::Native => {
                    ui.label(RichText::new("Volant en mode natif (C29B).").color(theme::TEXT));
                    ui.add_space(6.0);
                    let revert = egui::Button::new(
                        RichText::new("Revenir en compatibilité")
                            .small()
                            .color(theme::TEXT_DIM),
                    )
                    .frame(false);
                    if ui.add(revert).clicked() {
                        self.log.push(
                            LineKind::Info,
                            "Pour revenir en mode compatibilité, débranchez puis rebranchez le volant.",
                        );
                    }
                }
                Status::Absent => {
                    ui.label(
                        RichText::new("Branchez un Logitech G27 pour commencer.")
                            .color(theme::TEXT_DIM),
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
            section_label(ui, "ANGLE DE ROTATION");
            ui.add_space(8.0);
            ui.add_enabled_ui(is_native, |ui| {
                ui.horizontal(|ui| {
                    let slider = egui::Slider::new(&mut self.range_deg, 40..=900)
                        .show_value(false)
                        .suffix("°");
                    let response = ui.add(slider);
                    if response.drag_stopped() {
                        self.session.send(Command::SetRange(self.range_deg));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{}\u{b0}", self.range_deg))
                                .family(theme::heading_family())
                                .size(18.0)
                                .color(theme::GOLD),
                        );
                    });
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
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
                            self.session.send(Command::SetRange(degrees));
                        }
                    }
                });
            });
        });
    }

    /// Carte « Retour de force » : autocentrage + emplacement FFB (à venir).
    fn card_ffb(&mut self, ui: &mut egui::Ui) {
        let is_native = self.status == Status::Native;
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_label(ui, "RETOUR DE FORCE");
            ui.add_space(8.0);
            ui.add_enabled_ui(is_native, |ui| {
                let response =
                    ui.toggle_value(&mut self.autocenter_disabled, "Désactiver l'autocentrage matériel");
                if response.changed() {
                    self.session.send(Command::SetAutocenter {
                        enable: !self.autocenter_disabled,
                    });
                }
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Laissé actif par défaut : sans retour de force dynamique, c'est la seule force de centrage du volant.",
                )
                .small()
                .color(theme::TEXT_DIM),
            );
            ui.add_space(8.0);
            ui.add_enabled_ui(false, |ui| {
                let mut reserved = false;
                ui.toggle_value(&mut reserved, "Retour de force (vJoy) \u{2014} à venir (v0.3.0)");
            });
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

    /// Pied de page : liens centrés + « À propos ».
    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.add_space(4.0);
        ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                footer_link(ui, "Site", URL_SITE);
                footer_sep(ui);
                footer_link(ui, "Discord", URL_DISCORD);
                footer_sep(ui);
                footer_link(ui, "Soutenir", URL_TIP);
                footer_sep(ui);
                let about =
                    egui::Button::new(RichText::new("À propos").small().color(theme::GOLD_DARK))
                        .frame(false);
                if ui.add(about).clicked() {
                    self.about_open = true;
                }
            });
        });
    }

    /// Fenêtre « À propos ».
    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let mut open = self.about_open;
        egui::Window::new("À propos")
            .collapsible(false)
            .resizable(false)
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
                ui.label(format!("Auteur : {AUTHOR}"));
                ui.hyperlink_to("Site — la Confrérie des Ombres", URL_SITE);
                ui.add_space(8.0);
                ui.label(RichText::new("Licence : MIT").color(theme::TEXT_MUTED));
                ui.label(
                    RichText::new("Police Cinzel : SIL Open Font License 1.1")
                        .color(theme::TEXT_MUTED),
                );
            });
        self.about_open = open;
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll(ui.ctx());

        ui.add_space(6.0);
        self.header(ui);
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(10.0);

        self.card_mode(ui);
        ui.add_space(10.0);
        self.card_angle(ui);
        ui.add_space(10.0);
        self.card_ffb(ui);
        ui.add_space(10.0);

        // Le journal occupe l'espace restant, le pied de page est ancré en bas.
        let footer_height = 34.0;
        let journal_height = (ui.available_height() - footer_height).max(110.0);
        ui.allocate_ui(egui::vec2(ui.available_width(), journal_height), |ui| {
            self.card_journal(ui);
        });
        self.footer(ui);

        self.about_window(ui.ctx());
    }
}

/// Petit label de section (majuscules, atténué).
fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().strong().color(theme::TEXT_DIM));
}

/// Lien de pied de page discret (or atténué).
fn footer_link(ui: &mut egui::Ui, label: &str, url: &str) {
    ui.hyperlink_to(RichText::new(label).small().color(theme::GOLD_DARK), url);
}

/// Séparateur discret entre deux liens.
fn footer_sep(ui: &mut egui::Ui) {
    ui.label(RichText::new("\u{b7}").small().color(theme::TEXT_DIM));
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
        (OpKind::DisableAutocenter, Ok(())) => {
            (LineKind::Success, "Autocentrage matériel désactivé.".to_owned())
        }
        (OpKind::EnableAutocenter, _) => (
            LineKind::Info,
            "Réactivation paramétrable de l'autocentrage prévue en v0.3.0 — rebranchez le volant pour la rétablir.".to_owned(),
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
