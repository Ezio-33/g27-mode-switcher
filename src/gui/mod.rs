//! Interface graphique eframe/egui du G27 Mode Switcher.

mod app;
mod log;
mod theme;

use std::process::ExitCode;

use eframe::egui;

/// Lance la fenêtre principale (mode par défaut, sans sous-commande CLI).
///
/// Initialise le journal partagé et le pont `tracing`, applique le thème, puis
/// exécute la boucle eframe. Renvoie le code de sortie du processus.
#[must_use]
pub fn run(verbose: u8) -> ExitCode {
    let buffer = log::LogBuffer::new();
    log::init(verbose, buffer.clone());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([480.0, 680.0])
            .with_min_inner_size([420.0, 560.0])
            .with_title("G27 Mode Switcher"),
        ..Default::default()
    };

    let result = eframe::run_native(
        "G27 Mode Switcher",
        options,
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(app::App::new(buffer.clone())))
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
