//! Interface graphique eframe/egui du G27 Mode Switcher.

mod app;
mod theme;

use eframe::egui;

/// Lance la fenêtre principale (mode par défaut, sans sous-commande CLI).
///
/// # Errors
///
/// Renvoie l'erreur d'eframe si la fenêtre ne peut pas être créée ou si la
/// boucle d'événements échoue (backend graphique indisponible, p. ex.).
pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([460.0, 600.0])
            .with_min_inner_size([400.0, 480.0])
            .with_title("G27 Mode Switcher"),
        ..Default::default()
    };

    eframe::run_native(
        "G27 Mode Switcher",
        options,
        Box::new(|cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(app::App::new()))
        }),
    )
}
