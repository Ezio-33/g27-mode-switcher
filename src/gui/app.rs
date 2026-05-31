//! Application graphique : ossature (statut, placeholder, footer, à propos).
//!
//! Les contrôles (bascule, angle, autocentrage) et le journal seront ajoutés à
//! l'étape suivante ; cette ossature pose la mise en page et le pilotage live du
//! statut via la [`DeviceSession`].

use std::time::Duration;

use eframe::egui::{self, RichText};
use g27_mode_switcher::device::{DeviceSession, Event, Status};

use super::theme;

const URL_SITE: &str = "https://la-confrerie-des-ombres.vercel.app/index.html";
const URL_DISCORD: &str = "https://discord.gg/zckGmdg";
const URL_TIP: &str = "https://streamelements.com/ezio_33/tip";
const AUTHOR: &str = "Samuel.V (Ezio_33)";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Cadence de rafraîchissement pour refléter le statut en temps réel.
const REPAINT_INTERVAL: Duration = Duration::from_millis(350);

/// État de l'interface graphique.
pub struct App {
    session: DeviceSession,
    status: Status,
    about_open: bool,
}

impl App {
    /// Crée l'application et démarre la session matérielle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            session: DeviceSession::spawn(),
            status: Status::Absent,
            about_open: false,
        }
    }

    /// Draine les événements de la session et programme le prochain rafraîchissement.
    fn poll(&mut self, ctx: &egui::Context) {
        for event in self.session.drain_events() {
            if let Event::Status(status) = event {
                self.status = status;
            }
        }
        ctx.request_repaint_after(REPAINT_INTERVAL);
    }

    /// Titre + pastille de statut.
    fn header(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| ui.heading("G27 Mode Switcher"));
        ui.add_space(10.0);

        let (color, label) = match self.status {
            Status::Native => (theme::SUCCESS, "Mode natif (900°) — prêt"),
            Status::Compatibility => (theme::WARNING, "Mode compatibilité (200°)"),
            Status::Absent => (theme::TEXT_DIM, "Aucun G27 détecté"),
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new("\u{25cf}").color(color));
            ui.label(RichText::new(label).color(color).strong());
        });
    }

    /// Zone centrale : placeholder en attendant les contrôles (étape suivante).
    fn central(ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(
                    "Les contrôles (bascule, angle, autocentrage) arrivent à l'étape suivante.",
                )
                .color(theme::TEXT_MUTED),
            );
        });
    }

    /// Pied de page : liens discrets + bouton « À propos ».
    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            footer_link(ui, "Site", URL_SITE);
            footer_sep(ui);
            footer_link(ui, "Discord", URL_DISCORD);
            footer_sep(ui);
            footer_link(ui, "Soutenir", URL_TIP);
            footer_sep(ui);
            if ui
                .add(
                    egui::Button::new(RichText::new("À propos").small().color(theme::TEXT_DIM))
                        .frame(false),
                )
                .clicked()
            {
                self.about_open = true;
            }
        });
        ui.add_space(4.0);
    }

    /// Fenêtre « À propos » : auteur, version, licence.
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
                ui.heading("G27 Mode Switcher");
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

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll(ui.ctx());

        ui.add_space(4.0);
        self.header(ui);
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(10.0);
        Self::central(ui);

        // Le pied de page est ancré en bas de l'espace restant.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            self.footer(ui);
        });

        self.about_window(ui.ctx());
    }
}

/// Lien de pied de page discret (petite taille, couleur atténuée).
fn footer_link(ui: &mut egui::Ui, label: &str, url: &str) {
    ui.hyperlink_to(RichText::new(label).small().color(theme::TEXT_DIM), url);
}

/// Séparateur visuel discret entre deux liens.
fn footer_sep(ui: &mut egui::Ui) {
    ui.label(RichText::new("\u{b7}").small().color(theme::TEXT_DIM));
}
