//! Sortie FFB : transforme le flux de messages FFB du jeu en commandes de force
//! pour le G27 réel, à cadence régulière.
//!
//! Partie **isolée et pure** : agréger les [`MessageFfb`] dans la banque d'effets,
//! calculer le couple et produire l'[`OutputReport`] de force est sans FFI ni E/S —
//! donc testable. L'écriture HID, la coupure de l'autocentrage et le `stop` garanti
//! sont du ressort de l'appelant (le worker du feeder), seul détenteur du handle G27.
//!
//! ⚠️ **Sécurité** : on n'applique que la **force constante** (boucle ouverte, validée
//! matériel via `ffb force`). Les effets à boucle fermée (ressort/amortisseur/périodique)
//! provoquent une oscillation violente tant que leur signe n'est pas validé : ils sont
//! écartés au calcul (cf. [`crate::ffb::couple_constant`]).

use std::sync::mpsc::Receiver;

use crate::report::OutputReport;

use super::{
    BanqueEffets, MessageFfb, ModulateurAutocentrage, coeff_ressort, commande_force_constante,
    couple_constant,
};

/// Période minimale entre deux envois de force (ms) ≈ 100 Hz. Le G27 a un watchdog
/// FFB : sans réémission régulière il relâche la force ; on réémet donc à chaque
/// période, même si le couple n'a pas changé.
const PERIODE_ENVOI_MS: u64 = 10;

/// Pilote de force : agrège les messages FFB reçus et produit la commande à émettre.
pub struct PiloteForce {
    banque: BanqueEffets,
    messages: Receiver<MessageFfb>,
    prochain_envoi_ms: u64,
    modulateur: ModulateurAutocentrage,
    dernier_instant_ms: u64,
}

impl PiloteForce {
    /// Crée un pilote consommant les messages FFB de `messages`.
    #[must_use]
    pub fn new(messages: Receiver<MessageFfb>) -> Self {
        Self {
            banque: BanqueEffets::new(),
            messages,
            prochain_envoi_ms: 0,
            modulateur: ModulateurAutocentrage::default(),
            dernier_instant_ms: 0,
        }
    }

    /// Intègre les messages FFB en attente, puis — si la période de rafraîchissement
    /// est atteinte — renvoie la commande de force constante à émettre vers le G27.
    ///
    /// `instant_ms` est une horloge monotone en millisecondes. Renvoie `None` entre
    /// deux périodes (le throttle borne la cadence à ~100 Hz).
    pub fn prochaine_commande(&mut self, instant_ms: u64) -> Option<OutputReport> {
        while let Ok(message) = self.messages.try_recv() {
            self.banque.appliquer(message);
        }
        // Suit (à chaque appel, pas seulement à l'émission) l'intensité du ressort du
        // jeu pour moduler l'autocentrage matériel — cf. [`Self::magnitude_autocentre`].
        let ecoule = instant_ms.saturating_sub(self.dernier_instant_ms);
        self.dernier_instant_ms = instant_ms;
        self.modulateur
            .appliquer(coeff_ressort(&self.banque), ecoule);
        if instant_ms < self.prochain_envoi_ms {
            return None;
        }
        self.prochain_envoi_ms = instant_ms + PERIODE_ENVOI_MS;
        Some(commande_force_constante(couple_constant(&self.banque)))
    }

    /// Amplitude d'autocentrage matériel souhaitée (0..`0xFFFF`), déduite du ressort
    /// que le jeu envoie (fort à l'arrêt, doux en roulant). Le worker du feeder
    /// l'envoie au G27 via `autocenter::strength_report`, séparément de la force.
    #[must_use]
    pub fn magnitude_autocentre(&self) -> u16 {
        self.modulateur.magnitude()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::PiloteForce;
    use crate::ffb::{MessageFfb, OperationEffet, TypeEffet, commande_force_constante};

    #[test]
    fn throttle_borne_la_cadence() {
        let (_tx, rx) = mpsc::channel();
        let mut pilote = PiloteForce::new(rx);
        // 1er appel à t=0 : émet ; t=5 (< période) : rien ; t=10 : émet de nouveau.
        assert!(pilote.prochaine_commande(0).is_some());
        assert!(pilote.prochaine_commande(5).is_none());
        assert!(pilote.prochaine_commande(10).is_some());
    }

    #[test]
    fn banque_vide_renvoie_une_force_neutre() {
        let (_tx, rx) = mpsc::channel();
        let mut pilote = PiloteForce::new(rx);
        // Aucun effet ⇒ couple 0 ⇒ force constante NEUTRE (pas un stop, qui couperait
        // l'autocentrage matériel actif en parallèle).
        assert_eq!(
            pilote.prochaine_commande(0).unwrap().to_buffer(),
            crate::ffb::commande_force_constante(0).to_buffer()
        );
        assert_ne!(
            pilote.prochaine_commande(10).unwrap().to_buffer(),
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
        // Couple constant = 3200 → commande de force associée.
        assert_eq!(
            pilote.prochaine_commande(0).unwrap().to_buffer(),
            commande_force_constante(3200).to_buffer()
        );
    }
}
