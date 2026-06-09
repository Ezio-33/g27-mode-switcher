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

/// Angle de dérive avant (radians) saturant la force de virage au gain plein (~23°, slip
/// élevé rarement atteint). Combiné à la courbe progressive, la force monte **doucement**
/// en virage normal et ne devient ferme qu'à forte dérive.
const DERIVE_PLEINE_RAD: f32 = 0.40;
/// Vitesse (m/s ≈ 29 km/h) à partir de laquelle la force de virage (dérive) atteint son
/// plein facteur. En deçà, elle est réduite linéairement.
const VITESSE_PLEINE_M_S: f32 = 8.0;

/// Poids d'autocentrage matériel **à l'arrêt** (sur `0xFFFF`) : la « friction de parking »
/// — les pneus frottent sur place, le volant est **lourd**. ~73 % du max (le plein `0xFFFF`
/// est jugé trop rigide, mais un poids net est attendu à l'arrêt).
const POIDS_ARRET: f32 = 48_000.0;
/// Poids d'autocentrage matériel **en roulant** (sur `0xFFFF`) : les pneus roulent, la
/// friction chute, le volant **s'allège** nettement.
const POIDS_ROULANT: f32 = 9_000.0;
/// Vitesse (m/s ≈ 65 km/h) où l'allègement atteint son plateau léger : en deçà, le volant
/// reste **lourd à bas régime** et s'allège en **cosinus** (très progressif), comme une
/// vraie direction. Au-delà, il reste au poids léger (`POIDS_ROULANT`).
const VITESSE_ALLEGEMENT_M_S: f32 = 18.0;

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
    // Courbe progressive (carré signé) : douce près du centre, ferme seulement à forte
    // dérive — évite un couple « trop prononcé » dès le moindre virage.
    let progressif = derive * derive.abs();
    let facteur_vitesse = (t.vitesse_m_s / VITESSE_PLEINE_M_S).clamp(0.0, 1.0);
    let mut couple = -progressif * facteur_vitesse * gain;
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

/// Magnitude de l'**autocentrage matériel** (`0..0xFFFF`) déduite de la vitesse : **lourde
/// à l'arrêt** ([`POIDS_ARRET`], friction de parking) puis **s'allégeant** avec la vitesse
/// jusqu'à [`POIDS_ROULANT`], le tout modulé par le `gain`. Reproduit une vraie direction
/// (dure à l'arrêt, légère en roulant). Renvoie `0` hors gameplay (volant libre en menu).
#[must_use]
pub fn autocentre_depuis_vitesse(t: &Telemetrie, reglages: &ReglagesForza) -> u16 {
    if !t.course_active {
        return 0;
    }
    let facteur = (t.vitesse_m_s / VITESSE_ALLEGEMENT_M_S).clamp(0.0, 1.0);
    // Allègement en cosinus : pente nulle à l'arrêt (reste lourd à bas régime) puis
    // descente douce jusqu'au plateau léger — bien plus progressif qu'une rampe linéaire.
    let poids = (facteur * core::f32::consts::FRAC_PI_2).cos();
    let gain = f32::from(reglages.gain) / 100.0;
    let magnitude = (POIDS_ROULANT + poids * (POIDS_ARRET - POIDS_ROULANT)) * gain;
    borne_u16(magnitude)
}

/// Borne un flottant à `0..=0xFFFF` et le convertit en `u16`.
fn borne_u16(valeur: f32) -> u16 {
    let borne = valeur.clamp(0.0, f32::from(u16::MAX)).round();
    // `borne` ∈ [0, 65535] : la conversion ne peut ni tronquer ni perdre de signe.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        borne as u16
    }
}

#[cfg(test)]
mod tests {
    use super::{COUPLE_MAX, ReglagesForza, autocentre_depuis_vitesse, couple_depuis_telemetrie};
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
        // À pleine dérive (= DERIVE_PLEINE_RAD) et pleine vitesse, le couple sature.
        let couple = couple_depuis_telemetrie(&telem(true, 20.0, 0.40), &r);
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
    fn autocentre_lourd_a_l_arret_s_allege_en_roulant() {
        let r = ReglagesForza::default();
        let arret = autocentre_depuis_vitesse(&telem(true, 0.0, 0.0), &r);
        let lent = autocentre_depuis_vitesse(&telem(true, 6.0, 0.0), &r);
        let rapide = autocentre_depuis_vitesse(&telem(true, 30.0, 0.0), &r);
        // Friction de parking : lourd à l'arrêt, qui s'allège avec la vitesse.
        assert!(arret > lent && lent > rapide, "{arret} > {lent} > {rapide}");
        // Jamais le ressort plein (rigidité constante non naturelle), et léger en roulant.
        assert!(
            arret < u16::MAX && rapide > 0,
            "arret={arret} rapide={rapide}"
        );
    }

    #[test]
    fn autocentre_nul_hors_course() {
        let r = ReglagesForza::default();
        assert_eq!(autocentre_depuis_vitesse(&telem(false, 40.0, 0.0), &r), 0);
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
