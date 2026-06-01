//! Application graphique : mise en page en cartes et contrôles du G27.

use std::time::Duration;

use eframe::egui::{self, RichText, Stroke};
use g27_mode_switcher::config::Config;
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
pub struct App {
    session: DeviceSession,
    log: LogBuffer,
    status: Status,
    range_deg: u16,
    autocenter_disabled: bool,
    about_open: bool,
    config: Config,
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
            config,
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
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Volant en mode natif G27 (C29B)")
                                .color(theme::TEXT_MUTED)
                                .size(13.0),
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
                                self.log.push(
                                    LineKind::Info,
                                    "Pour revenir en mode compatibilité, débranchez puis rebranchez le volant.",
                                );
                            }
                        });
                    });
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

    /// Carte « Retour de force » : autocentrage (fonctionnel) + pont vJoy (à venir).
    fn card_ffb(&mut self, ui: &mut egui::Ui) {
        let is_native = self.status == Status::Native;
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_label(ui, "RETOUR DE FORCE");
            ui.add_space(10.0);

            // Autocentrage matériel : la désactivation est fonctionnelle ; la
            // réactivation (off→on) est journalisée comme prévue en v0.3.0.
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

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(12.0);

            // Pont retour de force vJoy : arrive en Phase 4/5, désactivé ici.
            control_row(
                ui,
                "Retour de force (vJoy)",
                "À venir (v0.3.0) — nécessite vJoy + HidHide",
                |ui| toggle_switch(ui, false, false),
            );
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
        centered_row(ui, "liens_pied", |ui| {
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
                about_clicked = true;
            }
        });
        if about_clicked {
            self.about_open = true;
        }
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
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::BG_BASE.to_normalized_gamma_f32()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // À la fermeture, on persiste la dernière géométrie de fenêtre (capturée
        // à chaque frame) ainsi que les réglages courants.
        self.persister_reglages();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll(ui.ctx());

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

        // Reste de la fenêtre : en-tête pleine largeur, cartes en retrait latéral
        // (16px) pour se détacher, journal occupant la hauteur restante.
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| {
                ui.add_space(8.0);
                self.header(ui);
                ui.add_space(10.0);
                ui.separator();

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
                        self.card_ffb(ui);
                        ui.add_space(12.0);
                        // Le journal remplit la hauteur restante du panneau central.
                        self.card_journal(ui);
                    });
            });

        self.about_window(ui.ctx());
    }
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

/// Slider d'angle dessiné au painter : piste de fond, portion remplie or, thumb
/// rond. Le `Response` (clic + glisser) permet d'envoyer la valeur au relâché.
fn angle_slider(ui: &mut egui::Ui, value: &mut u16, width: f32) -> egui::Response {
    const HEIGHT: f32 = 22.0;
    const TRACK_H: f32 = 5.0;
    const THUMB_R: f32 = 8.0;

    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(width, HEIGHT), egui::Sense::click_and_drag());
    let enabled = ui.is_enabled();

    let lo = f32::from(RANGE_MIN);
    let hi = f32::from(RANGE_MAX);
    let usable_left = rect.left() + THUMB_R;
    let usable_w = (rect.width() - 2.0 * THUMB_R).max(1.0);

    if enabled && let Some(pos) = response.interact_pointer_pos() {
        let t = ((pos.x - usable_left) / usable_w).clamp(0.0, 1.0);
        // `t` ∈ [0, 1] et l'amplitude est bornée → `steps` ∈ [0, 860] : la
        // conversion ne peut ni tronquer significativement ni perdre de signe.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = (t * (hi - lo)).round() as u16;
        let new = RANGE_MIN + steps;
        if new != *value {
            *value = new;
            response.mark_changed();
        }
    }

    let t = ((f32::from(*value) - lo) / (hi - lo)).clamp(0.0, 1.0);
    let thumb_x = usable_left + t * usable_w;
    let cy = rect.center().y;
    let (fill_color, thumb_color) = if enabled {
        (theme::GOLD, theme::GOLD_LIGHT)
    } else {
        (theme::GOLD_DARK, theme::TEXT_DIM)
    };

    let painter = ui.painter();
    let track = egui::Rect::from_min_max(
        egui::pos2(rect.left(), cy - TRACK_H / 2.0),
        egui::pos2(rect.right(), cy + TRACK_H / 2.0),
    );
    painter.rect_filled(track, egui::CornerRadius::same(3), theme::BG_ELEVATED);
    let filled = egui::Rect::from_min_max(
        egui::pos2(rect.left(), cy - TRACK_H / 2.0),
        egui::pos2(thumb_x, cy + TRACK_H / 2.0),
    );
    painter.rect_filled(filled, egui::CornerRadius::same(3), fill_color);
    painter.circle(
        egui::pos2(thumb_x, cy),
        THUMB_R,
        thumb_color,
        Stroke::new(2.0, theme::BG_BASE),
    );

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
            ui.label(RichText::new(title).size(14.0).color(theme::TEXT));
            ui.add_space(2.0);
            ui.label(RichText::new(subtitle).small().color(theme::TEXT_DIM));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            toggle = Some(add_toggle(ui));
        });
    });
    toggle.expect("control_row : interrupteur non fourni")
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
