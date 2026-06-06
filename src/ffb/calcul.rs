//! Calcul du couple net FFB à partir de la banque d'effets et de l'état du volant.
//!
//! Module **pur** (aucun FFI, aucune E/S) : il somme la contribution de chaque effet
//! en cours pour produire un couple unique (−10000..10000) que l'étape suivante
//! traduira en commande de force pour le G27 réel.
//!
//! ⚠️ Le **gain par effet n'est pas appliqué** : les jeux le laissent souvent à 0
//! (vérifié sur Forza) et pilotent l'intensité via la magnitude / les coefficients +
//! le **gain global** du device. On ne pondère donc que par ce gain global.

use super::effets::{BanqueEffets, Effet, ParametresEffet};
use super::message::TypeEffet;

/// Échelle des forces FFB : −PLAGE..PLAGE = −100 %..100 % (comme le SDK, −10000..10000).
const PLAGE: i32 = 10000;
/// Gain global maximal (255 = 100 %).
const GAIN_MAX: i64 = 255;

/// État instantané du volant, **normalisé** à −PLAGE..PLAGE (centre = 0).
#[derive(Debug, Clone, Copy, Default)]
pub struct EtatVolant {
    /// Position : −10000 = butée gauche, 0 = centre, +10000 = butée droite.
    pub position: i32,
    /// Vitesse de rotation (variation de position), même échelle.
    pub vitesse: i32,
}

/// Coefficients d'un effet conditionnel (ressort/amortisseur/…).
struct ParamsCondition {
    centre: i32,
    coeff_pos: i32,
    coeff_neg: i32,
    satur_pos: u32,
    satur_neg: u32,
    deadband: i32,
}

/// Couple net des effets en cours (−PLAGE..PLAGE). `0` si le device est inactif.
#[must_use]
pub fn couple_net(banque: &BanqueEffets, volant: EtatVolant, instant_ms: u64) -> i32 {
    if !banque.actif() {
        return 0;
    }
    let somme: i64 = banque
        .effets_en_cours()
        .map(|effet| i64::from(contribution(effet, volant, instant_ms)))
        .sum();
    borne_i64(somme * i64::from(banque.gain_global()) / GAIN_MAX)
}

/// Contribution d'un seul effet (−PLAGE..PLAGE), avant pondération par le gain global.
fn contribution(effet: &Effet, volant: EtatVolant, instant_ms: u64) -> i32 {
    match &effet.params {
        ParametresEffet::Constante { magnitude } => (*magnitude).clamp(-PLAGE, PLAGE),
        // Sans horloge de démarrage par effet, on approxime la rampe par sa valeur de
        // départ (les jeux testés ne l'utilisent pas ; à affiner avec un timing).
        ParametresEffet::Rampe { debut, .. } => (*debut).clamp(-PLAGE, PLAGE),
        ParametresEffet::Periodique {
            magnitude,
            offset,
            phase,
            periode,
        } => periodique(
            effet.type_effet,
            *magnitude,
            *offset,
            *phase,
            *periode,
            instant_ms,
        ),
        ParametresEffet::Condition {
            centre,
            coeff_pos,
            coeff_neg,
            satur_pos,
            satur_neg,
            deadband,
        } => condition(
            effet.type_effet,
            volant,
            &ParamsCondition {
                centre: *centre,
                coeff_pos: *coeff_pos,
                coeff_neg: *coeff_neg,
                satur_pos: *satur_pos,
                satur_neg: *satur_neg,
                deadband: *deadband,
            },
        ),
        ParametresEffet::Aucun => 0,
    }
}

/// Force d'un effet conditionnel (ressort/amortisseur/inertie/friction).
fn condition(type_effet: TypeEffet, volant: EtatVolant, p: &ParamsCondition) -> i32 {
    // Grandeur d'entrée selon le type : position pour le ressort, vitesse sinon.
    let entree = match type_effet {
        TypeEffet::Ressort => volant.position - p.centre,
        TypeEffet::Amortisseur | TypeEffet::Inertie => volant.vitesse,
        // Friction : force ~ constante s'opposant au sens du déplacement.
        TypeEffet::Friction => volant.vitesse.signum() * PLAGE,
        _ => return 0,
    };
    let entree = hors_bande_morte(entree, p.deadband);
    if entree == 0 {
        return 0;
    }
    let coeff = if entree > 0 { p.coeff_pos } else { p.coeff_neg };
    // Force opposée au déplacement, normalisée : −(coeff/PLAGE) × entree.
    let force = -(i64::from(coeff) * i64::from(entree) / i64::from(PLAGE));
    // Saturation par direction (satur_pos borne la poussée positive, satur_neg la négative).
    let max_pos = i64::from(p.satur_pos.min(PLAGE_U32));
    let max_neg = i64::from(p.satur_neg.min(PLAGE_U32));
    borne_i64(force.clamp(-max_neg, max_pos))
}

/// Valeur instantanée d'un effet périodique (sinus/carré/triangle/dent de scie).
///
/// Hypothèses à valider sur matériel : période en **millisecondes**, phase en
/// **centi-degrés** (0..35999). La magnitude est l'amplitude (0..10000), l'offset un
/// décalage signé.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn periodique(
    type_effet: TypeEffet,
    magnitude: u32,
    offset: i32,
    phase: u32,
    periode: u32,
    instant_ms: u64,
) -> i32 {
    if periode == 0 {
        return offset.clamp(-PLAGE, PLAGE);
    }
    let cycle = (instant_ms % u64::from(periode)) as f64 / f64::from(periode);
    let phase_frac = f64::from(phase) / 36000.0;
    let onde = forme_onde(type_effet, frac(cycle + phase_frac));
    borne_f64(f64::from(offset) + onde * f64::from(magnitude.min(PLAGE_U32)))
}

/// Forme d'onde normalisée (−1..1) à la fraction de cycle `x` ∈ [0,1).
fn forme_onde(type_effet: TypeEffet, x: f64) -> f64 {
    use std::f64::consts::TAU;
    match type_effet {
        TypeEffet::Sinus => (x * TAU).sin(),
        TypeEffet::Carre => {
            if x < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        // Triangle : −1 → 1 → −1 sur un cycle.
        TypeEffet::Triangle => 1.0 - 4.0 * (x - 0.5).abs(),
        TypeEffet::DentMontante => 2.0 * x - 1.0,
        TypeEffet::DentDescendante => 1.0 - 2.0 * x,
        _ => 0.0,
    }
}

/// Retire la bande morte : 0 dans [−deadband, +deadband], sinon ramené vers 0.
fn hors_bande_morte(entree: i32, deadband: i32) -> i32 {
    if deadband <= 0 {
        entree
    } else if entree > deadband {
        entree - deadband
    } else if entree < -deadband {
        entree + deadband
    } else {
        0
    }
}

/// Partie fractionnaire dans [0,1).
fn frac(x: f64) -> f64 {
    x - x.floor()
}

/// Borne une valeur à −PLAGE..PLAGE (le clamp garantit que le `as i32` est exact).
#[allow(clippy::cast_possible_truncation)]
fn borne_i64(valeur: i64) -> i32 {
    valeur.clamp(-i64::from(PLAGE), i64::from(PLAGE)) as i32
}

/// Idem pour un flottant (périodique).
#[allow(clippy::cast_possible_truncation)]
fn borne_f64(valeur: f64) -> i32 {
    valeur.clamp(-f64::from(PLAGE), f64::from(PLAGE)) as i32
}

/// `PLAGE` exprimé en `u32` (pour borner les saturations/amplitudes non signées).
#[allow(clippy::cast_sign_loss)]
const PLAGE_U32: u32 = PLAGE as u32;

#[cfg(test)]
mod tests {
    use super::{EtatVolant, couple_net};
    use crate::ffb::{BanqueEffets, MessageFfb, OperationEffet, TypeEffet};

    /// Crée un effet d'un type donné dans la banque, le paramètre et le démarre.
    fn effet(banque: &mut BanqueEffets, bloc: u8, type_effet: TypeEffet, params: MessageFfb) {
        banque.appliquer(MessageFfb::NouvelEffet { bloc, type_effet });
        banque.appliquer(params);
        banque.appliquer(MessageFfb::Operation {
            bloc,
            operation: OperationEffet::Demarrer,
            repetitions: 1,
        });
    }

    fn au_centre() -> EtatVolant {
        EtatVolant::default()
    }

    #[test]
    fn constante_passe_la_magnitude() {
        let mut banque = BanqueEffets::new();
        effet(
            &mut banque,
            1,
            TypeEffet::Constante,
            MessageFfb::Constante {
                bloc: 1,
                magnitude: -3200,
            },
        );
        assert_eq!(couple_net(&banque, au_centre(), 0), -3200);
    }

    #[test]
    fn ressort_pousse_vers_le_centre() {
        let mut banque = BanqueEffets::new();
        effet(
            &mut banque,
            1,
            TypeEffet::Ressort,
            MessageFfb::Condition {
                bloc: 1,
                centre: 0,
                coeff_pos: 5000,
                coeff_neg: 5000,
                satur_pos: 10000,
                satur_neg: 10000,
                deadband: 0,
            },
        );
        // Volant tourné à droite (+5000) → force vers la gauche (négative).
        let couple = couple_net(
            &banque,
            EtatVolant {
                position: 5000,
                vitesse: 0,
            },
            0,
        );
        assert_eq!(couple, -2500);
    }

    #[test]
    fn amortisseur_oppose_la_vitesse() {
        let mut banque = BanqueEffets::new();
        effet(
            &mut banque,
            1,
            TypeEffet::Amortisseur,
            MessageFfb::Condition {
                bloc: 1,
                centre: 0,
                coeff_pos: 5000,
                coeff_neg: 5000,
                satur_pos: 10000,
                satur_neg: 10000,
                deadband: 0,
            },
        );
        // Rotation vers la droite (vitesse +3000) → force qui freine (négative).
        let couple = couple_net(
            &banque,
            EtatVolant {
                position: 0,
                vitesse: 3000,
            },
            0,
        );
        assert_eq!(couple, -1500);
    }

    #[test]
    fn gain_global_reduit_la_force() {
        let mut banque = BanqueEffets::new();
        effet(
            &mut banque,
            1,
            TypeEffet::Constante,
            MessageFfb::Constante {
                bloc: 1,
                magnitude: 4000,
            },
        );
        banque.appliquer(MessageFfb::Gain(128));
        // 4000 × 128 / 255 = 2007.
        assert_eq!(couple_net(&banque, au_centre(), 0), 2007);
    }

    #[test]
    fn somme_bornee_a_la_plage() {
        let mut banque = BanqueEffets::new();
        effet(
            &mut banque,
            1,
            TypeEffet::Constante,
            MessageFfb::Constante {
                bloc: 1,
                magnitude: 8000,
            },
        );
        effet(
            &mut banque,
            2,
            TypeEffet::Constante,
            MessageFfb::Constante {
                bloc: 2,
                magnitude: 8000,
            },
        );
        assert_eq!(couple_net(&banque, au_centre(), 0), 10000);
    }

    #[test]
    fn device_inactif_aucune_force() {
        let mut banque = BanqueEffets::new();
        effet(
            &mut banque,
            1,
            TypeEffet::Constante,
            MessageFfb::Constante {
                bloc: 1,
                magnitude: 5000,
            },
        );
        banque.appliquer(MessageFfb::Controle(crate::ffb::ControleDevice::Desactiver));
        assert_eq!(couple_net(&banque, au_centre(), 0), 0);
    }

    #[test]
    fn bande_morte_annule_les_petits_ecarts() {
        let mut banque = BanqueEffets::new();
        effet(
            &mut banque,
            1,
            TypeEffet::Ressort,
            MessageFfb::Condition {
                bloc: 1,
                centre: 0,
                coeff_pos: 5000,
                coeff_neg: 5000,
                satur_pos: 10000,
                satur_neg: 10000,
                deadband: 1000,
            },
        );
        // Écart 500 < bande morte 1000 → aucune force.
        let couple = couple_net(
            &banque,
            EtatVolant {
                position: 500,
                vitesse: 0,
            },
            0,
        );
        assert_eq!(couple, 0);
    }
}
