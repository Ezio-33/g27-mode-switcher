//! Sortie FFB : transforme le flux de messages FFB du jeu en commandes de force
//! pour le G27 réel, à cadence régulière.
//!
//! Partie **isolée et pure** : agréger les [`MessageFfb`] dans la banque d'effets,
//! calculer le couple net et produire l'[`OutputReport`] de force est sans FFI ni
//! E/S — donc testable. L'écriture HID, la coupure de l'autocentrage et le `stop`
//! garanti sont du ressort de l'appelant (le worker du feeder), seul détenteur du
//! handle G27.

use std::sync::mpsc::Receiver;

use crate::report::OutputReport;

use super::{BanqueEffets, EtatVolant, MessageFfb, commande_force_constante, couple_net};

/// Période minimale entre deux envois de force (ms) ≈ 100 Hz. Le G27 a un watchdog
/// FFB : sans réémission régulière il relâche la force ; on réémet donc à chaque
/// période, même si le couple n'a pas changé.
const PERIODE_ENVOI_MS: u64 = 10;

/// Échelle de position attendue par le calcul (−PLAGE..PLAGE, cf. [`couple_net`]).
const PLAGE: i32 = 10000;

/// Centre de l'axe brut du volant (u16, ~32768).
const CENTRE_VOLANT: i32 = 32768;

/// Normalise l'axe brut du volant (0..65535, centre ~32768) en −PLAGE..PLAGE.
#[must_use]
pub fn normaliser_position(volant: u16) -> i32 {
    ((i32::from(volant) - CENTRE_VOLANT) * PLAGE / CENTRE_VOLANT).clamp(-PLAGE, PLAGE)
}

/// Pilote de force : agrège les messages FFB reçus et produit la commande à émettre.
pub struct PiloteForce {
    banque: BanqueEffets,
    messages: Receiver<MessageFfb>,
    position_precedente: i32,
    prochain_envoi_ms: u64,
}

impl PiloteForce {
    /// Crée un pilote consommant les messages FFB de `messages`.
    #[must_use]
    pub fn new(messages: Receiver<MessageFfb>) -> Self {
        Self {
            banque: BanqueEffets::new(),
            messages,
            position_precedente: 0,
            prochain_envoi_ms: 0,
        }
    }

    /// Intègre les messages FFB en attente, puis — si la période de rafraîchissement
    /// est atteinte — renvoie la commande de force à émettre vers le G27.
    ///
    /// `position` est l'axe du volant normalisé (−PLAGE..PLAGE) ; `instant_ms` une
    /// horloge monotone en millisecondes. Renvoie `None` entre deux périodes (le
    /// throttle borne la cadence à ~100 Hz).
    pub fn prochaine_commande(&mut self, position: i32, instant_ms: u64) -> Option<OutputReport> {
        while let Ok(message) = self.messages.try_recv() {
            self.banque.appliquer(message);
        }
        if instant_ms < self.prochain_envoi_ms {
            return None;
        }
        self.prochain_envoi_ms = instant_ms + PERIODE_ENVOI_MS;
        let vitesse = position - self.position_precedente;
        self.position_precedente = position;
        let couple = couple_net(&self.banque, EtatVolant { position, vitesse }, instant_ms);
        Some(commande_force_constante(couple))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::{PiloteForce, normaliser_position};
    use crate::ffb::{MessageFfb, OperationEffet, TypeEffet, commande_force_constante};

    #[test]
    fn normalisation_centre_et_butees() {
        assert_eq!(normaliser_position(32768), 0);
        assert_eq!(normaliser_position(0), -10000);
        // Butée haute : proche de +PLAGE (bornée).
        assert!(normaliser_position(u16::MAX) >= 9990);
    }

    #[test]
    fn throttle_borne_la_cadence() {
        let (_tx, rx) = mpsc::channel();
        let mut pilote = PiloteForce::new(rx);
        // 1er appel à t=0 : émet ; t=5 (< période) : rien ; t=10 : émet de nouveau.
        assert!(pilote.prochaine_commande(0, 0).is_some());
        assert!(pilote.prochaine_commande(0, 5).is_none());
        assert!(pilote.prochaine_commande(0, 10).is_some());
    }

    #[test]
    fn banque_vide_renvoie_le_stop() {
        let (_tx, rx) = mpsc::channel();
        let mut pilote = PiloteForce::new(rx);
        // Aucun effet en cours ⇒ couple 0 ⇒ commande de stop (volant neutre).
        assert_eq!(
            pilote.prochaine_commande(0, 0).unwrap().to_buffer(),
            crate::ffb::commande_stop_forces().to_buffer()
        );
    }

    #[test]
    fn force_constante_propagee_au_g27() {
        let (tx, rx) = mpsc::channel();
        let mut pilote = PiloteForce::new(rx);
        // Le jeu déclare puis démarre une force constante de magnitude 3200.
        tx.send(MessageFfb::NouvelEffet {
            bloc: 1,
            type_effet: TypeEffet::Constante,
        })
        .unwrap();
        tx.send(MessageFfb::Constante {
            bloc: 1,
            magnitude: 3200,
        })
        .unwrap();
        tx.send(MessageFfb::Operation {
            bloc: 1,
            operation: OperationEffet::Demarrer,
            repetitions: 1,
        })
        .unwrap();
        // Au centre, vitesse nulle : couple net = 3200 → commande de force associée.
        assert_eq!(
            pilote.prochaine_commande(0, 0).unwrap().to_buffer(),
            commande_force_constante(3200).to_buffer()
        );
    }
}
