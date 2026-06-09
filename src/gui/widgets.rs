//! Petits widgets dessinés à la main, partagés entre les cartes (zéro doublon).

use eframe::egui::{self, Stroke};

use super::theme;

/// Slider horizontal doré (piste, portion remplie, thumb), façon « angle de rotation ».
///
/// `fraction` ∈ `0.0..=1.0` est mise à jour au clic/glisser. `largeur`/`hauteur`/`rayon`
/// permettent une version pleine (angle) ou **compacte** (intensité Forza). Renvoie la
/// [`egui::Response`] (`changed()` quand la fraction bouge).
pub fn curseur_dore(
    ui: &mut egui::Ui,
    fraction: &mut f32,
    largeur: f32,
    hauteur: f32,
    rayon: f32,
) -> egui::Response {
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(largeur, hauteur), egui::Sense::click_and_drag());
    let actif = ui.is_enabled();

    let bord = rect.left() + rayon;
    let utile = (rect.width() - 2.0 * rayon).max(1.0);
    if actif && let Some(pos) = response.interact_pointer_pos() {
        let t = ((pos.x - bord) / utile).clamp(0.0, 1.0);
        if (t - *fraction).abs() > f32::EPSILON {
            *fraction = t;
            response.mark_changed();
        }
    }

    let t = (*fraction).clamp(0.0, 1.0);
    let thumb_x = bord + t * utile;
    let cy = rect.center().y;
    let piste_h = (hauteur * 0.25).clamp(3.0, 6.0);
    let (rempli, thumb) = if actif {
        (theme::GOLD, theme::GOLD_LIGHT)
    } else {
        (theme::GOLD_DARK, theme::TEXT_DIM)
    };

    let painter = ui.painter();
    let piste = egui::Rect::from_min_max(
        egui::pos2(rect.left(), cy - piste_h / 2.0),
        egui::pos2(rect.right(), cy + piste_h / 2.0),
    );
    painter.rect_filled(piste, egui::CornerRadius::same(3), theme::BG_ELEVATED);
    let rempli_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left(), cy - piste_h / 2.0),
        egui::pos2(thumb_x, cy + piste_h / 2.0),
    );
    painter.rect_filled(rempli_rect, egui::CornerRadius::same(3), rempli);
    painter.circle(
        egui::pos2(thumb_x, cy),
        rayon,
        thumb,
        Stroke::new(2.0, theme::BG_BASE),
    );

    response
}
