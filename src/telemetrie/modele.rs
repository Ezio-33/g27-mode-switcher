//! Modèle de force : convertit la télémétrie Forza en couple pour le volant.
//!
//! Le modèle combine quatre effets, tous tirés de la télémétrie et modulés par le **gain** :
//! - **force de virage** (couple d'auto-alignement ∝ dérive des pneus avant, courbe
//!   progressive) — la résistance ressentie en tournant ;
//! - **poids** (autocentrage matériel) **lourd à l'arrêt** s'allégeant avec la vitesse ;
//! - **secousses** (oscillation) de **rugosité de surface** + **impacts verticaux**
//!   (bosses, atterrissages) — le « grain » de la route et les chocs ;
//! - **allègement en l'air** : quand le train avant décolle (saut), le volant s'allège.
//!
//! Module **pur** (aucune E/S), donc entièrement testable.

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
/// Poids d'autocentrage matériel **en roulant** (plateau ≥ ~65 km/h, sur `0xFFFF`) : le
/// volant s'allège mais **reste ferme à haute vitesse** (cible calée sur le ressenti :
/// ~15 000 effectif à l'intensité par défaut). Au-delà du seuil, ce poids est maintenu.
const POIDS_ROULANT: f32 = 25_000.0;
/// Vitesse (m/s ≈ 65 km/h) où l'allègement atteint son plateau léger : en deçà, le volant
/// reste **lourd à bas régime** et s'allège en **cosinus** (très progressif), comme une
/// vraie direction. Au-delà, il reste au poids léger (`POIDS_ROULANT`).
const VITESSE_ALLEGEMENT_M_S: f32 = 18.0;

/// Accélération verticale (m/s²) donnant une secousse **pleine** (gros impact / atterrissage).
const ACCEL_SECOUSSE_PLEINE: f32 = 30.0;
/// Part max de la secousse due à la **rugosité de surface** (fraction de [`COUPLE_MAX`]) :
/// vibration continue sur route abîmée, trottoirs, hors-piste.
const SECOUSSE_RUMBLE_MAX: f32 = 0.30;
/// Part max de la secousse due aux **impacts verticaux** (fraction de [`COUPLE_MAX`]) :
/// bosses et surtout **atterrissages** (pic d'accélération verticale).
const SECOUSSE_IMPACT_MAX: f32 = 0.65;
/// Débattement de suspension avant (normalisé) en deçà duquel le train avant est « en
/// l'air » : sous ce seuil, le volant s'allège (saut) ; au-dessus, poids normal.
const SUSPENSION_SOL_MIN: f32 = 0.12;
/// Fraction du poids conservée quand le train avant est en pleine détente (saut) : le
/// volant s'allège sans devenir totalement libre.
const PLANCHER_AERIEN: f32 = 0.5;

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
    // En l'air (train avant délesté), la force de virage s'efface : plus de grip → volant léger.
    let mut couple = -progressif * facteur_vitesse * gain * facteur_sol(t);
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
    // Allègement quand le train avant décolle (saut) : le volant change de dureté en l'air.
    let magnitude = (POIDS_ROULANT + poids * (POIDS_ARRET - POIDS_ROULANT)) * gain * facteur_sol(t);
    borne_u16(magnitude)
}

/// Facteur « au sol » ([`PLANCHER_AERIEN`]..`1.0`) : `1` quand le train avant est posé,
/// réduit jusqu'à [`PLANCHER_AERIEN`] quand la suspension avant est en pleine détente
/// (saut). Allège le volant en l'air.
fn facteur_sol(t: &Telemetrie) -> f32 {
    let au_sol = (t.suspension_avant / SUSPENSION_SOL_MIN).clamp(0.0, 1.0);
    PLANCHER_AERIEN + (1.0 - PLANCHER_AERIEN) * au_sol
}

/// Amplitude (`0`..[`COUPLE_MAX`]) de la **secousse** à appliquer en oscillation rapide :
/// vibration de **rugosité de surface** + **impacts verticaux** (bosses, atterrissages).
/// Le worker en alterne le signe pour faire vibrer le volant. `0` hors gameplay.
#[must_use]
pub fn secousse_depuis_telemetrie(t: &Telemetrie, reglages: &ReglagesForza) -> i32 {
    if !t.course_active {
        return 0;
    }
    let gain = f32::from(reglages.gain) / 100.0;
    let rugosite = t.rumble_avant.clamp(0.0, 1.0);
    let impact = (t.accel_vertical.abs() / ACCEL_SECOUSSE_PLEINE).clamp(0.0, 1.0);
    let amplitude = (SECOUSSE_RUMBLE_MAX * rugosite + SECOUSSE_IMPACT_MAX * impact) * gain;
    arrondir_borne(amplitude * COUPLE_MAX_F)
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
    use super::{
        COUPLE_MAX, ReglagesForza, autocentre_depuis_vitesse, couple_depuis_telemetrie,
        secousse_depuis_telemetrie,
    };
    use crate::telemetrie::Telemetrie;

    fn telem(course_active: bool, vitesse_m_s: f32, derive_avant: f32) -> Telemetrie {
        Telemetrie {
            course_active,
            vitesse_m_s,
            derive_avant,
            rumble_avant: 0.0,
            accel_vertical: 0.0,
            suspension_avant: 1.0, // au sol (pas d'allègement aérien) par défaut.
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
    fn secousse_depuis_rugosite_et_impact() {
        let r = ReglagesForza::default();
        let lisse = telem(true, 20.0, 0.0);
        assert_eq!(secousse_depuis_telemetrie(&lisse, &r), 0, "route lisse → 0");
        // Rugosité de surface → secousse.
        let mut rugueux = lisse;
        rugueux.rumble_avant = 1.0;
        assert!(
            secousse_depuis_telemetrie(&rugueux, &r) > 0,
            "rugosité → secousse"
        );
        // Gros impact vertical (atterrissage) → secousse forte.
        let mut impact = lisse;
        impact.accel_vertical = 40.0;
        assert!(
            secousse_depuis_telemetrie(&impact, &r) > secousse_depuis_telemetrie(&rugueux, &r),
            "un atterrissage secoue plus qu'une route rugueuse"
        );
        // Hors course : aucune secousse.
        let mut hors = impact;
        hors.course_active = false;
        assert_eq!(secousse_depuis_telemetrie(&hors, &r), 0);
    }

    #[test]
    fn en_l_air_le_volant_s_allege() {
        let r = ReglagesForza::default();
        let mut au_sol = telem(true, 20.0, 0.20);
        au_sol.suspension_avant = 0.5; // posé
        let mut en_l_air = au_sol;
        en_l_air.suspension_avant = 0.0; // pleine détente (saut)
        // Poids d'autocentrage et force de virage réduits en l'air.
        assert!(
            autocentre_depuis_vitesse(&en_l_air, &r) < autocentre_depuis_vitesse(&au_sol, &r),
            "autocentrage plus léger en l'air"
        );
        assert!(
            couple_depuis_telemetrie(&en_l_air, &r).abs()
                < couple_depuis_telemetrie(&au_sol, &r).abs(),
            "force de virage réduite en l'air"
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
