//! Modèle de force : convertit la télémétrie Forza en couple pour le volant.
//!
//! Le couple dominant ressenti sur un volant est le **couple d'auto-alignement** : les
//! pneus avant, en dérive, génèrent une force qui tend à **ramener le volant au centre**,
//! proportionnelle à l'angle de dérive (jusqu'à saturation à la limite d'adhérence). On
//! le module par la **vitesse** (nul à l'arrêt, plein au-delà d'un seuil) et par un
//! **gain** utilisateur. Module **pur** (aucune E/S), donc testable.

use super::Telemetrie;

/// Couple maximal en sortie (échelle de [`crate::ffb::g27::commande_force_constante`]).
const COUPLE_MAX: i32 = 10000;
/// Idem en flottant (10000 est exact en `f32`, pas de perte).
const COUPLE_MAX_F: f32 = 10_000.0;

/// Angle de dérive avant (radians) saturant la force au gain plein (~11°, proche de la
/// limite d'adhérence). Au-delà, le couple reste à son maximum.
const DERIVE_PLEINE_RAD: f32 = 0.20;
/// Vitesse (m/s ≈ 29 km/h) à partir de laquelle le facteur vitesse atteint 1. En deçà, le
/// couple est réduit linéairement (à l'arrêt : nul — le centrage vient de l'autocentrage
/// matériel, conservé en parallèle).
const VITESSE_PLEINE_M_S: f32 = 8.0;

/// Réglages du retour de force Forza, ajustables à chaud.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReglagesForza {
    /// Intensité globale du retour de force, en pourcentage (`0..=100`).
    pub gain: u8,
    /// Inverser le sens du couple (à régler une fois selon le ressenti matériel : si le
    /// volant « fuit » au lieu de résister, activer).
    pub inverser: bool,
}

impl Default for ReglagesForza {
    fn default() -> Self {
        Self {
            gain: 60,
            inverser: false,
        }
    }
}

/// Calcule le couple (−[`COUPLE_MAX`]..[`COUPLE_MAX`]) à appliquer au volant pour la
/// télémétrie `t` et les `reglages`. Renvoie `0` hors gameplay (IsRaceOn faux).
#[must_use]
pub fn couple_depuis_telemetrie(t: &Telemetrie, reglages: &ReglagesForza) -> i32 {
    if !t.course_active {
        return 0;
    }
    let gain = f32::from(reglages.gain) / 100.0;
    // Couple d'auto-alignement : s'oppose à la dérive (signe négatif) → ramène au centre.
    let derive = (t.derive_avant / DERIVE_PLEINE_RAD).clamp(-1.0, 1.0);
    let facteur_vitesse = (t.vitesse_m_s / VITESSE_PLEINE_M_S).clamp(0.0, 1.0);
    let mut couple = -derive * facteur_vitesse * gain;
    if reglages.inverser {
        couple = -couple;
    }
    arrondir_borne(couple * COUPLE_MAX_F)
}

/// Arrondit et borne un couple flottant à `−COUPLE_MAX..=COUPLE_MAX` (entier).
fn arrondir_borne(valeur: f32) -> i32 {
    let borne = valeur.clamp(-COUPLE_MAX_F, COUPLE_MAX_F).round();
    // `borne` ∈ [−10000, 10000] : la conversion ne peut ni tronquer ni déborder.
    #[allow(clippy::cast_possible_truncation)]
    let entier = borne as i32;
    entier.clamp(-COUPLE_MAX, COUPLE_MAX)
}

#[cfg(test)]
mod tests {
    use super::{COUPLE_MAX, ReglagesForza, couple_depuis_telemetrie};
    use crate::telemetrie::Telemetrie;

    fn telem(course_active: bool, vitesse_m_s: f32, derive_avant: f32) -> Telemetrie {
        Telemetrie {
            course_active,
            vitesse_m_s,
            derive_avant,
            rumble_avant: 0.0,
        }
    }

    #[test]
    fn hors_gameplay_aucun_couple() {
        let r = ReglagesForza::default();
        assert_eq!(couple_depuis_telemetrie(&telem(false, 30.0, 0.3), &r), 0);
    }

    #[test]
    fn a_l_arret_couple_nul() {
        let r = ReglagesForza::default();
        assert_eq!(couple_depuis_telemetrie(&telem(true, 0.0, 0.3), &r), 0);
    }

    #[test]
    fn derive_a_droite_pousse_a_gauche_par_defaut() {
        // Dérive positive + pleine vitesse → couple négatif (rappel vers le centre).
        let r = ReglagesForza {
            gain: 100,
            inverser: false,
        };
        let couple = couple_depuis_telemetrie(&telem(true, 20.0, 0.30), &r);
        assert_eq!(couple, -COUPLE_MAX, "saturation au rappel, couple={couple}");
    }

    #[test]
    fn inversion_change_le_signe() {
        let direct = ReglagesForza {
            gain: 100,
            inverser: false,
        };
        let inverse = ReglagesForza {
            gain: 100,
            inverser: true,
        };
        let t = telem(true, 20.0, 0.10);
        assert_eq!(
            couple_depuis_telemetrie(&t, &direct),
            -couple_depuis_telemetrie(&t, &inverse)
        );
    }

    #[test]
    fn gain_module_l_intensite() {
        let plein = ReglagesForza {
            gain: 100,
            inverser: false,
        };
        let moitie = ReglagesForza {
            gain: 50,
            inverser: false,
        };
        let t = telem(true, 20.0, 0.10);
        let c_plein = couple_depuis_telemetrie(&t, &plein).abs();
        let c_moitie = couple_depuis_telemetrie(&t, &moitie).abs();
        assert!(
            c_moitie < c_plein && c_moitie > 0,
            "{c_moitie} vs {c_plein}"
        );
    }
}
