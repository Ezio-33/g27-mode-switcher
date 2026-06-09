//! Interface graphique eframe/egui du G27 Mode Switcher.

mod app;
mod carte_forza;
mod carte_pont;
mod log;
mod remap;
mod theme;
mod widgets;

use std::process::ExitCode;

use eframe::egui;

/// Lance la fenêtre principale (mode par défaut, sans sous-commande CLI).
///
/// Initialise le journal partagé et le pont `tracing`, applique le thème, puis
/// exécute la boucle eframe. Renvoie le code de sortie du processus.
#[must_use]
pub fn run(verbose: u8) -> ExitCode {
    let config = g27_mode_switcher::config::Config::charger();

    let buffer = log::LogBuffer::new();
    log::init(verbose, &config.journalisation.verbosite, buffer.clone());

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([config.fenetre.largeur, config.fenetre.hauteur])
        .with_min_inner_size([420.0, 560.0])
        .with_title("G27 Mode Switcher");
    // Icône de fenêtre = logo du projet (volant stylisé, identique au site).
    if let Ok(icone) =
        eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon/icon.png"))
    {
        viewport = viewport.with_icon(icone);
    }
    if let (Some(x), Some(y)) = (config.fenetre.pos_x, config.fenetre.pos_y) {
        viewport = viewport.with_position([x, y]);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let result = eframe::run_native(
        "G27 Mode Switcher",
        options,
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(app::App::new(config, buffer.clone())))
        }),
    );

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Erreur de l'interface graphique : {error}");
            ExitCode::FAILURE
        }
    }
}
