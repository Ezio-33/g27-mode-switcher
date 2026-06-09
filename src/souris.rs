//! Injection de mouvements souris depuis le D-pad du G27 (navigation de la map Forza).
//!
//! La map de Forza Horizon (PC) ne se navigue **pas** avec un périphérique `DirectInput`
//! comme vJoy : seuls le clavier, la souris ou une manette `XInput` sont lus pour son
//! curseur. Quand le G27 est masqué (pour que le FFB passe par vJoy), on traduit donc le
//! D-pad en **déplacements relatifs du curseur** (même API `SendInput` que
//! [`crate::clavier`], aucun logiciel ni dépendance en plus).
//!
//! Contrairement aux touches (émises sur les fronts), le mouvement est **continu** :
//! tant qu'une direction du D-pad est tenue, chaque rapport pousse le curseur de
//! quelques pixels. Un mouvement relatif est instantané : aucun état à relâcher au
//! `Drop` (rien ne reste « collé »).

use crate::entree::cardinaux_chapeau;

/// Déplacement du curseur par rapport HID quand une direction du D-pad est tenue (px).
/// Le G27 émet ses rapports à haute cadence (~100 Hz), soit ≈ 1200 px/s : assez vif pour
/// déplacer le curseur de la map sans être incontrôlable.
const VITESSE_PIXELS: i32 = 12;

/// Injecteur souris : traduit le D-pad du G27 en déplacements relatifs du curseur.
pub struct SourisG27 {
    /// Traduire le D-pad en mouvements souris (sinon aucun mouvement n'est émis).
    active: bool,
}

impl SourisG27 {
    /// Crée l'injecteur. `active` traduit le D-pad en mouvements souris.
    #[must_use]
    pub fn new(active: bool) -> Self {
        Self { active }
    }

    /// Vrai si l'injecteur a quelque chose à faire (traduction active).
    #[must_use]
    pub fn utile(&self) -> bool {
        self.active
    }

    /// Pousse le curseur selon les directions tenues du D-pad (rien si inactif/centré).
    pub fn appliquer(&self, chapeau: u8) {
        if !self.active {
            return;
        }
        let (dx, dy) = delta(chapeau);
        if dx != 0 || dy != 0 {
            deplacer(dx, dy);
        }
    }
}

/// Déplacement `(dx, dy)` en pixels d'après les directions cardinales tenues du D-pad
/// (1=haut, 2=droite, 4=bas, 8=gauche). Une diagonale pousse les deux axes.
fn delta(chapeau: u8) -> (i32, i32) {
    let cardinaux = cardinaux_chapeau(chapeau);
    let mut dx = 0;
    let mut dy = 0;
    if cardinaux & 2 != 0 {
        dx += VITESSE_PIXELS; // droite
    }
    if cardinaux & 8 != 0 {
        dx -= VITESSE_PIXELS; // gauche
    }
    if cardinaux & 4 != 0 {
        dy += VITESSE_PIXELS; // bas (Y vers le bas à l'écran)
    }
    if cardinaux & 1 != 0 {
        dy -= VITESSE_PIXELS; // haut
    }
    (dx, dy)
}

/// Émet un mouvement **relatif** du curseur de `(dx, dy)` pixels.
#[cfg(windows)]
#[allow(unsafe_code)]
fn deplacer(dx: i32, dy: i32) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput,
    };

    let entree = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let taille = i32::try_from(core::mem::size_of::<INPUT>()).expect("taille INPUT tient en i32");
    // SAFETY : `entree` est une `INPUT` complète et valide (variante souris renseignée),
    // on passe exactement 1 élément et la taille réelle de `INPUT`. `SendInput` copie la
    // structure et n'en conserve aucun pointeur après l'appel.
    unsafe {
        SendInput(1, &raw const entree, taille);
    }
}

/// Hors Windows : aucune injection souris (l'application cible Windows pour le pont).
#[cfg(not(windows))]
fn deplacer(_dx: i32, _dy: i32) {}

#[cfg(test)]
mod tests {
    use super::{VITESSE_PIXELS, delta};

    #[test]
    fn centre_ne_bouge_pas() {
        // Chapeau relâché (8) → aucun mouvement.
        assert_eq!(delta(8), (0, 0));
    }

    #[test]
    fn directions_cardinales() {
        assert_eq!(delta(0), (0, -VITESSE_PIXELS)); // haut
        assert_eq!(delta(2), (VITESSE_PIXELS, 0)); // droite
        assert_eq!(delta(4), (0, VITESSE_PIXELS)); // bas
        assert_eq!(delta(6), (-VITESSE_PIXELS, 0)); // gauche
    }

    #[test]
    fn diagonale_pousse_les_deux_axes() {
        // Haut-droite (1) → droite + haut.
        assert_eq!(delta(1), (VITESSE_PIXELS, -VITESSE_PIXELS));
        // Bas-gauche (5) → gauche + bas.
        assert_eq!(delta(5), (-VITESSE_PIXELS, VITESSE_PIXELS));
    }
}
