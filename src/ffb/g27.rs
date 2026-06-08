//! Traduction d'un couple FFB (−10000..10000) en commande de force pour le G27 réel.
//!
//! Module **pur** (aucune E/S) : il construit les HID output reports envoyés au
//! volant pour appliquer une force constante ou le remettre au neutre. Le format est
//! repris — à titre de **référence documentaire**, aucune ligne n'est copiée — du
//! pilote Linux `drivers/hid/hid-lg4ff.c` (cas `FF_CONSTANT` de `hid_lg4ff_play`) :
//!
//! - force constante (slot 1) : `[0x11, 0x08, niveau, 0x80, 0x00, 0x00, 0x00]`, où
//!   `niveau = 0x80 + force` borné à `0x01..=0xFF` (`0x80` = aucune force / centre) ;
//! - stop (désactive le slot 1) : `[0x13, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]`, émis
//!   dès que `niveau` retombe sur `0x80` (volant neutre), comme le fait le pilote.
//!
//! Convention de signe : un couple **positif** doit pousser vers la **droite** (cf.
//! [`super::EtatVolant`]). Sur le G27 testé, le firmware applique le sens inverse —
//! [`SENS_POSITIF_VERS_DROITE`] vaut donc `false` (validé sur matériel) : on nie le
//! couple avant l'encodage pour qu'un couple positif pousse bien à droite côté volant.

use crate::report::OutputReport;

/// Échelle du couple en entrée : −PLAGE..PLAGE (cf. [`super::couple_net`]).
const PLAGE: i32 = 10000;

/// Niveau de force « neutre » (aucun couple) côté firmware.
/// Réf. : `hid-lg4ff.c` (`x = level + 0x80`, `0x80` = no force).
const NIVEAU_NEUTRE: i32 = 0x80;

/// Amplitude maximale de part et d'autre du neutre (`0x80 ± 0x7F` → `0x01..=0xFF`).
const AMPLITUDE_MAX: i32 = 0x7F;

/// Niveau de force minimal accepté par le firmware (`0x01`).
const NIVEAU_MIN: i32 = NIVEAU_NEUTRE - AMPLITUDE_MAX;

/// Niveau de force maximal accepté par le firmware (`0xFF`).
const NIVEAU_MAX: i32 = NIVEAU_NEUTRE + AMPLITUDE_MAX;

/// En-tête « download force, slot 1 » : `value[0]=0x11` (slot 1), `value[1]=0x08`.
/// Réf. : `hid-lg4ff.c`.
const FORCE_CONSTANTE_CMD: [u8; 2] = [0x11, 0x08];

/// En-tête « stop / désactive le slot 1 » : `value[0]=0x13`.
/// Réf. : `hid-lg4ff.c`.
const STOP_CMD: u8 = 0x13;

/// Sens du couple, **validé sur matériel** : sur le G27 testé, un niveau `> 0x80`
/// pousse vers la **gauche**. On garde donc `false` pour qu'un couple positif (droite,
/// cf. [`super::EtatVolant`]) soit nié avant l'encodage et pousse bien à droite.
const SENS_POSITIF_VERS_DROITE: bool = false;

/// Construit la commande HID appliquant la **force constante** correspondant à
/// `couple` (−PLAGE..PLAGE).
///
/// Le couple est borné à la plage, puis converti en niveau firmware (`0x01..=0xFF`,
/// `0x80` = neutre). Si le niveau retombe sur le neutre (`couple` proche de 0), on
/// renvoie la commande de **stop** plutôt qu'une force nulle explicite — exactement
/// comme le pilote Linux.
#[must_use]
pub fn commande_force_constante(couple: i32) -> OutputReport {
    let couple = couple.clamp(-PLAGE, PLAGE);
    let oriente = if SENS_POSITIF_VERS_DROITE {
        couple
    } else {
        -couple
    };
    let niveau = (NIVEAU_NEUTRE + oriente * AMPLITUDE_MAX / PLAGE).clamp(NIVEAU_MIN, NIVEAU_MAX);
    if niveau == NIVEAU_NEUTRE {
        return commande_stop_forces();
    }
    OutputReport::unnumbered(vec![
        FORCE_CONSTANTE_CMD[0],
        FORCE_CONSTANTE_CMD[1],
        octet(niveau),
        octet(NIVEAU_NEUTRE),
        0x00,
        0x00,
        0x00,
    ])
}

/// Construit la commande HID remettant le volant **au neutre** (stop du slot 1).
#[must_use]
pub fn commande_stop_forces() -> OutputReport {
    OutputReport::unnumbered(vec![STOP_CMD, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
}

/// Réduit un niveau borné à `0x01..=0xFF` en octet (le clamp garantit l'exactitude).
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn octet(niveau: i32) -> u8 {
    niveau as u8
}

#[cfg(test)]
mod tests {
    use super::{NIVEAU_NEUTRE, PLAGE, commande_force_constante, commande_stop_forces};

    #[test]
    fn stop_desactive_le_slot_1() {
        // Préfixe 0x00 (pas de report ID) + en-tête 0x13.
        assert_eq!(
            commande_stop_forces().to_buffer(),
            vec![0x00, 0x13, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn couple_nul_remet_au_neutre() {
        // couple 0 ⇒ niveau == 0x80 ⇒ commande de stop (pas de force explicite).
        assert_eq!(
            commande_force_constante(0).to_buffer(),
            commande_stop_forces().to_buffer()
        );
    }

    #[test]
    fn couple_maximal_pousse_a_fond_a_droite() {
        // Couple +PLAGE = droite. Sens firmware inversé (validé matériel) : le couple
        // est nié avant encodage ⇒ niveau 0x80 − 0x7F = 0x01 (= droite côté volant).
        assert_eq!(
            commande_force_constante(PLAGE).to_buffer(),
            vec![0x00, 0x11, 0x08, 0x01, 0x80, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn couple_minimal_pousse_a_fond_a_gauche() {
        // Couple −PLAGE = gauche ⇒ après inversion firmware, niveau 0x80 + 0x7F = 0xFF.
        assert_eq!(
            commande_force_constante(-PLAGE).to_buffer(),
            vec![0x00, 0x11, 0x08, 0xFF, 0x80, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn sens_symetrique_autour_du_neutre() {
        // niveau(+c) et niveau(−c) sont équidistants du neutre 0x80.
        let droite = i32::from(commande_force_constante(4000).to_buffer()[3]);
        let gauche = i32::from(commande_force_constante(-4000).to_buffer()[3]);
        assert_eq!(droite - NIVEAU_NEUTRE, NIVEAU_NEUTRE - gauche);
    }

    #[test]
    fn petite_valeur_donne_une_petite_force() {
        // Sécurité physique : un couple minuscule reste tout près du neutre.
        let niveau = i32::from(commande_force_constante(150).to_buffer()[3]);
        assert!((niveau - NIVEAU_NEUTRE).abs() <= 2, "niveau={niveau}");
    }

    #[test]
    fn jamais_hors_bornes_sur_toute_la_plage() {
        // Même au-delà de la plage, l'octet de niveau reste dans 0x01..=0xFF et le
        // report fait toujours 8 octets (préfixe + 7).
        for couple in (-20000..=20000).step_by(250) {
            let buffer = commande_force_constante(couple).to_buffer();
            assert_eq!(buffer.len(), 8, "couple={couple}");
            if buffer[1] == 0x11 {
                assert!((0x01..=0xFF).contains(&buffer[3]), "couple={couple}");
            }
        }
    }
}
